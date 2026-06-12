//! The type representation and the per-item signature query.

use std::sync::Arc;

use base_db::Db;
use ena::unify::{NoError, UnifyKey, UnifyValue};

use crate::body::{ExprData, LiteralData};
use crate::item_tree::{TypeRef, item_tree};
use crate::{ItemId, item_loc};

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
        _ => None,
    }
}

/// Lower a syntactic type annotation. References are transparent for now
/// (`&'static str` and `str` are the same type to inference).
pub fn lower_type_ref(type_ref: &TypeRef) -> Ty {
    match type_ref {
        TypeRef::Unit => Ty::Unit,
        TypeRef::Never => Ty::Never,
        TypeRef::Ref(inner) => lower_type_ref(inner),
        TypeRef::Fn { params, ret } => Ty::fn_type(
            params.iter().map(lower_type_ref).collect(),
            ret.as_deref().map(lower_type_ref).unwrap_or(Ty::Unit),
        ),
        TypeRef::Path(name) => builtin_type_by_name(name).unwrap_or(Ty::Error),
        TypeRef::Error => Ty::Error,
    }
}

/// The type other items see for `item`. Reads only annotations (item tree)
/// plus a shallow, annotation-only peek at the body's root — it NEVER runs
/// inference of another body, so there are no query cycles, and a body edit
/// only propagates past this point if the signature value actually changes.
#[salsa::tracked]
pub fn signature<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Ty {
    let loc = item_loc(db, item);
    let tree = item_tree(db, item.file(db));
    if let Some(type_ref) = tree
        .items
        .get(loc.index as usize)
        .and_then(|it| it.type_ref.as_ref())
    {
        return lower_type_ref(type_ref);
    }

    // No annotation: peek at the body's root without inferring.
    let body = crate::body::body(db, item);
    let Some(root) = body.root else {
        return Ty::Error;
    };
    match &body.exprs[root] {
        ExprData::Literal(LiteralData::Int(_)) => Ty::Int,
        ExprData::Literal(LiteralData::Str(_)) => Ty::Str,
        ExprData::FnLiteral {
            params,
            ret_type,
            body: fn_body,
        } => {
            let ret = match ret_type {
                Some(type_ref) => lower_type_ref(type_ref),
                // A block without a tail expression is `()` without needing
                // inference. Anything else would need this item's own
                // inference — that's the future interprocedural step
                // (SCC groups/fixpoint).
                None => match &body.exprs[*fn_body] {
                    ExprData::Block { tail: None, .. } => Ty::Unit,
                    _ => Ty::Error,
                },
            };
            Ty::fn_type(
                params
                    .iter()
                    .map(|&p| {
                        body.bindings[p]
                            .type_ref
                            .as_ref()
                            .map(lower_type_ref)
                            .unwrap_or(Ty::Error)
                    })
                    .collect(),
                ret,
            )
        }
        _ => Ty::Error,
    }
}

/// Whether `{error}` parts of [`signature`] stem from *absent* annotations —
/// as opposed to written-but-broken types, which already carry their own
/// diagnostics at the definition. Mirrors the peek above case by case; uses
/// of such an item get a "add a type annotation" diagnostic.
///
/// Language decision: exported symbols will *always* require a written
/// contract, even once interprocedural inference lands — inference then
/// only relaxes this for non-exported items (there is no visibility notion
/// yet, so today it applies to everything).
pub fn signature_needs_annotation<'db>(db: &'db dyn Db, item: ItemId<'db>) -> bool {
    let loc = item_loc(db, item);
    let tree = item_tree(db, item.file(db));
    if tree
        .items
        .get(loc.index as usize)
        .is_none_or(|it| it.type_ref.is_some())
    {
        return false;
    }
    let body = crate::body::body(db, item);
    let Some(root) = body.root else {
        // No value at all: the parse errors cover it.
        return false;
    };
    match &body.exprs[root] {
        ExprData::Literal(_) => false,
        ExprData::FnLiteral {
            params,
            ret_type,
            body: fn_body,
        } => {
            params
                .iter()
                .any(|&p| body.bindings[p].type_ref.is_none())
                || (ret_type.is_none()
                    && !matches!(body.exprs[*fn_body], ExprData::Block { tail: None, .. }))
        }
        _ => true,
    }
}
