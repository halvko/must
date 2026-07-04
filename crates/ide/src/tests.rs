use crate::{AnalysisHost, FilePosition};
use base_db::SourceFile;
use syntax::TextSize;

const BROKEN: &str = "static = 1;";

/// Splits a fixture on `$0` (the cursor). Panics if absent.
fn fixture(text: &str) -> (crate::Analysis, SourceFile, FilePosition) {
    let cursor = text.find("$0").expect("fixture needs a $0 cursor marker");
    let text = text.replace("$0", "");
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), text);
    let analysis = host.snapshot();
    let pos = FilePosition {
        file,
        offset: TextSize::new(cursor as u32),
    };
    (analysis, file, pos)
}

/// Asserts goto-definition from `$0` lands focused on `expected`, located by
/// `occurrence` (0-based) of that exact text in the fixture (after `$0` is
/// stripped).
fn check_goto(fixture_text: &str, expected: &str, occurrence: usize) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let nav = analysis
        .goto_definition(pos)
        .expect("goto_definition returned None");
    let text = fixture_text.replace("$0", "");
    let expected_start = text
        .match_indices(expected)
        .nth(occurrence)
        .map(|(i, _)| i)
        .expect("expected text not found in fixture");
    assert_eq!(
        u32::from(nav.focus_range.start()),
        expected_start as u32,
        "focus range {:?} (text: {:?})",
        nav.focus_range,
        &text[nav.focus_range.start().into()..nav.focus_range.end().into()],
    );
}

fn check_no_goto(fixture_text: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert_eq!(analysis.goto_definition(pos), None);
}

#[test]
fn goto_local_picks_nearest_shadowing_binding() {
    check_goto(
        r#"
static f = fn {
    let x = 1;
    let x = 2;
    print(x$0);
}
"#,
        "x",
        1,
    );
}

#[test]
fn goto_param() {
    check_goto("static f = fn (count: usize) { count$0 + 1 }", "count", 0);
}

#[test]
fn goto_static_across_mutual_recursion() {
    check_goto(
        r#"
const a = fn { b$0() };
static b = fn { a() };
"#,
        // Occurrence 0 of `b` is the reference itself; 1 is the definition.
        "b",
        1,
    );
}

#[test]
fn goto_static_from_its_own_body() {
    check_goto("static rec = fn { rec$0() };", "rec", 0);
}

#[test]
fn goto_builtin_is_none() {
    check_no_goto(r#"static f = fn { print$0("hi") };"#);
}

#[test]
fn goto_assignment_lhs() {
    // The LHS of an assignment lowers as a normal expression, so goto-def
    // works on it exactly like any other read of the binding.
    check_goto(
        r#"
static f = fn {
    let mut x = 1;
    x$0 = 5;
};
"#,
        "x",
        0,
    );
}

fn check_hover(fixture_text: &str, expected_markup: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let hover = analysis.hover(pos).expect("hover returned None");
    assert_eq!(hover.markup, expected_markup);
}

fn check_no_hover(fixture_text: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert_eq!(analysis.hover(pos), None);
}

#[test]
fn goto_hole_pattern_is_none() {
    // `_` isn't a name, so there's nothing to jump to — and nothing panics.
    check_no_goto("static f = fn { let _ = 1; _$0; };");
}

#[test]
fn hover_hole_pattern_is_none() {
    check_no_hover("static f = fn { let _$0 = 1; };");
}

#[test]
fn hover_local_use() {
    check_hover(
        r#"static main = fn { let s = "hello"; print(s$0); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_binding_definition() {
    check_hover(
        r#"static main = fn { let s$0 = "hello"; print(s); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_mut_binding_definition_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s$0 = "hello"; s = "bye"; };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_binding_use_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s = "hello"; print(s$0); };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_param_shows_mut() {
    check_hover(
        "static f = fn (mut n$0: usize) { n = n + 1; };",
        "```must\nmut n: usize\n```",
    );
}

#[test]
fn hover_item_name_shows_inferred_fn_type() {
    check_hover(
        "static ma$0in = fn { print(\"hi\"); };",
        "```must\nmain: fn()\n```",
    );
}

#[test]
fn hover_static_use_shows_signature() {
    check_hover(
        r#"
static f = fn (n: usize) -> usize { n }
static main = fn { f$0(1); };
"#,
        "```must\nf: fn(usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_definition_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static high$0er_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    higher_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_call_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static higher_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    high$0er_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn non_block_fn_body_diagnostic_carries_wrap_fix() {
    let (analysis, file, _pos) = fixture("static f = fn 42$0;");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Wrap in `{ }`");
    // Insert "{ " before `42` (offset 14) and " }" after it (offset 16).
    assert_eq!(fix.edits.len(), 2);
    assert_eq!(u32::from(fix.edits[0].edit.range.start()), 14);
    assert_eq!(fix.edits[0].edit.insert, "{ ");
    assert_eq!(u32::from(fix.edits[1].edit.range.start()), 16);
    assert_eq!(fix.edits[1].edit.insert, " }");
}

#[test]
fn missing_semicolon_diagnostic_carries_insert_fix() {
    let (analysis, file, _pos) = fixture(
        r#"
static name = fn {
    let f = fn { 42 }$0
    f()
}
"#,
    );
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "expected `;`");
    // Anchored on the closing `}` of `fn { 42 }` (a visible, one-token
    // range a cursor can sit on); the fix inserts right after it.
    assert_eq!(diagnostics[0].range.len(), syntax::TextSize::new(1));
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Insert `;`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].edit.insert, ";");
    assert!(fix.edits[0].edit.range.is_empty());
    assert_eq!(fix.edits[0].edit.range.start(), diagnostics[0].range.end());
}

/// Renders each highlight as `start..end text tag[.mods]`, one per line.
fn check_highlights(text: &str, expect: expect_test::Expect) {
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), text.to_owned());
    let analysis = host.snapshot();
    let mut rendered = String::new();
    for hl in analysis.highlight(file) {
        let (start, end) = (u32::from(hl.range.start()), u32::from(hl.range.end()));
        rendered.push_str(&format!(
            "{start}..{end} {:?} {:?}",
            &text[start as usize..end as usize],
            hl.tag
        ));
        for (flag, name) in [
            (crate::HlMods::DECLARATION, "declaration"),
            (crate::HlMods::STATIC, "static"),
            (crate::HlMods::DEFAULT_LIBRARY, "defaultLibrary"),
            (crate::HlMods::MUTABLE, "mutable"),
        ] {
            if hl.mods.contains(flag) {
                rendered.push('.');
                rendered.push_str(name);
            }
        }
        rendered.push('\n');
    }
    expect.assert_eq(&rendered);
}

#[test]
fn highlights_classify_names_semantically() {
    check_highlights(
        r#"// the entry point
static greeting: str = "hi";
static main = fn (count: usize) {
    let next = count - 1;
    print(greeting);
    main(next);
};"#,
        expect_test::expect![[r#"
            0..18 "// the entry point" Comment
            19..25 "static" Keyword
            26..34 "greeting" Variable.declaration.static
            36..39 "str" Type.defaultLibrary
            40..41 "=" Operator
            42..46 "\"hi\"" String
            48..54 "static" Keyword
            55..59 "main" Function.declaration.static
            60..61 "=" Operator
            62..64 "fn" Keyword
            66..71 "count" Parameter.declaration
            73..78 "usize" Type.defaultLibrary
            86..89 "let" Keyword
            90..94 "next" Variable.declaration
            95..96 "=" Operator
            97..102 "count" Parameter
            103..104 "-" Operator
            105..106 "1" Number
            112..117 "print" Function.defaultLibrary
            118..126 "greeting" Variable.static
            133..137 "main" Function.static
            138..142 "next" Variable
        "#]],
    );
}

#[test]
fn highlights_hole_pattern_unstyled() {
    // `_` isn't an IDENT token, so it's left unclassified (no panic, no
    // bogus Variable highlight).
    check_highlights(
        "static f = fn { let _ = 1; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            22..23 "=" Operator
            24..25 "1" Number
        "#]],
    );
}

#[test]
fn highlights_const_fn_and_const_block_keywords() {
    check_highlights(
        "static f = const fn { const { 1 } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..16 "const" Keyword
            17..19 "fn" Keyword
            22..27 "const" Keyword
            30..31 "1" Number
        "#]],
    );
}

#[test]
fn highlights_struct_keyword() {
    check_highlights(
        "static p = struct { x: 1 };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "p" Variable.declaration.static
            9..10 "=" Operator
            11..17 "struct" Keyword
            23..24 "1" Number
        "#]],
    );
}

#[test]
fn highlights_mut_keyword() {
    check_highlights(
        "static f = fn { let mut x = 1; x = 2; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            20..23 "mut" Keyword
            24..25 "x" Variable.declaration.mutable
            26..27 "=" Operator
            28..29 "1" Number
            31..32 "x" Variable.mutable
            33..34 "=" Operator
            35..36 "2" Number
        "#]],
    );
}

#[test]
fn highlights_mutable_param_declaration_and_uses() {
    // The `mutable` modifier follows the binding, not just its declaration:
    // both the `mut` parameter's name and its later reads/assignment carry
    // it, while an ordinary immutable `let` next to it does not.
    check_highlights(
        "static f = fn (mut n: usize) { let y = n; n = y; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..18 "mut" Keyword
            19..20 "n" Parameter.declaration.mutable
            22..27 "usize" Type.defaultLibrary
            31..34 "let" Keyword
            35..36 "y" Variable.declaration
            37..38 "=" Operator
            39..40 "n" Parameter.mutable
            42..43 "n" Parameter.mutable
            44..45 "=" Operator
            46..47 "y" Variable
        "#]],
    );
}

#[test]
fn highlights_split_multiline_strings_per_line() {
    check_highlights(
        "static s = \"one\ntwo\";",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "s" Variable.declaration.static
            9..10 "=" Operator
            11..15 "\"one" String
            16..20 "two\"" String
        "#]],
    );
}

#[test]
fn diagnostics_include_mir_findings() {
    // Captures are invisible to name resolution and inference — only MIR
    // lowering notices the binding lives in an enclosing function.
    let (analysis, file, _pos) =
        fixture("static f = fn () -> usize { let a = 1; let g = fn () -> usize { a$0 }; g() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "`a` is a local of an enclosing function; captures are not supported yet"
    );
    // On the `a` inside the nested fn literal.
    assert_eq!(u32::from(diagnostics[0].range.start()), 64);
}

#[test]
fn diagnostics_include_const_check_findings() {
    // Item initializers are const contexts; `double` is a plain `fn`, so
    // calling it there is rejected — squiggle on the callee.
    let src = "static double = fn (n: usize) -> usize { n * 2 };\nstatic x: usize = double(2);";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `double` in a const context; marking it `const fn` would allow this"
    );
    // On the callee `double` of the call, not the whole call expression.
    assert_eq!(&src[errors[0].range], "double");
    assert!(
        u32::from(errors[0].range.start()) > 50,
        "the use, not the definition"
    );
    assert_eq!(
        errors[0].related.len(),
        2,
        "related: {:?}",
        errors[0].related
    );
    assert_eq!(errors[0].related[0].message, "`double` is defined here");
    assert_eq!(&src[errors[0].related[0].range], "double");
    // The second hint explains why the call site is a const context at
    // all: it's directly in `x`'s own initializer, so it cites `x`'s
    // leading `static` keyword.
    assert_eq!(
        errors[0].related[1].message,
        "this item's initializer is a const context"
    );
    assert_eq!(&src[errors[0].related[1].range], "static");
}

#[test]
fn assign_to_immutable_squiggles_the_lhs_name_with_related_info() {
    // The squiggle sits on the assignment's LHS use of `x`; the related
    // hint points back at the `let`'s binding name, where `mut` is missing.
    let src = "static f = fn { let x = 1; x = 2; };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot assign to `x`: it is not declared `mut`"
    );
    // The LHS use (offset 27), not the declaration (offset 20).
    assert_eq!(&src[errors[0].range], "x");
    assert_eq!(u32::from(errors[0].range.start()), 27);
    assert_eq!(errors[0].related.len(), 1);
    assert_eq!(
        errors[0].related[0].message,
        "`x` is declared without `mut` here"
    );
    assert_eq!(&src[errors[0].related[0].range], "x");
    assert_eq!(u32::from(errors[0].related[0].range.start()), 20);
}

#[test]
fn field_assign_to_immutable_root_offers_the_make_mut_fix() {
    // Same machinery as the plain-assignment case: the squiggle sits on
    // the ROOT name inside the place, the message carries the field path,
    // and the fix inserts `mut ` at the binding's declaration.
    let src = "static f = fn { let p = struct { x: 1 }; p.x = 2; };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot assign to `p.x`: `p` is not declared `mut`"
    );
    // The root `p` inside the place (offset 41), not the whole `p.x`.
    assert_eq!(&src[errors[0].range], "p");
    assert_eq!(u32::from(errors[0].range.start()), 41);
    let fix = errors[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `p` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].edit.insert, "mut ");
    // Right before the declaration's `p` (offset 20).
    assert_eq!(u32::from(fix.edits[0].edit.range.start()), 20);
}

#[test]
fn const_check_finding_is_not_double_reported_by_const_eval() {
    // Const-evaluating `x` crashes at the trap MIR planted for the
    // const-check violation, but that failure is `Trap` — an
    // already-reported diagnostic execution ran into — so the const-eval
    // layer must add nothing: one squiggle, no "constant evaluation
    // failed" companion.
    let src = "static double = fn (n: usize) -> usize { n * 2 };\nstatic x: usize = double(2);";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `double` in a const context; marking it `const fn` would allow this"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("constant evaluation")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn diagnostics_include_failing_const_blocks_in_uncalled_fns() {
    // Nothing ever calls `f`, but its `const { … }` is compile-time code:
    // the panic is a check-time diagnostic, at the failure inside the block.
    let src = r#"static f = fn { const { panic("boom") }; };"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].message, "constant evaluation panicked: boom");
    assert_eq!(&src[diagnostics[0].range], r#"panic("boom")"#);
}

#[test]
fn const_block_trap_failure_is_not_double_reported() {
    // `print` in a `const` block already has a const-check squiggle;
    // forcing the block runs into the trap MIR planted for that same
    // violation — a `Trap` failure must add nothing.
    let src = r#"static f = fn { const { print("hi") }; };"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `print` in a const context; const evaluation cannot have side effects"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("constant evaluation")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn hover_const_block_shows_its_value() {
    check_hover(
        "static f = fn () -> usize { con$0st { 2 + 3 } };",
        "```must\nconst { … }: usize = 5\n```",
    );
}

#[test]
fn hover_item_shows_const_value() {
    check_hover(
        "static exa$0mple = 4 + 5;",
        "```must\nexample: usize = 9\n```",
    );
}

#[test]
fn hover_item_shows_record_const_value() {
    // Records const-evaluate: `Value::Record` displays like the type
    // does, field by field, in the same canonical (sorted) order.
    check_hover(
        "static po$0int = struct { y: 2, x: 1 };",
        "```must\npoint: struct { x: usize, y: usize } = { x: 1, y: 2 }\n```",
    );
}

#[test]
fn hover_use_shows_const_value_of_the_target() {
    check_hover(
        r#"
static greeting = "hi";
static main = fn { print(gree$0ting); };
"#,
        "```must\ngreeting: str = \"hi\"\n```",
    );
}

#[test]
fn diagnostics_include_const_eval_failures() {
    let (analysis, file, _pos) = fixture("static bad: usize = 1 / 0;$0\nstatic also: usize = bad;");
    let diagnostics = analysis.diagnostics(file);
    // Reported once at the origin, not re-reported by the propagating user.
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "constant evaluation failed: attempt to divide by zero"
    );
    // On `1 / 0`.
    assert_eq!(u32::from(diagnostics[0].range.start()), 20);
    assert_eq!(u32::from(diagnostics[0].range.end()), 25);
}

#[test]
fn unused_static_with_panicking_initializer_is_reported_at_check_time() {
    // Statics are eager, not lazy — every item is evaluated at check
    // time regardless of whether anything (transitively) calls or uses it.
    // `x` is never referenced by anything here; its panic must still show up
    // as a check-time diagnostic. This pins behavior that already falls out
    // of `Analysis::diagnostics` forcing `eval::const_value` for every item
    // in the file (not just reachable ones).
    let (analysis, file, _pos) = fixture(r#"static x = panic("boom");$0"#);
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].message, "constant evaluation panicked: boom");
    assert_eq!(diagnostics[0].severity, crate::Severity::Error);
}

#[test]
fn hole_named_item_dead_code_warning_surfaces_through_ide() {
    let (analysis, file, _pos) = fixture("static _ = 5;$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "this item binds nothing and its value cannot be used"
    );
    assert_eq!(diagnostics[0].severity, crate::Severity::Warning);
}

#[test]
fn hole_named_item_with_panicking_initializer_reports_both_the_warning_and_the_panic() {
    // A hole-named item's value can never be used (dead-code warning), but
    // since statics are eager it is still evaluated — a panic is the
    // one side effect const contexts allow, so it still surfaces as its own
    // error alongside the warning.
    let (analysis, file, _pos) = fixture(r#"static _ = panic("x");$0"#);
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 2, "diagnostics: {diagnostics:?}");
    let warning = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Warning)
        .expect("expected the dead-code warning");
    assert_eq!(
        warning.message,
        "this item binds nothing and its value cannot be used"
    );
    let error = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error)
        .expect("expected the panic error");
    assert_eq!(error.message, "constant evaluation panicked: x");
}

#[test]
fn diagnostics_include_name_errors() {
    let (analysis, file, _pos) = fixture("static f = fn { missing$0() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unresolved name `missing`");
}

// ---------------------------------------------------------------------------
// File lifecycle at this level: a closed document reads as empty, and the
// same handle analyzes again once it has text.
// ---------------------------------------------------------------------------

#[test]
fn a_closed_file_analyzes_again_on_the_same_handle() {
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), BROKEN.to_owned());
    assert_eq!(host.snapshot().diagnostics(file).len(), 1);

    host.close_file(file);
    assert_eq!(host.snapshot().diagnostics(file), vec![]);

    host.set_file_text(file, BROKEN.to_owned());
    assert_eq!(host.snapshot().diagnostics(file).len(), 1);

    host.set_file_text(file, "static a = 1;".to_owned());
    assert_eq!(host.snapshot().diagnostics(file), vec![]);
}

#[test]
fn if_branch_mismatch_hint_points_to_other_branch() {
    let src = r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 1 } else { "one" };
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("incompatible types"))
        .expect("expected an incompatible-types diagnostic");
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(mismatch.related[0].message, "this branch has type `usize`");
    // The hint points at the value-producing tail expression `1`, not the
    // whole `{ 1 }` block.
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "1");
}

#[test]
fn related_locations_get_companion_hint_diagnostics() {
    // Every related location also becomes its own hint-severity diagnostic
    // that spells out the connection and links back to the error — editors
    // that render related information as bare underlines (Zed) then explain
    // the underline on hover.
    let src = r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { 0 } else { "" };
    print(x);
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let error = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error)
        .expect("expected the type mismatch");
    assert_eq!(&src[error.range], "0");
    let hints: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Info)
        .collect();
    assert_eq!(hints.len(), 2);
    assert_eq!(&src[hints[0].range], "print");
    assert_eq!(
        hints[0].message,
        "this call requires `str` — causes the error on line 3: \
         type mismatch: expected `str`, found `usize`"
    );
    assert_eq!(&src[hints[1].range], "x");
    assert_eq!(
        hints[1].message,
        "this argument needs to be `str` — causes the error on line 3: \
         type mismatch: expected `str`, found `usize`"
    );
    for hint in hints {
        assert_eq!(hint.related.len(), 1);
        assert_eq!(hint.related[0].message, "the error reported here");
        assert_eq!(hint.related[0].range, error.range);
    }
}

#[test]
fn let_annotation_mismatch_hints_at_annotation() {
    let src = r#"
static f = fn (n: usize) -> () {
    let x: str = 42;
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("type mismatch"))
        .expect("expected a type mismatch diagnostic");
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(
        mismatch.related[0].message,
        "expected `str` because of this annotation"
    );
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "str");
}

#[test]
fn let_annotation_mismatch_on_agreeing_if_branches_blames_annotation_not_then_branch() {
    // Branches agree (both usize) but the annotation says str. Must produce
    // exactly one every-branch diagnostic on the if-expression pointing at
    // the annotation, not a spurious IfBranchMismatch blaming the then-branch.
    let src = r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { 1 } else { 0 };
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.contains("incompatible types")),
        "got a spurious IfBranchMismatch: {diagnostics:?}"
    );
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("every branch"))
        .expect("expected an every-branch diagnostic");
    assert_eq!(
        mismatch.message,
        "every branch produces `usize`, but `str` is needed"
    );
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(
        mismatch.related[0].message,
        "expected `str` because of this annotation"
    );
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "str");
}

#[test]
fn hover_record_typed_binding() {
    // The record type renders canonically (sorted fields) on the binding.
    check_hover(
        r#"static f = fn { let p$0 = struct { y: "s", x: 1 }; };"#,
        "```must\np: struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_field_access_shows_the_field_type() {
    check_hover(
        r#"static f = fn { let p = struct { x: 1 }; let y = p.x$0; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_chained_field_access_intermediate_step() {
    // Hovering `b` in `a.b.c` shows the intermediate record's type.
    check_hover(
        r#"static f = fn { let a = struct { b: struct { c: "deep" } }; a.b$0.c; };"#,
        "```must\nb: struct { c: str }\n```",
    );
}

#[test]
fn record_expression_evaluates_cleanly() {
    // Records are typed structurally and have a MIR/eval story
    // (aggregates and field projections) — the "not yet" diagnostic is
    // gone, and a record initializer is exactly as clean as any other.
    let (analysis, file, _pos) = fixture("static p = struct { x: 1 };$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, Vec::new(), "diagnostics: {diagnostics:?}");
}

// ---- named types ----

#[test]
fn goto_type_item_from_annotation() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_type_item_from_construction_call() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x: 1 });
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_builtin_type_in_annotation_is_none() {
    check_no_goto("static f = fn (n: usize$0) { n };");
}

#[test]
fn hover_type_item_declaration() {
    check_hover(
        "type Foo$0 = struct { y: str, x: usize };",
        "```must\ntype Foo = struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_type_name_in_annotation() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_type_name_in_construction_call() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x: 1 });
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_named_typed_binding_shows_the_name() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn { let p$0 = Foo(struct { x: 1 }); p.x };
"#,
        "```must\np: Foo\n```",
    );
}

#[test]
fn highlights_named_types() {
    // The declaration and both reference positions (annotation,
    // construction head) carry the type highlight class; builtins keep
    // the library modifier, user types don't.
    check_highlights(
        r#"type Foo = struct { x: usize };
static f = fn (p: Foo) { Foo(struct { x: p.x }) };"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..8 "Foo" Type.declaration
            9..10 "=" Operator
            11..17 "struct" Keyword
            23..28 "usize" Type.defaultLibrary
            32..38 "static" Keyword
            39..40 "f" Function.declaration.static
            41..42 "=" Operator
            43..45 "fn" Keyword
            47..48 "p" Parameter.declaration
            50..53 "Foo" Type
            57..60 "Foo" Type
            61..67 "struct" Keyword
            73..74 "p" Parameter
        "#]],
    );
}

#[test]
fn type_item_file_evaluates_cleanly() {
    // Type items have no value; the eager check-eval loop must not invent
    // a diagnostic for them.
    let (analysis, file, _pos) =
        fixture("type Foo = struct { x: usize };\nstatic p = Foo(struct { x: 1 });$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, Vec::new(), "diagnostics: {diagnostics:?}");
}

// ---- enums and variants ----

#[test]
fn goto_enum_from_variant_path_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize) };
static s = Sha$0pe::Circle(3);
"#,
        "Shape",
        0,
    );
}

#[test]
fn goto_variant_from_variant_path() {
    // The second segment lands on the variant inside the declaration.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        "Point",
        0,
    );
}

#[test]
fn goto_variant_from_type_annotation() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "Circle",
        0,
    );
}

#[test]
fn hover_variant_value_shows_the_variant_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let c = Shape::Circle(3);
    c$0;
};
"#,
        "```must\nc: Shape::Circle\n```",
    );
}

#[test]
fn hover_widened_binding_shows_the_enum() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s = Shape::Point;
    s$0;
};
"#,
        "```must\nmut s: Shape\n```",
    );
}

#[test]
fn hover_enum_name_shows_the_declaration() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Sha$0pe::Point;
"#,
        "```must\ntype Shape = enum { Circle(usize), Point }\n```",
    );
}

#[test]
fn hover_variant_segment_shows_its_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        "```must\nPoint: Shape::Point\n```",
    );
}

#[test]
fn hover_variant_in_type_position_shows_the_declaration() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn highlights_enum_declaration_and_variant_paths() {
    check_highlights(
        r#"type Shape = enum { Circle(usize), Point };
static s: Shape::Circle = Shape::Circle(3);"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "s" Variable.declaration.static
            54..59 "Shape" Type
            61..67 "Circle" EnumMember
            68..69 "=" Operator
            70..75 "Shape" Type
            77..83 "Circle" EnumMember
            84..85 "3" Number
        "#]],
    );
}

// ---- match ----

#[test]
fn goto_variant_from_sigil_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 is the declaration inside the enum literal.
        "Circle",
        0,
    );
}

#[test]
fn goto_variant_from_qualified_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "Circle",
        0,
    );
}

#[test]
fn goto_enum_from_qualified_pattern_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape$0::Circle(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 of `Shape` is the `type` item's name.
        "Shape",
        0,
    );
}

#[test]
fn goto_on_bare_bind_named_like_variant_has_no_target() {
    // Bare `Point` (no `::`) is just a binding — its own declaration — even
    // though it spells a variant's name. Goto on it has nothing to jump to.
    check_no_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
    );
}

#[test]
fn goto_pattern_binding_use_in_arm_body() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r$0,
        _ => 0,
    }
};
"#,
        // `r` also occurs inside the two `Circle`s; occurrence 2 is the
        // pattern binding itself.
        "r",
        2,
    );
}

#[test]
fn hover_pattern_binding_shows_payload_type() {
    check_hover(
        r#"
type Shape = enum { Pair(usize, str) };
static f = fn (s: Shape) {
    match s {
        ::Pair(n, text$0) => text,
        _ => "",
    }
};
"#,
        "```must\ntext: str\n```",
    );
}

#[test]
fn hover_variant_name_in_pattern_shows_the_variant() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn hover_bind_arm_named_like_variant_shows_whole_scrutinee_type() {
    // Bare `Point` (no `::`) is a binding of the whole scrutinee, not a
    // variant match — hover shows the enum type, not the narrowed variant.
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
        "```must\nPoint: Shape\n```",
    );
}

#[test]
fn highlights_match_expression() {
    check_highlights(
        // A qualified arm (`Shape::Circle`), an elided sigil arm
        // (`::Point`, whose sole segment must still color as an enum
        // member), a bare bind whose name shadows a variant (`Point` —
        // now a plain `Variable`, never an `EnumMember`), and an ordinary
        // bind (`other`).
        r#"type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        ::Point => 0,
        Point => 1,
        other => 2,
    }
};"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "f" Function.declaration.static
            53..54 "=" Operator
            55..57 "fn" Keyword
            59..60 "s" Parameter.declaration
            62..67 "Shape" Type
            69..71 "->" Operator
            72..77 "usize" Type.defaultLibrary
            84..89 "match" Keyword
            90..91 "s" Parameter
            102..107 "Shape" Type
            109..115 "Circle" EnumMember
            116..117 "r" Variable.declaration
            119..121 "=>" Operator
            122..123 "r" Variable
            135..140 "Point" EnumMember
            141..143 "=>" Operator
            144..145 "0" Number
            155..160 "Point" Variable.declaration
            161..163 "=>" Operator
            164..165 "1" Number
            175..180 "other" Variable.declaration
            181..183 "=>" Operator
            184..185 "2" Number
        "#]],
    );
}

#[test]
fn highlights_loop_break_continue_keywords() {
    check_highlights(
        "static f = fn { loop { if true { break 1; }; continue; } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..20 "loop" Keyword
            23..25 "if" Keyword
            26..30 "true" Keyword
            33..38 "break" Keyword
            39..40 "1" Number
            45..53 "continue" Keyword
        "#]],
    );
}

#[test]
fn hover_on_loop_shows_its_type() {
    check_hover(
        r#"
static f = fn () -> usize {
    lo$0op {
        break 42;
    }
};
"#,
        "```must\nloop { … }: usize\n```",
    );
}

#[test]
fn hover_on_breakless_loop_shows_never() {
    check_hover(
        "static f = fn { lo$0op { } };",
        "```must\nloop { … }: !\n```",
    );
}

#[test]
fn hover_on_break_keyword_is_none() {
    // `break` names nothing; hover stays quiet (and doesn't panic) on it
    // and on its dangling outside-a-loop form.
    let (analysis, _file, pos) = fixture("static f = fn { loop { bre$0ak 1; } };");
    assert_eq!(analysis.hover(pos), None);
    let (analysis, _file, pos) = fixture("static f = fn { bre$0ak; };");
    assert_eq!(analysis.hover(pos), None);
}

// ---- record destructuring — hover and goto-def ----

#[test]
fn hover_record_destructured_binding_definition() {
    check_hover(
        r#"static f = fn { let struct { x$0, y } = struct { x: 1, y: "s" }; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_record_destructured_binding_use() {
    check_hover(
        r#"static f = fn { let struct { x, y } = struct { x: 1, y: "s" }; print(y$0); };"#,
        "```must\ny: str\n```",
    );
}

#[test]
fn hover_record_destructure_rename_shows_the_new_name() {
    check_hover(
        r#"static f = fn { let struct { x as alpha } = struct { x: 1 }; let b = alpha$0; };"#,
        "```must\nalpha: usize\n```",
    );
}

#[test]
fn hover_mut_field_binding_shows_mut() {
    check_hover(
        r#"static f = fn { let struct { mut x$0 } = struct { x: 1 }; x = 2; };"#,
        "```must\nmut x: usize\n```",
    );
}

#[test]
fn hover_param_record_destructured_binding() {
    check_hover(
        "static f = fn (struct { n$0 }: struct { n: usize }) { n };",
        "```must\nn: usize\n```",
    );
}

#[test]
fn hover_newtype_destructured_binding() {
    check_hover(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) { let Point(struct { x$0, y }) = p; x };
"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn goto_record_destructured_binding_use() {
    check_goto(
        r#"
static f = fn {
    let struct { x, y } = struct { x: 1, y: 2 };
    print(x$0);
}
"#,
        "x",
        // Occurrence 0 is the field/binding name in the pattern (its
        // declaration site — where the use inside `print` should jump to);
        // occurrence 1 is the record literal's field name.
        0,
    );
}

#[test]
fn goto_record_destructure_rename_use_lands_on_the_rename() {
    check_goto(
        r#"
static f = fn {
    let struct { x as alpha } = struct { x: 1 };
    print(alpha$0);
}
"#,
        "alpha",
        0,
    );
}

#[test]
fn goto_param_record_destructured_binding_use() {
    check_goto(
        "static f = fn (struct { num }: struct { num: usize }) { num$0 };",
        "num",
        0,
    );
}

/// Renders each completion as `label Kind (detail)`, one per line, in the
/// exact order `Analysis::completions` returns (already rank-sorted).
fn check_completions(fixture_text: &str, expect: expect_test::Expect) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let mut rendered = String::new();
    for item in analysis.completions(pos) {
        rendered.push_str(&item.label);
        rendered.push(' ');
        rendered.push_str(&format!("{:?}", item.kind));
        if let Some(detail) = &item.detail {
            rendered.push_str(&format!(" ({detail})"));
        }
        rendered.push('\n');
    }
    expect.assert_eq(&rendered);
}

fn check_no_completion(fixture_text: &str, label: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert!(
        !analysis.completions(pos).iter().any(|c| c.label == label),
        "expected no completion labeled {label:?}"
    );
}

fn check_has_completion(fixture_text: &str, label: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert!(
        analysis.completions(pos).iter().any(|c| c.label == label),
        "expected a completion labeled {label:?}"
    );
}

#[test]
fn completions_expression_position_ranks_locals_items_builtins_keywords() {
    // A call argument: expression position, but not a fresh statement — no
    // `let` among the keywords. The typed prefix `x` is a real
    // expression checked against `print`'s parameter, so the position
    // carries a `str` expectation — `panic` (returns `!`, which widens to
    // anything) is the one candidate whose *call* satisfies it, so its
    // type tier lifts it above every tier-2 candidate; below that the
    // provenance order (locals, items, builtins, keywords) is intact.
    check_completions(
        r#"
type Shape = struct { r: usize };
static area = fn (r: usize) -> usize { r * r };
static main = fn {
    let x = 1;
    print(x$0);
};
"#,
        expect_test::expect![[r#"
            panic Function (fn(str) -> !)
            x Variable (usize)
            Shape Struct (struct { r: usize })
            area Function (fn(usize) -> usize)
            main Function (fn())
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_mutable_local_carries_mut_in_detail() {
    // Doubles as the type-tier no-expectation regression pin: an empty
    // prefix in a call-argument hole is not one of `expected_type_at`'s two
    // provable shapes, so no type ranking applies and the provenance
    // ordering is untouched.
    check_completions(
        r#"
static main = fn {
    let mut x = 1;
    print($0);
};
"#,
        expect_test::expect![[r#"
            x Variable (mut usize)
            main Function (fn())
            panic Function (fn(str) -> !)
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_type_position_shows_only_types_and_builtins() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static make_point = fn (x: usize) -> $0 { x };
"#,
        expect_test::expect![[r#"
            Point Struct (struct { x: usize, y: usize })
            bool Keyword
            str Keyword
            string Keyword
            usize Keyword
            fn Keyword
            struct Keyword
        "#]],
    );
}

#[test]
fn completions_type_position_excludes_locals_and_value_items() {
    check_no_completion(
        r#"
static helper = fn { 1 };
static main = fn (n: $0) { n };
"#,
        "helper",
    );
}

#[test]
fn completions_statement_start_includes_let() {
    check_has_completion(
        r#"
static main = fn {
    let x = 1;
    $0
};
"#,
        "let",
    );
}

#[test]
fn completions_statement_start_sees_the_preceding_let() {
    // The trailing statement slot has no `ExprId` of its own (nothing was
    // ever lowered there) — this exercises the block-walk reconstruction in
    // `locals_in_block` rather than a plain `scope_of` lookup.
    check_has_completion(
        r#"
static main = fn {
    let x = 1;
    $0
};
"#,
        "x",
    );
}

#[test]
fn completions_expression_position_has_no_let() {
    check_no_completion(
        r#"
static main = fn {
    print($0);
};
"#,
        "let",
    );
}

#[test]
fn completions_break_continue_only_inside_a_loop() {
    check_has_completion(
        r#"
static main = fn {
    loop {
        $0
    }
};
"#,
        "break",
    );
    check_has_completion(
        r#"
static main = fn {
    loop {
        $0
    }
};
"#,
        "continue",
    );
    check_no_completion(
        r#"
static main = fn {
    $0
};
"#,
        "break",
    );
}

#[test]
fn completions_break_does_not_cross_a_nested_fn_literal_boundary() {
    // A closure written inside a loop's body is its own const-check/loop
    // boundary (mirrors `hir::infer`'s `loop_sinks` save/restore) — `break`
    // here couldn't exit the outer loop, so it shouldn't be offered.
    check_no_completion(
        r#"
static main = fn {
    loop {
        let inner = fn { $0 };
    }
};
"#,
        "break",
    );
}

#[test]
fn completions_top_level_offers_exactly_static_const_type() {
    check_completions(
        "$0",
        expect_test::expect![[r#"
            const Keyword
            static Keyword
            type Keyword
        "#]],
    );
}

#[test]
fn completions_empty_file_top_level() {
    // Same shape as the general top-level case, exercised on a genuinely
    // empty file (no tokens at all for the speculative parse to hang off).
    check_completions(
        "$0",
        expect_test::expect![[r#"
            const Keyword
            static Keyword
            type Keyword
        "#]],
    );
}

#[test]
fn completions_typed_prefix_still_classifies_and_edit_covers_the_prefix() {
    let (analysis, _file, pos) = fixture(
        r#"
static main = fn {
    let price = 1;
    pri$0
};
"#,
    );
    let items = analysis.completions(pos);
    let local = items
        .iter()
        .find(|c| c.label == "price")
        .expect("`price` offered even though only `pri` was typed — filtering is client-side");
    // `pri` sits right before the cursor; the edit replaces exactly that,
    // not the identifier that would follow it if there were one.
    let prefix_start = pos.offset - syntax::TextSize::new(3);
    assert_eq!(local.text_edit.range.start(), prefix_start);
    assert_eq!(local.text_edit.range.end(), pos.offset);
}

#[test]
fn completions_cursor_mid_identifier_of_an_existing_name() {
    let (analysis, _file, pos) = fixture(
        r#"
static main = fn {
    let abc = 1;
    print(ab$0c);
};
"#,
    );
    let items = analysis.completions(pos);
    assert!(
        items.iter().any(|c| c.label == "abc"),
        "still classifies as expression position and offers the shadowed-over local"
    );
    let edited = items.iter().find(|c| c.label == "abc").unwrap();
    // Only `ab` (the part before the cursor) is replaced; the `c` that
    // follows is left alone.
    let prefix_start = pos.offset - syntax::TextSize::new(2);
    assert_eq!(edited.text_edit.range.start(), prefix_start);
    assert_eq!(edited.text_edit.range.end(), pos.offset);
}

#[test]
fn completions_inside_a_string_literal_is_empty() {
    let (analysis, _file, pos) = fixture(r#"static main = fn { print("hi $0"); };"#);
    assert_eq!(analysis.completions(pos), Vec::new());
}

#[test]
fn completions_inside_a_comment_is_empty() {
    let (analysis, _file, pos) = fixture("// hi $0\nstatic main = fn { 1 };");
    assert_eq!(analysis.completions(pos), Vec::new());
}

// ---- field access ----

#[test]
fn completions_dot_field_access_on_record_local() {
    check_completions(
        r#"
static f = fn {
    let p = struct { x: 1, y: 2 };
    p.$0
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_through_named_type_erasure() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) {
    p.$0
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_chained() {
    check_completions(
        r#"
type Inner = struct { z: usize };
type Outer = struct { inner: Inner };
static f = fn (o: Outer) {
    o.inner.$0
};
"#,
        expect_test::expect![[r#"
            z Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_non_record_receiver_is_empty() {
    check_completions(
        r#"
static f = fn (n: usize) {
    n.$0
};
"#,
        expect_test::expect![""],
    );
}

#[test]
fn completions_dot_field_access_unknown_receiver_is_empty() {
    check_completions(
        r#"
static f = fn {
    nope.$0
};
"#,
        expect_test::expect![""],
    );
}

// ---- `::` second segment (member completions) ----

#[test]
fn completions_variant_segment_expression_position() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    Shape::$0
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_variant_segment_type_position() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape::$0) {};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_variant_segment_after_struct_type_is_empty() {
    check_completions(
        r#"
type Point = struct { x: usize };
static f = fn {
    Point::$0
};
"#,
        expect_test::expect![""],
    );
}

// ---- match-arm pattern position ----

#[test]
fn completions_match_arm_uncovered_variants_ranked_first() {
    // One arm (`::Point`) already written and covered; the bare slot of a
    // fresh arm should rank the two *uncovered* variants above the
    // already-covered one (rank-down, not filtered — an arm matching it
    // again would be unreachable, but it's still shown), with `_` last.
    // The bare slot labels/inserts the sigil form `::Variant` (see
    // `match_arm_items`' doc).
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point, Square(usize) };
static f = fn (s: Shape) {
    match s {
        ::Point => 1,
        $0
    }
};
"#,
        expect_test::expect![[r#"
            ::Circle EnumMember (Circle(usize))
            ::Square EnumMember (Square(usize))
            ::Point EnumMember (Point)
            _ Keyword
        "#]],
    );
}

#[test]
fn completions_match_arm_sigil_context_inserts_bare_variant_name() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        ::$0
    }
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_match_arm_qualified_context_inserts_bare_variant_name() {
    // Past an already-typed `Enum::`, same bare spelling as the elided
    // sigil — the qualifier is already on screen.
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_match_arm_bare_slot_inserts_sigil_variant_and_offers_wildcard() {
    // A bare pattern slot has no `::` at all yet — a bare `Circle` insert
    // would just bind a fresh local named `Circle` (G25: bare names
    // always bind), so this inserts the sigil form `::Circle` (label =
    // insertion; the filter text stays the bare name so a typed `Cir`
    // still matches). `_` is also offered (it's only reachable from a
    // bare slot — there's no way to spell it after a `::`).
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        $0
    }
};
"#;
    check_has_completion(fixture_text, "::Circle");
    check_no_completion(fixture_text, "Circle");
    check_no_completion(fixture_text, "Shape::Circle");
    check_has_completion(fixture_text, "_");

    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    let circle = items.iter().find(|c| c.label == "::Circle").unwrap();
    // `Circle` carries a payload, so the bare slot's sigil insertion is
    // a snippet with a tab-stop for it; a snippet-incapable client falls
    // back to the bare `::Circle` (no parens — the payload-less form).
    assert_eq!(
        circle.text_edit.insert,
        crate::InsertText::Snippet {
            snippet: "::Circle($1)".to_owned(),
            plain: "::Circle".to_owned(),
        }
    );
    assert_eq!(circle.filter_text, "Circle");
}

#[test]
fn completions_match_arm_non_enum_scrutinee() {
    // A bare slot still offers the universal `_`; a `::`-sigil slot has no
    // enum to draw variants from, so it offers nothing at all.
    check_completions(
        r#"
static f = fn (s: usize) {
    match s {
        $0
    }
};
"#,
        expect_test::expect![[r#"
            _ Keyword
        "#]],
    );
    check_completions(
        r#"
static f = fn (s: usize) {
    match s {
        ::$0
    }
};
"#,
        expect_test::expect![""],
    );
}

// ---- record-literal field names ----

#[test]
fn completions_record_literal_missing_fields_excludes_already_written() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    Point(struct { x: 1, $0 })
};
"#,
        expect_test::expect![[r#"
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_matching_local_ranked_first() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let y = 5;
    Point(struct { x: 1, $0 })
};
"#,
        expect_test::expect![[r#"
            y Variable (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_annotated_let_context() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let p: Point = struct { $0 };
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_no_expectation_fallback_is_expression_position() {
    // No construction head, no type annotation: the record's shape says
    // nothing here, so this falls back to ordinary expression-position
    // candidates rather than going silent.
    check_has_completion(
        r#"
static f = fn {
    let p = struct { $0 };
};
"#,
        "if",
    );
    check_no_completion(
        r#"
static f = fn {
    let p = struct { $0 };
};
"#,
        "let",
    );
}

// ---- `type X = …` RHS ----

#[test]
fn completions_type_item_rhs_offers_exactly_struct_and_enum() {
    check_completions(
        "type X = $0;",
        expect_test::expect![[r#"
            enum Keyword
            struct Keyword
        "#]],
    );
}

// ---- type-directed ranking ----

#[test]
fn completions_type_ranking_exact_local_above_mismatched_local() {
    // The typed prefix `s` sits in an annotated `let`'s initializer, so
    // the position expects `str`: the `str` local ranks tier 0, `panic`
    // (returns `!`) tier 1 via its return type, everything else tier 2 in
    // the usual provenance order — the mismatched `usize` local included.
    check_completions(
        r#"
static main = fn (s: str, n: usize) {
    let want: str = s$0;
};
"#,
        expect_test::expect![[r#"
            s Variable (str)
            panic Function (fn(str) -> !)
            n Variable (usize)
            main Function (fn(str, usize))
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_matching_return_type_above_mismatched() {
    // `g` resolves to nothing, but the *position* (an annotated `let`'s
    // initializer) still expects `str`: `get_s`'s call would satisfy it
    // (tier 1, like `panic`), `get_n`'s wouldn't (tier 2).
    check_completions(
        r#"
static get_s: fn() -> str = fn () -> str { "s" };
static get_n: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let x: str = g$0;
};
"#,
        expect_test::expect![[r#"
            get_s Function (fn() -> str)
            panic Function (fn(str) -> !)
            get_n Function (fn() -> usize)
            main Function (fn())
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_fn_typed_expectation_ranks_the_fn_itself_exact() {
    // Under a `fn`-typed expectation the matching function is tier 0 as a
    // *value* (passing it, not calling it); `panic` still reaches tier 1
    // through its `!` return, everything else stays tier 2.
    check_completions(
        r#"
static helper: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let f: fn() -> usize = h$0;
};
"#,
        expect_test::expect![[r#"
            helper Function (fn() -> usize)
            panic Function (fn(str) -> !)
            main Function (fn())
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_fresh_hole_after_annotated_let_eq() {
    // No prefix at all: `let x: Foo = |` lowers the absent initializer as
    // a `Missing` expression checked against the annotation — the one
    // no-prefix shape `expected_type_at` can prove. The `Point`-typed
    // local ranks tier 0 over the `usize` one.
    check_completions(
        r#"
type Point = struct { x: usize };
static main = fn (p: Point, n: usize) {
    let q: Point = $0;
};
"#,
        expect_test::expect![[r#"
            p Variable (Point)
            panic Function (fn(str) -> !)
            n Variable (usize)
            Point Struct (struct { x: usize })
            main Function (fn(Point, usize))
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_variant_under_enum_expectation() {
    // A variant candidate under its own enum's expectation widens to it
    // (tier 1): its sort key outranks every unrankable/mismatching
    // (tier 2) item's.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static main = fn {
    let s: Shape = Shape::C$0;
};
"#,
    );
    let items = analysis.completions(pos);
    let circle = items
        .iter()
        .find(|c| c.label == "Circle")
        .expect("Circle is offered");
    assert_eq!(circle.sort_text, "1_20_Circle");
    assert!(
        circle.sort_text.as_str() < "2_00_",
        "outranks any tier-2 item"
    );

    // Control: under a non-matching expectation the same variant is
    // tier 2 — type ranking is the expectation's doing, not the context's.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static main = fn {
    let n: usize = Shape::C$0;
};
"#,
    );
    let items = analysis.completions(pos);
    let circle = items
        .iter()
        .find(|c| c.label == "Circle")
        .expect("Circle is offered");
    assert_eq!(circle.sort_text, "2_20_Circle");
}

#[test]
fn completions_type_ranking_match_arm_variants_carry_the_widening_tier() {
    // Match-arm variant candidates rank against the scrutinee's enum: all
    // widen (tier 1), with gold-before-covered preserved inside the tier
    // and `_` staying an unranked keyword.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static f = fn (s: Shape) -> usize {
    match s { ::Square => 2, $0 }
};
"#,
    );
    let items = analysis.completions(pos);
    let sort_text = |label: &str| {
        items
            .iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label} is offered"))
            .sort_text
            .clone()
    };
    assert_eq!(sort_text("::Circle"), "1_00_::Circle", "uncovered: gold");
    assert_eq!(sort_text("::Square"), "1_20_::Square", "covered: item");
    assert_eq!(sort_text("_"), "2_40__");
}

#[test]
fn completions_type_ranking_statement_start_without_prefix_keeps_untyped_ordering() {
    // A fresh statement slot with no prefix has no provable expectation:
    // the provenance ordering (locals, items, builtins, keywords) is
    // untouched.
    check_completions(
        r#"
static main = fn {
    let s = "hi";
    $0
};
"#,
        expect_test::expect![[r#"
            s Variable (str)
            main Function (fn())
            panic Function (fn(str) -> !)
            print Function (fn(str))
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            let Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
        "#]],
    );
}

// ---- snippets ----

/// The `InsertText` of the one completion labeled `label` (of any kind).
/// Panics on zero or multiple matches — most snippet fixtures below only ever
/// produce one candidate under that label.
fn completion_insert(fixture_text: &str, label: &str) -> crate::InsertText {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"))
        .text_edit
        .insert
        .clone()
}

/// Same as [`completion_insert`], but disambiguated by kind too — needed
/// where a shorthand-matching local and a field share one label (the
/// record-literal gold case).
fn completion_insert_kind(
    fixture_text: &str,
    label: &str,
    kind: crate::CompletionItemKind,
) -> crate::InsertText {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label && c.kind == kind)
        .unwrap_or_else(|| panic!("no completion labeled {label:?} of kind {kind:?}: {items:?}"))
        .text_edit
        .insert
        .clone()
}

#[test]
fn completions_snippet_fn_call_with_params_inserts_a_snippet() {
    // A parameterful fn in expression position offers `name($1)`, falling
    // back to a bare `name()` for a snippet-incapable client.
    assert_eq!(
        completion_insert(
            r#"
static add = fn (a: usize, b: usize) -> usize { a };
static main = fn {
    $0
};
"#,
            "add",
        ),
        crate::InsertText::Snippet {
            snippet: "add($1)".to_owned(),
            plain: "add()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_zero_arity_fn_call_is_plain() {
    // No parameters, nothing for a tab stop to land on: always the plain
    // `name()`, snippet or not.
    assert_eq!(
        completion_insert(
            r#"
static go = fn { 1 };
static main = fn {
    $0
};
"#,
            "go",
        ),
        crate::InsertText::Plain("go()".to_owned())
    );
}

#[test]
fn completions_snippet_fn_typed_expectation_suppresses_the_call_snippet() {
    // Under a `fn`-typed expectation the position wants the fn *passed*,
    // not called — even though `helper` itself is nominally zero-arity, the
    // insertion is the bare name with no call syntax at all (distinct from
    // the zero-arity case above, which still inserts `()`).
    assert_eq!(
        completion_insert(
            r#"
static helper: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let f: fn() -> usize = h$0;
};
"#,
            "helper",
        ),
        crate::InsertText::Plain("helper".to_owned())
    );
}

#[test]
fn completions_snippet_builtin_fn_call_with_params() {
    // Builtins go through the same snippet rule as file items — `print`
    // takes one param.
    assert_eq!(
        completion_insert(
            r#"
static main = fn {
    $0
};
"#,
            "print",
        ),
        crate::InsertText::Snippet {
            snippet: "print($1)".to_owned(),
            plain: "print()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_type_item_rhs_struct_and_enum() {
    let fixture_text = "type X = $0;";
    assert_eq!(
        completion_insert(fixture_text, "struct"),
        crate::InsertText::Snippet {
            snippet: "struct { $1 }".to_owned(),
            plain: "struct { }".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "enum"),
        crate::InsertText::Snippet {
            snippet: "enum { $1 }".to_owned(),
            plain: "enum { }".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_match_arm_payload_variant_qualified_context() {
    // Past an already-typed qualifier, a payload-carrying variant inserts
    // just the bare name plus its own tab-stopped parens; a payload-less
    // one is untouched (still a plain bare name, the payload-less form).
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "Circle"),
        crate::InsertText::Snippet {
            snippet: "Circle($1)".to_owned(),
            plain: "Circle".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "Point"),
        crate::InsertText::Plain("Point".to_owned())
    );
}

#[test]
fn completions_snippet_record_literal_missing_field_inserts_a_snippet() {
    assert_eq!(
        completion_insert(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    Point(struct { x: 1, $0 })
};
"#,
            "y",
        ),
        crate::InsertText::Snippet {
            snippet: "y: $1".to_owned(),
            plain: "y: ".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_record_literal_gold_local_stays_a_plain_bare_name() {
    // The shorthand-matching local (`y` meaning `y: y`) is not the field
    // slot itself — it keeps inserting just its own name, snippet-free.
    assert_eq!(
        completion_insert_kind(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let y = 5;
    Point(struct { x: 1, $0 })
};
"#,
            "y",
            crate::CompletionItemKind::Variable,
        ),
        crate::InsertText::Plain("y".to_owned())
    );
}
