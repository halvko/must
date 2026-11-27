mod annotated_callers;
mod arrays;
mod assignment;
mod block_tail;
mod borrow_receivers;
mod borrowed_match;
mod borrows;
mod builtins;
mod const_context;
mod destructuring;
mod enums;
mod expectations;
mod extern_items;
mod generic_fns;
mod generic_types;
mod heap;
mod inference;
mod integers;
mod joins;
mod let_patterns;
mod literal_patterns;
mod loops_and_return;
mod matches;
mod member_generics;
mod members;
mod multi_file;
mod poison;
mod raw_pointers;
mod records;
mod regions;
mod resolve;
mod temporaries;
mod traits;
mod type_decls;
mod unsafe_fn;

use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

fn check_diagnostics(text: &str, expect: Expect) {
    expect.assert_eq(&render_diagnostics(text));
}

fn render_diagnostics(text: &str) -> String {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    crate::file_diagnostics(&db, file)
        .into_iter()
        .map(|d| {
            let related = d
                .related
                .iter()
                .map(|r| format!(" ({} at {:?})", r.message, r.range))
                .collect::<String>();
            format!("{:?}: {}{related}\n", d.range, d.message)
        })
        .collect::<String>()
}

/// Renders every expression and binding with its inferred type,
/// rust-analyzer style: `range 'snippet': type`.
fn check_infer(text: &str, expect: Expect) {
    expect.assert_eq(&render_infer(text));
}

fn render_infer(text: &str) -> String {
    render_rows(text, infer_pairs(text))
}

/// [`check_infer`] limited to the rows inside `snippet`, which must occur
/// once in `text`, so the rest of the program's types are not pinned.
fn check_infer_within(text: &str, snippet: &str, expect: Expect) {
    let start = text.find(snippet).expect("the snippet is in the text");
    assert_eq!(text.rfind(snippet), Some(start), "the snippet occurs once");
    let within = syntax::TextRange::at(
        syntax::TextSize::new(start as u32),
        syntax::TextSize::new(snippet.len() as u32),
    );
    let rows = infer_pairs(text)
        .into_iter()
        .filter(|(range, _)| within.contains_range(*range));
    expect.assert_eq(&render_rows(text, rows));
}

fn render_rows(text: &str, rows: impl IntoIterator<Item = (syntax::TextRange, String)>) -> String {
    rows.into_iter()
        .map(|(range, ty)| {
            let snippet: String = text[range].replace('\n', " ");
            let snippet = if snippet.len() > 20 {
                format!("{}...", &snippet[..17])
            } else {
                snippet
            };
            format!("{range:?} '{snippet}': {ty}\n")
        })
        .collect::<String>()
}

/// Every expression and binding as a (range, type) pair, sorted — the
/// structured form [`render_infer`] renders.
///
/// A test that COMPARES TWO PROGRAMS must use this, never the rendered
/// string: the rendering embeds a source snippet, so it reports a
/// difference wherever the two texts differ at all — and, worse, hides one
/// wherever the 17-character truncation happens to elide the difference.
/// Comparing types at ranges asks the question the test actually means.
fn infer_pairs(text: &str) -> Vec<(syntax::TextRange, String)> {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut pairs = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (_, source_map) = crate::body_with_source_map(&db, item);
        let result = crate::infer::infer(&db, item);
        for (expr, ty) in result.type_of_expr.iter() {
            if let Some(ptr) = source_map.node_for_expr(expr) {
                pairs.push((ptr.text_range(), ty.display()));
            }
        }
        for (binding, ty) in result.type_of_binding.iter() {
            if let Some(ptr) = source_map.node_for_binding(binding) {
                pairs.push((ptr.text_range(), ty.display()));
            }
        }
    }
    pairs.sort_by_key(|(range, _)| (range.start(), range.end()));
    pairs
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

/// Each top-level value item's published signature, one per line.
fn render_signatures(text: &str) -> String {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let tree = crate::item_tree::item_tree(&db, file);
    crate::file_item_ids(&db, file)
        .iter()
        .zip(&tree.items)
        .filter(|(_, data)| matches!(data.kind, crate::item_tree::ItemKind::Value(_)))
        .map(|(&item, data)| {
            format!(
                "{}: {}\n",
                data.name,
                crate::ty::signature(&db, item).display()
            )
        })
        .collect()
}

fn check_signatures(text: &str, expect: Expect) {
    expect.assert_eq(&render_signatures(text));
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

/// The fix on `text`'s only diagnostic, if any.
fn fix_of(text: &str) -> Option<syntax::Fix> {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    diagnostics[0].fix.clone()
}

/// The fixes on all of `text`'s diagnostics.
fn fixes_of(text: &str) -> Vec<syntax::Fix> {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    (crate::file_diagnostics(&db, file).into_iter())
        .filter_map(|d| d.fix)
        .collect()
}

/// `text` with `edits` applied.
fn apply_edits(text: &str, mut edits: Vec<syntax::TextEdit>) -> String {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start()));
    let mut fixed = text.to_owned();
    for edit in edits {
        fixed.replace_range(std::ops::Range::<usize>::from(edit.range), &edit.insert);
    }
    fixed
}

/// The program `text` becomes once the fix labelled `label` on its only
/// diagnostic is applied.
fn apply_fix(text: &str, label: &str) -> String {
    let fix = fix_of(text).expect("diagnostic has a fix");
    assert_eq!(fix.label, label);
    apply_edits(text, fix.edits)
}

/// The program `text` becomes once the fix labelled `label`, on whichever
/// of its diagnostics carries it, is applied.
fn apply_fix_among(text: &str, label: &str) -> String {
    let fix = (fixes_of(text).into_iter())
        .find(|fix| fix.label == label)
        .unwrap_or_else(|| panic!("no fix labelled {label:?} in {:?}", fixes_of(text)));
    apply_edits(text, fix.edits)
}

// ---- `unsafe fn` — unsafety lives in the function type ------------------

/// A safe function, an `extern` item (`unsafe fn`), and a higher-order
/// function that takes either — the shapes every test below draws from.
const UNSAFE_FN_PRELUDE: &str = r#"
unsafe extern static read: unsafe fn(u8.&raw, usize) -> isize;
static safe_read = fn(buf: u8.&raw, len: usize) -> isize { 0 };
"#;

/// Checks `body` after the prelude, and returns the whole text it checked.
fn check_unsafe_fn(body: &str, expect: Expect) -> String {
    let text = format!("{UNSAFE_FN_PRELUDE}{body}");
    check_diagnostics(&text, expect);
    text
}
