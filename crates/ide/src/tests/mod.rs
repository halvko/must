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

const BROKEN: &str = "static = true;";

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

/// The item is a call the checker refuses: sorted in the refused tier (P16).
fn is_refused(item: &crate::CompletionItem) -> bool {
    item.sort_text
        .starts_with(&crate::completions::TYPE_TIER_REFUSED.to_string())
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

/// `expected_markup` may open and close on lines of its own (a raw string);
/// those surrounding newlines are not part of the markup.
fn check_hover(fixture_text: &str, expected_markup: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let hover = analysis.hover(pos).expect("hover returned None");
    assert_eq!(hover.markup, expected_markup.trim_matches('\n'));
}

fn check_no_hover(fixture_text: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert_eq!(analysis.hover(pos), None);
}

/// The text after the only diagnostic's one-edit fix, and the fix's label.
fn apply_only_fix(text: &str) -> (String, String) {
    let (analysis, file, _pos) = fixture(&format!("{text}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.edits.len(), 1);
    let edit = &fix.edits[0].edit;
    let mut fixed = text.to_owned();
    fixed.replace_range(std::ops::Range::<usize>::from(edit.range), &edit.insert);
    (fix.label.clone(), fixed)
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

// ---- match-scrutinee ranking ----

/// Every layer of the scrutinee ranking in one body: a `let` in the
/// cursor's own scope, one further out, a fn parameter, a file item — and a
/// local the lens does not reach, which stays where every local has always
/// sorted.
const SCRUTINEE_LAYERS: &str = r#"
type Shape = enum { Circle(usize), Point };
static ambient: Shape = Shape::Point;
static f = fn (param: Shape) {
    let outer: Shape = param;
    let noise: str = "x";
    {
        let near: Shape = outer;
        match $0
    }
};
"#;

// ---- inherent members and dot-calls -------------------------------------

const MEMBER_FIXTURE_HEAD: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Self(struct { n = c.n + by }) };
    }
};
"#;

/// Applies the `label` completion's edit to the fixture, its snippet
/// spelling when it has one, and renders the line the cursor was on plus
/// the item's filter text.
fn check_completion_applied(fixture_text: &str, label: &str, expect: expect_test::Expect) {
    let (analysis, file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    let item = items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"));
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
    expect.assert_eq(&format!(
        "{}\nfilter: {}\n",
        text[line_start..line_end].trim(),
        item.filter_text
    ));
}

const BORROWED_MEMBER_FIXTURE: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        take = fn(s: Self) -> usize { s.n };
        get = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.n };
        add = fn::<@a>(by: usize, s: Self.&mut::<@a>) -> () { s.*.n = s.*.n + by; };
    }
};
"#;

/// A `main` declaring `let mut c: Counter`, with `BODY` replaced by `body`.
fn borrowed_member_main(body: &str) -> String {
    const MAIN: &str = r#"
static main = fn () -> () {
    let mut c: Counter = Counter(struct { n = 1 });
    BODY
};
"#;
    format!("{BORROWED_MEMBER_FIXTURE}{}", MAIN.replace("BODY", body))
}

// ---- a bound is a completion source ------------------------------------

/// The owner's shape: a `Write`-bounded writer, borrowed exclusively.
const BOUND_FIXTURE_HEAD: &str = r#"
trait Write = requires {
    push: fn::<@r>(char, Self.&mut::<@r>) -> ();
};
"#;

/// Two bounds, both declaring `push`, DIFFERENT shapes — and only one of
/// them fits the exclusive borrow this receiver is.
const AMBIGUOUS_SHAPES_HEAD: &str = r#"
trait A = requires {
    push: fn::<@r>(char, Self.&mut::<@r>) -> ();
    grow: fn::<@r>(Self.&mut::<@r>) -> ();
};
trait B = requires { push: fn(char, Self) -> (); };
"#;

/// The text after applying the one "insert inferred signature" assist
/// offered at `$0`, and its label; `None` when none is offered.
fn apply_assist(text: &str) -> Option<(String, String)> {
    let (analysis, file, pos) = fixture(text);
    let assists = analysis.assists(file, TextRange::empty(pos.offset));
    assert!(assists.len() <= 1, "{assists:#?}");
    let fix = assists.into_iter().next()?;
    Some((fix.label.clone(), apply_fix(&text.replace("$0", ""), &fix)))
}

fn apply_fix(text: &str, fix: &crate::Fix) -> String {
    let mut edits: Vec<_> = fix.edits.iter().map(|it| it.edit.clone()).collect();
    edits.sort_by_key(|edit| edit.range.start());
    let mut fixed = text.to_owned();
    for edit in edits.iter().rev() {
        fixed.replace_range(std::ops::Range::<usize>::from(edit.range), &edit.insert);
    }
    fixed
}

fn diagnostic_messages(text: &str) -> Vec<String> {
    let (analysis, file, _) = fixture(&format!("{}$0", text.replace("$0", "")));
    analysis
        .diagnostics(file)
        .into_iter()
        .map(|it| it.message)
        .collect()
}

/// Asserts the assist at `$0` rewrites `before` into `after`, and that the
/// result is fully typed: no assist is left to offer, and no diagnostic.
fn check_assist(before: &str, label: &str, after: &str) {
    let (got_label, fixed) = apply_assist(before).expect("an assist is offered");
    assert_eq!(got_label, label);
    assert_eq!(fixed, after);
    let cursor = before.find("$0").unwrap();
    let mut again = fixed.clone();
    again.insert_str(cursor, "$0");
    assert_eq!(apply_assist(&again), None, "offered again after applying");
    let (analysis, file, _) = fixture(&again);
    assert_eq!(analysis.diagnostics(file), Vec::new());
}
