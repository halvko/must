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
        if let Some(fn_literal) = ast::FnLiteral::cast(node) {
            validate_fn_body(&fn_literal, &mut errors);
        }
    }
    errors
}

/// The grammar parses any expression as a `fn` body; the language requires
/// a block.
fn validate_fn_body(fn_literal: &ast::FnLiteral, errors: &mut Vec<SyntaxError>) {
    let Some(body) = fn_literal.body() else {
        return; // the parser already reported "expected `{`"
    };
    if matches!(body, ast::Expr::BlockExpr(_)) {
        return;
    }
    let range = body.syntax().text_range();
    errors.push(SyntaxError {
        message: "function bodies are blocks; wrap this expression in `{ }`".to_owned(),
        range,
        fix: Some(Fix {
            label: "Wrap body in `{ }`".to_owned(),
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
