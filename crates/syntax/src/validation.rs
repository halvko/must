//! Checks for trees the grammar deliberately over-accepts.
//!
//! Parsing a superset keeps the user's intent in the tree (so inference,
//! hover and friends work on broken code) and gives errors found here
//! exactly the information a quick fix needs.

use crate::ast::{self, AstNode};
use crate::{BRACE_RULE, Fix, SyntaxError, SyntaxNode, SyntaxToken, TextEdit};
use text_size::TextRange;

pub(crate) fn validate(root: &SyntaxNode) -> Vec<SyntaxError> {
    let mut errors = Vec::new();
    for node in root.descendants() {
        if let Some(fn_literal) = ast::FnLiteral::cast(node.clone()) {
            // The parser reports "expected `{`" itself when the body is
            // absent entirely.
            if let Some(body) = fn_literal.body() {
                require_block(&body, BRACE_RULE, &mut errors);
            }
        } else if let Some(if_expr) = ast::IfExpr::cast(node.clone()) {
            if let Some(then) = if_expr.then_branch() {
                require_block(&then, "`if` branches are blocks", &mut errors);
            }
            // `else if` chains: the nested IfExpr validates itself.
            if let Some(els) = if_expr.else_branch()
                && !matches!(els, ast::Expr::IfExpr(_))
            {
                require_block(&els, "`else` branches are blocks", &mut errors);
            }
        } else if let Some(loop_expr) = ast::LoopExpr::cast(node.clone()) {
            // Same superset as `if` branches: any expression parses as the
            // body, only blocks are legal.
            if let Some(body) = loop_expr.body() {
                require_block(&body, "`loop` bodies are blocks", &mut errors);
            }
        } else if let Some(assign) = ast::AssignStmt::cast(node.clone()) {
            if let Some(lhs) = assign.lhs() {
                require_variable_target(&lhs, &mut errors);
            }
        } else if let Some(let_stmt) = ast::LetStmt::cast(node.clone()) {
            require_mut_names_a_binding(let_stmt.mut_token(), let_stmt.name(), &mut errors);
        } else if let Some(record_ty) = ast::RecordType::cast(node.clone()) {
            let names = record_ty
                .fields()
                .filter_map(|f| f.name())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicate_fields(names, &mut errors);
            reject_open_record(record_ty.dot3_token(), &mut errors);
        } else if let Some(record_expr) = ast::RecordExpr::cast(node.clone()) {
            let names = record_expr
                .fields()
                .filter_map(|f| f.name_ref())
                .map(|n| (n.text(), n.syntax().text_range()));
            report_duplicate_fields(names, &mut errors);
            reject_open_record(record_expr.dot3_token(), &mut errors);
        } else if let Some(type_item) = ast::TypeItem::cast(node.clone()) {
            reject_type_item_annotation(&type_item, &mut errors);
        } else if let Some(param) = ast::Param::cast(node) {
            require_mut_names_a_binding(param.mut_token(), param.name(), &mut errors);
        }
    }
    errors
}

/// Report every repeat of a field name after its first occurrence. Empty
/// names (missing in broken code) are ignored so they never collide.
fn report_duplicate_fields(
    fields: impl Iterator<Item = (String, TextRange)>,
    errors: &mut Vec<SyntaxError>,
) {
    let mut seen = std::collections::HashSet::new();
    for (name, range) in fields {
        if name.is_empty() {
            continue;
        }
        if !seen.insert(name.clone()) {
            errors.push(SyntaxError {
                message: format!("duplicate field `{name}`"),
                range,
                fix: None,
            });
        }
    }
}

/// `...` in a record type or literal parses (reserving the syntax) but is
/// always rejected: open records are not supported yet.
fn reject_open_record(dot3: Option<SyntaxToken>, errors: &mut Vec<SyntaxError>) {
    let Some(dot3) = dot3 else {
        return;
    };
    errors.push(SyntaxError {
        message: "open record types are not supported yet".to_owned(),
        range: dot3.text_range(),
        fix: None,
    });
}

/// The message for a non-variable assignment target. A `pub const` because
/// `mir` traps such targets with exactly the squiggle's text (the
/// single-render rule of `hir::diag`, except this message originates here
/// in validation rather than in a semantic analysis).
pub const CAN_ONLY_ASSIGN_TO_A_VARIABLE: &str = "can only assign to a variable";

/// The grammar superset-parses any expression as an assignment's LHS;
/// reject anything but a plain variable.
fn require_variable_target(expr: &ast::Expr, errors: &mut Vec<SyntaxError>) {
    if matches!(expr, ast::Expr::PathExpr(_)) {
        return;
    }
    errors.push(SyntaxError {
        message: CAN_ONLY_ASSIGN_TO_A_VARIABLE.to_owned(),
        range: expr.syntax().text_range(),
        fix: None,
    });
}

/// `mut` on a hole pattern (`let mut _ = ...` / `fn (mut _: T)`) has nothing
/// to act on: a hole binds no name, so it can never be the target of an
/// assignment. Offers a fix that drops the redundant `mut` (and the
/// whitespace between it and `_`) rather than making the user hand-edit.
fn require_mut_names_a_binding(
    mut_token: Option<SyntaxToken>,
    name: Option<ast::Name>,
    errors: &mut Vec<SyntaxError>,
) {
    let Some(mut_token) = mut_token else {
        return;
    };
    let Some(name) = name else {
        return;
    };
    if !name.is_hole() {
        return;
    }
    errors.push(SyntaxError {
        message: "`mut` has no effect on `_`: a hole can never be assigned".to_owned(),
        range: mut_token.text_range().cover(name.syntax().text_range()),
        fix: Some(Fix {
            label: "Remove `mut`".to_owned(),
            edits: vec![TextEdit {
                range: TextRange::new(
                    mut_token.text_range().start(),
                    name.syntax().text_range().start(),
                ),
                insert: String::new(),
            }],
        }),
    });
}

/// The grammar superset-parses a `: Type` annotation on a `type` item (the
/// item shape is shared with `static`/`const`); reject it — a `type`
/// declaration *is* the type, there is nothing to annotate — with a fix
/// that drops the annotation.
fn reject_type_item_annotation(type_item: &ast::TypeItem, errors: &mut Vec<SyntaxError>) {
    let Some(colon) = type_item.colon_token() else {
        return;
    };
    let end = type_item
        .ty()
        .map(|ty| ty.syntax().text_range().end())
        .unwrap_or_else(|| colon.text_range().end());
    let range = TextRange::new(colon.text_range().start(), end);
    errors.push(SyntaxError {
        message: "a `type` declaration takes no type annotation".to_owned(),
        range,
        fix: Some(Fix {
            label: "Remove the annotation".to_owned(),
            edits: vec![TextEdit {
                range,
                insert: String::new(),
            }],
        }),
    });
}

/// The grammar parses any expression where the language requires a block;
/// reject the superset with a wrap-in-braces fix.
fn require_block(expr: &ast::Expr, rule: &str, errors: &mut Vec<SyntaxError>) {
    if matches!(expr, ast::Expr::BlockExpr(_)) {
        return;
    }
    let range = expr.syntax().text_range();
    errors.push(SyntaxError {
        message: format!("{rule}; wrap this expression in `{{ }}`"),
        range,
        fix: Some(Fix {
            label: "Wrap in `{ }`".to_owned(),
            edits: vec![
                TextEdit {
                    range: TextRange::empty(range.start()),
                    insert: "{ ".to_owned(),
                },
                TextEdit {
                    range: TextRange::empty(range.end()),
                    insert: " }".to_owned(),
                },
            ],
        }),
    });
}
