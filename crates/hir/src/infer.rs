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
    BindingId, Body, ExprData, ExprId, GenericArgData, LiteralData, MatchArm, PatData, PatId, Stmt,
    body,
};
use crate::constraint::{
    self, Cause, Constraints, Join, RegionConstraint, RegionConstraintReason, Witness,
    is_unresolved_number, poison_unresolved_number, resolve_args_fully, resolve_finished,
    resolve_fully,
};
use crate::item_tree::RegionRef;
use crate::item_tree::{
    Constness, GenericArgRef, GenericParamData, GenericParamKind, TypeDeclData,
};
use crate::scopes::{Builtin, Resolution, resolutions, type_scope};
use crate::ty::{
    ConstArgValue, GenericArg, IntKind, IntValue, NamedTy, ParamScope, ReceiverShape, Region,
    SelfPosition, Ty, TyVar, TyVarValue, VariantTy, builtin_type_by_name, enum_variants,
    generic_param_scope, lower_type_ref_in, member_self_position, member_self_ty, receiver_takes,
    self_position_of, signature, signature_needs_annotation, substitute_args, type_underlying_for,
};
use crate::{ItemId, ItemLoc, Severity, TypeRef, item_loc};

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
    /// The type each expression was *checked against*: the `expected`
    /// parameter in hand at every position [`InferCtx::infer_expr_with`]
    /// visits — the same fact the blame system's causes are built on, just
    /// persisted. Recorded for every expression during traversal, then
    /// filtered in the finish pass: an entry survives only if it resolves
    /// to a concrete type — one still containing an unbound inference
    /// variable carried no real expectation (checking against a fresh
    /// variable is how "infer freely" is spelled), and one containing
    /// `{error}` describes broken code. Dropping the unresolved ones also
    /// keeps the result value-deterministic (no canonicalized variable
    /// indices), which the incrementality firewall's backdating relies on.
    /// Note the flip side: a fresh-variable expectation that unification
    /// later *pins* (typically to the expression's own type — a bare
    /// `let`'s initializer, an equality operand) resolves concrete and is
    /// kept. Consumed by ide completions for type-directed ranking.
    pub expectation_of_expr: ArenaMap<ExprId, Ty>,
    /// The const arguments of every turbofish mention whose arity matched:
    /// `(binder index, value expression)` pairs, in source order. Inference
    /// only *type-checks* the values against their declared const-param
    /// types; evaluation and instance identity consume this later.
    pub const_args_of_expr: ArenaMap<ExprId, Vec<(u32, ExprId)>>,
    /// Every dot-call (`recv.name(a, b)`) that resolved STRUCTURALLY to an
    /// inherent member (TR01: `name` is a member fn of recv's type whose
    /// LAST parameter is Self-typed) — or to a trait-impl member
    /// on a concrete receiver (impl-directed resolution) — keyed by the
    /// CALL expression and mapping to the member's location. MIR lowers
    /// these as ordinary direct calls of the member with the receiver
    /// appended as the LAST argument (so the written arguments evaluate
    /// BEFORE the receiver binds).
    pub member_of_expr: ArenaMap<ExprId, ItemLoc>,
    /// Qualified short-form calls (`Display::fmt(w, x)`) whose inferred
    /// `Self` resolved to a concrete implementer: the impl's member, keyed
    /// by the CALL expression. Unlike [`Self::member_of_expr`] the
    /// arguments are passed exactly as written (Self is an ordinary
    /// parameter here — no receiver is appended).
    pub qualified_member_of_expr: ArenaMap<ExprId, ItemLoc>,
    /// Qualified member PATHS that name one member's fn value, keyed by the
    /// PATH expression: `Point::len` (an inherent member — an ordinary fn,
    /// no dictionary) and `Display::<Self = Foo>::fmt` (a named-Self impl
    /// member, TR01). MIR lowers these to the member item's value; a direct
    /// call of one is then an ordinary call.
    pub member_value_of_expr: ArenaMap<ExprId, QualifiedMemberValue>,
    /// Dot-calls that resolved to a BUILTIN member (`line.next_char(i)`),
    /// keyed by the CALL expression. MIR lowers these exactly as it lowers
    /// a user member's dot-call: a direct call of the builtin's value with
    /// the receiver appended LAST, because dot-callability is structural
    /// and a builtin member is spelled to the same shape (TR01) — no
    /// second convention for the backend or the interpreter to learn.
    pub builtin_member_of_expr: ArenaMap<ExprId, Builtin>,
    /// Bound-directed member calls — `x.fmt(w)` on a rigid `T: Display`
    /// receiver, or a qualified call whose `Self` resolved to a bounded
    /// rigid param — keyed by the CALL expression. MIR lowers these as
    /// indirect calls through the enclosing body's hidden dictionary
    /// parameter (the erased dictionary-passing lowering).
    pub bound_member_of_expr: ArenaMap<ExprId, BoundMemberCall>,
    /// The dictionary operands each bounded instantiation needs, keyed by
    /// the INSTANTIATION expression (the mention for path calls, the CALL
    /// for dot-form and qualified calls), in canonical slot order
    /// ([`crate::traits::bound_slots`]). MIR appends one operand per slot
    /// per trait requirement.
    pub bound_dicts_of_expr: ArenaMap<ExprId, Vec<DictEntry>>,
    /// The body's OUTLIVES constraints, in emission order — the whole
    /// input the outlives module needs from inference, RECORDED here
    /// rather than re-derived from region-erased MIR (the wasm honesty
    /// report's lesson, applied a third time).
    ///
    /// Nothing in hir consumes these. They are inert by construction: the
    /// specialization law means no region can select an impl or change a
    /// lowering, so the checker's entire output is diagnostics.
    pub region_constraints: Vec<RegionConstraint>,
    /// How many region variables this body minted — the outlives module's
    /// table size. Variables are numbered `0..region_count`.
    pub region_count: u32,
    /// Expressions at which the checker INSERTED a reborrow, with the
    /// flavor it produced (`true` = `.&mut`, `false` = a degradation to
    /// `.&`). MIR materializes each one; G14's bounded auto-ref exception
    /// is exactly this list — a compiler-inserted SAFE borrow of `x.*`
    /// where `x` is already a borrow, never of `x` itself and never raw.
    pub reborrows: ArenaMap<ExprId, bool>,
    pub diagnostics: Vec<InferenceDiagnostic>,
}

impl InferenceResult {
    /// The BUILTIN, if any, that the call `call` reaches — one lookup for
    /// both ways a builtin is named. A name resolution answers directly
    /// (`copy(p, q, n)`); a dot-call has none at all (a builtin member
    /// never appears in `resolutions`), so the dot-call table answers
    /// instead. The two cases never overlap, so nothing here "wins"
    /// between them — this is one name for "which builtin, if any".
    ///
    /// [`crate::unsafe_check`] reaches every builtin call through this one
    /// helper. [`crate::const_check`]'s named path calls
    /// [`Builtin::const_legality`] directly and uses this helper only for
    /// its dot-call arm — either way, every path ends at
    /// [`Builtin::const_legality`] / [`Builtin::requires_unsafe`], never a
    /// re-derived guess.
    pub fn builtin_of_call(&self, callee: Option<&Resolution>, call: ExprId) -> Option<Builtin> {
        match callee {
            Some(Resolution::Builtin(builtin)) => Some(*builtin),
            _ => self.builtin_member_of_expr.get(call).copied(),
        }
    }
}

/// One qualified member reference used as a VALUE — see
/// [`InferenceResult::member_value_of_expr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualifiedMemberValue {
    pub member: ItemLoc,
    /// The OWNER's generic arguments the reference instantiates the member
    /// at — a dot-call reads these off the receiver's type; a qualified
    /// reference writes them (`Buf::<3>::len`). Empty for trait-impl
    /// members (their owner is non-generic by the non-generic-trait rules).
    pub args: Vec<GenericArg>,
}

/// One bound-directed member call — see
/// [`InferenceResult::bound_member_of_expr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundMemberCall {
    /// The bounded rigid param's index in the enclosing item's binder.
    pub param_index: u32,
    pub trait_: ItemLoc,
    /// The requirement's index in the trait's declaration order.
    pub member_index: u32,
    /// Whether MIR appends the receiver as the last (written) argument —
    /// true for dot-form calls, false for the qualified short form (whose
    /// Self argument is written explicitly).
    pub receiver_appended: bool,
}

/// One dictionary slot's resolution at an instantiation edge — see
/// [`InferenceResult::bound_dicts_of_expr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictEntry {
    /// The bound resolved to a concrete implementer: the impl's member
    /// items, in trait-requirement order.
    Impl(Vec<ItemLoc>),
    /// The bound resolved to a rigid param of the enclosing binder that
    /// carries the same bound: forward the caller's own dictionary.
    Forward { param_index: u32, trait_: ItemLoc },
    /// Unresolvable (unsatisfied, undetermined, or a broken impl) — the
    /// call site carries a diagnostic and MIR traps the call.
    Error,
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
    /// An assignment whose target is (or is a chain of field accesses
    /// rooted at) a local declared without `mut` — mutability is transitive
    /// from the binding to every field, so the *root* is what's judged (no
    /// per-field `mut`). Squiggle on the root name; the binding's
    /// declaration carries the related hint (and the insert-`mut` quick
    /// fix).
    AssignToImmutable {
        /// The root name expression inside the target place.
        target: ExprId,
        binding: BindingId,
        /// The binding's name, carried here so [`Self::message`] can render
        /// without the body in hand (same reason `NeedsAnnotation` carries
        /// an [`ItemLoc`]).
        name: String,
        /// The whole place as written (`p` for a plain assignment, `p.x.y`
        /// for a field chain) — equal to `name` exactly when the target is
        /// the bare binding; [`Self::message`] words the two differently.
        place: String,
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
    /// `::None` in expression position with no enum in view. The elided
    /// sigil is REJECT-ONLY sugar: it reads the position's expected type
    /// and nothing else — it never runs inference backwards to discover
    /// one — so the qualified spelling stays canonical and is always the
    /// named escape.
    ElidedVariantNoEnum {
        /// The elided-variant expression.
        expr: ExprId,
        /// The variant name as written.
        variant: String,
        /// The expected type, when the position had one that simply is not
        /// an enum; `None` when nothing pinned the position at all.
        expected: Option<Ty>,
    },
    NoSuchVariant {
        /// The variant-path expression.
        expr: ExprId,
        /// The enum `type` item.
        item: ItemLoc,
        /// The name that resolved to nothing.
        name: String,
    },
    /// A `::` path on a `type` item that declares a struct shape
    /// (`Point::nope` where `Point = struct { ... }`): only enums have
    /// variants. When the second segment names a FIELD it is
    /// [`Self::QualifiedPathIsField`] instead — that one has an escape to
    /// offer.
    NoVariantsOnStruct {
        /// The variant-path expression.
        expr: ExprId,
        /// The struct `type` item.
        item: ItemLoc,
    },
    /// A `::` path whose second segment names a FIELD of the type
    /// (`Point::x`). The qualified path reaches the type's NAMESPACE —
    /// variants and members — and a field is not in it: a field belongs to
    /// a value, so it is reached through one.
    QualifiedPathIsField {
        /// The variant-path expression.
        expr: ExprId,
        /// The `type` item declaring the field.
        item: ItemLoc,
        /// The field's name.
        name: String,
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
        /// The enum `type` item, carried structurally so [`Self::message`],
        /// the "Add missing match arms" fix, and MIR's trap fallback can
        /// all render `{decl.display_name()}::{name}` without re-deriving
        /// the declaration from the scrutinee's type.
        decl: ItemLoc,
        /// Each uncovered variant's bare name, in declaration order.
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
    /// struct type, a record, ...). What CAN match such a scrutinee is
    /// type-dependent — a `char` takes literal patterns too — so the
    /// message asks the type rather than asserting a blanket rule.
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
    /// A literal pattern whose type is not the scrutinee's (`'x'` in a
    /// match on a `usize`). The pattern-side twin of
    /// [`InferenceDiagnostic::TypeMismatch`]: a literal pattern's type is
    /// definite, so the pattern — not the scrutinee — is at fault.
    PatLiteralTypeMismatch {
        /// The enclosing match expression (where MIR traps).
        match_expr: ExprId,
        /// The pattern (carries the squiggle).
        pat: PatId,
        /// The scrutinee's type.
        expected: Ty,
        /// The literal's own type.
        found: Ty,
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
    /// A `return` with no enclosing body to leave — an item initializer's
    /// own top level, which is a value expression, not a function. (A `fn`
    /// literal IS a body, so a `return` inside one is fine and leaves that
    /// literal; a `const` block is [`Self::ReturnInConstBlock`].)
    ReturnOutsideFn {
        /// The return expression.
        expr: ExprId,
    },
    /// A `return` whose nearest enclosing body is a `const { ... }` block.
    /// Conceptually it bails from the OUTER fn body, and that needs
    /// cross-body machinery no v1 pass has — so it is RESERVED rather than
    /// given the reachable-but-wrong reading (yielding the block's value,
    /// the way a `const` block bounds `break`). A `fn` literal nested
    /// inside the block is its own body, so a `return` in THAT is
    /// untouched.
    ReturnInConstBlock {
        /// The return expression.
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
    /// A turbofish with the wrong number of arguments for the generic
    /// item's binder. The declaration is the related location.
    GenericArgCount {
        /// The turbofish mention expression.
        expr: ExprId,
        /// The generic item.
        item: ItemLoc,
        /// How many generic parameters the binder declares.
        expected: usize,
        found: usize,
    },
    /// A turbofish on something that takes no generic arguments: a
    /// non-generic item, a local, a builtin, a (non-generic — all of them,
    /// today) type item, a const parameter.
    NotGeneric {
        /// The turbofish mention expression.
        expr: ExprId,
        /// The mentioned name, carried so [`Self::message`] renders without
        /// the body in hand.
        name: String,
        /// The owner's display name when the arguments plausibly belong to
        /// it rather than to the binder-less thing they were written on:
        /// `Owner::member::<usize>` where `member` has no binder of its own
        /// but `Owner` does, and the path does not already write
        /// `Owner::<...>`. `None` everywhere else — for a trait owner (that
        /// spelling collides with the separately reserved generic-trait
        /// form), a non-generic owner (the hint would trade one refusal for
        /// another), and every non-member mention.
        owner_list_hint: Option<String>,
    },
    /// `_` written in a *const* argument position. Const args are never
    /// inferred (TR06: running an instance backwards is
    /// inference-through-conversion, categorically refused).
    ConstArgHole {
        /// The turbofish mention expression.
        expr: ExprId,
    },
    /// A REGION argument in a type or const slot of a TYPE's own list
    /// (`Pair::<@a>`). Its own variant, and not
    /// [`InferenceDiagnostic::GenericArgKindMismatch`], because the
    /// ANNOTATION mirror (`apply_position_diagnostics`, at the crate root)
    /// reports exactly this mistake: `Pair::<@a>` in a `static`'s type and
    /// `Pair::<@a>(...)` in its value are one error, so they render one
    /// sentence ([`crate::diag::unexpected_region_arg`]) over one range —
    /// the written argument, not the mention around it.
    UnexpectedRegionArg {
        /// The turbofish mention expression.
        expr: ExprId,
        /// The argument's position in the written list. A type mention's
        /// list is whole-binder positional (a type declaration's regions
        /// are reserved), so the binder index IS the written position.
        index: u32,
        /// The parameter's declared name.
        param: String,
    },
    /// A REGION argument written in a mention's turbofish. Regions are
    /// elided at every call site, in both lists a mention can carry, so a
    /// written one is refused on its own and the rest of the list still
    /// spends ([`InferCtx::spellable_args`]).
    RegionArgAtMention {
        /// The expression the list hangs off: the item mention, or the
        /// member's callee/path — see [`TurbofishList`].
        expr: ExprId,
        /// The argument's position in the WRITTEN list, so the report lands
        /// on the region and not on the whole mention.
        index: u32,
        /// Which of the node's two lists it was written in.
        list: TurbofishList,
    },
    /// A turbofish argument of the wrong kind for its position: a value
    /// where the binder declares a type parameter, or a type where it
    /// declares a const parameter.
    GenericArgKindMismatch {
        /// The turbofish mention expression.
        expr: ExprId,
        /// The parameter's declared name.
        param: String,
        /// Whether the binder declares a *const* parameter at this
        /// position (so a type was written where a value belongs).
        param_is_const: bool,
    },
    /// A bare mention of a generic item that declares const parameters:
    /// const args are never inferred (TR06), so the mention must spell them
    /// with a turbofish.
    MissingConstArgs {
        /// The referencing expression.
        expr: ExprId,
        /// The generic item.
        item: ItemLoc,
    },
    /// A mention of a generic item whose type parameter was never pinned —
    /// neither a turbofish nor any value/expectation determined it by the
    /// end of inference. The mention-site sibling of
    /// [`Self::NeedsAnnotation`].
    CannotInferGenericParam {
        /// The referencing expression.
        expr: ExprId,
        /// The generic item (its declaration is the related location).
        item: ItemLoc,
        /// The undetermined type parameter's name.
        param: String,
        /// What the use site CALLS the binder that declares it. Not
        /// `item.display_name()`: a member's own binder is blamed on the
        /// member (`Option::fresh`), and a requirement's on the
        /// requirement — not on the trait its `ItemLoc` names.
        owner: String,
        /// How THIS site can write the turbofish that pins it. The
        /// spelling is the site's to give, not the item's: a member value
        /// reached through a trait cannot write the bare path its
        /// `ItemLoc` renders.
        spelling: TurbofishSpelling,
    },
    /// A generic type parameter instantiated with a type that has no
    /// `forget` capability, where the parameter did not opt out of the
    /// default bound. The refusal is about the CONTRACT, not about safety:
    /// containment would make the resulting type linear anyway (see
    /// [`crate::capability`]), so nothing could leak — what the bound buys
    /// is that a generic's callers can read off its binder whether it may
    /// hold something that must be consumed.
    ForgetBoundUnsatisfied {
        /// The mention that instantiated the binder.
        expr: ExprId,
        /// The generic item whose parameter was instantiated.
        item: ItemLoc,
        /// The parameter's declared name.
        param: String,
        /// What it was instantiated with.
        ty: Ty,
        /// Why that type has no `forget` (see
        /// [`crate::capability::no_forget_reason`]).
        reason: String,
    },
    /// An assignment whose target resolves to a const parameter of the
    /// enclosing generic binder — a compile-time value, not a place.
    AssignToConstParam {
        /// The assignment's target expression.
        target: ExprId,
        /// The parameter's name, carried for [`Self::message`].
        name: String,
    },
    /// A const argument in a position whose declared type mentions an fn
    /// type: fn values are outside the const-arg domain (TR06: concrete
    /// data types only — their `BodyId` identity is edit-unstable). The
    /// mention-side belt of the declaration-site rejection in
    /// [`crate::file_diagnostics`]; both
    /// render [`crate::diag::FN_CONST_ARG`].
    FnConstArg {
        /// The turbofish mention expression.
        expr: ExprId,
    },
    /// A const argument that must flow into a TYPE's identity but isn't a
    /// literal or a const-param name. Type identity lives on the eval-free
    /// annotation path, so a `const { ... }` block (or any other computed
    /// value) can never parameterize a type — the clean way out is binding
    /// the value through a generic *function*'s const parameter.
    TypeConstArgUnsupported {
        /// The turbofish mention expression.
        expr: ExprId,
        /// Whether the offending argument is a `const { ... }` block (the
        /// message points at the generic-fn escape hatch) or some other
        /// non-literal value.
        is_block: bool,
    },
    /// An integer literal whose NUMBER-CLASS variable no defining use ever
    /// pinned (never defaulted — T01): annotate. The squiggle
    /// lands on the literal; the type renders `{number}` everywhere.
    CannotInferNumberType {
        /// The literal expression.
        expr: ExprId,
    },
    /// A literal that does not fit the integer type its defining use
    /// resolved it to (`300` in `u8`; `-1` in any unsigned type). Reported
    /// at resolution, range-checked with the sign applied (`-128` fits
    /// `i8`).
    IntLiteralOutOfRange {
        /// The literal expression (carries the squiggle and the trap).
        expr: ExprId,
        /// The literal as written, minus applied (`"-129"`), so the
        /// message renders without the body in hand.
        literal: String,
        /// The resolved integer type.
        ty: Ty,
    },
    /// `p.*` where `p` is not a raw pointer.
    DerefNonPointer {
        /// The deref expression.
        expr: ExprId,
        /// The receiver's (non-pointer) type.
        ty: Ty,
    },
    /// `a[i]` where `a` is not an array — the index sibling of
    /// [`Self::DerefNonPointer`].
    IndexNonArray {
        /// The index expression.
        expr: ExprId,
        /// The base's (non-array) type.
        ty: Ty,
    },
    /// `a[3]` on an `[T; 3]` — both the length and the index are
    /// compile-time known and the index is out of bounds. The message is
    /// EXACTLY the runtime bounds trap's ([`crate::diag::index_out_of_bounds`],
    /// the single-render discipline): the squiggle and the crash say the
    /// same thing.
    IndexOutOfBounds {
        /// The index expression (carries the squiggle and the trap).
        expr: ExprId,
        len: u128,
        index: u128,
    },
    /// `[]` with nothing pinning the element type — the array sibling of
    /// [`Self::NeedsAnnotation`]: compile-time only (an empty array runs
    /// fine whatever its element type would have been), so no trap.
    EmptyArrayNeedsAnnotation {
        /// The empty array literal.
        expr: ExprId,
    },
    /// A const argument in a position whose declared type mentions an
    /// array: array VALUES stay outside the const-arg domain for now (the
    /// ruled domain is builtins + records + variants). The mention-side
    /// belt of the declaration-site rejection in
    /// [`crate::file_diagnostics`]; both render
    /// [`crate::diag::ARRAY_CONST_ARG`] — the array twin of
    /// [`Self::FnConstArg`].
    ArrayConstArg {
        /// The turbofish mention expression.
        expr: ExprId,
    },
    /// A flavor-polymorphic builtin ([`Builtin::flavor_polymorphic`])
    /// applied to something that is not a raw pointer. These builtins
    /// accept `T.&raw` AND `T.&raw mut` in the same position, so the
    /// argument cannot be checked against one expected type — the mismatch
    /// gets its own diagnostic.
    BuiltinExpectsRawPtr {
        /// The call expression (where MIR refuses the operation).
        call: ExprId,
        /// The offending argument (carries the squiggle).
        arg: ExprId,
        builtin: Builtin,
        found: Ty,
    },
    /// A flavor-polymorphic builtin ([`Builtin::flavor_polymorphic`])
    /// mentioned without being called. Its pointer parameter may be
    /// `T.&raw` or `T.&raw mut`, so it has no ONE function type to be a
    /// value at.
    BuiltinNotFirstClass {
        /// The referencing expression.
        expr: ExprId,
        builtin: Builtin,
    },
    /// A field access or dot-call whose receiver is a BORROW. Reserved,
    /// not rejected: reaching through would be auto-deref, which stays
    /// reserved, and the explicit `r.*` escape already works.
    DotThroughBorrow {
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
    },
    /// `.&`/`.&mut` of something that is not a place — the safe-borrow
    /// twin of [`Self::AddrOfNonPlace`], with the same accepted places and
    /// the same reason (borrowing a temporary would need rvalue promotion,
    /// which the language does not have).
    BorrowNonPlace { expr: ExprId },
    /// `.&mut` of a place whose ROOT binding is not `mut` — the safe twin
    /// of [`Self::AddrOfMutImmutable`], same transitive-mutability rule.
    BorrowMutImmutable {
        /// The whole borrow expression (where MIR refuses the value).
        borrow: ExprId,
        /// The root name expression (carries the squiggle).
        root: ExprId,
        /// The root binding's name.
        name: String,
        /// The whole place as written.
        place: String,
    },
    /// `.&mut` of an item — a `static` has no `mut` form yet and a `const`
    /// is a copied value, so neither is an exclusively-borrowable place.
    BorrowMutItem {
        borrow: ExprId,
        root: ExprId,
        item: ItemLoc,
        constness: Constness,
    },
    /// `.&mut` reached through a SHARED borrow (`r.*.&mut` where
    /// `r: T.&::<@a>`) — exactly the raw rule one flavor up: a shared
    /// borrow must not launder into a write permission.
    BorrowMutThroughShared {
        borrow: ExprId,
        /// The governing borrow's (shared) type.
        ty: Ty,
    },
    /// `r.*.&`/`r.*.&mut` where the governing pointer (the receiver of
    /// the place's outermost deref) is a RAW pointer — refused for both
    /// flavors, because a raw pointer carries no region for the new
    /// borrow to be bounded by. Distinct from [`Self::BorrowMutThroughShared`],
    /// whose parent DOES have a region and is refused only for `.&mut`.
    BorrowThroughRawPointer {
        borrow: ExprId,
        /// The governing pointer's type.
        ty: Ty,
    },
    /// Reading `x.*` where the referent cannot be copied — safe `.*` yields
    /// a PLACE, and reading a place copies it, which an affine value
    /// forbids.
    MoveOutOfBorrow {
        expr: ExprId,
        /// The referent type that cannot be copied.
        ty: Ty,
    },
    /// `.&raw`/`.&raw mut` of something that is not a place — the accepted
    /// places are a variable, a chain of its fields and elements, a
    /// `static`/`const` item, or a chain rooted in a deref (`.&raw` of a
    /// temporary is refused outright, dodging rvalue promotion entirely).
    AddrOfNonPlace {
        /// The address-of expression.
        expr: ExprId,
    },
    /// `p.*.x.&raw mut` (a `.&raw mut` of a deref-rooted place) where a
    /// SHARED step governs the chain — a `T.&raw` or a `T.&`, at the
    /// outermost deref or deeper. Minting a mutating address through it
    /// would launder the shared flavor into a write permission. The
    /// write-side twin is [`Self::AssignThroughShared`]; `.&raw` (shared)
    /// through any pointer is fine.
    AddrOfMutThroughShared {
        /// The whole address-of expression (carries the squiggle, and
        /// where MIR refuses the value).
        addr_of: ExprId,
        /// The governing (shared) step's type.
        ty: Ty,
    },
    /// `.&raw mut` of a place whose ROOT binding is not `mut` — the same
    /// transitive-mutability rule assignments use: the root is what's
    /// judged, there is no per-field `mut`.
    AddrOfMutImmutable {
        /// The whole address-of expression (where MIR refuses the value).
        addr_of: ExprId,
        /// The root name expression inside the place (carries the
        /// squiggle).
        root: ExprId,
        binding: BindingId,
        /// The binding's name, for rendering without the body in hand.
        name: String,
        /// The whole place as written (`x`, or `x.f.g` for a field chain).
        place: String,
    },
    /// `.&raw mut` of a top-level item: a `static` has one place but
    /// `static mut` stays deferred, and a `const` is copied into each use —
    /// there is no place to hand out mutably either way. (`.&raw` — shared —
    /// of both is fine: a static's one place, a const use's own copy.)
    AddrOfMutItem {
        /// The whole address-of expression (where MIR refuses the value).
        addr_of: ExprId,
        /// The root name expression (carries the squiggle).
        root: ExprId,
        item: ItemLoc,
        constness: Constness,
    },
    /// `p.* = v;` (or `p.*.x = v;`, any deref-rooted chain) where a
    /// SHARED step governs the chain: writing needs `T.&raw mut` or
    /// `T.&mut` at every step it travels through. (`p` itself need not be
    /// a `mut` binding: writing through it does not reassign it. A deeper
    /// deref of a RAW mut pointer stops the walk — reading one out is a
    /// copy, and a copy carries the whole permission — but a deeper
    /// `T.&mut` does not: reading one out is a reborrow, which the place
    /// holding it must be allowed to grant.)
    AssignThroughShared {
        /// The governing deref expression (the target itself for
        /// `p.* = v;`, the chain's outermost deref otherwise).
        target: ExprId,
        /// The governing (shared) step's type.
        ty: Ty,
    },
    /// A dot-call `recv.name(...)` where `name` is neither a field nor a
    /// member of the receiver's type. Module-level statics are NEVER
    /// dot-callable (G13's deliberate opt-out) — the diagnostics
    /// aggregator adds the "call `name(...)` instead" hint when one
    /// exists.
    NoSuchMember {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
    },
    /// A dot-call of a member whose LAST parameter is not Self-typed —
    /// TR01 makes dot-callability structural, and this member doesn't have
    /// the shape.
    NotDotCallable {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        /// The member (its definition is the related location).
        member: ItemLoc,
    },
    /// A dot-call whose member wants a BORROW of `Self` but whose receiver
    /// is an owned place. NOT auto-ref: inserting `x.&mut` here would
    /// borrow the LOCAL `x` itself, which clause 2 of the ratified G14
    /// exception forbids outright. The escape is to write the borrow, and
    /// the postfix chain `m.&mut.f(...)` then arrives as a borrow receiver.
    MemberWantsBorrowReceiver {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        /// Whether the member's `Self` parameter is exclusive.
        mutable: bool,
        /// The member (its definition is the related location).
        member: ItemLoc,
    },
    /// A dot-call whose member wants `Self.&mut` but whose receiver is a
    /// SHARED borrow. Shared never sharpens to exclusive — the same rule
    /// `Constraints::try_reborrow` enforces at every other argument
    /// position, stated where the receiver can still name its own place.
    MemberWantsExclusiveReceiver {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
        /// The member (its definition is the related location).
        member: ItemLoc,
    },
    /// A member fn reached through a bare dot (`v.len` without a call):
    /// members are not field values.
    MemberNotCalled {
        /// The field-access expression.
        expr: ExprId,
        name: String,
    },
    /// A named generic argument that isn't the one nameable argument of
    /// TR01, or is named where nothing takes named arguments.
    NamedGenericArg {
        /// The mention (carries the squiggle).
        expr: ExprId,
        /// The written name.
        name: String,
        reason: NamedArgReason,
    },
    /// `recv.name(...)` where `name` is a plain (non-fn) FIELD and no
    /// member exists: under the syntax-directed namespace rule the field
    /// carries the call as a value, and this one is no fn.
    FieldNotCallable {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        /// The field's (non-callable) type.
        ty: Ty,
        /// The receiver's type, for the no-such-member half of the story.
        receiver_ty: Ty,
    },
    /// A bare trait name in expression position: traits are neither
    /// values nor types.
    TraitNotValue {
        /// The referencing expression (carries the squiggle).
        expr: ExprId,
        name: String,
    },
    /// A bound at an instantiation edge that the (solved) argument type
    /// does not satisfy: no impl of the trait for the type, and — for a
    /// rigid param — no matching bound on it.
    UnsatisfiedBound {
        /// The instantiation site (the mention or the call).
        expr: ExprId,
        /// The bounded param's declared name.
        param: String,
        /// The required trait.
        trait_: ItemLoc,
        /// The type the param was instantiated to.
        ty: Ty,
    },
    /// A qualified short-form call whose inferred `Self` has no impl of
    /// the trait.
    NoTraitImpl {
        /// The call expression (where MIR traps).
        expr: ExprId,
        trait_: ItemLoc,
        ty: Ty,
    },
    /// `Type::member` naming a member the type gets from a TRAIT. Each
    /// spelling names exactly one thing (G13), so a type's own qualified
    /// path reaches its INHERENT members only — the trait's own spellings
    /// reach the impl's.
    QualifiedTraitMemberOnType {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        type_name: String,
        member: String,
        /// The traits providing it for this type, in file order.
        traits: Vec<String>,
    },
    /// `Trait::<Self = T>::Item` — an associated-type path. Associated
    /// types are reserved; the trait declares the name, so this
    /// is not a "no such requirement".
    AssocTypeReserved {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        trait_: ItemLoc,
        name: String,
    },
    /// A qualified short-form call whose `Self` no argument determined.
    CannotInferSelf {
        /// The call expression (where MIR traps).
        expr: ExprId,
        /// The trait's display name.
        trait_name: String,
        member: String,
    },
    /// `Trait::member` used as a VALUE (not called directly): a member
    /// value is impl-specific, so the implementer must be named — the
    /// named-Self form (TR01), which nothing infers here.
    QualifiedTraitMemberValue {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        trait_name: String,
        member: String,
    },
    /// `Trait::<Self = T>::member` as a VALUE with a RIGID `Self`: the
    /// value would be a read of the enclosing body's dictionary — the same
    /// capture wall as [`InferenceDiagnostic::BoundFnValue`].
    BoundMemberValue {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        trait_name: String,
        member: String,
    },
    /// `Trait::name` where the trait declares no such requirement.
    TraitHasNoMember {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        trait_: ItemLoc,
        name: String,
    },
    /// A mention of a bound-carrying generic fn outside a direct call:
    /// its value would need a captured dictionary, which is reserved.
    BoundFnValue {
        /// The mention expression.
        expr: ExprId,
        /// The item's display name.
        name: String,
    },
    /// A use of a RESERVED generic trait (`requires::<...>`):
    /// the reservation must not go semantically live.
    GenericTraitReserved {
        /// The referencing expression (carries the squiggle).
        expr: ExprId,
        /// The trait's display name.
        name: String,
    },
    /// Call syntax where MORE THAN ONE thing on the receiver could carry
    /// the call — an inherent member, a trait member, an fn-typed field, in
    /// any combination (G13: no fall-through). Silent shadowing would
    /// be action at a distance: a trait impl may be added in the TRAIT's
    /// own chain, nowhere near the type, and reroute existing calls. The
    /// message names EVERY candidate with the spelling that selects it.
    MemberCallAmbiguity {
        /// The call expression (where MIR traps).
        expr: ExprId,
        name: String,
        receiver_ty: Ty,
        /// Every candidate, in resolution order (inherent, traits in file
        /// order, field).
        candidates: Vec<MemberCandidate>,
    },
    /// A bound-directed use (a bound member call, or a call forwarding
    /// the enclosing bounds) inside a fn literal NESTED in the bounded
    /// fn: the literal would need to capture the enclosing dictionary,
    /// which is reserved (the same capture wall as `BoundFnValue`).
    NestedBoundUse {
        /// The call expression (where MIR traps).
        expr: ExprId,
    },
    /// Generic arguments written at a use site on a member whose own
    /// binder declares a CONST parameter — the last kind still reserved,
    /// and the last thing this diagnostic says.
    ///
    /// The other two kinds are live and spelled differently, which is why
    /// they are not positions in the written list at all: a member's
    /// REGIONS are per-call existentials and always inferred (there has
    /// never been a spelling for one), and its TYPE parameters are exactly
    /// what the list spends. So a member with no const parameter never
    /// reaches this; a member with one refuses the WHOLE list, because a
    /// list matched positionally past a reserved hole is worse than no
    /// list. Granting member-own const generics deletes it; the grammar
    /// does not move (it never did — `MEMBER_GENERIC_ARGS` has parsed
    /// since the reservation was written).
    MemberOwnConstArgs {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        /// The owner-qualified spelling of the member (`Display::fmt`).
        path: String,
    },
    /// A member turbofish whose length does not match the member's own
    /// binder (`Option::map::<usize, bool>`). The item twin is
    /// [`Self::GenericArgCount`], and both render through
    /// [`crate::diag::generic_arg_count`] — but a member is not an item:
    /// it has no `ItemLoc` whose display name is the spelling that selects
    /// it (`item_source` answers `None` for every member id, so there is
    /// no declaration to point at either), so the report carries the
    /// owner-qualified path itself.
    MemberGenericArgCount {
        /// The path or dot-call callee the list is written on.
        expr: ExprId,
        /// The owner-qualified spelling of the member (`Option::map`).
        path: String,
        /// How many TYPE parameters the member's own binder declares — its
        /// regions are always inferred and are not positions in the list.
        expected: usize,
        found: usize,
    },
    /// `Shape::Circle::<usize>` — generic arguments written on a VARIANT.
    /// A CORRECTION, not a reservation: a variant is a case of its enum
    /// and never gets a binder of its own, so arguments written there are
    /// the OWNER's, misplaced — the fix is to move them
    /// (`Shape::<usize>::Circle`).
    VariantOwnGenericArgs {
        /// The path expression (carries the squiggle).
        expr: ExprId,
        /// The enum's display name.
        owner: String,
        /// The variant's name.
        variant: String,
        /// Whether `{owner}::<...>::{variant}` is worth hinting at: the
        /// enum has a binder of its own to receive the arguments, and the
        /// path does not already write one. False for a non-generic enum
        /// (the hint would just trade this diagnostic for "takes no
        /// generic arguments") or a path that already writes
        /// `{owner}::<...>::{variant}` (nothing left to suggest) — the
        /// same condition [`Self::NotGeneric`]'s `owner_list_hint` gates
        /// for a binder-less MEMBER.
        suggest_owner_list: bool,
    },
}

/// How a use site can WRITE the turbofish that pins a binder. An item's
/// own name always carries its list; a member's does not, so the spelling
/// is the SITE's to state — suggesting one the site cannot contain sends
/// the reader to a form the compiler then refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurbofishSpelling {
    /// The name carries the list: `f::<...>`, `Option::fresh::<...>`,
    /// `Mk::mk::<...>`.
    Path,
    /// A dot-call, where the member stands alone: `.fresh::<...>(...)` —
    /// the shape [`InferenceDiagnostic::MemberNotCalled`] already uses.
    Dot,
    /// A member VALUE reached through a trait. Such a value is
    /// impl-specific (TR01), so the bare `Trait::member` path is refused
    /// here and the implementer naming is part of the spelling. Carries
    /// the qualifier up to the member — `Mk::<Self = usize>`.
    NamedSelf(String),
}

impl TurbofishSpelling {
    /// The turbofish this site can write to pin `owner`'s binder, where
    /// `owner` is the blame name (`Option::fresh`, `Mk::mk`, or a plain
    /// item name).
    fn write(&self, owner: &str) -> String {
        match self {
            Self::Path => format!("{owner}::<...>"),
            Self::Dot => format!(".{}::<...>(...)", bare_member_name(owner)),
            Self::NamedSelf(qualifier) => {
                format!("{qualifier}::{}::<...>", bare_member_name(owner))
            }
        }
    }
}

/// Which of the two turbofish lists one path node can carry an argument
/// was written in. `Owner::<usize>::member::<bool>` has both on one node,
/// so a position alone does not locate an argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurbofishList {
    /// The name's own list: `f::<...>`, `Pair::<...>`, and the OWNER's
    /// list of a qualified path (`Trait::<...>::member`).
    Item,
    /// The member's own list, one level down: `o.get::<...>`,
    /// `Owner::member::<...>`.
    Member,
}

/// Why a named generic argument is refused — see
/// [`InferenceDiagnostic::NamedGenericArg`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamedArgReason {
    /// A name other than `Self`: general named arguments are gated on the
    /// binder-names-as-API ruling (TR01).
    NotSelf,
    /// `Self = ...` on something that is not a trait — only a trait has a
    /// `Self` argument.
    NotATrait,
    /// `Self = ...` in a MEMBER's own turbofish. The owner may well be a
    /// trait, so `NotATrait` would deny something the reader can see; the
    /// `Self` they mean belongs to the owner's list, one segment left.
    OwnersSelf,
    /// `Self` supplied twice in one argument list.
    Duplicate,
    /// `Self = _` — the argument is a HOLE. `Self` names the implementer,
    /// which is the one thing the named form exists to state, so a hole
    /// there is the absence of an answer, not an under-specified type.
    /// Refused structurally at lowering (both call and value position), so
    /// no inference variable can reach trait resolution — nor a message.
    Hole,
}

/// One thing a call-syntax name could resolve to, with the spelling that
/// selects it — see [`InferenceDiagnostic::MemberCallAmbiguity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberCandidate {
    /// An inherent (`impl Self`) member: `Type::name(value)`.
    Inherent {
        /// The receiver type as written in the escape.
        type_name: String,
        /// The member's definition, for the related location.
        def: ItemLoc,
    },
    /// A trait member, spelled short (`Trait::name(value)`) or with the
    /// named `Self` (`Trait::<Self = Type>::name(value)`).
    Trait {
        trait_name: String,
        /// The implementer, for the named-Self spelling.
        type_name: String,
        /// Whether the SHORT form suffices — it does whenever an argument
        /// determines `Self`, which every collision site today guarantees
        /// (the collisions are dot-calls, whose receiver IS the `Self`
        /// argument). The named-Self rendering is what a collision site
        /// that cannot determine `Self` will need.
        short: bool,
        /// The impl member's definition (the trait's declaration for a
        /// bound-directed candidate), for the related location.
        def: Option<ItemLoc>,
    },
    /// An fn-typed field: `(value.name)(...)`.
    Field,
}

impl MemberCandidate {
    /// The spelling that selects this candidate, plus what it selects.
    fn escape(&self, name: &str) -> String {
        match self {
            MemberCandidate::Inherent { type_name, .. } => {
                format!("the inherent member (`{type_name}::{name}(value)`)")
            }
            MemberCandidate::Trait {
                trait_name,
                type_name,
                short,
                ..
            } => {
                let spelling = if *short {
                    format!("{trait_name}::{name}(value)")
                } else {
                    format!("{trait_name}::<Self = {type_name}>::{name}(value)")
                };
                format!("`{trait_name}`'s member (`{spelling}`)")
            }
            MemberCandidate::Field => format!("the fn-typed field (`(value.{name})(...)`)"),
        }
    }

    /// The definition to point at and what to say about it, when there is
    /// one. A field's declaration is the receiver type's own body — the
    /// reader is already there, so it gets no hint.
    pub fn related(&self, name: &str) -> Option<(&ItemLoc, String)> {
        match self {
            MemberCandidate::Inherent { type_name, def } => {
                Some((def, format!("`{type_name}::{name}` is defined here")))
            }
            MemberCandidate::Trait {
                trait_name,
                type_name,
                def,
                ..
            } => Some((
                def.as_ref()?,
                format!("`{trait_name}::{name}` for `{type_name}` is defined here"),
            )),
            MemberCandidate::Field => None,
        }
    }
}

/// Why an arm can never run — one message per cause, so the fix is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreachableReason {
    /// The variant is already covered by an earlier arm.
    VariantCovered(String),
    /// The same literal is already matched by an earlier arm — rendered
    /// pre-quoted (`'('`), so the message needs no knowledge of which
    /// literal kind it was.
    LiteralCovered(String),
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
            | InferenceDiagnostic::ElidedVariantNoEnum { expr, .. }
            | InferenceDiagnostic::NoSuchVariant { expr, .. }
            | InferenceDiagnostic::NoVariantsOnStruct { expr, .. }
            | InferenceDiagnostic::QualifiedPathIsField { expr, .. }
            | InferenceDiagnostic::VariantPathOnValue { expr, .. }
            | InferenceDiagnostic::EnumCtorIsVariant { expr, .. }
            | InferenceDiagnostic::NonExhaustiveMatch { expr, .. }
            | InferenceDiagnostic::MatchWithoutCatchAll { expr, .. }
            | InferenceDiagnostic::BreakOutsideLoop { expr }
            | InferenceDiagnostic::ContinueOutsideLoop { expr }
            | InferenceDiagnostic::ReturnOutsideFn { expr }
            | InferenceDiagnostic::ReturnInConstBlock { expr }
            | InferenceDiagnostic::GenericArgCount { expr, .. }
            | InferenceDiagnostic::NotGeneric { expr, .. }
            | InferenceDiagnostic::ConstArgHole { expr }
            | InferenceDiagnostic::UnexpectedRegionArg { expr, .. }
            | InferenceDiagnostic::RegionArgAtMention { expr, .. }
            | InferenceDiagnostic::GenericArgKindMismatch { expr, .. }
            | InferenceDiagnostic::MissingConstArgs { expr, .. }
            | InferenceDiagnostic::CannotInferGenericParam { expr, .. }
            | InferenceDiagnostic::ForgetBoundUnsatisfied { expr, .. }
            | InferenceDiagnostic::FnConstArg { expr }
            | InferenceDiagnostic::TypeConstArgUnsupported { expr, .. }
            | InferenceDiagnostic::CannotInferNumberType { expr }
            | InferenceDiagnostic::IntLiteralOutOfRange { expr, .. }
            | InferenceDiagnostic::DerefNonPointer { expr, .. }
            | InferenceDiagnostic::IndexNonArray { expr, .. }
            | InferenceDiagnostic::IndexOutOfBounds { expr, .. }
            | InferenceDiagnostic::EmptyArrayNeedsAnnotation { expr }
            | InferenceDiagnostic::ArrayConstArg { expr }
            | InferenceDiagnostic::BuiltinNotFirstClass { expr, .. }
            | InferenceDiagnostic::NoSuchMember { expr, .. }
            | InferenceDiagnostic::NotDotCallable { expr, .. }
            | InferenceDiagnostic::MemberWantsBorrowReceiver { expr, .. }
            | InferenceDiagnostic::MemberWantsExclusiveReceiver { expr, .. }
            | InferenceDiagnostic::MemberNotCalled { expr, .. }
            | InferenceDiagnostic::NamedGenericArg { expr, .. }
            | InferenceDiagnostic::QualifiedTraitMemberOnType { expr, .. }
            | InferenceDiagnostic::FieldNotCallable { expr, .. }
            | InferenceDiagnostic::TraitNotValue { expr, .. }
            | InferenceDiagnostic::UnsatisfiedBound { expr, .. }
            | InferenceDiagnostic::NoTraitImpl { expr, .. }
            | InferenceDiagnostic::AssocTypeReserved { expr, .. }
            | InferenceDiagnostic::CannotInferSelf { expr, .. }
            | InferenceDiagnostic::QualifiedTraitMemberValue { expr, .. }
            | InferenceDiagnostic::BoundMemberValue { expr, .. }
            | InferenceDiagnostic::TraitHasNoMember { expr, .. }
            | InferenceDiagnostic::BoundFnValue { expr, .. }
            | InferenceDiagnostic::GenericTraitReserved { expr, .. }
            | InferenceDiagnostic::MemberCallAmbiguity { expr, .. }
            | InferenceDiagnostic::NestedBoundUse { expr }
            | InferenceDiagnostic::MemberOwnConstArgs { expr, .. }
            | InferenceDiagnostic::MemberGenericArgCount { expr, .. }
            | InferenceDiagnostic::VariantOwnGenericArgs { expr, .. }
            | InferenceDiagnostic::AddrOfNonPlace { expr }
            | InferenceDiagnostic::BorrowNonPlace { expr }
            | InferenceDiagnostic::DotThroughBorrow { expr, .. }
            | InferenceDiagnostic::MoveOutOfBorrow { expr, .. } => *expr,
            InferenceDiagnostic::BorrowMutImmutable { root, .. }
            | InferenceDiagnostic::BorrowMutItem { root, .. } => *root,
            InferenceDiagnostic::BorrowMutThroughShared { borrow, .. }
            | InferenceDiagnostic::BorrowThroughRawPointer { borrow, .. } => *borrow,
            InferenceDiagnostic::BuiltinExpectsRawPtr { arg, .. } => *arg,
            InferenceDiagnostic::AddrOfMutImmutable { root, .. }
            | InferenceDiagnostic::AddrOfMutItem { root, .. } => *root,
            InferenceDiagnostic::AddrOfMutThroughShared { addr_of, .. } => *addr_of,
            InferenceDiagnostic::AssignThroughShared { target, .. } => *target,
            InferenceDiagnostic::UnreachableArm { match_expr, .. }
            | InferenceDiagnostic::NonEnumScrutineeVariantPat { match_expr, .. }
            | InferenceDiagnostic::PatNoSuchVariant { match_expr, .. }
            | InferenceDiagnostic::PatWrongEnum { match_expr, .. }
            | InferenceDiagnostic::PatLiteralTypeMismatch { match_expr, .. }
            | InferenceDiagnostic::BindShadowsVariant { match_expr, .. }
            | InferenceDiagnostic::PatArity { match_expr, .. }
            | InferenceDiagnostic::PatPathError { match_expr, .. }
            | InferenceDiagnostic::VariantPatUnknownScrutinee { match_expr, .. } => *match_expr,
            InferenceDiagnostic::FieldOnUnknownType { receiver, .. } => *receiver,
            InferenceDiagnostic::IfBranchMismatch { else_expr, .. } => *else_expr,
            InferenceDiagnostic::AssignToImmutable { target, .. }
            | InferenceDiagnostic::AssignToItem { target, .. }
            | InferenceDiagnostic::AssignToBuiltin { target, .. }
            | InferenceDiagnostic::AssignToConstParam { target, .. } => *target,
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
            | InferenceDiagnostic::PatLiteralTypeMismatch { pat, .. }
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
                let (want_str, got_str) = (expected.display(), actual.display());
                let base = format!("type mismatch: expected `{want_str}`, found `{got_str}`");
                // TWO DIFFERENT PARAMETERS THAT SHARE A NAME. A member may
                // bind a `T` of its own beside the owner's (last one wins
                // inside the member — the rule a binder's const params
                // already follow), so the plain message can read "expected
                // `T`, found `T`", which is unactionable.
                //
                // The gate is the RENDERINGS coming out equal from unequal
                // types, which is exactly when the base message says
                // nothing — and the pair is looked for anywhere inside
                // either type, not just at the top: the shape the rule
                // invites most is `Self` against `P::<T>`, where the two
                // `T`s are nested one constructor down and the message
                // reads "expected `P::<T>`, found `P::<T>`". Two unresolved
                // variables also render alike; they find no pair and fall
                // through to the plain message.
                if want_str == got_str
                    && expected != actual
                    && let Some((want, got)) = shadowed_param_pair(expected, actual)
                {
                    return format!("{base} — {}", shadowed_param_note(want, got));
                }
                // A nominal/structural near-miss: the found record may even
                // be the declared shape, but a named type never coerces —
                // say how to actually make one.
                if let (Ty::Named(named), Ty::Record(_)) = (expected, actual) {
                    return format!(
                        "{base}; `{name}` is a distinct type — construct it with `{name}(...)`",
                        name = named.decl.display_name()
                    );
                }
                // The BORROW-where-owned near-miss: found is a borrow of
                // exactly the type wanted. The gate is that shape alone, so
                // this fires wherever a value is wanted — a typed `let`, an
                // argument, a return — and names both routes out: `.*` for
                // a copyable referent, and the owned value for anything
                // that has to move. Every binding a `match` through a
                // borrow makes is one of these, so it is also the message
                // borrowed-match arm bodies land on.
                if let Ty::Borrow { referent, .. } = actual
                    && referent.as_ref() == expected
                {
                    return format!(
                        "{base}; a borrow is not the value — write `.*` to read through it \
                         (a copy, so the referent must be copyable), or use the owned value \
                         instead of a borrow of it"
                    );
                }
                base
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
            InferenceDiagnostic::NoSuchMember {
                name, receiver_ty, ..
            } => format!("no field or member `{name}` on `{}`", receiver_ty.display()),
            InferenceDiagnostic::NotDotCallable { name, .. } => format!(
                "`{name}` is not dot-callable: its last parameter is neither `Self` nor a \
                 safe borrow of `Self` (dot-call resolution is structural)"
            ),
            InferenceDiagnostic::MemberWantsBorrowReceiver { name, mutable, .. } => {
                let borrow = if *mutable { ".&mut" } else { ".&" };
                format!(
                    "`{name}` takes `Self{borrow}`, and a borrow is never inserted for an \
                     owned receiver — write `{borrow}.{name}(...)`"
                )
            }
            InferenceDiagnostic::MemberWantsExclusiveReceiver {
                name, receiver_ty, ..
            } => format!(
                "`{name}` takes `Self.&mut`, but `{}` is a shared borrow — a shared borrow \
                 never becomes exclusive",
                receiver_ty.display()
            ),
            InferenceDiagnostic::MemberNotCalled { name, .. } => {
                format!("`{name}` is a member fn, not a field; call it: `.{name}(...)`")
            }
            InferenceDiagnostic::NamedGenericArg { name, reason, .. } => match reason {
                NamedArgReason::NotSelf => crate::diag::named_arg_not_self(name),
                NamedArgReason::NotATrait => crate::diag::named_arg_not_a_trait("Self"),
                NamedArgReason::OwnersSelf => crate::diag::NAMED_ARG_OWNERS_SELF.to_owned(),
                NamedArgReason::Duplicate => "`Self` is given more than once".to_owned(),
                NamedArgReason::Hole => crate::diag::NAMED_ARG_SELF_HOLE.to_owned(),
            },
            InferenceDiagnostic::FieldNotCallable {
                name,
                ty,
                receiver_ty,
                ..
            } => format!(
                "field `{name}` is not callable (its type is `{}`), and `{}` has no \
                 member `{name}`",
                ty.display(),
                receiver_ty.display()
            ),
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
            InferenceDiagnostic::AssignToImmutable { name, place, .. } => {
                if place == name {
                    format!("cannot assign to `{name}`: it is not declared `mut`")
                } else {
                    // A field chain: the root binding carries the blame —
                    // immutability is transitive, there is no per-field
                    // `mut` to add.
                    format!("cannot assign to `{place}`: `{name}` is not declared `mut`")
                }
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
            InferenceDiagnostic::ElidedVariantNoEnum {
                variant, expected, ..
            } => match expected {
                Some(expected) => format!(
                    "cannot resolve `::{variant}`: the expected type `{}` is not an enum — \
                     write `Enum::{variant}`",
                    expected.display()
                ),
                None => format!(
                    "cannot resolve `::{variant}` without an expected type — \
                     write `Enum::{variant}`"
                ),
            },
            InferenceDiagnostic::NoVariantsOnStruct { item, .. } => {
                format!(
                    "`{}` has no variants (it is a `struct` type)",
                    item.display_name()
                )
            }
            InferenceDiagnostic::QualifiedPathIsField { item, name, .. } => format!(
                "`{name}` is a field of `{}`, not a member — fields are reached through \
                 a value: `value.{name}`",
                item.display_name()
            ),
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
            InferenceDiagnostic::NonExhaustiveMatch {
                decl, uncovered, ..
            } => {
                let prefix = decl.display_name();
                let list = uncovered
                    .iter()
                    .map(|name| format!("`{prefix}::{name}`"))
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
                UnreachableReason::LiteralCovered(literal) => {
                    format!("unreachable arm: {literal} is already covered by a previous arm")
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
            // What CAN match is type-dependent since literal patterns
            // arrived: a `char` takes those too, so the old blanket "only
            // `_` or a binding" became false for exactly one type.
            InferenceDiagnostic::NonEnumScrutineeVariantPat { scrutinee, .. } => {
                let writable = match scrutinee {
                    Ty::Char => "`_`, a binding, or a character literal",
                    _ => "only `_` or a binding",
                };
                format!(
                    "a variant pattern needs an enum scrutinee; {writable} can match a `{}`",
                    scrutinee.display()
                )
            }
            InferenceDiagnostic::PatNoSuchVariant { item, name, .. } => {
                format!("`{}` has no variant `{name}`", item.display_name())
            }
            InferenceDiagnostic::PatLiteralTypeMismatch {
                expected, found, ..
            } => format!(
                "type mismatch: this `match` is on a `{}`, and `{}` cannot match one",
                expected.display(),
                found.display()
            ),
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
            InferenceDiagnostic::ReturnOutsideFn { .. } => {
                "`return` outside of a function: there is no enclosing `fn` body to return from"
                    .to_owned()
            }
            InferenceDiagnostic::ReturnInConstBlock { .. } => {
                "`return` inside a `const` block is not supported yet: it would have to \
                 leave the enclosing `fn` body, and a `const` block is compiled as a body \
                 of its own"
                    .to_owned()
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
            InferenceDiagnostic::GenericArgCount {
                item,
                expected,
                found,
                ..
            } => crate::diag::generic_arg_count(item.display_name(), *expected, *found),
            InferenceDiagnostic::NotGeneric {
                name,
                owner_list_hint,
                ..
            } => {
                let base = crate::diag::takes_no_generic_args(name);
                match owner_list_hint {
                    Some(owner) => {
                        let member = name.rsplit("::").next().unwrap_or(name);
                        format!(
                            "{base}; if these are meant for `{owner}`, write \
                             `{owner}::<...>::{member}`"
                        )
                    }
                    None => base,
                }
            }
            InferenceDiagnostic::ConstArgHole { .. } => crate::diag::CONST_ARG_HOLE.to_owned(),
            InferenceDiagnostic::UnexpectedRegionArg { param, .. } => {
                crate::diag::unexpected_region_arg(param)
            }
            InferenceDiagnostic::RegionArgAtMention { .. } => {
                crate::diag::REGION_ARG_AT_MENTION.to_owned()
            }
            InferenceDiagnostic::GenericArgKindMismatch {
                param,
                param_is_const,
                ..
            } => {
                if *param_is_const {
                    crate::diag::const_param_needs_value(param)
                } else {
                    crate::diag::type_param_needs_type(param)
                }
            }
            InferenceDiagnostic::MissingConstArgs { item, .. } => {
                format!(
                    "const arguments must be written explicitly; write `{}::<...>`",
                    item.display_name()
                )
            }
            InferenceDiagnostic::CannotInferGenericParam {
                param,
                owner,
                spelling,
                ..
            } => {
                let write = spelling.write(owner);
                format!(
                    "cannot infer the type parameter `{param}` of `{owner}`; \
                     write `{write}` to specify it"
                )
            }
            InferenceDiagnostic::ForgetBoundUnsatisfied {
                param, ty, reason, ..
            } => crate::diag::forget_bound_unsatisfied(param, &ty.display(), reason),
            InferenceDiagnostic::AssignToConstParam { name, .. } => {
                format!("cannot assign to `{name}`: it is a const parameter")
            }
            InferenceDiagnostic::FnConstArg { .. } => crate::diag::FN_CONST_ARG.to_owned(),
            InferenceDiagnostic::TypeConstArgUnsupported { is_block, .. } => {
                if *is_block {
                    crate::diag::CONST_BLOCK_TYPE_ARG.to_owned()
                } else {
                    crate::diag::TYPE_CONST_ARG_NOT_LITERAL.to_owned()
                }
            }
            InferenceDiagnostic::CannotInferNumberType { .. } => {
                "cannot infer the type of this number: it has no defining use — \
                 add a type annotation"
                    .to_owned()
            }
            InferenceDiagnostic::IntLiteralOutOfRange { literal, ty, .. } => {
                format!("`{literal}` does not fit in `{}`", ty.display())
            }
            InferenceDiagnostic::DerefNonPointer { ty, .. } => {
                format!("type `{}` cannot be dereferenced", ty.display())
            }
            InferenceDiagnostic::IndexNonArray { ty, .. } => {
                format!("type `{}` cannot be indexed", ty.display())
            }
            InferenceDiagnostic::IndexOutOfBounds { len, index, .. } => {
                crate::diag::index_out_of_bounds(*len, *index)
            }
            InferenceDiagnostic::EmptyArrayNeedsAnnotation { .. } => {
                "cannot infer the element type of an empty array; add a type annotation".to_owned()
            }
            InferenceDiagnostic::ArrayConstArg { .. } => crate::diag::ARRAY_CONST_ARG.to_owned(),
            InferenceDiagnostic::BuiltinExpectsRawPtr { builtin, found, .. } => {
                format!(
                    "`{}` expects a raw pointer (`T.&raw` or `T.&raw mut`) here, found `{}`",
                    builtin.name(),
                    found.display()
                )
            }
            InferenceDiagnostic::BuiltinNotFirstClass { builtin, .. } => {
                format!(
                    "`{}` must be called directly; its pointer parameter accepts both \
                     `T.&raw` and `T.&raw mut`, so it has no one function type to be \
                     a value at",
                    builtin.name()
                )
            }
            InferenceDiagnostic::DotThroughBorrow {
                name, receiver_ty, ..
            } => format!(
                "`{}` is a borrow, so `.{name}` does not reach through it — \
                 there is no auto-deref; write `.*.{name}`",
                receiver_ty.display()
            ),
            InferenceDiagnostic::BorrowNonPlace { .. } => {
                "`.&` can only borrow a variable, one of its fields, or a `static`".to_owned()
            }
            InferenceDiagnostic::BorrowMutImmutable { name, place, .. } => {
                if place == name {
                    format!("cannot borrow `{name}` as `.&mut`: it is not declared `mut`")
                } else {
                    format!("cannot borrow `{place}` as `.&mut`: `{name}` is not declared `mut`")
                }
            }
            InferenceDiagnostic::BorrowMutItem { constness, .. } => match constness {
                Constness::Static => {
                    "cannot borrow a `static` as `.&mut`: `static mut` is not supported yet"
                        .to_owned()
                }
                Constness::Const => {
                    "cannot borrow a `const` as `.&mut`: a `const` is copied at every \
                     mention, so there is no one place to borrow"
                        .to_owned()
                }
            },
            InferenceDiagnostic::BorrowMutThroughShared { ty, .. } => format!(
                "cannot borrow `.&mut` through `{}`: an exclusive borrow needs {} parent",
                ty.display(),
                // The governing step may be a RAW pointer when a `.&mut`
                // sits behind one: `pb.*.*.&mut` for `pb: T.&mut.&raw`.
                match ty {
                    Ty::Borrow { .. } => "a `.&mut`",
                    _ => "a `.&raw mut`",
                }
            ),
            InferenceDiagnostic::BorrowThroughRawPointer { ty, .. } => format!(
                "cannot mint a safe borrow through `{}`: a raw pointer carries no region \
                 for the new borrow to be bounded by",
                ty.display()
            ),
            InferenceDiagnostic::MoveOutOfBorrow { ty, .. } => format!(
                "{}: `{}` cannot be copied",
                crate::diag::MOVE_OUT_OF_BORROW,
                ty.display()
            ),
            InferenceDiagnostic::AddrOfNonPlace { .. } => {
                "`.&raw` can only take the address of a variable, a chain of its \
                 fields and elements, a `static`/`const` item, or a chain rooted \
                 in a deref"
                    .to_owned()
            }
            InferenceDiagnostic::AddrOfMutThroughShared { ty, .. } => format!(
                "cannot take `.&raw mut` through `{}`: minting a mutating address needs {}",
                ty.display(),
                exclusive_flavor(ty)
            ),
            InferenceDiagnostic::AddrOfMutImmutable { name, place, .. } => {
                if place == name {
                    format!("cannot take `.&raw mut` of `{name}`: it is not declared `mut`")
                } else {
                    // A field chain: the root binding carries the blame —
                    // mutability is transitive, exactly as for assignments.
                    format!("cannot take `.&raw mut` of `{place}`: `{name}` is not declared `mut`")
                }
            }
            InferenceDiagnostic::AddrOfMutItem {
                item, constness, ..
            } => match constness {
                Constness::Static => format!(
                    "cannot take `.&raw mut` of `{}`: `static mut` is not supported yet",
                    item.display_name()
                ),
                Constness::Const => format!(
                    "cannot take `.&raw mut` of `{}`: a `const` is copied into each use, \
                     so there is no place to modify",
                    item.display_name()
                ),
            },
            InferenceDiagnostic::AssignThroughShared { ty, .. } => format!(
                "cannot assign through `{}`: writing needs {}",
                ty.display(),
                exclusive_flavor(ty)
            ),
            InferenceDiagnostic::TraitNotValue { name, .. } => {
                format!("`{name}` is a trait, not a value")
            }
            InferenceDiagnostic::UnsatisfiedBound {
                param, trait_, ty, ..
            } => format!(
                "the bound `{param}: {trait}` is not satisfied here: `{ty}` does not \
                 implement `{trait}`",
                trait = trait_.display_name(),
                ty = ty.display()
            ),
            InferenceDiagnostic::NoTraitImpl { trait_, ty, .. } => format!(
                "`{}` does not implement `{}`",
                ty.display(),
                trait_.display_name()
            ),
            InferenceDiagnostic::QualifiedTraitMemberOnType {
                type_name,
                member,
                traits,
                ..
            } => {
                let providers = match traits.split_last() {
                    Some((last, [])) => format!("`{last}` provides it"),
                    Some((last, rest)) => format!(
                        "{} and `{last}` provide it",
                        rest.iter()
                            .map(|t| format!("`{t}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None => String::new(),
                };
                let spellings = traits
                    .iter()
                    .map(|t| format!("`{t}::{member}(value)`"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                format!(
                    "`{member}` is not a member of `{type_name}` itself: {providers} — \
                     write {spellings} (a trait member is spelled through its trait)"
                )
            }
            InferenceDiagnostic::AssocTypeReserved { trait_, name, .. } => format!(
                "`{}::{name}` is an associated type; associated types are not supported yet",
                trait_.display_name()
            ),
            InferenceDiagnostic::CannotInferSelf {
                trait_name, member, ..
            } => format!(
                "cannot infer `Self` for `{trait_name}::{member}`: no argument \
                 determines the implementing type — annotate an argument"
            ),
            InferenceDiagnostic::QualifiedTraitMemberValue {
                trait_name, member, ..
            } => format!(
                "a member value is impl-specific, so it must name the implementer: \
                 `{trait_name}::<Self = Type>::{member}`"
            ),
            InferenceDiagnostic::BoundMemberValue {
                trait_name, member, ..
            } => format!(
                "`{trait_name}::{member}` on a rigid `Self` comes from the enclosing \
                 dictionary, so it cannot be used as a value yet; call it directly"
            ),
            InferenceDiagnostic::TraitHasNoMember { trait_, name, .. } => {
                format!("`{}` has no requirement `{name}`", trait_.display_name())
            }
            InferenceDiagnostic::BoundFnValue { name, .. } => format!(
                "`{name}` has bounds on its type parameters, so it cannot be used as a \
                 value yet; call it directly"
            ),
            InferenceDiagnostic::GenericTraitReserved { name, .. } => format!(
                "`{name}` is a reserved generic trait (generic traits are not supported \
                 yet) and cannot be used"
            ),
            InferenceDiagnostic::MemberCallAmbiguity {
                name,
                receiver_ty,
                candidates,
                ..
            } => {
                let escapes: Vec<String> = candidates
                    .iter()
                    .map(|candidate| candidate.escape(name))
                    .collect();
                let list = match escapes.split_last() {
                    Some((last, [])) => last.clone(),
                    Some((last, [first])) => format!("{first} or {last}"),
                    Some((last, rest)) => format!("{}, or {last}", rest.join(", ")),
                    None => String::new(),
                };
                format!(
                    "`{name}` is ambiguous on `{recv}`: it could be {list} — \
                     spell the one you mean",
                    recv = receiver_ty.display()
                )
            }
            InferenceDiagnostic::NestedBoundUse { .. } => {
                "code nested inside a bounded fn (a nested fn literal or a `const` \
                 block) cannot use the enclosing bounds yet (it would have to \
                 capture the dictionary)"
                    .to_owned()
            }
            InferenceDiagnostic::MemberOwnConstArgs { path, .. } => format!(
                "`{path}` declares a const parameter of its own, and const member \
                 arguments are not supported yet (a member's type arguments are \
                 written here; its region arguments are always inferred)"
            ),
            InferenceDiagnostic::MemberGenericArgCount {
                path,
                expected,
                found,
                ..
            } => crate::diag::generic_arg_count(path, *expected, *found),
            InferenceDiagnostic::VariantOwnGenericArgs {
                owner,
                variant,
                suggest_owner_list,
                ..
            } => {
                if *suggest_owner_list {
                    format!(
                        "a variant has no generic arguments of its own: they \
                         belong to the owner — write `{owner}::<...>::{variant}`"
                    )
                } else {
                    format!(
                        "a variant has no generic arguments of its own: \
                         arguments written on `{owner}::{variant}` cannot be \
                         applied here"
                    )
                }
            }
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

    // The item's own generic binder (TR06): its type params resolve to rigid
    // `Ty::Param`s in every type position lowered inside this body, and its
    // const params are value names (resolved by `scopes`). The whole body
    // is inside the binder — for a generic item the root IS the binder's
    // literal — so one body-wide scope is exact.
    let generics: &[GenericParamData] = crate::item_data(db, item)
        .as_ref()
        .map(|it| it.generics.as_slice())
        .unwrap_or(&[]);
    // Members always take the scope (even with an empty binder, `Self` is
    // in scope in every type position of the member's body).
    let type_params = if generics.is_empty() && item.member(db).is_none() {
        ParamScope::default()
    } else {
        generic_param_scope(db, item, generics)
    };

    if let Some(root) = body.root {
        // Check the body against the item's annotation, if any. For a
        // generic item the "annotation" is the scheme synthesized from the
        // literal's own (mandatory) annotations, lowered under the param
        // scope — the same `Ty` `signature` returns.
        let ty_ref = crate::item_data(db, item)
            .as_ref()
            .and_then(|it| it.type_ref.as_ref());
        let expected = lower_type_ref_in(
            db,
            file,
            ty_ref.unwrap_or(&TypeRef::Hole),
            &mut table,
            &type_params,
        );
        let cause = ty_ref.is_some().then_some(Cause::ItemAnnotation);
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
        ctx.type_params = type_params;
        ctx.own_generics = generics;
        ctx.own_item = Some(item_loc(db, item));
        ctx.infer_expr_with(root, &expected, cause);
    } else {
        ctx = InferCtx::new(db, file, body, resolutions(db, item), &mut table, &no_group);
    }

    ctx.finish()
}

/// One entry of [`InferCtx::return_targets`] — a body a `return` could be
/// leaving.
#[derive(Debug, Clone)]
enum ReturnTarget {
    /// A `fn` literal: the type its value must have (its own `-> T`, or
    /// its inferred variable), and why that type is expected, for blame on
    /// the operand.
    Fn { ty: Ty, cause: Option<Cause> },
    /// A `const { ... }` block: a body of its own, but a `return` landing
    /// here is reserved — it should bail from the OUTER fn body, which
    /// needs cross-body machinery no v1 pass has. Carries nothing: the
    /// reservation is refused outright, never checked against anything.
    ConstBlock,
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
    /// The return-target stack: for each BODY currently being inferred
    /// (innermost last) — a `fn` literal or a `const` block — the type its
    /// value must have, plus the [`Cause`] that made it expected. A
    /// `return` checks its operand against the innermost entry, which is
    /// literally the same check the body's TAIL expression gets: one
    /// mechanism, so an annotated return type blames the operand and cites
    /// the annotation, and an inferred one is pinned by `return e` exactly
    /// as a tail would pin it.
    ///
    /// A `const` block pushes an entry for the same reason it resets
    /// [`Self::loop_sinks`]: it is a compile-time unit of its own (MIR
    /// lowers it to a separate body). Its entry is MARKED, though — a
    /// `return` landing on one is RESERVED, not checked (see
    /// [`InferenceDiagnostic::ReturnInConstBlock`]). Empty — at an item
    /// initializer's top level — is the outside-a-function error.
    return_targets: Vec<ReturnTarget>,
    /// The item's own generic binder scope (type params → rigid
    /// [`Ty::Param`]s, const params → rigid [`ConstArgValue::Param`]s),
    /// applied by [`Self::lower_type_ref`] to every type annotation
    /// lowered inside this body (TR06). Empty for non-generic items and in
    /// group mode (generic items never join groups).
    type_params: ParamScope,
    /// The item's own binder entries, for const-param value types
    /// ([`Resolution::ConstParam`] carries only the index).
    own_generics: &'db [GenericParamData],
    /// The item's own identity — the `item` half of a rigid
    /// [`ConstArgValue::Param`] minted from a body-side const-param read
    /// (`Buf::<const N>(...)`). `None` only in group mode, where const
    /// params can't occur (generic items never join groups).
    own_item: Option<ItemLoc>,
    /// Every generic-item mention instantiated with fresh variables, so
    /// [`Self::finish`] can report type params the whole traversal never
    /// pinned (the mention-site sibling of `NeedsAnnotation`).
    pending_instantiations: Vec<PendingInstantiation>,
    /// Every empty array literal (`[]`) whose element type started as a
    /// fresh variable: if the whole traversal (joins included) never pins
    /// it, [`Self::finish`] reports
    /// [`InferenceDiagnostic::EmptyArrayNeedsAnnotation`].
    pending_empty_arrays: Vec<(ExprId, Ty)>,
    /// Every integer literal, with its NUMBER-CLASS variable: after the
    /// whole traversal (joins included), [`Self::finish`] range-checks each
    /// against its resolved type — or reports the no-defining-use
    /// diagnostic when nothing ever pinned it (never defaulted — T01).
    pending_number_literals: Vec<PendingNumberLiteral>,
    /// Literals directly under a unary minus (`-5`): their range check
    /// applies the sign (`-128` fits `i8`; plain `128` does not). Filled by
    /// the [`crate::body::ExprData::Neg`] arm before its operand is
    /// visited.
    negated_literals: rustc_hash::FxHashSet<ExprId>,
    /// Bound obligations from instantiation edges, resolved in
    /// [`Self::finish`] once joins have solved: per KEY expression, in
    /// canonical slot order (push order — see
    /// [`crate::traits::bound_slots`]).
    pending_obligations: Vec<PendingObligation>,
    /// `const { ... }` nesting depth: a const block lowers to a separate
    /// MIR body, so — like a nested fn literal — it cannot reach the root
    /// body's dictionary parameters (see [`Self::in_nested_body`]).
    const_block_depth: usize,
    /// Every expression that is a `Call`'s callee — the direct-call set a
    /// bound-carrying generic mention must be in (its VALUE would need a
    /// captured dictionary, which is reserved).
    direct_callees: rustc_hash::FxHashSet<ExprId>,
    /// Every expression standing in PLACE position: the receiver of a
    /// field, deref or index step, the place under `.&`/`.&mut`/`.&raw`,
    /// and an assignment target. A `.*` there is a projection STEP, not a
    /// value read — see [`InferCtx::finish_deref_reads`].
    place_positions: rustc_hash::FxHashSet<ExprId>,
    /// The dot-call receivers a MEMBER took (as its last argument), so
    /// they were consumed BY VALUE after all. The exception to
    /// [`Self::place_positions`], and filled during inference because
    /// only resolution knows it: an fn-typed field called with the same
    /// syntax reads the field and leaves the receiver a place.
    value_receivers: rustc_hash::FxHashSet<ExprId>,
    /// Every `.*` read out of a borrow, with the referent type it read.
    /// Judged at the end of inference, not at the expression — see
    /// [`InferCtx::finish_deref_reads`].
    deref_reads: Vec<(ExprId, Ty)>,
    /// The projection steps of a chain rooted in a borrow `.*` whose own
    /// read [`Self::place_positions`] defers — grown as the chain is
    /// inferred, so the copy question lands on the step that actually
    /// materializes a value. See [`InferCtx::carry_borrow_projection`].
    borrow_projections: rustc_hash::FxHashSet<ExprId>,
}

/// One trait that could carry a concrete-receiver dot-call — see
/// [`InferCtx::trait_call_candidates`].
struct TraitCallCandidate {
    trait_: ItemLoc,
    /// The impl's member for the requirement; `None` when the impl doesn't
    /// provide it (the impl's definition site carries that diagnostic).
    member: Option<ItemLoc>,
    /// Whether the impl member can take the call (TR01's structural shape).
    dot_callable: bool,
    /// Whether the impl member's signature is broken (its definition site
    /// carries the diagnostic; the call refuses silently).
    broken: bool,
}

/// One `T: Trait` obligation at an instantiation edge, awaiting its
/// post-solve resolution into a [`DictEntry`].
struct PendingObligation {
    /// The instantiation expression dictionary operands attach to (the
    /// mention for path calls, the call for dot-form/qualified calls).
    key: ExprId,
    /// The param's instantiation (a fresh var, or the written arg).
    var: Ty,
    trait_: ItemLoc,
    /// The bounded param's declared name, for diagnostics.
    param_name: String,
    /// Whether the edge sits in a NESTED body (a nested fn literal or a
    /// `const` block): a Forward resolution there would need to capture
    /// the root body's dictionary — reserved ([`InferenceDiagnostic::NestedBoundUse`]).
    nested: bool,
}

/// One integer literal awaiting its post-traversal range/pinned check.
struct PendingNumberLiteral {
    /// The literal expression.
    expr: ExprId,
    /// The literal's magnitude as written.
    value: u128,
    /// Whether a direct unary minus negates it.
    negated: bool,
    /// The literal's NUMBER-CLASS variable.
    var: Ty,
}

/// What a use site knows about the member it reached and the member's
/// `ItemLoc` cannot say: how the site NAMES it, how a turbofish would be
/// written there, and what sits one segment to its left. Passed as one
/// value because every message about a member's own arguments needs the
/// same answers, and re-deriving them per message is how two of them came
/// to disagree.
struct MemberSite {
    /// The owner-qualified naming used in every such message
    /// (`Option::map`, `usize::n`, `Mk::mk`).
    path: String,
    /// How this site writes the member's own turbofish.
    spelling: TurbofishSpelling,
    /// Whether the segment to the member's left is a TRAIT. It decides
    /// what a written `Self` is told: on a trait's member the name is
    /// merely misplaced ([`NamedArgReason::OwnersSelf`]), on an inherent
    /// owner there is no `Self` to name at all
    /// ([`NamedArgReason::NotATrait`]).
    owner_is_trait: bool,
    /// A GENERIC owner whose own list this path left unwritten — the one
    /// place a list the member cannot take may still have a home, one
    /// segment to the left (`Owner::<...>::member`).
    owner_hint: Option<String>,
}

/// One instantiation of a generic item's scheme at a mention: which fresh
/// variable stands for which of the item's type params.
struct PendingInstantiation {
    /// The mentioning expression (a `NameRef` or `GenericApp`).
    expr: ExprId,
    item: ItemLoc,
    /// The blame name and the spelling for
    /// [`InferenceDiagnostic::CannotInferGenericParam`] — carried from the
    /// site, because only the site knows how the binder can be WRITTEN (a
    /// member's is not written the way its `ItemLoc` renders).
    owner: String,
    spelling: TurbofishSpelling,
    /// `(param name, the fresh variable, whether it opted out of the
    /// default `forget` bound)` per *type* param. The opt-out flag is
    /// CARRIED rather than re-derived at check time: a builtin's binder has
    /// no item tree to look it up in, so re-deriving silently lost the flag
    /// and refused `alloc_array::<Lin>` — record, don't re-derive.
    params: Vec<(String, Ty, bool)>,
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
            return_targets: Vec::new(),
            type_params: ParamScope::default(),
            own_generics: &[],
            own_item: None,
            pending_instantiations: Vec::new(),
            pending_empty_arrays: Vec::new(),
            pending_number_literals: Vec::new(),
            negated_literals: rustc_hash::FxHashSet::default(),
            pending_obligations: Vec::new(),
            const_block_depth: 0,
            direct_callees: body
                .exprs
                .iter()
                .filter_map(|(_, data)| match data {
                    ExprData::Call { callee, .. } => Some(*callee),
                    _ => None,
                })
                .collect(),
            place_positions: place_positions(body),
            value_receivers: rustc_hash::FxHashSet::default(),
            deref_reads: Vec::new(),
            borrow_projections: rustc_hash::FxHashSet::default(),
        }
    }

    /// A fresh EXISTENTIAL region. Minted by the constraint store, which
    /// also needs to mint (a voted join of borrows produces the MEET of
    /// its branches) — one counter, one numbering.
    fn fresh_region(&mut self) -> Region {
        self.constraints.fresh_region()
    }

    fn push_outlives(
        &mut self,
        sup: Region,
        sub: Region,
        origin: ExprId,
        reason: RegionConstraintReason,
    ) {
        self.constraints.push_outlives(sup, sub, origin, reason);
    }

    /// See `Constraints::try_reborrow` — the ONE emitter, shared with the
    /// join solver so that wrapping a borrow in an `if` cannot change
    /// which obligations it incurs.
    fn try_reborrow(&mut self, expr: ExprId, actual: &Ty, expected: &Ty) -> bool {
        self.constraints
            .try_reborrow(self.table, actual, expected, expr)
    }

    /// The SHARED step governing a place chain, if there is one: `None`
    /// when the place may be written through (or when nothing is known
    /// about it yet), `Some(ty)` for the pointer or borrow whose shared
    /// flavor refuses. The one judgement behind every "cannot write
    /// through this" rule — assignment, `.&mut`, `.&raw mut`, and the
    /// implicit mutable reborrow.
    ///
    /// Fields and elements are transparent. A deref is where a flavor
    /// enters, and the two worlds part there:
    ///
    /// * `T.&raw mut` grants the permission and STOPS. Reading a raw
    ///   pointer out of a place COPIES it, and a copy carries the full
    ///   permission — that laundering is the raw world's own, gated by
    ///   `unsafe` rather than by this walk.
    /// * `T.&raw` and `T.&` stop too, refusing: a shared flavor never
    ///   becomes a write permission.
    /// * `T.&mut` RECURSES into the place that holds it. Reading a
    ///   `.&mut` out is a REBORROW, not a copy (M07's affinity), so the
    ///   holder must itself be reachable exclusively — `bb.*.* = 5`
    ///   through a `.&mut` held behind a `.&` writes the root through a
    ///   shared borrow, which is the one thing that may not happen.
    ///
    /// A name (or item) root permits: writing through a pointer
    /// reassigns no binding, so the root's own `mut`ness is a different
    /// rule, judged by the callers on NAME-rooted chains only.
    fn shared_step_governing(&mut self, place: ExprId) -> Option<Ty> {
        match &self.body.exprs[place] {
            ExprData::Field { receiver, .. } => {
                let receiver = *receiver;
                self.shared_step_governing(receiver)
            }
            ExprData::Index { base, .. } => {
                let base = *base;
                self.shared_step_governing(base)
            }
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                let receiver_ty = self
                    .result
                    .type_of_expr
                    .get(receiver)
                    .cloned()
                    .unwrap_or(Ty::Error);
                match self.resolve_shallow(&receiver_ty) {
                    Ty::RawPtr { mutable: true, .. } => None,
                    Ty::Borrow { mutable: true, .. } => self.shared_step_governing(receiver),
                    shared @ (Ty::RawPtr { .. } | Ty::Borrow { .. }) => Some(shared),
                    // An unknown or broken receiver: the read already
                    // carries its own diagnostic.
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The place rules for a SAFE borrow — `check_addr_of_place`'s twin,
    /// one flavor up. The accepted places are identical (a variable, a
    /// chain of its fields, a `static`, or a deref-rooted chain) and the
    /// `mut` flavor asks the same question of the chain; only what the
    /// deref case names differs, since a safe borrow must also be BOUNDED
    /// by its parent's region.
    ///
    /// A `.&mut` is refused wherever a SHARED step governs the place —
    /// through a shared parent (`b.*.&mut`), and through a `.&mut` that
    /// itself sits behind one (`bb.*.*.&mut`), which
    /// [`InferCtx::shared_step_governing`] walks out. A shared flavor
    /// must never launder into a write permission. That single rule is
    /// also what makes G14's auto-ref exception safe to apply — the
    /// compiler-inserted borrow of `x.*` inherits `x`'s flavor and can
    /// only weaken it.
    ///
    /// A deref-rooted place also incurs the SAME outlives edge an implicit
    /// reborrow would (`try_reborrow`'s `r_src : r_tgt`): the parent
    /// borrow's region must outlive `region`, the region this EXPLICIT
    /// borrow is minted at. Without this a written `x.*.&`/`x.*.&mut`
    /// is a silent, unbounded region-laundering path — the implicit path
    /// (any other USE of a borrow-typed value) already pushes the edge,
    /// so a spelled-out reborrow must not be the one place that skips it.
    fn check_borrow_place(&mut self, borrow: ExprId, mutable: bool, place: ExprId, region: Region) {
        // The whole chain's write permission, needed only by the `.&mut`
        // flavor: a shared borrow of a shared place is exactly what a
        // shared borrow is for.
        let governing = mutable.then(|| self.shared_step_governing(place)).flatten();
        let mut segments: Vec<String> = Vec::new();
        let mut root = place;
        loop {
            match &self.body.exprs[root] {
                ExprData::Field { receiver, name, .. } => {
                    segments.push(format!(".{name}"));
                    root = *receiver;
                }
                ExprData::Index { base, .. } => {
                    segments.push("[_]".to_owned());
                    root = *base;
                }
                _ => break,
            }
        }
        match &self.body.exprs[root] {
            ExprData::NameRef(name) => match self.resolutions.get(root) {
                Some(Resolution::Local(binding)) => {
                    if mutable && !self.body.bindings[*binding].mutable {
                        segments.reverse();
                        let place_text = format!("{name}{}", segments.concat());
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::BorrowMutImmutable {
                                borrow,
                                root,
                                name: name.clone(),
                                place: place_text,
                            });
                    }
                }
                Some(Resolution::Item(loc)) => {
                    if mutable {
                        let constness = crate::item_data(self.db, loc.to_id(self.db))
                            .as_ref()
                            .and_then(|it| it.kind.constness())
                            .unwrap_or(Constness::Static);
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::BorrowMutItem {
                                borrow,
                                root,
                                item: loc.clone(),
                                constness,
                            });
                    }
                }
                Some(
                    Resolution::ConstParam(_)
                    | Resolution::TypeItem(_)
                    | Resolution::TraitItem(_)
                    | Resolution::Builtin(_),
                ) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::BorrowNonPlace { expr: borrow });
                }
                Some(Resolution::Ambiguous(_)) | None => {}
            },
            // `r.*.field.&mut` — a reborrow through the parent named by
            // this outermost deref. A RAW parent is refused outright:
            // `.&raw` is deliberately not a decayed safe borrow, so a
            // safe borrow may not be minted from one.
            ExprData::Deref { receiver } => {
                let receiver_ty = self
                    .result
                    .type_of_expr
                    .get(*receiver)
                    .cloned()
                    .unwrap_or(Ty::Error);
                match self.resolve_shallow(&receiver_ty) {
                    Ty::Borrow {
                        mutable: parent_mutable,
                        region: parent_region,
                        ..
                    } => {
                        // A `.&mut` needs the whole chain to be reachable
                        // exclusively, not merely its parent step.
                        if let Some(ty) = governing {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::BorrowMutThroughShared { borrow, ty });
                            return;
                        }
                        // A `.&` through a `.&mut` parent: the parent
                        // SUSPENDS — degradation, same as the implicit
                        // path. Every other pairing is an ordinary
                        // reborrow, bounded by the parent's region: a
                        // shared parent does not mean an UNBOUNDED child.
                        let reason = if parent_mutable && !mutable {
                            RegionConstraintReason::Degradation
                        } else {
                            RegionConstraintReason::Reborrow
                        };
                        self.push_outlives(parent_region, region.clone(), borrow, reason);
                    }
                    resolved @ Ty::RawPtr { .. } => {
                        // Refused for BOTH flavors, deliberately: a raw
                        // pointer carries no region, so a safe borrow
                        // minted through one would have nothing to bound
                        // it. `.&mut` through a shared parent one flavor
                        // up (`BorrowMutThroughShared`) is a different
                        // rule — that parent DOES have a region, it is
                        // just the wrong permission.
                        self.result.diagnostics.push(
                            InferenceDiagnostic::BorrowThroughRawPointer {
                                borrow,
                                ty: resolved,
                            },
                        );
                    }
                    _ => {}
                }
            }
            ExprData::Missing => {}
            _ => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::BorrowNonPlace { expr: borrow });
            }
        }
    }

    /// Judge the recorded `.*` reads. Reading `x.*` copies the referent —
    /// legal exactly when the referent IS copyable — the rule M07 makes
    /// load-bearing, and the reason safe `.*` needed a ruling at all. But
    /// none of the three questions it asks can be answered at the
    /// expression, so all three are answered here.
    ///
    /// A read the checker turned into a REBORROW moved nothing: the
    /// parent suspends for the child loan's region rather than being
    /// duplicated. Whether that happened is known only once `check` has
    /// run.
    ///
    /// A read in PLACE position — `b.*.x`, `b.*[i]`, `b.*.x = 9`,
    /// `b.*.q.&mut`, `b.*.*` — copies only what lies beyond it (or
    /// nothing at all), so an affine referent is no obstacle there: it is
    /// projected THROUGH, never duplicated. That set is
    /// [`place_positions`], collected up front because a `.*` is typed
    /// long before its parent is. What lies beyond it is not thereby
    /// unjudged: [`InferCtx::carry_borrow_projection`] pushes each further
    /// step of the chain here in the deferred `.*`'s stead, so `b.*.q`
    /// with an exclusive `q` is refused as the copy it is.
    ///
    /// Its one exception is resolution-dependent: a dot-call's receiver
    /// becomes the member's last argument BY VALUE (G13's structural
    /// selection), but the same syntax on an fn-typed FIELD reads the
    /// field and leaves the receiver a place. Only the resolved call
    /// knows which, so it records the by-value ones in
    /// [`InferCtx::value_receivers`].
    ///
    /// What is left is a genuine copy.
    fn finish_deref_reads(&mut self, reborrows: &ArenaMap<ExprId, bool>) {
        for (expr, referent) in std::mem::take(&mut self.deref_reads) {
            if reborrows.contains_idx(expr) {
                continue;
            }
            if self.place_positions.contains(&expr) && !self.value_receivers.contains(&expr) {
                continue;
            }
            let resolved = self.resolve_shallow(&referent);
            if self.is_copyable(&resolved) {
                continue;
            }
            self.result
                .diagnostics
                .push(InferenceDiagnostic::MoveOutOfBorrow { expr, ty: resolved });
        }
    }

    /// Judge the MUTABLE reborrows the checker inserted. An implicit
    /// reborrow is a `.&mut` mint with the `.&mut` left unwritten —
    /// `set_to(bb.*, 5)` is `set_to(bb.*.&mut, 5)` — so it answers to the
    /// same rule [`InferCtx::check_borrow_place`] applies to the written
    /// form: no shared step may govern the place it is minted from.
    ///
    /// Deferred to the end for the same reason the reads are: whether a
    /// mention became a reborrow at all is the checker's answer, not the
    /// expression's. A source that is no place (a call's result, a
    /// spelled-out borrow) has no chain to walk and passes.
    fn finish_mut_reborrows(&mut self, reborrows: &ArenaMap<ExprId, bool>) {
        let sources: Vec<ExprId> = reborrows
            .iter()
            .filter(|(_, mutable)| **mutable)
            .map(|(expr, _)| expr)
            .collect();
        for expr in sources {
            if let Some(ty) = self.shared_step_governing(expr) {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::BorrowMutThroughShared { borrow: expr, ty });
            }
        }
    }

    /// Carry a borrow-rooted projection chain one step. The `.*` at its
    /// root read nothing — a place-position deref is projected THROUGH —
    /// so THIS step's type is what a read would materialize, and it joins
    /// [`Self::deref_reads`] to be judged by the one site that judges
    /// them. A step that is itself a place position keeps the chain (and
    /// the deferral) going, so the question always lands on the step the
    /// value actually comes out of.
    fn carry_borrow_projection(&mut self, expr: ExprId, receiver: ExprId, ty: &Ty) {
        if !self.borrow_projections.contains(&receiver) {
            return;
        }
        self.deref_reads.push((expr, ty.clone()));
        if self.place_positions.contains(&expr) {
            self.borrow_projections.insert(expr);
        }
    }

    /// Whether a value of this type may be duplicated by a plain read.
    ///
    /// Two affine classes now. `T.&mut`: duplicating it would duplicate a
    /// PERMISSION, and the whole exclusivity story rests on it being
    /// unduplicable. And LINEAR types — those without the `forget`
    /// capability: duplicating one would duplicate an OBLIGATION whose two
    /// copies name the same resource, which is how a checked-linear
    /// `String` would come to be freed twice. Copyability is therefore not
    /// a separate judgement from linearity; it follows from it, and the one
    /// arm below is the whole of the connection.
    fn is_copyable(&self, ty: &Ty) -> bool {
        if !crate::capability::has_forget(self.db, ty) {
            return false;
        }
        match ty {
            Ty::Borrow { mutable, .. } => !*mutable,
            Ty::Record(rec) => rec.fields.iter().all(|(_, ty)| self.is_copyable(ty)),
            Ty::Array { elem, .. } => self.is_copyable(elem),
            Ty::Named(named) => match crate::ty::type_underlying_for(self.db, named) {
                Some(underlying) => self.is_copyable(&underlying),
                None => true,
            },
            Ty::Variant(variant) => match crate::ty::variant_payloads_for(self.db, variant) {
                Some(payloads) => payloads.iter().all(|ty| self.is_copyable(ty)),
                None => true,
            },
            _ => true,
        }
    }

    /// Lower a type annotation under this body's generic binder (the
    /// param scope is empty outside generic items). Every annotation
    /// lowered during inference of this body must go through here, not
    /// through the free function — that is what makes `x: T` inside a
    /// generic body resolve to the rigid param.
    fn lower_type_ref(&mut self, type_ref: &TypeRef) -> Ty {
        let ty = lower_type_ref_in(self.db, self.file, type_ref, self.table, &self.type_params);
        // Signature lowering has no inference context, so it turns every
        // `@_` into `Region::Error`. Inside a BODY the wildcard is exactly
        // what it says — an existential — so mint one here. Body
        // annotations are the only place `@_` is legal, which is the
        // no-elision rule read correctly: a signature's regions are
        // parameters and must be named.
        self.mint_wildcard_regions(type_ref, &ty)
    }

    /// Replace the `Region::Error`s a wildcard produced with fresh
    /// variables, guided by the SYNTAX (`type_ref`) so a genuinely broken
    /// region — an unknown name, a wrong-kind argument — keeps its error.
    fn mint_wildcard_regions(&mut self, type_ref: &TypeRef, ty: &Ty) -> Ty {
        match (type_ref, ty) {
            (
                TypeRef::Borrow {
                    region: written,
                    inner,
                    ..
                },
                Ty::Borrow {
                    mutable,
                    region,
                    referent,
                },
            ) => {
                let region = match written {
                    Some(RegionRef::Wildcard) | None => self.fresh_region(),
                    _ => region.clone(),
                };
                Ty::borrow(
                    *mutable,
                    region,
                    self.mint_wildcard_regions(inner, referent),
                )
            }
            (TypeRef::RawPtr { inner, .. }, Ty::RawPtr { mutable, pointee }) => {
                Ty::raw_ptr(*mutable, self.mint_wildcard_regions(inner, pointee))
            }
            (TypeRef::Array { elem, .. }, Ty::Array { elem: lowered, len }) => {
                Ty::array(self.mint_wildcard_regions(elem, lowered), len.clone())
            }
            (TypeRef::Fn { params, ret, .. }, Ty::Fn(f)) => f.rebuilt(
                params
                    .iter()
                    .zip(&f.params)
                    .map(|(w, l)| self.mint_wildcard_regions(w, l))
                    .collect(),
                match ret {
                    Some(ret) => self.mint_wildcard_regions(ret, &f.ret),
                    None => f.ret.clone(),
                },
            ),
            (TypeRef::Record(written), Ty::Record(rec)) => Ty::record(
                written
                    .iter()
                    .zip(&rec.fields)
                    .map(|((_, w), (name, l))| (name.clone(), self.mint_wildcard_regions(w, l)))
                    .collect(),
            ),
            // A generic type's ARGUMENTS — the same walker hole closed
            // four times over: `Ty::Record`'s fields were walked,
            // `Ty::Named`'s args were not. Without it,
            // `let o: Opt::<usize.&::<@_>>` keeps the `Region::Error` the
            // wildcard lowered to, which surfaces either as a type spelling
            // nobody may write (`@{error}`) or as the compiler accusing
            // itself — and `Opt::<V.&mut>` is a shape users write, so `@_`
            // inside one is what they type.
            //
            // Both argument positions are handled: a wildcard NESTED in a
            // type argument (`Opt::<usize.&::<@_>>`), and a wildcard that
            // IS the argument (`Slice::<@_, usize>` — unreachable until a
            // type declaration may take a region, live the day it can).
            (TypeRef::Apply { args: written, .. }, Ty::Named(named)) => Ty::Named(NamedTy {
                decl: named.decl.clone(),
                args: self.mint_wildcard_regions_args(written, &named.args),
            }),
            (TypeRef::Apply { args: written, .. }, Ty::Variant(variant)) => {
                Ty::Variant(VariantTy {
                    args: self.mint_wildcard_regions_args(written, &variant.args),
                    ..variant.clone()
                })
            }
            _ => ty.clone(),
        }
    }

    /// [`Self::mint_wildcard_regions`] over a generic argument list, paired
    /// with the syntax that produced it.
    fn mint_wildcard_regions_args(
        &mut self,
        written: &[GenericArgRef],
        lowered: &[GenericArg],
    ) -> Vec<GenericArg> {
        // A length disagreement is an arity error, already diagnosed at the
        // mention: pair by index and leave anything unpaired as lowered.
        lowered
            .iter()
            .enumerate()
            .map(|(index, arg)| match arg {
                // Exhaustive on the LOWERED argument (see
                // `freshen_regions_args`); the written side is an `Option`
                // because an arity error leaves it unpaired, and then the
                // lowered argument stands as it is.
                GenericArg::Ty(ty) => match written.get(index) {
                    Some(GenericArgRef::Type(w)) => {
                        GenericArg::Ty(self.mint_wildcard_regions(w, ty))
                    }
                    _ => arg.clone(),
                },
                GenericArg::Region(_) => match written.get(index) {
                    Some(GenericArgRef::Region(RegionRef::Wildcard)) => {
                        GenericArg::Region(self.fresh_region())
                    }
                    _ => arg.clone(),
                },
                // A const argument carries no region.
                GenericArg::Const(_) => arg.clone(),
            })
            .collect()
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
        // Bound obligations, after every join and axiom has spoken: each
        // resolves into a dictionary entry — a concrete impl's member set,
        // a forward of the enclosing binder's own dictionary, or an error
        // (with the unsatisfied-bound diagnostic naming the bound, the
        // type and the site). Push order per key IS canonical slot order.
        let pending = std::mem::take(&mut self.pending_obligations);
        for obligation in pending {
            let entry = self.resolve_obligation(&obligation);
            match self.result.bound_dicts_of_expr.get_mut(obligation.key) {
                Some(entries) => entries.push(entry),
                None => {
                    self.result
                        .bound_dicts_of_expr
                        .insert(obligation.key, vec![entry]);
                }
            }
        }
        // Instantiations the whole traversal (joins included) never pinned:
        // the mention-site sibling of `NeedsAnnotation` — the definition is
        // fine, this particular use just doesn't say which type it wants.
        let pending = std::mem::take(&mut self.pending_instantiations);
        for instantiation in pending {
            for (param, var, opted_out) in instantiation.params {
                let resolved = resolve_fully(self.table, &var);
                // The DEFAULT BOUND, checked at the one place a binder is
                // spent: every type parameter requires `forget` unless it
                // was written `T without forget`. The flag rode here from
                // the binder itself — a builtin's binder has no item tree
                // to look it back up in.
                if !opted_out
                    && !resolved.contains_infer()
                    && !crate::capability::has_forget(self.db, &resolved)
                {
                    let reason = crate::capability::no_forget_reason(self.db, &resolved)
                        .unwrap_or_else(|| format!("`{}` has no `forget`", resolved.display()));
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::ForgetBoundUnsatisfied {
                            expr: instantiation.expr,
                            item: instantiation.item.clone(),
                            param: param.clone(),
                            ty: resolved.clone(),
                            reason,
                        });
                    continue;
                }
                if !resolved.contains_infer() {
                    continue;
                }
                // A type param whose only information is a bare literal
                // resolves to an unresolved NUMBER variable: an unresolved
                // number is not a type, so the param stays undetermined —
                // but the actionable diagnostic is the literal's own
                // no-defining-use error (annotate the number), reported by
                // the pass below; repeating it as a cannot-infer-`T` here
                // would be noise.
                if is_unresolved_number(self.table, &var) {
                    continue;
                }
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::CannotInferGenericParam {
                        expr: instantiation.expr,
                        item: instantiation.item.clone(),
                        param,
                        owner: instantiation.owner.clone(),
                        spelling: instantiation.spelling.clone(),
                    });
            }
        }
        // Empty array literals whose element type nothing ever pinned —
        // the array sibling of the loop above.
        let pending = std::mem::take(&mut self.pending_empty_arrays);
        for (expr, elem) in pending {
            if resolve_fully(self.table, &elem).contains_infer() {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::EmptyArrayNeedsAnnotation { expr });
            }
        }
        // Integer literals, after every join and axiom has spoken: a pinned
        // literal is range-checked against its resolved type (sign
        // applied); a never-pinned one is the no-defining-use diagnostic —
        // never a silent default. One diagnostic per merged variable (a
        // chain of literals that all share one unpinned number reads best
        // with one squiggle, on the first).
        let pending = std::mem::take(&mut self.pending_number_literals);
        let mut reported_number_roots: Vec<TyVar> = Vec::new();
        for literal in pending {
            match constraint::resolve_shallow(self.table, &literal.var) {
                Ty::Int(kind) => {
                    let fits = i128::try_from(literal.value)
                        .ok()
                        .map(|value| if literal.negated { -value } else { value })
                        .and_then(|value| IntValue::new(kind, value))
                        .is_some();
                    if !fits {
                        let rendered = if literal.negated {
                            format!("-{}", literal.value)
                        } else {
                            literal.value.to_string()
                        };
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::IntLiteralOutOfRange {
                                expr: literal.expr,
                                literal: rendered,
                                ty: Ty::Int(kind),
                            });
                    }
                }
                Ty::Infer(var)
                    if matches!(self.table.probe_value(var), TyVarValue::UnknownNumber) =>
                {
                    let root = self.table.find(var);
                    if !reported_number_roots.contains(&root) {
                        reported_number_roots.push(root);
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::CannotInferNumberType {
                                expr: literal.expr,
                            });
                    }
                }
                // Poisoned (a mismatch already told the story), or some
                // other broken resolution: silent.
                _ => {}
            }
        }
        // Reborrows are final now, so the two deferred judgements can
        // run — before `result` is taken, so their diagnostics are
        // resolved by the loop below like every other one.
        let reborrows = self.constraints.take_reborrows();
        self.finish_deref_reads(&reborrows);
        self.finish_mut_reborrows(&reborrows);
        let mut result = std::mem::take(&mut self.result);
        for (_, ty) in result.type_of_expr.iter_mut() {
            *ty = resolve_finished(self.table, ty);
        }
        for (_, ty) in result.type_of_binding.iter_mut() {
            *ty = resolve_finished(self.table, ty);
        }
        // `VariantTy`s persisted outside any `Ty` carry generic args of
        // their own — resolve them too, both for MIR (payload substitution
        // needs the pinned args) and for value-determinism (an unresolved
        // canonical variable index would break backdating).
        for (_, variant) in result.widened.iter_mut() {
            variant.args = resolve_args_fully(self.table, &variant.args);
        }
        for (_, variant) in result.variant_of_expr.iter_mut() {
            variant.args = resolve_args_fully(self.table, &variant.args);
        }
        for (_, variant) in result.variant_of_pat.iter_mut() {
            variant.args = resolve_args_fully(self.table, &variant.args);
        }
        result.region_count = self.constraints.region_count();
        result.region_constraints = self.constraints.take_region_edges();
        result.reborrows = reborrows;
        for diag in result.diagnostics.iter_mut() {
            match diag {
                InferenceDiagnostic::TypeMismatch {
                    expected, actual, ..
                }
                | InferenceDiagnostic::AllBranchesMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_finished(self.table, expected);
                    *actual = resolve_finished(self.table, actual);
                }
                InferenceDiagnostic::NotCallable { ty, .. } => {
                    *ty = resolve_finished(self.table, ty);
                }
                // `found` is always a definite `char` — resolved for
                // symmetry, so the pair can never drift apart.
                InferenceDiagnostic::PatLiteralTypeMismatch {
                    expected, found, ..
                } => {
                    *expected = resolve_finished(self.table, expected);
                    *found = resolve_finished(self.table, found);
                }
                InferenceDiagnostic::IfBranchMismatch {
                    then_ty, else_ty, ..
                } => {
                    *then_ty = resolve_finished(self.table, then_ty);
                    *else_ty = resolve_finished(self.table, else_ty);
                }
                InferenceDiagnostic::RecordLitMissingFields { fields, .. } => {
                    for (_, ty) in fields.iter_mut() {
                        *ty = resolve_finished(self.table, ty);
                    }
                }
                InferenceDiagnostic::RecordLitExtraField { expected, .. } => {
                    *expected = resolve_finished(self.table, expected);
                }
                InferenceDiagnostic::ElidedVariantNoEnum {
                    expected: Some(expected),
                    ..
                } => {
                    *expected = resolve_finished(self.table, expected);
                }
                InferenceDiagnostic::NoSuchField { receiver_ty, .. }
                | InferenceDiagnostic::NoSuchMember { receiver_ty, .. }
                | InferenceDiagnostic::MemberWantsExclusiveReceiver { receiver_ty, .. }
                | InferenceDiagnostic::MemberCallAmbiguity { receiver_ty, .. } => {
                    *receiver_ty = resolve_finished(self.table, receiver_ty);
                }
                InferenceDiagnostic::FieldNotCallable {
                    ty, receiver_ty, ..
                } => {
                    *ty = resolve_finished(self.table, ty);
                    *receiver_ty = resolve_finished(self.table, receiver_ty);
                }
                InferenceDiagnostic::MatchWithoutCatchAll { scrutinee, .. }
                | InferenceDiagnostic::NonEnumScrutineeVariantPat { scrutinee, .. }
                | InferenceDiagnostic::PatWrongEnum { scrutinee, .. }
                | InferenceDiagnostic::UnreachableArm {
                    reason: UnreachableReason::OtherVariant { scrutinee },
                    ..
                } => {
                    *scrutinee = resolve_finished(self.table, scrutinee);
                }
                InferenceDiagnostic::PatUnknownField { record_ty, .. } => {
                    *record_ty = resolve_finished(self.table, record_ty);
                }
                InferenceDiagnostic::PatMissingFields { fields, .. } => {
                    for (_, ty) in fields.iter_mut() {
                        *ty = resolve_finished(self.table, ty);
                    }
                }
                InferenceDiagnostic::PatNotRecord { ty, .. }
                | InferenceDiagnostic::DerefNonPointer { ty, .. }
                | InferenceDiagnostic::IndexNonArray { ty, .. }
                | InferenceDiagnostic::AssignThroughShared { ty, .. }
                | InferenceDiagnostic::DotThroughBorrow {
                    receiver_ty: ty, ..
                }
                | InferenceDiagnostic::BorrowMutThroughShared { ty, .. }
                | InferenceDiagnostic::BorrowThroughRawPointer { ty, .. }
                | InferenceDiagnostic::MoveOutOfBorrow { ty, .. }
                | InferenceDiagnostic::AddrOfMutThroughShared { ty, .. } => {
                    *ty = resolve_finished(self.table, ty);
                }
                InferenceDiagnostic::PatNamedTypeMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_finished(self.table, expected);
                    *actual = resolve_finished(self.table, actual);
                }
                InferenceDiagnostic::BuiltinExpectsRawPtr { found, .. } => {
                    *found = resolve_finished(self.table, found);
                }
                InferenceDiagnostic::UnsatisfiedBound { ty, .. }
                | InferenceDiagnostic::NoTraitImpl { ty, .. } => {
                    *ty = resolve_finished(self.table, ty);
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
                | InferenceDiagnostic::ElidedVariantNoEnum { expected: None, .. }
                | InferenceDiagnostic::NoVariantsOnStruct { .. }
                | InferenceDiagnostic::QualifiedPathIsField { .. }
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
                | InferenceDiagnostic::ReturnOutsideFn { .. }
                | InferenceDiagnostic::ReturnInConstBlock { .. }
                | InferenceDiagnostic::PatBindingNeedsAnnotation { .. }
                | InferenceDiagnostic::PatUnknownType { .. }
                | InferenceDiagnostic::GenericArgCount { .. }
                | InferenceDiagnostic::NotGeneric { .. }
                | InferenceDiagnostic::ConstArgHole { .. }
                | InferenceDiagnostic::UnexpectedRegionArg { .. }
                | InferenceDiagnostic::RegionArgAtMention { .. }
                | InferenceDiagnostic::GenericArgKindMismatch { .. }
                | InferenceDiagnostic::MissingConstArgs { .. }
                | InferenceDiagnostic::CannotInferGenericParam { .. }
                | InferenceDiagnostic::ForgetBoundUnsatisfied { .. }
                | InferenceDiagnostic::AssignToConstParam { .. }
                | InferenceDiagnostic::FnConstArg { .. }
                | InferenceDiagnostic::TypeConstArgUnsupported { .. }
                | InferenceDiagnostic::AddrOfNonPlace { .. }
                | InferenceDiagnostic::AddrOfMutImmutable { .. }
                | InferenceDiagnostic::AddrOfMutItem { .. }
                | InferenceDiagnostic::BorrowNonPlace { .. }
                | InferenceDiagnostic::BorrowMutImmutable { .. }
                | InferenceDiagnostic::BorrowMutItem { .. }
                | InferenceDiagnostic::IndexOutOfBounds { .. }
                | InferenceDiagnostic::EmptyArrayNeedsAnnotation { .. }
                | InferenceDiagnostic::ArrayConstArg { .. }
                | InferenceDiagnostic::CannotInferNumberType { .. }
                | InferenceDiagnostic::IntLiteralOutOfRange { .. }
                | InferenceDiagnostic::BuiltinNotFirstClass { .. }
                | InferenceDiagnostic::NotDotCallable { .. }
                | InferenceDiagnostic::MemberWantsBorrowReceiver { .. }
                | InferenceDiagnostic::MemberNotCalled { .. }
                | InferenceDiagnostic::NamedGenericArg { .. }
                | InferenceDiagnostic::QualifiedTraitMemberOnType { .. }
                | InferenceDiagnostic::TraitNotValue { .. }
                | InferenceDiagnostic::AssocTypeReserved { .. }
                | InferenceDiagnostic::CannotInferSelf { .. }
                | InferenceDiagnostic::QualifiedTraitMemberValue { .. }
                | InferenceDiagnostic::BoundMemberValue { .. }
                | InferenceDiagnostic::TraitHasNoMember { .. }
                | InferenceDiagnostic::BoundFnValue { .. }
                | InferenceDiagnostic::GenericTraitReserved { .. }
                | InferenceDiagnostic::MemberOwnConstArgs { .. }
                | InferenceDiagnostic::MemberGenericArgCount { .. }
                | InferenceDiagnostic::VariantOwnGenericArgs { .. }
                | InferenceDiagnostic::NestedBoundUse { .. } => {}
            }
        }
        for (_, ty) in result.type_of_pat.iter_mut() {
            *ty = resolve_finished(self.table, ty);
        }
        // Expectations: resolve like `type_of_expr`, but *drop* what
        // doesn't resolve to a concrete type — an unbound variable means
        // the position had no real expectation, and an `{error}` means the
        // expectation itself came from broken code (see the field's doc
        // for why both must go).
        let recorded = std::mem::take(&mut result.expectation_of_expr);
        for (expr, ty) in recorded.iter() {
            let resolved = resolve_finished(self.table, ty);
            if resolved.contains_infer() || resolved.contains_error() {
                continue;
            }
            result.expectation_of_expr.insert(expr, resolved);
        }
        result
    }

    /// Resolve one bound obligation after solving — see [`Self::finish`].
    fn resolve_obligation(&mut self, obligation: &PendingObligation) -> DictEntry {
        let resolved = resolve_fully(self.table, &obligation.var);
        // Broken or undetermined: covered by their own diagnostics (a
        // mismatch, cannot-infer, or the literal's no-defining-use).
        if resolved.contains_error() || is_unresolved_number(self.table, &obligation.var) {
            return DictEntry::Error;
        }
        if resolved.contains_infer() {
            return DictEntry::Error;
        }
        // A rigid param: satisfied exactly when the enclosing binder gives
        // it the same bound — the forwarding edge.
        if let Ty::Param(param) = &resolved {
            let own = self.own_item.as_ref().is_some_and(|own| param.item == *own);
            if own
                && let Some(data) = self.own_generics.get(param.index as usize)
                && data.bounds.iter().any(|bound| {
                    crate::traits::bound_trait(self.db, self.file, bound).as_ref()
                        == Some(&obligation.trait_)
                })
            {
                if obligation.nested {
                    // Forwarding needs the root body's dictionary
                    // parameters, out of reach from a nested body —
                    // reserved (the captured-dictionary wall).
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NestedBoundUse {
                            expr: obligation.key,
                        });
                    return DictEntry::Error;
                }
                return DictEntry::Forward {
                    param_index: param.index,
                    trait_: obligation.trait_.clone(),
                };
            }
            self.result
                .diagnostics
                .push(InferenceDiagnostic::UnsatisfiedBound {
                    expr: obligation.key,
                    param: obligation.param_name.clone(),
                    trait_: obligation.trait_.clone(),
                    ty: resolved,
                });
            return DictEntry::Error;
        }
        let Some(self_key) = crate::traits::SelfKey::for_ty(&resolved) else {
            // Structural types implement nothing (TR03).
            self.result
                .diagnostics
                .push(InferenceDiagnostic::UnsatisfiedBound {
                    expr: obligation.key,
                    param: obligation.param_name.clone(),
                    trait_: obligation.trait_.clone(),
                    ty: resolved,
                });
            return DictEntry::Error;
        };
        let impls = crate::traits::trait_impls(self.db, self.file);
        let Some(site) = impls.impl_for(&obligation.trait_, &self_key) else {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::UnsatisfiedBound {
                    expr: obligation.key,
                    param: obligation.param_name.clone(),
                    trait_: obligation.trait_.clone(),
                    ty: resolved,
                });
            return DictEntry::Error;
        };
        match crate::traits::impl_dict_members(self.db, &obligation.trait_, site) {
            Some(members) => DictEntry::Impl(members),
            // A broken impl (missing members): the definition site carries
            // the diagnostic; the call traps.
            None => DictEntry::Error,
        }
    }

    /// Whether inference currently sits inside a body that lowers
    /// SEPARATELY from the item's root fn body (a nested fn literal, a
    /// `const` block) — where the root's dictionary parameters are out of
    /// reach.
    fn in_nested_body(&self) -> bool {
        self.scope_depth > 1 || self.const_block_depth > 0
    }

    /// Push the obligations of one bounded instantiation, keyed by
    /// `key` (the mention or the call), in canonical slot order. `var_of`
    /// maps a binder index to the param's instantiation.
    fn push_bound_obligations(
        &mut self,
        key: ExprId,
        generics: &[GenericParamData],
        var_of: &FxHashMap<u32, Ty>,
    ) {
        let nested = self.in_nested_body();
        for slot in crate::traits::bound_slots(self.db, self.file, generics) {
            let Some(var) = var_of.get(&slot.param_index) else {
                continue;
            };
            let param_name = generics
                .get(slot.param_index as usize)
                .map(|param| param.name.clone())
                .unwrap_or_default();
            self.pending_obligations.push(PendingObligation {
                key,
                var: var.clone(),
                trait_: slot.trait_,
                param_name,
                nested,
            });
        }
    }

    fn fresh_var(&mut self) -> Ty {
        Ty::Infer(self.table.new_key(TyVarValue::Unknown))
    }

    /// A NUMBER-CLASS inference variable: unifies only with integer scalar
    /// types and other number variables — what an integer literal (and an
    /// arithmetic operand position) starts as.
    fn fresh_number_var(&mut self) -> Ty {
        Ty::Infer(self.table.new_key(TyVarValue::UnknownNumber))
    }

    /// Infer the arguments of an already-broken call freely, then silence
    /// any still-free NUMBER variables among them: the call's own
    /// diagnostic is the whole story — a literal argument must not pile a
    /// no-defining-use error on top (errors are infectious and silent).
    fn infer_args_broken(&mut self, args: &[ExprId]) {
        for &arg in args {
            let fresh = self.fresh_var();
            self.infer_expr(arg, &fresh);
            poison_unresolved_number(self.table, &fresh);
        }
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
        // Persist the expectation before anything else so every arm —
        // including the early-`return`ing ones (`if` without `else`, block
        // tails, `const` blocks, record literals, `match`, constructions) —
        // records it; `finish` resolves and filters. Recording only:
        // nothing below reads this map.
        self.result
            .expectation_of_expr
            .insert(expr, expected.clone());
        // A join sink propagates only through transparent wrappers (block
        // tails, `const` blocks) into an `if`'s witness position. Any other
        // expression is a leaf of the enclosing join, and its
        // subexpressions are ordinary non-witness positions — an `if`
        // inside a call argument or a statement resolves as its own join.
        let sink = self.witness_sink.take();
        let ty = match &self.body.exprs[expr] {
            ExprData::Missing => Ty::Error,
            // A HOST IMPORT *is* its declared type. There is no value here
            // to infer from and nothing to check an annotation against, so
            // the item's own contract is the answer, whole.
            //
            // What a BROKEN declaration types as is decided once, in
            // `item_tree::import_contract`: a data import (reserved) and a
            // hole in the contract both arrive here already erased, so the
            // root and every mention — which read `ty::signature` off that
            // same contract — cannot disagree about them.
            //
            // The `Ty::Fn` guard is the local half of the same rule, for
            // the one shape the contract rule cannot erase: an import with
            // no annotation at all arrives as a free variable, and NO CALL
            // SITE MAY DECIDE an import's type. Group inference erases that
            // leftover too, so this is a belt, not the only strap.
            ExprData::ExternImport => match self.resolve_shallow(expected) {
                ty @ Ty::Fn(_) => ty,
                _ => Ty::Error,
            },
            // ENTIRELY INFERRED, never defaulted: the literal is a
            // NUMBER-CLASS variable until a defining use pins it; `finish`
            // range-checks the pinned ones and reports the never-pinned
            // ones (no silent pick).
            ExprData::Literal(LiteralData::Int(value)) => {
                let var = self.fresh_number_var();
                if let Some(value) = value {
                    self.pending_number_literals.push(PendingNumberLiteral {
                        expr,
                        value: *value,
                        negated: self.negated_literals.contains(&expr),
                        var: var.clone(),
                    });
                }
                var
            }
            ExprData::Literal(LiteralData::Str(_)) => Ty::Str,
            ExprData::Literal(LiteralData::Bool(_)) => Ty::Bool,
            // Definite, unlike an integer literal: there is one character
            // type, so `'x'` has no defining-use question to leave open and
            // never mints a variable.
            ExprData::Literal(LiteralData::Char(_)) => Ty::Char,
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
                Some(Resolution::TraitItem(_)) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::TraitNotValue {
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
                // A const param of the enclosing binder reads as a value of
                // its declared type. That type is lowered WITHOUT the param
                // scope: a dependent `const N: T` is rejected (TR06) — the
                // diagnostics pass reports it and `T` lowers to a silent
                // `{error}` here.
                Some(Resolution::ConstParam(index)) => self.const_param_value_ty(*index),
                Some(Resolution::Item(loc)) => {
                    // A member of this item's own binding group resolves to
                    // its shared signature variable — that's interprocedural
                    // inference happening.
                    if let Some(member_sig) = self.in_group.get(loc) {
                        member_sig.clone()
                    } else {
                        let target = loc.to_id(self.db);
                        // A generic item has no ONE type: every mention
                        // instantiates the scheme afresh (let-polymorphism
                        // at the item boundary, TR06). A bare mention gets a
                        // fresh variable per type param — and is an error
                        // when the binder declares const params, which are
                        // never inferred (TR06).
                        let generics = item_generics(self.db, target);
                        if !generics.is_empty() {
                            let sig = signature(self.db, target);
                            self.instantiate_mention(expr, loc.clone(), sig, generics, None)
                        } else {
                            let sig = signature(self.db, target);
                            // Inference couldn't determine the signature
                            // from the definition: that's only visible from
                            // uses (an unused undetermined item is fine),
                            // so the diagnostic lives here.
                            if signature_needs_annotation(self.db, target) {
                                self.result.diagnostics.push(
                                    InferenceDiagnostic::NeedsAnnotation {
                                        expr,
                                        item: loc.clone(),
                                    },
                                );
                            }
                            sig
                        }
                    }
                }
                // No one signature a use could take on; the duplicate
                // definitions carry the diagnostic.
                Some(Resolution::Ambiguous(_)) => Ty::Error,
                Some(Resolution::Builtin(builtin)) => {
                    let builtin = *builtin;
                    self.infer_builtin_mention(expr, builtin, None)
                }
                None => Ty::Error, // unresolved: already diagnosed by name resolution
            },
            ExprData::VariantPath {
                base,
                variant,
                args,
                member_args,
            } => {
                // The value-form entry point's free-at-exit wrapper — the
                // twin of the dot-call's and the trait-call's. Two arms
                // below SPEND the member turbofish (an inherent member's
                // value, a named-`Self` trait member's); every other arm
                // refuses the path for its own reason, and its leftover is
                // consumed here, silently and exactly once.
                let (base, variant) = (*base, variant.clone());
                let (args, member_args) = (args.clone(), member_args.clone());
                let mut unspent = member_args.as_deref();
                let ty =
                    self.infer_variant_path(expr, base, &variant, args.as_deref(), &mut unspent);
                self.drop_member_args(&mut unspent);
                ty
            }
            // `::None` — the elided sigil in expression position. Resolved
            // against the position's EXPECTED type, which is why it is
            // read here and nowhere else.
            ExprData::ElidedVariant { variant } => {
                let variant = variant.clone();
                self.infer_elided_variant(expr, &variant, expected)
            }
            ExprData::GenericApp { base, args } => self.infer_generic_app(expr, *base, args),
            ExprData::Call {
                callee,
                args,
                dot_call,
            } => {
                // A construction call: the type name used as a plain
                // constructor function taking the underlying record —
                // `Foo(struct { x = 1 })`, or `Pair::<usize>(...)` with the
                // type's generic arguments spelled. Intercepted before the
                // callee is inferred (a bare type name in expression
                // position is an error; as a construction head it is the
                // one legal use).
                let ctor = match &self.body.exprs[*callee] {
                    ExprData::GenericApp { base, args } => {
                        match self.resolutions.get(*base).cloned() {
                            Some(Resolution::TypeItem(loc)) => Some((loc, Some(args.clone()))),
                            _ => None,
                        }
                    }
                    _ => match self.resolutions.get(*callee).cloned() {
                        Some(Resolution::TypeItem(loc)) => Some((loc, None)),
                        _ => None,
                    },
                };
                if let Some((loc, generic_args)) = ctor {
                    return self.infer_construction(
                        expr,
                        *callee,
                        &loc,
                        generic_args.as_deref(),
                        args,
                        expected,
                        cause,
                    );
                }
                // The flavor-polymorphic builtins (see
                // [`Builtin::flavor_polymorphic`]) are intercepted like
                // construction heads: their callee has no one function type
                // to infer, so the call itself is the special case (see
                // `infer_builtin_special_call`).
                if let Some(Resolution::Builtin(builtin)) = self.resolutions.get(*callee)
                    && builtin.flavor_polymorphic()
                {
                    let builtin = *builtin;
                    return {
                        let ty = self.infer_builtin_special_call(expr, builtin, args);
                        let ty = self.check(expr, ty, expected, cause);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        ty
                    };
                }
                // TR01 short form: `Trait::member(args...)` — intercepted
                // before the callee is inferred (a bare `Trait::member`
                // VALUE is the reserved named-Self territory; the direct
                // call is the live spelling).
                if let ExprData::VariantPath {
                    base,
                    variant,
                    args: vp_args,
                    member_args,
                } = &self.body.exprs[*callee]
                    && let Some(Resolution::TraitItem(trait_loc)) = self.resolutions.get(*base)
                {
                    let (callee, trait_loc, variant, vp_args, member_args) = (
                        *callee,
                        trait_loc.clone(),
                        variant.clone(),
                        vp_args.clone(),
                        member_args.clone(),
                    );
                    // One free-at-exit wrapper per entry point that can
                    // carry a member turbofish: the three spend sites TAKE
                    // it, and whatever survives is consumed silently here
                    // rather than at each of this call's many refusals.
                    let mut unspent = member_args.as_deref();
                    let ty = self.infer_qualified_trait_call(
                        expr,
                        callee,
                        trait_loc,
                        &variant,
                        vp_args.as_deref(),
                        &mut unspent,
                        args,
                        expected,
                        cause,
                    );
                    self.drop_member_args(&mut unspent);
                    return ty;
                }
                // TR01 dot-call: `recv.name(a, b)` — but only the WRITTEN
                // dot-call shape; `(recv.name)(...)` is an ordinary value
                // call of the field (the parens are the field-selection
                // spelling). Resolution is STRUCTURAL and syntax-directed
                // (G13): under call syntax the MEMBER resolves
                // first — a dot-callable member shadows a same-named field
                // — then the field; the call desugars to the member with
                // recv as the LAST argument (arguments evaluate BEFORE the
                // receiver binds).
                if *dot_call
                    && let ExprData::Field {
                        receiver,
                        name,
                        member_args,
                    } = &self.body.exprs[*callee]
                {
                    let (callee, receiver, name) = (*callee, *receiver, name.clone());
                    let member_args = member_args.clone();
                    return self.infer_dot_call(
                        expr,
                        callee,
                        receiver,
                        &name,
                        member_args.as_deref(),
                        args,
                        expected,
                        cause,
                    );
                }
                // `::Some(v)` — the elided sigil with payloads. The
                // CALLEE is what carries the sigil, but the enum is in the
                // CALL's expectation, not the callee's (a callee's own
                // expectation is a fn type), so the callee is resolved by
                // hand against `expected` and the call then proceeds
                // through the ordinary constructor-call machinery — the
                // identical path `Option::Some(v)` takes.
                if let ExprData::ElidedVariant { variant } = &self.body.exprs[*callee] {
                    let (callee, variant) = (*callee, variant.clone());
                    let callee_expectation = self.fresh_var();
                    self.result
                        .expectation_of_expr
                        .insert(callee, callee_expectation);
                    let callee_ty = self.infer_elided_variant(callee, &variant, expected);
                    self.result.type_of_expr.insert(callee, callee_ty.clone());
                    return {
                        let ty = self.call_of_value(expr, callee, args, callee_ty);
                        let ty = self.check(expr, ty, expected, cause);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        ty
                    };
                }
                let fresh = self.fresh_var();
                let callee_ty = self.infer_expr(*callee, &fresh);
                self.call_of_value(expr, *callee, args, callee_ty)
            }
            ExprData::Bin { op, lhs, rhs } => {
                use crate::body::BinOp::*;
                match op {
                    // Arithmetic works on any ONE integer type: both
                    // operands meet a shared NUMBER-CLASS variable — an
                    // already-typed operand pins it (a defining use for a
                    // literal on the other side), two literals stay an
                    // unpinned number, and mixed types (`u8 + u32`) are an
                    // ordinary mismatch (no implicit conversions).
                    Some(Add | Sub | Mul | Div) => {
                        let operand = self.fresh_number_var();
                        self.infer_expr_with(*lhs, &operand, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &operand, Some(Cause::Operator(expr)));
                        operand
                    }
                    // Comparisons: same-type integer operands, `bool` out.
                    Some(Lt | Le | Gt | Ge) => {
                        let operand = self.fresh_number_var();
                        self.infer_expr_with(*lhs, &operand, Some(Cause::Operator(expr)));
                        self.infer_expr_with(*rhs, &operand, Some(Cause::Operator(expr)));
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
            // `-x`: number-classed like the binary operators — the operand
            // pins the shared variable (or a defining use of the whole
            // negation does). A literal directly underneath participates in
            // range checking with the sign applied.
            ExprData::Neg { operand } => {
                let operand = *operand;
                if matches!(
                    self.body.exprs[operand],
                    ExprData::Literal(LiteralData::Int(Some(_)))
                ) {
                    self.negated_literals.insert(operand);
                }
                let num = self.fresh_number_var();
                self.infer_expr_with(operand, &num, Some(Cause::Operator(expr)));
                num
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
                            self.adopt(&result, &Ty::Never);
                            Ty::Never
                        }
                        // One surviving leaf (the rest diverged): its type
                        // is the `if`'s type outright, no agreement left to
                        // defer. Unified into `result` so nested `if`s that
                        // routed the leaf here still resolve.
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.adopt(&result, &ty);
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
                // Whether some *statement* of this block DIVERGED — a
                // `return;`/`break;`/`continue;`, or a call that never comes
                // back. A tail-less block normally produces `()`, but a
                // block execution never runs off the end of produces nothing
                // at all: it is `!`, and the never machinery takes it from
                // there. That is what makes the SEMICOLON spellings work —
                // `else { return 0; }`, `else { break; }` — which without it
                // would type `()` purely because of the trailing `;` and
                // mismatch the branch they belong to. (Deliberately only
                // statement position: `let x = return 1;` diverges too, but
                // typing the block off a binding's initializer is a step
                // towards the reachability analysis this isn't.)
                let mut diverged = false;
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
                                    .map(|it| self.lower_type_ref(it))
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
                                    // Argument-preserving, like every
                                    // widening edge.
                                    ty = Ty::Named(NamedTy {
                                        decl: variant.decl,
                                        args: variant.args,
                                    });
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
                                    Some(tr) => self.lower_type_ref(tr),
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
                            // with `Cause::Binding`. For a field-chain
                            // target this read-typing already yields the
                            // *field's* type (and reports unknown fields /
                            // non-record receivers on the way).
                            let fresh = self.fresh_var();
                            let target_ty = self.infer_expr(*target, &fresh);
                            // Place-chain targets — field/element chains
                            // and writes through pointers (`p.* = v;`,
                            // `p.*.x = v;`): one walk judges them all. A
                            // deref makes a new root: the judgement there
                            // is the pointer's `.&raw mut`-ness, not any
                            // binding's `mut`-ness (`p` itself need not be
                            // `mut` — writing through it does not reassign
                            // it).
                            if let ExprData::Field { .. }
                            | ExprData::Index { .. }
                            | ExprData::Deref { .. } = &self.body.exprs[*target]
                            {
                                let cause = self.check_field_assign_target(*target);
                                self.infer_expr_with(*value, &target_ty, cause);
                                continue;
                            }
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
                                                place: data.name.clone(),
                                            },
                                        );
                                    }
                                    Some(Cause::Binding(*binding))
                                }
                                // A const param is a compile-time value,
                                // not a place.
                                Some(Resolution::ConstParam(index)) => {
                                    let name = self
                                        .own_generics
                                        .get(*index as usize)
                                        .map(|param| param.name.clone())
                                        .unwrap_or_default();
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::AssignToConstParam {
                                            target: *target,
                                            name,
                                        },
                                    );
                                    None
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
                                // Same for a trait name.
                                Some(Resolution::TypeItem(_) | Resolution::TraitItem(_)) => None,
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
                            let ty = self.infer_expr(*e, &fresh_var);
                            if matches!(self.resolve_shallow(&ty), Ty::Never) {
                                diverged = true;
                            }
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
                    None if diverged => Ty::Never,
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
                // inside it cannot exit a loop outside it — and, for the
                // same separate-body reason, it cannot reach the enclosing
                // bounds' dictionary (`const_block_depth`).
                self.witness_sink = sink;
                let saved_loops = std::mem::take(&mut self.loop_sinks);
                // A body of its own, so it shadows the enclosing `fn`'s
                // return target — but as a RESERVED one: a `return` that
                // lands here is refused, never checked against anything
                // (see `ReturnTarget::ConstBlock`).
                self.return_targets.push(ReturnTarget::ConstBlock);
                self.const_block_depth += 1;
                let ty = self.infer_expr_with(*inner, expected, cause);
                self.const_block_depth -= 1;
                self.return_targets.pop();
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
                // A field's own `: Type` ascription is an extra local
                // axiom: the value checks against it first, and IT must
                // then agree with the expected field type.
                if let Ty::Record(expected_rec) = self.resolve_shallow(expected) {
                    let has = |name: &str| fields.iter().any(|f| f.name == name);
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
                    let mut seen: Vec<String> = Vec::new();
                    for field in fields {
                        let ascription = field
                            .type_ref
                            .clone()
                            .map(|type_ref| self.lower_type_ref(&type_ref));
                        match expected_rec.field_ty(&field.name) {
                            Some(field_ty) => match &ascription {
                                Some(ascribed) => {
                                    self.infer_expr_with(field.value, ascribed, None);
                                    self.check(field.value, ascribed.clone(), field_ty, cause);
                                }
                                None => {
                                    self.infer_expr_with(field.value, &field_ty.clone(), cause);
                                }
                            },
                            None => {
                                // An extra field (exact field-set equality:
                                // nothing is dropped silently). Duplicates of
                                // one extra name get a single diagnostic —
                                // validation already flags the duplication.
                                if !seen.contains(&field.name) {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::RecordLitExtraField {
                                            expr: field.value,
                                            name: field.name.clone(),
                                            expected: Ty::Record(expected_rec.clone()),
                                        },
                                    );
                                }
                                let expected_field = ascription.unwrap_or_else(|| self.fresh_var());
                                self.infer_expr(field.value, &expected_field);
                            }
                        }
                        seen.push(field.name.clone());
                    }
                    // Field mismatches were reported above (or the sets
                    // match); either way the literal recovers with the
                    // expected type so nothing cascades.
                    let ty = Ty::Record(expected_rec);
                    self.result.type_of_expr.insert(expr, ty.clone());
                    return ty;
                }
                // No record expectation: infer every field — against its
                // own ascription when it wrote one — and conclude a record
                // type, then let the ordinary check judge it (binding a
                // free variable, or reporting a plain mismatch against a
                // non-record expectation).
                let field_tys = fields
                    .iter()
                    .map(|field| {
                        let expected_field = match &field.type_ref {
                            Some(type_ref) => {
                                let type_ref = type_ref.clone();
                                self.lower_type_ref(&type_ref)
                            }
                            None => self.fresh_var(),
                        };
                        (
                            field.name.clone(),
                            self.infer_expr(field.value, &expected_field),
                        )
                    })
                    .collect();
                Ty::record(field_tys)
            }
            ExprData::Field {
                receiver,
                name,
                member_args,
            } => {
                let receiver = *receiver;
                let name = name.clone();
                let mut member_args = member_args.as_deref();
                let fresh = self.fresh_var();
                let receiver_ty = self.infer_expr(receiver, &fresh);
                if name.is_empty() {
                    // `a.` — the parse error covers it.
                    self.drop_member_args(&mut member_args);
                    Ty::Error
                } else {
                    let resolved = self.resolve_shallow(&receiver_ty);
                    let ty = self.field_access_ty(expr, receiver, &name, resolved);
                    // `h.go::<usize>` NOT under call syntax: the dot selects
                    // a FIELD, and a field has no binder — the third place
                    // the refusal is true, and only when a field was
                    // actually found. A dot that landed on a member
                    // (`MemberNotCalled`), on nothing, or on a half-typed
                    // `o.map::<` already carries its own diagnostic, and
                    // that one is the whole story.
                    if self.resolve_shallow(&ty).contains_error() {
                        self.drop_member_args(&mut member_args);
                    } else {
                        self.refuse_member_args(expr, expr, &name, &mut member_args);
                    }
                    self.carry_borrow_projection(expr, receiver, &ty);
                    ty
                }
            }
            ExprData::ArrayLit { elements } => {
                let elements = elements.clone();
                // Bidirectional: an array expectation flows its element
                // type into every element (with the same cause), exactly
                // like record literals — a wrong element blames the
                // element and cites the annotation/call that demanded it.
                if let Ty::Array { elem, .. } = self.resolve_shallow(expected) {
                    let elem = (*elem).clone();
                    for &element in &elements {
                        self.infer_expr_with(element, &elem, cause);
                    }
                    // The literal's length is its element count; a length
                    // disagreement with the expectation is an ordinary
                    // mismatch on the whole literal (the final `check`).
                    Ty::array(elem, ConstArgValue::Int(elements.len() as u128))
                } else {
                    // No array expectation: the elements are witnesses of
                    // ONE join — the same family-aware machinery `if`
                    // branches and match arms use, so identical elements
                    // keep their type, mixed variants of one enum LUB to
                    // the enum (with the conversion planted per element),
                    // and blame speaks about elements.
                    let result = self.fresh_var();
                    self.join_sinks.push(JoinSink {
                        result: result.clone(),
                        witnesses: Vec::new(),
                    });
                    let sink_index = self.join_sinks.len() - 1;
                    for &element in &elements {
                        let fresh = self.fresh_var();
                        self.witness_sink = Some(sink_index);
                        let element_ty = self.infer_expr(element, &fresh);
                        self.contribute_witness(sink_index, element, &element_ty);
                    }
                    let JoinSink { result, witnesses } =
                        self.join_sinks.pop().expect("sink pushed above");
                    match witnesses.len() {
                        // `[]` (or every element diverges): the element
                        // type is the context's to decide; a genuinely
                        // unpinned `[]` is reported in `finish`.
                        0 => {
                            if elements.is_empty() {
                                self.pending_empty_arrays.push((expr, result.clone()));
                            }
                        }
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.adopt(&result, &ty);
                        }
                        _ => {
                            // Deferral exists so axioms arriving later can
                            // pick the winner before witnesses are played
                            // against each other — when the elements
                            // already AGREE (they all unify, unpinned
                            // number literals included) there is nothing
                            // left to decide, and resolving eagerly keeps
                            // the element type usable *during* traversal
                            // (indexing is the array's primary operation:
                            // `m[1][0]` / `pts[1].x` on a nested literal
                            // must project right away, which a deferred
                            // join can't offer). Tried under a snapshot so
                            // a disagreement leaves no trace and defers to
                            // the blame-aware join solver.
                            //
                            // A BORROW-typed element never takes this
                            // path. `unify` is region-blind by design, so
                            // two borrows differing only in region always
                            // "agree" — the eager route would commit and
                            // the join solver, which is the only thing
                            // that reborrows a leaf into its context,
                            // would never run. That is not a missing
                            // relate to add here: relating against the
                            // FIRST element is the shape that was wrong
                            // for `if`/`else`, and there is one correct
                            // implementation of a join. Elements go to it.
                            //
                            // Cost: an unannotated array of borrows loses
                            // eager projection, so `arr[0].*` needs the
                            // annotation. That is the same pre-existing
                            // deferred-join limitation as `getx`-style
                            // record projection, not a new one.
                            let snapshot = self.constraints.snapshot(self.table);
                            let agree = !witnesses
                                .iter()
                                .any(|witness| self.resolve_shallow(&witness.ty).contains_borrow())
                                && witnesses.iter().all(|witness| {
                                    self.constraints
                                        .adopt(self.table, &result, &witness.ty, None)
                                });
                            if agree {
                                self.constraints.commit(self.table, snapshot);
                            } else {
                                self.constraints.rollback_to(self.table, snapshot);
                                self.constraints.push_join(Join {
                                    expr,
                                    depth: self.scope_depth,
                                    result: result.clone(),
                                    witnesses,
                                });
                            }
                        }
                    }
                    Ty::array(result, ConstArgValue::Int(elements.len() as u128))
                }
            }
            ExprData::ArrayRepeat { element, count } => {
                let element = *element;
                let count = *count;
                let elem_expected = match self.resolve_shallow(expected) {
                    Ty::Array { elem, .. } => (*elem).clone(),
                    _ => self.fresh_var(),
                };
                let elem_ty = self.infer_expr_with(element, &elem_expected, cause);
                // The count is a `usize` (a defining use for a literal)...
                self.infer_expr_with(count, &Ty::Int(IntKind::Usize), None);
                // ...and it parameterizes the array's TYPE, so it is
                // restricted to the annotation-representable const domain
                // (a literal or a const-param read) — a computed count
                // (any expression, `const { ... }` blocks included) gets
                // the same diagnostic a type's const argument does.
                let len = match self.try_type_const_arg_value(count) {
                    Ok(value) => value,
                    Err(is_block) => {
                        self.result.diagnostics.push(
                            InferenceDiagnostic::TypeConstArgUnsupported {
                                expr: count,
                                is_block,
                            },
                        );
                        ConstArgValue::Error
                    }
                };
                Ty::array(elem_ty, len)
            }
            ExprData::Index { base, index } => {
                let base = *base;
                let index = *index;
                let base_fresh = self.fresh_var();
                let base_ty = self.infer_expr(base, &base_fresh);
                // Indexing pins `usize` — a defining use.
                self.infer_expr_with(index, &Ty::Int(IntKind::Usize), None);
                let element = match self.resolve_shallow(&base_ty) {
                    Ty::Array { elem, len } => {
                        // Both the length and the index compile-time known
                        // and out of bounds: squiggle here, trap at this
                        // expression — with exactly the text the runtime
                        // bounds check uses.
                        if let ConstArgValue::Int(len_value) = len
                            && let ExprData::Literal(LiteralData::Int(Some(index_value))) =
                                &self.body.exprs[index]
                            && *index_value >= len_value
                        {
                            let index_value = *index_value;
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::IndexOutOfBounds {
                                    expr,
                                    len: len_value,
                                    index: index_value,
                                });
                        }
                        (*elem).clone()
                    }
                    // Evaluating the base already diverges.
                    Ty::Never => Ty::Never,
                    // An undetermined base: like a field access, the
                    // element type can't be run backwards — ask for an
                    // annotation.
                    Ty::Infer(_) => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::FieldOnUnknownType {
                                expr,
                                receiver: base,
                            });
                        Ty::Error
                    }
                    // Errors are infectious and silent.
                    broken if broken.contains_error() => Ty::Error,
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::IndexNonArray { expr, ty: other });
                        Ty::Error
                    }
                };
                self.carry_borrow_projection(expr, base, &element);
                element
            }
            // `place.&raw` / `place.&raw mut`: the operand reads like any
            // expression (so field chains get their diagnostics on the
            // way), then the place rules are judged on its structure.
            // `place.&` / `place.&mut` — a SAFE borrow. Same two-phase shape
            // as `.&raw`: the operand reads like any expression (so field
            // chains report their own problems), then the place rules are
            // judged on its structure.
            ExprData::Borrow { mutable, place } => {
                let mutable = *mutable;
                let place = *place;
                let fresh = self.fresh_var();
                let place_ty = self.infer_expr(place, &fresh);
                // Always fresh: a borrow is an operation, and operations
                // carry no region (G11). Pinning it is an annotation's
                // job, which constrains this very variable from a type
                // position.
                let region = self.fresh_region();
                self.check_borrow_place(expr, mutable, place, region.clone());
                Ty::borrow(mutable, region, place_ty)
            }
            ExprData::AddrOf { mutable, place } => {
                let mutable = *mutable;
                let place = *place;
                let fresh = self.fresh_var();
                let place_ty = self.infer_expr(place, &fresh);
                self.check_addr_of_place(expr, mutable, place);
                Ty::raw_ptr(mutable, place_ty)
            }
            // `p.*`: reads through the pointer — `T.&raw` and `T.&raw mut`
            // both deref-read to `T` (writing is the assignment path's
            // judgement).
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                let fresh = self.fresh_var();
                let receiver_ty = self.infer_expr(receiver, &fresh);
                match self.resolve_shallow(&receiver_ty) {
                    Ty::RawPtr { pointee, .. } => (*pointee).clone(),
                    // Safe `.*`: deref of a borrow reads the referent, and
                    // needs no `unsafe` — the pointer's FLAVOR determines
                    // safety, which is already how the language works.
                    // Whether the READ is legal (copy vs. move out of a
                    // borrow) is [`InferCtx::finish_deref_reads`]'s
                    // judgement, once inference has settled.
                    Ty::Borrow { referent, .. } => {
                        let referent = (*referent).clone();
                        self.deref_reads.push((expr, referent.clone()));
                        // A `.*` in place position reads nothing itself —
                        // the field or element beyond it does. Its entry
                        // above is skipped, so the chain it roots carries
                        // the question onward, one step at a time.
                        if self.place_positions.contains(&expr) {
                            self.borrow_projections.insert(expr);
                        }
                        referent
                    }
                    // Evaluating the receiver already diverges.
                    Ty::Never => Ty::Never,
                    // An undetermined receiver: like a field access, the
                    // pointee can't be run backwards — ask for an
                    // annotation (same message, same recovery).
                    Ty::Infer(_) => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::FieldOnUnknownType { expr, receiver });
                        Ty::Error
                    }
                    // Errors are infectious and silent.
                    broken if broken.contains_error() => Ty::Error,
                    other => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::DerefNonPointer { expr, ty: other });
                        Ty::Error
                    }
                }
            }
            // `unsafe { ... }`: a pure checker region — fully transparent
            // for typing, exactly like a `const` block, except the loop
            // context survives (the body is the same runtime code, not a
            // separate unit): `loop { unsafe { break p.* } }` is fine.
            ExprData::Unsafe { body: inner } => {
                self.witness_sink = sink;
                let ty = self.infer_expr_with(*inner, expected, cause);
                self.result.type_of_expr.insert(expr, ty.clone());
                return ty;
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
                        if let PatData::Bind(binding) = &self.body.pats[param.pat] {
                            // The common case: a bare name's own annotation
                            // (if any) is the axiom.
                            match &self.body.bindings[*binding].type_ref {
                                Some(type_ref) => self.lower_type_ref(type_ref),
                                None => self.fresh_var(),
                            }
                        } else {
                            // A destructuring parameter: its own written
                            // annotation is the axiom; failing that, a
                            // `Newtype` pattern names its own type outright
                            // (`fn (Foo(...))` needs no annotation).
                            match &param.type_ref {
                                Some(type_ref) => self.lower_type_ref(type_ref),
                                None => self
                                    .declared_type_for_pat(param.pat)
                                    .unwrap_or_else(|| self.fresh_var()),
                            }
                        }
                    })
                    .collect();
                let ret = match ret_type {
                    Some(type_ref) => self.lower_type_ref(type_ref),
                    None => self.fresh_var(),
                };
                let ret_cause = match ret_type {
                    Some(_) => Some(Cause::ReturnAnnotation(expr)),
                    // Unwritten: the return type is whatever the position
                    // supplied, so the POSITION is what a tail mismatch
                    // inside the body points at ("because of this
                    // annotation"/"because of this argument").
                    None => cause,
                };
                // The literal's own signature is checked against its
                // context HERE — before the parameters are destructured,
                // before the body, and before any diverging-body default —
                // so a written slot (`let f: fn() -> usize = fn { panic("x")
                // };`) gets first say and everything the literal left
                // unwritten is a fresh variable the structural unify binds
                // to the slot's counterpart.
                //
                // BEFORE the body, not after, because the body's tail is
                // checked against the RETURN type and the sanctioned
                // variant→enum conversion happens at a CHECK: a tail
                // `Option::Some(v)` needs `Option::<usize>` in hand while it
                // is being checked, or it types as the tag-free variant and
                // the literal mismatches as a WHOLE — squiggle on the
                // literal, message about a type the reader never wrote.
                //
                // No arity guard is written here because `unify`'s `Fn` arm
                // is one: it compares parameter counts before binding any
                // part, so a literal that disagrees with the slot inherits
                // nothing and its body is checked on its own. `Ty::Error` is
                // infectious and silent there for the same reason.
                let own_sig = Ty::fn_type(param_tys.clone(), ret.clone());
                let reported = self.result.diagnostics.len();
                let fn_ty = self.check(expr, own_sig.clone(), expected, cause);
                // `check` poisons the unresolved numbers in what it was
                // handed, so a mismatch is the whole story — but the parts
                // this literal left unwritten are still bare variables
                // here; the body mints their numbers afterwards. Re-poison
                // once the body has run (below), or a literal that already
                // mismatched as a whole also collects a no-defining-use
                // diagnostic for a tail it was never going to keep.
                let slot_mismatched = self.result.diagnostics.len() != reported;
                self.result.type_of_expr.insert(expr, fn_ty.clone());
                // A parameter pattern has no per-call site to blame a broken
                // destructure on; the whole body is the best available
                // anchor (every call runs it). Destructured after the check
                // above, so a pattern whose type came from the position sees
                // it.
                for (param, ty) in params.iter().zip(&param_tys) {
                    self.check_pat(param.pat, ty, *fn_body);
                }
                let fn_body = *fn_body;
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
                // A `return` in the body targets THIS literal — the same
                // `ret`/`ret_cause` pair the tail below is checked against.
                // Pushed (not swapped): an outer entry stays reachable only
                // to the outer body, so a nested literal's `return` exits
                // the nested literal — including a literal nested inside a
                // `const` block, whose own reserved entry it covers.
                self.return_targets.push(ReturnTarget::Fn {
                    ty: ret.clone(),
                    cause: ret_cause,
                });
                let body_ty = self.infer_expr_with(fn_body, &ret, ret_cause);
                self.return_targets.pop();
                self.loop_sinks = saved_loops;
                self.scope_depth -= 1;
                // `check`'s `!`-coerces-to-anything shortcut leaves a still-
                // free expectation unbound rather than pinning it to `!`
                // (right, in general: a witness that happens to diverge must
                // not force a join's other branches to `!` too), and the
                // slot check above may already have bound `ret` to whatever
                // the context demanded. Only when NEITHER determined it —
                // `ret` is still genuinely free — does the body's own
                // divergence become the answer: a body that never completes
                // says nothing about what the function produces, so `!` is
                // the honest one.
                if matches!(self.resolve_shallow(&ret), Ty::Infer(_)) {
                    self.adopt(&ret, &body_ty);
                }
                if slot_mismatched {
                    poison_unresolved_number(self.table, &own_sig);
                }
                return fn_ty;
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
                            self.adopt(&result, &Ty::Never);
                            Ty::Never
                        }
                        1 => {
                            let ty = witnesses.into_iter().next().unwrap().ty;
                            self.adopt(&result, &ty);
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
            ExprData::Return { value } => match self.return_targets.last().cloned() {
                // RESERVED: the nearest body is a `const` block.
                // Conceptually this `return` bails from the OUTER fn body —
                // which needs cross-body machinery that no v1 pass has — so
                // it is refused outright rather than silently retargeted at
                // the block. The operand is still inferred (freely: nothing
                // here demands a type of it) so its contents get types and
                // diagnostics, and the `return` still types `!`: it
                // produces no value under any reading, and MIR traps on it.
                Some(ReturnTarget::ConstBlock) => {
                    if let Some(value) = value {
                        let fresh = self.fresh_var();
                        let ty = self.infer_expr(*value, &fresh);
                        // The reservation is the whole story: nothing here
                        // demanded a type of the operand, so a bare literal
                        // in it must not add a no-defining-use sibling.
                        poison_unresolved_number(self.table, &ty);
                    }
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::ReturnInConstBlock { expr });
                    Ty::Never
                }
                Some(ReturnTarget::Fn {
                    ty: ret,
                    cause: ret_cause,
                }) => {
                    match value {
                        // THE seam: the operand is checked against the
                        // enclosing body's return type with that type's own
                        // cause — the identical call the body's tail gets.
                        // An annotated return type therefore blames the
                        // operand and cites the annotation; an inferred one
                        // is pinned here just as a tail would pin it.
                        Some(value) => {
                            self.infer_expr_with(*value, &ret, ret_cause);
                        }
                        // `return;` returns `()`. Nothing else carries the
                        // blame, so the `return` keyword itself does.
                        None => {
                            self.check(expr, Ty::Unit, &ret, ret_cause);
                        }
                    }
                    // The `return` expression itself never produces a value.
                    Ty::Never
                }
                None => {
                    // At an item initializer's top level: there is no
                    // function to leave. The value is still inferred (freely:
                    // nothing here demands a type of it) so its contents get
                    // types and diagnostics, and — matching the const-block
                    // arm above — a bare literal in it must not add a
                    // no-defining-use sibling to the refusal.
                    if let Some(value) = value {
                        let fresh = self.fresh_var();
                        let ty = self.infer_expr(*value, &fresh);
                        poison_unresolved_number(self.table, &ty);
                    }
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::ReturnOutsideFn { expr });
                    // Unlike the const-block arm's `Ty::Never`, this is
                    // `Ty::Error`: the refusal marks a genuinely malformed
                    // program (no body encloses the `return` at all), so its
                    // type should not participate in further inference the
                    // way a real `!` would.
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

    /// The type of `receiver.name` given the receiver's SHALLOW-RESOLVED
    /// type — the Field arm's judgement, factored out so the dot-call
    /// path in the `Call` arm (which must decide field-vs-member before
    /// choosing a callee) shares it verbatim.
    fn field_access_ty(&mut self, expr: ExprId, receiver: ExprId, name: &str, resolved: Ty) -> Ty {
        // A BORROW receiver. Reaching through it to the referent's FIELDS
        // would be auto-deref, which G14 rules out absolutely — and the
        // one licensed exception runs the other way (the compiler may
        // insert a borrow of `x.*`, never a deref of `x`). The escape is
        // one character: write the deref.
        //
        // Its MEMBERS are the other question, and a different path: a
        // member whose own `Self` parameter is a borrow takes the call
        // (G14 clause 1), which `receiver_takes` decides before the
        // dot-call path ever falls through to this helper. A value `Self`
        // still lands here, on the callee, with this same message.
        if matches!(resolved, Ty::Borrow { .. }) {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::DotThroughBorrow {
                    expr,
                    name: name.to_owned(),
                    receiver_ty: resolved,
                });
            return Ty::Error;
        }
        match resolved {
            Ty::Record(rec) => match rec.field_ty(name) {
                Some(field_ty) => field_ty.clone(),
                None => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NoSuchField {
                            expr,
                            name: name.to_owned(),
                            receiver_ty: Ty::Record(rec),
                        });
                    Ty::Error
                }
            },
            // A named type projects through to its declared
            // shape — with the mention's generic args
            // substituted in: `p.a` on a `Pair::<usize>` is a
            // `usize`.
            Ty::Named(named) => {
                match type_underlying_for(self.db, &named) {
                    Some(Ty::Record(rec)) => match rec.field_ty(name) {
                        Some(field_ty) => field_ty.clone(),
                        None => {
                            // A member fn is not a field value — a bare
                            // dot reaching one gets the call-it hint
                            // instead of "no such field".
                            if self.member_of(&named.decl, name).is_some() {
                                self.result.diagnostics.push(
                                    InferenceDiagnostic::MemberNotCalled {
                                        expr,
                                        name: name.to_owned(),
                                    },
                                );
                            } else {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::NoSuchField {
                                        expr,
                                        name: name.to_owned(),
                                        receiver_ty: Ty::Named(named),
                                    });
                            }
                            Ty::Error
                        }
                    },
                    _ => {
                        // An enum value has no fields at all (v1 payloads
                        // are positional and only reachable through
                        // `match`) — but it can have members. A BROKEN
                        // declaration is neither a record nor an enum, and
                        // its own diagnostic sits at the declaration site:
                        // stay silent, members included. A `with`-chain
                        // parses independently of a broken RHS, so without
                        // this guard a *call* on such a type falls through
                        // to here and is told to call what it just called.
                        if enum_variants(self.db, named.decl.to_id(self.db)).is_some() {
                            if self.member_of(&named.decl, name).is_some() {
                                self.result.diagnostics.push(
                                    InferenceDiagnostic::MemberNotCalled {
                                        expr,
                                        name: name.to_owned(),
                                    },
                                );
                            } else {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::NoSuchField {
                                        expr,
                                        name: name.to_owned(),
                                        receiver_ty: Ty::Named(named),
                                    });
                            }
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
                    .push(InferenceDiagnostic::FieldOnUnknownType { expr, receiver });
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
                        name: name.to_owned(),
                        receiver_ty: other,
                    });
                Ty::Error
            }
        }
    }

    /// Whether the `type` item declares a FIELD by this name — the
    /// namespace a qualified `::` path is NOT looking in (fields belong to
    /// values, members to the type). Used to turn "no variants" into the
    /// message that names the escape.
    fn decl_has_field(&self, item: ItemId<'db>, name: &str) -> bool {
        matches!(
            crate::type_decl(self.db, item),
            Some(TypeDeclData::Struct { fields }) if fields.iter().any(|(f, _)| f == name)
        )
    }

    /// The member of `decl`'s `with`-chain named `name`, as a member
    /// [`ItemLoc`] — first occurrence wins (duplicates carry their own
    /// diagnostic at the definition).
    fn member_of(&self, decl: &ItemLoc, name: &str) -> Option<ItemLoc> {
        if decl.member.is_some() {
            return None;
        }
        crate::item_tree::type_members(self.db, decl.to_id(self.db))
            .iter()
            .find(|m| m.name == name)
            .map(|m| ItemLoc {
                file: decl.file,
                name: decl.name.clone(),
                disambiguator: decl.disambiguator,
                member: Some((std::sync::Arc::from(m.name.as_str()), m.disambiguator)),
            })
    }

    /// Complete a call whose callee is an ordinary VALUE of type
    /// `callee_ty` — the `Call` arm's judgement, factored out so the
    /// dot-call path's field fallback shares it. Returns the call's type
    /// (pre-`check`, like any match-arm value).
    fn call_of_value(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        args: &[ExprId],
        callee_ty: Ty,
    ) -> Ty {
        match self.resolve_shallow(&callee_ty) {
            // The callee's type is still being inferred (an
            // in-group signature, e.g. mutual recursion): calling
            // it commits it to a function of this shape. A
            // NUMBER-CLASS callee refuses the commitment — a
            // number is not callable.
            Ty::Infer(var) => {
                let params: Vec<Ty> = args.iter().map(|_| self.fresh_var()).collect();
                let ret = self.fresh_var();
                // A call learns the callee's SHAPE and must not invent its
                // SAFETY — but committing the variable to a `Ty::Fn` means
                // naming one, so the answer is asked for rather than
                // guessed: a join that has not solved yet is the one thing
                // that can still be holding the value, and its witnesses
                // know. Anything else (a mutually-recursive item signature,
                // the usual case here) is safe, and can only be safe: an
                // inferred signature is never an import's.
                let unsafe_to_call = self
                    .constraints
                    .pending_join_mints_unsafe_fn(self.table, var);
                let shape = Ty::fn_type_with(unsafe_to_call, params.clone(), ret.clone());
                if self.adopt(&Ty::Infer(var), &shape) {
                    for (i, &arg) in args.iter().enumerate() {
                        self.infer_expr(arg, &params[i]);
                    }
                    ret
                } else {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NotCallable {
                            expr: callee,
                            ty: Ty::UnresolvedNumber,
                        });
                    // The commitment contradicts what the callee's
                    // signature is already committed to: poison it
                    // so the member reports via the needs-annotation
                    // path instead of publishing a guess.
                    self.table.union_value(var, TyVarValue::Known(Ty::Error));
                    self.infer_args_broken(args);
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
                    self.infer_expr_with(arg, &param, Some(Cause::CallSite { call: expr, arg }));
                }
                f.ret.clone()
            }
            Ty::Error => {
                self.infer_args_broken(args);
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
                    // error (the no-defining-use error on an
                    // unreachable literal argument stays one for the
                    // same reason).
                    self.infer_expr(arg, &arg_fresh);
                }
                Ty::Never
            }
            other => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::NotCallable {
                        expr: callee,
                        ty: other,
                    });
                self.infer_args_broken(args);
                Ty::Error
            }
        }
    }

    /// A dot-call `recv.name(a, b)`, TR01's structural dot-call with
    /// G13's SYNTAX-DIRECTED namespace rule: everything that could carry
    /// the call — an inherent member of recv's type, a trait-impl member
    /// for it, an fn-typed field — is collected FIRST. Exactly one
    /// candidate resolves the call; MORE THAN ONE is an ambiguity error
    /// naming every escape (no fall-through: a trait impl may live in
    /// the trait's own chain, nowhere near the type, so silent shadowing
    /// either way would be action at a distance). Zero candidates keep
    /// the old story: a non-dot-callable member, a plain field, or
    /// `NoSuchMember`.
    ///
    /// (Bare `recv.name` selects the FIELD; `(recv.name)(...)` is a value
    /// call of the field — neither reaches this function.) The member
    /// call desugars to the member with recv as the LAST argument.
    /// Module-level statics never resolve here (G13's deliberate opt-out);
    /// NO auto-deref, NO auto-ref.
    #[allow(clippy::too_many_arguments)]
    fn infer_dot_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        name: &str,
        member_args: Option<&[GenericArgData]>,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        // A written member turbofish (`s.flat_map::<usize>(f)`) is
        // spendable on THREE outcomes below — an inherent member, a
        // trait-impl member, and a bound-directed requirement — and each
        // TAKES it out of this slot.
        //
        // What is left over is consumed SILENTLY, and that is the whole
        // design: the refusals that are TRUE ("a field takes no generic
        // arguments", "a builtin member takes none") are written at the
        // three sites where the target genuinely resolved and genuinely has
        // no binder. Every other exit is a path already carrying its own
        // diagnostic — a broken receiver, an ambiguous name, a member
        // reached through the wrong receiver shape, or a half-typed
        // `o.map::<` — and on those "`map` takes no generic arguments" is
        // both noise and, usually, a lie.
        //
        // The `&mut` is what makes "exactly once" structural rather than a
        // rule every exit has to remember. It buys that, and one thing
        // more: an argument's const expression is typed WHERE IT LANDS —
        // loudly at a spend site, quietly here — so `o.map::<3>(w)` reports
        // the number the same way `id::<3>(1)` does, while a list nobody
        // could spend stays silent. Typing the whole list up front instead
        // would let the value be passed by value, at the price of that
        // parity.
        let mut unspent = member_args;
        let ty = self.infer_dot_call_resolved(
            expr,
            callee,
            receiver,
            name,
            &mut unspent,
            args,
            expected,
            cause,
        );
        self.drop_member_args(&mut unspent);
        ty
    }

    /// [`Self::infer_dot_call`]'s resolution proper — see there for what
    /// `member_args` being a `&mut Option` buys.
    #[allow(clippy::too_many_arguments)]
    fn infer_dot_call_resolved(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        name: &str,
        member_args: &mut Option<&[GenericArgData]>,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        // The callee (the field-access expression) is visited by hand, so
        // record its expectation and (below) its type like any visited
        // expression.
        let callee_expectation = self.fresh_var();
        self.result
            .expectation_of_expr
            .insert(callee, callee_expectation.clone());
        let receiver_fresh = self.fresh_var();
        let receiver_ty = self.infer_expr(receiver, &receiver_fresh);
        let resolved = self.resolve_shallow(&receiver_ty);

        // A BORROW receiver reaches the referent's MEMBERS — and only its
        // members. Which members is the whole question, and the answer is
        // `receiver_takes`: a member whose own `Self` parameter is a borrow
        // takes the call (the receiver reborrows into it, G14 clause 1);
        // one whose `Self` is a value does NOT, because reaching it would
        // be auto-deref, which G14 seals absolutely. FIELDS are never
        // reached through a borrow for the same reason — `.*` is one
        // character, and `field_access_ty` below says so.
        //
        // Note this is a *lookup* through the borrow, not a coercion: the
        // receiver's type is unchanged, and the only thing that ever meets
        // the member's `Self` parameter is the borrow the user wrote.
        let receiver_shape = ReceiverShape::of(&resolved);
        let member_recv = match &resolved {
            Ty::Borrow { referent, .. } => self.resolve_shallow(referent),
            _ => resolved.clone(),
        };

        // A RIGID receiver: only its bounds can re-open members (TR07) —
        // bound-directed resolution, lowered through the hidden dictionary.
        // Read off `member_recv`, so a BORROW of a rigid receiver reaches
        // the same bounds a bare one does: inside a generic body
        // `T.&::<@z>` is exactly the shape a `Self.&`-taking requirement is
        // called on, and routing it down the concrete path instead produced
        // "write `.*.pass`" — advice that would move out of a borrow.
        if let Ty::Param(param) = &member_recv
            && !name.is_empty()
        {
            let param = param.clone();
            // A member takes the receiver as its LAST ARGUMENT, so this
            // mention is a value position however it is written — the
            // answer `place_positions` could not have.
            self.value_receivers.insert(receiver);
            let ty = self.infer_bound_member_dot_call(
                expr,
                callee,
                receiver,
                &receiver_ty,
                &param,
                name,
                member_args,
                args,
                &callee_expectation,
                receiver_shape,
                &resolved,
            );
            return self.finish_dot_call(expr, ty, expected, cause);
        }

        // A named receiver (a variant-typed one reaches its ENUM's
        // members: the receiver widens — variant → enum, the sanctioned
        // conversion — into the Self argument, exactly as it would into
        // any enum-typed parameter).
        let named_recv = match &member_recv {
            Ty::Named(named) if !name.is_empty() => Some(named.clone()),
            Ty::Variant(variant) if !name.is_empty() => Some(NamedTy {
                decl: variant.decl.clone(),
                args: variant.args.clone(),
            }),
            _ => None,
        };
        // Whether the receiver's type declares `name` as a FIELD — the
        // call-syntax fallback, and (when a member wins) the shadowed half
        // of the shared dot. An FN-TYPED field can carry the call itself,
        // so it is a CANDIDATE beside the members; a plain field is not
        // (nothing about it could take a call).
        let mut named_has_field = false;
        // A BUILTIN-typed receiver (`5.fmt(w)`, `"x".fmt(w)`) has no fields
        // and no inherent members, but trait impls reach it (`impl usize`
        // in a trait's chain) — the same candidate collection, minus the
        // two halves a builtin cannot have. Which scalars those are is
        // `SelfKey`'s to say, so the set is not spelled twice.
        let builtin_recv = named_recv.is_none()
            && !name.is_empty()
            && matches!(
                crate::traits::SelfKey::for_ty(&member_recv),
                Some(crate::traits::SelfKey::Builtin(_))
            );
        if named_recv.is_some() || builtin_recv {
            let underlying = named_recv
                .as_ref()
                .and_then(|named| type_underlying_for(self.db, named));
            let field_ty = match &underlying {
                // A borrow receiver reaches no fields: that IS auto-deref.
                _ if receiver_shape.is_borrow() => None,
                Some(Ty::Record(rec)) => rec.field_ty(name).cloned(),
                _ => None,
            };
            named_has_field = field_ty.is_some();
            let fn_field = matches!(&field_ty, Some(Ty::Fn(_)));
            // A BROKEN declaration (neither a struct shape nor an enum):
            // its own diagnostic sits at the declaration site — fall
            // through to the field path, whose broken arm stays silent
            // (errors are infectious and silent).
            let decl_broken = underlying.is_none()
                && named_recv.as_ref().is_some_and(|named| {
                    enum_variants(self.db, named.decl.to_id(self.db)).is_none()
                });
            if !decl_broken {
                // The receiver's type as the escapes spell it — the ENUM
                // for a variant-typed receiver (impls live on the enum, and
                // the receiver widens into the Self argument, the ordinary
                // sanctioned conversion).
                //
                // Under a BORROW receiver this is the REFERENT's type, not
                // the borrow: impls live on the referent, and every escape
                // (`Map::get(k, m)`) spells the referent too.
                let recv_ty = match &named_recv {
                    Some(named) => Ty::Named(named.clone()),
                    None => member_recv.clone(),
                };
                let inherent = named_recv
                    .as_ref()
                    .and_then(|named| self.member_of(&named.decl, name));
                if inherent
                    .as_ref()
                    .is_some_and(|loc| signature(self.db, loc.to_id(self.db)).contains_error())
                {
                    // A broken member definition (not fully annotated): the
                    // definition site carries the diagnostic.
                    self.result.type_of_expr.insert(callee, Ty::Error);
                    self.infer_args_broken(args);
                    return self.finish_dot_call(expr, Ty::Error, expected, cause);
                }
                let inherent_carrier = inherent
                    .as_ref()
                    .filter(|loc| self.member_takes_receiver(loc, receiver_shape))
                    .cloned();
                let traits = self.trait_call_candidates(&recv_ty, name, receiver_shape);
                // A broken IMPL member is the definition site's problem
                // too — it carries no call and makes nothing ambiguous.
                if traits.iter().any(|candidate| candidate.broken) {
                    self.result.type_of_expr.insert(callee, Ty::Error);
                    self.infer_args_broken(args);
                    return self.finish_dot_call(expr, Ty::Error, expected, cause);
                }
                let trait_carriers: Vec<&TraitCallCandidate> = traits
                    .iter()
                    .filter(|candidate| candidate.dot_callable)
                    .collect();
                let carriers = usize::from(inherent_carrier.is_some())
                    + trait_carriers.len()
                    + usize::from(fn_field);
                // MORE THAN ONE candidate: refuse, naming every escape.
                if carriers > 1 {
                    let mut candidates: Vec<MemberCandidate> = Vec::new();
                    if let Some(def) = &inherent_carrier {
                        candidates.push(MemberCandidate::Inherent {
                            type_name: recv_ty.display(),
                            def: def.clone(),
                        });
                    }
                    for candidate in &trait_carriers {
                        candidates.push(MemberCandidate::Trait {
                            trait_name: candidate.trait_.display_name().to_owned(),
                            type_name: recv_ty.display(),
                            // A dot-call's receiver sits at the
                            // requirement's `Self` parameter, so the short
                            // form determines `Self` by itself.
                            short: true,
                            def: candidate.member.clone(),
                        });
                    }
                    if fn_field {
                        candidates.push(MemberCandidate::Field);
                    }
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::MemberCallAmbiguity {
                            expr,
                            name: name.to_owned(),
                            receiver_ty: recv_ty,
                            candidates,
                        });
                    self.result.type_of_expr.insert(callee, Ty::Error);
                    self.infer_args_broken(args);
                    return self.finish_dot_call(expr, Ty::Error, expected, cause);
                }
                // Exactly one candidate takes the call (a lone fn-typed
                // field falls through to the field path below).
                if let Some(member_loc) = inherent_carrier {
                    let named = named_recv
                        .clone()
                        .expect("an inherent member needs a named receiver");
                    self.value_receivers.insert(receiver);
                    let sig = signature(self.db, member_loc.to_id(self.db));
                    let ret = self.infer_member_call(
                        expr,
                        callee,
                        receiver,
                        &receiver_ty,
                        &named,
                        member_loc,
                        sig,
                        &callee_expectation,
                        member_args,
                        args,
                    );
                    return self.finish_dot_call(expr, ret, expected, cause);
                }
                if let [candidate] = trait_carriers.as_slice() {
                    let member_loc = candidate
                        .member
                        .clone()
                        .expect("a carrying candidate has its impl member");
                    self.value_receivers.insert(receiver);
                    let ty = self.infer_impl_member_call(
                        expr,
                        callee,
                        receiver,
                        &receiver_ty,
                        member_loc,
                        name,
                        member_args,
                        args,
                        &callee_expectation,
                        receiver_shape,
                    );
                    return self.finish_dot_call(expr, ty, expected, cause);
                }
                // A BUILTIN member (`line.next_char(i)`). Consulted HERE,
                // after every user candidate has had its turn and found
                // nothing: a `next_char` written in an `impl ... for str`
                // shadows this one, which is the `print` rule (user
                // declarations shadow builtins) applied to a member.
                if let Some(builtin) = Builtin::member_by_name(&member_recv, name) {
                    // A builtin member takes its receiver BY VALUE, so a
                    // borrow receiver does not reach it — that would be
                    // auto-deref, sealed. It gets the same refusal a
                    // value-`Self` user member gets in this position
                    // (`receiver_shape_refusal`'s `(Borrow, Value)` arm):
                    // named, on the callee, pointing at `.*`. Falling
                    // through to "no such member" instead would have been
                    // a lie about a member that plainly exists.
                    if receiver_shape.is_borrow() {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::DotThroughBorrow {
                                expr: callee,
                                name: name.to_owned(),
                                receiver_ty: resolved.clone(),
                            });
                        self.result.type_of_expr.insert(callee, Ty::Error);
                        self.infer_args_broken(args);
                        return self.finish_dot_call(expr, Ty::Error, expected, cause);
                    }
                    // Resolved, and a builtin member has no binder: one
                    // of the three places the refusal is TRUE.
                    self.refuse_member_args(callee, callee, name, member_args);
                    let ty = self.infer_builtin_member_call(
                        expr,
                        callee,
                        receiver,
                        &receiver_ty,
                        builtin,
                        args,
                        &callee_expectation,
                    );
                    return self.finish_dot_call(expr, ty, expected, cause);
                }
                // Nothing carries the call: an impl missing the
                // requirement, a member without the dot-callable shape
                // (TR01 is structural), or an unknown name. A field — even a
                // plain one — still gets its own (better) story below.
                let impl_gap = traits.iter().find(|candidate| candidate.member.is_none());
                if let Some(candidate) = impl_gap {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NoTraitImpl {
                            expr,
                            trait_: candidate.trait_.clone(),
                            ty: recv_ty,
                        });
                    self.result.type_of_expr.insert(callee, Ty::Error);
                    self.infer_args_broken(args);
                    return self.finish_dot_call(expr, Ty::Error, expected, cause);
                }
                if !named_has_field {
                    let not_dot_callable =
                        inherent.or_else(|| traits.first().and_then(|c| c.member.clone()));
                    match not_dot_callable {
                        // A member exists but this RECEIVER cannot reach it.
                        // Three reasons, and each names its own escape —
                        // `receiver_takes`'s table read backwards.
                        Some(member) => {
                            let position = self.self_position(&member);
                            let diagnostic = self.receiver_shape_refusal(
                                expr,
                                callee,
                                name,
                                receiver_shape,
                                position,
                                &resolved,
                                member,
                            );
                            self.result.diagnostics.push(diagnostic);
                        }
                        None => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NoSuchMember {
                                    expr,
                                    name: name.to_owned(),
                                    receiver_ty: recv_ty,
                                });
                        }
                    }
                    self.result.type_of_expr.insert(callee, Ty::Error);
                    self.infer_args_broken(args);
                    return self.finish_dot_call(expr, Ty::Error, expected, cause);
                }
            }
        }

        // The field path: the callee is an ordinary field access, judged
        // by the shared helper, and the call proceeds on its value.
        let callee_ty = if name.is_empty() {
            Ty::Error
        } else {
            self.field_access_ty(callee, receiver, name, resolved.clone())
        };
        let callee_ty = self.check(callee, callee_ty, &callee_expectation, None);
        self.result.type_of_expr.insert(callee, callee_ty.clone());
        // A named receiver's plain (non-fn) field under call syntax gets
        // the precise story — the field exists but is no fn, and there is
        // no member either — instead of the generic not-callable text.
        if named_has_field {
            let resolved_callee = self.resolve_shallow(&callee_ty);
            let callable_shaped = matches!(
                resolved_callee,
                Ty::Fn(_) | Ty::Infer(_) | Ty::Never | Ty::Error
            );
            if !callable_shaped {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::FieldNotCallable {
                        expr,
                        name: name.to_owned(),
                        ty: resolved_callee,
                        receiver_ty: resolved.clone(),
                    });
                self.infer_args_broken(args);
                return self.finish_dot_call(expr, Ty::Error, expected, cause);
            }
        }
        // The call is carried by the field's VALUE, and a value has no
        // binder: the second place the refusal is true — but only when
        // there IS a value. A field access that produced `{error}` (an
        // unknown receiver type, a name that is no field) is already
        // diagnosed, and errors are infectious and silent.
        if !self.resolve_shallow(&callee_ty).contains_error() {
            self.refuse_member_args(callee, callee, name, member_args);
        }
        let ty = self.call_of_value(expr, callee, args, callee_ty);
        self.finish_dot_call(expr, ty, expected, cause)
    }

    /// The rigid `Self` of the enclosing MEMBER body, when `mention` is a
    /// `Self`-based type mention (a bare `Self` constructor head, a
    /// `Self::Variant` path's base, or a — rejected — `Self::<...>`
    /// turbofish). `None` anywhere else.
    fn rigid_self_mention(&self, mention: ExprId) -> Option<NamedTy> {
        let base = match &self.body.exprs[mention] {
            ExprData::NameRef(_) => mention,
            ExprData::VariantPath { base, .. } | ExprData::GenericApp { base, .. } => *base,
            _ => return None,
        };
        match &self.body.exprs[base] {
            ExprData::NameRef(name) if name == "Self" => {}
            _ => return None,
        }
        let own = self.own_item.as_ref()?;
        own.member.as_ref()?;
        match member_self_ty(self.db, own.to_id(self.db)) {
            Some(Ty::Named(named)) => Some(named),
            _ => None,
        }
    }

    /// The common tail every dot-call outcome funnels through: check the
    /// call's type against its expectation and persist it — exactly what
    /// [`InferCtx::infer_expr_with`]'s own tail does for ordinary arms.
    fn finish_dot_call(&mut self, expr: ExprId, ty: Ty, expected: &Ty, cause: Option<Cause>) -> Ty {
        let ty = self.check(expr, ty, expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// The resolved-member half of a dot-call: instantiate the member's
    /// scheme at the RECEIVER's generic arguments (the receiver's type is
    /// the turbofish a dot-call never spells for the OWNER's half) plus its
    /// own binder (which a dot-call MAY spell: `s.flat_map::<usize>(f)`),
    /// record the resolution for MIR/IDE, and check the written arguments
    /// plus the receiver-as-last-argument. Returns the call's (pre-`check`)
    /// type.
    #[allow(clippy::too_many_arguments)]
    fn infer_member_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        receiver_ty: &Ty,
        named: &NamedTy,
        member_loc: ItemLoc,
        sig: Ty,
        callee_expectation: &Ty,
        member_args: &mut Option<&[GenericArgData]>,
        args: &[ExprId],
    ) -> Ty {
        // A member's binder is the OWNER's followed by its own (TR10), so
        // the receiver type's arguments map onto the owner's PREFIX
        // position by position; everything past it is the member's own,
        // instantiated fresh per call and spellable by the member
        // turbofish. A SHORT argument list — a mention that failed to
        // resolve its own arguments — would leave the tail rigid and then
        // blame a type parameter the user never wrote, so the member call
        // is abandoned instead: the mention carries the error.
        let owner_arity = self.owner_binder_arity(&member_loc);
        if named.args.len() != owner_arity {
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        let (mut subst, const_subst) = owner_arg_subst(&named.args);
        let generics = item_generics(self.db, member_loc.to_id(self.db)).to_vec();
        subst.extend(self.member_own_type_subst(
            expr,
            callee,
            &member_loc,
            &sig,
            &generics,
            owner_arity,
            member_args,
            &MemberSite {
                path: Self::member_path(named.decl.display_name(), member_loc.display_name()),
                spelling: TurbofishSpelling::Dot,
                owner_is_trait: false,
                owner_hint: None,
            },
        ));
        let region_subst = self.member_own_region_subst(expr, &member_loc, owner_arity);
        let inst = instantiate_scheme(&sig, &member_loc, &subst, &const_subst);
        let inst = substitute_regions(&inst, &member_loc, &region_subst);
        self.result.member_of_expr.insert(expr, member_loc);
        // Arity is checked by `finish_receiver_call`; the receiver IS the
        // last argument, checked against the instantiated Self param —
        // identical by construction for enum/struct-typed receivers (a
        // plain unification that lets leftover inference variables
        // flow), a WIDENING for variant-typed ones (the tag injection
        // lands on the receiver expression like at any other check
        // site).
        self.finish_receiver_call(
            expr,
            callee,
            receiver,
            receiver_ty,
            inst,
            callee_expectation,
            args,
        )
    }

    /// The shared tail of every receiver-appending call resolution
    /// (inherent, trait-impl and bound-directed dot-calls, once each has
    /// resolved its own member and instantiated `inst`): check the
    /// callee against `inst`, check arity, infer each written argument
    /// against its param, then check the receiver against the LAST
    /// param.
    #[allow(clippy::too_many_arguments)]
    fn finish_receiver_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        receiver_ty: &Ty,
        inst: Ty,
        callee_expectation: &Ty,
        args: &[ExprId],
    ) -> Ty {
        let inst_ty = self.check(callee, inst.clone(), callee_expectation, None);
        self.result.type_of_expr.insert(callee, inst_ty);
        let Ty::Fn(f) = inst else {
            unreachable!("member signatures are fn-typed by construction");
        };
        // Arity: the Self param is supplied by the receiver, so it
        // doesn't count.
        if f.params.len() != args.len() + 1 {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::ArgCountMismatch {
                    expr,
                    expected: f.params.len() - 1,
                    found: args.len(),
                });
        }
        for (i, &arg) in args.iter().enumerate() {
            let param = f.params.get(i).cloned().unwrap_or(Ty::Error);
            self.infer_expr_with(arg, &param, Some(Cause::CallSite { call: expr, arg }));
        }
        // The receiver IS the last argument: check it against the
        // instantiated Self param — identical by construction for
        // enum/struct-typed receivers (a plain unification that lets
        // leftover inference variables flow), a WIDENING for
        // variant-typed ones (the tag injection lands on the receiver
        // expression like at any other check site).
        if let Some(last) = f.params.last() {
            self.check(receiver, receiver_ty.clone(), last, None);
        }
        f.ret.clone()
    }

    /// The arity of the OWNER's binder for an inherent member — the prefix
    /// of the member's binder the receiver's type supplies (TR10: the
    /// owner's params keep the low indices, the member's own are appended).
    fn owner_binder_arity(&self, member_loc: &ItemLoc) -> usize {
        crate::member_owner(self.db, member_loc.to_id(self.db))
            .and_then(|owner| {
                crate::item_data(self.db, owner)
                    .as_ref()
                    .map(|data| data.generics.len())
            })
            .unwrap_or_default()
    }

    /// The BUILTIN-member half of a dot-call (`line.next_char(i)`):
    /// [`Self::infer_member_call`]'s shape with none of the instantiation
    /// (a builtin member has no binder, no owner and no regions), so it is
    /// just the signature and the shared receiver-appending tail.
    fn infer_builtin_member_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        receiver_ty: &Ty,
        builtin: Builtin,
        args: &[ExprId],
        callee_expectation: &Ty,
    ) -> Ty {
        self.result.builtin_member_of_expr.insert(expr, builtin);
        self.finish_receiver_call(
            expr,
            callee,
            receiver,
            receiver_ty,
            builtin_type(builtin, self.file),
            callee_expectation,
            args,
        )
    }

    /// Consume a member turbofish that nothing could spend, SILENTLY —
    /// the const arguments still get types (every expression in a body
    /// must), and no diagnostic is added.
    ///
    /// This is the default for every exit: errors are infectious and
    /// silent, and by the time a path gives up on a member turbofish it is
    /// giving up on something already diagnosed. [`Self::refuse_member_args`]
    /// is the deliberate opposite, and its claim has to be TRUE.
    fn drop_member_args(&mut self, member_args: &mut Option<&[GenericArgData]>) {
        if let Some(args) = member_args.take() {
            // QUIET, and the `_quiet` is the whole point: a bare integer in
            // a list nobody spent has no defining use, and telling the
            // reader to annotate a number the program was never going to
            // keep is the same noise the refusal above already avoids.
            self.infer_const_args_free_quiet(args);
        }
    }

    /// Refuse a member turbofish on a target that RESOLVED and simply has
    /// no binder — a field, a builtin member, a member declaring none.
    /// Only callable where "`name` takes no generic arguments" is a true
    /// sentence about a thing that exists; everywhere else the silent
    /// [`Self::drop_member_args`] is the answer. A written region is
    /// refused first, on `mention` (the node the list hangs off), exactly
    /// as a binder would refuse it ([`Self::owes_not_generic`]).
    fn refuse_member_args(
        &mut self,
        key: ExprId,
        mention: ExprId,
        name: &str,
        member_args: &mut Option<&[GenericArgData]>,
    ) {
        if let Some(args) = *member_args
            && self.owes_not_generic(mention, args, TurbofishList::Member)
        {
            self.push_not_generic(key, name);
        }
        self.drop_member_args(member_args);
    }

    /// Spend ONE written member-turbofish argument on the fresh variable
    /// standing for the member's type parameter `param_name`. A type
    /// argument joins the variable (a `_` hole lowers to a fresh variable
    /// of its own — explicitly "infer this one"); a const argument is the
    /// ordinary wrong-kind report.
    ///
    /// `mention` is the node the member's argument list hangs off — the
    /// qualified path or the dot-call's callee — and is what
    /// [`Cause::MemberGenericArg`] points the blame at. It is not always
    /// `key`: a dot-call's obligations are keyed on the CALL, while its
    /// turbofish is written on the callee. `position` is the argument's
    /// index in the WRITTEN list, which the blame renderer counts.
    fn spend_member_arg(
        &mut self,
        key: ExprId,
        mention: ExprId,
        position: usize,
        param_name: &str,
        arg: &GenericArgData,
        var: &Ty,
    ) {
        match arg {
            GenericArgData::Type(type_ref) => {
                let written = self.lower_type_ref(type_ref);
                self.constraints.adopt(
                    self.table,
                    var,
                    &written,
                    Some(Cause::MemberGenericArg {
                        mention,
                        index: position as u32,
                    }),
                );
            }
            GenericArgData::Const(value) => {
                let value = *value;
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::GenericArgKindMismatch {
                        expr: key,
                        param: param_name.to_owned(),
                        param_is_const: false,
                    });
                // LOUD, unlike the quiet typing a list nobody spent gets
                // (`drop_member_args`): this argument was matched to a
                // slot and is being kept, so it is an expression of the
                // body like any other. `o.map::<3>(w)` therefore also asks
                // for an annotation on the `3` — exactly what the ITEM
                // turbofish already says for `id::<3>(1)`. Same shape, same
                // pair of messages; a member list that answered differently
                // would be a fork nothing asked for.
                let fresh = self.fresh_var();
                self.infer_expr(value, &fresh);
            }
            // Already refused before matching — a region on its own
            // (`spellable_args`: regions are inferred at every call), a
            // named argument for the whole list (`reject_named_args_because`:
            // a member's own arguments are positional, and the one nameable
            // argument v1 has is the OWNER's `Self`). Nothing further to
            // say, and nothing to spend.
            GenericArgData::Region(_) | GenericArgData::Named { .. } => {}
        }
    }

    /// Which written member-turbofish arguments may actually be SPENT on a
    /// member's own binder — as POSITIONS in the written list, in binder
    /// order, because a refused region makes the written position and the
    /// spent one diverge. The list is positional over that binder's TYPE
    /// parameters, and it is refused WHOLE — never in part — for the three
    /// reasons a positional match cannot be trusted: the binder declares a
    /// still-reserved CONST parameter (`MemberOwnConstArgs`), the member
    /// has no spendable binder at all (`NotGeneric` — the same sentence
    /// every other binder-less turbofish gets, INCLUDING for an empty
    /// `::<>`, which is a written list like any other), or the count does
    /// not line up (`MemberGenericArgCount`). `None` means "spend nothing";
    /// the diagnostic and the const arguments' own inference are done.
    ///
    /// A written REGION argument is refused on its own before any of that
    /// and then dropped ([`Self::spellable_args`]) — the same rule and the
    /// same sentence an item mention applies. A dropped region also
    /// displaces the count, so no arity sentence is trustworthy about a
    /// list the reader can see it in; the refusal stands alone.
    ///
    /// Every message here names the member through [`MemberSite::path`] —
    /// ONE naming per site rather than three re-derivations from three
    /// different sources. `mention` is the node the list hangs off, where
    /// the per-argument refusal is reported.
    fn matched_member_args(
        &mut self,
        key: ExprId,
        mention: ExprId,
        site: &MemberSite,
        generics: &[GenericParamData],
        owner_arity: usize,
        args: Option<&[GenericArgData]>,
    ) -> Option<Vec<usize>> {
        let path = site.path.as_str();
        let args = args?;
        // A member's binder has no nameable argument: `Self` is the OWNER's
        // (TR01 gives v1 exactly one, on a trait's own list), and a member's
        // own parameters are positional. Judged before anything else, so a
        // named argument is never counted as a positional one. WHICH
        // refusal depends on the owner: a trait's member is told the name
        // is misplaced, an inherent owner's that there is no `Self` at all
        // — the second sentence would be false of the first program, and
        // the first sentence sends an inherent owner's reader to
        // `Owner::<Self = ...>::member`, which is refused in turn.
        let self_reason = if site.owner_is_trait {
            NamedArgReason::OwnersSelf
        } else {
            NamedArgReason::NotATrait
        };
        self.reject_named_args_because(key, Some(args), self_reason);
        let kept = self.spellable_args(mention, args, TurbofishList::Member);
        let displaced = kept.len() != args.len();
        let own = generics.iter().skip(owner_arity);
        if own
            .clone()
            .any(|param| matches!(param.kind, GenericParamKind::Const(_)))
        {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::MemberOwnConstArgs {
                    expr: key,
                    path: path.to_owned(),
                });
            self.infer_const_args_free_quiet(args);
            return None;
        }
        let expected = own
            .filter(|param| matches!(param.kind, GenericParamKind::Type))
            .count();
        if expected == 0 {
            // Nothing to spend the list on — including the empty `::<>`,
            // which is a written list and must not be silently ignored. A
            // list that held nothing but refused regions has already been
            // told what is wrong with it. `owner_hint` is the one place
            // the list may still have a home: the OWNER's binder, one
            // segment to the left.
            if !displaced || !kept.is_empty() {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::NotGeneric {
                        expr: key,
                        name: path.to_owned(),
                        owner_list_hint: site.owner_hint.clone(),
                    });
            }
            self.infer_const_args_free_quiet(args);
            return None;
        }
        if kept.len() != expected {
            if !displaced {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::MemberGenericArgCount {
                        expr: key,
                        path: path.to_owned(),
                        expected,
                        found: kept.len(),
                    });
            }
            self.infer_const_args_free_quiet(args);
            return None;
        }
        Some(kept)
    }

    /// The owner-qualified spelling of a member, for every message about
    /// its own arguments. A trait-impl member's item name is already the
    /// QUALIFIED `head::member` form (member items are keyed by it), so the
    /// bare tail is taken — otherwise `usize::n` under the head `usize`
    /// reads `usize::usize::n`.
    ///
    /// `owner_name` is a name that can be WRITTEN — the owner declaration's
    /// or the trait's, never the receiver's rendered type: a `Self.&`
    /// member reached through a borrow would otherwise be named
    /// `usize.&::peek`, a spelling no program can contain. Where the
    /// member's own item name is already the qualified form (a trait-impl
    /// member at a dot-call), the callers pass it through instead of
    /// calling this.
    fn member_path(owner_name: &str, member_name: &str) -> String {
        format!("{owner_name}::{}", bare_member_name(member_name))
    }

    /// Mint one fresh variable per member-own TYPE binder — unified with
    /// the written turbofish argument when the use site spelled one — and
    /// register them for the `forget` default bound and the cannot-infer
    /// report. The TYPE half of instantiating a member at a use site; its
    /// region twin is [`Self::member_own_region_subst`], and the two
    /// compose exactly as [`instantiate_scheme`] and [`substitute_regions`]
    /// do.
    ///
    /// `owner_arity` is where the member's own binder starts: an INHERENT
    /// member's binder is the owner's followed by its own, so everything at
    /// or past `owner_arity` is the member's (the owner's half comes off
    /// the receiver's type, via [`owner_arg_subst`]); a TRAIT-IMPL member's
    /// binder is its own alone and starts at 0.
    ///
    /// **What the written list spells: the member's own TYPE parameters, in
    /// order, and nothing else.** A member's REGIONS are per-call
    /// existentials with no use-site spelling — a region argument is always
    /// inferred — so they are not positions in this list; leaving them out
    /// is what lets `get = fn::<@b, U>(...)` be called `x.get::<usize>(...)`
    /// rather than forcing a spelling for something that has none. CONST
    /// member parameters stay reserved, and a member whose own binder
    /// declares one refuses the WHOLE list rather than spending part of it.
    #[allow(clippy::too_many_arguments)]
    fn member_own_type_subst(
        &mut self,
        key: ExprId,
        mention: ExprId,
        member_loc: &ItemLoc,
        sig: &Ty,
        generics: &[GenericParamData],
        owner_arity: usize,
        member_args: &mut Option<&[GenericArgData]>,
        site: &MemberSite,
    ) -> FxHashMap<u32, Ty> {
        let type_slots: Vec<u32> = member_own_type_slots(generics, owner_arity);
        // TWO of the three spend sites (inherent and trait-impl members;
        // the third is a requirement, in `instantiate_requirement_sig`).
        // TAKING the list here is what tells the entry-point wrapper it was
        // spent, so nothing below has to remember to say so.
        let written = member_args.take();
        let matched = self.matched_member_args(key, mention, site, generics, owner_arity, written);
        let mut subst: FxHashMap<u32, Ty> = FxHashMap::default();
        let mut pending: Vec<(String, Ty, bool)> = Vec::new();
        for (position, &index) in type_slots.iter().enumerate() {
            let param = &generics[index as usize];
            let var = self.fresh_var();
            if let Some(at) = matched.as_ref().map(|kept| kept[position])
                && let Some(arg) = written.map(|args| &args[at])
            {
                self.spend_member_arg(key, mention, at, &param.name, arg, &var);
            }
            pending.push((param.name.clone(), var.clone(), param.without_forget));
            subst.insert(index, var);
        }
        if !sig.contains_error() && !pending.is_empty() {
            self.pending_instantiations.push(PendingInstantiation {
                expr: key,
                item: member_loc.clone(),
                owner: site.path.clone(),
                spelling: site.spelling.clone(),
                params: pending,
            });
        }
        // The member's own bounds become obligations of this call. Slots
        // over the OWNER's half of the binder find no variable in `subst`
        // and are skipped — an owner's bounds are the type declaration's
        // business, checked where the receiver was built.
        self.push_bound_obligations(key, generics, &subst);
        subst
    }

    /// Mint one fresh EXISTENTIAL per member-own REGION binder, and relate
    /// the member's own declared outlives bounds between them — the region
    /// half of instantiating an inherent member at a call site.
    ///
    /// An inherent member's binder is the owner's followed by its own (see
    /// [`crate::item_data`]), so everything at or past `owner_arity` is the
    /// member's. Minting here is not an optimisation: without it the
    /// member's rigid `Region::Param`s survive into the CALLER's body,
    /// where the outlives solver reads a region's binder index as a node
    /// number — so a member's `@b` at index 0 would silently BE the
    /// caller's universal at index 0, and two calls in one body would share
    /// one region. Fresh existentials are what make a member's region
    /// per-call, which is the whole reason the binder exists.
    fn member_own_region_subst(
        &mut self,
        key: ExprId,
        member_loc: &ItemLoc,
        owner_arity: usize,
    ) -> FxHashMap<u32, Region> {
        let generics = item_generics(self.db, member_loc.to_id(self.db)).to_vec();
        let mut region_subst: FxHashMap<u32, Region> = FxHashMap::default();
        for (index, param) in generics.iter().enumerate().skip(owner_arity) {
            if matches!(param.kind, GenericParamKind::Region) {
                region_subst.insert(index as u32, self.fresh_region());
            }
        }
        self.push_region_binder_bounds(&generics, &region_subst, key);
        region_subst
    }

    /// A binder's declared outlives bounds (`fn::<@b, @c: @b>`) become
    /// obligations at the site that instantiates it, between the regions
    /// standing for its params there — one reading, shared by free-fn
    /// mentions, inherent members and trait requirements, so no spelling
    /// can silently drop a bound the callee wrote.
    ///
    /// `region_subst` maps a region param's binder index to its region at
    /// this site; a param with no entry is skipped, and so is a bound
    /// naming one (an inherent member's binder is prefixed by the owner's,
    /// which the receiver's type supplies rather than this site).
    fn push_region_binder_bounds(
        &mut self,
        generics: &[GenericParamData],
        region_subst: &FxHashMap<u32, Region>,
        key: ExprId,
    ) {
        for (index, param) in generics.iter().enumerate() {
            if !matches!(param.kind, GenericParamKind::Region) {
                continue;
            }
            let Some(sup) = region_subst.get(&(index as u32)).cloned() else {
                continue;
            };
            for bound in &param.outlives {
                let sub = generics
                    .iter()
                    .position(|other| other.name == *bound)
                    .and_then(|i| region_subst.get(&(i as u32)).cloned());
                if let Some(sub) = sub {
                    self.push_outlives(sup.clone(), sub, key, RegionConstraintReason::CalleeBound);
                }
            }
        }
    }

    /// Where a member's `Self` sits (structural: its LAST parameter is
    /// `Self`-typed, or a SAFE borrow of `Self`), or `None` when the member
    /// has no dot-callable shape at all.
    ///
    /// A raw pointer to `Self` is deliberately NOT a self position: `.&raw`
    /// is not a decayed borrow, no raw borrow is ever inserted, and there is
    /// no reborrow relation to check it against.
    fn self_position(&self, member: &ItemLoc) -> Option<SelfPosition> {
        member_self_position(self.db, member.to_id(self.db))
    }

    /// The precise refusal when a member EXISTS but this receiver cannot
    /// reach it — [`receiver_takes`]'s table read backwards, and the one
    /// place that reading lives. Shared by the concrete-receiver path and
    /// the bound-directed one so a rigid receiver never gets different
    /// advice from a nominal one; `member` is what the diagnostic points at
    /// (the member itself concretely, the trait for a bound-directed call).
    fn receiver_shape_refusal(
        &self,
        expr: ExprId,
        callee: ExprId,
        name: &str,
        receiver_shape: ReceiverShape,
        position: Option<SelfPosition>,
        resolved: &Ty,
        member: ItemLoc,
    ) -> InferenceDiagnostic {
        match (receiver_shape, position) {
            // An owned receiver, a borrow `Self`: NOT auto-ref (clause 2
            // forbids borrowing the local) — write the borrow.
            (ReceiverShape::Owned, Some(SelfPosition::Borrow { mutable })) => {
                InferenceDiagnostic::MemberWantsBorrowReceiver {
                    expr,
                    name: name.to_owned(),
                    mutable,
                    member,
                }
            }
            // A shared receiver, an exclusive `Self`.
            (
                ReceiverShape::Borrow { mutable: false },
                Some(SelfPosition::Borrow { mutable: true }),
            ) => InferenceDiagnostic::MemberWantsExclusiveReceiver {
                expr,
                name: name.to_owned(),
                receiver_ty: resolved.clone(),
                member,
            },
            // A borrow receiver, a value `Self`: that is auto-deref,
            // sealed. Reported on the CALLEE, exactly where the field path
            // has always reported it, so the squiggle and the MIR value
            // trap do not move now that this branch reaches it first.
            (ReceiverShape::Borrow { .. }, Some(SelfPosition::Value)) => {
                InferenceDiagnostic::DotThroughBorrow {
                    expr: callee,
                    name: name.to_owned(),
                    receiver_ty: resolved.clone(),
                }
            }
            // No self position at all: the structural shape is absent.
            _ => InferenceDiagnostic::NotDotCallable {
                expr,
                name: name.to_owned(),
                member,
            },
        }
    }

    /// Whether a member takes a dot-call from a receiver of this shape —
    /// the structural test of [`crate::ty::member_self_position`], narrowed
    /// by [`receiver_takes`].
    fn member_takes_receiver(&self, member: &ItemLoc, receiver: ReceiverShape) -> bool {
        self.self_position(member)
            .is_some_and(|position| receiver_takes(receiver, position))
    }

    /// Every trait that could answer `recv.name(...)` on a CONCRETE
    /// receiver (impl-directed, TR01 extended): the trait declares `name`
    /// AND is implemented for the receiver. In file order; whether each
    /// one can actually take the call is on the candidate.
    fn trait_call_candidates(
        &self,
        resolved: &Ty,
        name: &str,
        receiver: ReceiverShape,
    ) -> Vec<TraitCallCandidate> {
        let Some(self_key) = crate::traits::SelfKey::for_ty(resolved) else {
            return Vec::new();
        };
        let impls = crate::traits::trait_impls(self.db, self.file);
        crate::traits::traits_providing_member(self.db, self.file, &self_key, name)
            .into_iter()
            .map(|(trait_loc, member_index)| {
                let member = impls
                    .impl_for(&trait_loc, &self_key)
                    .and_then(|site| crate::traits::impl_dict_members(self.db, &trait_loc, site))
                    .and_then(|members| members.get(member_index as usize).cloned());
                let broken = member
                    .as_ref()
                    .is_some_and(|loc| signature(self.db, loc.to_id(self.db)).contains_error());
                let dot_callable = !broken
                    && member
                        .as_ref()
                        .is_some_and(|loc| self.member_takes_receiver(loc, receiver));
                TraitCallCandidate {
                    trait_: trait_loc,
                    member,
                    dot_callable,
                    broken,
                }
            })
            .collect()
    }

    /// Call a TRAIT-IMPL member on a concrete receiver: like
    /// [`Self::infer_member_call`], with the member's OWN binder
    /// instantiated at the written turbofish where there is one and fresh
    /// otherwise, and its bounds becoming obligations of this call.
    #[allow(clippy::too_many_arguments)]
    fn infer_impl_member_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        receiver_ty: &Ty,
        member_loc: ItemLoc,
        name: &str,
        member_args: &mut Option<&[GenericArgData]>,
        args: &[ExprId],
        callee_expectation: &Ty,
        receiver_shape: ReceiverShape,
    ) -> Ty {
        let member_id = member_loc.to_id(self.db);
        let sig = signature(self.db, member_id);
        if sig.contains_error() {
            // A broken member definition: the definition site carries the
            // diagnostic; refusing here would double-report. The call
            // still traps through the member-matching diagnostics.
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        // The SAME rule the candidate filter used — one statement of G14's
        // structural test plus `receiver_takes`, so a candidate can never
        // be admitted here and refused there (or the reverse).
        if !self.member_takes_receiver(&member_loc, receiver_shape) {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NotDotCallable {
                    expr,
                    name: name.to_owned(),
                    member: member_loc,
                });
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        let generics = item_generics(self.db, member_id).to_vec();
        let inst = self.instantiate_member_own_binder(
            expr,
            callee,
            &member_loc,
            sig,
            &generics,
            member_args,
            // Already the qualified `head::member` spelling — and the head
            // is the one the impl WROTE, which the receiver's rendered type
            // is not (a `Self.&` member reached through a borrow would be
            // named `usize.&::peek`).
            member_loc.display_name().to_owned(),
            TurbofishSpelling::Dot,
        );
        self.result.member_of_expr.insert(expr, member_loc);
        self.finish_receiver_call(
            expr,
            callee,
            receiver,
            receiver_ty,
            inst,
            callee_expectation,
            args,
        )
    }

    /// Instantiate a member's OWN generic binder — at the written
    /// turbofish where the use site spelled one, fresh otherwise — pushing
    /// its bounds as obligations of `key` and its params for the
    /// cannot-infer report. A TRAIT-IMPL member's binder is its own alone,
    /// so it starts at index 0 — asserted below, not assumed.
    ///
    /// Both call sites reach a member through a TRAIT (an impl's, at a
    /// dot-call; or the named-Self value form), which is what
    /// [`MemberSite::owner_is_trait`] is set from here rather than passed.
    #[allow(clippy::too_many_arguments)]
    fn instantiate_member_own_binder(
        &mut self,
        key: ExprId,
        mention: ExprId,
        member_loc: &ItemLoc,
        sig: Ty,
        generics: &[GenericParamData],
        member_args: &mut Option<&[GenericArgData]>,
        path: String,
        spelling: TurbofishSpelling,
    ) -> Ty {
        let site = MemberSite {
            path,
            spelling,
            owner_is_trait: true,
            owner_hint: None,
        };
        if generics.is_empty() {
            // A member with no binder at all, handed arguments: the shared
            // binder-less refusal, named the way every other one is.
            self.refuse_member_args(key, mention, &site.path, member_args);
            return sig;
        }
        // A TRAIT-IMPL member's binder is its own alone — its owner is
        // non-generic by TR03 (impls attach to nominal types only), so
        // nothing precedes it. ASSERTED rather than assumed: the day
        // impls attach to generic types, this `0` is where the owner's
        // half would have to come from.
        debug_assert!(
            crate::member_owner(self.db, member_loc.to_id(self.db))
                .map(|owner| item_generics(self.db, owner).is_empty())
                .unwrap_or(true),
            "a trait-impl member's owner must be non-generic for owner_arity 0"
        );
        let subst = self.member_own_type_subst(
            key,
            mention,
            member_loc,
            &sig,
            generics,
            0,
            member_args,
            &site,
        );
        // Regions too, and for the same reason a free fn's are minted at
        // its mention: left rigid, a member's `@b` survives into the
        // CALLER's body, where the outlives solver reads a region param's
        // binder index as a node number.
        let region_subst = self.member_own_region_subst(key, member_loc, 0);
        let inst = instantiate_scheme(&sig, member_loc, &subst, &FxHashMap::default());
        substitute_regions(&inst, member_loc, &region_subst)
    }

    /// A dot-call on a RIGID receiver: bound-directed resolution (TR07 —
    /// bounds are the only re-opener). The call lowers as an indirect
    /// call through the enclosing body's hidden dictionary parameter.
    #[allow(clippy::too_many_arguments)]
    fn infer_bound_member_dot_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        receiver: ExprId,
        receiver_ty: &Ty,
        param: &crate::ty::ParamTy,
        name: &str,
        member_args: &mut Option<&[GenericArgData]>,
        args: &[ExprId],
        callee_expectation: &Ty,
        receiver_shape: ReceiverShape,
        resolved: &Ty,
    ) -> Ty {
        let own = self.own_item.as_ref().is_some_and(|own| param.item == *own);
        let bounds = if own {
            self.own_generics
                .get(param.index as usize)
                .map(|p| p.bounds.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let candidates =
            crate::traits::bound_traits_providing_member(self.db, self.file, &bounds, name);
        let (trait_loc, member_index) = match candidates.len() {
            1 => candidates.into_iter().next().expect("len is 1"),
            0 => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::NoSuchMember {
                        expr,
                        name: name.to_owned(),
                        receiver_ty: Ty::Param(param.clone()),
                    });
                self.result.type_of_expr.insert(callee, Ty::Error);
                self.infer_args_broken(args);
                return Ty::Error;
            }
            // Two bounds providing the same name: the same G13 ambiguity as
            // on a concrete receiver — with `Self` rigid, the escapes name
            // the param.
            _ => {
                let receiver_ty = Ty::Param(param.clone());
                let candidates = candidates
                    .iter()
                    .map(|(trait_, _)| MemberCandidate::Trait {
                        trait_name: trait_.display_name().to_owned(),
                        type_name: receiver_ty.display(),
                        short: true,
                        def: Some(trait_.clone()),
                    })
                    .collect();
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::MemberCallAmbiguity {
                        expr,
                        name: name.to_owned(),
                        receiver_ty,
                        candidates,
                    });
                self.result.type_of_expr.insert(callee, Ty::Error);
                self.infer_args_broken(args);
                return Ty::Error;
            }
        };
        // A bound-directed call reads the ROOT body's dictionary
        // parameters — out of reach from a nested fn literal or a `const`
        // block (the captured-dictionary wall, reserved).
        if self.in_nested_body() {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NestedBoundUse { expr });
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        let req = crate::item_tree::trait_requirements(self.db, trait_loc.to_id(self.db))
            .get(member_index as usize)
            .cloned();
        let Some(req) = req else {
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        };
        let Some(sig_ref) = req.sig.clone() else {
            // A requirement that isn't fully written: the trait carries
            // the diagnostic.
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        };
        // Lower the requirement's signature with `Self` as the rigid
        // receiver and the requirement's own binder fresh.
        let (inst, var_of) = self.instantiate_requirement_sig(
            expr,
            callee,
            &trait_loc,
            &req,
            &sig_ref,
            Ty::Param(param.clone()),
            member_args,
            TurbofishSpelling::Dot,
        );
        self.push_bound_obligations(expr, &req.generics, &var_of);
        // A requirement whose signature is not fn-shaped is broken at its
        // declaration, which carries that diagnostic.
        if !matches!(&inst, Ty::Fn(_)) {
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        // The SAME table the concrete path uses, over the requirement's
        // freshly-lowered signature: a requirement whose `Self` parameter
        // is a BORROW takes a borrowed rigid receiver, which reborrows into
        // it exactly as a nominal receiver does.
        let position = self_position_of(&inst, &Ty::Param(param.clone()));
        if !position.is_some_and(|position| receiver_takes(receiver_shape, position)) {
            let diagnostic = self.receiver_shape_refusal(
                expr,
                callee,
                name,
                receiver_shape,
                position,
                resolved,
                trait_loc,
            );
            self.result.diagnostics.push(diagnostic);
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return Ty::Error;
        }
        self.result.bound_member_of_expr.insert(
            expr,
            BoundMemberCall {
                param_index: param.index,
                trait_: trait_loc,
                member_index,
                receiver_appended: true,
            },
        );
        self.finish_receiver_call(
            expr,
            callee,
            receiver,
            receiver_ty,
            inst,
            callee_expectation,
            args,
        )
    }

    /// Lower a requirement's signature with `Self` bound to `self_ty` and
    /// the requirement's own binder instantiated — at the written member
    /// turbofish where the use site spelled one, fresh otherwise; returns
    /// the instantiated fn type and the binder-index → variable map (for
    /// obligations). The fresh vars register for the cannot-infer report,
    /// blamed on the trait.
    #[allow(clippy::too_many_arguments)]
    fn instantiate_requirement_sig(
        &mut self,
        key: ExprId,
        mention: ExprId,
        trait_loc: &ItemLoc,
        req: &crate::item_tree::TraitRequirement,
        sig_ref: &TypeRef,
        self_ty: Ty,
        member_args: &mut Option<&[GenericArgData]>,
        spelling: TurbofishSpelling,
    ) -> (Ty, FxHashMap<u32, Ty>) {
        // THE THIRD spend site: a bound-directed dot-call and a qualified
        // trait call both instantiate the REQUIREMENT, and both may spell
        // its own type arguments. A requirement's owner is a trait by
        // construction, so its site says so.
        let site = MemberSite {
            path: Self::member_path(trait_loc.display_name(), &req.name),
            spelling,
            owner_is_trait: true,
            owner_hint: None,
        };
        let written = member_args.take();
        let matched = self.matched_member_args(key, mention, &site, &req.generics, 0, written);
        let mut scope = crate::ty::ParamScope::default();
        scope.types.insert("Self".to_owned(), self_ty);
        let mut var_of: FxHashMap<u32, Ty> = FxHashMap::default();
        let mut pending: Vec<(String, Ty, bool)> = Vec::new();
        // A requirement's signature is LOWERED fresh rather than
        // substituted, so a written argument is joined onto the variable the
        // scope carries — the same variable the lowering then reads.
        for (position, &index) in member_own_type_slots(&req.generics, 0).iter().enumerate() {
            let gp = &req.generics[index as usize];
            let var = self.fresh_var();
            if let Some(at) = matched.as_ref().map(|kept| kept[position])
                && let Some(arg) = written.map(|args| &args[at])
            {
                self.spend_member_arg(key, mention, at, &gp.name, arg, &var);
            }
            if gp.name.is_empty() {
                continue;
            }
            scope.types.insert(gp.name.clone(), var.clone());
            pending.push((gp.name.clone(), var.clone(), gp.without_forget));
            var_of.insert(index, var);
        }
        // A requirement's REGION params are existentials of THIS call. The
        // signature is lowered fresh rather than substituted, so the fresh
        // region goes into the scope the lowering reads; without it the
        // name resolves to nothing and the borrow's region is `Error`.
        let mut region_subst: FxHashMap<u32, Region> = FxHashMap::default();
        for (index, gp) in req.generics.iter().enumerate() {
            if matches!(gp.kind, GenericParamKind::Region) && !gp.name.is_empty() {
                let region = self.fresh_region();
                scope.regions.insert(gp.name.clone(), region.clone());
                region_subst.insert(index as u32, region);
            }
        }
        self.push_region_binder_bounds(&req.generics, &region_subst, key);
        if !pending.is_empty() {
            self.pending_instantiations.push(PendingInstantiation {
                expr: key,
                item: trait_loc.clone(),
                owner: site.path.clone(),
                spelling: site.spelling.clone(),
                params: pending,
            });
        }
        let inst = lower_type_ref_in(self.db, self.file, sig_ref, self.table, &scope);
        (inst, var_of)
    }

    /// A qualified trait call (TR01), in either form: the requirement's
    /// signature is the contract, and resolution is impl-directed (concrete
    /// `Self`) or bound-directed (rigid `Self`). The short form
    /// `Trait::member(args...)` infers `Self` from the arguments; the full
    /// named-Self form `Trait::<Self = Type>::member(args...)` states it —
    /// which is what disambiguates a collision, and the only spelling where
    /// no argument determines `Self` at all.
    #[allow(clippy::too_many_arguments)]
    fn infer_qualified_trait_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        trait_loc: ItemLoc,
        member: &str,
        vp_args: Option<&[GenericArgData]>,
        member_args: &mut Option<&[GenericArgData]>,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        let callee_expectation = self.fresh_var();
        self.result
            .expectation_of_expr
            .insert(callee, callee_expectation.clone());
        // A RESERVED generic trait: nothing on it may go semantically
        // live (reserved for generic traits).
        if crate::traits::trait_is_generic(self.db, &trait_loc) {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::GenericTraitReserved {
                    expr: callee,
                    name: trait_loc.display_name().to_owned(),
                });
            self.result.type_of_expr.insert(callee, Ty::Error);
            if let Some(vp_args) = vp_args {
                self.infer_const_args_free(vp_args);
            }
            self.infer_args_broken(args);
            return self.finish_dot_call(expr, Ty::Error, expected, cause);
        }
        // `Trait::<Self = Type>::member(...)` — the written implementer.
        let named_self = self.trait_path_self_arg(callee, &trait_loc, vp_args);
        let requirements = crate::item_tree::trait_requirements(self.db, trait_loc.to_id(self.db));
        let Some(member_index) = requirements.iter().position(|req| req.name == member) else {
            if !member.is_empty() {
                self.no_such_trait_member(callee, &trait_loc, member);
            }
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return self.finish_dot_call(expr, Ty::Error, expected, cause);
        };
        // `Display::fmt::<W>(...)` — the requirement's own binder applied
        // at the use site. Spent inside `instantiate_requirement_sig`
        // below, exactly as the dot-call form spends it; only a requirement
        // that declares a still-reserved CONST parameter refuses the list,
        // and that refusal comes from the shared `matched_member_args`.
        let req = requirements[member_index].clone();
        let Some(sig_ref) = req.sig.clone() else {
            // The requirement isn't fully written: the trait carries the
            // diagnostic.
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return self.finish_dot_call(expr, Ty::Error, expected, cause);
        };
        // The named form PINS `Self`; the short form leaves a variable the
        // receiver-like argument pass determines below.
        let self_var = match named_self {
            Some(self_ty) => self_ty,
            None => self.fresh_var(),
        };
        let (inst, var_of) = self.instantiate_requirement_sig(
            expr,
            callee,
            &trait_loc,
            &req,
            &sig_ref,
            self_var.clone(),
            member_args,
            TurbofishSpelling::Path,
        );
        self.push_bound_obligations(expr, &req.generics, &var_of);
        let inst_ty = self.check(callee, inst.clone(), &callee_expectation, None);
        self.result.type_of_expr.insert(callee, inst_ty);
        let Ty::Fn(f) = inst else {
            self.result.type_of_expr.insert(callee, Ty::Error);
            self.infer_args_broken(args);
            return self.finish_dot_call(expr, Ty::Error, expected, cause);
        };
        if f.params.len() != args.len() {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::ArgCountMismatch {
                    expr,
                    expected: f.params.len(),
                    found: args.len(),
                });
        }
        // Arguments at literal-`Self` positions are RECEIVER-LIKE: they
        // are inferred freely first, `Self` is determined from them
        // (variants widening to their ENUM — never letting `Self` bind to
        // a tag-free variant type), and then each is checked against the
        // determined `Self` so the ordinary variant→enum conversion is
        // recorded per argument. Everything else checks against its
        // instantiated parameter directly.
        let self_positions: Vec<bool> = match &sig_ref {
            TypeRef::Fn { params, .. } => params
                .iter()
                .map(|param| matches!(param, TypeRef::Path(name) if name == "Self"))
                .collect(),
            _ => Vec::new(),
        };
        let mut self_args: Vec<(ExprId, Ty)> = Vec::new();
        for (i, &arg) in args.iter().enumerate() {
            if self_positions.get(i).copied().unwrap_or(false) {
                let fresh = self.fresh_var();
                let ty = self.infer_expr(arg, &fresh);
                self_args.push((arg, ty));
            } else {
                let param_ty = f.params.get(i).cloned().unwrap_or(Ty::Error);
                self.infer_expr_with(arg, &param_ty, Some(Cause::CallSite { call: expr, arg }));
            }
        }
        for (_, ty) in &self_args {
            if !matches!(self.resolve_shallow(&self_var), Ty::Infer(_)) {
                break;
            }
            let candidate = match self.resolve_shallow(ty) {
                Ty::Variant(variant) => Ty::Named(NamedTy {
                    decl: variant.decl,
                    args: variant.args,
                }),
                Ty::Infer(_) => continue,
                other => other,
            };
            self.adopt(&self_var, &candidate);
        }
        for (arg, ty) in &self_args {
            self.check(
                *arg,
                ty.clone(),
                &self_var,
                Some(Cause::CallSite {
                    call: expr,
                    arg: *arg,
                }),
            );
        }
        // `Self` is inferred from the arguments (TR01's short form) —
        // resolved eagerly, right after they were checked.
        let self_resolved = self.resolve_shallow(&self_var);
        match &self_resolved {
            broken if broken.contains_error() => {}
            Ty::Param(p) => {
                let own = self.own_item.as_ref().is_some_and(|own| p.item == *own);
                let has_bound = own
                    && self.own_generics.get(p.index as usize).is_some_and(|data| {
                        data.bounds.iter().any(|bound| {
                            crate::traits::bound_trait(self.db, self.file, bound).as_ref()
                                == Some(&trait_loc)
                        })
                    });
                if has_bound {
                    if self.in_nested_body() {
                        // The dictionary lives in the ROOT body — nested
                        // code would have to capture it (reserved).
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NestedBoundUse { expr });
                    } else {
                        self.result.bound_member_of_expr.insert(
                            expr,
                            BoundMemberCall {
                                param_index: p.index,
                                trait_: trait_loc,
                                member_index: member_index as u32,
                                receiver_appended: false,
                            },
                        );
                    }
                } else {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::UnsatisfiedBound {
                            expr,
                            param: p.name.to_string(),
                            trait_: trait_loc,
                            ty: self_resolved.clone(),
                        });
                }
            }
            Ty::Infer(_) => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::CannotInferSelf {
                        expr,
                        trait_name: trait_loc.display_name().to_owned(),
                        member: member.to_owned(),
                    });
            }
            concrete => {
                // NOTE: a VARIANT-typed Self can only still appear here
                // when a nested-`Self` position (never a literal one — the
                // receiver-like pass above widens those) pinned it — a
                // variant has no impls of its own (`SelfKey::for_ty` is
                // `None`), so it lands on the sound `NoTraitImpl` below.
                let resolved_impl = crate::traits::SelfKey::for_ty(concrete).and_then(|key| {
                    let impls = crate::traits::trait_impls(self.db, self.file);
                    impls
                        .impl_for(&trait_loc, &key)
                        .cloned()
                        .and_then(|site| {
                            crate::traits::impl_dict_members(self.db, &trait_loc, &site)
                        })
                        .and_then(|members| members.get(member_index).cloned())
                });
                match resolved_impl {
                    Some(member_loc) => {
                        self.result
                            .qualified_member_of_expr
                            .insert(expr, member_loc);
                    }
                    None => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NoTraitImpl {
                                expr,
                                trait_: trait_loc,
                                ty: concrete.clone(),
                            });
                    }
                }
            }
        }
        self.finish_dot_call(expr, f.ret.clone(), expected, cause)
    }

    /// Enforcement for a place-chain assignment target (`p.x = e;`,
    /// `p.a.b = e;`, `a[i] = e;`, `m[0][1].x = e;`): assignability is
    /// Rust's transitivity rule — legal exactly when the ROOT binding is
    /// `mut`; outer immutability implies inner immutability and vice
    /// versa, with no per-field (or per-element) `mut`. The chain was
    /// already inferred as an ordinary read (unknown fields, non-record
    /// receivers and non-array bases got their squiggles there), so only
    /// the root is ruled on here. Returns the cause the RHS check should
    /// cite: the root binding's annotation when it has one — its type
    /// spells the field's/element's type out, the same axiom
    /// record-literal field checking flows down — and nothing otherwise
    /// (the inferred-from-initializer fallback hint would claim the
    /// *binding* has the projected type).
    fn check_field_assign_target(&mut self, target: ExprId) -> Option<Cause> {
        let mut segments: Vec<String> = Vec::new();
        let mut root = target;
        loop {
            match &self.body.exprs[root] {
                ExprData::Field { receiver, name, .. } => {
                    segments.push(format!(".{name}"));
                    root = *receiver;
                }
                // The index expression has no stable rendering here
                // (message texts are range-free); `[_]` says "an element".
                ExprData::Index { base, .. } => {
                    segments.push("[_]".to_owned());
                    root = *base;
                }
                _ => break,
            }
        }
        let ExprData::NameRef(root_name) = &self.body.exprs[root] else {
            // A deref roots the chain (`p.* = e;`, `p.*.x = e;`, or
            // `bb.*.* = e;`): the write goes through the pointers, so the
            // judgement is [`InferCtx::shared_step_governing`]'s — every
            // step of the chain must grant a write permission, not just
            // the outermost one. No binding-`mut` rule applies: writing
            // through a pointer reassigns nothing.
            if matches!(self.body.exprs[root], ExprData::Deref { .. }) {
                if let Some(ty) = self.shared_step_governing(root) {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::AssignThroughShared { target: root, ty });
                }
                return None;
            }
            // A chain rooted in a non-variable: validation already
            // squiggled the whole target ("can only assign to a variable
            // or its fields"); a missing root is a parse error. No second
            // squiggle either way.
            return None;
        };
        segments.push(root_name.clone());
        segments.reverse();
        let place = segments.concat();
        // The same target taxonomy as a plain-name assignment, judged at
        // the root; the silent arms match its reasoning too (the root read
        // already carries `TypeNotValue` / unresolved-name / duplicate
        // diagnostics).
        match self.resolutions.get(root) {
            Some(Resolution::Local(binding)) => {
                let data = &self.body.bindings[*binding];
                if !data.mutable {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::AssignToImmutable {
                            target: root,
                            binding: *binding,
                            name: data.name.clone(),
                            place,
                        });
                }
                data.type_ref.is_some().then_some(Cause::Binding(*binding))
            }
            // A const param is a compile-time value, not a place — and it
            // has no fields anyway; the root is what's judged, as for
            // locals.
            Some(Resolution::ConstParam(index)) => {
                let name = self
                    .own_generics
                    .get(*index as usize)
                    .map(|param| param.name.clone())
                    .unwrap_or_default();
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::AssignToConstParam { target: root, name });
                None
            }
            Some(Resolution::Item(loc)) => {
                let constness = crate::item_data(self.db, loc.to_id(self.db))
                    .as_ref()
                    .and_then(|it| it.kind.constness())
                    .unwrap_or(Constness::Static);
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::AssignToItem {
                        target: root,
                        item: loc.clone(),
                        constness,
                    });
                None
            }
            Some(Resolution::TypeItem(_) | Resolution::TraitItem(_)) => None,
            Some(Resolution::Builtin(builtin)) => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::AssignToBuiltin {
                        target: root,
                        builtin: *builtin,
                    });
                None
            }
            Some(Resolution::Ambiguous(_)) | None => None,
        }
    }

    /// The place rules for `place.&raw` / `place.&raw mut`, judged on the
    /// operand's structure after it was read-typed: the accepted places
    /// are a variable, a chain of its fields and elements, a
    /// `static`/`const` item (a `const` use's own copy — const=copied,
    /// now observable), or a deref-rooted chain (`p.*.x.&raw mut` — a
    /// pointer into the pointee, the original allocation's address with
    /// an extended path). The `mut` flavor additionally requires the ROOT
    /// binding to be `mut` — the same transitive-mutability rule
    /// assignments use — refuses items (`static mut` is deferred; a
    /// `const` has no place to hand out mutably), and, for deref-rooted
    /// places, requires every step the chain travels through to grant a
    /// write permission ([`InferCtx::shared_step_governing`]).
    fn check_addr_of_place(&mut self, addr_of: ExprId, mutable: bool, place: ExprId) {
        // The whole chain's write permission, needed only by the `mut`
        // flavor: a shared address of a shared place is what `.&raw` is
        // for.
        let governing = mutable.then(|| self.shared_step_governing(place)).flatten();
        let mut segments: Vec<String> = Vec::new();
        let mut root = place;
        loop {
            match &self.body.exprs[root] {
                ExprData::Field { receiver, name, .. } => {
                    segments.push(format!(".{name}"));
                    root = *receiver;
                }
                // The index expression has no stable rendering here
                // (message texts are range-free); `[_]` says "an element".
                ExprData::Index { base, .. } => {
                    segments.push("[_]".to_owned());
                    root = *base;
                }
                _ => break,
            }
        }
        match &self.body.exprs[root] {
            ExprData::NameRef(root_name) => {
                segments.push(root_name.clone());
                segments.reverse();
                let place_str = segments.concat();
                match self.resolutions.get(root) {
                    Some(Resolution::Local(binding)) => {
                        let data = &self.body.bindings[*binding];
                        if mutable && !data.mutable {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::AddrOfMutImmutable {
                                    addr_of,
                                    root,
                                    binding: *binding,
                                    name: data.name.clone(),
                                    place: place_str,
                                });
                        }
                    }
                    Some(Resolution::Item(loc)) => {
                        if mutable {
                            let constness = crate::item_data(self.db, loc.to_id(self.db))
                                .as_ref()
                                .and_then(|it| it.kind.constness())
                                .unwrap_or(Constness::Static);
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::AddrOfMutItem {
                                    addr_of,
                                    root,
                                    item: loc.clone(),
                                    constness,
                                });
                        }
                    }
                    // A compile-time value, a type, a trait, a builtin:
                    // none of them is a place.
                    Some(
                        Resolution::ConstParam(_)
                        | Resolution::TypeItem(_)
                        | Resolution::TraitItem(_)
                        | Resolution::Builtin(_),
                    ) => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::AddrOfNonPlace { expr: addr_of });
                    }
                    // Unresolved/ambiguous roots carry their own
                    // diagnostics from the read.
                    Some(Resolution::Ambiguous(_)) | None => {}
                }
            }
            // `p.*....&raw [mut]`: a pointer into the pointee — the
            // result is the original allocation's address with an
            // extended path. The `mut` flavor is judged by
            // [`InferCtx::shared_step_governing`]: a shared step anywhere
            // in the chain, `T.&raw` or `T.&`, must not launder into a
            // write permission.
            ExprData::Deref { .. } => {
                if let Some(ty) = governing {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::AddrOfMutThroughShared { addr_of, ty });
                }
            }
            // Broken source: the parse error covers it.
            ExprData::Missing => {}
            _ => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::AddrOfNonPlace { expr: addr_of });
            }
        }
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
    ///
    /// `member_args` are the SECOND segment's own written arguments
    /// (`Option::map::<bool>`). They are never merged into `args`, and
    /// what happens to them is decided by what the segment turned out to
    /// NAME — a MEMBER spends them on its own binder, a VARIANT never can
    /// (a variant has no binder; its arguments are the owner's, misplaced)
    /// — which is precisely the question this function answers and the
    /// parser cannot. Passed by `&mut` so the two spending arms can TAKE
    /// them: see the caller's wrapper for why that is the mechanism.
    fn infer_variant_path(
        &mut self,
        expr: ExprId,
        base: ExprId,
        variant: &str,
        args: Option<&[GenericArgData]>,
        member_args: &mut Option<&[GenericArgData]>,
    ) -> Ty {
        // A MEMBER's own arguments are spendable (its type binder is live);
        // a VARIANT's are a mistake for the owner's and never are. Only the
        // member arms below TAKE `member_args`; every other exit leaves it
        // for the caller's wrapper, which is what makes "every const
        // argument gets a type, exactly once" hold without a call at each
        // of this function's nine exits.
        let has_member_args = member_args.is_some();
        match self.resolutions.get(base) {
            Some(Resolution::TypeItem(loc)) => {
                let loc = loc.clone();
                let item = loc.to_id(self.db);
                let Some(variants) = enum_variants(self.db, item).as_ref() else {
                    // A struct type has no variants; a broken declaration
                    // carries its own diagnostics (infectious, silent).
                    // `Type::member` naming an inherent member is the G13
                    // escape — the type's own member, as a plain fn value.
                    if let Some(member) = self.member_of(&loc, variant) {
                        return self.infer_qualified_member_value(
                            expr,
                            &loc,
                            member,
                            args,
                            member_args,
                        );
                    }
                    if self.push_trait_member_on_type(expr, &loc, variant) {
                        self.infer_const_args_free(args.unwrap_or(&[]));
                        return Ty::Error;
                    }
                    // The name IS declared — as a FIELD. The field/member
                    // namespace split puts fields on VALUES and members on
                    // the type, so `P::f` is looking in the wrong namespace
                    // rather than at nothing at all: say which one, and
                    // name the spelling that works. The generic "no
                    // variants" answer below is for names that genuinely
                    // aren't there.
                    if self.decl_has_field(item, variant) {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::QualifiedPathIsField {
                                expr,
                                item: loc.clone(),
                                name: variant.to_owned(),
                            });
                        self.infer_const_args_free(args.unwrap_or(&[]));
                        return Ty::Error;
                    }
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
                    self.infer_const_args_free(args.unwrap_or(&[]));
                    return Ty::Error;
                };
                // `Shape::` — the parse error covers the missing name.
                if variant.is_empty() {
                    self.infer_const_args_free(args.unwrap_or(&[]));
                    return Ty::Error;
                }
                match variants.iter().position(|(name, _)| name == variant) {
                    Some(index) => {
                        // `Shape::Circle::<usize>` — a variant carries no
                        // binder of its own and never will: the arguments
                        // are the ENUM's, written one segment too late.
                        if has_member_args {
                            let suggest_owner_list =
                                args.is_none() && !item_generics(self.db, item).is_empty();
                            self.result.diagnostics.push(
                                InferenceDiagnostic::VariantOwnGenericArgs {
                                    expr,
                                    owner: loc.display_name().to_owned(),
                                    variant: variant.to_owned(),
                                    suggest_owner_list,
                                },
                            );
                            self.infer_const_args_free(args.unwrap_or(&[]));
                            return Ty::Error;
                        }
                        // Instantiate the ENUM mention: written turbofish
                        // args are checked, unwritten type params get fresh
                        // variables that the payload (or the expectation
                        // the value meets) pins by ordinary unification.
                        let named = self.instantiate_type_mention(expr, &loc, args);
                        let variant_ty = VariantTy {
                            decl: loc.clone(),
                            args: named.args,
                            index: index as u32,
                            name: std::sync::Arc::from(variant),
                        };
                        self.result.variant_of_expr.insert(expr, variant_ty.clone());
                        let payload = &variants[index].1;
                        if payload.is_empty() {
                            Ty::Variant(variant_ty)
                        } else {
                            let payload = payload
                                .iter()
                                .map(|ty| substitute_args(ty, &loc, &variant_ty.args))
                                .collect();
                            Ty::fn_type(payload, Ty::Variant(variant_ty))
                        }
                    }
                    None => {
                        // An enum's `Type::member` reference is the same
                        // qualified member spelling as a struct's (variants
                        // win the name — they are the enum's own second
                        // segment).
                        if let Some(member) = self.member_of(&loc, variant) {
                            return self.infer_qualified_member_value(
                                expr,
                                &loc,
                                member,
                                args,
                                member_args,
                            );
                        }
                        if !self.push_trait_member_on_type(expr, &loc, variant) {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::NoSuchVariant {
                                    expr,
                                    item: loc.clone(),
                                    name: variant.to_owned(),
                                });
                        }
                        self.infer_const_args_free(args.unwrap_or(&[]));
                        Ty::Error
                    }
                }
            }
            // `Trait::member` as a VALUE — a called short form is
            // intercepted in the `Call` arm and never reaches this. A
            // member value is IMPL-SPECIFIC, so it needs the named-Self
            // form (TR01): `Display::<Self = Foo>::fmt` is one impl's fn,
            // while a bare `Display::fmt` names no one function.
            Some(Resolution::TraitItem(loc)) => {
                let loc = loc.clone();
                if crate::traits::trait_is_generic(self.db, &loc) {
                    // A RESERVED generic trait: nothing on it may go live.
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::GenericTraitReserved {
                            expr,
                            name: loc.display_name().to_owned(),
                        });
                    self.infer_const_args_free(args.unwrap_or(&[]));
                    return Ty::Error;
                }
                if variant.is_empty() {
                    // `Trait::` — the parse error covers it.
                    self.infer_const_args_free(args.unwrap_or(&[]));
                    return Ty::Error;
                }
                let named_self = self.trait_path_self_arg(expr, &loc, args);
                let requirements =
                    crate::item_tree::trait_requirements(self.db, loc.to_id(self.db));
                if !requirements.iter().any(|req| req.name == variant) {
                    self.no_such_trait_member(expr, &loc, variant);
                    return Ty::Error;
                }
                match named_self {
                    Some(self_ty) => self.infer_named_self_member_value(
                        expr,
                        &loc,
                        variant,
                        self_ty,
                        member_args,
                    ),
                    None => {
                        self.result.diagnostics.push(
                            InferenceDiagnostic::QualifiedTraitMemberValue {
                                expr,
                                trait_name: loc.display_name().to_owned(),
                                member: variant.to_owned(),
                            },
                        );
                        Ty::Error
                    }
                }
            }
            Some(
                Resolution::Local(_)
                | Resolution::Item(_)
                | Resolution::Builtin(_)
                | Resolution::ConstParam(_),
            ) => {
                let name = match &self.body.exprs[base] {
                    ExprData::NameRef(name) => name.clone(),
                    _ => String::new(),
                };
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::VariantPathOnValue { expr, name });
                self.infer_const_args_free(args.unwrap_or(&[]));
                Ty::Error
            }
            // Duplicate definitions / an unresolved base carry their own
            // diagnostics (the base is an ordinary `NameRef` to name
            // resolution).
            Some(Resolution::Ambiguous(_)) | None => {
                self.infer_const_args_free(args.unwrap_or(&[]));
                Ty::Error
            }
        }
    }

    /// Resolve `::Variant` — the elided sigil in EXPRESSION position, the
    /// mirror of the elided-sigil variant pattern. The pattern reads the
    /// SCRUTINEE's enum; the expression reads the position's EXPECTED
    /// type, and that is the whole of the rule.
    ///
    /// Deliberately REJECT-ONLY sugar, and the three properties that make
    /// it so are worth stating together, because loosening any one turns it
    /// into an inference feature:
    ///
    /// 1. It reads `expected` and nothing else. It never runs inference
    ///    backwards, never scans siblings, never defers. A position whose
    ///    expectation is still a variable — a match arm's body, an `if`
    ///    branch, an unannotated `let` — has no enum in view and is
    ///    REFUSED, with the qualified spelling named.
    /// 2. It resolves to exactly what the qualified spelling resolves to,
    ///    `variant_of_expr` included, so everything downstream (widening,
    ///    MIR, hover) sees no difference at all.
    /// 3. The enum's generic ARGS come from the expectation rather than
    ///    from a fresh mention, which is what lets `-> Option::<V.&mut>`
    ///    accept a bare `::None`: there is no second instantiation to
    ///    reconcile.
    ///
    /// So the qualified spelling stays canonical — everything spellable
    /// here is spellable there, and this only ever removes a rejection.
    fn infer_elided_variant(&mut self, expr: ExprId, variant: &str, expected: &Ty) -> Ty {
        let resolved = self.resolve_shallow(expected);
        // A variant-typed expectation names its enum just as well; the
        // value is then checked against the precise variant afterwards by
        // the ordinary `check`, exactly as a qualified path would be.
        let named = match &resolved {
            Ty::Named(named) if enum_variants(self.db, named.decl.to_id(self.db)).is_some() => {
                named.clone()
            }
            Ty::Variant(variant) => NamedTy {
                decl: variant.decl.clone(),
                args: variant.args.clone(),
            },
            // Errors are infectious and silent.
            broken if broken.contains_error() => return Ty::Error,
            other => {
                // An unpinned expectation is "no enum in view", not "the
                // wrong type": say so with the shape that names no type.
                let expected = match other {
                    Ty::Infer(_) | Ty::UnresolvedNumber => None,
                    other => Some(other.clone()),
                };
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::ElidedVariantNoEnum {
                        expr,
                        variant: variant.to_owned(),
                        expected,
                    });
                return Ty::Error;
            }
        };
        let variants = enum_variants(self.db, named.decl.to_id(self.db));
        // A broken enum declaration carries its own diagnostic.
        let Some(variants) = variants.as_ref() else {
            return Ty::Error;
        };
        let Some(index) = variants.iter().position(|(name, _)| name == variant) else {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NoSuchVariant {
                    expr,
                    item: named.decl.clone(),
                    name: variant.to_owned(),
                });
            return Ty::Error;
        };
        let variant_ty = VariantTy {
            decl: named.decl.clone(),
            args: named.args,
            index: index as u32,
            name: std::sync::Arc::from(variant),
        };
        self.result.variant_of_expr.insert(expr, variant_ty.clone());
        let payload = &variants[index].1;
        if payload.is_empty() {
            Ty::Variant(variant_ty)
        } else {
            let payload = payload
                .iter()
                .map(|ty| substitute_args(ty, &variant_ty.decl, &variant_ty.args))
                .collect();
            Ty::fn_type(payload, Ty::Variant(variant_ty))
        }
    }

    /// `Point::len` — a qualified reference to an INHERENT member (the G13
    /// escape naming the type's own member). An inherent member is an
    /// ordinary fn whose `Self` is simply its last parameter, so the
    /// reference IS its fn value: `Point::len(p)` is an ordinary call, and
    /// `let f = Point::len;` is legal (no dictionary is involved anywhere).
    /// Written type arguments on the FIRST segment belong to the TYPE
    /// (`Pair::<usize>::first`) and those on the SECOND to the member
    /// (`Option::flat_map::<usize>`) — an inherent member's binder is the
    /// owner's followed by its own — so the mention instantiates the member
    /// exactly as a dot-call instantiates it, the same two halves written
    /// out instead of read off a receiver.
    ///
    /// TRAIT-impl members are deliberately unreachable here: their
    /// spellings are `Trait::member` and `Trait::<Self = Type>::member`, so
    /// every spelling names exactly one thing. (Member items are keyed by
    /// the qualified `head::member` name, so this lookup cannot even see
    /// them.)
    fn infer_qualified_member_value(
        &mut self,
        expr: ExprId,
        owner: &ItemLoc,
        member: ItemLoc,
        args: Option<&[GenericArgData]>,
        member_args: &mut Option<&[GenericArgData]>,
    ) -> Ty {
        let sig = signature(self.db, member.to_id(self.db));
        if sig.contains_error() {
            // The definition site carries the fully-annotated member rule's
            // diagnostic; errors are infectious and silent (the member
            // turbofish is consumed by the entry-point wrapper).
            self.infer_const_args_free(args.unwrap_or(&[]));
            return Ty::Error;
        }
        // The OWNER's binder, judged exactly like any type mention (arity,
        // kinds, const args, `_` holes), then substituted into the member's
        // scheme — member schemes are keyed by the MEMBER's `ItemLoc` at the
        // owner's binder indices.
        let named = self.instantiate_type_mention(expr, owner, args);
        let (mut subst, const_subst) = owner_arg_subst(&named.args);
        let generics = item_generics(self.db, member.to_id(self.db)).to_vec();
        // The one spelling where a list the member cannot take may still
        // have a home: `Owner::<...>::member`, one segment to the left. Not
        // offered when the owner has no binder either (that would trade one
        // refusal for another) or when the path already writes the owner's
        // list (nothing left to suggest).
        let owner_hint = (args.is_none()
            && !item_generics(self.db, owner.to_id(self.db)).is_empty())
        .then(|| owner.display_name().to_owned());
        subst.extend(self.member_own_type_subst(
            expr,
            expr,
            &member,
            &sig,
            &generics,
            named.args.len(),
            member_args,
            &MemberSite {
                path: Self::member_path(owner.display_name(), member.display_name()),
                spelling: TurbofishSpelling::Path,
                owner_is_trait: false,
                owner_hint,
            },
        ));
        // The member's OWN regions are per-MENTION existentials, exactly as
        // at a dot-call — a qualified spelling is the same instantiation
        // written differently, and leaving them rigid would launder every
        // obligation the mention incurs (see `member_own_region_subst`).
        let region_subst =
            self.member_own_region_subst(expr, &member, self.owner_binder_arity(&member));
        let inst = instantiate_scheme(&sig, &member, &subst, &const_subst);
        let inst = substitute_regions(&inst, &member, &region_subst);
        self.result.member_value_of_expr.insert(
            expr,
            QualifiedMemberValue {
                member,
                args: named.args,
            },
        );
        inst
    }

    /// `Type::member` where the TYPE has no such member but a trait
    /// provides one for it: say which trait, and how its spelling goes.
    /// Returns whether the diagnostic was pushed.
    fn push_trait_member_on_type(&mut self, expr: ExprId, loc: &ItemLoc, member: &str) -> bool {
        if member.is_empty() {
            return false;
        }
        let self_key = crate::traits::SelfKey::Decl(loc.clone());
        let traits: Vec<String> =
            crate::traits::traits_providing_member(self.db, self.file, &self_key, member)
                .into_iter()
                .map(|(trait_, _)| trait_.display_name().to_owned())
                .collect();
        if traits.is_empty() {
            return false;
        }
        self.result
            .diagnostics
            .push(InferenceDiagnostic::QualifiedTraitMemberOnType {
                expr,
                type_name: loc.display_name().to_owned(),
                member: member.to_owned(),
                traits,
            });
        true
    }

    /// The `Self = Type` argument of a qualified trait path (TR01: named,
    /// position-irrelevant), lowered. The rest of the list is judged here
    /// too: only `Self` is nameable, `Self` may be given once, and a
    /// non-generic trait takes no positional arguments of its own.
    fn trait_path_self_arg(
        &mut self,
        expr: ExprId,
        trait_loc: &ItemLoc,
        args: Option<&[GenericArgData]>,
    ) -> Option<Ty> {
        let args = args?;
        let mut self_ref: Option<TypeRef> = None;
        let mut positional = 0usize;
        // A written region is refused first, per argument, as on every
        // other list (regions are inferred at every call), and does not
        // count as a positional argument the trait "takes none" of.
        self.spellable_args(expr, args, TurbofishList::Item);
        for arg in args {
            match arg {
                GenericArgData::Region(_) => {}
                GenericArgData::Named { name, ty } if name == "Self" => {
                    if self_ref.is_some() {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NamedGenericArg {
                                expr,
                                name: name.clone(),
                                reason: NamedArgReason::Duplicate,
                            });
                        continue;
                    }
                    self_ref = Some(ty.clone());
                }
                GenericArgData::Named { name, .. } => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NamedGenericArg {
                            expr,
                            name: name.clone(),
                            reason: NamedArgReason::NotSelf,
                        });
                }
                // A trait's OWN arguments belong to generic traits
                // (reserved), and their reservation is reported before
                // this runs, so a positional argument here belongs to a
                // trait that takes none.
                GenericArgData::Type(_) => positional += 1,
                GenericArgData::Const(value) => {
                    positional += 1;
                    let fresh = self.fresh_var();
                    self.infer_expr(*value, &fresh);
                }
            }
        }
        if positional > 0 {
            self.push_not_generic(expr, trait_loc.display_name());
        }
        let self_ref = self_ref?;
        // A `_` ANYWHERE in the named `Self` argument — refused
        // STRUCTURALLY, right here, before anything downstream mints a
        // variable for it. The `Self` argument names the implementer: it is
        // the one thing that decides WHICH impl the path denotes, so a hole
        // is not an under-specified type, it is the absence of the answer
        // the form exists to give. Letting one through hands trait
        // resolution an inference variable and renders it into the failure
        // message (`_ does not implement D`, `Pair::<_> does not implement
        // E`) — an inference variable in user-facing text.
        //
        // Every position, deliberately, and strict-first:
        //
        // - In CALL position a top-level hole IS inferable from the
        //   arguments, but it says nothing the short form `D::m(v)` does
        //   not already say, so one rule beats two (the named form exists
        //   to state `Self`, not to decline to).
        // - A NESTED hole (`Self = Pair::<_>`) could in principle name the
        //   implementer and leave its arguments open — but v1's impls are
        //   all ground (coherence buckets are keyed by decl; generic-type
        //   impls are reserved), so today it can only ever
        //   reach the failure path. Allowing it is a purely additive
        //   relaxation whenever generic impls land.
        if self_ref.contains_hole() {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NamedGenericArg {
                    expr,
                    name: "Self".to_owned(),
                    reason: NamedArgReason::Hole,
                });
            // `{error}` rather than `None`: `None` means "no `Self` was
            // written", which would draw the value form's own
            // must-name-the-implementer diagnostic on top of this one.
            // Errors are infectious and silent, so every consumer below
            // goes quiet — which is the whole story here.
            return Some(Ty::Error);
        }
        Some(self.lower_type_ref(&self_ref))
    }

    /// A trait path's last segment naming no requirement: an associated
    /// type (reserved) or nothing at all.
    fn no_such_trait_member(&mut self, expr: ExprId, trait_loc: &ItemLoc, name: &str) {
        let assoc = crate::item_tree::trait_assoc_types(self.db, trait_loc.to_id(self.db))
            .iter()
            .any(|assoc| assoc == name);
        if assoc {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::AssocTypeReserved {
                    expr,
                    trait_: trait_loc.clone(),
                    name: name.to_owned(),
                });
            return;
        }
        self.result
            .diagnostics
            .push(InferenceDiagnostic::TraitHasNoMember {
                expr,
                trait_: trait_loc.clone(),
                name: name.to_owned(),
            });
    }

    /// `Display::<Self = Foo>::fmt` as a VALUE — TR01's impl-specific fn
    /// value: the named implementer's own member. A RIGID `Self` names a
    /// dictionary slot instead, which a value would have to capture
    /// (reserved, the same wall as [`InferenceDiagnostic::BoundFnValue`]).
    fn infer_named_self_member_value(
        &mut self,
        expr: ExprId,
        trait_loc: &ItemLoc,
        member: &str,
        self_ty: Ty,
        member_args: &mut Option<&[GenericArgData]>,
    ) -> Ty {
        let resolved = self.resolve_shallow(&self_ty);
        if resolved.contains_error() {
            return Ty::Error;
        }
        if matches!(resolved, Ty::Param(_)) {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::BoundMemberValue {
                    expr,
                    trait_name: trait_loc.display_name().to_owned(),
                    member: member.to_owned(),
                });
            return Ty::Error;
        }
        let requirements = crate::item_tree::trait_requirements(self.db, trait_loc.to_id(self.db));
        let Some(member_index) = requirements.iter().position(|req| req.name == member) else {
            return Ty::Error;
        };
        let member_loc = crate::traits::SelfKey::for_ty(&resolved).and_then(|key| {
            let impls = crate::traits::trait_impls(self.db, self.file);
            impls
                .impl_for(trait_loc, &key)
                .cloned()
                .and_then(|site| crate::traits::impl_dict_members(self.db, trait_loc, &site))
                .and_then(|members| members.get(member_index).cloned())
        });
        let Some(member_loc) = member_loc else {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NoTraitImpl {
                    expr,
                    trait_: trait_loc.clone(),
                    ty: resolved,
                });
            return Ty::Error;
        };
        let sig = signature(self.db, member_loc.to_id(self.db));
        if sig.contains_error() {
            // The impl member's definition site carries the diagnostic.
            return Ty::Error;
        }
        // The member's OWN binder (a requirement may be a generic fn):
        // instantiated fresh where the value is directly called; its bounds
        // would need a captured dictionary anywhere else — reserved.
        let generics = item_generics(self.db, member_loc.to_id(self.db)).to_vec();
        if !crate::traits::bound_slots(self.db, self.file, &generics).is_empty()
            && !self.direct_callees.contains(&expr)
        {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::BoundFnValue {
                    expr,
                    name: format!("{}::{member}", trait_loc.display_name()),
                });
            return Ty::Error;
        }
        let inst = self.instantiate_member_own_binder(
            expr,
            expr,
            &member_loc,
            sig,
            &generics,
            member_args,
            // Named for the spelling the use site WROTE
            // (`Peek::<Self = usize>::peek`), not for the impl it resolved
            // to.
            Self::member_path(trait_loc.display_name(), member_loc.display_name()),
            // And WRITABLE in that spelling only: a member value is
            // impl-specific, so the bare `Peek::peek` a path elsewhere
            // would carry is refused here (`QualifiedTraitMemberValue`).
            // The implementer is the one the path already named.
            TurbofishSpelling::NamedSelf(format!(
                "{}::<Self = {}>",
                trait_loc.display_name(),
                resolved.display()
            )),
        );
        self.result.member_value_of_expr.insert(
            expr,
            QualifiedMemberValue {
                member: member_loc,
                args: Vec::new(),
            },
        );
        inst
    }

    /// The value type of the enclosing binder's const param `index`
    /// (`const N: usize` — a read of `N` has type `usize`). Lowered with
    /// no param scope (dependent `const N: T` is rejected, TR06) and no
    /// inference context (a declaration has nothing to fill `_` with):
    /// any minted variable erases to `{error}`; the diagnostics pass in
    /// [`crate::file_diagnostics`] reports both cases at the declaration.
    fn const_param_value_ty(&mut self, index: u32) -> Ty {
        match self.own_generics.get(index as usize) {
            Some(GenericParamData {
                kind: GenericParamKind::Const(type_ref),
                ..
            }) => self.lower_const_param_ty(type_ref),
            // A stale index or a type param read as a value: broken
            // source, silently `{error}`.
            _ => Ty::Error,
        }
    }

    /// Lower a const param's *declared* type — see
    /// [`Self::const_param_value_ty`] for the no-param-scope/no-holes
    /// reasoning; also used for the declared type a turbofish const
    /// argument is checked against.
    fn lower_const_param_ty(&mut self, type_ref: &TypeRef) -> Ty {
        let ty = lower_type_ref_in(
            self.db,
            self.file,
            type_ref,
            self.table,
            &ParamScope::default(),
        );
        if ty.contains_infer() { Ty::Error } else { ty }
    }

    /// Instantiate a mention of the generic item `loc` (TR06): a fresh
    /// variable per type param — checked/unified against the written
    /// turbofish type args when present (`_` stays free), with
    /// [`Cause::GenericArg`] recorded so later mismatches can point at the
    /// pinning argument — and every const arg type-checked against its
    /// declared type and recorded for later use. `args: None` is a bare
    /// mention: type params are left to inference; const params error
    /// (never inferred). Returns the instantiated signature.
    ///
    /// **Regions are ELIDED here.** The written list is positional over the
    /// binder's TYPE and CONST parameters only: a region parameter takes no
    /// written argument, and a written region is refused per argument
    /// ([`Self::spellable_args`]). It is the same rule a MEMBER's list
    /// obeys, and the only rule — `fn::<@b, U>` is called `f::<usize>`
    /// whether it is reached as an item or through a dot. Matching does not
    /// need the regions to be a contiguous prefix (the filter takes them
    /// out wherever they sit); the prefix rule (`require_regions_first` in
    /// `syntax::validation`) is for the READER.
    fn instantiate_mention(
        &mut self,
        expr: ExprId,
        loc: ItemLoc,
        sig: Ty,
        generics: &[GenericParamData],
        args: Option<&[GenericArgData]>,
    ) -> Ty {
        self.reject_named_args(expr, args);
        // The binder indices a written argument may spend, in order.
        let spellable: Vec<usize> = generics
            .iter()
            .enumerate()
            .filter(|(_, param)| !matches!(param.kind, GenericParamKind::Region))
            .map(|(index, _)| index)
            .collect();
        // Written-list positions, in binder order, once the regions are
        // refused and dropped — so a list that only got the elision wrong
        // still spends the rest.
        let matched_args = match args {
            Some(args) => {
                let kept = self.spellable_args(expr, args, TurbofishList::Item);
                // A dropped region DISPLACES the count: `id::<@z>(1)` on
                // `fn::<T>` has one argument visible and none spendable,
                // and "takes 1 generic argument, found 0" would be a false
                // sentence about a list the reader can see. The refusal is
                // the whole story until the region is gone.
                let displaced = kept.len() != args.len();
                if spellable.is_empty() {
                    // A binder of regions ALONE has nothing spellable, so
                    // "takes no generic arguments" is the true sentence
                    // about it — the same one a binder-less item and a
                    // member's list get. The empty `::<>` is included: a
                    // written list is never silently ignored.
                    if !displaced || !kept.is_empty() {
                        self.push_not_generic(expr, loc.display_name());
                    }
                    self.infer_const_args_free(args);
                    None
                } else if kept.len() != spellable.len() {
                    if !displaced {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::GenericArgCount {
                                expr,
                                item: loc.clone(),
                                // The SPELLABLE slots only: telling a caller
                                // that `fn::<@b, U>` "takes 2" would be
                                // counting a position they may not write.
                                expected: spellable.len(),
                                found: kept.len(),
                            });
                    }
                    // No positional matching is trustworthy; the const-value
                    // expressions are still inferred (freely) so their
                    // contents get types and diagnostics.
                    self.infer_const_args_free(args);
                    None
                } else {
                    Some(kept)
                }
            }
            None => {
                // A bare mention: const args are never inferred (TR06 —
                // running an instance backwards is
                // inference-through-conversion, categorically refused).
                if generics
                    .iter()
                    .any(|param| matches!(param.kind, GenericParamKind::Const(_)))
                {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::MissingConstArgs {
                            expr,
                            item: loc.clone(),
                        });
                }
                None
            }
        };
        // Each binder slot's written argument, paired with its position in
        // the WRITTEN list — no longer the binder index, because the
        // regions in front of it spend nothing. A cause that points at a
        // node needs the written position; substitution needs the binder
        // one.
        let mut written_for: Vec<Option<(u32, &GenericArgData)>> = vec![None; generics.len()];
        if let (Some(kept), Some(args)) = (&matched_args, args) {
            for (&slot, &at) in spellable.iter().zip(kept) {
                written_for[slot] = Some((at as u32, &args[at]));
            }
        }
        let mut subst: FxHashMap<u32, Ty> = FxHashMap::default();
        let mut const_subst: FxHashMap<u32, ConstArgValue> = FxHashMap::default();
        let mut region_subst: FxHashMap<u32, Region> = FxHashMap::default();
        let mut pending: Vec<(String, Ty, bool)> = Vec::new();
        let mut const_args: Vec<(u32, ExprId)> = Vec::new();
        for (index, param) in generics.iter().enumerate() {
            let arg_position = written_for[index].map(|(position, _)| position);
            let written = written_for[index].map(|(_, arg)| arg);
            match &param.kind {
                // A REGION parameter of the mentioned item. It spends no
                // written argument — the elision — and nothing is
                // substituted into the SCHEME either: regions are erased,
                // so a region at a mention constrains the borrow checker,
                // never the type. A fresh EXISTENTIAL per region parameter,
                // exactly as an omitted turbofish on a borrow expression
                // mints one: the callee's universals become this body's
                // inference variables, and its outlives relations ride
                // along as constraints between them
                // (`push_region_binder_bounds`, below).
                GenericParamKind::Region => {
                    region_subst.insert(index as u32, self.fresh_region());
                }
                GenericParamKind::Type => {
                    let var = self.fresh_var();
                    match written {
                        Some(GenericArgData::Type(type_ref)) => {
                            // Written under the MENTION's own binder scope:
                            // a turbofish inside a generic body may say
                            // `id::<T>`. A `_` lowers to a fresh variable —
                            // explicitly "infer this one".
                            let written_ty = self.lower_type_ref(type_ref);
                            self.constraints.adopt(
                                self.table,
                                &var,
                                &written_ty,
                                arg_position.map(|at| Cause::GenericArg {
                                    mention: expr,
                                    index: at,
                                    param: index as u32,
                                }),
                            );
                        }
                        Some(GenericArgData::Const(value)) => {
                            let value = *value;
                            self.result.diagnostics.push(
                                InferenceDiagnostic::GenericArgKindMismatch {
                                    expr,
                                    param: param.name.clone(),
                                    param_is_const: false,
                                },
                            );
                            let fresh = self.fresh_var();
                            self.infer_expr(value, &fresh);
                        }
                        // Refused and dropped before matching: a region
                        // (`spellable_args`), a named argument
                        // (`reject_named_args` — only a trait has a
                        // nameable one).
                        Some(GenericArgData::Region(_))
                        | Some(GenericArgData::Named { .. })
                        | None => {}
                    }
                    pending.push((param.name.clone(), var.clone(), param.without_forget));
                    subst.insert(index as u32, var);
                }
                GenericParamKind::Const(declared) => match written {
                    Some(GenericArgData::Const(value)) => {
                        let value = *value;
                        let declared = self.lower_const_param_ty(declared);
                        // Fn values are outside the const-arg domain
                        // (TR06: concrete data types only): the
                        // declaration already carries this exact text
                        // (see `file_diagnostics`); repeating it here is
                        // the belt at the mention — and the argument is
                        // NOT recorded, so no fn value can ever reach
                        // instance identity or evaluation. Array values
                        // are excluded the same way (the ruled domain is
                        // builtins + records + variants).
                        let fn_valued = declared.mentions_fn();
                        if fn_valued {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::FnConstArg { expr });
                        }
                        let array_valued = declared.mentions_array();
                        if array_valued {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::ArrayConstArg { expr });
                        }
                        self.infer_expr_with(
                            value,
                            &declared,
                            arg_position.map(|at| Cause::GenericArg {
                                mention: expr,
                                index: at,
                                param: index as u32,
                            }),
                        );
                        if !fn_valued && !array_valued {
                            const_args.push((index as u32, value));
                            // The scheme's TYPES may mention this const
                            // param (`fn::<const N: usize>(b: Buf::<N>)`) —
                            // substitution then needs the value at the type
                            // level, where only the annotation-representable
                            // domain exists. A computed value (a
                            // `const { ... }` block) is fine for the
                            // *instance* (eval handles it) but cannot enter
                            // TYPE identity — diagnosed only if the scheme
                            // actually embeds the param in a type.
                            match self.try_type_const_arg_value(value) {
                                Ok(arg_value) => {
                                    const_subst.insert(index as u32, arg_value);
                                }
                                Err(is_block) => {
                                    if ty_mentions_const_param(&sig, &loc, index as u32) {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::TypeConstArgUnsupported {
                                                expr,
                                                is_block,
                                            },
                                        );
                                    }
                                    const_subst.insert(index as u32, ConstArgValue::Error);
                                }
                            }
                        }
                    }
                    Some(GenericArgData::Type(TypeRef::Hole)) => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::ConstArgHole { expr });
                    }
                    Some(GenericArgData::Type(_)) => {
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::GenericArgKindMismatch {
                                expr,
                                param: param.name.clone(),
                                param_is_const: true,
                            });
                    }
                    // Already reported: `MissingConstArgs` (bare mention),
                    // `GenericArgCount` (unmatchable list),
                    // `RegionArgAtMention` (a region, dropped before
                    // matching) or `NamedGenericArg` (a named argument).
                    Some(GenericArgData::Region(_)) | Some(GenericArgData::Named { .. }) | None => {
                    }
                },
            }
        }
        if !const_args.is_empty() {
            self.result.const_args_of_expr.insert(expr, const_args);
        }
        // Bounds on the binder become obligations at this mention, keyed
        // here (MIR appends the dictionary operands to the direct call
        // whose callee this mention is). A bounded generic used as a VALUE
        // would need a captured dictionary — reserved; the reservation is
        // the WHOLE story for such a mention (no cannot-infer noise on
        // params the reserved value was never going to determine).
        let mut reserved_as_value = false;
        if !crate::traits::bound_slots(self.db, self.file, generics).is_empty() {
            if self.direct_callees.contains(&expr) {
                self.push_bound_obligations(expr, generics, &subst);
            } else {
                reserved_as_value = true;
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::BoundFnValue {
                        expr,
                        name: loc.display_name().to_owned(),
                    });
            }
        }
        if !sig.contains_error() && !pending.is_empty() && !reserved_as_value {
            // A broken scheme (fully-annotated rule violated) is excluded:
            // the definition carries the diagnostic, and piling
            // cannot-infer noise on every mention would drown it.
            self.pending_instantiations.push(PendingInstantiation {
                expr,
                item: loc.clone(),
                owner: loc.display_name().to_owned(),
                spelling: TurbofishSpelling::Path,
                params: pending,
            });
        }
        // The callee's declared outlives bounds (`fn::<@a, @b: @a>`)
        // travel with the instantiation: at this call site they become
        // obligations between the fresh variables that stand for them.
        self.push_region_binder_bounds(generics, &region_subst, expr);
        let inst = instantiate_scheme(&sig, &loc, &subst, &const_subst);
        substitute_regions(&inst, &loc, &region_subst)
    }

    /// A turbofish mention `f::<usize, 42>`. The base resolves like any
    /// name but is not inferred as an expression (the same interception
    /// idea as variant-path bases and construction heads); the arguments
    /// are judged against the resolved item's binder. A turbofish on
    /// anything non-generic is an error, recovering with the base's own
    /// type as if the turbofish weren't there.
    fn infer_generic_app(&mut self, expr: ExprId, base: ExprId, args: &[GenericArgData]) -> Ty {
        let base_name = match &self.body.exprs[base] {
            ExprData::NameRef(name) => name.clone(),
            _ => String::new(),
        };
        match self.resolutions.get(base) {
            Some(Resolution::Item(loc)) => {
                let loc = loc.clone();
                if let Some(member_sig) = self.in_group.get(&loc) {
                    // Generic items never join groups, so an in-group
                    // member is non-generic by construction.
                    let member_sig = member_sig.clone();
                    self.refuse_item_args(expr, &base_name, args);
                    return member_sig;
                }
                let target = loc.to_id(self.db);
                let generics = item_generics(self.db, target);
                if generics.is_empty() {
                    self.refuse_item_args(expr, &base_name, args);
                    return signature(self.db, target);
                }
                let sig = signature(self.db, target);
                self.instantiate_mention(expr, loc, sig, generics, Some(args))
            }
            Some(Resolution::Local(binding)) => {
                let binding = *binding;
                self.refuse_item_args(expr, &base_name, args);
                self.result
                    .type_of_binding
                    .get(binding)
                    .cloned()
                    .unwrap_or(Ty::Error)
            }
            Some(Resolution::ConstParam(index)) => {
                let index = *index;
                self.refuse_item_args(expr, &base_name, args);
                self.const_param_value_ty(index)
            }
            Some(Resolution::Builtin(builtin)) => {
                let builtin = *builtin;
                self.infer_builtin_mention(expr, builtin, Some(args))
            }
            // A bare `Pair::<usize>` in value position: types are not
            // first-class values — the legal positions (a construction
            // head, a variant path's base) are intercepted before this
            // runs, so reaching here IS the error, exactly like a bare
            // un-turbofished type name.
            Some(Resolution::TypeItem(_)) => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::TypeNotValue {
                        expr,
                        name: base_name.clone(),
                    });
                self.infer_const_args_free(args);
                Ty::Error
            }
            // `Display::<Self = Foo>` with no member segment: the argument
            // list is the named-Self form's position (TR01), but a trait is
            // still not a value — only `Trait::<...>::member` names one. A
            // RESERVED generic trait's own reservation wins.
            Some(Resolution::TraitItem(loc)) => {
                if crate::traits::trait_is_generic(self.db, loc) {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::GenericTraitReserved {
                            expr,
                            name: loc.display_name().to_owned(),
                        });
                } else {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::TraitNotValue {
                            expr,
                            name: base_name.clone(),
                        });
                }
                self.infer_const_args_free(args);
                Ty::Error
            }
            // The duplicate-definition/unresolved-name diagnostics sit on
            // the base.
            Some(Resolution::Ambiguous(_)) | None => {
                self.infer_const_args_free(args);
                Ty::Error
            }
        }
    }

    /// Named generic arguments belong to a TRAIT's argument list and name
    /// nothing but `Self` (TR01). Everywhere else — a fn mention, a type
    /// mention, a construction head — every named argument is refused here;
    /// the positional matching then treats it as an unusable argument.
    fn reject_named_args(&mut self, expr: ExprId, args: Option<&[GenericArgData]>) {
        self.reject_named_args_because(expr, args, NamedArgReason::NotATrait);
    }

    /// [`Self::reject_named_args`] with `self_reason` for a written
    /// `Self` — a member's own list needs one of its own, because there
    /// the owner may BE a trait and the argument is still misplaced.
    fn reject_named_args_because(
        &mut self,
        expr: ExprId,
        args: Option<&[GenericArgData]>,
        self_reason: NamedArgReason,
    ) {
        for arg in args.unwrap_or(&[]) {
            let GenericArgData::Named { name, .. } = arg else {
                continue;
            };
            let reason = if name == "Self" {
                self_reason.clone()
            } else {
                NamedArgReason::NotSelf
            };
            self.result
                .diagnostics
                .push(InferenceDiagnostic::NamedGenericArg {
                    expr,
                    name: name.clone(),
                    reason,
                });
        }
    }

    /// Refuse every REGION argument in a mention's written list and answer
    /// the positions of the arguments that remain — the SPELLABLE ones.
    ///
    /// One rule, in the one place both lists reach it: a turbofish spells
    /// type and const arguments, and regions are inferred at every call.
    /// Refusing per ARGUMENT rather than per list is what lets a use site
    /// that only got the elision wrong (`f::<@_, usize>`) keep the rest of
    /// its arguments and produce exactly one message. `expr` is the node
    /// the list hangs off, and `list` says which of its lists it is.
    fn spellable_args(
        &mut self,
        expr: ExprId,
        args: &[GenericArgData],
        list: TurbofishList,
    ) -> Vec<usize> {
        let mut kept = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            if matches!(arg, GenericArgData::Region(_)) {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::RegionArgAtMention {
                        expr,
                        index: index as u32,
                        list,
                    });
                continue;
            }
            kept.push(index);
        }
        kept
    }

    /// [`Self::spellable_args`] for a list its target has NOTHING to spend
    /// on — an item, local, builtin or member with no binder, a trait's
    /// own list. The regions are refused just the same, and the answer says
    /// whether "takes no generic arguments" is still owed: it is unless the
    /// list held nothing but refused regions, which were told the whole
    /// story. The regions-only binder in [`Self::instantiate_mention`]
    /// makes the same call, so `fn::<@a>` and no binder at all answer a
    /// written list identically.
    fn owes_not_generic(
        &mut self,
        expr: ExprId,
        args: &[GenericArgData],
        list: TurbofishList,
    ) -> bool {
        let kept = self.spellable_args(expr, args, list);
        let displaced = kept.len() != args.len();
        !displaced || !kept.is_empty()
    }

    /// A turbofish on a NAME with no binder to spend it on (a non-generic
    /// item, a local, a const param, a monomorphic builtin): the regions
    /// are refused per argument, the rest is told there is nothing to
    /// spend it on, and the const arguments still get their types.
    fn refuse_item_args(&mut self, expr: ExprId, name: &str, args: &[GenericArgData]) {
        if self.owes_not_generic(expr, args, TurbofishList::Item) {
            self.push_not_generic(expr, name);
        }
        self.infer_const_args_free(args);
    }

    fn push_not_generic(&mut self, expr: ExprId, name: &str) {
        self.result
            .diagnostics
            .push(InferenceDiagnostic::NotGeneric {
                expr,
                name: name.to_owned(),
                owner_list_hint: None,
            });
    }

    /// A mention of a builtin, bare (`args: None`) or turbofished. The
    /// monomorphic builtins keep their one fixed type; the scheme-shaped
    /// ones ([`builtin_generics`]) run the exact same instantiation
    /// machinery as generic fn items; the flavor-polymorphic builtins have
    /// no first-class type at all — their one legal position, a direct
    /// call, is intercepted in the `Call` arm before the callee would be
    /// inferred, so reaching here IS the error.
    fn infer_builtin_mention(
        &mut self,
        expr: ExprId,
        builtin: Builtin,
        args: Option<&[GenericArgData]>,
    ) -> Ty {
        if let Some(generics) = builtin_generics(builtin) {
            let (loc, sig) = builtin_scheme(builtin, self.file);
            return self.instantiate_mention(expr, loc, sig, &generics, args);
        }
        if let Some(args) = args {
            self.refuse_item_args(expr, builtin.name(), args);
            return builtin_type(builtin, self.file);
        }
        if builtin.flavor_polymorphic() {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::BuiltinNotFirstClass { expr, builtin });
            return Ty::Error;
        }
        builtin_type(builtin, self.file)
    }

    /// A direct call of a flavor-polymorphic builtin — the checker special
    /// case the ruled spec asks for: `add`/`offset` preserve their pointer
    /// argument's flavor (`.&raw mut` in → `.&raw mut` out), `copy` accepts
    /// either flavor for `src`, and the blesses accept either for the
    /// buffer they read while pinning its pointee to `u8`; no one `fn` type
    /// says any of that. Everything else about the call is the ordinary
    /// machinery (argument expectations with causes, arity as
    /// `ArgCountMismatch`).
    fn infer_builtin_special_call(
        &mut self,
        call: ExprId,
        builtin: Builtin,
        args: &[ExprId],
    ) -> Ty {
        let expected_arity = match builtin {
            Builtin::Add
            | Builtin::Offset
            | Builtin::StrFromUtf8
            | Builtin::StrFromUtf8Unchecked => 2,
            Builtin::Copy => 3,
            _ => unreachable!("not a flavor-polymorphic builtin"),
        };
        if args.len() != expected_arity {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::ArgCountMismatch {
                    expr: call,
                    expected: expected_arity,
                    found: args.len(),
                });
            for &arg in args {
                let fresh = self.fresh_var();
                self.infer_expr(arg, &fresh);
            }
            return Ty::Error;
        }
        // The flavor-polymorphic pointer argument: inferred freely, then
        // judged — a checked expectation would have to pick one flavor.
        let ptr_arg = |this: &mut Self, arg: ExprId| {
            let fresh = this.fresh_var();
            let ty = this.infer_expr(arg, &fresh);
            match this.resolve_shallow(&ty) {
                Ty::RawPtr { mutable, pointee } => Some((mutable, pointee)),
                // Broken or diverging: silent, like every infectious type.
                Ty::Error | Ty::Never => None,
                found => {
                    this.result
                        .diagnostics
                        .push(InferenceDiagnostic::BuiltinExpectsRawPtr {
                            call,
                            arg,
                            builtin,
                            found,
                        });
                    None
                }
            }
        };
        match builtin {
            // `add(p, i)`: pointer to element `i` past `p`, same
            // allocation, same flavor. `offset(p, i)` is the signed
            // sibling — element arithmetic both directions, `i: isize`.
            Builtin::Add | Builtin::Offset => {
                let ptr = ptr_arg(self, args[0]);
                let index_ty = match builtin {
                    Builtin::Add => Ty::Int(IntKind::Usize),
                    _ => Ty::Int(IntKind::Isize),
                };
                self.infer_expr_with(
                    args[1],
                    &index_ty,
                    Some(Cause::CallSite { call, arg: args[1] }),
                );
                match ptr {
                    Some((mutable, pointee)) => Ty::RawPtr { mutable, pointee },
                    None => Ty::Error,
                }
            }
            // `copy(src, dst, n)`: `src` may be either flavor, `dst` must
            // be `.&raw mut`, and the pointees must agree — `dst` is
            // checked against `<src's pointee>.&raw mut` so the mismatch
            // diagnostics are the ordinary type-mismatch ones.
            Builtin::Copy => {
                let elem = self.fresh_var();
                if let Some((_, pointee)) = ptr_arg(self, args[0]) {
                    self.adopt(&elem, pointee.as_ref());
                }
                self.infer_expr_with(
                    args[1],
                    &Ty::raw_ptr(true, elem),
                    Some(Cause::CallSite { call, arg: args[1] }),
                );
                self.infer_expr_with(
                    args[2],
                    &Ty::Int(IntKind::Usize),
                    Some(Cause::CallSite { call, arg: args[2] }),
                );
                Ty::Unit
            }
            // `str_from_utf8(p, len)` / `str_from_utf8_unchecked(p, len)`:
            // `len` bytes at `p`, taken as text. `p` may be either flavor
            // (reading is what happens to it) but its pointee is pinned to
            // `u8` — the boundary is BYTES-FIRST, and a bless over some
            // other element type would be a layout claim, not a text one.
            Builtin::StrFromUtf8 | Builtin::StrFromUtf8Unchecked => {
                if let Some((mutable, pointee)) = ptr_arg(self, args[0]) {
                    let byte = Ty::Int(IntKind::U8);
                    if !self.adopt(&byte, pointee.as_ref()) {
                        let found = Ty::RawPtr {
                            mutable,
                            pointee: pointee.clone(),
                        };
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::TypeMismatch {
                                expr: args[0],
                                expected: Ty::raw_ptr(mutable, byte),
                                actual: found,
                                reasons: vec![Cause::CallSite { call, arg: args[0] }],
                            });
                    }
                }
                self.infer_expr_with(
                    args[1],
                    &Ty::Int(IntKind::Usize),
                    Some(Cause::CallSite { call, arg: args[1] }),
                );
                match builtin {
                    Builtin::StrFromUtf8 => {
                        Ty::Named(NamedTy::plain(crate::utf8_result_loc(self.file)))
                    }
                    _ => Ty::Str,
                }
            }
            _ => unreachable!("not a flavor-polymorphic builtin"),
        }
    }

    /// Infer every const-argument value freely — the error paths, where no
    /// declared type can be matched up; contents still get types and
    /// diagnostics.
    fn infer_const_args_free(&mut self, args: &[GenericArgData]) {
        for arg in args {
            if let GenericArgData::Const(value) = arg {
                let fresh = self.fresh_var();
                self.infer_expr(*value, &fresh);
            }
        }
    }

    /// [`Self::infer_const_args_free`] for a list that has just been
    /// REFUSED: the arguments still get types, and a bare integer among
    /// them is then silenced. Errors are infectious and silent — the
    /// refusal is the whole story, and "cannot infer the type of this
    /// number" on top of it is advice about a number the program was never
    /// going to keep.
    fn infer_const_args_free_quiet(&mut self, args: &[GenericArgData]) {
        for arg in args {
            if let GenericArgData::Const(value) = arg {
                let fresh = self.fresh_var();
                self.infer_expr(*value, &fresh);
                poison_unresolved_number(self.table, &fresh);
            }
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
                    // A generic enum still pins the scrutinee, with fresh
                    // variables per type param (the arms' payload bindings
                    // determine them) — EXCEPT when the binder declares
                    // const params, which are never inferred (TR06): the
                    // scrutinee then stays unknown and the pattern gets
                    // the ordinary annotate-the-scrutinee diagnostic.
                    let generics = item_generics(self.db, loc.to_id(self.db));
                    if generics
                        .iter()
                        .any(|param| matches!(param.kind, GenericParamKind::Const(_)))
                    {
                        continue;
                    }
                    let args = generics
                        .iter()
                        .map(|_| GenericArg::Ty(self.fresh_var()))
                        .collect();
                    let named = NamedTy {
                        decl: loc.clone(),
                        args,
                    };
                    self.adopt(&scrut_ty, &Ty::Named(named));
                    break;
                }
            }
        }
        // MATCH PROJECTS THROUGH BORROWS (M13). A borrowed scrutinee
        // dispatches on the enum BEHIND the borrow, and every payload
        // binding comes out as a borrow of the corresponding sub-place —
        // never a copy, never a move. The lens is what the rest of the
        // match carries to remember it is looking through one; `None`
        // means an OWNED scrutinee, and every path below is then
        // byte-identical to what it was before this existed.
        //
        // A referent this match can DISPATCH on lifts the lens (the same
        // `dispatches_on` mir asks): an enum or a variant by tag, a `char`
        // by equality against a literal pattern — `match c { 'a' => ... }`
        // means the same thing whether `c` is a `char` or a `char.&`, and
        // the equality test becomes a read THROUGH the borrow, so an
        // invalidated scrutinee is caught at the `match`. A `struct.&` or a
        // `usize.&` scrutinee stays `Scrutinee::Other` holding the BORROW
        // type, exactly as before, so its diagnostics do not move.
        let mut lens = None;
        let mut classify = self.resolve_shallow(&scrut_ty);
        if let Ty::Borrow {
            mutable,
            region,
            referent,
        } = &classify
        {
            let referent = self.resolve_shallow(referent);
            if crate::dispatches_on(self.db, &referent) {
                lens = Some(MatchLens {
                    mutable: *mutable,
                    region: region.clone(),
                    scrutinee,
                });
                classify = referent;
            }
        }
        let scrut = match classify {
            Ty::Named(named) => {
                if enum_variants(self.db, named.decl.to_id(self.db)).is_some() {
                    Scrutinee::Enum(named)
                } else if matches!(
                    crate::type_decl(self.db, named.decl.to_id(self.db)),
                    Some(TypeDeclData::Struct { .. })
                ) {
                    Scrutinee::Other(Ty::Named(named))
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
            Scrutinee::Enum(named) => enum_variants(self.db, named.decl.to_id(self.db))
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0),
            _ => 0,
        };
        let mut covered = vec![false; n_variants];
        // The literal values already matched, for the duplicate-arm warning
        // — a plain list, not a set-cover: the domain is far too big to
        // enumerate, so this answers "seen before?" and nothing else.
        let mut covered_literals: Vec<char> = Vec::new();
        let mut catch_all = false;
        let mut all_diverge = true;
        for arm in arms {
            let cover = self.check_match_pat(expr, arm.pat, &scrut, lens.as_ref());
            match cover {
                Cover::Nothing => {}
                _ if catch_all => {
                    // Reached by a literal arm too: `_ => ...` before
                    // `'(' => ...` makes the literal dead, and the reason
                    // is the catch-all, exactly as for a variant.
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
                Cover::Literal => {
                    if let PatData::Char(value) = self.body.pats[arm.pat] {
                        if covered_literals.contains(&value) {
                            self.push_unreachable(
                                expr,
                                arm.pat,
                                UnreachableReason::LiteralCovered(format!("{value:?}")),
                            );
                        } else {
                            covered_literals.push(value);
                        }
                    }
                }
                Cover::All => {
                    if n_variants > 0 && covered.iter().all(|&c| c) {
                        let name = match &scrut {
                            Scrutinee::Enum(named) => named.decl.display_name().to_owned(),
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
                Scrutinee::Enum(named) => {
                    if !covered.iter().all(|&c| c) {
                        let variants = enum_variants(self.db, named.decl.to_id(self.db))
                            .as_ref()
                            .expect("classified as an enum above");
                        let uncovered = variants
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| !covered[i])
                            .map(|(_, (name, _))| name.clone())
                            .collect();
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NonExhaustiveMatch {
                                expr,
                                decl: named.decl.clone(),
                                uncovered,
                            });
                    }
                }
                Scrutinee::Variant(variant) => {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::NonExhaustiveMatch {
                            expr,
                            decl: variant.decl.clone(),
                            uncovered: vec![variant.name.to_string()],
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
                    self.adopt(&result, &Ty::Never);
                    Ty::Never
                }
                1 => {
                    let ty = witnesses.into_iter().next().unwrap().ty;
                    self.adopt(&result, &ty);
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

    /// Generic args for a pattern's enum: taken from the scrutinee when it
    /// is (a variant of) the same enum — patterns never introduce args of
    /// their own beyond a written turbofish, and the scrutinee's are the
    /// ground truth. A wrong-enum pattern (its own diagnostic) gets
    /// all-error args, so the declaration's rigid params never leak into
    /// the arm's binding types.
    fn pat_enum_args(&mut self, target: &ItemLoc, scrut: &Scrutinee) -> Vec<GenericArg> {
        match scrut {
            Scrutinee::Enum(named) if named.decl == *target => named.args.clone(),
            Scrutinee::Variant(variant) if variant.decl == *target => variant.args.clone(),
            _ => item_generics(self.db, target.to_id(self.db))
                .iter()
                .map(|param| match param.kind {
                    GenericParamKind::Type => GenericArg::Ty(Ty::Error),
                    GenericParamKind::Region => GenericArg::Region(Region::Erased),
                    GenericParamKind::Const(_) => GenericArg::Const(ConstArgValue::Error),
                })
                .collect(),
        }
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
    ///
    /// `lens` is `Some` exactly when the scrutinee is a BORROW of the enum
    /// (or variant) `scrut` describes — see [`MatchLens`]. It changes only
    /// what the bindings are typed as: a whole-value binder gets the
    /// borrow back, and each payload binder gets a borrow of that
    /// payload's sub-place. Coverage, variant resolution, arity and every
    /// diagnostic below are untouched by it, which is what makes the owned
    /// path provably unchanged.
    fn check_match_pat(
        &mut self,
        match_expr: ExprId,
        pat: PatId,
        scrut: &Scrutinee,
        lens: Option<&MatchLens>,
    ) -> Cover {
        match self.body.pats[pat].clone() {
            PatData::Missing => Cover::Nothing,
            PatData::Wildcard => Cover::All,
            // A character literal pattern. It binds nothing, so the whole
            // check is agreement with the scrutinee — and unlike a variant
            // pattern there is no name to resolve type-directed: `'x'` is a
            // `char` before anything is known about what it is matched
            // against, which is exactly why it can carry the blame itself.
            PatData::Char(_) => {
                let found = Ty::Char;
                let scrut_ty = match scrut {
                    Scrutinee::Enum(named) => Ty::Named(named.clone()),
                    Scrutinee::Variant(variant) => Ty::Variant(variant.clone()),
                    Scrutinee::Other(ty) | Scrutinee::Unknown(ty) => ty.clone(),
                    Scrutinee::Error => Ty::Error,
                };
                // An UNKNOWN scrutinee is pinned by the pattern, the same
                // way a qualified variant pattern pins one: a literal
                // pattern is construction's mirror image too, and `'x'`
                // says the value is a `char`.
                if matches!(scrut, Scrutinee::Unknown(_)) {
                    self.adopt(&scrut_ty, &found);
                    return Cover::Literal;
                }
                let resolved = self.resolve_shallow(&scrut_ty);
                if !matches!(resolved, Ty::Char | Ty::Error) {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::PatLiteralTypeMismatch {
                            match_expr,
                            pat,
                            expected: resolved,
                            found,
                        });
                    return Cover::Nothing;
                }
                Cover::Literal
            }
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
                    Scrutinee::Enum(named) => Some(named.decl.clone()),
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
                    Scrutinee::Enum(named) => Ty::Named(named.clone()),
                    Scrutinee::Variant(variant) => Ty::Variant(variant.clone()),
                    Scrutinee::Other(ty) | Scrutinee::Unknown(ty) => ty.clone(),
                    Scrutinee::Error => Ty::Error,
                };
                // A whole-value binder on a BORROWED scrutinee binds the
                // borrow itself, with the scrutinee's own region and no
                // new edge: it names the very same place, so there is
                // nothing to project and nothing to shorten. (This is also
                // what the arm got before the lens existed, when a
                // borrowed scrutinee classified as `Scrutinee::Other`.)
                let ty = match lens {
                    Some(lens) => lens.wrap(lens.region.clone(), ty),
                    None => ty,
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
                        Scrutinee::Enum(named) => named.decl.clone(),
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
                let args = self.pat_enum_args(&target, scrut);
                let variant_ty = VariantTy {
                    decl: target.clone(),
                    args,
                    index: index as u32,
                    name: std::sync::Arc::from(variant.as_str()),
                };
                self.result.variant_of_pat.insert(pat, variant_ty.clone());
                // Payload binding types come from the declaration either
                // way — even a wrong-enum pattern's arm body shouldn't
                // cascade — with the scrutinee's generic args substituted
                // in (a wrong-enum pattern gets all-error args, so the
                // declaration's rigid params never leak into a binding).
                let payload: Vec<Ty> = variants[index]
                    .1
                    .iter()
                    .map(|ty| substitute_args(ty, &target, &variant_ty.args))
                    .collect();
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
                    // THE PROJECTION. Through a borrow the binder does not
                    // receive the payload — it receives a borrow OF the
                    // payload's sub-place, in the scrutinee's flavor, at a
                    // fresh region the scrutinee's region must cover.
                    //
                    // Fresh-and-bounded rather than the scrutinee's region
                    // verbatim, for the same reason every other mention of
                    // a borrow mints one: the payload loan is allowed to be
                    // SHORTER than the parent's, and the single directed
                    // edge is what forbids it ever being longer. It is not
                    // forced shorter — `@r` has only this upper bound, so
                    // it can still take the parent's whole region, which is
                    // exactly the `as_ref` requirement.
                    //
                    // `Ty::Error` stays bare: an already-broken binding
                    // reads worse wrapped in a borrow, and cascades.
                    let ty = match lens {
                        Some(lens) if !matches!(ty, Ty::Error) => {
                            let region = self.fresh_region();
                            self.push_outlives(
                                lens.region.clone(),
                                region.clone(),
                                lens.scrutinee,
                                RegionConstraintReason::Projection,
                            );
                            lens.wrap(region, ty)
                        }
                        _ => ty,
                    };
                    self.result.type_of_binding.insert(binding, ty);
                }
                match scrut {
                    Scrutinee::Enum(scrut_named) => {
                        if scrut_named.decl != target {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::PatWrongEnum {
                                    match_expr,
                                    pat,
                                    item: target,
                                    variant: variant_ty.name.to_string(),
                                    scrutinee: Ty::Named(scrut_named.clone()),
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
                    Some(Resolution::TypeItem(loc)) => {
                        // A generic newtype pattern determines the type
                        // FAMILY; its args come from the initializer (fresh
                        // vars, ordinary unification) — except const
                        // params, which are never inferred (TR06): the value
                        // side must pin the type then, so claim nothing.
                        let generics = item_generics(self.db, loc.to_id(self.db));
                        if generics
                            .iter()
                            .any(|param| matches!(param.kind, GenericParamKind::Const(_)))
                        {
                            return None;
                        }
                        let named = self.named_with_fresh_args(&loc);
                        Some(Ty::Named(named))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The named type `loc` with a fresh inference variable per type param
    /// (and `Error` for const params — never inferred): the "some instance
    /// of this family" type a pattern or recovery path claims.
    fn named_with_fresh_args(&mut self, loc: &ItemLoc) -> NamedTy {
        let args = item_generics(self.db, loc.to_id(self.db))
            .iter()
            .map(|param| match param.kind {
                GenericParamKind::Type => GenericArg::Ty(self.fresh_var()),
                GenericParamKind::Region => GenericArg::Region(Region::Erased),
                GenericParamKind::Const(_) => GenericArg::Const(ConstArgValue::Error),
            })
            .collect();
        NamedTy {
            decl: loc.clone(),
            args,
        }
    }

    /// Recursively type every binding a pattern introduces as `{error}` —
    /// the destructuring counterpart of [`Self::bind_error`], for a pattern
    /// whose scrutinee type is broken or unknown.
    fn bind_pat_error(&mut self, pat: PatId) {
        self.result.type_of_pat.insert(pat, Ty::Error);
        match self.body.pats[pat].clone() {
            PatData::Missing | PatData::Wildcard | PatData::Char(_) => {}
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
            // A literal pattern is refutable, so `binding_pattern`'s
            // grammar never produces one here; defensive, like `Variant`.
            PatData::Missing | PatData::Wildcard | PatData::Char(_) => {}
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
                    Ty::Named(named) if named.decl == target => {
                        let underlying = type_underlying_for(self.db, &named).unwrap_or(Ty::Error);
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
                        let expected = self.named_with_fresh_args(&target);
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::PatNamedTypeMismatch {
                                pat,
                                expr: anchor,
                                expected: Ty::Named(expected),
                                actual: other,
                            });
                        self.bind_pat_error(inner);
                    }
                }
            }
        }
    }

    /// A construction call `Foo(arg)` / `Pair::<usize>(arg)`: instantiate
    /// the type mention (written turbofish args checked, missing ones left
    /// to inference), then type-check the single argument against the
    /// *substituted* underlying record bidirectionally (the declared field
    /// types — with the mention's args in place of the rigid params — flow
    /// into a literal argument's fields, blame cites the declaration via
    /// [`Cause::Constructor`]) and produce [`Ty::Named`]. An unwritten
    /// type arg is pinned by the payload through ordinary unification.
    #[allow(clippy::too_many_arguments)]
    fn infer_construction(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        loc: &ItemLoc,
        generic_args: Option<&[GenericArgData]>,
        args: &[ExprId],
        expected: &Ty,
        cause: Option<Cause>,
    ) -> Ty {
        let named = self.instantiate_type_mention(callee, loc, generic_args);
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
            let ty = self.check(expr, Ty::Named(named), expected, cause);
            self.result.type_of_expr.insert(expr, ty.clone());
            return ty;
        }
        // `None` when the declaration is broken (RHS not a `struct`
        // literal): the declaration site carries the diagnostic, so the
        // argument is checked against `{error}` — infectious and silent.
        let underlying = type_underlying_for(self.db, &named).unwrap_or(Ty::Error);
        // The constructor *is* a function value conceptually; give the
        // callee name that type so hover on `Foo` in `Foo(...)` is honest.
        self.result.type_of_expr.insert(
            callee,
            Ty::fn_type(vec![underlying.clone()], Ty::Named(named.clone())),
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
        let ty = self.check(expr, Ty::Named(named), expected, cause);
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    /// Instantiate a mention of the TYPE item `loc` — the type-side sibling
    /// of [`Self::instantiate_mention`]: a fresh variable per type param
    /// (unified with the written turbofish arg when present, `_` staying
    /// free, with [`Cause::GenericArg`] recorded), and const args
    /// restricted to the annotation-representable domain — literals and
    /// const-param reads. Anything computed (a `const { ... }` block, an
    /// item) is rejected: type identity lives on the eval-free path, so a
    /// value that needs evaluation can never enter it. `written: None` is a
    /// bare mention (`Pair(...)`): type params are left to inference, const
    /// params error (never inferred, TR06). Non-generic declarations get the
    /// plain identity (plus `NotGeneric` when a turbofish was written).
    fn instantiate_type_mention(
        &mut self,
        mention: ExprId,
        loc: &ItemLoc,
        written: Option<&[GenericArgData]>,
    ) -> NamedTy {
        // Expression-position `Self` is the RIGID Self (TR01): the
        // owner type at the member's own binders, exactly as in type
        // position — ONE meaning of `Self` per body, never a fresh
        // instantiation. A turbofish on it is rejected like a turbofish on
        // any non-generic name (`Self` already IS the type at its args).
        if let Some(self_named) = self.rigid_self_mention(mention) {
            if let Some(args) = written {
                self.push_not_generic(mention, "Self");
                self.infer_const_args_free(args);
            }
            return self_named;
        }
        self.reject_named_args(mention, written);
        let generics = item_generics(self.db, loc.to_id(self.db));
        if generics.is_empty() {
            if let Some(args) = written {
                self.push_not_generic(mention, loc.display_name());
                self.infer_const_args_free(args);
            }
            return NamedTy::plain(loc.clone());
        }
        let matched = match written {
            Some(args) if args.len() != generics.len() => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::GenericArgCount {
                        expr: mention,
                        item: loc.clone(),
                        expected: generics.len(),
                        found: args.len(),
                    });
                self.infer_const_args_free(args);
                None
            }
            Some(args) => Some(args),
            None => {
                // A bare mention: const args are never inferred (TR06).
                if generics
                    .iter()
                    .any(|param| matches!(param.kind, GenericParamKind::Const(_)))
                {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::MissingConstArgs {
                            expr: mention,
                            item: loc.clone(),
                        });
                }
                None
            }
        };
        let mut pending: Vec<(String, Ty, bool)> = Vec::new();
        let mut out: Vec<GenericArg> = Vec::with_capacity(generics.len());
        for (index, param) in generics.iter().enumerate() {
            let arg = matched.map(|args| &args[index]);
            match &param.kind {
                GenericParamKind::Type => {
                    let var = self.fresh_var();
                    match arg {
                        Some(GenericArgData::Type(type_ref)) => {
                            let written_ty = self.lower_type_ref(type_ref);
                            self.constraints.adopt(
                                self.table,
                                &var,
                                &written_ty,
                                Some(Cause::GenericArg {
                                    mention,
                                    // A type mention's list is WHOLE-binder
                                    // positional (a type declaration's
                                    // regions are reserved), so the binder
                                    // index IS the written position here.
                                    index: index as u32,
                                    param: index as u32,
                                }),
                            );
                        }
                        Some(GenericArgData::Const(value)) => {
                            let value = *value;
                            self.result.diagnostics.push(
                                InferenceDiagnostic::GenericArgKindMismatch {
                                    expr: mention,
                                    param: param.name.clone(),
                                    param_is_const: false,
                                },
                            );
                            let fresh = self.fresh_var();
                            self.infer_expr(value, &fresh);
                        }
                        Some(GenericArgData::Region(_)) => {
                            self.result.diagnostics.push(
                                InferenceDiagnostic::UnexpectedRegionArg {
                                    expr: mention,
                                    index: index as u32,
                                    param: param.name.clone(),
                                },
                            );
                        }
                        // Refused at the list (`reject_named_args`).
                        Some(GenericArgData::Named { .. }) | None => {}
                    }
                    pending.push((param.name.clone(), var.clone(), param.without_forget));
                    out.push(GenericArg::Ty(var));
                }
                // Regions on a TYPE declaration are reserved (variance and
                // well-formedness are a later arc's decisions); the mirror
                // pass says so. Arity stays honest — the slot is filled
                // with the erased region so later arguments keep their
                // positions.
                GenericParamKind::Region => out.push(GenericArg::Region(Region::Erased)),
                GenericParamKind::Const(declared) => {
                    let value = match arg {
                        Some(GenericArgData::Region(_)) => {
                            self.result.diagnostics.push(
                                InferenceDiagnostic::UnexpectedRegionArg {
                                    expr: mention,
                                    index: index as u32,
                                    param: param.name.clone(),
                                },
                            );
                            ConstArgValue::Error
                        }
                        Some(GenericArgData::Const(value)) => {
                            let value = *value;
                            let declared = self.lower_const_param_ty(declared);
                            // Fn values are outside the const-arg domain
                            // (TR06: concrete data types only) — same belt
                            // as fn mentions. Array values are excluded
                            // the same way.
                            let fn_valued = declared.mentions_fn();
                            if fn_valued {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::FnConstArg { expr: mention });
                            }
                            let array_valued = declared.mentions_array();
                            if array_valued {
                                self.result
                                    .diagnostics
                                    .push(InferenceDiagnostic::ArrayConstArg { expr: mention });
                            }
                            self.infer_expr_with(
                                value,
                                &declared,
                                Some(Cause::GenericArg {
                                    mention,
                                    // Whole-binder positional; see above.
                                    index: index as u32,
                                    param: index as u32,
                                }),
                            );
                            if fn_valued || array_valued {
                                ConstArgValue::Error
                            } else {
                                self.type_const_arg_value(mention, value)
                            }
                        }
                        Some(GenericArgData::Type(TypeRef::Hole)) => {
                            self.result
                                .diagnostics
                                .push(InferenceDiagnostic::ConstArgHole { expr: mention });
                            ConstArgValue::Error
                        }
                        // A bare `N`: a const position whose argument
                        // parsed as a type naming an in-scope const
                        // param — same acceptance (and the same
                        // declared-type agreement check) as the
                        // annotation path.
                        Some(GenericArgData::Type(TypeRef::Path(path)))
                            if self.type_params.consts.contains_key(path.as_str()) =>
                        {
                            let forwarded = self.type_params.consts[path.as_str()].clone();
                            if let ConstArgValue::Param {
                                index: own_index, ..
                            } = &forwarded
                                && let Some(GenericParamData {
                                    kind: GenericParamKind::Const(own_declared),
                                    ..
                                }) = self.own_generics.get(*own_index as usize)
                            {
                                let own_declared = own_declared.clone();
                                let expected = self.lower_const_param_ty(declared);
                                let found = self.lower_const_param_ty(&own_declared);
                                if !expected.contains_error()
                                    && !found.contains_error()
                                    && expected != found
                                {
                                    self.result.diagnostics.push(
                                        InferenceDiagnostic::TypeMismatch {
                                            expr: mention,
                                            expected,
                                            actual: found,
                                            reasons: Vec::new(),
                                        },
                                    );
                                }
                            }
                            forwarded
                        }
                        Some(GenericArgData::Type(_)) => {
                            self.result.diagnostics.push(
                                InferenceDiagnostic::GenericArgKindMismatch {
                                    expr: mention,
                                    param: param.name.clone(),
                                    param_is_const: true,
                                },
                            );
                            ConstArgValue::Error
                        }
                        // Already reported: `MissingConstArgs` (bare
                        // mention), `GenericArgCount` (unmatchable list) or
                        // `NamedGenericArg` (a named argument here).
                        Some(GenericArgData::Named { .. }) | None => ConstArgValue::Error,
                    };
                    out.push(GenericArg::Const(value));
                }
            }
        }
        // A broken declaration (RHS not a `struct`/`enum` literal) never
        // pins its params — the declaration site carries the diagnostic;
        // cannot-infer noise per mention would drown it.
        if !pending.is_empty() && crate::type_decl(self.db, loc.to_id(self.db)).is_some() {
            self.pending_instantiations.push(PendingInstantiation {
                expr: mention,
                item: loc.clone(),
                owner: loc.display_name().to_owned(),
                spelling: TurbofishSpelling::Path,
                params: pending,
            });
        }
        NamedTy {
            decl: loc.clone(),
            args: out,
        }
    }

    /// The type-level value of an expression-position const argument —
    /// restricted to the annotation-representable domain (literals and
    /// const-param reads), because the value enters TYPE identity and type
    /// identity lives on the eval-free path. Anything computed gets the
    /// clean diagnostic pointing at the generic-fn escape hatch.
    fn type_const_arg_value(&mut self, mention: ExprId, value: ExprId) -> ConstArgValue {
        match self.try_type_const_arg_value(value) {
            Ok(value) => value,
            Err(is_block) => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::TypeConstArgUnsupported {
                        expr: mention,
                        is_block,
                    });
                ConstArgValue::Error
            }
        }
    }

    /// [`Self::type_const_arg_value`] without the diagnostic: `Err(is_block)`
    /// is "outside the annotation-representable domain" — the caller
    /// decides whether that's worth reporting (a fn mention only cares when
    /// its scheme embeds the param in a type). Silently-broken cases (an
    /// overflowing literal, an unresolved name) are `Ok(Error)`: their own
    /// diagnostics cover them.
    fn try_type_const_arg_value(&self, value: ExprId) -> Result<ConstArgValue, bool> {
        match &self.body.exprs[value] {
            ExprData::Literal(LiteralData::Int(Some(v))) => Ok(ConstArgValue::Int(*v)),
            // Overflow: the literal's own diagnostic covers it.
            ExprData::Literal(LiteralData::Int(None)) => Ok(ConstArgValue::Error),
            ExprData::Literal(LiteralData::Str(s)) => {
                Ok(ConstArgValue::Str(std::sync::Arc::from(s.as_str())))
            }
            ExprData::Literal(LiteralData::Bool(b)) => Ok(ConstArgValue::Bool(*b)),
            ExprData::NameRef(_) => match (self.resolutions.get(value), &self.own_item) {
                (Some(Resolution::ConstParam(index)), Some(own)) => {
                    let name = self
                        .own_generics
                        .get(*index as usize)
                        .map(|param| param.name.as_str())
                        .unwrap_or_default();
                    Ok(ConstArgValue::Param {
                        item: own.clone(),
                        index: *index,
                        name: std::sync::Arc::from(name),
                    })
                }
                // Unresolved: the unresolved-name diagnostic covers it.
                (None, _) => Ok(ConstArgValue::Error),
                _ => Err(false),
            },
            ExprData::ConstBlock { .. } => Err(true),
            _ => Err(false),
        }
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
        // NO region relation here. Relating each branch to the FIRST
        // branch was the wrong shape twice over: it demanded mutual
        // outlives between two independent universals (an `if` over two
        // borrows became unusable, and writing the ruled `@a + @b` could
        // not rescue it), and it never related a branch to the join's
        // CONTEXT at all — which is what let an `if` launder every
        // obligation its leaves would otherwise have incurred.
        //
        // Both are the same missing edge, and it belongs where the context
        // is known: the join solver reborrows every leaf into the join's
        // result, exactly as a direct check site does.
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
        // A borrow meeting a borrow is a REBORROW, not an equation: the
        // regions get one directed edge instead of two, so the value may
        // flow into a shorter-lived position (and an exclusive one may
        // degrade to shared). Tried BEFORE unification, because unifying
        // two borrow types is invariant by design and would pin the regions
        // equal — which is exactly what reborrow-at-every-use avoids.
        if matches!(self.resolve_shallow(&actual), Ty::Borrow { .. })
            && matches!(self.resolve_shallow(expected), Ty::Borrow { .. })
        {
            if self.try_reborrow(expr, &actual, expected) {
                return self.resolve_shallow(expected);
            }
        } else if self
            .constraints
            .relate(self.table, &actual, expected, expr, cause)
        {
            return actual;
        }
        let resolved_actual = self.resolve_shallow(&actual);
        let resolved_expected = self.resolve_shallow(expected);
        if let Some(variant) =
            self.constraints
                .widen_to_enum(self.table, &resolved_actual, &resolved_expected, expr)
        {
            self.result.widened.insert(expr, variant);
            // The context's type is what flows on from here — the value is
            // tagged at this edge, so hover past it shows the enum.
            return resolved_expected;
        }
        // SAFE fn → UNSAFE fn, the one directed conversion on function
        // types and the only one there will be: a value that needs no
        // vouching is welcome in a position willing to vouch. Nothing is
        // recorded for MIR — unlike variant → enum this converts no bits,
        // it only declines a permission the position was not going to use.
        // The judgement itself lives in `Constraints`, next to the widening,
        // because the JOIN solver is a check site too and has to make
        // exactly this call.
        if self.constraints.coerce_fn_to_unsafe(
            self.table,
            &resolved_actual,
            &resolved_expected,
            expr,
        ) {
            return resolved_expected;
        }
        let mut reasons: Vec<Cause> = cause.into_iter().collect();
        // When a turbofish pinned the expected side, say so: direct checks
        // don't otherwise consult the cause store (only the join solver
        // does), and "because `T` was instantiated to `usize` by this
        // argument" is exactly the missing why.
        for recorded in self.constraints.generic_arg_causes(self.table, expected) {
            if !reasons.contains(&recorded) {
                reasons.push(recorded);
            }
        }
        // A still-free NUMBER variable on either side renders `{number}` —
        // captured eagerly, because the variable is then poisoned to
        // `{error}` below (the mismatch is the whole story; the literal
        // that minted it must not pile a no-defining-use diagnostic on
        // top), which would otherwise erase the honest rendering.
        let expected_shown = if is_unresolved_number(self.table, expected) {
            Ty::UnresolvedNumber
        } else {
            expected.clone()
        };
        let actual_shown = if is_unresolved_number(self.table, &actual) {
            Ty::UnresolvedNumber
        } else {
            actual.clone()
        };
        self.result
            .diagnostics
            .push(InferenceDiagnostic::TypeMismatch {
                expr,
                expected: expected_shown,
                actual: actual_shown,
                reasons,
            });
        poison_unresolved_number(self.table, &actual);
        poison_unresolved_number(self.table, expected);
        expected.clone()
    }

    /// Adoption — see `Constraints::adopt`. Every caller of this in the
    /// traversal has a fresh variable on one side (a join sink's result, a
    /// scrutinee variable, a pointee variable); a value meeting a CONTEXT
    /// goes through `check`, which relates.
    pub(crate) fn adopt(&mut self, a: &Ty, b: &Ty) -> bool {
        self.constraints.adopt(self.table, a, b, None)
    }

    fn resolve_shallow(&mut self, ty: &Ty) -> Ty {
        constraint::resolve_shallow(self.table, ty)
    }
}

/// The BORROW a `match` is looking through, when it is looking through
/// one — the whole of "match projects through borrows" as data.
///
/// Held beside [`Scrutinee`] rather than as a variant of it on purpose:
/// the scrutinee classification answers "what universe do the arms cover",
/// which a borrow does not change at all (the enum behind `T.&` has the
/// same variants `T` does). What the borrow changes is only what the
/// BINDERS are typed as, and that is what this carries.
struct MatchLens {
    /// The scrutinee borrow's flavor, which every payload borrow inherits.
    /// There is no per-binder mode and there is no pattern syntax for one:
    /// a `.&mut` scrutinee binds `.&mut` payloads, a `.&` scrutinee binds
    /// `.&` payloads, and copying out afterwards is spelled `t.*`.
    mutable: bool,
    /// The scrutinee's region — the upper bound on every payload's.
    region: Region,
    /// The scrutinee expression, which every projection edge is blamed on.
    /// A pattern has no `ExprId` of its own, and the scrutinee is the
    /// operation that made the projection possible, so it is the honest
    /// anchor for "this borrow does not live long enough".
    scrutinee: ExprId,
}

impl MatchLens {
    /// A type reached through this lens, at `region`.
    fn wrap(&self, region: Region, ty: Ty) -> Ty {
        Ty::borrow(self.mutable, region, ty)
    }
}

/// What a `match` scrutinee's resolved type says about the value universe
/// the arms must cover.
enum Scrutinee {
    /// Enum-typed (tagged at runtime): the arms dispatch over the
    /// declaration's variants; the mention's generic args flow into the
    /// arms' payload binding types.
    Enum(NamedTy),
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
    /// ONE value of the scrutinee, over a domain the set-cover does not
    /// enumerate — a character literal pattern. It is a real, reachable arm
    /// (so it participates in after-a-catch-all deadness) but it never
    /// contributes to exhaustiveness: `char` has 1_112_064 values, so
    /// covering it by listing is not a thing a program does, and the `_`
    /// arm stays required.
    Literal,
    /// Nothing (broken or rejected pattern, or an unreachable variant).
    Nothing,
}

/// The expressions a body consumes as a PLACE rather than as a value: the
/// receiver of a field or deref step, the base of an index step, the
/// operand of `.&`/`.&mut`/`.&raw`, and an assignment target. Only one
/// judgement depends on the distinction today —
/// [`InferCtx::finish_deref_reads`] — and it needs it for the whole body
/// at once (a `.*` is typed long before its parent is), so the set is
/// collected up front, exactly like [`InferCtx::direct_callees`].
///
/// Syntax is all this pass can see, so it is all it decides: a dot-call's
/// receiver may turn out to be a value position, but only resolution
/// knows, and [`InferCtx::value_receivers`] carries that answer.
fn place_positions(body: &Body) -> rustc_hash::FxHashSet<ExprId> {
    let mut set = rustc_hash::FxHashSet::default();
    for (_, data) in body.exprs.iter() {
        match data {
            ExprData::Field { receiver, .. } => {
                set.insert(*receiver);
            }
            // `bb.*.*`: the inner `.*` is the outer one's receiver, so it
            // is a projection step like any other — without this arm a
            // borrow behind a borrow could not be read at all.
            ExprData::Deref { receiver } => {
                set.insert(*receiver);
            }
            ExprData::Index { base, .. } => {
                set.insert(*base);
            }
            ExprData::AddrOf { place, .. } | ExprData::Borrow { place, .. } => {
                set.insert(*place);
            }
            ExprData::Block { stmts, .. } => {
                for stmt in stmts {
                    if let crate::body::Stmt::Assign { target, .. } = stmt {
                        set.insert(*target);
                    }
                }
            }
            _ => {}
        }
    }
    set
}

/// The exclusive flavor a write would need at a place step of this type —
/// the one rule, spelled in each world's own syntax. Used by the three
/// "cannot write through this" messages so they read as the twins they
/// are.
fn exclusive_flavor(ty: &Ty) -> &'static str {
    match ty {
        Ty::Borrow { .. } => "a `.&mut` borrow",
        _ => "a `.&raw mut` pointer",
    }
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
            ExprData::Unsafe { body: inner } => *inner,
            _ => return expr,
        };
    }
}

/// The type of a builtin, the one resolution and checking use. `pub` so the
/// editor's dot-completions render exactly what a call would check against.
pub fn builtin_type(builtin: Builtin, file: SourceFile) -> Ty {
    priced(builtin, builtin_type_unpriced(builtin, file))
}

/// Stamp the safety flag onto a builtin's fn type from the ONE home for the
/// question, [`Builtin::requires_unsafe`] — so a builtin cannot be unsafe to
/// call by name and safe to call through a binding, or vice versa, and so a
/// new unsafe builtin gets the right VALUE type by answering the property it
/// already has to answer. Non-fn shapes (the flavor-polymorphic builtins,
/// which have no one fn type and are not first-class) pass through.
fn priced(builtin: Builtin, ty: Ty) -> Ty {
    match ty {
        Ty::Fn(f) if builtin.requires_unsafe() => {
            Ty::unsafe_fn_type(f.params.clone(), f.ret.clone())
        }
        ty => ty,
    }
}

fn builtin_type_unpriced(builtin: Builtin, file: SourceFile) -> Ty {
    match builtin {
        Builtin::Print => Ty::fn_type(vec![Ty::Str], Ty::Unit),
        Builtin::Panic => Ty::fn_type(vec![Ty::Str], Ty::Never),
        // `read_line()` — monomorphic and nullary, like `print`/`panic`,
        // but its return type needs `file` to name the per-file
        // `ReadLineResult` declaration (the same trick `alloc_array`'s
        // scheme uses for `AllocResult`, except this builtin has no
        // generic binder of its own to route through `builtin_scheme`).
        Builtin::ReadLine => Ty::fn_type(
            Vec::new(),
            Ty::Named(NamedTy::plain(crate::read_line_result_loc(file))),
        ),
        // `s.next_char(i)` — a DOT-CALLABLE shape, so its `str` parameter
        // is LAST, which is what makes it reachable through the dot at all
        // (TR01 is structural, and it does not have one rule for builtins).
        // The same `file` trick names the per-file `NextChar` declaration.
        Builtin::NextChar => Ty::fn_type(
            vec![Ty::Int(IntKind::Usize), Ty::Str],
            Ty::Named(NamedTy::plain(crate::next_char_loc(file))),
        ),
        // `s.len()` — the receiver is the only parameter, so `next_char`'s
        // receiver-LAST shape is the whole signature here.
        Builtin::StrLen => Ty::fn_type(vec![Ty::Str], Ty::Int(IntKind::Usize)),
        // `str_bytes(s, dst)` — a written destination is `u8.&raw mut` and
        // nothing else, so unlike the blesses this one HAS a `fn` type and
        // is first-class like `print`.
        Builtin::StrBytes => Ty::fn_type(
            vec![Ty::Str, Ty::raw_ptr(true, Ty::Int(IntKind::U8))],
            Ty::Unit,
        ),
        // The generic builtins have no ONE type — every mention
        // instantiates [`builtin_scheme`] instead (see the `NameRef` and
        // `GenericApp` arms); the flavor-polymorphic builtins have no fn
        // type at all (special-cased at the call, `BuiltinNotFirstClass`
        // elsewhere). Reached only on error-recovery paths, where the
        // infectious silent type is right.
        Builtin::AllocArray
        | Builtin::DeallocArray
        | Builtin::Add
        | Builtin::Offset
        | Builtin::Copy
        | Builtin::Dangling
        // The blesses read a RANGE through either raw flavor, exactly like
        // `copy`'s source — no one `fn` type says that either.
        | Builtin::StrFromUtf8
        | Builtin::StrFromUtf8Unchecked => Ty::Error,
    }
}

/// The generic binder of a scheme-shaped builtin (`alloc_array`,
/// `dealloc_array`, `dangling` — one type param `T`), or `None` for the
/// monomorphic (`print`, `panic`) and flavor-polymorphic
/// ([`Builtin::flavor_polymorphic`]) ones. Mirrors [`crate::item_data`]'s
/// shape for generic items so mentions run the exact same instantiation
/// machinery.
fn builtin_generics(builtin: Builtin) -> Option<Vec<GenericParamData>> {
    match builtin {
        Builtin::AllocArray | Builtin::DeallocArray | Builtin::Dangling => {
            Some(vec![GenericParamData {
                name: "T".to_owned(),
                kind: GenericParamKind::Type,
                bounds: Vec::new(),
                outlives: Vec::new(),
                // The heap builtins never hold a `T`: they hand out and
                // take back POINTERS to storage. Nothing here can lose a
                // value, so nothing here needs the `forget` bound —
                // `alloc_array::<String>` is exactly as sound as
                // `alloc_array::<usize>`.
                without_forget: true,
            }])
        }
        Builtin::Print
        | Builtin::Panic
        | Builtin::Add
        | Builtin::Offset
        | Builtin::Copy
        | Builtin::ReadLine
        | Builtin::NextChar
        | Builtin::StrLen
        | Builtin::StrBytes
        | Builtin::StrFromUtf8
        | Builtin::StrFromUtf8Unchecked => None,
    }
}

/// The scheme of a generic builtin, exactly as [`signature`] would present
/// a generic item's: a `Ty::Fn` whose rigid [`crate::ty::ParamTy`]s are
/// keyed by the builtin's own reserved [`ItemLoc`] (same file-scoped
/// identity trick as [`crate::alloc_result_loc`]), so
/// [`InferCtx::instantiate_mention`] substitutes them with zero special
/// cases.
fn builtin_scheme(builtin: Builtin, file: SourceFile) -> (ItemLoc, Ty) {
    let loc = ItemLoc::top_level(
        file,
        std::sync::Arc::from(builtin.name()),
        crate::BUILTIN_DISAMBIGUATOR,
    );
    let t = Ty::Param(crate::ty::ParamTy {
        item: loc.clone(),
        index: 0,
        name: std::sync::Arc::from("T"),
    });
    let sig = match builtin {
        // `alloc_array::<T>(n)` — result-shaped (ruled A03): allocating is
        // fallible at the SIGNATURE level for codegen-era OOM; the
        // interpreter itself never produces the `Err` arm.
        Builtin::AllocArray => Ty::fn_type(
            vec![Ty::Int(IntKind::Usize)],
            Ty::Named(NamedTy {
                decl: crate::alloc_result_loc(file),
                args: vec![GenericArg::Ty(t)],
            }),
        ),
        Builtin::DeallocArray => Ty::fn_type(
            vec![Ty::raw_ptr(true, t), Ty::Int(IntKind::Usize)],
            Ty::Unit,
        ),
        Builtin::Dangling => Ty::fn_type(Vec::new(), Ty::raw_ptr(true, t)),
        Builtin::Print
        | Builtin::Panic
        | Builtin::Add
        | Builtin::Offset
        | Builtin::Copy
        | Builtin::ReadLine
        | Builtin::NextChar
        | Builtin::StrLen
        | Builtin::StrBytes
        | Builtin::StrFromUtf8
        | Builtin::StrFromUtf8Unchecked => {
            unreachable!("not a scheme-shaped builtin")
        }
    };
    // Same stamp as the monomorphic path: `dealloc_array`'s SCHEME is an
    // `unsafe fn`, so every instantiation of it is one, and taking it as a
    // value carries the call's price with it.
    (loc, priced(builtin, sig))
}

/// The type/const halves of a member's substitution, read off the OWNER's
/// generic arguments — an inherent member's scheme is keyed by the MEMBER's
/// `ItemLoc` at the owner's binder indices ([`crate::ty::member_self_ty`]),
/// so the owner's argument list substitutes into it directly. Regions in
/// this list are erased (type declarations carry no live region params);
/// the member's OWN regions are minted separately and freshly per call, by
/// `InferCtx::member_own_region_subst`.
fn owner_arg_subst(args: &[GenericArg]) -> (FxHashMap<u32, Ty>, FxHashMap<u32, ConstArgValue>) {
    let mut subst: FxHashMap<u32, Ty> = FxHashMap::default();
    let mut const_subst: FxHashMap<u32, ConstArgValue> = FxHashMap::default();
    for (index, arg) in args.iter().enumerate() {
        match arg {
            GenericArg::Ty(ty) => {
                subst.insert(index as u32, ty.clone());
            }
            GenericArg::Const(value) => {
                const_subst.insert(index as u32, value.clone());
            }
            GenericArg::Region(_) => {}
        }
    }
    (subst, const_subst)
}

/// The first same-named but DIFFERENT pair of rigid parameters, one from
/// each side of a mismatch — the pair whose shared name is why the two
/// renderings came out equal. Both sides are searched WHOLE rather than in
/// lockstep: the two occurrences need not sit at the same position (a `T`
/// under `P::<T>` on one side can meet a bare `T` on the other), and the
/// first one found is enough, because renaming it is the fix either way.
fn shadowed_param_pair<'t>(
    expected: &'t Ty,
    actual: &'t Ty,
) -> Option<(&'t crate::ty::ParamTy, &'t crate::ty::ParamTy)> {
    let (mut want, mut got) = (Vec::new(), Vec::new());
    collect_params(expected, &mut want);
    collect_params(actual, &mut got);
    want.into_iter().find_map(|w| {
        got.iter()
            .find(|g| g.name == w.name && **g != w)
            .map(|g| (w, *g))
    })
}

/// Every rigid parameter appearing anywhere in a type, in traversal order.
/// Regions and const arguments carry no [`Ty::Param`], so only the type
/// positions are walked.
fn collect_params<'t>(ty: &'t Ty, out: &mut Vec<&'t crate::ty::ParamTy>) {
    match ty {
        Ty::Param(param) => out.push(param),
        Ty::Fn(f) => {
            for param in &f.params {
                collect_params(param, out);
            }
            collect_params(&f.ret, out);
        }
        Ty::Record(rec) => {
            for (_, field) in &rec.fields {
                collect_params(field, out);
            }
        }
        Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
            for arg in args {
                if let GenericArg::Ty(ty) = arg {
                    collect_params(ty, out);
                }
            }
        }
        Ty::RawPtr { pointee, .. } => collect_params(pointee, out),
        Ty::Borrow { referent, .. } => collect_params(referent, out),
        Ty::Array { elem, .. } => collect_params(elem, out),
        Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => {}
    }
}

/// Tell two same-named rigid parameters apart, for the one message that has
/// to. Range-free and db-free like the rest of
/// [`InferenceDiagnostic::message`], which is why it speaks in BINDER
/// POSITIONS rather than "the owner's" vs "the member's own": those two
/// halves live in ONE binder (a member's is the owner's followed by its
/// own — see `crate::item_data`), so telling them apart needs the owner's
/// arity, which is a database question. The ordering rule is stated
/// instead, and it is enough to act on: rename one of them.
fn shadowed_param_note(want: &crate::ty::ParamTy, got: &crate::ty::ParamTy) -> String {
    let name = &want.name;
    // Two DIFFERENT items each declaring the name. Unreachable today, and
    // for a reason worth stating rather than asserting: a param cannot
    // escape its body (every mention of a generic item instantiates it to
    // fresh variables — see [`instantiate_scheme`]'s note), so the only way
    // two rigid params meet is inside one binder. Written for totality, and
    // because the day associated types or `dyn` change that, the message
    // that fires must not be the index one — the indices would belong to
    // two different binders and mean nothing together.
    if want.item != got.item {
        return format!(
            "two different items each declare a `{name}`: the expected one is \
             `{}`'s, the found one is `{}`'s",
            want.item.display_name(),
            got.item.display_name(),
        );
    }
    let item = match &want.item.member {
        Some((member, _)) => format!("{}::{member}", want.item.name),
        None => want.item.display_name().to_owned(),
    };
    // Binder indices count EVERY parameter kind — a member's `T` in
    // `fn::<@b, T>` on a one-parameter owner is index 2, not 1 — so the
    // message says so rather than leaving a reader who was just told
    // "owner first, then the member's own" to arrive at 1 and doubt it.
    format!(
        "`{item}` declares `{name}` twice (the owner's parameters come first, then \
         the member's own, and every kind counts — regions included): this position \
         wants the one at binder index {want_i}, the value has the one at index \
         {got_i} — rename one of them",
        want_i = want.index,
        got_i = got.index,
    )
}

/// The member's own name out of an owner-qualified spelling
/// (`Option::fresh` -> `fresh`) — what a dot-call and the named-Self value
/// form write where the qualified path is not writable.
fn bare_member_name(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// The BINDER INDICES of a member's own TYPE parameters, in order — the
/// positions a written member turbofish spells (see
/// `InferCtx::member_own_type_subst`). Regions and (reserved) consts are
/// not positions in that list, so this is deliberately not `owner_arity..`.
fn member_own_type_slots(generics: &[GenericParamData], owner_arity: usize) -> Vec<u32> {
    generics
        .iter()
        .enumerate()
        .skip(owner_arity)
        .filter(|(_, param)| matches!(param.kind, GenericParamKind::Type))
        .map(|(index, _)| index as u32)
        .collect()
}

/// The generic binder of `item` — empty for non-generic items.
fn item_generics<'db>(db: &'db dyn Db, item: ItemId<'db>) -> &'db [GenericParamData] {
    crate::item_data(db, item)
        .as_ref()
        .map(|it| it.generics.as_slice())
        .unwrap_or(&[])
}

/// Instantiate `item`'s scheme: replace each of its rigid [`Ty::Param`]s
/// with the mention's substitution (binder index → type), and each rigid
/// [`ConstArgValue::Param`] inside a generic-type mention's args with the
/// written const arg's type-level value (`fn::<const N: usize>(b:
/// Buf::<N>)` at `f::<3>` gives `Buf::<3>`; an unwritten/unrepresentable
/// value lands on `Error` — the mention carries the diagnostic). Params of
/// a *different* item never occur in a scheme today (params can't escape
/// their body), but are left untouched for totality.
fn instantiate_scheme(
    ty: &Ty,
    item: &ItemLoc,
    subst: &FxHashMap<u32, Ty>,
    const_subst: &FxHashMap<u32, ConstArgValue>,
) -> Ty {
    match ty {
        Ty::Param(param) if param.item == *item => subst
            .get(&param.index)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Ty::Fn(f) => f.rebuilt(
            f.params
                .iter()
                .map(|p| instantiate_scheme(p, item, subst, const_subst))
                .collect(),
            instantiate_scheme(&f.ret, item, subst, const_subst),
        ),
        Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(
            *mutable,
            instantiate_scheme(pointee, item, subst, const_subst),
        ),
        // A borrow's REFERENT is an ordinary scheme position: `fn::<@b,
        // T>(v: T.&mut::<@b>)` must instantiate `T` here, or the parameter
        // stays rigid and every borrowed argument mismatches against a
        // param the caller cannot possibly name. The REGION is untouched —
        // it is [`substitute_regions`]'s, deliberately a separate walk (see
        // its doc); the two compose, and neither may skip this constructor.
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(
            *mutable,
            region.clone(),
            instantiate_scheme(referent, item, subst, const_subst),
        ),
        // The scheme may embed a const param as an array LENGTH
        // (`fn::<const N: usize>(b: [usize; N])`): substitute the written
        // argument's type-level value, exactly like a generic-type
        // mention's const args below.
        Ty::Array { elem, len } => {
            let len = match len {
                ConstArgValue::Param {
                    item: param_item,
                    index,
                    ..
                } if param_item == item => const_subst
                    .get(index)
                    .cloned()
                    .unwrap_or(ConstArgValue::Error),
                other => other.clone(),
            };
            Ty::array(instantiate_scheme(elem, item, subst, const_subst), len)
        }
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        instantiate_scheme(ty, item, subst, const_subst),
                    )
                })
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: instantiate_scheme_args(&named.args, item, subst, const_subst),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: instantiate_scheme_args(&variant.args, item, subst, const_subst),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

/// Replace `item`'s rigid REGION params by a mention's fresh region
/// variables. Deliberately a walk of its own rather than a fifth parameter
/// threaded through [`instantiate_scheme`]: regions are erased, so nothing
/// else in instantiation needs to know they exist, and keeping the walk
/// separable is what lets the whole region layer be lifted out if the
/// approach turns out wrong.
fn substitute_regions(ty: &Ty, item: &ItemLoc, subst: &FxHashMap<u32, Region>) -> Ty {
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(
            *mutable,
            substitute_one_region(region, item, subst),
            substitute_regions(referent, item, subst),
        ),
        Ty::Fn(f) => f.rebuilt(
            f.params
                .iter()
                .map(|p| substitute_regions(p, item, subst))
                .collect(),
            substitute_regions(&f.ret, item, subst),
        ),
        Ty::RawPtr { mutable, pointee } => {
            Ty::raw_ptr(*mutable, substitute_regions(pointee, item, subst))
        }
        Ty::Array { elem, len } => Ty::array(substitute_regions(elem, item, subst), len.clone()),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_regions(ty, item, subst)))
                .collect(),
        ),
        // A generic type mention's ARGS can carry a borrow, and a callee's
        // return type is where they do: `-> Option::<V.&mut::<@b>>`. Left
        // out, the callee's rigid `@b` survives into the CALLER's body,
        // where the outlives solver reads a region param's binder index as
        // a node number — so it silently becomes whichever universal of the
        // caller sits at that index. `Ty::erase_regions` already recurses
        // here; this is its instantiation-time twin.
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: substitute_regions_args(&named.args, item, subst),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: substitute_regions_args(&variant.args, item, subst),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

fn substitute_regions_args(
    args: &[GenericArg],
    item: &ItemLoc,
    subst: &FxHashMap<u32, Region>,
) -> Vec<GenericArg> {
    args.iter()
        .map(|arg| match arg {
            GenericArg::Ty(ty) => GenericArg::Ty(substitute_regions(ty, item, subst)),
            GenericArg::Region(region) => {
                GenericArg::Region(substitute_one_region(region, item, subst))
            }
            // Exhaustive on purpose: see `freshen_regions_args`.
            GenericArg::Const(value) => GenericArg::Const(value.clone()),
        })
        .collect()
}

fn substitute_one_region(
    region: &Region,
    item: &ItemLoc,
    subst: &FxHashMap<u32, Region>,
) -> Region {
    match region {
        Region::Param {
            item: param_item,
            index,
            ..
        } if param_item == item => subst.get(index).cloned().unwrap_or(Region::Error),
        Region::Join(parts) => Region::Join(
            parts
                .iter()
                .map(|part| substitute_one_region(part, item, subst))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn instantiate_scheme_args(
    args: &[GenericArg],
    item: &ItemLoc,
    subst: &FxHashMap<u32, Ty>,
    const_subst: &FxHashMap<u32, ConstArgValue>,
) -> Vec<GenericArg> {
    args.iter()
        .map(|arg| match arg {
            GenericArg::Ty(ty) => GenericArg::Ty(instantiate_scheme(ty, item, subst, const_subst)),
            GenericArg::Const(ConstArgValue::Param {
                item: param_item,
                index,
                ..
            }) if param_item == item => GenericArg::Const(
                const_subst
                    .get(index)
                    .cloned()
                    .unwrap_or(ConstArgValue::Error),
            ),
            GenericArg::Const(value) => GenericArg::Const(value.clone()),
            // Regions are deliberately NOT this walk's business —
            // `substitute_regions` is a separate pass, so that the whole
            // region layer stays liftable. Stated as an arm rather than
            // left to a catch-all: exhaustive on purpose, see
            // `Constraints::freshen_regions_args`.
            GenericArg::Region(region) => GenericArg::Region(region.clone()),
        })
        .collect()
}

/// Whether `item`'s const param `index` appears anywhere in `ty` (inside a
/// generic-type mention's args) — decides whether an unrepresentable const
/// argument matters at the TYPE level (see `instantiate_mention`).
fn ty_mentions_const_param(ty: &Ty, item: &ItemLoc, index: u32) -> bool {
    let in_args = |args: &[GenericArg]| {
        args.iter().any(|arg| match arg {
            GenericArg::Region(_) => false,
            GenericArg::Ty(ty) => ty_mentions_const_param(ty, item, index),
            GenericArg::Const(ConstArgValue::Param {
                item: param_item,
                index: param_index,
                ..
            }) => param_item == item && *param_index == index,
            GenericArg::Const(_) => false,
        })
    };
    match ty {
        Ty::Fn(f) => {
            f.params
                .iter()
                .any(|p| ty_mentions_const_param(p, item, index))
                || ty_mentions_const_param(&f.ret, item, index)
        }
        // `b: [usize; N].&::<@a>` embeds the const param under a borrow —
        // same reachability question, same answer.
        Ty::Borrow { referent, .. } => ty_mentions_const_param(referent, item, index),
        Ty::RawPtr { pointee, .. } => ty_mentions_const_param(pointee, item, index),
        Ty::Record(rec) => rec
            .fields
            .iter()
            .any(|(_, ty)| ty_mentions_const_param(ty, item, index)),
        Ty::Array { elem, len } => {
            ty_mentions_const_param(elem, item, index)
                || matches!(
                    len,
                    ConstArgValue::Param {
                        item: param_item,
                        index: param_index,
                        ..
                    } if param_item == item && *param_index == index
                )
        }
        Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => in_args(args),
        _ => false,
    }
}
