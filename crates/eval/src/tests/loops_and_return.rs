//! `loop`, `break`, `continue` and `return`.

use super::*;

#[test]
fn accumulator_loop_runs() {
    // The mutability motivation: a mutable accumulator stepped by a loop.
    check_run(
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
        "sum()",
        expect![[r#"
            => 45
        "#]],
    );
}

#[test]
fn accumulator_loop_const_evaluates() {
    // The same accumulator inside a `const fn`, forced at compile time.
    check_const(
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
static answer = sum();
"#,
        expect![[r#"
            sum = fn
            answer = 45
        "#]],
    );
}

#[test]
fn bare_break_carries_unit() {
    check_run(
        r#"
static f = fn () {
    let mut i: usize = 0;
    loop {
        if i == 3 { break; };
        i = i + 1;
    }
};
"#,
        "f()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn continue_skips_work() {
    // Sum 0..10 skipping the even numbers: 1 + 3 + 5 + 7 + 9 = 25.
    check_run(
        r#"
static is_even = fn (n: usize) -> bool { (n / 2) * 2 == n };
static sum_odd = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        i = i + 1;
        if is_even(i - 1) { continue; };
        acc = acc + (i - 1);
    }
};
"#,
        "sum_odd()",
        expect![[r#"
            => 25
        "#]],
    );
}

#[test]
fn nested_loops_inner_break_stays_inner() {
    // 3 * 4: the inner loop finishes per outer iteration — its break never
    // exits the outer loop.
    check_run(
        r#"
static grid = fn () -> usize {
    let mut total: usize = 0;
    let mut row: usize = 0;
    loop {
        if row == 3 { break total; };
        let mut col: usize = 0;
        total = total + loop {
            if col == 4 { break col; };
            col = col + 1;
        };
        row = row + 1;
    }
};
"#,
        "grid()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn break_in_nested_if_exits_the_loop() {
    check_run(
        r#"
static first_over = fn (limit: usize) -> usize {
    let mut n = 0;
    loop {
        if limit < n {
            if true { break n * 10; };
        };
        n = n + 1;
    }
};
"#,
        "first_over(2)",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn infinite_loop_in_an_initializer_runs_out_of_fuel() {
    // Loops are const-legal; the machine's existing fuel budget is what
    // bounds a runaway compile-time loop.
    check_const(
        "static spin = loop { };",
        expect![[r#"
            spin = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

#[test]
fn infinite_loop_in_a_const_block_runs_out_of_fuel() {
    check_const_blocks(
        "static f = fn { const { loop { } } };",
        expect![[r#"
            f#0 = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

// ---- `return` end to end ----------------------------------------------

#[test]
fn return_skips_the_statements_after_it() {
    // The observable proof that the early exit really happens: "after"
    // never prints.
    check_run(
        r#"
static f = fn (c: bool) -> usize {
    print("before ");
    if c { return 1; };
    print("after ");
    2
};
"#,
        "f(true)",
        expect![[r#"
            output: "before "
            => 1
        "#]],
    );
}

#[test]
fn without_the_early_exit_the_rest_still_runs() {
    // The same function down the other edge — the contrast that makes the
    // test above mean something.
    check_run(
        r#"
static f = fn (c: bool) -> usize {
    print("before ");
    if c { return 1; };
    print("after ");
    2
};
"#,
        "f(false)",
        expect![[r#"
            output: "before after "
            => 2
        "#]],
    );
}

#[test]
fn return_leaves_a_loop_and_the_function_together() {
    // A `break` would only leave the loop and fall into `999`; `return`
    // leaves both.
    check_run(
        r#"
static first_over = fn (limit: usize) -> usize {
    let mut n = 0;
    loop {
        if limit < n { return n * 10; };
        n = n + 1;
    };
    999
};
"#,
        "first_over(2)",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn bare_return_evaluates_to_unit() {
    check_run(
        r#"
static f = fn (c: bool) -> () {
    if c { return; };
    print("not taken");
};
"#,
        "f(true)",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_only_exits_that_literal() {
    // `inner` returns early; the OUTER function carries on and prints
    // after the call — the semantics people get wrong, run for real.
    check_run(
        r#"
static f = fn () -> usize {
    let inner = fn (n: usize) -> usize {
        if n == 0 { return 100; };
        n
    };
    let a = inner(0);
    print("outer still running ");
    a + 1
};
"#,
        "f()",
        expect![[r#"
            output: "outer still running "
            => 101
        "#]],
    );
}

#[test]
fn return_in_a_const_fn_evaluates_at_compile_time() {
    check_const(
        r#"
static clamped = const fn (n: usize) -> usize {
    if n > 10 { return 10; };
    n
};
static x: usize = clamped(42);
static y: usize = clamped(3);
"#,
        expect![[r#"
            clamped = fn
            x = 10
            y = 3
        "#]],
    );
}

#[test]
fn return_in_a_const_block_traps_at_the_return() {
    // The reserve's runtime half: the block is still lowered and still
    // evaluated — deferred errors, like every other refused construct —
    // and the execution that REACHES the `return` traps there with the
    // checker's own message. The old behavior (this block evaluating to
    // `42`) must not survive anywhere.
    check_const_blocks(
        "static f = fn { let x: usize = const { if true { return 42; }; 0 }; };",
        expect![[r#"
            f#0 = error[Trap]: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_fn_literal_inside_a_const_block_still_runs() {
    // The other side of the boundary: the literal is its own body, so its
    // `return` is an ordinary early exit and the block evaluates fine.
    check_const_blocks(
        "static f = fn { let x: usize = const { let inner = fn () -> usize { return 7; }; 9 }; };",
        expect![[r#"
            f#0 = 9
        "#]],
    );
}
