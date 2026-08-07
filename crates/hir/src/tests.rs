use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

fn check_diagnostics(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let rendered = crate::file_diagnostics(&db, file)
        .into_iter()
        .map(|d| {
            let related = d
                .related
                .iter()
                .map(|r| format!(" ({} at {:?})", r.message, r.range))
                .collect::<String>();
            format!("{:?}: {}{related}\n", d.range, d.message)
        })
        .collect::<String>();
    expect.assert_eq(&rendered);
}

/// Renders every expression and binding with its inferred type,
/// rust-analyzer style: `range 'snippet': type`.
fn check_infer(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut lines = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (_, source_map) = crate::body_with_source_map(&db, item);
        let result = crate::infer::infer(&db, item);
        for (expr, ty) in result.type_of_expr.iter() {
            if let Some(ptr) = source_map.node_for_expr(expr) {
                lines.push((ptr.text_range(), ty.display()));
            }
        }
        for (binding, ty) in result.type_of_binding.iter() {
            if let Some(ptr) = source_map.node_for_binding(binding) {
                lines.push((ptr.text_range(), ty.display()));
            }
        }
    }
    lines.sort_by_key(|(range, _)| (range.start(), range.end()));
    let rendered = lines
        .into_iter()
        .map(|(range, ty)| {
            let snippet: String = text[range].replace('\n', " ");
            let snippet = if snippet.len() > 20 {
                format!("{}...", &snippet[..17])
            } else {
                snippet
            };
            format!("{range:?} '{snippet}': {ty}\n")
        })
        .collect::<String>();
    expect.assert_eq(&rendered);
}

#[test]
fn unresolved_name() {
    check_diagnostics(
        "static main = fn { missing() };",
        expect![[r#"
            19..26: unresolved name `missing`
        "#]],
    );
}

#[test]
fn locals_params_and_shadowing_resolve() {
    check_diagnostics(
        r#"
static f = fn (a: str) {
    let b = a;
    let b = b;
    print(b);
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn let_initializer_does_not_see_its_own_binding() {
    check_diagnostics(
        "static f = fn { let x = x; };",
        expect![[r#"
            24..25: unresolved name `x`
        "#]],
    );
}

#[test]
fn let_hole_pattern_binds_nothing() {
    // `_` is not a name: a later use of `_` doesn't resolve to the binding
    // (it doesn't even parse as a reference — `_` only lexes as a hole).
    check_diagnostics(
        "static f = fn { let _ = 5; let x = _; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            35..36: expected an expression
        "#]],
    );
}

#[test]
fn let_hole_pattern_initializer_is_still_type_checked() {
    // The binding is discarded, but the initializer is fully inferred, so a
    // type error inside it is still reported.
    check_diagnostics(
        r#"static f = fn { let _: usize = "hello"; };"#,
        expect![[r#"
            31..38: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..28)
        "#]],
    );
}

#[test]
fn param_hole_pattern_binds_nothing() {
    check_diagnostics("static f = fn (_: usize) { };", expect![[r#""#]]);
}

#[test]
fn hole_named_static_item_gets_dead_code_warning() {
    // `_` binds nothing, so `5` can never be referenced — squiggle on `_`.
    check_diagnostics(
        "static _ = 5;",
        expect![[r#"
            7..8: this item binds nothing and its value cannot be used
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn hole_named_const_item_gets_dead_code_warning() {
    check_diagnostics(
        "const _ = 5;",
        expect![[r#"
            6..7: this item binds nothing and its value cannot be used
            10..11: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn named_item_gets_no_dead_code_warning() {
    check_diagnostics("static x: usize = 5;", expect![[r#""#]]);
}

#[test]
fn broken_item_with_missing_name_gets_no_dead_code_warning() {
    // No `NAME` node at all (not even a hole one) — distinct from
    // `static _ = ...`, so this must not also get the dead-code warning on
    // top of the parse error.
    check_diagnostics(
        "static = 5;",
        expect![[r#"
            7..8: expected a name for the item
            9..10: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn infer_let_hole_pattern() {
    check_infer(
        r#"static f = fn { let _ = 5; };"#,
        expect![[r#"
            11..28 'fn { let _ = 5; }': fn()
            14..28 '{ let _ = 5; }': ()
            20..21 '_': {number}
            24..25 '5': {number}
        "#]],
    );
}

#[test]
fn mutual_recursion_and_self_reference_resolve() {
    check_diagnostics(
        r#"
const fib1 = fn (n: usize) -> usize { fib2(n-1) + fib2(n-2) }
static fib2 = fn (n: usize) -> usize { fib1(n-1) + fib2(n-2) }
"#,
        expect![[r#""#]],
    );
}

#[test]
fn builtins_resolve() {
    check_diagnostics(
        r#"static main = fn { print("hi"); panic("boom"); };"#,
        expect![[r#""#]],
    );
}

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
            .filter(|entry| entry.contains("infer"))
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
        log.iter().filter(|entry| entry.contains("infer")).count(),
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
    // `resolutions` does re-run — a new name entered the file scope, which
    // genuinely could matter — but its *value* is unchanged (ItemLocs are
    // name-based, not positional), so inference backdates behind it.
    for query in ["infer", "expr_scopes"] {
        assert_eq!(
            log.iter().filter(|entry| entry.contains(query)).count(),
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
fn duplicate_definition_diagnosed_on_the_later_one() {
    check_diagnostics(
        r#"
static name = 42 + 52;

static name = fn {
    let v = name;
}
"#,
        expect![[r#"
            15..17: cannot infer the type of this number: it has no defining use — add a type annotation
            32..36: `name` is defined multiple times (first defined here at 8..12)
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
            57..75 'fn (f, a) { f(a) }': fn(fn(_) -> _, _) -> _
            61..62 'f': fn(_) -> _
            64..65 'a': _
            67..75 '{ f(a) }': _
            69..70 'f': fn(_) -> _
            69..73 'f(a)': _
            71..72 'a': _
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
            46..48: cannot infer the type of `id` across items; add a type annotation to its definition (defined here at 8..10)
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
            47..48: cannot infer the type of `x` across items; add a type annotation to its definition (defined here at 62..63)
            47..48: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this item's initializer is a const context at 18..24)
            66..67: cannot infer the type of `y` across items; add a type annotation to its definition (defined here at 25..26)
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
            119..120 '_': _
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
            49..51: cannot infer the type of `id` across items; add a type annotation to its definition (defined here at 8..10)
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

#[test]
fn if_is_an_expression_and_branches_must_agree() {
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    let big = if n > 100 { true } else { false };
    if big { n / 2 } else { n * 2 }
}
"#,
        expect![[r#"
            12..124 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..124 '{     let big = i...': usize
            45..48 'big': bool
            51..85 'if n > 100 { true...': bool
            54..55 'n': usize
            54..61 'n > 100': bool
            58..61 '100': usize
            62..70 '{ true }': bool
            64..68 'true': bool
            76..85 '{ false }': bool
            78..83 'false': bool
            91..122 'if big { n / 2 } ...': usize
            94..97 'big': bool
            98..107 '{ n / 2 }': usize
            100..101 'n': usize
            100..105 'n / 2': usize
            104..105 '2': usize
            113..122 '{ n * 2 }': usize
            115..116 'n': usize
            115..120 'n * 2': usize
            119..120 '2': usize
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 1 } else { "one" };
    print("done");
}
"#,
        expect![[r#"
            58..59: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 69..74)
        "#]],
    );
}

#[test]
fn annotated_let_blames_branch_not_whole_if() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { 0 } else { "" };
    print(x);
}
"#,
        expect![[r#"
            63..64: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { 0 } else { "" };
    print("");
}
"#,
        expect![[r#"
            76..78: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn if_result_constrained_by_use_blames_branch() {
    // Then branch is the culprit — squiggle on the literal, hint at print call.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 0 } else { "" };
    print(x);
}
"#,
        expect![[r#"
            58..59: type mismatch: expected `str`, found `{number}` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
        "#]],
    );
    // Else branch is the culprit — squiggle on the literal, hint at print call.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { 0 };
    print(x);
}
"#,
        expect![[r#"
            70..71: type mismatch: expected `str`, found `{number}` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
        "#]],
    );
}

#[test]
fn nested_if_blame_propagates_to_innermost_culprit() {
    // The wrong `0` is inside a nested if; blame reaches it rather than
    // stopping at the outer else branch, and the hints cite *every* branch
    // that voted `str` (inner and outer) plus the call that demanded it.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    print(x);
}
"#,
        expect![[r#"
            94..95: type mismatch: expected `str`, found `{number}` (this call requires `str` at 105..110) (this argument needs to be `str` at 111..112)
        "#]],
    );
}

#[test]
fn every_offending_branch_gets_its_own_squiggle_not_the_whole_if() {
    // Two branches are wrong (`""` in both nesting levels), one is right
    // (`0`). Each culprit gets its own squiggle; the outer `if` as a whole
    // stays clean — it isn't wrong, its branches are.
    check_diagnostics(
        r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    constrainer("", x);
}
"#,
        expect![[r#"
            105..107: type mismatch: expected `usize`, found `str` (this call requires `usize` at 152..163) (this argument needs to be `usize` at 168..169)
            129..131: type mismatch: expected `usize`, found `str` (this call requires `usize` at 152..163) (this argument needs to be `usize` at 168..169)
        "#]],
    );
}

#[test]
fn unanimous_branches_against_call_axiom_blame_the_whole_construct() {
    // Every branch produces `str`, so no single branch is the culprit: the
    // construct as a whole conflicts with the call's requirement. Hints
    // point at the call and the argument the requirement travels through.
    check_diagnostics(
        r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { "" };
    constrainer("", x);
}
"#,
        expect![[r#"
            93..121: every branch produces `str`, but `usize` is needed (this call requires `usize` at 127..138) (this argument needs to be `usize` at 143..144)
        "#]],
    );
}

#[test]
fn plurality_of_branches_decides_without_any_axiom() {
    // No annotation and no use constrains `x`: the branches vote, the
    // `str` plurality wins, and the odd one out gets the squiggle with
    // every winning leaf as a hint, in source order.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 1 { 1 } else { "" } };
    print("done");
}
"#,
        expect![[r#"
            82..83: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 58..60) (this branch has type `str` at 93..95)
        "#]],
    );
}

#[test]
fn nested_branches_vote_individually() {
    // The vote flattens across nested `if`s in the same function: the two
    // `0` leaves outvote the single `""` two to one, even though the `""`
    // sits shallower. The odd one out gets the squiggle; every winning
    // leaf is a hint.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { if n == 0 { 0 } else { 0 } } else { "" };
    print("done");
}
"#,
        expect![[r#"
            70..71: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 94..96)
            81..82: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 94..96)
        "#]],
    );
}

#[test]
fn nested_join_resolves_once_at_the_annotation() {
    // A nest of `if`s is ONE join with witnesses {leaf, leaf, leaf},
    // resolved at the `let`'s annotation (the axiom): exactly one
    // diagnostic, on the offending leaf, citing the annotation — never an
    // intermediate "the inner if has type …" step. Variants: the culprit in
    // the inner-then, inner-else, and outer-else slots.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { 0 } else { "a" } } else { "b" };
    print(x);
}
"#,
        expect![[r#"
            75..76: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { "a" } else { 0 } } else { "b" };
    print(x);
}
"#,
        expect![[r#"
            88..89: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { "a" } else { "b" } } else { 0 };
    print(x);
}
"#,
        expect![[r#"
            103..104: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
}

#[test]
fn inner_branches_blamed_individually_never_the_inner_if() {
    // Both leaves of the *inner* if disagree with the annotation while the
    // outer-else leaf agrees: each wrong leaf gets its own squiggle citing
    // the annotation. The inner `if` is not a blame target — there is no
    // "every branch of the inner if" verdict, because the nest is one join.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { if n == 1 { "a" } else { "b" } } else { 1 };
    print("done");
}
"#,
        expect![[r#"
            77..80: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
            90..93: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn nested_leaves_unanimous_against_annotation_blame_the_whole_nest() {
    // Every leaf across both nesting levels produces `str`: one diagnostic
    // on the whole (outermost) construct, not per leaf and not on the
    // inner if.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: usize = if n == 0 { if n == 1 { "a" } else { "b" } } else { "c" };
    print("done");
}
"#,
        expect![[r#"
            53..110: every branch produces `str`, but `usize` is needed (expected `usize` because of this annotation at 45..50)
        "#]],
    );
}

#[test]
fn statement_position_if_inside_a_branch_is_its_own_join() {
    // The inner `if` sits in a branch's *statements* (a `let`), not its
    // tail: it is statement position, so it resolves as its own join (an
    // unresolvable tie here) and does not leak its leaves into the outer
    // join — which is internally consistent and stays clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { let y = if n == 1 { 1 } else { "s" }; 2 } else { 3 };
    print("done");
}
"#,
        expect![[r#"
            78..79: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 89..92)
            96..97: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_bound_join_resolves_at_its_let_not_inside_a_later_join() {
    // `x`'s join meets a non-join consumer (the `let`), so it resolves
    // there — unanimously `usize` — before the second `if` consumes the
    // binding. The second join then sees {usize, str}: an honest tie, not
    // a plurality vote over `x`'s leaves.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 1 } else { 2 };
    let z = if n == 1 { x } else { "s" };
    print("done");
}
"#,
        expect![[r#"
            98..99: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 109..112)
        "#]],
    );
}

#[test]
fn call_argument_join_resolves_against_the_parameter_type() {
    // A call argument is not statement position, but the call boundary
    // makes the parameter type an axiom: the join resolves against it
    // right there, blaming the disagreeing leaf and citing the call.
    check_diagnostics(
        r#"static f = fn (n: usize) -> () { print(if n == 0 { "s" } else { 1 }); };"#,
        expect![[r#"
            64..65: type mismatch: expected `str`, found `{number}`
        "#]],
    );
}

#[test]
fn never_leaf_in_a_nested_if_neither_votes_nor_blocks_flattening() {
    // A diverging leaf inside the inner if widens (it contributes no
    // witness) — the flat join judges the surviving leaves {0, "s"}
    // against the annotation, blaming exactly the `0`, once.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { panic("boom") } else { 0 } } else { "s" };
    print(x);
}
"#,
        expect![[r#"
            98..99: type mismatch: expected `str`, found `{number}` (expected `str` because of this annotation at 45..48)
        "#]],
    );
    // And when the surviving leaves all agree, the nest is clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { if n == 1 { panic("boom") } else { "a" } } else { "b" };
    print(x);
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn hover_on_a_nested_if_shows_the_flat_joins_type() {
    // Decision: an `if` in witness position types as the enclosing join's
    // result, so hovering the inner `if` shows the whole nest's resolved
    // type (`usize`), not a partial verdict of its own.
    check_infer(
        r#"static f = fn (n: usize) -> usize { if n == 0 { if n == 1 { 1 } else { 2 } } else { 3 } };"#,
        expect![[r#"
            11..89 'fn (n: usize) -> ...': fn(usize) -> usize
            15..16 'n': usize
            34..89 '{ if n == 0 { if ...': usize
            36..87 'if n == 0 { if n ...': usize
            39..40 'n': usize
            39..45 'n == 0': bool
            44..45 '0': usize
            46..76 '{ if n == 1 { 1 }...': usize
            48..74 'if n == 1 { 1 } e...': usize
            51..52 'n': usize
            51..57 'n == 1': bool
            56..57 '1': usize
            58..63 '{ 1 }': usize
            60..61 '1': usize
            69..74 '{ 2 }': usize
            71..72 '2': usize
            82..87 '{ 3 }': usize
            84..85 '3': usize
        "#]],
    );
}

#[test]
fn fn_literal_votes_once_as_a_unit() {
    // Wrapping the nested `if` in a function changes the vote: the
    // function settles its type internally (`usize`, unanimously) and
    // contributes exactly one vote outside — a genuine tie with `""`.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { fn { if true { 0 } else { 0 } }() } else { "" };
    print("done");
}
"#,
        expect![[r#"
            58..91: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 101..103)
        "#]],
    );
}

#[test]
fn fn_internal_inconsistency_not_resolved_by_use() {
    // The function's branches disagree with each other; the annotation on
    // `x` must not settle that argument from outside — a function has to
    // be internally consistent on its own. The tie is reported inside the
    // function, and the conflicting use at the call, as a unit.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { fn { if true { 0 } else { "" } }() } else { "s" };
    print(x);
}
"#,
        expect![[r#"
            78..79: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 89..91)
        "#]],
    );
}

#[test]
fn agreeing_branches_against_return_annotation_get_one_diagnostic() {
    // Both branches say str; the return annotation says usize. One
    // diagnostic on the whole `if`, not a squiggle per branch.
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n == 0 { "a" } else { "b" } };"#,
        expect![[r#"
            36..66: every branch produces `str`, but `usize` is needed (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn group_member_signature_not_overridden_by_use() {
    // `g` and `main` are unannotated, so they infer as one group. `g`'s
    // branches unanimously say usize; the `print` in `main` must not flip
    // `g`'s signature to `str` — the mismatch belongs at the call site.
    check_diagnostics(
        r#"
static g = fn (c: bool, n: usize) { if c { n } else { 2 } };
static main = fn { print(g(true, 3)); };
"#,
        expect![[r#"
            87..97: type mismatch: expected `str`, found `usize`
        "#]],
    );
}

#[test]
fn if_condition_must_be_bool() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> () { if n { print("hi"); } };"#,
        expect![[r#"
            36..37: type mismatch: expected `bool`, found `usize` (this `if` requires a `bool` condition at 33..35)
        "#]],
    );
}

#[test]
fn if_without_else_is_unit() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n > 0 { n } };"#,
        expect![[r#"
            36..50: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
            47..48: type mismatch: expected `()`, found `usize` (this `if` has no `else`, so its value is `()` at 36..38)
        "#]],
    );
}

#[test]
fn diverging_if_branch_takes_the_other_branches_type() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n == 0 { panic("zero") } else { n } };"#,
        expect![[r#""#]],
    );
    check_infer(
        r#"static f = fn (n: usize) -> usize { if n == 0 { panic("a") } else { panic("b") } };"#,
        expect![[r#"
            11..82 'fn (n: usize) -> ...': fn(usize) -> usize
            15..16 'n': usize
            34..82 '{ if n == 0 { pan...': usize
            36..80 'if n == 0 { panic...': usize
            39..40 'n': usize
            39..45 'n == 0': bool
            44..45 '0': usize
            46..60 '{ panic("a") }': !
            48..53 'panic': fn(str) -> !
            48..58 'panic("a")': !
            54..57 '"a"': str
            66..80 '{ panic("b") }': !
            68..73 'panic': fn(str) -> !
            68..78 'panic("b")': !
            74..77 '"b"': str
        "#]],
    );
}

#[test]
fn equality_operands_must_agree() {
    check_diagnostics(
        r#"static x: bool = 1 == "one";"#,
        expect![[r#"
            22..27: type mismatch: expected `{number}`, found `str` (this operand has type `{number}` at 17..18)
        "#]],
    );
}

#[test]
fn item_tree_classifies_constness() {
    use crate::item_tree::{Constness, item_tree};

    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        r#"
static a = 1;
const b = 5;
const c = fn { 1 };
static d = const fn { 1 };
static e = const { 2 };
"#
        .to_owned(),
    );
    let tree = item_tree(&db, file);
    let constness: Vec<Constness> = tree
        .items
        .iter()
        .filter_map(|item| item.kind.constness())
        .collect();
    assert_eq!(
        constness,
        vec![
            Constness::Static,
            Constness::Const,
            Constness::Const,
            // A `const` starting the *initializer* (`const fn` literal,
            // `const { ... }` block) must not make the item itself const.
            Constness::Static,
            Constness::Static,
        ]
    );
}

/// Const-checking is a separate pass; `const { ... }` is transparent
/// for typing — it gets its own entry in `type_of_expr`, but its type is
/// exactly the inner block's.
#[test]
fn const_block_is_transparent_for_typing() {
    check_infer(
        "static x = const { 5 };",
        expect![[r#"
            11..22 'const { 5 }': {number}
            17..22 '{ 5 }': {number}
            19..20 '5': {number}
        "#]],
    );
}

/// `const fn` is an explicit marker orthogonal to typing: a `const fn`
/// literal infers exactly the same signature a plain `fn` literal would.
#[test]
fn const_fn_literal_infers_like_plain_fn_literal() {
    check_infer(
        "static f = const fn (n: usize) -> usize { n };",
        expect![[r#"
            11..45 'const fn (n: usiz...': fn(usize) -> usize
            21..22 'n': usize
            40..45 '{ n }': usize
            42..43 'n': usize
        "#]],
    );
}

#[test]
fn const_fn_item_callable_from_item_initializer() {
    // An item initializer is a const context (for `static` and `const`
    // alike); an item whose initializer is a `const fn` literal may be
    // called there — the item's own keyword is irrelevant.
    check_diagnostics(
        r#"
static double = const fn (n: usize) -> usize { n * 2 };
const x = double(2);
static y = double(3);
"#,
        expect![[r#""#]],
    );
}

#[test]
fn directly_called_const_fn_literal_is_allowed() {
    check_diagnostics(
        "static x = (const fn (n: usize) -> usize { n })(1);",
        expect![[r#""#]],
    );
}

#[test]
fn panic_is_allowed_in_const_contexts() {
    check_diagnostics(
        r#"static x: usize = if 1 == 2 { panic("impossible") } else { 5 };"#,
        expect![[r#"
            21..22: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn plain_fn_body_exits_the_const_context() {
    // The initializer is a const context, but entering the plain fn
    // literal's body leaves it: that body is runtime code, `print` is fine.
    check_diagnostics(r#"static main = fn { print("hi"); };"#, expect![[r#""#]]);
}

#[test]
fn defining_a_plain_fn_literal_in_a_const_context_is_fine() {
    // Only calls are checked: a plain fn literal may be *defined* in a
    // const context (even inside a `const { ... }`), and its body is
    // runtime code again — the `print` inside is not flagged.
    check_diagnostics(
        r#"static f = const { fn { print("later") } };"#,
        expect![[r#""#]],
    );
}

#[test]
fn calling_a_plain_fn_item_from_an_initializer_is_rejected() {
    check_diagnostics(
        r#"
static double = fn (n: usize) -> usize { n * 2 };
static x = double(2);
"#,
        expect![[r#"
            62..68: cannot call `double` in a const context; marking it `const fn` would allow this (`double` is defined here at 8..14) (this item's initializer is a const context at 51..57)
        "#]],
    );
}

#[test]
fn directly_called_plain_fn_literal_in_const_context_is_rejected() {
    check_diagnostics(
        "static x = (fn (n: usize) -> usize { n })(1);",
        expect![[r#"
            12..40: cannot call this `fn` literal in a const context; marking it `const fn` would allow this (this item's initializer is a const context at 0..6)
        "#]],
    );
}

#[test]
fn print_in_a_const_block_is_rejected() {
    check_diagnostics(
        r#"static x = const { print("hi") };"#,
        expect![[r#"
            19..24: cannot call `print` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 11..16)
        "#]],
    );
}

#[test]
fn read_line_in_a_const_block_is_rejected() {
    // `print`'s input twin gets `print`'s exact treatment: a
    // side-effecting builtin, refused in a const context with the same
    // message shape (the builtin's own name is the only thing that
    // varies).
    check_diagnostics(
        r#"static x = const { read_line() };"#,
        expect![[r#"
            19..28: cannot call `read_line` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 11..16)
        "#]],
    );
}

#[test]
fn const_block_in_a_plain_fn_body_reenters_the_const_context() {
    // Rule 2 exits the const context at the fn body, but `const { ... }`
    // re-enters it — the violation inside is flagged.
    check_diagnostics(
        r#"
static main = fn {
    print("runtime is fine");
    const { print("compile time is not") };
}
"#,
        expect![[r#"
            62..67: cannot call `print` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 54..59)
        "#]],
    );
}

#[test]
fn calling_a_parameter_in_a_const_fn_body_is_rejected() {
    // Const-ness is not part of fn types: a parameter can't be known to be
    // a `const fn`, so calling it is rejected conservatively.
    check_diagnostics(
        "static apply = const fn (f: fn() -> usize) -> usize { f() };",
        expect![[r#"
            54..55: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const fn` is always a const context at 15..20)
        "#]],
    );
}

#[test]
fn calling_a_let_bound_value_in_a_const_block_is_rejected() {
    // Even provably bound to a `const fn`, a let-bound value is rejected:
    // const-ness is not tracked through bindings.
    check_diagnostics(
        r#"
static double = const fn (n: usize) -> usize { n * 2 };
static f = fn {
    let d = double;
    const { d(2) };
}
"#,
        expect![[r#"
            105..106: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const` block is a const context at 97..102)
        "#]],
    );
}

#[test]
fn item_whose_root_is_not_a_fn_literal_is_a_value_call() {
    // No peeling: the `const fn` literal sits inside a `const { ... }`
    // wrapper, so the item's root is not a fn literal and calling it is
    // conservatively a value call.
    check_diagnostics(
        r#"
static wrapped = const { const fn (n: usize) -> usize { n } };
static x = wrapped(1);
"#,
        expect![[r#"
            75..82: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this item's initializer is a const context at 64..70)
        "#]],
    );
}

#[test]
fn unresolved_callee_in_const_context_is_not_double_reported() {
    // The unresolved name already carries a diagnostic; const-check stays
    // silent about it.
    check_diagnostics(
        "static x = missing();",
        expect![[r#"
            11..18: unresolved name `missing`
        "#]],
    );
}

#[test]
fn call_nested_in_a_rejected_calls_args_is_still_checked() {
    // No cascading suppression: the outer call is rejected, and the `print`
    // in its argument list is judged on its own merits too.
    check_diagnostics(
        r#"
static f = fn (n: ()) -> usize { 1 };
static x = f(print("hi"));
"#,
        expect![[r#"
            50..51: cannot call `f` in a const context; marking it `const fn` would allow this (`f` is defined here at 8..9) (this item's initializer is a const context at 39..45)
            52..57: cannot call `print` in a const context; const evaluation cannot have side effects (this item's initializer is a const context at 39..45)
        "#]],
    );
}

#[test]
fn assignment_infers_value_against_the_bindings_type() {
    check_infer(
        "static f = fn { let mut x = 1; x = 2; x };",
        expect![[r#"
            11..41 'fn { let mut x = ...': fn() -> {number}
            14..41 '{ let mut x = 1; ...': {number}
            24..25 'x': {number}
            28..29 '1': {number}
            31..32 'x': {number}
            35..36 '2': {number}
            38..39 'x': {number}
        "#]],
    );
}

#[test]
fn assignment_value_must_match_the_bindings_type() {
    // No annotation on `x`, so the hint falls back to the binding's name:
    // that's where its inferred type became attached.
    check_diagnostics(
        r#"static f = fn { let mut x = 1; x = "no"; };"#,
        expect![[r#"
            35..39: type mismatch: expected `{number}`, found `str` (`x` was inferred to have type `{number}` from its initializer at 24..25)
        "#]],
    );
}

#[test]
fn assignment_mismatch_cites_the_bindings_annotation_when_present() {
    check_diagnostics(
        r#"static f = fn { let mut x: usize = 1; x = "no"; };"#,
        expect![[r#"
            42..46: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 27..32)
        "#]],
    );
}

#[test]
fn assignment_to_an_immutable_let_is_rejected() {
    check_diagnostics(
        "static f = fn { let x = 1; x = 2; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            27..28: cannot assign to `x`: it is not declared `mut` (`x` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn assignment_to_a_mut_param_is_allowed() {
    // Regression guard: the happy path must stay diagnostic-free.
    check_diagnostics(
        "static f = fn (mut n: usize) -> usize { n = n + 1; n };",
        expect![[""]],
    );
}

#[test]
fn assignment_to_an_immutable_param_is_rejected() {
    check_diagnostics(
        "static f = fn (n: usize) { n = 2; };",
        expect![[r#"
            27..28: cannot assign to `n`: it is not declared `mut` (`n` is declared without `mut` here at 15..16)
        "#]],
    );
}

#[test]
fn assignment_to_a_static_item_is_rejected() {
    check_diagnostics(
        "static x: usize = 1;\nstatic f = fn { x = 2; };",
        expect![[r#"
            37..38: cannot assign to `x`: `static` items cannot be reassigned (`x` is defined here at 7..8)
        "#]],
    );
}

#[test]
fn assignment_to_a_const_item_is_rejected() {
    check_diagnostics(
        "const x: usize = 1;\nstatic f = fn { x = 2; };",
        expect![[r#"
            36..37: cannot assign to `x`: a `const` is copied into each use, so there is no single place to assign to (`x` is defined here at 6..7)
        "#]],
    );
}

#[test]
fn assignment_to_a_builtin_is_rejected() {
    // The value side reads `print` (same type), so the target diagnostic
    // is the only one.
    check_diagnostics(
        "static f = fn { print = print; };",
        expect![[r#"
            16..21: cannot assign to `print`: it is a builtin function
        "#]],
    );
}

#[test]
fn mutation_is_allowed_in_const_contexts() {
    // No const-check rule rejects local mutation; only calls are restricted.
    check_diagnostics(
        "static x = const { let mut n = 1; n = n + 1; n };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn assign_to_immutable_let_offers_a_make_mutable_fix() {
    let text = "static f = fn { let x: usize = 1; x = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `x` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    assert!(fix.edits[0].range.is_empty());
    // Immediately before the binding's name `x` in `let x` (offset 20):
    // applying it yields `let mut x = 1;`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 20);
}

#[test]
fn assign_to_immutable_param_offers_a_make_mutable_fix() {
    let text = "static f = fn (n: usize) { n = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `n` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the parameter's name `n` (offset 15): applying it
    // yields `fn (mut n: usize)` — params take `mut` before the name.
    assert_eq!(u32::from(fix.edits[0].range.start()), 15);
}

#[test]
fn mut_on_hole_offers_a_remove_mut_fix() {
    // Validation's "`mut` has no effect on `_`" error carries a fix that
    // deletes the `mut` keyword and the whitespace up to the hole.
    let text = "static f = fn { let mut _: usize = 1; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Remove `mut`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "");
    // Deletes `mut ` (offsets 20..24), leaving `let _ = 1;`.
    assert_eq!(&text[fix.edits[0].range], "mut ");
}

#[test]
fn assignment_to_an_unresolved_name_does_not_panic() {
    // The target is a plain variable syntactically (so validation has
    // nothing to say), but it doesn't resolve to any binding — reported
    // exactly like an unresolved name anywhere else, and lowering stays
    // total downstream (see `mir`'s equivalent capture-write coverage).
    check_diagnostics(
        "static f = fn { y = 2; };",
        expect![[r#"
            16..17: unresolved name `y`
        "#]],
    );
}

// --- Records: structural typing ---

#[test]
fn record_literal_infers_structurally() {
    check_infer(
        r#"static f = fn { let p = struct { x = 1, y = "s" }; };"#,
        expect![[r#"
            11..52 'fn { let p = stru...': fn()
            14..52 '{ let p = struct ...': ()
            20..21 'p': struct { x: {number}, y: str }
            24..49 'struct { x = 1, y...': struct { x: {number}, y: str }
            37..38 '1': {number}
            44..47 '"s"': str
        "#]],
    );
}

#[test]
fn record_type_canonicalizes_field_order() {
    // The annotation writes the fields in one order, the literal in the
    // other: field order is irrelevant to the type, so both are the same
    // record and nothing is reported.
    check_diagnostics(
        r#"static f = fn { let p: struct { y: str, x: usize } = struct { x = 1, y = "s" }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn annotated_let_with_matching_record_is_ok() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str } = struct { x = 1, y = "s" }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_field_mismatch_blames_the_field_and_cites_the_annotation() {
    // Bidirectional: the annotation's field types flow into the field
    // initializers, so the squiggle lands on `"s"` (not the whole literal)
    // and cites the annotation as the cause.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x = "s" }; };"#,
        expect![[r#"
            58..61: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..42)
        "#]],
    );
}

#[test]
fn record_literal_missing_field() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str } = struct { x = 1 }; };"#,
        expect![[r#"
            53..69: record literal is missing field `y: str`
        "#]],
    );
}

#[test]
fn record_literal_missing_several_fields() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str, z: bool } = struct { y = "s" }; };"#,
        expect![[r#"
            62..80: record literal is missing fields `x: usize`, `z: bool`
        "#]],
    );
}

#[test]
fn record_literal_extra_field() {
    // Exact field-set equality: the extra field is an error (squiggle on
    // its name), never silently dropped.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x = 1, z = 2 }; };"#,
        expect![[r#"
            61..62: no field `z` in expected type `struct { x: usize }`
            65..66: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_literal_against_non_record_expectation() {
    check_diagnostics(
        r#"static f = fn { let n: usize = struct { x = 1 }; };"#,
        expect![[r#"
            31..47: type mismatch: expected `usize`, found `struct { x: {error} }` (expected `usize` because of this annotation at 23..28)
        "#]],
    );
}

#[test]
fn field_access_infers_the_field_type() {
    check_infer(
        r#"static f = fn { let p = struct { x = 1 }; let y = p.x; };"#,
        expect![[r#"
            11..56 'fn { let p = stru...': fn()
            14..56 '{ let p = struct ...': ()
            20..21 'p': struct { x: {number} }
            24..40 'struct { x = 1 }': struct { x: {number} }
            37..38 '1': {number}
            46..47 'y': {number}
            50..51 'p': struct { x: {number} }
            50..53 'p.x': {number}
        "#]],
    );
}

#[test]
fn field_access_unknown_field() {
    check_diagnostics(
        r#"static f = fn { let p = struct { x = 1 }; p.z; };"#,
        expect![[r#"
            37..38: cannot infer the type of this number: it has no defining use — add a type annotation
            44..45: no field `z` on `struct { x: {number} }`
        "#]],
    );
}

#[test]
fn field_access_on_non_record() {
    check_diagnostics(
        r#"static f = fn { let n = 1; n.x; };"#,
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            27..28: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn chained_field_access_through_nested_records() {
    check_infer(
        r#"static f = fn { let a = struct { b = struct { c = "deep" } }; a.b.c };"#,
        expect![[r#"
            11..69 'fn { let a = stru...': fn() -> str
            14..69 '{ let a = struct ...': str
            20..21 'a': struct { b: struct { c: str } }
            24..60 'struct { b = stru...': struct { b: struct { c: str } }
            37..58 'struct { c = "dee...': struct { c: str }
            50..56 '"deep"': str
            62..63 'a': struct { b: struct { c: str } }
            62..65 'a.b': struct { c: str }
            62..67 'a.b.c': str
        "#]],
    );
}

#[test]
fn field_access_on_unannotated_param_requires_an_annotation() {
    // Exact structural equality can't run backwards from a field name, so
    // an undetermined receiver is reported (on the receiver — the
    // annotation goes there), not silently left dangling.
    check_diagnostics(
        r#"static f = fn (p) { p.x };"#,
        expect![[r#"
            20..21: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn shorthand_field_resolves_and_type_checks() {
    check_diagnostics(
        r#"static f = fn { let x = 1; let p: struct { x: usize } = struct { x, }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn shorthand_field_with_wrong_type_blames_the_shorthand() {
    // The shorthand's value is a normal reference to `s`, so the squiggle
    // lands on the shorthand name itself.
    check_diagnostics(
        r#"static f = fn { let s = "a"; let p: struct { s: usize } = struct { s, }; };"#,
        expect![[r#"
            67..68: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 36..55)
        "#]],
    );
}

#[test]
fn unresolved_shorthand_field_reports_unresolved_name() {
    check_diagnostics(
        r#"static f = fn { let p = struct { x, }; };"#,
        expect![[r#"
            33..34: unresolved name `x`
        "#]],
    );
}

#[test]
fn if_branches_with_mismatched_records_report_branch_mismatch() {
    // Join regression: records go through the existing join machinery
    // unchanged, so two branches disagreeing on a record type produce the
    // ordinary branch-mismatch diagnostic, verbatim.
    check_diagnostics(
        r#"static f = fn (c: bool) { if c { struct { x = 1 } } else { struct { x = "s" } } };"#,
        expect![[r#"
            33..49: type mismatch: expected `struct { x: str }`, found `struct { x: {number} }` (this branch has type `struct { x: str }` at 59..77)
        "#]],
    );
}

#[test]
fn record_literal_in_initializer_is_const_clean() {
    // Record construction is not a call: const-checking has nothing to say
    // about a pure record literal in an item initializer.
    check_diagnostics(
        r#"static p = struct { x = { let n: usize = 1; n }, y = "s" };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_signature_flows_across_items() {
    check_diagnostics(
        r#"
static origin: struct { x: usize, y: usize } = struct { x = 0, y = 0 };
static f = fn { let x = origin.x; };
"#,
        expect![[r#""#]],
    );
}

/// `is_fully_typed` digs into record fields: a record annotation with a
/// hole field is only a partial contract, so the item routes through group
/// inference and — the hole staying unfilled — the use site asks for an
/// annotation instead of the annotation acting as a firewall. A hole-free
/// record annotation is a full contract: its item is a firewall edge, and
/// editing its body never re-runs its caller's group inference.
#[test]
fn hole_field_record_annotation_is_not_a_firewall_contract() {
    // `panic` diverges, so nothing ever fills `y`: the group signature has
    // an undetermined field, and the use site requests an annotation
    // rather than silently publishing `y: _` as if it were a contract.
    check_diagnostics(
        r#"
static s: struct { x: usize, y: _ } = panic("boom");
static main = fn { s; };
"#,
        expect![[r#"
            73..74: cannot infer the type of `s` across items; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );

    // Positive: `a`'s annotation names every field, so it is a full
    // contract — `a` stays out of `b`'s binding group.
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: struct { x: usize } = struct { x = 1 };\n\
                   static b = fn { a.x; };\n";
    // Only `a`'s body changes; the annotation (the whole contract) is
    // identical, so dependents must backdate.
    let text_v2 = "static a: struct { x: usize } = struct { x = 2 };\n\
                   static b = fn { a.x; };\n";

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
    // `a`'s annotation is a full contract, so `b` never joins its group:
    // editing `a`'s body re-infers `a` alone and never runs `infer_group`.
    assert!(
        !log.iter().any(|entry| entry.contains("infer_group")),
        "a hole-free record annotation must be a firewall edge: {log:#?}"
    );
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

// ---- named types (`type Foo = struct { ... };`) ----

#[test]
fn type_decl_alone_is_clean() {
    check_diagnostics("type Foo = struct { x: usize, y: str };", expect![[r#""#]]);
}

#[test]
fn type_and_static_share_the_flat_namespace() {
    // One namespace: a `type` and a `static` of the same name collide via
    // the ordinary duplicate machinery.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static Foo = 5;
"#,
        expect![[r#"
            40..43: `Foo` is defined multiple times (first defined here at 6..9)
            46..47: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn type_rhs_must_be_a_struct_literal() {
    check_diagnostics(
        "type Foo = 5;",
        expect![[r#"
            11..12: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

#[test]
fn type_decl_fields_must_be_types() {
    // The retired `a: 5` spelling gets the targeted respell parse error, a
    // shorthand field has no type to read, and an unknown or value name in
    // a field gets the type-position errors.
    check_diagnostics(
        r#"
static five = 5;
type Foo = struct { a: 5, b, c: missing, d: five };
"#,
        expect![[r#"
            15..16: cannot infer the type of this number: it has no defining use — add a type annotation
            41..42: record fields are defined with `=` (`name = value`); `:` annotates a type
            44..45: expected a type for field `b`: `name: Type`
            50..57: unknown type `missing`
            62..66: `five` is not a type
        "#]],
    );
}

#[test]
fn annotation_resolves_named_type_and_field_projects() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo) -> usize { p.x };
"#,
        expect![[r#"
            44..72 'fn (p: Foo) -> us...': fn(Foo) -> usize
            48..49 'p': Foo
            65..72 '{ p.x }': usize
            67..68 'p': Foo
            67..70 'p.x': usize
        "#]],
    );
}

#[test]
fn value_item_in_type_position_is_an_error() {
    check_diagnostics(
        r#"
static double = fn (x: usize) -> usize { x + x };
static f = fn (p: double) { };
"#,
        expect![[r#"
            69..75: `double` is not a type
        "#]],
    );
}

#[test]
fn bare_type_name_is_not_a_value() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static x = Foo;
"#,
        expect![[r#"
            44..47: `Foo` is a type, not a value
        "#]],
    );
}

#[test]
fn construction_call_produces_the_named_type() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = 1 });
"#,
        expect![[r#"
            44..47 'Foo': fn(struct { x: usize }) -> Foo
            44..65 'Foo(struct { x = ...': Foo
            48..64 'struct { x = 1 }': struct { x: usize }
            61..62 '1': usize
        "#]],
    );
}

#[test]
fn construction_field_mismatch_cites_the_field_declaration() {
    // The squiggle lands on the wrong field's value; the hint points at the
    // field's declaration inside the `type` item.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = "s" });
"#,
        expect![[r#"
            61..64: type mismatch: expected `usize`, found `str` (expected `usize` because of this field declaration at 21..29)
        "#]],
    );
}

#[test]
fn construction_non_record_argument_cites_the_declaration() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(5);
"#,
        expect![[r#"
            48..49: type mismatch: expected `struct { x: usize }`, found `{number}` (expected `struct { x: usize }` because of `Foo`'s declaration at 6..9)
        "#]],
    );
}

#[test]
fn construction_takes_exactly_one_argument() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = 1 }, 2);
static q = Foo();
"#,
        expect![[r#"
            44..68: `Foo` takes exactly one argument (its underlying `struct` value), found 2 (`Foo` is defined here at 6..9)
            66..67: cannot infer the type of this number: it has no defining use — add a type annotation
            81..86: `Foo` takes exactly one argument (its underlying `struct` value), found 0 (`Foo` is defined here at 6..9)
        "#]],
    );
}

#[test]
fn named_type_is_distinct_from_its_underlying_record() {
    // No implicit nominal↔structural coercion in either direction. The
    // named-expected direction carries the construct-it hint.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static a: Foo = struct { x = 1 };
static b: struct { x: usize } = Foo(struct { x = 1 });
"#,
        expect![[r#"
            49..65: type mismatch: expected `Foo`, found `struct { x: {error} }`; `Foo` is a distinct type — construct it with `Foo(...)` (expected `Foo` because of this annotation at 43..46)
            99..120: type mismatch: expected `struct { x: usize }`, found `Foo` (expected `struct { x: usize }` because of this annotation at 77..96)
        "#]],
    );
}

#[test]
fn named_types_unify_by_declaration_not_shape() {
    // Same shape, different declarations: still different types.
    check_diagnostics(
        r#"
type Meters = struct { value: usize };
type Feet = struct { value: usize };
static len: Meters = Feet(struct { value = 3 });
"#,
        expect![[r#"
            98..124: type mismatch: expected `Meters`, found `Feet` (expected `Meters` because of this annotation at 89..95)
        "#]],
    );
}

#[test]
fn no_such_field_on_named_type_points_at_the_declaration() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo) { p.z };
"#,
        expect![[r#"
            60..61: no field `z` on `Foo` (the fields of `Foo` are declared here at 12..31)
        "#]],
    );
}

#[test]
fn equality_between_named_and_bare_record_is_a_type_error() {
    // Eq/Ne go through unification, so nominal vs structural is rejected
    // like any other mismatch.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static eq = Foo(struct { x = 1 }) == struct { x = 1 };
"#,
        expect![[r#"
            70..86: type mismatch: expected `Foo`, found `struct { x: {error} }`; `Foo` is a distinct type — construct it with `Foo(...)` (this operand has type `Foo` at 45..66)
        "#]],
    );
}

#[test]
fn construction_is_legal_in_const_contexts() {
    // Pure construction, not a user fn call: fine in an item initializer
    // (always a const context) and inside a `const { ... }` block.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static a = Foo(struct { x = 1 });
const b = const { Foo(struct { x = 2 }) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn recursive_type_declaration_does_not_hang() {
    // `Ty::Named` is identity, not expansion: a self-referential field
    // lowers to `Named(Foo)` and stops.
    check_diagnostics(
        r#"
type Foo = struct { next: Foo };
static f = fn (p: Foo) { p.next.next };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn assigning_to_a_type_name_reports_type_not_value() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static f = fn { Foo = 5; };
"#,
        expect![[r#"
            49..52: `Foo` is a type, not a value
        "#]],
    );
}

#[test]
fn hole_typed_binding_can_hold_a_named_type() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn { let p = Foo(struct { x = 1 }); p.x };
"#,
        expect![[r#"
            44..85 'fn { let p = Foo(...': fn() -> usize
            47..85 '{ let p = Foo(str...': usize
            53..54 'p': Foo
            57..60 'Foo': fn(struct { x: usize }) -> Foo
            57..78 'Foo(struct { x = ...': Foo
            61..77 'struct { x = 1 }': struct { x: usize }
            74..75 '1': usize
            80..81 'p': Foo
            80..83 'p.x': usize
        "#]],
    );
}

// ---- enums and variant types ----

#[test]
fn variant_construction_infers_the_variant_type() {
    // Precise types survive: a constructed value is `Shape::Circle`, not
    // `Shape`; a payload-less variant path IS the value.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static c = Shape::Circle(3);
static p = Shape::Point;
"#,
        expect![[r#"
            56..69 'Shape::Circle': fn(usize) -> Shape::Circle
            56..72 'Shape::Circle(3)': Shape::Circle
            70..71 '3': usize
            85..97 'Shape::Point': Shape::Point
        "#]],
    );
}

#[test]
fn variant_constructor_is_a_first_class_function() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let make = Shape::Circle;
    make(3)
};
"#,
        expect![[r#"
            56..104 'fn {     let make...': fn() -> Shape::Circle
            59..104 '{     let make = ...': Shape::Circle
            69..73 'make': fn(usize) -> Shape::Circle
            76..89 'Shape::Circle': fn(usize) -> Shape::Circle
            95..99 'make': fn(usize) -> Shape::Circle
            95..102 'make(3)': Shape::Circle
            100..101 '3': usize
        "#]],
    );
}

#[test]
fn same_variant_nested_join_keeps_precision() {
    // A whole nest of `if`s whose leaves agree on one variant: the join
    // (flattened by J1) resolves to that variant — no widening anywhere.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool, b: bool) {
    let s = if a { Shape::Circle(1) } else if b { Shape::Circle(2) } else { Shape::Circle(3) };
    s
};
"#,
        expect![[r#"
            56..183 'fn (a: bool, b: b...': fn(bool, bool) -> Shape::Circle
            60..61 'a': bool
            69..70 'b': bool
            78..183 '{     let s = if ...': Shape::Circle
            88..89 's': Shape::Circle
            92..174 'if a { Shape::Cir...': Shape::Circle
            95..96 'a': bool
            97..117 '{ Shape::Circle(1) }': Shape::Circle
            99..112 'Shape::Circle': fn(usize) -> Shape::Circle
            99..115 'Shape::Circle(1)': Shape::Circle
            113..114 '1': usize
            123..174 'if b { Shape::Cir...': Shape::Circle
            126..127 'b': bool
            128..148 '{ Shape::Circle(2) }': Shape::Circle
            130..143 'Shape::Circle': fn(usize) -> Shape::Circle
            130..146 'Shape::Circle(2)': Shape::Circle
            144..145 '2': usize
            154..174 '{ Shape::Circle(3) }': Shape::Circle
            156..169 'Shape::Circle': fn(usize) -> Shape::Circle
            156..172 'Shape::Circle(3)': Shape::Circle
            170..171 '3': usize
            180..181 's': Shape::Circle
        "#]],
    );
}

#[test]
fn mixed_variant_join_widens_to_the_enum() {
    // Family-aware voting: Circle and Point are one family (Shape); the
    // LUB within the family is the enum, with a conversion at each edge.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool) {
    let s = if a { Shape::Circle(1) } else { Shape::Point };
    s
};
"#,
        expect![[r#"
            56..139 'fn (a: bool) {   ...': fn(bool) -> Shape
            60..61 'a': bool
            69..139 '{     let s = if ...': Shape
            79..80 's': Shape
            83..130 'if a { Shape::Cir...': Shape
            86..87 'a': bool
            88..108 '{ Shape::Circle(1) }': Shape::Circle
            90..103 'Shape::Circle': fn(usize) -> Shape::Circle
            90..106 'Shape::Circle(1)': Shape::Circle
            104..105 '1': usize
            114..130 '{ Shape::Point }': Shape::Point
            116..128 'Shape::Point': Shape::Point
            136..137 's': Shape
        "#]],
    );
}

#[test]
fn cross_enum_join_is_still_incompatible() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
type Color = enum { Red };
static f = fn (a: bool) {
    let s = if a { Shape::Circle(1) } else { Color::Red };
    s
};
"#,
        expect![[r#"
            136..146: `if` branches have incompatible types: `Shape::Circle` vs `Color::Red`; add a type annotation to decide between them (this branch has type `Shape::Circle` at 110..126)
        "#]],
    );
}

#[test]
fn enum_annotation_accepts_any_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = Shape::Circle(3);
static p: Shape = Shape::Point;
static f = fn (a: bool) -> Shape {
    if a { Shape::Circle(1) } else { Shape::Point }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn variant_annotation_rejects_other_variants() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Circle = Shape::Point;
"#,
        expect![[r#"
            71..83: type mismatch: expected `Shape::Circle`, found `Shape::Point` (expected `Shape::Circle` because of this annotation at 55..68)
        "#]],
    );
}

#[test]
fn fn_demanding_a_variant_rejects_another_variant() {
    // The state-machine case: an API that demands one specific state.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static wants_circle = fn (s: Shape::Circle) {};
static f = fn { wants_circle(Shape::Point); };
"#,
        expect![[r#"
            122..134: type mismatch: expected `Shape::Circle`, found `Shape::Point`
        "#]],
    );
}

#[test]
fn let_mut_widens_but_plain_let_keeps_the_variant() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let p = Shape::Point;
    let mut s = Shape::Point;
    s = Shape::Circle(1);
};
"#,
        expect![[r#"
            56..144 'fn {     let p = ...': fn()
            59..144 '{     let p = Sha...': ()
            69..70 'p': Shape::Point
            73..85 'Shape::Point': Shape::Point
            99..100 's': Shape
            103..115 'Shape::Point': Shape::Point
            121..122 's': Shape
            125..138 'Shape::Circle': fn(usize) -> Shape::Circle
            125..141 'Shape::Circle(1)': Shape
            139..140 '1': usize
        "#]],
    );
}

#[test]
fn annotated_let_mut_keeps_precision() {
    // `let mut s: Shape::Circle` is an axiom: no widening, and another
    // variant is rejected on assignment.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s: Shape::Circle = Shape::Circle(1);
    s = Shape::Point;
};
"#,
        expect![[r#"
            118..130: type mismatch: expected `Shape::Circle`, found `Shape::Point` (expected `Shape::Circle` because of this annotation at 76..89)
        "#]],
    );
}

#[test]
fn unknown_variant_is_reported_with_the_declaration() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Missing;
"#,
        expect![[r#"
            63..70: `Shape` has no variant `Missing` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn variant_path_on_a_struct_type_item() {
    // `x` IS declared — as a field, one namespace over — so the answer
    // names the namespace and the escape rather than the generic
    // no-variants line. See
    // `qualified_path_naming_a_field_says_so_and_names_the_escape` for the
    // names that genuinely aren't there, which keep it.
    check_diagnostics(
        r#"
type Point = struct { x: usize };
static p = Point::x;
"#,
        expect![[r#"
            46..54: `x` is a field of `Point`, not a member — fields are reached through a value: `value.x` (`Point` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn variant_path_on_a_value_base() {
    check_diagnostics(
        r#"
static five = 5;
static x = five::Circle;
"#,
        expect![[r#"
            15..16: cannot infer the type of this number: it has no defining use — add a type annotation
            29..41: `five` is not a type; only an `enum` type has `::` variants
        "#]],
    );
}

#[test]
fn bare_enum_name_is_a_type_not_a_value() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static s = Shape;
"#,
        expect![[r#"
            49..54: `Shape` is a type, not a value
        "#]],
    );
}

#[test]
fn enum_cannot_be_constructed_directly() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static s = Shape(3);
"#,
        expect![[r#"
            49..57: `Shape` is an `enum`; construct it through one of its variants (`Shape::<variant>(...)`) (`Shape` is defined here at 6..11)
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn variant_constructor_arity_is_checked() {
    // Constructors are plain functions: wrong arity is the ordinary
    // ArgCountMismatch, wrong payload type the ordinary call-site mismatch.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static a = Shape::Circle(1, 2);
static b = Shape::Circle("no");
"#,
        expect![[r#"
            56..75: expected 1 argument(s), found 2
            102..106: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn payload_less_variant_is_not_callable() {
    check_diagnostics(
        r#"
type Shape = enum { Point };
static p = Shape::Point();
"#,
        expect![[r#"
            41..53: expression of type `Shape::Point` is not callable
        "#]],
    );
}

#[test]
fn variant_type_annotation_errors_mirror_lowering() {
    // Every silent `Ty::Error` from variant-path lowering has a diagnostic:
    // unknown variant, struct base, builtin base.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
type Point = struct { x: usize };
static a: Shape::Missing = 1;
static b: Point::Circle = 2;
static c: usize::Circle = 3;
"#,
        expect![[r#"
            82..96: `Shape` has no variant `Missing`
            112..125: `Point` has no variants (it is a `struct` type)
            141..154: `usize` has no variants (it is a builtin type)
        "#]],
    );
}

#[test]
fn enum_payloads_must_be_fully_written() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(_) };
"#,
        expect![[r#"
            28..29: a variant payload must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn field_access_on_an_enum_value_is_an_error() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static f = fn (s: Shape) { s.x };
"#,
        expect![[r#"
            67..68: no field `x` on `Shape` (`Shape` is an `enum`, declared here — it has variants, not fields at 14..36)
        "#]],
    );
}

#[test]
fn let_mut_widening_explains_a_later_mismatch() {
    // When the widened binding bites later, the mismatch carries the
    // binding hint ("inferred from its initializer") — which the ide layer
    // surfaces as an info-severity companion at the `let mut`.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s = Shape::Point;
    s = 5;
};
"#,
        expect![[r#"
            99..100: type mismatch: expected `Shape`, found `{number}` (`s` was inferred to have type `Shape` from its initializer at 73..74)
        "#]],
    );
}

// ---- match ----

#[test]
fn match_arms_same_variant_keep_precision() {
    // Every arm produces the same variant: the match keeps the precise
    // variant type — no widening, no conversions (the join's family vote LUBs
    // identical leaves to themselves).
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    let t = match s {
        ::Circle(r) => Shape::Circle(r + 1),
        ::Point => Shape::Circle(0),
    };
    t
};
"#,
        expect![[r#"
            56..190 'fn (s: Shape) {  ...': fn(Shape) -> Shape::Circle
            60..61 's': Shape
            70..190 '{     let t = mat...': Shape::Circle
            80..81 't': Shape::Circle
            84..181 'match s {        ...': Shape::Circle
            90..91 's': Shape
            111..112 'r': usize
            117..130 'Shape::Circle': fn(usize) -> Shape::Circle
            117..137 'Shape::Circle(r + 1)': Shape::Circle
            131..132 'r': usize
            131..136 'r + 1': usize
            135..136 '1': usize
            158..171 'Shape::Circle': fn(usize) -> Shape::Circle
            158..174 'Shape::Circle(0)': Shape::Circle
            172..173 '0': usize
            187..188 't': Shape::Circle
        "#]],
    );
}

#[test]
fn match_arms_mixed_variants_lub_to_enum() {
    // Arms produce different variants of one enum: the family LUB is the
    // enum, with the conversion on each arm edge.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    let t = match s {
        ::Circle(r) => Shape::Circle(r + 1),
        ::Point => Shape::Point,
    };
    t
};
"#,
        expect![[r#"
            56..186 'fn (s: Shape) {  ...': fn(Shape) -> Shape
            60..61 's': Shape
            70..186 '{     let t = mat...': Shape
            80..81 't': Shape
            84..177 'match s {        ...': Shape
            90..91 's': Shape
            111..112 'r': usize
            117..130 'Shape::Circle': fn(usize) -> Shape::Circle
            117..137 'Shape::Circle(r + 1)': Shape::Circle
            131..132 'r': usize
            131..136 'r + 1': usize
            135..136 '1': usize
            158..170 'Shape::Point': Shape::Point
            183..184 't': Shape
        "#]],
    );
}

#[test]
fn match_nested_in_if_flattens_into_one_join() {
    // A match in an if's branch tail contributes its arms' leaves to the
    // enclosing join (J1 through match): the culprit squiggle lands on the
    // one disagreeing *arm tail*, and the sibling hints name leaves from
    // both constructs.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape, c: bool) {
    let x = if c {
        match s {
            ::Circle(r) => r,
            ::Point => "no",
        }
    } else {
        2
    };
    x
};
"#,
        expect![[r#"
            171..175: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 145..146)
            208..209: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn match_nonexhaustive_missing_one_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            85..90: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn match_nonexhaustive_missing_several_variants() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            103..108: this `match` does not cover `Shape::Pair`, `Shape::Point`
        "#]],
    );
}

#[test]
fn match_nonexhaustive_partial_coverage_offers_add_missing_arms_fix() {
    // Partially-covered match: only the missing arm is added, as a pure
    // insertion right after the last existing one — the buffer's own
    // `\n    }` that follows is left untouched (so the insert carries no
    // trailing newline of its own; adding one would leave a blank line).
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );
    let last_arm_end = text.find("::Circle(r) => r,").unwrap() + "::Circle(r) => r,".len();
    assert_eq!(u32::from(fix.edits[0].range.start()), last_arm_end as u32);
    assert_eq!(
        fix.edits[0].range.start(),
        fix.edits[0].range.end(),
        "a pure insertion"
    );

    // Applying the edit never introduces a new diagnostic: the generated
    // arm's `panic(...)` body joins with the other arm's `usize` because
    // `panic` types as `!`.
    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_last_arm_without_a_trailing_comma_gets_one_inserted() {
    // A comma-less last arm is legal (grammar.rs's `match_arm`: nothing
    // checks for a comma right before the arm list's own `}`) — inserting
    // straight after it would otherwise glue two arms into unparseable
    // text (`r        ::Point`).
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        ",\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_last_arm_ending_in_a_block_needs_no_comma_but_tolerates_one() {
    // A `}`-bodied last arm never needs a comma either way (the grammar
    // `eat`s one if present, between arms and before the list's own `}`
    // alike) — prefixing one unconditionally whenever the last token isn't
    // already a comma is therefore always safe, block-bodied or not.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        ::Circle(r) => { let _ = r; }
    };
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        ",\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_empty_arm_list_offers_add_all_variants_fix() {
    // Empty arm list: `match s {}` — the checker's own non-exhaustive
    // diagnostic covers it (every variant is "uncovered" when there are no
    // arms at all), so the same fix construction handles it with no
    // special-casing and no separate diagnostic.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("this `match` does not cover")
    );
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );
    let anchor = text.find("match s {").unwrap() + "match s {".len();
    assert_eq!(u32::from(fix.edits[0].range.start()), anchor as u32);
    assert_eq!(u32::from(fix.edits[0].range.end()), anchor as u32);
}

#[test]
fn match_empty_arm_list_with_space_before_brace_still_gets_indented() {
    // `match s { }` (a space, not touching braces) still has no line break
    // between the anchor and `}` — the synthesized trailing newline keys
    // on that, not on the anchor and `}` coinciding exactly.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s { };
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_add_missing_arms_fix_leaves_a_trailing_comment_alone() {
    // A comment sitting between the last arm and `}` is never dropped: the
    // insertion lands right after the last arm (trivia attaches to the
    // following real token, not the arm), pushing the comment down rather
    // than overwriting it — the tradeoff a range-replace would have had.
    let text = "type Shape = enum { Circle(usize), Point };\n\
                 static f = fn (s: Shape) -> usize {\n    \
                 match s {\n        \
                 ::Circle(r) => r,\n        \
                 // keep me\n    \
                 }\n\
                 };\n";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    assert!(
        patched.contains("// keep me"),
        "the comment must survive the edit: {patched}"
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_add_missing_arms_fix_names_binders_by_payload_arity() {
    // Unit / one-payload / two-payload variants, in declaration order:
    // `::Point` gets no parens, `::Circle` gets one binder (`v`), `::Pair`
    // gets one per field (`v1, v2`).
    let text = r#"
type Shape = enum { Point, Circle(usize), Pair(usize, str) };
static f = fn (s: Shape) {
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Pair(v1, v2) => panic(\"unhandled ::Pair\"),\n    "
    );
}

#[test]
fn match_add_missing_arms_fix_projects_through_a_borrowed_scrutinee() {
    // A borrowed enum scrutinee: match still dispatches on the enum behind
    // the borrow (the projection lens), so the fix fires and computes the
    // same missing-variant set as the unborrowed case.
    let text = "type Shape = enum { Circle(usize), Point };\n\
                 static f = fn::<@a>(s: Shape.&::<@a>) -> () {\n    \
                 match s {\n        ::Circle(r) => { let _ = r; },\n    };\n\
                 };\n";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );
}

#[test]
fn match_add_missing_arms_fix_on_a_variant_typed_scrutinee_covers_only_that_variant() {
    // Variant-typed scrutinees only ever demand ONE variant (the
    // scrutinee's own) — `uncovered` already reflects that, so the fix
    // generates a single arm even though the enum has two.
    let text = r#"
type State = enum { Idle, Running(usize) };
static f = fn (s: State::Running) {
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Running(v) => panic(\"unhandled ::Running\"),\n    "
    );
}

#[test]
fn match_add_missing_arms_fix_avoids_shadowing_a_visible_local() {
    // A `v` already visible at the match's own scope (a `let` right before
    // it) is not shadowed by the generated payload binder — it gets
    // suffixed until free instead.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    let v: usize = 1;
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v_) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );
}

#[test]
fn match_without_catch_all_offers_no_fix() {
    // The sibling diagnostic for a non-enum scrutinee with no `_`/binding
    // arm: there is no variant list to enumerate, so no fix is offered
    // (this fix is scoped to `NonExhaustiveMatch` only).
    let text = r#"
static f = fn (n: usize) -> usize {
    match n { }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("this `match` does not cover every possible")
    );
    assert!(diagnostics[0].fix.is_none());
}

#[test]
fn match_wildcard_covers_everything() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        _ => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn match_arm_after_wildcard_is_unreachable() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        _ => 0,
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            119..130: unreachable arm: `Circle` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn match_duplicate_variant_arm_is_unreachable() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Circle(d) => d + d,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            129..140: unreachable arm: `Circle` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn match_on_variant_typed_scrutinee_other_variant_is_unreachable() {
    // The scrutinee can only be a `State::Running`; the `Idle` arm is dead
    // (warning), the same-variant arm covers, and no wildcard is needed.
    check_diagnostics(
        r#"
type State = enum { Idle, Running(usize) };
static f = fn (s: State::Running) -> usize {
    match s {
        ::Idle => 0,
        ::Running(n) => n,
    }
};
"#,
        expect![[r#"
            112..118: this arm is unreachable: the scrutinee is a `State::Running`
        "#]],
    );
}

#[test]
fn match_binding_arm_binds_scrutinee_type() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        whole => 1,
    }
};
"#,
        expect![[r#"
            56..122 'fn (s: Shape) -> ...': fn(Shape) -> usize
            60..61 's': Shape
            79..122 '{     match s {  ...': usize
            85..120 'match s {        ...': usize
            91..92 's': Shape
            103..108 'whole': Shape
            112..113 '1': usize
        "#]],
    );
}

#[test]
fn match_payload_binding_types_come_from_the_declaration() {
    check_infer(
        r#"
type Shape = enum { Pair(usize, str) };
static f = fn (s: Shape) {
    match s {
        ::Pair(n, text) => { print(text); n },
        _ => 0,
    }
};
"#,
        expect![[r#"
            52..152 'fn (s: Shape) {  ...': fn(Shape) -> usize
            56..57 's': Shape
            66..152 '{     match s {  ...': usize
            72..150 'match s {        ...': usize
            78..79 's': Shape
            97..98 'n': usize
            100..104 'text': str
            109..127 '{ print(text); n }': usize
            111..116 'print': fn(str)
            111..122 'print(text)': ()
            117..121 'text': str
            124..125 'n': usize
            142..143 '0': usize
        "#]],
    );
}

#[test]
fn match_pattern_arity_mismatch() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r, extra) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            103..121: `Circle` has 1 payload, this pattern names 2
        "#]],
    );
}

#[test]
fn match_sigil_variant_missing_payloads_needs_them() {
    // `::Circle` names a payload-carrying variant with no payload list: a
    // variant pattern must name (or hole) its payloads, so this is an
    // arity error — the direct successor of the retired bare-`Circle`
    // reinterpretation case.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle => 1,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            103..111: `Circle` has 1 payload, this pattern names 0
        "#]],
    );
}

#[test]
fn match_sigil_and_qualified_patterns_resolve_identically() {
    // Both spellings of both variants — elided `::Variant` and fully
    // qualified `Shape::Variant` — resolve against the scrutinee's enum:
    // fully covered, no diagnostics, and the payload binding types line up.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        Shape::Point => 0,
    }
};
static g = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn match_bind_arm_named_like_variant_binds_and_warns() {
    // `Point` (bare, no `::`) is a binding, not a variant match — but its
    // name shadows the `Point` variant, almost always a migration mistake.
    // It stays well-typed (a full catch-all binding the whole value), so
    // this is only a warning pointing at the `::Point` spelling.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        Point => 0,
    }
};
"#,
        expect![[r#"
            129..134: `Point` binds the whole value; write `::Point` (or `Shape::Point`) to match the variant (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_bind_arm_named_like_variant_is_still_a_catch_all() {
    // A bind named like a variant is a *full* catch-all (never narrowed),
    // so a trailing `_` arm after it is unreachable — proof the reinterpretation
    // is gone. Two diagnostics: the shadow-name warning and the dead arm.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        Point => 0,
        _ => 1,
    }
};
"#,
        expect![[r#"
            129..134: `Point` binds the whole value; write `::Point` (or `Shape::Point`) to match the variant (`Shape` is defined here at 6..11)
            149..150: unreachable arm: a previous arm already matches anything
        "#]],
    );
}

#[test]
fn match_bind_arm_named_like_payload_variant_is_not_an_error() {
    // G25's motivating scenario: a bare bind named like a
    // *payload-carrying* variant used to require a `PatArity` error under
    // reinterpretation. Now it just binds the whole value — no error, only
    // the shadow-name warning.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Circle => 0,
    }
};
"#,
        expect![[r#"
            103..109: `Circle` binds the whole value; write `::Circle` (or `Shape::Circle`) to match the variant (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_wrong_enum_pattern_is_rejected() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
type State = enum { Idle, Running(usize) };
static f = fn (s: Shape) -> usize {
    match s {
        State::Idle => 0,
        _ => 1,
    }
};
"#,
        expect![[r#"
            147..158: this pattern matches `State::Idle`, but the scrutinee is a `Shape` (`State` is defined here at 50..55)
        "#]],
    );
}

#[test]
fn match_pattern_no_such_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Square(x) => x,
        _ => 0,
    }
};
"#,
        expect![[r#"
            103..119: `Shape` has no variant `Square` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_non_enum_scrutinee_rejects_variant_patterns() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        ::Circle(r) => r,
        _ => 0,
    }
};
"#,
        expect![[r#"
            59..70: a variant pattern needs an enum scrutinee; only `_` or a binding can match a `usize`
        "#]],
    );
}

#[test]
fn match_non_enum_scrutinee_needs_a_catch_all() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n { }
};
"#,
        expect![[r#"
            41..46: this `match` does not cover every possible `usize`; add a `_` arm
        "#]],
    );
}

#[test]
fn match_in_const_fn_is_clean() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static area_ish = const fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r * r,
        ::Point => 0,
    }
};
static x = area_ish(Shape::Circle(3));
"#,
        expect![[r#""#]],
    );
}

#[test]
fn loop_types_as_its_break_value() {
    // The owner's motivating accumulator: break values are the witnesses of
    // one join, and that join's result is the loop's type.
    check_infer(
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
            14..166 'fn () -> usize { ...': fn() -> usize
            29..166 '{     let mut acc...': usize
            43..46 'acc': usize
            49..50 '0': usize
            64..65 'i': usize
            68..69 '0': usize
            75..164 'loop {         if...': usize
            80..164 '{         if i ==...': ()
            90..115 'if i == 10 { brea...': ()
            93..94 'i': usize
            93..100 'i == 10': bool
            98..100 '10': usize
            101..115 '{ break acc; }': ()
            103..112 'break acc': !
            109..112 'acc': usize
            125..128 'acc': usize
            131..134 'acc': usize
            131..138 'acc + i': usize
            137..138 'i': usize
            148..149 'i': usize
            152..153 'i': usize
            152..157 'i + 1': usize
            156..157 '1': usize
        "#]],
    );
}

#[test]
fn breakless_loop_types_never() {
    // …and so does the fn around it: the body diverges and nothing ever
    // said what `f` produces, so the return type defaults to `!` instead
    // of staying an unpinned `_`.
    check_infer(
        "static f = fn { loop { } };",
        expect![[r#"
            11..26 'fn { loop { } }': fn() -> !
            14..26 '{ loop { } }': !
            16..24 'loop { }': !
            21..24 '{ }': ()
        "#]],
    );
}

#[test]
fn bare_break_is_a_unit_witness() {
    // `break;` carries `()` as the loop's value.
    check_infer(
        "static f = fn { loop { break; } };",
        expect![[r#"
            11..33 'fn { loop { break...': fn()
            14..33 '{ loop { break; } }': ()
            16..31 'loop { break; }': ()
            21..31 '{ break; }': !
            23..28 'break': !
        "#]],
    );
}

#[test]
fn loop_break_values_join_widens_variants() {
    // Break values go through the same join seam as `if` branches and
    // `match` arms: mixed variants of one enum LUB to the enum.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (stop: bool) {
    let s = loop {
        if stop { break Shape::Circle(1); };
        break Point_or(stop);
    };
};
static Point_or = fn (b: bool) -> Shape::Point { Shape::Point };
"#,
        expect![[r#"
            56..176 'fn (stop: bool) {...': fn(bool)
            60..64 'stop': bool
            72..176 '{     let s = loo...': ()
            82..83 's': Shape
            86..173 'loop {         if...': Shape
            91..173 '{         if stop...': !
            101..136 'if stop { break S...': ()
            104..108 'stop': bool
            109..136 '{ break Shape::Ci...': ()
            111..133 'break Shape::Circ...': !
            117..130 'Shape::Circle': fn(usize) -> Shape::Circle
            117..133 'Shape::Circle(1)': Shape::Circle
            131..132 '1': usize
            146..166 'break Point_or(stop)': !
            152..160 'Point_or': fn(bool) -> Shape::Point
            152..166 'Point_or(stop)': Shape::Point
            161..165 'stop': bool
            196..241 'fn (b: bool) -> S...': fn(bool) -> Shape::Point
            200..201 'b': bool
            225..241 '{ Shape::Point }': Shape::Point
            227..239 'Shape::Point': Shape::Point
        "#]],
    );
}

#[test]
fn loop_break_value_checks_against_the_return_annotation() {
    check_diagnostics(
        r#"
static f = fn () -> usize {
    loop {
        break "text";
    }
};
"#,
        expect![[r#"
            33..67: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 18..26)
        "#]],
    );
}

#[test]
fn nested_loops_inner_break_does_not_exit_the_outer() {
    // The inner loop's break is the *inner* loop's value; the outer loop
    // has no value-carrying break, so it types `!` and the fn returns `!`
    // — pinned by the (clean) types below.
    check_infer(
        r#"
static f = fn {
    loop {
        let n = loop { break 1; };
        n;
    }
};
"#,
        expect![[r#"
            12..81 'fn {     loop {  ...': fn() -> !
            15..81 '{     loop {     ...': !
            21..79 'loop {         le...': !
            26..79 '{         let n =...': ()
            40..41 'n': {number}
            44..61 'loop { break 1; }': {number}
            49..61 '{ break 1; }': !
            51..58 'break 1': !
            57..58 '1': {number}
            71..72 'n': {number}
        "#]],
    );
}

#[test]
fn break_outside_loop_errors() {
    check_diagnostics(
        "static f = fn { break 1; };",
        expect![[r#"
            16..23: `break` outside of a loop: there is no enclosing `loop` to exit
            22..23: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn continue_outside_loop_errors() {
    check_diagnostics(
        "static f = fn { continue; };",
        expect![[r#"
            16..24: `continue` outside of a loop: there is no enclosing `loop` to restart
        "#]],
    );
}

#[test]
fn dangling_break_at_top_level_errors() {
    check_diagnostics(
        "static x = break 1;",
        expect![[r#"
            11..18: `break` outside of a loop: there is no enclosing `loop` to exit
            17..18: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn break_inside_fn_literal_does_not_escape_the_loop() {
    // A fn body is a fresh context: `break` never crosses a fn boundary,
    // so this is the same outside-a-loop error.
    check_diagnostics(
        r#"
static f = fn {
    loop {
        let g = fn { break 1; };
        g();
    }
};
"#,
        expect![[r#"
            49..56: `break` outside of a loop: there is no enclosing `loop` to exit
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn break_inside_const_block_does_not_escape_the_loop() {
    // A `const` block is a compile-time unit of its own (MIR lowers it to
    // a separate body), so it bounds the loop context like a fn literal.
    check_diagnostics(
        r#"
static f = fn {
    loop {
        let x = const { break 1; };
    }
};
"#,
        expect![[r#"
            52..59: `break` outside of a loop: there is no enclosing `loop` to exit
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn loop_in_const_fn_is_clean() {
    check_diagnostics(
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
static x = sum();
"#,
        expect![[r#""#]],
    );
}

// ---- `return` ---------------------------------------------------------

#[test]
fn return_with_a_value_types_never_and_checks_the_operand() {
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    return n + 1;
    0
};
"#,
        expect![[r#"
            12..62 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..62 '{     return n + ...': usize
            41..53 'return n + 1': !
            48..49 'n': usize
            48..53 'n + 1': usize
            52..53 '1': usize
            59..60 '0': usize
        "#]],
    );
}

#[test]
fn bare_return_is_a_unit_return() {
    // `return;` returns `()` — the same value a bare `break;` carries.
    check_infer(
        r#"
static f = fn (c: bool) -> () {
    if c { return; };
};
"#,
        expect![[r#"
            12..56 'fn (c: bool) -> (...': fn(bool)
            16..17 'c': bool
            31..56 '{     if c { retu...': ()
            37..53 'if c { return; }': ()
            40..41 'c': bool
            42..53 '{ return; }': ()
            44..50 'return': !
        "#]],
    );
}

#[test]
fn return_as_a_block_tail_is_clean() {
    check_diagnostics(
        "static f = fn (n: usize) -> usize { return n };",
        expect![""],
    );
}

#[test]
fn return_in_one_if_branch_lets_the_other_branch_win() {
    // The `Never` join: a diverging branch does not vote, so `x` takes the
    // surviving branch's type. Both the tail spelling (`return 0`) and the
    // statement spelling (`return 0;`) diverge.
    check_infer(
        r#"
static f = fn (c: bool) -> usize {
    let x = if c { 1 } else { return 0 };
    let y = if c { 2 } else { return 0; };
    x + y
};
"#,
        expect![[r#"
            12..132 'fn (c: bool) -> u...': fn(bool) -> usize
            16..17 'c': bool
            34..132 '{     let x = if ...': usize
            44..45 'x': usize
            48..76 'if c { 1 } else {...': usize
            51..52 'c': bool
            53..58 '{ 1 }': usize
            55..56 '1': usize
            64..76 '{ return 0 }': !
            66..74 'return 0': !
            73..74 '0': usize
            86..87 'y': usize
            90..119 'if c { 2 } else {...': usize
            93..94 'c': bool
            95..100 '{ 2 }': usize
            97..98 '2': usize
            106..119 '{ return 0; }': !
            108..116 'return 0': !
            115..116 '0': usize
            125..126 'x': usize
            125..130 'x + y': usize
            129..130 'y': usize
        "#]],
    );
}

#[test]
fn return_from_inside_loop_match_and_nested_blocks_is_clean() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape, n: usize) -> usize {
    { { if n == 0 { return 0; }; }; };
    loop {
        match s {
            ::Circle(r) => return r,
            ::Point => return n,
        };
    }
};
"#,
        expect![""],
    );
}

#[test]
fn return_mismatch_blames_the_operand_and_cites_the_return_type() {
    // The annotated case: the OPERAND carries the squiggle and the written
    // return type is the related note — the identical treatment the body's
    // tail expression gets, because it is the identical check.
    check_diagnostics(
        r#"
static f = fn (c: bool) -> usize {
    if c { return "text"; };
    0
};
"#,
        expect![[r#"
            54..60: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn bare_return_mismatch_blames_the_return_keyword() {
    // Nothing else exists to blame, so the `return` itself is squiggled.
    check_diagnostics(
        r#"
static f = fn (c: bool) -> usize {
    if c { return; };
    0
};
"#,
        expect![[r#"
            47..53: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn return_pins_an_inferred_return_type() {
    // No `-> T` annotation anywhere: the `return` is the only thing that
    // says what `f` produces, and it pins the signature exactly as a tail
    // expression would.
    check_infer(
        r#"
static f = fn (c: bool) {
    if c { return "yes"; };
    "no"
};
"#,
        expect![[r#"
            12..65 'fn (c: bool) {   ...': fn(bool) -> str
            16..17 'c': bool
            25..65 '{     if c { retu...': str
            31..53 'if c { return "ye...': ()
            34..35 'c': bool
            36..53 '{ return "yes"; }': ()
            38..50 'return "yes"': !
            45..50 '"yes"': str
            59..63 '"no"': str
        "#]],
    );
}

#[test]
fn return_and_tail_must_agree_on_an_inferred_return_type() {
    // The flip side: `return` feeds the SAME constraint the tail does, so
    // two disagreeing producers are a mismatch, not a silent widening.
    check_diagnostics(
        r#"
static f = fn (c: bool) {
    if c { return "yes"; };
    0
};
"#,
        expect![[r#"
            59..60: type mismatch: expected `str`, found `{number}`
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_returns_from_that_literal() {
    // THE semantics people get wrong. `inner`'s `return` produces
    // `inner`'s value; the outer fn carries on and returns a `str`. If it
    // returned from the outer fn instead, `-> usize` on `inner` and
    // `-> str` on the outer literal could not both be clean.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> str {
    let inner = fn (m: usize) -> usize { return m + 1; };
    if inner(n) == 0 { return "zero"; };
    "more"
};
"#,
        expect![""],
    );
}

#[test]
fn return_in_a_nested_fn_literal_checks_against_that_literal() {
    // Same boundary from the failing side: the inner literal's own return
    // type is what the inner `return` is checked against — the outer
    // `-> str` never enters the picture.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let inner = fn () -> usize { return "text"; };
    "ok"
};
"#,
        expect![[r#"
            67..73: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 49..57)
        "#]],
    );
}

#[test]
fn return_in_a_const_fn_is_clean() {
    check_diagnostics(
        r#"
static clamped = const fn (n: usize) -> usize {
    if n > 10 { return 10; };
    n
};
static x = clamped(42);
"#,
        expect![""],
    );
}

#[test]
fn return_in_a_const_block_is_reserved() {
    // REVERSING what the `return` arc first shipped (a `return` here used
    // to yield the CONST BLOCK's value, like a `break` bound by it).
    // Conceptually it bails from the OUTER fn body — which needs
    // cross-body machinery no v1 pass has — so v1 refuses it outright
    // rather than picking the reachable-but-wrong reading.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { if true { return 42; }; 0 };
    "ok"
};
"#,
        expect![[r#"
            64..73: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_const_block_reports_once() {
    // The reservation is the whole story: the operand is inferred (so its
    // contents still get types and diagnostics) but nothing demands a type
    // of it, so neither the block's expectation nor a bare literal in it
    // adds a sibling diagnostic.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { return "text"; };
    "ok"
};
static g = const { return 1; };
"#,
        expect![[r#"
            54..67: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
            103..111: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_fn_literal_nested_in_a_const_block_is_fine() {
    // The reservation is about the `const` block being the NEAREST body.
    // A `fn` literal inside one is its own body, so its `return` leaves
    // the literal exactly as anywhere else — nothing to reserve.
    check_diagnostics(
        r#"
static f = fn () -> str {
    let x: usize = const { let inner = fn () -> usize { return 7; }; 0 };
    "ok"
};
"#,
        expect![""],
    );
}

#[test]
fn return_outside_a_function_errors() {
    // An item initializer's own top level is a value expression, not a
    // function body — there is nothing to leave. The refusal is the whole
    // story: nothing here demanded a type of the operand, so the bare
    // literal must not add a no-defining-use sibling.
    check_diagnostics(
        "static x = return 1;",
        expect![[r#"
            11..19: `return` outside of a function: there is no enclosing `fn` body to return from
        "#]],
    );
}

#[test]
fn statements_after_a_return_still_check() {
    // No unreachable-code lint yet (that is its own round) — but the
    // statements after a `return` must still type, resolve and hover
    // exactly as written, and must not manufacture errors of their own.
    check_infer(
        r#"
static f = fn (n: usize) -> usize {
    return n;
    let doubled = n + n;
    doubled
};
"#,
        expect![[r#"
            12..89 'fn (n: usize) -> ...': fn(usize) -> usize
            16..17 'n': usize
            35..89 '{     return n;  ...': usize
            41..49 'return n': !
            48..49 'n': usize
            59..66 'doubled': usize
            69..70 'n': usize
            69..74 'n + n': usize
            73..74 'n': usize
            80..87 'doubled': usize
        "#]],
    );
}

// ---- record destructuring, `pub` reservation, field assignment ----

#[test]
fn let_record_destructure_binds_correct_types() {
    check_infer(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = "s" }; };"#,
        expect![[r#"
            11..66 'fn { let struct {...': fn()
            14..66 '{ let struct { x,...': ()
            29..30 'x': {number}
            32..33 'y': str
            38..63 'struct { x = 1, y...': struct { x: {number}, y: str }
            51..52 '1': {number}
            58..61 '"s"': str
        "#]],
    );
}

#[test]
fn let_record_destructure_rename_binds_only_the_new_name() {
    // `x` is not bound under its own name; only the rename `a` is.
    check_diagnostics(
        r#"static f = fn { let struct { x as a } = struct { x = 1 }; let b = a; let c = x; };"#,
        expect![[r#"
            53..54: cannot infer the type of this number: it has no defining use — add a type annotation
            77..78: unresolved name `x`
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_field_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            20..32: pattern does not mention field `y`; add `..` to ignore it
            48..49: cannot infer the type of this number: it has no defining use — add a type annotation
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_several_fields_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x = 1, y = 2, z = 3 }; };"#,
        expect![[r#"
            20..32: pattern does not mention fields `y`, `z`; add `..` to ignore them
            48..49: cannot infer the type of this number: it has no defining use — add a type annotation
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
            62..63: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_record_destructure_with_rest_ignores_missing_fields() {
    check_diagnostics(
        r#"static f = fn { let struct { x, .. } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            52..53: cannot infer the type of this number: it has no defining use — add a type annotation
            59..60: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_record_destructure_unknown_field_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x, z } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            20..35: no field `z` on `struct { x: {number}, y: {number} }`
            20..35: pattern does not mention field `y`; add `..` to ignore it
            51..52: cannot infer the type of this number: it has no defining use — add a type annotation
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_bare_record_pattern_needs_annotation_without_one() {
    // No annotation and no newtype wrapper: nothing pins the initializer's
    // type down before the pattern must be checked against it.
    check_diagnostics(
        r#"static f = fn (mk: fn() -> struct { x: usize }) { let struct { x } = mk(); };"#,
        expect![[r#""#]],
    );
    // `x` is a free parameter of `f` itself (no call site anywhere pins its
    // type — unlike a group-inferred callee, nothing forces it concrete),
    // so it is still genuinely undetermined when the pattern is checked.
    check_diagnostics(
        r#"static f = fn (x) { let struct { y } = x; };"#,
        expect![[r#"
            24..36: cannot destructure this pattern: its type is not known here; add a type annotation
        "#]],
    );
}

#[test]
fn param_record_destructure_binds_correct_types() {
    check_infer(
        r#"static f = fn (struct { x, y }: struct { x: usize, y: str }) { x }; "#,
        expect![[r#"
            11..66 'fn (struct { x, y...': fn(struct { x: usize, y: str }) -> usize
            24..25 'x': usize
            27..28 'y': str
            61..66 '{ x }': usize
            63..64 'x': usize
        "#]],
    );
}

#[test]
fn param_bare_record_destructure_needs_annotation() {
    check_diagnostics(
        r#"static f = fn (struct { x }) { x };"#,
        expect![[r#"
            15..27: cannot destructure this pattern: its type is not known here; add a type annotation
        "#]],
    );
}

#[test]
fn let_newtype_destructure_binds_underlying_fields() {
    check_infer(
        r#"
type Foo = struct { x: usize, y: str };
static f = fn (v: Foo) { let Foo(struct { x, y }) = v; x };
"#,
        expect![[r#"
            52..99 'fn (v: Foo) { let...': fn(Foo) -> usize
            56..57 'v': Foo
            64..99 '{ let Foo(struct ...': usize
            83..84 'x': usize
            86..87 'y': str
            93..94 'v': Foo
            96..97 'x': usize
        "#]],
    );
}

#[test]
fn param_newtype_destructure_infers_param_type_without_annotation() {
    // `Foo(...)` names its own type — no annotation needed, mirroring
    // construction's callee.
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn (Foo(struct { x })) { x };
"#,
        expect![[r#"
            44..72 'fn (Foo(struct { ...': fn(Foo) -> usize
            61..62 'x': usize
            67..72 '{ x }': usize
            69..70 'x': usize
        "#]],
    );
}

#[test]
fn newtype_destructure_wrong_named_type_errors() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
type Bar = struct { x: usize };
static f = fn (v: Bar) { let Foo(struct { x }) = v; };
"#,
        expect![[r#"
            114..115: type mismatch: expected `Foo`, found `Bar`
        "#]],
    );
}

#[test]
fn newtype_destructure_unknown_type_errors() {
    check_diagnostics(
        r#"static f = fn { let Bogus(struct { x }) = 1; };"#,
        expect![[r#"
            20..39: `Bogus` does not name a type
            42..43: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_destructure_unknown_field_still_types_it_as_error_not_cascading() {
    // The unknown field's binding recovers as `{error}` (infectious and
    // silent) rather than blocking the rest of the pattern from checking.
    check_diagnostics(
        r#"static f = fn { let struct { x, z, .. } = struct { x = 1 }; let n: usize = z; };"#,
        expect![[r#"
            20..39: no field `z` on `struct { x: {number} }`
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn per_binding_mut_in_record_pattern_allows_assignment() {
    check_diagnostics(
        r#"static f = fn { let struct { mut x, y } = struct { x = 1, y = 2 }; x = 3; };"#,
        expect![[r#"
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
            62..63: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_pattern_binding_without_mut_is_immutable() {
    check_diagnostics(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = 2 }; x = 3; };"#,
        expect![[r#"
            51..52: cannot infer the type of this number: it has no defining use — add a type annotation
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
            63..64: cannot assign to `x`: it is not declared `mut` (`x` is declared without `mut` here at 29..30)
        "#]],
    );
}

#[test]
fn let_mut_on_a_destructuring_pattern_is_a_syntax_error() {
    check_diagnostics(
        r#"static f = fn { let mut struct { x } = struct { x = 1 }; };"#,
        expect![[r#"
            20..36: `mut` applies to individual bindings in a destructuring pattern
            52..53: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn field_assign_on_a_mut_root_is_clean() {
    // Mutability is transitive from the binding: a `mut` root makes every
    // field of it assignable — no per-field `mut` exists.
    check_diagnostics(
        r#"static f = fn (mut p: struct { x: usize }) -> usize { p.x = 1; p.x };"#,
        expect![[r#""#]],
    );
}

#[test]
fn nested_field_assign_on_a_mut_root_is_clean() {
    check_diagnostics(
        r#"static f = fn { let mut p = struct { a = struct { b = 1 } }; p.a.b = 2; };"#,
        expect![[r#"
            54..55: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn field_assign_on_an_immutable_root_blames_the_root() {
    // The squiggle sits on the root name inside the place — the fix
    // (adding `mut`) belongs to the binding, not the field — and the
    // message spells both the place and the root out.
    check_diagnostics(
        r#"static f = fn (p: struct { x: usize }) { p.x = 1; };"#,
        expect![[r#"
            41..42: cannot assign to `p.x`: `p` is not declared `mut` (`p` is declared without `mut` here at 15..16)
        "#]],
    );
}

#[test]
fn field_assign_to_immutable_root_offers_the_make_mut_fix() {
    // Same machinery as a plain assignment to an immutable binding: the
    // insert-`mut` fix anchors at the binding's declaration.
    let text = "static f = fn { let p: struct { x: usize } = struct { x = 1 }; p.x = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "cannot assign to `p.x`: `p` is not declared `mut`"
    );
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `p` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Right before `p`'s declaration (offset 20 is the `p` in `let p`).
    assert_eq!(u32::from(fix.edits[0].range.start()), 20);
}

#[test]
fn field_assign_through_a_named_type_is_clean() {
    // A named type projects through its declared record for writes the
    // same as for reads.
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (mut p: Point) -> usize { p.x = 3; p.x + p.y };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn unknown_field_in_an_assign_target_reports_no_such_field() {
    // The target lowers as an ordinary field read for typing, so the
    // existing `NoSuchField` fires at the target position — no
    // assignment-specific wording needed.
    check_diagnostics(
        r#"static f = fn { let mut p = struct { x = 1 }; p.y = 2; };"#,
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
            48..49: no field `y` on `struct { x: {number} }`
        "#]],
    );
}

#[test]
fn field_assign_through_a_non_record_reports_no_such_field() {
    check_diagnostics(
        r#"static f = fn { let mut n = 1; n.x = 2; };"#,
        expect![[r#"
            28..29: cannot infer the type of this number: it has no defining use — add a type annotation
            31..32: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn field_assign_rhs_mismatch_blames_the_annotated_root() {
    // The RHS is checked against the *field's* type; the root binding's
    // annotation is the axiom cited (its record type spells the field's
    // type out — the same flow record-literal field checking uses).
    check_diagnostics(
        r#"static f = fn (mut p: struct { x: usize }) { p.x = "one"; };"#,
        expect![[r#"
            51..56: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 22..41)
        "#]],
    );
}

#[test]
fn field_assign_in_a_const_fn_is_clean() {
    // Field assignment is as legal in a const context as local assignment:
    // mutating a private local record is not an observable effect.
    check_diagnostics(
        r#"
static bump = const fn (mut p: struct { x: usize }) -> usize { p.x = p.x + 1; p.x };
static two: usize = bump(struct { x = 1 });
"#,
        expect![[r#""#]],
    );
}

#[test]
fn pub_field_on_type_decl_is_reserved() {
    check_diagnostics(
        r#"type Foo = struct { pub x: usize };"#,
        expect![[r#"
            20..23: field visibility is not supported yet
        "#]],
    );
}

#[test]
fn non_const_fn_call_offers_a_mark_const_fn_fix() {
    // The callee's initializer is a plain `fn` literal in the same file:
    // the fix inserts `const ` right before it.
    let text = "static f = fn (n: usize) -> usize { n };\nstatic x = f(1);";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Mark `f` as `const fn`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "const ");
    assert!(fix.edits[0].range.is_empty());
    // Right before `f`'s own `fn` keyword (offset 11): applying it yields
    // `static f = const fn (n: usize) -> usize { n };`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 11);
}

#[test]
fn non_const_fn_literal_call_offers_no_fix() {
    // A directly-called plain `fn` literal has no named declaration to
    // edit — nothing to offer.
    check_diagnostics(
        "static f = const { (fn () -> usize { 1 })() };",
        expect![[r#"
            20..40: cannot call this `fn` literal in a const context; marking it `const fn` would allow this (this `const` block is a const context at 11..16)
        "#]],
    );
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = const { (fn () -> usize { 1 })() };".to_owned(),
    );
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].fix.is_none());
}

/// Renders every *kept* expectation, `check_infer`-style: `range 'snippet':
/// expected type`. What is absent matters as much as what is present — an
/// entry survives `InferCtx::finish` only if it resolves to a concrete type
/// (no unbound inference variable, no `{error}`); see
/// `InferenceResult::expectation_of_expr`.
fn check_expectations(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut lines = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (_, source_map) = crate::body_with_source_map(&db, item);
        let result = crate::infer::infer(&db, item);
        for (expr, ty) in result.expectation_of_expr.iter() {
            if let Some(ptr) = source_map.node_for_expr(expr) {
                lines.push((ptr.text_range(), ty.display()));
            }
        }
    }
    lines.sort_by_key(|(range, _)| (range.start(), range.end()));
    let rendered = lines
        .into_iter()
        .map(|(range, ty)| {
            let snippet: String = text[range].replace('\n', " ");
            let snippet = if snippet.len() > 20 {
                format!("{}...", &snippet[..17])
            } else {
                snippet
            };
            format!("{range:?} '{snippet}': {ty}\n")
        })
        .collect::<String>();
    expect.assert_eq(&rendered);
}

#[test]
fn expectation_recorded_for_annotated_let_initializer() {
    check_expectations(
        "static main = fn { let x: usize = 5; };",
        expect![[r#"
            14..38 'fn { let x: usize...': fn()
            17..38 '{ let x: usize = ...': ()
            34..35 '5': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_call_argument() {
    check_expectations(
        r#"static main = fn { print("hi"); };"#,
        expect![[r#"
            14..33 'fn { print("hi"); }': fn()
            17..33 '{ print("hi"); }': ()
            19..24 'print': fn(str)
            19..30 'print("hi")': ()
            25..29 '"hi"': str
        "#]],
    );
}

#[test]
fn expectation_recorded_for_record_literal_field() {
    check_expectations(
        r#"
type Foo = struct { x: usize };
static main = fn { Foo(struct { x = 1 }); };
"#,
        expect![[r#"
            47..76 'fn { Foo(struct {...': fn()
            50..76 '{ Foo(struct { x ...': ()
            52..73 'Foo(struct { x = ...': Foo
            56..72 'struct { x = 1 }': struct { x: usize }
            69..70 '1': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_if_branches_under_annotation() {
    check_expectations(
        "static f: fn(bool) -> usize = fn (b: bool) -> usize { if b { 1 } else { 2 } };",
        expect![[r#"
            30..77 'fn (b: bool) -> u...': fn(bool) -> usize
            52..77 '{ if b { 1 } else...': usize
            54..75 'if b { 1 } else {...': usize
            57..58 'b': bool
            59..64 '{ 1 }': usize
            61..62 '1': usize
            70..75 '{ 2 }': usize
            72..73 '2': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_match_arm_tails() {
    check_expectations(
        r#"
type Shape = enum { Circle, Square };
static f: fn(Shape) -> usize = fn (s: Shape) -> usize {
    match s { ::Circle => 1, ::Square => 2 }
};
"#,
        expect![[r#"
            70..141 'fn (s: Shape) -> ...': fn(Shape) -> usize
            93..141 '{     match s { :...': usize
            99..139 'match s { ::Circl...': usize
            105..106 's': Shape
            121..122 '1': usize
            136..137 '2': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_break_value() {
    check_expectations(
        "static f: fn() -> usize = fn () -> usize { loop { break 1; } };",
        expect![[r#"
            26..62 'fn () -> usize { ...': fn() -> usize
            41..62 '{ loop { break 1;...': usize
            43..60 'loop { break 1; }': usize
            56..57 '1': usize
        "#]],
    );
}

#[test]
fn expectation_for_bare_let_initializer_is_its_own_type() {
    // Pinned behavior, not a design statement: a bare `let`'s initializer
    // is checked against a fresh variable, which the check then unifies
    // with the initializer's own type — so the recorded expectation
    // resolves concrete (to `usize` here) and is KEPT, even though nothing
    // outside the expression demanded it.
    check_expectations(
        "static main = fn { let x = 5; };",
        expect![[r#"
            14..31 'fn { let x = 5; }': fn()
            17..31 '{ let x = 5; }': ()
        "#]],
    );
}

#[test]
fn expectation_dropped_when_fresh_variable_stays_unbound() {
    // `panic(..)` is `!`, which *adopts* a still-free expectation instead
    // of unifying with it (see `InferCtx::check`'s early `Never` path) —
    // the bare `let`'s fresh variable stays unbound, so the initializer
    // call gets NO kept expectation. Its argument still does (`str`, the
    // builtin's parameter type).
    check_expectations(
        r#"static main = fn { let x = panic("msg"); };"#,
        expect![[r#"
            14..42 'fn { let x = pani...': fn()
            17..42 '{ let x = panic("...': ()
            27..32 'panic': fn(str) -> !
            33..38 '"msg"': str
        "#]],
    );
}

/// Companion to the two firewall tests above (same event-log style):
/// expectation recording must not leak extra invalidation. A body edit
/// that changes no types — and so no *kept* expectations — re-executes
/// only the edited item's own `infer`, and produces a value-EQUAL
/// `InferenceResult` (expectations included: they are range-free and hold
/// no canonicalized variable indices, those entries are dropped), so
/// everything downstream of the query backdates exactly as before
/// expectation recording existed.
#[test]
fn firewall_expectation_recording_backdates_unchanged_types() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: fn() -> usize = fn () -> usize { let x: usize = 1; x };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";
    // Only the literal changes: every type, and every kept expectation
    // (`x`'s annotated initializer among them), is identical.
    let text_v2 = "static a: fn() -> usize = fn () -> usize { let x: usize = 2; x };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    let before: Vec<crate::InferenceResult> = crate::file_item_ids(&db, file)
        .iter()
        .map(|&item| crate::infer::infer(&db, item).clone())
        .collect();
    assert!(
        before
            .iter()
            .any(|result| result.expectation_of_expr.iter().count() > 0),
        "the fixture records expectations at all"
    );

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    let after: Vec<crate::InferenceResult> = crate::file_item_ids(&db, file)
        .iter()
        .map(|&item| crate::infer::infer(&db, item).clone())
        .collect();
    assert_eq!(
        before, after,
        "a types-preserving edit must leave the results value-equal (backdating)"
    );
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

// ---- generics: item tree, schemes, rigid bodies, instantiation ----

#[test]
fn item_tree_records_generic_binder() {
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = fn::<T, const N: usize>(x: T) -> T { x };".to_owned(),
    );
    let item = crate::file_item_ids(&db, file)[0];
    let data = crate::item_data(&db, item).as_ref().expect("item data");
    let rendered: Vec<String> = data
        .generics
        .iter()
        .map(|param| match &param.kind {
            crate::item_tree::GenericParamKind::Region => format!("region {}", param.name),
            crate::item_tree::GenericParamKind::Type => format!("type {}", param.name),
            crate::item_tree::GenericParamKind::Const(ty) => {
                format!("const {}: {ty:?}", param.name)
            }
        })
        .collect();
    assert_eq!(
        rendered,
        vec!["type T".to_owned(), "const N: Path(\"usize\")".to_owned()],
        "binder kinds and order are recorded"
    );
    assert!(
        data.type_ref.is_some(),
        "a fully annotated binder synthesizes the scheme's TypeRef"
    );
}

/// Same style as the body-edit firewall test above: an edit inside a
/// generic item's body must not re-run its callers' inference — the scheme
/// (from the binder's mandatory annotations) is unchanged, so `item_data`
/// backdates and `signature` never re-fires downstream.
#[test]
fn firewall_generic_body_edit_does_not_reinfer_callers() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static id = fn::<T>(x: T) -> T { x };\n\
                   static g: fn() -> usize = fn () -> usize { id::<usize>(4) };\n";
    let text_v2 = "static id = fn::<T>(x: T) -> T { let y = x; y };\n\
                   static g: fn() -> usize = fn () -> usize { id::<usize>(4) };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited generic item may re-infer; executed: {log:#?}"
    );
}

#[test]
fn generic_fn_fully_annotated_is_clean() {
    check_diagnostics("static id = fn::<T>(x: T) -> T { x };", expect![[r#""#]]);
}

#[test]
fn generic_fn_missing_param_annotation_errors() {
    check_diagnostics(
        "static id = fn::<T>(x) -> T { x };",
        expect![[r#"
            14..19: a generic function must annotate all parameters and its return type
        "#]],
    );
}

#[test]
fn generic_fn_missing_return_type_errors() {
    check_diagnostics(
        "static id = fn::<T>(x: T) { x };",
        expect![[r#"
            14..19: a generic function must annotate all parameters and its return type
        "#]],
    );
}

#[test]
fn rigid_param_passes_stores_returns_and_compares() {
    // Everything an opaque value supports: pass through a `let` (with a
    // `T` annotation resolving to the rigid param), store in a record
    // field, compare with `==`, return through a join.
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { let y: T = x; let r = struct { v = y }; if x == y { r.v } else { x } };",
        expect![[r#""#]],
    );
}

#[test]
fn rigid_param_field_access_call_and_arithmetic_error_ordinarily() {
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { let a = x.field; let b = x(); let c = x + 1; x };",
        expect![[r#"
            42..47: no field `field` on `T`
            57..58: expression of type `T` is not callable
            70..71: type mismatch: expected `{number}`, found `T` (`+` requires `{number}` operands at 72..73)
        "#]],
    );
}

#[test]
fn const_param_reads_as_a_value_of_its_declared_type() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N + 1 };",
        expect![[r#""#]],
    );
}

#[test]
fn const_param_misused_errors_ordinarily() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N() };",
        expect![[r#"
            45..46: expression of type `usize` is not callable
        "#]],
    );
}

#[test]
fn const_param_cannot_be_assigned() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N = 3; N };",
        expect![[r#"
            45..46: cannot assign to `N`: it is a const parameter
        "#]],
    );
}

#[test]
fn dependent_const_param_type_is_rejected() {
    // TR06: no dependent params — a const param's type cannot name a type
    // param of the same binder.
    check_diagnostics(
        "static f = fn::<T, const N: T>(x: T) -> T { x };",
        expect![[r#"
            28..29: a const parameter's type cannot mention a type parameter
        "#]],
    );
}

#[test]
fn const_param_hole_type_is_rejected() {
    check_diagnostics(
        "static f = fn::<const N: _>() -> usize { N };",
        expect![[r#"
            25..26: a const parameter's type must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn variant_path_on_type_param_errors() {
    check_diagnostics(
        "static f = fn::<T>(x: T) -> usize { let y: T::Bad = 1; 1 };",
        expect![[r#"
            43..49: `T` has no variants (it is a type parameter)
        "#]],
    );
}

#[test]
fn nested_generic_binder_is_rejected() {
    // TR06: generics are item-level; a binder on a nested literal parses but
    // is rejected (generic closures wait for the capture story). The
    // binder's own error also covers the nested `T` mentions, which lower
    // to a silent `{error}`.
    check_diagnostics(
        "static f = fn () -> usize { let id = fn::<T>(x: T) -> T { x }; 1 };",
        expect![[r#"
            39..44: generic function literals are only supported as item initializers
        "#]],
    );
}

#[test]
fn generic_item_never_joins_an_inference_group() {
    // TR06, load-bearing: a group's shared signature variable is a monotype,
    // so membership would pin the scheme to one instantiation.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static id = fn::<T>(x: T) -> T { x };\nstatic caller = fn () { print(id(\"hi\")) };"
            .to_owned(),
    );
    let groups = crate::groups::inference_groups(&db, file);
    assert_eq!(groups.group_of[0], None, "the generic item joins no group");
    assert!(
        groups.group_of[1].is_some(),
        "the unannotated caller still gets a group of its own"
    );
    // The other direction: the caller sees only the scheme, and the call
    // type-checks against a fresh instantiation (T = str) — no diagnostics.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic caller = fn () { print(id(\"hi\")) };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_instantiates_the_scheme() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(4) };",
        expect![[r#""#]],
    );
}

#[test]
fn type_params_infer_from_value_arguments() {
    // A generic item with ONLY type params may be mentioned bare: ordinary
    // unification fills the params from the value arguments.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> str { id(\"hi\") };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_pins_the_param_against_the_value_argument() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(\"hi\") };",
        expect![[r#"
            78..82: type mismatch: expected `usize`, found `str` (because `T` was instantiated to `usize` by this argument at 71..76)
        "#]],
    );
}

#[test]
fn turbofish_arity_mismatch() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize, usize>(4) };",
        expect![[r#"
            66..84: `id` takes 1 generic argument, found 2 (declared here at 7..9)
        "#]],
    );
}

#[test]
fn turbofish_hole_leaves_a_type_param_to_inference() {
    check_diagnostics(
        "static pick = fn::<A, B>(a: A, b: B) -> A { a };\nstatic g = fn () -> usize { pick::<_, str>(4, \"x\") };",
        expect![[r#""#]],
    );
}

#[test]
fn const_arg_hole_is_an_error() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, _>(4) };",
        expect![[r#"
            83..98: const arguments cannot be inferred
        "#]],
    );
}

#[test]
fn turbofish_on_a_non_generic_item_errors() {
    check_diagnostics(
        "static f = fn (x: usize) -> usize { x };\nstatic g = fn () -> usize { f::<usize>(1) };",
        expect![[r#"
            69..79: `f` takes no generic arguments
        "#]],
    );
}

#[test]
fn bare_mention_with_const_params_requires_a_turbofish() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep(4) };",
        expect![[r#"
            83..86: const arguments must be written explicitly; write `rep::<...>` (declared here at 7..10)
        "#]],
    );
}

#[test]
fn const_arg_type_checks_against_the_declared_type() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, \"x\">(4) };",
        expect![[r#"
            96..99: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn generic_arg_kind_mismatches() {
    // A value where a type param is declared...
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<42>(4) };",
        expect![[r#"
            66..74: `T` is a type parameter; write a type
            71..73: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
    // ...and a type where a const param is declared.
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, str>(4) };",
        expect![[r#"
            83..100: `N` is a const parameter; write a value (a literal, or `const <expr>`)
        "#]],
    );
}

#[test]
fn unresolved_type_param_at_a_mention_is_reported() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () { id; };",
        expect![[r#"
            57..59: cannot infer the type parameter `T` of `id`; write `id::<...>` to specify it (defined here at 7..9)
        "#]],
    );
}

#[test]
fn turbofish_in_type_position_takes_no_generic_arguments() {
    check_diagnostics(
        "type Foo = struct { x: usize };\nstatic f: Foo::<usize> = Foo(struct { x = 1 });",
        expect![[r#"
            42..54: `Foo` takes no generic arguments (declared here at 5..8)
        "#]],
    );
}

#[test]
fn bare_angle_generic_mention_gets_only_the_syntax_correction() {
    // `Boxed<usize>` parses into the same `GENERIC_ARG_LIST` `Boxed::<usize>`
    // does (G06/X03 — see `syntax::validation`), so hir judges the arguments
    // it can see: the arity matches, and the ONE diagnostic is the syntax
    // correction. The bare-mention arity mirror, which would fire on a
    // `Boxed` with no list at all, never sees this.
    check_diagnostics(
        "type Boxed = struct::<T> { value: T };\n\
         static f = fn (b: Boxed::<usize>) -> usize { b.value };\n\
         static g = fn (b: Boxed<usize>) -> usize { b.value };",
        expect![[r#"
            113..125: generic arguments use the turbofish: write `Boxed::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_generic_call_gets_only_the_syntax_correction() {
    // Expression position, the case the heuristic exists for: the call's
    // arguments are really in the tree, so inference reads them and the
    // ONE diagnostic is the syntax correction.
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { x };\n\
         static u = fn () -> usize { f<usize>(3) };",
        expect![[r#"
            65..73: generic arguments use the turbofish: write `f::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_ordering_chain_is_read_as_a_call() {
    // The heuristic's residual measured where it is paid: `a < b > (d)` is
    // token-for-token `f::<T>(d)`, is read as the call, and the misread
    // costs the whole expression rather than one message. No well-typed
    // program is spelled that way — `a < b` is a `bool` and `>` wants
    // numbers — so nothing that compiles is traded for it. A comma makes
    // the shape a working program again, and then the heuristic declines
    // it (see `syntax`'s `bare_angle_top_level_comma_keeps_the_comparison_reading`).
    check_diagnostics(
        "static x = fn (a: usize, b: usize, d: usize) -> bool { a < b > (d) };",
        expect![[r#"
            55..62: generic arguments use the turbofish: write `a::<...>`
            55..62: `a` takes no generic arguments
            55..62: expression of type `usize` is not callable
            59..60: unknown type `b`
        "#]],
    );
}

#[test]
fn bare_angle_on_a_non_generic_type_also_reports_the_arity() {
    // The honest other half: a non-generic target gets the correction AND
    // the judgement the correctly-spelled `Boxed::<usize>` gets, because the
    // arguments are really there in the tree. That is why the worked example
    // in `examples/errors.must` uses a GENERIC target.
    check_diagnostics(
        "type Boxed = struct { value: usize };\n\
         static f = fn (b: Boxed<usize>) -> usize { b.value };",
        expect![[r#"
            56..68: generic arguments use the turbofish: write `Boxed::<...>`
            56..68: `Boxed` takes no generic arguments (declared here at 5..10)
        "#]],
    );
}

#[test]
fn generic_const_fn_body_is_a_const_context() {
    check_diagnostics(
        "static f = const fn::<const N: usize>() -> usize { g() };\nstatic g = fn () -> usize { 1 };",
        expect![[r#"
            51..52: cannot call `g` in a const context; marking it `const fn` would allow this (`g` is defined here at 65..66) (this `const fn` is always a const context at 11..16)
        "#]],
    );
}

#[test]
fn instantiated_generic_const_fn_call_is_const_legal() {
    // The const-check diagnostic must NOT fire: the rule keys off the
    // `const fn` marker exactly as for a plain mention. (The
    // initializer also genuinely EVALUATES — pinned on the eval side.)
    check_diagnostics(
        "static cid = const fn::<T>(x: T) -> T { x };\nstatic v: usize = cid::<usize>(4);",
        expect![[r#""#]],
    );
}

#[test]
fn instantiated_generic_plain_fn_call_in_a_const_context_is_rejected() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic v: usize = id::<usize>(4);",
        expect![[r#"
            56..67: cannot call `id` in a const context; marking it `const fn` would allow this (`id` is defined here at 7..9) (this item's initializer is a const context at 38..44)
        "#]],
    );
}

/// Type-tier interplay: an expectation against a rigid-param annotation is
/// a REAL expectation — `Ty::Param` is not an inference variable, so the
/// finish pass must keep it (`contains_infer` is false for params).
#[test]
fn expectation_recorded_against_rigid_param_annotation() {
    check_expectations(
        "static id = fn::<T>(x: T) -> T { let y: T = x; y };",
        expect![[r#"
            12..50 'fn::<T>(x: T) -> ...': fn(T) -> T
            31..50 '{ let y: T = x; y }': T
            44..45 'x': T
            47..48 'y': T
        "#]],
    );
}

#[test]
fn generic_item_may_recurse_through_a_fresh_instantiation() {
    // The mention in its own body instantiates the scheme like any other
    // (no group membership, no cycle: the scheme never consults the body).
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { id(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_may_name_the_enclosing_binder_param() {
    // Inside a generic body, a turbofish type argument may be the body's
    // own rigid param: `id::<U>` pins the callee's `T` to rigid `U`.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic wrap = fn::<U>(x: U) -> U { id::<U>(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn divergence_widens_into_a_rigid_param_return() {
    // `!` widens to everything, a rigid param included (divergence produces
    // no value to convert); no VALUE type widens to or from a param.
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { panic(\"unimplemented\") };",
        expect![[r#""#]],
    );
}

#[test]
fn a_binder_names_each_type_param_once() {
    // A rigid `Ty::Param` is positional, so a repeated name leaves the
    // first parameter unnameable; the second occurrence is the error.
    check_diagnostics(
        "static id = fn::<T, T>(x: T) -> T { x };",
        expect![[r#"
            20..21: duplicate generic parameter `T` (first declared here at 17..18)
        "#]],
    );
}

#[test]
fn a_binder_names_each_const_param_once() {
    // Same rule for const params, whose mentions resolve to the LAST
    // declaration of the name.
    check_diagnostics(
        "static sq = fn::<const N: usize, const N: usize>() -> usize { N };",
        expect![[r#"
            39..40: duplicate generic parameter `N` (first declared here at 23..24)
        "#]],
    );
}

#[test]
fn type_and_const_params_share_the_binders_namespace() {
    check_diagnostics(
        "static f = fn::<T, const T: usize>(x: T) -> usize { T };",
        expect![[r#"
            25..26: duplicate generic parameter `T` (first declared here at 16..17)
        "#]],
    );
}

#[test]
fn a_type_declarations_binder_names_each_param_once() {
    check_diagnostics(
        "type Pair = struct::<T, T> { a: T, b: T };",
        expect![[r#"
            24..25: duplicate generic parameter `T` (first declared here at 21..22)
        "#]],
    );
}

#[test]
fn a_binder_names_each_region_once() {
    // Same rule for region params; their `@` sigil keeps them from
    // colliding with type and const parameter names.
    check_diagnostics(
        "static get = fn::<@a, @a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#"
            22..24: duplicate generic parameter `@a` (first declared here at 18..20)
        "#]],
    );
}

#[test]
fn the_wildcard_region_is_not_a_binder_name() {
    // `@_` asks for a region to be inferred; a binder is where regions are
    // declared, so it names nothing there — and a second one is the same
    // mistake again, not a duplicate of the first.
    check_diagnostics(
        "static f = fn::<@_>(x: usize) -> usize { x };",
        expect![[r#"
            16..18: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
        "#]],
    );
    check_diagnostics(
        "static g = fn::<@_, @_>(x: usize) -> usize { x };",
        expect![[r#"
            16..18: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
            20..22: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
        "#]],
    );
}

#[test]
fn a_region_in_a_type_slot_reads_the_same_in_both_positions() {
    // A type declaration binds no region, so `@a` in its own list is a
    // wrong-kind argument. Annotation and expression are one mistake: one
    // sentence, and the same range — the written argument.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T };
static q: Pair::<@a> = Pair::<str>(struct { a = "l", b = "r" });
"#,
        expect![[r#"
            58..60: `T` is not a region parameter; a region argument (`@a`) does not belong here
        "#]],
    );
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T };
static q = Pair::<@a>(struct { a = "l", b = "r" });
"#,
        expect![[r#"
            59..61: `T` is not a region parameter; a region argument (`@a`) does not belong here
        "#]],
    );
}

#[test]
fn two_items_may_each_bind_the_same_param_name() {
    // The rule is per BINDER, not per file: separate binders are separate
    // namespaces.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic other = fn::<T>(x: T) -> T { x };",
        expect![[r#""#]],
    );
}

// ---- generics: the const-arg domain excludes fn values ----

#[test]
fn fn_typed_const_param_is_rejected_at_the_declaration() {
    // TR06 (concrete data types only): fn values carry edit-unstable
    // `BodyId` identity, so they are outside the const-arg domain —
    // rejected where the domain is declared.
    check_diagnostics(
        "static f = fn::<const N: fn() -> usize>() -> usize { 1 };",
        expect![[r#"
            25..38: a function value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn fn_typed_const_param_smuggled_in_a_record_is_rejected_too() {
    check_diagnostics(
        "static f = fn::<const N: { g: fn() -> usize }>() -> usize { 1 };",
        expect![[r#"
            25..26: expected a type
            27..28: expected `,`
            30..43: only a trait name can be a bound
            44..45: expected `,`
        "#]],
    );
}

#[test]
fn fn_typed_const_arg_is_rejected_at_the_mention() {
    // The belt: a mention that would pass an fn value repeats the
    // declaration's exact text at the mention — and the argument is never
    // recorded, so no fn value can reach instance identity.
    check_diagnostics(
        "static f = fn::<const N: fn() -> usize>() -> usize { 1 };\nstatic g = fn () -> usize { let h = fn () -> usize { 2 }; f::<const h>() };",
        expect![[r#"
            25..38: a function value cannot be a const argument (yet)
            116..128: a function value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn const_param_forwards_as_a_const_arg() {
    // The composition case: inside a generic body, the binder's own
    // const param is a legal const ARGUMENT — `rep::<const N>` records
    // and type-checks like any const-arg expression.
    check_diagnostics(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };",
        expect![[r#""#]],
    );
}

// ---- generic type declarations ----

#[test]
fn generic_record_construction_with_explicit_args() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { let p = Pair::<usize>(struct { a = 1, b = 2 }); p.a + p.b };",
        expect![[r#"
            65..141 'fn () -> usize { ...': fn() -> usize
            80..141 '{ let p = Pair::<...': usize
            86..87 'p': Pair::<usize>
            90..103 'Pair::<usize>': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            90..128 'Pair::<usize>(str...': Pair::<usize>
            104..127 'struct { a = 1, b...': struct { a: usize, b: usize }
            117..118 '1': usize
            124..125 '2': usize
            130..131 'p': Pair::<usize>
            130..133 'p.a': usize
            130..139 'p.a + p.b': usize
            136..137 'p': Pair::<usize>
            136..139 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_record_construction_infers_args_from_payload() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { let p = Pair(struct { a = 1, b = 2 }); p.b };",
        expect![[r#"
            65..126 'fn () -> usize { ...': fn() -> usize
            80..126 '{ let p = Pair(st...': usize
            86..87 'p': Pair::<usize>
            90..94 'Pair': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            90..119 'Pair(struct { a =...': Pair::<usize>
            95..118 'struct { a = 1, b...': struct { a: usize, b: usize }
            108..109 '1': usize
            115..116 '2': usize
            121..122 'p': Pair::<usize>
            121..124 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p = Pair::<usize, str>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            81..99: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
            113..114: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_in_annotation_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<usize, str> = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            80..98: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn bare_generic_type_in_annotation_requires_the_turbofish() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            80..84: `Pair` takes 1 generic argument, found 0 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn annotation_hole_arg_is_pinned_by_the_initializer() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<_> = Pair(struct { a = 1, b = 2 }); };",
        expect![[r#"
            65..124 'fn () { let p: Pa...': fn()
            71..124 '{ let p: Pair::<_...': ()
            77..78 'p': Pair::<{number}>
            92..96 'Pair': fn(struct { a: {number}, b: {number} }) -> Pair::<{number}>
            92..121 'Pair(struct { a =...': Pair::<{number}>
            97..120 'struct { a = 1, b...': struct { a: {number}, b: {number} }
            110..111 '1': {number}
            117..118 '2': {number}
        "#]],
    );
}

#[test]
fn different_type_args_do_not_unify() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<str> = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            94..132: type mismatch: expected `Pair::<str>`, found `Pair::<usize>` (expected `Pair::<str>` because of this annotation at 80..91)
        "#]],
    );
}

#[test]
fn same_args_unify_across_bodies() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static mk = fn () -> Pair::<usize> { Pair::<usize>(struct { a = 1, b = 2 }) };\n\\\n         static use_it = fn (p: Pair::<usize>) -> usize { p.a };\n\\\n         static main = fn () -> usize { use_it(mk()) };",
        expect![[r#"
            40..41: unexpected character `\`
            130..131: unexpected character `\`
            197..198: unexpected character `\`
        "#]],
    );
}

#[test]
fn generic_variant_construction_and_widening_preserve_args() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             let mut o = Option::<usize>::Some(3);\n\
             o = Option::<usize>::None;\n\
             1\n\
         };",
        expect![[r#"
            57..142 'fn () -> usize { ...': fn() -> usize
            72..142 '{ let mut o = Opt...': usize
            82..83 'o': Option::<usize>
            86..107 'Option::<usize>::...': fn(usize) -> Option::<usize>::Some
            86..110 'Option::<usize>::...': Option::<usize>::Some
            108..109 '3': usize
            112..113 'o': Option::<usize>
            116..137 'Option::<usize>::...': Option::<usize>
            139..140 '1': usize
        "#]],
    );
}

#[test]
fn generic_variant_args_infer_from_the_payload() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::Some(3) };",
        expect![[r#"
            57..101 'fn () -> Option::...': fn() -> Option::<usize>
            82..101 '{ Option::Some(3) }': Option::<usize>
            84..96 'Option::Some': fn(usize) -> Option::<usize>::Some
            84..99 'Option::Some(3)': Option::<usize>
            97..98 '3': usize
        "#]],
    );
}

#[test]
fn widening_to_the_wrong_args_is_a_mismatch() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::<str>::Some(\"x\") };",
        expect![[r#"
            84..108: type mismatch: expected `Option::<usize>`, found `Option::<str>::Some` (expected `Option::<usize>` because of this return type at 63..81)
        "#]],
    );
}

#[test]
fn match_over_a_generic_enum_is_exhaustive_and_typed() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             match Option::<usize>::Some(3) { ::Some(x) => x, ::None => 0, }\n\
         };",
        expect![[r#"
            57..139 'fn () -> usize { ...': fn() -> usize
            72..139 '{ match Option::<...': usize
            74..137 'match Option::<us...': usize
            80..101 'Option::<usize>::...': fn(usize) -> Option::<usize>::Some
            80..104 'Option::<usize>::...': Option::<usize>::Some
            102..103 '3': usize
            114..115 'x': usize
            120..121 'x': usize
            133..134 '0': usize
        "#]],
    );
}

#[test]
fn non_exhaustive_match_over_a_generic_enum_is_reported() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             let o: Option::<usize> = Option::<usize>::Some(3);\n\
             match o { ::Some(x) => x, }\n\
         };",
        expect![[r#"
            125..130: this `match` does not cover `Option::None`
        "#]],
    );
}

#[test]
fn const_params_on_types_distinguish_instances() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b: Buf::<8> = Buf::<9>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            103..131: type mismatch: expected `Buf::<8>`, found `Buf::<9>` (expected `Buf::<8>` because of this annotation at 92..100)
        "#]],
    );
}

#[test]
fn const_param_type_annotation_and_construction_agree() {
    check_infer(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () -> usize { let b: Buf::<8> = Buf::<8>(struct { len = 3 }); b.len };",
        expect![[r#"
            77..149 'fn () -> usize { ...': fn() -> usize
            92..149 '{ let b: Buf::<8>...': usize
            98..99 'b': Buf::<8>
            112..120 'Buf::<8>': fn(struct { len: usize }) -> Buf::<8>
            112..140 'Buf::<8>(struct {...': Buf::<8>
            118..119 '8': usize
            121..139 'struct { len = 3 }': struct { len: usize }
            136..137 '3': usize
            142..143 'b': Buf::<8>
            142..147 'b.len': usize
        "#]],
    );
}

#[test]
fn const_block_in_annotation_position_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b: Buf::<const { 4 + 4 }> = Buf::<8>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            98..113: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn const_block_in_type_construction_turbofish_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b = Buf::<const { 4 + 4 }>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            93..115: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn const_param_as_a_field_type_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: N };",
        expect![[r#"
            43..44: `N` is a const parameter, not a type
        "#]],
    );
}

#[test]
fn const_param_forwards_into_a_generic_type_inside_a_generic_fn() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static mk = fn::<const N: usize>(len: usize) -> Buf::<N> { Buf::<N>(struct { len }) };\n\
         static main = fn () { let b: Buf::<8> = mk::<8>(3); };",
        expect![[""]],
    );
}

#[test]
fn self_referential_generic_type_does_not_hang() {
    check_diagnostics(
        "type List = struct::<T> { next: List::<T> };",
        expect![[""]],
    );
}

#[test]
fn unpinned_generic_type_param_is_reported_at_the_mention() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () { let o = Option::None; };",
        expect![[r#"
            73..85: cannot infer the type parameter `T` of `Option`; write `Option::<...>` to specify it (defined here at 5..11)
        "#]],
    );
}

#[test]
fn generic_type_display_in_diagnostics() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { Pair::<usize>(struct { a = 1, b = 2 }) };",
        expect![[r#"
            40..41: unexpected character `\`
            82..120: type mismatch: expected `usize`, found `Pair::<usize>` (expected `usize` because of this return type at 71..79)
        "#]],
    );
}

#[test]
fn generic_type_name_is_not_a_value() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let x = Pair::<usize>; };",
        expect![[r#"
            70..83: `Pair` is a type, not a value
        "#]],
    );
}

#[test]
fn kind_mismatches_on_a_generic_type_mention() {
    check_diagnostics(
        "type Buf = struct::<T, const N: usize> { x: T };\n\\\n         static main = fn () { let b: Buf::<8, usize> = Buf::<8, usize>(struct { x = 1 }); };",
        expect![[r#"
            49..50: unexpected character `\`
            95..96: `T` is a type parameter; write a type
            98..103: `N` is a const parameter; write a value (a literal, or `const <expr>`)
            107..122: `T` is a type parameter; write a type
            107..122: `N` is a const parameter; write a value (a literal, or `const <expr>`)
            113..114: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn generic_mention_in_a_declaration_field_mirrors_lowering() {
    // Unknown arg name inside a declaration's generic mention.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair::<Missing> };",
        expect![[r#"
            73..80: unknown type `Missing`
        "#]],
    );
    // A bare generic mention in a field needs its args spelled.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair };",
        expect![[r#"
            66..70: `Pair` takes 1 generic argument, found 0 (declared here at 5..9)
        "#]],
    );
    // A hole arg has nothing to infer from in a declaration.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair::<_> };",
        expect![[r#"
            66..75: a field's type must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn generic_variant_type_annotation_is_not_spellable_yet() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn (o: Option::Some) {};",
        expect![[r#"
            64..76: `Option` is generic; a generic enum's variant types cannot be written in annotations yet
        "#]],
    );
}

#[test]
fn generic_enum_payload_mentions_check_in_declarations() {
    // A generic mention inside another enum's payload.
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         type Holder = enum { Held(Option::<usize>), Empty };\n\
         static main = fn () -> Holder { Holder::Held(Option::<usize>::Some(1)) };",
        expect![[""]],
    );
}

#[test]
fn raw_pointer_types_infer_and_display() {
    check_infer(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    let q = x.&raw;
    unsafe { p.* }
};
"#,
        expect![[r#"
            15..114 'fn() -> usize {  ...': fn() -> usize
            29..114 '{     let mut x =...': usize
            43..44 'x': usize
            47..48 '1': usize
            58..59 'p': usize.&raw mut
            62..63 'x': usize
            62..72 'x.&raw mut': usize.&raw mut
            82..83 'q': usize.&raw
            86..87 'x': usize
            86..92 'x.&raw': usize.&raw
            98..112 'unsafe { p.* }': usize
            105..112 '{ p.* }': usize
            107..108 'p': usize.&raw mut
            107..110 'p.*': usize
        "#]],
    );
}

#[test]
fn addr_of_mut_requires_a_mut_root() {
    check_diagnostics(
        "static main = fn { let x = 1; let p = x.&raw mut; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: cannot take `.&raw mut` of `x`: it is not declared `mut` (`x` is declared without `mut` here at 23..24)
        "#]],
    );
}

#[test]
fn addr_of_mut_of_a_field_blames_the_root() {
    check_diagnostics(
        "static main = fn { let r = struct { a = 1 }; let p = r.a.&raw mut; };",
        expect![[r#"
            40..41: cannot infer the type of this number: it has no defining use — add a type annotation
            53..54: cannot take `.&raw mut` of `r.a`: `r` is not declared `mut` (`r` is declared without `mut` here at 23..24)
        "#]],
    );
}

#[test]
fn addr_of_shared_needs_no_mut() {
    check_diagnostics(
        "static main = fn { let x = 1; let p = x.&raw; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_mut_of_a_static_is_reserved() {
    check_diagnostics(
        "static s = 7;\nstatic main = fn { let p = s.&raw mut; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            41..42: cannot infer the type of `s` across items; add a type annotation to its definition (defined here at 7..8)
            41..42: cannot take `.&raw mut` of `s`: `static mut` is not supported yet (`s` is defined here at 7..8)
        "#]],
    );
}

#[test]
fn addr_of_shared_of_items_is_fine() {
    check_diagnostics(
        "static s = 7;\nconst c = 8;\nstatic main = fn { let p = s.&raw; let q = c.&raw; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            54..55: cannot infer the type of `s` across items; add a type annotation to its definition (defined here at 7..8)
            70..71: cannot infer the type of `c` across items; add a type annotation to its definition (defined here at 20..21)
        "#]],
    );
}

#[test]
fn addr_of_a_non_place_errors() {
    check_diagnostics(
        "static main = fn { let p = 3.&raw; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
            27..33: `.&raw` can only take the address of a variable, a chain of its fields and elements, a `static`/`const` item, or a chain rooted in a deref
        "#]],
    );
}

#[test]
fn safe_borrow_expr_lowers_and_infers() {
    // `.&`/`.&mut` in expression position mint their own region variable
    // (no turbofish needed) and lower cleanly.
    check_diagnostics(
        "static main = fn { let mut x = 1; let a = x.&; let b = x.&mut; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn safe_borrow_type_without_a_region_is_reported() {
    // `T.&`/`T.&mut` in a field type is a SIGNATURE position: elision is
    // deferred, so an unannotated region is reported, not guessed.
    check_diagnostics(
        "type Bad = struct { r: usize.& };",
        expect![[r#"
            29..30: a safe borrow must name its region (`T.&::<@a>`); regions are never elided yet
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_works_inside_unsafe() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw mut; let q = unsafe { p.*.&raw mut }; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_still_requires_unsafe() {
    // The deref rule is uniform: the deref inside a `.&raw` place needs
    // `unsafe` like any other deref site.
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw mut; let q = p.*.&raw mut; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            62..65: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn addr_of_mut_through_a_shared_pointer_errors() {
    check_diagnostics(
        r#"
static main = fn {
    let mut r = struct { a = 1 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw mut };
};
"#,
        expect![[r#"
            49..50: cannot infer the type of this number: it has no defining use — add a type annotation
            95..109: cannot take `.&raw mut` through `struct { a: {number} }.&raw`: minting a mutating address needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn addr_of_shared_through_any_pointer_is_fine() {
    check_diagnostics(
        r#"
static main = fn {
    let mut r = struct { a = 1 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw };
};
"#,
        expect![[r#"
            49..50: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_types_as_the_projected_pointee() {
    check_infer(
        r#"
static main = fn {
    let mut r = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    let q = unsafe { p.*.a.&raw mut };
};
"#,
        expect![[r#"
            15..125 'fn {     let mut ...': fn()
            18..125 '{     let mut r =...': ()
            32..33 'r': struct { a: {number}, b: {number} }
            36..59 'struct { a = 1, b...': struct { a: {number}, b: {number} }
            49..50 '1': {number}
            56..57 '2': {number}
            69..70 'p': struct { a: {number}, b: {number} }.&raw mut
            73..74 'r': struct { a: {number}, b: {number} }
            73..83 'r.&raw mut': struct { a: {number}, b: {number} }.&raw mut
            93..94 'q': {number}.&raw mut
            97..122 'unsafe { p.*.a.&r...': {number}.&raw mut
            104..122 '{ p.*.a.&raw mut }': {number}.&raw mut
            106..107 'p': struct { a: {number}, b: {number} }.&raw mut
            106..109 'p.*': struct { a: {number}, b: {number} }
            106..111 'p.*.a': {number}
            106..120 'p.*.a.&raw mut': {number}.&raw mut
        "#]],
    );
}

#[test]
fn deref_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; p.* };",
        expect![[r#"
            65..68: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_inside_unsafe_is_clean() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; unsafe { p.* } };",
        expect![[r#""#]],
    );
}

#[test]
fn deref_write_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw mut; p.* = 2; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            54..57: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_through_a_shared_pointer_errors() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw; unsafe { p.* = 2; } };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            59..62: cannot assign through `{number}.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_works() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; unsafe { p.*.a = 2; } };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_through_a_shared_pointer_errors() {
    // A shared step anywhere in the chain refuses the write: `p` is
    // shared, so the field write is refused at the pointer's flavor —
    // same message as `p.* = v;`.
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw; unsafe { p.*.a = 2; } };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
            74..77: cannot assign through `struct { a: {number} }.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_still_requires_unsafe() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; p.*.a = 2; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
            69..72: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_needs_no_mut_binding_on_the_pointer() {
    // `p` itself is not `mut` — writing through it does not reassign it,
    // for projected targets exactly like for `p.* = v;`.
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; unsafe { p.*.a = 2; }; let x = p; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn pointee_field_reads_work() {
    check_diagnostics(
        "static main = fn() -> usize { let mut r = struct { a = 1, b = 2 }; let p = r.&raw mut; unsafe { p.*.a + p.*.b } };",
        expect![[r#""#]],
    );
}

#[test]
fn raw_pointer_unification_is_exact_no_mut_mixing() {
    check_diagnostics(
        "static f = fn(p: usize.&raw) {};\nstatic main = fn { let mut x = 1; f(x.&raw mut); };",
        expect![[r#"
            69..79: type mismatch: expected `usize.&raw`, found `{error}.&raw mut`
        "#]],
    );
}

#[test]
fn raw_pointer_unification_is_exact_no_shared_to_mut() {
    check_diagnostics(
        "static f = fn(p: usize.&raw mut) {};\nstatic main = fn { let x = 1; f(x.&raw); };",
        expect![[r#"
            69..75: type mismatch: expected `usize.&raw mut`, found `{error}.&raw`
        "#]],
    );
}

#[test]
fn raw_pointer_pointee_must_match_exactly() {
    check_diagnostics(
        r#"static main = fn { let mut x = 1; let p: str.&raw mut = x.&raw mut; };"#,
        expect![[r#"
            56..66: type mismatch: expected `str.&raw mut`, found `{error}.&raw mut` (expected `str.&raw mut` because of this annotation at 41..53)
        "#]],
    );
}

#[test]
fn raw_pointer_does_not_coerce_to_pointee() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let y: usize = x.&raw mut; };",
        expect![[r#"
            49..59: type mismatch: expected `usize`, found `{error}.&raw mut` (expected `usize` because of this annotation at 41..46)
        "#]],
    );
}

#[test]
fn deref_of_a_non_pointer_errors() {
    check_diagnostics(
        "static main = fn { let x = 1; unsafe { x.*; } };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
            39..40: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn pointer_equality_is_legal_and_safe() {
    check_diagnostics(
        "static main = fn() -> bool { let mut x = 1; x.&raw mut == x.&raw mut };",
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

// ---- fixed-size arrays ----

#[test]
fn array_literal_and_index_infer() {
    check_infer(
        "static f = fn { let a = [1, 2, 3]; let x = a[0]; };",
        expect![[r#"
            11..50 'fn { let a = [1, ...': fn()
            14..50 '{ let a = [1, 2, ...': ()
            20..21 'a': [{number}; 3]
            24..33 '[1, 2, 3]': [{number}; 3]
            25..26 '1': {number}
            28..29 '2': {number}
            31..32 '3': {number}
            39..40 'x': {number}
            43..44 'a': [{number}; 3]
            43..47 'a[0]': {number}
            45..46 '0': usize
        "#]],
    );
}

#[test]
fn arrays_in_records_and_records_in_arrays_infer() {
    // `a` carries an annotation: like an `if`-joined record binding, a
    // record-element type decided by a multi-witness join is not available
    // to `.x` *during* traversal (joins solve after it) — the annotation is
    // the same way out `field_access_on_unannotated_param` names.
    check_infer(
        r#"
static f = fn {
    let r = struct { data = [1, 2], len = 2 };
    let a: [struct { x: usize }; 2] = [struct { x = 1 }, struct { x = 2 }];
    let n = r.data[0] + a[1].x;
};
"#,
        expect![[r#"
            12..173 'fn {     let r = ...': fn()
            15..173 '{     let r = str...': ()
            25..26 'r': struct { data: [usize; 2], len: {number} }
            29..62 'struct { data = [...': struct { data: [usize; 2], len: {number} }
            45..51 '[1, 2]': [usize; 2]
            46..47 '1': usize
            49..50 '2': usize
            59..60 '2': {number}
            72..73 'a': [struct { x: usize }; 2]
            102..138 '[struct { x = 1 }...': [struct { x: usize }; 2]
            103..119 'struct { x = 1 }': struct { x: usize }
            116..117 '1': usize
            121..137 'struct { x = 2 }': struct { x: usize }
            134..135 '2': usize
            148..149 'n': usize
            152..153 'r': struct { data: [usize; 2], len: {number} }
            152..158 'r.data': [usize; 2]
            152..161 'r.data[0]': usize
            152..170 'r.data[0] + a[1].x': usize
            159..160 '0': usize
            164..165 'a': [struct { x: usize }; 2]
            164..168 'a[1]': struct { x: usize }
            164..170 'a[1].x': usize
            166..167 '1': usize
        "#]],
    );
}

#[test]
fn array_length_mismatch_blames_the_literal() {
    check_diagnostics(
        "static f = fn { let a: [usize; 3] = [1, 2]; };",
        expect![[r#"
            36..42: type mismatch: expected `[usize; 3]`, found `[usize; 2]` (expected `[usize; 3]` because of this annotation at 23..33)
        "#]],
    );
}

#[test]
fn array_lengths_never_unify_across_a_call() {
    check_diagnostics(
        r#"
static g = fn (a: [usize; 2]) {};
static f = fn { g([1, 2, 3]); };
"#,
        expect![[r#"
            53..62: type mismatch: expected `[usize; 2]`, found `[usize; 3]`
        "#]],
    );
}

#[test]
fn array_element_mismatch_blames_the_element() {
    check_diagnostics(
        r#"static f = fn { let a: [usize; 2] = [1, "two"]; };"#,
        expect![[r#"
            40..45: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..33)
        "#]],
    );
}

#[test]
fn array_elements_vote_without_an_annotation() {
    check_diagnostics(
        r#"static f = fn { let a = [1, 2, "three"]; };"#,
        expect![[r#"
            25..26: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 31..38)
            28..29: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 31..38)
        "#]],
    );
}

#[test]
fn empty_array_needs_annotation() {
    check_diagnostics(
        r#"static f = fn { let a = []; };"#,
        expect![[r#"
            24..26: cannot infer the element type of an empty array; add a type annotation
        "#]],
    );
}

#[test]
fn empty_array_with_annotation_is_fine() {
    check_diagnostics(
        "static f = fn { let a: [usize; 0] = []; };",
        expect![[r#""#]],
    );
}

#[test]
fn index_must_be_usize() {
    check_diagnostics(
        r#"static f = fn { let a = [1, 2]; let x = a["nope"]; };"#,
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            42..48: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn compile_time_out_of_bounds_is_reported() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; let x = a[2]; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            40..44: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn index_on_non_array_is_reported() {
    check_diagnostics(
        "static f = fn { let x = 5; let y = x[0]; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            35..36: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn index_assign_requires_mut_root() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; a[0] = 5; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            32..33: cannot assign to `a[_]`: `a` is not declared `mut` (`a` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn index_assign_to_mut_binding_is_fine() {
    check_diagnostics(
        "static f = fn { let mut a = [1, 2]; a[0] = 5; print(\"\"); };",
        expect![[r#"
            29..30: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn const_param_array_length_end_to_end() {
    check_diagnostics(
        r#"
static sum2 = const fn::<const N: usize>(b: [usize; N]) -> usize { b[0] + b[1] };
static r = sum2::<2>([1, 2]);
"#,
        expect![""],
    );
}

#[test]
fn const_param_array_length_mismatch_at_instantiation() {
    check_diagnostics(
        r#"
static sum2 = const fn::<const N: usize>(b: [usize; N]) -> usize { b[0] + b[1] };
static r = sum2::<3>([1, 2]);
"#,
        expect![[r#"
            104..110: type mismatch: expected `[usize; 3]`, found `[usize; 2]`
        "#]],
    );
}

#[test]
fn generic_type_with_array_field_end_to_end() {
    check_diagnostics(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
static b = Buf::<2>(struct { data = [1, 2], len = 2 });
static first = fn (buf: Buf::<2>) -> usize { buf.data[0] };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn const_block_array_length_rejected_in_type_position() {
    check_diagnostics(
        "static x: [usize; const { 3 }] = [1, 2, 3];",
        expect![[r#"
            18..29: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn non_const_array_length_name_rejected_in_type_position() {
    check_diagnostics(
        "static x: [usize; huh] = [1];",
        expect![[r#"
            18..21: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn str_array_length_rejected_in_type_position() {
    check_diagnostics(
        r#"static x: [usize; "two"] = [1, 2];"#,
        expect![[r#"
            18..23: type mismatch: expected `usize`, found `str`
            27..33: type mismatch: expected `[usize; "two"]`, found `[usize; 2]` (expected `[usize; "two"]` because of this annotation at 10..24)
        "#]],
    );
}

#[test]
fn array_repeat_infers_and_repeat_count_forms() {
    check_infer(
        "static f = fn { let a = [0; 4]; };",
        expect![[r#"
            11..33 'fn { let a = [0; ...': fn()
            14..33 '{ let a = [0; 4]; }': ()
            20..21 'a': [{number}; 4]
            24..30 '[0; 4]': [{number}; 4]
            25..26 '0': {number}
            28..29 '4': usize
        "#]],
    );
}

#[test]
fn array_repeat_with_variable_count_rejected() {
    check_diagnostics(
        "static f = fn (n: usize) { let a = [0; n]; };",
        expect![[r#"
            36..37: cannot infer the type of this number: it has no defining use — add a type annotation
            39..40: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn array_repeat_with_const_block_count_rejected() {
    check_diagnostics(
        "static f = fn { let a = [0; const { 2 }]; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            28..39: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn array_repeat_with_const_param_count() {
    check_diagnostics(
        r#"
static rep = const fn::<const N: usize>(v: usize) -> [usize; N] { [v; N] };
static r = rep::<3>(7);
"#,
        expect![""],
    );
}

#[test]
fn array_valued_const_param_rejected_at_declaration() {
    check_diagnostics(
        "static f = fn::<const N: [usize; 2]>() -> usize { 0 };",
        expect![[r#"
            25..35: an array value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn addr_of_whole_array_and_element_work() {
    check_diagnostics(
        r#"
static f = fn {
    let mut a = [1, 2];
    let p = unsafe { (a.&raw mut).* };
    let q = a[0].&raw mut;
};
"#,
        expect![[r#"
            34..35: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_array_element_types_as_element_pointer() {
    check_infer(
        r#"
static f = fn {
    let mut a = [1, 2];
    let q = a[0].&raw mut;
};
"#,
        expect![[r#"
            12..69 'fn {     let mut ...': fn()
            15..69 '{     let mut a =...': ()
            29..30 'a': [{number}; 2]
            33..39 '[1, 2]': [{number}; 2]
            34..35 '1': {number}
            37..38 '2': {number}
            49..50 'q': {number}.&raw mut
            53..54 'a': [{number}; 2]
            53..57 'a[0]': {number}
            53..66 'a[0].&raw mut': {number}.&raw mut
            55..56 '0': usize
        "#]],
    );
}

#[test]
fn addr_of_mut_of_an_element_still_requires_a_mut_root() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; let q = a[0].&raw mut; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            40..41: cannot take `.&raw mut` of `a[_]`: `a` is not declared `mut` (`a` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn match_on_array_scrutinee_rejects_variant_patterns() {
    check_diagnostics(
        r#"
type Shape = enum { Point };
static f = fn (a: [usize; 2]) -> usize {
    match a {
        ::Point => 1,
        _ => 2,
    }
};
"#,
        expect![[r#"
            93..100: a variant pattern needs an enum scrutinee; only `_` or a binding can match a `[usize; 2]`
        "#]],
    );
}

#[test]
fn type_declaration_with_array_field() {
    check_diagnostics(
        r#"
type Buf = struct { data: [usize; 2] };
static b = Buf(struct { data = [1, 2] });
"#,
        expect![[r#""#]],
    );
}

#[test]
fn type_declaration_array_field_bad_lengths() {
    check_diagnostics(
        r#"
type A = struct { data: [usize; const { 2 }] };
type B = struct { data: [usize; nope] };
"#,
        expect![[r#"
            33..44: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
            81..85: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn mixed_variant_array_elements_widen_to_the_enum() {
    check_infer(
        r#"
type Shape = enum { Point, Circle(usize) };
static f = fn {
    let shapes = [Shape::Circle(1), Shape::Point];
};
"#,
        expect![[r#"
            56..113 'fn {     let shap...': fn()
            59..113 '{     let shapes ...': ()
            69..75 'shapes': [Shape; 2]
            78..110 '[Shape::Circle(1)...': [Shape; 2]
            79..92 'Shape::Circle': fn(usize) -> Shape::Circle
            79..95 'Shape::Circle(1)': Shape::Circle
            93..94 '1': usize
            97..109 'Shape::Point': Shape::Point
        "#]],
    );
}

// ---- the heap builtins --------------------------------------------------

#[test]
fn alloc_array_types_as_the_result_enum() {
    check_infer(
        r#"
static f = fn {
    let r = alloc_array::<usize>(2);
};
"#,
        expect![[r#"
            12..55 'fn {     let r = ...': fn()
            15..55 '{     let r = all...': ()
            25..26 'r': AllocResult::<usize>
            29..49 'alloc_array::<usize>': fn(usize) -> AllocResult::<usize>
            29..52 'alloc_array::<usi...': AllocResult::<usize>
            50..51 '2': usize
        "#]],
    );
}

#[test]
fn alloc_result_ok_payload_binds_the_pointer_type() {
    check_infer(
        r#"
static f = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { p.* },
        AllocResult::Err => 0,
    }
};
"#,
        expect![[r#"
            12..149 'fn () -> usize { ...': fn() -> usize
            27..149 '{     match alloc...': usize
            33..147 'match alloc_array...': usize
            39..59 'alloc_array::<usize>': fn(usize) -> AllocResult::<usize>
            39..62 'alloc_array::<usi...': AllocResult::<usize>
            60..61 '2': usize
            89..90 'p': usize.&raw mut
            95..109 'unsafe { p.* }': usize
            102..109 '{ p.* }': usize
            104..105 'p': usize.&raw mut
            104..107 'p.*': usize
            139..140 '0': usize
        "#]],
    );
}

#[test]
fn match_on_alloc_result_must_cover_the_err_arm() {
    // The result shape has teeth: the `Err` arm exists in the type even
    // though the interpreter never produces it.
    check_diagnostics(
        r#"
static f = fn () -> () {
    match alloc_array::<usize>(1) {
        AllocResult::Ok(p) => { },
    }
};
"#,
        expect![[r#"
            30..35: this `match` does not cover `AllocResult::Err`
        "#]],
    );
}

#[test]
fn dealloc_array_infers_its_type_argument_from_the_pointer() {
    check_infer(
        r#"
static f = fn (p: str.&raw mut) {
    unsafe { dealloc_array(p, 1) };
};
"#,
        expect![[r#"
            12..72 'fn (p: str.&raw m...': fn(str.&raw mut)
            16..17 'p': str.&raw mut
            33..72 '{     unsafe { de...': ()
            39..69 'unsafe { dealloc_...': ()
            46..69 '{ dealloc_array(p...': ()
            48..61 'dealloc_array': fn(str.&raw mut, usize)
            48..67 'dealloc_array(p, 1)': ()
            62..63 'p': str.&raw mut
            65..66 '1': usize
        "#]],
    );
}

#[test]
fn dealloc_array_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    dealloc_array(p, 1);
};
"#,
        expect![[r#"
            41..60: calling `dealloc_array` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn copy_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, q: usize.&raw mut) {
    copy(p, q, 1);
};
"#,
        expect![[r#"
            56..69: calling `copy` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn turbofished_dealloc_outside_unsafe_is_rejected_too() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    dealloc_array::<usize>(p, 1);
};
"#,
        expect![[r#"
            41..69: calling `dealloc_array` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn alloc_and_dangling_need_no_unsafe() {
    // Allocating cannot UB and a dangling pointer only hurts when
    // dereferenced — neither needs the marker. `add`, by contrast, now
    // carries a real precondition, so it is wrapped in `unsafe` here.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    let r = alloc_array::<usize>(1);
    let q = unsafe { add(p, 1) };
    let d = dangling::<usize>();
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn add_outside_unsafe_is_rejected() {
    // `add` joins `dealloc_array`/`copy` as an unsafe builtin: advancing a
    // pointer that does not address an array element (with `i > 0`) is
    // detected UB at the call, so the call needs the marker.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    let q = add(p, 1);
};
"#,
        expect![[r#"
            49..58: calling `add` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn add_preserves_the_pointer_flavor() {
    check_infer(
        r#"
static f = fn (s: usize.&raw, m: usize.&raw mut) {
    let a = unsafe { add(s, 1) };
    let b = unsafe { add(m, 1) };
};
"#,
        expect![[r#"
            12..121 'fn (s: usize.&raw...': fn(usize.&raw, usize.&raw mut)
            16..17 's': usize.&raw
            31..32 'm': usize.&raw mut
            50..121 '{     let a = uns...': ()
            60..61 'a': usize.&raw
            64..84 'unsafe { add(s, 1) }': usize.&raw
            71..84 '{ add(s, 1) }': usize.&raw
            73..82 'add(s, 1)': usize.&raw
            77..78 's': usize.&raw
            80..81 '1': usize
            94..95 'b': usize.&raw mut
            98..118 'unsafe { add(m, 1) }': usize.&raw mut
            105..118 '{ add(m, 1) }': usize.&raw mut
            107..116 'add(m, 1)': usize.&raw mut
            111..112 'm': usize.&raw mut
            114..115 '1': usize
        "#]],
    );
}

#[test]
fn copy_requires_a_mutable_destination() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, q: usize.&raw) {
    unsafe { copy(p, q, 1) };
};
"#,
        expect![[r#"
            69..70: type mismatch: expected `usize.&raw mut`, found `usize.&raw`
        "#]],
    );
}

#[test]
fn copy_source_may_be_either_flavor_but_pointees_must_agree() {
    check_diagnostics(
        r#"
static f = fn (p: str.&raw, q: usize.&raw mut) {
    unsafe { copy(p, q, 1) };
};
"#,
        expect![[r#"
            71..72: type mismatch: expected `str.&raw mut`, found `usize.&raw mut`
        "#]],
    );
}

#[test]
fn add_of_a_non_pointer_is_rejected() {
    check_diagnostics(
        r#"
static f = fn {
    let a = unsafe { add(4, 1) };
};
"#,
        expect![[r#"
            42..43: `add` expects a raw pointer (`T.&raw` or `T.&raw mut`) here, found `{number}`
            42..43: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn offset_is_a_free_identifier_after_the_rename() {
    // `offset` is reserved for a future signed (`isize`) variant but binds
    // NOTHING builtin today: a user item named `offset` is an ordinary
    // item. It even works as a first-class value and needs no `unsafe` —
    // neither of which the flavor-polymorphic `add` builtin could do —
    // proving `offset` no longer resolves to a builtin.
    check_diagnostics(
        r#"
static offset = fn (p: usize.&raw mut) { };
static main = fn (p: usize.&raw mut) {
    let g = offset;
    offset(p);
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn add_is_not_a_first_class_value() {
    // Its pointer parameter accepts both flavors, so there is no one fn
    // type for a `let` to bind.
    check_diagnostics(
        r#"
static f = fn {
    let g = add;
};
"#,
        expect![[r#"
            29..32: `add` must be called directly; its pointer parameter accepts both `T.&raw` and `T.&raw mut`, so it has no one function type to be a value at
        "#]],
    );
}

#[test]
fn heap_calls_are_rejected_in_const_contexts() {
    // The eager const fence (C04): both heap builtins refuse at check
    // time; `add`/`copy`/`dangling` are deliberately not fenced.
    check_diagnostics(
        r#"
static a = alloc_array::<usize>(1);
static b = const fn () -> () { unsafe { dealloc_array(dangling::<usize>(), 0) } };
static ok = const {
    let d = dangling::<usize>();
    unsafe { add(d, 1) };
    1
};
"#,
        expect![[r#"
            12..32: cannot allocate during compile-time evaluation: const-built heap values wait for an interning design (this item's initializer is a const context at 1..7)
            77..90: cannot deallocate during compile-time evaluation: const-built heap values wait for an interning design (this `const fn` is always a const context at 48..53)
            203..204: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn raw_pointer_fields_in_type_declarations_lower() {
    // The declaration side: `T.&raw mut` is a legal field type in
    // a `type` declaration, rigid params included.
    check_infer(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static f = fn (v: HeapVec::<str>) {
    let p = v.ptr;
};
"#,
        expect![[r#"
            84..129 'fn (v: HeapVec::<...': fn(HeapVec::<str>)
            88..89 'v': HeapVec::<str>
            107..129 '{     let p = v.p...': ()
            117..118 'p': str.&raw mut
            121..122 'v': HeapVec::<str>
            121..126 'v.ptr': str.&raw mut
        "#]],
    );
}

#[test]
fn retired_prefix_borrow_type_field_migrates_and_still_needs_a_region() {
    // The retired prefix `&x` superset-parses into the same `BORROW_TYPE`
    // node the postfix form produces (a syntax-level migration diagnostic,
    // see `syntax::tests`), so it flows into hir exactly like a real safe
    // borrow — including the requirement that it name a region. Two
    // genuinely independent complaints, both surviving: the migration
    // (syntax) and the missing region (semantic) — plus `x` naming a value,
    // not a type, unrelated to either.
    check_diagnostics(
        r#"
static x = 4;
type Bad = struct { r: &x };
"#,
        expect![[r#"
            12..13: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: a safe borrow must name its region (`T.&::<@a>`); regions are never elided yet
            38..40: borrow types are spelled postfix: `T.&` / `T.&mut`
            39..40: `x` is not a type
        "#]],
    );
}

#[test]
fn alloc_result_can_be_named_constructed_and_returned() {
    // The compiler-provided enum is an ordinary nominal enum: nameable in
    // annotations, constructible through variant paths, widened
    // variant → enum at the return boundary.
    check_diagnostics(
        r#"
static maybe = fn::<T>(p: T.&raw mut, full: bool) -> AllocResult::<T> {
    if full {
        AllocResult::<T>::Err
    } else {
        AllocResult::<T>::Ok(p)
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_alloc_result() {
    // The `print` precedent, applied to the type namespace: a file
    // declaring its own `AllocResult` sees its own everywhere.
    check_infer(
        r#"
type AllocResult = struct { tag: usize };
static f = fn () -> AllocResult {
    AllocResult(struct { tag = 1 })
};
"#,
        expect![[r#"
            54..114 'fn () -> AllocRes...': fn() -> AllocResult
            75..114 '{     AllocResult...': AllocResult
            81..92 'AllocResult': fn(struct { tag: usize }) -> AllocResult
            81..112 'AllocResult(struc...': AllocResult
            93..111 'struct { tag = 1 }': struct { tag: usize }
            108..109 '1': usize
        "#]],
    );
}

#[test]
fn every_compiler_provided_enum_resolves_with_its_variants() {
    // The end-to-end half of the table's contract: each name resolves in
    // ANNOTATION position without the file declaring it, and each carries
    // exactly its own variants with exactly their payloads — a dropped row
    // makes the annotation unresolved, a wrong variant list makes the
    // exhaustive `match` complain. Named enums on purpose, so a fixture
    // reads like the Must a user writes;
    // `every_table_row_is_registered_and_declared` covers whatever ROWS the
    // table happens to hold, and the per-enum shadowing tests pin the other
    // half of the rule (a user declaration wins).
    check_diagnostics(
        r#"
static f = fn (
    a: AllocResult::<usize>,
    b: ReadLineResult,
    c: NextChar,
    d: Utf8Result,
) -> usize {
    let w = match a { ::Ok(p) => unsafe { p.* }, ::Err => 0 };
    let x = match b { ::Line(line) => line.len(), ::End => 0 };
    let y = match c { ::Char(ch, next) => next, ::End => 0 };
    let z = match d { ::Ok(text) => text.len(), ::Err => 0 };
    w + x + y + z
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn every_table_row_is_registered_and_declared() {
    // The addition half of the table's contract, at the Rust level so that
    // it covers ROWS and not four hand-named enums: whatever
    // `synthetic_decls` holds, every row is in both scopes of an ordinary
    // file under its own reserved location, `type_decl` hands back exactly
    // the row's variants, and `item_data` hands back exactly its binder. A
    // fifth row that nothing registered fails here without anyone having to
    // think to write a fixture for it.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = fn () {};".to_owned(),
    );
    let table = crate::scopes::synthetic_decls();
    assert!(!table.is_empty(), "the table is what this test is about");
    for decl in table.iter() {
        let loc = crate::synthetic_decl_loc(file, decl.name);
        assert_eq!(
            crate::type_scope(&db, file).resolve(decl.name),
            Some(crate::Resolution::TypeItem(loc.clone())),
            "`{}` is missing from the type scope",
            decl.name
        );
        assert_eq!(
            crate::file_scope(&db, file).resolve(decl.name),
            Some(crate::Resolution::TypeItem(loc.clone())),
            "`{}` is missing from the file scope",
            decl.name
        );
        let item = loc.to_id(&db);
        assert_eq!(
            crate::type_decl(&db, item),
            &Some(crate::TypeDeclData::Enum {
                variants: decl.variants.clone(),
            }),
            "`{}`'s declared shape is not its row's",
            decl.name
        );
        let data = crate::item_data(&db, item)
            .as_ref()
            .expect("a compiler-provided enum has item data");
        assert_eq!(data.generics, decl.generics, "`{}`'s binder", decl.name);
        assert_eq!(data.kind, crate::ItemKind::Type, "`{}`'s kind", decl.name);
    }
}

#[test]
fn a_value_item_taking_a_builtin_enum_name_takes_the_type_with_it() {
    // The cross-kind edge of shadowing, pinned as the honest behavior it
    // is: both registrations ask only whether the NAME is declared, never
    // by what KIND of item, so a `static NextChar` takes the name out of
    // the type scope as thoroughly as a `type NextChar` would — and a value
    // item declares no type, so the annotation then resolves to nothing at
    // all — reported as the ordinary "not a type", exactly as it would be
    // for a user's own `static NextChar` shadowing a user's own `type`. The
    // `print` precedent read consistently (a file that spells the name owns
    // it); the alternative, letting a value item and a compiler-provided
    // type share a name, is a rule about KINDS that nothing else in the
    // language has yet.
    check_diagnostics(
        r#"
static NextChar = 'x';
static f = fn (c: NextChar) -> usize { 1 };
"#,
        expect![[r#"
            42..50: `NextChar` is not a type
        "#]],
    );
}

#[test]
fn read_line_result_can_be_matched_in_the_documented_idiom() {
    // The exact idiom `read_line`'s doc comment shows: `loop { match
    // read_line() { ::Line(s) => ..., ::End => break ... } }` typechecks
    // clean with no annotation anywhere — `ReadLineResult` is an ordinary
    // nominal enum resolved the same way `AllocResult` is.
    check_diagnostics(
        r#"
static count_lines = fn () -> usize {
    let mut n = 0;
    loop {
        match read_line() {
            ::Line(s) => { n = n + 1; },
            ::End => break n,
        }
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn read_line_is_a_nullary_first_class_function() {
    // Bare mention (no call): `read_line`'s type names the per-file
    // `ReadLineResult` decl, exactly like `print`'s bare mention names its
    // fixed `fn(str) -> ()`.
    check_infer(
        r#"
static f = fn () {
    let g = read_line;
};
"#,
        expect![[r#"
            12..44 'fn () {     let g...': fn()
            18..44 '{     let g = rea...': ()
            28..29 'g': fn() -> ReadLineResult
            32..41 'read_line': fn() -> ReadLineResult
        "#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_read_line_result() {
    // The `AllocResult`/`print` precedent, applied to `read_line`'s result
    // type: a file declaring its own `ReadLineResult` sees its own
    // everywhere.
    check_infer(
        r#"
type ReadLineResult = struct { tag: usize };
static f = fn () -> ReadLineResult {
    ReadLineResult(struct { tag = 1 })
};
"#,
        expect![[r#"
            57..123 'fn () -> ReadLine...': fn() -> ReadLineResult
            81..123 '{     ReadLineRes...': ReadLineResult
            87..101 'ReadLineResult': fn(struct { tag: usize }) -> ReadLineResult
            87..121 'ReadLineResult(st...': ReadLineResult
            102..120 'struct { tag = 1 }': struct { tag: usize }
            117..118 '1': usize
        "#]],
    );
}

// --- char ---------------------------------------------------------------

#[test]
fn a_character_literal_has_a_definite_type() {
    // No number-class variable, no defining use, no `{number}`: there is
    // exactly one character type, so `'x'` is a `char` the moment it is
    // written — annotated or not, and in either direction.
    check_infer(
        r#"
static f = fn () {
    let a = 'x';
    let b: char = '\n';
    let c = a == b;
};
"#,
        expect![[r#"
            12..82 'fn () {     let a...': fn()
            18..82 '{     let a = 'x'...': ()
            28..29 'a': char
            32..35 ''x'': char
            45..46 'b': char
            55..59 ''\n'': char
            69..70 'c': bool
            73..74 'a': char
            73..79 'a == b': bool
            78..79 'b': char
        "#]],
    );
}

#[test]
fn a_character_is_not_an_integer() {
    // The ruling, checked: `char` is its own primitive, so it never
    // unifies with an integer type in either direction, and it carries no
    // arithmetic.
    check_diagnostics(
        r#"
static a: char = 65;
static b: usize = 'A';
static c = fn (x: char) -> char { x + x };
"#,
        expect![[r#"
            18..20: type mismatch: expected `char`, found `{number}` (expected `char` because of this annotation at 11..15)
            40..43: type mismatch: expected `usize`, found `char` (expected `usize` because of this annotation at 32..37)
            79..80: type mismatch: expected `{number}`, found `char` (`+` requires `{number}` operands at 81..82)
        "#]],
    );
}

#[test]
fn a_character_match_needs_a_catch_all() {
    // A `char` match is never exhaustive by enumeration, so the `_` arm is
    // required as policy, not arithmetic; its absence gets the ordinary
    // non-enum-scrutinee message.
    check_diagnostics(
        r#"
static f = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
static g = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
    }
};
"#,
        expect![[r#"
            150..155: this `match` does not cover every possible `char`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_character_pattern_must_match_the_scrutinee() {
    // A literal pattern's type is DEFINITE, so the pattern is what carries
    // the blame — the scrutinee is not re-typed to suit it.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            59..62: type mismatch: this `match` is on a `usize`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn a_character_pattern_pins_an_unknown_scrutinee() {
    // The other direction of the same rule: the pattern's type is
    // definite, so when the scrutinee is still a variable the pattern
    // pins it — a literal pattern is construction's mirror image, like a
    // qualified variant pattern.
    check_infer(
        r#"
static f = fn (c) -> usize {
    match c {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            12..85 'fn (c) -> usize {...': fn(char) -> usize
            16..17 'c': char
            28..85 '{     match c {  ...': usize
            34..83 'match c {        ...': usize
            40..41 'c': char
            59..60 '1': usize
            75..76 '0': usize
        "#]],
    );
}

#[test]
fn a_repeated_character_arm_is_unreachable() {
    // The variant precedent, applied to literals: the second `'a'` can
    // never run, and saying so beats silently dropping it.
    check_diagnostics(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
        'b' => 2,
        'a' => 3,
        _ => 0,
    }
};
"#,
        expect![[r#"
            94..97: unreachable arm: 'a' is already covered by a previous arm
        "#]],
    );
}

#[test]
fn next_char_is_a_builtin_member_of_str() {
    // Reached through the dot on a `str` receiver, so its type has the
    // dot-callable shape every member has — `str` last (TR01) — and it
    // answers the per-file `NextChar` decl, the way `read_line` answers
    // `ReadLineResult`.
    check_infer(
        r#"
static f = fn (s: str) {
    let n = s.next_char(0);
};
"#,
        expect![[r#"
            12..55 'fn (s: str) {    ...': fn(str)
            16..17 's': str
            24..55 '{     let n = s.n...': ()
            34..35 'n': NextChar
            38..39 's': str
            38..49 's.next_char': fn(usize, str) -> NextChar
            38..52 's.next_char(0)': NextChar
            50..51 '0': usize
        "#]],
    );
}

#[test]
fn a_builtin_member_call_counts_only_the_written_arguments() {
    // The receiver supplies one parameter, so arity is reported against
    // what was written. Not a rule of its own: this is the shared
    // receiver-appending tail, down to the follow-on mismatch an extra
    // argument gets from landing in the receiver's slot.
    check_diagnostics(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        plus = fn(extra: usize, c: Self) -> usize { c.v + extra };
    }
};
static f = fn (s: str, c: Cell) {
    let a = s.next_char();
    let b = s.next_char(0, 1);
    let d = c.plus();
    let e = c.plus(0, 1);
};
"#,
        expect![[r#"
            178..191: expected 1 argument(s), found 0
            205..222: expected 1 argument(s), found 2
            220..221: type mismatch: expected `str`, found `{number}`
            236..244: expected 1 argument(s), found 0
            258..270: expected 1 argument(s), found 2
            268..269: type mismatch: expected `Cell`, found `{number}`
        "#]],
    );
}

#[test]
fn next_char_walks_a_string_in_the_documented_idiom() {
    // The idiom `NextChar`'s doc comment shows: thread the index back in,
    // stop at `::End`. Typechecks with no annotation anywhere.
    check_diagnostics(
        r#"
static count = fn (s: str) -> usize {
    let mut n = 0;
    let mut i = 0;
    loop {
        match s.next_char(i) {
            ::Char(c, next) => { n = n + 1; i = next; },
            ::End => break n,
        }
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn next_char_is_not_a_top_level_name() {
    // The member is reachable only through the dot: `next_char` claims
    // nothing in the value namespace, so a bare mention is an ordinary
    // unresolved name and a user's own `next_char` item is unshadowed.
    check_diagnostics(
        r#"
static f = fn (s: str) { next_char(s, 0); };
"#,
        expect![[r#"
            26..35: unresolved name `next_char`
        "#]],
    );
}

#[test]
fn next_char_is_only_a_member_of_str() {
    // Not a member of every type: a non-`str` receiver gets the ordinary
    // "no such member" story, not a special case.
    check_diagnostics(
        r#"
static f = fn (n: usize) { n.next_char(0); };
"#,
        expect![[r#"
            28..42: no field or member `next_char` on `usize`
        "#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_next_char_enum() {
    // The `AllocResult`/`ReadLineResult` precedent once more: a file
    // declaring its own `NextChar` sees its own everywhere.
    check_infer(
        r#"
type NextChar = struct { tag: usize };
static f = fn () -> NextChar {
    NextChar(struct { tag = 1 })
};
"#,
        expect![[r#"
            51..105 'fn () -> NextChar...': fn() -> NextChar
            69..105 '{     NextChar(st...': NextChar
            75..83 'NextChar': fn(struct { tag: usize }) -> NextChar
            75..103 'NextChar(struct {...': NextChar
            84..102 'struct { tag = 1 }': struct { tag: usize }
            99..100 '1': usize
        "#]],
    );
}

#[test]
fn next_char_is_const_legal() {
    // Decoding a `str` is pure, so there is no effect for a const context
    // to refuse — the pointer builtins' reason, reached through the dot.
    check_diagnostics(
        r#"
static first = const {
    match "ab".next_char(0) {
        ::Char(c, next) => next,
        ::End => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn every_builtin_answers_its_properties() {
    use crate::{Builtin, ConstLegality};

    // The property TABLE, written out: what each builtin answers when
    // asked whether const evaluation accepts it (and if not, why), whether
    // calling it needs `unsafe`, and whether it takes a pointer argument
    // in both raw flavors. All three properties are EXHAUSTIVE matches on
    // the enum, so it is the compiler, not this test, that forces a new
    // builtin to have answers; what this adds is that a CHANGED answer
    // must be changed twice, deliberately.
    //
    // What it does NOT probe is the wiring — that the checkers actually
    // consult the properties. Nothing can, until a builtin exists whose
    // answers differ from the current members' (both are pure and safe,
    // so a member path that ignored the table entirely would still pass
    // every test in this file). That half is held by construction
    // instead: every path — const_check's named callee, const_check's
    // dot-call via `InferenceResult::builtin_of_call`, unsafe_check's
    // named and dot-call arms alike — ends at `Builtin::const_legality`
    // / `Builtin::requires_unsafe`, and none of them names a builtin.
    const ALL: [Builtin; 14] = [
        Builtin::Print,
        Builtin::Panic,
        Builtin::AllocArray,
        Builtin::DeallocArray,
        Builtin::Add,
        Builtin::Offset,
        Builtin::Copy,
        Builtin::Dangling,
        Builtin::ReadLine,
        Builtin::NextChar,
        Builtin::StrFromUtf8,
        Builtin::StrFromUtf8Unchecked,
        Builtin::StrLen,
        Builtin::StrBytes,
    ];

    // `(const_legality, requires_unsafe, flavor_polymorphic)` per builtin.
    fn expected(builtin: Builtin) -> (ConstLegality, bool, bool) {
        match builtin {
            // The host effects: refused by const evaluation, safe to
            // call, no pointer argument.
            Builtin::Print | Builtin::ReadLine => (ConstLegality::HostEffect, false, false),
            // The heap pair: const-FENCED, not effectful (C04) — and
            // freeing is UNSAFE because it invalidates every pointer into
            // the allocation, while allocating cannot UB. Neither takes a
            // flavor-polymorphic pointer: `alloc_array` takes a count,
            // and `dealloc_array` frees one fixed flavor.
            Builtin::AllocArray => (ConstLegality::HeapFence, false, false),
            Builtin::DeallocArray => (ConstLegality::HeapFence, true, false),
            // Pure and safe: `panic` is the one effect const contexts
            // allow, `dangling` only names an address, and the two `str`
            // MEMBERS read what the receiver already holds. None reads a
            // caller-supplied pointer in either flavor.
            Builtin::Panic | Builtin::Dangling | Builtin::NextChar | Builtin::StrLen => {
                (ConstLegality::Legal, false, false)
            }
            // Const-legal AND unsafe — the two properties are orthogonal:
            // pointer arithmetic and range reads carry preconditions
            // nothing checks, but const evaluation detects every would-be
            // UB as a deterministic trap, so it need not refuse them. All
            // four read a pointer in either raw flavor.
            Builtin::Add
            | Builtin::Offset
            | Builtin::Copy
            | Builtin::StrFromUtf8
            | Builtin::StrFromUtf8Unchecked => (ConstLegality::Legal, true, true),
            // Const-legal and unsafe like the group above, but a WRITE
            // through one fixed flavor (`u8.&raw mut`) rather than a read
            // through either — not flavor-polymorphic.
            Builtin::StrBytes => (ConstLegality::Legal, true, false),
        }
    }

    for builtin in ALL {
        assert_eq!(
            (
                builtin.const_legality(),
                builtin.requires_unsafe(),
                builtin.flavor_polymorphic()
            ),
            expected(builtin),
            "{}: (const_legality, requires_unsafe, flavor_polymorphic)",
            builtin.name()
        );
    }
    // A duplicated entry would silently stand in for a missing one.
    for (i, a) in ALL.iter().enumerate() {
        for b in &ALL[..i] {
            assert_ne!(a, b, "{} listed twice", a.name());
        }
    }
}

#[test]
fn a_character_const_argument_type_checks_by_kind() {
    // `char` is in the annotation-representable const domain because it
    // fell out of the same machinery `usize`/`str`/`bool` use, not because
    // it was carved in — so the mismatches have to be checked rather than
    // assumed. An integer, a string and a MALFORMED literal each get their
    // own answer; the malformed one is silent here because the lexer
    // already said what is wrong with it (errors are infectious and
    // silent, never doubled).
    check_diagnostics(
        r#"
static pick = const fn::<const C: char>() -> char { C };
static ok = pick::<'x'>();
static from_int = pick::<5>();
static from_str = pick::<"x">();
static from_empty = pick::<''>();
"#,
        expect![[r#"
            110..111: type mismatch: expected `char`, found `{number}`
            141..144: type mismatch: expected `char`, found `str`
            176..178: empty character literal: a character literal holds exactly one character
        "#]],
    );
}

#[test]
fn next_char_through_a_borrow_names_the_deref() {
    // A builtin member takes its receiver BY VALUE, so a borrow does not
    // reach it — that is auto-deref, sealed. The refusal must be the one a
    // value-`Self` USER member gets in the same position (`.*.name`), not
    // "no such member": the member plainly exists, and saying otherwise
    // would send the reader hunting for a spelling instead of a `.*`.
    check_diagnostics(
        r#"
static f = fn::<@a>(r: str.&::<@a>) -> () {
    let n = r.next_char(0);
};
"#,
        expect![[r#"
            57..68: `str.&::<@a>` is a borrow, so `.next_char` does not reach through it — there is no auto-deref; write `.*.next_char`
        "#]],
    );
}

#[test]
fn a_user_member_shadows_the_builtin_next_char() {
    // Load-bearing in three doc comments: builtin members are consulted
    // only after every user candidate has had its turn, so a `next_char`
    // written in an `impl ... for str` WINS. The return type is the
    // witness — `usize` here, `NextChar` if the builtin had taken it.
    check_infer(
        r#"
trait Chars = requires {
    next_char: fn(i: usize, s: Self) -> usize;
} with {
    impl str {
        next_char = fn (i: usize, s: str) -> usize { i };
    }
};
static f = fn (s: str) {
    let n = s.next_char(0);
};
"#,
        expect![[r#"
            175..218 'fn (s: str) {    ...': fn(str)
            179..180 's': str
            187..218 '{     let n = s.n...': ()
            197..198 'n': usize
            201..202 's': str
            201..212 's.next_char': fn(usize, str) -> usize
            201..215 's.next_char(0)': usize
            213..214 '0': usize
        "#]],
    );
}

#[test]
fn a_character_pattern_projects_through_a_borrow() {
    // M13's sealed rule is "the scrutinee's flavor decides, all the way
    // down", and a literal pattern is a pattern: `match c { 'a' => ... }`
    // means the same thing whether `c` is a `char` or a `char.&`.
    // Exhaustiveness is unchanged — a `_` arm is still required — and the
    // message now names the REFERENT rather than the borrow, because the
    // lens is what the scrutinee is being read through.
    check_diagnostics(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
static g = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        '(' => 1,
    }
};
"#,
        expect![[r#"
            176..181: this `match` does not cover every possible `char`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_binder_on_a_borrowed_character_still_binds_the_borrow() {
    // The whole-value binder rule, unchanged by the projection: it names
    // the same place the scrutinee does, so it gets the borrow itself back
    // at the scrutinee's own region — never a copied-out `char`.
    check_infer(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        'x' => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            12..108 'fn::<@a>(c: char....': fn(char.&::<@a>) -> usize
            21..22 'c': char.&::<@a>
            47..108 '{     match c {  ...': usize
            53..106 'match c {        ...': usize
            59..60 'c': char.&::<@a>
            78..79 '1': usize
            89..94 'other': char.&::<@a>
            98..99 '0': usize
        "#]],
    );
}

#[test]
fn dangling_infers_from_the_expected_pointer_type() {
    check_infer(
        r#"
static f = fn {
    let p: str.&raw mut = dangling();
};
"#,
        expect![[r#"
            12..56 'fn {     let p: s...': fn()
            15..56 '{     let p: str....': ()
            25..26 'p': str.&raw mut
            43..51 'dangling': fn() -> str.&raw mut
            43..53 'dangling()': str.&raw mut
        "#]],
    );
}

#[test]
fn unpinned_generic_builtin_mentions_ask_for_a_turbofish() {
    check_diagnostics(
        r#"
static f = fn {
    let r = alloc_array(1);
};
"#,
        expect![[r#"
            29..40: cannot infer the type parameter `T` of `alloc_array`; write `alloc_array::<...>` to specify it
        "#]],
    );
}

// --- Integer types: the i/u × 8/16/32/64 menu plus usize/isize ---

#[test]
fn every_integer_type_annotates_and_infers() {
    check_infer(
        r#"
static f = fn (
    a: i8, b: i16, c: i32, d: i64,
    e: u8, g: u16, h: u32, i: u64,
    j: usize, k: isize,
) {};
"#,
        expect![[r#"
            12..115 'fn (     a: i8, b...': fn(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize)
            21..22 'a': i8
            28..29 'b': i16
            36..37 'c': i32
            44..45 'd': i64
            56..57 'e': u8
            63..64 'g': u16
            71..72 'h': u32
            79..80 'i': u64
            91..92 'j': usize
            101..102 'k': isize
            113..115 '{}': ()
        "#]],
    );
}

#[test]
fn literal_pins_through_an_annotation() {
    check_infer(
        "static f = fn { let x: u8 = 7; };",
        expect![[r#"
            11..32 'fn { let x: u8 = ...': fn()
            14..32 '{ let x: u8 = 7; }': ()
            20..21 'x': u8
            28..29 '7': u8
        "#]],
    );
}

#[test]
fn literal_pins_through_a_parameter_type() {
    check_infer(
        r#"
static g = fn (n: u16) {};
static f = fn { g(3); };
"#,
        expect![[r#"
            12..26 'fn (n: u16) {}': fn(u16)
            16..17 'n': u16
            24..26 '{}': ()
            39..51 'fn { g(3); }': fn()
            42..51 '{ g(3); }': ()
            44..45 'g': fn(u16)
            44..48 'g(3)': ()
            46..47 '3': u16
        "#]],
    );
}

#[test]
fn index_position_pins_usize() {
    check_infer(
        r#"static f = fn (a: [u8; 4]) -> u8 { a[1] };"#,
        expect![[r#"
            11..41 'fn (a: [u8; 4]) -...': fn([u8; 4]) -> u8
            15..16 'a': [u8; 4]
            33..41 '{ a[1] }': u8
            35..36 'a': [u8; 4]
            35..39 'a[1]': u8
            37..38 '1': usize
        "#]],
    );
}

#[test]
fn repeat_count_pins_usize_and_the_element_pins_through_the_annotation() {
    check_infer(
        r#"static f = fn { let a: [u8; 3] = [0; 3]; };"#,
        expect![[r#"
            11..42 'fn { let a: [u8; ...': fn()
            14..42 '{ let a: [u8; 3] ...': ()
            20..21 'a': [u8; 3]
            33..39 '[0; 3]': [u8; 3]
            34..35 '0': u8
            37..38 '3': usize
        "#]],
    );
}

#[test]
fn alloc_count_pins_usize() {
    check_infer(
        r#"static f = fn { let r = alloc_array::<u8>(4); };"#,
        expect![[r#"
            11..47 'fn { let r = allo...': fn()
            14..47 '{ let r = alloc_a...': ()
            20..21 'r': AllocResult::<u8>
            24..41 'alloc_array::<u8>': fn(usize) -> AllocResult::<u8>
            24..44 'alloc_array::<u8>(4)': AllocResult::<u8>
            42..43 '4': usize
        "#]],
    );
}

#[test]
fn typed_operand_pins_the_other_side() {
    check_infer(
        r#"static f = fn (n: u32) -> u32 { n + 1 };"#,
        expect![[r#"
            11..39 'fn (n: u32) -> u3...': fn(u32) -> u32
            15..16 'n': u32
            30..39 '{ n + 1 }': u32
            32..33 'n': u32
            32..37 'n + 1': u32
            36..37 '1': u32
        "#]],
    );
}

#[test]
fn number_variables_merge_then_a_late_use_pins_the_whole_chain() {
    // `1`, `2` and the intermediates share one NUMBER variable; the final
    // `i64` annotation is the one defining use — it pins them all.
    check_infer(
        r#"
static f = fn {
    let a = 1;
    let b = a + 2;
    let c: i64 = b;
};
"#,
        expect![[r#"
            12..72 'fn {     let a = ...': fn()
            15..72 '{     let a = 1; ...': ()
            25..26 'a': i64
            29..30 '1': i64
            40..41 'b': i64
            44..45 'a': i64
            44..49 'a + 2': i64
            48..49 '2': i64
            59..60 'c': i64
            68..69 'b': i64
        "#]],
    );
}

#[test]
fn unpinned_number_renders_as_number_and_reports_no_defining_use() {
    check_infer(
        "static f = fn { let n = 3; };",
        expect![[r#"
            11..28 'fn { let n = 3; }': fn()
            14..28 '{ let n = 3; }': ()
            20..21 'n': {number}
            24..25 '3': {number}
        "#]],
    );
    check_diagnostics(
        "static f = fn { let n = 3; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn unannotated_static_number_reports_no_defining_use_too() {
    // The same never-default rule at item level: the definition carries
    // the number diagnostic; uses see an undetermined signature.
    check_diagnostics(
        "static x = 3;",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
    check_diagnostics(
        "static x = 3;\nstatic f = fn { let y = x; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: cannot infer the type of `x` across items; add a type annotation to its definition (defined here at 7..8)
        "#]],
    );
}

#[test]
fn literal_out_of_range_for_its_resolved_type() {
    check_diagnostics(
        "static a: u8 = 300;",
        expect![[r#"
            15..18: `300` does not fit in `u8`
        "#]],
    );
    check_diagnostics("static b: u8 = 255;", expect![[r#""#]]);
}

#[test]
fn negative_literal_in_an_unsigned_type_is_a_range_error() {
    check_diagnostics(
        "static a: u32 = -1;",
        expect![[r#"
            17..18: `-1` does not fit in `u32`
        "#]],
    );
    // `-0` is the one negation every unsigned type holds.
    check_diagnostics("static b: u32 = -0;", expect![[r#""#]]);
}

#[test]
fn i64_min_fits_and_one_below_does_not() {
    check_diagnostics("static min: i64 = -9223372036854775808;", expect![[r#""#]]);
    check_diagnostics(
        "static below: i64 = -9223372036854775809;",
        expect![[r#"
            21..40: `-9223372036854775809` does not fit in `i64`
        "#]],
    );
    check_diagnostics(
        "static above: i64 = 9223372036854775808;",
        expect![[r#"
            20..39: `9223372036854775808` does not fit in `i64`
        "#]],
    );
}

#[test]
fn i8_range_edges_respect_the_sign() {
    check_diagnostics("static min: i8 = -128;", expect![[r#""#]]);
    check_diagnostics(
        "static plus: i8 = 128;",
        expect![[r#"
            18..21: `128` does not fit in `i8`
        "#]],
    );
}

#[test]
fn mixed_integer_types_are_an_ordinary_mismatch() {
    // No implicit conversions: `u8 + u32` doesn't unify, and no cast
    // syntax exists to bridge them.
    check_diagnostics(
        "static f = fn (a: u8, b: u32) -> u8 { a + b };",
        expect![[r#"
            42..43: type mismatch: expected `u8`, found `u32` (`+` requires `u8` operands at 40..41)
        "#]],
    );
}

#[test]
fn unary_minus_types_as_its_operand() {
    check_infer(
        "static f = fn (n: i32) -> i32 { -n };",
        expect![[r#"
            11..36 'fn (n: i32) -> i3...': fn(i32) -> i32
            15..16 'n': i32
            30..36 '{ -n }': i32
            32..34 '-n': i32
            33..34 'n': i32
        "#]],
    );
    // Legal syntax on unsigned operands (traps at runtime unless zero).
    check_diagnostics("static f = fn (n: u8) -> u8 { -n };", expect![[r#""#]]);
}

#[test]
fn offset_builtin_takes_an_isize_and_preserves_the_flavor() {
    check_infer(
        r#"
static f = fn (p: u8.&raw mut, i: isize) -> u8.&raw mut {
    unsafe { offset(p, i) }
};
"#,
        expect![[r#"
            12..88 'fn (p: u8.&raw mu...': fn(u8.&raw mut, isize) -> u8.&raw mut
            16..17 'p': u8.&raw mut
            32..33 'i': isize
            57..88 '{     unsafe { of...': u8.&raw mut
            63..86 'unsafe { offset(p...': u8.&raw mut
            70..86 '{ offset(p, i) }': u8.&raw mut
            72..84 'offset(p, i)': u8.&raw mut
            79..80 'p': u8.&raw mut
            82..83 'i': isize
        "#]],
    );
    // The literal pins to `isize` through the parameter position.
    check_infer(
        r#"
static f = fn (p: u8.&raw) -> u8.&raw {
    unsafe { offset(p, 1) }
};
"#,
        expect![[r#"
            12..70 'fn (p: u8.&raw) -...': fn(u8.&raw) -> u8.&raw
            16..17 'p': u8.&raw
            39..70 '{     unsafe { of...': u8.&raw
            45..68 'unsafe { offset(p...': u8.&raw
            52..68 '{ offset(p, 1) }': u8.&raw
            54..66 'offset(p, 1)': u8.&raw
            61..62 'p': u8.&raw
            64..65 '1': isize
        "#]],
    );
}

#[test]
fn offset_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: u8.&raw mut, i: isize) {
    let q = offset(p, i);
};
"#,
        expect![[r#"
            56..68: calling `offset` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn offset_index_must_be_isize() {
    check_diagnostics(
        r#"
static f = fn (p: u8.&raw mut, n: usize) {
    let q = unsafe { offset(p, n) };
};
"#,
        expect![[r#"
            75..76: type mismatch: expected `isize`, found `usize`
        "#]],
    );
}

#[test]
fn generic_call_with_only_a_bare_literal_reports_the_number_not_the_param() {
    // The only information about `T` is an unpinned literal: an unresolved
    // number is not a type, and the actionable diagnostic is the literal's
    // own annotate-me error — not a cannot-infer-`T` on top.
    check_diagnostics(
        r#"
static id = fn::<T>(x: T) -> T { x };
static f = fn { let y = id(3); };
"#,
        expect![[r#"
            66..67: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn turbofish_pins_a_literal_argument_through_the_type_param() {
    check_infer(
        r#"
static id = fn::<T>(x: T) -> T { x };
static f = fn { let y = id::<u8>(3); };
"#,
        expect![[r#"
            13..37 'fn::<T>(x: T) -> ...': fn(T) -> T
            21..22 'x': T
            32..37 '{ x }': T
            34..35 'x': T
            50..77 'fn { let y = id::...': fn()
            53..77 '{ let y = id::<u8...': ()
            59..60 'y': u8
            63..71 'id::<u8>': fn(u8) -> u8
            63..74 'id::<u8>(3)': u8
            72..73 '3': u8
        "#]],
    );
}

#[test]
fn const_param_declared_with_a_sized_type_checks_literal_range() {
    // A const param may use any integer type; the written argument is
    // range-checked against it like any pinned literal.
    check_diagnostics(
        r#"
type Buf = struct::<const N: u8> { len: usize };
static b: Buf::<300> = Buf::<300>(struct { len = 1 });
"#,
        expect![[r#"
            66..69: `300` does not fit in `u8`
            79..82: `300` does not fit in `u8`
        "#]],
    );
}

#[test]
fn match_on_an_integer_scrutinee_requires_a_catch_all_uniformly() {
    // Integers are open-domain for every width — same rule `usize` always
    // had, extended uniformly.
    check_diagnostics(
        r#"
static f = fn (n: u8) -> str {
    match n {
        _ => "something",
    }
};
"#,
        expect![[r#""#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: i16) -> str {
    match n {
    }
};
"#,
        expect![[r#"
            37..42: this `match` does not cover every possible `i16`; add a `_` arm
        "#]],
    );
}

// ---- inherent members and dot-calls -------------------------------------

#[test]
fn dot_call_happy_path() {
    check_infer(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Counter(struct { n = c.n + by }) };
    }
};
static main = fn() -> usize {
    let c = Counter(struct { n = 3 });
    c.bump(2).get()
};
"#,
        expect![[r#"
            210..286 'fn() -> usize {  ...': fn() -> usize
            224..286 '{     let c = Cou...': usize
            234..235 'c': Counter
            238..245 'Counter': fn(struct { n: usize }) -> Counter
            238..263 'Counter(struct { ...': Counter
            246..262 'struct { n = 3 }': struct { n: usize }
            259..260 '3': usize
            269..270 'c': Counter
            269..275 'c.bump': fn(usize, Counter) -> Counter
            269..278 'c.bump(2)': Counter
            269..282 'c.bump(2).get': fn(Counter) -> usize
            269..284 'c.bump(2).get()': usize
            276..277 '2': usize
        "#]],
    );
}

#[test]
fn dot_call_happy_path_no_diagnostics() {
    check_diagnostics(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
    }
};
static main = fn() -> usize { Counter(struct { n = 3 }).get() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn dot_call_on_generic_type_member() {
    check_infer(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { b.v };
        put = fn(x: T, b: Self) -> Self { Box2::<T>(struct { v = x }) };
    }
};
static main = fn() -> str {
    Box2(struct { v = "hi" }).put("ho").get()
};
"#,
        expect![[r#"
            193..254 'fn() -> str {    ...': fn() -> str
            205..254 '{     Box2(struct...': str
            211..215 'Box2': fn(struct { v: str }) -> Box2::<str>
            211..236 'Box2(struct { v =...': Box2::<str>
            211..240 'Box2(struct { v =...': fn(str, Box2::<str>) -> Box2::<str>
            211..246 'Box2(struct { v =...': Box2::<str>
            211..250 'Box2(struct { v =...': fn(Box2::<str>) -> str
            211..252 'Box2(struct { v =...': str
            216..235 'struct { v = "hi" }': struct { v: str }
            229..233 '"hi"': str
            241..245 '"ho"': str
        "#]],
    );
}

#[test]
fn member_self_resolves_in_signature_and_body() {
    check_diagnostics(
        r#"
type Wrap = struct { v: usize } with {
    impl Self {
        dup = fn(w: Self) -> Self { Self(struct { v = w.v }) };
    }
};
static main = fn() -> Wrap { Wrap(struct { v = 1 }).dup() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn member_signature_must_be_fully_annotated() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        f = fn(a) { a.x };
    }
};
"#,
        expect![[r#"
            61..62: member `f` must spell its full signature: every parameter and the return type
            73..74: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn statics_are_never_dot_callable() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static double = fn(a: A) -> usize { a.get() * 2 };
static main = fn() -> usize {
    let a = A(struct { x = 3 });
    a.double()
};
"#,
        expect![[r#"
            224..234: no field or member `double` on `A` (a module-level `double` is defined here — statics are never dot-callable; call `double(...)` instead at 113..119)
        "#]],
    );
}

#[test]
fn member_without_self_last_param_is_not_dot_callable() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        mk = fn(x: usize) -> usize { x };
        rev = fn(a: Self, x: usize) -> usize { x };
    }
};
static main = fn() -> usize {
    let a = A(struct { x = 3 });
    a.mk(1) + a.rev(2)
};
"#,
        expect![[r#"
            223..230: `mk` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`mk` is defined here at 61..63)
            233..241: `rev` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`rev` is defined here at 103..106)
        "#]],
    );
}

#[test]
fn member_reached_without_a_call() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 3 }).get };
"#,
        expect![[r#"
            136..159: `get` is a member fn, not a field; call it: `.get(...)`
        "#]],
    );
}

#[test]
fn duplicate_member_reported() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
        get = fn(a: Self) -> usize { 0 };
    }
};
"#,
        expect![[r#"
            105..108: duplicate member `get` (first defined here at 61..64)
        "#]],
    );
}

#[test]
fn member_may_share_a_field_name_getter_idiom() {
    // SEPARATE NAMESPACES (G13): a member named like a field is
    // legal. Call syntax selects the MEMBER (which may read the FIELD of
    // the same name through bare access in its own body) — the getter
    // idiom, clean end to end.
    check_infer(
        r#"
type Vecish = struct { len: usize } with {
    impl Self {
        len = fn(v: Self) -> usize { v.len };
    }
};
static main = fn() -> usize {
    let v = Vecish(struct { len = 3 });
    v.len() + v.len
};
"#,
        expect![[r#"
            129..206 'fn() -> usize {  ...': fn() -> usize
            143..206 '{     let v = Vec...': usize
            153..154 'v': Vecish
            157..163 'Vecish': fn(struct { len: usize }) -> Vecish
            157..183 'Vecish(struct { l...': Vecish
            164..182 'struct { len = 3 }': struct { len: usize }
            179..180 '3': usize
            189..190 'v': Vecish
            189..194 'v.len': fn(Vecish) -> usize
            189..196 'v.len()': usize
            189..204 'v.len() + v.len': usize
            199..200 'v': Vecish
            199..204 'v.len': usize
        "#]],
    );
}

#[test]
fn parenthesized_field_access_is_a_value_call_not_a_dot_call() {
    // `(recv.name)(...)` is an ordinary value call of the FIELD access —
    // the parens opt out of member selection, so a member-only name gets
    // the member-must-be-called diagnostic from the bare-access path.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { (A(struct { x = 1 }).get)() };
"#,
        expect![[r#"
            137..160: `get` is a member fn, not a field; call it: `.get(...)`
        "#]],
    );
}

#[test]
fn calling_a_plain_field_names_the_field_and_the_missing_member() {
    check_diagnostics(
        r#"
type A = struct { x: usize };
static main = fn() -> usize { A(struct { x = 1 }).x() };
"#,
        expect![[r#"
            61..84: field `x` is not callable (its type is `usize`), and `A` has no member `x`
        "#]],
    );
}

#[test]
fn qualified_inherent_member_reference_is_live() {
    // G13: `Type::member(value)` names the type's OWN member — the
    // escape a collision's diagnostic points at, and legal on its own.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A::get(A(struct { x = 1 })) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn dot_call_is_field_first() {
    // A fn-valued field keeps today's meaning on receivers without
    // members: `a.f(5)` on a record receiver calls the FIELD's fn value —
    // structural records cannot carry members, so the field is the only
    // namespace in play. (On NAMED receivers, call syntax selects a
    // member FIRST — see `member_may_share_a_field_name_getter_idiom`.)
    check_infer(
        r#"
static main = fn() -> usize {
    let a = struct { f = fn(n: usize) -> usize { n + n } };
    a.f(5)
};
"#,
        expect![[r#"
            15..103 'fn() -> usize {  ...': fn() -> usize
            29..103 '{     let a = str...': usize
            39..40 'a': struct { f: fn(usize) -> usize }
            43..89 'struct { f = fn(n...': struct { f: fn(usize) -> usize }
            56..87 'fn(n: usize) -> u...': fn(usize) -> usize
            59..60 'n': usize
            78..87 '{ n + n }': usize
            80..81 'n': usize
            80..85 'n + n': usize
            84..85 'n': usize
            95..96 'a': struct { f: fn(usize) -> usize }
            95..98 'a.f': fn(usize) -> usize
            95..101 'a.f(5)': usize
            99..100 '5': usize
        "#]],
    );
}

#[test]
fn dot_call_arity_does_not_count_self() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        add = fn(y: usize, a: Self) -> usize { a.x + y };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).add() };
"#,
        expect![[r#"
            150..175: expected 1 argument(s), found 0
        "#]],
    );
}

#[test]
fn dot_call_argument_mismatch_blames_the_argument() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        add = fn(y: usize, a: Self) -> usize { a.x + y };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).add("no") };
"#,
        expect![[r#"
            174..178: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn member_bodies_are_checked() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> str { a.x };
    }
};
"#,
        expect![[r#"
            88..91: type mismatch: expected `str`, found `usize` (expected `str` because of this return type at 79..85)
        "#]],
    );
}

#[test]
fn members_call_members_through_the_dot() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
        twice = fn(a: Self) -> usize { a.get() + a.get() };
    }
};
static main = fn() -> usize { A(struct { x = 2 }).twice() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn enum_types_can_carry_members() {
    check_diagnostics(
        r#"
type Light = enum { Red, Green } with {
    impl Self {
        flip = fn(l: Self) -> Light {
            match l {
                ::Red => Light::Green,
                ::Green => Light::Red,
            }
        };
    }
};
static main = fn() -> Light { Light::Red.flip() };
"#,
        expect![""],
    );
}

#[test]
fn reserved_groups_mint_no_members() {
    // A member inside a constrained group parses (and the group carries
    // its reservation diagnostic) but resolves nothing.
    check_diagnostics(
        r#"
type A = struct { x: usize } with T: copy {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            35..42: `with T: ...` constrained groups are not supported yet
            144..169: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn reserved_member_spelling_mints_no_member() {
    // A `type`-keyword member parses inside a plain `impl Self { ... }`
    // (and validation flags its spelling as reserved) but must not mint a
    // dot-callable item — a minted item here would make the "not
    // supported yet" diagnostic a lie, since the member would still
    // resolve and dot-call despite being reserved.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        type get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            61..65: associated types are not supported yet
            141..166: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn dot_call_on_type_without_members() {
    check_diagnostics(
        r#"
type A = struct { x: usize };
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            61..86: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn member_signature_pinned_to_concrete_args_is_not_self_typed() {
    // On a generic type, `Self` means the type at its FULL binders — a
    // member whose last param pins the args is not dot-callable.
    check_diagnostics(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get_pinned = fn(b: Box2::<usize>) -> usize { b.v };
    }
};
static main = fn() -> usize { Box2(struct { v = 1 }).get_pinned() };
"#,
        expect![[r#"
            156..191: `get_pinned` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`get_pinned` is defined here at 65..75)
            174..175: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

/// The signature/body query split for MEMBERS: editing one member's
/// BODY re-infers only that member — its sibling's signature and the
/// dot-calling item both backdate (member signatures are annotation-
/// derived through the range-free `type_members`, which a body edit
/// leaves value-equal).
#[test]
fn firewall_member_body_edit_does_not_reinfer_siblings_or_callers() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "type A = struct { x: usize } with {\n\
                       impl Self {\n\
                           get = fn(a: Self) -> usize { a.x };\n\
                           dbl = fn(a: Self) -> usize { a.x * 2 };\n\
                       }\n\
                   };\n\
                   static use_it: fn(A) -> usize = fn (a: A) -> usize { a.get() };\n";
    // Only `get`'s BODY changes; every signature stays identical.
    let text_v2 = "type A = struct { x: usize } with {\n\
                       impl Self {\n\
                           get = fn(a: Self) -> usize { a.x + 0 };\n\
                           dbl = fn(a: Self) -> usize { a.x * 2 };\n\
                       }\n\
                   };\n\
                   static use_it: fn(A) -> usize = fn (a: A) -> usize { a.get() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for item in crate::all_checkable_items(&db, file) {
        crate::infer::infer(&db, item);
    }
    let executed_infers = |log: &Mutex<Vec<String>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.contains("infer"))
            .count()
    };
    // The type item itself has no body to infer, but its query still runs
    // once: 4 checkable units (A, get, dbl, use_it).
    assert_eq!(executed_infers(&log), 4, "all units inferred initially");

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for item in crate::all_checkable_items(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited member may re-infer; executed: {log:#?}"
    );
}

#[test]
fn no_auto_deref_through_raw_pointers() {
    // NO auto-deref, ever (G14): a pointer to a type with members does not
    // dot-call them. A raw pointer is not a decayed borrow, so G14's
    // bounded exception — which reborrows a borrow receiver into a
    // borrow-`Self` member — never reaches it.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize {
    let mut a = A(struct { x = 1 });
    let p = a.&raw mut;
    p.get()
};
"#,
        expect![[r#"
            203..206: no field `get` on `A.&raw mut`
        "#]],
    );
}

#[test]
fn generic_owner_member_annotations_are_clean() {
    // The annotation mirror must treat the OWNER's binder (and `Self`) as
    // bound inside member signatures and bodies — no spurious unknown-type
    // diagnostics.
    check_diagnostics(
        r#"
type Stack = struct::<T> { top: T, rest: usize } with {
    impl Self {
        peek = fn(s: Self) -> T { s.top };
        with_top = fn(x: T, s: Self) -> Stack::<T> {
            let keep: T = x;
            Stack::<T>(struct { top = keep, rest = s.rest })
        };
    }
};
static main = fn() -> usize { Stack(struct { top = 4, rest = 0 }).with_top(9).peek() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn const_generic_owner_member_annotations_are_clean() {
    check_diagnostics(
        r#"
type Buf = struct::<const N: usize> { used: usize } with {
    impl Self {
        cap = fn(b: Self) -> usize { N };
        pad = fn(b: Self) -> [usize; N] { [0; N] };
    }
};
static main = fn() -> usize { Buf::<8>(struct { used = 3 }).cap() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn expression_self_is_rigid() {
    // Expression-position `Self` is the RIGID Self (one meaning of `Self`
    // per body): on a generic owner it constructs at the member's own
    // binders — so a literal payload where `T` is expected is a mismatch,
    // and `let x: Self = Self(...)` round-trips clean.
    check_diagnostics(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        keep = fn(b: Self) -> Self {
            let x: Self = Self(struct { v = b.v });
            x
        };
        bad = fn(b: Self) -> Self { Self(struct { v = 1 }) };
    }
};
"#,
        expect![[r#"
            225..226: type mismatch: expected `T`, found `{number}` (expected `T` because of this field declaration at 27..31)
        "#]],
    );
}

/// Member bodies are NOT a second class of call site. A member is required
/// to spell its full signature, which makes it a fully-typed item — and a
/// fully-typed item is a firewall: its body's call sites contribute no
/// reference edge, so a group-inferrable static called only from inside one
/// stays unconstrained. The comparison arm proves that is the SAME rule an
/// annotated static obeys, not a member-specific gap.
#[test]
fn a_member_body_firewalls_its_callees_like_an_annotated_static() {
    let from_member = r#"
static id = fn (x) { x };
type A = struct { n: usize } with {
    impl Self {
        use_it = fn(a: Self) -> usize { id(a.n) };
    }
};
"#;
    let from_annotated_static = r#"
static id = fn (x) { x };
static use_it = fn (n: usize) -> usize { id(n) };
"#;
    check_diagnostics(
        from_member,
        expect![[r#"
            119..121: cannot infer the type of `id` across items; add a type annotation to its definition (defined here at 8..10)
        "#]],
    );
    check_diagnostics(
        from_annotated_static,
        expect![[r#"
            68..70: cannot infer the type of `id` across items; add a type annotation to its definition (defined here at 8..10)
        "#]],
    );
}

#[test]
fn dot_call_on_broken_declaration_with_members_stays_silent() {
    // The `with`-chain parses independently of a broken RHS, so the member
    // resolves even though the type does not. Errors are infectious and
    // SILENT: the declaration carries its own diagnostic, and a dot-call
    // must not additionally be told to call what it already called.
    check_diagnostics(
        r#"
type A = 5 with {
    impl Self {
        get = fn(a: Self) -> usize { 0 };
    }
};
static main = fn (a: A) -> usize { a.get() };
"#,
        expect![[r#"
            10..11: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

#[test]
fn dot_call_on_broken_declaration_stays_silent() {
    // Errors are infectious and SILENT: a broken type declaration carries
    // its own diagnostic; a dot-call on a value of that type must not
    // cascade a "no field or member" on top (same posture as plain field
    // access).
    check_diagnostics(
        r#"
type A = 5;
static main = fn (a: A) -> usize { a.get() };
"#,
        expect![[r#"
            10..11: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

// ---- the record-literal equals-defines respell --------------------------

#[test]
fn record_field_ascription_checks_the_value() {
    // The spelled-out member production `name: Type = value`: the colon
    // half is a real annotation — it pins the value's type (a defining use
    // for the literal) and must itself agree with the field's expected
    // type.
    check_infer(
        r#"
static a = struct { n: u8 = 10 };
"#,
        expect![[r#"
            12..33 'struct { n: u8 = ...': struct { n: u8 }
            29..31 '10': u8
        "#]],
    );
}

#[test]
fn record_field_ascription_mismatch_is_reported() {
    check_diagnostics(
        r#"
static a = struct { n: u8 = "no" };
"#,
        expect![[r#"
            29..33: type mismatch: expected `u8`, found `str`
        "#]],
    );
}

#[test]
fn record_field_ascription_must_agree_with_the_expected_field() {
    check_diagnostics(
        r#"
type P = struct { n: usize };
static a = P(struct { n: u8 = 10 });
"#,
        expect![[r#"
            61..63: type mismatch: expected `usize`, found `u8` (expected `usize` because of this field declaration at 19..27)
        "#]],
    );
}

#[test]
fn construction_field_without_a_value_is_rejected() {
    check_diagnostics(
        r#"
static a: struct { n: usize } = struct { n: usize };
"#,
        expect![[r#"
            42..50: this field has a type but no value; write `name: Type = value` (or `name = value`)
        "#]],
    );
}

#[test]
fn type_decl_field_with_a_value_is_rejected() {
    check_diagnostics(
        r#"
type P = struct { n: usize = 5 };
"#,
        expect![[r#"
            28..31: a `type` declaration's field declares a type, not a value
        "#]],
    );
}

#[test]
fn old_colon_construction_spelling_gets_the_targeted_error() {
    check_diagnostics(
        r#"
static a: struct { n: usize } = struct { n: 1 };
"#,
        expect![[r#"
            45..46: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

// ---- trait declarations: coherence, matching, bounds, resolution --------

/// The proof shape: both impl homes, a bounded generic fn, a
/// generic requirement, bound-directed and impl-directed calls, the
/// qualified short form. Must be diagnostics-clean.
const TRAIT_FIXTURE: &str = r#"
trait Write = requires {
    push: fn(s: str, w: Self) -> Self;
};
trait Display = requires {
    fmt: fn::<W: Write>(w: W, x: Self) -> W;
} with {
    impl str {
        fmt = fn::<W: Write>(w: W, x: str) -> W { w.push(x) };
    }
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("n") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self { Sink(struct { pushes = w.pushes + 1 }) };
    }
};
type Point = struct { x: usize, y: usize } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, p: Self) -> W {
            let w = Display::fmt(w, p.x);
            Display::fmt(w, p.y)
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn() -> () {
    let a = show::<usize>(1);
    let b = show::<str>("s");
    let c = show::<Point>(Point(struct { x = 1, y = 2 }));
    let n: usize = 7;
    let d = n.fmt(Sink(struct { pushes = 0 }));
    let e = Display::fmt(Sink(struct { pushes = 0 }), "q");
    let _ = a + b + c + d.pushes + e.pushes;
};
"#;

#[test]
fn trait_fixture_is_clean() {
    check_diagnostics(TRAIT_FIXTURE, expect![[r#""#]]);
}

#[test]
fn duplicate_impl_names_both_sites() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { 1 }; }
    impl usize { m = fn(x: usize) -> usize { 2 }; }
};
"#,
        expect![[r#"
            117..122: duplicate impl of `D` for `usize` (first implemented here at 65..70)
            125..126: duplicate member `m` (first defined here at 73..74)
        "#]],
    );
}

#[test]
fn duplicate_impl_across_homes_names_both_sites() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl P { m = fn(x: Self) -> usize { 1 }; }
};
type P = struct { a: usize } with {
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
"#,
        expect![[r#"
            151..152: duplicate impl of `D` for `P` (first implemented here at 65..66)
        "#]],
    );
}

#[test]
fn impl_missing_and_extra_members() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { extra = fn(x: usize) -> usize { 1 }; }
};
"#,
        expect![[r#"
            65..70: this impl of `D` is missing the member `m` (required by the trait here at 22..23)
            73..78: `D` has no requirement `extra` (declared here at 7..8)
        "#]],
    );
}

#[test]
fn impl_member_signature_mismatch() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> str { "s" }; }
};
"#,
        expect![[r#"
            73..74: member `m` does not match `D`'s requirement: expected `fn(usize) -> usize`, found `fn(usize) -> str` (required by the trait here at 22..23)
        "#]],
    );
}

#[test]
fn impl_member_binder_mismatch() {
    check_diagnostics(
        r#"
trait W = requires { p: fn(x: Self) -> usize; } with {
    impl usize { p = fn(x: usize) -> usize { 1 }; }
};
trait D = requires { m: fn::<X: W>(v: X, x: Self) -> usize; } with {
    impl usize { m = fn::<X>(v: X, x: usize) -> usize { 1 }; }
};
"#,
        expect![[r#"
            197..198: member `m`'s generic binder does not match `D`'s requirement (arity, kinds and bounds must agree) (required by the trait here at 132..133)
        "#]],
    );
}

#[test]
fn unknown_trait_in_bound_and_impl_head() {
    check_diagnostics(
        r#"
static f = fn::<T: Nope>(x: T) -> usize { 1 };
type P = struct { a: usize } with {
    impl Missing { m = fn(x: Self) -> usize { 1 }; }
};
"#,
        expect![[r#"
            20..24: unknown trait `Nope`
            93..100: unknown trait `Missing`
        "#]],
    );
}

#[test]
fn bound_naming_a_type_is_not_a_trait() {
    check_diagnostics(
        r#"
type P = struct { a: usize };
static f = fn::<T: P>(x: T) -> usize { 1 };
static g = fn::<T: usize>(x: T) -> usize { 1 };
"#,
        expect![[r#"
            50..51: `P` is not a trait
            94..99: `usize` is not a trait
        "#]],
    );
}

#[test]
fn trait_side_impl_head_resolution_errors() {
    check_diagnostics(
        r#"
trait D = requires { } with {
    impl Unknown { };
    impl D { };
};
type V = struct::<T> { a: T };
trait E = requires { } with {
    impl V { };
};
"#,
        expect![[r#"
            40..47: unknown type `Unknown`
            62..63: `D` is a trait; an impl in a trait's `with`-chain names the IMPLEMENTING type
            142..143: impls for generic types are not supported yet
        "#]],
    );
}

#[test]
fn unsatisfied_bound_at_instantiation() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let a = f::<P>(P(struct { a = 1 }));
    let _ = a;
};
"#,
        expect![[r#"
            167..173: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

#[test]
fn unsatisfied_bound_on_structural_type() {
    // Structural types implement nothing (TR03: impls attach to nominal
    // types only), so a record can never satisfy a bound.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let a = f(struct { q: usize = 1 });
    let _ = a;
};
"#,
        expect![[r#"
            137..138: the bound `T: D` is not satisfied here: `struct { q: usize }` does not implement `D`
        "#]],
    );
}

#[test]
fn bound_forwarding_requires_the_bound() {
    // `g` forwards its unbounded `T` into `f`'s bounded param: the bound
    // is unsatisfied AT g's call of f — bodies check against bounds.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { x.m() };
static g = fn::<T>(x: T) -> usize { f(x) };
"#,
        expect![[r#"
            195..196: the bound `T: D` is not satisfied here: `T` does not implement `D`
        "#]],
    );
}

#[test]
fn ambiguous_member_and_qualified_disambiguation() {
    check_diagnostics(
        r#"
trait A = requires { m: fn(x: Self) -> usize; } with { impl usize { m = fn(x: usize) -> usize { 1 }; } };
trait B = requires { m: fn(x: Self) -> usize; } with { impl usize { m = fn(x: usize) -> usize { 2 }; } };
static main = fn() -> () {
    let n: usize = 4;
    let x = n.m();
    let y = A::m(n);
    let _ = x + y;
};
"#,
        expect![[r#"
            274..279: `m` is ambiguous on `usize`: it could be `A`'s member (`A::m(value)`) or `B`'s member (`B::m(value)`) — spell the one you mean (`A::m` for `usize` is defined here at 69..70) (`B::m` for `usize` is defined here at 175..176)
        "#]],
    );
}

#[test]
fn trait_member_shadows_field_under_call_syntax() {
    // `s.m()` picks the trait member; bare `s.m` stays the field.
    check_infer(
        r#"
trait D = requires { m: fn(x: Self) -> str; };
type S = struct { m: usize } with {
    impl D { m = fn(x: Self) -> str { "member" }; }
};
static f = fn(s: S) -> str { let a = s.m; s.m() };
"#,
        expect![[r#"
            150..188 'fn(s: S) -> str {...': fn(S) -> str
            153..154 's': S
            166..188 '{ let a = s.m; s....': str
            172..173 'a': usize
            176..177 's': S
            176..179 's.m': usize
            181..182 's': S
            181..184 's.m': fn(S) -> str
            181..186 's.m()': str
        "#]],
    );
}

#[test]
fn trait_names_are_not_values_or_types() {
    check_diagnostics(
        r#"
trait D = requires { };
static x = fn() -> usize { let a = D; 1 };
static y = fn(p: D) -> usize { 1 };
"#,
        expect![[r#"
            60..61: `D` is a trait, not a value
            85..86: `D` is a trait; traits are bounds, not types — did you mean a bounded generic param (`T: D`)?
        "#]],
    );
}

#[test]
fn bare_trait_member_value_needs_the_implementer() {
    // A member VALUE is impl-specific, so a bare `D::m` names no one
    // function — the fix is the named-Self form. A trait's own POSITIONAL
    // arguments are reserved (generic traits): a non-generic trait takes none.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static a = fn() -> usize { let f = D::m; 1 };
static b = fn() -> usize { let n: usize = 1; D::<usize>::m(n) };
"#,
        expect![[r#"
            146..150: a member value is impl-specific, so it must name the implementer: `D::<Self = Type>::m`
            202..215: `D` takes no generic arguments
        "#]],
    );
}

#[test]
fn qualified_call_self_resolution_errors() {
    // A rigid Self without the bound is an unsatisfied bound; a Self no
    // argument determines cannot be inferred by the SHORT form (the
    // named-Self form spells it — see `named_self_call_pins_self`).
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; n: fn(k: usize) -> Self; } with {
    impl usize { m = fn(x: usize) -> usize { x }; n = fn(k: usize) -> usize { k }; }
};
static a = fn::<T>(x: T) -> usize { D::m(x) };
static b = fn() -> usize { let v = D::n(3); 1 };
"#,
        expect![[r#"
            205..212: the bound `T: D` is not satisfied here: `T` does not implement `D`
            251..258: cannot infer `Self` for `D::n`: no argument determines the implementing type — annotate an argument
        "#]],
    );
}

#[test]
fn bound_fn_value_reserved() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let g = f::<usize>;
    let _ = g(1);
};
"#,
        expect![[r#"
            198..208: `f` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn requirement_rules() {
    check_diagnostics(
        r#"
trait D = requires {
    m: fn(x: Self) -> usize;
    m: fn(x: Self) -> str;
    n: fn(x, y: usize) -> usize;
};
"#,
        expect![[r#"
            55..56: duplicate requirement `m`
            82..83: requirement `n` must spell its full signature: every parameter and the return type
        "#]],
    );
}

#[test]
fn no_member_in_bounds_on_rigid_receiver() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
static f = fn::<T: D>(x: T) -> usize { x.q() };
"#,
        expect![[r#"
            89..94: no field or member `q` on `T`
        "#]],
    );
}

#[test]
fn trait_member_call_types_flow() {
    // Bound-directed and impl-directed calls carry the requirement's
    // instantiated types (the generic requirement's W pins from the
    // argument).
    check_infer(
        r#"
trait W = requires { push: fn(s: str, w: Self) -> Self; };
trait D = requires { fmt: fn::<X: W>(w: X, v: Self) -> X; } with {
    impl usize { fmt = fn::<X: W>(w: X, v: usize) -> X { w.push("n") }; }
};
type S = struct { c: usize } with {
    impl W { push = fn(s: str, w: Self) -> Self { w }; }
};
static f = fn(n: usize) -> S { n.fmt(S(struct { c = 0 })) };
"#,
        expect![[r#"
            311..359 'fn(n: usize) -> S...': fn(usize) -> S
            314..315 'n': usize
            329..359 '{ n.fmt(S(struct ...': S
            331..332 'n': usize
            331..336 'n.fmt': fn(S, usize) -> S
            331..357 'n.fmt(S(struct { ...': S
            337..338 'S': fn(struct { c: usize }) -> S
            337..356 'S(struct { c = 0 })': S
            339..355 'struct { c = 0 }': struct { c: usize }
            352..353 '0': usize
        "#]],
    );
}

// ---- traits: nested-body reservation, variant Self, ambiguity errors --

#[test]
fn nested_fn_literal_cannot_use_enclosing_bounds() {
    // The dictionary lives in the ROOT body: a nested literal would have
    // to capture it — reserved (both the bound-directed call and the
    // forwarding call forms).
    check_diagnostics(
        r#"
trait Size = requires { size: fn(x: Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static bounded = fn::<T: Size>(x: T) -> usize { x.size() };
static outer = fn::<T: Size>(x: T) -> usize {
    let f = fn(y: T) -> usize { y.size() };
    let g = fn(y: T) -> usize { bounded(y) };
    f(x) + g(x)
};
"#,
        expect![[r#"
            258..266: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
            302..309: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}

#[test]
fn const_block_cannot_use_enclosing_bounds() {
    // A `const` block lowers to a separate body — the same
    // captured-dictionary wall as a nested literal (the const-check
    // rejection fires alongside).
    check_diagnostics(
        r#"
trait Size = requires { size: fn(x: Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static outer = fn::<T: Size>(x: T) -> usize { const { x.size() } };
"#,
        expect![[r#"
            174..180: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const` block is a const context at 166..171)
            174..182: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}

#[test]
fn qualified_variant_self_resolves_through_the_enum() {
    // A variant-typed argument at a `Self` position determines `Self` as
    // its ENUM (widened, so the tag is real) — the return type is the
    // enum too, so a single-variant match is honestly non-exhaustive.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point } with {
    impl Id { id = fn(x: Self) -> Self { Shape::Point }; }
};
trait Id = requires { id: fn(x: Self) -> Self; };
static main = fn() -> () {
    let c = Id::id(Shape::Circle(3));
    let r = match c { ::Circle(n) => n };
    let _ = r;
};
"#,
        expect![[r#"
            240..245: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn reserved_generic_trait_is_fully_non_live() {
    // The reservation is real everywhere: the bound, the impl and the
    // qualified use each carry their own diagnostic, and nothing
    // dispatches through the reserved trait.
    check_diagnostics(
        r#"
trait Gen = requires::<T> { get: fn(x: Self) -> usize; };
type X = struct { a: usize } with {
    impl Gen { get = fn(x: Self) -> usize { x.a }; }
};
static f = fn::<B: Gen>(x: B) -> usize { x.get() };
static g = fn(x: X) -> usize { Gen::get(x) };
"#,
        expect![[r#"
            21..26: generic traits are not supported yet
            104..107: `Gen` is a reserved generic trait (generic traits are not supported yet); it cannot be implemented
            170..173: `Gen` is a reserved generic trait (generic traits are not supported yet); it cannot be a bound
            192..199: no field or member `get` on `B`
            234..242: `Gen` is a reserved generic trait (generic traits are not supported yet) and cannot be used
        "#]],
    );
}

#[test]
fn trait_member_vs_fn_field_call_is_ambiguous() {
    // G13: a dot-callable member beside an fn-typed field is an
    // ambiguity ERROR under call syntax — silent shadowing would let a
    // distant impl reroute existing field calls. Both escapes work:
    // `(b.get)()` reaches the field, `Get::get(b)` the trait member. Bare
    // access stays the field; a NON-fn field keeps the sealed
    // member-shadows-field rule.
    check_diagnostics(
        r#"
trait Get = requires { get: fn(x: Self) -> usize; };
type B = struct { get: fn() -> usize } with {
    impl Get { get = fn(x: Self) -> usize { 100 }; }
};
type C = struct { get: usize } with {
    impl Get { get = fn(x: Self) -> usize { 7 }; }
};
static f = fn(b: B) -> usize { b.get() };
static escapes = fn(b: B, c: C) -> usize {
    let field = (b.get)();
    let member = Get::get(b);
    let bare = b.get;
    let nonfn = c.get();
    field + member + bare() + nonfn
};
"#,
        expect![[r#"
            279..286: `get` is ambiguous on `B`: it could be `Get`'s member (`Get::get(value)`) or the fn-typed field (`(value.get)(...)`) — spell the one you mean (`Get::get` for `B` is defined here at 115..118)
        "#]],
    );
}

#[test]
fn inherent_member_vs_fn_field_call_is_ambiguous() {
    check_diagnostics(
        r#"
type B = struct { get: fn() -> usize } with {
    impl Self { get = fn(x: Self) -> usize { 100 }; }
};
static f = fn(b: B) -> usize { b.get() };
"#,
        expect![[r#"
            135..142: `get` is ambiguous on `B`: it could be the inherent member (`B::get(value)`) or the fn-typed field (`(value.get)(...)`) — spell the one you mean (`B::get` is defined here at 63..66)
        "#]],
    );
}

// ---- G13: member collisions and their escapes ----

#[test]
fn inherent_member_vs_trait_member_call_is_ambiguous() {
    // G13: an inherent member no longer silently shadows a trait
    // member — the impl may be added in the TRAIT's chain, nowhere near
    // the type, so either winner would be action at a distance. Both
    // escapes are named, and both work.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { v: usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static collides = fn(p: P) -> usize { p.m() };
static escapes = fn(p: P) -> usize { P::m(p) + D::m(p) };
"#,
        expect![[r#"
            224..229: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`) or `D`'s member (`D::m(value)`) — spell the one you mean (`P::m` is defined here at 102..103) (`D::m` for `P` is defined here at 149..150)
        "#]],
    );
}

#[test]
fn inherent_trait_and_field_call_is_ambiguous() {
    // All three namespaces at once: the message lists one escape per
    // candidate, in resolution order (inherent, traits, field).
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { m: fn() -> usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static collides = fn(p: P) -> usize { p.m() };
static escapes = fn(p: P) -> usize { P::m(p) + D::m(p) + (p.m)() };
"#,
        expect![[r#"
            232..237: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`), `D`'s member (`D::m(value)`), or the fn-typed field (`(value.m)(...)`) — spell the one you mean (`P::m` is defined here at 110..111) (`D::m` for `P` is defined here at 157..158)
        "#]],
    );
}

#[test]
fn collision_leaves_bare_access_alone() {
    // Bare `p.m` is the FIELD, collision or not — only call syntax has
    // more than one candidate to choose between.
    check_infer(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { m: fn() -> usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static bare = fn(p: P) -> fn() -> usize { p.m };
"#,
        expect![[r#"
            208..241 'fn(p: P) -> fn() ...': fn(P) -> fn() -> usize
            211..212 'p': P
            234..241 '{ p.m }': fn() -> usize
            236..237 'p': P
            236..239 'p.m': fn() -> usize
        "#]],
    );
}

#[test]
fn bound_directed_collision_is_ambiguous() {
    // Two bounds providing the same name on a RIGID receiver: the same
    // ambiguity, with the escapes naming the param.
    check_diagnostics(
        r#"
trait A = requires { m: fn(x: Self) -> usize; };
trait B = requires { m: fn(x: Self) -> usize; };
static collides = fn::<T: A + B>(x: T) -> usize { x.m() };
static escapes = fn::<T: A + B>(x: T) -> usize { A::m(x) + B::<Self = T>::m(x) };
"#,
        expect![[r#"
            149..154: `m` is ambiguous on `T`: it could be `A`'s member (`A::m(value)`) or `B`'s member (`B::m(value)`) — spell the one you mean (required by the trait here at 22..23) (required by the trait here at 71..72)
        "#]],
    );
}

#[test]
fn qualified_inherent_member_is_a_value() {
    // An inherent member is an ordinary fn (its `Self` is just the last
    // parameter), so the qualified reference is its plain fn VALUE — no
    // dictionary is involved anywhere.
    check_infer(
        r#"
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static main = fn(p: P) -> usize { let f = P::len; f(p) };
"#,
        expect![[r#"
            108..150 'fn(p: P) -> usize...': fn(P) -> usize
            111..112 'p': P
            126..150 '{ let f = P::len;...': usize
            132..133 'f': fn(P) -> usize
            136..142 'P::len': fn(P) -> usize
            144..145 'f': fn(P) -> usize
            144..148 'f(p)': usize
            146..147 'p': P
        "#]],
    );
}

#[test]
fn qualified_path_naming_a_field_says_so_and_names_the_escape() {
    // The field/member namespace split puts fields on VALUES: `P::len` is
    // looking in the type's namespace, where only variants and members
    // live. The old answer ("`P` has no variants") was true and useless —
    // the name IS declared, one namespace over. Genuinely-unknown names
    // keep that answer, and an enum is untouched (it has no fields to
    // confuse anything with).
    check_diagnostics(
        r#"
type P = struct { len: usize } with {
    impl Self { size = fn(p: Self) -> usize { p.len }; }
};
type Shape = enum { Circle, Point };
static a = fn() -> () { let i = P::len; };
static b = fn() -> () { let j = P::nosuch; };
static c = fn(p: P) -> usize { P::size(p) };
static d = fn() -> () { let m = Shape::Nope; };
"#,
        expect![[r#"
            168..174: `len` is a field of `P`, not a member — fields are reached through a value: `value.len` (`P` is defined here at 6..7)
            211..220: `P` has no variants (it is a `struct` type) (`P` is defined here at 6..7)
            309..313: `Shape` has no variant `Nope` (`Shape` is defined here at 104..109)
        "#]],
    );
}

#[test]
fn qualified_member_path_reaches_inherent_members_only() {
    // Each spelling names exactly one thing: `Type::m` is the type's OWN
    // member, and a trait member is spelled through its trait.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { v: usize } with {
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static main = fn(p: P) -> usize { P::m(p) };
"#,
        expect![[r#"
            170..174: `m` is not a member of `P` itself: `D` provides it — write `D::m(value)` (a trait member is spelled through its trait)
        "#]],
    );
}

#[test]
fn self_qualified_member_inside_a_member_body() {
    // Expression-`Self` is RIGID (TR01), and the qualified
    // path reads it like any type name: `Self::one(p)` is the enclosing
    // type's own member at the body's own instantiation.
    check_diagnostics(
        r#"
type P = struct { v: usize } with {
    impl Self {
        one = fn(p: Self) -> usize { 1 };
        two = fn(p: Self) -> usize { Self::one(p) + 1 };
    }
};
static main = fn(p: P) -> usize { P::two(p) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn qualified_member_path_on_an_enum_keeps_variants_first() {
    // An enum's second segment is its VARIANT's home; a member of the same
    // name would be shadowed there, and any other name reaches the members.
    // The qualified path also ignores dot-callability — it passes `Self`
    // like any other argument, so a member the dot cannot reach is still
    // callable here.
    check_diagnostics(
        r#"
type Shape = enum { Circle, Point } with {
    impl Self {
        area = fn(s: Self) -> usize { 1 };
        scaled = fn(s: Self, by: usize) -> usize { by };
    }
};
static main = fn() -> usize {
    let c = Shape::Circle;
    Shape::area(c) + Shape::scaled(c, 2)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn qualified_member_on_a_generic_type_takes_the_type_args() {
    // An inherent member's binder IS the owner's, so the turbofish sits on
    // the TYPE — exactly the arguments a dot-call reads off the receiver.
    check_infer(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::<usize>::first(p)
};
"#,
        expect![[r#"
            116..213 'fn() -> usize {  ...': fn() -> usize
            130..213 '{     let p = Pai...': usize
            140..141 'p': Pair::<usize>
            144..157 'Pair::<usize>': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            144..182 'Pair::<usize>(str...': Pair::<usize>
            158..181 'struct { a = 1, b...': struct { a: usize, b: usize }
            171..172 '1': usize
            178..179 '2': usize
            188..208 'Pair::<usize>::first': fn(Pair::<usize>) -> usize
            188..211 'Pair::<usize>::fi...': usize
            209..210 'p': Pair::<usize>
        "#]],
    );
}

// ---- an fn literal takes its signature from its position ---------------
//
// An unannotated parameter or return type on a `fn` literal comes from the
// EXPECTATION at the position the literal sits in, before the body is
// checked. Recorded with its scope, because it has a sharp edge: a join
// leaf has no expectation to give, so a literal in witness position keeps
// its own fresh variables and mismatches as a whole.

#[test]
fn an_annotation_flows_its_signature_into_the_literal() {
    // The bug this rule fixes, in its smallest form and with nothing
    // generic in sight: the tail is a VARIANT, the sanctioned variant→enum
    // conversion happens at a CHECK, and the check is against the
    // literal's RETURN type — so the expected `Option::<usize>` has to be
    // in hand while the tail is checked. Before the rule this was
    // "expected `fn(usize) -> Option::<usize>`, found `fn(usize) ->
    // Option::<usize>::Some`", with the squiggle on the whole literal
    // rather than on anything the reader could act on.
    //
    // Both annotation positions, because they are different axioms: an
    // ITEM's own annotation, and a `let` binding's.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static item_annotation: fn(usize) -> Option::<usize> = fn(t) { Option::Some(t) };
static let_annotation = fn(n: usize) -> Option::<usize> {
    let f: fn(usize) -> Option::<usize> = fn(t) { Option::Some(t) };
    f(n)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_field_an_element_and_a_return_all_carry_the_expectation() {
    // The register in `types-and-data.md` claims to be exhaustive, so the
    // four positions that are neither an annotation nor a call argument
    // are pinned here — for BOTH rules that turn on it: the `::` sigil
    // (which needs an expected ENUM) and the fn-literal rule (which needs
    // an expected SIGNATURE).
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
type Box = struct { v: Option::<usize>, fs: [fn(usize) -> Option::<usize>; 1] };
static field_and_element = fn(n: usize) -> Box {
    Box(struct { v = ::Some(n), fs = [fn(t) { ::Some(t) }] })
};
static returned_sigil = fn(n: usize) -> Option::<usize> { return ::Some(n); };
static returned_literal = fn() -> fn(usize) -> Option::<usize> {
    return fn(t) { Option::Some(t) };
};
static assigned_rhs = fn(n: usize) -> Box {
    let mut b = Box(struct { v = ::None, fs = [fn(t) { ::Some(t) }] });
    b.v = ::Some(n);
    b.fs = [fn(t) { Option::Some(t) }];
    b
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn the_registers_other_enum_capable_positions_carry_it_too() {
    // The rest of the register's rows that can hold an ENUM, pinned for
    // the `::` sigil the same way: the right operand of `==`/`!=` takes
    // the left operand's type, and the three pass-through positions — a
    // block's tail, a `const` block's body and an `unsafe` block's body —
    // hand the enclosing expectation straight down. (A `Newtype`
    // constructor's argument is an expectation position too, but its
    // underlying type is always a `struct`/`enum` literal, so no sigil
    // can land there.)
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static eq_operand = fn(o: Option::<usize>, n: usize) -> bool { o == ::Some(n) };
static block_tail = fn(n: usize) -> Option::<usize> { { ::Some(n) } };
static const_block = fn() -> Option::<usize> { const { ::Some(1) } };
static unsafe_block = fn(n: usize) -> Option::<usize> { unsafe { ::Some(n) } };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_argument_position_flows_its_signature_into_the_literal() {
    // The same rule at the position that matters most in practice — a
    // literal handed to a higher-order function — in isolation from
    // members and from `let`.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static apply = fn::<T, U>(g: fn(T) -> U, t: T) -> U { g(t) };
static main = fn(n: usize) -> Option::<usize> {
    apply::<usize, Option::<usize>>(fn(t) { Option::Some(t) }, n)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_expected_signature_names_the_literals_parameters() {
    // The PARAMETER half of the same rule: an unannotated parameter takes
    // its type from the position too, so the body may use it for real
    // (`t.n` needs `t` to be a `Counter`, and nothing in the literal says
    // so). The literal's own annotation still wins where there is one.
    check_infer(
        r#"
type Counter = struct { n: usize };
static apply = fn(g: fn(Counter) -> usize, c: Counter) -> usize { g(c) };
static main = fn(c: Counter) -> usize { apply(fn(t) { t.n }, c) };
"#,
        expect![[r#"
            52..109 'fn(g: fn(Counter)...': fn(fn(Counter) -> usize, Counter) -> usize
            55..56 'g': fn(Counter) -> usize
            80..81 'c': Counter
            101..109 '{ g(c) }': usize
            103..104 'g': fn(Counter) -> usize
            103..107 'g(c)': usize
            105..106 'c': Counter
            125..176 'fn(c: Counter) ->...': fn(Counter) -> usize
            128..129 'c': Counter
            149..176 '{ apply(fn(t) { t...': usize
            151..156 'apply': fn(fn(Counter) -> usize, Counter) -> usize
            151..174 'apply(fn(t) { t.n...': usize
            157..170 'fn(t) { t.n }': fn(Counter) -> usize
            160..161 't': Counter
            163..170 '{ t.n }': usize
            165..166 't': Counter
            165..168 't.n': usize
            172..173 'c': Counter
        "#]],
    );
}

#[test]
fn an_expected_signature_reaches_a_destructuring_parameter() {
    // The other parameter-lowering path: a pattern parameter with no
    // annotation of its own and no `Newtype` head to name its type. It
    // asks the position, and the destructure then checks against what came
    // back.
    check_diagnostics(
        r#"
static apply = fn(
    g: fn(struct { x: usize, y: usize }) -> usize,
    p: struct { x: usize, y: usize },
) -> usize { g(p) };
static main = fn(p: struct { x: usize, y: usize }) -> usize {
    apply(fn(struct { x, y }) { x + y }, p)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_inherited_return_type_is_an_axiom_position_for_the_sigil() {
    // A consequence worth pinning rather than discovering: the `::`
    // variant sigil resolves wherever an expectation reaches, so giving a
    // literal's body one opened a NEW axiom position for it. Recorded in
    // `types-and-data.md`'s expectation register.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static f: fn(usize) -> Option::<usize> = fn(t) { ::Some(t) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_expectation_of_the_wrong_arity_is_not_inherited() {
    // The guard: the expectation is only a signature for this literal when
    // it AGREES on parameter count. A literal that takes two parameters at
    // a one-parameter position must keep its own fresh variables and
    // mismatch as a whole — inheriting positionally from a list of the
    // wrong length would put the wrong type on the wrong name and blame
    // the body for it.
    check_diagnostics(
        r#"
static apply = fn(g: fn(usize) -> usize, n: usize) -> usize { g(n) };
static main = fn(n: usize) -> usize { apply(fn(a, b) { n }, n) };
"#,
        expect![[r#"
            115..129: type mismatch: expected `fn(usize) -> usize`, found `fn(_, _) -> usize`
        "#]],
    );
}

#[test]
fn an_inherited_return_type_points_at_the_position_it_came_from() {
    // A tail mismatch inside the literal names the POSITION as its
    // reason: the return type was not written on the literal, so the slot
    // that supplied it is the only thing there is to point at — and the
    // reader did write that. (The second complaint is the join-leaf limit
    // below, not this rule.)
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static f: fn(bool) -> Option::<usize> = fn(b) { if b { 1 } else { ::None } };
"#,
        expect![[r#"
            92..118: every branch produces `{number}`, but `Option::<usize>` is needed (expected `Option::<usize>` because of this annotation at 54..81)
            110..116: cannot resolve `::None` without an expected type — write `Enum::None`
        "#]],
    );
}

#[test]
fn a_literal_that_mismatches_its_slot_does_not_also_blame_its_tail() {
    // A signature that disagrees is the whole story. The unwritten return
    // is still a bare variable when the mismatch is reported, so `check`'s
    // unresolved-number poisoning cannot reach the number the BODY mints
    // afterwards; it runs again once the body is in. Without that, a
    // literal that already mismatched as a whole also collects a
    // "no defining use" complaint about a tail nothing was going to keep.
    check_diagnostics(
        r#"
static apply = fn(g: fn(usize) -> usize, n: usize) -> usize { g(n) };
static arity = fn(n: usize) -> usize { apply(fn(a, b) { 1 }, n) };
static param: fn(usize) -> usize = fn(t: str) { 1 };
"#,
        expect![[r#"
            116..130: type mismatch: expected `fn(usize) -> usize`, found `fn(_, _) -> {error}`
            173..189: type mismatch: expected `fn(usize) -> usize`, found `fn(str) -> {error}` (expected `fn(usize) -> usize` because of this annotation at 152..170)
        "#]],
    );
}

#[test]
fn an_unknown_type_in_the_slot_is_inherited_silently() {
    // Why no `Ty::Error` filtering is written at the literal: `unify`
    // binds `{error}` to the literal's parameter like any other type and
    // errors are infectious and silent, so the body's use of it is
    // accepted and the unknown type is reported once, where it is written.
    check_diagnostics(
        r#"
static apply = fn(g: fn(Bogus) -> usize, n: usize) -> usize { n };
static main = fn(n: usize) -> usize { apply(fn(t) { t.n }, n) };
"#,
        expect![[r#"
            25..30: unknown type `Bogus`
        "#]],
    );
}

#[test]
fn the_expectation_does_not_reach_a_join_leaf() {
    // THE SCOPE LIMIT, pinned so the claim stays honest: a literal in
    // WITNESS position (an `if`/`match` branch whose value is the join's)
    // gets NO expectation — a join leaf has none to give, the register's
    // "none" row and the "Expectations reach join leaves" entry in
    // `types-and-data.md`, hit here by a program with no `match` in it.
    // The literal types as a tag-free variant return and mismatches,
    // exactly as every literal did before this rule.
    //
    // The fix is that entry's, not this rule's: it is about where an
    // expectation reaches, not about what a literal does with one.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static pick = fn(b: bool) -> fn(usize) -> Option::<usize> {
    if b { fn(t) { Option::Some(t) } } else { fn(t) { Option::Some(t) } }
};
"#,
        expect![[r#"
            115..140: type mismatch: expected `fn(usize) -> Option::<usize>`, found `fn(usize) -> Option::<usize>::Some` (expected `fn(usize) -> Option::<usize>` because of this return type at 70..101)
            150..175: type mismatch: expected `fn(usize) -> Option::<usize>`, found `fn(usize) -> Option::<usize>::Some` (expected `fn(usize) -> Option::<usize>` because of this return type at 70..101)
        "#]],
    );
}

// ---- second-segment generic arguments -----------------------------------
//
// A turbofish on a path's SECOND segment parses into a real node of its
// own (`syntax`'s `MEMBER_GENERIC_ARGS`) and is judged HERE, by what the
// segment names: a MEMBER's own binder SPENDS it, a VARIANT's arguments
// are the owner's and misplaced. The split cannot live in the parser — it
// does not know which is which.

#[test]
fn member_own_generic_arguments_are_matched_against_the_members_own_binder() {
    // A member with no binder of its own takes no arguments — the same
    // arity report a non-generic item's turbofish gets, naming the member.
    // (The list is not the OWNER's and never falls back to it; that claim
    // is `member_own_generic_arguments_do_not_instantiate_the_owner`.)
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
static main = fn() -> usize {
    let f = Measured::size::<usize>;
    0
};
"#,
        expect![[r#"
            144..167: `Measured::size` takes no generic arguments
        "#]],
    );
}

#[test]
fn member_turbofish_on_an_unknown_base_is_still_one_diagnostic() {
    // The cascade this whole area exists to prevent, checked end to end:
    // `P::len::<usize>` on an undeclared `P` reports the unresolved NAME
    // and nothing else — no parse errors (the list is real syntax now), no
    // second complaint about the argument, and the statements around it
    // are untouched.
    check_diagnostics(
        r#"
static main = fn() -> usize {
    let g = P::len::<usize>;
    let n = 1;
    n
};
"#,
        expect![[r#"
            43..44: unresolved name `P`
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_do_not_instantiate_the_owner() {
    // The failure mode the ERROR node used to prevent structurally, now
    // prevented by the tree itself: were `::<usize>` read as the OWNER's
    // list, this program would be exactly `Pair::<usize>::first(p)` and
    // would check CLEAN. It must not — `first` declares no binder of its
    // own, so its own list is empty and one argument is one too many. The
    // OWNER's arguments still come from inference (from `p`), which is
    // what the type side below pins.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::first::<usize>(p)
};
"#,
        expect![[r#"
            188..208: `Pair::first` takes no generic arguments; if these are meant for `Pair`, write `Pair::<...>::first`
        "#]],
    );
    // And the type side of the same claim: the OWNER's argument came from
    // `p`, never from the written list.
    check_infer(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
static main = fn(p: Pair::<usize>) -> usize { Pair::first::<usize>(p) };
"#,
        expect![[r#"
            116..173 'fn(p: Pair::<usiz...': fn(Pair::<usize>) -> usize
            119..120 'p': Pair::<usize>
            146..173 '{ Pair::first::<u...': usize
            148..168 'Pair::first::<usize>': fn(Pair::<usize>) -> usize
            148..171 'Pair::first::<usi...': usize
            169..170 'p': Pair::<usize>
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_no_hint_when_the_owner_list_is_already_written() {
    // `Pair::<usize>::first::<usize>` already writes the owner's list — the
    // `{owner}::<...>::{member}` hint has nothing left to suggest, so it
    // must not repeat the user's own spelling back at them.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::<usize>::first::<usize>(p)
};
"#,
        expect![[r#"
            188..217: `Pair::first` takes no generic arguments
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_belong_to_the_owner() {
    // A variant is a case of its enum and never gets a binder of its own,
    // so this one is a CORRECTION (move them), not a reservation — the
    // same tree, a different message, decided by what the segment names.
    check_diagnostics(
        r#"
type Shape = enum::<T> { Circle(T), Point };
static main = fn() -> usize {
    let c = Shape::Circle::<usize>;
    0
};
"#,
        expect![[r#"
            88..110: a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_no_hint_on_a_non_generic_owner() {
    // `Shape` has no binder at all, so `Shape::<...>::Circle` is not a fix —
    // it is a second error ("Shape takes no generic arguments"). The hint
    // must not steer the reader into it.
    check_diagnostics(
        r#"
type Shape = enum { Circle, Point };
static main = fn() -> usize {
    let c = Shape::Circle::<usize>;
    0
};
"#,
        expect![[r#"
            80..102: a variant has no generic arguments of its own: arguments written on `Shape::Circle` cannot be applied here
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_no_hint_when_the_owner_list_is_already_written() {
    // `Option::<usize>::Some::<bool>` already writes the owner's list — the
    // hint has nothing left to suggest, so it must not echo the user's own
    // spelling back.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static main = fn() -> usize {
    let o = Option::<usize>::Some::<bool>;
    0
};
"#,
        expect![[r#"
            86..115: a variant has no generic arguments of its own: arguments written on `Option::Some` cannot be applied here
        "#]],
    );
}

#[test]
fn trait_member_own_type_generic_arguments_are_spendable_in_both_forms() {
    // A trait requirement has always been allowed its own binder; what
    // changed is that the use site may now SPELL its type arguments — in
    // the value form and in the TR01 short/named call forms alike, through
    // the one `matched_member_args` every member path shares.
    check_diagnostics(
        r#"
trait D = requires { n: fn::<T>(t: T, s: Self) -> usize; } with {
    impl usize { n = fn::<T>(t: T, s: usize) -> usize { s }; }
};
static value = fn() -> usize {
    let f = D::<Self = usize>::n::<usize>;
    0
};
static called = fn(s: usize) -> usize { D::n::<usize>(1, s) };
"#,
        expect![[r#""#]],
    );
    // And the binder-less report on a requirement with none of its own —
    // the same sentence an inherent member gets, named for the trait whose
    // requirement it is, and with NO `{owner}::<...>::{member}` hint: that
    // spelling collides with the separately reserved generic-trait form.
    check_diagnostics(
        r#"
trait D = requires { n: fn(s: Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static value = fn() -> usize {
    let f = D::<Self = usize>::n::<usize>;
    0
};
static called = fn(s: usize) -> usize { D::n::<usize>(s) };
"#,
        expect![[r#"
            154..183: `D::n` takes no generic arguments
            234..250: `D::n` takes no generic arguments
        "#]],
    );
}

#[test]
fn a_member_reached_through_a_borrow_is_named_by_its_impl_head() {
    // Every message about a member's own arguments names a spelling a
    // program could CONTAIN. A trait-impl member's item name is already the
    // written `head::member` form, so it is passed through — naming it from
    // the RECEIVER's rendered type instead would report `usize.&::peek` for
    // the ordinary `Self.&` member, which is not a path.
    check_diagnostics(
        r#"
trait Peek = requires {
    none: fn::<@a>(s: Self.&::<@a>) -> usize;
    one: fn::<@a, U>(u: U, s: Self.&mut::<@a>) -> usize;
    counted: fn::<@a, const N: usize>(s: Self.&::<@a>) -> usize;
} with {
    impl usize {
        none = fn::<@a>(s: usize.&::<@a>) -> usize { 1 };
        one = fn::<@a, U>(u: U, s: usize.&mut::<@a>) -> usize { 1 };
        counted = fn::<@a, const N: usize>(s: usize.&::<@a>) -> usize { N };
    }
};
static f = fn::<@a>(n: usize.&::<@a>, m: usize.&mut::<@a>) -> usize {
    n.none::<bool>() + m.one::<bool, str>(true) + n.counted::<3>()
};
"#,
        expect![[r#"
            506..522: `usize::none` takes no generic arguments
            525..549: `usize::one` takes 1 generic argument, found 2
            552..568: `usize::counted` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_on_a_trait_declaring_its_own_binder() {
    // The requirement's own BOUND rides its binder to the use site: a
    // spelled argument is checked against it, exactly as a free generic
    // fn's turbofish argument is.
    check_diagnostics(
        r#"
trait Write = requires { push: fn(s: str, w: Self) -> Self; } with {
    impl usize { push = fn(s: str, w: usize) -> usize { w }; }
};
trait Display = requires { fmt: fn::<W: Write>(w: W, x: Self) -> W; } with {
    impl bool { fmt = fn::<W: Write>(w: W, x: bool) -> W { w }; }
};
static ok = fn(w: usize, x: bool) -> usize { Display::fmt::<usize>(w, x) };
static bad = fn(w: str, x: bool) -> str { Display::fmt::<str>(w, x) };
"#,
        expect![[r#"
            400..425: the bound `W: Write` is not satisfied here: `str` does not implement `Write`
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_infer_the_owners_const_args_exactly_once() {
    // Regression: the owner's turbofish const args used to be inferred
    // TWICE on the trait path — `trait_path_self_arg` types every const in
    // `vp_args` regardless of whether it finds `Self`, so nothing under
    // `infer_qualified_trait_call` may re-infer them. One type-mismatch,
    // not two, in both the value and called forms.
    check_diagnostics(
        r#"
trait D = requires { n: fn(k: usize) -> Self; } with {
    impl usize { n = fn(k: usize) -> usize { k }; }
};
static value = fn() -> usize {
    let f = D::<const { 1 + true }>::n::<usize>;
    0
};
static called = fn() -> usize { D::<const { 1 + true }>::n::<usize>(3) };
"#,
        expect![[r#"
            154..189: `D` takes no generic arguments
            154..189: a member value is impl-specific, so it must name the implementer: `D::<Self = Type>::n`
            170..174: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 168..169)
            232..267: `D` takes no generic arguments
            232..270: `D::n` takes no generic arguments
            232..270: cannot infer `Self` for `D::n`: no argument determines the implementing type — annotate an argument
            248..252: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 246..247)
        "#]],
    );
}

#[test]
fn the_member_generic_program_the_future_made_legal() {
    // The program the reservation was written against, now checking
    // clean — the whole grant, in one example: the DECLARATION's binder is
    // live (`syntax::validation` no longer refuses the type half) and the
    // USE site's arguments are spent on it.
    //
    // Neither half ever needed a grammar change: both were always
    // diagnostics over a REAL parse, and deleting them was the grant.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self {
        size = fn::<T>(m: Self, t: T) -> usize { m.n };
    }
};
static main = fn() -> usize {
    let f = Measured::size::<usize>;
    0
};
"#,
        expect![[r#""#]],
    );
}

// ---- member-own TYPE binders --------------------------------------------
//
// The acid test for the whole arc, and the reason it was asked for: the
// five `Option` members — two of which (`flat_map`, `map`) cannot be
// written at all without a member-own type binder — over a forgettable
// payload and a linear one. The two programs differ by exactly one
// type-level clause.

/// The five members, shared so the payload cases below cannot drift apart.
const OPTION_MEMBERS: &str = r#"    impl Self {
        is_some = const fn::<@local>(s: Self.&::<@local>) -> bool {
            match s {
                ::Some(_) => true,
                ::None => false,
            }
        }
        unwrap = const fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
        flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self) -> Option::<U> {
            match s {
                ::Some(v) => { f(v) }
                ::None => { Option::None }
            }
        }
        map = fn::<U>(f: fn(T) -> U, s: Self) -> Option::<U> {
            s.flat_map(fn(t) { Option::Some(f(t)) })
        }
        as_ref = fn::<@a>(s: Self.&::<@a>) -> Option::<T.&::<@a>> {
            match s {
                ::Some(t) => Option::Some(t),
                ::None => Option::None,
            }
        }
    }"#;

#[test]
fn the_five_option_members_check_clean_over_a_forgettable_payload() {
    check_diagnostics(
        &format!(
            "\ntype Option = enum::<T> {{\n    Some(T),\n    None,\n}} with {{\n{OPTION_MEMBERS}\n}}\n"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_five_option_members_check_clean_over_a_linear_payload() {
    // The SAME five bodies with `without forget` on the binder — the one
    // type-level clause that says the payload may be linear. Nothing in
    // `flat_map` or `map` moves a `T` twice or drops one, so nothing here
    // should fire; a spurious linear diagnostic would mean the member's
    // own `U` had quietly leaked into the payload's obligations.
    check_diagnostics(
        &format!(
            "\ntype Option = enum::<T without forget> {{\n    Some(T),\n    None,\n}} with {{\n{OPTION_MEMBERS}\n}}\n"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_five_option_members_check_clean_over_option_of_string() {
    // The acid test's point, exercised on the real linear type: `String`
    // is `without forget`, so `Option::<String>` is the instantiation the
    // type-level clause exists for. All five members, and `flat_map`/`map`
    // over a payload that must be consumed.
    check_string(
        &format!(
            r#"
type Option = enum::<T without forget> {{
    Some(T),
    None,
}} with {{
{OPTION_MEMBERS}
}}
static length = fn(s: String) -> usize {{ let n = s.&.len(); s.drop(); n }};
static wrap_length = fn(s: String) -> Option::<usize> {{ Option::Some(length(s)) }};
static via_map = fn(text: str) -> usize {{
    let o: Option::<String> = Option::Some(to_owned(text.&));
    o.map(length).unwrap()
}};
static via_flat_map = fn(text: str) -> usize {{
    let o: Option::<String> = Option::Some(to_owned(text.&));
    o.flat_map::<usize>(wrap_length).unwrap()
}};
"#
        ),
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_type_param_is_inferred_from_the_call() {
    // No turbofish: `U` is an ordinary inference variable at the call site,
    // pinned by the argument exactly as a free generic fn's would be.
    check_infer(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static main = fn(o: Option::<usize>) -> bool { o.map_to(width) };
"#,
        expect![[r#"
            223..253 'fn(n: usize) -> b...': fn(usize) -> bool
            226..227 'n': usize
            244..253 '{ n > 2 }': bool
            246..247 'n': usize
            246..251 'n > 2': bool
            250..251 '2': usize
            269..319 'fn(o: Option::<us...': fn(Option::<usize>) -> bool
            272..273 'o': Option::<usize>
            300..319 '{ o.map_to(width) }': bool
            302..303 'o': Option::<usize>
            302..310 'o.map_to': fn(fn(usize) -> bool, Option::<usize>) -> bool
            302..317 'o.map_to(width)': bool
            311..316 'width': fn(usize) -> bool
        "#]],
    );
}

#[test]
fn a_member_own_type_param_is_spellable_at_both_call_spellings() {
    // The two use-site spellings of the same instantiation: the dot-call's
    // own turbofish (a NEW grammar position — see `syntax`'s
    // `member_turbofish_on_a_dot_call`) and the qualified path's, which
    // has parsed into `MEMBER_GENERIC_ARGS` since the reservation.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static dotted = fn(o: Option::<usize>) -> bool { o.map_to::<bool>(width) };
static pathed = fn(o: Option::<usize>) -> bool { Option::map_to::<bool>(width, o) };
static owner_and_member = fn(o: Option::<usize>) -> bool {
    Option::<usize>::map_to::<bool>(width, o)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_written_member_type_argument_is_checked_against_the_call() {
    // The turbofish PINS the parameter, so a disagreeing argument is an
    // ordinary mismatch at the argument — the same thing a free generic
    // fn's turbofish does.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static wrong = fn(o: Option::<usize>) -> usize { o.map_to::<usize>(width) };
"#,
        expect![[r#"
            322..327: type mismatch: expected `fn(usize) -> usize`, found `fn(usize) -> bool`
        "#]],
    );
}

#[test]
fn a_member_own_type_param_is_rigid_inside_the_body() {
    // Rigid-parameter checking (TR07), unchanged by whose binder the param
    // came from: the body is checked ONCE against a rigid `U`, so an
    // operation `U` does not support is refused at the DEFINITION, not at
    // some instantiation.
    check_diagnostics(
        r#"
type Wrap = struct { n: usize } with {
    impl Self {
        bad = fn::<U>(u: U, w: Self) -> usize { u + w.n };
    }
};
"#,
        expect![[r#"
            104..105: type mismatch: expected `{number}`, found `U` (`+` requires `{number}` operands at 106..107)
        "#]],
    );
}

#[test]
fn the_forget_default_bound_applies_to_a_member_own_type_param() {
    // A member's `U` is a type parameter like any other: it requires
    // `forget` unless it says otherwise, and the check happens at the
    // instantiation edge the member call is.
    check_diagnostics(
        r#"
type Wrap = struct { n: usize } with {
    impl Self {
        take = fn::<U>(u: U, w: Self) -> usize { w.n };
        take_linear = fn::<U without forget>(u: U, w: Self) -> U { u };
    }
};
type Lin = struct { n: usize } without forget with {
    impl Self { sink = fn(l: Self) -> usize { let Lin(struct { n }) = l; n }; }
};
static refused = fn(w: Wrap, l: Lin) -> usize { w.take(l) };
static allowed = fn(w: Wrap, l: Lin) -> usize { w.take_linear(l).sink() };
"#,
        expect![[r#"
            377..386: `Lin` cannot be a `U`: `Lin` is declared `without forget`, and `U` requires `forget` (every type parameter does unless it is written `U without forget`)
        "#]],
    );
}

#[test]
fn a_member_own_type_binder_composes_with_its_region_binder() {
    // Both halves of the member's own binder at once, in the order the
    // owner's binder is extended: regions stay always-inferred (no
    // spelling at the call), the type is the one the turbofish spells.
    check_diagnostics(
        r#"
type Cell = struct::<T> { v: T } with {
    impl Self {
        pick = fn::<@b, U>(alt: U, c: Self.&::<@b>) -> U { alt };
    }
};
static f = fn::<@a>(c: Cell::<usize>.&::<@a>) -> bool { c.pick::<bool>(true) };
static g = fn::<@a>(c: Cell::<usize>.&::<@a>) -> bool { c.pick(true) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_type_param_shadows_a_same_named_owner_param() {
    // Two `T`s in one binder — the owner's and the member's. The member's
    // is written LAST and wins inside the member, which is the rule
    // `scopes` already applies to a binder's const params (and the rule
    // local shadowing follows). `Self` is unaffected: it is the owner at
    // the OWNER's binder, whatever the member renamed on top of it.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        m = fn::<T>(t: T, p: Self) -> T { t };
    }
};
static main = fn(p: P::<usize>) -> bool { p.m::<bool>(true) };
static plain = fn(p: P::<usize>) -> usize { p.a };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn every_refused_member_turbofish_still_types_its_const_arguments() {
    // THE INVARIANT: every expression in a body gets a type, and a const
    // argument inside a member turbofish is an ordinary expression. It is
    // easy to lose on the exits that refuse the PATH for some other
    // reason, because those return before anything looks at the list — so
    // the freeing is structural (one wrapper per entry point, the spend
    // sites TAKE), and this pins one probe per family of exit.
    //
    // `const { 1 + true }` is the probe: if the argument were never
    // inferred, its own mismatch would go missing.
    check_diagnostics(
        r#"
type Shape = enum::<T> { Circle(T), Point } with {
    impl Self { area = fn(s: Self) -> usize { 1 }; }
};
trait D = requires { n: fn(s: Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static nope = fn() -> usize { Shape::missing::<const { 1 + true }>(1) };
static variant = fn() -> usize { let c = Shape::Circle::<const { 1 + true }>; 0 };
static wrong_ns = fn() -> usize { let f = D::gone::<const { 1 + true }>; 0 };
static no_impl = fn() -> usize { let f = D::<Self = bool>::n::<const { 1 + true }>; 0 };
static on_a_value = fn(n: usize) -> usize { n.oops::<const { 1 + true }>(); 0 };
"#,
        expect![[r#"
            255..262: `Shape` has no variant `missing` (`Shape` is defined here at 6..11)
            277..281: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 275..276)
            332..367: a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`
            360..364: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 358..359)
            416..445: `D` has no requirement `gone`
            438..442: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 436..437)
            493..534: `bool` does not implement `D`
            527..531: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 525..526)
            585..615: no field or member `oops` on `usize`
            606..610: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 604..605)
        "#]],
    );
}

#[test]
fn a_refused_member_turbofish_says_nothing_it_cannot_back_up() {
    // The other half of the invariant: the const arguments are consumed
    // SILENTLY on every path that is already refusing the call. "`map_to`
    // takes no generic arguments" is a lie about a member that HAS a
    // binder, and piling it on an unrelated refusal is the noise the
    // infectious-and-silent rule exists to prevent. Not one of the
    // programs below may produce it.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U { panic("x") }; }
};
type Holder = struct { go: fn(usize) -> usize };
static half_typed = fn(o: Option::<usize>) -> usize { o.map_to::<>; 0 };
static wrong_name = fn(o: Option::<usize>) -> usize { o.map_two::<usize>(1) };
static on_broken = fn(o: Nope) -> usize { o.map_to::<usize>(1) };
// A BARE INTEGER is the only const-arg form that can produce the
// no-defining-use diagnostic, so it is the only probe that can catch the
// noise coming back: the list is dropped, and a number nobody kept must
// not be told to annotate itself. (`const { ... }` cannot show this — a
// block's contents are inferred whatever happens to the list.)
static dropped_number = fn(h: Holder) -> usize { h.go::<3>(1) };
static unknown_number = fn(h: Holder) -> usize { h.nope::<3>(1) };
"#,
        expect![[r#"
            236..248: `map_to` is a member fn, not a field; call it: `.map_to(...)`
            309..330: no field or member `map_two` on `Option::<usize>`
            359..363: unknown type `Nope`
            801..810: `go` takes no generic arguments
            866..880: no field or member `nope` on `Holder`
        "#]],
    );
}

#[test]
fn a_binderless_member_refuses_even_an_empty_turbofish() {
    // `::<>` is a WRITTEN list: accepting it silently would make the
    // spelling mean two things. One sentence for every binder-less
    // turbofish, the same one a non-generic item's gets.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
trait D = requires { n: fn(s: Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static empty_inherent = fn(m: Measured) -> usize { m.size::<>() };
static empty_impl = fn(s: usize) -> usize { s.n::<>() };
static empty_path = fn(m: Measured) -> usize { Measured::size::<>(m) };
"#,
        expect![[r#"
            263..275: `Measured::size` takes no generic arguments
            323..332: `usize::n` takes no generic arguments
            383..401: `Measured::size` takes no generic arguments
        "#]],
    );
}

#[test]
fn a_member_turbofish_has_no_nameable_argument() {
    // TR01 gives v1 exactly ONE nameable argument, a trait's `Self`, and it
    // is written on the OWNER's list. A member's own arguments are
    // positional; a name here is refused rather than swallowed.
    //
    // WHICH refusal is the owner's to decide, and both sentences below are
    // true of their own program. An INHERENT owner (the first two) has no
    // `Self` argument at all, so it gets the same not-a-trait sentence any
    // other non-trait list gets. A TRAIT's member (the last three) does
    // have one, one segment to the left, so denying the trait-ness would
    // be false about what the reader can see — there the wrong thing is
    // the POSITION, and the message names the spelling that has it.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U { panic("x") }; }
};
trait Pk = requires { pick: fn::<U>(alt: U, s: Self) -> U; } with {
    impl usize { pick = fn::<U>(alt: U, s: usize) -> U { alt }; }
};
static width = fn(n: usize) -> bool { n > 2 };
static dotted = fn(o: Option::<usize>) -> bool { o.map_to::<Self = bool>(width) };
static pathed = fn(o: Option::<usize>) -> bool { Option::map_to::<Self = bool>(width, o) };
static impl_dot = fn(n: usize) -> bool { n.pick::<Self = bool>(true) };
static trait_path = fn(n: usize) -> bool { Pk::pick::<Self = bool>(true, n) };
static bound = fn::<T: Pk>(x: T) -> bool { x.pick::<Self = bool>(true) };
"#,
        expect![[r#"
            366..396: only a trait has a `Self` argument to name
            449..478: only a trait has a `Self` argument to name
            533..560: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
            607..639: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
            686..713: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
        "#]],
    );
}

#[test]
fn cannot_infer_a_member_own_param_names_the_spelling_that_pins_it() {
    // The one prompt that exists to send a reader to the member
    // turbofish, so it has to name a turbofish the reader can WRITE — and
    // that is the SITE's answer, not the member's. At a dot-call the
    // member stands alone (`.fresh::<...>(...)`); at a path the qualified
    // name carries the list (`Option::fresh::<...>`, `Mk::mk::<...>`); and
    // a trait member VALUE is impl-specific, so there the bare path is
    // itself refused and the implementer naming is part of the spelling
    // (`Mk::<Self = usize>::mk::<...>`). Blame follows the site too — a
    // requirement's own `U` is `Mk::mk`'s, not `Mk`'s. Every spelling
    // below is checked clean when written out.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { fresh = fn::<U>(s: Self) -> U { panic("x") }; }
};
trait Mk = requires { mk: fn::<U>(s: Self) -> U; } with {
    impl usize { mk = fn::<U>(s: usize) -> U { panic("x") }; }
};
static a = fn(o: Option::<usize>) -> usize { o.fresh(); 1 };
static b = fn(o: Option::<usize>) -> usize { Option::fresh(o); 1 };
static c = fn(n: usize) -> usize { n.mk(); 1 };
static d = fn(n: usize) -> usize { Mk::mk(n); 1 };
static e = fn::<T: Mk>(x: T) -> usize { x.mk(); 1 };
static f = fn(n: usize) -> usize { let g = Mk::<Self = usize>::mk; g(n); 1 };
"#,
        expect![[r#"
            286..295: cannot infer the type parameter `U` of `Option::fresh`; write `.fresh::<...>(...)` to specify it
            347..360: cannot infer the type parameter `U` of `Option::fresh`; write `Option::fresh::<...>` to specify it
            405..411: cannot infer the type parameter `U` of `usize::mk`; write `.mk::<...>(...)` to specify it
            453..462: cannot infer the type parameter `U` of `Mk::mk`; write `Mk::mk::<...>` to specify it (defined here at 123..125)
            509..515: cannot infer the type parameter `U` of `Mk::mk`; write `.mk::<...>(...)` to specify it (defined here at 123..125)
            565..587: cannot infer the type parameter `U` of `Mk::mk`; write `Mk::<Self = usize>::mk::<...>` to specify it
        "#]],
    );
}

#[test]
fn a_bound_directed_dot_call_spends_a_member_turbofish() {
    // The THIRD spend site, and the one no test reached: a rigid receiver
    // resolves through the enclosing dictionary, and the REQUIREMENT's own
    // binder is instantiated there — so its type arguments are spellable
    // exactly as a concrete receiver's are.
    check_diagnostics(
        r#"
trait D = requires { pick: fn::<U>(alt: U, s: Self) -> U; } with {
    impl usize { pick = fn::<U>(alt: U, s: usize) -> U { alt }; }
};
static generic = fn::<T: D>(x: T) -> bool { x.pick::<bool>(true) };
static inferred = fn::<T: D>(x: T) -> bool { x.pick(true) };
static main = fn() -> bool { generic::<usize>(1) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_turbofish_argument_is_named_in_the_blame() {
    // `Cause::MemberGenericArg`: the hint reads the MEMBER's list, which
    // hangs one level down inside `MEMBER_GENERIC_ARGS` — under a dot-call
    // callee here, under a path in the second program. Sharing
    // `Cause::GenericArg`'s renderer would have pointed at the OWNER's
    // argument instead, which is the wrong node and the wrong `usize`.
    check_diagnostics(
        r#"
type Cell = struct::<T> { v: T } with {
    impl Self { pick = fn::<U>(alt: U, c: Self) -> U { alt }; }
};
static dotted = fn(c: Cell::<usize>) -> bool { c.pick::<bool>(1) };
static pathed = fn(c: Cell::<usize>) -> bool { Cell::<usize>::pick::<bool>(1, c) };
"#,
        expect![[r#"
            170..171: type mismatch: expected `bool`, found `{number}` (because this member argument instantiated the parameter to `bool` at 164..168)
            251..252: type mismatch: expected `bool`, found `{number}` (because this member argument instantiated the parameter to `bool` at 245..249)
        "#]],
    );
}

#[test]
fn two_same_named_parameters_are_told_apart_in_the_message() {
    // `shadowed_param_note`, both branches. ONE binder declaring the name
    // twice (a member shadowing its owner's `T`) is the blessed case, so
    // its mismatch has to be actionable — "expected `T`, found `T`" is not.
    // Indices count every kind, which is why the member's `T` here is 1 and
    // not 0, and why the message says so.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        bad = fn::<T>(t: T, p: Self) -> T { p.a };
    }
};
"#,
        expect![[r#"
            98..101: type mismatch: expected `T`, found `T` — `P::bad` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 1, the value has the one at index 0 — rename one of them (expected `T` because of this return type at 91..95)
        "#]],
    );
    // The pair need not be the WHOLE type: the shape the blessing invites
    // most is `Self` against `Owner::<T>`, where the two `T`s sit one
    // constructor down and the plain message reads "expected `P::<T>`,
    // found `P::<T>`". A borrow of one is the same case one more
    // constructor down, and pins that regions count in the index (the
    // member's `T` is 2 there, not 1).
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        nested = fn::<T>(t: T, p: Self) -> P::<T> { p };
        deep = fn::<@x, T>(t: T, p: Self.&::<@x>) -> P::<T>.&::<@x> { p };
    }
};
"#,
        expect![[r#"
            106..107: type mismatch: expected `P::<T>`, found `P::<T>` — `P::nested` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 1, the value has the one at index 0 — rename one of them (expected `P::<T>` because of this return type at 94..103)
            181..182: type mismatch: expected `P::<T>.&::<@x>`, found `P::<T>.&::<@x>` — `P::deep` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 2, the value has the one at index 0 — rename one of them (expected `P::<T>.&::<@x>` because of this return type at 161..178)
        "#]],
    );
    // The note's OTHER branch (two different items each declaring the
    // name) is UNREACHABLE today and is not pinned as behaviour, because
    // there is no program that produces it: a param cannot escape its
    // body, so every cross-item mention instantiates to fresh variables
    // and two rigid params only ever meet inside ONE binder. Both shapes
    // below are the near-misses, and both check clean — which is the
    // claim, and what would break first if that invariant ever moved.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        one = fn::<T>(t: T, p: Self) -> T { P::two(t, p) };
        two = fn::<T>(t: T, p: Self) -> T { t };
    }
};
static f = fn::<T>(x: T) -> T { g(x) };
static g = fn::<T>(x: T) -> T { x };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_const_argument_is_the_one_reservation_left() {
    // What `MemberOwnConstArgs` still says, and the only shape that can
    // still say it: an INHERENT member cannot declare a const parameter at
    // all (`syntax::validation` refuses it), so this needs a trait
    // requirement — whose own binder has always been live — and the
    // refusal is of the WHOLE list, in both the called and the dot forms.
    check_diagnostics(
        r#"
trait D = requires { n: fn::<const N: usize>(s: Self) -> usize; } with {
    impl usize { n = fn::<const N: usize>(s: usize) -> usize { N }; }
};
static called = fn(s: usize) -> usize { D::n::<3>(s) };
static dotted = fn(s: usize) -> usize { s.n::<3>() };
"#,
        expect![[r#"
            187..199: `D::n` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
            243..253: `usize::n` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
        "#]],
    );
}

#[test]
fn a_member_turbofish_lands_only_on_a_member() {
    // The dot's new turbofish position is spendable on a MEMBER and on
    // nothing else the dot can reach: an fn-typed field carries the call
    // as a VALUE (no binder), and a builtin member has no binder either.
    // One refusal for both, stated on the name.
    check_diagnostics(
        r#"
type Holder = struct { go: fn(usize) -> usize };
static field_call = fn(h: Holder) -> usize { h.go::<usize>(1) };
static builtin_call = fn(s: str, i: usize) -> usize { let c = s.next_char::<usize>(i); i };
static plain_field = fn(h: Holder) -> fn(usize) -> usize { h.go::<usize> };
"#,
        expect![[r#"
            95..108: `go` takes no generic arguments
            177..197: `next_char` takes no generic arguments
            266..279: `go` takes no generic arguments
        "#]],
    );
}

#[test]
fn named_self_call_pins_an_uninferable_self() {
    // The short form cannot infer `Self` when no argument mentions it;
    // the named-Self form states it — much of its point.
    check_diagnostics(
        r#"
trait D = requires { n: fn(k: usize) -> Self; } with {
    impl usize { n = fn(k: usize) -> usize { k }; }
};
static main = fn() -> usize { D::<Self = usize>::n(3) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn named_self_call_on_a_rigid_self_dispatches_through_the_dictionary() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { D::<Self = T>::m(x) };
static main = fn() -> usize { f::<usize>(7) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn named_self_member_value_is_impl_specific() {
    // TR01's impl-specific fn value: one impl's member, usable as a value.
    // A RIGID `Self` would read the enclosing dictionary — the same
    // capture wall as `BoundFnValue`.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static ok = fn() -> usize { let f = D::<Self = usize>::m; f(4) };
static reserved = fn::<T: D>(x: T) -> usize { let f = D::<Self = T>::m; f(x) };
"#,
        expect![[r#"
            231..247: `D::m` on a rigid `Self` comes from the enclosing dictionary, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn named_self_argument_list_rules() {
    // Only `Self` is nameable (TR01), it may be given once, and a
    // non-generic trait takes no positional arguments of its own.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static a = fn() -> usize { D::<Self = usize, Self = usize>::m(1) };
static b = fn() -> usize { D::<W = usize, Self = usize>::m(1) };
static c = fn() -> usize { D::<usize, Self = usize>::m(1) };
static d = fn(x: usize) -> usize { id::<Self = usize>(x) };
static id = fn::<T>(x: T) -> T { x };
type Pair = struct::<T> { a: T, b: T };
static e = fn(p: Pair::<Self = usize>) -> usize { 1 };
"#,
        expect![[r#"
            138..172: `Self` is given more than once
            206..237: `W` cannot be supplied by name: `Self` is the only nameable generic argument (`Trait::<Self = Type>::member`)
            271..298: `D` takes no generic arguments
            340..358: only a trait has a `Self` argument to name
            467..479: only a trait has a `Self` argument to name
        "#]],
    );
}

#[test]
fn named_self_argument_may_not_be_a_hole() {
    // `Self` NAMES the implementer — it is what decides which impl the
    // path denotes — so a `_` there declines to answer the only question
    // the spelling asks. Refused structurally at lowering, in BOTH
    // positions and at ANY depth, so no inference variable ever reaches
    // trait resolution (the old shape rendered `_ does not implement D`
    // and `Pair::<_> does not implement E`).
    //
    // Uniform on purpose: in call position a top-level hole IS inferable
    // from the arguments, but it adds nothing over the short form
    // `D::m(v)` — and both short and full spellings still work below.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
type Pair = struct::<T> { a: T, b: T };
trait E = requires { n: fn(x: Self) -> usize; };
static a = fn(v: usize) -> usize { D::<Self = _>::m(v) };
static b = fn() -> () { let f = D::<Self = _>::m; };
static c = fn() -> () { let f = E::<Self = Pair::<_>>::n; };
static ok = fn(v: usize) -> usize { D::<Self = usize>::m(v) + D::m(v) };
"#,
        expect![[r#"
            235..251: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
            290..306: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
            343..367: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
        "#]],
    );
}

#[test]
fn named_self_reservations_survive() {
    // Associated types (reserved), generic traits (reserved) and the
    // bounded-value capture wall are untouched by the named-Self form.
    check_diagnostics(
        r#"
trait Gen = requires::<T> { g: fn(x: Self) -> T; };
trait D = requires {
    type Item;
    m: fn::<W: D>(w: W, x: Self) -> usize;
} with {
    impl usize { m = fn::<W: D>(w: W, x: usize) -> usize { x }; }
};
static a = fn(x: usize) -> usize { D::<Self = usize>::Item };
static b = fn(x: usize) -> usize { Gen::<Self = usize>::g(x) };
static c = fn(x: usize) -> usize { let f = D::<Self = usize>::m; 1 };
"#,
        expect![[r#"
            21..26: generic traits are not supported yet
            78..82: associated types are not supported yet
            245..268: `D::Item` is an associated type; associated types are not supported yet
            307..329: `Gen` is a reserved generic trait (generic traits are not supported yet) and cannot be used
            379..399: `D::m` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn bounded_fn_as_value_reports_once() {
    // The reservation is the whole story — no cannot-infer sibling.
    check_diagnostics(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static bounded = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let v = bounded;
    let _ = v;
};
"#,
        expect![[r#"
            204..211: `bounded` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn builtin_type_as_type_side_impl_head_is_not_a_trait() {
    check_diagnostics(
        r#"
type P = struct { a: usize } with {
    impl usize { m = fn(x: Self) -> usize { 1 }; }
};
"#,
        expect![[r#"
            46..51: `usize` is not a trait
        "#]],
    );
}

// ---- safe borrows and regions -------------------------------------------

#[test]
fn borrow_types_carry_their_region() {
    check_infer(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#"
            13..56 'fn::<@a>(r: usize...': fn(usize.&::<@a>) -> usize
            22..23 'r': usize.&::<@a>
            49..56 '{ r.* }': usize
            51..52 'r': usize.&::<@a>
            51..54 'r.*': usize
        "#]],
    );
}

#[test]
fn borrow_without_a_region_is_reported_not_guessed() {
    // Elision is DEFERRED, not absent by oversight — the whole point of
    // launching without it is that the omission is visible.
    check_diagnostics(
        "static f = fn (r: usize.&) -> usize { r.* };",
        expect![[r#"
            24..25: a safe borrow must name its region (`T.&::<@a>`); regions are never elided yet
        "#]],
    );
}

#[test]
fn wildcard_region_is_a_body_answer_not_a_signature_one() {
    // `@_` says "there is a region here, infer it". A body can answer
    // that; a signature cannot, because its regions are parameters the
    // caller chooses.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let s: usize.&::<@_> = r; s.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn (r: usize.&::<@_>) -> usize { r.* };",
        expect![[r#"
            28..30: `@_` cannot be used in a signature — declare the region in the binder (`fn::<@a>`) and name it here
        "#]],
    );
}

#[test]
fn safe_deref_needs_no_unsafe_but_raw_still_does() {
    // Safety is decided by the pointer's FLAVOR — the one place
    // `unsafe_check` consults types.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn (r: usize.&raw) -> usize { r.* };",
        expect![[r#"
            41..44: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_non_copyable_referent_cannot_be_read_out_of_a_borrow() {
    // M07's copy rule, and the reason safe `.*` needed a ruling: `r.*` as
    // a VALUE duplicates the referent, which `usize.&mut` may not be.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>) \
         -> () { let s = r.*; };",
        expect![[r#"
            92..95: cannot move out of a borrow: `struct { q: usize.&mut::<@a>, x: usize }` cannot be copied
        "#]],
    );
}

#[test]
fn a_deref_in_place_position_projects_instead_of_copying() {
    // The same borrow, PROJECTED: a field read, a write through it, a
    // reborrow of the inner borrow, and a new borrow of a field. None of
    // them copies the affine referent, so the copy rule does not apply —
    // this is the ordinary context-struct shape.
    let ty = "struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>";
    for body in [
        "r.*.x",
        "{ r.*.x = 9; 0 }",
        "r.*.q.*",
        "{ let m = r.*.x.&mut::<@a>; 0 }",
    ] {
        check_diagnostics(
            &format!("static f = fn::<@a>(r: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
}

#[test]
fn a_borrow_projection_is_judged_at_its_last_step() {
    // Projecting THROUGH a deref defers the copy question; it does not
    // retire it. What the chain finally materializes is copied like any
    // other read, so an exclusive borrow reached through a shared one is
    // refused — two live `.&mut` to one place is exactly what M07 exists
    // to prevent, and the whole-referent read is no longer the only route
    // to it.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> () { let c = r.*.q; };",
        expect![[r#"
            89..94: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(r: [usize.&mut::<@a>; 2].&::<@a>) -> () { let c = r.*[0]; };",
        expect![[r#"
            70..76: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
    // And the steps that are not the last one keep projecting: a copyable
    // field is a copy of THAT, a further step stays a place, and a step
    // handed to a borrow-expecting parameter is a reborrow.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> usize { r.*.x };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> usize { r.*.q.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static set_to = fn::<@c>(m: usize.&mut::<@c>, v: usize) -> () { m.* = v; }; \
         static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>) \
         -> () { set_to(r.*.q, 5); };",
        expect![[r#""#]],
    );
}

#[test]
fn a_borrow_behind_a_borrow_projects_and_reborrows() {
    // `T.&mut.&mut` is a type the language accepts, so every use of it
    // has to be reachable. The inner `.*` is the outer one's RECEIVER —
    // a place, not a value — and `bb.*` in an argument position is a
    // reborrow (M07), which suspends the parent instead of copying it.
    let ty = "usize.&mut::<@a>.&mut::<@b>";
    for body in [
        "bb.*.*",
        "{ bb.*.* = 5; 0 }",
        "{ let c = bb.*.*.&mut::<@a>; 0 }",
    ] {
        check_diagnostics(
            &format!("static f = fn::<@a, @b>(bb: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
    check_diagnostics(
        "static set_to = fn::<@c>(r: usize.&mut::<@c>, v: usize) -> () { r.* = v; }; \
         static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize \
         { set_to(bb.*, 5); 0 };",
        expect![[r#""#]],
    );
    // The one read that really is a copy still cannot happen: no
    // borrow-typed context caught it, so nothing reborrowed.
    check_diagnostics(
        "static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> () { let c = bb.*; };",
        expect![[r#"
            73..77: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
}

#[test]
fn a_dot_call_receiver_is_a_value_position() {
    // `recv.name(args)` hands the receiver to the MEMBER by value (G13's
    // structural selection), so a `.*` there is a copy like any other —
    // the one field receiver that is not a place.
    check_diagnostics(
        "type Cell = struct::<T> { v: T } with \
         { impl Self { take = fn (c: Self) -> T { c.v }; } }; \
         static f = fn::<@a>(r: Cell::<usize.&mut::<@a>>.&mut::<@a>) -> () \
         { let m = r.*.take(); };",
        expect![[r#"
            167..170: cannot move out of a borrow: `Cell::<usize.&mut::<@a>>` cannot be copied
        "#]],
    );
    // Which is why the question waits for resolution: the SAME syntax on
    // an fn-typed field reads only the field, leaving the receiver a
    // place — as the parenthesized call of that field plainly does.
    let ty = "struct { q: usize.&mut::<@a>, f: fn(usize) -> usize }.&mut::<@a>";
    for body in ["r.*.f(3)", "(r.*.f)(3)"] {
        check_diagnostics(
            &format!("static f = fn::<@a>(r: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
}

#[test]
fn exclusive_borrow_needs_a_mut_root() {
    check_diagnostics(
        "static f = fn () -> () { let n: usize = 1; let m = n.&mut; };",
        expect![[r#"
            51..52: cannot borrow `n` as `.&mut`: it is not declared `mut`
        "#]],
    );
}

#[test]
fn an_exclusive_borrow_of_an_item_points_at_its_definition() {
    // Neither item is an exclusively-borrowable place, each for its own
    // reason, and both say where the name is defined — the courtesy
    // `AssignToItem` and `AddrOfMutItem` already extend.
    check_diagnostics(
        "static s: usize = 7;\nconst g: usize = 8;\n\
         static f = fn () -> () { let a = s.&mut; let b = g.&mut; };",
        expect![[r#"
            74..75: cannot borrow a `static` as `.&mut`: `static mut` is not supported yet (`s` is defined here at 7..8)
            90..91: cannot borrow a `const` as `.&mut`: a `const` is copied at every mention, so there is no one place to borrow (`g` is defined here at 27..28)
        "#]],
    );
}

#[test]
fn shared_parent_cannot_launder_into_an_exclusive_child() {
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> () { let m = r.*.&mut::<@_>; };",
        expect![[r#"
            54..68: cannot borrow `.&mut` through `usize.&::<@a>`: an exclusive borrow needs a `.&mut` parent
        "#]],
    );
}

#[test]
fn a_shared_step_anywhere_in_a_place_refuses_a_write() {
    // The headline rule: a shared borrow never grants a write. It is
    // judged over the WHOLE chain, not the outermost step — a `.&mut`
    // held behind a `.&` is read out by REBORROWING it (M07's affinity),
    // which the place holding it must be allowed to grant.
    let direct = "usize.&::<@a>";
    let nested = "usize.&mut::<@a>.&::<@b>";
    for (ty, body, expected) in [
        (
            direct,
            "{ b.* = 5; 0 }",
            expect![[r#"
                55..58: cannot assign through `usize.&::<@a>`: writing needs a `.&mut` borrow
            "#]],
        ),
        (
            direct,
            "{ let p = b.*.&raw mut; 0 }",
            expect![[r#"
                63..75: cannot take `.&raw mut` through `usize.&::<@a>`: minting a mutating address needs a `.&mut` borrow
            "#]],
        ),
        (
            nested,
            "{ b.*.* = 5; 0 }",
            expect![[r#"
                66..71: cannot assign through `usize.&mut::<@a>.&::<@b>`: writing needs a `.&mut` borrow
            "#]],
        ),
        (
            nested,
            "{ let c = b.*.*.&mut::<@a>; 0 }",
            expect![[r#"
                74..90: cannot borrow `.&mut` through `usize.&mut::<@a>.&::<@b>`: an exclusive borrow needs a `.&mut` parent
            "#]],
        ),
        (
            nested,
            "{ let p = b.*.*.&raw mut; 0 }",
            expect![[r#"
                74..88: cannot take `.&raw mut` through `usize.&mut::<@a>.&::<@b>`: minting a mutating address needs a `.&mut` borrow
            "#]],
        ),
        (
            "usize.&::<@a>.&mut::<@b>",
            "{ b.*.* = 5; 0 }",
            expect![[r#"
                66..71: cannot assign through `usize.&::<@a>`: writing needs a `.&mut` borrow
            "#]],
        ),
        // An element step is as transparent as a field: a shared borrow
        // of an array cannot launder a write into one of its elements.
        (
            "[usize.&mut::<@a>; 2].&::<@b>",
            "{ b.*[0].* = 5; 0 }",
            expect![[r#"
                71..79: cannot assign through `[usize.&mut::<@a>; 2].&::<@b>`: writing needs a `.&mut` borrow
            "#]],
        ),
    ] {
        check_diagnostics(
            &format!("static f = fn::<@a, @b>(b: {ty}) -> usize {{ {body} }};"),
            expected,
        );
    }
    // The implicit mutable reborrow is the same mint with the `.&mut`
    // left unwritten, so it answers to the same rule.
    check_diagnostics(
        "static set_to = fn::<@c>(r: usize.&mut::<@c>, v: usize) -> () { r.* = v; }; \
         static f = fn::<@a, @b>(b: usize.&mut::<@a>.&::<@b>) -> usize \
         { set_to(b.*, 5); 0 };",
        expect![[r#"
            147..150: cannot borrow `.&mut` through `usize.&mut::<@a>.&::<@b>`: an exclusive borrow needs a `.&mut` parent
        "#]],
    );
}

#[test]
fn a_raw_mut_step_grants_the_permission_it_carries() {
    // The raw world's own laundering, and the reason the walk stops at a
    // `.&raw mut`: reading a raw pointer out of a place COPIES it, and a
    // copy carries the whole permission. `unsafe` is what gates it — a
    // `.&raw` above makes no difference.
    check_diagnostics(
        "static f = fn (pp: usize.&raw mut.&raw) -> () { unsafe { pp.*.* = 5; } };",
        expect![[r#""#]],
    );
    // A `.&mut` in the same position does NOT stop the walk: reading it
    // out is a reborrow, not a copy.
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw) -> () { unsafe { pb.*.* = 5; } };",
        expect![[r#"
            64..70: cannot assign through `usize.&mut::<@a>.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
    // Minting through the same pair, so the advice the diagnostic gives
    // is the advice that works.
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw) -> () \
         { unsafe { let c = pb.*.*.&mut::<@a>; }; };",
        expect![[r#"
            72..89: cannot borrow `.&mut` through `usize.&mut::<@a>.&raw`: an exclusive borrow needs a `.&raw mut` parent
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw mut) -> () \
         { unsafe { let c = pb.*.*.&mut::<@a>; }; };",
        expect![[r#""#]],
    );
    // And a non-`mut` binding holding a `.&mut` still writes: the root's
    // own mutability is a different rule, about reassigning the binding.
    check_diagnostics(
        "static f = fn () -> usize { let mut n: usize = 1; let b = n.&mut::<@_>; b.* = 5; n };",
        expect![[r#""#]],
    );
}

#[test]
fn a_shared_borrow_cannot_be_minted_through_a_raw_pointer() {
    // A raw pointer carries no region — unlike the shared-parent case
    // (`shared_parent_cannot_launder_into_an_exclusive_child`, whose
    // parent HAS a region and is refused only for `.&mut`), no safe
    // borrow at all can be minted through one.
    check_diagnostics(
        "static f = fn (r: usize.&raw) -> () { let m = unsafe { r.*.& }; };",
        expect![[r#"
            55..60: cannot mint a safe borrow through `usize.&raw`: a raw pointer carries no region for the new borrow to be bounded by
        "#]],
    );
}

#[test]
fn an_exclusive_borrow_cannot_be_minted_through_a_raw_pointer() {
    check_diagnostics(
        "static f = fn (r: usize.&raw mut) -> () { let m = unsafe { r.*.&mut }; };",
        expect![[r#"
            59..67: cannot mint a safe borrow through `usize.&raw mut`: a raw pointer carries no region for the new borrow to be bounded by
        "#]],
    );
}

#[test]
fn a_region_name_must_be_declared() {
    check_diagnostics(
        "static f = fn () -> () { let n: usize = 1; let r = n.&::<@q>; };",
        expect![[r#"
            51..60: no region named `@q` is in scope; declare it in the binder (`fn::<@q>`)
        "#]],
    );
}

#[test]
fn regions_on_type_declarations_are_reserved() {
    check_diagnostics(
        "type Slice = struct::<@a, T> { n: usize };",
        expect![[r#"
            22..24: region parameters on type declarations are not supported yet
        "#]],
    );
}

#[test]
fn undeclared_outlives_is_rejected_and_the_clause_fixes_it() {
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };",
        expect![[r#"
            61..62: using this borrow where a longer-lived one is expected needs `@a` to outlive `@b`, which this signature does not declare; add `@a: @b` to the binder
        "#]],
    );
    // With the clause written, the same body is accepted.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn outlives_is_transitive_through_declared_bounds() {
    check_diagnostics(
        "static f = fn::<@a: @b, @b: @c, @c>(x: usize.&::<@a>) -> usize.&::<@c> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn a_written_region_argument_carries_the_calls_declared_bound() {
    // `f::<@p, @q>` substitutes the CALLER's own regions directly, so
    // `f`'s declared `@a: @b` becomes a direct obligation between `@p`
    // and `@q` at THIS call — reported as `CalleeBound`, not laundered
    // through whichever argument happens to carry the element across.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@b>) -> usize.&::<@b> { x };\n\
         static g = fn::<@p, @q>(x: usize.&::<@q>) -> usize.&::<@q> { f::<@p, @q>(x) };",
        expect![[r#"
            131..142: calling this function needs `@p` to outlive `@q`, which this signature does not declare; add `@p: @q` to the binder
        "#]],
    );
    // Declaring the bound the call needs makes it clean.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@b>) -> usize.&::<@b> { x };\n\
         static g = fn::<@p: @q, @q>(x: usize.&::<@q>) -> usize.&::<@q> { f::<@p, @q>(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn an_omitted_region_argument_still_infers_freely() {
    // No turbofish at all, and the wildcard, both still mean "infer this
    // one" — a fresh existential, exactly as before this ruling: a
    // written argument only changes anything when one is actually
    // written.
    check_diagnostics(
        "static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> { x };\n\
         static g = fn::<@p>(x: usize.&::<@p>) -> usize.&::<@p> { f(x) };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> { x };\n\
         static g = fn::<@p>(x: usize.&::<@p>) -> usize.&::<@p> { f::<@_>(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn a_borrow_of_a_local_cannot_escape_the_body() {
    check_diagnostics(
        "static f = fn::<@a>() -> usize.&::<@a> { let n: usize = 5; n.& };",
        expect![[r#"
            59..62: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

#[test]
fn an_explicit_shared_reborrow_needs_the_parents_region_to_outlive_it() {
    // `x.*.&` is a WRITTEN reborrow, not a use-site coercion — it must
    // incur the same outlives edge the implicit path does. Without it
    // `@b`'s region launders into `@a` with no bound declared.
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@b>) -> usize.&::<@a> { x.*.& };",
        expect![[r#"
            61..66: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
    // Declaring the bound makes it clean.
    check_diagnostics(
        "static f = fn::<@a, @b: @a>(x: usize.&::<@b>) -> usize.&::<@a> { x.*.& };",
        expect![[r#""#]],
    );
}

#[test]
fn an_explicit_mut_reborrow_needs_the_parents_region_to_outlive_it() {
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&mut::<@b>) -> usize.&::<@a> { x.*.&mut };",
        expect![[r#"
            64..72: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn an_explicit_reborrow_with_a_named_region_still_needs_the_bound() {
    // Spelling the target region out (`x.*.&::<@a>`) is the same event as
    // leaving it to inference — the region comes from the write, but the
    // outlives edge it incurs is identical.
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@b>) -> usize.&::<@a> { x.*.&::<@a> };",
        expect![[r#"
            61..72: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn a_two_step_escape_through_an_explicit_reborrow_is_rejected() {
    // `n.&` alone would be caught directly by the escape check (it names
    // `n` as its root). Going through `r.*.&` first used to defeat that
    // check entirely, because the reborrow it performs recorded no edge —
    // the escape then had to be re-derived from a region that was already
    // free of the local it came from.
    check_diagnostics(
        "static f = fn::<@a>() -> usize.&::<@a> {\n\
             let n: usize = 5;\n\
             let r = n.&;\n\
             r.*.&\n\
         };",
        expect![[r#"
            67..70: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

#[test]
fn reborrow_at_every_use_lets_a_mut_borrow_be_used_repeatedly() {
    // The affinity correction made real: `m` is passed twice and then read.
    // A language that MOVED on the first use would reject this.
    check_diagnostics(
        "static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = 1; };\n\
         static f = fn::<@a>(m: usize.&mut::<@a>) -> usize { bump(m); bump(m); m.* };",
        expect![[r#""#]],
    );
}

#[test]
fn exclusive_degrades_to_shared_but_never_the_reverse() {
    check_diagnostics(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static f = fn::<@a>(m: usize.&mut::<@a>) -> usize { get(m) };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static set = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = 1; };\n\
         static f = fn::<@a>(r: usize.&::<@a>) -> () { set(r) };",
        expect![[r#"
            113..114: type mismatch: expected `usize.&mut`, found `usize.&::<@a>`
        "#]],
    );
}

#[test]
fn a_region_join_is_outlived_by_every_member() {
    // `@a + @b` reads as conjunction: an obligation against the join is an
    // obligation against each member, so this needs BOTH clauses.
    check_diagnostics(
        "static f = fn::<@a, @b, @c>(x: usize.&::<@a + @b>) -> usize.&::<@c> { x };",
        expect![[r#"
            70..71: using this borrow where a longer-lived one is expected needs `@a` to outlive `@c`, which this signature does not declare; add `@a: @c` to the binder
            70..71: using this borrow where a longer-lived one is expected needs `@b` to outlive `@c`, which this signature does not declare; add `@b: @c` to the binder
        "#]],
    );
}

#[test]
fn a_join_of_borrows_reborrows_into_its_context() {
    // A join is a use site like any other, so each branch REBORROWS into
    // the join's result — one directed edge per branch, never a mutual
    // relation between the branches.
    //
    // The comment here used to say the opposite: that relating the
    // branches invariantly was "the whole cost of the invariance-first
    // posture". That diagnosis was wrong and it hid a soundness hole for
    // an entire arc. Invariance governs the REFERENT of a borrow, not the
    // region of a borrow being formed — under reborrow-at-every-use each
    // branch mints its own shorter loan, and the join's region is their
    // MEET. Because the framing said the rejection was expected, nobody
    // asked why a join emitted no edge to its context at all.
    //
    // So: two independent universals join cleanly...
    check_diagnostics(
        "static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static pick = fn::<@a, @b>(p: usize.&::<@a>, q: usize.&::<@b>, c: bool) -> usize {\n\
             get(if c { p } else { q })\n\
         };",
        expect![[r#""#]],
    );
    // ...and the obligation a branch owes its CONTEXT still lands.
    check_diagnostics(
        "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {\n\
             if c { p } else { p }\n\
         };",
        expect![[r#"
            77..78: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn a_region_is_not_part_of_a_types_identity() {
    // Two `usize.&mut` at DIFFERENT regions join without a TYPE mismatch:
    // unification decides type identity, and a region is not part of one.
    // What the difference costs is an outlives obligation, which the
    // declared clauses here discharge — so the user sees nothing, rather
    // than a message about types disagreeing.
    check_diagnostics(
        "static f = fn::<@a: @b, @b: @a>(x: usize.&mut::<@a>, y: usize.&mut::<@b>)\n\
                        -> usize.&mut::<@a> { if true { x } else { y } };",
        expect![[r#""#]],
    );
}

#[test]
fn a_dot_through_a_borrow_is_reserved_with_its_escape_named() {
    // G14's exception is one-directional: the compiler may insert a
    // borrow of `x.*`, never a deref of `x`. A member whose own `Self` is
    // a borrow is reached through a borrow receiver; a VALUE `Self` is
    // not — that would be the deref — and the message names the
    // one-character escape.
    check_diagnostics(
        "type Counter = struct { n: usize } with {\n\
             impl Self { get = fn (c: Self) -> usize { c.n }; }\n\
         };\n\
         static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.get() };",
        expect![[r#"
            147..152: `Counter.&::<@a>` is a borrow, so `.get` does not reach through it — there is no auto-deref; write `.*.get`
        "#]],
    );
    // The escape, working.
    check_diagnostics(
        "type Counter = struct { n: usize } with {\n\
             impl Self { get = fn (c: Self) -> usize { c.n }; }\n\
         };\n\
         static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.*.get() };",
        expect![[r#""#]],
    );
}

// ---- members that borrow `Self` ----------------------------------------

#[test]
fn an_inherent_members_own_region_binder_is_live() {
    // A member-own REGION was the first half of the binder to go live, and
    // it is the half with no alternative at all: regions on type
    // declarations are themselves reserved, so a member taking a borrow of
    // `Self` has nowhere else to bind the per-call region it needs, and
    // with no elision it may not decline to name one. (The TYPE half is
    // live too — TR10 — and spelled at the use site; only CONSTS are still
    // reserved.)
    check_diagnostics(
        r#"
type Map = struct::<K, V> { n: usize } with {
    impl Self {
        peek = fn::<@b>(m: Self.&mut::<@b>) -> usize { m.*.n };
    }
};
static f = fn::<@a>(m: Map::<usize, usize>.&mut::<@a>) -> usize { m.peek() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_region_is_per_call_never_the_callers_universal() {
    // The member's binder is the OWNER's followed by its own, so a
    // member-own region sits at `owner_arity + i`. Left rigid, it survives
    // into the CALLER's body, where the outlives solver reads a region
    // param's binder index as a NODE NUMBER — a member's `@b` at index 0
    // silently BECOMES the caller's universal at index 0, and two calls in
    // one body share one region. Both spellings mint a fresh existential
    // instead, so this program (which relates nothing to `@a`) checks.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        pass = fn::<@b>(p: usize.&::<@b>, m: Self.&::<@b>) -> usize.&::<@b> { p };
    }
};
static dot = fn::<@a>(c: Cell.&::<@a>) -> usize {
    let short: usize = 5;
    c.pass(short.&).*
};
static qualified = fn::<@a>(c: Cell.&::<@a>) -> usize {
    let short: usize = 5;
    Cell::pass(short.&, c).*
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_region_still_carries_its_declared_outlives_bounds() {
    // `fn::<@b, @c: @b>` at a member is the same instantiation a free fn
    // gets: the declared bound becomes an obligation between the two fresh
    // variables, so the body may rely on it and the caller must supply it.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        widen = fn::<@b, @c: @b>(p: usize.&::<@c>, m: Self.&::<@b>) -> usize.&::<@b> { p };
    }
};
static f = fn::<@a>(c: Cell.&::<@a>, p: usize.&::<@a>) -> usize { c.widen(p).* };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_const_binder_stays_reserved_precisely() {
    // The one kind that keeps its reservation, still stated per kind and
    // squiggling the individual param rather than the whole list — so a
    // mixed binder grants the region and the type and refuses only the
    // const.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self {
        size = fn::<@b, T, const N: usize>(m: Self.&::<@b>, t: T) -> usize { m.*.n };
    }
};
"#,
        expect![[r#"
            87..101: a member's own const parameters are not supported yet (a const argument is part of an instance's identity, and a member's arguments are read off the receiver's own type)
        "#]],
    );
}

#[test]
fn a_borrow_receiver_dot_call_reborrows_into_the_members_region() {
    // G14 and TR01 say the receiver IS the last argument, so a member whose last
    // parameter is a BORROW of `Self` takes a borrow receiver — through
    // exactly the reborrow every other argument position gets. This is G14's
    // exception clause 1: the inserted borrow is of `map.*`, where `map` is
    // already a borrow. Exclusive receivers reach shared members too, by
    // the ordinary degradation reborrow.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        get = fn::<@b>(m: Self.&::<@b>) -> usize { m.*.n };
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn::<@a>(c: Cell.&mut::<@a>) -> usize {
    c.bump();
    c.get()
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_owned_receiver_never_gets_a_borrow_inserted_for_it() {
    // NOT auto-ref, and the reason is clause 2 of the G14 exception: the
    // borrow the compiler would have to insert is a borrow of the LOCAL
    // itself, which the exception forbids outright. So the escape is
    // spelled, and the diagnostic spells it.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn() -> usize {
    let mut c = Cell(struct { n = 1 });
    c.bump();
    c.n
};
"#,
        expect![[r#"
            210..218: `bump` takes `Self.&mut`, and a borrow is never inserted for an owned receiver — write `.&mut.bump(...)` (`bump` is defined here at 64..68)
        "#]],
    );
    // The escape, working: `c.&mut.bump()` is an ordinary postfix chain
    // whose receiver is then a borrow, so it comes back through the
    // licensed case.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn() -> usize {
    let mut c = Cell(struct { n = 1 });
    c.&mut.bump();
    c.n
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_shared_receiver_cannot_reach_an_exclusive_member() {
    // Shared never sharpens to exclusive — the same rule `try_reborrow`
    // enforces at every other argument position, stated at the receiver,
    // where the place is still nameable.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn::<@a>(c: Cell.&::<@a>) -> () { c.bump() };
"#,
        expect![[r#"
            184..192: `bump` takes `Self.&mut`, but `Cell.&::<@a>` is a shared borrow — a shared borrow never becomes exclusive (`bump` is defined here at 64..68)
        "#]],
    );
}

#[test]
fn the_receiver_shape_disambiguates_a_member_against_a_same_named_field() {
    // A pleasant consequence of the rule being a TABLE rather than a
    // filter: a fn-typed field and a `Self.&`-taking member of the same
    // name are never both candidates, so neither spelling is ambiguous and
    // each names exactly one thing.
    //
    // Through a BORROW the field is not reachable at all (that would be
    // auto-deref), so the member wins; on an OWNED receiver the member is
    // not reachable (clause 2 forbids borrowing the local), so the field
    // wins — and the reader who wanted the member writes `b.&.get()`.
    check_diagnostics(
        r#"
type B = struct { get: fn() -> usize } with {
    impl Self { get = fn::<@b>(x: Self.&::<@b>) -> usize { 100 }; }
};
static through_a_borrow = fn::<@a>(b: B.&::<@a>) -> usize { b.get() };
static owned = fn(b: B) -> usize { b.get() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_variant_typed_borrow_does_not_widen_into_an_enum_member() {
    // The boundary, pinned because it looks like an omission and is not.
    // A variant-typed RECEIVER widens into an enum member's `Self`
    // parameter — that is the sanctioned conversion. Through a BORROW it
    // cannot: widening injects a tag, which changes the representation,
    // and there is nowhere to put the tagged value when all you hold is a
    // pointer at the untagged one. So the member is still found (the
    // referent's enum owns it) and the receiver check reports the honest
    // mismatch; `NoSuchMember` would have been the worse answer.
    //
    // Nothing here is auto-deref-adjacent: `.*.area()` reads the referent,
    // which is a copy, and widens like any other value.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point } with {
    impl Self {
        area = fn::<@b>(s: Self.&::<@b>) -> usize { 1 };
    }
};
static borrowed_variant = fn::<@a>(c: Shape::Circle.&::<@a>) -> usize { c.area() };
"#,
        expect![[r#"
            205..206: type mismatch: expected `Shape.&`, found `Shape::Circle.&::<@a>`
        "#]],
    );
}

#[test]
fn a_trait_requirement_may_carry_a_region_binder() {
    // `binders_match` had arms for Type and Const and none for Region, so
    // a region param in a requirement's binder made EVERY textually
    // identical impl member "not match". A `Self.&`-taking requirement
    // must declare a region (nothing is elided), so that one guard was
    // what made the trait half of borrow-`Self` members unreachable.
    //
    // All three kinds side by side, each with a conforming impl.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait A = requires { f: fn(x: usize, s: Self) -> usize; }
  with { impl Cell { f = fn(x: usize, s: Self) -> usize { x }; } };
trait B = requires { g: fn::<T>(x: T, s: Self) -> usize; }
  with { impl Cell { g = fn::<T>(x: T, s: Self) -> usize { 1 }; } };
trait C = requires { h: fn::<@b>(x: usize.&::<@b>, s: Self) -> usize; }
  with { impl Cell { h = fn::<@b>(x: usize.&::<@b>, s: Self) -> usize { 1 }; } };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_region_binders_outlives_bounds_must_agree_positionally() {
    // Region outlives bounds name SIBLING params, so they compare by
    // position, not by spelling — the requirement's `@a`/`@b` and the
    // impl's `@x`/`@y` are the same binder. An impl that declares a
    // DIFFERENT bound set does not match, the same exact-set rule type
    // bounds already follow.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Renamed = requires { f: fn::<@a, @b: @a>(p: usize.&::<@b>, s: Self) -> usize; }
  with { impl Cell { f = fn::<@x, @y: @x>(p: usize.&::<@y>, s: Self) -> usize { 1 }; } };
"#,
        expect![[r#"
            141..142: member `f` does not match `Renamed`'s requirement: expected `fn(usize.&::<@b>, Cell) -> usize`, found `fn(usize.&::<@y>, Cell) -> usize` (required by the trait here at 61..62)
        "#]],
    );
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Dropped = requires { f: fn::<@a, @b: @a>(p: usize.&::<@b>, s: Self) -> usize; }
  with { impl Cell { f = fn::<@x, @y>(p: usize.&::<@y>, s: Self) -> usize { 1 }; } };
"#,
        expect![[r#"
            141..142: member `f`'s generic binder does not match `Dropped`'s requirement (arity, kinds and bounds must agree) (required by the trait here at 61..62)
        "#]],
    );
}

#[test]
fn a_borrow_self_requirement_is_reachable_both_ways() {
    // The trait half, end to end: declared, implemented, called on a
    // CONCRETE borrow receiver (impl-directed) and on a BORROWED RIGID one
    // (bound-directed, through the hidden dictionary). `examples/` has the
    // runnable twin — this same shape returns 82.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Readable = requires {
    read: fn::<@r>(s: Self.&::<@r>) -> usize;
} with {
    impl Cell { read = fn::<@r>(s: Cell.&::<@r>) -> usize { s.*.n }; }
};
static direct = fn::<@a>(c: Cell.&::<@a>) -> usize { c.read() };
static generic = fn::<@z, T: Readable>(t: T.&::<@z>) -> usize { t.read() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_borrowed_rigid_receiver_reaches_its_bounds() {
    // The bound-directed path is routed off the referent, so `T.&::<@z>`
    // reaches the same bounds a bare `T` does. Before, it fell past the
    // rigid branch to the concrete path and got `write .*.pass` — advice
    // that would move out of a borrow, on the one shape a `Self.&`
    // requirement is actually called on.
    //
    // The obligation flows through the dictionary path too: the escaping
    // form is rejected for ESCAPING, which is the proof the region edge is
    // emitted and not laundered by the indirection.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Pass = requires {
    pass: fn::<@b>(p: usize.&::<@b>, s: Self.&::<@b>) -> usize.&::<@b>;
} with {
    impl Cell { pass = fn::<@b>(p: usize.&::<@b>, s: Self.&::<@b>) -> usize.&::<@b> { p }; }
};
static ok = fn::<@z, T: Pass>(c: T.&::<@z>, p: usize.&::<@z>) -> usize.&::<@z> { c.pass(p) };
static leak = fn::<@z, T: Pass>(c: T.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    c.pass(n.&)
};
"#,
        expect![[r#"
            427..430: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
        "#]],
    );
}

#[test]
fn a_requirements_own_region_bound_travels_to_the_call_site() {
    // A requirement carrying its own region binder (`fn::<@b, @c: @b>`)
    // reaches a call for the first time here, so the bound it declares has
    // to reach it too: a requirement's binder is instantiated like any
    // other, and its outlives bounds become obligations between the fresh
    // regions. Without them both trait spellings launder the bound — the
    // escaping calls below check clean while their free-fn twin is
    // rejected.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Widen = requires {
    widen: fn::<@b, @c: @b>(p: usize.&::<@c>, s: Self.&::<@b>) -> usize.&::<@b>;
} with {
    impl Cell {
        widen = fn::<@b, @c: @b>(p: usize.&::<@c>, s: Cell.&::<@b>) -> usize.&::<@b> { p };
    }
};
static bound_directed = fn::<@z, T: Widen>(t: T.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    t.widen(n.&)
};
static qualified = fn::<@z>(c: Cell.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    Widen::widen(n.&, c)
};
"#,
        expect![[r#"
            376..379: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
            487..490: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
        "#]],
    );
}

#[test]
fn the_receiver_shape_table_is_the_same_on_a_rigid_receiver() {
    // One table, one set of messages, whether `Self` is nominal or rigid —
    // which is the point of routing both paths through
    // `receiver_shape_refusal`. A rigid receiver used to get a different
    // (and, for the borrow cases, wrong) story.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Bump = requires {
    bump: fn::<@r>(s: Self.&mut::<@r>) -> ();
} with {
    impl Cell { bump = fn::<@r>(s: Cell.&mut::<@r>) -> () { s.*.n = 1; }; }
};
static owned = fn::<T: Bump>(t: T) -> () { t.bump() };
static shared = fn::<@z, T: Bump>(t: T.&::<@z>) -> () { t.bump() };
static exclusive = fn::<@z, T: Bump>(t: T.&mut::<@z>) -> () { t.bump() };
"#,
        expect![[r#"
            235..243: `bump` takes `Self.&mut`, and a borrow is never inserted for an owned receiver — write `.&mut.bump(...)`
            303..311: `bump` takes `Self.&mut`, but `T.&::<@z>` is a shared borrow — a shared borrow never becomes exclusive
        "#]],
    );
}

// ---- region walkers: generic arguments and borrows --------------------

#[test]
fn a_type_param_under_a_borrow_is_instantiated() {
    // `instantiate_scheme` had no `Ty::Borrow` arm, so `T` in
    // `T.&mut::<@b>` stayed RIGID at every call: inference could not learn
    // `T` from a borrowed argument, and an explicit turbofish substituted
    // everywhere EXCEPT under the borrow, producing the impossible
    // "expected `T.&mut`, found `usize.&mut`" — a mismatch against a
    // parameter the caller has no way to name. One arm, both symptoms.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static pick = fn::<@b, T>(v: T.&mut::<@b>, c: bool) -> Opt::<T.&mut::<@b>> {
    Opt::<T.&mut::<@b>>::None
};
static inferred = fn() -> usize {
    let mut x: usize = 3;
    let r = pick(x.&mut, true);
    0
};
static explicit = fn() -> usize {
    let mut x: usize = 3;
    let r = pick::<@_, usize>(x.&mut, true);
    0
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_region_wildcard_inside_a_generic_argument_is_an_inference_variable() {
    // `@_` is a written token meaning "there is a region here, infer it".
    // `mint_wildcard_regions` replaced the placeholder with a fresh
    // variable for a borrow, a raw pointer, an array, an fn and a record —
    // and not for a generic type's ARGUMENTS, the same hole closed four
    // times elsewhere. `Opt::<V.&mut::<@_>>` is a shape users write.
    //
    // Both symptoms were rendering failures, not laundering: a type
    // spelling nobody may write (`@{error}`), and the compiler accusing
    // itself. The obligations were always emitted — which is why the
    // `leaks` case below was rejected before this fix and still is.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static mk = fn::<@b>(p: usize.&::<@b>) -> Opt::<usize.&::<@b>> { Opt::<usize.&::<@b>>::Some(p) };
static annotated = fn::<@a>(p: usize.&::<@a>) -> usize {
    let o: Opt::<usize.&::<@_>> = mk(p);
    0
};
"#,
        expect![[r#""#]],
    );
    // The mismatch case names a type the reader may actually write.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static mk = fn::<@b>(p: usize.&::<@b>) -> Opt::<usize.&::<@b>> { Opt::<usize.&::<@b>>::Some(p) };
static wrong = fn() -> usize {
    let n: usize = 7;
    let o: Opt::<bool.&::<@_>> = mk(n.&);
    0
};
"#,
        expect![[r#"
            225..232: type mismatch: expected `Opt::<bool.&>`, found `Opt::<usize.&>` (expected `Opt::<bool.&>` because of this annotation at 203..222)
        "#]],
    );
    // And the obligation still lands: an intermediate annotated with `@_`
    // relates the two regions, so an undeclared relation is still reported
    // — the same message the un-annotated control gets, not an internal
    // error.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None } with {
    impl Self { unwrap = fn(s: Self) -> T { match s { ::Some(t) => t, ::None => panic("no"), } } }
};
static leaks = fn::<@a, @z>(p: usize.&::<@a>) -> usize.&::<@z> {
    let o: Opt::<usize.&::<@_>> = Opt::<usize.&::<@a>>::Some(p);
    o.unwrap()
};
"#,
        expect![[r#"
            248..277: requiring these two borrows to be the same type needs `@a` to outlive `@z`, which this signature does not declare; add `@a: @z` to the binder
        "#]],
    );
}

#[test]
fn a_broken_receiver_does_not_also_get_accused_of_a_raw_deref() {
    // `derefs_a_raw_pointer` read "not a safe borrow" as "a raw pointer",
    // which is the right fail-safe default for an UNRESOLVED type and the
    // wrong one for an already-diagnosed one. Errors are infectious and
    // SILENT, and a broken receiver in a postfix chain is where that
    // shows.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self { take = fn(x: usize, s: Self) -> usize { 1 }; }
};
static f = fn(c: Cell) -> usize { c.nosuch().* };
"#,
        expect![[r#"
            140..150: no field or member `nosuch` on `Cell`
        "#]],
    );
    // The real thing is still caught.
    check_diagnostics(
        "static f = fn(p: usize.&raw) -> usize { p.* };",
        expect![[r#"
            40..43: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

// ---- `::Variant` in expression position --------------------------------

#[test]
fn an_elided_variant_expression_resolves_from_the_expected_type() {
    // The mirror of the elided-sigil variant PATTERN: the pattern reads the
    // scrutinee's enum, the expression reads the position's expected type.
    // Every axiom position works — an item annotation, a return type, a
    // call argument, a `let` annotation — including a generic enum, whose
    // ARGS come from the expectation rather than from a fresh mention.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
type Opt = enum::<T> { Some(T), None };
static annotated: Shape = ::Circle(3);
static returned = fn() -> Shape { ::Point };
static taken = fn(s: Shape) -> usize { 1 };
static passed = fn() -> usize { taken(::Point) };
static bound = fn() -> usize {
    let s: Shape = ::Circle(2);
    1
};
static generic = fn::<@a>(p: usize.&::<@a>) -> Opt::<usize.&::<@a>> { ::Some(p) };
static empty = fn::<@a>(p: usize.&::<@a>) -> Opt::<usize.&::<@a>> { ::None };
static returned_by_keyword = fn() -> Shape { return ::Point; };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_elided_variant_expression_is_reject_only_sugar() {
    // It reads `expected` and NOTHING else: no backwards inference, no
    // sibling scan, no deferral. Where no enum is in view the qualified
    // spelling is named, and it always works — so this only ever removes a
    // rejection, and the qualified form stays canonical.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static unpinned = fn() -> usize {
    let s = ::Point;
    1
};
static not_an_enum = fn() -> usize { ::Point };
static no_such_variant = fn() -> Shape { ::Square };
"#,
        expect![[r#"
            91..98: cannot resolve `::Point` without an expected type — write `Enum::Point`
            146..153: cannot resolve `::Point`: the expected type `usize` is not an enum — write `Enum::Point`
            198..206: `Shape` has no variant `Square` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn an_elided_variant_expression_is_refused_in_a_join_position() {
    // FLAGGED, and deliberate. A match arm's body and an `if` branch are
    // JOIN leaves: each is inferred against a fresh variable so "outer
    // expectation pressure never leaks in" (the join's own comment), and
    // the construct's expectation is applied to the RESULT after every leaf
    // has been visited. So there is genuinely no enum in view at the leaf,
    // and the sigil is refused with the escape named.
    //
    // Lifting this is a second expectation channel through the join — an
    // inference-shape decision, not sugar — and it is purely additive: it
    // would only ever turn these errors into acceptances.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static in_arm = fn(c: bool) -> Shape { match c { _ => ::Point } };
static in_branch = fn(c: bool) -> Shape { if c { ::Point } else { ::Point } };
"#,
        expect![[r#"
            99..106: cannot resolve `::Point` without an expected type — write `Enum::Point`
            161..168: cannot resolve `::Point` without an expected type — write `Enum::Point`
            178..185: cannot resolve `::Point` without an expected type — write `Enum::Point`
        "#]],
    );
}

#[test]
fn the_owners_reborrow_program_checks_clean() {
    // The acceptance test for members that borrow `Self`: the owner's own
    // `get_or_default`, unmodified. It exercises all four gaps at once —
    // a member-own region binder, a dot-call on a borrow receiver, `V`
    // inferred through `Self.&mut`, and `::None` in expression position —
    // and it is also the Polonius conditional-return case: `::Some(v) => v`
    // returns a loan derived from `map` out of a match whose other arm
    // borrows `map` again.
    //
    // `examples/reborrow.must` is the same program with a body, and it
    // RUNS: the borrow it hands back really aliases the map's storage.
    check_diagnostics(
        r#"
type Option = enum::<T> {
    Some(T),
    None,
} with {
    impl Self {
        unwrap = fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
    }
}

type Map = struct::<K, V> { } with {
    impl Self {
        get = fn::<@b>(key: K, m: Self.&mut::<@b>) -> Option::<V.&mut::<@b>> { ::None };
        insert = fn::<@b>(key: K, val: V, m: Self.&mut::<@b>) -> () { };
    }
};

static get_or_default = fn::<@a, K, V>(
    key: K, val: V, map: Map::<K, V>.&mut::<@a>
) -> V.&mut::<@a> {
    match map.get(key) {
        ::Some(v) => v,
        ::None => {
            map.insert(key, val);
            map.get(key).unwrap()
        },
    }
};
"#,
        expect![[r#""#]],
    );
}

// ---- region obligations survive wrapping (joins, arrays, records) ------

#[test]
fn a_join_cannot_launder_an_outlives_obligation() {
    // Wrapping a borrow in an `if` must not erase the obligation its use
    // site would otherwise impose: join leaves go through the reborrow
    // relation, never region-blind `unify`, so the control and the joined
    // form agree.
    let joined = "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {\n\
                      let r: usize.&::<@a> = if c { p } else { p };\n\
                      r\n\
                  };";
    let control = "static f = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {\n\
                       let r: usize.&::<@a> = p;\n\
                       r\n\
                   };";
    check_diagnostics(
        joined,
        expect![[r#"
        100..101: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
    "#]],
    );
    check_diagnostics(
        control,
        expect![[r#"
        84..85: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
    "#]],
    );
}

#[test]
fn a_join_cannot_launder_an_escaping_borrow() {
    // The escaping-borrow shape of the same rule: both branches return a
    // borrow of a body-local, so the joined form must be rejected too —
    // not left for the interpreter's dangling-pointer check alone to
    // catch.
    check_diagnostics(
        "static leak = fn::<@a>(c: bool) -> usize.&::<@a> {\n\
             let x: usize = 7;\n\
             if c { x.& } else { x.& }\n\
         };",
        expect![[r#"
            76..79: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
            89..92: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

#[test]
fn a_meet_in_the_shorter_position_constrains() {
    // `@b + @c` names the OVERLAP of two universals, and the only way
    // another universal can be known to cover it is to cover a member —
    // the element-set model says the intersection is empty, so
    // propagation alone demands nothing on its own.
    check_diagnostics(
        "static f = fn::<@a, @b, @c>(x: usize.&::<@a>) -> usize.&::<@b + @c> { x };",
        expect![[r#"
            70..71: using this borrow where a longer-lived one is expected needs `@a` to outlive the overlap of `@b + @c`, which this signature does not declare; declaring `@a: @b` or `@a: @c` would be enough
        "#]],
    );
    // Declaring either member is enough — the meet is shorter than both.
    check_diagnostics(
        "static f = fn::<@a: @b, @b, @c>(x: usize.&::<@a>) -> usize.&::<@b + @c> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn the_escape_check_sees_join_regions() {
    // The escape check must also catch a borrow whose region is a join,
    // not only a plain region — a join is not itself a solver node, so a
    // body-local borrow returned at `@b + @c` needs no helper function to
    // reach it.
    check_diagnostics(
        "static leak = fn::<@b, @c>() -> usize.&::<@b + @c> {\n\
             let n: usize = 7;\n\
             n.&\n\
         };",
        expect![[r#"
            71..74: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@b`, which outlives the body
        "#]],
    );
}

#[test]
fn an_unknown_region_in_a_signature_says_so() {
    // An unknown region name is reported as such — never as an internal
    // "no error was reported" contradiction — including inside a join.
    check_diagnostics(
        "static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@nope> { x };",
        expect![[r#"
            51..56: no region named `@nope` is in scope; declare it in the binder (`fn::<@nope>`)
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@a>) -> usize.&::<@b + @nope> { x };",
        expect![[r#"
            60..65: no region named `@nope` is in scope; declare it in the binder (`fn::<@nope>`)
        "#]],
    );
}

#[test]
fn blame_lands_on_the_obligation_that_introduced_the_element() {
    // The report must blame the edge that actually introduced the
    // region element, not merely the first edge that has this region as
    // its longer side. Line 3 is the culprit here; line 2 is benign.
    check_diagnostics(
        "static f = fn::<@a, @b>(p: usize.&::<@a>) -> usize.&::<@b> {\n\
             let r: usize.&::<@a> = p;\n\
             r\n\
         };",
        expect![[r#"
            87..88: using this borrow where a longer-lived one is expected needs `@a` to outlive `@b`, which this signature does not declare; add `@a: @b` to the binder
        "#]],
    );
}

#[test]
fn a_nested_fn_literal_may_infer_its_regions() {
    // Three intentional rules compose into what would otherwise be a
    // cliff: a nested literal cannot declare a binder, `@_` was refused in
    // every parameter position, and nothing is elided — so no function
    // literal in a body could take a borrow parameter at all. A nested
    // literal's regions genuinely are body-local existentials, so `@_` is
    // the true thing to say about them.
    check_diagnostics(
        "static main = fn::<@a>(p: usize.&::<@a>) -> usize {\n\
             let f = fn (r: usize.&::<@_>) -> usize { r.* };\n\
             f(p)\n\
         };",
        expect![[r#""#]],
    );
    // An ITEM's signature still refuses it — its regions are parameters.
    check_diagnostics(
        "static f = fn (r: usize.&::<@_>) -> usize { r.* };",
        expect![[r#"
            28..30: `@_` cannot be used in a signature — declare the region in the binder (`fn::<@a>`) and name it here
        "#]],
    );
}

#[test]
fn a_nested_fn_literals_own_frame_escape_is_not_yet_caught() {
    // Known gap, documented at `in_signature_position` and in
    // `docs/main.typ`'s "What is checked, and what is checked yet": the
    // escape check measures a borrow's reach only against the ENCLOSING
    // ITEM's universals. A borrow returned at `@_` from a nested literal
    // escapes that literal's OWN frame without ever reaching a universal
    // of the outer item, so this checks clean even though running it is a
    // dangling-pointer trap (see eval's
    // `a_nested_literal_frame_escape_is_caught_dynamically`).
    check_diagnostics(
        "static main = fn () -> usize {\n\
             let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };\n\
             f().*\n\
         };",
        expect![[r#""#]],
    );
}

#[test]
fn degradation_happens_at_a_join_too() {
    // A join must degrade `&mut` to `&` exactly as the direct form does:
    // an `if` whose branches are a `&` and a `&mut`, consumed by a `&`
    // parameter, is the same reborrow either way.
    check_diagnostics(
        "static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static f = fn::<@a>(p: usize.&::<@a>, m: usize.&mut::<@a>, c: bool) -> usize {\n\
             get(if c { p } else { m })\n\
         };",
        expect![[r#""#]],
    );
}

// ---- region obligations, pinned as a wrapping-invariant property --------

/// The property this pins, stated so it can FAIL: **wrapping a value in a
/// construct must not change which region obligations it incurs.**
///
/// Each case is one context shape, paired with a control that reaches the
/// same context directly. The wrapped and direct forms must produce the
/// same diagnostics — that is the property, and a new seam that agrees two
/// types without relating their regions breaks it here rather than in a
/// user's program. Direct joins, array literals and aggregate join leaves
/// are each their own context shape, which is what makes a new one loud.
///
/// The structural half of the enforcement lives in `constraint.rs`:
/// region-blind `unify` is private, the two callable entry points are
/// `adopt` (one side adopts wholesale) and `relate` (a value meets a
/// context, regions related), and `adopt` debug-asserts its own safety
/// condition on every call in this corpus.
#[track_caller]
fn check_wrapping_changes_nothing(direct: &str, wrapped: &str) {
    let db = RootDatabase::default();
    let render = |text: &str| {
        let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
        crate::file_diagnostics(&db, file)
            .into_iter()
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    // Compared on the OBLIGATION, not the wording. A borrow under a type
    // constructor is legitimately related invariantly where a bare one is
    // reborrowed, so the "because" clause can differ while the demand —
    // which region must outlive which, and which borrow escapes — must
    // not. That demand is the property; the prose is not.
    let obligation = |message: String| match message.find("needs `") {
        Some(at) => message[at..].to_owned(),
        None => message,
    };
    let direct_messages: Vec<String> = render(direct).into_iter().map(obligation).collect();
    let wrapped_messages: Vec<String> = render(wrapped).into_iter().map(obligation).collect();
    assert!(
        !direct_messages.is_empty(),
        "the control must actually report something, or the case proves nothing"
    );
    assert_eq!(
        direct_messages, wrapped_messages,
        "wrapping changed which obligations were incurred — a region-blind \
         seam has reappeared\n  direct:  {direct_messages:?}\n  wrapped: {wrapped_messages:?}"
    );
}

#[test]
fn no_context_shape_launders_a_region_obligation() {
    let sig = "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {";
    // An `if`.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} if c {{ p }} else {{ p }} }};"),
    );
    // A `match` — the same join machinery through another spelling.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} match c {{ _ => p }} }};"),
    );
    // A `loop`/`break` value.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} loop {{ break p }} }};"),
    );
    // An unannotated ARRAY literal, indexed. The literal's elements are a
    // join too, and an eager fast path must not skip it.
    check_wrapping_changes_nothing(
        "static g = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> { p };",
        "static g = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {\n\
             let arr = [p, p];\n\
             arr[0]\n\
         };",
    );
    // A borrow nested inside a RECORD, joined. The function context
    // is load-bearing: projecting `.x` off a joined value hits a
    // pre-existing deferred-join limitation and would mask this.
    let getx = "static getx = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x };\n";
    check_wrapping_changes_nothing(
        &format!(
            "{getx}static h = fn::<@a, @b>(q: usize.&::<@b>) -> usize.&::<@a> {{\n\
                 getx(struct {{ x = q }})\n\
             }};"
        ),
        &format!(
            "{getx}static h = fn::<@a, @b>(q: usize.&::<@b>, c: bool) -> usize.&::<@a> {{\n\
                 getx(if c {{ struct {{ x = q }} }} else {{ struct {{ x = q }} }})\n\
             }};"
        ),
    );
    // A borrow under a NOMINAL type's generic argument. `Ty::Record` above
    // and `Ty::Named` here are the same obligation one constructor apart,
    // but the region walkers stopped
    // at `Record` — a generic type constructor was a laundry, and a local
    // escaped through `Opt::<usize.&>` with no diagnostic at all. Three
    // walkers had the hole (`relate_type_regions`, `freshen_regions`,
    // `substitute_regions`) plus the widening edge; one shape pins them.
    let opt = "type Opt = enum::<T> { Some(T), None };\n";
    check_wrapping_changes_nothing(
        &format!(
            "{opt}static k = fn::<@a, @b>(q: usize.&::<@b>) -> Opt::<usize.&::<@a>> {{\n\
                 Opt::<usize.&::<@b>>::Some(q)\n\
             }};"
        ),
        &format!(
            "{opt}static k = fn::<@a, @b>(q: usize.&::<@b>, c: bool) -> Opt::<usize.&::<@a>> {{\n\
                 if c {{ Opt::<usize.&::<@b>>::Some(q) }} \
                 else {{ Opt::<usize.&::<@b>>::Some(q) }}\n\
             }};"
        ),
    );
    // The MEMBER-CALL shape. A dot-call's receiver is checked against
    // the member's `Self` parameter through the same `check`, so it must
    // incur the same obligation the plain call does — and the member's own
    // `@b` must be a fresh existential at the call, not a rigid param that
    // the outlives solver would read as whichever universal of the CALLER
    // sits at the same binder index.
    let cell = "type Cell = struct { n: usize } with {\n\
                    impl Self {\n\
                        pass = fn::<@b>(p: usize.&::<@b>, m: Self.&::<@b>) -> usize.&::<@b> { p };\n\
                    }\n\
                };\n";
    check_wrapping_changes_nothing(
        &format!(
            "{cell}static m1 = fn::<@a>(c: Cell.&::<@a>) -> usize.&::<@a> {{\n\
                 let local: usize = 5;\n\
                 Cell::pass(local.&, c)\n\
             }};"
        ),
        &format!(
            "{cell}static m1 = fn::<@a>(c: Cell.&::<@a>) -> usize.&::<@a> {{\n\
                 let local: usize = 5;\n\
                 c.pass(local.&)\n\
             }};"
        ),
    );
}

#[test]
fn an_array_literal_of_borrows_cannot_launder_an_escape() {
    // r01/r02/r03. The eager array fast path agreed its elements with
    // region-blind unification and committed, so the join solver — the one
    // thing that reborrows a leaf into its context — never ran. A
    // body-local escaped through it and only the interpreter's
    // dangling-pointer check stopped the program built on it.
    check_diagnostics(
        "static leak = fn::<@a>(p: usize.&::<@a>) -> usize.&::<@a> {\n\
             let n: usize = 7;\n\
             let arr = [p, n.&];\n\
             arr[1]\n\
         };",
        expect![[r#"
            92..95: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

#[test]
fn a_joined_record_of_borrows_cannot_launder_an_escape() {
    // r15. The aggregate-leaf half of the same class.
    check_diagnostics(
        "static getx = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x };\n\
         static leak = fn::<@z>(c: bool) -> usize.&::<@z> {\n\
             let n: usize = 7;\n\
             getx(if c { struct { x = n.& } } else { struct { x = n.& } })\n\
         };",
        expect![[r#"
            175..178: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
            203..206: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
        "#]],
    );
}

// ---- match projects through borrows -------------------------------------
//
// The ruling in one sentence: matching a BORROW binds payload sub-place
// borrows; matching an owned place copies or moves, exactly as before.
// There is no new pattern grammar, and no per-binder mode — the
// scrutinee's flavor decides.
//
// The tests split three ways, and the split is the argument:
//
//   * what a binder is TYPED as, through each flavor and through nesting;
//   * what REGION obligations the projection incurs (the soundness half —
//     a payload borrow may never outlive its scrutinee's loan);
//   * that the OWNED path is untouched, diagnostics included.

const OPT: &str = "type Opt = enum::<T> { Some(T), None };\n";

#[test]
fn a_shared_borrowed_match_binds_shared_payload_borrows() {
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {\n\
             match s { ::Some(t) => t.*, ::None => 0 }\n\
         };",
        expect![[r#"
            51..139 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&::<@a>) -> usize
            60..61 's': Opt::<usize>.&::<@a>
            94..139 '{ match s { ::Som...': usize
            96..137 'match s { ::Some(...': usize
            102..103 's': Opt::<usize>.&::<@a>
            113..114 't': usize.&
            119..120 't': usize.&
            119..122 't.*': usize
            134..135 '0': usize
        "#]],
    );
}

#[test]
fn an_exclusive_borrowed_match_binds_exclusive_payload_borrows() {
    // The flavor is inherited, not chosen: `.&mut` in, `.&mut` out. That
    // is what makes the binding writable, and it is also why there is no
    // binder marker — there would be nothing for one to say.
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&mut::<@a>) -> () {\n\
             match s { ::Some(t) => { t.* = 5; }, ::None => {} }\n\
         };",
        expect![[r#"
            51..149 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&mut::<@a>)
            60..61 's': Opt::<usize>.&mut::<@a>
            94..149 '{ match s { ::Som...': ()
            96..147 'match s { ::Some(...': ()
            102..103 's': Opt::<usize>.&mut::<@a>
            113..114 't': usize.&mut
            119..131 '{ t.* = 5; }': ()
            121..122 't': usize.&mut
            121..124 't.*': usize
            127..128 '5': usize
            143..145 '{}': ()
        "#]],
    );
}

#[test]
fn a_borrowed_match_binds_every_payload_of_the_variant() {
    // Two payloads, two independent borrows of two disjoint sub-places.
    check_infer(
        "type Pair = enum { Both(usize, str), Neither };\n\
         static f = fn::<@a>(p: Pair.&mut::<@a>) -> usize {\n\
             match p { ::Both(x, y) => x.*, ::Neither => 0 }\n\
         };",
        expect![[r#"
            59..148 'fn::<@a>(p: Pair....': fn(Pair.&mut::<@a>) -> usize
            68..69 'p': Pair.&mut::<@a>
            97..148 '{ match p { ::Bot...': usize
            99..146 'match p { ::Both(...': usize
            105..106 'p': Pair.&mut::<@a>
            116..117 'x': usize.&mut
            119..120 'y': str.&mut
            125..126 'x': usize.&mut
            125..128 'x.*': usize
            143..144 '0': usize
        "#]],
    );
}

#[test]
fn a_whole_value_binder_on_a_borrowed_scrutinee_stays_the_borrow() {
    // A bare bind names the very same place the scrutinee does, so there
    // is nothing to project and nothing to shorten: it gets the borrow
    // back, at the scrutinee's own region. (This is also exactly what it
    // got before the projection existed, when a borrowed scrutinee was
    // classified as "some other type".)
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> Opt::<usize>.&::<@a> {\n\
             match s { whole => whole }\n\
         };",
        expect![[r#"
            51..139 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&::<@a>) -> Opt::<usize>.&::<@a>
            60..61 's': Opt::<usize>.&::<@a>
            109..139 '{ match s { whole...': Opt::<usize>.&::<@a>
            111..137 'match s { whole =...': Opt::<usize>.&::<@a>
            117..118 's': Opt::<usize>.&::<@a>
            121..126 'whole': Opt::<usize>.&::<@a>
            130..135 'whole': Opt::<usize>.&::<@a>
        "#]],
    );
}

#[test]
fn a_variant_typed_borrowed_scrutinee_projects_too() {
    // Tag-free at runtime, so there is no dispatch — but the binder is
    // still a borrow of the payload slot, and it is still writable.
    check_infer(
        "type State = enum { Run(usize), Stop };\n\
         static f = fn::<@a>(s: State::Run.&mut::<@a>) -> () {\n\
             match s { ::Run(n) => { n.* = n.* + 1; } }\n\
         };",
        expect![[r#"
            51..138 'fn::<@a>(s: State...': fn(State::Run.&mut::<@a>)
            60..61 's': State::Run.&mut::<@a>
            92..138 '{ match s { ::Run...': ()
            94..136 'match s { ::Run(n...': ()
            100..101 's': State::Run.&mut::<@a>
            110..111 'n': usize.&mut
            116..134 '{ n.* = n.* + 1; }': ()
            118..119 'n': usize.&mut
            118..121 'n.*': usize
            124..125 'n': usize.&mut
            124..127 'n.*': usize
            124..131 'n.* + 1': usize
            130..131 '1': usize
        "#]],
    );
}

#[test]
fn projection_is_transitive_through_nested_matches() {
    // "All the way down" — spelled as two matches, because the binding IS
    // a borrow and so matching IT projects again. (Nested PATTERNS are not
    // grammar yet, so `::Some(::Pair(a, b))` has nothing to parse into;
    // this is the shape that expresses the same rule today.)
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(o: Opt::<Opt::<usize>>.&::<@a>) -> usize {{\n\
                 match o {{\n\
                     ::Some(inner) => match inner {{ ::Some(n) => n.*, ::None => 0 }},\n\
                     ::None => 0,\n\
                 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_owners_project_in_checks_clean() {
    // The shape the projection exists for: `Opt::<T>.&::<@a>` in,
    // `Opt::<T.&::<@a>>` out — an `as_ref`-shaped function, in the
    // surface language.
    //
    // What it proves is the REGION half. The payload binder's region is a
    // fresh variable with one upper bound, `@a`, so it is *allowed* to be
    // shorter and *able* to be exactly `@a`. Had the projection pinned the
    // binder to something shorter, this signature would be unwritable; had
    // it emitted no edge at all, the two rejections below would not fire.
    //
    // FLAGGED — the arm bodies name the enum instead of writing the elided
    // `::Some(t)`. That is not about borrows: a match arm's body is a JOIN
    // LEAF, which by design has no expected type to read the sigil against
    // (`an_elided_variant_expression_is_refused_in_a_join_position`), and
    // the owned twin below fails identically. Lifting it is the second
    // expectation channel that test names, and it is a separate ruling.
    check_diagnostics(
        &format!(
            "{OPT}static project_in = fn::<@a, T>(s: Opt::<T>.&::<@a>) -> Opt::<T.&::<@a>> {{\n\
                 match s {{\n\
                     ::Some(t) => Opt::<T.&::<@a>>::Some(t),\n\
                     ::None => Opt::<T.&::<@a>>::None,\n\
                 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_elided_sigil_in_an_arm_body_fails_the_same_way_owned() {
    // The control for the flag above: the join-leaf limitation is not
    // something the projection introduced. Borrowed and owned spellings of
    // the same program produce the same two rejections, in the same
    // places.
    let borrowed = format!(
        "{OPT}static f = fn::<@a, T>(s: Opt::<T>.&::<@a>) -> Opt::<T.&::<@a>> {{\n\
             match s {{ ::Some(t) => ::Some(t), ::None => ::None }}\n\
         }};"
    );
    let owned = format!(
        "{OPT}static f = fn::<T>(s: Opt::<T>) -> Opt::<T> {{\n\
             match s {{ ::Some(t) => ::Some(t), ::None => ::None }}\n\
         }};"
    );
    let db = RootDatabase::default();
    let messages = |text: &str| {
        let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
        crate::file_diagnostics(&db, file)
            .into_iter()
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    let borrowed = messages(&borrowed);
    assert_eq!(
        borrowed,
        vec![
            "cannot resolve `::Some` without an expected type — write `Enum::Some`".to_owned(),
            "cannot resolve `::None` without an expected type — write `Enum::None`".to_owned(),
        ]
    );
    assert_eq!(borrowed, messages(&owned));
}

// ---- the region half: a payload borrow can never outlive its scrutinee --

#[test]
fn a_payload_borrow_cannot_outlive_the_scrutinees_loan() {
    // The projection emits ONE directed edge, `@scrutinee: @payload`, and
    // this is it failing. Without the edge the body would hand the caller
    // a borrow good for `@a` derived from one only good for `@b` — the
    // exact unsoundness the edge exists to rule out.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
        expect![[r#"
            114..115: matching this borrow to bind its payloads needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn a_declared_bound_makes_the_projection_legal() {
    // The same program with the guarantee the message asked for. This is
    // the other half of the edge being real: it is checkable, so declaring
    // the relation satisfies it.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b: @a>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn a_payload_borrow_of_a_local_cannot_escape_the_body() {
    // The escape check, reached THROUGH the projection: the scrutinee
    // borrows a body-local, the payload borrow is returned, and the chain
    // `@local ⊇ @payload ⊇ @a` makes the local's loan reach a universal's
    // end. Blamed at the borrow, which is the operation that cannot be
    // honored.
    check_diagnostics(
        &format!(
            "{OPT}static leak = fn::<@a>() -> usize.&::<@a> {{\n\
                 let o: Opt::<usize> = Opt::<usize>::Some(5);\n\
                 match o.& {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
        expect![[r#"
            135..138: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

#[test]
fn a_borrowed_match_cannot_launder_a_region_obligation() {
    // No laundering through a projection. Returning a `@b` borrow where
    // `@a` is promised is refused without `@b: @a`, and reaching `@a`
    // through a borrowed match's binder must incur exactly the obligation
    // reaching it directly does. A projection that forgot its edge, or
    // that adopted instead of relating, would let the wrapped spelling
    // pass where the direct one is refused — a hole in the region graph
    // that no error message would name.
    check_wrapping_changes_nothing(
        &format!("{OPT}static f = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {{ p }};"),
        &format!(
            "{OPT}static f = fn::<@a, @b>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
    );
}

// ---- the owned path, unchanged -----------------------------------------

#[test]
fn an_owned_match_still_moves_a_noncopyable_payload_out() {
    // The byte-identity claim, at its sharpest point. `usize.&mut` is the
    // one affine type there is, so this program is only possible if the
    // owned path still MOVES: a projection would have handed back
    // `usize.&mut.&mut`, and reading through a borrow of it would be
    // "cannot move out of a borrow". Clean means the owned path did not
    // move.
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@b>(o: Opt::<usize.&mut::<@b>>) -> usize {\n\
             match o { ::Some(t) => t.*, ::None => 0 }\n\
         };",
        expect![[r#"
            51..142 'fn::<@b>(o: Opt::...': fn(Opt::<usize.&mut::<@b>>) -> usize
            60..61 'o': Opt::<usize.&mut::<@b>>
            97..142 '{ match o { ::Som...': usize
            99..140 'match o { ::Some(...': usize
            105..106 'o': Opt::<usize.&mut::<@b>>
            116..117 't': usize.&mut::<@b>
            122..123 't': usize.&mut::<@b>
            122..125 't.*': usize
            137..138 '0': usize
        "#]],
    );
}

#[test]
fn a_borrowed_match_of_a_noncopyable_payload_refuses_the_read() {
    // And the contrast that proves the two paths are different: the SAME
    // enum behind a borrow binds `usize.&mut.&`, and copying the payload
    // OUT of that would duplicate the exclusive permission. Refused by the
    // ordinary safe-`.*` rule — no new diagnostic was needed.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(o: Opt::<usize.&mut::<@b>>.&::<@a>) -> usize {{\n\
                 match o {{ ::Some(t) => {{ let c = t.*; 0 }}, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            144..147: cannot move out of a borrow: `usize.&mut::<@b>` cannot be copied
        "#]],
    );
    // Reading THROUGH the binding is not that: `t.*.*` projects, and only
    // the `usize` at the end is copied, so it is legal exactly as
    // `b.*.*` on any other doubly-borrowed place is.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(o: Opt::<usize.&mut::<@b>>.&::<@a>) -> usize {{\n\
                 match o {{ ::Some(t) => t.*.*, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn a_borrow_where_the_owned_value_is_wanted_names_both_ways_out() {
    // The hint is gated on shape alone — found is a borrow of exactly the
    // type wanted — so it is language-wide: a plain typed `let` gets it,
    // with no `match` anywhere. Both routes out are named, because which
    // one is right depends on the referent: `.*` for a copyable one, the
    // owned value for anything that has to move.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let x: usize = r; x };",
        expect![[r#"
            64..65: type mismatch: expected `usize`, found `usize.&::<@a>`; a borrow is not the value — write `.*` to read through it (a copy, so the referent must be copyable), or use the owned value instead of a borrow of it (expected `usize` because of this annotation at 56..61)
        "#]],
    );
    // And its loudest customer: the message someone lands on the first
    // time they match a borrow, where the binder is a borrow and the
    // position wants the value.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {{\n\
                 match s {{ ::Some(t) => t, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            119..120: type mismatch: expected `usize`, found `usize.&`; a borrow is not the value — write `.*` to read through it (a copy, so the referent must be copyable), or use the owned value instead of a borrow of it (expected `usize` because of this return type at 85..93)
        "#]],
    );
}

#[test]
fn a_borrow_of_a_non_matchable_type_is_still_not_a_scrutinee() {
    // The lens is lifted only for a referent this match can DISPATCH on —
    // an enum, a variant, or a `char` — so a `struct.&` (or a `usize.&`)
    // scrutinee reaches exactly the diagnostics it always did, naming the
    // BORROW rather than what is behind it. Nothing here was widened by
    // accident when literal patterns arrived.
    check_diagnostics(
        "type P = struct { x: usize };\n\
         static f = fn::<@a>(p: P.&::<@a>, n: usize.&::<@a>) -> usize {\n\
             match p { ::Some(t) => 1, _ => 0 } + match n { _ => 2 }\n\
         };\n\
         static g = fn::<@a>(n: usize.&::<@a>) -> usize { match n { } };",
        expect![[r#"
            103..112: a variant pattern needs an enum scrutinee; only `_` or a binding can match a `P.&::<@a>`
            201..206: this `match` does not cover every possible `usize.&::<@a>`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_bind_shadowing_a_variant_warns_through_a_borrow_too() {
    // FLAGGED as a deliberate behavior change: before the projection a
    // borrowed scrutinee classified as "some other type", so this warning
    // — a bare binder named like a variant of the scrutinee's enum, which
    // is almost always a stale rename — could not fire through one. It is
    // the same footgun either way, so it now does.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {{\n\
                 match s {{ None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            106..110: `None` binds the whole value; write `::None` (or `Opt::None`) to match the variant (`Opt` is defined here at 5..8)
        "#]],
    );
}

// ---- `extern fn` — host imports ----------------------------------------

#[test]
fn an_extern_fn_has_an_ordinary_fn_type() {
    // The import is deliberately NOT a distinguished type: it is a
    // `fn(...) -> T` like any other, so it is annotatable, passable and
    // callable with nothing new to learn. What makes it an import is the
    // DECLARATION, which is why the checkers ask the item, not the type.
    check_infer(
        "static read = extern fn(buf: u8.&raw mut, len: usize) -> i64;",
        expect![[r#"
            14..60 'extern fn(buf: u8...': fn(u8.&raw mut, usize) -> i64
            24..27 'buf': u8.&raw mut
            42..45 'len': usize
        "#]],
    );
}

#[test]
fn calling_an_extern_fn_outside_unsafe_is_rejected() {
    check_diagnostics(
        "static read = extern fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn(p: u8.&raw mut) -> i64 { read(p, 1) };",
        expect![[r#"
            101..111: calling the host import `read` requires an `unsafe { ... }` block; nothing on this side of the boundary can check what it does
        "#]],
    );
}

#[test]
fn calling_an_extern_fn_in_a_const_context_is_rejected() {
    // Not `NonConstFnCall`: "marking it `const fn` would allow this" is
    // advice that cannot be taken, because `const extern fn` is itself
    // rejected. The honest refusal names the missing host.
    check_diagnostics(
        "static read = extern fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = const fn(p: u8.&raw mut) -> i64 { unsafe { read(p, 1) } };",
        expect![[r#"
            116..120: cannot call the host import `read` in a const context; there is no host at compile time (this `const fn` is always a const context at 73..78)
        "#]],
    );
}

#[test]
fn taking_an_extern_fn_as_a_value_requires_unsafe() {
    // `let f = read; f(buf, 8)` used to reach the host with no marker
    // anywhere: the call site says only that SOMETHING is being called, so
    // the last place a reader can see which import is in play is where the
    // value is taken. That is where the marker goes — imports stay
    // first-class, they are just priced.
    check_diagnostics(
        "static read = extern fn(buf: u8.&raw mut, len: usize) -> isize;\n\
         static loose = fn() -> () { let f = read; };\n\
         static vouched = fn() -> () { let f = unsafe { read }; };\n\
         static called = fn(p: u8.&raw mut) -> isize { unsafe { read(p, 1) } };",
        expect![[r#"
            100..104: taking the host import `read` as a value requires an `unsafe { ... }` block; a value can be called from anywhere, so vouching happens where it is taken
        "#]],
    );
}

#[test]
fn an_extern_fn_with_a_body_is_not_a_host_import() {
    // The syntax error stands (see `syntax`'s own test); what must not
    // happen is the item becoming an import ANYWAY, which would turn a
    // written body into a run-time refusal naming a boundary the program
    // never crossed. The body wins: an import exists only where the
    // declaration is well formed, so neither the `unsafe` rule nor the
    // const rule fires here.
    check_diagnostics(
        "static bad = extern fn(n: i64) -> i64 { n };\n\
         static f = fn() -> i64 { bad(1) };\n\
         static g = const fn() -> i64 { bad(1) };",
        expect![[r#"
            38..43: an `extern fn` declares a host import and has no body; the implementation lives on the other side of the boundary
            111..114: cannot call `bad` in a const context; marking it `const fn` would allow this (`bad` is defined here at 7..10) (this `const fn` is always a const context at 91..96)
        "#]],
    );
}

#[test]
fn a_misplaced_extern_fn_is_not_a_host_import() {
    // Same rule as the written body, for the other half of a well-formed
    // declaration: an import's name IS its item's name, so an `extern fn`
    // that is not a plain `static`'s initializer has no name to import
    // under. Lowering it as an import anyway would refuse at run time under
    // the ENCLOSING item's name. It lowers as an ordinary fn literal with a
    // missing body instead — exactly what dropping `extern` would give.
    check_diagnostics(
        "static outer = fn() -> i64 { let f = extern fn(n: i64) -> i64; f(1) };\n\
         const copied = extern fn(n: i64) -> i64;",
        expect![[r#"
            37..43: an `extern fn` must be a `static`'s initializer — `static name = extern fn(...) -> T;` — because the item's name is the name the host is asked for
            86..92: an `extern fn` must be a `static`'s initializer — `static name = extern fn(...) -> T;` — because the item's name is the name the host is asked for
        "#]],
    );
}

// ---- the blesses: bytes into `str` --------------------------------------

#[test]
fn user_declarations_shadow_the_builtin_utf8_result() {
    // The `AllocResult`/`ReadLineResult` rule, once more: a file that
    // declares the name sees its own everywhere.
    check_infer(
        "type Utf8Result = struct { tag: usize };\n\
         static x = Utf8Result(struct { tag = 1 });",
        expect![[r#"
            52..62 'Utf8Result': fn(struct { tag: usize }) -> Utf8Result
            52..82 'Utf8Result(struct...': Utf8Result
            63..81 'struct { tag = 1 }': struct { tag: usize }
            78..79 '1': usize
        "#]],
    );
}

#[test]
fn both_blesses_require_unsafe_and_pin_their_pointee_to_u8() {
    // `unsafe` is about the POINTER in both spellings — that it addresses
    // `len` readable bytes is the caller's unchecked claim, so "checked"
    // names only whether the bytes spell UTF-8. And the pointee is `u8`,
    // not `T`: this boundary is bytes-first, and a bless over some other
    // element type would be a layout claim, not a text one. The first call
    // is the clean one: `p` is a `u8.&raw`, so it also pins that either raw
    // flavor is accepted — the reason neither bless is first-class.
    check_diagnostics(
        "static f = fn(p: u8.&raw, q: usize.&raw mut) -> () {\n\
             unsafe { str_from_utf8(p, 1); };\n\
             str_from_utf8(q, 1);\n\
             str_from_utf8_unchecked(p, 1);\n\
             unsafe { str_from_utf8_unchecked(q, 1); };\n\
         };",
        expect![[r#"
            86..105: calling `str_from_utf8` requires an `unsafe { ... }` block
            100..101: type mismatch: expected `u8.&raw mut`, found `usize.&raw mut`
            107..136: calling `str_from_utf8_unchecked` requires an `unsafe { ... }` block
            171..172: type mismatch: expected `u8.&raw mut`, found `usize.&raw mut`
        "#]],
    );
}

#[test]
fn a_bless_is_not_a_first_class_value() {
    // Same reason `copy` is not: its pointer parameter accepts either raw
    // flavor, which no one `fn` type says.
    check_diagnostics(
        "static f = fn() -> () { let g = str_from_utf8; };",
        expect![[r#"
            32..45: `str_from_utf8` must be called directly; its pointer parameter accepts both `T.&raw` and `T.&raw mut`, so it has no one function type to be a value at
        "#]],
    );
}

#[test]
fn both_blesses_are_const_legal() {
    // A bless has no effect for a const context to refuse — the pointer
    // builtins' reason, and `unsafe` is orthogonal to `const`.
    check_diagnostics(
        "static empty = const { unsafe { str_from_utf8_unchecked(dangling::<u8>(), 0) } };\n\
         static checked = const {\n\
             match unsafe { str_from_utf8(dangling::<u8>(), 0) } {\n\
                 Utf8Result::Ok(s) => s,\n\
                 Utf8Result::Err => \"not utf-8\",\n\
             }\n\
         };",
        expect![""],
    );
}

#[test]
fn str_bytes_requires_unsafe_and_both_str_primitives_are_const_legal() {
    // `str_bytes` is the only one of the two that touches raw memory, and
    // it WRITES: that `dst` addresses `s.len()` writable bytes is the
    // caller's unchecked claim, so it carries `copy`'s destination gate.
    // `len` reads a value it was handed and carries none. Const-legality is
    // orthogonal to `unsafe` and is `next_char`'s reason for both: neither
    // observes anything outside its arguments.
    check_diagnostics(
        "static f = fn(p: u8.&raw mut) -> () { str_bytes(\"hi\", p); };\n\
         static n = const { \"hi\".len() };\n\
         static z = const { unsafe { str_bytes(\"\", dangling::<u8>()) } };",
        expect![[r#"
            38..56: calling `str_bytes` requires an `unsafe { ... }` block
        "#]],
    );
}

// ---- linear types: the `forget` capability and must-consume checking ----

/// The shape every test below builds on: a nominal type that has shed
/// `forget`, plus a consuming `drop` that takes it apart. `drop` needs no
/// compiler support — destructuring hands the obligation to the parts, and
/// the parts are two integers.
const LINEAR_PRELUDE: &str = r#"
type Res = struct { id: usize, size: usize } without forget with {
    impl Self {
        drop = fn(r: Self) -> () {
            let Res(struct { id, size }) = r;
            print_two(id, size);
        };
    }
};
static print_two = fn(a: usize, b: usize) -> () { };
static make = fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
"#;

fn check_linear(body: &str, expect: Expect) {
    check_diagnostics(&format!("{LINEAR_PRELUDE}{body}"), expect);
}

#[test]
fn a_linear_consumed_on_the_one_path_is_clean() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn a_linear_left_alive_at_the_end_of_a_scope_is_a_leak() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
};
"#,
        expect![[r#"
            366..390: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 376..377)
        "#]],
    );
}

#[test]
fn consuming_a_linear_twice_is_the_use_after_move_error() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    r.drop();
    r.drop();
};
"#,
        expect![[r#"
            407..408: `r` was already consumed (`r` is born here and must be consumed at 376..377) (first consumed here at 393..394)
        "#]],
    );
}

#[test]
fn a_linear_consumed_in_one_arm_only_fails_at_the_join() {
    check_linear(
        r#"
static main = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { };
};
"#,
        expect![[r#"
            400..427: `r` is consumed on some paths through this expression and not on others (`r` is born here and must be consumed at 383..384)
        "#]],
    );
}

#[test]
fn both_arms_consuming_is_clean_and_a_diverging_arm_needs_nothing() {
    check_linear(
        r#"
static both = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { r.drop(); };
};
static diverging = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { panic("no"); };
};
"#,
        expect![""],
    );
}

#[test]
fn a_diverging_tail_owes_nothing_and_a_falling_one_still_does() {
    check_linear(
        r#"
static nope = fn() -> ! { panic("n") };
static panicking = fn() -> usize {
    let r = make(1);
    panic("gone")
};
static returning = fn() -> usize {
    let r = make(1);
    return panic("gone")
};
static nested = fn() -> usize {
    let r = make(1);
    { panic("gone") }
};
static by_name = fn() -> usize {
    let r = make(1);
    nope()
};
static branching = fn(c: bool) -> usize {
    let r = make(1);
    if c { panic("a") } else { panic("b") }
};
static looping = fn() -> usize {
    let r = make(1);
    loop {}
};
static falling = fn() -> usize {
    let r = make(1);
    1
};
"#,
        expect![[r#"
            898..928: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 908..909)
        "#]],
    );
}

#[test]
fn returning_a_linear_consumes_it_and_leaving_one_behind_does_not() {
    check_linear(
        r#"
static handed_back = fn() -> Res {
    let r = make(1);
    r
};
static returned_early = fn(c: bool) -> () {
    let r = make(1);
    if c { return; };
    r.drop();
};
"#,
        expect![[r#"
            482..488: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 458..459)
        "#]],
    );
}

#[test]
fn a_discarded_linear_temporary_is_reported_without_a_name() {
    check_linear(
        r#"
static main = fn() -> () {
    make(1);
};
"#,
        expect![[r#"
            372..379: this value must be consumed; its type has no `forget` capability, so it cannot be discarded
        "#]],
    );
}

#[test]
fn a_static_cannot_hold_a_linear() {
    check_linear(
        r#"
static held = Res(struct { id = 1, size = 2 });
"#,
        expect![[r#"
            355..387: an item's value must have the `forget` capability: a `static` is never destroyed, so nothing could ever consume this
        "#]],
    );
}

#[test]
fn assigning_over_a_live_linear_loses_it() {
    check_linear(
        r#"
static main = fn() -> () {
    let mut r = make(1);
    r = make(2);
    r.drop();
};
"#,
        expect![[r#"
            397..398: `r` still holds a value that must be consumed; assigning here would lose it (`r` is born here and must be consumed at 380..381)
        "#]],
    );
}

#[test]
fn assigning_over_a_linear_place_with_no_name_loses_it_too() {
    // The same loss where no binding names the place — a field, an
    // element, a `.&mut` referent. Liveness cannot say the old value is
    // already gone (there is nothing to track it on) and the TYPE says one
    // is there, so the write always loses one.
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static overwrite_field = fn::<@a>(h: Holder.&mut::<@a>) -> () { h.*.res = make(2); };
static overwrite_elem = fn::<@a>(a: [Res; 3].&mut::<@a>) -> () { a.*[0] = make(2); };
static overwrite_referent = fn::<@a>(m: Res.&mut::<@a>) -> () { m.* = make(2); };
"#,
        expect![[r#"
            452..459: this place still holds a value that must be consumed; assigning here would lose it
            539..545: this place still holds a value that must be consumed; assigning here would lose it
            624..627: this place still holds a value that must be consumed; assigning here would lose it
        "#]],
    );
    // The raw hatch is the one way past it, where it always was: writing
    // through a raw pointer is `unsafe`, and what it does to an obligation
    // is the caller's word.
    check_linear(
        r#"
static overwrite_raw = fn(p: Res.&raw mut) -> () { unsafe { p.* = make(2); }; };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_const_block_may_not_produce_a_linear() {
    // A `const { ... }` is a CONSTANT — computed once and copied into
    // every evaluation — so a linear one is one obligation per evaluation
    // and no path discharges it once. `ItemHoldsLinear`'s reason, one
    // nesting level down, and not covered by the leaked-INSIDE-the-block
    // case: here the block itself hands the value out.
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static main = fn() -> () {
    let r = const { const_make(1) };
    r.drop();
};
"#,
        expect![[r#"
            461..484: a `const` block's value must have the `forget` capability: it is computed once and copied into every evaluation, so no single path could consume it
        "#]],
    );
}

#[test]
fn a_diverging_const_block_answers_for_the_type_it_was_pinned_to() {
    // The block's TYPE decides this, not a path through it — the same
    // question the item-level twin asks. A diverging body is no excuse:
    // pinned by an annotation the block still promises a `Res` per
    // evaluation, while unpinned its own type is `!`, which anyone may
    // forget. The binding is not reported either way: nothing reaches it.
    check_linear(
        r#"
static main = fn() -> usize {
    let pinned: Res = const { panic("x") };
    1
};
static free = fn() -> usize {
    let loose = const { panic("y") };
    1
};
"#,
        expect![[r#"
            393..413: a `const` block's value must have the `forget` capability: it is computed once and copied into every evaluation, so no single path could consume it
        "#]],
    );
}

#[test]
fn a_static_holding_a_const_block_says_it_once() {
    // Both rules fire on the same range here — the block's and the item's,
    // which is the block's one level up. One mistake gets one sentence, and
    // it is the item's: a `static` is never destroyed, which is the reason
    // a reader needs.
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static held = const { const_make(1) };
"#,
        expect![[r#"
            436..459: an item's value must have the `forget` capability: a `static` is never destroyed, so nothing could ever consume this
        "#]],
    );
}

#[test]
fn a_borrow_of_a_linear_consumes_nothing_and_a_copy_out_is_refused() {
    check_linear(
        r#"
static peek = fn::<@a>(r: Res.&::<@a>) -> usize { r.*.id };
static main = fn() -> () {
    let r = make(1);
    let n = peek(r.&);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn containment_infects_a_container_and_a_rest_pattern_cannot_skip_it() {
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static take = fn(h: Holder) -> () {
    let Holder(struct { res, tag }) = h;
    res.drop();
};
static skipped = fn(h: Holder) -> () {
    let Holder(struct { tag, .. }) = h;
};
"#,
        expect![[r#"
            538..556: `..` would skip `res`, which must be consumed; name it in the pattern so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_field_read_of_a_linear_field_is_a_copy_and_is_refused() {
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static main = fn(h: Holder) -> () {
    let r = h.res;
    r.drop();
};
"#,
        expect![[r#"
            422..458: `h` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`h` is born here and must be consumed at 405..406)
            436..441: cannot copy a value that must be consumed out of a place; take the whole value apart instead (`let Name(struct { .. }) = value;`)
        "#]],
    );
}

#[test]
fn matching_an_owned_linear_enum_consumes_it_through_its_payloads() {
    check_linear(
        r#"
type Maybe = enum { One(Res), Nothing };
static main = fn(m: Maybe) -> () {
    match m {
        ::One(r) => { r.drop(); },
        ::Nothing => { },
    };
};
static leaky = fn(m: Maybe) -> () {
    match m {
        ::One(r) => { },
        ::Nothing => { },
    };
};
"#,
        expect![[r#"
            572..575: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 566..567)
        "#]],
    );
}

#[test]
fn a_loop_may_not_consume_a_linear_born_outside_it() {
    check_linear(
        r#"
static main = fn(n: usize) -> () {
    let r = make(1);
    loop {
        r.drop();
    };
};
"#,
        expect![[r#"
            406..431: `r` is left in a different state than the loop found it in; the next iteration would run against a world this body was not checked in (`r` is born here and must be consumed at 384..385)
        "#]],
    );
}

#[test]
fn a_loop_that_replaces_what_it_consumed_is_clean() {
    check_linear(
        r#"
static main = fn(n: usize) -> () {
    let mut r = make(1);
    loop {
        r.drop();
        r = make(2);
        if n == 0 { break; };
    };
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn a_linear_born_and_consumed_inside_a_loop_is_clean_and_a_break_checks_it() {
    check_linear(
        r#"
static clean = fn(n: usize) -> () {
    loop {
        let r = make(1);
        r.drop();
        if n == 0 { break; };
    };
};
static broken = fn(n: usize) -> () {
    loop {
        let r = make(1);
        if n == 0 { break; };
        r.drop();
    };
};
"#,
        expect![[r#"
            564..569: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 531..532)
        "#]],
    );
}

#[test]
fn a_linear_cannot_be_repeated_into_an_array() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    let a = [r; 3];
};
"#,
        expect![[r#"
            366..410: `a` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`a` is born here and must be consumed at 397..398)
            402..403: cannot repeat a value that must be consumed: the copies would each have to be consumed, and there is only one value
        "#]],
    );
}

#[test]
fn a_linear_born_in_a_const_context_must_be_consumed_there() {
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static n: usize = const { let r = const_make(1); 5 };
"#,
        expect![[r#"
            446..474: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 452..453)
        "#]],
    );
}

#[test]
fn the_forget_bound_is_the_default_on_every_type_parameter() {
    // Nothing was written about `T`, so `T` requires `forget` — and the
    // refusal names the bound and the spelling that relaxes it. Note the
    // second diagnostic: containment made `Box::<Res>` linear anyway, so
    // the bound is a promise about the binder, not the safety net.
    check_linear(
        r#"
type Box = enum::<T> { Full(T), Empty };
static main = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
};
"#,
        expect![[r#"
            407..457: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 438..439)
            442..451: `Res` cannot be a `T`: `Res` is declared `without forget`, and `T` requires `forget` (every type parameter does unless it is written `T without forget`)
        "#]],
    );
}

#[test]
fn opting_a_parameter_out_lets_it_carry_a_linear() {
    // `unwrap`'s shape is the reason rigid checking is bearable: inside the
    // body the only things you can do with a `T` you may not forget are
    // hand it back and pass it on — and handing it back is what a container
    // is for.
    check_linear(
        r#"
type Box = enum::<T without forget> { Full(T), Empty } with {
    impl Self {
        unwrap = fn(b: Self) -> T {
            match b {
                ::Full(t) => t,
                ::Empty => panic("empty box"),
            }
        };
    }
};
static clean = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
    let back = b.unwrap();
    back.drop();
};
static leaky = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
};
static forgettable = fn() -> usize {
    let b = Box::<usize>::Full(5);
    b.unwrap()
};
"#,
        expect![[r#"
            738..788: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 769..770)
        "#]],
    );
}

#[test]
fn an_opted_out_parameter_is_checked_rigidly_inside_the_generic_body() {
    // The caller may hand it a linear, so the body may not assume it can
    // drop one on the floor — even though `T` might be `usize`.
    check_linear(
        r#"
static ignore = fn::<T without forget>(t: T) -> () { };
static hand_back = fn::<T without forget>(t: T) -> T { t };
"#,
        expect![[r#"
            392..395: `t` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`t` is born here and must be consumed at 380..381)
        "#]],
    );
}

#[test]
fn an_opted_out_parameter_cannot_satisfy_the_bound_and_is_blamed_as_one() {
    // The blame root for a parameter is not a declaration: `U` was never
    // declared `without forget`, it opted out of the bound every type
    // parameter carries by default, and the sentence has to say so.
    check_linear(
        r#"
type Box = enum::<T> { Full(T), Empty };
static wrap = fn::<U without forget>(u: U) -> () {
    let b = Box::Full(u);
};
"#,
        expect![[r#"
            431..460: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 441..442)
            445..454: `U` cannot be a `T`: `U` is a type parameter written `without forget`, and `T` requires `forget` (every type parameter does unless it is written `T without forget`)
        "#]],
    );
}

#[test]
fn a_match_arm_wildcard_cannot_swallow_an_owned_linear() {
    // The match CONSUMED the scrutinee; `_` bound nothing to consume it
    // with. Its neighbours (`let _ = s`, `::Some(_)`) were caught from the
    // start — this is the same sentence at the third position, and it
    // composes: the nested `match x { _ => {} }` is the same mistake one
    // level down.
    check_linear(
        r#"
type Maybe = enum::<T without forget> { One(T), Nothing };
static direct = fn() -> () {
    let r = make(1);
    match r {
        _ => { },
    };
};
static catch_all = fn(m: Maybe::<Res>) -> () {
    match m {
        _ => { },
    };
};
static nested = fn(m: Maybe::<Res>) -> () {
    match m {
        ::One(x) => {
            match x {
                _ => { },
            };
        },
        ::Nothing => { },
    };
};
"#,
        expect![[r#"
            472..473: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
            561..562: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
            699..700: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_match_arm_wildcard_through_a_borrow_owes_nothing() {
    // Nothing was consumed, so nothing is owed — the scrutinee decides,
    // exactly as match projection already rules.
    check_linear(
        r#"
static peek = fn::<@a>(r: Res.&::<@a>) -> usize {
    match r {
        _ => { 0 },
    }
};
static main = fn() -> () {
    let r = make(1);
    let n = peek(r.&);
    r.drop();
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_break_carries_its_state_out_instead_of_restoring_the_loop() {
    // A break is not a back edge: it LEAVES. So the take-ownership-and-stop
    // search loop — the shape every search is written in — checks clean,
    // while falling off the body's end (which runs it again) still has to
    // leave the world as it found it.
    check_linear(
        r#"
static takes_and_stops = fn(n: usize) -> () {
    let r = make(1);
    let mut i = 0;
    loop {
        i = i + 1;
        if i > n {
            r.drop();
            break;
        };
    };
};
static nested_break = fn(n: usize) -> () {
    let r = make(1);
    let mut i = 0;
    loop {
        loop {
            i = i + 1;
            if i > n { break; };
        };
        r.drop();
        break;
    };
};
static falls_off_the_end = fn() -> () {
    let r = make(1);
    loop {
        r.drop();
    };
};
"#,
        expect![[r#"
            827..852: `r` is left in a different state than the loop found it in; the next iteration would run against a world this body was not checked in (`r` is born here and must be consumed at 805..806)
        "#]],
    );
}

#[test]
fn a_broken_consumption_site_poisons_the_value_rather_than_accusing_it() {
    // Half-typed (`s.`) and unresolvable (`s.nope()`) consumptions are
    // consumptions in progress. Reporting a leak beside the real error
    // would accuse the writer of the opposite of what they are doing —
    // which is the promise `crate::capability` makes about broken
    // programs, now kept.
    check_linear(
        r#"
static half_typed = fn() -> () {
    let r = make(1);
    r.
};
static unresolvable = fn() -> () {
    let r = make(1);
    r.nope();
};
"#,
        expect![[r#"
            402..403: expected a field name after `.`
            465..473: no field or member `nope` on `Res`
        "#]],
    );
}

#[test]
fn a_copy_out_of_a_borrow_is_diagnosed_once() {
    // `r.*[0]` of a linear element is BOTH "cannot move out of a borrow"
    // and "cannot copy a value that must be consumed". One mistake, one
    // message — the borrow-side one, which names the place.
    check_linear(
        r#"
static elem = fn::<@a>(r: [Res; 3].&::<@a>) -> Res { r.*[0] };
"#,
        expect![[r#"
            394..400: cannot move out of a borrow: `Res` cannot be copied
        "#]],
    );
}

#[test]
fn the_heap_builtins_take_a_linear_element_type() {
    // `alloc_array::<T>` hands out a POINTER to storage and never holds a
    // `T`, so its binder opts out of the bound — and `AllocResult::<T>`,
    // whose payload is that pointer, opts out for the same reason. The two
    // have to agree: accepting the result type while refusing the call
    // that produces it is not a rule, it is a bug.
    check_linear(
        r#"
static main = fn() -> () {
    let p = match alloc_array::<Res>(4) {
        ::Ok(p) => p,
        ::Err => panic("oom"),
    };
    unsafe { p.* = make(7); };
    unsafe { dealloc_array(p, 4); };
};
static result_shape = fn(r: AllocResult::<Res>) -> () {
    match r {
        ::Ok(p) => { },
        ::Err => { },
    };
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn the_forget_bound_is_checked_in_annotation_position_too() {
    // A signature is an instantiation edge no expression ever crosses.
    // Checking only expression mentions would let `fn(b: Box::<Res>)` in
    // through the front door. The argument has to name a CONCRETE type to
    // be judged here: a declared type lowers scope-lessly, so one naming
    // the enclosing binder's own parameter is `{error}` and silent —
    // containment still makes the annotated value linear inside the body.
    check_linear(
        r#"
type Box = struct::<T> { v: T };
static take = fn(b: Box::<Res>) -> () {
    let Box(struct { v }) = b;
    v.drop();
};
"#,
        expect![[r#"
            400..403: `Res` cannot be a `T`: `Res` is declared `without forget`, and `T` requires `forget` (every type parameter does unless it is written `T without forget`) (declared here at 346..349)
        "#]],
    );
}

// ---- `String`: the first linear type, as a library ----------------------

/// The acid test: an OWNED, allocation-backed string, declared as a library
/// type and not as a compiler special case. Nothing in `hir` knows the name;
/// the whole of what makes it work is the clause and the members below, and
/// the refusals that follow are the ones a user of it would meet.
///
/// This mirrors `examples/string_lib.must`'s declaration exactly, but is
/// pinned here rather than shared with it: that example must check clean,
/// so the refusal cases below have nowhere to live except a copy.
const STRING_PRELUDE: &str = r#"
type String = struct {
    ptr: u8.&raw mut,
    len: usize,
    cap: usize,
} without forget with {
    impl Self {
        as_str = fn::<@a>(s: Self.&::<@a>) -> str {
            unsafe { str_from_utf8_unchecked(s.*.ptr, s.*.len) }
        };
        len = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.len };
        drop = fn(s: Self) -> () {
            let String(struct { ptr, cap, .. }) = s;
            if cap > 0 {
                unsafe { dealloc_array(ptr, cap); };
            };
        };
    }
};
static empty_string = fn() -> String {
    String(struct { ptr = unsafe { dangling::<u8>() }, len = 0, cap = 0 })
};
static to_owned = fn::<@a>(s: str.&::<@a>) -> String {
    let text = s.*;
    let n = text.len();
    if n == 0 { return empty_string(); };
    let p = match alloc_array::<u8>(n) {
        ::Ok(p) => p,
        ::Err => panic("out of memory"),
    };
    unsafe { str_bytes(text, p); };
    String(struct { ptr = p, len = n, cap = n })
};
"#;

fn check_string(body: &str, expect: Expect) {
    check_diagnostics(&format!("{STRING_PRELUDE}{body}"), expect);
}

#[test]
fn the_string_library_itself_checks_clean() {
    check_string(
        r#"
static main = fn(text: str) -> usize {
    let s = to_owned(text.&);
    let n = s.&.len();
    print(s.&.as_str());
    s.drop();
    n
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn not_dropping_a_string_is_a_check_error() {
    check_string(
        r#"
static leak = fn(text: str) -> () {
    let s = to_owned(text.&);
    print(s.&.as_str());
};
"#,
        expect![[r#"
            1001..1059: `s` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`s` is born here and must be consumed at 1011..1012)
        "#]],
    );
}

#[test]
fn dropping_a_string_twice_is_a_check_error() {
    check_string(
        r#"
static double = fn(text: str) -> () {
    let s = to_owned(text.&);
    s.drop();
    s.drop();
};
"#,
        expect![[r#"
            1053..1054: `s` was already consumed (`s` is born here and must be consumed at 1013..1014) (first consumed here at 1039..1040)
        "#]],
    );
}

#[test]
fn a_string_cannot_be_copied_out_of_a_borrow_or_a_container() {
    check_string(
        r#"
type Pair = struct { left: String, right: String };
static steal = fn::<@a>(s: String.&::<@a>) -> String { s.* };
static halve = fn(p: Pair) -> String {
    let Pair(struct { left, right }) = p;
    right.drop();
    left
};
static half_only = fn(p: Pair) -> String { p.left };
"#,
        expect![[r#"
            1074..1077: cannot move out of a borrow: `String` cannot be copied
            1233..1243: `p` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`p` is born here and must be consumed at 1214..1215)
            1235..1241: cannot copy a value that must be consumed out of a place; take the whole value apart instead (`let Name(struct { .. }) = value;`)
        "#]],
    );
}

#[test]
fn an_option_of_string_is_the_container_shape_that_has_to_work() {
    // Three of `examples/option.must`'s five members — a borrowing
    // `is_some`, a consuming `unwrap`, a region-projecting `as_ref` — over
    // a payload binder that sheds `forget` (the example itself keeps a
    // plain `T`). Nothing here is `String`-specific: it is the acid test
    // that the capability machinery composes with the generic machinery.
    check_string(
        r#"
type Option = enum::<T without forget> {
    Some(T),
    None,
} with {
    impl Self {
        is_some = const fn::<@local>(s: Self.&::<@local>) -> bool {
            match s {
                ::Some(_) => true,
                ::None => false,
            }
        }
        unwrap = const fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
        as_ref = fn::<@a>(s: Self.&::<@a>) -> Option::<T.&::<@a>> {
            match s {
                ::Some(t) => Option::Some(t),
                ::None => Option::None,
            }
        }
    }
};
static main = fn(text: str) -> () {
    // Annotated so the variant widens to its enum — the ordinary
    // variant-vs-enum story, nothing to do with the payload.
    let o: Option::<String> = Option::Some(to_owned(text.&));
    let present = o.&.is_some();
    // `as_ref` projects the payload as a BORROW, so the `Option` it hands
    // back is FORGETTABLE even though the one it came from is not — the
    // containment rule read through indirection, and the reason `as_ref`
    // needs no consuming of its own.
    let borrowed = o.&.as_ref();
    let still_there = borrowed.&.is_some();
    let s = o.unwrap();
    s.drop();
};
static leaks_the_payload = fn(text: str) -> () {
    let o: Option::<String> = Option::Some(to_owned(text.&));
    match o {
        ::Some(s) => { },
        ::None => { },
    };
};
"#,
        expect![[r#"
            2426..2429: `s` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`s` is born here and must be consumed at 2420..2421)
        "#]],
    );
}

#[test]
fn a_hole_binding_cannot_swallow_a_linear_and_says_so_without_a_name() {
    // `let _ = ...` binds nothing to quote, and quoting the empty string
    // reads as a compiler bug — so the message names the position instead.
    check_linear(
        r#"
static main = fn() -> () {
    let _ = make(1);
};
"#,
        expect![[r#"
            366..390: the value bound by `_` is not consumed on this path; its type has no `forget` capability, so every path must consume it (the value bound by `_` is born here and must be consumed at 376..377)
        "#]],
    );
}

#[test]
fn a_join_that_produces_a_fresh_linear_still_has_to_answer_for_the_old_one() {
    // Affine would let the untaken branch's value fall on the floor. Linear
    // does not: the `else` path never consumed `r`, and the join is where
    // that shows.
    check_linear(
        r#"
static main = fn(c: bool) -> Res {
    let r = make(1);
    if c { r } else { make(2) }
};
"#,
        expect![[r#"
            401..428: `r` is consumed on some paths through this expression and not on others (`r` is born here and must be consumed at 384..385)
        "#]],
    );
}

// ---- `Reader`: a heap-owning type declared `without forget` -------------

/// The shipped `examples/stdin_lib.must` `Reader`, trimmed to what the
/// checker sees — the buffering/line-scanning machinery is not the point
/// here, only that the type owns a heap allocation, is declared `without
/// forget`, and disposes of itself by being taken apart, exactly like
/// `String.drop` above. `eof` stays in the trimmed type, unused by any test
/// here, so `..` in `drop` has a forgettable field to actually skip.
const READER_PRELUDE: &str = r#"
type Reader = struct {
    buf: u8.&raw mut,
    cap: usize,
    eof: bool,
} without forget with {
    impl Self {
        drop = fn(r: Self) -> () {
            let Reader(struct { buf, cap, .. }) = r;
            unsafe { dealloc_array(buf, cap); };
        };
    }
};
static reader_new = fn(cap: usize) -> Reader {
    let buf = match alloc_array::<u8>(cap) {
        ::Ok(p) => p,
        ::Err => panic("out of memory"),
    };
    Reader(struct { buf, cap, eof = false })
};
"#;

fn check_reader(body: &str, expect: Expect) {
    check_diagnostics(&format!("{READER_PRELUDE}{body}"), expect);
}

#[test]
fn a_reader_dropped_on_the_one_path_is_clean() {
    check_reader(
        r#"
static main = fn() -> () {
    let r = reader_new(16);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn not_dropping_a_reader_is_a_check_error() {
    // `Reader` owns an allocation and is `without forget`, so a path that
    // never calls `drop` is refused (T20, M16).
    check_reader(
        r#"
static leak = fn() -> () {
    let r = reader_new(16);
};
"#,
        expect![[r#"
            510..541: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 520..521)
        "#]],
    );
}

#[test]
fn a_reader_drop_that_only_reads_a_field_is_refused() {
    // The obvious `drop` reads `r.buf`/`r.cap` and frees them — but a
    // plain field read leaves `r` itself untouched by the checker, so `r`
    // is still owed a consume. This is why `drop` above takes `r` apart
    // instead of reading out of it.
    check_diagnostics(
        r#"
type Reader = struct {
    buf: u8.&raw mut,
    cap: usize,
} without forget with {
    impl Self {
        drop = fn(r: Self) -> () {
            unsafe { dealloc_array(r.buf, r.cap); };
        };
    }
};
"#,
        expect![[r#"
            135..199: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 120..121)
        "#]],
    );
}
