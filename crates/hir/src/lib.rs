//! Semantic layer: item tree, body lowering, name resolution, type inference.
//!
//! Query layering (the incrementality firewall): everything range-carrying
//! is split from everything range-free, and items see each other only
//! through range-free, value-comparable data. An edit inside one body can
//! therefore only reach other items if a *value* on that path changes.

pub mod body;
pub mod infer;
pub mod item_tree;
pub mod scopes;
pub mod ty;

#[cfg(test)]
mod tests;

use base_db::{Db, SourceFile, parse};
use syntax::TextRange;

pub use body::{Body, BodySourceMap, ExprId, BindingId, body_with_source_map};
pub use infer::{InferenceDiagnostic, InferenceResult};
pub use item_tree::{ItemTree, TypeRef, item_source};
pub use scopes::{Builtin, ExprScopes, Resolution, expr_scopes, file_scope, resolutions};
pub use ty::{FnTy, Ty, signature};

/// Stable identity of a top-level item: survives edits to other items,
/// reordering of unrelated code, and any edit inside its own body.
///
/// Used as a *query key*. Inside query *values* use [`ItemLoc`], which is
/// lifetime-free (salsa values holding `'db` ids would need `salsa::Update`).
#[salsa::interned(debug)]
pub struct ItemId<'db> {
    pub file: SourceFile,
    #[returns(ref)]
    pub name: String,
    /// Which occurrence of `name` in the file (0-based); keeps duplicates
    /// and unnamed (broken) items distinct.
    pub disambiguator: u32,
}

/// Lifetime-free reference to an item: its position in [`file_item_ids`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemLoc {
    pub file: SourceFile,
    pub index: u32,
}

impl ItemLoc {
    pub fn to_id<'db>(self, db: &'db dyn Db) -> Option<ItemId<'db>> {
        file_item_ids(db, self.file)
            .get(self.index as usize)
            .copied()
    }
}

pub fn item_loc(db: &dyn Db, item: ItemId<'_>) -> ItemLoc {
    let file = item.file(db);
    let index = file_item_ids(db, file)
        .iter()
        .position(|&it| it == item)
        .expect("ItemId not produced by file_item_ids") as u32;
    ItemLoc { file, index }
}

#[salsa::tracked(returns(ref))]
pub fn file_item_ids<'db>(db: &'db dyn Db, file: SourceFile) -> Vec<ItemId<'db>> {
    let tree = item_tree::item_tree(db, file);
    let mut seen: rustc_hash::FxHashMap<&str, u32> = rustc_hash::FxHashMap::default();
    tree.items
        .iter()
        .map(|item| {
            let disambiguator = seen.entry(item.name.as_str()).or_insert(0);
            let id = ItemId::new(db, file, item.name.clone(), *disambiguator);
            *disambiguator += 1;
            id
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub message: String,
    pub fix: Option<syntax::Fix>,
}

/// All semantic diagnostics for a file. This is the one place that converts
/// range-free facts back into text ranges (via the source maps).
pub fn file_diagnostics(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = parse(db, file)
        .errors()
        .iter()
        .map(|err| Diagnostic {
            range: err.range,
            message: err.message.clone(),
            fix: err.fix.clone(),
        })
        .collect();

    for &item in file_item_ids(db, file) {
        let (body, source_map) = body_with_source_map(db, item);
        let resolutions = resolutions(db, item);
        for (expr, data) in body.exprs.iter() {
            let body::ExprData::NameRef(name) = data else {
                continue;
            };
            if resolutions.get(expr).is_none() {
                let Some(ptr) = source_map.node_for_expr(expr) else {
                    continue;
                };
                diagnostics.push(Diagnostic {
                    range: ptr.text_range(),
                    message: format!("unresolved name `{name}`"),
                    fix: None,
                });
            }
        }

        for diag in &infer::infer(db, item).diagnostics {
            let (expr, message) = match diag {
                InferenceDiagnostic::TypeMismatch {
                    expr,
                    expected,
                    actual,
                } => (
                    *expr,
                    format!(
                        "type mismatch: expected `{}`, found `{}`",
                        expected.display(),
                        actual.display()
                    ),
                ),
                InferenceDiagnostic::NotCallable { expr, ty } => (
                    *expr,
                    format!("expression of type `{}` is not callable", ty.display()),
                ),
                InferenceDiagnostic::ArgCountMismatch {
                    expr,
                    expected,
                    found,
                } => (
                    *expr,
                    format!("expected {expected} argument(s), found {found}"),
                ),
            };
            let Some(ptr) = source_map.node_for_expr(expr) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                message,
                fix: None,
            });
        }
    }

    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    diagnostics
}
