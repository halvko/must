//! `loop`, `break`, `continue` and `return`.

use super::*;

#[test]
fn accumulator_loop_lowers_with_a_back_edge() {
    // The first cyclic CFG: bb-header re-entered by the back edge at the
    // body's end, break edges to the exit carrying the loop's value in a
    // dedicated result local.
    check_mir(
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
            item sum:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // acc
              _2: usize  // i
              _3: usize
              _4: usize
              _5: bool
              _6: ()
              _7: usize
              _8: usize
              _9: usize
              _10: usize
              _11: usize
              _12: usize
              bb0:
                _1 = 0
                _2 = 0
                goto -> bb1
              bb1:
                _4 = _2
                _5 = Eq(_4, 10)
                if _5 -> [then: bb3, else: bb4]
              bb2:
                _0 = _3
                return
              bb3:
                _7 = _1
                _3 = _7
                goto -> bb2
              bb4:
                _6 = ()
                goto -> bb6
              bb5:
                _6 = ()
                goto -> bb6
              bb6:
                _8 = _1
                _9 = _2
                _10 = Add(_8, _9)
                _1 = _10
                _11 = _2
                _12 = Add(_11, 1)
                _2 = _12
                goto -> bb1
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn break_with_value_stores_into_the_loop_result() {
    check_mir(
        "static f = fn () -> usize { loop { break 5; } };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                goto -> bb1
              bb1:
                _1 = 5
                goto -> bb2
              bb2:
                _0 = _1
                return
              bb3:
                goto -> bb1
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn breakless_loop_has_an_unreachable_exit() {
    // No break ever targets the exit block: it stays predecessor-less
    // (like the continuation after a diverging call), and the code after
    // the loop lowers into it so the CFG stays total.
    check_mir(
        "static f = fn -> usize { loop { }; 1 };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: !
              bb0:
                goto -> bb1
              bb1:
                goto -> bb1
              bb2:
                _0 = 1
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn continue_jumps_to_the_header() {
    check_mir(
        r#"
static f = fn (skip: bool) -> usize {
    loop {
        if skip { continue; };
        break 1;
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> usize {
              _0: usize  // return
              _1: bool  // param skip
              _2: usize
              _3: bool
              _4: ()
              bb0:
                goto -> bb1
              bb1:
                _3 = _1
                if _3 -> [then: bb3, else: bb4]
              bb2:
                _0 = _2
                return
              bb3:
                goto -> bb1
              bb4:
                _4 = ()
                goto -> bb6
              bb5:
                _4 = ()
                goto -> bb6
              bb6:
                _2 = 1
                goto -> bb2
              bb7:
                goto -> bb1
            }
            fn b1() -> fn(bool) -> usize {
              _0: fn(bool) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn break_outside_loop_traps_with_the_diagnostic() {
    check_mir(
        "static f = fn { break \"x\"; };",
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: {error}
              bb0:
                _1 = trap "`break` outside of a loop: there is no enclosing `loop` to exit" -> bb1
              bb1:
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- `return` ---------------------------------------------------------

#[test]
fn return_assigns_the_return_place_and_terminates() {
    // The existing function-exit path, reached early: the same
    // `_0 = <value>; return` pair the body's tail value lowers to. The
    // statements after it lower into a predecessor-less block, so the CFG
    // stays total and no execution can reach them.
    check_mir(
        r#"
static f = fn (c: bool) -> usize {
    if c { return 1; };
    2
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> usize {
              _0: usize  // return
              _1: bool  // param c
              _2: bool
              _3: ()
              bb0:
                _2 = _1
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _0 = 1
                return
              bb2:
                _3 = ()
                goto -> bb4
              bb3:
                _3 = ()
                goto -> bb4
              bb4:
                _0 = 2
                return
            }
            fn b1() -> fn(bool) -> usize {
              _0: fn(bool) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn bare_return_returns_unit() {
    check_mir(
        "static f = fn (c: bool) -> () { if c { return; }; };",
        expect![[r#"
            item f:
            fn b0(_1: bool) -> () {
              _0: ()  // return
              _1: bool  // param c
              _2: bool
              _3: ()
              bb0:
                _2 = _1
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _0 = ()
                return
              bb2:
                _3 = ()
                goto -> bb4
              bb3:
                _3 = ()
                goto -> bb4
              bb4:
                _0 = ()
                return
            }
            fn b1() -> fn(bool) {
              _0: fn(bool)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn return_inside_a_loop_leaves_the_function_not_the_loop() {
    // A `break` jumps to the loop's exit block; a `return` in the same
    // position terminates outright — the loop's exit is not involved.
    check_mir(
        r#"
static f = fn (n: usize) -> usize {
    loop {
        if n == 0 { return 7; };
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              _3: usize
              _4: bool
              _5: ()
              bb0:
                goto -> bb1
              bb1:
                _3 = _1
                _4 = Eq(_3, 0)
                if _4 -> [then: bb3, else: bb4]
              bb2:
                _0 = _2
                return
              bb3:
                _0 = 7
                return
              bb4:
                _5 = ()
                goto -> bb6
              bb5:
                _5 = ()
                goto -> bb6
              bb6:
                goto -> bb1
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_terminates_that_body() {
    // Structural: `b` is one MIR body's builder, and a `fn` literal builds
    // its own, so the inner `return` can only terminate the inner body.
    check_mir(
        r#"
static f = fn () -> usize {
    let inner = fn () -> usize { return 1; };
    inner()
};
"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 1
                return
              bb1:
                _0 = ()
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: fn() -> usize  // inner
              _2: fn() -> usize
              _3: usize
              bb0:
                _1 = fn b0
                _2 = _1
                _3 = call _2() -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn return_outside_a_function_traps_with_the_diagnostic() {
    // The trap has to sit on the path that REACHES the `return`: emitting
    // the exit edge first buries it in the unreachable block after the
    // `return`, and the execution leaves normally instead of trapping.
    check_mir(
        "static x = return 1;",
        expect![[r#"
            item x:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "`return` outside of a function: there is no enclosing `fn` body to return from" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn return_in_a_const_block_traps_instead_of_leaving_the_block() {
    // The reserve's MIR half: the `const` block's own body still lowers
    // whole, but the refused `return` plants a trap where it used to plant
    // the block's exit edge — so nothing produces the block's value early
    // any more.
    check_mir(
        "static x = fn () -> usize { const { if true { return 42; }; 0 } };",
        expect![[r#"
            item x:
            fn b0() -> usize {
              _0: usize  // return
              _1: ()
              _2: !
              bb0:
                if true -> [then: bb1, else: bb2]
              bb1:
                _2 = trap "`return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own" -> bb3
              bb2:
                _1 = ()
                goto -> bb4
              bb3:
                _1 = ()
                goto -> bb4
              bb4:
                _0 = 0
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              bb0:
                _0 = const b0
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}
