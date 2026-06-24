//! Semantic layer: item tree, body lowering, name resolution, type inference.
//!
//! Query layering (the incrementality firewall): everything range-carrying
//! is split from everything range-free, and items see each other only
//! through range-free, value-comparable data. An edit inside one body can
//! therefore only reach other items if a *value* on that path changes.

pub mod body;
pub mod constraint;
pub mod diag;
pub mod groups;
pub mod infer;
pub mod item_tree;
pub mod scopes;
pub mod ty;

#[cfg(test)]
mod tests;

use base_db::{Db, SourceFile, parse};
use syntax::TextRange;
use syntax::ast::{self, AstNode as _};

pub use body::{BindingId, Body, BodySourceMap, ExprId, body_with_source_map};
pub use constraint::Cause;
pub use infer::{InferenceDiagnostic, InferenceResult};
pub use item_tree::{ItemTree, TypeRef, item_source};
pub use scopes::{
    Builtin, Duplicate, ExprScopes, FileScope, Resolution, expr_scopes, file_scope, resolutions,
};
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

/// Lifetime-free reference to an item, carrying the same identity as
/// [`ItemId`] (name + disambiguator, *not* a positional index): inserting an
/// unrelated item above doesn't change any `ItemLoc`, so query values that
/// embed one — resolutions, MIR constants, eval origins — backdate across
/// item reordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemLoc {
    pub file: SourceFile,
    pub name: std::sync::Arc<str>,
    pub disambiguator: u32,
}

impl ItemLoc {
    /// Always succeeds (interning): an `ItemLoc` held across an edit that
    /// deleted the item yields an id whose queries all answer the empty/
    /// error case — total, never a panic.
    pub fn to_id<'db>(&self, db: &'db dyn Db) -> ItemId<'db> {
        ItemId::new(db, self.file, self.name.to_string(), self.disambiguator)
    }

    /// The name for messages; unnamed (broken) items render as `?`.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "?"
        } else {
            &self.name
        }
    }
}

pub fn item_loc(db: &dyn Db, item: ItemId<'_>) -> ItemLoc {
    ItemLoc {
        file: item.file(db),
        name: std::sync::Arc::from(item.name(db).as_str()),
        disambiguator: item.disambiguator(db),
    }
}

/// The item's position in its file (= its index in [`item_tree`]). `None`
/// for a stale `ItemId` held across an edit that removed the item.
pub fn item_index(db: &dyn Db, item: ItemId<'_>) -> Option<usize> {
    file_item_ids(db, item.file(db))
        .iter()
        .position(|&it| it == item)
}

/// The item-tree entry for `item` (its contract, constness, name).
/// Tracked so that consumers (`signature`, `infer`) depend on this item's
/// *entry* rather than on the whole positional item list — inserting an
/// unrelated item above re-executes only this cheap lookup, and its
/// unchanged value backdates everything downstream.
#[salsa::tracked(returns(ref))]
pub fn item_data<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<item_tree::ItemData> {
    item_tree::item_tree(db, item.file(db))
        .items
        .get(item_index(db, item)?)
        .cloned()
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
    /// Other locations that explain this diagnostic (e.g. "first defined
    /// here" on a duplicate definition). Each carries its own file, which is
    /// not necessarily the one diagnosed (see [`RelatedInfo::file`]).
    pub related: Vec<RelatedInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedInfo {
    /// The file `range` lives in — *not* necessarily the diagnosed file
    /// (e.g. "first defined here" will cross files once imports exist).
    /// Consumers must resolve positions through this file's own line index.
    pub file: SourceFile,
    pub range: TextRange,
    pub message: String,
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
            related: Vec::new(),
        })
        .collect();

    let item_name = |loc: &ItemLoc| item_source(db, loc.to_id(db)).and_then(|it| it.name());

    // Duplicate definitions, discovered by `file_scope` (the analysis that
    // decides first-wins also knows about the losers); only the range
    // attachment happens here.
    for dup in &file_scope(db, file).duplicates {
        let Some(second) = item_name(&dup.second) else {
            continue;
        };
        let related = item_name(&dup.first)
            .map(|first| {
                vec![RelatedInfo {
                    file: dup.first.file,
                    range: first.syntax().text_range(),
                    message: "first defined here".to_owned(),
                }]
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: second.syntax().text_range(),
            message: diag::defined_multiple_times(&second.text()),
            fix: None,
            related,
        });
    }

    // Unknown type names, in any annotation position. Without this, a
    // typo'd type lowers to a silent `{error}`.
    for path_type in parse(db, file)
        .syntax_node()
        .descendants()
        .filter_map(ast::PathType::cast)
    {
        let Some(name_ref) = path_type.name_ref() else {
            continue;
        };
        if ty::builtin_type_by_name(&name_ref.text()).is_none() {
            diagnostics.push(Diagnostic {
                range: path_type.syntax().text_range(),
                message: format!("unknown type `{}`", name_ref.text()),
                fix: None,
                related: Vec::new(),
            });
        }
    }

    let syntax_root = parse(db, file).syntax_node();
    for &item in file_item_ids(db, file) {
        let (body, source_map) = body_with_source_map(db, item);
        let resolutions = resolutions(db, item);
        for (expr, data) in body.exprs.iter() {
            let message = match data {
                body::ExprData::NameRef(name) if resolutions.get(expr).is_none() => {
                    diag::unresolved_name(name)
                }
                // Without this, an overflowing literal would be a value MIR
                // can only trap on with no diagnostic to borrow.
                body::ExprData::Literal(body::LiteralData::Int(None)) => {
                    diag::INT_LITERAL_TOO_LARGE.to_owned()
                }
                _ => continue,
            };
            let Some(ptr) = source_map.node_for_expr(expr) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                message,
                fix: None,
                related: Vec::new(),
            });
        }

        for diag in &infer::infer(db, item).diagnostics {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            let range = ptr.text_range();
            // Messages render in `InferenceDiagnostic::message` (shared with
            // MIR's traps); only ranges and related locations attach here.
            let mut related = match diag {
                InferenceDiagnostic::NeedsAnnotation { item, .. } => item_name(item)
                    .map(|n| {
                        vec![RelatedInfo {
                            file: item.file,
                            range: n.syntax().text_range(),
                            message: "defined here".to_owned(),
                        }]
                    })
                    .unwrap_or_default(),
                InferenceDiagnostic::TypeMismatch {
                    reasons, expected, ..
                }
                | InferenceDiagnostic::AllBranchesMismatch {
                    reasons, expected, ..
                } => {
                    // Resolve an expression to its AST node for hints that
                    // point at a sub-element (a return type, an operator
                    // token) rather than the whole expression.
                    let ast_for_expr = |expr: body::ExprId| {
                        Some(source_map.node_for_expr(expr)?.to_node(&syntax_root))
                    };
                    let render_reason = |r: &Cause| -> Vec<RelatedInfo> {
                        match r {
                            Cause::Binding(binding) => source_map
                                .annotation_for_binding(*binding)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "expected `{}` because of this annotation",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::ItemAnnotation => item_source(db, item)
                                .and_then(|it| it.ty())
                                .map(|ty| RelatedInfo {
                                    file,
                                    range: ty.syntax().text_range(),
                                    message: format!(
                                        "expected `{}` because of this annotation",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::ReturnAnnotation(fn_expr) => ast_for_expr(*fn_expr)
                                .and_then(ast::FnLiteral::cast)
                                .and_then(|f| f.ret_type())
                                .map(|ret| RelatedInfo {
                                    file,
                                    range: ret.syntax().text_range(),
                                    message: format!(
                                        "expected `{}` because of this return type",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Operator(bin_expr) => ast_for_expr(*bin_expr)
                                .and_then(ast::BinExpr::cast)
                                .and_then(|bin| bin.op_token())
                                .map(|op| RelatedInfo {
                                    file,
                                    range: op.text_range(),
                                    message: format!(
                                        "`{}` requires `{}` operands",
                                        op.text(),
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Condition(if_expr) => ast_for_expr(*if_expr)
                                .and_then(ast::IfExpr::cast)
                                .and_then(|it| it.if_token())
                                .map(|token| RelatedInfo {
                                    file,
                                    range: token.text_range(),
                                    message: "this `if` requires a `bool` condition".to_owned(),
                                })
                                .into_iter()
                                .collect(),
                            Cause::MissingElse(if_expr) => ast_for_expr(*if_expr)
                                .and_then(ast::IfExpr::cast)
                                .and_then(|it| it.if_token())
                                .map(|token| RelatedInfo {
                                    file,
                                    range: token.text_range(),
                                    message: "this `if` has no `else`, so its value is `()`"
                                        .to_owned(),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Operand(operand_expr) => source_map
                                .node_for_expr(*operand_expr)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "this operand has type `{}`",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Branch(branch_expr) => source_map
                                .node_for_expr(*branch_expr)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "this branch has type `{}`",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::CallSite { call, arg } => {
                                // Self-evident when the mismatch sits inside
                                // the call itself; skip the hints then. (The
                                // generic containment filter below no longer
                                // catches this once the hints point at the
                                // callee and argument instead of the whole
                                // call.)
                                let Some(call_ptr) = source_map.node_for_expr(*call) else {
                                    return Vec::new();
                                };
                                if call_ptr.text_range().contains_range(range) {
                                    return Vec::new();
                                }
                                // Point at the function name and the argument
                                // the requirement travels through, not the
                                // whole call expression.
                                let callee = match &body.exprs[*call] {
                                    body::ExprData::Call { callee, .. } => *callee,
                                    _ => *call,
                                };
                                let callee_hint =
                                    source_map.node_for_expr(callee).map(|ptr| RelatedInfo {
                                        file,
                                        range: ptr.text_range(),
                                        message: format!(
                                            "this call requires `{}`",
                                            expected.display()
                                        ),
                                    });
                                let arg_hint =
                                    source_map.node_for_expr(*arg).map(|ptr| RelatedInfo {
                                        file,
                                        range: ptr.text_range(),
                                        message: format!(
                                            "this argument needs to be `{}`",
                                            expected.display()
                                        ),
                                    });
                                callee_hint.into_iter().chain(arg_hint).collect()
                            }
                        }
                    };
                    reasons.iter().flat_map(render_reason).collect()
                }
                InferenceDiagnostic::IfBranchMismatch {
                    then_expr, then_ty, ..
                } => source_map
                    .node_for_expr(*then_expr)
                    .map(|ptr| {
                        vec![RelatedInfo {
                            file,
                            range: ptr.text_range(),
                            message: format!("this branch has type `{}`", then_ty.display()),
                        }]
                    })
                    .unwrap_or_default(),
                InferenceDiagnostic::ArgCountMismatch { expr, .. } => {
                    // Show where the function is defined, so its parameter
                    // list is one click away.
                    let callee_loc = match &body.exprs[*expr] {
                        body::ExprData::Call { callee, .. } => match resolutions.get(*callee) {
                            Some(Resolution::Item(loc)) => Some(loc),
                            _ => None,
                        },
                        _ => None,
                    };
                    callee_loc
                        .and_then(|loc| {
                            let name = item_name(loc)?;
                            Some(vec![RelatedInfo {
                                file: loc.file,
                                range: name.syntax().text_range(),
                                message: format!("`{}` is defined here", loc.display_name()),
                            }])
                        })
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            };
            // A hint enclosing the squiggle adds nothing — the user is
            // already looking at it (e.g. "this call requires `str`" on the
            // very call whose argument carries the mismatch).
            related.retain(|r| !(r.file == file && r.range.contains_range(range)));
            diagnostics.push(Diagnostic {
                range,
                message: diag.message(),
                fix: None,
                related,
            });
        }
    }

    // Tripwire (rustc's "delayed bug" pattern): the invariant is that every
    // `{error}` in the file is downstream of at least one diagnostic above —
    // that's what makes "no diagnostics" mean "lowerable". If types are
    // broken but the file looks clean, a diagnostic is missing somewhere;
    // say so loudly instead of leaving hover-only weirdness.
    if diagnostics.is_empty() {
        'items: for &item in file_item_ids(db, file) {
            let (_, source_map) = body_with_source_map(db, item);
            for (expr, ty) in infer::infer(db, item).type_of_expr.iter() {
                if !ty.contains_error() {
                    continue;
                }
                let Some(ptr) = source_map.node_for_expr(expr) else {
                    continue;
                };
                diagnostics.push(Diagnostic {
                    range: ptr.text_range(),
                    message: "internal error: this expression has type `{error}` but no \
                              error was reported — this is a bug in the Must language server"
                        .to_owned(),
                    fix: None,
                    related: Vec::new(),
                });
                break 'items;
            }
        }
    }

    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    diagnostics
}
