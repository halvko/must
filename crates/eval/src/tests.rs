use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

use crate::machine::{Machine, RunMode, StepEvent};
use crate::{EvalErrorKind, Value};

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

/// Renders `const_block_values` for every item in the fixture: each
/// `const { … }` block's check-time result, in lowering order (inner blocks
/// before the blocks enclosing them), indexed per item.
fn check_const_blocks(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        for (i, (_, result)) in crate::const_block_values(&db, item).iter().enumerate() {
            rendered.push_str(&match result {
                Ok(value) => format!("{name}#{i} = {}\n", value.display()),
                Err(err) => format!("{name}#{i} = error[{:?}]: {}\n", err.kind, err.message),
            });
        }
    }
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
            before
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
            side effect
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

/// Finds an item by name in the fixture.
fn item<'db>(db: &'db RootDatabase, file: SourceFile, name: &str) -> hir::ItemId<'db> {
    *hir::file_item_ids(db, file)
        .iter()
        .find(|&&it| it.name(db) == name)
        .unwrap_or_else(|| panic!("no item `{name}`"))
}

/// Pins the single-step surface: `start` pushes one frame, `step` advances
/// one statement or terminator, `frame_origin`/`frame_named_locals` stay
/// inspectable between steps (past the last statement they report the
/// terminator's origin), and a step that const-forces an item swaps in and
/// out without leaving a trace in the frame list.
#[test]
fn single_step_surface_walks_frames_across_calls_and_const_forcing() {
    let db = RootDatabase::default();
    let text = r#"
static helper = fn (n: usize) -> usize { n + 1 }
static main = fn (n: usize) -> usize {
    let doubled = helper(n) * 2;
    doubled
}
static entrypoint = (main(20));
"#;
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let (entry, main, helper) = (
        item(&db, file, "entrypoint"),
        item(&db, file, "main"),
        item(&db, file, "helper"),
    );
    let main_body = hir::body::body(&db, main);
    let helper_body = hir::body::body(&db, helper);
    let entry_body = hir::body::body(&db, entry);
    // Origins named through the HIR: main's `helper(n)` call, helper's
    // `n + 1` tail, and the fn-literal blocks (the `return` terminators'
    // origin).
    let hir::body::ExprData::FnLiteral {
        body: main_fn_block,
        ..
    } = &main_body.exprs[main_body.root.unwrap()]
    else {
        panic!("main is a fn literal");
    };
    let hir::body::ExprData::Block { stmts, .. } = &main_body.exprs[*main_fn_block] else {
        panic!("a fn literal's body is a block");
    };
    let hir::body::Stmt::Let { init, .. } = &stmts[0] else {
        panic!("main starts with a let");
    };
    let hir::body::ExprData::Bin { lhs: main_call, .. } = &main_body.exprs[*init] else {
        panic!("the let's initializer is a product");
    };
    let hir::body::ExprData::FnLiteral {
        body: helper_fn_block,
        ..
    } = &helper_body.exprs[helper_body.root.unwrap()]
    else {
        panic!("helper is a fn literal");
    };
    let hir::body::ExprData::Block {
        tail: Some(helper_add),
        ..
    } = &helper_body.exprs[*helper_fn_block]
    else {
        panic!("helper's body is a block with a tail");
    };

    let mut machine = Machine::new(&db, RunMode { out: Vec::new() });
    machine.start(&hir::item_loc(&db, entry)).unwrap();
    assert_eq!(machine.frames().len(), 1);

    // The entry's call terminator: `main`'s initializer const-forces inside
    // the step; only the run-mode frame appears.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 2);
    assert_eq!(
        machine.frame_named_locals(1),
        vec![("n".to_owned(), hir::Ty::Int, Value::Int(20))]
    );
    assert_eq!(
        machine.frame_origin(1),
        Some((hir::item_loc(&db, main), *main_call))
    );

    // Main's call terminator: `helper`'s initializer const-forces inside the
    // step, and helper's run frame is pushed on top.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 3);
    assert_eq!(
        machine.frame_origin(2),
        Some((hir::item_loc(&db, helper), *helper_add))
    );

    // Helper's two statements, then the past-the-end origin: the `return`
    // terminator's, which the next step executes (popping the frame).
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(
        machine.frame_origin(2),
        Some((hir::item_loc(&db, helper), *helper_fn_block))
    );
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 2);

    // Main's product and let: `doubled` shows up once it holds a value.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(
        machine.frame_named_locals(1),
        vec![
            ("n".to_owned(), hir::Ty::Int, Value::Int(20)),
            ("doubled".to_owned(), hir::Ty::Int, Value::Int(42))
        ]
    );
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // ret = doubled
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // main returns
    assert_eq!(machine.frames().len(), 1);

    // The entry body has only compiler temps: no named locals.
    assert!(machine.frame_named_locals(0).is_empty());
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // ret = call
    assert_eq!(
        machine.frame_origin(0),
        Some((hir::item_loc(&db, entry), entry_body.root.unwrap()))
    );
    match machine.step() {
        Ok(StepEvent::Done(value)) => assert_eq!(value, Value::Int(42)),
        Ok(StepEvent::Progress) | Err(_) => panic!("expected Done(42)"),
    }
    assert!(machine.frames().is_empty());
}

/// A step that errors leaves the frames exactly as they were, so a debugger
/// can inspect the crash site; the top frame's origin is the panicking call.
#[test]
fn step_errors_leave_frames_intact_for_inspection() {
    let db = RootDatabase::default();
    let text = r#"
static main = fn {
    print("before");
    panic("boom");
}
static entrypoint = (main());
"#;
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let (entry, main) = (item(&db, file, "entrypoint"), item(&db, file, "main"));
    let main_body = hir::body::body(&db, main);
    let hir::body::ExprData::FnLiteral {
        body: main_fn_block,
        ..
    } = &main_body.exprs[main_body.root.unwrap()]
    else {
        panic!("main is a fn literal");
    };
    let hir::body::ExprData::Block { stmts, .. } = &main_body.exprs[*main_fn_block] else {
        panic!("a fn literal's body is a block");
    };
    let hir::body::Stmt::Expr(panic_call) = &stmts[1] else {
        panic!("the second statement is the panic");
    };

    let mut machine = Machine::new(&db, RunMode { out: Vec::new() });
    machine.start(&hir::item_loc(&db, entry)).unwrap();
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // the entry's call
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // print("before")

    let err = match machine.step() {
        Err(err) => err,
        Ok(_) => panic!("expected the panic to stop the machine"),
    };
    assert_eq!(err.kind, EvalErrorKind::Panic);
    assert_eq!(err.message, "boom");
    assert_eq!(machine.frames().len(), 2);
    assert_eq!(
        machine.frame_origin(1),
        Some((hir::item_loc(&db, main), *panic_call))
    );
    assert_eq!(
        machine.frame_origin(0).map(|(loc, _)| loc),
        Some(hir::item_loc(&db, entry))
    );
}

/// A chain of *distinct* items forcing each other is not a cycle: it is the
/// forcing-depth cap that stops it (each level recurses on the Rust stack).
#[test]
fn distinct_item_chains_hit_the_forcing_depth_cap() {
    let db = RootDatabase::default();
    // Annotated so hir's cross-item inference (which gives up earlier) does
    // not trap first; the chain below is pure const forcing.
    let mut text = String::from("static s0: usize = 1;\n");
    for i in 1..130 {
        text.push_str(&format!("static s{i}: usize = s{};\n", i - 1));
    }
    let file = SourceFile::new(&db, "test.must".to_owned(), text);
    let top = item(&db, file, "s129");
    let Err(err) = crate::const_value(&db, top) else {
        panic!("a 130-item chain must not const-evaluate");
    };
    assert_eq!(err.kind, EvalErrorKind::NotConst);
    assert_eq!(
        err.message.as_str(),
        "constant evaluation exceeded 128 nested items"
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
            before
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
    let y = const { 2 + 3 };
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
    let mut machine = Machine::new(&db, RunMode { out: Vec::new() });
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    assert_eq!(result, Ok(crate::Value::Int(10)));
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

#[test]
fn mutation_of_a_let_binding_is_visible_to_later_reads() {
    check_run(
        "",
        "(fn { let mut x = 1; x = x + 2; x })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn mutating_a_mut_param_is_visible_to_later_reads() {
    check_run(
        "static f = fn (mut n: usize) -> usize { n = n + 1; n };",
        "f(41)",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn assigning_to_an_immutable_binding_traps_at_runtime() {
    // The executed path runs into the assignment: it crashes with exactly
    // the message the editor shows as a squiggle.
    check_run(
        "",
        "(fn { let x = 1; x = 2; x })()",
        expect![[r#"
            error[Trap]: cannot assign to `x`: it is not declared `mut`
        "#]],
    );
}

#[test]
fn illegal_assignment_in_an_unevaluated_branch_does_not_crash() {
    // The dead branch's assignment is squiggled (tested in hir) and
    // trapped, but only the evaluated path crashes: with the condition
    // true the function returns normally.
    check_run(
        "",
        "(fn () -> usize { let x = 1; if true { x } else { x = 2; x } })()",
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn mutation_inside_a_const_fn_body_works_at_compile_time() {
    // `double` mutates a local of its own body; forcing `x` through it at
    // check time exercises mutation in a const context end to end.
    check_const(
        r#"
static double = const fn (n: usize) -> usize {
    let mut r = n;
    r = r + r;
    r
};
static x = double(21);
"#,
        expect![[r#"
            double = fn
            x = 42
        "#]],
    );
}

#[test]
fn record_construction_and_field_access() {
    check_run(
        "",
        "(fn { let p = struct { x: 1, y: 2 }; p.x + p.y })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn nested_records_construct_and_project() {
    check_run(
        "",
        r#"(fn { let a = struct { b: struct { c: 5 } }; a.b.c })()"#,
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn records_const_evaluate() {
    check_const(
        r#"
static p = struct { x: 1, y: 2 };
static sum = p.x + p.y;
"#,
        expect![[r#"
            p = { x: 1, y: 2 }
            sum = 3
        "#]],
    );
}

#[test]
fn shorthand_fields_evaluate() {
    check_run(
        "",
        r#"(fn { let x = 5; let y = 6; let p = struct { x, y }; p.x + p.y })()"#,
        expect![[r#"
            => 11
        "#]],
    );
}

#[test]
fn record_equality_is_structural_both_ways() {
    check_run(
        "",
        r#"(struct { x: 1 } == struct { x: 1 })"#,
        expect![[r#"
            => true
        "#]],
    );
    check_run(
        "",
        r#"(struct { x: 1 } == struct { x: 2 })"#,
        expect![[r#"
            => false
        "#]],
    );
    check_run(
        "",
        r#"(struct { x: 1 } != struct { x: 2 })"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn field_access_on_a_nonexistent_field_still_traps_at_runtime() {
    // Records have a real MIR/eval story now, but a field inference
    // rejected is exactly as trapped as before records had an eval story
    // (the earlier message, unchanged).
    check_run(
        "",
        r#"(fn { let p = struct { x: 1 }; p.y })()"#,
        expect![[r#"
            error[Trap]: no field `y` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn named_type_construction_erases_to_its_record() {
    // Full erasure: `Foo(v)` is `v` at runtime — the const value of a
    // Foo-typed item is a plain record value. Forcing the `type` item
    // itself (this helper forces every item) yields a Trap-kind error,
    // the kind `ide` never surfaces as a diagnostic: a type has no value,
    // and reads of `Foo` already trap with their own type-not-a-value
    // message.
    check_const(
        r#"
type Foo = struct { x: usize, y: str };
static p = Foo(struct { x: 1, y: "s" });
static x = p.x;
"#,
        expect![[r#"
            Foo = error[Trap]: `Foo` has no value
            p = { x: 1, y: "s" }
            x = 1
        "#]],
    );
}

#[test]
fn named_type_equality_is_structural_under_the_hood() {
    // The type system keeps `Foo` and bare records apart; between two
    // `Foo`s, equality is the underlying records' structural equality.
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x: 1 }) == Foo(struct { x: 1 }))"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn named_type_inequality_observes_field_values() {
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x: 1 }) == Foo(struct { x: 2 }))"#,
        expect![[r#"
            => false
        "#]],
    );
}

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
    let mut i = 0;
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
    let mut total = 0;
    let mut row = 0;
    loop {
        if row == 3 { break total; };
        let mut col = 0;
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
