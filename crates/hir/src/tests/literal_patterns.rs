//! Character and integer literals as match patterns.

use super::*;

// --- char ---------------------------------------------------------------

#[test]
fn a_character_literal_has_a_definite_type() {
    // No number-class variable, no defining use, no `{number}`: there is
    // exactly one character type, so `'x'` is a `char` the moment it is
    // written — annotated or not, and in either direction.
    check_infer(
        r#"
static f = fn () {
    let a = 'x';
    let b: char = '\n';
    let c = a == b;
};
"#,
        expect![[r#"
            12..82 'fn () {     let a...': fn()
            18..82 '{     let a = 'x'...': ()
            28..29 'a': char
            32..35 ''x'': char
            45..46 'b': char
            55..59 ''\n'': char
            69..70 'c': bool
            73..74 'a': char
            73..79 'a == b': bool
            78..79 'b': char
        "#]],
    );
}

#[test]
fn a_character_is_not_an_integer() {
    // The ruling, checked: `char` is its own primitive, so it never
    // unifies with an integer type in either direction, and it carries no
    // arithmetic.
    check_diagnostics(
        r#"
static a: char = 65;
static b: usize = 'A';
static c = fn (x: char) -> char { x + x };
"#,
        expect![[r#"
            18..20: type mismatch: expected `char`, found `{number}` (expected `char` because of this annotation at 11..15)
            40..43: type mismatch: expected `usize`, found `char` (expected `usize` because of this annotation at 32..37)
            79..80: type mismatch: expected `{number}`, found `char` (`+` requires `{number}` operands at 81..82)
        "#]],
    );
}

#[test]
fn a_character_match_needs_a_catch_all() {
    // A `char` match is never exhaustive by enumeration, so the `_` arm is
    // required as policy, not arithmetic; its absence gets the ordinary
    // non-enum-scrutinee message.
    check_diagnostics(
        r#"
static f = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
static g = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
    }
};
"#,
        expect![[r#"
            150..155: this `match` does not cover every possible `char`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_character_pattern_must_match_the_scrutinee() {
    // A literal pattern's type is DEFINITE, so the pattern is what carries
    // the blame — the scrutinee is not re-typed to suit it.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            59..62: type mismatch: this `match` is on a `usize`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn a_character_pattern_pins_an_unknown_scrutinee() {
    // The other direction of the same rule: the pattern's type is
    // definite, so when the scrutinee is still a variable the pattern
    // pins it — a literal pattern is construction's mirror image, like a
    // qualified variant pattern.
    check_infer(
        r#"
static f = fn (c) -> usize {
    match c {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            12..85 'fn (c) -> usize {...': fn(char) -> usize
            16..17 'c': char
            28..85 '{     match c {  ...': usize
            34..83 'match c {        ...': usize
            40..41 'c': char
            59..60 '1': usize
            75..76 '0': usize
        "#]],
    );
}

#[test]
fn a_repeated_character_arm_is_unreachable() {
    // The variant precedent, applied to literals: the second `'a'` can
    // never run, and saying so beats silently dropping it.
    check_diagnostics(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
        'b' => 2,
        'a' => 3,
        _ => 0,
    }
};
"#,
        expect![[r#"
            94..97: unreachable arm: `'a'` is already covered by a previous arm
        "#]],
    );
}

// ---- integer literal patterns ------------------------------------------

#[test]
fn an_integer_pattern_takes_its_type_from_the_scrutinee() {
    // THE DEFINING-USE RULE, applied where a pattern meets a value: `0`
    // has no width of its own — no suffix, no default — so the scrutinee
    // decides it, exactly as context decides an integer literal's type in
    // expression position.
    //
    // Pinned where the claim is OBSERVABLE. The same written `200` is
    // accepted against a `u8`, REFUSED against an `i8`, and accepted again
    // against a `usize` — so the width it was range-checked at came from
    // the scrutinee and from nowhere else. (`check_infer` cannot show
    // this: it prints expression and binding types, and a pattern is
    // neither.)
    check_diagnostics(
        r#"
static narrow = fn (b: u8) -> usize { match b { 200 => 1, _ => 0 } };
static signed = fn (b: i8) -> usize { match b { 200 => 1, _ => 0 } };
static wide = fn (d: usize) -> usize { match d { 200 => 1, _ => 0 } };
"#,
        expect![[r#"
            119..122: `200` does not fit in `i8`
        "#]],
    );
}

#[test]
fn a_match_arm_binder_takes_the_scrutinee_type_beside_a_literal() {
    // The other half of the same fact, from the side inference DOES print:
    // a catch-all binder in a literal-dispatched match is the scrutinee's
    // own type, per match — so the two matches below are visibly over two
    // different integer types while their literal arms are written
    // identically.
    check_infer(
        r#"
static f = fn (b: u8, n: i64) -> usize {
    match b {
        0 => 1,
        small => 0,
    } + match n {
        0 => 1,
        big => 0,
    }
};
"#,
        expect![[r#"
            12..151 'fn (b: u8, n: i64...': fn(u8, i64) -> usize
            16..17 'b': u8
            23..24 'n': i64
            40..151 '{     match b {  ...': usize
            46..97 'match b {        ...': usize
            46..149 'match b {        ...': usize
            52..53 'b': u8
            69..70 '1': usize
            80..85 'small': u8
            89..90 '0': usize
            100..149 'match n {        ...': usize
            106..107 'n': i64
            123..124 '1': usize
            134..137 'big': i64
            141..142 '0': usize
        "#]],
    );
}

#[test]
fn an_integer_match_needs_a_catch_all() {
    // The `char` rule, and the same policy behind it: a literal arm never
    // counts toward exhaustiveness at any width, so the `_` arm is
    // required — a `u8` is listable in 256 arms and still needs it — and
    // its absence gets the ordinary non-enum-scrutinee message. Nothing
    // here is new machinery — the join/exhaustiveness path already said
    // this for every non-enum scrutinee.
    check_diagnostics(
        r#"
static f = fn (d: usize) -> usize {
    match d {
        0 => 1,
        1 => 2,
        _ => 0,
    }
};
static g = fn (d: usize) -> usize {
    match d {
        0 => 1,
        1 => 2,
    }
};
"#,
        expect![[r#"
            148..153: this `match` does not cover every possible `usize`; add a `_` arm
        "#]],
    );
}

#[test]
fn an_integer_pattern_out_of_range_is_refused() {
    // The scrutinee is the literal's defining use, so the ordinary
    // range check answers it — word-for-word the expression form's
    // message, with the squiggle on the pattern.
    check_diagnostics(
        r#"
static f = fn (b: u8) -> usize {
    match b {
        300 => 1,
        _ => 0,
    }
};
static g = fn (b: u8) -> usize {
    match b {
        255 => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            56..59: `300` does not fit in `u8`
        "#]],
    );
}

#[test]
fn an_integer_pattern_with_no_defining_use_needs_an_annotation() {
    // A pattern literal ASSERTS number-ness and nothing more, so it can
    // pin an unknown scrutinee to the number class — but never to a width.
    // Nothing defaults: the un-pinned literal gets the same "no defining
    // use" answer an expression one does, on the pattern.
    check_diagnostics(
        r#"
static f = fn (d) -> usize {
    match d {
        0 => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            52..53: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn an_unresolvable_scrutinee_is_reported_once_not_once_per_arm() {
    // TWO SYMPTOMS, ONE FACT. A variant pattern that cannot resolve and an
    // integer literal that has no width are both the same missing
    // annotation on the scrutinee, and the variant one already names where
    // to write it — so the number complaint stays quiet rather than saying
    // it again in other words. Arm ORDER does not matter: the suppression
    // is decided after the whole traversal, from the match, not from which
    // arm spoke first. (Compare
    // `an_integer_pattern_with_no_defining_use_needs_an_annotation`, where
    // there is no variant arm and the number complaint IS the answer.)
    check_diagnostics(
        r#"
static f = fn (d) -> usize {
    match d {
        ::Circle(r) => 1,
        0 => 2,
        _ => 0,
    }
};
static g = fn (d) -> usize {
    match d {
        0 => 2,
        ::Circle(r) => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            52..63: cannot resolve this pattern: the type of the matched value is not known here; add a type annotation to the scrutinee
            178..189: cannot resolve this pattern: the type of the matched value is not known here; add a type annotation to the scrutinee
        "#]],
    );
}

#[test]
fn a_repeated_integer_arm_is_unreachable() {
    // The `char` precedent, unchanged: one duplicate list serves every
    // literal kind, since the arms of a match all share a scrutinee type.
    check_diagnostics(
        r#"
static f = fn (d: usize) -> usize {
    match d {
        0 => 1,
        1 => 2,
        0 => 3,
        _ => 0,
    }
};
"#,
        expect![[r#"
            91..92: unreachable arm: `0` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn an_integer_pattern_must_match_the_scrutinee() {
    // What the pattern asserts is `{number}` — definite enough to carry
    // the blame, exactly as `char` does. The scrutinee is not re-typed to
    // suit it, in either direction.
    check_diagnostics(
        r#"
static f = fn (c: char) -> usize {
    match c {
        0 => 1,
        _ => 0,
    }
};
static g = fn (s: str) -> usize {
    match s {
        0 => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            58..59: type mismatch: this `match` is on a `char`, and `{number}` cannot match one
            147..148: type mismatch: this `match` is on a `str`, and `{number}` cannot match one
        "#]],
    );
}

#[test]
fn an_over_long_integer_pattern_names_no_value() {
    // A magnitude that doesn't fit `u128` names no value any scrutinee
    // could hold, so there is nothing to pin and nothing to range-check —
    // the expression form's sentence, in pattern position.
    check_diagnostics(
        r#"
static f = fn (d: usize) -> usize {
    match d {
        999999999999999999999999999999999999999999 => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            59..101: integer literal is too large
        "#]],
    );
}

#[test]
fn an_integer_pattern_projects_through_a_borrow() {
    // M13 generalises rather than gaining an exception: a referent this
    // match can DISPATCH on lifts the lens, and the integer types dispatch
    // by equality exactly as `char` does. So `match d { 0 => ... }` means
    // the same thing whether `d` is a `usize` or a `usize.&`, the
    // exhaustiveness message names the REFERENT, and a whole-value binder
    // still gets the borrow back.
    check_diagnostics(
        r#"
static f = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 1,
        _ => 0,
    }
};
static g = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 1,
    }
};
"#,
        expect![[r#"
            158..163: this `match` does not cover every possible `usize`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_binder_on_a_borrowed_integer_still_binds_the_borrow() {
    // The whole-value binder rule, unchanged by the projection — the
    // character twin of this test, over the other scalar.
    check_infer(
        r#"
static f = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            12..107 'fn::<@a>(d: usize...': fn(usize.&::<@a>) -> usize
            21..22 'd': usize.&::<@a>
            48..107 '{     match d {  ...': usize
            54..105 'match d {        ...': usize
            60..61 'd': usize.&::<@a>
            77..78 '1': usize
            88..93 'other': usize.&::<@a>
            97..98 '0': usize
        "#]],
    );
}

#[test]
fn a_literal_arm_re_reads_a_scrutinee_an_earlier_arm_pinned() {
    // THE ARMS OF ONE MATCH ARE NOT INDEPENDENT. The scrutinee is
    // classified ONCE, before the arm loop, and a literal arm can PIN a
    // still-unknown one — so "unknown" is a fact with a shelf life, and an
    // arm that trusts the pre-loop classification is reading a stale one.
    //
    // With a single literal kind this could not be observed. With two it
    // is a soundness hole: an arm loop trusting the pre-loop
    // classification accepts `f` below (int arm first) with no diagnostic
    // and lowers a `char` equality test against a number-typed local. Both
    // orders refuse here, and the refusal names the second arm — the one
    // that disagrees with what is already known.
    check_diagnostics(
        r#"
static takes_usize = fn (n: usize) -> usize { n };
static takes_char = fn (c: char) -> usize { 0 };
static f = fn (d) -> usize {
    match d {
        0 => 1,
        'a' => 2,
        _ => 0,
    } + takes_usize(d)
};
static g = fn (c) -> usize {
    match c {
        'a' => 1,
        0 => 2,
        _ => 0,
    } + takes_char(c)
};
static annotated = fn (d: usize) -> usize {
    match d {
        0 => 1,
        'a' => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            168..171: type mismatch: this `match` is on a `usize`, and `char` cannot match one
            289..290: type mismatch: this `match` is on a `char`, and `{number}` cannot match one
            420..423: type mismatch: this `match` is on a `usize`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn the_lens_is_decided_when_the_match_is_checked() {
    // A NAMED LIMIT, not a new one: the borrow lens is lifted from the
    // referent as it resolves AT THE MATCH, so a scrutinee whose referent
    // is still unknown there does not lift one — the pattern then meets
    // the BORROW and is refused, with a message that renders the referent
    // it later turned out to have. Pre-existing and universal to M13
    // (enums show it too); integer patterns only make it commoner, since
    // an integer scrutinee is so often pinned by a later use.
    //
    // Both scalars pinned, because the limit belongs to the lens and not
    // to a pattern kind. The fix a user needs is to annotate or pin the
    // scrutinee before the match; re-deciding the lift once the referent
    // resolves is M13's own Re-evaluate entry.
    check_diagnostics(
        r#"
static takes_usize = fn (n: usize) -> usize { n };
static takes_char = fn (c: char) -> usize { 0 };
static f = fn (d) -> usize {
    let r = match d.& {
        0 => 1,
        _ => 0,
    };
    r + takes_usize(d)
};
static g = fn (c) -> usize {
    let r = match c.& {
        'a' => 1,
        _ => 0,
    };
    r + takes_char(c)
};
"#,
        expect![[r#"
            162..163: type mismatch: this `match` is on a `usize.&`, and `{number}` cannot match one
            280..283: type mismatch: this `match` is on a `char.&`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn a_borrowed_integer_scrutinee_names_its_referent_in_pattern_advice() {
    // The lens moved `usize.&` from "not a scrutinee" to "dispatches by
    // equality", so the what-can-match clause is answered about the
    // REFERENT — the reading a user needs, since the literal they would
    // write is an integer one either way.
    check_diagnostics(
        r#"
static f = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        ::Circle(r) => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            72..83: a variant pattern needs an enum scrutinee; `_`, a binding, or an integer literal can match a `usize`
        "#]],
    );
}

#[test]
fn a_negative_integer_pattern_is_range_checked_with_its_sign() {
    // The sign folds in where the width is known: `-128` fits `i8` and
    // `-129` does not, and on an unsigned scrutinee every negative value
    // but `-0` is out of range.
    check_diagnostics(
        r#"
static f = fn (b: i8) -> usize {
    match b {
        -128 => 1,
        -129 => 2,
        _ => 0,
    }
};
static g = fn (n: usize) -> usize {
    match n {
        -1 => 1,
        -0 => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            75..79: `-129` does not fit in `i8`
            169..171: `-1` does not fit in `usize`
        "#]],
    );
}

#[test]
fn a_negative_literal_let_pattern_is_refutable() {
    // A negative literal in a `let` or parameter pattern can fail to match,
    // like any literal there.
    check_diagnostics(
        r#"
static f = fn (n: i64) -> usize {
    let -1 = n;
    0
};
"#,
        expect![[r#"
            43..45: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn minus_zero_and_zero_are_one_covered_value() {
    // The duplicate-arm key is the VALUE: `-0` is `0` spelled differently,
    // so the second arm can never run. `-1` and `1` are two values.
    check_diagnostics(
        r#"
static f = fn (n: i64) -> usize {
    match n {
        0 => 1,
        -0 => 2,
        -1 => 3,
        1 => 4,
        _ => 0,
    }
};
"#,
        expect![[r#"
            73..75: unreachable arm: `0` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn a_character_pattern_projects_through_a_borrow() {
    // M13's sealed rule is "the scrutinee's flavor decides, all the way
    // down", and a literal pattern is a pattern: `match c { 'a' => ... }`
    // means the same thing whether `c` is a `char` or a `char.&`.
    // Exhaustiveness is unchanged — a `_` arm is still required — and the
    // message now names the REFERENT rather than the borrow, because the
    // lens is what the scrutinee is being read through.
    check_diagnostics(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
static g = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        '(' => 1,
    }
};
"#,
        expect![[r#"
            176..181: this `match` does not cover every possible `char`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_binder_on_a_borrowed_character_still_binds_the_borrow() {
    // The whole-value binder rule, unchanged by the projection: it names
    // the same place the scrutinee does, so it gets the borrow itself back
    // at the scrutinee's own region — never a copied-out `char`.
    check_infer(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        'x' => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            12..108 'fn::<@a>(c: char....': fn(char.&::<@a>) -> usize
            21..22 'c': char.&::<@a>
            47..108 '{     match c {  ...': usize
            53..106 'match c {        ...': usize
            59..60 'c': char.&::<@a>
            78..79 '1': usize
            89..94 'other': char.&::<@a>
            98..99 '0': usize
        "#]],
    );
}
