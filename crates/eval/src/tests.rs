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
        vec![("n".to_owned(), Value::Int(20))]
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
            ("n".to_owned(), Value::Int(20)),
            ("doubled".to_owned(), Value::Int(42))
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
