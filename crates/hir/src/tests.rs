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
            47..48: cannot call a value in a const context; whether it is a `const fn` is not known from its type
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
    // every winning branch as a hint.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 1 { 1 } else { "" } };
    print("done");
}
"#,
        expect![[r#"
            82..83: type mismatch: expected `str`, found `usize` (this branch has type `str` at 93..95) (this branch has type `str` at 58..60)
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
    let constness: Vec<Constness> = tree.items.iter().map(|item| item.constness).collect();
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
            62..68: cannot call `double` in a const context; marking it `const fn` would allow this (`double` is defined here at 8..14)
        "#]],
    );
}

#[test]
fn directly_called_plain_fn_literal_in_const_context_is_rejected() {
    check_diagnostics(
        "static x = (fn (n: usize) -> usize { n })(1);",
        expect![[r#"
            12..40: cannot call this `fn` literal in a const context; marking it `const fn` would allow this
        "#]],
    );
}

#[test]
fn print_in_a_const_block_is_rejected() {
    check_diagnostics(
        r#"static x = const { print("hi") };"#,
        expect![[r#"
            19..24: cannot call `print` in a const context; const evaluation cannot have side effects
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
            62..67: cannot call `print` in a const context; const evaluation cannot have side effects
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
            54..55: cannot call a value in a const context; whether it is a `const fn` is not known from its type
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
            105..106: cannot call a value in a const context; whether it is a `const fn` is not known from its type
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
            75..82: cannot call a value in a const context; whether it is a `const fn` is not known from its type
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
            50..51: cannot call `f` in a const context; marking it `const fn` would allow this (`f` is defined here at 8..9)
            52..57: cannot call `print` in a const context; const evaluation cannot have side effects
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
