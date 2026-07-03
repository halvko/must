//! Type inference: a pure per-body query.
//!
//! Other items are seen only through [`signature`]; builtins through a fixed
//! table. The [`InferCtx`] owns an ena unification table plus a
//! [`Constraints`] store: axioms (annotations, call parameter types) unify
//! eagerly with their [`Cause`] recorded, while joins (`if`/`else` branch
//! agreement) are deferred and solved after traversal so blame can be
//! attributed with all axioms known — see [`crate::constraint`] for the
//! model. Trait obligations and further deferred constraint kinds slot into
//! the same store later without changing the query graph.

use base_db::{Db, SourceFile};
use ena::unify::InPlaceUnificationTable;
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::body::{
    BindingId, Body, ExprData, ExprId, LiteralData, MatchArm, PatData, PatId, Stmt, body,
};
use crate::constraint::{self, Cause, Constraints, Join, Witness, resolve_fully};
use crate::item_tree::{Constness, TypeDeclData};
use crate::scopes::{Builtin, Resolution, resolutions, type_scope};
use crate::ty::{
    Ty, TyVar, TyVarValue, VariantTy, builtin_type_by_name, enum_variants, lower_type_ref,
    signature, signature_needs_annotation, type_underlying, widens_to,
};
use crate::{ItemId, ItemLoc, Severity, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceResult {
    pub type_of_expr: ArenaMap<ExprId, Ty>,
    pub type_of_binding: ArenaMap<BindingId, Ty>,
    /// Where a variant-typed value was accepted by the *widening
    /// conversion* (variant → its enum): the expression whose value gets
    /// the tag injected, mapped to the variant it was. MIR plants its
    /// `WidenToEnum` op exactly at these expressions and nowhere else; the
    /// expression's own entry in [`Self::type_of_expr`] is the
    /// post-conversion type where the conversion happened at a direct
    /// check site, and the precise variant where it happened at a deferred
    /// join edge.
    pub widened: ArenaMap<ExprId, VariantTy>,
    /// Resolution of every `Enum::Variant` path expression that named a
    /// real variant — the type-directed second-segment resolution (`::`
    /// paths resolve against the enum's declaration during inference, not
    /// in `scopes`). Consumed by MIR (construction) and ide (goto-def,
    /// highlighting).
    pub variant_of_expr: ArenaMap<ExprId, VariantTy>,
    /// The pattern-side counterpart of [`Self::variant_of_expr`]: every
    /// match-arm variant pattern that named a real variant — the qualified
    /// (`Shape::Circle`) and elided sigil (`::Circle`) spellings, resolved
    /// type-directed against the scrutinee's enum. Only
    /// [`crate::body::PatData::Variant`] populates this now; a bare binding
    /// is never reinterpreted as a
    /// variant. Consumed by MIR (dispatch) and ide (goto-def, hover,
    /// highlighting).
    pub variant_of_pat: ArenaMap<PatId, VariantTy>,
    /// The type a `let`/parameter pattern destructures — every pattern, not
    /// just [`crate::body::PatData::Bind`] (which is also mirrored into
    /// [`Self::type_of_binding`] under its own binding). MIR reads this for
    /// the synthetic "whole value" local a `Record`/`Newtype` pattern needs.
    pub type_of_pat: ArenaMap<PatId, Ty>,
    pub diagnostics: Vec<InferenceDiagnostic>,
}

/// Range-free (keyed by HIR ids); ranges are attached by the diagnostics
/// layer through the body source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceDiagnostic {
    TypeMismatch {
        expr: ExprId,
        expected: Ty,
        actual: Ty,
        /// Why the expected type was required — rendered as "because of
        /// this" hints. Possibly several: a join mismatch cites every
        /// sibling branch that agreed on the type plus the axiom (annotation
        /// or call) that demanded it.
        reasons: Vec<Cause>,
    },
    /// Every branch of a join agrees on a type — the construct as a whole
    /// is what conflicts with the axiom that decided the expected type, so
    /// it carries one diagnostic instead of a squiggle per branch.
    AllBranchesMismatch {
        /// The joining construct (the whole `if` expression).
        expr: ExprId,
        expected: Ty,
        /// The type every branch produced.
        actual: Ty,
        /// The axiom that demanded `expected`, when recorded.
        reasons: Vec<Cause>,
    },
    /// The two branches of an `if`/`else` produced different types and
    /// nothing (no annotation, no use) breaks the tie.
    IfBranchMismatch {
        /// The else branch expression (carries the squiggle).
        else_expr: ExprId,
        /// The then branch expression (carries the related hint).
        then_expr: ExprId,
        then_ty: Ty,
        else_ty: Ty,
    },
    NotCallable {
        /// The callee expression.
        expr: ExprId,
        ty: Ty,
    },
    ArgCountMismatch {
        /// The call expression.
        expr: ExprId,
        expected: usize,
        found: usize,
    },
    /// A use of an item whose signature can't be determined because its
    /// definition lacks annotations. Permanent for exported symbols (their
    /// contract must be written); interprocedural inference will lift it
    /// for private items. See [`signature_needs_annotation`].
    NeedsAnnotation {
        /// The referencing expression.
        expr: ExprId,
        /// The item whose type couldn't be inferred.
        item: ItemLoc,
    },
    /// An assignment whose target resolves to a local declared without
    /// `mut`. Squiggle on the target; the binding's declaration carries the
    /// related hint (and is where a later quick fix will insert `mut`).
    AssignToImmutable {
        /// The assignment's target expression.
        target: ExprId,
        binding: BindingId,
        /// The binding's name, carried here so [`Self::message`] can render
        /// without the body in hand (same reason `NeedsAnnotation` carries
        /// an [`ItemLoc`]).
        name: String,
    },
    /// An assignment whose target resolves to a top-level item. `static`s
    /// have an identity but cannot be reassigned; `const`s don't even have
    /// a single place an assignment could write to — the message
    /// distinguishes the two.
    AssignToItem {
        /// The assignment's target expression.
        target: ExprId,
        item: ItemLoc,
        constness: Constness,
    },
    /// An assignment whose target resolves to a builtin function.
    AssignToBuiltin {
        /// The assignment's target expression.
        target: ExprId,
        builtin: Builtin,
    },
    /// A record literal checked against a record type that requires fields
    /// the literal doesn't have. One diagnostic on the whole literal naming
    /// every missing field — the fix (adding fields) happens there.
    RecordLitMissingFields {
        /// The record literal expression.
        expr: ExprId,
        /// The missing `(name, type)` pairs, sorted by name.
        fields: Vec<(String, Ty)>,
    },
    /// A record literal field its expected record type has no room for
    /// (exact field-set equality: extra fields are errors, not dropped).
    /// One diagnostic per extra field; the renderer narrows the squiggle to
    /// the field's name.
    RecordLitExtraField {
        /// The extra field's value expression.
        expr: ExprId,
        name: String,
        /// The expected record type that lacks the field.
        expected: Ty,
    },
    /// A field access on a type that has no such field: a record without
    /// it, or a non-record. The renderer narrows the squiggle to the field
    /// name.
    NoSuchField {
        /// The field-access expression.
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
    },
    /// A field access whose receiver's type is still undetermined when the
    /// access is checked. Structural records use exact equality, so a field
    /// name can never determine the receiver's type backwards — the way out
    /// is an annotation.
    FieldOnUnknownType {
        /// The field-access expression (where MIR will trap).
        expr: ExprId,
        /// The receiver — carries the squiggle: the annotation goes there.
        receiver: ExprId,
    },
    /// A bare use of a `type` item in expression position. Types are not
    /// first-class values; the one expression position a type name may
    /// appear in is as a construction head (`Foo(...)`), which never
    /// reaches this.
    TypeNotValue {
        /// The referencing expression (the name — carries the squiggle).
        expr: ExprId,
        /// The type's name, carried so [`Self::message`] renders without
        /// the body in hand (same reason `AssignToImmutable` carries one).
        name: String,
    },
    /// A construction call `Foo(...)` with anything but exactly one
    /// argument. A constructor is a plain function taking the underlying
    /// record value — nothing more.
    TypeCtorArgCount {
        /// The call expression.
        expr: ExprId,
        /// The `type` item being constructed.
        item: ItemLoc,
        found: usize,
    },
    /// `Shape::Missing` — the enum exists but declares no such variant.
    /// The squiggle narrows to the variant name; the declaration is the
    /// related location.
    NoSuchVariant {
        /// The variant-path expression.
        expr: ExprId,
        /// The enum `type` item.
        item: ItemLoc,
        /// The name that resolved to nothing.
        name: String,
    },
    /// A `::` path on a `type` item that declares a struct shape
    /// (`Point::x` where `Point = struct { ... }`): only enums have
    /// variants.
    NoVariantsOnStruct {
        /// The variant-path expression.
        expr: ExprId,
        /// The struct `type` item.
        item: ItemLoc,
    },
    /// A `::` path whose base names a value (a local, a `static`/`const`
    /// item, or a builtin) instead of a type.
    VariantPathOnValue {
        /// The variant-path expression.
        expr: ExprId,
        /// The base name, carried so [`Self::message`] renders without the
        /// body in hand.
        name: String,
    },
    /// A direct construction call on an enum type (`Shape(...)`): an enum
    /// has no single shape to construct — one of its variants does.
    EnumCtorIsVariant {
        /// The call expression.
        expr: ExprId,
        /// The enum `type` item.
        item: ItemLoc,
    },
    /// A `match` on an enum-typed scrutinee whose arms don't cover every
    /// variant. One diagnostic on the `match` keyword naming each uncovered
    /// variant; MIR's otherwise-arm traps with the identical message.
    NonExhaustiveMatch {
        /// The match expression.
        expr: ExprId,
        /// Each uncovered variant, `Enum::Variant`-rendered, in
        /// declaration order.
        uncovered: Vec<String>,
    },
    /// A `match` on a non-enum scrutinee with no `_`/binding arm: those are
    /// the only patterns that can match it, so one is required.
    MatchWithoutCatchAll {
        /// The match expression.
        expr: ExprId,
        scrutinee: Ty,
    },
    /// A match arm that can never run. Warning-severity: the code is
    /// well-typed, just dead — MIR lowers the arm as an unreachable block
    /// and plants no trap.
    UnreachableArm {
        /// The enclosing match expression.
        match_expr: ExprId,
        /// The arm's pattern (carries the squiggle).
        pat: PatId,
        reason: UnreachableReason,
    },
    /// A variant pattern on a scrutinee that has no variants (a builtin, a
    /// struct type, a record, ...): only `_` or a binding can match it.
    NonEnumScrutineeVariantPat {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        scrutinee: Ty,
    },
    /// A variant pattern naming a variant its enum doesn't declare — the
    /// pattern-side sibling of [`Self::NoSuchVariant`].
    PatNoSuchVariant {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        /// The enum `type` item.
        item: ItemLoc,
        /// The name that resolved to nothing.
        name: String,
    },
    /// A qualified variant pattern of a *different* enum than the
    /// scrutinee's (`Other::X` in a match on a `Shape`).
    PatWrongEnum {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        /// The enum the pattern belongs to.
        item: ItemLoc,
        /// The pattern's variant name.
        variant: String,
        scrutinee: Ty,
    },
    /// A binding arm whose name happens to match a variant of the
    /// scrutinee's enum. Bare binds are never reinterpreted (see
    /// `check_match_pat`'s `PatData::Bind` arm) — this is almost always a
    /// migration mistake or confusion, so it's flagged even though the code
    /// is well-typed (warning severity, like `UnreachableArm`).
    BindShadowsVariant {
        /// The enclosing match expression.
        match_expr: ExprId,
        /// The binding pattern (carries the squiggle).
        pat: PatId,
        /// The enum whose variant the name shadows.
        item: ItemLoc,
        name: String,
    },
    /// A variant pattern naming the wrong number of payload bindings.
    PatArity {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        /// The variant's name.
        variant: String,
        /// How many payloads the variant declares.
        payloads: usize,
        /// How many bindings the pattern names.
        found: usize,
    },
    /// A qualified variant pattern whose first segment doesn't name an
    /// enum type. The message is rendered at construction (it mirrors the
    /// type-position wording, which needs the database).
    PatPathError {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        message: String,
    },
    /// A bare variant pattern (`Circle(r)`) on a scrutinee whose type is
    /// still undetermined: there is no enum to resolve the name against.
    VariantPatUnknownScrutinee {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
    },
    /// A `break` with no enclosing `loop`. Function literals and `const`
    /// blocks reset the loop context — they are units of their own, so a
    /// `break` inside one never exits a loop outside it.
    BreakOutsideLoop {
        /// The break expression.
        expr: ExprId,
    },
    /// A `continue` with no enclosing `loop`; same boundaries as
    /// [`Self::BreakOutsideLoop`].
    ContinueOutsideLoop {
        /// The continue expression.
        expr: ExprId,
    },
    /// A record-destructuring `let`/parameter pattern names a field its
    /// type doesn't have — the pattern-side sibling of
    /// [`Self::RecordLitExtraField`].
    PatUnknownField {
        pat: PatId,
        /// Anchor for the IDE range gate / a future MIR trap: the `let`'s
        /// initializer, or the enclosing `fn` literal's body for a
        /// parameter pattern (see `InferCtx::check_pat`'s callers).
        expr: ExprId,
        name: String,
        record_ty: Ty,
    },
    /// A record-destructuring pattern without `..` doesn't name every field
    /// of its type — the pattern-side sibling of
    /// [`Self::RecordLitMissingFields`]. Exact-equality typing of the
    /// scrutinee still applies; `..` is the only way to leave fields out.
    PatMissingFields {
        pat: PatId,
        expr: ExprId,
        /// The unmentioned `(name, type)` pairs, sorted by name.
        fields: Vec<(String, Ty)>,
    },
    /// A `let`/parameter pattern destructures a value whose type isn't
    /// known here (an unannotated bare `struct { ... }` pattern, with
    /// nothing else pinning the initializer's type down).
    PatBindingNeedsAnnotation { pat: PatId, expr: ExprId },
    /// A record-destructuring pattern applied to a non-record, non-`Infer`,
    /// non-`Error` type.
    PatNotRecord { pat: PatId, expr: ExprId, ty: Ty },
    /// `Name(...)` names something other than a declared `type`.
    PatUnknownType {
        pat: PatId,
        expr: ExprId,
        name: String,
    },
    /// `Name(...)` unwraps a *different* named type than the scrutinee
    /// actually has.
    PatNamedTypeMismatch {
        pat: PatId,
        expr: ExprId,
        expected: Ty,
        actual: Ty,
    },
}

/// Why an arm can never run — one message per cause, so the fix is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreachableReason {
    /// The variant is already covered by an earlier arm.
    VariantCovered(String),
    /// An earlier `_`/binding arm already matches anything.
    AfterCatchAll,
    /// Every variant of the enum is already covered individually.
    AllVariantsCovered(String),
    /// The scrutinee is variant-typed; this arm matches a different
    /// variant of its enum.
    OtherVariant { scrutinee: Ty },
}

impl InferenceDiagnostic {
    /// The expression the diagnostic is reported on.
    pub fn expr(&self) -> ExprId {
        match self {
            InferenceDiagnostic::TypeMismatch { expr, .. }
            | InferenceDiagnostic::AllBranchesMismatch { expr, .. }
            | InferenceDiagnostic::NotCallable { expr, .. }
            | InferenceDiagnostic::ArgCountMismatch { expr, .. }
            | InferenceDiagnostic::NeedsAnnotation { expr, .. }
            | InferenceDiagnostic::RecordLitMissingFields { expr, .. }
            | InferenceDiagnostic::RecordLitExtraField { expr, .. }
            | InferenceDiagnostic::NoSuchField { expr, .. }
            | InferenceDiagnostic::TypeNotValue { expr, .. }
            | InferenceDiagnostic::TypeCtorArgCount { expr, .. }
            | InferenceDiagnostic::NoSuchVariant { expr, .. }
            | InferenceDiagnostic::NoVariantsOnStruct { expr, .. }
            | InferenceDiagnostic::VariantPathOnValue { expr, .. }
            | InferenceDiagnostic::EnumCtorIsVariant { expr, .. }
            | InferenceDiagnostic::NonExhaustiveMatch { expr, .. }
            | InferenceDiagnostic::MatchWithoutCatchAll { expr, .. }
            | InferenceDiagnostic::BreakOutsideLoop { expr }
            | InferenceDiagnostic::ContinueOutsideLoop { expr } => *expr,
            InferenceDiagnostic::UnreachableArm { match_expr, .. }
            | InferenceDiagnostic::NonEnumScrutineeVariantPat { match_expr, .. }
            | InferenceDiagnostic::PatNoSuchVariant { match_expr, .. }
            | InferenceDiagnostic::PatWrongEnum { match_expr, .. }
            | InferenceDiagnostic::BindShadowsVariant { match_expr, .. }
            | InferenceDiagnostic::PatArity { match_expr, .. }
            | InferenceDiagnostic::PatPathError { match_expr, .. }
            | InferenceDiagnostic::VariantPatUnknownScrutinee { match_expr, .. } => *match_expr,
            InferenceDiagnostic::FieldOnUnknownType { receiver, .. } => *receiver,
            InferenceDiagnostic::IfBranchMismatch { else_expr, .. } => *else_expr,
            InferenceDiagnostic::AssignToImmutable { target, .. }
            | InferenceDiagnostic::AssignToItem { target, .. }
            | InferenceDiagnostic::AssignToBuiltin { target, .. } => *target,
            InferenceDiagnostic::PatUnknownField { expr, .. }
            | InferenceDiagnostic::PatMissingFields { expr, .. }
            | InferenceDiagnostic::PatBindingNeedsAnnotation { expr, .. }
            | InferenceDiagnostic::PatNotRecord { expr, .. }
            | InferenceDiagnostic::PatUnknownType { expr, .. }
            | InferenceDiagnostic::PatNamedTypeMismatch { expr, .. } => *expr,
        }
    }

    /// The pattern the diagnostic squiggles, for the pattern-side
    /// diagnostics ([`Self::expr`] then carries a resolvable anchor only —
    /// the enclosing match (where MIR refuses the value) for match-arm
    /// patterns, the `let`'s initializer or the enclosing `fn` literal's
    /// body for a `let`/parameter destructuring pattern).
    pub fn pat(&self) -> Option<PatId> {
        match self {
            InferenceDiagnostic::UnreachableArm { pat, .. }
            | InferenceDiagnostic::NonEnumScrutineeVariantPat { pat, .. }
            | InferenceDiagnostic::PatNoSuchVariant { pat, .. }
            | InferenceDiagnostic::PatWrongEnum { pat, .. }
            | InferenceDiagnostic::BindShadowsVariant { pat, .. }
            | InferenceDiagnostic::PatArity { pat, .. }
            | InferenceDiagnostic::PatPathError { pat, .. }
            | InferenceDiagnostic::VariantPatUnknownScrutinee { pat, .. }
            | InferenceDiagnostic::PatUnknownField { pat, .. }
            | InferenceDiagnostic::PatMissingFields { pat, .. }
            | InferenceDiagnostic::PatBindingNeedsAnnotation { pat, .. }
            | InferenceDiagnostic::PatNotRecord { pat, .. }
            | InferenceDiagnostic::PatUnknownType { pat, .. }
            | InferenceDiagnostic::PatNamedTypeMismatch { pat, .. } => Some(*pat),
            _ => None,
        }
    }

    /// Unreachable arms are dead code, not wrong code — a warning;
    /// everything else is an error.
    pub fn severity(&self) -> Severity {
        match self {
            InferenceDiagnostic::UnreachableArm { .. }
            | InferenceDiagnostic::BindShadowsVariant { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// The human-readable message. Shared between editor diagnostics and MIR
    /// trap terminators, so a deferred error crashes at runtime with exactly
    /// the text the squiggle showed.
    pub fn message(&self) -> String {
        match self {
            InferenceDiagnostic::TypeMismatch {
                expected, actual, ..
            } => {
                let base = format!(
                    "type mismatch: expected `{}`, found `{}`",
                    expected.display(),
                    actual.display()
                );
                // A nominal/structural near-miss: the found record may even
                // be the declared shape, but a named type never coerces —
                // say how to actually make one.
                if let (Ty::Named(loc), Ty::Record(_)) = (expected, actual) {
                    format!(
                        "{base}; `{name}` is a distinct type — construct it with `{name}(...)`",
                        name = loc.display_name()
                    )
                } else {
                    base
                }
            }
            InferenceDiagnostic::AllBranchesMismatch {
                expected, actual, ..
            } => format!(
                "every branch produces `{}`, but `{}` is needed",
                actual.display(),
                expected.display()
            ),
            InferenceDiagnostic::IfBranchMismatch {
                then_ty, else_ty, ..
            } => format!(
                "`if` branches have incompatible types: `{}` vs `{}`; \
                 add a type annotation to decide between them",
                then_ty.display(),
                else_ty.display()
            ),
            InferenceDiagnostic::NotCallable { ty, .. } => {
                format!("expression of type `{}` is not callable", ty.display())
            }
            InferenceDiagnostic::ArgCountMismatch {
                expected, found, ..
            } => format!("expected {expected} argument(s), found {found}"),
            InferenceDiagnostic::NeedsAnnotation { item, .. } => {
                let display = if item.name.is_empty() {
                    "this item"
                } else {
                    &item.name
                };
                format!(
                    "cannot infer the type of `{display}` across items; \
                     add a type annotation to its definition"
                )
            }
            InferenceDiagnostic::AssignToImmutable { name, .. } => {
                format!("cannot assign to `{name}`: it is not declared `mut`")
            }
            InferenceDiagnostic::AssignToItem {
                item, constness, ..
            } => match constness {
                Constness::Static => format!(
                    "cannot assign to `{}`: `static` items cannot be reassigned",
                    item.display_name()
                ),
                Constness::Const => format!(
                    "cannot assign to `{}`: a `const` is copied into each use, \
                     so there is no single place to assign to",
                    item.display_name()
                ),
            },
            InferenceDiagnostic::AssignToBuiltin { builtin, .. } => {
                format!(
                    "cannot assign to `{}`: it is a builtin function",
                    builtin.name()
                )
            }
            InferenceDiagnostic::RecordLitMissingFields { fields, .. } => {
                let list = fields
                    .iter()
                    .map(|(name, ty)| format!("`{name}: {}`", ty.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if fields.len() == 1 {
                    format!("record literal is missing field {list}")
                } else {
                    format!("record literal is missing fields {list}")
                }
            }
            InferenceDiagnostic::RecordLitExtraField { name, expected, .. } => {
                format!(
                    "no field `{name}` in expected type `{}`",
                    expected.display()
                )
            }
            InferenceDiagnostic::NoSuchField {
                name, receiver_ty, ..
            } => {
                format!("no field `{name}` on `{}`", receiver_ty.display())
            }
            InferenceDiagnostic::FieldOnUnknownType { .. } => {
                "cannot determine the type of this expression; add a type annotation".to_owned()
            }
            InferenceDiagnostic::TypeNotValue { name, .. } => {
                format!("`{name}` is a type, not a value")
            }
            InferenceDiagnostic::TypeCtorArgCount { item, found, .. } => format!(
                "`{}` takes exactly one argument (its underlying `struct` value), found {found}",
                item.display_name()
            ),
            InferenceDiagnostic::NoSuchVariant { item, name, .. } => {
                format!("`{}` has no variant `{name}`", item.display_name())
            }
            InferenceDiagnostic::NoVariantsOnStruct { item, .. } => {
                format!(
                    "`{}` has no variants (it is a `struct` type)",
                    item.display_name()
                )
            }
            InferenceDiagnostic::VariantPathOnValue { name, .. } => {
                format!("`{name}` is not a type; only an `enum` type has `::` variants")
            }
            InferenceDiagnostic::EnumCtorIsVariant { item, .. } => {
                let name = item.display_name();
                format!(
                    "`{name}` is an `enum`; construct it through one of its variants \
                     (`{name}::<variant>(...)`)"
                )
            }
            InferenceDiagnostic::NonExhaustiveMatch { uncovered, .. } => {
                let list = uncovered
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("this `match` does not cover {list}")
            }
            InferenceDiagnostic::MatchWithoutCatchAll { scrutinee, .. } => format!(
                "this `match` does not cover every possible `{}`; add a `_` arm",
                scrutinee.display()
            ),
            InferenceDiagnostic::UnreachableArm { reason, .. } => match reason {
                UnreachableReason::VariantCovered(variant) => {
                    format!("unreachable arm: `{variant}` is already covered by a previous arm")
                }
                UnreachableReason::AfterCatchAll => {
                    "unreachable arm: a previous arm already matches anything".to_owned()
                }
                UnreachableReason::AllVariantsCovered(name) => {
                    format!("unreachable arm: every variant of `{name}` is already covered")
                }
                UnreachableReason::OtherVariant { scrutinee } => format!(
                    "this arm is unreachable: the scrutinee is a `{}`",
                    scrutinee.display()
                ),
            },
            InferenceDiagnostic::NonEnumScrutineeVariantPat { scrutinee, .. } => format!(
                "only `_` or a binding can match a `{}` (for now)",
                scrutinee.display()
            ),
            InferenceDiagnostic::PatNoSuchVariant { item, name, .. } => {
                format!("`{}` has no variant `{name}`", item.display_name())
            }
            InferenceDiagnostic::BindShadowsVariant { item, name, .. } => format!(
                "`{name}` binds the whole value; write `::{name}` (or `{}::{name}`) to match the variant",
                item.display_name()
            ),
            InferenceDiagnostic::PatWrongEnum {
                item,
                variant,
                scrutinee,
                ..
            } => format!(
                "this pattern matches `{}::{variant}`, but the scrutinee is a `{}`",
                item.display_name(),
                scrutinee.display()
            ),
            InferenceDiagnostic::PatArity {
                variant,
                payloads,
                found,
                ..
            } => {
                let noun = if *payloads == 1 {
                    "payload"
                } else {
                    "payloads"
                };
                format!("`{variant}` has {payloads} {noun}, this pattern names {found}")
            }
            InferenceDiagnostic::PatPathError { message, .. } => message.clone(),
            InferenceDiagnostic::VariantPatUnknownScrutinee { .. } => {
                "cannot resolve this pattern: the type of the matched value is not known \
                 here; add a type annotation to the scrutinee"
                    .to_owned()
            }
            InferenceDiagnostic::BreakOutsideLoop { .. } => {
                "`break` outside of a loop: there is no enclosing `loop` to exit".to_owned()
            }
            InferenceDiagnostic::ContinueOutsideLoop { .. } => {
                "`continue` outside of a loop: there is no enclosing `loop` to restart".to_owned()
            }
            InferenceDiagnostic::PatUnknownField {
                name, record_ty, ..
            } => {
                format!("no field `{name}` on `{}`", record_ty.display())
            }
            InferenceDiagnostic::PatMissingFields { fields, .. } => {
                let list = fields
                    .iter()
                    .map(|(name, _)| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                if fields.len() == 1 {
                    format!("pattern does not mention field {list}; add `..` to ignore it")
                } else {
                    format!("pattern does not mention fields {list}; add `..` to ignore them")
                }
            }
            InferenceDiagnostic::PatBindingNeedsAnnotation { .. } => {
                "cannot destructure this pattern: its type is not known here; \
                 add a type annotation"
                    .to_owned()
            }
            InferenceDiagnostic::PatNotRecord { ty, .. } => format!(
                "this pattern only matches a `struct` value; found `{}`",
                ty.display()
            ),
            InferenceDiagnostic::PatUnknownType { name, .. } => {
                format!("`{name}` does not name a type")
            }
            InferenceDiagnostic::PatNamedTypeMismatch {
                expected, actual, ..
            } => format!(
                "type mismatch: expected `{}`, found `{}`",
                expected.display(),
                actual.display()
            ),
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn infer<'db>(db: &'db dyn Db, item: ItemId<'db>) -> InferenceResult {
    let body = body(db, item);
    let file = item.file(db);
    let mut table = InPlaceUnificationTable::new();
    let no_group = FxHashMap::default();
    let mut ctx;

    if let Some(root) = body.root {
        // Check the body against the item's annotation, if any.
        let ty_ref = crate::item_data(db, item)
            .as_ref()
            .and_then(|it| it.type_ref.as_ref());
        let expected = lower_type_ref(db, file, ty_ref.unwrap_or(&TypeRef::Hole), &mut table);
        let cause = ty_ref.is_some().then_some(Cause::ItemAnnotation);
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
        ctx.infer_expr_with(root, &expected, cause);
    } else {
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
    }

    ctx.finish()
}

pub(crate) struct InferCtx<'a, 'db> {
    db: &'db dyn Db,
    /// The file the body's annotations resolve type names in.
    file: SourceFile,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    table: &'a mut InPlaceUnificationTable<TyVar>,
    result: InferenceResult,
    /// Signature variables of this item's binding group (interprocedural
    /// inference): references to these items use the shared variable
    /// instead of the `signature` query, which would cycle.
    in_group: &'a FxHashMap<ItemLoc, Ty>,
    /// Deferred joins and cause provenance, solved by [`InferCtx::solve`].
    constraints: Constraints,
    /// Function-literal nesting depth (0 at the item initializer's top
    /// level): joins are tagged with it so the solver settles each function
    /// internally before anything outside consumes its type.
    scope_depth: usize,
    /// Joins being assembled during traversal (LIFO — an inner
    /// statement-position `if` completes before the construct enclosing
    /// it). See [`JoinSink`].
    join_sinks: Vec<JoinSink>,
    /// The sink the expression currently being inferred contributes to if
    /// it is a joining construct: `Some` exactly in *witness position* — a
    /// branch tail of an enclosing `if`, reached through transparent
    /// wrappers (block tails, `const` blocks). Everywhere else (`let`
    /// initializers, call arguments, statements, conditions, …) it is
    /// `None` and an `if` there resolves as its own join.
    witness_sink: Option<usize>,
    /// The loop-context stack: for each `loop` currently being inferred
    /// (innermost last), the index of the join sink its `break` values are
    /// witnesses of. Function literals and `const` blocks RESET it (a
    /// `break` never escapes those boundaries — they are units of their
    /// own); a `break`/`continue` with this empty is the outside-a-loop
    /// error.
    loop_sinks: Vec<usize>,
}

/// A join under construction. The root `if` of a nest opens one; every
/// `if`/`else` reached in witness position below it contributes its leaf
/// witnesses here instead of forming a join of its own, so the whole nest
/// resolves as ONE flat join where its value meets a non-join consumer.
/// Match arms and loop-break values contribute witnesses through the same
/// mechanism ([`InferCtx::contribute_witness`]).
struct JoinSink {
    /// Fresh variable standing for the whole nest's type.
    result: Ty,
    witnesses: Vec<Witness>,
}

impl<'a, 'db> InferCtx<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        file: SourceFile,
        body: &'db Body,
        resolutions: &'db ArenaMap<ExprId, Resolution>,
        table: &'a mut InPlaceUnificationTable<TyVar>,
        in_group: &'a FxHashMap<ItemLoc, Ty>,
    ) -> InferCtx<'a, 'db> {
        InferCtx {
            db,
            file,
            body,
            resolutions,
            table,
            result: InferenceResult::default(),
            in_group,
            constraints: Constraints::default(),
            scope_depth: 0,
            join_sinks: Vec::new(),
            witness_sink: None,
            loop_sinks: Vec::new(),
        }
    }

    /// Solve the deferred constraints. Must run after traversal in *every*
    /// mode: group inference discards the resulting diagnostics (the
    /// per-item query re-derives them) but needs the unifications for
    /// join-typed signatures to come out concrete.
    pub(crate) fn solve(&mut self) {
        let diagnostics = self.constraints.solve(self.table);
        // Widenings the join solver accepted land in the result next to
        // the ones `check` recorded directly — one map, one MIR consumer.
        for (expr, variant) in self.constraints.take_widenings() {
            self.result.widened.insert(expr, variant);
        }
        self.result.diagnostics.extend(diagnostics);
    }

    fn finish(mut self) -> InferenceResult {
        self.solve();
        let mut result = std::mem::take(&mut self.result);
        for (_, ty) in result.type_of_expr.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        for (_, ty) in result.type_of_binding.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        for diag in result.diagnostics.iter_mut() {
            match diag {
                InferenceDiagnostic::TypeMismatch {
                    expected, actual, ..
                }
                | InferenceDiagnostic::AllBranchesMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_fully(self.table, expected);
                    *actual = resolve_fully(self.table, actual);
                }
                InferenceDiagnostic::NotCallable { ty, .. } => {
                    *ty = resolve_fully(self.table, ty);
                }
                InferenceDiagnostic::IfBranchMismatch {
                    then_ty, else_ty, ..
                } => {
                    *then_ty = resolve_fully(self.table, then_ty);
                    *else_ty = resolve_fully(self.table, else_ty);
                }
                InferenceDiagnostic::RecordLitMissingFields { fields, .. } => {
                    for (_, ty) in fields.iter_mut() {
                        *ty = resolve_fully(self.table, ty);
                    }
                }
                InferenceDiagnostic::RecordLitExtraField { expected, .. } => {
                    *expected = resolve_fully(self.table, expected);
                }
                InferenceDiagnostic::NoSuchField { receiver_ty, .. } => {
                    *receiver_ty = resolve_fully(self.table, receiver_ty);
                }
                InferenceDiagnostic::MatchWithoutCatchAll { scrutinee, .. }
                | InferenceDiagnostic::NonEnumScrutineeVariantPat { scrutinee, .. }
                | InferenceDiagnostic::PatWrongEnum { scrutinee, .. }
                | InferenceDiagnostic::UnreachableArm {
                    reason: UnreachableReason::OtherVariant { scrutinee },
                    ..
                } => {
                    *scrutinee = resolve_fully(self.table, scrutinee);
                }
                InferenceDiagnostic::PatUnknownField { record_ty, .. } => {
                    *record_ty = resolve_fully(self.table, record_ty);
                }
                InferenceDiagnostic::PatMissingFields { fields, .. } => {
                    for (_, ty) in fields.iter_mut() {
                        *ty = resolve_fully(self.table, ty);
                    }
                }
                InferenceDiagnostic::PatNotRecord { ty, .. } => {
                    *ty = resolve_fully(self.table, ty);
                }
                InferenceDiagnostic::PatNamedTypeMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_fully(self.table, expected);
                    *actual = resolve_fully(self.table, actual);
                }
                InferenceDiagnostic::ArgCountMismatch { .. }
                | InferenceDiagnostic::NeedsAnnotation { .. }
                | InferenceDiagnostic::AssignToImmutable { .. }
                | InferenceDiagnostic::AssignToItem { .. }
                | InferenceDiagnostic::AssignToBuiltin { .. }
                | InferenceDiagnostic::FieldOnUnknownType { .. }
                | InferenceDiagnostic::TypeNotValue { .. }
                | InferenceDiagnostic::TypeCtorArgCount { .. }
                | InferenceDiagnostic::NoSuchVariant { .. }
                | InferenceDiagnostic::NoVariantsOnStruct { .. }
                | InferenceDiagnostic::VariantPathOnValue { .. }
                | InferenceDiagnostic::EnumCtorIsVariant { .. }
                | InferenceDiagnostic::NonExhaustiveMatch { .. }
                | InferenceDiagnostic::UnreachableArm { .. }
                | InferenceDiagnostic::PatNoSuchVariant { .. }
                | InferenceDiagnostic::PatArity { .. }
                | InferenceDiagnostic::PatPathError { .. }
                | InferenceDiagnostic::VariantPatUnknownScrutinee { .. }
                | InferenceDiagnostic::BindShadowsVariant { .. }
                | InferenceDiagnostic::BreakOutsideLoop { .. }
                | InferenceDiagnostic::ContinueOutsideLoop { .. }
                | InferenceDiagnostic::PatBindingNeedsAnnotation { .. }
                | InferenceDiagnostic::PatUnknownType { .. } => {}
            }
        }
        for (_, ty) in result.type_of_pat.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        result
    }

    fn fresh_var(&mut self) -> Ty {
        Ty::Infer(self.table.new_key(TyVarValue::Unknown))
    }

    /// Infer `expr`; if `expected` is given, check against it (recording a
    /// diagnostic on mismatch and recovering with the expected type).
    pub(crate) fn infer_expr(&mut self, expr: ExprId, expected: &Ty) -> Ty {
        self.infer_expr_with(expr, expected, None)
    }

    /// As [`InferCtx::infer_expr`], with the [`Cause`] that makes `expected`
    /// expected — recorded when the expectation binds a type variable, and
    /// cited when the check fails outright.
    fn infer_expr_with(&mut self, expr: ExprId, expected: &Ty, cause: Option<Cause>) -> Ty {
        // A join sink propagates only through transparent wrappers (block
        // tails, `const` blocks) into an `if`'s witness position. Any other
        // expression is a leaf of the enclosing join, and its
        // subexpressions are ordinary non-witness positions — an `if`
        // inside a call argument or a statement resolves as its own join.
        let sink = self.witness_sink.take();
        let ty = match &self.body.exprs[expr] {
            ExprData::Missing => Ty::Error,
            ExprData::Literal(LiteralData::Int(_)) => Ty::Int,
            ExprData::Literal(LiteralData::Str(_)) => Ty::Str,
            ExprData::Literal(LiteralData::Bool(_)) => Ty::Bool,
            ExprData::NameRef(name) => match self.resolutions.get(expr) {
                // A type is not a first-class value. The one legal
                // expression position for a type name — the head of a
                // construction call — is intercepted in the `Call` arm and
                // never infers the callee, so reaching this *is* the error.
                Some(Resolution::TypeItem(_)) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::TypeNotValue {
                            expr,
                            name: name.clone(),
                        });
                    Ty::Error
                }
                Some(Resolution::Local(binding)) => self
                    .result
                    .type_of_binding
                    .get(*binding)
                    .cloned()
                    .unwrap_or(Ty::Error),
                Some(Resolution::Item(loc)) => {
                    // A member of this item's own binding group resolves to
                    // its shared signature variable — that's interprocedural
                    // inference happening.
                    if let Some(member_sig) = self.in_group.get(loc) {
                        member_sig.clone()
                    } else {
                        let target = loc.to_id(self.db);
                        let sig = signature(self.db, target);
                        // Inference couldn't determine the signature from
                        // the definition: that's only visible from uses (an
                        // unused undetermined item is fine), so the
                        // diagnostic lives here.
                        if signature_needs_annotation(self.db, target) {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NeedsAnnotation {
                                    expr,
                                    item: loc.clone(),
                                });
                        }
                        sig
                    }
                }
                // No one signature a use could take on; the duplicate
                // definitions carry the diagnostic.
                Some(Resolution::Ambiguous(_)) => Ty::Error,
                Some(Resolution::Builtin(builtin)) => builtin_type(*builtin),
                None => Ty::Error, // unresolved: already diagnosed by name resolution
            },
            ExprData::VariantPath { base, variant } => {
                self.infer_variant_path(expr, *base, variant)
            }
            ExprData::Call { callee, args } => {
                // A construction call: the type name used as a plain
                // constructor function taking the underlying record —
                // `Foo(struct { x: 1 })`. Intercepted before the callee is
                // inferred (a bare type name in expression position is an
                // error; as a construction head it is the one legal use).
                if let Some(Resolution::TypeItem(loc)) = self.resolutions.get(*callee).cloned() {
                    return self.infer_construction(expr, *callee, &loc, args, expected, cause);
                }
                let fresh = self.fresh_var();
                let callee_ty = self.infer_expr(*callee, &fresh);
                match self.resolve_shallow(&callee_ty) {
                    // The callee's type is still being inferred (an
                    // in-group signature, e.g. mutual recursion): calling
                    // it commits it to a function of this shape.
                    Ty::Infer(var) => {
                        let params: Vec<Ty> = args.iter().map(|_| self.fresh_var()).collect();
                        let ret = self.fresh_var();
                        if self.unify(&Ty::Infer(var), &Ty::fn_type(params.clone(), ret.clone())) {
                            for (i, &arg) in args.iter().enumerate() {
                                self.infer_expr(arg, &params[i]);
                            }
                            ret
                        } else {
                            // The commitment contradicts what the callee's
                            // signature is already committed to: poison it
                            // so the member reports via the needs-annotation
                            // path instead of publishing a guess.
                            self.table.union_value(var, TyVarValue::Known(Ty::Error));
                            for &arg in args {
                                let arg_fresh = self.fresh_var();
                                self.infer_expr(arg, &arg_fresh);
                            }
                            Ty::Error
                        }
                    }
                    Ty::Fn(f) => {
                        if f.params.len() != args.len() {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::ArgCountMismatch {
                                    expr,
                                    expected: f.params.len(),
                                    found: args.len(),
                                });
                        }
                        for (i, &arg) in args.iter().enumerate() {
                            // The parameter type is an axiom: the cause is
                            // recorded when it binds a variable (blaming a
                            // branch through this call) and cited on direct
                            // mismatches (where the renderer drops it as
                            // self-evident: the call encloses the argument).
                            let param = f.params.get(i).cloned().unwrap_or(Ty::Error);
                            self.infer_expr_with(
                                arg,
                                &param,
                                Some(Cause::CallSite { call: expr, arg }),
                            );
                        }
                        f.ret.clone()
                    }
                    Ty::Error => {
                        for &arg in args {
                            let arg_fresh = self.fresh_var();
                            self.infer_expr(arg, &arg_fresh);
                        }
                        Ty::Error
                    }
                    // Evaluating the callee already diverges, so the call
                    // diverges; `{error}` here would be an error type with
                    // no diagnostic to explain it.
                    Ty::Never => {
                        for &arg in args {
                            let arg_fresh = self.fresh_var();
                            // TODO: If we cannot infer the type of something, but we can see it's
                            // unreachable, it would be ok for it to be a warning rather than an
                            // error
                            self.infer_expr(arg, &arg_fresh);
                        }
                        Ty::Never
                    }
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NotCallable {
                                expr: *callee,
                                ty: other,
                            });
                        for &arg in args {
                            let fresh_var = self.fresh_var();
                            self.infer_expr(arg, &fresh_var);
                        }
                        Ty::Error
                    }
                }
            }
            ExprData::Bin { op, lhs, rhs } => {
                use crate::body::BinOp::*;
                match op {
                    Some(Add | Sub | Mul | Div) => {
                        self.infer_expr_with(*lhs, &Ty::Int, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &Ty::Int, Some(Cause::Operator(expr)));
                        Ty::Int
                    }
                    Some(Lt | Le | Gt | Ge) => {
                        self.infer_expr_with(*lhs, &Ty::Int, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &Ty::Int, Some(Cause::Operator(expr)));
                        Ty::Bool
                    }
                    // Equality works on any type; the operands just have to
                    // agree with each other — whatever the first operand
                    // concluded, the second must match.
                    Some(Eq | Ne) => {
                        let fresh = self.fresh_var();
                        self.infer_expr(*lhs, &fresh);
                        self.infer_expr_with(*rhs, &fresh, Some(Cause::Operand(*lhs)));
                        Ty::Bool
                    }
                    // No operator token means broken source with its own
                    // parse error.
                    None => {
                        let lhs_fresh = self.fresh_var();
                        let rhs_fresh = self.fresh_var();
                        self.infer_expr(*lhs, &lhs_fresh);
                        self.infer_expr(*rhs, &rhs_fresh);
                        Ty::Error
                    }
                }
            }
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.infer_expr_with(*condition, &Ty::Bool, Some(Cause::Condition(expr)));
                let Some(else_branch) = else_branch else {
                    // Without an `else`, the value when the condition is
                    // false is `()`, so the then-branch must be too.
                    self.infer_expr_with(*then_branch, &Ty::Unit, Some(Cause::MissingElse(expr)));
                    let ty = self.check(expr, Ty::Unit, expected, cause);
                    self.result.type_of_expr.insert(expr, ty.clone());
                    return ty;
                };
                // Statement vs. witness position, decided by `sink`.
                // Reached through a transparent tail chain from an
                // enclosing `if`'s branch, this `if` is a witness: its
                // leaves join the enclosing sink and the whole nest
                // resolves as one flat join where the outermost `if`'s
                // value meets a non-join consumer. Anywhere else the join
                // resolves here: open a fresh sink.
                let (sink_index, is_root) = match sink {
                    Some(index) => (index, false),
                    None => {
                        let result = self.fresh_var();
                        self.join_sinks.push(JoinSink {
                            result,
                            witnesses: Vec::new(),
                        });
                        (self.join_sinks.len() - 1, true)
                    }
                };
                // Each branch gets an independent fresh variable so outer
                // expectation pressure never leaks in: the branches' honest
                // types are what the join judges.
                let then_fresh = self.fresh_var();
                self.witness_sink = Some(sink_index);
                let then_ty = self.infer_expr(*then_branch, &then_fresh);
                self.contribute_witness(sink_index, *then_branch, &then_ty);
                let else_fresh = self.fresh_var();
                self.witness_sink = Some(sink_index);
                let else_ty = self.infer_expr(*else_branch, &else_fresh);
                self.contribute_witness(sink_index, *else_branch, &else_ty);
                let diverges = matches!(self.resolve_shallow(&then_ty), Ty::Never)
                    && matches!(self.resolve_shallow(&else_ty), Ty::Never);
                if !is_root {
                    // A nested `if` types as the enclosing join's result:
                    // hover on it shows the whole nest's resolved type —
                    // there is no intermediate "the inner if has type …"
                    // verdict of its own.
                    if diverges {
                        Ty::Never
                    } else {
                        self.join_sinks[sink_index].result.clone()
                    }
                } else {
                    let JoinSink { result, witnesses } =
                        self.join_sinks.pop().expect("sink pushed above");
                    match witnesses.len() {
                        // Every leaf diverges: so does the `if`.
                        0 => {
                            self.unify(&result, &Ty::Never);
                            Ty::Never
                        }
                        // One surviving leaf (the rest diverged): its type
                        // is the `if`'s type outright, no agreement left to
                        // defer. Unified into `result` so nested `if`s that
                        // routed the leaf here still resolve.
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.unify(&result, &ty);
                            ty
                        }
                        // Leaf agreement is a join: deferred so axioms
                        // arriving later in the traversal (an annotation
                        // above, the call this feeds into below) pick the
                        // winner before the leaves are played against each
                        // other.
                        _ => {
                            self.constraints.push_join(Join {
                                expr,
                                depth: self.scope_depth,
                                result: result.clone(),
                                witnesses,
                            });
                            result
                        }
                    }
                }
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let {
                            pat,
                            type_ref,
                            init,
                        } => {
                            if let PatData::Bind(binding) = self.body.pats[*pat].clone() {
                                // The common case, unchanged: a bare name's
                                // own annotation (if any) is the axiom.
                                let has_annotation = self.body.bindings[binding].type_ref.is_some();
                                let declared = self.body.bindings[binding]
                                    .type_ref
                                    .as_ref()
                                    .map(|it| lower_type_ref(self.db, self.file, it, self.table))
                                    .unwrap_or_else(|| self.fresh_var());
                                let binding_cause =
                                    has_annotation.then_some(Cause::Binding(binding));
                                let mut ty = self.infer_expr_with(*init, &declared, binding_cause);
                                // `let mut` widening: an UNANNOTATED mutable
                                // binding initialized with a variant-typed value
                                // widens to the enum at binding time (with the
                                // conversion on the initializer) — a `mut`
                                // state variable is meant to be reassigned
                                // across variants. `let mut x: Shape::Circle`
                                // keeps precision (the annotation is the
                                // axiom), and a plain `let` keeps the variant.
                                // Applies when the initializer's type is
                                // already known here; a join-typed initializer
                                // resolves later, driven by the axioms its
                                // uses provide.
                                if !has_annotation
                                    && self.body.bindings[binding].mutable
                                    && let Ty::Variant(variant) = self.resolve_shallow(&ty)
                                {
                                    self.result.widened.insert(*init, variant.clone());
                                    ty = Ty::Named(variant.decl);
                                }
                                self.result.type_of_binding.insert(binding, ty.clone());
                                self.result.type_of_pat.insert(*pat, ty);
                            } else {
                                // A destructuring pattern: its own written
                                // annotation is the axiom; failing that, a
                                // `Newtype` pattern names its own type
                                // outright (mirrors a construction call's
                                // callee) — a bare `Record` pattern has
                                // nothing to go on and needs one explicitly.
                                let declared = match type_ref {
                                    Some(tr) => lower_type_ref(self.db, self.file, tr, self.table),
                                    None => self
                                        .declared_type_for_pat(*pat)
                                        .unwrap_or_else(|| self.fresh_var()),
                                };
                                let ty = self.infer_expr_with(*init, &declared, None);
                                self.check_pat(*pat, &ty, *init);
                            }
                        }
                        Stmt::Assign { target, value } => {
                            // The target is an ordinary read for typing
                            // purposes — it just so happens to also be
                            // written. Its own expectation is free (no
                            // outer axiom constrains what's being assigned
                            // to); the resulting type is what the value
                            // must match, same as a `let` with a declared
                            // type flows its annotation down as `expected`
                            // with `Cause::Binding`.
                            let fresh = self.fresh_var();
                            let target_ty = self.infer_expr(*target, &fresh);
                            // Enforcement: the only legal target is a `mut`
                            // local. Unresolved names already carry the
                            // unresolved-name diagnostic, non-name targets a
                            // validation error, and ambiguous names the
                            // duplicate-definition diagnostics — none of
                            // those gets a second squiggle here.
                            let cause = match self.resolutions.get(*target) {
                                Some(Resolution::Local(binding)) => {
                                    let data = &self.body.bindings[*binding];
                                    if !data.mutable {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::AssignToImmutable {
                                                target: *target,
                                                binding: *binding,
                                                name: data.name.clone(),
                                            },
                                        );
                                    }
                                    Some(Cause::Binding(*binding))
                                }
                                Some(Resolution::Item(loc)) => {
                                    let constness = crate::item_data(self.db, loc.to_id(self.db))
                                        .as_ref()
                                        .and_then(|it| it.kind.constness())
                                        .unwrap_or(Constness::Static);
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::AssignToItem {
                                            target: *target,
                                            item: loc.clone(),
                                            constness,
                                        },
                                    );
                                    None
                                }
                                // The target read already reported "is a
                                // type, not a value" (the `NameRef` arm ran
                                // on it); a second assignment-specific
                                // squiggle on the same name adds nothing.
                                Some(Resolution::TypeItem(_)) => None,
                                Some(Resolution::Builtin(builtin)) => {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::AssignToBuiltin {
                                            target: *target,
                                            builtin: *builtin,
                                        },
                                    );
                                    None
                                }
                                Some(Resolution::Ambiguous(_)) | None => None,
                            };
                            self.infer_expr_with(*value, &target_ty, cause);
                        }
                        Stmt::Expr(e) => {
                            let fresh_var = self.fresh_var();
                            self.infer_expr(*e, &fresh_var);
                        }
                    }
                }
                match tail {
                    // Propagate the expectation so mismatches point at the
                    // tail expression, then skip re-checking at block level.
                    Some(tail) => {
                        // The join sink survives only along the tail chain
                        // (witness position); the statements above were
                        // ordinary non-witness positions.
                        self.witness_sink = sink;
                        let ty = self.infer_expr_with(*tail, expected, cause);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        return ty;
                    }
                    None => Ty::Unit,
                }
            }
            // Fully transparent for typing: same expectation and cause flow
            // straight through to `body`, so a mismatch is reported (and
            // blamed) exactly as if the `const` wrapper weren't there. This
            // expression still gets its own entry in `type_of_expr` (the
            // early return below), so hover on the `const { ... }` itself
            // shows the right type.
            ExprData::ConstBlock { body: inner } => {
                // Transparent for the join sink too, matching
                // `peel_blocks`: an `if` at a `const` block's core is still
                // in witness position. NOT transparent for the loop
                // context: a `const` block is a compile-time unit of its
                // own (MIR lowers it to a separate body), so a `break`
                // inside it cannot exit a loop outside it.
                self.witness_sink = sink;
                let saved_loops = std::mem::take(&mut self.loop_sinks);
                let ty = self.infer_expr_with(*inner, expected, cause);
                self.loop_sinks = saved_loops;
                self.result.type_of_expr.insert(expr, ty.clone());
                return ty;
            }
            ExprData::RecordLit { fields } => {
                // Bidirectional: when the context already expects a record,
                // the expected field types flow into the field initializers
                // (with the same cause, so a wrong field blames the field's
                // expression and cites the annotation/call that demanded the
                // type — same shape as call arguments and annotated lets).
                if let Ty::Record(expected_rec) = self.resolve_shallow(expected) {
                    let has = |name: &str| fields.iter().any(|(n, _)| n.as_str() == name);
                    let missing: Vec<(String, Ty)> = expected_rec
                        .fields
                        .iter()
                        .filter(|(name, _)| !has(name))
                        .cloned()
                        .collect();
                    if !missing.is_empty() {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::RecordLitMissingFields {
                                expr,
                                fields: missing,
                            });
                    }
                    let mut seen: Vec<&str> = Vec::new();
                    for (name, field_expr) in fields {
                        match expected_rec.field_ty(name) {
                            Some(field_ty) => {
                                self.infer_expr_with(*field_expr, &field_ty.clone(), cause);
                            }
                            None => {
                                // An extra field (exact field-set equality:
                                // nothing is dropped silently). Duplicates of
                                // one extra name get a single diagnostic —
                                // validation already flags the duplication.
                                if !seen.contains(&name.as_str()) {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::RecordLitExtraField {
                                            expr: *field_expr,
                                            name: name.clone(),
                                            expected: Ty::Record(expected_rec.clone()),
                                        },
                                    );
                                }
                                let fresh = self.fresh_var();
                                self.infer_expr(*field_expr, &fresh);
                            }
                        }
                        seen.push(name.as_str());
                    }
                    // Field mismatches were reported above (or the sets
                    // match); either way the literal recovers with the
                    // expected type so nothing cascades.
                    let ty = Ty::Record(expected_rec);
                    self.result.type_of_expr.insert(expr, ty.clone());
                    return ty;
                }
                // No record expectation: infer every field and conclude a
                // record type, then let the ordinary check judge it (binding
                // a free variable, or reporting a plain mismatch against a
                // non-record expectation).
                let field_tys = fields
                    .iter()
                    .map(|(name, field_expr)| {
                        let fresh = self.fresh_var();
                        (name.clone(), self.infer_expr(*field_expr, &fresh))
                    })
                    .collect();
                Ty::record(field_tys)
            }
            ExprData::Field { receiver, name } => {
                let fresh = self.fresh_var();
                let receiver_ty = self.infer_expr(*receiver, &fresh);
                if name.is_empty() {
                    // `a.` — the parse error covers it.
                    Ty::Error
                } else {
                    match self.resolve_shallow(&receiver_ty) {
                        Ty::Record(rec) => match rec.field_ty(name) {
                            Some(field_ty) => field_ty.clone(),
                            None => {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::NoSuchField {
                                        expr,
                                        name: name.clone(),
                                        receiver_ty: Ty::Record(rec),
                                    });
                                Ty::Error
                            }
                        },
                        // A named type projects through to its declared
                        // shape: `p.x` on a `Foo` works exactly as on the
                        // underlying record.
                        Ty::Named(loc) => {
                            match type_underlying(self.db, loc.to_id(self.db)) {
                                Some(Ty::Record(rec)) => match rec.field_ty(name) {
                                    Some(field_ty) => field_ty.clone(),
                                    None => {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::NoSuchField {
                                                expr,
                                                name: name.clone(),
                                                receiver_ty: Ty::Named(loc),
                                            },
                                        );
                                        Ty::Error
                                    }
                                },
                                _ => {
                                    // An enum value has no fields at all
                                    // (v1 payloads are positional and only
                                    // reachable through `match`, later); a
                                    // broken declaration's own diagnostic
                                    // sits at the declaration site — stay
                                    // silent for it.
                                    if enum_variants(self.db, loc.to_id(self.db)).is_some() {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::NoSuchField {
                                                expr,
                                                name: name.clone(),
                                                receiver_ty: Ty::Named(loc),
                                            },
                                        );
                                    }
                                    Ty::Error
                                }
                            }
                        }
                        // Evaluating the receiver already diverges (same
                        // reasoning as a diverging callee).
                        Ty::Never => Ty::Never,
                        // Structural equality can't run backwards from a
                        // field name, so an undetermined receiver stays
                        // undetermined: ask for the annotation.
                        Ty::Infer(_) => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::FieldOnUnknownType {
                                    expr,
                                    receiver: *receiver,
                                });
                            Ty::Error
                        }
                        // Errors are infectious and silent — a broken
                        // receiver must not cascade into field diagnostics.
                        broken if broken.contains_error() => Ty::Error,
                        other => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NoSuchField {
                                    expr,
                                    name: name.clone(),
                                    receiver_ty: other,
                                });
                            Ty::Error
                        }
                    }
                }
            }
            ExprData::FnLiteral {
                params,
                ret_type,
                body: fn_body,
                // Const-checking is a separate pass (`const_check`);
                // `is_const` doesn't affect typing here.
                is_const: _,
            } => {
                let param_tys: Vec<Ty> = params
                    .iter()
                    .map(|param| {
                        let ty = if let PatData::Bind(binding) = &self.body.pats[param.pat] {
                            // The common case, unchanged: a bare name's own
                            // annotation (if any) is the axiom.
                            match &self.body.bindings[*binding].type_ref {
                                Some(type_ref) => {
                                    lower_type_ref(self.db, self.file, type_ref, self.table)
                                }
                                None => self.fresh_var(),
                            }
                        } else {
                            // A destructuring parameter: its own written
                            // annotation is the axiom; failing that, a
                            // `Newtype` pattern names its own type outright
                            // (`fn (Foo(...))` needs no annotation).
                            match &param.type_ref {
                                Some(type_ref) => {
                                    lower_type_ref(self.db, self.file, type_ref, self.table)
                                }
                                None => self
                                    .declared_type_for_pat(param.pat)
                                    .unwrap_or_else(|| self.fresh_var()),
                            }
                        };
                        // A parameter pattern has no per-call site to blame
                        // a broken destructure on; the whole body is the
                        // best available anchor (every call runs it).
                        self.check_pat(param.pat, &ty, *fn_body);
                        ty
                    })
                    .collect();
                let ret = match ret_type {
                    Some(type_ref) => lower_type_ref(self.db, self.file, type_ref, self.table),
                    None => self.fresh_var(),
                };
                let ret_cause = ret_type.is_some().then_some(Cause::ReturnAnnotation(expr));
                // The function is a unit that must be internally
                // consistent: its joins solve (by depth) before any outer
                // join consumes its type, and they never flatten into one —
                // the sink was not restored for the body, so a function
                // literal in witness position is one opaque leaf and its
                // body-tail `if` is a root join of its own (the return type
                // is the boundary the join resolves against). The loop
                // context resets the same way: a `break` in the body never
                // exits a loop enclosing the literal (fns bound everything).
                self.scope_depth += 1;
                let saved_loops = std::mem::take(&mut self.loop_sinks);
                self.infer_expr_with(*fn_body, &ret, ret_cause);
                self.loop_sinks = saved_loops;
                self.scope_depth -= 1;
                Ty::fn_type(param_tys, ret)
            }
            ExprData::Match { scrutinee, arms } => {
                return self.infer_match(expr, *scrutinee, arms, sink, expected, cause);
            }
            ExprData::Loop { body: loop_body } => {
                // The BREAK VALUES are the witnesses of one join whose
                // result is the loop's type. Statement vs. witness
                // position, exactly as for `if`/`match`: a loop in witness
                // position contributes its break values to the enclosing
                // join, anywhere else it resolves a join of its own.
                let (sink_index, is_root) = match sink {
                    Some(index) => (index, false),
                    None => {
                        let result = self.fresh_var();
                        self.join_sinks.push(JoinSink {
                            result,
                            witnesses: Vec::new(),
                        });
                        (self.join_sinks.len() - 1, true)
                    }
                };
                let witnesses_before = self.join_sinks[sink_index].witnesses.len();
                // The body's own tail value is discarded — running off the
                // body's end continues the loop — so it is inferred free:
                // only `break` produces the loop's value.
                self.loop_sinks.push(sink_index);
                let body_fresh = self.fresh_var();
                self.infer_expr(*loop_body, &body_fresh);
                self.loop_sinks.pop();
                // Everything contributed to the sink during the body came
                // from this loop's breaks (nested constructs in statement
                // position open sinks of their own, LIFO): no contribution
                // means no break ever carries a value out — the loop never
                // finishes, `!`, and the never machinery (widening, no
                // vote) takes it from there.
                let broke_with_value =
                    self.join_sinks[sink_index].witnesses.len() > witnesses_before;
                if !is_root {
                    // A nested loop types as the enclosing join's result,
                    // exactly like a nested `if`/`match`.
                    if broke_with_value {
                        self.join_sinks[sink_index].result.clone()
                    } else {
                        Ty::Never
                    }
                } else {
                    let JoinSink { result, witnesses } =
                        self.join_sinks.pop().expect("sink pushed above");
                    match witnesses.len() {
                        // No break carries a value out: an infinite loop.
                        0 => {
                            self.unify(&result, &Ty::Never);
                            Ty::Never
                        }
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.unify(&result, &ty);
                            ty
                        }
                        _ => {
                            self.constraints.push_join(Join {
                                expr,
                                depth: self.scope_depth,
                                result: result.clone(),
                                witnesses,
                            });
                            result
                        }
                    }
                }
            }
            ExprData::Break { value } => match self.loop_sinks.last().copied() {
                Some(sink_index) => {
                    match value {
                        Some(value) => {
                            // The value sits in witness position of the
                            // loop's join: an `if`/`match`/`loop` at its
                            // core flattens its leaves into the same join,
                            // so blame speaks about the leaves.
                            let fresh = self.fresh_var();
                            self.witness_sink = Some(sink_index);
                            let value_ty = self.infer_expr(*value, &fresh);
                            self.contribute_witness(sink_index, *value, &value_ty);
                        }
                        // `break;` is a witness of type `()`.
                        None => self.contribute_witness(sink_index, expr, &Ty::Unit),
                    }
                    // The break expression itself never produces a value.
                    Ty::Never
                }
                None => {
                    // Outside any loop (or across a fn-literal/`const`
                    // block boundary). The value is still inferred so its
                    // contents get types and diagnostics.
                    if let Some(value) = value {
                        let fresh = self.fresh_var();
                        self.infer_expr(*value, &fresh);
                    }
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::BreakOutsideLoop { expr });
                    Ty::Error
                }
            },
            ExprData::Continue => {
                if self.loop_sinks.is_empty() {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::ContinueOutsideLoop { expr });
                    Ty::Error
                } else {
                    Ty::Never
                }
            }
        };

        let ty = self.check(expr, ty, expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// Resolve `Enum::Variant` in expression position — the type-directed
    /// second-segment resolution: the base's resolution comes from `scopes`
    /// like any name, the variant is looked up in the enum's declaration
    /// right here. A payload-less variant *is* the value (typed
    /// [`Ty::Variant`]); a variant with payloads is a constructor function
    /// `fn(payload...) -> Enum::Variant` — first-class, and a direct call
    /// of it goes through the ordinary `Call` machinery (arity errors are
    /// plain `ArgCountMismatch`es).
    ///
    /// The base is deliberately *not* inferred as an expression: a bare
    /// type name in expression position is an error (`TypeNotValue`), but
    /// as a variant path's base it is legal — the same interception idea
    /// as construction heads.
    fn infer_variant_path(&mut self, expr: ExprId, base: ExprId, variant: &str) -> Ty {
        match self.resolutions.get(base) {
            Some(Resolution::TypeItem(loc)) => {
                let item = loc.to_id(self.db);
                let Some(variants) = enum_variants(self.db, item).as_ref() else {
                    // A struct type has no variants; a broken declaration
                    // carries its own diagnostics (infectious, silent).
                    if matches!(
                        crate::type_decl(self.db, item),
                        Some(TypeDeclData::Struct { .. })
                    ) {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NoVariantsOnStruct {
                                expr,
                                item: loc.clone(),
                            });
                    }
                    return Ty::Error;
                };
                // `Shape::` — the parse error covers the missing name.
                if variant.is_empty() {
                    return Ty::Error;
                }
                match variants.iter().position(|(name, _)| name == variant) {
                    Some(index) => {
                        let variant_ty = VariantTy {
                            decl: loc.clone(),
                            index: index as u32,
                            name: std::sync::Arc::from(variant),
                        };
                        self.result.variant_of_expr.insert(expr, variant_ty.clone());
                        let payload = &variants[index].1;
                        if payload.is_empty() {
                            Ty::Variant(variant_ty)
                        } else {
                            Ty::fn_type(payload.clone(), Ty::Variant(variant_ty))
                        }
                    }
                    None => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NoSuchVariant {
                                expr,
                                item: loc.clone(),
                                name: variant.to_owned(),
                            });
                        Ty::Error
                    }
                }
            }
            Some(Resolution::Local(_) | Resolution::Item(_) | Resolution::Builtin(_)) => {
                let name = match &self.body.exprs[base] {
                    ExprData::NameRef(name) => name.clone(),
                    _ => String::new(),
                };
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::VariantPathOnValue { expr, name });
                Ty::Error
            }
            // Duplicate definitions / an unresolved base carry their own
            // diagnostics (the base is an ordinary `NameRef` to name
            // resolution).
            Some(Resolution::Ambiguous(_)) | None => Ty::Error,
        }
    }

    /// A `match` expression. The arms are witnesses of ONE join,
    /// contributed through the same seam as `if`/`else` branches
    /// ([`InferCtx::contribute_witness`]): in witness position the arms'
    /// leaves flatten into the enclosing join, anywhere else the match
    /// opens a join of its own — same-variant arms keep their precision,
    /// mixed variants of one enum LUB to the enum, blame speaks about the
    /// arm tails. Patterns are checked against the scrutinee's type
    /// (variant names resolve type-directed, like `::` paths do), and flat
    /// set-cover over variant indices decides exhaustiveness.
    fn infer_match(
        &mut self,
        expr: ExprId,
        scrutinee: ExprId,
        arms: &[MatchArm],
        sink: Option<usize>,
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        let scrut_fresh = self.fresh_var();
        let scrut_ty = self.infer_expr(scrutinee, &scrut_fresh);
        // A qualified variant pattern can pin a still-unknown scrutinee:
        // `Shape::Circle(r) =>` says the value dispatches over `Shape`
        // (patterns are construction's mirror image, so the qualified
        // spelling carries the same type information construction does).
        if matches!(self.resolve_shallow(&scrut_ty), Ty::Infer(_)) {
            for arm in arms {
                if let PatData::Variant {
                    enum_name: Some(enum_name),
                    ..
                } = &self.body.pats[arm.pat]
                    && let Some(Resolution::TypeItem(loc)) =
                        type_scope(self.db, self.file).resolve(enum_name)
                    && enum_variants(self.db, loc.to_id(self.db)).is_some()
                {
                    self.unify(&scrut_ty, &Ty::Named(loc.clone()));
                    break;
                }
            }
        }
        let scrut = match self.resolve_shallow(&scrut_ty) {
            Ty::Named(loc) => {
                if enum_variants(self.db, loc.to_id(self.db)).is_some() {
                    Scrutinee::Enum(loc)
                } else if matches!(
                    crate::type_decl(self.db, loc.to_id(self.db)),
                    Some(TypeDeclData::Struct { .. })
                ) {
                    Scrutinee::Other(Ty::Named(loc))
                } else {
                    // A broken declaration: its own diagnostics sit at the
                    // declaration site (infectious, silent).
                    Scrutinee::Error
                }
            }
            Ty::Variant(variant) => Scrutinee::Variant(variant),
            Ty::Infer(var) => Scrutinee::Unknown(Ty::Infer(var)),
            // A diverging or broken scrutinee: the arms never run; stay
            // silent (the scrutinee carries its own story).
            Ty::Error | Ty::Never => Scrutinee::Error,
            other => Scrutinee::Other(other),
        };

        // Statement vs. witness position, exactly as for `if` (see there).
        let (sink_index, is_root) = match sink {
            Some(index) => (index, false),
            None => {
                let result = self.fresh_var();
                self.join_sinks.push(JoinSink {
                    result,
                    witnesses: Vec::new(),
                });
                (self.join_sinks.len() - 1, true)
            }
        };

        // Flat set-cover over variant indices: which variants the arms
        // reach, and whether a catch-all (`_`/binding — or the scrutinee's
        // own variant on a variant-typed scrutinee) has been seen.
        let n_variants = match &scrut {
            Scrutinee::Enum(loc) => enum_variants(self.db, loc.to_id(self.db))
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0),
            _ => 0,
        };
        let mut covered = vec![false; n_variants];
        let mut catch_all = false;
        let mut all_diverge = true;
        for arm in arms {
            let cover = self.check_match_pat(expr, arm.pat, &scrut);
            match cover {
                Cover::Nothing => {}
                _ if catch_all => {
                    // Which earlier arm to blame is in `catch_all` already;
                    // a duplicate *variant* still reads better named.
                    let reason = match (&cover, self.result.variant_of_pat.get(arm.pat)) {
                        (Cover::Variant(_), Some(vt)) => {
                            UnreachableReason::VariantCovered(vt.name.to_string())
                        }
                        _ => UnreachableReason::AfterCatchAll,
                    };
                    self.push_unreachable(expr, arm.pat, reason);
                }
                Cover::Variant(index) => {
                    let index = index as usize;
                    if covered[index] {
                        let name = self
                            .result
                            .variant_of_pat
                            .get(arm.pat)
                            .map(|vt| vt.name.to_string())
                            .unwrap_or_default();
                        self.push_unreachable(
                            expr,
                            arm.pat,
                            UnreachableReason::VariantCovered(name),
                        );
                    } else {
                        covered[index] = true;
                    }
                }
                Cover::All => {
                    if n_variants > 0 && covered.iter().all(|&c| c) {
                        let name = match &scrut {
                            Scrutinee::Enum(loc) => loc.display_name().to_owned(),
                            _ => String::new(),
                        };
                        self.push_unreachable(
                            expr,
                            arm.pat,
                            UnreachableReason::AllVariantsCovered(name),
                        );
                    }
                    // On a variant-typed scrutinee a same-variant arm is
                    // irrefutable — everything after it is dead, same as a
                    // wildcard.
                    catch_all = true;
                }
            }
            let arm_fresh = self.fresh_var();
            self.witness_sink = Some(sink_index);
            let arm_ty = self.infer_expr(arm.body, &arm_fresh);
            self.contribute_witness(sink_index, arm.body, &arm_ty);
            if !matches!(self.resolve_shallow(&arm_ty), Ty::Never) {
                all_diverge = false;
            }
        }

        // Exhaustiveness. Skipped for unknown/broken scrutinees — there is
        // no value universe to cover (and the pattern diagnostics above
        // already said what's wrong).
        if !catch_all {
            match &scrut {
                Scrutinee::Enum(loc) => {
                    if !covered.iter().all(|&c| c) {
                        let variants = enum_variants(self.db, loc.to_id(self.db))
                            .as_ref()
                            .expect("classified as an enum above");
                        let uncovered = variants
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| !covered[i])
                            .map(|(_, (name, _))| format!("{}::{name}", loc.display_name()))
                            .collect();
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NonExhaustiveMatch { expr, uncovered });
                    }
                }
                Scrutinee::Variant(variant) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NonExhaustiveMatch {
                            expr,
                            uncovered: vec![format!(
                                "{}::{}",
                                variant.decl.display_name(),
                                variant.name
                            )],
                        });
                }
                Scrutinee::Other(ty) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::MatchWithoutCatchAll {
                            expr,
                            scrutinee: ty.clone(),
                        });
                }
                Scrutinee::Unknown(_) | Scrutinee::Error => {}
            }
        }

        let ty = if !is_root {
            // A nested match types as the enclosing join's result, exactly
            // like a nested `if` (no intermediate verdict of its own).
            if all_diverge {
                Ty::Never
            } else {
                self.join_sinks[sink_index].result.clone()
            }
        } else {
            let JoinSink { result, witnesses } = self.join_sinks.pop().expect("sink pushed above");
            match witnesses.len() {
                // Every arm diverges (or there are none): so does the match.
                0 => {
                    self.unify(&result, &Ty::Never);
                    Ty::Never
                }
                1 => {
                    let ty = witnesses.into_iter().next().unwrap().ty;
                    self.unify(&result, &ty);
                    ty
                }
                _ => {
                    self.constraints.push_join(Join {
                        expr,
                        depth: self.scope_depth,
                        result: result.clone(),
                        witnesses,
                    });
                    result
                }
            }
        };
        let ty = self.check(expr, ty, expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    fn push_unreachable(&mut self, match_expr: ExprId, pat: PatId, reason: UnreachableReason) {
        self.result
            .diagnostics
            .push(InferenceDiagnostic::UnreachableArm {
                match_expr,
                pat,
                reason,
            });
    }

    /// Check one arm's pattern against the scrutinee: resolve variant
    /// names (type-directed), type the pattern's bindings, and report what
    /// the pattern covers.
    fn check_match_pat(&mut self, match_expr: ExprId, pat: PatId, scrut: &Scrutinee) -> Cover {
        match self.body.pats[pat].clone() {
            PatData::Missing => Cover::Nothing,
            PatData::Wildcard => Cover::All,
            PatData::Bind(binding) => {
                let name = self.body.bindings[binding].name.clone();
                // A bare bind always binds the whole scrutinee — patterns
                // are never reinterpreted as variants (G25: silent
                // reinterpretation was a footgun — rename/remove a variant
                // later and an old bare-name arm would silently degrade to
                // a catch-all). Warn when the name shadows a variant of the
                // scrutinee's enum: almost always a migration mistake or a
                // stale rename; write `::Name` (or `Enum::Name`) to
                // actually match it. This does *not* change the binding's
                // type or the `Cover::All` outcome — it only warns.
                let enum_loc = match scrut {
                    Scrutinee::Enum(loc) => Some(loc.clone()),
                    Scrutinee::Variant(variant) => Some(variant.decl.clone()),
                    _ => None,
                };
                if let Some(loc) = &enum_loc
                    && !name.is_empty()
                    && let Some(variants) = enum_variants(self.db, loc.to_id(self.db)).as_ref()
                    && variants.iter().any(|(n, _)| *n == name)
                {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::BindShadowsVariant {
                            match_expr,
                            pat,
                            item: loc.clone(),
                            name: name.clone(),
                        });
                }
                let ty = match scrut {
                    Scrutinee::Enum(loc) => Ty::Named(loc.clone()),
                    Scrutinee::Variant(variant) => Ty::Variant(variant.clone()),
                    Scrutinee::Other(ty) | Scrutinee::Unknown(ty) => ty.clone(),
                    Scrutinee::Error => Ty::Error,
                };
                self.result.type_of_binding.insert(binding, ty);
                Cover::All
            }
            PatData::Variant {
                enum_name,
                variant,
                bindings,
                rest,
            } => {
                // Which enum the variant name resolves against: the named
                // one (qualified spelling), or the scrutinee's (bare).
                let target = match &enum_name {
                    Some(enum_name) => match self.resolve_pat_enum(match_expr, pat, enum_name) {
                        Some(loc) => loc,
                        None => {
                            self.bind_error(&bindings);
                            return Cover::Nothing;
                        }
                    },
                    None => match scrut {
                        Scrutinee::Enum(loc) => loc.clone(),
                        Scrutinee::Variant(variant) => variant.decl.clone(),
                        Scrutinee::Other(ty) => {
                            self.result.diagnostics.push(
                                InferenceDiagnostic::NonEnumScrutineeVariantPat {
                                    match_expr,
                                    pat,
                                    scrutinee: ty.clone(),
                                },
                            );
                            self.bind_error(&bindings);
                            return Cover::Nothing;
                        }
                        Scrutinee::Unknown(_) => {
                            self.result.diagnostics.push(
                                InferenceDiagnostic::VariantPatUnknownScrutinee { match_expr, pat },
                            );
                            self.bind_error(&bindings);
                            return Cover::Nothing;
                        }
                        Scrutinee::Error => {
                            self.bind_error(&bindings);
                            return Cover::Nothing;
                        }
                    },
                };
                // `Shape:: =>` — the parse error covers the missing name.
                if variant.is_empty() {
                    self.bind_error(&bindings);
                    return Cover::Nothing;
                }
                let Some(variants) = enum_variants(self.db, target.to_id(self.db)).as_ref() else {
                    // `resolve_pat_enum` only returns enums; a bare
                    // pattern's scrutinee enum was classified above.
                    self.bind_error(&bindings);
                    return Cover::Nothing;
                };
                let Some(index) = variants.iter().position(|(n, _)| *n == variant) else {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::PatNoSuchVariant {
                            match_expr,
                            pat,
                            item: target.clone(),
                            name: variant,
                        });
                    self.bind_error(&bindings);
                    return Cover::Nothing;
                };
                let variant_ty = VariantTy {
                    decl: target.clone(),
                    index: index as u32,
                    name: std::sync::Arc::from(variant.as_str()),
                };
                self.result.variant_of_pat.insert(pat, variant_ty.clone());
                // Payload binding types come from the declaration either
                // way — even a wrong-enum pattern's arm body shouldn't
                // cascade.
                let payload = variants[index].1.clone();
                if !rest && bindings.len() != payload.len() {
                    self.result.diagnostics.push(InferenceDiagnostic::PatArity {
                        match_expr,
                        pat,
                        variant,
                        payloads: payload.len(),
                        found: bindings.len(),
                    });
                }
                for (i, &binding) in bindings.iter().enumerate() {
                    let ty = payload.get(i).cloned().unwrap_or(Ty::Error);
                    self.result.type_of_binding.insert(binding, ty);
                }
                match scrut {
                    Scrutinee::Enum(scrut_loc) => {
                        if *scrut_loc != target {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::PatWrongEnum {
                                    match_expr,
                                    pat,
                                    item: target,
                                    variant: variant_ty.name.to_string(),
                                    scrutinee: Ty::Named(scrut_loc.clone()),
                                });
                            Cover::Nothing
                        } else {
                            Cover::Variant(variant_ty.index)
                        }
                    }
                    Scrutinee::Variant(scrut_variant) => {
                        if scrut_variant.decl != target {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::PatWrongEnum {
                                    match_expr,
                                    pat,
                                    item: target,
                                    variant: variant_ty.name.to_string(),
                                    scrutinee: Ty::Variant(scrut_variant.clone()),
                                });
                            Cover::Nothing
                        } else if scrut_variant.index == variant_ty.index {
                            // The scrutinee can only be this one variant:
                            // the pattern is irrefutable, no dispatch.
                            Cover::All
                        } else {
                            self.push_unreachable(
                                match_expr,
                                pat,
                                UnreachableReason::OtherVariant {
                                    scrutinee: Ty::Variant(scrut_variant.clone()),
                                },
                            );
                            Cover::Nothing
                        }
                    }
                    // A qualified pattern on a non-enum scrutinee: the
                    // scrutinee is the problem, same as the bare spelling.
                    Scrutinee::Other(ty) => {
                        self.result.diagnostics.push(
                            InferenceDiagnostic::NonEnumScrutineeVariantPat {
                                match_expr,
                                pat,
                                scrutinee: ty.clone(),
                            },
                        );
                        Cover::Nothing
                    }
                    // Unknown only when the pre-scan couldn't pin the
                    // scrutinee (this pattern resolved, so it did) or the
                    // scrutinee is broken: nothing to cover either way.
                    Scrutinee::Unknown(_) | Scrutinee::Error => Cover::Nothing,
                }
            }
            // `let`/parameter-only shapes; `match_pattern`'s grammar never
            // produces them. Defensive fallback.
            PatData::Record { .. } | PatData::Newtype { .. } => Cover::Nothing,
        }
    }

    /// Resolve a qualified variant pattern's first segment to an enum
    /// `type` item, or report why it isn't one. The message mirrors the
    /// type-position wording (rendered here — it needs the database).
    fn resolve_pat_enum(
        &mut self,
        match_expr: ExprId,
        pat: PatId,
        enum_name: &str,
    ) -> Option<ItemLoc> {
        let message = match type_scope(self.db, self.file).resolve(enum_name) {
            Some(Resolution::TypeItem(loc)) => {
                if enum_variants(self.db, loc.to_id(self.db)).is_some() {
                    return Some(loc);
                }
                if matches!(
                    crate::type_decl(self.db, loc.to_id(self.db)),
                    Some(TypeDeclData::Struct { .. })
                ) {
                    format!("`{enum_name}` has no variants (it is a `struct` type)")
                } else {
                    // A broken declaration carries its own diagnostics.
                    return None;
                }
            }
            // The duplicate definitions carry the diagnostics.
            Some(Resolution::Ambiguous(_)) => return None,
            _ => {
                if builtin_type_by_name(enum_name).is_some() {
                    format!("`{enum_name}` has no variants (it is a builtin type)")
                } else {
                    format!("`{enum_name}` does not name an `enum` type")
                }
            }
        };
        self.result
            .diagnostics
            .push(InferenceDiagnostic::PatPathError {
                match_expr,
                pat,
                message,
            });
        None
    }

    fn bind_error(&mut self, bindings: &[BindingId]) {
        for &binding in bindings {
            self.result.type_of_binding.insert(binding, Ty::Error);
        }
    }

    /// The type a `let`/parameter pattern determines *on its own*, without
    /// an explicit annotation: only [`PatData::Newtype`] manages this
    /// (`Foo(...)` names its own type outright, mirroring a construction
    /// call's callee — see [`Self::infer_construction`]). A bare
    /// [`PatData::Record`] pattern has no way to know which fields it may
    /// destructure without either an annotation or a newtype wrapper, so it
    /// returns `None`; the caller falls back to a fresh variable, and
    /// [`Self::check_pat`] reports [`InferenceDiagnostic::PatBindingNeedsAnnotation`]
    /// if nothing else pins the initializer's type down before the pattern
    /// is checked. An unresolvable name also returns `None` here — checked
    /// again (and diagnosed) inside `check_pat`, once there is a `ty` to
    /// blame the mismatch on too.
    fn declared_type_for_pat(&mut self, pat: PatId) -> Option<Ty> {
        match &self.body.pats[pat] {
            PatData::Newtype { type_name, .. } => {
                match type_scope(self.db, self.file).resolve(type_name) {
                    Some(Resolution::TypeItem(loc)) => Some(Ty::Named(loc)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Recursively type every binding a pattern introduces as `{error}` —
    /// the destructuring counterpart of [`Self::bind_error`], for a pattern
    /// whose scrutinee type is broken or unknown.
    fn bind_pat_error(&mut self, pat: PatId) {
        self.result.type_of_pat.insert(pat, Ty::Error);
        match self.body.pats[pat].clone() {
            PatData::Missing | PatData::Wildcard => {}
            PatData::Bind(binding) => {
                self.result.type_of_binding.insert(binding, Ty::Error);
            }
            PatData::Variant { bindings, .. } => self.bind_error(&bindings),
            PatData::Record { fields, .. } => {
                for f in &fields {
                    self.result.type_of_binding.insert(f.binding, Ty::Error);
                }
            }
            PatData::Newtype { inner, .. } => self.bind_pat_error(inner),
        }
    }

    /// Check a `let`/parameter pattern against the type it destructures,
    /// binding every name it introduces. Construction's mirror image: a
    /// `Record` pattern is checked bidirectionally against `ty` the same
    /// way [`ExprData::RecordLit`] is checked against an expected type
    /// (missing/extra fields), and a `Newtype` pattern is checked the same
    /// way [`Self::infer_construction`] checks a construction call.
    /// Patterns here are always irrefutable — a `let`/parameter destructure
    /// either matches or the program doesn't type-check — so unlike
    /// [`Self::check_match_pat`] there is no `Cover` to compute and no
    /// dispatch to decide.
    ///
    /// `anchor` is the expression [`InferenceDiagnostic::expr`] reports a
    /// finding on: the `let`'s initializer (whose value fails to destructure
    /// — the direct reconciliation, same as a bad record literal), or the
    /// enclosing `fn` literal's body for a parameter pattern (there is no
    /// per-call expression to blame; every call runs the body). Either way
    /// [`InferenceDiagnostic::pat`] carries the precise pattern, so the
    /// rendered squiggle always lands on the pattern itself.
    fn check_pat(&mut self, pat: PatId, ty: &Ty, anchor: ExprId) {
        self.result.type_of_pat.insert(pat, ty.clone());
        match self.body.pats[pat].clone() {
            PatData::Missing | PatData::Wildcard => {}
            PatData::Bind(binding) => {
                self.result.type_of_binding.insert(binding, ty.clone());
            }
            // Never produced by `binding_pattern`'s grammar; defensive.
            PatData::Variant { bindings, .. } => self.bind_error(&bindings),
            PatData::Record { fields, rest } => match self.resolve_shallow(ty) {
                Ty::Record(rec) => {
                    let mut seen: Vec<&str> = Vec::new();
                    for f in &fields {
                        match rec.field_ty(&f.field) {
                            Some(field_ty) => {
                                self.result
                                    .type_of_binding
                                    .insert(f.binding, field_ty.clone());
                            }
                            None => {
                                self.result.diagnostics.push(
                                    InferenceDiagnostic::PatUnknownField {
                                        pat,
                                        expr: anchor,
                                        name: f.field.clone(),
                                        record_ty: Ty::Record(rec.clone()),
                                    },
                                );
                                self.result.type_of_binding.insert(f.binding, Ty::Error);
                            }
                        }
                        seen.push(f.field.as_str());
                    }
                    if !rest {
                        let missing: Vec<(String, Ty)> = rec
                            .fields
                            .iter()
                            .filter(|(name, _)| !seen.contains(&name.as_str()))
                            .cloned()
                            .collect();
                        if !missing.is_empty() {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::PatMissingFields {
                                    pat,
                                    expr: anchor,
                                    fields: missing,
                                });
                        }
                    }
                }
                Ty::Infer(_) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::PatBindingNeedsAnnotation { pat, expr: anchor });
                    self.bind_error(&fields.iter().map(|f| f.binding).collect::<Vec<_>>());
                }
                Ty::Error => {
                    self.bind_error(&fields.iter().map(|f| f.binding).collect::<Vec<_>>());
                }
                other => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::PatNotRecord {
                            pat,
                            expr: anchor,
                            ty: other,
                        });
                    self.bind_error(&fields.iter().map(|f| f.binding).collect::<Vec<_>>());
                }
            },
            PatData::Newtype { type_name, inner } => {
                let target = match type_scope(self.db, self.file).resolve(&type_name) {
                    Some(Resolution::TypeItem(loc)) => Some(loc),
                    _ => None,
                };
                let Some(target) = target else {
                    // An empty name is broken source (the parse error
                    // covers it); anything else names no type at all.
                    if !type_name.is_empty() {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::PatUnknownType {
                                pat,
                                expr: anchor,
                                name: type_name.clone(),
                            });
                    }
                    self.bind_pat_error(inner);
                    return;
                };
                match self.resolve_shallow(ty) {
                    Ty::Named(loc) if loc == target => {
                        let underlying =
                            type_underlying(self.db, target.to_id(self.db)).unwrap_or(Ty::Error);
                        self.check_pat(inner, &underlying, anchor);
                    }
                    Ty::Infer(_) => {
                        self.result.diagnostics.push(
                            InferenceDiagnostic::PatBindingNeedsAnnotation { pat, expr: anchor },
                        );
                        self.bind_pat_error(inner);
                    }
                    Ty::Error => self.bind_pat_error(inner),
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::PatNamedTypeMismatch {
                                pat,
                                expr: anchor,
                                expected: Ty::Named(target),
                                actual: other,
                            });
                        self.bind_pat_error(inner);
                    }
                }
            }
        }
    }

    /// A construction call `Foo(arg)`: type-check the single argument
    /// against the declared underlying record bidirectionally (the declared
    /// field types flow into a literal argument's fields, blame cites the
    /// declaration via [`Cause::Constructor`]) and produce [`Ty::Named`].
    fn infer_construction(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        loc: &ItemLoc,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        // An enum type constructs through its variants, never directly:
        // there is no one shape `Shape(...)` could take. Recover with the
        // enum type (that's what the user meant to produce) so downstream
        // code still checks.
        if enum_variants(self.db, loc.to_id(self.db)).is_some() {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::EnumCtorIsVariant {
                    expr,
                    item: loc.clone(),
                });
            for &arg in args {
                let fresh = self.fresh_var();
                self.infer_expr(arg, &fresh);
            }
            let ty = self.check(expr, Ty::Named(loc.clone()), expected, cause);
            self.result.type_of_expr.insert(expr, ty.clone());
            return ty;
        }
        // `None` when the declaration is broken (RHS not a `struct`
        // literal): the declaration site carries the diagnostic, so the
        // argument is checked against `{error}` — infectious and silent.
        let underlying = type_underlying(self.db, loc.to_id(self.db)).unwrap_or(Ty::Error);
        // The constructor *is* a function value conceptually; give the
        // callee name that type so hover on `Foo` in `Foo(...)` is honest.
        self.result.type_of_expr.insert(
            callee,
            Ty::fn_type(vec![underlying.clone()], Ty::Named(loc.clone())),
        );
        if args.len() != 1 {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::TypeCtorArgCount {
                    expr,
                    item: loc.clone(),
                    found: args.len(),
                });
        }
        for (i, &arg) in args.iter().enumerate() {
            if i == 0 {
                self.infer_expr_with(arg, &underlying, Some(Cause::Constructor(expr)));
            } else {
                // Surplus arguments (already diagnosed): infer freely so
                // their contents still get types and diagnostics.
                let fresh = self.fresh_var();
                self.infer_expr(arg, &fresh);
            }
        }
        let ty = self.check(expr, Ty::Named(loc.clone()), expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// Register the value of a branch as a witness of the join being
    /// assembled in `sink` — the witness-contribution seam every joining
    /// construct plugs into: `if`/`else` branches, match arms, and
    /// loop-break values.
    ///
    /// Two kinds of branch contribute nothing: a diverging branch (it
    /// doesn't vote, it widens — the join is decided by the surviving
    /// leaves alone), and a branch whose tail is itself an `if`/`else`, a
    /// `match` or a `loop` (it was inferred with this sink as its witness
    /// position, so its leaves — a loop's break values — are already in;
    /// that's the flattening).
    fn contribute_witness(&mut self, sink: usize, branch: ExprId, ty: &Ty) {
        if matches!(self.resolve_shallow(ty), Ty::Never) {
            return;
        }
        let blame = peel_blocks(self.body, branch);
        if matches!(
            self.body.exprs[blame],
            ExprData::If {
                else_branch: Some(_),
                ..
            } | ExprData::Match { .. }
                | ExprData::Loop { .. }
        ) {
            return;
        }
        self.join_sinks[sink].witnesses.push(Witness {
            blame,
            ty: ty.clone(),
        });
    }

    /// Check `actual` against `expected`; on mismatch, report on `expr` and
    /// recover with the expected type (trust the annotation).
    ///
    /// A successful unification that binds a type variable records `cause`
    /// in the constraint store — that's how the join solver later knows why
    /// an `if`'s result type was decided.
    ///
    /// Between unification and the mismatch sits the widening lattice
    /// ([`widens_to`]): `!` adopts anything (no value to convert), and a
    /// variant type converts to *its* enum — accepted silently, with the
    /// conversion recorded in [`InferenceResult::widened`] so MIR injects
    /// the tag at exactly this expression.
    fn check(&mut self, expr: ExprId, actual: Ty, expected: &Ty, cause: Option<Cause>) -> Ty {
        // `!` coerces to anything — but only on the actual side. Its own
        // early path (rather than `widens_to` below) because `!` also
        // adopts a still-free expectation, which a conversion can't.
        if matches!(self.resolve_shallow(&actual), Ty::Never) {
            match self.resolve_shallow(expected) {
                Ty::Never => {}
                Ty::Infer(_) => return actual,
                _ => return expected.clone(),
            }
        }
        if self.constraints.unify(self.table, &actual, expected, cause) {
            return actual;
        }
        let resolved_actual = self.resolve_shallow(&actual);
        let resolved_expected = self.resolve_shallow(expected);
        if widens_to(&resolved_actual, &resolved_expected) {
            if let Ty::Variant(variant) = resolved_actual {
                self.result.widened.insert(expr, variant);
            }
            // The context's type is what flows on from here — the value is
            // tagged at this edge, so hover past it shows the enum.
            return resolved_expected;
        }
        self.result
            .diagnostics
            .push(InferenceDiagnostic::TypeMismatch {
                expr,
                expected: expected.clone(),
                actual,
                reasons: cause.into_iter().collect(),
            });
        expected.clone()
    }

    pub(crate) fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        self.constraints.unify(self.table, a, b, None)
    }

    fn resolve_shallow(&mut self, ty: &Ty) -> Ty {
        constraint::resolve_shallow(self.table, ty)
    }
}

/// What a `match` scrutinee's resolved type says about the value universe
/// the arms must cover.
enum Scrutinee {
    /// Enum-typed (tagged at runtime): the arms dispatch over the
    /// declaration's variants.
    Enum(ItemLoc),
    /// Variant-typed (tag-free at runtime): only this one variant can
    /// ever show up — no dispatch.
    Variant(VariantTy),
    /// Any other concrete type: only `_`/binding arms can match it (v1).
    Other(Ty),
    /// Still an inference variable.
    Unknown(Ty),
    /// Broken upstream (or diverging): stay silent.
    Error,
}

/// What one arm's pattern covers of the scrutinee's value universe.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cover {
    /// Matches anything the scrutinee can be.
    All,
    /// Exactly one variant, by declaration index.
    Variant(u32),
    /// Nothing (broken or rejected pattern, or an unreachable variant).
    Nothing,
}

/// Dig through block and `const` block wrappers to the value-producing
/// sub-expression that should carry a squiggle — both are transparent, so
/// blame belongs on whatever is actually inside.
fn peel_blocks(body: &Body, mut expr: ExprId) -> ExprId {
    loop {
        expr = match &body.exprs[expr] {
            ExprData::Block {
                tail: Some(tail), ..
            } => *tail,
            ExprData::ConstBlock { body: inner } => *inner,
            _ => return expr,
        };
    }
}

fn builtin_type(builtin: Builtin) -> Ty {
    match builtin {
        Builtin::Print => Ty::fn_type(vec![Ty::Str], Ty::Unit),
        Builtin::Panic => Ty::fn_type(vec![Ty::Str], Ty::Never),
    }
}
