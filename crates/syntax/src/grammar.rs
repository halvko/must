//! The Must grammar, written against `Parser`. Resilience rules:
//! the parser never bails; unexpected tokens end up wrapped in `ERROR` nodes;
//! sub-parsers refuse to consume tokens their caller can recover on.

use crate::BRACE_RULE;
use crate::SyntaxKind;
use crate::SyntaxKind::*;
use crate::parser::{CompletedMarker, Parser};

/// Tokens an expression parser must not consume on error: the enclosing
/// construct knows what to do with them.
///
/// `UNSAFE_KW` is deliberately absent, unlike the other item-keyword
/// recovery sets in this file: `unsafe` also opens an expression
/// (`unsafe { ... }`, and — superset-parsed — `unsafe fn(...) { ... }`), so
/// a caller here (a `fn` body, an `if`/`unsafe` branch) must still be able
/// to take it as one via the `expr(p)` fallback rather than bail with
/// "expected `{`".
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
            | EXTERN_KW
            | LET_KW
            | ELSE_KW
            | WITH_KW
            | ONLY_KW
            | IMPL_KW
            | FOR_KW
    )
}

/// THE BRACE RULE, asked of the TOKEN: a value that already ended in `}`
/// closes itself, so the separator that would have ended it is not demanded.
/// `item`, `member` and `expr_stmt_or_tail` ask this one question at three
/// levels, so every brace-ended form — `fn () { }`, `struct { a = 1 }`,
/// `enum { A }`, `if`, `match`, `loop`, `unsafe { ... }`, a bare block —
/// self-terminates, and any added later does so with no edit here.
/// `match_arm` is the fourth site and spells it itself: an arm's separator
/// is `,`, and an arm body ends in a real `}` or not at all, so it asks
/// `R_BRACE` alone rather than this function's `SEMICOLON` half.
///
/// `SEMICOLON` as the previous token can only come from broken nesting (an
/// unclosed block that swallowed one). Demanding another `;` against it is
/// pure noise, so it answers yes too. At the statement level that half is
/// INERT and kept for parity: `builder::error` already drops an after-prev
/// report whose anchor token carries an error, and an unclosed block has
/// reported "expected `}`" against that `;` first — so the answer is one
/// rule, asked the same way, at all three levels rather than two spellings
/// of nearly the same rule.
fn value_closed_itself(p: &Parser<'_>) -> bool {
    matches!(p.prev(), Some(R_BRACE | SEMICOLON))
}

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at(EOF) {
        match p.current() {
            // `unsafe` opens an item too — it is the VOUCH marker of a host
            // import (`unsafe extern static ...`), and nothing else at file
            // scope starts with it.
            STATIC_KW | CONST_KW | TYPE_KW | TRAIT_KW | EXTERN_KW | UNSAFE_KW => item(p),
            _ => p.err_and_bump(
                "expected an item (`static`, `const`, `type`, `trait`, `extern` or `unsafe`)",
            ),
        }
    }
    m.complete(p, SOURCE_FILE);
}

fn item(p: &mut Parser<'_>) {
    let m = p.start();
    // `unsafe extern static read: unsafe fn(...) -> T;` — a HOST IMPORT.
    // TWO markers lead the whole item (it is the item that is declared, not
    // a value that is written), because a boundary owes two obligations:
    // `unsafe` is the VOUCH — the human asserts that the signature written
    // here is the one the host really provides, which nothing on this side
    // can check — and `extern` says the name comes from outside. Both ride
    // the item's own token slot: superset here on every item keyword AND in
    // either order, validation rejects every arrangement but
    // `unsafe extern static`.
    let leading_unsafe = p.eat(UNSAFE_KW);
    let is_extern = p.eat(EXTERN_KW);
    // `extern unsafe static` — the markers written the other way round.
    // Superset-parsed into the same item (it MEANS the vouched import, and
    // refusing an order must not reinterpret what was written); validation
    // moves it back with a fix.
    let mut is_unsafe = leading_unsafe || (is_extern && p.eat(UNSAFE_KW));
    // `unsafe extern unsafe static x: T;` — the vouch written twice. A typo,
    // not a second obligation: eaten here (as a further `UNSAFE_KW` child of
    // this same item) so the declaration still reads as the one thing it
    // is, rather than stopping at "expected `static` after `extern`" — an
    // untrue sentence, since `static` is right there. Validation reports
    // the repeat with its own message.
    if is_extern {
        while p.eat(UNSAFE_KW) {
            is_unsafe = true;
        }
    }
    // `type Foo = expr;` shares the whole item shape with `static`/`const`
    // (superset parsing: a `: Type` annotation on a `type` item parses too;
    // validation rejects it with a removal fix). Only the node kind — and
    // for `trait` items the RHS grammar — differs.
    let kind = match p.current() {
        TYPE_KW => TYPE_ITEM,
        TRAIT_KW => TRAIT_ITEM,
        _ => STATIC_ITEM,
    };
    if !(is_extern || is_unsafe) || matches!(p.current(), STATIC_KW | CONST_KW | TYPE_KW | TRAIT_KW)
    {
        p.bump_any(); // STATIC_KW | CONST_KW | TYPE_KW | TRAIT_KW
    } else {
        // A marker with no item keyword after it — `extern fn g(...)`, the
        // C spelling, above all, or a lone `unsafe` with nothing importable
        // after it. Nothing that follows can be the item this marker leads,
        // so the declaration is unreadable as a whole: say the one thing
        // that is wrong and take the rest of it as ERROR, in this item.
        // Reading on instead cost one message PER TOKEN, all of them
        // consequences of this one.
        p.error(if is_extern {
            "expected `static` after `extern`: an import declares one name with one type"
        } else {
            "expected `extern static` after `unsafe`: the marker vouches for a host \
             import's declared signature, and only an import declares one"
        });
        let e = p.start();
        while !p.at(EOF) && !p.at(SEMICOLON) && !at_item_recovery(p) {
            p.bump_any();
        }
        e.complete(p, ERROR);
        p.eat(SEMICOLON);
        m.complete(p, kind);
        return;
    }
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
        trailing_clauses(p);
        if value_closed_itself(p) {
            p.eat(SEMICOLON);
        } else {
            p.expect_after_prev(SEMICOLON);
        }
    } else if is_extern {
        // THE DECLARATION IS THE WHOLE CONTRACT: an import sets nothing to
        // anything — it promises that a name of this type exists, and the
        // linker (or the host, or the environment) is what provides it. So
        // there is no `=` to demand, and the `;` closes the item. The
        // trailing clauses still parse here: an import is the fifth item
        // head, and a clause written on one earns the same one-sentence
        // refusal the other four give instead of a parse cascade.
        trailing_clauses(p);
        p.expect_after_prev(SEMICOLON);
    } else {
        p.error("expected `=` followed by the item's value");
        p.eat(SEMICOLON);
    }
    m.complete(p, kind);
}

/// The clauses that trail an item's head: attachment `with`-chains
/// (`type X = struct { ... } with { elements } with { ... };`, TR01) and
/// the capability CEILING `only move` (G21), in either order
/// (`} only move with { ... }` and `} with { ... } only move` both parse)
/// — one loop, so the pair reads as a pair and no order is privileged by
/// the grammar.
///
/// Superset on every item head, `extern` included: validation rejects a
/// clause wherever it is not at home, in that head's own words. An import
/// is why this is a function and not a loop written once — it has no
/// `= rhs` for a clause to trail, so it needs the same call from its own
/// arm.
fn trailing_clauses(p: &mut Parser<'_>) {
    while p.at(WITH_KW) || p.at(ONLY_KW) || p.at_word("without") {
        if p.at(WITH_KW) {
            with_group(p);
        } else if p.at(ONLY_KW) {
            only_clause(p);
        } else {
            retired_without_clause(p);
        }
    }
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
///
/// `unsafe` joins the set only through the two-token shape that can mean
/// nothing else — `unsafe extern`, a host import's lead. A BARE `unsafe`
/// is not in this set: inside a `with`-group element block it is the
/// `unsafe impl`/`unsafe for` modifier ([`element`]), a construct this
/// function's own callers must still be allowed to read.
fn at_item_recovery(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        STATIC_KW | TYPE_KW | TRAIT_KW | LET_KW | CONST_KW | EXTERN_KW
    ) || (p.at(UNSAFE_KW) && p.nth(1) == EXTERN_KW)
}

/// The member-loop recovery set: like [`at_item_recovery`] minus the
/// member-shaped keyword openers — `type Item ...;` and `const N: usize;`
/// are (reserved) members, so those prefixes stay inside the body.
fn at_member_recovery(p: &Parser<'_>) -> bool {
    matches!(p.current(), STATIC_KW | TRAIT_KW | LET_KW | EXTERN_KW)
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
/// wrapped in `FN_TYPE` so it sits in the tree as the annotation it is.
///
/// What is left distinguishing it from [`anon_fn_type`], now that a plain
/// fn type takes `name: Type` parameters too, is the parameter GRAMMAR: a
/// requirement is written as the signature it is, every parameter named
/// ([`param_list`], so hir can refuse a half-written one), while a fn
/// type's names are optional documentation. Routed through the fn-type
/// list instead, `m: fn(x, y: usize) -> R;` would stop being a
/// half-written signature and silently become a parameter of type `x` —
/// so the two productions stay apart until the requirement form is ruled
/// on its own.
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
    if value_closed_itself(p) {
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
        // `x.&` / `x.&mut` (DOT AMP without `raw`). Those still ACCEPT the
        // borrow operator's turbofish (`x.&mut::<@a>`) even though a
        // region there is refused: parsing the superset is what lets
        // validation name the argument and offer to delete it, rather than
        // the parser inventing a recovery. Lexed DOT AMP [raw] [mut];
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
                // `s.flat_map::<usize>(f)` — the MEMBER's own turbofish, in
                // the one position a dot-call can spell it. Parsed into the
                // same `MEMBER_GENERIC_ARGS` node the qualified spelling
                // uses (`Option::flat_map::<usize>`), so the two forms are
                // one thing to everything downstream; whether the name
                // turns out to be a member at all is inference's question.
                member_generic_args(p);
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
    // `const` only starts an expression as a fn literal's modifier (see
    // `at_fn_literal`) or immediately followed by `{` (a const block). Any
    // other `const` is left for the caller to recover on (see
    // `at_expr_recovery` and the CONST_KW arm in `block_expr`'s statement
    // loop) — most commonly a misplaced item.
    if p.at(CONST_KW) {
        if at_fn_literal(p) {
            return Some(fn_literal(p));
        }
        if p.nth(1) == L_BRACE {
            return Some(const_block_expr(p));
        }
    }
    // `extern` starts an expression only as `extern fn` — the RETIRED
    // host-import initializer, superset-parsed so validation can rewrite it
    // (`extern static name: unsafe fn(...)` is the live spelling). A bare
    // `extern` opens an ITEM, so it falls through to the catch-all here.
    if p.at(EXTERN_KW) && at_fn_literal(p) {
        return Some(fn_literal(p));
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
    // postfix `x.&raw` / `x.&raw mut`. Checked first so `raw` claims this
    // shape ahead of the general prefix-borrow arm below.
    if p.at(AMP) && p.nth(1) == RAW_KW {
        return Some(addr_of_expr(p));
    }
    // The RETIRED prefix safe borrow `&x` / `&mut x`. Safe borrows are now
    // spelled postfix (`x.&` / `x.&mut`); the prefix form superset-parses
    // into the same `BORROW_EXPR` node the postfix form produces (never a
    // silent reinterpretation) so downstream stays coherent — a migration
    // diagnostic (with a rewrite-to-postfix quick fix) is validation's job,
    // keyed off the missing leading `DOT` (see `validation.rs`).
    if p.at(AMP) {
        return Some(prefix_borrow_expr(p));
    }
    // `-x` — unary minus on numbers. Same operand tier as the retired
    // prefix borrows above: a primary expression plus its postfix chain, so
    // `-a.b` negates the field and `-x + y` stays a sum of the negation.
    if p.at(MINUS) {
        let m = p.start();
        p.bump(MINUS);
        expr_bp(p, 7);
        return Some(m.complete(p, NEG_EXPR));
    }
    let m = match p.current() {
        INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
            let m = p.start();
            p.bump_any();
            m.complete(p, LITERAL)
        }
        // `::Variant` / `::Variant(args)` — the elided sigil in EXPRESSION
        // position, the mirror of the pattern spelling: a variant of the
        // *expected* type's enum, enum segment dropped. Same node shape as
        // `VARIANT_PAT`'s sigil arm (one `NameRef`, `COLON2` before it),
        // and the payload form is left to the ordinary postfix loop — so
        // `::Some(v)` is a `CALL_EXPR` over this node exactly as
        // `Option::Some(v)` is one over a `PATH_EXPR`.
        COLON2 => {
            let m = p.start();
            p.bump(COLON2);
            if p.at(IDENT) {
                name_ref(p);
            } else {
                p.error("expected a variant name after `::`");
            }
            m.complete(p, ELIDED_VARIANT_EXPR)
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
                    trailing_qualified_segment(p);
                } else if p.at(IDENT) {
                    name_ref(p);
                    member_generic_args(p);
                } else {
                    p.error("expected a variant name after `::`");
                }
            } else if bare_angle_generic_args_expr(p) {
                // `f<T>(...)` / `f<T>::assoc` — the same bare-angle typo
                // type position takes, parsed into the same shape and
                // corrected the same way; only the gate differs, because
                // `<` is a real operator here and some spellings of a
                // comparison are indistinguishable from a call.
                generic_arg_list(p);
                trailing_qualified_segment(p);
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

/// Whether the parser is at a `fn` literal, counting the `const`/`extern`
/// modifier prefix in either order. A prefix is claimed as soon as it is
/// unambiguous, before the `fn` arrives: half-written `const extern` is the
/// literal the user is typing, not a misplaced item, and `fn_literal`
/// reports the missing keyword itself.
fn at_fn_literal(p: &Parser<'_>) -> bool {
    match p.current() {
        FN_KW => true,
        CONST_KW => matches!(p.nth(1), FN_KW | EXTERN_KW),
        EXTERN_KW => matches!(p.nth(1), FN_KW | CONST_KW),
        _ => false,
    }
}

fn fn_literal(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.eat(CONST_KW); // optional `const` marker; the caller has checked `at_fn_literal`
    // `extern` rides the same modifier slot `const` does — one `FN_LITERAL`
    // node with one more token child, not a wrapper — so every consumer that
    // casts an item's body to `ast::FnLiteral` keeps working and only has a
    // new fact to ask about. Both orders parse (validation rejects the
    // combination itself, so `const extern fn` never depends on which one the
    // user wrote first).
    let is_extern = p.eat(EXTERN_KW);
    if is_extern {
        p.eat(CONST_KW);
    }
    // Not `bump`: `at_fn_literal` claims a modifier prefix before the `fn` is
    // typed, so `const extern` with nothing after it reaches here and gets a
    // diagnostic rather than an assertion.
    p.expect(FN_KW, "`fn`");
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
        // Superset for the `extern` case: an `extern fn` has no body, but a
        // written one parses into its real tree shape so validation can
        // reject it where the user wrote it.
        block_expr(p);
    } else if is_extern {
        // The declaration ends here — `item` takes the `;`.
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
            // `unsafe` joins bare — no pattern ever starts with it, so
            // seeing one here always means recovery, whether it leads a
            // host import or stands alone.
            if matches!(
                p.current(),
                STATIC_KW | TYPE_KW | TRAIT_KW | EXTERN_KW | UNSAFE_KW
            ) || (p.at(CONST_KW) && !matches!(p.nth(1), FN_KW | L_BRACE))
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
/// calls). Plus a LITERAL pattern (`'(' =>`, `0 =>`), and the *reserved* `..`
/// (parses, validation rejects it). A bare name with no `::` and no parens
/// is *always* a binding now — never reinterpreted type-directed as a
/// variant (see `infer.rs`'s `check_match_pat` `PatData::Bind` arm). No
/// nesting, or-patterns or guards yet.
fn match_pattern(p: &mut Parser<'_>) {
    match p.current() {
        // A literal pattern. EVERY literal kind parses here, not just the
        // two scalars that have semantics yet: `match s { "a" => ... }` is
        // a thing people write, and the superset parse lets `validation`
        // hand back "string literal patterns are not supported yet"
        // instead of the parser's blank "expected a pattern". A NEGATIVE
        // literal is deliberately NOT in the superset: `-1` is an operator
        // applied to a literal, and taking it here would be the first step
        // of a pattern *expression* grammar — reserved with the rest of
        // the pattern language.
        INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
            let m = p.start();
            let lit = p.start();
            p.bump_any();
            lit.complete(p, LITERAL);
            m.complete(p, LITERAL_PAT);
        }
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

/// The RETIRED prefix safe borrow `&x` / `&mut x`. The caller has already
/// confirmed `p.at(AMP)` (and that `&raw ...` was ruled out). Safe borrows
/// are now spelled postfix (`x.&` / `x.&mut`); the prefix form
/// superset-parses into the same `BORROW_EXPR` node the postfix form
/// produces — never a silent reinterpretation — so downstream (hir, the
/// checker) sees exactly the node it already knows how to handle. Unlike
/// its postfix dual, the prefix spelling never carried a region turbofish,
/// so none is attempted here. The migration diagnostic itself is
/// validation's job (see `validation.rs`), keyed off the missing leading
/// `DOT` this node never gets. The operand parses at the same binding power
/// as `&raw`'s: a primary expression plus its postfix chain.
fn prefix_borrow_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(AMP);
    p.eat(MUT_KW);
    expr_bp(p, 7);
    m.complete(p, BORROW_EXPR)
}

/// `unsafe { ... }` — an expression-position block that marks a checker
/// region (deref of a raw pointer is legal inside). Same shape as
/// `const { ... }`. An `unsafe fn` LITERAL superset-parses (the literal
/// becomes the node's child) so validation can reject that spelling with
/// an honest "not supported yet" — the `unsafe fn(...)` TYPE is live and
/// parses in `type_ref`; any other non-block body gets the wrap-in-braces
/// treatment, like `if` branches.
fn unsafe_block_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(UNSAFE_KW);
    if p.at(L_BRACE) {
        block_expr(p);
    } else if at_fn_literal(p) {
        // Reserved: an `unsafe fn` LITERAL parses whole, validation
        // rejects it.
        fn_literal(p);
    } else if at_expr_recovery(p) {
        p.error("expected `{`: `unsafe` blocks are blocks");
    } else {
        expr(p);
    }
    m.complete(p, UNSAFE_BLOCK_EXPR)
}

/// Whether the current token can start an expression — the dispatch set of
/// `primary_expr`, including its lookahead `const`/`extern`/`struct`/`enum`
/// cases. Used where an expression is *optional* (a `break` value).
fn at_expr_start(p: &Parser<'_>) -> bool {
    match p.current() {
        INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW | IDENT | L_PAREN | L_BRACE | L_BRACKET
        | FN_KW | IF_KW | MATCH_KW | LOOP_KW | BREAK_KW | CONTINUE_KW | RETURN_KW | UNSAFE_KW
        // `::Variant` — the elided sigil starts an expression too, so
        // `return ::None;` and `break ::None;` carry their value instead of
        // stopping at the keyword and leaving the sigil stranded.
        | COLON2
        | MINUS => true,
        CONST_KW => at_fn_literal(p) || p.nth(1) == L_BRACE,
        EXTERN_KW => at_fn_literal(p),
        STRUCT_KW | ENUM_KW => at_type_literal_body(p),
        // `&raw ...` and the retired prefix borrows `&x` / `&mut x` both
        // start an expression now — see `primary_expr`'s AMP arms.
        AMP => true,
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
        if at_param_capability_clause(p) {
            param_capability_clause(p, REGION_PARAM);
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
        if at_param_capability_clause(p) {
            param_capability_clause(p, CONST_PARAM);
        }
        m.complete(p, CONST_PARAM);
    } else if matches!(p.current(), IDENT | HOLE) {
        let m = p.start();
        pattern(p, "expected a type parameter name");
        // `T: Display + Debug` — bounds live where params are born (TR05:
        // binder-position bounds, `+` composition). A CAPABILITY the body
        // needs rides this same slot (`T: forget`, T22): it is a positive
        // requirement on the argument like any other, so it is written
        // where requirements are written, and the grammar needs no clause
        // of its own for it.
        if p.eat(COLON) {
            type_(p);
            while p.eat(PLUS) {
                type_(p);
            }
        }
        // A capability clause in the parameter home, in EITHER spelling,
        // refused in the words of the ruling that left it no home (T22)
        // rather than as two "expected `,`" errors at tokens that are each
        // individually fine. Consumes the whole clause, so the binder list
        // after it still parses — recovery for a retired spelling is
        // diagnostics-layer work (G26).
        //
        // Superset, as the retired grammar itself was: the REVERSE order
        // (`T without forget: Bound`) parsed into the same node, so the
        // bounds slot is offered again after the clause. Without that the
        // trailing `: Bound` strands and the one refusal grows two
        // "expected `,`" errors behind it — the cascade this arm exists
        // to prevent.
        if at_param_capability_clause(p) {
            param_capability_clause(p, TYPE_PARAM);
            if p.eat(COLON) {
                type_(p);
                while p.eat(PLUS) {
                    type_(p);
                }
            }
        }
        m.complete(p, TYPE_PARAM);
    } else if !p.at(COMMA) && !p.at(R_ANGLE) {
        p.err_and_bump("expected a generic parameter");
    } else {
        p.error("expected a generic parameter");
    }
}

/// Whether a capability clause is starting here, in either spelling: the
/// retired `without` (an ordinary identifier now — see [`Parser::at_word`])
/// or the live `only`, which is a keyword but is a DECLARATION's word.
fn at_param_capability_clause(p: &Parser<'_>) -> bool {
    p.at(ONLY_KW) || p.at_word("without")
}

/// `T without forget` / `T only move` — a capability clause where no
/// capability clause belongs, recognized only to say where the two
/// spellings' meanings went. All three parameter KINDS reach it: the
/// retired clause was superset-parsed on region and const params too, and
/// consuming it there keeps their refusal one sentence instead of two
/// "expected `,`" errors at tokens that are each individually fine (G26:
/// recovery for a retired spelling is diagnostics-layer work).
///
/// Both spellings are recognized because a reader who learns the new one
/// first will write it in the old one's home, and a ceiling written on a
/// parameter is exactly as wrong as the opt-out it replaced — for a
/// different reason, which is why the two leads differ. G21 promises the
/// recognition and one diagnostic, not a rewriting fix: in this corpus a
/// retired spelling's fix is attached in VALIDATION over a castable node,
/// so offering one here would mean superset-parsing the clause into a real
/// node and deciding what it MEANS in hir — more than the row asks for.
///
/// What the three parameter kinds are TOLD differs, because "why not"
/// does. Only a TYPE parameter ever stood for something that could hold a
/// capability, so it is the only one offered a migration; a region names a
/// duration and a const's values are plain data, and each is told that
/// instead of advice it cannot take.
///
/// The type parameter's sentence is position-neutral in the other
/// direction: that arm is every binder's — a fn literal's, a fn type's, a
/// `type` declaration's — so it leads with the instruction that is right in
/// all of them. It leads with the DELETION, not with `T: forget`: the
/// opt-out said "this may be a linear", which is what a bare `T` now says,
/// while the bound says the opposite ("this may be discarded"). Naming the
/// bound first would steer a migration into reversing its own meaning.
fn param_capability_clause(p: &mut Parser<'_>, kind: SyntaxKind) {
    let m = p.start();
    let message = match kind {
        // Neither of these says "retired": both spellings reach them now,
        // and neither kind ever had a capability for either one to talk
        // about.
        REGION_PARAM => "a capability clause has no place on a region parameter: a region names \
             a duration, not a value, so it has no capability to speak of"
            .to_owned(),
        CONST_PARAM => "a capability clause has no place on a const parameter: a const \
             parameter's values are always plain data"
            .to_owned(),
        _ => {
            let lead = if p.at(ONLY_KW) {
                "a capability ceiling is a `type` declaration's, not a parameter's"
            } else {
                "a capability clause on a parameter is retired: `T without forget` is now \
                 spelled `T`"
            };
            format!(
                "{lead} — an unbounded parameter is already checked as one that may have to be \
                 consumed; write `T: forget` only where the body DISCARDS a `T`, and a `type` \
                 declaration's parameters carry no capability bounds at all"
            )
        }
    };
    p.error(message);
    p.bump_any();
    if p.at(IDENT) {
        p.bump(IDENT);
        while p.eat(PLUS) && p.at(IDENT) {
            p.bump(IDENT);
        }
    }
    m.complete(p, ERROR);
}

/// `} without forget;` — the retired capability clause at a declaration,
/// recognized ONLY here (the trailing slot it used to live in) and only to
/// say what replaced it. `without` is not a keyword any more: it lexes as
/// an ordinary identifier, and this is one of the two places the parser
/// looks at a WORD rather than a kind.
///
/// It consumes the whole clause it recognizes — the head and the `+`-joined
/// capability names — so the refusal is ONE sentence instead of the
/// three-error desync a stranded identifier produced (the item ends at the
/// word, then the file-level loop reports `expected an item` and bumps,
/// twice). That is diagnostics-layer work, which is exactly where G26 puts
/// recovery for a retired spelling — and it costs the language nothing,
/// because it reserves no word and deletes cleanly the day nobody has the
/// old form left.
///
/// No rewriting FIX rides it, and G21 is the authority for that: it
/// promises the site is recognized and consumed, one diagnostic, and the
/// item after it still parses. Every rewriting fix in this corpus is
/// attached in `validation` over a castable node, never by the parser —
/// offering one here would mean superset-parsing `without forget` into a
/// real node and then deciding whether it still MEANS `only move` in hir,
/// which is more than the ruling asks for.
fn retired_without_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.error(
        "the `without` clause is retired: write the ceiling instead \
         (`only move` where you wrote `without forget`)",
    );
    p.bump(IDENT);
    if p.at(IDENT) {
        p.bump(IDENT);
        while p.eat(PLUS) && p.at(IDENT) {
            p.bump(IDENT);
        }
    }
    m.complete(p, ERROR);
}

/// `only move` (or `only move + send`) — the capability CEILING of a `type`
/// declaration: the MOST that can be done with a value of it. ONE
/// production, ONE home — a ceiling is a fact about a declared type, and
/// nothing else in the language declares one; a generic parameter takes no
/// clause at all (T22). The capability names are ordinary [`NAME_REF`]s — a
/// capability is a thing the language knows about by name, not a keyword
/// each — and they compose with `+`, exactly as bounds do (TR05), because
/// the list reads as a conjunction too: "the most you can do is move it,
/// and send it".
fn only_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(ONLY_KW);
    if p.at(IDENT) {
        name_ref(p);
        while p.eat(PLUS) {
            if p.at(IDENT) {
                name_ref(p);
            } else {
                p.error("expected a capability name after `+`");
                break;
            }
        }
    } else {
        p.error("expected a capability name after `only` (`only move`)");
    }
    m.complete(p, ONLY_CLAUSE);
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
/// confirmed `p.at(L_ANGLE)`; the `::` in front is the caller's business,
/// and its ABSENCE is exactly what marks the result as the bare-angle typo
/// for `validation` to correct.
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
/// Nothing is diagnosed here. A member's own TYPE parameters are legal:
/// `size = fn::<T>(m: Self, t: T) -> usize` declares one, and the qualified
/// spelling `Measured::size::<usize>` applies it through this very list.
/// Member-own CONST parameters stay reserved, and are refused at the
/// DECLARATION (`syntax::validation`) rather than here, so this list needs
/// no judgement of its own. `Shape::Circle::<usize>` parses the same shape
/// and gets hir's variant-flavored correction: the two readings differ by
/// what the segment NAMES, which is not a question the parser can answer.
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

/// The segment that may follow a path's OWNER turbofish
/// (`Option::<usize>::Some` — a variant of a generic enum; `Pair::<T>::first`
/// — a member of a generic owner), with its own possible turbofish. A no-op
/// when no `::` follows.
fn trailing_qualified_segment(p: &mut Parser<'_>) {
    if !p.at(COLON2) {
        return;
    }
    p.bump(COLON2);
    if p.at(IDENT) {
        name_ref(p);
        member_generic_args(p);
    } else {
        p.error("expected a variant name after `::`");
    }
}

/// Whether the `<` the caller is sitting on (right after a bare name in
/// expression position) is the bare-angle typo for `::<` rather than a
/// comparison — a pure lookahead, consuming nothing, so the caller parses
/// the group with [`generic_arg_list`] exactly as the correctly-spelled
/// path does (X03: the never-legal-but-recognizable form gets its real tree
/// shape and a later correction, not a bespoke recovery node).
///
/// Type position needs no gate at all; expression position does, because
/// `<` is a real operator here and `a < b` closes just as cleanly as
/// `f<T>`. So this fires only on an angle-balanced group followed by `::`
/// (a further qualified segment — nothing valid puts `::` after a
/// comparison) or by `(` (a generic call), and the call shape additionally
/// requires the group to hold no top-level comma.
///
/// That comma is the whole difference between a misread and a broken
/// program: `g(a < b, c > (d))` — two comparisons as call arguments,
/// legal and well-typed — is otherwise token-for-token a one-argument call
/// on `a::<b, c>`. Refusing it costs only the correction on a
/// MULTI-argument bare-angle call (`f<A, B>(x)`, left to read as
/// comparisons, as at any earlier commit). What remains ambiguous is
/// `a < b > (d)`, deliberately read as the call: `a < b` is a `bool` and
/// `>` wants numbers, so no well-typed program is spelled that way, while
/// the call is a typo people really make.
fn bare_angle_generic_args_expr(p: &Parser<'_>) -> bool {
    let Some(group) = scan_bare_angle_group(p) else {
        return false;
    };
    match p.nth(group.len) {
        COLON2 => true,
        L_PAREN => !group.top_level_comma,
        _ => false,
    }
}

/// A balanced `<...>` group found by [`scan_bare_angle_group`].
struct BareAngleGroup {
    /// Token count from the opening `<` through its match: `p.nth(len)` is
    /// the first token past the group.
    len: usize,
    /// Whether a `,` separates arguments at the group's own level.
    top_level_comma: bool,
}

/// Bounded, token-only lookahead from an opening `<`: the group running
/// from that `<` through its match, or `None` if no match is in reach.
///
/// Everything except the scan bound and `EOF` is judged at nest 0 — outside
/// any paren, bracket or brace pair opened after the `<`. Angles included:
/// a `>` deeper in closes nothing here (`a < f(b > (c))` is a comparison
/// around a call whose argument is another one, not a group), while a real
/// argument's own angles balance within it, so ignoring them costs nothing.
///
/// Parens and brackets nest legitimately inside an argument (`Vec<[T; 4]>`,
/// `Vec<fn(T) -> U>`); braces do not on their own — an ordinary block would
/// swallow the rest of a function body hunting for a `>` — so only the two
/// legal shapes, `const { ... }` and `struct { ... }`, are let through, by
/// peeking back at the keyword that must introduce them.
///
/// Bails on: the scan bound (a `<` this far from any `>` was never a
/// turbofish); `EOF`; a brace that is not one of those two shapes; or, at
/// nest 0, a comparison operator, a `=>`, a `;`, or a statement/item
/// keyword — none of which can appear at an argument list's own level,
/// though all are ordinary content deeper in.
fn scan_bare_angle_group(p: &Parser<'_>) -> Option<BareAngleGroup> {
    const SCAN_BOUND: usize = 128;
    if !p.at(L_ANGLE) {
        return None;
    }
    let mut angle = 0i32;
    let mut nest = 0i32;
    let mut top_level_comma = false;
    for k in 0..SCAN_BOUND {
        match p.nth(k) {
            EOF => return None,
            L_ANGLE if nest == 0 => angle += 1,
            R_ANGLE if nest == 0 => {
                angle -= 1;
                if angle == 0 {
                    return Some(BareAngleGroup {
                        len: k + 1,
                        top_level_comma,
                    });
                }
            }
            L_PAREN | L_BRACKET => nest += 1,
            R_PAREN | R_BRACKET => {
                nest -= 1;
                if nest < 0 {
                    return None;
                }
            }
            L_BRACE if k > 0 && matches!(p.nth(k - 1), CONST_KW | STRUCT_KW) => nest += 1,
            L_BRACE => return None,
            R_BRACE => {
                nest -= 1;
                if nest < 0 {
                    return None;
                }
            }
            COMMA if angle == 1 && nest == 0 => top_level_comma = true,
            // `T.&::<@a>`, `@a + @b`, `Self = T`, `fn(T) -> U` are all legal at
            // an argument list's own level; the equality/ordering operators and
            // a match arm's `=>` are not, so finding one there means this was
            // a chain of comparisons. Inside a `const { a == b }` argument
            // they are ordinary content, which is why this arm — like the
            // angle counters and every bail but the brace — reads `nest`.
            EQ2 | NEQ | LTEQ | GTEQ | FAT_ARROW if nest == 0 => return None,
            SEMICOLON if nest == 0 => return None,
            STATIC_KW | TRAIT_KW | TYPE_KW | EXTERN_KW | LET_KW | WITH_KW | ONLY_KW | IMPL_KW
            | FOR_KW
                if nest == 0 =>
            {
                return None;
            }
            _ => {}
        }
    }
    None
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
        INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
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
                INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
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
            INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW | MINUS
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
            // here when the lookahead rules those out. `type`
            // and `trait` never start an expression, so they always mean
            // an item.
            STATIC_KW | TYPE_KW | TRAIT_KW => break,
            // A bare `extern` opens an ITEM (`extern static ...`); only the
            // RETIRED `extern fn` initializer is an expression here.
            EXTERN_KW if !at_fn_literal(p) => break,
            CONST_KW if !at_fn_literal(p) && p.nth(1) != L_BRACE => break,
            // `unsafe extern` opens an ITEM (the vouch marker leading a host
            // import); every other `unsafe` here is an expression —
            // `unsafe { ... }`, or (superset) an `unsafe fn` literal — so
            // only THIS two-token shape bails.
            UNSAFE_KW if p.nth(1) == EXTERN_KW => break,
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
        // The brace rule, one level further in than `item`'s and `member`'s:
        // a statement whose expression ends in `}` closes itself. The `;`
        // stays LEGAL (it is eaten above) — this is a carve-out for the
        // shapes that already end in a brace, not a general
        // optional-semicolon rule, and every other expression still owes
        // its own.
        //
        // What keeps the carve-out this simple is that the expression
        // grammar above is GREEDY and stays greedy — the inverse of Rust's
        // fiat, which declares a statement-position block-tail finished and
        // re-reads a following `-` as the next statement's unary minus. Here
        // `if c { } - 1` is a SUBTRACTION: `-` continues an expression, so
        // `expr_bp` has already taken it before this line runs, `p.prev()`
        // is `1` rather than `}`, and the `;` is owed as usual. The price is
        // the mirror image, and was accepted: a statement that genuinely
        // STARTS with a continuation-shaped token (`-x`, `(f)(x)`, `[a][0]`)
        // right after a block-tail statement needs an explicit `;` between
        // the two to force the split. Continuing across a LINE BREAK is
        // warned about — not refused — one layer out, where trivia is
        // visible (`hir::file_diagnostics`).
        if !value_closed_itself(p) {
            p.error_after_prev(SEMICOLON);
        }
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

/// The borrow operator's OWN turbofish. In a TYPE position (`T.&::<@a>`)
/// this is where regions are written, and a signature still requires it;
/// after an EXPRESSION (`x.&mut::<@a>`) the same shape parses here and is
/// then refused by validation. A no-op unless the two-token
/// `COLON2 L_ANGLE` lookahead is there (the same unambiguous gate every
/// other turbofish uses); the list is an ordinary
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

/// A fn type — `fn(usize) -> usize`, or `unsafe fn(usize) -> usize`, which
/// is a DIFFERENT type (a value of it may only be called inside
/// `unsafe { ... }`). The `unsafe` token rides the same slot it does on a
/// fn literal, so both spellings produce one `FN_TYPE` node and the flag is
/// read off the token.
///
/// Parameters may be NAMED (`unsafe fn(buf: u8.&raw mut, len: usize) ->
/// isize`), which is how a host import's declaration spells its contract —
/// a signature a reader must be able to read. The names are documentation:
/// no call passes arguments by name, and hir keeps only the types.
fn anon_fn_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.eat(UNSAFE_KW);
    p.bump(FN_KW);
    // A generic binder on a fn TYPE: superset (the colon-declared member
    // signature is the one place it means something — see
    // [`member_decl_fn_signature`]), refused by validation everywhere else
    // with the reason, rather than by a bare "expected `(`".
    if p.at(COLON2) && p.nth(1) == L_ANGLE {
        generic_param_list(p);
    }
    if p.at(L_PAREN) {
        fn_type_param_list(p);
    } else {
        p.error("expected `(`: function types are written `fn(...) -> ...`");
    }
    if p.at(THIN_ARROW) {
        ret_type(p);
    }
    m.complete(p, FN_TYPE)
}

/// A fn TYPE's parameter list. Decided PER PARAMETER, not per list: a named
/// parameter and a bare one may sit side by side, and neither position
/// changes what the other means (`fn(usize, y: usize)` and
/// `fn(x: usize, usize)` are the same type, written two ways).
///
/// Distinct from [`param_list`], which parses a fn LITERAL's parameters:
/// those are PATTERNS and they BIND. A fn type's name binds nothing, so the
/// pattern grammar is not admitted here at all — the same cut Rust makes on
/// fn-pointer types (E0561) — and the shapes that only a pattern could mean
/// are refused where they are written instead of being reinterpreted as
/// something else.
fn fn_type_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(L_PAREN);
    while !p.at(R_PAREN) && !p.at(EOF) {
        let before = p.pos();
        fn_type_param(p);
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

/// One parameter of a fn TYPE: `Type`, or `name: Type` where the name is an
/// identifier or `_` (documentation, kept in the tree, dropped below the
/// syntax layer).
fn fn_type_param(p: &mut Parser<'_>) {
    let m = p.start();
    // `mut` is a binding-mode marker and a fn type binds nothing. Consumed
    // (so one written mistake costs the reader one message, not the rest of
    // the list) and named where it stands.
    if p.at(MUT_KW) {
        p.error("a function type's parameters are not patterns: write `name: Type` or `Type`");
        p.bump(MUT_KW);
    }
    if matches!(p.current(), IDENT | HOLE) && p.nth(1) == COLON {
        let nm = p.start();
        p.bump_any();
        nm.complete(p, NAME);
        p.bump(COLON);
    }
    type_(p);
    // A pattern that PARSED as a type and then met its colon
    // (`struct { x }: T`): same refusal, said at the colon that gives it
    // away, and the annotation is consumed so the list stays readable.
    if p.at(COLON) {
        p.error("a function type's parameters are not patterns: write `name: Type` or `Type`");
        p.bump(COLON);
        type_(p);
    }
    m.complete(p, PARAM);
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
        // The RETIRED prefix safe borrow type `&T` / `&mut T`. Safe borrow
        // types are now spelled postfix (`T.&` / `T.&mut`, region turbofish
        // `T.&::<@a>`); the prefix form superset-parses into the same
        // `BORROW_TYPE` node the postfix form produces — never a silent
        // reinterpretation — so hir sees exactly the node it already knows
        // how to handle. The migration diagnostic is validation's job, keyed
        // off the missing leading `DOT`. Legacy `&'a T` is NOT recognized
        // here: the `&` fires its own migration and the freed `'` lexes as
        // an unterminated character literal (G26).
        AMP => {
            let m = p.start();
            p.bump(AMP);
            p.eat(MUT_KW);
            type_(p);
            m.complete(p, BORROW_TYPE)
        }
        // `fn(...) -> ...` and its unsafe-to-call twin, `unsafe fn(...) ->
        // ...`. One arm because they are ONE type constructor differing in
        // one bit: unsafety lives in the function type.
        FN_KW => anon_fn_type(p),
        UNSAFE_KW if p.nth(1) == FN_KW => anon_fn_type(p),
        // `unsafe` in type position with no `fn` after it. The marker
        // belongs to a FUNCTION type and to nothing else, so say that once
        // and parse the real type that follows — a wrong marker must not
        // cost the reader the rest of the item.
        UNSAFE_KW => {
            p.error("`unsafe` marks a FUNCTION type: `unsafe fn(...) -> ...`");
            p.bump(UNSAFE_KW);
            return type_core(p);
        }
        IDENT => {
            let m = p.start();
            name_ref(p);
            // `Shape::Circle` in type position: a variant type. `Pair::<T>`:
            // a generic type mention, spelled the same as on the expression
            // side.
            if p.at(COLON2) {
                p.bump(COLON2);
                if p.at(L_ANGLE) {
                    generic_arg_list(p);
                } else if p.at(IDENT) {
                    name_ref(p);
                } else {
                    p.error("expected a variant name after `::`");
                }
            } else if p.at(L_ANGLE) {
                // `Pair<T>` — the bare-angle typo for `Pair::<T>` (Rust
                // muscle memory), permanently illegal under G06 but
                // perfectly recognizable, so X03 applies: parse it into the
                // real shape it means — the very `GENERIC_ARG_LIST` the
                // turbofish spelling builds — and let `validation` CORRECT
                // the missing `::`. Unambiguous in type position (there is
                // no comparison operator here), and no lookahead is needed
                // to bound an unclosed `Pair<`: `generic_arg_list`'s own
                // no-progress break and `expect_after_prev(R_ANGLE)` end it
                // exactly where `Pair::<` mid-typing ends.
                generic_arg_list(p);
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
        INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
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
                INT_NUMBER | STRING | CHAR | TRUE_KW | FALSE_KW => {
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
