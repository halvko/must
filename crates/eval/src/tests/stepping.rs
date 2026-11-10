//! The single-step machine surface the debugger drives.

use super::*;

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
    // Origins named through the HIR: main's `helper(n)` call and its
    // argument, helper's `n + 1` tail and its left operand, and the
    // fn-literal blocks (the `return` terminators' origin).
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
    let hir::body::Stmt::Let {
        init: Some(init), ..
    } = &stmts[0]
    else {
        panic!("main starts with a let");
    };
    let hir::body::ExprData::Bin { lhs: main_call, .. } = &main_body.exprs[*init] else {
        panic!("the let's initializer is a product");
    };
    let hir::body::ExprData::Call { args, .. } = &main_body.exprs[*main_call] else {
        panic!("the product's left operand is a call");
    };
    let main_arg = args[0];
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
    let hir::body::ExprData::Bin { lhs: helper_n, .. } = &helper_body.exprs[*helper_add] else {
        panic!("helper's tail is a sum");
    };

    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    machine.start(&hir::item_loc(&db, entry)).unwrap();
    assert_eq!(machine.frames().len(), 1);

    // The entry's call terminator: `main`'s initializer const-forces inside
    // the step; only the run-mode frame appears.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 2);
    assert_eq!(
        machine.frame_named_locals(1),
        vec![(
            "n".to_owned(),
            hir::Ty::Int(hir::IntKind::Usize),
            Value::Int(hir::IntValue::Usize(20)),
        )]
    );
    assert_eq!(
        machine.frame_origin(1),
        Some((hir::item_loc(&db, main), main_arg))
    );

    // The read of the argument `n`, which leaves the call next.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
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
        Some((hir::item_loc(&db, helper), *helper_n))
    );

    // The read of `n`, which leaves the sum next.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
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
            (
                "n".to_owned(),
                hir::Ty::Int(hir::IntKind::Usize),
                Value::Int(hir::IntValue::Usize(20)),
            ),
            (
                "doubled".to_owned(),
                hir::Ty::Int(hir::IntKind::Usize),
                Value::Int(hir::IntValue::Usize(42)),
            ),
        ]
    );
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // read doubled
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // ret = the read
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
        Ok(StepEvent::Done(value)) => assert_eq!(value, Value::Int(hir::IntValue::Usize(42))),
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

    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
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
/// Runs on a dedicated thread sized like the server's pool workers (8 MiB,
/// see `must-lsp`'s `pool.rs`): the test asserts that the *cap* fires, and
/// the per-level frames (origin tracking, variant values) need more
/// headroom than libtest's default thread gives.
#[test]
fn distinct_item_chains_hit_the_forcing_depth_cap() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
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
        })
        .unwrap()
        .join()
        .unwrap()
}
