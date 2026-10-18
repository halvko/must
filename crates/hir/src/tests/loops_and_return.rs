//! `loop`, `break`, `continue` and `return`.

use super::*;

#[test]
fn loop_types_as_its_break_value() {
    // The owner's motivating accumulator: break values are the witnesses of
    // one join, and that join's result is the loop's type.
    check_infer(
        r#"
static sum = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
"#,
        expect![[r#"
            14..166 'fn () -> usize { ...': fn() -> usize
            29..166 '{     let mut acc...': usize
            43..46 'acc': usize
            49..50 '0': usize
            64..65 'i': usize
            68..69 '0': usize
            75..164 'loop {         if...': usize
            80..164 '{         if i ==...': ()
            90..115 'if i == 10 { brea...': ()
            93..94 'i': usize
            93..100 'i == 10': bool
            98..100 '10': usize
            101..115 '{ break acc; }': ()
            103..112 'break acc': !
            109..112 'acc': usize
            125..128 'acc': usize
            131..134 'acc': usize
            131..138 'acc + i': usize
            137..138 'i': usize
            148..149 'i': usize
            152..153 'i': usize
            152..157 'i + 1': usize
            156..157 '1': usize
        "#]],
    );
}

#[test]
fn breakless_loop_types_never() {
    // …and so does the fn around it: the body diverges and nothing ever
    // said what `f` produces, so the return type defaults to `!` instead
    // of staying an unpinned `_`.
    check_infer(
        "static f = fn { loop { } };",
        expect![[r#"
            11..26 'fn { loop { } }': fn() -> !
            14..26 '{ loop { } }': !
            16..24 'loop { }': !
            21..24 '{ }': ()
        "#]],
    );
}

#[test]
fn bare_break_is_a_unit_witness() {
    // `break;` carries `()` as the loop's value.
    check_infer(
        "static f = fn { loop { break; } };",
        expect![[r#"
            11..33 'fn { loop { break...': fn()
            14..33 '{ loop { break; } }': ()
            16..31 'loop { break; }': ()
            21..31 '{ break; }': !
            23..28 'break': !
        "#]],
    );
}

#[test]
fn loop_break_values_of_different_variants_join_to_the_enum() {
    // Break values go through the same join seam as `if` branches and
    // `match` arms: different variants of one enum are all the enum.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (stop: bool) {
    let s = loop {
        if stop { break Shape::Circle(1); };
        break Point_or(stop);
    };
};
static Point_or = fn (b: bool) -> Shape { Shape::Point };
"#,
        expect![[r#"
            56..176 'fn (stop: bool) {...': fn(bool)
            60..64 'stop': bool
            72..176 '{     let s = loo...': ()
            82..83 's': Shape
            86..173 'loop {         if...': Shape
            91..173 '{         if stop...': !
            101..136 'if stop { break S...': ()
            104..108 'stop': bool
            109..136 '{ break Shape::Ci...': ()
            111..133 'break Shape::Circ...': !
            117..130 'Shape::Circle': fn(usize) -> Shape
            117..133 'Shape::Circle(1)': Shape
            131..132 '1': usize
            146..166 'break Point_or(stop)': !
            152..160 'Point_or': fn(bool) -> Shape
            152..166 'Point_or(stop)': Shape
            161..165 'stop': bool
            196..234 'fn (b: bool) -> S...': fn(bool) -> Shape
            200..201 'b': bool
            218..234 '{ Shape::Point }': Shape
            220..232 'Shape::Point': Shape
        "#]],
    );
}

#[test]
fn loop_break_value_checks_against_the_return_annotation() {
    check_diagnostics(
        r#"
static f = fn () -> usize {
    loop {
        break "text";
    }
};
"#,
        expect![[r#"
            33..67: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 18..26)
        "#]],
    );
}

#[test]
fn nested_loops_inner_break_does_not_exit_the_outer() {
    // The inner loop's break is the *inner* loop's value; the outer loop
    // has no value-carrying break, so it types `!` and the fn returns `!`
    // — pinned by the (clean) types below.
    check_infer(
        r#"
static f = fn {
    loop {
        let n = loop { break 1; };
        n;
    }
};
"#,
        expect![[r#"
            12..81 'fn {     loop {  ...': fn() -> !
            15..81 '{     loop {     ...': !
            21..79 'loop {         le...': !
            26..79 '{         let n =...': ()
            40..41 'n': {number}
            44..61 'loop { break 1; }': {number}
            49..61 '{ break 1; }': !
            51..58 'break 1': !
            57..58 '1': {number}
            71..72 'n': {number}
        "#]],
    );
}

#[test]
fn break_outside_loop_errors() {
    check_diagnostics(
        "static f = fn { break 1; };",
        expect![[r#"
            16..23: `break` outside of a loop: there is no enclosing `loop` to exit
            22..23: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn continue_outside_loop_errors() {
    check_diagnostics(
        "static f = fn { continue; };",
        expect![[r#"
            16..24: `continue` outside of a loop: there is no enclosing `loop` to restart
        "#]],
    );
}

#[test]
fn dangling_break_at_top_level_errors() {
    check_diagnostics(
        "static x = break 1;",
        expect![[r#"
            11..18: `break` outside of a loop: there is no enclosing `loop` to exit
            17..18: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn break_inside_fn_literal_does_not_escape_the_loop() {
    // A fn body is a fresh context: `break` never crosses a fn boundary,
    // so this is the same outside-a-loop error.
    check_diagnostics(
        r#"
static f = fn {
    loop {
        let g = fn { break 1; };
        g();
    }
};
"#,
        expect![[r#"
            49..56: `break` outside of a loop: there is no enclosing `loop` to exit
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn break_inside_const_block_does_not_escape_the_loop() {
    // A `const` block is a compile-time unit of its own (MIR lowers it to
    // a separate body), so it bounds the loop context like a fn literal.
    check_diagnostics(
        r#"
static f = fn {
    loop {
        let x = const { break 1; };
    }
};
"#,
        expect![[r#"
            52..59: `break` outside of a loop: there is no enclosing `loop` to exit
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn loop_in_const_fn_is_clean() {
    check_diagnostics(
        r#"
static sum = const fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
static x = sum();
"#,
        expect![[r#""#]],
    );
}

// ---- `return` ---------------------------------------------------------

#[test]
fn return_with_a_value_types_never_and_checks_the_operand() {
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    return n + 1;
    0
};
"#,
        expect![[r#"
            12..62 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..62 '{     return n + ...': usize
            41..53 'return n + 1': !
            48..49 'n': usize
            48..53 'n + 1': usize
            52..53 '1': usize
            59..60 '0': usize
        "#]],
    );
}

#[test]
fn bare_return_is_a_unit_return() {
    // `return;` returns `()` — the same value a bare `break;` carries.
    check_infer(
        r#"
static f = fn (c: bool) -> () {
    if c { return; };
};
"#,
        expect![[r#"
            12..56 'fn (c: bool) -> (...': fn(bool)
            16..17 'c': bool
            31..56 '{     if c { retu...': ()
            37..53 'if c { return; }': ()
            40..41 'c': bool
            42..53 '{ return; }': ()
            44..50 'return': !
        "#]],
    );
}

#[test]
fn return_as_a_block_tail_is_clean() {
    check_diagnostics(
        "static f = fn (n: usize) -> usize { return n };",
        expect![""],
    );
}

#[test]
fn return_in_one_if_branch_lets_the_other_branch_win() {
    // The `Never` join: a diverging branch does not vote, so `x` takes the
    // surviving branch's type. Both the tail spelling (`return 0`) and the
    // statement spelling (`return 0;`) diverge.
    check_infer(
        r#"
static f = fn (c: bool) -> usize {
    let x = if c { 1 } else { return 0 };
    let y = if c { 2 } else { return 0; };
    x + y
};
"#,
        expect![[r#"
            12..132 'fn (c: bool) -> u...': fn(bool) -> usize
            16..17 'c': bool
            34..132 '{     let x = if ...': usize
            44..45 'x': usize
            48..76 'if c { 1 } else {...': usize
            51..52 'c': bool
            53..58 '{ 1 }': usize
            55..56 '1': usize
            64..76 '{ return 0 }': !
            66..74 'return 0': !
            73..74 '0': usize
            86..87 'y': usize
            90..119 'if c { 2 } else {...': usize
            93..94 'c': bool
            95..100 '{ 2 }': usize
            97..98 '2': usize
            106..119 '{ return 0; }': !
            108..116 'return 0': !
            115..116 '0': usize
            125..126 'x': usize
            125..130 'x + y': usize
            129..130 'y': usize
        "#]],
    );
}

#[test]
fn return_from_inside_loop_match_and_nested_blocks_is_clean() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape, n: usize) -> usize {
    { { if n == 0 { return 0; }; }; };
    loop {
        match s {
            ::Circle(r) => return r,
            ::Point => return n,
        };
    }
};
"#,
        expect![""],
    );
}

#[test]
fn return_mismatch_blames_the_operand_and_cites_the_return_type() {
    // The annotated case: the OPERAND carries the squiggle and the written
    // return type is the related note — the identical treatment the body's
    // tail expression gets, because it is the identical check.
    check_diagnostics(
        r#"
static f = fn (c: bool) -> usize {
    if c { return "text"; };
    0
};
"#,
        expect![[r#"
            54..60: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn bare_return_mismatch_blames_the_return_keyword() {
    // Nothing else exists to blame, so the `return` itself is squiggled.
    check_diagnostics(
        r#"
static f = fn (c: bool) -> usize {
    if c { return; };
    0
};
"#,
        expect![[r#"
            47..53: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn return_pins_an_inferred_return_type() {
    // No `-> T` annotation anywhere: the `return` is the only thing that
    // says what `f` produces, and it pins the signature exactly as a tail
    // expression would.
    check_infer(
        r#"
static f = fn (c: bool) {
    if c { return "yes"; };
    "no"
};
"#,
        expect![[r#"
            12..65 'fn (c: bool) {   ...': fn(bool) -> str
            16..17 'c': bool
            25..65 '{     if c { retu...': str
            31..53 'if c { return "ye...': ()
            34..35 'c': bool
            36..53 '{ return "yes"; }': ()
            38..50 'return "yes"': !
            45..50 '"yes"': str
            59..63 '"no"': str
        "#]],
    );
}

#[test]
fn return_and_tail_must_agree_on_an_inferred_return_type() {
    // The flip side: `return` feeds the SAME constraint the tail does, so
    // two disagreeing producers are a mismatch, not a silent widening.
    check_diagnostics(
        r#"
static f = fn (c: bool) {
    if c { return "yes"; };
    0
};
"#,
        expect![[r#"
            59..60: type mismatch: expected `str`, found `{number}`
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_returns_from_that_literal() {
    // THE semantics people get wrong. `inner`'s `return` produces
    // `inner`'s value; the outer fn carries on and returns a `str`. If it
    // returned from the outer fn instead, `-> usize` on `inner` and
    // `-> str` on the outer literal could not both be clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> str {
    let inner = fn (m: usize) -> usize { return m + 1; };
    if inner(n) == 0 { return "zero"; };
    "more"
};
"#,
        expect![""],
    );
}

#[test]
fn return_in_a_nested_fn_literal_checks_against_that_literal() {
    // Same boundary from the failing side: the inner literal's own return
    // type is what the inner `return` is checked against — the outer
    // `-> str` never enters the picture.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let inner = fn () -> usize { return "text"; };
    "ok"
};
"#,
        expect![[r#"
            67..73: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 49..57)
        "#]],
    );
}

#[test]
fn return_in_a_const_fn_is_clean() {
    check_diagnostics(
        r#"
static clamped = const fn (n: usize) -> usize {
    if n > 10 { return 10; };
    n
};
static x = clamped(42);
"#,
        expect![""],
    );
}

#[test]
fn return_in_a_const_block_is_reserved() {
    // REVERSING what the `return` arc first shipped (a `return` here used
    // to yield the CONST BLOCK's value, like a `break` bound by it).
    // Conceptually it bails from the OUTER fn body — which needs
    // cross-body machinery no v1 pass has — so v1 refuses it outright
    // rather than picking the reachable-but-wrong reading.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { if true { return 42; }; 0 };
    "ok"
};
"#,
        expect![[r#"
            64..73: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_const_block_reports_once() {
    // The reservation is the whole story: the operand is inferred (so its
    // contents still get types and diagnostics) but nothing demands a type
    // of it, so neither the block's expectation nor a bare literal in it
    // adds a sibling diagnostic.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { return "text"; };
    "ok"
};
static g = const { return 1; };
"#,
        expect![[r#"
            54..67: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
            103..111: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_fn_literal_nested_in_a_const_block_is_fine() {
    // The reservation is about the `const` block being the NEAREST body.
    // A `fn` literal inside one is its own body, so its `return` leaves
    // the literal exactly as anywhere else — nothing to reserve.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { let inner = fn () -> usize { return 7; }; 0 };
    "ok"
};
"#,
        expect![""],
    );
}

#[test]
fn return_outside_a_function_errors() {
    // An item initializer's own top level is a value expression, not a
    // function body — there is nothing to leave. The refusal is the whole
    // story: nothing here demanded a type of the operand, so the bare
    // literal must not add a no-defining-use sibling.
    check_diagnostics(
        "static x = return 1;",
        expect![[r#"
            11..19: `return` outside of a function: there is no enclosing `fn` body to return from
        "#]],
    );
}

#[test]
fn statements_after_a_return_still_check() {
    // No unreachable-code lint yet (that is its own round) — but the
    // statements after a `return` must still type, resolve and hover
    // exactly as written, and must not manufacture errors of their own.
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    return n;
    let doubled = n + n;
    doubled
};
"#,
        expect![[r#"
            12..89 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..89 '{     return n;  ...': usize
            41..49 'return n': !
            48..49 'n': usize
            59..66 'doubled': usize
            69..70 'n': usize
            69..74 'n + n': usize
            73..74 'n': usize
            80..87 'doubled': usize
        "#]],
    );
}
