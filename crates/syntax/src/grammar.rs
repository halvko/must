//! The Must grammar, written against `Parser`. Resilience rules:
//! the parser never bails; unexpected tokens end up wrapped in `ERROR` nodes;
//! sub-parsers refuse to consume tokens their caller can recover on.

use crate::SyntaxKind::*;
use crate::parser::{CompletedMarker, Parser};

/// Tokens an expression parser must not consume on error: the enclosing
/// construct knows what to do with them.
fn at_expr_recovery(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        EOF | R_BRACE | R_PAREN | SEMICOLON | COMMA | STATIC_KW | CONST_KW | LET_KW
    )
}

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at(EOF) {
        match p.current() {
            STATIC_KW | CONST_KW => item(p),
            _ => p.err_and_bump("expected an item (`static` or `const`)"),
        }
    }
    m.complete(p, SOURCE_FILE);
}

fn item(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump_any(); // STATIC_KW | CONST_KW
    name(p, "expected a name for the item");
    if p.eat(COLON) {
        type_(p);
    }
    if p.eat(EQ) {
        expr(p);
    } else {
        p.error("expected `=` followed by the item's value");
    }
    p.eat(SEMICOLON);
    m.complete(p, STATIC_ITEM);
}

fn name(p: &mut Parser<'_>, msg: &str) {
    if p.at(IDENT) {
        let m = p.start();
        p.bump(IDENT);
        m.complete(p, NAME);
    } else {
        p.error(msg);
    }
}

fn name_ref(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(IDENT);
    m.complete(p, NAME_REF)
}

// ---- expressions ----

pub(crate) fn expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_bp(p, 0)
}

fn expr_bp(p: &mut Parser<'_>, min_bp: u8) -> Option<CompletedMarker> {
    let mut lhs = primary_expr(p)?;
    loop {
        // Calls bind tighter than any binary operator.
        if p.at(L_PAREN) {
            let m = lhs.precede(p);
            arg_list(p);
            lhs = m.complete(p, CALL_EXPR);
            continue;
        }
        let (l_bp, r_bp) = match p.current() {
            PLUS | MINUS => (1, 2),
            STAR | SLASH => (3, 4),
            _ => break,
        };
        if l_bp < min_bp {
            break;
        }
        let m = lhs.precede(p);
        p.bump_any(); // the operator
        if expr_bp(p, r_bp).is_none() {
            // error already reported; complete what we have
        }
        lhs = m.complete(p, BIN_EXPR);
    }
    Some(lhs)
}

fn primary_expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let m = match p.current() {
        INT_NUMBER | STRING => {
            let m = p.start();
            p.bump_any();
            m.complete(p, LITERAL)
        }
        IDENT => {
            let m = p.start();
            name_ref(p);
            m.complete(p, PATH_EXPR)
        }
        L_PAREN => {
            let m = p.start();
            p.bump(L_PAREN);
            expr(p);
            p.expect(R_PAREN, "`)`");
            m.complete(p, PAREN_EXPR)
        }
        L_BRACE => block_expr(p),
        FN_KW => fn_literal(p),
        _ => {
            if at_expr_recovery(p) {
                p.error("expected an expression");
            } else {
                p.err_and_bump("expected an expression");
            }
            return None;
        }
    };
    Some(m)
}

fn fn_literal(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(FN_KW);
    // `fn (` is ambiguous: parameter list, or the `fn expr` shorthand with a
    // parenthesized body. Parameters win iff the parens look like parameters:
    // `()`, or an IDENT followed by `:`/`,`/`)`.
    if p.at(L_PAREN) && looks_like_params(p) {
        param_list(p);
    }
    if p.at(THIN_ARROW) {
        ret_type(p);
    }
    // Body: a block is just one kind of expression; anything else is the
    // `fn expr` shorthand.
    expr(p);
    m.complete(p, FN_LITERAL)
}

fn looks_like_params(p: &Parser<'_>) -> bool {
    p.nth(1) == R_PAREN || (p.nth(1) == IDENT && matches!(p.nth(2), COLON | COMMA | R_PAREN))
}

fn param_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(L_PAREN);
    while !p.at(R_PAREN) && !p.at(EOF) {
        let before = p.pos();
        param(p);
        if !p.at(R_PAREN) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect(R_PAREN, "`)`");
    m.complete(p, PARAM_LIST);
}

fn param(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(IDENT) {
        name(p, "expected a parameter name");
        if p.eat(COLON) {
            type_(p);
        }
    } else if !p.at(COMMA) {
        p.err_and_bump("expected a parameter name");
    } else {
        p.error("expected a parameter name");
    }
    m.complete(p, PARAM);
}

fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(L_PAREN);
    while !p.at(R_PAREN) && !p.at(EOF) {
        let before = p.pos();
        expr(p);
        if !p.at(R_PAREN) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect(R_PAREN, "`)`");
    m.complete(p, ARG_LIST);
}

fn block_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        match p.current() {
            LET_KW => let_stmt(p),
            // Recover at the enclosing item: don't consume, and leave the
            // "expected `}`" report to the expect below.
            STATIC_KW | CONST_KW => break,
            SEMICOLON => p.bump_any(),
            _ => {
                let before = p.pos();
                expr_stmt_or_tail(p);
                if p.pos() == before {
                    p.err_and_bump("expected a statement");
                }
            }
        }
    }
    p.expect(R_BRACE, "`}`");
    m.complete(p, BLOCK_EXPR)
}

fn expr_stmt_or_tail(p: &mut Parser<'_>) {
    let m = p.start();
    let parsed = expr(p);
    if p.eat(SEMICOLON) {
        m.complete(p, EXPR_STMT);
        return;
    }
    if parsed.is_some() && !p.at(R_BRACE) && !p.at(EOF) {
        p.error("expected `;`");
        m.complete(p, EXPR_STMT);
        return;
    }
    // Tail expression (or nothing parsed): no EXPR_STMT wrapper.
    m.abandon(p);
}

fn let_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(LET_KW);
    name(p, "expected a binding name");
    if p.eat(COLON) {
        type_(p);
    }
    if p.eat(EQ) {
        expr(p);
    } else {
        p.error("expected `=` followed by an initializer");
    }
    p.expect(SEMICOLON, "`;`");
    m.complete(p, LET_STMT);
}

// ---- types ----

fn type_(p: &mut Parser<'_>) {
    match p.current() {
        L_PAREN => {
            let m = p.start();
            p.bump(L_PAREN);
            p.expect(R_PAREN, "`)` (only the unit type `()` is supported here)");
            m.complete(p, UNIT_TYPE);
        }
        BANG => {
            let m = p.start();
            p.bump(BANG);
            m.complete(p, NEVER_TYPE);
        }
        AMP => {
            let m = p.start();
            p.bump(AMP);
            p.eat(LIFETIME_IDENT);
            type_(p);
            m.complete(p, REF_TYPE);
        }
        FN_KW => {
            let m = p.start();
            p.bump(FN_KW);
            if p.eat(L_PAREN) {
                while !p.at(R_PAREN) && !p.at(EOF) {
                    let before = p.pos();
                    type_(p);
                    if !p.at(R_PAREN) {
                        p.expect(COMMA, "`,`");
                    }
                    if p.pos() == before {
                        break;
                    }
                }
                p.expect(R_PAREN, "`)`");
            }
            if p.at(THIN_ARROW) {
                ret_type(p);
            }
            m.complete(p, FN_TYPE);
        }
        IDENT => {
            let m = p.start();
            name_ref(p);
            m.complete(p, PATH_TYPE);
        }
        _ => p.error("expected a type"),
    }
}

fn ret_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(THIN_ARROW);
    type_(p);
    m.complete(p, RET_TYPE);
}
