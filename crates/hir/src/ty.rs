//! The type representation and the per-item signature query.

use std::sync::Arc;

use base_db::{Db, SourceFile};
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};

use crate::item_tree::{TypeRef, type_decl};
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
    /// A nominal type declared by a `type` item. Identity *is* the
    /// declaration (the same range-free scheme as [`ItemLoc`]): two `Named`s
    /// unify exactly when they point at the same declaration, and a `Named`
    /// never unifies with the structurally-identical bare record — no
    /// implicit nominal↔structural coercion in either direction. The
    /// declared shape is *not* carried here; it is projected on demand
    /// through [`type_underlying`] (field access, construction) and
    /// [`enum_variants`].
    Named(ItemLoc),
    /// `Shape::Circle`: the type of one variant of an enum, a first-class
    /// type of its own. It does *not* unify with its enum — a variant-typed
    /// value is tag-free at runtime, an enum-typed value is tagged, so
    /// going variant → enum is a real conversion ([`widens_to`]), never
    /// equality. Payload types are not carried here (identity only, like
    /// [`Ty::Named`]); they are projected through [`enum_variants`].
    Variant(VariantTy),
    /// Type of broken code. Infectious and silent: producing further
    /// diagnostics from an `Error` type would only be noise.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// Identity of a variant type: the enum declaration plus the variant's
/// position in it. The name is carried for display (and for the runtime
/// value's DAP rendering); it is determined by `(decl, index)`, so including
/// it in equality is harmless.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariantTy {
    /// The declaring `type ... = enum { ... };` item.
    pub decl: ItemLoc,
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
pub fn widens_to(actual: &Ty, expected: &Ty) -> bool {
    match (actual, expected) {
        (Ty::Never, _) => true,
        (Ty::Variant(variant), Ty::Named(loc)) => variant.decl == *loc,
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

    pub fn contains_error(&self) -> bool {
        match self {
            Ty::Error => true,
            Ty::Fn(f) => f.ret.contains_error() || f.params.iter().any(Ty::contains_error),
            Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| ty.contains_error()),
            // Identity only: a broken *declaration* carries its own
            // diagnostics at the declaration site; uses of the name must
            // not cascade.
            Ty::Named(_) | Ty::Variant(_) => false,
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
            Ty::Named(loc) => loc.display_name().to_owned(),
            Ty::Variant(variant) => {
                format!("{}::{}", variant.decl.display_name(), variant.name)
            }
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

/// Resolve a name in *type* position: a `type` item wins, then the builtin
/// types, and a value item is no type at all (the diagnostics pass in
/// [`crate::file_diagnostics`] mirrors this order exactly — the two must
/// agree so every silent `Ty::Error` here has a diagnostic there).
///
/// Goes through [`type_scope`] — not [`crate::scopes::file_scope`] — so
/// annotation lowering (and with it `infer`/`signature`) only depends on
/// the file's *type* items and backdates across value-item edits.
fn lower_type_path(db: &dyn Db, file: SourceFile, name: &str) -> Ty {
    match type_scope(db, file).resolve(name) {
        Some(Resolution::TypeItem(loc)) => Ty::Named(loc),
        // The duplicate definitions carry the diagnostics.
        Some(Resolution::Ambiguous(_)) => Ty::Error,
        // A value item (invisible here) or nothing: only a builtin type
        // name can save it.
        _ => builtin_type_by_name(name).unwrap_or(Ty::Error),
    }
}

/// Resolve `Enum::Variant` in *type* position to a [`Ty::Variant`]. Every
/// `Ty::Error` case (base not an enum type, unknown variant) has a matching
/// diagnostic in [`crate::file_diagnostics`]'s `PathType` pass — same
/// mirror contract as [`lower_type_path`].
fn lower_variant_type_path(db: &dyn Db, file: SourceFile, enum_name: &str, variant: &str) -> Ty {
    let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(enum_name) else {
        return Ty::Error;
    };
    let Some(variants) = enum_variants(db, loc.to_id(db)).as_ref() else {
        // A struct `type` item, or a broken declaration.
        return Ty::Error;
    };
    match variants.iter().position(|(name, _)| name == variant) {
        Some(index) => Ty::Variant(VariantTy {
            decl: loc,
            index: index as u32,
            name: std::sync::Arc::from(variant),
        }),
        None => Ty::Error,
    }
}

/// Lower a syntactic type annotation. References are transparent for now
/// (`&'static str` and `str` are the same type to inference).
pub(crate) fn lower_type_ref(
    db: &dyn Db,
    file: SourceFile,
    value: &TypeRef,
    table: &mut InPlaceUnificationTable<TyVar>,
) -> Ty {
    match value {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Fn { params, ret } => {
            let params = params
                .iter()
                .map(|param_ty| lower_type_ref(db, file, param_ty, table))
                .collect();

            let ret = ret
                .as_ref()
                .map(|ret_ty| lower_type_ref(db, file, ret_ty, table))
                .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));

            Ty::fn_type(params, ret)
        }
        TypeRef::Ref(type_ref) => {
            // TODO: once we introduce references this can't discard them any longer
            lower_type_ref(db, file, type_ref, table)
        }
        TypeRef::Path(path) => lower_type_path(db, file, path),
        TypeRef::Variant { enum_name, variant } => {
            lower_variant_type_path(db, file, enum_name, variant)
        }
        TypeRef::Hole => Ty::Infer(table.new_key(TyVarValue::Unknown)),
        TypeRef::Record(fields) => Ty::record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), lower_type_ref(db, file, ty, table)))
                .collect(),
        ),
        TypeRef::Error => Ty::Error,
    }
}

/// Whether the annotation pins down every type it mentions. Holes and
/// elided fn returns don't: [`lower_type_ref`] lowers both to unconstrained
/// inference variables, so such an annotation only *constrains* inference —
/// it can't determine a signature on its own.
pub(crate) fn is_fully_typed(value: &TypeRef) -> bool {
    match value {
        TypeRef::Fn { params, ret } => {
            params.iter().all(is_fully_typed) && ret.as_deref().is_some_and(is_fully_typed)
        }
        TypeRef::Ref(inner) => is_fully_typed(inner),
        TypeRef::Record(fields) => fields.iter().all(|(_, ty)| is_fully_typed(ty)),
        TypeRef::Hole => false,
        _ => true,
    }
}

/// The underlying record type a `type` item declares, or `None` when the
/// declaration is broken (RHS not a `struct` literal — diagnosed at the
/// declaration). Field access on a [`Ty::Named`] and construction calls
/// project through this; MIR uses its sorted field order for projections.
///
/// A declaration's `TypeRef`s can only be `Path`/`Record`/`Error` (see
/// [`crate::item_tree::type_decl`]), none of which mint inference
/// variables, so lowering with a throwaway table is sound. Recursive
/// declarations (`type Foo = struct { next: Foo }`) terminate because
/// lowering a path stops at [`Ty::Named`] — nothing expands.
#[salsa::tracked]
pub fn type_underlying<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<Ty> {
    let crate::item_tree::TypeDeclData::Struct { fields } = type_decl(db, item).as_ref()? else {
        return None;
    };
    let file = item.file(db);
    let mut table = InPlaceUnificationTable::new();
    Some(Ty::record(
        fields
            .iter()
            .map(|(name, ty)| (name.clone(), lower_type_ref(db, file, ty, &mut table)))
            .collect(),
    ))
}

/// The variants an enum `type` item declares — `(name, payload types)` in
/// source order (a variant's index is its identity) — or `None` when the
/// item declares a struct shape or is broken. The enum-side counterpart of
/// [`type_underlying`], and the same incrementality firewall.
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
    let mut table = InPlaceUnificationTable::new();
    Some(
        variants
            .iter()
            .map(|(name, payload)| {
                (
                    name.clone(),
                    payload
                        .iter()
                        .map(|ty| erase_infer(&lower_type_ref(db, file, ty, &mut table)))
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
        other => other.clone(),
    }
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
#[salsa::tracked]
pub fn signature<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Ty {
    if let Some(type_ref) = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
    {
        if is_fully_typed(type_ref) {
            // Fully typed: no hole ever creates a variable, so the
            // lowering table stays empty and is thrown away.
            return lower_type_ref(
                db,
                item.file(db),
                type_ref,
                &mut InPlaceUnificationTable::new(),
            );
        }
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
    if let Some(type_ref) = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
    {
        if is_fully_typed(type_ref) {
            return false;
        }
    }
    if crate::body::body(db, item).root.is_none() {
        // No value at all: the parse errors cover it.
        return false;
    }
    signature(db, item).contains_error()
}
