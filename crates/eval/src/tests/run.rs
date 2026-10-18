//! Run mode: printing, recursion and branches.

use super::*;

#[test]
fn run_hello() {
    check_run(
        r#"static main = fn { print("hello"); };"#,
        "main()",
        expect![[r#"
            output: "hello"
            => ()
        "#]],
    );
}

#[test]
fn print_appends_no_newline() {
    // P03, pinned at the only place it is observable: two `print`s land
    // on one line, because `print` writes its argument and nothing else.
    check_run(
        r#"static main = fn { print("a"); print("b"); };"#,
        "main()",
        expect![[r#"
            output: "ab"
            => ()
        "#]],
    );
}

#[test]
fn a_program_writes_its_own_line_breaks() {
    // With no auto-newline, `\n` is the only way to end a line — which is
    // exactly why the escape set exists.
    check_run(
        r#"static main = fn { print("a\n"); print("b\n"); };"#,
        "main()",
        expect![[r#"
            output: "a\nb\n"
            => ()
        "#]],
    );
}

#[test]
fn every_escape_reaches_the_value() {
    // Each escape, cooked once during HIR lowering, observed as the
    // string's runtime bytes, all six in one literal. `\0` is a NUL, not
    // the character `0`.
    check_const(
        r#"static all = "\n\t\r\\\"\0";"#,
        expect![[r#"
            all = "\n\t\r\\\"\0"
        "#]],
    );
}

#[test]
fn run_recursive_fib() {
    check_run(
        r#"
static fib = fn (n: usize) -> usize {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
"#,
        "fib(10)",
        expect![[r#"
            => 55
        "#]],
    );
}

#[test]
fn statics_are_const_contexts_even_in_run_mode() {
    // `print` is fine at the entry, but the static's initializer is an
    // implicit `const { … }` whichever driver evaluates it: forcing `x`
    // fires the const-check trap with the editor's message.
    check_run(
        r#"static x: () = print("side effect");"#,
        "x",
        expect![[r#"
            error[Trap]: cannot call `print` in a const context; const evaluation cannot have side effects
        "#]],
    );
}

#[test]
fn deferred_type_errors_crash_at_the_trap_not_before() {
    // Code before the broken line runs; the crash carries the same message
    // the editor shows as a diagnostic.
    check_run(
        r#"
static main = fn {
    print("before");
    let v: usize = "s";
    print("after");
};
"#,
        "main()",
        expect![[r#"
            output: "before"
            error[Trap]: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn only_the_evaluated_culprit_branch_traps() {
    // Both string branches carry type-error traps, but the `0` path is
    // fine: with n != 0 neither wrong branch is reached, so the partial
    // program runs to completion.
    let program = r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    constrainer("", x);
}
"#;
    check_run(
        program,
        "f(1)",
        expect![[r#"
            => ()
        "#]],
    );
    // With n == 0 the outer wrong branch is evaluated and traps with
    // exactly the diagnostic the editor shows.
    check_run(
        program,
        "f(0)",
        expect![[r#"
            error[Trap]: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn let_hole_pattern_runs_initializer_for_its_side_effects() {
    // The value is discarded, but `print` still runs.
    check_run(
        r#"
static main = fn {
    let _ = print("side effect");
}
"#,
        "main()",
        expect![[r#"
            output: "side effect"
            => ()
        "#]],
    );
}

#[test]
fn run_if_else_chain() {
    check_run(
        r#"
static classify = fn (n: usize) -> str {
    if n == 0 { "zero" } else if n < 10 { "small" } else { "big" }
}
"#,
        r#"classify(5)"#,
        expect![[r#"
            => "small"
        "#]],
    );
}
