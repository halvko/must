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
    check_goto("static f = fn (count: usize) count$0 + 1;", "count", 0);
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
static f = fn (n: usize) -> usize n;
static main = fn { f$0(1); };
"#,
        "```must\nf: fn(usize) -> usize\n```",
    );
}

#[test]
fn diagnostics_include_name_errors() {
    let (analysis, file, _pos) = fixture("static f = fn { missing$0() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unresolved name `missing`");
}
