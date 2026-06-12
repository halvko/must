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
fn infer_fn_shorthand_arg() {
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
            18..25: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn type_mismatch_points_at_block_tail() {
    check_diagnostics(
        r#"static f = fn () -> usize { let s = "x"; s };"#,
        expect![[r#"
            41..42: type mismatch: expected `usize`, found `str`
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
            64..71: expected 1 argument(s), found 2
            77..81: expression of type `usize` is not callable
        "#]],
    );
}

#[test]
fn binexpr_operands_must_be_int() {
    check_diagnostics(
        r#"static x = 1 + "two";"#,
        expect![[r#"
            15..20: type mismatch: expected `usize`, found `str`
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
        "#]],
    );
    // …and the tree keeps the intent: types are still inferred inside.
    check_infer(
        "static f = fn 42;",
        expect![[r#"
            11..16 'fn 42': fn() -> usize
            14..16 '42': usize
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
            log_handle
                .lock()
                .unwrap()
                .push(format!("{database_key:?}"));
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
            log_handle
                .lock()
                .unwrap()
                .push(format!("{database_key:?}"));
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
            15..16 '1': usize
            32..56 'fn {     let v = ...': fn()
            35..56 '{     let v = nam...': ()
            45..46 'v': {error}
            49..53 'name': {error}
        "#]],
    );
}

#[test]
fn cross_item_use_of_unannotated_item_infers() {
    // Interprocedural inference: `a`'s signature comes from its body, so
    // the use in `b` sees `usize` — and the *real* type error surfaces.
    check_diagnostics(
        r#"
static a = 42 + 52;
static b = fn { print(a); };
"#,
        expect![[r#"
            43..44: type mismatch: expected `str`, found `usize`
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
fn unannotated_recursion_with_unannotated_params_infers() {
    // Even the params and return come out of the body: `n < 2` forces
    // usize, the call commits `fib` to fn(usize) -> usize.
    check_infer(
        r#"
static fib = fn (n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } };
static use_it: usize = fib(10);
"#,
        expect![[r#"
            14..72 'fn (n) { if n < 2...': fn(usize) -> usize
            18..19 'n': usize
            21..72 '{ if n < 2 { n } ...': usize
            23..70 'if n < 2 { n } el...': usize
            26..27 'n': usize
            26..31 'n < 2': bool
            30..31 '2': usize
            32..37 '{ n }': usize
            34..35 'n': usize
            43..70 '{ fib(n - 1) + fi...': usize
            45..48 'fib': fn(usize) -> usize
            45..55 'fib(n - 1)': usize
            45..68 'fib(n - 1) + fib(...': usize
            49..50 'n': usize
            49..54 'n - 1': usize
            53..54 '1': usize
            58..61 'fib': fn(usize) -> usize
            58..68 'fib(n - 2)': usize
            62..63 'n': usize
            62..67 'n - 2': usize
            66..67 '2': usize
            97..100 'fib': fn(usize) -> usize
            97..104 'fib(10)': usize
            101..103 '10': usize
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
    // `{error}` here would be an error type with no diagnostic explaining
    // it (the tripwire below would catch exactly this).
    check_infer(
        r#"
static f = fn {
    let x = panic("boom");
    x();
};
"#,
        expect![[r#"
            12..54 'fn {     let x = ...': fn()
            15..54 '{     let x = pan...': ()
            25..26 'x': !
            29..34 'panic': fn(str) -> !
            29..42 'panic("boom")': !
            35..41 '"boom"': str
            48..49 'x': !
            48..51 'x()': !
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
            67..76: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn if_condition_must_be_bool() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> () { if n { print("hi"); } };"#,
        expect![[r#"
            36..37: type mismatch: expected `bool`, found `usize`
        "#]],
    );
}

#[test]
fn if_without_else_is_unit() {
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n > 0 { n } };"#,
        expect![[r#"
            36..50: type mismatch: expected `usize`, found `()`
            47..48: type mismatch: expected `()`, found `usize`
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
            46..60 '{ panic("a") }': usize
            48..53 'panic': fn(str) -> !
            48..58 'panic("a")': usize
            54..57 '"a"': str
            66..80 '{ panic("b") }': usize
            68..73 'panic': fn(str) -> !
            68..78 'panic("b")': usize
            74..77 '"b"': str
        "#]],
    );
}

#[test]
fn equality_operands_must_agree() {
    check_diagnostics(
        r#"static x: bool = 1 == "one";"#,
        expect![[r#"
            22..27: type mismatch: expected `usize`, found `str`
        "#]],
    );
}
