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
        "#]],
    );
}

#[test]
fn hole_named_const_item_gets_dead_code_warning() {
    check_diagnostics(
        "const _ = 5;",
        expect![[r#"
            6..7: this item binds nothing and its value cannot be used
        "#]],
    );
}

#[test]
fn named_item_gets_no_dead_code_warning() {
    check_diagnostics("static x = 5;", expect![[r#""#]]);
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
            20..21 '_': usize
            24..25 '5': usize
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
            15..20: type mismatch: expected `usize`, found `str` (`+` requires `usize` operands at 13..14)
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
fn local_shadow_of_a_duplicated_name_keeps_its_type() {
    check_infer(
        r#"
static name = 1;
static name = fn {
    let name = "x";
    let n = name;
};
"#,
        expect![[r#"
            15..16 '1': usize
            32..76 'fn {     let name...': fn()
            35..76 '{     let name = ...': ()
            45..49 'name': str
            52..55 '"x"': str
            65..66 'n': str
            69..73 'name': str
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
fn caller_pins_callee_params() {
    // Nothing in `apply`'s own body applies `f` or `a` to anything concrete:
    // the concrete use `apply(double, 2)` pins them, which only works
    // because the caller joined the callee's group (reverse reference
    // edges). Forward edges alone would leave `apply`'s params `{error}`
    // and ask for an annotation at this use. (`apply`'s own literal renders
    // with `_`s either way: the per-item query never sees group tables.)
    check_diagnostics(
        r#"
static double = fn (n) { n + n };
static apply = fn (f, a) { f(a) };
static main = fn { apply(double, 2) };
"#,
        expect![[r#""#]],
    );
    check_infer(
        r#"
static double = fn (n) { n + n };
static apply = fn (f, a) { f(a) };
static main = fn { apply(double, 2) };
"#,
        expect![[r#"
            17..33 'fn (n) { n + n }': fn(usize) -> usize
            21..22 'n': usize
            24..33 '{ n + n }': usize
            26..27 'n': usize
            26..31 'n + n': usize
            30..31 'n': usize
            50..68 'fn (f, a) { f(a) }': fn(fn(_) -> _, _) -> _
            54..55 'f': fn(_) -> _
            57..58 'a': _
            60..68 '{ f(a) }': _
            62..63 'f': fn(_) -> _
            62..66 'f(a)': _
            64..65 'a': _
            84..107 'fn { apply(double...': fn() -> usize
            87..107 '{ apply(double, 2) }': usize
            89..94 'apply': fn(fn(usize) -> usize, usize) -> usize
            89..105 'apply(double, 2)': usize
            95..101 'double': fn(usize) -> usize
            103..104 '2': usize
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
    // makes it usize: the group's commitments contradict each other, so the
    // signatures are poisoned to `{error}` rather than silently publishing
    // the guessed one.
    check_diagnostics(
        r#"
static c = true;
static y = if c { 1 } else { x(2) };
static x = y;
"#,
        expect![[r#"
            47..48: cannot infer the type of `x` across items; add a type annotation to its definition (defined here at 62..63)
            47..48: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this item's initializer is a const context at 18..24)
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
    let y = x + 1;
}
static g = fn (p: _) { p + 1 };
static h = fn (_: _) { 5 };
"#,
        expect![[r#"
            12..55 'fn {     let x: _...': fn()
            15..55 '{     let x: _ = ...': ()
            25..26 'x': usize
            32..33 '5': usize
            43..44 'y': usize
            47..48 'x': usize
            47..52 'x + 1': usize
            51..52 '1': usize
            67..86 'fn (p: _) { p + 1 }': fn(usize) -> usize
            71..72 'p': usize
            77..86 '{ p + 1 }': usize
            79..80 'p': usize
            79..84 'p + 1': usize
            83..84 '1': usize
            99..114 'fn (_: _) { 5 }': fn(_) -> usize
            103..104 '_': _
            109..114 '{ 5 }': usize
            111..112 '5': usize
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
static even: _ = fn (n) { if n == 0 { true } else { odd(n - 1) } };
static odd = fn (n: _) { if n == 0 { false } else { even(n - 1) } };
static main = fn { print(even(4)); };
"#,
        expect![[r#"
            163..170: type mismatch: expected `str`, found `bool`
        "#]],
    );
}

#[test]
fn hole_annotated_item_signature_flows_to_dependents() {
    // `static x: _ = 5;` publishes the group-inferred signature: the use
    // in main sees `usize` (and only the print mismatch) instead of the
    // diagnostics being swallowed by a silent `{error}`.
    check_diagnostics(
        r#"
static x: _ = 5;
static main = fn { print(x); };
"#,
        expect![[r#"
            43..44: type mismatch: expected `str`, found `usize`
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
            69..74: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 58..59)
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
            63..64: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
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
            58..59: type mismatch: expected `str`, found `usize` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
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
            70..71: type mismatch: expected `str`, found `usize` (this call requires `str` at 79..84) (this argument needs to be `str` at 85..86)
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
            94..95: type mismatch: expected `str`, found `usize` (this call requires `str` at 105..110) (this argument needs to be `str` at 111..112)
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
            82..83: type mismatch: expected `str`, found `usize` (this branch has type `str` at 58..60) (this branch has type `str` at 93..95)
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
            94..96: type mismatch: expected `usize`, found `str` (this branch has type `usize` at 70..71) (this branch has type `usize` at 81..82)
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
            75..76: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
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
            88..89: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
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
            103..104: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
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
            89..92: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 78..79)
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
            109..112: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 98..99)
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
            64..65: type mismatch: expected `str`, found `usize`
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
            98..99: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
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
            101..103: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 58..91)
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
            63..97: type mismatch: expected `str`, found `usize` (expected `str` because of this annotation at 45..48)
            89..91: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 78..79)
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
static g = fn (c: bool) { if c { 1 } else { 2 } };
static main = fn { print(g(true)); };
"#,
        expect![[r#"
            77..84: type mismatch: expected `str`, found `usize`
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
            22..27: type mismatch: expected `usize`, found `str` (this operand has type `usize` at 17..18)
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
            11..22 'const { 5 }': usize
            17..22 '{ 5 }': usize
            19..20 '5': usize
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
        expect![[r#""#]],
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
            11..41 'fn { let mut x = ...': fn() -> usize
            14..41 '{ let mut x = 1; ...': usize
            24..25 'x': usize
            28..29 '1': usize
            31..32 'x': usize
            35..36 '2': usize
            38..39 'x': usize
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
            35..39: type mismatch: expected `usize`, found `str` (`x` was inferred to have type `usize` from its initializer at 24..25)
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
        expect![[""]],
    );
}

#[test]
fn assign_to_immutable_let_offers_a_make_mutable_fix() {
    let text = "static f = fn { let x = 1; x = 2; };";
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
    let text = "static f = fn { let mut _ = 1; };";
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
        r#"static f = fn { let p = struct { x: 1, y: "s" }; };"#,
        expect![[r#"
            11..50 'fn { let p = stru...': fn()
            14..50 '{ let p = struct ...': ()
            20..21 'p': struct { x: usize, y: str }
            24..47 'struct { x: 1, y:...': struct { x: usize, y: str }
            36..37 '1': usize
            42..45 '"s"': str
        "#]],
    );
}

#[test]
fn record_type_canonicalizes_field_order() {
    // The annotation writes the fields in one order, the literal in the
    // other: field order is irrelevant to the type, so both are the same
    // record and nothing is reported.
    check_diagnostics(
        r#"static f = fn { let p: struct { y: str, x: usize } = struct { x: 1, y: "s" }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn annotated_let_with_matching_record_is_ok() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str } = struct { x: 1, y: "s" }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_field_mismatch_blames_the_field_and_cites_the_annotation() {
    // Bidirectional: the annotation's field types flow into the field
    // initializers, so the squiggle lands on `"s"` (not the whole literal)
    // and cites the annotation as the cause.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x: "s" }; };"#,
        expect![[r#"
            57..60: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..42)
        "#]],
    );
}

#[test]
fn record_literal_missing_field() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str } = struct { x: 1 }; };"#,
        expect![[r#"
            53..68: record literal is missing field `y: str`
        "#]],
    );
}

#[test]
fn record_literal_missing_several_fields() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str, z: bool } = struct { y: "s" }; };"#,
        expect![[r#"
            62..79: record literal is missing fields `x: usize`, `z: bool`
        "#]],
    );
}

#[test]
fn record_literal_extra_field() {
    // Exact field-set equality: the extra field is an error (squiggle on
    // its name), never silently dropped.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x: 1, z: 2 }; };"#,
        expect![[r#"
            60..61: no field `z` in expected type `struct { x: usize }`
        "#]],
    );
}

#[test]
fn record_literal_against_non_record_expectation() {
    check_diagnostics(
        r#"static f = fn { let n: usize = struct { x: 1 }; };"#,
        expect![[r#"
            31..46: type mismatch: expected `usize`, found `struct { x: usize }` (expected `usize` because of this annotation at 23..28)
        "#]],
    );
}

#[test]
fn field_access_infers_the_field_type() {
    check_infer(
        r#"static f = fn { let p = struct { x: 1 }; let y = p.x; };"#,
        expect![[r#"
            11..55 'fn { let p = stru...': fn()
            14..55 '{ let p = struct ...': ()
            20..21 'p': struct { x: usize }
            24..39 'struct { x: 1 }': struct { x: usize }
            36..37 '1': usize
            45..46 'y': usize
            49..50 'p': struct { x: usize }
            49..52 'p.x': usize
        "#]],
    );
}

#[test]
fn field_access_unknown_field() {
    check_diagnostics(
        r#"static f = fn { let p = struct { x: 1 }; p.z; };"#,
        expect![[r#"
            43..44: no field `z` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn field_access_on_non_record() {
    check_diagnostics(
        r#"static f = fn { let n = 1; n.x; };"#,
        expect![[r#"
            29..30: no field `x` on `usize`
        "#]],
    );
}

#[test]
fn chained_field_access_through_nested_records() {
    check_infer(
        r#"static f = fn { let a = struct { b: struct { c: "deep" } }; a.b.c };"#,
        expect![[r#"
            11..67 'fn { let a = stru...': fn() -> str
            14..67 '{ let a = struct ...': str
            20..21 'a': struct { b: struct { c: str } }
            24..58 'struct { b: struc...': struct { b: struct { c: str } }
            36..56 'struct { c: "deep" }': struct { c: str }
            48..54 '"deep"': str
            60..61 'a': struct { b: struct { c: str } }
            60..63 'a.b': struct { c: str }
            60..65 'a.b.c': str
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
        r#"static f = fn (c: bool) { if c { struct { x: 1 } } else { struct { x: "s" } } };"#,
        expect![[r#"
            58..75: `if` branches have incompatible types: `struct { x: usize }` vs `struct { x: str }`; add a type annotation to decide between them (this branch has type `struct { x: usize }` at 33..48)
        "#]],
    );
}

#[test]
fn record_literal_in_initializer_is_const_clean() {
    // Record construction is not a call: const-checking has nothing to say
    // about a pure record literal in an item initializer.
    check_diagnostics(r#"static p = struct { x: 1, y: "s" };"#, expect![[r#""#]]);
}

#[test]
fn record_signature_flows_across_items() {
    check_diagnostics(
        r#"
static origin: struct { x: usize, y: usize } = struct { x: 0, y: 0 };
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

    let text_v1 = "static a: struct { x: usize } = struct { x: 1 };\n\
                   static b = fn { a.x; };\n";
    // Only `a`'s body changes; the annotation (the whole contract) is
    // identical, so dependents must backdate.
    let text_v2 = "static a: struct { x: usize } = struct { x: 2 };\n\
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
    // A computed expression and a shorthand field are both "not a type";
    // an unknown or value name in a field gets the type-position errors.
    check_diagnostics(
        r#"
static five = 5;
type Foo = struct { a: 5, b, c: missing, d: five };
"#,
        expect![[r#"
            41..42: expected a type for field `a`
            44..45: expected a type for field `b`
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
static p = Foo(struct { x: 1 });
"#,
        expect![[r#"
            44..47 'Foo': fn(struct { x: usize }) -> Foo
            44..64 'Foo(struct { x: 1 })': Foo
            48..63 'struct { x: 1 }': struct { x: usize }
            60..61 '1': usize
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
static p = Foo(struct { x: "s" });
"#,
        expect![[r#"
            60..63: type mismatch: expected `usize`, found `str` (expected `usize` because of this field declaration at 21..29)
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
            48..49: type mismatch: expected `struct { x: usize }`, found `usize` (expected `struct { x: usize }` because of `Foo`'s declaration at 6..9)
        "#]],
    );
}

#[test]
fn construction_takes_exactly_one_argument() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x: 1 }, 2);
static q = Foo();
"#,
        expect![[r#"
            44..67: `Foo` takes exactly one argument (its underlying `struct` value), found 2 (`Foo` is defined here at 6..9)
            80..85: `Foo` takes exactly one argument (its underlying `struct` value), found 0 (`Foo` is defined here at 6..9)
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
static a: Foo = struct { x: 1 };
static b: struct { x: usize } = Foo(struct { x: 1 });
"#,
        expect![[r#"
            49..64: type mismatch: expected `Foo`, found `struct { x: usize }`; `Foo` is a distinct type — construct it with `Foo(...)` (expected `Foo` because of this annotation at 43..46)
            98..118: type mismatch: expected `struct { x: usize }`, found `Foo` (expected `struct { x: usize }` because of this annotation at 76..95)
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
static len: Meters = Feet(struct { value: 3 });
"#,
        expect![[r#"
            98..123: type mismatch: expected `Meters`, found `Feet` (expected `Meters` because of this annotation at 89..95)
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
static eq = Foo(struct { x: 1 }) == struct { x: 1 };
"#,
        expect![[r#"
            69..84: type mismatch: expected `Foo`, found `struct { x: usize }`; `Foo` is a distinct type — construct it with `Foo(...)` (this operand has type `Foo` at 45..65)
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
static a = Foo(struct { x: 1 });
const b = const { Foo(struct { x: 2 }) };
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
static f = fn { let p = Foo(struct { x: 1 }); p.x };
"#,
        expect![[r#"
            44..84 'fn { let p = Foo(...': fn() -> usize
            47..84 '{ let p = Foo(str...': usize
            53..54 'p': Foo
            57..60 'Foo': fn(struct { x: usize }) -> Foo
            57..77 'Foo(struct { x: 1 })': Foo
            61..76 'struct { x: 1 }': struct { x: usize }
            73..74 '1': usize
            79..80 'p': Foo
            79..82 'p.x': usize
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
    check_diagnostics(
        r#"
type Point = struct { x: usize };
static p = Point::x;
"#,
        expect![[r#"
            46..54: `Point` has no variants (it is a `struct` type) (`Point` is defined here at 6..11)
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
            99..100: type mismatch: expected `Shape`, found `usize` (`s` was inferred to have type `Shape` from its initializer at 73..74)
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
            171..175: type mismatch: expected `usize`, found `str` (this branch has type `usize` at 145..146) (this branch has type `usize` at 208..209)
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
            59..70: only `_` or a binding can match a `usize` (for now)
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
    check_infer(
        "static f = fn { loop { } };",
        expect![[r#"
            11..26 'fn { loop { } }': fn() -> _
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
            21..31 '{ break; }': ()
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
            91..173 '{         if stop...': ()
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
            12..81 'fn {     loop {  ...': fn() -> _
            15..81 '{     loop {     ...': !
            21..79 'loop {         le...': !
            26..79 '{         let n =...': ()
            40..41 'n': usize
            44..61 'loop { break 1; }': usize
            49..61 '{ break 1; }': ()
            51..58 'break 1': !
            57..58 '1': usize
            71..72 'n': usize
        "#]],
    );
}

#[test]
fn break_outside_loop_errors() {
    check_diagnostics(
        "static f = fn { break 1; };",
        expect![[r#"
            16..23: `break` outside of a loop: there is no enclosing `loop` to exit
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

// ---- record destructuring, `pub` reservation, field assignment ----

#[test]
fn let_record_destructure_binds_correct_types() {
    check_infer(
        r#"static f = fn { let struct { x, y } = struct { x: 1, y: "s" }; };"#,
        expect![[r#"
            11..64 'fn { let struct {...': fn()
            14..64 '{ let struct { x,...': ()
            29..30 'x': usize
            32..33 'y': str
            38..61 'struct { x: 1, y:...': struct { x: usize, y: str }
            50..51 '1': usize
            56..59 '"s"': str
        "#]],
    );
}

#[test]
fn let_record_destructure_rename_binds_only_the_new_name() {
    // `x` is not bound under its own name; only the rename `a` is.
    check_diagnostics(
        r#"static f = fn { let struct { x as a } = struct { x: 1 }; let b = a; let c = x; };"#,
        expect![[r#"
            76..77: unresolved name `x`
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_field_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x: 1, y: 2 }; };"#,
        expect![[r#"
            20..32: pattern does not mention field `y`; add `..` to ignore it
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_several_fields_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x: 1, y: 2, z: 3 }; };"#,
        expect![[r#"
            20..32: pattern does not mention fields `y`, `z`; add `..` to ignore them
        "#]],
    );
}

#[test]
fn let_record_destructure_with_rest_ignores_missing_fields() {
    check_diagnostics(
        r#"static f = fn { let struct { x, .. } = struct { x: 1, y: 2 }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn let_record_destructure_unknown_field_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x, z } = struct { x: 1, y: 2 }; };"#,
        expect![[r#"
            20..35: no field `z` on `struct { x: usize, y: usize }`
            20..35: pattern does not mention field `y`; add `..` to ignore it
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
        "#]],
    );
}

#[test]
fn record_destructure_unknown_field_still_types_it_as_error_not_cascading() {
    // The unknown field's binding recovers as `{error}` (infectious and
    // silent) rather than blocking the rest of the pattern from checking.
    check_diagnostics(
        r#"static f = fn { let struct { x, z, .. } = struct { x: 1 }; let n: usize = z; };"#,
        expect![[r#"
            20..39: no field `z` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn per_binding_mut_in_record_pattern_allows_assignment() {
    check_diagnostics(
        r#"static f = fn { let struct { mut x, y } = struct { x: 1, y: 2 }; x = 3; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_pattern_binding_without_mut_is_immutable() {
    check_diagnostics(
        r#"static f = fn { let struct { x, y } = struct { x: 1, y: 2 }; x = 3; };"#,
        expect![[r#"
            61..62: cannot assign to `x`: it is not declared `mut` (`x` is declared without `mut` here at 29..30)
        "#]],
    );
}

#[test]
fn let_mut_on_a_destructuring_pattern_is_a_syntax_error() {
    check_diagnostics(
        r#"static f = fn { let mut struct { x } = struct { x: 1 }; };"#,
        expect![[r#"
            20..36: `mut` applies to individual bindings in a destructuring pattern
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
        r#"static f = fn { let mut p = struct { a: struct { b: 1 } }; p.a.b = 2; };"#,
        expect![[r#""#]],
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
    let text = "static f = fn { let p = struct { x: 1 }; p.x = 2; };";
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
        r#"static f = fn { let mut p = struct { x: 1 }; p.y = 2; };"#,
        expect![[r#"
            47..48: no field `y` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn field_assign_through_a_non_record_reports_no_such_field() {
    check_diagnostics(
        r#"static f = fn { let mut n = 1; n.x = 2; };"#,
        expect![[r#"
            33..34: no field `x` on `usize`
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
static two: usize = bump(struct { x: 1 });
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
        "static f = const { (fn { 1 })() };",
        expect![[r#"
            20..28: cannot call this `fn` literal in a const context; marking it `const fn` would allow this (this `const` block is a const context at 11..16)
        "#]],
    );
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = const { (fn { 1 })() };".to_owned(),
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
static main = fn { Foo(struct { x: 1 }); };
"#,
        expect![[r#"
            47..75 'fn { Foo(struct {...': fn()
            50..75 '{ Foo(struct { x:...': ()
            52..72 'Foo(struct { x: 1 })': Foo
            56..71 'struct { x: 1 }': struct { x: usize }
            68..69 '1': usize
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
            48..60 '{ break 1; }': ()
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
            27..28 '5': usize
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
        "static f = fn::<T>(x: T) -> T { let y: T = x; let r = struct { v: y }; if x == y { r.v } else { x } };",
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
            70..71: type mismatch: expected `usize`, found `T` (`+` requires `usize` operands at 72..73)
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
        "type Foo = struct { x: usize };\nstatic f: Foo::<usize> = Foo(struct { x: 1 });",
        expect![[r#"
            42..54: `Foo` takes no generic arguments (declared here at 5..8)
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
            28..29: expected `,`
            30..32: expected `,`
            32..33: expected `,`
            33..34: expected `,`
            35..37: expected `,`
            38..43: expected `,`
            44..45: expected `,`
            60..61: type mismatch: expected `usize`, found `usize` (expected `usize` because of this return type at 49..57)
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
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () -> usize { let p = Pair::<usize>(struct { a: 1, b: 2 }); p.a + p.b };",
        expect![[r#"
            54..128 'fn () -> usize { ...': fn() -> usize
            69..128 '{ let p = Pair::<...': usize
            75..76 'p': Pair::<usize>
            79..92 'Pair::<usize>': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            79..115 'Pair::<usize>(str...': Pair::<usize>
            93..114 'struct { a: 1, b:...': struct { a: usize, b: usize }
            105..106 '1': usize
            111..112 '2': usize
            117..118 'p': Pair::<usize>
            117..120 'p.a': usize
            117..126 'p.a + p.b': usize
            123..124 'p': Pair::<usize>
            123..126 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_record_construction_infers_args_from_payload() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () -> usize { let p = Pair(struct { a: 1, b: 2 }); p.b };",
        expect![[r#"
            54..113 'fn () -> usize { ...': fn() -> usize
            69..113 '{ let p = Pair(st...': usize
            75..76 'p': Pair::<usize>
            79..83 'Pair': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            79..106 'Pair(struct { a: ...': Pair::<usize>
            84..105 'struct { a: 1, b:...': struct { a: usize, b: usize }
            96..97 '1': usize
            102..103 '2': usize
            108..109 'p': Pair::<usize>
            108..111 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let p = Pair::<usize, str>(struct { a: 1, b: 2 }); };",
        expect![[r#"
            70..88: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_in_annotation_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let p: Pair::<usize, str> = Pair::<usize>(struct { a: 1, b: 2 }); };",
        expect![[r#"
            69..87: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn bare_generic_type_in_annotation_requires_the_turbofish() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let p: Pair = Pair::<usize>(struct { a: 1, b: 2 }); };",
        expect![[r#"
            69..73: `Pair` takes 1 generic argument, found 0 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn annotation_hole_arg_is_pinned_by_the_initializer() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let p: Pair::<_> = Pair(struct { a: 1, b: 2 }); };",
        expect![[r#"
            54..111 'fn () { let p: Pa...': fn()
            60..111 '{ let p: Pair::<_...': ()
            66..67 'p': Pair::<usize>
            81..85 'Pair': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            81..108 'Pair(struct { a: ...': Pair::<usize>
            86..107 'struct { a: 1, b:...': struct { a: usize, b: usize }
            98..99 '1': usize
            104..105 '2': usize
        "#]],
    );
}

#[test]
fn different_type_args_do_not_unify() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let p: Pair::<str> = Pair::<usize>(struct { a: 1, b: 2 }); };",
        expect![[r#"
            83..119: type mismatch: expected `Pair::<str>`, found `Pair::<usize>` (expected `Pair::<str>` because of this annotation at 69..80)
        "#]],
    );
}

#[test]
fn same_args_unify_across_bodies() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static mk = fn () -> Pair::<usize> { Pair::<usize>(struct { a: 1, b: 2 }) };\n\
         static use_it = fn (p: Pair::<usize>) -> usize { p.a };\n\
         static main = fn () -> usize { use_it(mk()) };",
        expect![[""]],
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
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static main = fn () { let b: Buf::<8> = Buf::<9>(struct { len: 1 }); };",
        expect![[r#"
            92..119: type mismatch: expected `Buf::<8>`, found `Buf::<9>` (expected `Buf::<8>` because of this annotation at 81..89)
        "#]],
    );
}

#[test]
fn const_param_type_annotation_and_construction_agree() {
    check_infer(
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static main = fn () -> usize { let b: Buf::<8> = Buf::<8>(struct { len: 3 }); b.len };",
        expect![[r#"
            66..137 'fn () -> usize { ...': fn() -> usize
            81..137 '{ let b: Buf::<8>...': usize
            87..88 'b': Buf::<8>
            101..109 'Buf::<8>': fn(struct { len: usize }) -> Buf::<8>
            101..128 'Buf::<8>(struct {...': Buf::<8>
            107..108 '8': usize
            110..127 'struct { len: 3 }': struct { len: usize }
            124..125 '3': usize
            130..131 'b': Buf::<8>
            130..135 'b.len': usize
        "#]],
    );
}

#[test]
fn const_block_in_annotation_position_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static main = fn () { let b: Buf::<const { 4 + 4 }> = Buf::<8>(struct { len: 1 }); };",
        expect![[r#"
            87..102: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn const_block_in_type_construction_turbofish_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static main = fn () { let b = Buf::<const { 4 + 4 }>(struct { len: 1 }); };",
        expect![[r#"
            82..104: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
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
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () -> usize { Pair::<usize>(struct { a: 1, b: 2 }) };",
        expect![[r#"
            71..107: type mismatch: expected `usize`, found `Pair::<usize>` (expected `usize` because of this return type at 60..68)
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
        "type Buf = struct::<T, const N: usize> { x: T };\n\
         static main = fn () { let b: Buf::<8, usize> = Buf::<8, usize>(struct { x: 1 }); };",
        expect![[r#"
            84..85: `T` is a type parameter; write a type
            87..92: `N` is a const parameter; write a value (a literal, or `const <expr>`)
            96..111: `T` is a type parameter; write a type
            96..111: `N` is a const parameter; write a value (a literal, or `const <expr>`)
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
            66..70: `Pair` takes 1 generic argument, found 0
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
