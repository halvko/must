use crate::{AnalysisHost, FilePosition};
use base_db::SourceFile;
use syntax::TextSize;

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

fn check_hover(fixture_text: &str, expected_markup: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let hover = analysis.hover(pos).expect("hover returned None");
    assert_eq!(hover.markup, expected_markup);
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
fn non_block_fn_body_diagnostic_carries_wrap_fix() {
    let (analysis, file, _pos) = fixture("static f = fn 42$0;");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Wrap in `{ }`");
    // Insert "{ " before `42` (offset 14) and " }" after it (offset 16).
    assert_eq!(fix.edits.len(), 2);
    assert_eq!(u32::from(fix.edits[0].range.start()), 14);
    assert_eq!(fix.edits[0].insert, "{ ");
    assert_eq!(u32::from(fix.edits[1].range.start()), 16);
    assert_eq!(fix.edits[1].insert, " }");
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
    assert_eq!(fix.edits[0].insert, ";");
    assert!(fix.edits[0].range.is_empty());
    assert_eq!(fix.edits[0].range.start(), diagnostics[0].range.end());
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
    let (analysis, file, _pos) = fixture(
        "static f = fn () -> usize { let a = 1; let g = fn () -> usize { a$0 }; g() };",
    );
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
fn diagnostics_include_name_errors() {
    let (analysis, file, _pos) = fixture("static f = fn { missing$0() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unresolved name `missing`");
}
