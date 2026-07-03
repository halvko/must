//! Semantic layer: item tree, body lowering, name resolution, type inference.
//!
//! Query layering (the incrementality firewall): everything range-carrying
//! is split from everything range-free, and items see each other only
//! through range-free, value-comparable data. An edit inside one body can
//! therefore only reach other items if a *value* on that path changes.

pub mod body;
pub mod const_check;
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

pub use body::{BindingId, Body, BodySourceMap, ExprId, PatId, body_with_source_map};
pub use const_check::ConstCheckDiagnostic;
pub use constraint::Cause;
pub use infer::{InferenceDiagnostic, InferenceResult};
pub use item_tree::{Constness, ItemKind, ItemTree, TypeDeclData, TypeRef, item_source, type_decl};
pub use scopes::{
    Builtin, Duplicate, ExprScopes, FileScope, Resolution, TypeScope, expr_scopes, file_scope,
    resolutions, type_scope,
};
pub use ty::{FnTy, Ty, VariantTy, enum_variants, signature, type_underlying, widens_to};

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
    pub severity: Severity,
    pub message: String,
    pub fix: Option<syntax::Fix>,
    /// Other locations that explain this diagnostic (e.g. "first defined
    /// here" on a duplicate definition). Each carries its own file, which is
    /// not necessarily the one diagnosed (see [`RelatedInfo::file`]).
    pub related: Vec<RelatedInfo>,
}

/// Severity of a [`Diagnostic`]. `ide` maps this onto its own richer
/// `Severity` (which additionally has `Info`, synthesized only at the ide
/// layer for related-location companions — hir never produces those).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
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
            severity: Severity::Error,
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
            severity: Severity::Error,
            message: diag::defined_multiple_times(&second.text()),
            fix: None,
            related,
        });
    }

    // Bad type names, in any annotation position: unknown, naming a value
    // item, or a `::` path that names no variant. Without this, a typo'd
    // type lowers to a silent `{error}`.
    for path_type in parse(db, file)
        .syntax_node()
        .descendants()
        .filter_map(ast::PathType::cast)
    {
        let Some(name_ref) = path_type.name_ref() else {
            continue;
        };
        let message = match path_type.variant_name_ref() {
            Some(variant) => variant_position_error(db, file, &name_ref.text(), &variant.text()),
            None => type_position_error(db, file, &name_ref.text()),
        };
        if let Some(message) = message {
            diagnostics.push(Diagnostic {
                range: path_type.syntax().text_range(),
                severity: Severity::Error,
                message,
                fix: None,
                related: Vec::new(),
            });
        }
    }

    // `type` declarations: the RHS must be a `struct` or `enum` literal
    // whose field values / variant payloads are types. `type_decl` reads
    // the same shape syntactically; every `TypeRef::Error` (and every
    // erased inference variable) it can produce has a diagnostic from here.
    for &item in file_item_ids(db, file) {
        let Some(ast::Item::TypeItem(decl)) = item_source(db, item) else {
            continue;
        };
        let Some(rhs) = decl.body() else {
            // No RHS at all: the parse errors cover it.
            continue;
        };
        match rhs {
            ast::Expr::RecordExpr(record) => {
                type_decl_field_diagnostics(db, file, &record, &mut diagnostics);
            }
            ast::Expr::EnumExpr(en) => {
                enum_decl_payload_diagnostics(&en, &mut diagnostics);
            }
            other => diagnostics.push(Diagnostic {
                range: other.syntax().text_range(),
                severity: Severity::Error,
                message: "only a `struct` or `enum` literal can declare a type".to_owned(),
                fix: None,
                related: Vec::new(),
            }),
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
                severity: Severity::Error,
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
            // Field diagnostics squiggle the field *name*, not the whole
            // expression (which may span a long receiver or initializer):
            // the name is the thing that's wrong.
            let range = match diag {
                InferenceDiagnostic::NoSuchField { .. } => {
                    ast::FieldExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.name_ref())
                        .map(|n| n.syntax().text_range())
                        .unwrap_or(range)
                }
                // Reported on the extra field's value expression; the name
                // sits on the enclosing record-field node. (A shorthand
                // field's value *is* its name, so this is a no-op there.)
                InferenceDiagnostic::RecordLitExtraField { .. } => ptr
                    .to_node(&syntax_root)
                    .ancestors()
                    .find_map(ast::RecordExprField::cast)
                    .and_then(|f| f.name_ref())
                    .map(|n| n.syntax().text_range())
                    .unwrap_or(range),
                // The variant name is the wrong part, not the (correct)
                // enum name in front of it.
                InferenceDiagnostic::NoSuchVariant { .. } => {
                    ast::PathExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.variant_name_ref())
                        .map(|n| n.syntax().text_range())
                        .unwrap_or(range)
                }
                // Reported on the `match` keyword: the construct as a whole
                // is what fails to cover — no single arm is the culprit.
                InferenceDiagnostic::NonExhaustiveMatch { .. }
                | InferenceDiagnostic::MatchWithoutCatchAll { .. } => {
                    ast::MatchExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.match_token())
                        .map(|t| t.text_range())
                        .unwrap_or(range)
                }
                _ => range,
            };
            // Pattern diagnostics squiggle the pattern; `diag.expr()` (the
            // enclosing match, where MIR traps) is only the fallback.
            let range = match diag.pat().and_then(|pat| source_map.node_for_pat(pat)) {
                Some(pat_ptr) => pat_ptr.text_range(),
                None => range,
            };
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
                                .or_else(|| {
                                    // No annotation to point at: the type
                                    // was inferred, so blame the binding's
                                    // name — that's where the decision
                                    // became attached.
                                    source_map
                                        .node_for_binding(*binding)
                                        .map(|ptr| RelatedInfo {
                                            file,
                                            range: ptr.text_range(),
                                            message: format!(
                                                "`{}` was inferred to have type `{}` \
                                             from its initializer",
                                                body.bindings[*binding].name,
                                                expected.display()
                                            ),
                                        })
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
                            Cause::Constructor(call) => {
                                // Blame the *declaration*: the mismatching
                                // field's declared type when the diagnosed
                                // expression sits in a record-literal field,
                                // the declaration's name otherwise.
                                let callee = match &body.exprs[*call] {
                                    body::ExprData::Call { callee, .. } => *callee,
                                    _ => *call,
                                };
                                let Some(Resolution::TypeItem(loc)) = resolutions.get(callee)
                                else {
                                    return Vec::new();
                                };
                                let Some(ast::Item::TypeItem(decl)) =
                                    item_source(db, loc.to_id(db))
                                else {
                                    return Vec::new();
                                };
                                let field_decl = ptr
                                    .to_node(&syntax_root)
                                    .ancestors()
                                    .find_map(ast::RecordExprField::cast)
                                    .and_then(|f| f.name_ref())
                                    .map(|n| n.text())
                                    .and_then(|name| match decl.body()? {
                                        ast::Expr::RecordExpr(record) => {
                                            record.fields().find(|f| {
                                                f.name_ref().is_some_and(|n| n.text() == name)
                                            })
                                        }
                                        _ => None,
                                    });
                                match field_decl {
                                    Some(field) => vec![RelatedInfo {
                                        file: loc.file,
                                        range: field.syntax().text_range(),
                                        message: format!(
                                            "expected `{}` because of this field declaration",
                                            expected.display()
                                        ),
                                    }],
                                    None => decl
                                        .name()
                                        .map(|n| RelatedInfo {
                                            file: loc.file,
                                            range: n.syntax().text_range(),
                                            message: format!(
                                                "expected `{}` because of `{}`'s declaration",
                                                expected.display(),
                                                loc.display_name()
                                            ),
                                        })
                                        .into_iter()
                                        .collect(),
                                }
                            }
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
                InferenceDiagnostic::AssignToImmutable { binding, name, .. } => {
                    // Where `mut` is missing — also the anchor for the
                    // upcoming insert-`mut` quick fix.
                    source_map
                        .node_for_binding(*binding)
                        .map(|ptr| {
                            vec![RelatedInfo {
                                file,
                                range: ptr.text_range(),
                                message: format!("`{name}` is declared without `mut` here"),
                            }]
                        })
                        .unwrap_or_default()
                }
                InferenceDiagnostic::AssignToItem { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // Same shape as `ArgCountMismatch`: the constructor's
                // declaration is one click away.
                InferenceDiagnostic::TypeCtorArgCount { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // The declaration explains what variants (or fields) do
                // exist — one click away.
                InferenceDiagnostic::NoSuchVariant { item: target, .. }
                | InferenceDiagnostic::NoVariantsOnStruct { item: target, .. }
                | InferenceDiagnostic::EnumCtorIsVariant { item: target, .. }
                | InferenceDiagnostic::PatNoSuchVariant { item: target, .. }
                | InferenceDiagnostic::PatWrongEnum { item: target, .. }
                | InferenceDiagnostic::BindShadowsVariant { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // The message names the nominal type only; where its shape
                // is declared is the hint (an enum has variants, not
                // fields — say so instead of promising fields).
                InferenceDiagnostic::NoSuchField {
                    receiver_ty: Ty::Named(loc),
                    ..
                } => item_source(db, loc.to_id(db))
                    .and_then(|it| it.body())
                    .map(|decl_body| {
                        let message = if ty::enum_variants(db, loc.to_id(db)).is_some() {
                            format!(
                                "`{}` is an `enum`, declared here — it has variants, not fields",
                                loc.display_name()
                            )
                        } else {
                            format!("the fields of `{}` are declared here", loc.display_name())
                        };
                        vec![RelatedInfo {
                            file: loc.file,
                            range: decl_body.syntax().text_range(),
                            message,
                        }]
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            // A hint enclosing the squiggle adds nothing — the user is
            // already looking at it (e.g. "this call requires `str`" on the
            // very call whose argument carries the mismatch).
            related.retain(|r| !(r.file == file && r.range.contains_range(range)));
            let fix = match diag {
                // Insert `mut ` right before the binding's name, whether it
                // came from a `let` or a parameter — both render the fixed
                // source as `let mut x = …` / `fn (mut n: usize)`. A hole
                // (`_`) never resolves as an assignment target, so this
                // shouldn't fire for one; skip defensively rather than offer
                // a nonsensical `mut _`.
                InferenceDiagnostic::AssignToImmutable { binding, name, .. } if name != "_" => {
                    source_map
                        .node_for_binding(*binding)
                        .map(|ptr| syntax::Fix {
                            label: format!("Make `{name}` mutable"),
                            edits: vec![syntax::TextEdit {
                                range: TextRange::empty(ptr.text_range().start()),
                                insert: "mut ".to_owned(),
                            }],
                        })
                }
                _ => None,
            };
            diagnostics.push(Diagnostic {
                range,
                severity: diag.severity(),
                message: diag.message(),
                fix,
                related,
            });
        }

        for diag in const_check::const_check(db, item) {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            // Messages render in `ConstCheckDiagnostic::message` (via
            // `diag`, shared with MIR's traps); only ranges and related
            // locations attach here.
            let mut related = match diag {
                ConstCheckDiagnostic::NonConstFnCall { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            // Every const-check finding fires *at* a call inside a const
            // context; a second hint explains *why* that location is one —
            // the enclosing `const { ... }` block's keyword, the enclosing
            // `const fn`'s marker, or (neither found climbing from the
            // callee) the item's own initializer.
            related.extend(const_context_reason(
                db,
                item,
                file,
                ptr.to_node(&syntax_root),
            ));
            // `NonConstFnCall` names exactly the fix: the callee item's
            // initializer is (by construction of the diagnostic — see
            // `root_fn_is_const`) a plain `fn` literal; inserting `const `
            // right before it is always well-typed. Offered only when the
            // declaration is edited in the same file as the fix's range
            // (`Fix`/`TextEdit` carry no file of their own).
            let fix = match diag {
                ConstCheckDiagnostic::NonConstFnCall { item: target, .. }
                    if target.file == file =>
                {
                    item_source(db, target.to_id(db))
                        .and_then(|it| it.body())
                        .and_then(|body| match body {
                            ast::Expr::FnLiteral(fn_lit) if !fn_lit.is_const() => Some(fn_lit),
                            _ => None,
                        })
                        .map(|fn_lit| syntax::Fix {
                            label: format!("Mark `{}` as `const fn`", target.display_name()),
                            edits: vec![syntax::TextEdit {
                                range: TextRange::empty(fn_lit.syntax().text_range().start()),
                                insert: "const ".to_owned(),
                            }],
                        })
                }
                _ => None,
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                severity: Severity::Error,
                message: diag.message(),
                fix,
                related,
            });
        }
    }

    // Tripwire (rustc's "delayed bug" pattern): the invariant is that every
    // `{error}` in the file is downstream of at least one diagnostic above —
    // that's what makes "no diagnostics" mean "lowerable". If types are
    // broken but the file looks clean, a diagnostic is missing somewhere;
    // say so loudly instead of leaving hover-only weirdness. Warnings
    // (unreachable arms) don't justify an `{error}`, so they don't disarm it.
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
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
                    severity: Severity::Error,
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

    // Hole-named items (`static _ = ...` / `const _ = ...`) bind nothing, so
    // their value can never be referenced — but they are still evaluated at
    // check time (statics and consts evaluate eagerly; see
    // `eval::const_value`/`const_block_values`, driven from `ide`), so a
    // panicking initializer still reports its own error alongside this
    // warning. A broken item with no name at all (a parse error, already
    // reported above) gets nothing: `_` is a real, distinct token from an
    // absent name (see `ast::Name::is_hole`), so this never fires for it.
    // Runs after the tripwire above so a hole item alone never masks it.
    for &item in file_item_ids(db, file) {
        let Some(name) = item_source(db, item).and_then(|it| it.name()) else {
            continue;
        };
        if !name.is_hole() {
            continue;
        }
        diagnostics.push(Diagnostic {
            range: name.syntax().text_range(),
            severity: Severity::Warning,
            message: "this item binds nothing and its value cannot be used".to_owned(),
            fix: None,
            related: Vec::new(),
        });
    }

    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    diagnostics
}

/// The error for `name` in *type* position, if any. Mirrors the resolution
/// order of `ty::lower_type_path` exactly — a `type` item wins, then the
/// builtin types, then a value item is "not a type" — so every silent
/// `Ty::Error` the lowering produces has a diagnostic from here. (Only the
/// *message choice* consults the full [`file_scope`]; this function runs in
/// the diagnostics aggregator, outside the inference firewall.)
/// Why `callee_node` sits in a const context, for a const-check finding's
/// second [`RelatedInfo`]: climbing from the callee, the nearest enclosing
/// `const { ... }` block or `const fn` literal, or — climbing all the way
/// out without finding either — the item's own initializer (every
/// `static`/`const` item's value is a const context to begin with). A
/// plain (non-`const`) `fn` literal exits the const context, so climbing
/// stops there without a match (const-checking itself never marks
/// anything inside one `in_const`, so a finding can't be *inside* a plain
/// fn's body without also being inside one of the two reasons above,
/// nested within it).
fn const_context_reason(
    db: &dyn Db,
    item: ItemId<'_>,
    file: SourceFile,
    callee_node: syntax::SyntaxNode,
) -> Option<RelatedInfo> {
    // `ancestors()` yields the node itself first; skip it — a directly
    // *called* plain `fn` literal is itself the finding (never the const
    // context's boundary: that reasoning only applies to an *enclosing*
    // literal the callee sits inside of).
    for ancestor in callee_node.ancestors().skip(1) {
        if let Some(block) = ast::ConstBlockExpr::cast(ancestor.clone()) {
            let token = block.const_token()?;
            return Some(RelatedInfo {
                file,
                range: token.text_range(),
                message: "this `const` block is a const context".to_owned(),
            });
        }
        if let Some(fn_lit) = ast::FnLiteral::cast(ancestor.clone()) {
            if fn_lit.is_const() {
                let token = fn_lit.const_token()?;
                return Some(RelatedInfo {
                    file,
                    range: token.text_range(),
                    message: "this `const fn` is always a const context".to_owned(),
                });
            }
            // A plain `fn` literal boundary: nothing further out is the
            // reason for a callee actually inside it.
            break;
        }
    }
    let token = item_source(db, item)?.syntax().first_token()?;
    Some(RelatedInfo {
        file,
        range: token.text_range(),
        message: "this item's initializer is a const context".to_owned(),
    })
}

fn type_position_error(db: &dyn Db, file: SourceFile, name: &str) -> Option<String> {
    match type_scope(db, file).resolve(name) {
        // A type item; or a duplicate name, whose definitions already
        // carry the diagnostics.
        Some(_) => None,
        None => {
            if ty::builtin_type_by_name(name).is_some() {
                return None;
            }
            match file_scope(db, file).resolve(name) {
                // Also covers value-item duplicates: whichever way the
                // ambiguity resolves, it is not a type.
                Some(_) => Some(format!("`{name}` is not a type")),
                None => Some(format!("unknown type `{name}`")),
            }
        }
    }
}

/// The error for `Enum::Variant` in *type* position, if any. Mirrors
/// [`ty::lower_variant_type_path`] exactly (the same silent-`Ty::Error`
/// contract as [`type_position_error`]): the base must name an enum `type`
/// item and the variant must exist in it. An unknown or ambiguous base is
/// covered by [`type_position_error`]-style reporting here too, so a
/// two-segment path never needs a second pass over its first segment.
fn variant_position_error(
    db: &dyn Db,
    file: SourceFile,
    base: &str,
    variant: &str,
) -> Option<String> {
    match type_scope(db, file).resolve(base) {
        Some(Resolution::TypeItem(loc)) => {
            let item = loc.to_id(db);
            match ty::enum_variants(db, item) {
                Some(variants) => {
                    if variant.is_empty() {
                        // `Shape::` — the parse error covers it.
                        return None;
                    }
                    if variants.iter().any(|(name, _)| name == variant) {
                        None
                    } else {
                        Some(format!(
                            "`{}` has no variant `{variant}`",
                            loc.display_name()
                        ))
                    }
                }
                None => {
                    if matches!(type_decl(db, item), Some(TypeDeclData::Struct { .. })) {
                        Some(format!(
                            "`{}` has no variants (it is a `struct` type)",
                            loc.display_name()
                        ))
                    } else {
                        // A broken declaration carries its own diagnostics.
                        None
                    }
                }
            }
        }
        // The duplicate definitions carry the diagnostics.
        Some(_) => None,
        None => {
            if ty::builtin_type_by_name(base).is_some() {
                return Some(format!("`{base}` has no variants (it is a builtin type)"));
            }
            // Reuse the single-segment wording for a base that is no type
            // at all ("unknown type" / "is not a type").
            type_position_error(db, file, base)
        }
    }
}

/// Check the variant payloads of an `enum` literal used as a type
/// declaration: payload positions are real type syntax, so unknown names
/// are covered by the file-wide `PathType` pass — but a payload written
/// with a hole (`_`, or an `fn` type without a return) would silently
/// lower to an erased inference variable ([`ty::enum_variants`] erases it
/// to `Ty::Error`); this is that error's diagnostic.
fn enum_decl_payload_diagnostics(en: &ast::EnumExpr, diagnostics: &mut Vec<Diagnostic>) {
    for variant in en.variants() {
        for payload in variant.payload_types() {
            if TypeRef::from_ast(payload.clone()).is_fully_typed() {
                continue;
            }
            diagnostics.push(Diagnostic {
                range: payload.syntax().text_range(),
                severity: Severity::Error,
                message: "a variant payload must be a fully written type; \
                          a declaration has nothing to infer `_` from"
                    .to_owned(),
                fix: None,
                related: Vec::new(),
            });
        }
    }
}

/// Check the fields of a `struct` literal used as a type declaration: each
/// field value must itself denote a type — a type name or a nested `struct`
/// literal (mirroring `item_tree::expr_as_type_ref`).
fn type_decl_field_diagnostics(
    db: &dyn Db,
    file: SourceFile,
    record: &ast::RecordExpr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    fn simple_error(range: TextRange, message: String) -> Diagnostic {
        Diagnostic {
            range,
            severity: Severity::Error,
            message,
            fix: None,
            related: Vec::new(),
        }
    }
    for field in record.fields() {
        let Some(name_ref) = field.name_ref() else {
            // No field name: broken source, the parse error covers it.
            continue;
        };
        match field.expr() {
            // Shorthand (`x` without `: type`): there is no type to read.
            None => diagnostics.push(simple_error(
                field.syntax().text_range(),
                format!("expected a type for field `{}`", name_ref.text()),
            )),
            Some(ast::Expr::PathExpr(path)) => {
                let Some(type_name) = path.name_ref() else {
                    continue;
                };
                let message = match path.variant_name_ref() {
                    // `x: Shape::Circle` as a field's type: same checks as
                    // annotation position.
                    Some(variant) => {
                        variant_position_error(db, file, &type_name.text(), &variant.text())
                    }
                    None => type_position_error(db, file, &type_name.text()),
                };
                if let Some(message) = message {
                    diagnostics.push(simple_error(path.syntax().text_range(), message));
                }
            }
            Some(ast::Expr::RecordExpr(nested)) => {
                type_decl_field_diagnostics(db, file, &nested, diagnostics);
            }
            Some(other) => diagnostics.push(simple_error(
                other.syntax().text_range(),
                format!("expected a type for field `{}`", name_ref.text()),
            )),
        }
    }
}
