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
    assert_eq!(errors[0].related.len(), 1);
    assert_eq!(errors[0].related[0].message, "`double` is defined here");
    assert_eq!(&src[errors[0].related[0].range], "double");
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
