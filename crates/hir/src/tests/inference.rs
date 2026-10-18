//! Inference basics: annotations, firewalls, binding groups, holes and divergence.

use super::*;

#[test]
fn infer_hello() {
    check_infer(
        r#"
static main = fn {
    let s = "hello";
    print(s);
}
"#,
        expect![[r#"
            15..56 'fn {     let s = ...': fn()
            18..56 '{     let s = "he...': ()
            28..29 's': str
            32..39 '"hello"': str
            45..50 'print': fn(str)
            45..53 'print(s)': ()
            51..52 's': str
        "#]],
    );
}

#[test]
fn infer_block_bodied_fn_literal_arg() {
    check_infer(
        r#"
static example = fn (arg: fn() -> usize) -> usize { arg() }
static main = fn { example(fn { 42 + 69 }); }
"#,
        expect![[r#"
            18..60 'fn (arg: fn() -> ...': fn(fn() -> usize) -> usize
            22..25 'arg': fn() -> usize
            51..60 '{ arg() }': usize
            53..56 'arg': fn() -> usize
            53..58 'arg()': usize
            75..106 'fn { example(fn {...': fn()
            78..106 '{ example(fn { 42...': ()
            80..87 'example': fn(fn() -> usize) -> usize
            80..103 'example(fn { 42 +...': usize
            88..102 'fn { 42 + 69 }': fn() -> usize
            91..102 '{ 42 + 69 }': usize
            93..95 '42': usize
            93..100 '42 + 69': usize
            98..100 '69': usize
        "#]],
    );
}

#[test]
fn type_mismatch_on_annotation() {
    check_diagnostics(
        r#"static x: usize = "hello";"#,
        expect![[r#"
            18..25: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 10..15)
        "#]],
    );
}

#[test]
fn type_mismatch_points_at_block_tail() {
    check_diagnostics(
        r#"static f = fn () -> usize { let s = "x"; s };"#,
        expect![[r#"
            41..42: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 17..25)
        "#]],
    );
}

#[test]
fn never_coerces_to_expected_type() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { panic("boom") };"#,
        expect![[r#""#]],
    );
}

#[test]
fn wrong_arg_count_and_not_callable() {
    check_diagnostics(
        // The return annotation matters: an unannotated return makes f's
        // signature `fn(usize) -> {error}`, and Error silences the
        // not-callable diagnostic on `f(1)(2)`.
        r#"
static f = fn (n: usize) -> usize { n }
static main = fn {
    f(1, 2);
    f(1)(2);
}
"#,
        expect![[r#"
            64..71: expected 1 argument(s), found 2 (`f` is defined here at 8..9)
            77..81: expression of type `usize` is not callable
        "#]],
    );
}

#[test]
fn binexpr_operands_must_be_int() {
    check_diagnostics(
        r#"static x = 1 + "two";"#,
        expect![[r#"
            15..20: type mismatch: expected `{number}`, found `str` (`+` requires `{number}` operands at 13..14)
        "#]],
    );
}

#[test]
fn unannotated_param_inferred_from_use() {
    check_infer(
        "static f = fn (s) { print(s) }",
        expect![[r#"
            11..30 'fn (s) { print(s) }': fn(str)
            15..16 's': str
            18..30 '{ print(s) }': ()
            20..25 'print': fn(str)
            20..28 'print(s)': ()
            26..27 's': str
        "#]],
    );
}

#[test]
fn non_block_fn_body_one_diagnostic_and_inference_still_works() {
    // Superset parsing: exactly one error (with a fix), no cascade…
    check_diagnostics(
        "static f = fn 42;",
        expect![[r#"
            14..16: function bodies are blocks; wrap this expression in `{ }`
            14..16: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
    // …and the tree keeps the intent: types are still inferred inside.
    check_infer(
        "static f = fn 42;",
        expect![[r#"
            11..16 'fn 42': fn() -> {number}
            14..16 '42': {number}
        "#]],
    );
}

/// The incrementality firewall: editing one item's body must not re-run
/// inference for other items (both annotated, so signatures can't change).
#[test]
fn firewall_body_edit_does_not_reinfer_other_items() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: fn() -> usize = fn () -> usize { 1 };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";
    // Only `a`'s body changes; everything name- and signature-level is
    // identical.
    let text_v2 = "static a: fn() -> usize = fn () -> usize { 1 + 1 };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let executed_infers = |log: &Mutex<Vec<String>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with("infer("))
            .count()
    };
    assert_eq!(executed_infers(&log), 2, "both items inferred initially");

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter()
            .filter(|entry| entry.starts_with("infer("))
            .count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

/// A fn-literal body that fully types itself (every param and an explicit
/// return) synthesizes the item's contract without a written annotation —
/// the item is a hard firewall edge and stays out of its caller's binding
/// group: editing its body must not re-run the shared group inference.
#[test]
fn self_sufficient_fn_literal_body_firewalls_its_caller() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a = fn () -> usize { 1 };\n\
                   static b = fn { a() };\n";
    // Only `a`'s body changes; the literal's written signature is identical,
    // so the synthesized contract compares equal and backdates.
    let text_v2 = "static a = fn () -> usize { 1 + 1 };\n\
                   static b = fn { a() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let executed_infers = |log: &Mutex<Vec<String>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.contains("infer"))
            .count()
    };
    assert!(
        executed_infers(&log) >= 2,
        "both items inferred initially; executed: {:?}",
        log.lock().unwrap()
    );

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    // Without the synthesized contract, `a` and `b` share a binding group
    // and editing `a`'s body re-runs `infer_group`.
    assert!(
        !log.iter().any(|entry| entry.contains("infer_group")),
        "a self-sufficient item must not share its caller's group: {log:#?}"
    );
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

/// `ItemLoc` carries name+disambiguator, not a positional index — so
/// inserting an unrelated item at the top of the file must not re-run
/// name resolution or inference of the items below it.
#[test]
fn firewall_item_insertion_does_not_reinfer_items_below() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: fn() -> usize = fn () -> usize { 1 };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";
    let text_v2 = "static zzz = 1;\n\
                   static a: fn() -> usize = fn () -> usize { 1 };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    // Re-demand a and b (skip the new first item).
    for &item in crate::file_item_ids(&db, file).iter().skip(1) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    // The file's inference groups read every item's names, the new one's
    // included; that's the new item's own work, not a re-run.
    let new_item = format!(
        "({:?})",
        salsa::plumbing::AsId::as_id(&crate::file_item_ids(&db, file)[0])
    );
    // `resolutions` does re-run — a new name entered the file scope, which
    // genuinely could matter — but its *value* is unchanged (ItemLocs are
    // name-based, not positional), so inference backdates behind it.
    for query in ["infer(", "expr_scopes"] {
        assert_eq!(
            log.iter()
                .filter(|entry| entry.starts_with(query) && !entry.ends_with(&new_item))
                .count(),
            0,
            "`{query}` should backdate across item insertion; executed: {log:#?}"
        );
    }
}

#[test]
fn fn_params_scoped_to_their_literal() {
    check_diagnostics(
        r#"
static f = fn {
    (fn (inner: usize) { inner })(1);
    print(inner);
}
"#,
        expect![[r#"
            65..70: unresolved name `inner`
        "#]],
    );
}

#[test]
fn a_duplicate_definition_is_diagnosed_at_every_one() {
    check_diagnostics(
        r#"
static name = 42 + 52;

static name = fn {
    let v = name;
}
"#,
        expect![[r#"
            8..12: `name` is defined multiple times (also defined here at 32..36)
            15..17: cannot infer the type of this number: it has no defining use — add a type annotation
            32..36: `name` is defined multiple times (also defined here at 8..12)
        "#]],
    );
}

#[test]
fn uses_of_a_duplicated_name_infer_as_error() {
    check_infer(
        r#"
static name = 1;
static name = fn {
    let v = name;
};
"#,
        expect![[r#"
            15..16 '1': {number}
            32..56 'fn {     let v = ...': fn()
            35..56 '{     let v = nam...': ()
            45..46 'v': {error}
            49..53 'name': {error}
        "#]],
    );
}

#[test]
fn local_shadow_of_a_duplicated_name_keeps_its_type() {
    check_infer(
        r#"
static name: usize = 1;
static name = fn {
    let name = "x";
    let n = name;
};
"#,
        expect![[r#"
            22..23 '1': usize
            39..83 'fn {     let name...': fn()
            42..83 '{     let name = ...': ()
            52..56 'name': str
            59..62 '"x"': str
            72..73 'n': str
            76..80 'name': str
        "#]],
    );
}

#[test]
fn cross_item_use_of_unannotated_item_infers() {
    // Interprocedural inference: `a`'s signature comes from its body, so
    // the use in `b` sees `usize` — and the *real* type error surfaces.
    check_diagnostics(
        r#"
static a = { let n: usize = 42; n + 52 };
static b = fn { print(a); };
"#,
        expect![[r#"
            65..66: type mismatch: expected `str`, found `usize`
        "#]],
    );
}

#[test]
fn unannotated_items_infer_across_items() {
    // Tail expressions, unannotated returns, chains through several
    // unannotated items: all inferred from bodies.
    check_diagnostics(
        r#"
static f = fn (n: usize) { n + 1 };
static g = fn { f(2) };
static main = fn { print(g()); };
"#,
        expect![[r#"
            86..89: type mismatch: expected `str`, found `usize`
        "#]],
    );
}

#[test]
fn unannotated_mutual_recursion_infers_via_binding_groups() {
    check_diagnostics(
        r#"
static is_even = fn (n: usize) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
}
static is_odd = fn (n: usize) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
}
static main = fn { print(is_even(4)); };
"#,
        expect![[r#"
            205..215: type mismatch: expected `str`, found `bool`
        "#]],
    );
}

#[test]
fn unannotated_recursion_infers_from_a_param_annotation() {
    // The return type still comes out of the body; the param annotation is
    // the one defining use the literals hang off (literal typing is never
    // defaulted, so a fully-unannotated recursive fn now needs one).
    check_infer(
        r#"
static fib = fn (n: usize) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } };
static use_it: usize = fib(10);
"#,
        expect![[r#"
            14..79 'fn (n: usize) { i...': fn(usize) -> usize
            18..19 'n': usize
            28..79 '{ if n < 2 { n } ...': usize
            30..77 'if n < 2 { n } el...': usize
            33..34 'n': usize
            33..38 'n < 2': bool
            37..38 '2': usize
            39..44 '{ n }': usize
            41..42 'n': usize
            50..77 '{ fib(n - 1) + fi...': usize
            52..55 'fib': fn(usize) -> usize
            52..62 'fib(n - 1)': usize
            52..75 'fib(n - 1) + fib(...': usize
            56..57 'n': usize
            56..61 'n - 1': usize
            60..61 '1': usize
            65..68 'fib': fn(usize) -> usize
            65..75 'fib(n - 2)': usize
            69..70 'n': usize
            69..74 'n - 2': usize
            73..74 '2': usize
            104..107 'fib': fn(usize) -> usize
            104..111 'fib(10)': usize
            108..110 '10': usize
        "#]],
    );
}

#[test]
fn caller_pins_callee_params() {
    // Nothing in `apply`'s own body applies `f` or `a` to anything concrete:
    // the concrete use `apply(double, 2)` pins them, which only works
    // because the caller joined the callee's group (reverse reference
    // edges). Forward edges alone would leave `apply`'s params `{error}`
    // and ask for an annotation at this use. (`apply`'s own literal renders
    // with `_`s either way: the per-item query never sees group tables.)
    check_diagnostics(
        r#"
static double = fn (n: usize) { n + n };
static apply = fn (f, a) { f(a) };
static main = fn { apply(double, 2) };
"#,
        expect![[r#""#]],
    );
    check_infer(
        r#"
static double = fn (n: usize) { n + n };
static apply = fn (f, a) { f(a) };
static main = fn { apply(double, 2) };
"#,
        expect![[r#"
            17..40 'fn (n: usize) { n...': fn(usize) -> usize
            21..22 'n': usize
            31..40 '{ n + n }': usize
            33..34 'n': usize
            33..38 'n + n': usize
            37..38 'n': usize
            57..75 'fn (f, a) { f(a) }': fn(fn(usize) -> usize, usize) -> usize
            61..62 'f': fn(usize) -> usize
            64..65 'a': usize
            67..75 '{ f(a) }': usize
            69..70 'f': fn(usize) -> usize
            69..73 'f(a)': usize
            71..72 'a': usize
            91..114 'fn { apply(double...': fn() -> usize
            94..114 '{ apply(double, 2) }': usize
            96..101 'apply': fn(fn(usize) -> usize, usize) -> usize
            96..112 'apply(double, 2)': usize
            102..108 'double': fn(usize) -> usize
            110..111 '2': usize
        "#]],
    );
}

#[test]
fn underdetermined_items_still_need_annotations() {
    // `id` is never called with anything concrete; monomorphic inference
    // can't pick a type, so the use-site annotation request remains.
    check_diagnostics(
        r#"
static id = fn (x) { x };
static main = fn { id; };
"#,
        expect![[r#"
            46..48: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        "#]],
    );
}

#[test]
fn conflicting_group_commitments_need_annotations() {
    // Calling `x` commits it to fn(usize) -> usize, but x's own body (`y`)
    // makes it a number: the group's commitments contradict each other, so
    // the signatures are poisoned to `{error}` rather than silently
    // publishing the guessed one — and neither use can tell which type the
    // item has, so both ask for an annotation.
    check_diagnostics(
        r#"
static c = true;
static y = if c { 1 } else { x(2) };
static x = y;
"#,
        expect![[r#"
            47..48: not enough information to infer the type of `x`; add a type annotation to its definition (defined here at 62..63)
            47..48: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this item's initializer is a const context at 18..24)
            66..67: not enough information to infer the type of `y`; add a type annotation to its definition (defined here at 25..26)
        "#]],
    );
}

#[test]
fn holes_in_let_and_params_infer_from_context() {
    // `_` lowers to an unconstrained variable: the initializer, the body,
    // or the surrounding context fills it in.
    check_infer(
        r#"
static f = fn {
    let x: _ = 5;
    let y: usize = x + 1;
}
static g = fn (p: _) -> usize { p + 1 };
static h = fn (_: _) -> usize { 5 };
"#,
        expect![[r#"
            12..62 'fn {     let x: _...': fn()
            15..62 '{     let x: _ = ...': ()
            25..26 'x': usize
            32..33 '5': usize
            43..44 'y': usize
            54..55 'x': usize
            54..59 'x + 1': usize
            58..59 '1': usize
            74..102 'fn (p: _) -> usiz...': fn(usize) -> usize
            78..79 'p': usize
            93..102 '{ p + 1 }': usize
            95..96 'p': usize
            95..100 'p + 1': usize
            99..100 '1': usize
            115..139 'fn (_: _) -> usiz...': fn(_) -> usize
            134..139 '{ 5 }': usize
            136..137 '5': usize
        "#]],
    );
}

#[test]
fn hole_annotated_item_infers_together_with_its_group() {
    // `even`'s `_` annotation doesn't bench it from the group: the mutual
    // recursion pins it to fn(usize) -> bool, the hole filled by the
    // bodies — so the use in main sees bool, not a silent `{error}`.
    check_diagnostics(
        r#"
static even: _ = fn (n: usize) { if n == 0 { true } else { odd(n - 1) } };
static odd = fn (n: _) { if n == 0 { false } else { even(n - 1) } };
static main = fn { print(even(4)); };
"#,
        expect![[r#"
            170..177: type mismatch: expected `str`, found `bool`
        "#]],
    );
}

#[test]
fn hole_annotated_item_signature_flows_to_dependents() {
    // `static x: _ = true;` publishes the group-inferred signature: the use
    // in main sees `bool` (and only the print mismatch) instead of the
    // diagnostics being swallowed by a silent `{error}`. (The initializer is
    // a bool because number literals are never defaulted — a bare `5` has no
    // defining use to pin it.)
    check_diagnostics(
        r#"
static x: _ = true;
static main = fn { print(x); };
"#,
        expect![[r#"
            46..47: type mismatch: expected `str`, found `bool`
        "#]],
    );
}

#[test]
fn undetermined_hole_annotated_item_needs_annotation() {
    // A `_` contract the body can't fill stays undetermined: the use site
    // asks for an annotation instead of a silent `{error}` publish.
    check_diagnostics(
        r#"
static id: _ = fn (x) { x };
static main = fn { id; };
"#,
        expect![[r#"
            49..51: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        "#]],
    );
}

#[test]
fn elided_fn_return_is_a_hole() {
    // `fn(usize)` without `-> ...` leaves the return unconstrained, like
    // a hole: the body fills it in and the signature still publishes.
    check_infer(
        r#"
static f: fn(usize) = fn (n) { n };
static main = fn { f(1) + 1 };
"#,
        expect![[r#"
            23..35 'fn (n) { n }': fn(usize) -> usize
            27..28 'n': usize
            30..35 '{ n }': usize
            32..33 'n': usize
            51..66 'fn { f(1) + 1 }': fn() -> usize
            54..66 '{ f(1) + 1 }': usize
            56..57 'f': fn(usize) -> usize
            56..60 'f(1)': usize
            56..64 'f(1) + 1': usize
            58..59 '1': usize
            63..64 '1': usize
        "#]],
    );
}

#[test]
fn unknown_type_name_is_diagnosed() {
    check_diagnostics(
        r#"
static x: foo = 1;
static f = fn (s: bar) {};
"#,
        expect![[r#"
            11..14: unknown type `foo`
            38..41: unknown type `bar`
        "#]],
    );
}

#[test]
fn calling_a_diverging_value_is_never_not_error() {
    // The body's tail is `x()`, itself `!` (calling a diverging value stays
    // diverging) — `f`'s unannotated return type is pinned to `!` from
    // that alone, not left an unresolved `{error}` with no diagnostic to
    // explain it.
    check_infer(
        r#"
static f = fn {
    let x = panic("boom");
    x();
};
"#,
        expect![[r#"
            12..54 'fn {     let x = ...': fn() -> !
            15..54 '{     let x = pan...': !
            25..26 'x': !
            29..34 'panic': fn(str) -> !
            29..42 'panic("boom")': !
            35..41 '"boom"': str
            48..49 'x': !
            48..51 'x()': !
        "#]],
    );
}

// ---- T14: an unannotated diverging fn body concludes `!` -------------

#[test]
fn a_diverging_body_is_callable_across_items_without_an_annotation() {
    // Before this default, `boom`'s return type stayed an unpinned
    // variable, group inference erased it to `{error}`, and every
    // cross-item use demanded an annotation the author had nothing useful
    // to write in. `!` is what the body actually means.
    check_diagnostics(
        r#"
static boom = fn () { panic("boom") };
static use_it = fn () -> usize { boom() };
static main = fn () { boom(); };
"#,
        expect![""],
    );
}

#[test]
fn a_diverging_bodys_value_is_usable_wherever_never_coerces() {
    // The default is `!`, not a fresh opaque type: the call's value flows
    // into a `usize` position (and any other) by the ordinary `!` coercion.
    check_infer(
        r#"
static forever = fn () { loop { } };
static n: usize = 1;
static pick = fn (c: bool) -> usize { if c { 1 } else { forever() } };
"#,
        expect![[r#"
            18..36 'fn () { loop { } }': fn() -> !
            24..36 '{ loop { } }': !
            26..34 'loop { }': !
            31..34 '{ }': ()
            56..57 '1': usize
            73..128 'fn (c: bool) -> u...': fn(bool) -> usize
            77..78 'c': bool
            95..128 '{ if c { 1 } else...': usize
            97..126 'if c { 1 } else {...': usize
            100..101 'c': bool
            102..107 '{ 1 }': usize
            104..105 '1': usize
            113..126 '{ forever() }': !
            115..122 'forever': fn() -> !
            115..124 'forever()': !
        "#]],
    );
}

#[test]
fn a_diverging_body_pinned_by_its_own_return_keeps_that_type() {
    // A `return e` inside the body IS a real constraint, so the default
    // yields to it even though the body diverges — and the number
    // discipline is untouched: the literal still needs a defining use.
    check_diagnostics(
        r#"
static f = fn (c: bool) { if c { return 1; }; panic("x") };
"#,
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn an_annotated_slot_around_a_diverging_body_is_not_pinned_to_never() {
    // The default fires only where `ret` ends GENUINELY free: an item-level
    // annotated slot is checked against the literal before any diverging-
    // body default is considered, so the body's own `!` coerces at the
    // tail instead of being forced onto the signature. Without checking
    // the slot first, this used to report
    // "expected `fn() -> usize`, found `fn() -> !`".
    check_diagnostics(
        r#"
static f: fn() -> usize = fn () { panic("x") };
static use_it = fn () { let n: usize = f(); };
"#,
        expect![""],
    );
}

#[test]
fn an_annotated_let_binding_around_a_diverging_literal_is_not_pinned_to_never() {
    // Same guard, `let`-annotated rather than item-annotated.
    check_diagnostics(
        r#"
static use_it = fn () {
    let f: fn() -> usize = fn () { panic("x") };
    let n: usize = f();
};
"#,
        expect![""],
    );
}

#[test]
fn a_diverging_items_own_signature_beats_a_later_annotated_use() {
    // T13: `fn() -> T` is invariant in `T`, so a `!` in return position
    // does not widen to `usize` just because a later use wants it there.
    // `diverges` has no annotation of its own, so ITS signature defaults
    // to `fn() -> !`; `pin_it`'s annotation disagrees with that, honestly
    // — the same mismatch a hand-written `fn() -> !` would draw.
    check_diagnostics(
        r#"
static diverges = fn () { panic("x") };
static pin_it = fn () { let f: fn() -> usize = diverges; f() };
"#,
        expect![[r#"
            88..96: type mismatch: expected `fn() -> usize`, found `fn() -> !` (expected `fn() -> usize` because of this annotation at 72..85)
        "#]],
    );
}
