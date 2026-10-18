//! Const evaluation: items, `const fn`, const blocks and cycles.

use super::*;

#[test]
fn arithmetic_and_literals_const_evaluate() {
    check_const(
        r#"
static example: usize = 4 + 5;
static greeting = "hi";
static truth = example < 20;
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
    // `double` carries the `const fn` marker: only const fns are callable
    // in an initializer, and this test is about the call *working*.
    check_const(
        r#"
static double = const fn (n: usize) -> usize { n * 2 }
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
    check_const(
        "static f = fn { print(\"hi\"); };",
        expect![[r#"
        f = fn
    "#]],
    );
}

#[test]
fn print_is_refused_at_compile_time() {
    // The const-check trap fires with the editor's exact message; the
    // machine's own dynamic refusal (`NotConst`) stays behind it as
    // defense in depth.
    check_const(
        r#"static x = print("hi");"#,
        expect![[r#"
            x = error[Trap]: cannot call `print` in a const context; const evaluation cannot have side effects
        "#]],
    );
}

#[test]
fn panic_in_const_is_an_error() {
    check_const(
        r#"static x: usize = panic("boom");"#,
        expect![[r#"
        x = error[Panic]: boom
    "#]],
    );
}

#[test]
fn division_by_zero_and_overflow_are_runtime_errors() {
    check_const(
        r#"
static div: usize = 1 / 0;
static sub: usize = 0 - 1;
"#,
        expect![[r#"
            div = error[Runtime]: attempt to divide by zero
            sub = error[Runtime]: arithmetic overflow: `0 - 1` does not fit in `usize`
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
    // `rec` is `const fn` so the calls pass const-check — the subject here
    // is the frame limit, which needs the recursion to actually run.
    check_const(
        r#"
static rec: fn() -> usize = const fn { rec() };
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
    // `g` is `const fn` so the call passes const-check — the subject here
    // is the `!` mismatch trap, which needs the call to be otherwise fine.
    check_const(
        r#"
static g = const fn () -> usize { 1 }
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
    check_const(
        "static x = missing;",
        expect![[r#"
        x = error[Trap]: unresolved name `missing`
    "#]],
    );
}

#[test]
fn static_initializer_calling_a_plain_fn_traps_at_const_eval() {
    // The initializer is a const context and `double` is a plain fn: const
    // evaluation crashes at the trap with exactly the message the editor
    // shows as a squiggle.
    check_const(
        r#"
static double = fn (n: usize) -> usize { n * 2 }
static x = double(2);
"#,
        expect![[r#"
            double = fn
            x = error[Trap]: cannot call `double` in a const context; marking it `const fn` would allow this
        "#]],
    );
}

#[test]
fn run_mode_reaching_an_illegal_call_in_a_const_fn_body_traps() {
    // A `const fn` body is a const context under every execution: run-mode
    // code calling `apply` runs up to the illegal value call inside it,
    // then crashes with the editor's message.
    check_run(
        r#"
static apply = const fn (f: fn() -> usize) -> usize { f() };
static main = fn {
    print("before");
    apply(fn () -> usize { 1 });
};
"#,
        "main()",
        expect![[r#"
            output: "before"
            error[Trap]: cannot call a value in a const context; whether it is a `const fn` is not known from its type
        "#]],
    );
}

#[test]
fn illegal_call_in_an_unevaluated_branch_does_not_crash_const_eval() {
    // The dead branch's call is squiggled (tested in hir) and trapped, but
    // only the evaluated culprit branch traps: with the condition true the
    // initializer const-evaluates to completion.
    check_const(
        r#"
static f = fn () -> usize { 1 }
static x: usize = if true { 5 } else { f() };
"#,
        expect![[r#"
            f = fn
            x = 5
        "#]],
    );
}

#[test]
fn const_block_evaluates_to_its_inner_value() {
    // In run mode the forced compile-time value flows in where the block
    // sits, indistinguishable from evaluating it inline.
    check_run(
        r#"
static main = fn {
    let y: usize = const { 2 + 3 };
    y
}
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn const_blocks_memoize_within_a_machine_run() {
    // `f` runs twice, but its `const { … }` body executes once per machine
    // run: the second hit is served from the per-run memo.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        r#"
static f = fn () -> usize { const { 2 + 3 } }
static entrypoint = (f() + f());
"#
        .to_owned(),
    );
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    assert_eq!(result, Ok(crate::Value::Int(hir::IntValue::Usize(10))));
    assert_eq!(machine.const_block_evaluations(), 1);
}

#[test]
fn const_block_in_a_never_called_fn_fails_at_check_time() {
    // Nothing calls the function, but the `const` block inside it is
    // compile-time code: the panic surfaces from `const_block_values`.
    check_const_blocks(
        r#"static f = fn { const { panic("boom") }; };"#,
        expect![[r#"
            f#0 = error[Panic]: boom
        "#]],
    );
}

#[test]
fn nested_const_blocks_evaluate_inside_out() {
    check_const_blocks(
        r#"static f = fn () -> usize { const { const { 2 } + 3 } };"#,
        expect![[r#"
            f#0 = 2
            f#1 = 5
        "#]],
    );
}

#[test]
fn const_block_forces_items_and_detects_cycles() {
    // `ok`'s block forces the `base` static like any other use; `cyc`'s
    // block forces `cyc` itself, whose initializer re-enters the very block
    // being forced — the cycle is detected, not an infinite regress.
    check_const_blocks(
        r#"
static base: usize = 2 + 3;
static ok = fn () -> usize { const { base } };
static cyc: usize = const { cyc };
"#,
        expect![[r#"
            ok#0 = 5
            cyc#0 = error[NotConst]: cycle detected while evaluating a `const` block in `cyc`
        "#]],
    );
}
