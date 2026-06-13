//! The type representation and the per-item signature query.

use std::sync::Arc;

use base_db::Db;
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};

use crate::ItemId;
use crate::item_tree::TypeRef;

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
    /// Type of broken code. Infectious and silent: producing further
    /// diagnostics from an `Error` type would only be noise.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

impl Ty {
    pub fn fn_type(params: Vec<Ty>, ret: Ty) -> Ty {
        Ty::Fn(Arc::new(FnTy { params, ret }))
    }

    pub fn contains_error(&self) -> bool {
        match self {
            Ty::Error => true,
            Ty::Fn(f) => f.ret.contains_error() || f.params.iter().any(Ty::contains_error),
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

/// Lower a syntactic type annotation. References are transparent for now
/// (`&'static str` and `str` are the same type to inference).
pub(crate) fn lower_type_ref(value: &TypeRef, table: &mut InPlaceUnificationTable<TyVar>) -> Ty {
    match value {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Fn { params, ret } => {
            let params = params
                .iter()
                .map(|param_ty| lower_type_ref(param_ty, table))
                .collect();

            let ret = ret
                .as_ref()
                .map(|ret_ty| lower_type_ref(ret_ty, table))
                .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));

            Ty::fn_type(params, ret)
        }
        TypeRef::Ref(type_ref) => {
            // TODO: once we introduce references this can't discard them any longer
            lower_type_ref(&**type_ref, table)
        }
        TypeRef::Path(path) => builtin_type_by_name(path).unwrap_or(Ty::Error),
        TypeRef::Hole => Ty::Infer(table.new_key(TyVarValue::Unknown)),
        TypeRef::Error => Ty::Error,
    }
}

fn try_lower_fully_typed(value: &TypeRef) -> Option<Ty> {
    Some(match value {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Fn { params, ret } => {
            let params = params
                .iter()
                .map(|param_ty| try_lower_fully_typed(param_ty))
                .collect::<Option<Vec<_>>>()?;

            let ret = try_lower_fully_typed(ret.as_ref()?)?;

            Ty::fn_type(params, ret)
        }
        TypeRef::Ref(type_ref) => {
            // TODO: once we introduce references this can't discard them any longer
            return try_lower_fully_typed(&**type_ref);
        }
        TypeRef::Path(path) => builtin_type_by_name(path).unwrap_or(Ty::Error),
        TypeRef::Hole => return None,
        // Can happen on invalid syntax, e.g. `fn() -> {}()`
        TypeRef::Error => Ty::Error,
    })
}

/// The type other items see for `item`. An annotation is the whole answer
/// (a hard firewall edge: body edits never reach dependents). Without one,
/// the signature comes from the item's binding group — its own body,
/// inferred together with any unannotated items it's mutually recursive
/// with (see [`crate::groups`]); the firewall is then salsa early-cutoff:
/// dependents re-run only when the *inferred* signature value changes.
#[salsa::tracked]
pub fn signature<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Ty {
    let lowered_ty = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
        .and_then(try_lower_fully_typed);
    if let Some(lowered_ty) = lowered_ty {
        return lowered_ty;
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

/// Whether uses of `item` should say "add a type annotation": unannotated
/// AND inference couldn't determine its type from the definition (group
/// inference erases undetermined leftovers to `{error}`).
///
/// Language decision: exported symbols will *always* require a written
/// contract, even with interprocedural inference — once a visibility notion
/// exists, exported items go back to requiring annotations; today every
/// item counts as private.
pub fn signature_needs_annotation<'db>(db: &'db dyn Db, item: ItemId<'db>) -> bool {
    if crate::item_data(db, item)
        .as_ref()
        .is_none_or(|it| it.type_ref.is_some())
    {
        return false;
    }
    if crate::body::body(db, item).root.is_none() {
        // No value at all: the parse errors cover it.
        return false;
    }
    signature(db, item).contains_error()
}
