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
            | R_BRACKET
            | SEMICOLON
            | COMMA
            | STATIC_KW
            | CONST_KW
            | TYPE_KW
            | TRAIT_KW
            | LET_KW
            | ELSE_KW
            | WITH_KW
            | IMPL_KW
            | FOR_KW
    )
}

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at(EOF) {
        match p.current() {
            STATIC_KW | CONST_KW | TYPE_KW | TRAIT_KW => item(p),
            _ => p.err_and_bump("expected an item (`static`, `const`, `type` or `trait`)"),
        }
    }
    m.complete(p, SOURCE_FILE);
}

fn item(p: &mut Parser<'_>) {
    let m = p.start();
    // `type Foo = expr;` shares the whole item shape with `static`/`const`
    // (superset parsing: a `: Type` annotation on a `type` item parses too;
    // validation rejects it with a removal fix). Only the node kind — and
    // for `trait` items the RHS grammar — differs.
    let kind = match p.current() {
        TYPE_KW => TYPE_ITEM,
        TRAIT_KW => TRAIT_ITEM,
        _ => STATIC_ITEM,
    };
    p.bump_any(); // STATIC_KW | CONST_KW | TYPE_KW | TRAIT_KW
    pattern(p, "expected a name for the item");
    if p.eat(COLON) {
        type_(p);
    }
    if p.eat(EQ) {
        if kind == TRAIT_ITEM {
            trait_rhs(p);
        } else {
            expr(p);
        }
        // Attachment `with`-chains trail the RHS: `type X = struct { ... }
        // with { elements } with { ... };` (TR01). Parsed on any item
        // kind (superset — validation rejects them on `static`/`const`
        // items).
        while p.at(WITH_KW) {
            with_group(p);
        }
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

// ---- trait declarations (sealed trait-syntax grammar, TR01) ------------

/// The RHS of a `trait` item: the `requires` constructor
/// (`requires ::<binders>? clause,* { member* }`, optionally
/// `unsafe`-headed) or a trait-alias composition (`Eq + PartialOrd` —
/// parse-and-reserve). Only the plain non-generic `requires { ... }` form
/// is semantically supported; binders, clauses, `unsafe` and aliases
/// parse cleanly and are rejected by validation.
fn trait_rhs(p: &mut Parser<'_>) {
    if p.at(REQUIRES_KW) || (p.at(UNSAFE_KW) && p.nth(1) == REQUIRES_KW) {
        requires_def(p);
        return;
    }
    // Alias RHS: `Trait + Trait + ...` — reserved.
    let m = p.start();
    if p.at(IDENT) {
        type_(p);
        while p.eat(PLUS) {
            type_(p);
        }
    } else {
        p.error("expected `requires { ... }` or a trait-alias composition");
    }
    m.complete(p, TRAIT_ALIAS);
}

/// `unsafe? requires ::<binders>? clause,* { member* }` — the trait
/// constructor. Clauses (`Self: Iterator`) constrain like a `with`
/// clause; the brace holds the requirement members (the shared member
/// grammar: colon-declares are the live form, equals-defines are
/// reserved defaults).
fn requires_def(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(UNSAFE_KW);
    p.bump(REQUIRES_KW);
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    // Supertrait clauses: `Self: Bound + Bound, ...` — each opens with an
    // IDENT (the brace opens the member block, so the boundary is
    // token-recognizable).
    while p.at(IDENT) {
        requires_clause(p);
        if !p.at(L_BRACE) && !p.eat(COMMA) {
            break;
        }
    }
    if p.at(L_BRACE) {
        p.bump(L_BRACE);
        while !p.at(R_BRACE) && !p.at(EOF) {
            if at_member_recovery(p) {
                break;
            }
            let before = p.pos();
            member(p);
            if p.pos() == before {
                p.err_and_bump("expected a member");
            }
        }
        p.expect_after_prev(R_BRACE);
    } else {
        p.error("expected `{` followed by the trait's members");
    }
    m.complete(p, REQUIRES_DEF);
}

/// One supertrait clause: `Self: Bound + Bound` (parse-and-reserve).
fn requires_clause(p: &mut Parser<'_>) {
    let m = p.start();
    name_ref(p);
    if p.eat(COLON) {
        type_(p);
        while p.eat(PLUS) {
            type_(p);
        }
    } else {
        p.error("expected `:` after the clause's name");
    }
    m.complete(p, REQUIRES_CLAUSE);
}

// ---- attachment `with`-chains (sealed trait-syntax grammar, TR01) ------
//
// `with { impl Self { members } }` (inherent members) and `with { impl
// Trait { members } }` / `with { impl Type { members } }` (trait impls,
// at either home) are semantically supported; the FULL element grammar
// parses cleanly — element boundaries recognizable from tokens alone,
// heads readable while member bodies stay ordinary expressions — with
// modifier heads, markers and generic-owner impls rejected by validation
// ("not supported yet", the house parse-and-reserve pattern).

/// `with ::<binders>? clause,* { element* }` — one attachment group. The
/// turbofish DECLARES fresh binders, a clause CONSTRAINS (`T: Bound`) or
/// PINS (`T = usize`) an in-scope name; both are reserved (only the plain
/// `with { ... }` form is supported). The brace holds the group's
/// elements.
fn with_group(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(WITH_KW);
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    // Clause list: `T: Bound + Bound, U = usize, @b: @a, ...` — each clause
    // opens with an IDENT or a region (the brace opens the element block, so
    // the boundary is token-recognizable).
    while p.at(IDENT) || p.at(REGION_IDENT) {
        with_clause(p);
        if !p.at(L_BRACE) && !p.eat(COMMA) {
            break;
        }
    }
    if p.at(L_BRACE) {
        element_block(p);
    } else {
        p.error("expected `{` followed by the group's elements");
    }
    m.complete(p, WITH_GROUP);
}

/// One group clause: `T: Bound + Bound` (constrain), `T = usize` (pin), or
/// the outlives form `@b: @a + @c` (constrain, region-headed — a region
/// head takes only region bounds). Reserved (only plain groups are
/// supported).
fn with_clause(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(REGION_IDENT) {
        p.bump(REGION_IDENT);
        if p.eat(COLON) {
            region_bound(p);
            while p.eat(PLUS) {
                region_bound(p);
            }
        } else {
            p.error("expected `:` followed by the regions `@…` must outlive");
        }
        m.complete(p, WITH_CLAUSE);
        return;
    }
    name_ref(p);
    if p.eat(COLON) {
        type_(p);
        while p.eat(PLUS) {
            type_(p);
        }
    } else if p.eat(EQ) {
        type_(p);
    } else {
        p.error("expected `:` (constrain) or `=` (pin) after the clause's name");
    }
    m.complete(p, WITH_CLAUSE);
}

/// `{ element* }` — a group's (or modifier head's) element block.
fn element_block(p: &mut Parser<'_>) {
    p.bump(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        if at_item_recovery(p) {
            break;
        }
        let before = p.pos();
        element(p);
        if p.pos() == before {
            p.err_and_bump("expected an element (`impl`, `unsafe` or `for`)");
        }
    }
    p.expect_after_prev(R_BRACE);
}

/// Whether the current token can only mean an enclosing item continues.
/// The one item-keyword recovery set, asked wherever a nested construct
/// has to give up (the element loop, an arm body's skip): nothing
/// element-shaped and nothing expression-shaped starts with an item
/// keyword, so a copy of this list is only a chance for two copies to
/// disagree.
fn at_item_recovery(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        STATIC_KW | TYPE_KW | TRAIT_KW | LET_KW | CONST_KW
    )
}

/// The member-loop recovery set: like [`at_item_recovery`] minus the
/// member-shaped keyword openers — `type Item ...;` and `const N: usize;`
/// are (reserved) members, so those prefixes stay inside the body.
fn at_member_recovery(p: &Parser<'_>) -> bool {
    matches!(p.current(), STATIC_KW | TRAIT_KW | LET_KW)
        || (p.at(CONST_KW) && !(p.nth(1) == IDENT && p.nth(2) == COLON))
}

/// One element of a `with` group: an `impl` element, or a modifier head
/// (`unsafe`, `for ⟨Type⟩`) applying to the next element or a brace group
/// (TR01; composition is `for` outside `unsafe`). Both modifier heads are
/// reserved — the one supported form is the bare `impl` element.
fn element(p: &mut Parser<'_>) {
    match p.current() {
        IMPL_KW => impl_element(p),
        UNSAFE_KW => {
            let m = p.start();
            p.bump(UNSAFE_KW);
            element_or_group(p);
            m.complete(p, UNSAFE_ELEMENT);
        }
        FOR_KW => {
            let m = p.start();
            p.bump(FOR_KW);
            type_(p);
            element_or_group(p);
            m.complete(p, FOR_ELEMENT);
        }
        SEMICOLON => p.bump_any(),
        _ => {}
    }
}

/// A modifier head's payload: the next element, or `{ element* }`.
fn element_or_group(p: &mut Parser<'_>) {
    if p.at(L_BRACE) {
        element_block(p);
    } else if matches!(p.current(), IMPL_KW | UNSAFE_KW | FOR_KW) {
        element(p);
    } else {
        p.error("expected an element (`impl`, `unsafe` or `for`) or `{`");
    }
}

/// `impl ⟨head⟩ { member* }` (or the body-elided marker form
/// `impl ⟨head⟩;`). The head is a type mention: `Self` (inherent) or a
/// bare trait/type name (a trait impl) is supported; a generic-owner
/// name or a marker is superset-parsed and rejected by validation.
fn impl_element(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(IMPL_KW);
    if p.at(L_BRACE) || p.at(SEMICOLON) {
        // A head is not optional: saying so here is what stops validation
        // from reporting a *missing* head as an unsupported trait impl.
        p.error("expected a name after `impl`");
    } else {
        type_(p);
    }
    if p.at(L_BRACE) {
        p.bump(L_BRACE);
        while !p.at(R_BRACE) && !p.at(EOF) {
            if at_member_recovery(p) {
                break;
            }
            let before = p.pos();
            member(p);
            if p.pos() == before {
                p.err_and_bump("expected a member");
            }
        }
        p.expect_after_prev(R_BRACE);
    } else if !p.eat(SEMICOLON) {
        p.error("expected `{` followed by the impl's members, or `;`");
    }
    m.complete(p, IMPL_ELEMENT);
}

/// A colon-declared member's fn signature: like a fn literal's head
/// (named, annotated params; optional binder; return type) with no body —
/// wrapped in `FN_TYPE` so it sits in the tree as the annotation it is. The
/// plain fn TYPE grammar takes bare types, so the sealed spelling
/// (`alloc: fn(n: usize, v: Self) -> R;`) needs its own production; the
/// form itself is rejected, but it has to parse whole first for the
/// rejection to be the only error.
fn member_decl_fn_signature(p: &mut Parser<'_>) {
    let m = p.start();
    // `dealloc: unsafe fn(...)` — an unsafe-to-call requirement; parses
    // into the FN_TYPE (reserved: validation rejects it for now).
    p.eat(UNSAFE_KW);
    p.bump(FN_KW);
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    if p.at(L_PAREN) {
        param_list(p);
    } else {
        p.error("expected `(`");
    }
    if p.at(THIN_ARROW) {
        ret_type(p);
    }
    m.complete(p, FN_TYPE);
}

/// One member, the shared member grammar: equals-defines (`name =
/// fn(...) -> R { ... };` — an impl-body member, or a reserved default in
/// a requires body), colon-declares (`name: fn(...);` — a trait's
/// requirement form; rejected in impl bodies). `type Item = T;` and
/// `const N: usize;` members parse into the same node (reserved:
/// associated types/consts), distinguished by their leading keyword token.
fn member(p: &mut Parser<'_>) {
    match p.current() {
        SEMICOLON => {
            p.bump_any();
            return;
        }
        IDENT | TYPE_KW => {}
        CONST_KW if p.nth(1) == IDENT => {}
        _ => {
            p.error("expected a member");
            return;
        }
    }
    let m = p.start();
    // Reserved leading keywords: `type Item ...;` / `const N: usize;`.
    if p.at(TYPE_KW) || p.at(CONST_KW) {
        p.bump_any();
    }
    pattern(p, "expected a member name");
    if p.eat(COLON) {
        // The sealed colon-declare form spells NAMED params
        // (`alloc: fn(n: usize, v: Self) -> R;`), which the plain fn TYPE
        // grammar doesn't accept — parse a signature-shaped fn instead.
        if p.at(FN_KW) || (p.at(UNSAFE_KW) && p.nth(1) == FN_KW) {
            member_decl_fn_signature(p);
        } else {
            type_(p);
        }
    }
    if p.eat(EQ) {
        expr(p);
    }
    // The item brace rule, one level down: a member whose value ends in
    // `}` doesn't need its `;` re-demanded on broken nesting.
    if matches!(p.prev(), Some(R_BRACE | SEMICOLON)) {
        p.eat(SEMICOLON);
    } else {
        p.expect_after_prev(SEMICOLON);
    }
    m.complete(p, MEMBER);
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

/// A `let`/parameter pattern — construction's mirror image. A bare name or
/// `_` (`BIND_PAT`/wrapped `NAME`, holes included — same shape a `let`/param
/// name has always had, just now nested one level so it can sit alongside
/// the richer forms below), a `struct { ... }` record destructure
/// (`RECORD_PAT`), or `Name(pattern)` unwrapping a newtype (`NEWTYPE_PAT`).
/// Unlike [`match_pattern`] there is no bare `_`-as-whole-pattern distinct
/// node and no top-level `..` — those stay reserved for `match`.
fn binding_pattern(p: &mut Parser<'_>, msg: &str) {
    match p.current() {
        IDENT if p.nth(1) == L_PAREN => {
            newtype_pat(p);
        }
        IDENT | HOLE => {
            let m = p.start();
            pattern(p, msg);
            m.complete(p, BIND_PAT);
        }
        STRUCT_KW if p.nth(1) == L_BRACE => {
            record_pat(p);
        }
        _ => p.error(msg),
    }
}

/// `struct { x, y as z, mut w, .. }` — a record-destructuring pattern. The
/// caller has already confirmed `p.at(STRUCT_KW) && p.nth(1) == L_BRACE`.
///
/// Future note (pre-ruled with the equals-defines respell): when the
/// patterns round enriches field patterns, the sub-pattern spelling is
/// `field = pattern` — construction's mirror image under the global
/// colon-annotates/equals-defines split (`as`-renames may then retire).
fn record_pat(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(STRUCT_KW);
    p.bump(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        let before = p.pos();
        if p.at(DOT2) {
            let rest = p.start();
            p.bump(DOT2);
            rest.complete(p, REST_PAT);
        } else {
            record_pat_field(p);
        }
        if !p.at(R_BRACE) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_BRACE);
    m.complete(p, RECORD_PAT)
}

/// One field of a record pattern: `mut? name (as rename)?`. The field
/// name is wrapped in `NAME` (a declaration, not a reference) because the
/// shorthand spelling — no `as` — reuses the same token as the bound name's
/// declaration site, exactly like a bare `BIND_PAT` does.
fn record_pat_field(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(MUT_KW);
    pattern(p, "expected a field name");
    if p.eat(AS_KW) {
        pattern(p, "expected a binding name");
    }
    m.complete(p, RECORD_PAT_FIELD);
}

/// `Name(pattern)` — unwraps a newtype and destructures its underlying
/// shape. The caller has already confirmed `p.at(IDENT) && p.nth(1) ==
/// L_PAREN`.
fn newtype_pat(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    name_ref(p);
    p.bump(L_PAREN);
    binding_pattern(p, "expected a pattern");
    p.expect_after_prev(R_PAREN);
    m.complete(p, NEWTYPE_PAT)
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
        // Postfix deref `p.*` sits in the field-access tier: it chains as
        // just another postfix segment (`p.*.x`, `q.*.buf.*`), which is the
        // whole point of the spelling — no parens, ever. Lexed as DOT STAR;
        // checked before the field arm so a `.` followed by `*` never
        // half-parses as a broken field access.
        if p.at(DOT) && p.nth(1) == STAR {
            let m = lhs.precede(p);
            p.bump(DOT);
            p.bump(STAR);
            lhs = m.complete(p, DEREF_EXPR);
            continue;
        }
        // Postfix address-of `x.&raw` / `x.&raw mut` — the dual of `.*`,
        // sitting in the same field-access tier so it chains greedily
        // (`x.&raw mut.*`, `p.*.&raw mut`) — and its SAFE siblings
        // `x.&` / `x.&mut` (DOT AMP without `raw`), which carry an optional
        // region turbofish of their own (`x.&mut::<@a>`; body-local
        // annotations normally write `@_`). Lexed DOT AMP [raw] [mut];
        // checked before the plain field arm so the `.` never half-parses
        // as a broken field access.
        if p.at(DOT) && p.nth(1) == AMP {
            let m = lhs.precede(p);
            p.bump(DOT);
            p.bump(AMP);
            if p.at(RAW_KW) {
                p.bump(RAW_KW);
                p.eat(MUT_KW);
                lhs = m.complete(p, ADDR_OF_EXPR);
            } else {
                p.eat(MUT_KW);
                borrow_op_generic_args(p);
                lhs = m.complete(p, BORROW_EXPR);
            }
            continue;
        }
        // Indexing sits in the call/field tier too, so `a[0][1]`, `m[i].x`
        // and `f()[0]` all chain naturally. A `[` after an expression is
        // always an index — array *literals* only start expressions
        // (`primary_expr`), so there is no ambiguity to disambiguate.
        if p.at(L_BRACKET) {
            let m = lhs.precede(p);
            p.bump(L_BRACKET);
            expr(p);
            p.expect_after_prev(R_BRACKET);
            lhs = m.complete(p, INDEX_EXPR);
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
    if p.at(STRUCT_KW) && at_type_literal_body(p) {
        return Some(record_expr(p));
    }
    // `enum { ... }` — same dispatch scheme as `struct`. Grammar-wise it is
    // an expression (a `type` item's RHS parses with `expr`); validation
    // rejects it everywhere but as a `type` declaration's value.
    if p.at(ENUM_KW) && at_type_literal_body(p) {
        return Some(enum_expr(p));
    }
    // The RETIRED prefix `&raw x` / `&raw mut x` — kept only to emit the
    // migration diagnostic (see `addr_of_expr`); the live spelling is
    // postfix `x.&raw` / `x.&raw mut`. `&` alone is not an expression
    // starter (references are reserved), so the arm is gated on the `raw`
    // keyword following.
    if p.at(AMP) && p.nth(1) == RAW_KW {
        return Some(addr_of_expr(p));
    }
    // `-x` — unary minus on numbers. Same operand tier as the retired
    // prefix `&raw`: a primary expression plus its postfix chain, so
    // `-a.b` negates the field and `-x + y` stays a sum of the negation.
    if p.at(MINUS) {
        let m = p.start();
        p.bump(MINUS);
        expr_bp(p, 7);
        return Some(m.complete(p, NEG_EXPR));
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
            // `Shape::Circle` — a two-segment variant path. Exactly two
            // segments for now: a further plain `::` is left for the
            // caller to stumble over (there is nothing deeper to name
            // yet) — a turbofish immediately following the second segment
            // is its own case below.
            // `f::<usize, 42>` — a turbofish argument list, gated on the
            // same unambiguous `COLON2 L_ANGLE` lookahead as a generic
            // binder: nothing else follows `::` with `<`.
            if p.at(COLON2) {
                p.bump(COLON2);
                if p.at(L_ANGLE) {
                    generic_arg_list(p);
                    // `Option::<usize>::Some` — a variant of a generic
                    // enum: the turbofish sits on the enum, the variant
                    // segment follows.
                    if p.at(COLON2) {
                        p.bump(COLON2);
                        if p.at(IDENT) {
                            name_ref(p);
                            member_generic_args(p);
                        } else {
                            p.error("expected a variant name after `::`");
                        }
                    }
                } else if p.at(IDENT) {
                    name_ref(p);
                    member_generic_args(p);
                } else {
                    p.error("expected a variant name after `::`");
                }
            }
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
        L_BRACKET => array_expr(p),
        FN_KW => fn_literal(p),
        UNSAFE_KW => unsafe_block_expr(p),
        IF_KW => if_expr(p),
        MATCH_KW => match_expr(p),
        LOOP_KW => loop_expr(p),
        BREAK_KW => break_expr(p),
        CONTINUE_KW => continue_expr(p),
        RETURN_KW => return_expr(p),
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
    // `fn::<T, const V: usize>(...)` — the generic binder list. Gated on the
    // unambiguous two-token `COLON2 L_ANGLE` lookahead: nothing else legally
    // follows `fn` with a `::`.
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
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

/// `match scrutinee { arms }` — the basic braced form (the docs sketch
/// further forms: `match x => pat;`, `if match`, `match ... else`; those
/// land later as one coherent pattern-language feature).
fn match_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(MATCH_KW);
    // The scrutinee. `{` is not a postfix operator, so the expression
    // parser stops exactly at the arm list.
    expr(p);
    if p.at(L_BRACE) {
        p.bump(L_BRACE);
        while !p.at(R_BRACE) && !p.at(EOF) {
            // Recover at the enclosing item, same as block statements:
            // an item keyword inside an arm list means the `}` is missing.
            if matches!(p.current(), STATIC_KW | TYPE_KW | TRAIT_KW)
                || (p.at(CONST_KW) && !matches!(p.nth(1), FN_KW | L_BRACE))
            {
                break;
            }
            let before = p.pos();
            match_arm(p);
            if p.pos() == before {
                p.err_and_bump("expected a match arm");
            }
        }
        p.expect_after_prev(R_BRACE);
    } else {
        p.error("expected `{` followed by the match arms");
    }
    m.complete(p, MATCH_EXPR)
}

fn match_arm(p: &mut Parser<'_>) {
    let m = p.start();
    let before = p.pos();
    match_pattern(p);
    if p.pos() == before {
        // The pattern read nothing, so there is no arm here. Reading a
        // body anyway would judge it against the match's RESULT type and
        // invent a second error naming a type the user never wrote, so
        // take what is left of the arm as ERROR and recover at the next
        // one.
        let e = p.start();
        skip_arm_body(p);
        e.complete(p, ERROR);
        p.eat(COMMA);
        m.complete(p, MATCH_ARM);
        return;
    }
    p.expect(FAT_ARROW, "`=>`");
    expr(p);
    // Brace rule, as for items: an arm whose body ends in `}` doesn't need
    // the `,`. A trailing comma before the closing `}` is fine.
    if !p.at(R_BRACE) {
        if matches!(p.prev(), Some(R_BRACE)) {
            p.eat(COMMA);
        } else {
            p.expect(COMMA, "`,`");
        }
    }
    m.complete(p, MATCH_ARM);
}

/// Skip what is left of an arm whose pattern read nothing, up to the next
/// arm or the end of the list. NESTING-AWARE: an arm body is an ordinary
/// expression, so its own `,` and `}` (`{ a, b }`, `g(x, y)`) are not the
/// arm list's separators. Counting bracket depth is what keeps a bodied
/// arm at the one pattern error — stopping at the body's own `}` would
/// close the arm list on it and cascade through everything after.
fn skip_arm_body(p: &mut Parser<'_>) {
    let mut depth = 0u32;
    loop {
        match p.current() {
            EOF => return,
            L_BRACE | L_PAREN | L_BRACKET => depth += 1,
            R_BRACE | R_PAREN | R_BRACKET if depth == 0 => return,
            R_BRACE | R_PAREN | R_BRACKET => depth -= 1,
            // At depth 0 these end the arm; nested, they are the body's
            // own.
            COMMA | SEMICOLON if depth == 0 => return,
            // An item keyword at depth 0 means the arm list's `}` is
            // missing and an enclosing item resumed. Asking
            // [`at_item_recovery`] rather than repeating its tokens is
            // what keeps the skip from stopping at a smaller set than the
            // arm loop itself recovers on.
            _ if depth == 0 && at_item_recovery(p) => return,
            _ => {}
        }
        p.bump_any();
    }
}

/// A match-arm pattern — deliberately flat in v1: `_`, a plain binding
/// name, and a variant pattern in one of three spellings — qualified
/// `Enum::Variant(bindings...)`, elided `::Variant(bindings...)` (the
/// scrutinee's enum, enum segment dropped), and the retired unqualified
/// `Name(bindings...)` (still parsed as a `VARIANT_PAT` so `validation`
/// can hand back an honest "write `::Name(...)`" error — patterns have no
/// calls). Plus the *reserved* `..` (parses, validation rejects it). A
/// bare name with no `::` and no parens is *always* a binding now — never
/// reinterpreted type-directed as a variant (see `infer.rs`'s
/// `check_match_pat` `PatData::Bind` arm). No nesting, or-patterns, guards
/// or literal patterns yet.
fn match_pattern(p: &mut Parser<'_>) {
    match p.current() {
        HOLE => {
            let m = p.start();
            p.bump(HOLE);
            m.complete(p, WILDCARD_PAT);
        }
        // Reserved for record patterns (`Foo(struct { x, .. })` and
        // friends): the token parses wherever a pattern does, validation
        // rejects it for now.
        DOT2 => {
            let m = p.start();
            p.bump(DOT2);
            m.complete(p, REST_PAT);
        }
        // The elided sigil spelling `::Variant(...)`: a variant of the
        // scrutinee's enum with the enum segment dropped. Exactly one
        // `NameRef` child (the variant), with the `COLON2` *before* it.
        COLON2 => {
            let m = p.start();
            p.bump(COLON2);
            if p.at(IDENT) {
                name_ref(p);
            } else {
                p.error("expected a variant name after `::`");
            }
            if p.at(L_PAREN) {
                pattern_binding_list(p);
            }
            m.complete(p, VARIANT_PAT);
        }
        IDENT => {
            // `Name::…` (qualified) or the retired `Name(...)` shape parse
            // as a variant pattern; a bare name alone is *always* a binding
            // now (no type-directed reinterpretation). The retired
            // `Name(...)` shape is kept parseable only so `validation` can
            // report an honest "write `::Name(...)`" error.
            if matches!(p.nth(1), COLON2 | L_PAREN) {
                let m = p.start();
                name_ref(p);
                if p.eat(COLON2) {
                    if p.at(IDENT) {
                        name_ref(p);
                    } else {
                        p.error("expected a variant name after `::`");
                    }
                }
                if p.at(L_PAREN) {
                    pattern_binding_list(p);
                }
                m.complete(p, VARIANT_PAT);
            } else {
                let m = p.start();
                let nm = p.start();
                p.bump(IDENT);
                nm.complete(p, NAME);
                m.complete(p, BIND_PAT);
            }
        }
        _ => p.error("expected a pattern"),
    }
}

/// The positional bindings of a variant pattern: names, `_` holes, and the
/// reserved `..` rest marker.
fn pattern_binding_list(p: &mut Parser<'_>) {
    p.bump(L_PAREN);
    while !p.at(R_PAREN) && !p.at(EOF) {
        let before = p.pos();
        match p.current() {
            IDENT | HOLE => pattern(p, "expected a binding name"),
            DOT2 => {
                let m = p.start();
                p.bump(DOT2);
                m.complete(p, REST_PAT);
            }
            FAT_ARROW => {
                // The arm's own arrow: the `)` is missing — don't eat it.
                p.error("expected a binding name");
                break;
            }
            _ if at_expr_recovery(p) => {
                p.error("expected a binding name");
                break;
            }
            _ => p.err_and_bump("expected a binding name"),
        }
        if !p.at(R_PAREN) && !p.at(FAT_ARROW) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_PAREN);
}

/// `loop { ... }` — an infinite loop; `break`/`continue` steer it. The body
/// parses with `block_expr` directly (bare braces stay blocks, no
/// lookahead); like `if` branches, any other expression superset-parses and
/// validation rejects it with a wrap-in-braces fix.
fn loop_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(LOOP_KW);
    branch(p, "`loop` bodies");
    m.complete(p, LOOP_EXPR)
}

/// `break` with an optional value. A primary expression (typed `!` by
/// inference), so it composes in statement position through ordinary
/// expression statements — and pathologies like `break loop { ... }` just
/// fall out of the grammar.
fn break_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(BREAK_KW);
    // A value only when the next token can start one: `break;` must not
    // swallow its `;` (or the caller's recovery tokens) hunting for a value.
    if at_expr_start(p) {
        expr(p);
    }
    m.complete(p, BREAK_EXPR)
}

fn continue_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(CONTINUE_KW);
    m.complete(p, CONTINUE_EXPR)
}

/// `return` with an optional value — `break`'s sibling, one tier up: it
/// exits the enclosing *body* (a `fn` literal or a `const` block) instead
/// of the enclosing `loop`. A primary expression (typed `!` by inference),
/// so `let x = if c { 1 } else { return 0 };` and a `return` block tail
/// need no statement-level special case. The optional value is gated on
/// [`at_expr_start`] for exactly `break`'s reason: `return;` must not go
/// hunting for a value past its own `;`.
fn return_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(RETURN_KW);
    if at_expr_start(p) {
        expr(p);
    }
    m.complete(p, RETURN_EXPR)
}

/// `[e1, e2, e3]` — an array literal — or `[e; N]` — the repeat form. One
/// node kind for both: the `;` decides (see `ast::ArrayExpr`). Elements are
/// ordinary expressions; the repeat count parses as an expression too and
/// is restricted semantically (a const argument: a literal or a const
/// parameter — inference rejects the rest).
fn array_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(L_BRACKET);
    if p.eat(R_BRACKET) {
        // `[]` — the empty array literal.
        return m.complete(p, ARRAY_EXPR);
    }
    expr(p);
    if p.eat(SEMICOLON) {
        // The repeat form: `[e; N]`.
        expr(p);
    } else {
        while p.at(COMMA) {
            p.bump(COMMA);
            if p.at(R_BRACKET) {
                break; // trailing comma
            }
            let before = p.pos();
            expr(p);
            if p.pos() == before {
                break;
            }
        }
    }
    p.expect_after_prev(R_BRACKET);
    m.complete(p, ARRAY_EXPR)
}

/// The RETIRED prefix address-of `&raw x` / `&raw mut x`. The caller has
/// already confirmed `p.at(AMP) && p.nth(1) == RAW_KW`. Raw borrows are now
/// spelled postfix (`x.&raw` / `x.&raw mut`); the prefix form superset-parses
/// into the same `ADDR_OF_EXPR` node (never a silent reinterpretation) so
/// downstream keeps working, with a targeted migration diagnostic anchored on
/// the leading `&`. The operand parses with a binding power above every
/// binary operator, so it takes exactly a primary expression plus its postfix
/// chain — a superset of the place expressions the language accepts.
fn addr_of_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.error("raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`");
    p.bump(AMP);
    p.bump(RAW_KW);
    p.eat(MUT_KW);
    expr_bp(p, 7);
    m.complete(p, ADDR_OF_EXPR)
}

/// `unsafe { ... }` — an expression-position block that marks a checker
/// region (deref of a raw pointer is legal inside). Same shape as
/// `const { ... }`. `unsafe fn` superset-parses (the literal becomes the
/// node's child) so validation can reject it with an honest "not
/// supported yet"; any other non-block body gets the wrap-in-braces
/// treatment, like `if` branches.
fn unsafe_block_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(UNSAFE_KW);
    if p.at(L_BRACE) {
        block_expr(p);
    } else if p.at(FN_KW) || (p.at(CONST_KW) && p.nth(1) == FN_KW) {
        // Reserved: `unsafe fn ...` parses whole, validation rejects it.
        fn_literal(p);
    } else if at_expr_recovery(p) {
        p.error("expected `{`: `unsafe` blocks are blocks");
    } else {
        expr(p);
    }
    m.complete(p, UNSAFE_BLOCK_EXPR)
}

/// Whether the current token can start an expression — the dispatch set of
/// `primary_expr`, including its one-token-lookahead `const`/`struct`/`enum`
/// cases. Used where an expression is *optional* (a `break` value).
fn at_expr_start(p: &Parser<'_>) -> bool {
    match p.current() {
        INT_NUMBER | STRING | TRUE_KW | FALSE_KW | IDENT | L_PAREN | L_BRACE | L_BRACKET
        | FN_KW | IF_KW | MATCH_KW | LOOP_KW | BREAK_KW | CONTINUE_KW | RETURN_KW | UNSAFE_KW
        | MINUS => true,
        CONST_KW => matches!(p.nth(1), FN_KW | L_BRACE),
        STRUCT_KW | ENUM_KW => at_type_literal_body(p),
        AMP => p.nth(1) == RAW_KW,
        _ => false,
    }
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
    if matches!(p.current(), IDENT | HOLE | MUT_KW) || (p.at(STRUCT_KW) && p.nth(1) == L_BRACE) {
        p.eat(MUT_KW);
        binding_pattern(p, "expected a parameter name");
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

/// `::<T, const V: usize>` — a generic fn literal's binder list. The caller
/// has already confirmed `p.at(COLON2) && p.nth(1) == L_ANGLE`.
fn generic_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(COLON2);
    p.bump(L_ANGLE);
    while !p.at(R_ANGLE) && !p.at(EOF) {
        let before = p.pos();
        generic_param(p);
        if !p.at(R_ANGLE) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_ANGLE);
    m.complete(p, GENERIC_PARAM_LIST);
}

/// One generic parameter: a region (`REGION_PARAM`), a bare name
/// (`TYPE_PARAM`) or `const name: Type` (`CONST_PARAM`) — one binder list,
/// three kinds. Regions ride the same slot as types and consts, told apart
/// by their sigil, and are a distinguished kind downstream: erased, never
/// reaching instance keys.
fn generic_param(p: &mut Parser<'_>) {
    if p.at(REGION_IDENT) {
        let m = p.start();
        p.bump(REGION_IDENT);
        // `@b: @a + @c` — outlives bounds live where the param is born,
        // exactly like a type param's trait bounds (TR05).
        if p.eat(COLON) {
            region_bound(p);
            while p.eat(PLUS) {
                region_bound(p);
            }
        }
        m.complete(p, REGION_PARAM);
        return;
    }
    if p.at(CONST_KW) {
        let m = p.start();
        p.bump(CONST_KW);
        pattern(p, "expected a name for the const parameter");
        if p.eat(COLON) {
            type_(p);
        } else {
            p.error("expected `:` followed by the const parameter's type");
        }
        m.complete(p, CONST_PARAM);
    } else if matches!(p.current(), IDENT | HOLE) {
        let m = p.start();
        pattern(p, "expected a type parameter name");
        // `T: Display + Debug` — bounds live where params are born (TR05:
        // binder-position bounds, `+` composition).
        if p.eat(COLON) {
            type_(p);
            while p.eat(PLUS) {
                type_(p);
            }
        }
        m.complete(p, TYPE_PARAM);
    } else if !p.at(COMMA) && !p.at(R_ANGLE) {
        p.err_and_bump("expected a generic parameter");
    } else {
        p.error("expected a generic parameter");
    }
}

/// One region on the right of an outlives `:` — always a region name, never
/// a type. Kept a bare token (no wrapper node): the bound list of a
/// [`REGION_PARAM`] is exactly its `REGION_IDENT` children after the first.
fn region_bound(p: &mut Parser<'_>) {
    if !p.eat(REGION_IDENT) {
        p.error("expected a region name (`@a`) after `:`");
    }
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

/// `::<usize, 42, const LEN>` — a turbofish argument list. The caller has
/// already bumped the `COLON2` and confirmed `p.at(L_ANGLE)`.
fn generic_arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(L_ANGLE);
    while !p.at(R_ANGLE) && !p.at(EOF) {
        let before = p.pos();
        generic_arg(p);
        if !p.at(R_ANGLE) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_ANGLE);
    m.complete(p, GENERIC_ARG_LIST);
}

/// `::<usize>` written on a path's SECOND segment (`Pair::first::<usize>`,
/// `Shape::Circle::<usize>`) — that segment's OWN generic arguments.
///
/// The list gets a node of its own so the TREE says whose arguments these
/// are: the owner's turbofish is a direct `GENERIC_ARG_LIST` child of the
/// `PATH_EXPR` (`Pair::<usize>::first`), the second segment's hangs one
/// level down, inside `MEMBER_GENERIC_ARGS`. That is the whole structural
/// difference, and it is what keeps `PathExpr::generic_arg_list()` meaning
/// the OWNER's list — no consumer can read one as the other by accident.
///
/// Nothing is diagnosed here. `size::<usize>` on a member is FUTURE-LEGAL
/// by declared intent (member-own binders are reserved, not rejected: one
/// day `size = fn::<T>(m: Self, t: T) -> usize` will declare one and
/// `Measured::size::<usize>` will apply it), so it parses as the tree it
/// really is and hir states the reservation — granting it later deletes a
/// diagnostic instead of changing the grammar. `Shape::Circle::<usize>`
/// parses the same shape and gets hir's variant-flavored correction: the
/// two readings differ by what the segment NAMES, which is not a question
/// the parser can answer.
///
/// A no-op unless the two-token `COLON2 L_ANGLE` lookahead is there — the
/// same unambiguous gate every other turbofish uses.
fn member_generic_args(p: &mut Parser<'_>) {
    if !(p.at(COLON2) && p.nth(1) == L_ANGLE) {
        return;
    }
    let m = p.start();
    p.bump(COLON2);
    generic_arg_list(p);
    m.complete(p, MEMBER_GENERIC_ARGS);
}

/// One turbofish argument. Disambiguated by form, not by the declaration
/// (trees stay stable under declaration edits). The const-arg forms are:
///
/// - a literal-shaped token (`42`, `"x"`, `true`, `false`) — const by form;
/// - `const` + a bare name (`const N`) — a forced value reading of a name
///   that would otherwise parse as a type;
/// - `const` + a literal (`const 42`) — also legal;
/// - `const { ... }` — a const block, delegated to the shared const-block
///   parser so its const-context checking, staging, evaluation and blame all
///   come from the existing `const { ... }` machinery unchanged.
///
/// A NAMED argument (`Self = Type`) is recognized by form too — a bare name
/// followed by `=`. TR01 gives v1 exactly one nameable argument (`Self`,
/// position irrelevant); the grammar accepts any name and leaves "only
/// `Self` is nameable" to semantics, so the tree stays stable and the
/// diagnostic can be precise.
///
/// Everything else — any type, including the `_` hole — parses as `TYPE_ARG`.
/// Whether a given position actually accepts a const or a type is a semantic
/// question, deferred.
///
/// There is no additive-precedence or paren escape any more, and a bare
/// braced block is not accepted either: a compound expression must be
/// written `const { ... }` (the `const` keyword is required in v1). A bare
/// `>`/`<` still closes the list, so comparisons live inside a const block.
///
/// A `TYPE_ARG`'s type is parsed with the ordinary `type_` grammar, which
/// means a nested turbofish (`Pair::<Pair::<usize>>`) parses today as a
/// side effect — there is no lexer-level `>>` merge in this language (every
/// `>` is its own token), so no ambiguity forces restricting this. Nothing
/// downstream acts on nested generic args yet.
fn generic_arg(p: &mut Parser<'_>) {
    // `Self = Type` — a named argument, recognized by the two-token
    // `IDENT EQ` lookahead (nothing else follows a bare name with `=` in
    // an argument list).
    if p.at(IDENT) && p.nth(1) == EQ {
        let m = p.start();
        name_ref(p);
        p.bump(EQ);
        type_(p);
        m.complete(p, NAMED_ARG);
        return;
    }
    match p.current() {
        // Region by form: the `@` sigil. `@a`, the wildcard `@_`, or the
        // join `@a + @b` — one argument, several regions, "outlived by all
        // of them" (`+` reads as conjunction here, exactly as bound
        // composition does).
        REGION_IDENT => {
            let m = p.start();
            p.bump(REGION_IDENT);
            while p.eat(PLUS) {
                region_bound(p);
            }
            m.complete(p, REGION_ARG);
        }
        // Const by form: a bare literal.
        INT_NUMBER | STRING | TRUE_KW | FALSE_KW => {
            let m = p.start();
            let lit = p.start();
            p.bump_any();
            lit.complete(p, LITERAL);
            m.complete(p, CONST_ARG);
        }
        // A bare braced block is not a const arg in v1 — the `const` keyword
        // is required (`const { ... }`). Consume the block under an ERROR
        // node so recovery is clean, and point at the required spelling.
        L_BRACE => {
            let m = p.start();
            p.error("a braced const argument must be written `const { ... }`");
            block_expr(p);
            m.complete(p, ERROR);
        }
        // `const { ... }` — a const block; delegate to the shared parser so
        // the tree and its compile-time semantics (const-context checking,
        // staging, evaluation, blame) come from the existing `const { ... }`
        // machinery unchanged.
        CONST_KW if p.nth(1) == L_BRACE => {
            let m = p.start();
            const_block_expr(p);
            m.complete(p, CONST_ARG);
        }
        CONST_KW => {
            let m = p.start();
            p.bump(CONST_KW);
            // After `const`, only a bare name (forced value reading) or a
            // literal is legal. A compound expression must be a const block —
            // `const N + 1` and `const (a > b)` are gone; point at braces.
            match p.current() {
                INT_NUMBER | STRING | TRUE_KW | FALSE_KW => {
                    let lit = p.start();
                    p.bump_any();
                    lit.complete(p, LITERAL);
                }
                IDENT => {
                    let path = p.start();
                    name_ref(p);
                    path.complete(p, PATH_EXPR);
                }
                _ => {
                    p.error(
                        "expected a name, literal, or `{ ... }` block after `const`; \
                         wrap a compound expression in `const { ... }`",
                    );
                }
            }
            m.complete(p, CONST_ARG);
        }
        _ => {
            let m = p.start();
            type_(p);
            m.complete(p, TYPE_ARG);
        }
    }
}

/// Whether the token after a `struct`/`enum` keyword opens a type-literal
/// body: `{` directly, or a `::<...>` generic binder (which the body's `{`
/// then follows) — `struct::<T> { a: T }`.
fn at_type_literal_body(p: &Parser<'_>) -> bool {
    p.nth(1) == L_BRACE || (p.nth(1) == COLON2 && p.nth(2) == L_ANGLE)
}

/// `struct { field, field: expr, ... }` — a record literal, with an
/// optional `::<T, const N: usize>` binder between the keyword and the
/// braces (`type Pair = struct::<T> { ... }`). The caller has already
/// confirmed `p.at(STRUCT_KW)` and [`at_type_literal_body`].
fn record_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(STRUCT_KW);
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    if !p.expect(L_BRACE, "`{`") {
        return m.complete(p, RECORD_EXPR);
    }
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

/// `enum { Variant(Type, ...), Variant, ... }` — an enum literal (only
/// meaningful as a `type` declaration's RHS), with an optional generic
/// binder like [`record_expr`]'s. The caller has already confirmed
/// `p.at(ENUM_KW)` and [`at_type_literal_body`].
fn enum_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(ENUM_KW);
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    if !p.expect(L_BRACE, "`{`") {
        return m.complete(p, ENUM_EXPR);
    }
    while !p.at(R_BRACE) && !p.at(EOF) {
        let before = p.pos();
        enum_variant(p);
        if !p.at(R_BRACE) {
            p.expect(COMMA, "`,`");
        }
        if p.pos() == before {
            break;
        }
    }
    p.expect_after_prev(R_BRACE);
    m.complete(p, ENUM_EXPR)
}

/// One variant of an enum literal: a name (a declaration, so `NAME` like
/// record-type fields) with an optional parenthesized list of positional
/// payload *types* — payloads are type syntax, not expressions.
fn enum_variant(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(IDENT) {
        let nm = p.start();
        p.bump(IDENT);
        nm.complete(p, NAME);
    } else {
        p.error("expected a variant name");
    }
    if p.at(L_PAREN) {
        p.bump(L_PAREN);
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
    }
    m.complete(p, ENUM_VARIANT);
}

/// A record-literal field, the full member production `name[: Type][= value]`
/// (the equals-defines respell, G12): `name = expr` defines,
/// `name: Type` annotates a TYPE — everywhere, uniformly — `name: Type =
/// expr` does both, and bare `name` is shorthand for `name = name`. The
/// retired construction spelling `name: value` gets a targeted parse error
/// on literal-shaped values (never a silent reinterpretation — the shape
/// may return later as a value-type annotation); an identifier after the
/// colon reads as the type it now is, and the value-less-field check
/// carries the story from there. `pub` superset-parses here too (a `type`
/// declaration's shape is written as a `struct` literal); validation
/// rejects it everywhere fields appear.
fn record_expr_field(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(PUB_KW);
    if p.at(IDENT) {
        let nm = p.start();
        p.bump(IDENT);
        nm.complete(p, NAME_REF);
    } else {
        p.error("expected a field name");
    }
    if p.eat(COLON) {
        if matches!(
            p.current(),
            INT_NUMBER | STRING | TRUE_KW | FALSE_KW | MINUS
        ) {
            // The retired `name: value` construction spelling, recognized
            // on a literal-shaped value before anything is consumed. A
            // composite value (`x: foo()`, `x: fn () -> R { .. }`) is
            // type-shaped at its first token and is left to the ordinary
            // type grammar: by the time it gives itself away the field is
            // already unrecoverable, so naming the retired spelling there
            // buries the message in a cascade instead of replacing one.
            p.error("record fields are defined with `=` (`name = value`); `:` annotates a type");
            expr(p);
        } else {
            type_(p);
        }
    }
    if p.eat(EQ) {
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
            // and `trait` never start an expression, so they always mean
            // an item.
            STATIC_KW | TYPE_KW | TRAIT_KW => break,
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
    binding_pattern(p, "expected a binding name");
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
    let Some(mut lhs) = type_core(p) else { return };
    loop {
        // Postfix raw pointer `T.&raw` / `T.&raw mut`, mirroring the
        // expression-side postfix address-of, and the SAFE borrow types
        // `T.&::<@a>` / `T.&mut::<@a>` (DOT AMP without `raw`), whose
        // region rides the borrow operator's own turbofish. Chains the same
        // way its expression dual does (`T.&raw mut.&raw`,
        // `T.&::<@a>.&::<@b>`). Lexed DOT AMP [raw] [mut].
        if p.at(DOT) && p.nth(1) == AMP {
            let m = lhs.precede(p);
            p.bump(DOT);
            p.bump(AMP);
            if p.at(RAW_KW) {
                p.bump(RAW_KW);
                p.eat(MUT_KW);
                lhs = m.complete(p, RAW_PTR_TYPE);
            } else {
                p.eat(MUT_KW);
                borrow_op_generic_args(p);
                lhs = m.complete(p, BORROW_TYPE);
            }
            continue;
        }
        break;
    }
}

/// The borrow operator's OWN turbofish — `T.&::<@a>`, `x.&mut::<@_>`. A
/// no-op unless the two-token `COLON2 L_ANGLE` lookahead is there (the same
/// unambiguous gate every other turbofish uses); the list is an ordinary
/// [`generic_arg_list`], so a wrong-kind argument (`T.&::<usize>`) parses
/// into the tree it really is and hir says what was expected.
///
/// The list hangs directly off the `BORROW_TYPE`/`BORROW_EXPR` node, which
/// has no other generic-argument child, so no ownership wrapper is needed
/// (unlike a path's second segment — see [`member_generic_args`]).
fn borrow_op_generic_args(p: &mut Parser<'_>) {
    if !(p.at(COLON2) && p.nth(1) == L_ANGLE) {
        return;
    }
    p.bump(COLON2);
    generic_arg_list(p);
}

/// The core (non-postfix) type. Returns `None` only when nothing was parsed
/// (the `_ => error` arm), so the postfix loop in [`type_`] has no operand.
fn type_core(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let cm = match p.current() {
        L_PAREN => {
            let m = p.start();
            p.bump(L_PAREN);
            p.expect(R_PAREN, "`)` (only the unit type `()` is supported here)");
            m.complete(p, UNIT_TYPE)
        }
        BANG => {
            let m = p.start();
            p.bump(BANG);
            m.complete(p, NEVER_TYPE)
        }
        // The RETIRED prefix raw pointer type `&raw T` / `&raw mut T`. Raw
        // pointers are now spelled postfix (`T.&raw` / `T.&raw mut`); the
        // prefix form superset-parses into the same `RAW_PTR_TYPE` node (never
        // a silent reinterpretation) with a migration diagnostic.
        AMP if p.nth(1) == RAW_KW => {
            let m = p.start();
            p.error("raw pointer types are spelled postfix: `T.&raw` / `T.&raw mut`");
            p.bump(AMP);
            p.bump(RAW_KW);
            p.eat(MUT_KW);
            type_(p);
            m.complete(p, RAW_PTR_TYPE)
        }
        // Without the `raw` keyword the `&` parses as a reference type, which
        // stays reserved ("references are not supported yet", see validation)
        // — `&T`/`&mut T` are kept unclaimed for real references, which stay
        // unspoken for. Safe borrows spell postfix `T.&`/`T.&mut` instead, a
        // distinct `BORROW_TYPE` node parsed in `type_`'s own postfix loop
        // above, never through this prefix arm.
        AMP => {
            let m = p.start();
            p.bump(AMP);
            p.eat(LIFETIME_IDENT);
            type_(p);
            m.complete(p, REF_TYPE)
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
            m.complete(p, FN_TYPE)
        }
        IDENT => {
            let m = p.start();
            name_ref(p);
            // `Shape::Circle` in type position: a variant type. `Pair::<T>`
            // in type position: a generic type mention (future — no generic
            // type declarations exist yet, but the syntax parses uniformly
            // with the expression side).
            if p.at(COLON2) {
                p.bump(COLON2);
                if p.at(L_ANGLE) {
                    generic_arg_list(p);
                } else if p.at(IDENT) {
                    name_ref(p);
                } else {
                    p.error("expected a variant name after `::`");
                }
            }
            m.complete(p, PATH_TYPE)
        }
        HOLE => {
            let m = p.start();
            p.bump(HOLE);
            m.complete(p, HOLE_TYPE)
        }
        // Record types are introduced by `struct`; a bare `{` is not a type.
        STRUCT_KW => record_type(p),
        L_BRACKET => array_type(p),
        _ => {
            p.error("expected a type");
            return None;
        }
    };
    Some(cm)
}

/// `[T; N]` — a fixed-size array type. The length is a const argument in
/// annotation position, so it follows the ruled const-arg forms: a bare
/// literal, a bare (const-param) name, or the `const`-prefixed spellings —
/// including `const { ... }`, which parses here (resilience) but is always
/// rejected semantically (type identity lives on the eval-free path; see
/// `hir`'s annotation mirror).
fn array_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(L_BRACKET);
    type_(p);
    if p.expect(SEMICOLON, "`;` (array types are written `[T; N]`)") {
        array_len_arg(p);
    }
    p.expect_after_prev(R_BRACKET);
    m.complete(p, ARRAY_TYPE)
}

/// The length of an [`array_type`]: a `CONST_ARG` node, same shapes as a
/// turbofish's const argument ([`generic_arg`]'s const arms) plus the bare
/// name — the position is unambiguously a value, so no `const` sigil is
/// needed to force the reading.
fn array_len_arg(p: &mut Parser<'_>) {
    match p.current() {
        INT_NUMBER | STRING | TRUE_KW | FALSE_KW => {
            let m = p.start();
            let lit = p.start();
            p.bump_any();
            lit.complete(p, LITERAL);
            m.complete(p, CONST_ARG);
        }
        // A bare name: a const parameter (`[T; N]`).
        IDENT => {
            let m = p.start();
            let path = p.start();
            name_ref(p);
            path.complete(p, PATH_EXPR);
            m.complete(p, CONST_ARG);
        }
        // `const { ... }` / `const N` / `const 42` — accepted by the
        // grammar so the tree keeps the user's intent; the block spelling
        // is rejected semantically (see `array_type`'s doc).
        CONST_KW if p.nth(1) == L_BRACE => {
            let m = p.start();
            const_block_expr(p);
            m.complete(p, CONST_ARG);
        }
        CONST_KW => {
            let m = p.start();
            p.bump(CONST_KW);
            match p.current() {
                INT_NUMBER | STRING | TRUE_KW | FALSE_KW => {
                    let lit = p.start();
                    p.bump_any();
                    lit.complete(p, LITERAL);
                }
                IDENT => {
                    let path = p.start();
                    name_ref(p);
                    path.complete(p, PATH_EXPR);
                }
                _ => {
                    p.error("expected a name, literal, or `{ ... }` block after `const`");
                }
            }
            m.complete(p, CONST_ARG);
        }
        _ => p.error("expected an array length (a literal or a const parameter name)"),
    }
}

/// `struct { name: Type, name: Type, ... }` — a record type. Dispatched on the
/// leading `struct` keyword; the `{` is expected (not guaranteed) afterwards.
fn record_type(p: &mut Parser<'_>) -> CompletedMarker {
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
    m.complete(p, RECORD_TYPE)
}

/// A record-type field: `name: Type`. The name is a declaration, so it is
/// wrapped in `NAME` (like `param`/`pattern`), not `NAME_REF`. `pub`
/// superset-parses (reserved: field visibility isn't supported yet).
fn record_type_field(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(PUB_KW);
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
