//! The Must grammar, written against `Parser`. Resilience rules:
//! the parser never bails; unexpected tokens end up wrapped in `ERROR` nodes;
//! sub-parsers refuse to consume tokens their caller can recover on.

use crate::BRACE_RULE;
use crate::SyntaxKind::*;
use crate::parser::{CompletedMarker, Parser};

/// Tokens an expression parser must not consume on error: the enclosing
/// construct knows what to do with them.
fn at_expr_recovery(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        EOF | R_BRACE
            | R_PAREN
            | SEMICOLON
            | COMMA
            | STATIC_KW
            | CONST_KW
            | TYPE_KW
            | LET_KW
            | ELSE_KW
    )
}

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at(EOF) {
        match p.current() {
            STATIC_KW | CONST_KW | TYPE_KW => item(p),
            _ => p.err_and_bump("expected an item (`static`, `const` or `type`)"),
        }
    }
    m.complete(p, SOURCE_FILE);
}

fn item(p: &mut Parser<'_>) {
    let m = p.start();
    // `type Foo = expr;` shares the whole item shape with `static`/`const`
    // (superset parsing: a `: Type` annotation on a `type` item parses too;
    // validation rejects it with a removal fix). Only the node kind differs.
    let kind = if p.at(TYPE_KW) {
        TYPE_ITEM
    } else {
        STATIC_ITEM
    };
    p.bump_any(); // STATIC_KW | CONST_KW | TYPE_KW
    pattern(p, "expected a name for the item");
    if p.eat(COLON) {
        type_(p);
    }
    if p.eat(EQ) {
        expr(p);
        // Brace rule: items whose value ends in `}` don't need a `;`.
        // A value "ending" in `;` only happens in broken nesting (e.g. an
        // unclosed block) — demanding another `;` there is pure noise.
        if matches!(p.prev(), Some(R_BRACE | SEMICOLON)) {
            p.eat(SEMICOLON);
        } else {
            p.expect_after_prev(SEMICOLON);
        }
    } else {
        p.error("expected `=` followed by the item's value");
        p.eat(SEMICOLON);
    }
    m.complete(p, kind);
}

/// A pattern an assignment is destructured to
fn pattern(p: &mut Parser<'_>, msg: &str) {
    match p.current() {
        IDENT | HOLE => {
            let m = p.start();
            p.bump_any();
            m.complete(p, NAME);
        }
        _ => p.error(msg),
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
        // Field access sits in the same tier as calls, so `a.b.c`, `f().x`
        // and `a.b()` all fall out of this loop naturally.
        if p.at(DOT) {
            let m = lhs.precede(p);
            p.bump(DOT);
            // `name_ref` asserts it is at IDENT, so guard first.
            if p.at(IDENT) {
                name_ref(p);
            } else {
                p.error("expected a field name after `.`");
            }
            lhs = m.complete(p, FIELD_EXPR);
            continue;
        }
        let (l_bp, r_bp) = match p.current() {
            EQ2 | NEQ | L_ANGLE | R_ANGLE | LTEQ | GTEQ => (1, 2),
            PLUS | MINUS => (3, 4),
            STAR | SLASH => (5, 6),
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
    // `const` only starts an expression when immediately followed by `fn`
    // (a const fn literal) or `{` (a const block). Any other `const` is left
    // for the caller to recover on (see `at_expr_recovery` and the CONST_KW
    // arm in `block_expr`'s statement loop) — most commonly a misplaced item.
    if p.at(CONST_KW) {
        match p.nth(1) {
            FN_KW => return Some(fn_literal(p)),
            L_BRACE => return Some(const_block_expr(p)),
            _ => {}
        }
    }
    // `struct` only starts an expression when immediately followed by `{` (a
    // record literal). Unlike `const`, a dangling `struct` has no second life
    // as an item keyword, so when `{` doesn't follow we deliberately let it
    // fall through to the catch-all below, which consumes it as a garbage
    // token with "expected an expression" — there is nothing else useful a
    // caller could do with it.
    if p.at(STRUCT_KW) && p.nth(1) == L_BRACE {
        return Some(record_expr(p));
    }
    let m = match p.current() {
        INT_NUMBER | STRING | TRUE_KW | FALSE_KW => {
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
            p.expect_after_prev(R_PAREN);
            m.complete(p, PAREN_EXPR)
        }
        // A bare `{` is always a block. Record literals are introduced by the
        // `struct` keyword (handled above), so no lookahead is needed here.
        L_BRACE => block_expr(p),
        FN_KW => fn_literal(p),
        IF_KW => if_expr(p),
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
    p.eat(CONST_KW); // optional `const` marker; caller has already checked FN_KW follows
    p.bump(FN_KW);
    // In valid code bodies are blocks, so `(` after `fn` can only be
    // parameters — no lookahead needed anywhere in here.
    if p.at(L_PAREN) {
        param_list(p);
    }
    if p.at(THIN_ARROW) {
        ret_type(p);
    }
    if p.at(L_BRACE) {
        block_expr(p);
    } else if at_expr_recovery(p) {
        p.error(&format!("expected `{{`: {BRACE_RULE}"));
    } else {
        // Superset parsing: take any expression as the body so the tree
        // keeps the user's intent (inference and hover still work);
        // validation rejects it with a wrap-in-braces fix.
        expr(p);
    }
    m.complete(p, FN_LITERAL)
}

fn if_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(IF_KW);
    expr(p); // the condition
    branch(p, "`if` branches");
    if p.eat(ELSE_KW) {
        if p.at(IF_KW) {
            if_expr(p);
        } else {
            branch(p, "`else` branches");
        }
    }
    m.complete(p, IF_EXPR)
}

/// Superset parsing, same deal as `fn` bodies: take any expression so the
/// tree keeps the user's intent; validation rejects non-blocks with a
/// wrap-in-braces fix.
fn branch(p: &mut Parser<'_>, what: &str) {
    if p.at(L_BRACE) {
        block_expr(p);
    } else if at_expr_recovery(p) {
        p.error(format!("expected `{{`: {what} are blocks"));
    } else {
        expr(p);
    }
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
    p.expect_after_prev(R_PAREN);
    m.complete(p, PARAM_LIST);
}

fn param(p: &mut Parser<'_>) {
    let m = p.start();
    if matches!(p.current(), IDENT | HOLE | MUT_KW) {
        p.eat(MUT_KW);
        pattern(p, "expected a parameter name");
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
    p.expect_after_prev(R_PAREN);
    m.complete(p, ARG_LIST);
}

/// `struct { field, field: expr, ... }` — a record literal. The caller has
/// already confirmed `p.at(STRUCT_KW) && p.nth(1) == L_BRACE`.
fn record_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(STRUCT_KW);
    p.bump(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        let before = p.pos();
        if p.at(DOT3) {
            // Open-record marker: a bare token, validation rejects it later.
            p.bump(DOT3);
        } else {
            record_expr_field(p);
        }
        if !p.at(R_BRACE) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_BRACE);
    m.complete(p, RECORD_EXPR)
}

/// A record-literal field: `name` (shorthand for `name: name`) or `name: expr`.
fn record_expr_field(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(IDENT) {
        let nm = p.start();
        p.bump(IDENT);
        nm.complete(p, NAME_REF);
    } else {
        p.error("expected a field name");
    }
    if p.eat(COLON) {
        expr(p);
    }
    m.complete(p, RECORD_EXPR_FIELD);
}

fn block_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        match p.current() {
            LET_KW => let_stmt(p),
            // Recover at the enclosing item: don't consume, and leave the
            // "expected `}`" report to the expect below. `const fn` and
            // `const {` are expressions, not a misplaced item, so only bail
            // here when the one-token lookahead rules those out. `type`
            // never starts an expression, so it always means an item.
            STATIC_KW | TYPE_KW => break,
            CONST_KW if !matches!(p.nth(1), FN_KW | L_BRACE) => break,
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
    p.expect_after_prev(R_BRACE);
    m.complete(p, BLOCK_EXPR)
}

/// `const { ... }`: caller has already checked `L_BRACE` follows `CONST_KW`.
fn const_block_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(CONST_KW);
    block_expr(p);
    m.complete(p, CONST_BLOCK_EXPR)
}

fn expr_stmt_or_tail(p: &mut Parser<'_>) {
    let m = p.start();
    let parsed = expr(p);
    // Superset-parse any expression as the assignment target; validation
    // rejects anything but a plain variable. The RHS is parsed with `expr`,
    // which stops before a following `=` (it isn't a binary operator), so
    // `x = y = z` naturally falls through to the `;` expectation below
    // instead of chaining.
    if p.at(EQ) {
        p.bump(EQ);
        expr(p);
        p.expect_after_prev(SEMICOLON);
        m.complete(p, ASSIGN_STMT);
        return;
    }
    if p.eat(SEMICOLON) {
        m.complete(p, EXPR_STMT);
        return;
    }
    if parsed.is_some() && !p.at(R_BRACE) && !p.at(EOF) {
        p.error_after_prev(SEMICOLON);
        m.complete(p, EXPR_STMT);
        return;
    }
    // Tail expression (or nothing parsed): no EXPR_STMT wrapper.
    m.abandon(p);
}

fn let_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(LET_KW);
    p.eat(MUT_KW);
    pattern(p, "expected a binding name");
    if p.eat(COLON) {
        type_(p);
    }
    if p.eat(EQ) {
        expr(p);
    } else {
        p.error("expected `=` followed by an initializer");
    }
    p.expect_after_prev(SEMICOLON);
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
                p.expect_after_prev(R_PAREN);
            } else {
                p.error("expected `(`: function types are written `fn(...) -> ...`");
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
        HOLE => {
            let m = p.start();
            p.bump(HOLE);
            m.complete(p, HOLE_TYPE);
        }
        // Record types are introduced by `struct`; a bare `{` is not a type.
        STRUCT_KW => record_type(p),
        _ => p.error("expected a type"),
    }
}

/// `struct { name: Type, name: Type, ... }` — a record type. Dispatched on the
/// leading `struct` keyword; the `{` is expected (not guaranteed) afterwards.
fn record_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(STRUCT_KW);
    if p.expect(L_BRACE, "`{`") {
        while !p.at(R_BRACE) && !p.at(EOF) {
            let before = p.pos();
            if p.at(DOT3) {
                // Open-record marker: a bare token, validation rejects it later.
                p.bump(DOT3);
            } else {
                record_type_field(p);
            }
            if !p.at(R_BRACE) {
                p.expect(COMMA, "`,`");
            }
            if p.pos() == before {
                break;
            }
        }
        p.expect_after_prev(R_BRACE);
    }
    m.complete(p, RECORD_TYPE);
}

/// A record-type field: `name: Type`. The name is a declaration, so it is
/// wrapped in `NAME` (like `param`/`pattern`), not `NAME_REF`.
fn record_type_field(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(IDENT) {
        let nm = p.start();
        p.bump(IDENT);
        nm.complete(p, NAME);
    } else {
        p.error("expected a field name");
    }
    if p.eat(COLON) {
        type_(p);
    } else {
        p.error("expected `:` followed by the field's type");
    }
    m.complete(p, RECORD_TYPE_FIELD);
}

fn ret_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(THIN_ARROW);
    type_(p);
    m.complete(p, RET_TYPE);
}
