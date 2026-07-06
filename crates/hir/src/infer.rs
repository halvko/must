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
    self, Cause, Constraints, Join, Witness, resolve_args_fully, resolve_fully,
};
use crate::item_tree::{Constness, GenericParamData, GenericParamKind, TypeDeclData};
use crate::scopes::{Builtin, Resolution, resolutions, type_scope};
use crate::ty::{
    ConstArgValue, GenericArg, NamedTy, ParamScope, Ty, TyVar, TyVarValue, VariantTy,
    builtin_type_by_name, enum_variants, generic_param_scope, lower_type_ref_in, signature,
    signature_needs_annotation, substitute_args, type_underlying_for,
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
    },
    /// `_` written in a *const* argument position. Const args are never
    /// inferred (TR06: running an instance backwards is
    /// inference-through-conversion, categorically refused).
    ConstArgHole {
        /// The turbofish mention expression.
        expr: ExprId,
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
        /// The generic item.
        item: ItemLoc,
        /// The undetermined type parameter's name.
        param: String,
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
    /// A flavor-polymorphic builtin (`add`, `copy`) applied to something
    /// that is not a raw pointer. These builtins accept `&raw T` AND
    /// `&raw mut T` in the same position, so the argument cannot be checked
    /// against one expected type — the mismatch gets its own diagnostic.
    BuiltinExpectsRawPtr {
        /// The call expression (where MIR refuses the operation).
        call: ExprId,
        /// The offending argument (carries the squiggle).
        arg: ExprId,
        builtin: Builtin,
        found: Ty,
    },
    /// A flavor-polymorphic builtin (`add`, `copy`) mentioned without
    /// being called. Its pointer parameter may be `&raw T` or `&raw mut T`,
    /// so it has no ONE function type to be a value at.
    BuiltinNotFirstClass {
        /// The referencing expression.
        expr: ExprId,
        builtin: Builtin,
    },
    /// `&raw`/`&raw mut` of something that is not a place — the accepted
    /// places are a variable, a chain of its fields, or a `static`
    /// (`&raw` of a temporary is refused outright, dodging rvalue
    /// promotion entirely).
    AddrOfNonPlace {
        /// The address-of expression.
        expr: ExprId,
    },
    /// `&raw mut p.*...` where the governing pointer (the receiver of the
    /// place's outermost deref) is a shared `&raw T` — minting a mutating
    /// address through it would launder the shared flavor into a write
    /// permission. The write-side twin is
    /// [`Self::AssignThroughImmutablePointer`]; `&raw` (shared) through
    /// any pointer is fine.
    AddrOfMutThroughImmutablePointer {
        /// The whole address-of expression (carries the squiggle, and
        /// where MIR refuses the value).
        addr_of: ExprId,
        /// The governing pointer's (shared) type.
        ty: Ty,
    },
    /// `&raw mut` of a place whose ROOT binding is not `mut` — the same
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
    /// `&raw mut` of a top-level item: a `static` has one place but
    /// `static mut` stays deferred, and a `const` is copied into each use —
    /// there is no place to hand out mutably either way. (`&raw` — shared —
    /// of both is fine: a static's one place, a const use's own copy.)
    AddrOfMutItem {
        /// The whole address-of expression (where MIR refuses the value).
        addr_of: ExprId,
        /// The root name expression (carries the squiggle).
        root: ExprId,
        item: ItemLoc,
        constness: Constness,
    },
    /// `p.* = v;` (or `p.*.x = v;`, any deref-rooted chain) where the
    /// governing pointer — the receiver of the target's outermost deref —
    /// is a `&raw T`: writing through a pointer requires `&raw mut T`.
    /// (`p` itself need not be a `mut` binding: writing through it does
    /// not reassign it. Derefs deeper in the chain are ordinary *reads*,
    /// so their pointers' flavors don't matter.)
    AssignThroughImmutablePointer {
        /// The governing deref expression (the target itself for
        /// `p.* = v;`, the chain's outermost deref otherwise).
        target: ExprId,
        /// The pointer's type.
        ty: Ty,
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
            | InferenceDiagnostic::ContinueOutsideLoop { expr }
            | InferenceDiagnostic::GenericArgCount { expr, .. }
            | InferenceDiagnostic::NotGeneric { expr, .. }
            | InferenceDiagnostic::ConstArgHole { expr }
            | InferenceDiagnostic::GenericArgKindMismatch { expr, .. }
            | InferenceDiagnostic::MissingConstArgs { expr, .. }
            | InferenceDiagnostic::CannotInferGenericParam { expr, .. }
            | InferenceDiagnostic::FnConstArg { expr }
            | InferenceDiagnostic::TypeConstArgUnsupported { expr, .. }
            | InferenceDiagnostic::DerefNonPointer { expr, .. }
            | InferenceDiagnostic::IndexNonArray { expr, .. }
            | InferenceDiagnostic::IndexOutOfBounds { expr, .. }
            | InferenceDiagnostic::EmptyArrayNeedsAnnotation { expr }
            | InferenceDiagnostic::ArrayConstArg { expr }
            | InferenceDiagnostic::BuiltinNotFirstClass { expr, .. }
            | InferenceDiagnostic::AddrOfNonPlace { expr } => *expr,
            InferenceDiagnostic::BuiltinExpectsRawPtr { arg, .. } => *arg,
            InferenceDiagnostic::AddrOfMutImmutable { root, .. }
            | InferenceDiagnostic::AddrOfMutItem { root, .. } => *root,
            InferenceDiagnostic::AddrOfMutThroughImmutablePointer { addr_of, .. } => *addr_of,
            InferenceDiagnostic::AssignThroughImmutablePointer { target, .. } => *target,
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
                if let (Ty::Named(named), Ty::Record(_)) = (expected, actual) {
                    format!(
                        "{base}; `{name}` is a distinct type — construct it with `{name}(...)`",
                        name = named.decl.display_name()
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
            InferenceDiagnostic::GenericArgCount {
                item,
                expected,
                found,
                ..
            } => crate::diag::generic_arg_count(item.display_name(), *expected, *found),
            InferenceDiagnostic::NotGeneric { name, .. } => {
                crate::diag::takes_no_generic_args(name)
            }
            InferenceDiagnostic::ConstArgHole { .. } => crate::diag::CONST_ARG_HOLE.to_owned(),
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
            InferenceDiagnostic::CannotInferGenericParam { item, param, .. } => {
                format!(
                    "cannot infer the type parameter `{param}` of `{}`; \
                     write `{}::<...>` to specify it",
                    item.display_name(),
                    item.display_name()
                )
            }
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
                    "`{}` expects a raw pointer (`&raw T` or `&raw mut T`) here, found `{}`",
                    builtin.name(),
                    found.display()
                )
            }
            InferenceDiagnostic::BuiltinNotFirstClass { builtin, .. } => {
                format!(
                    "`{}` must be called directly; its pointer parameter accepts both \
                     `&raw T` and `&raw mut T`, so it has no one function type to be \
                     a value at",
                    builtin.name()
                )
            }
            InferenceDiagnostic::AddrOfNonPlace { .. } => {
                "`&raw` can only take the address of a variable, one of its fields, \
                 or a `static`"
                    .to_owned()
            }
            InferenceDiagnostic::AddrOfMutThroughImmutablePointer { ty, .. } => format!(
                "cannot take `&raw mut` through `{}`: minting a mutating address \
                 needs a `&raw mut` pointer",
                ty.display()
            ),
            InferenceDiagnostic::AddrOfMutImmutable { name, place, .. } => {
                if place == name {
                    format!("cannot take `&raw mut` of `{name}`: it is not declared `mut`")
                } else {
                    // A field chain: the root binding carries the blame —
                    // mutability is transitive, exactly as for assignments.
                    format!("cannot take `&raw mut` of `{place}`: `{name}` is not declared `mut`")
                }
            }
            InferenceDiagnostic::AddrOfMutItem {
                item, constness, ..
            } => match constness {
                Constness::Static => format!(
                    "cannot take `&raw mut` of `{}`: `static mut` is not supported yet",
                    item.display_name()
                ),
                Constness::Const => format!(
                    "cannot take `&raw mut` of `{}`: a `const` is copied into each use, \
                     so there is no place to modify",
                    item.display_name()
                ),
            },
            InferenceDiagnostic::AssignThroughImmutablePointer { ty, .. } => format!(
                "cannot assign through `{}`: writing needs a `&raw mut` pointer",
                ty.display()
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

    // The item's own generic binder (TR06): its type params resolve to rigid
    // `Ty::Param`s in every type position lowered inside this body, and its
    // const params are value names (resolved by `scopes`). The whole body
    // is inside the binder — for a generic item the root IS the binder's
    // literal — so one body-wide scope is exact.
    let generics: &[GenericParamData] = crate::item_data(db, item)
        .as_ref()
        .map(|it| it.generics.as_slice())
        .unwrap_or(&[]);
    let type_params = if generics.is_empty() {
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
}

/// One instantiation of a generic item's scheme at a mention: which fresh
/// variable stands for which of the item's type params.
struct PendingInstantiation {
    /// The mentioning expression (a `NameRef` or `GenericApp`).
    expr: ExprId,
    item: ItemLoc,
    /// `(param name, the fresh variable)` per *type* param.
    params: Vec<(String, Ty)>,
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
            type_params: ParamScope::default(),
            own_generics: &[],
            own_item: None,
            pending_instantiations: Vec::new(),
            pending_empty_arrays: Vec::new(),
        }
    }

    /// Lower a type annotation under this body's generic binder (the
    /// param scope is empty outside generic items). Every annotation
    /// lowered during inference of this body must go through here, not
    /// through the free function — that is what makes `x: T` inside a
    /// generic body resolve to the rigid param.
    fn lower_type_ref(&mut self, type_ref: &TypeRef) -> Ty {
        lower_type_ref_in(self.db, self.file, type_ref, self.table, &self.type_params)
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
        // Instantiations the whole traversal (joins included) never pinned:
        // the mention-site sibling of `NeedsAnnotation` — the definition is
        // fine, this particular use just doesn't say which type it wants.
        let pending = std::mem::take(&mut self.pending_instantiations);
        for instantiation in pending {
            for (param, var) in instantiation.params {
                if resolve_fully(self.table, &var).contains_infer() {
                    self.result
                        .diagnostics
                        .push(InferenceDiagnostic::CannotInferGenericParam {
                            expr: instantiation.expr,
                            item: instantiation.item.clone(),
                            param,
                        });
                }
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
        let mut result = std::mem::take(&mut self.result);
        for (_, ty) in result.type_of_expr.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        for (_, ty) in result.type_of_binding.iter_mut() {
            *ty = resolve_fully(self.table, ty);
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
                InferenceDiagnostic::PatNotRecord { ty, .. }
                | InferenceDiagnostic::DerefNonPointer { ty, .. }
                | InferenceDiagnostic::IndexNonArray { ty, .. }
                | InferenceDiagnostic::AssignThroughImmutablePointer { ty, .. }
                | InferenceDiagnostic::AddrOfMutThroughImmutablePointer { ty, .. } => {
                    *ty = resolve_fully(self.table, ty);
                }
                InferenceDiagnostic::PatNamedTypeMismatch {
                    expected, actual, ..
                } => {
                    *expected = resolve_fully(self.table, expected);
                    *actual = resolve_fully(self.table, actual);
                }
                InferenceDiagnostic::BuiltinExpectsRawPtr { found, .. } => {
                    *found = resolve_fully(self.table, found);
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
                | InferenceDiagnostic::PatUnknownType { .. }
                | InferenceDiagnostic::GenericArgCount { .. }
                | InferenceDiagnostic::NotGeneric { .. }
                | InferenceDiagnostic::ConstArgHole { .. }
                | InferenceDiagnostic::GenericArgKindMismatch { .. }
                | InferenceDiagnostic::MissingConstArgs { .. }
                | InferenceDiagnostic::CannotInferGenericParam { .. }
                | InferenceDiagnostic::AssignToConstParam { .. }
                | InferenceDiagnostic::FnConstArg { .. }
                | InferenceDiagnostic::TypeConstArgUnsupported { .. }
                | InferenceDiagnostic::AddrOfNonPlace { .. }
                | InferenceDiagnostic::AddrOfMutImmutable { .. }
                | InferenceDiagnostic::AddrOfMutItem { .. }
                | InferenceDiagnostic::IndexOutOfBounds { .. }
                | InferenceDiagnostic::EmptyArrayNeedsAnnotation { .. }
                | InferenceDiagnostic::ArrayConstArg { .. }
                | InferenceDiagnostic::BuiltinNotFirstClass { .. } => {}
            }
        }
        for (_, ty) in result.type_of_pat.iter_mut() {
            *ty = resolve_fully(self.table, ty);
        }
        // Expectations: resolve like `type_of_expr`, but *drop* what
        // doesn't resolve to a concrete type — an unbound variable means
        // the position had no real expectation, and an `{error}` means the
        // expectation itself came from broken code (see the field's doc
        // for why both must go).
        let recorded = std::mem::take(&mut result.expectation_of_expr);
        for (expr, ty) in recorded.iter() {
            let resolved = resolve_fully(self.table, ty);
            if resolved.contains_infer() || resolved.contains_error() {
                continue;
            }
            result.expectation_of_expr.insert(expr, resolved);
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
            } => self.infer_variant_path(expr, *base, variant, args.as_deref()),
            ExprData::GenericApp { base, args } => self.infer_generic_app(expr, *base, args),
            ExprData::Call { callee, args } => {
                // A construction call: the type name used as a plain
                // constructor function taking the underlying record —
                // `Foo(struct { x: 1 })`, or `Pair::<usize>(...)` with the
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
                // The flavor-polymorphic builtins (`add`, `copy`) are
                // intercepted like construction heads: their callee has no
                // one function type to infer, so the call itself is the
                // special case (see `infer_builtin_special_call`).
                if let Some(Resolution::Builtin(builtin @ (Builtin::Add | Builtin::Copy))) =
                    self.resolutions.get(*callee)
                {
                    let builtin = *builtin;
                    return {
                        let ty = self.infer_builtin_special_call(expr, builtin, args);
                        let ty = self.check(expr, ty, expected, cause);
                        self.result.type_of_expr.insert(expr, ty.clone());
                        ty
                    };
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
                            // is the pointer's `&raw mut`-ness, not any
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
                        // shape — with the mention's generic args
                        // substituted in: `p.a` on a `Pair::<usize>` is a
                        // `usize`.
                        Ty::Named(named) => {
                            match type_underlying_for(self.db, &named) {
                                Some(Ty::Record(rec)) => match rec.field_ty(name) {
                                    Some(field_ty) => field_ty.clone(),
                                    None => {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::NoSuchField {
                                                expr,
                                                name: name.clone(),
                                                receiver_ty: Ty::Named(named),
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
                                    if enum_variants(self.db, named.decl.to_id(self.db)).is_some() {
                                        self.result.diagnostics.push(
                                            InferenceDiagnostic::NoSuchField {
                                                expr,
                                                name: name.clone(),
                                                receiver_ty: Ty::Named(named),
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
                            self.unify(&result, &ty);
                        }
                        _ => {
                            // Deferral exists so axioms arriving later can
                            // pick the winner before witnesses are played
                            // against each other — when every element
                            // already resolves to ONE concrete type there
                            // is nothing left to decide, and resolving
                            // eagerly keeps the element type usable
                            // *during* traversal (indexing is the array's
                            // primary operation: `m[1][0]` / `pts[1].x`
                            // on a nested literal must project right
                            // away, which a deferred join can't offer).
                            let resolved: Vec<Ty> = witnesses
                                .iter()
                                .map(|witness| resolve_fully(self.table, &witness.ty))
                                .collect();
                            let unanimous = !resolved[0].contains_infer()
                                && !resolved[0].contains_error()
                                && resolved.iter().all(|ty| *ty == resolved[0]);
                            if unanimous {
                                self.unify(&result, &resolved[0]);
                            } else {
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
                // The count is a `usize`...
                self.infer_expr_with(count, &Ty::Int, None);
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
                self.infer_expr_with(index, &Ty::Int, None);
                match self.resolve_shallow(&base_ty) {
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
                }
            }
            // `&raw place` / `&raw mut place`: the operand reads like any
            // expression (so field chains get their diagnostics on the
            // way), then the place rules are judged on its structure.
            ExprData::AddrOf { mutable, place } => {
                let mutable = *mutable;
                let place = *place;
                let fresh = self.fresh_var();
                let place_ty = self.infer_expr(place, &fresh);
                self.check_addr_of_place(expr, mutable, place);
                Ty::raw_ptr(mutable, place_ty)
            }
            // `p.*`: reads through the pointer — `&raw T` and `&raw mut T`
            // both deref-read to `T` (writing is the assignment path's
            // judgement).
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                let fresh = self.fresh_var();
                let receiver_ty = self.infer_expr(receiver, &fresh);
                match self.resolve_shallow(&receiver_ty) {
                    Ty::RawPtr { pointee, .. } => (*pointee).clone(),
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
                        let ty = if let PatData::Bind(binding) = &self.body.pats[param.pat] {
                            // The common case, unchanged: a bare name's own
                            // annotation (if any) is the axiom.
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
                        };
                        // A parameter pattern has no per-call site to blame
                        // a broken destructure on; the whole body is the
                        // best available anchor (every call runs it).
                        self.check_pat(param.pat, &ty, *fn_body);
                        ty
                    })
                    .collect();
                let ret = match ret_type {
                    Some(type_ref) => self.lower_type_ref(type_ref),
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
                ExprData::Field { receiver, name } => {
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
            // A deref roots the chain (`p.* = e;`, `p.*.x = e;`): the
            // write goes through the pointer, so the judgement is the
            // GOVERNING pointer's `&raw mut`-ness — the receiver of the
            // chain's outermost deref (derefs deeper down are ordinary
            // reads; their flavors don't matter). No binding-`mut` rule
            // applies: writing through a pointer reassigns nothing.
            if let ExprData::Deref { receiver } = &self.body.exprs[root] {
                let receiver_ty = self
                    .result
                    .type_of_expr
                    .get(*receiver)
                    .cloned()
                    .unwrap_or(Ty::Error);
                if let resolved @ Ty::RawPtr { mutable: false, .. } =
                    self.resolve_shallow(&receiver_ty)
                {
                    self.result.diagnostics.push(
                        InferenceDiagnostic::AssignThroughImmutablePointer {
                            target: root,
                            ty: resolved,
                        },
                    );
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
            Some(Resolution::TypeItem(_)) => None,
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

    /// The place rules for `&raw place` / `&raw mut place`, judged on the
    /// operand's structure after it was read-typed: the accepted places
    /// are a variable, a chain of its fields and elements, a
    /// `static`/`const` item (a `const` use's own copy — const=copied,
    /// now observable), or a deref-rooted chain (`&raw mut p.*.x` — a
    /// pointer into the pointee, the original allocation's address with
    /// an extended path). The `mut` flavor additionally requires the ROOT
    /// binding to be `mut` — the same transitive-mutability rule
    /// assignments use — refuses items (`static mut` is deferred; a
    /// `const` has no place to hand out mutably), and, for deref-rooted
    /// places, requires the GOVERNING pointer (the outermost deref's
    /// receiver) to be `&raw mut` itself.
    fn check_addr_of_place(&mut self, addr_of: ExprId, mutable: bool, place: ExprId) {
        let mut segments: Vec<String> = Vec::new();
        let mut root = place;
        loop {
            match &self.body.exprs[root] {
                ExprData::Field { receiver, name } => {
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
                    // A compile-time value, a type, a builtin: none of
                    // them is a place.
                    Some(
                        Resolution::ConstParam(_)
                        | Resolution::TypeItem(_)
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
            // `&raw [mut] p.*...`: a pointer into the pointee — the
            // result is the original allocation's address with an
            // extended path. The `mut` flavor is judged on the GOVERNING
            // pointer (this outermost deref's receiver): a shared `&raw T`
            // must not launder into a write permission. Deeper derefs are
            // ordinary reads, already typed (and unsafe-checked) on their
            // own.
            ExprData::Deref { receiver } => {
                if mutable {
                    let receiver_ty = self
                        .result
                        .type_of_expr
                        .get(*receiver)
                        .cloned()
                        .unwrap_or(Ty::Error);
                    if let resolved @ Ty::RawPtr { mutable: false, .. } =
                        self.resolve_shallow(&receiver_ty)
                    {
                        self.result.diagnostics.push(
                            InferenceDiagnostic::AddrOfMutThroughImmutablePointer {
                                addr_of,
                                ty: resolved,
                            },
                        );
                    }
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
    fn infer_variant_path(
        &mut self,
        expr: ExprId,
        base: ExprId,
        variant: &str,
        args: Option<&[GenericArgData]>,
    ) -> Ty {
        match self.resolutions.get(base) {
            Some(Resolution::TypeItem(loc)) => {
                let loc = loc.clone();
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
                        self.result
                            .diagnostics
                            .push(InferenceDiagnostic::NoSuchVariant {
                                expr,
                                item: loc.clone(),
                                name: variant.to_owned(),
                            });
                        self.infer_const_args_free(args.unwrap_or(&[]));
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
    fn instantiate_mention(
        &mut self,
        expr: ExprId,
        loc: ItemLoc,
        sig: Ty,
        generics: &[GenericParamData],
        args: Option<&[GenericArgData]>,
    ) -> Ty {
        let matched_args = match args {
            Some(args) if args.len() != generics.len() => {
                self.result
                    .diagnostics
                    .push(InferenceDiagnostic::GenericArgCount {
                        expr,
                        item: loc.clone(),
                        expected: generics.len(),
                        found: args.len(),
                    });
                // No positional matching is trustworthy; the const-value
                // expressions are still inferred (freely) so their
                // contents get types and diagnostics.
                self.infer_const_args_free(args);
                None
            }
            Some(args) => Some(args),
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
        let mut subst: FxHashMap<u32, Ty> = FxHashMap::default();
        let mut const_subst: FxHashMap<u32, ConstArgValue> = FxHashMap::default();
        let mut pending: Vec<(String, Ty)> = Vec::new();
        let mut const_args: Vec<(u32, ExprId)> = Vec::new();
        for (index, param) in generics.iter().enumerate() {
            let written = matched_args.map(|args| &args[index]);
            match &param.kind {
                GenericParamKind::Type => {
                    let var = self.fresh_var();
                    match written {
                        Some(GenericArgData::Type(type_ref)) => {
                            // Written under the MENTION's own binder scope:
                            // a turbofish inside a generic body may say
                            // `id::<T>`. A `_` lowers to a fresh variable —
                            // explicitly "infer this one".
                            let written_ty = self.lower_type_ref(type_ref);
                            self.constraints.unify(
                                self.table,
                                &var,
                                &written_ty,
                                Some(Cause::GenericArg {
                                    mention: expr,
                                    index: index as u32,
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
                        None => {}
                    }
                    pending.push((param.name.clone(), var.clone()));
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
                            Some(Cause::GenericArg {
                                mention: expr,
                                index: index as u32,
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
                    // Already reported: `MissingConstArgs` (bare mention)
                    // or `GenericArgCount` (unmatchable list).
                    None => {}
                },
            }
        }
        if !const_args.is_empty() {
            self.result.const_args_of_expr.insert(expr, const_args);
        }
        if !sig.contains_error() && !pending.is_empty() {
            // A broken scheme (fully-annotated rule violated) is excluded:
            // the definition carries the diagnostic, and piling
            // cannot-infer noise on every mention would drown it.
            self.pending_instantiations.push(PendingInstantiation {
                expr,
                item: loc.clone(),
                params: pending,
            });
        }
        instantiate_scheme(&sig, &loc, &subst, &const_subst)
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
                    self.push_not_generic(expr, &base_name);
                    self.infer_const_args_free(args);
                    return member_sig;
                }
                let target = loc.to_id(self.db);
                let generics = item_generics(self.db, target);
                if generics.is_empty() {
                    self.push_not_generic(expr, &base_name);
                    self.infer_const_args_free(args);
                    return signature(self.db, target);
                }
                let sig = signature(self.db, target);
                self.instantiate_mention(expr, loc, sig, generics, Some(args))
            }
            Some(Resolution::Local(binding)) => {
                let binding = *binding;
                self.push_not_generic(expr, &base_name);
                self.infer_const_args_free(args);
                self.result
                    .type_of_binding
                    .get(binding)
                    .cloned()
                    .unwrap_or(Ty::Error)
            }
            Some(Resolution::ConstParam(index)) => {
                let index = *index;
                self.push_not_generic(expr, &base_name);
                self.infer_const_args_free(args);
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
            // The duplicate-definition/unresolved-name diagnostics sit on
            // the base.
            Some(Resolution::Ambiguous(_)) | None => {
                self.infer_const_args_free(args);
                Ty::Error
            }
        }
    }

    fn push_not_generic(&mut self, expr: ExprId, name: &str) {
        self.result
            .diagnostics
            .push(InferenceDiagnostic::NotGeneric {
                expr,
                name: name.to_owned(),
            });
    }

    /// A mention of a builtin, bare (`args: None`) or turbofished. The
    /// monomorphic builtins keep their one fixed type; the scheme-shaped
    /// ones ([`builtin_generics`]) run the exact same instantiation
    /// machinery as generic fn items; the flavor-polymorphic pair
    /// (`add`/`copy`) has no first-class type at all — its one legal
    /// position, a direct call, is intercepted in the `Call` arm before
    /// the callee would be inferred, so reaching here IS the error.
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
            self.push_not_generic(expr, builtin.name());
            self.infer_const_args_free(args);
            return builtin_type(builtin);
        }
        if matches!(builtin, Builtin::Add | Builtin::Copy) {
            self.result
                .diagnostics
                .push(InferenceDiagnostic::BuiltinNotFirstClass { expr, builtin });
            return Ty::Error;
        }
        builtin_type(builtin)
    }

    /// A direct call of a flavor-polymorphic builtin — the checker special
    /// case the ruled spec asks for: `add` preserves its pointer
    /// argument's flavor (`&raw mut` in → `&raw mut` out) and `copy`
    /// accepts either flavor for `src`, neither of which one `fn` type can
    /// say. Everything else about the call is the ordinary machinery
    /// (argument expectations with causes, arity as `ArgCountMismatch`).
    fn infer_builtin_special_call(
        &mut self,
        call: ExprId,
        builtin: Builtin,
        args: &[ExprId],
    ) -> Ty {
        let expected_arity = match builtin {
            Builtin::Add => 2,
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
            // allocation, same flavor.
            Builtin::Add => {
                let ptr = ptr_arg(self, args[0]);
                self.infer_expr_with(
                    args[1],
                    &Ty::Int,
                    Some(Cause::CallSite { call, arg: args[1] }),
                );
                match ptr {
                    Some((mutable, pointee)) => Ty::RawPtr { mutable, pointee },
                    None => Ty::Error,
                }
            }
            // `copy(src, dst, n)`: `src` may be either flavor, `dst` must
            // be `&raw mut`, and the pointees must agree — `dst` is
            // checked against `&raw mut <src's pointee>` so the mismatch
            // diagnostics are the ordinary type-mismatch ones.
            Builtin::Copy => {
                let elem = self.fresh_var();
                if let Some((_, pointee)) = ptr_arg(self, args[0]) {
                    self.unify(&elem, pointee.as_ref());
                }
                self.infer_expr_with(
                    args[1],
                    &Ty::raw_ptr(true, elem),
                    Some(Cause::CallSite { call, arg: args[1] }),
                );
                self.infer_expr_with(
                    args[2],
                    &Ty::Int,
                    Some(Cause::CallSite { call, arg: args[2] }),
                );
                Ty::Unit
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
                    self.unify(&scrut_ty, &Ty::Named(named));
                    break;
                }
            }
        }
        let scrut = match self.resolve_shallow(&scrut_ty) {
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
                            .map(|(_, (name, _))| format!("{}::{name}", named.decl.display_name()))
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
        let mut pending: Vec<(String, Ty)> = Vec::new();
        let mut out: Vec<GenericArg> = Vec::with_capacity(generics.len());
        for (index, param) in generics.iter().enumerate() {
            let arg = matched.map(|args| &args[index]);
            match &param.kind {
                GenericParamKind::Type => {
                    let var = self.fresh_var();
                    match arg {
                        Some(GenericArgData::Type(type_ref)) => {
                            let written_ty = self.lower_type_ref(type_ref);
                            self.constraints.unify(
                                self.table,
                                &var,
                                &written_ty,
                                Some(Cause::GenericArg {
                                    mention,
                                    index: index as u32,
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
                        None => {}
                    }
                    pending.push((param.name.clone(), var.clone()));
                    out.push(GenericArg::Ty(var));
                }
                GenericParamKind::Const(declared) => {
                    let value = match arg {
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
                                    index: index as u32,
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
                        // mention) or `GenericArgCount` (unmatchable list).
                        None => ConstArgValue::Error,
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
        if let Some(variant) =
            self.constraints
                .widen_to_enum(self.table, &resolved_actual, &resolved_expected)
        {
            self.result.widened.insert(expr, variant);
            // The context's type is what flows on from here — the value is
            // tagged at this edge, so hover past it shows the enum.
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
        self.result
            .diagnostics
            .push(InferenceDiagnostic::TypeMismatch {
                expr,
                expected: expected.clone(),
                actual,
                reasons,
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
            ExprData::Unsafe { body: inner } => *inner,
            _ => return expr,
        };
    }
}

fn builtin_type(builtin: Builtin) -> Ty {
    match builtin {
        Builtin::Print => Ty::fn_type(vec![Ty::Str], Ty::Unit),
        Builtin::Panic => Ty::fn_type(vec![Ty::Str], Ty::Never),
        // The generic builtins have no ONE type — every mention
        // instantiates [`builtin_scheme`] instead (see the `NameRef` and
        // `GenericApp` arms); the flavor-polymorphic pair has no fn type
        // at all (special-cased at the call, `BuiltinNotFirstClass`
        // elsewhere). Reached only on error-recovery paths, where the
        // infectious silent type is right.
        Builtin::AllocArray
        | Builtin::DeallocArray
        | Builtin::Add
        | Builtin::Copy
        | Builtin::Dangling => Ty::Error,
    }
}

/// The generic binder of a scheme-shaped builtin (`alloc_array`,
/// `dealloc_array`, `dangling` — one type param `T`), or `None` for the
/// monomorphic (`print`, `panic`) and flavor-polymorphic (`add`, `copy`)
/// ones. Mirrors [`crate::item_data`]'s shape for generic items so mentions
/// run the exact same instantiation machinery.
fn builtin_generics(builtin: Builtin) -> Option<Vec<GenericParamData>> {
    match builtin {
        Builtin::AllocArray | Builtin::DeallocArray | Builtin::Dangling => {
            Some(vec![GenericParamData {
                name: "T".to_owned(),
                kind: GenericParamKind::Type,
            }])
        }
        Builtin::Print | Builtin::Panic | Builtin::Add | Builtin::Copy => None,
    }
}

/// The scheme of a generic builtin, exactly as [`signature`] would present
/// a generic item's: a `Ty::Fn` whose rigid [`crate::ty::ParamTy`]s are
/// keyed by the builtin's own reserved [`ItemLoc`] (same file-scoped
/// identity trick as [`crate::alloc_result_loc`]), so
/// [`InferCtx::instantiate_mention`] substitutes them with zero special
/// cases.
fn builtin_scheme(builtin: Builtin, file: SourceFile) -> (ItemLoc, Ty) {
    let loc = ItemLoc {
        file,
        name: std::sync::Arc::from(builtin.name()),
        disambiguator: crate::BUILTIN_DISAMBIGUATOR,
    };
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
            vec![Ty::Int],
            Ty::Named(NamedTy {
                decl: crate::alloc_result_loc(file),
                args: vec![GenericArg::Ty(t)],
            }),
        ),
        Builtin::DeallocArray => Ty::fn_type(vec![Ty::raw_ptr(true, t), Ty::Int], Ty::Unit),
        Builtin::Dangling => Ty::fn_type(Vec::new(), Ty::raw_ptr(true, t)),
        Builtin::Print | Builtin::Panic | Builtin::Add | Builtin::Copy => {
            unreachable!("not a scheme-shaped builtin")
        }
    };
    (loc, sig)
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
        Ty::Fn(f) => Ty::fn_type(
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
            other => other.clone(),
        })
        .collect()
}

/// Whether `item`'s const param `index` appears anywhere in `ty` (inside a
/// generic-type mention's args) — decides whether an unrepresentable const
/// argument matters at the TYPE level (see `instantiate_mention`).
fn ty_mentions_const_param(ty: &Ty, item: &ItemLoc, index: u32) -> bool {
    let in_args = |args: &[GenericArg]| {
        args.iter().any(|arg| match arg {
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
