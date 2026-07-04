//! The type representation and the per-item signature query.

use std::sync::Arc;

use base_db::{Db, SourceFile};
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::FxHashMap;

use crate::item_tree::{ConstArgRef, GenericArgRef, GenericParamKind, TypeRef, type_decl};
use crate::scopes::{Resolution, type_scope};
use crate::{ItemId, ItemLoc};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// An unresolved inference variable. Appears in [`crate::infer::InferenceResult`]
    /// only when inference couldn't pin the type down (rendered as `_`).
    Infer(TyVar),
    Unit,
    /// `!`: the bottom type; coerces to any expected type.
    Never,
    /// `usize` — the only integer type so far.
    Int,
    Str,
    Bool,
    Fn(Arc<FnTy>),
    /// `{ x: usize, y: str }`: a structural record. Two record types are the
    /// same type exactly when their field sets are equal (same names, same
    /// types) — no subtyping, no width coercion. Fields are always sorted by
    /// name (see [`Ty::record`]), so plain equality is field-set equality.
    Record(Arc<RecordTy>),
    /// A nominal type declared by a `type` item. Identity is the
    /// declaration (the same range-free scheme as [`ItemLoc`]) plus the
    /// FULL generic-argument list in binder order — applicative: same
    /// declaration + same args = the same type everywhere, and two
    /// `Named`s unify exactly when the declarations match and the args
    /// unify pointwise (no variance of any kind). A `Named` never unifies
    /// with the structurally-identical bare record — no implicit
    /// nominal↔structural coercion in either direction. The declared shape
    /// is *not* carried here; it is projected on demand through
    /// [`type_underlying_for`] (field access, construction) and
    /// [`variant_payloads_for`], which substitute the args for the
    /// declaration's rigid params.
    Named(NamedTy),
    /// `Shape::Circle`: the type of one variant of an enum, a first-class
    /// type of its own. It does *not* unify with its enum — a variant-typed
    /// value is tag-free at runtime, an enum-typed value is tagged, so
    /// going variant → enum is a real conversion ([`widens_to`]), never
    /// equality. Payload types are not carried here (identity only, like
    /// [`Ty::Named`] — though the enum's generic args are, so the widening
    /// conversion preserves them); they are projected through
    /// [`variant_payloads_for`].
    Variant(VariantTy),
    /// A rigid type parameter of a generic item (`T` in `fn::<T>`).
    /// Identity is `(item, index)` — the declaring item plus the position
    /// in its binder — the same range-free scheme as [`Ty::Named`] /
    /// [`VariantTy`]. Rigid: it unifies ONLY with itself (never with a
    /// concrete type, never bound like an inference variable), widens to
    /// nothing and nothing widens to it — inside the generic body the
    /// param is fully opaque (pass/store/return/`==` work; field access,
    /// calls, arithmetic get the ordinary diagnostics). Mentions of the
    /// item *instantiate* the signature, replacing these with fresh
    /// variables — a `Param` never leaks into a non-generic body's types.
    Param(ParamTy),
    /// Type of broken code. Infectious and silent: producing further
    /// diagnostics from an `Error` type would only be noise.
    Error,
}

/// Identity of a rigid type parameter: the declaring item plus the
/// parameter's position in the item's full generic binder (type and const
/// params share one index space). The name is carried for display; it is
/// determined by `(item, index)`, so including it in equality is harmless
/// (same reasoning as [`VariantTy`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamTy {
    /// The declaring generic item.
    pub item: ItemLoc,
    /// Position in the item's binder (`ItemData::generics`).
    pub index: u32,
    pub name: std::sync::Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// Identity of a nominal type mention: the declaration plus the full
/// generic-argument list in binder order. Non-generic declarations carry an
/// empty list. Range-free by construction (an [`ItemLoc`], types, and
/// literal const values) — nothing position-dependent may ever enter this
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamedTy {
    pub decl: ItemLoc,
    pub args: Vec<GenericArg>,
}

impl NamedTy {
    /// A mention of a non-generic declaration.
    pub fn plain(decl: ItemLoc) -> NamedTy {
        NamedTy {
            decl,
            args: Vec::new(),
        }
    }
}

/// One generic argument of a [`NamedTy`]/[`VariantTy`], in binder order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArg {
    Ty(Ty),
    Const(ConstArgValue),
}

/// A type-level *const* argument value. This is the annotation-representable
/// const domain — literal values and rigid const params — NOT the full
/// const-eval value domain: const eval never runs on the annotation path
/// (the firewall), so a `const { ... }` block can never parameterize a type
/// and everything here is readable straight off the syntax.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstArgValue {
    Int(u128),
    Str(std::sync::Arc<str>),
    Bool(bool),
    /// A rigid const parameter of an enclosing generic binder (`N` in
    /// `Buf::<N>` inside another generic body) — the const-side sibling of
    /// [`Ty::Param`], with the same `(item, index)` identity scheme (the
    /// name is display-only, determined by the identity).
    Param {
        item: ItemLoc,
        index: u32,
        name: std::sync::Arc<str>,
    },
    /// Broken or unrepresentable (diagnosed at the use site). Infectious
    /// and silent in comparisons, like [`Ty::Error`]: it unifies with any
    /// const value so broken mentions don't cascade.
    Error,
}

impl ConstArgValue {
    pub fn display(&self) -> String {
        match self {
            ConstArgValue::Int(v) => v.to_string(),
            ConstArgValue::Str(s) => format!("{s:?}"),
            ConstArgValue::Bool(b) => b.to_string(),
            ConstArgValue::Param { name, .. } => name.to_string(),
            ConstArgValue::Error => "{error}".to_owned(),
        }
    }
}

impl GenericArg {
    pub fn display(&self) -> String {
        match self {
            GenericArg::Ty(ty) => ty.display(),
            GenericArg::Const(value) => value.display(),
        }
    }
}

/// Render a generic-argument list as `::<a, b>`; empty for an empty list.
fn display_args(args: &[GenericArg]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let list = args
        .iter()
        .map(GenericArg::display)
        .collect::<Vec<_>>()
        .join(", ");
    format!("::<{list}>")
}

/// Identity of a variant type: the enum declaration plus the variant's
/// position in it. The name is carried for display (and for the runtime
/// value's DAP rendering); it is determined by `(decl, index)`, so including
/// it in equality is harmless.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariantTy {
    /// The declaring `type ... = enum { ... };` item.
    pub decl: ItemLoc,
    /// The enum mention's generic arguments — the same full-list identity
    /// as [`NamedTy::args`], carried so variant → enum widening preserves
    /// them and payload projection can substitute.
    pub args: Vec<GenericArg>,
    /// Position in the declaration's source-order variant list.
    pub index: u32,
    pub name: std::sync::Arc<str>,
}

/// The implicit-conversion lattice, checked where unification fails at a
/// check site (an annotation, a call argument, a join edge): `!` widens to
/// everything (divergence produces no value to convert), and a variant type
/// widens to *its* enum — that one is a real runtime conversion (the tag is
/// injected; see `mir`'s `WidenToEnum`). Nothing else widens, and only
/// shallowly: no variance through `fn` types or record fields. Unification
/// itself stays equational — `unify(Variant, Named)` is false.
///
/// Declaration-level only: the enum's generic ARGS must additionally agree
/// (widening preserves them — `Option::<usize>::Some` widens to
/// `Option::<usize>`, never to `Option::<str>`). The two check sites
/// (`InferCtx::check` and the join solver) unify the args through
/// `Constraints::widen_to_enum`; this predicate alone is not the whole
/// judgement for a generic enum.
pub fn widens_to(actual: &Ty, expected: &Ty) -> bool {
    match (actual, expected) {
        (Ty::Never, _) => true,
        (Ty::Variant(variant), Ty::Named(named)) => variant.decl == named.decl,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordTy {
    /// Sorted by field name — the canonical order for equality, unification
    /// and display. Construct via [`Ty::record`] to keep the invariant.
    pub fields: Vec<(String, Ty)>,
}

impl RecordTy {
    pub fn field_ty(&self, name: &str) -> Option<&Ty> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, ty)| ty)
    }
}

impl Ty {
    pub fn fn_type(params: Vec<Ty>, ret: Ty) -> Ty {
        Ty::Fn(Arc::new(FnTy { params, ret }))
    }

    /// A record type in canonical form: fields sorted by name. Duplicate
    /// names keep the FIRST occurrence (the stable sort preserves source
    /// order among equals) — validation already errors on the duplicate, so
    /// this is recovery, not semantics.
    pub fn record(mut fields: Vec<(String, Ty)>) -> Ty {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        fields.dedup_by(|second, first| second.0 == first.0);
        Ty::Record(Arc::new(RecordTy { fields }))
    }

    /// Whether any part of the type is an inference variable — after
    /// `resolve_fully`, an *unbound* one. `Named`/`Variant` are
    /// identity-only for the declared SHAPE (not carried here), but their
    /// generic args ARE the identity, so the traversal recurses into them
    /// (`Pair::<_>` is undetermined until the arg resolves).
    pub fn contains_infer(&self) -> bool {
        match self {
            Ty::Infer(_) => true,
            Ty::Fn(f) => f.ret.contains_infer() || f.params.iter().any(Ty::contains_infer),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_infer()),
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(GenericArg::contains_infer)
            }
            _ => false,
        }
    }

    /// Whether an fn type appears anywhere in the type. Same traversal
    /// shape (and the same declared-shape blind spot on `Named`/`Variant`,
    /// whose args are still visited) as [`Ty::contains_infer`]. Used by the
    /// mention-side belt that keeps fn values out of the const-arg domain
    /// (TR06) — the declaration-side twin checks the syntactic `TypeRef`
    /// (see `TypeRef::mentions_fn`).
    pub fn mentions_fn(&self) -> bool {
        match self {
            Ty::Fn(_) => true,
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.mentions_fn()),
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(|arg| match arg {
                    GenericArg::Ty(ty) => ty.mentions_fn(),
                    GenericArg::Const(_) => false,
                })
            }
            _ => false,
        }
    }

    pub fn contains_error(&self) -> bool {
        match self {
            Ty::Error => true,
            Ty::Fn(f) => f.ret.contains_error() || f.params.iter().any(Ty::contains_error),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_error()),
            // The declared shape stays identity-only: a broken *declaration*
            // carries its own diagnostics at the declaration site; uses of
            // the name must not cascade. But the mention's own ARGS are
            // this type's business — a broken argument (`Pair::<Unknown>`)
            // makes the whole mention broken. A rigid `Param` is
            // identity-only too — and it is *not* an inference variable
            // either (`contains_infer` must stay false for it: a
            // `Param`-typed expectation is a real, concrete expectation,
            // never "no expectation").
            Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
                args.iter().any(GenericArg::contains_error)
            }
            Ty::Param(_) => false,
            _ => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::Infer(_) => "_".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Never => "!".to_owned(),
            Ty::Int => "usize".to_owned(),
            Ty::Str => "str".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Error => "{error}".to_owned(),
            Ty::Fn(f) => {
                let params = f
                    .params
                    .iter()
                    .map(Ty::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                match &f.ret {
                    Ty::Unit => format!("fn({params})"),
                    ret => format!("fn({params}) -> {}", ret.display()),
                }
            }
            Ty::Record(rec) => {
                if rec.fields.is_empty() {
                    return "struct {}".to_owned();
                }
                let fields = rec
                    .fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", ty.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("struct {{ {fields} }}")
            }
            Ty::Named(named) => {
                format!("{}{}", named.decl.display_name(), display_args(&named.args))
            }
            Ty::Variant(variant) => {
                format!(
                    "{}{}::{}",
                    variant.decl.display_name(),
                    display_args(&variant.args),
                    variant.name
                )
            }
            Ty::Param(param) => param.name.to_string(),
        }
    }
}

impl GenericArg {
    fn contains_infer(&self) -> bool {
        match self {
            GenericArg::Ty(ty) => ty.contains_infer(),
            GenericArg::Const(_) => false,
        }
    }

    fn contains_error(&self) -> bool {
        match self {
            GenericArg::Ty(ty) => ty.contains_error(),
            GenericArg::Const(value) => matches!(value, ConstArgValue::Error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVar(u32);

impl UnifyKey for TyVar {
    type Value = TyVarValue;
    fn index(&self) -> u32 {
        self.0
    }
    fn from_index(i: u32) -> TyVar {
        TyVar(i)
    }
    fn tag() -> &'static str {
        "TyVar"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TyVarValue {
    Known(Ty),
    Unknown,
}

impl UnifyValue for TyVarValue {
    type Error = NoError;
    fn unify_values(a: &Self, b: &Self) -> Result<Self, NoError> {
        Ok(match (a, b) {
            // A poisoned variable stays poisoned: `Ty::Error` is infectious,
            // so it beats an earlier (wrong) commitment instead of the
            // commitment beating the poison.
            (TyVarValue::Known(Ty::Error), _) | (_, TyVarValue::Known(Ty::Error)) => {
                TyVarValue::Known(Ty::Error)
            }
            // Two `Known`s only meet after the inference context has already
            // unified them structurally (it resolves vars before unioning),
            // so keeping either is fine.
            (TyVarValue::Known(t), _) | (_, TyVarValue::Known(t)) => TyVarValue::Known(t.clone()),
            (TyVarValue::Unknown, TyVarValue::Unknown) => TyVarValue::Unknown,
        })
    }
}

/// The nameable builtin types. Also the authority for "is this type name
/// known?" — diagnostics use `is_none` to report unknown type names.
pub fn builtin_type_by_name(name: &str) -> Option<Ty> {
    match name {
        "usize" => Some(Ty::Int),
        "str" | "string" => Some(Ty::Str),
        "bool" => Some(Ty::Bool),
        _ => None,
    }
}

/// The generic binder of a declaration, for lowering to consult arity and
/// kinds. Empty for non-generic (and vanished) items.
fn decl_generics(db: &dyn Db, loc: &ItemLoc) -> Vec<crate::item_tree::GenericParamData> {
    crate::item_data(db, loc.to_id(db))
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default()
}

/// Resolve a name in *type* position: a `type` item wins, then the builtin
/// types, and a value item is no type at all (the diagnostics pass in
/// [`crate::file_diagnostics`] mirrors this order exactly — the two must
/// agree so every silent `Ty::Error` here has a diagnostic there).
///
/// A *generic* `type` item mentioned bare (no turbofish) is `Ty::Error`:
/// annotation lowering is syntactic, so the argument list must always be
/// spelled in type position (`Pair::<usize>`, or `Pair::<_>` where
/// inference may fill it) — the mirror pass reports the arity.
///
/// Goes through [`type_scope`] — not [`crate::scopes::file_scope`] — so
/// annotation lowering (and with it `infer`/`signature`) only depends on
/// the file's *type* items and backdates across value-item edits.
fn lower_type_path(db: &dyn Db, file: SourceFile, name: &str) -> Ty {
    match type_scope(db, file).resolve(name) {
        Some(Resolution::TypeItem(loc)) => {
            if decl_generics(db, &loc).is_empty() {
                Ty::Named(NamedTy::plain(loc))
            } else {
                Ty::Error
            }
        }
        // The duplicate definitions carry the diagnostics.
        Some(Resolution::Ambiguous(_)) => Ty::Error,
        // A value item (invisible here) or nothing: only a builtin type
        // name can save it.
        _ => builtin_type_by_name(name).unwrap_or(Ty::Error),
    }
}

/// Resolve `Enum::Variant` in *type* position to a [`Ty::Variant`]. Every
/// `Ty::Error` case (base not an enum type, unknown variant, generic enum —
/// variant types of a generic enum have no annotation spelling yet) has a
/// matching diagnostic in [`crate::file_diagnostics`]'s `PathType` pass —
/// same mirror contract as [`lower_type_path`].
fn lower_variant_type_path(db: &dyn Db, file: SourceFile, enum_name: &str, variant: &str) -> Ty {
    let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(enum_name) else {
        return Ty::Error;
    };
    if !decl_generics(db, &loc).is_empty() {
        return Ty::Error;
    }
    let Some(variants) = enum_variants(db, loc.to_id(db)).as_ref() else {
        // A struct `type` item, or a broken declaration.
        return Ty::Error;
    };
    match variants.iter().position(|(name, _)| name == variant) {
        Some(index) => Ty::Variant(VariantTy {
            decl: loc,
            args: Vec::new(),
            index: index as u32,
            name: std::sync::Arc::from(variant),
        }),
        None => Ty::Error,
    }
}

/// Lower a turbofish mention in type position (`Pair::<usize, 8>`) —
/// purely syntactic, the annotation firewall: type args lower recursively
/// (`_` mints an inference variable where a table can fill it), const args
/// are restricted to literal-shaped values and in-scope const-param names.
/// A `const { ... }` block never lowers here (const eval must not run on
/// this path) — it becomes [`ConstArgValue::Error`], and the mirror pass
/// carries the clean diagnostic.
///
/// Recovery shape (load-bearing for the mirror contract): a whole-mention
/// problem (unknown name, non-generic target, arity mismatch) is
/// `Ty::Error`; a per-argument problem (kind mismatch, unrepresentable
/// const value) breaks only that ARGUMENT (`Ty::Error` / `Error` in its
/// slot) so the mention still names the right type family and downstream
/// checking doesn't cascade.
fn lower_apply(
    db: &dyn Db,
    file: SourceFile,
    name: &str,
    written: &[GenericArgRef],
    table: &mut InPlaceUnificationTable<TyVar>,
    scope: &ParamScope,
) -> Ty {
    // A binder param takes no generic arguments (`T::<usize>`).
    if scope.types.contains_key(name) || scope.consts.contains_key(name) {
        return Ty::Error;
    }
    let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(name) else {
        return Ty::Error;
    };
    let generics = decl_generics(db, &loc);
    if generics.is_empty() || written.len() != generics.len() {
        return Ty::Error;
    }
    let args = generics
        .iter()
        .zip(written)
        .map(|(param, arg)| match (&param.kind, arg) {
            (GenericParamKind::Type, GenericArgRef::Type(type_ref)) => {
                GenericArg::Ty(lower_type_ref_in(db, file, type_ref, table, scope))
            }
            (GenericParamKind::Type, GenericArgRef::Const(_)) => GenericArg::Ty(Ty::Error),
            (GenericParamKind::Const(_), GenericArgRef::Const(value)) => {
                GenericArg::Const(lower_const_arg_ref(value, scope))
            }
            // A bare `N`: a const position whose written argument
            // parsed as a type naming an in-scope const param.
            (GenericParamKind::Const(_), GenericArgRef::Type(TypeRef::Path(path)))
                if scope.consts.contains_key(path.as_str()) =>
            {
                GenericArg::Const(scope.const_param_value(path))
            }
            (GenericParamKind::Const(_), GenericArgRef::Type(_)) => {
                GenericArg::Const(ConstArgValue::Error)
            }
        })
        .collect();
    Ty::Named(NamedTy { decl: loc, args })
}

fn lower_const_arg_ref(value: &ConstArgRef, scope: &ParamScope) -> ConstArgValue {
    match value {
        ConstArgRef::Int(v) => ConstArgValue::Int(*v),
        ConstArgRef::Str(s) => ConstArgValue::Str(std::sync::Arc::from(s.as_str())),
        ConstArgRef::Bool(b) => ConstArgValue::Bool(*b),
        ConstArgRef::Name(name) => scope.const_param_value(name),
        // Blocks are outside the annotation domain (the firewall); the
        // mirror pass carries the diagnostic.
        ConstArgRef::Block | ConstArgRef::Error => ConstArgValue::Error,
    }
}

/// The rigid-param scope a generic binder puts type annotations under:
/// type-param names resolve to rigid [`Ty::Param`]s (TR06), and const-param
/// names are nameable in *const-argument* positions (`Buf::<N>`), where
/// they resolve to rigid [`ConstArgValue::Param`]s. Empty everywhere
/// outside a generic body.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParamScope {
    /// Type-param name → its rigid [`Ty::Param`].
    pub(crate) types: FxHashMap<String, Ty>,
    /// Const-param name → its rigid identity. Const params are *value*
    /// names, not types — in plain type position they resolve to nothing
    /// (the mirror pass reports it); only const-argument positions consult
    /// this map.
    pub(crate) consts: FxHashMap<String, ConstArgValue>,
}

impl ParamScope {
    /// The rigid value of the const param `name`, or `Error` when no such
    /// param is in scope (diagnosed by the mirror pass).
    fn const_param_value(&self, name: &str) -> ConstArgValue {
        self.consts
            .get(name)
            .cloned()
            .unwrap_or(ConstArgValue::Error)
    }
}

/// Lower a syntactic type annotation under a generic binder's
/// [`ParamScope`] (empty outside generic bodies — the binder shadows
/// file-level type items and builtins, so params are consulted first).
/// References are transparent for now (`&'static str` and `str` are the
/// same type to inference).
pub(crate) fn lower_type_ref_in(
    db: &dyn Db,
    file: SourceFile,
    value: &TypeRef,
    table: &mut InPlaceUnificationTable<TyVar>,
    scope: &ParamScope,
) -> Ty {
    match value {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Fn { params, ret } => {
            let params = params
                .iter()
                .map(|param_ty| lower_type_ref_in(db, file, param_ty, table, scope))
                .collect();

            let ret = ret
                .as_ref()
                .map(|ret_ty| lower_type_ref_in(db, file, ret_ty, table, scope))
                .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));

            Ty::fn_type(params, ret)
        }
        TypeRef::Ref(type_ref) => {
            // TODO: once we introduce references this can't discard them any longer
            lower_type_ref_in(db, file, type_ref, table, scope)
        }
        TypeRef::Path(path) => match scope.types.get(path) {
            Some(param) => param.clone(),
            None => lower_type_path(db, file, path),
        },
        TypeRef::Apply { name, args } => lower_apply(db, file, name, args, table, scope),
        TypeRef::Variant { enum_name, variant } => {
            // A type param has no variants (`T::X` is silently `{error}` —
            // the diagnostics pass reports it); an unshadowed name resolves
            // as usual.
            if scope.types.contains_key(enum_name) {
                Ty::Error
            } else {
                lower_variant_type_path(db, file, enum_name, variant)
            }
        }
        TypeRef::Hole => Ty::Infer(table.new_key(TyVarValue::Unknown)),
        TypeRef::Record(fields) => Ty::record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), lower_type_ref_in(db, file, ty, table, scope)))
                .collect(),
        ),
        TypeRef::Error => Ty::Error,
    }
}

/// Lower a const param's *declared* type outside any inference context —
/// scope-less (dependent `const N: T` is rejected, TR06) and hole-free (any
/// minted variable erases to `{error}`; the declaration has nothing to
/// fill it with). Used by the annotation mirror in
/// [`crate::file_diagnostics`] for literal/forwarded const-arg checks.
pub(crate) fn lower_const_decl_ty(db: &dyn Db, file: SourceFile, type_ref: &TypeRef) -> Ty {
    let mut table = InPlaceUnificationTable::new();
    let ty = lower_type_ref_in(db, file, type_ref, &mut table, &ParamScope::default());
    if ty.contains_infer() { Ty::Error } else { ty }
}

/// The [`ParamScope`] of `item`'s generic binder.
pub(crate) fn generic_param_scope(
    db: &dyn Db,
    item: ItemId<'_>,
    generics: &[crate::item_tree::GenericParamData],
) -> ParamScope {
    let loc = crate::item_loc(db, item);
    let mut scope = ParamScope::default();
    for (index, param) in generics.iter().enumerate() {
        if param.name.is_empty() {
            continue;
        }
        match param.kind {
            GenericParamKind::Type => {
                scope.types.insert(
                    param.name.clone(),
                    Ty::Param(ParamTy {
                        item: loc.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    }),
                );
            }
            GenericParamKind::Const(_) => {
                scope.consts.insert(
                    param.name.clone(),
                    ConstArgValue::Param {
                        item: loc.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    },
                );
            }
        }
    }
    scope
}

/// The underlying record type a `type` item declares — for a GENERIC
/// declaration, the generic body: the binder's params appear as rigid
/// [`Ty::Param`]s / [`ConstArgValue::Param`]s. `None` when the declaration
/// is broken (RHS not a `struct` literal — diagnosed at the declaration).
/// Field access on a [`Ty::Named`] and construction calls project through
/// [`type_underlying_for`], which substitutes a mention's args into this
/// stored body; MIR uses its sorted field order for projections (the order
/// is arg-independent — names sort the same under any substitution).
///
/// A declaration's `TypeRef`s are mostly `Path`/`Record`/`Error` (see
/// [`crate::item_tree::type_decl`]), but a generic mention's args
/// (`next: Wrap::<_>`) can contain holes that would mint inference
/// variables; a declaration has no inference context to fill them, so any
/// variable-typed leftover is erased to `Ty::Error` here, with the matching
/// diagnostic in [`crate::file_diagnostics`]. Recursive declarations
/// (`type List = struct::<T> { next: List::<T> }`) terminate because
/// lowering a path stops at [`Ty::Named`] — nothing expands, and
/// substitution ([`substitute_args`]) stops at `Named` args the same way.
#[salsa::tracked]
pub fn type_underlying<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<Ty> {
    let crate::item_tree::TypeDeclData::Struct { fields } = type_decl(db, item).as_ref()? else {
        return None;
    };
    let file = item.file(db);
    let generics = crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let scope = generic_param_scope(db, item, &generics);
    let mut table = InPlaceUnificationTable::new();
    Some(Ty::record(
        fields
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    erase_infer(&lower_type_ref_in(db, file, ty, &mut table, &scope)),
                )
            })
            .collect(),
    ))
}

/// The variants an enum `type` item declares — `(name, payload types)` in
/// source order (a variant's index is its identity) — or `None` when the
/// item declares a struct shape or is broken. The enum-side counterpart of
/// [`type_underlying`], and the same incrementality firewall; for a generic
/// enum the payloads are the generic body (rigid params), substituted per
/// mention by [`variant_payloads_for`].
///
/// Payload `TypeRef`s come from real type syntax, so they *can* contain
/// holes (`_`) or `fn` types without a return — positions that would mint
/// inference variables. A declaration has no inference context to fill
/// them, so any variable-typed leftover is erased to `Ty::Error` here;
/// [`crate::file_diagnostics`] rejects those payloads with a diagnostic.
#[salsa::tracked(returns(ref))]
pub fn enum_variants<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<Vec<(String, Vec<Ty>)>> {
    let crate::item_tree::TypeDeclData::Enum { variants } = type_decl(db, item).as_ref()? else {
        return None;
    };
    let file = item.file(db);
    let generics = crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let scope = generic_param_scope(db, item, &generics);
    let mut table = InPlaceUnificationTable::new();
    Some(
        variants
            .iter()
            .map(|(name, payload)| {
                (
                    name.clone(),
                    payload
                        .iter()
                        .map(|ty| erase_infer(&lower_type_ref_in(db, file, ty, &mut table, &scope)))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// Erase dangling inference variables to `Ty::Error` — for lowering done
/// outside any inference context (declarations).
fn erase_infer(ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(_) => Ty::Error,
        Ty::Fn(f) => Ty::fn_type(
            f.params.iter().map(erase_infer).collect(),
            erase_infer(&f.ret),
        ),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), erase_infer(ty)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: named.args.iter().map(erase_infer_arg).collect(),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: variant.args.iter().map(erase_infer_arg).collect(),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

fn erase_infer_arg(arg: &GenericArg) -> GenericArg {
    match arg {
        GenericArg::Ty(ty) => GenericArg::Ty(erase_infer(ty)),
        GenericArg::Const(value) => GenericArg::Const(value.clone()),
    }
}

/// Substitute `decl`'s rigid binder params by `args` throughout `ty` — the
/// on-demand projection that turns a stored generic body ([`type_underlying`],
/// [`enum_variants`]) into a mention's concrete shape. A pure walk keyed by
/// `(item, index)`, exactly like fn-scheme instantiation; `Named`/`Variant`
/// args are substituted but their declared shapes never expand, so
/// self-referential generic types terminate. Out-of-range or kind-crossed
/// substitutions (broken mentions, diagnosed upstream) land on
/// `Ty::Error`/`ConstArgValue::Error`.
pub fn substitute_args(ty: &Ty, decl: &ItemLoc, args: &[GenericArg]) -> Ty {
    if args.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::Param(param) if param.item == *decl => match args.get(param.index as usize) {
            Some(GenericArg::Ty(ty)) => ty.clone(),
            _ => Ty::Error,
        },
        Ty::Fn(f) => Ty::fn_type(
            f.params
                .iter()
                .map(|param| substitute_args(param, decl, args))
                .collect(),
            substitute_args(&f.ret, decl, args),
        ),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_args(ty, decl, args)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: named
                .args
                .iter()
                .map(|arg| substitute_generic_arg(arg, decl, args))
                .collect(),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: variant
                .args
                .iter()
                .map(|arg| substitute_generic_arg(arg, decl, args))
                .collect(),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

fn substitute_generic_arg(arg: &GenericArg, decl: &ItemLoc, args: &[GenericArg]) -> GenericArg {
    match arg {
        GenericArg::Ty(ty) => GenericArg::Ty(substitute_args(ty, decl, args)),
        GenericArg::Const(ConstArgValue::Param { item, index, .. }) if item == decl => {
            match args.get(*index as usize) {
                Some(GenericArg::Const(value)) => GenericArg::Const(value.clone()),
                _ => GenericArg::Const(ConstArgValue::Error),
            }
        }
        other => other.clone(),
    }
}

/// The underlying record of a *mention* — [`type_underlying`] with the
/// mention's args substituted for the declaration's rigid params. Memoized
/// per declaration (the salsa query stores the generic body once);
/// substitution is an on-demand walk, cheap and unmemoized.
pub fn type_underlying_for(db: &dyn Db, named: &NamedTy) -> Option<Ty> {
    let ty = type_underlying(db, named.decl.to_id(db))?;
    Some(substitute_args(&ty, &named.decl, &named.args))
}

/// The payload types of one variant *mention* — [`enum_variants`]'s stored
/// generic payloads with the variant's enum args substituted. Same
/// memoization split as [`type_underlying_for`].
pub fn variant_payloads_for(db: &dyn Db, variant: &VariantTy) -> Option<Vec<Ty>> {
    let variants = enum_variants(db, variant.decl.to_id(db)).as_ref()?;
    let (_, payload) = variants.get(variant.index as usize)?;
    Some(
        payload
            .iter()
            .map(|ty| substitute_args(ty, &variant.decl, &variant.args))
            .collect(),
    )
}

/// The type other items see for `item`. A fully-typed contract is the
/// whole answer (a hard firewall edge: body edits never reach dependents) —
/// written as an annotation, or synthesized from a self-sufficient
/// fn-literal body (see [`crate::item_tree`]). Anything less — no contract,
/// or one with holes — comes from the item's binding group: its own body,
/// inferred together with everything grouped with it (mutual recursion and
/// callers alike — see [`crate::groups`]), where holes join in as
/// unconstrained variables the bodies fill in. The firewall is then salsa
/// early-cutoff: dependents re-run only when the *inferred* signature value
/// changes.
///
/// **Generic items — the scheme.** For a generic item this returns the
/// SCHEME: the `Ty::Fn` lowered from the literal's mandatory annotations
/// (TR06), with the binder's type params appearing as rigid [`Ty::Param`]s.
/// The scheme representation is deliberately just this `Ty` plus
/// `item_data(item).generics` (arity and kinds): every `Ty::Param` carries
/// its declaring item and binder index, so instantiation is a pure
/// walk-and-replace keyed by `(item, index)` with no separate binder
/// structure — see `infer`'s `instantiate_scheme`. `Ty::Error` when the
/// fully-annotated rule is violated (the definition site carries the
/// diagnostic). Generic items never join binding groups, so this never
/// consults [`crate::groups`] for them.
#[salsa::tracked]
pub fn signature<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Ty {
    if let Some(data) = crate::item_data(db, item).as_ref()
        && !data.generics.is_empty()
    {
        let Some(type_ref) = data.type_ref.as_ref() else {
            // The fully-annotated rule is violated; the definition site
            // carries the diagnostic (see `file_diagnostics`).
            return Ty::Error;
        };
        let type_params = generic_param_scope(db, item, &data.generics);
        // The scheme's `TypeRef` is fully typed by construction (see
        // `item_tree::type_ref_from_fn_literal`), so no inference variables
        // are minted and a throwaway table is sound — the same reasoning as
        // [`type_underlying`].
        let mut table = InPlaceUnificationTable::new();
        return lower_type_ref_in(db, item.file(db), type_ref, &mut table, &type_params);
    }
    if let Some(type_ref) = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
        && type_ref.is_fully_typed()
    {
        // Fully typed: no hole ever creates a variable, so the
        // lowering table stays empty and is thrown away.
        let mut table = InPlaceUnificationTable::new();
        return lower_type_ref_in(
            db,
            item.file(db),
            type_ref,
            &mut table,
            &ParamScope::default(),
        );
    }
    let Some(index) = crate::item_index(db, item) else {
        return Ty::Error;
    };
    let file = item.file(db);
    let groups = crate::groups::inference_groups(db, file);
    let Some(Some(group)) = groups.group_of.get(index).copied() else {
        return Ty::Error;
    };
    let members = &groups.groups[group as usize];
    let Some(position) = members.iter().position(|&member| member == index) else {
        return Ty::Error;
    };
    crate::groups::infer_group(db, crate::groups::GroupId::new(db, file, group))
        .signatures
        .get(position)
        .cloned()
        .unwrap_or(Ty::Error)
}

/// Whether uses of `item` should say "add a type annotation": inference
/// couldn't determine its type from the definition (group inference erases
/// undetermined leftovers to `{error}`). A hole-bearing annotation counts
/// as undetermined when the body couldn't fill the holes — a partial
/// contract that stays partial publishes no silent `{error}`.
///
/// Language decision: exported symbols will *always* require a written
/// contract, even with interprocedural inference — once a visibility notion
/// exists, exported items go back to requiring annotations; today every
/// item counts as private.
pub fn signature_needs_annotation<'db>(db: &'db dyn Db, item: ItemId<'db>) -> bool {
    if let Some(data) = crate::item_data(db, item).as_ref() {
        // A generic item is never "needs annotation": its scheme either is
        // complete (fully-annotated rule) or the *definition* carries the
        // rule's unconditional diagnostic — repeating it at every mention
        // would be noise.
        if !data.generics.is_empty() {
            return false;
        }
        if let Some(type_ref) = data.type_ref.as_ref()
            && type_ref.is_fully_typed()
        {
            return false;
        }
    }
    if crate::body::body(db, item).root.is_none() {
        // No value at all: the parse errors cover it.
        return false;
    }
    signature(db, item).contains_error()
}
