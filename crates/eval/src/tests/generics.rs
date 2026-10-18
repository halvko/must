//! Generic instantiation at run time and compile time.

use super::*;

// ---- generics: instances actually run ----

#[test]
fn generic_const_fn_instantiates_and_const_evaluates() {
    // The arc's accumulator test: a `const fn` with a const param,
    // instantiated in an initializer, EVALUATES at check time — the value
    // of `N` rides the instance (`FnValue.const_args`), resolved by the
    // frame executing the one shared MIR body.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<3>(14);",
        expect![[r#"
            rep = fn
            y = 42
        "#]],
    );
}

#[test]
fn generic_plain_fn_runs_in_run_mode() {
    // A PLAIN generic fn called as runtime code: the entry initializer is
    // the runner's one const-context escape, so the call proceeds at
    // const depth 0 like any non-const call.
    check_run(
        "static rep = fn::<const N: usize>(x: usize) -> usize { x * N };",
        "rep::<3>(14)",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn type_param_generic_runs_end_to_end() {
    // Type args need nothing at runtime (TR06): the explicit and the
    // inferred mention run the same item value.
    check_run(
        "static id = fn::<T>(x: T) -> T { x };",
        "id::<usize>(4)",
        expect![[r#"
            => 4
        "#]],
    );
    check_run(
        "static id = fn::<T>(x: T) -> T { x };",
        r#"id("s")"#,
        expect![[r#"
            => "s"
        "#]],
    );
}

#[test]
fn mixed_binder_instantiates() {
    // `fn::<T: forget, const N: usize>`: the type param claims no runtime slot —
    // `N` is dense const index 0 even though its binder index is 1.
    check_run(
        "static tag = fn::<T: forget, const N: usize>(x: T) -> usize { N };",
        r#"tag::<str, 7>("s")"#,
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn const_param_driven_recursion_terminates() {
    // Recursion in a generic fn: each level re-instantiates the scheme
    // (`count::<const N>` forwards the frame's own value), the runtime
    // argument does the counting — well within the fuel budget.
    check_const(
        "static count = const fn::<const N: usize>(x: usize) -> usize { if x < N { count::<const N>(x + 1) } else { x } };\nstatic y = count::<3>(0);",
        expect![[r#"
            count = fn
            y = 3
        "#]],
    );
}

#[test]
fn const_arg_may_reference_a_const_item() {
    // TR06's ruled spelling: a non-literal const argument is written with
    // the `const` prefix — `rep::<const LEN>` reads the const item.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nconst LEN: usize = 3;\nstatic y = rep::<const LEN>(14);",
        expect![[r#"
            rep = fn
            LEN = 3
            y = 42
        "#]],
    );
}

#[test]
fn generic_calling_generic_forwards_the_const_param() {
    // The key composition case: `rep2`'s const argument `const N` is a
    // compile-time body reading the ENCLOSING frame's const param —
    // forced per instance at the mention, passing the value through to
    // `rep`'s instance.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };\nstatic y = rep2::<3>(14);",
        expect![[r#"
            rep = fn
            rep2 = fn
            y = 42
        "#]],
    );
}

#[test]
fn braced_const_arg_evaluates_to_a_value() {
    // Behavior floor: a `const { ... }` const argument is a compile-time
    // body that reaches evaluation and produces the right `Value` — here the
    // block computes `N = 40 + 2`, and the instance runs `x * N`.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<const { 40 + 2 }>(2);",
        expect![[r#"
            rep = fn
            y = 84
        "#]],
    );
}

#[test]
fn const_block_in_a_generic_body_is_per_instance() {
    // The same `const { … }` body under two instantiations is two values:
    // the machine's compile-time memo is keyed by `Instance` (item + const
    // args), not by body alone.
    check_const(
        "static f = const fn::<const N: usize>() -> usize { const { N + 1 } };\nstatic a = f::<1>();\nstatic b = f::<2>();",
        expect![[r#"
            f = fn
            a = 2
            b = 3
        "#]],
    );
}

#[test]
fn const_block_reading_a_const_param_is_uninstantiated_at_check_time() {
    // Forced standalone (no instance), a compile-time body inside a
    // generic item reports the distinguished `Uninstantiated` kind — the
    // diagnostics layer skips it (the value simply isn't knowable
    // pre-instantiation, TR06); instantiated executions never produce it.
    check_const_blocks(
        "static f = const fn::<const N: usize>() -> usize { const { N } };",
        expect![[r#"
            f#0 = error[Uninstantiated]: the value of a const parameter is not known before instantiation
        "#]],
    );
}

#[test]
fn const_arg_panic_fails_the_instantiating_item() {
    // A panicking const argument fails the mention's evaluation — the
    // check-time surface (`const_arg_values`, exercised in the ide loop)
    // and the item's own forcing report the same origin.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<const { panic(\"nope\") }>(14);",
        expect![[r#"
            rep = fn
            y = error[Panic]: nope
        "#]],
    );
}

#[test]
fn nested_fn_literal_inherits_the_const_env() {
    // A plain fn literal nested in a generic body reads the binder's
    // const param: the fn value captures the frame's const args at
    // construction, so the value survives being returned and called from
    // non-generic code.
    check_run(
        "static make = fn::<const N: usize>() -> fn() -> usize { fn () -> usize { N } };",
        "make::<9>()()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn generic_frame_shows_const_params_as_named_locals() {
    // The debugger surface: a frame executing a generic instance lists
    // the binder's const params ahead of its locals.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static rep = fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic entry = (rep::<3>(14));".to_owned(),
    );
    let entry = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entry")
        .expect("entry item");
    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    machine
        .start(&hir::item_loc(&db, entry))
        .expect("entry starts");
    while machine.frames().len() < 2 {
        match machine.step().expect("no crash before the call") {
            crate::StepEvent::Progress => {}
            crate::StepEvent::Done(_) => panic!("finished without entering `rep`"),
        }
    }
    let locals = machine.frame_named_locals(1);
    assert_eq!(locals[0].0, "N");
    assert_eq!(locals[0].2, crate::Value::Int(hir::IntValue::Usize(3)));
    assert!(
        locals.iter().any(|(name, _, _)| name == "x"),
        "the ordinary param is still listed: {locals:?}"
    );
}

// ---- generic type declarations ----

#[test]
fn generic_record_constructs_and_projects() {
    check_run(
        r#"
type Pair = struct::<T> { a: T, b: T };
static main = fn () -> usize { let p = Pair::<usize>(struct { a = 1, b = 2 }); p.a + p.b };
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn generic_enum_matches_and_reassigns() {
    check_run(
        r#"
type Option = enum::<T> { Some(T), None };
static unwrap_or = fn (o: Option::<usize>, d: usize) -> usize {
    match o { ::Some(x) => x, ::None => d, }
};
static main = fn () -> usize {
    let mut o = Option::<usize>::Some(3);
    let first = unwrap_or(o, 0);
    o = Option::<usize>::None;
    first + unwrap_or(o, 10)
};
"#,
        "main()",
        expect![[r#"
            => 13
        "#]],
    );
}

#[test]
fn generic_variant_value_renders() {
    check_run(
        r#"
type Option = enum::<T> { Some(T), None };
static main = fn () -> Option::<usize> { Option::Some(3) };
"#,
        "main()",
        expect![[r#"
            => Option::Some(3)
        "#]],
    );
}

#[test]
fn const_param_type_constructs_and_evaluates() {
    check_run(
        r#"
type Buf = struct::<const N: usize> { len: usize };
static main = fn () -> usize { let b: Buf::<8> = Buf::<8>(struct { len = 3 }); b.len };
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}
