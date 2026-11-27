mod completions;
mod diagnostics;
mod dot_completions;
mod goto;
mod highlights;
mod hover;
mod inferred_signature;
mod match_completions;
mod snippet_completions;

use crate::{AnalysisHost, FilePosition};
use base_db::SourceFile;
use syntax::{TextRange, TextSize};

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

/// `expected_markup` may open and close on lines of its own (a raw string);
/// those surrounding newlines are not part of the markup.
fn check_hover(fixture_text: &str, expected_markup: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let hover = analysis.hover(pos).expect("hover returned None");
    assert_eq!(hover.markup, expected_markup.trim_matches('\n'));
}

/// Renders each completion as `label Kind (detail)`, one per line, in the
/// exact order `Analysis::completions` returns (already rank-sorted).
fn check_completions(fixture_text: &str, expect: expect_test::Expect) {
    check_completions_head(fixture_text, usize::MAX, expect);
}

/// [`check_completions`], rendering only the first `rows` candidates: for
/// ranking claims about the head of the list, so the builtin and keyword
/// tail below it is not pinned again by every such test.
fn check_completions_head(fixture_text: &str, rows: usize, expect: expect_test::Expect) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let mut rendered = String::new();
    for item in analysis.completions(pos).into_iter().take(rows) {
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

// ---- The `match` arm-list template ----

/// The sort key of one labelled candidate — read directly rather than
/// eyeballing a rendered order.
fn completion_sort_text(fixture_text: &str, label: &str) -> String {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"))
        .sort_text
        .clone()
}

// ---- inherent members and dot-calls -------------------------------------

const MEMBER_FIXTURE_HEAD: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Self(struct { n = c.n + by }) };
    }
};
"#;

/// Every candidate's detail and the line it leaves once applied: for rows
/// that share a label, so each is pinned to the text it writes.
fn check_completions_applied(fixture_text: &str, expect: expect_test::Expect) {
    let (analysis, file, pos) = fixture(fixture_text);
    let mut rendered = String::new();
    for item in analysis.completions(pos) {
        let detail = item.detail.as_deref().unwrap_or_default();
        let line = applied_line(&analysis, file, &item);
        rendered.push_str(&format!(
            r#"{detail}: {line}
"#
        ));
    }
    expect.assert_eq(&rendered);
}

/// The line holding `item`'s edit, with the edit applied.
fn applied_line(
    analysis: &crate::Analysis,
    file: SourceFile,
    item: &crate::CompletionItem,
) -> String {
    let insert = match &item.text_edit.insert {
        crate::InsertText::Plain(text) => text,
        crate::InsertText::Snippet { snippet, .. } => snippet,
    };
    let mut text = analysis.file_text(file).to_string();
    let range = item.text_edit.range;
    text.replace_range(usize::from(range.start())..usize::from(range.end()), insert);
    let line_start = text[..usize::from(range.start())]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    text[line_start..line_end].trim().to_owned()
}
