use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

use crate::machine::{Machine, RunMode};

/// Renders `const_value` for every item in the fixture.
fn check_const(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        rendered.push_str(&match crate::const_value(&db, item) {
            Ok(value) => format!("{name} = {}\n", value.display()),
            Err(err) => format!("{name} = error[{:?}]: {}\n", err.kind, err.message),
        });
    }
    expect.assert_eq(&rendered);
}

/// Appends `static entrypoint = (<entry>);` to the fixture and runs it with
/// run-mode semantics (`print` is legal at the entry, statics still force as
/// consts). Renders captured output, then the result.
fn check_run(text: &str, entry: &str, expect: Expect) {
    let db = RootDatabase::default();
    let full = format!("{text}\nstatic entrypoint = ({entry});\n");
    let file = SourceFile::new(&db, "test.must".to_owned(), full);
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(&db, RunMode { out: Vec::new() });
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    let mut rendered = String::from_utf8(machine.mode.out).unwrap();
    rendered.push_str(&match result {
        Ok(value) => format!("=> {}\n", value.display()),
        Err(err) => format!("error[{:?}]: {}\n", err.kind, err.message),
    });
    expect.assert_eq(&rendered);
}

#[test]
fn arithmetic_and_literals_const_evaluate() {
    check_const(
        r#"
static example = 4 + 5;
static greeting = "hi";
static truth = 1 < 2;
"#,
        expect![[r#"
            example = 9
            greeting = "hi"
            truth = true
        "#]],
    );
}

#[test]
fn if_and_calls_const_evaluate() {
    check_const(
        r#"
static double = fn (n: usize) -> usize { n * 2 }
static pick: usize = if true { double(21) } else { 0 };
static chained = pick + 1;
"#,
        expect![[r#"
            double = fn
            pick = 42
            chained = 43
        "#]],
    );
}

#[test]
fn fn_items_are_fn_values() {
    check_const("static f = fn { print(\"hi\"); };", expect![[r#"
        f = fn
    "#]]);
}

#[test]
fn print_is_refused_at_compile_time() {
    check_const(r#"static x = print("hi");"#, expect![[r#"
        x = error[NotConst]: cannot call `print` at compile time
    "#]]);
}

#[test]
fn panic_in_const_is_an_error() {
    check_const(r#"static x: usize = panic("boom");"#, expect![[r#"
        x = error[Panic]: boom
    "#]]);
}

#[test]
fn division_by_zero_and_overflow_are_runtime_errors() {
    check_const(
        r#"
static div = 1 / 0;
static sub = 0 - 1;
"#,
        expect![[r#"
            div = error[Runtime]: attempt to divide by zero
            sub = error[Runtime]: attempt to subtract with overflow
        "#]],
    );
}

#[test]
fn const_cycles_are_detected() {
    check_const(
        r#"
static a: usize = b;
static b: usize = a;
"#,
        expect![[r#"
            a = error[NotConst]: cycle detected while evaluating `a`
            b = error[NotConst]: cycle detected while evaluating `b`
        "#]],
    );
}

#[test]
fn runaway_recursion_hits_the_frame_limit() {
    check_const(
        r#"
static rec: fn() -> usize = fn { rec() };
static r: usize = rec();
"#,
        expect![[r#"
            rec = fn
            r = error[NotConst]: stack overflow: recursion exceeded 10000 frames
        "#]],
    );
}

#[test]
fn never_annotated_call_traps_with_the_mismatch_not_an_internal_error() {
    check_const(
        r#"
static g = fn () -> usize { 1 }
static f: ! = g();
"#,
        expect![[r#"
            g = fn
            f = error[Trap]: type mismatch: expected `!`, found `usize`
        "#]],
    );
}

#[test]
fn run_mode_recursion_overflow_is_a_runtime_error() {
    check_run(
        r#"
static rec = fn (n: usize) -> usize { rec(n + 1) }
"#,
        "rec(0)",
        expect![[r#"
            error[Runtime]: stack overflow: recursion exceeded 10000 frames
        "#]],
    );
}

#[test]
fn reaching_a_trap_reports_the_borrowed_diagnostic() {
    check_const("static x = missing;", expect![[r#"
        x = error[Trap]: unresolved name `missing`
    "#]]);
}

#[test]
fn run_hello() {
    check_run(
        r#"static main = fn { print("hello"); };"#,
        "main()",
        expect![[r#"
            hello
            => ()
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
    // implicit `const { … }` whichever driver evaluates it.
    check_run(
        r#"static x: () = print("side effect");"#,
        "x",
        expect![[r#"
            error[NotConst]: cannot call `print` at compile time
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
            before
            error[Trap]: type mismatch: expected `usize`, found `str`
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
