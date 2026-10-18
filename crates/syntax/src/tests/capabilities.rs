//! `only` clauses and capability bounds.

use super::*;

// ---- the capability ceiling (`only move`) -------------------------------

#[test]
fn only_move_trails_a_type_declaration() {
    // The chosen spelling (G21): `only` rides the same trailing slot `with`
    // does, so the pair reads as a pair — `with` attaches, `only` caps.
    check(
        "type S = struct { n: usize } only move;",
        expect![[r#"
            SOURCE_FILE@0..39
              TYPE_ITEM@0..39
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..6
                  IDENT@5..6 "S"
                WHITESPACE@6..7 " "
                EQ@7..8 "="
                WHITESPACE@8..9 " "
                RECORD_EXPR@9..28
                  STRUCT_KW@9..15 "struct"
                  WHITESPACE@15..16 " "
                  L_BRACE@16..17 "{"
                  WHITESPACE@17..18 " "
                  RECORD_EXPR_FIELD@18..26
                    NAME_REF@18..19
                      IDENT@18..19 "n"
                    COLON@19..20 ":"
                    WHITESPACE@20..21 " "
                    PATH_TYPE@21..26
                      NAME_REF@21..26
                        IDENT@21..26 "usize"
                  WHITESPACE@26..27 " "
                  R_BRACE@27..28 "}"
                WHITESPACE@28..29 " "
                ONLY_CLAUSE@29..38
                  ONLY_KW@29..33 "only"
                  WHITESPACE@33..34 " "
                  NAME_REF@34..38
                    IDENT@34..38 "move"
                SEMICOLON@38..39 ";"
        "#]],
    );
}

#[test]
fn the_retired_declaration_clause_is_consumed_whole() {
    // `without` is an ordinary identifier now, so a stranded one would end
    // the item and desync the file-level loop into two more errors. The
    // recognizer takes the head and every `+`-joined name with it: ONE
    // sentence, and the item after it still parses (G21/G26). No rewriting
    // fix rides it — see `retired_without_clause`.
    check_errors(
        "type S = struct { n: usize } without forget + send;\n\
         type T = struct { n: usize } only move;\n",
        expect![[r#"
            29..36: the `without` clause is retired: write the ceiling instead (`only move` where you wrote `without forget`)
        "#]],
    );
}

#[test]
fn only_and_with_compose_in_either_order() {
    // One loop parses both, so neither order is privileged by the grammar.
    let a = crate::parse("type S = struct { n: usize } only move with { impl Self {} };");
    let b = crate::parse("type S = struct { n: usize } with { impl Self {} } only move;");
    assert!(a.errors().is_empty(), "{:?}", a.errors());
    assert!(b.errors().is_empty(), "{:?}", b.errors());
    assert_eq!(a.debug_dump().matches("ONLY_CLAUSE").count(), 1);
    assert_eq!(b.debug_dump().matches("ONLY_CLAUSE").count(), 1);
}

#[test]
fn only_clause_misplacements_and_unknown_capabilities() {
    // Every item head parses the clause and refuses it in its own words —
    // an `extern` item (`extern static`) included, so a ceiling written on one
    // is one sentence rather than a parse cascade.
    check_errors(
        "static x = 5 only move;\n\
         extern static ex: unsafe fn(usize) -> isize only move;\n\
         trait T = requires {} only move;\n\
         type A = struct {} only leak;\n\
         type B = struct {} only send;\n\
         type C = struct {} only forget;\n\
         type D = struct {} only access;\n\
         type E = struct {} only;\n",
        expect![[r#"
            13..17: a capability ceiling belongs on a `type` declaration; an item has whatever ceiling its type has
            24..30: declaring an `extern` item is a VOUCH: write `unsafe extern static`
            68..72: a capability ceiling belongs on a `type` declaration; an item has whatever ceiling its type has
            101..105: a capability ceiling belongs on a `type` declaration, not on a `trait`
            136..140: unknown capability `leak`; `move` is the only ceiling that can be written
            166..170: the `send` capability does not exist yet; `move` is the only ceiling that can be written
            196..202: `forget` is the default ceiling: a type whose values may be dropped on the floor needs no `only` clause
            228..234: the `access` capability does not exist yet; `move` is the only ceiling that can be written
            259..260: expected a capability name after `only` (`only move`)
        "#]],
    );
}

#[test]
fn a_declaration_ceils_one_ladder_once() {
    // A clause ceils only the ladders it NAMES, so the once-only rule is
    // per LADDER: the same rung twice and two rungs of one ladder are the
    // same mistake, wearing different words. Both spellings of a repeat —
    // a second clause and a second `+` term — get the same answer.
    //
    // `F` is the ordering control. A RESERVED rung is answered as reserved
    // wherever it is written: `access` composed with `move` would be two
    // rungs of one ladder if `access` meant anything, and the fact the
    // writer needs is that it does not — the same sentence it gets alone.
    check_errors(
        "type C = struct { n: usize } only move only move;\n\
         type D = struct { n: usize } only move + move;\n\
         type E = struct { n: usize } only move + forget;\n\
         type F = struct { n: usize } only move + access;\n",
        expect![[r#"
            44..48: `move` is already this declaration's ceiling
            91..95: `move` is already this declaration's ceiling
            138..144: `move` and `forget` are rungs of the same ladder — a declaration ceils one ladder once
            187..193: the `access` capability does not exist yet; `move` is the only ceiling that can be written
        "#]],
    );
}

#[test]
fn the_capability_vocabulary_is_one_list_in_this_crate() {
    // `CAPABILITIES` is the whole vocabulary, and `hir` reads the same
    // rows: a capability that only one layer knows about is how the two
    // come to disagree about what `T: forget` asks for. A capability name
    // is never a keyword either — it is a name the language knows, so a
    // file may still declare a `trait forget` (hir warns that nothing can
    // reach it).
    for capability in crate::CAPABILITIES {
        assert!(
            crate::SyntaxKind::from_keyword(capability.name).is_none(),
            "`{}` is a capability name and must not be a keyword",
            capability.name
        );
        assert_eq!(
            crate::capability_named(capability.name).map(|c| c.name),
            Some(capability.name)
        );
    }
    assert!(crate::capability_named("leak").is_none());
    // The clause HEAD is the keyword, and it is the new one.
    assert_eq!(
        crate::SyntaxKind::from_keyword("only"),
        Some(crate::SyntaxKind::ONLY_KW)
    );
    assert_eq!(crate::SyntaxKind::from_keyword("without"), None);
}

#[test]
fn a_capability_clause_on_a_parameter_is_refused_in_both_spellings() {
    // T22 deleted the parameter home outright: `T without forget` is what
    // a bare `T` now means. Recognized and CONSUMED so the refusal is one
    // sentence and the binder list after it still parses — all three
    // parameter kinds, since the clause was superset-parsed on every one,
    // and in BOTH orders, since the retired grammar took the bounds on
    // either side of the clause. Only the TYPE parameter is offered a
    // migration: the other two never stood for something that could hold a
    // capability, so they are told what they are instead.
    //
    // The LIVE spelling in the same wrong home gets the same treatment
    // with its own lead: a reader who learns `only move` first will write
    // it where the opt-out used to go, and a ceiling on a parameter is
    // wrong for a different reason than a retired opt-out is.
    check_errors(
        "type Option = enum::<T without forget> { Some(T), None };\n\
         static f = fn::<T without forget, U>(t: T, u: U) -> T { t };\n\
         type A = enum::<@a without forget> { X };\n\
         type B = enum::<const N: usize without forget> { X };\n\
         static g = fn::<T without forget: usize>(t: T) -> T { t };\n\
         static h = fn::<T only move>(t: T) -> T { t };\n\
         type C = enum::<@a only move> { X };\n",
        expect![[r#"
            23..30: a capability clause on a parameter is retired: `T without forget` is now spelled `T` — an unbounded parameter is already checked as one that may have to be consumed; write `T: forget` only where the body DISCARDS a `T`, and a `type` declaration's parameters carry no capability bounds at all
            76..83: a capability clause on a parameter is retired: `T without forget` is now spelled `T` — an unbounded parameter is already checked as one that may have to be consumed; write `T: forget` only where the body DISCARDS a `T`, and a `type` declaration's parameters carry no capability bounds at all
            138..145: a capability clause has no place on a region parameter: a region names a duration, not a value, so it has no capability to speak of
            192..199: a capability clause has no place on a const parameter: a const parameter's values are always plain data
            233..240: a capability clause on a parameter is retired: `T without forget` is now spelled `T` — an unbounded parameter is already checked as one that may have to be consumed; write `T: forget` only where the body DISCARDS a `T`, and a `type` declaration's parameters carry no capability bounds at all
            292..296: a capability ceiling is a `type` declaration's, not a parameter's — an unbounded parameter is already checked as one that may have to be consumed; write `T: forget` only where the body DISCARDS a `T`, and a `type` declaration's parameters carry no capability bounds at all
            340..344: a capability clause has no place on a region parameter: a region names a duration, not a value, so it has no capability to speak of
        "#]],
    );
}

#[test]
fn a_capability_bound_rides_the_ordinary_bounds_slot() {
    // T22: a capability a BODY needs is a positive requirement, so it is
    // written where requirements are written. On the DATA side there is
    // nothing to require — a container is linear when what it holds is —
    // and saying "not supported yet" there would promise a spelling that
    // is never coming.
    check_errors(
        "static f = fn::<T: forget>(t: T) -> usize { 0 };\n\
         type Box = struct::<T: forget> { v: T };\n\
         type Pair = struct::<T: Display> { a: T };\n",
        expect![[r#"
            69..78: a `type` declaration's parameters carry no capability bounds: a container is linear when what it holds is
            111..121: bounds on a `type` declaration's binder are not supported yet
        "#]],
    );
}
