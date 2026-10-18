//! If/else joins: branch agreement and where a mismatch is blamed.

use super::*;

#[test]
fn if_is_an_expression_and_branches_must_agree() {
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    let big = if n > 100 { true } else { false };
    if big { n / 2 } else { n * 2 }
}
"#,
        expect![[r#"
            12..124 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..124 '{     let big = i...': usize
            45..48 'big': bool
            51..85 'if n > 100 { true...': bool
            54..55 'n': usize
            54..61 'n > 100': bool
            58..61 '100': usize
            62..70 '{ true }': bool
            64..68 'true': bool
            76..85 '{ false }': bool
            78..83 'false': bool
            91..122 'if big { n / 2 } ...': usize
            94..97 'big': bool
            98..107 '{ n / 2 }': usize
            100..101 'n': usize
            100..105 'n / 2': usize
            104..105 '2': usize
            113..122 '{ n * 2 }': usize
            115..116 'n': usize
            115..120 'n * 2': usize
            119..120 '2': usize
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 1 } else { "one" };
    print("done");
}
"#,
        expect![[r#"
            58..59: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 69..74)
        "#]],
    );
}

#[test]
fn annotated_let_blames_branch_not_whole_if() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { 0 } else { "" };
    print(x);
}
"#,
        expect![[r#"
            63..64: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { 0 } else { "" };
    print("");
}
"#,
        expect![[r#"
            76..78: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn if_result_constrained_by_use_blames_branch() {
    // Then branch is the culprit — squiggle on the literal, hint at print call.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 0 } else { "" };
    print(x);
}
"#,
        expect![[r#"
            58..59: type mismatch: expected `str`, found `{number}` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
        "#]],
    );
    // Else branch is the culprit — squiggle on the literal, hint at print call.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { 0 };
    print(x);
}
"#,
        expect![[r#"
            70..71: type mismatch: expected `str`, found `{number}` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
        "#]],
    );
}

#[test]
fn nested_if_blame_propagates_to_innermost_culprit() {
    // The wrong `0` is inside a nested if; blame reaches it rather than
    // stopping at the outer else branch, and the hints cite *every* branch
    // that voted `str` (inner and outer) plus the call that demanded it.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    print(x);
}
"#,
        expect![[r#"
            94..95: type mismatch: expected `str`, found `{number}` (this call requires `str` at 105..110) (this argument needs to be `str` at 111..112)
        "#]],
    );
}

#[test]
fn every_offending_branch_gets_its_own_squiggle_not_the_whole_if() {
    // Two branches are wrong (`""` in both nesting levels), one is right
    // (`0`). Each culprit gets its own squiggle; the outer `if` as a whole
    // stays clean — it isn't wrong, its branches are.
    check_diagnostics(
        r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    constrainer("", x);
}
"#,
        expect![[r#"
            105..107: type mismatch: expected `usize`, found `str` (this call requires `usize` at 152..163) (this argument needs to be `usize` at 168..169)
            129..131: type mismatch: expected `usize`, found `str` (this call requires `usize` at 152..163) (this argument needs to be `usize` at 168..169)
        "#]],
    );
}

#[test]
fn unanimous_branches_against_call_axiom_blame_the_whole_construct() {
    // Every branch produces `str`, so no single branch is the culprit: the
    // construct as a whole conflicts with the call's requirement. Hints
    // point at the call and the argument the requirement travels through.
    check_diagnostics(
        r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { "" };
    constrainer("", x);
}
"#,
        expect![[r#"
            93..121: every branch produces `str`, but `usize` is needed (this call requires `usize` at 127..138) (this argument needs to be `usize` at 143..144)
        "#]],
    );
}

#[test]
fn plurality_of_branches_decides_without_any_axiom() {
    // No annotation and no use constrains `x`: the branches vote, the
    // `str` plurality wins, and the odd one out gets the squiggle with
    // every winning leaf as a hint, in source order.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 1 { 1 } else { "" } };
    print("done");
}
"#,
        expect![[r#"
            82..83: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 58..60) (this branch has type `str` at 93..95)
        "#]],
    );
}

#[test]
fn nested_branches_vote_individually() {
    // The vote flattens across nested `if`s in the same function: the two
    // `0` leaves outvote the single `""` two to one, even though the `""`
    // sits shallower. The odd one out gets the squiggle; every winning
    // leaf is a hint.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { if n == 0 { 0 } else { 0 } } else { "" };
    print("done");
}
"#,
        expect![[r#"
            70..71: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 94..96)
            81..82: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 94..96)
        "#]],
    );
}

#[test]
fn nested_join_resolves_once_at_the_annotation() {
    // A nest of `if`s is ONE join with witnesses {leaf, leaf, leaf},
    // resolved at the `let`'s annotation (the axiom): exactly one
    // diagnostic, on the offending leaf, citing the annotation — never an
    // intermediate "the inner if has type …" step. Variants: the culprit in
    // the inner-then, inner-else, and outer-else slots.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { 0 } else { "a" } } else { "b" };
    print(x);
}
"#,
        expect![[r#"
            75..76: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { "a" } else { 0 } } else { "b" };
    print(x);
}
"#,
        expect![[r#"
            88..89: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { "a" } else { "b" } } else { 0 };
    print(x);
}
"#,
        expect![[r#"
            103..104: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
}

#[test]
fn inner_branches_blamed_individually_never_the_inner_if() {
    // Both leaves of the *inner* if disagree with the annotation while the
    // outer-else leaf agrees: each wrong leaf gets its own squiggle citing
    // the annotation. The inner `if` is not a blame target — there is no
    // "every branch of the inner if" verdict, because the nest is one join.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { if n == 1 { "a" } else { "b" } } else { 1 };
    print("done");
}
"#,
        expect![[r#"
            77..80: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
            90..93: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn nested_leaves_unanimous_against_annotation_blame_the_whole_nest() {
    // Every leaf across both nesting levels produces `str`: one diagnostic
    // on the whole (outermost) construct, not per leaf and not on the
    // inner if.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { if n == 1 { "a" } else { "b" } } else { "c" };
    print("done");
}
"#,
        expect![[r#"
            53..110: every branch produces `str`, but `usize` is needed (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn statement_position_if_inside_a_branch_is_its_own_join() {
    // The inner `if` sits in a branch's *statements* (a `let`), not its
    // tail: it is statement position, so it resolves as its own join (an
    // unresolvable tie here) and does not leak its leaves into the outer
    // join — which is internally consistent and stays clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { let y = if n == 1 { 1 } else { "s" }; 2 } else { 3 };
    print("done");
}
"#,
        expect![[r#"
            78..79: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 89..92)
            96..97: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_bound_join_resolves_at_its_let_not_inside_a_later_join() {
    // `x`'s join meets a non-join consumer (the `let`), so it resolves
    // there — unanimously `usize` — before the second `if` consumes the
    // binding. The second join then sees {usize, str}: an honest tie, not
    // a plurality vote over `x`'s leaves.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 1 } else { 2 };
    let z = if n == 1 { x } else { "s" };
    print("done");
}
"#,
        expect![[r#"
            98..99: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 109..112)
        "#]],
    );
}

#[test]
fn call_argument_join_resolves_against_the_parameter_type() {
    // A call argument is not statement position, but the call boundary
    // makes the parameter type an axiom: the join resolves against it
    // right there, blaming the disagreeing leaf and citing the call.
    check_diagnostics(
        r#"static f = fn (n: usize) -> () { print(if n == 0 { "s" } else { 1 }); };"#,
        expect![[r#"
            64..65: type mismatch: expected `str`, found `{number}`
        "#]],
    );
}

#[test]
fn never_leaf_in_a_nested_if_neither_votes_nor_blocks_flattening() {
    // A diverging leaf inside the inner if widens (it contributes no
    // witness) — the flat join judges the surviving leaves {0, "s"}
    // against the annotation, blaming exactly the `0`, once.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { panic("boom") } else { 0 } } else { "s" };
    print(x);
}
"#,
        expect![[r#"
            98..99: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    // And when the surviving leaves all agree, the nest is clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { panic("boom") } else { "a" } } else { "b" };
    print(x);
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn hover_on_a_nested_if_shows_the_flat_joins_type() {
    // Decision: an `if` in witness position types as the enclosing join's
    // result, so hovering the inner `if` shows the whole nest's resolved
    // type (`usize`), not a partial verdict of its own.
    check_infer(
        r#"static f = fn (n: usize) -> usize { if n == 0 { if n == 1 { 1 } else { 2 } } else { 3 } };"#,
        expect![[r#"
            11..89 'fn (n: usize) -> ...': fn(usize) -> usize
            15..16 'n': usize
            34..89 '{ if n == 0 { if ...': usize
            36..87 'if n == 0 { if n ...': usize
            39..40 'n': usize
            39..45 'n == 0': bool
            44..45 '0': usize
            46..76 '{ if n == 1 { 1 }...': usize
            48..74 'if n == 1 { 1 } e...': usize
            51..52 'n': usize
            51..57 'n == 1': bool
            56..57 '1': usize
            58..63 '{ 1 }': usize
            60..61 '1': usize
            69..74 '{ 2 }': usize
            71..72 '2': usize
            82..87 '{ 3 }': usize
            84..85 '3': usize
        "#]],
    );
}

#[test]
fn fn_literal_votes_once_as_a_unit() {
    // Wrapping the nested `if` in a function changes the vote: the
    // function settles its type internally (`usize`, unanimously) and
    // contributes exactly one vote outside — a genuine tie with `""`.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { fn { if true { 0 } else { 0 } }() } else { "" };
    print("done");
}
"#,
        expect![[r#"
            58..91: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 101..103)
        "#]],
    );
}

#[test]
fn fn_internal_inconsistency_not_resolved_by_use() {
    // The function's branches disagree with each other; the annotation on
    // `x` must not settle that argument from outside — a function has to
    // be internally consistent on its own. The tie is reported inside the
    // function, and the conflicting use at the call, as a unit.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { fn { if true { 0 } else { "" } }() } else { "s" };
    print(x);
}
"#,
        expect![[r#"
            78..79: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 89..91)
        "#]],
    );
}

#[test]
fn agreeing_branches_against_return_annotation_get_one_diagnostic() {
    // Both branches say str; the return annotation says usize. One
    // diagnostic on the whole `if`, not a squiggle per branch.
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n == 0 { "a" } else { "b" } };"#,
        expect![[r#"
            36..66: every branch produces `str`, but `usize` is needed (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn group_member_signature_not_overridden_by_use() {
    // `g` and `main` are unannotated, so they infer as one group. `g`'s
    // branches unanimously say usize; the `print` in `main` must not flip
    // `g`'s signature to `str` — the mismatch belongs at the call site.
    check_diagnostics(
        r#"
static g = fn (c: bool, n: usize) { if c { n } else { 2 } };
static main = fn { print(g(true, 3)); };
"#,
        expect![[r#"
            87..97: type mismatch: expected `str`, found `usize`
        "#]],
    );
}

#[test]
fn if_condition_must_be_bool() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> () { if n { print("hi"); } };"#,
        expect![[r#"
            36..37: type mismatch: expected `bool`, found `usize` (this `if` requires a `bool` condition at 33..35)
        "#]],
    );
}

#[test]
fn if_without_else_is_unit() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n > 0 { n } };"#,
        expect![[r#"
            36..50: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
            47..48: type mismatch: expected `()`, found `usize` (this `if` has no `else`, so its value is `()` at 36..38)
        "#]],
    );
}

#[test]
fn diverging_if_branch_takes_the_other_branches_type() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n == 0 { panic("zero") } else { n } };"#,
        expect![[r#""#]],
    );
    check_infer(
        r#"static f = fn (n: usize) -> usize { if n == 0 { panic("a") } else { panic("b") } };"#,
        expect![[r#"
            11..82 'fn (n: usize) -> ...': fn(usize) -> usize
            15..16 'n': usize
            34..82 '{ if n == 0 { pan...': usize
            36..80 'if n == 0 { panic...': usize
            39..40 'n': usize
            39..45 'n == 0': bool
            44..45 '0': usize
            46..60 '{ panic("a") }': !
            48..53 'panic': fn(str) -> !
            48..58 'panic("a")': !
            54..57 '"a"': str
            66..80 '{ panic("b") }': !
            68..73 'panic': fn(str) -> !
            68..78 'panic("b")': !
            74..77 '"b"': str
        "#]],
    );
}

#[test]
fn equality_operands_must_agree() {
    check_diagnostics(
        r#"static x: bool = 1 == "one";"#,
        expect![[r#"
            22..27: type mismatch: expected `{number}`, found `str` (this operand has type `{number}` at 17..18)
        "#]],
    );
}

#[test]
fn comparing_an_enum_with_another_type_says_to_match() {
    // On the enum operand, whichever side it is on; the variant carrying
    // the other operand's type is named when there is one, every variant
    // otherwise.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: str, sh: Shape) {
    let c = s.next_char(0);
    let a = c == 'x';
    let b = 'x' != c;
    let d = sh == "circle";
    let e = sh == Shape::Point;
};
"#,
        expect![[r#"
            126..129: type mismatch: expected `NextChar`, found `char` (this operand has type `NextChar` at 121..122) (`NextChar` is an enum: `match` on it to get at the `char` in `NextChar::Char(char, usize)` at 121..122)
            150..151: type mismatch: expected `char`, found `NextChar` (this operand has type `char` at 143..146) (`NextChar` is an enum: `match` on it to get at the `char` in `NextChar::Char(char, usize)` at 150..151)
            171..179: type mismatch: expected `Shape`, found `str` (this operand has type `Shape` at 165..167) (`Shape` is an enum: `match` on it to tell its variants apart (`Shape::Circle(usize)`, `Shape::Point`) at 165..167)
        "#]],
    );
}

#[test]
fn comparing_a_generic_enum_names_the_instantiated_payload() {
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn (o: Opt::<char>) -> bool { o == 'x' };
"#,
        expect![[r#"
            87..90: type mismatch: expected `Opt::<char>`, found `char` (this operand has type `Opt::<char>` at 82..83) (`Opt` is an enum: `match` on it to get at the `char` in `Opt::Some(char)` at 82..83)
        "#]],
    );
}
