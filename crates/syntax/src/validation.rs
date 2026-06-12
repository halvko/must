//! Checks for trees the grammar deliberately over-accepts.
//!
//! Parsing a superset keeps the user's intent in the tree (so inference,
//! hover and friends work on broken code) and gives errors found here
//! exactly the information a quick fix needs.

use crate::ast::{self, AstNode};
use crate::{Fix, SyntaxError, SyntaxNode, TextEdit};
use text_size::TextRange;

pub(crate) fn validate(root: &SyntaxNode) -> Vec<SyntaxError> {
    let mut errors = Vec::new();
    for node in root.descendants() {
        if let Some(fn_literal) = ast::FnLiteral::cast(node.clone()) {
            // The parser reports "expected `{`" itself when the body is
            // absent entirely.
            if let Some(body) = fn_literal.body() {
                require_block(&body, "function bodies", &mut errors);
            }
        } else if let Some(if_expr) = ast::IfExpr::cast(node) {
            if let Some(then) = if_expr.then_branch() {
                require_block(&then, "`if` branches", &mut errors);
            }
            // `else if` chains: the nested IfExpr validates itself.
            if let Some(els) = if_expr.else_branch() {
                if !matches!(els, ast::Expr::IfExpr(_)) {
                    require_block(&els, "`else` branches", &mut errors);
                }
            }
        }
    }
    errors
}

/// The grammar parses any expression where the language requires a block;
/// reject the superset with a wrap-in-braces fix.
fn require_block(expr: &ast::Expr, what: &str, errors: &mut Vec<SyntaxError>) {
    if matches!(expr, ast::Expr::BlockExpr(_)) {
        return;
    }
    let range = expr.syntax().text_range();
    errors.push(SyntaxError {
        message: format!("{what} are blocks; wrap this expression in `{{ }}`"),
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
