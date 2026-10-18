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
    infer_pairs(text)
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

/// The one diagnostic a fixture produces, or a panic naming what it got.
fn only_diagnostic(text: &str) -> crate::Diagnostic {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let [diagnostic] = &diagnostics[..] else {
        panic!("expected exactly one diagnostic, got {diagnostics:?}");
    };
    diagnostic.clone()
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

fn render_diagnostics_with_big_group(text: &str, big_inference_group: Option<u32>) -> String {
    use salsa::Setter as _;
    let mut db = RootDatabase::default();
    base_db::settings(&db)
        .set_big_inference_group(&mut db)
        .to(big_inference_group);
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    crate::file_diagnostics(&db, file)
        .into_iter()
        .map(|d| {
            let related = d
                .related
                .iter()
                .map(|r| format!(" ({} at {:?})", r.message, r.range))
                .collect::<String>();
            format!("{:?} {:?}: {}{related}\n", d.severity, d.range, d.message)
        })
        .collect::<String>()
}

// ---- trait declarations: coherence, matching, bounds, resolution --------

/// The proof shape: both impl homes, a bounded generic fn, a
/// generic requirement, bound-directed and impl-directed calls, the
/// qualified short form. Must be diagnostics-clean.
const TRAIT_FIXTURE: &str = r#"
trait Write = requires {
    push: fn(str, Self) -> Self;
};
trait Display = requires {
    fmt: fn::<W: Write>(W, Self) -> W;
} with {
    impl str {
        fmt = fn::<W: Write>(w: W, x: str) -> W { w.push(x) };
    }
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("n") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self { Sink(struct { pushes = w.pushes + 1 }) };
    }
};
type Point = struct { x: usize, y: usize } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, p: Self) -> W {
            let w = Display::fmt(w, p.x);
            Display::fmt(w, p.y)
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn() -> () {
    let a = show::<usize>(1);
    let b = show::<str>("s");
    let c = show::<Point>(Point(struct { x = 1, y = 2 }));
    let n: usize = 7;
    let d = n.fmt(Sink(struct { pushes = 0 }));
    let e = Display::fmt(Sink(struct { pushes = 0 }), "q");
    let _ = a + b + c + d.pushes + e.pushes;
};
"#;

/// Applies the single diagnostic's fix to `text`, checking its label, and
/// returns the fixed program.
fn apply_insertion_fix(text: &str, label: &str) -> String {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, label);
    assert_eq!(fix.edits.len(), 1);
    assert!(fix.edits[0].range.is_empty());
    let mut fixed = text.to_owned();
    fixed.insert_str(
        u32::from(fix.edits[0].range.start()) as usize,
        &fix.edits[0].insert,
    );
    fixed
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

// ---- match projects through borrows -------------------------------------
//
// The ruling in one sentence: matching a BORROW binds payload sub-place
// borrows; matching an owned place copies or moves, exactly as before.
// There is no new pattern grammar, and no per-binder mode — the
// scrutinee's flavor decides.
//
// The tests split three ways, and the split is the argument:
//
//   * what a binder is TYPED as, through each flavor and through nesting;
//   * what REGION obligations the projection incurs (the soundness half —
//     a payload borrow may never outlive its scrutinee's loan);
//   * that the OWNED path is untouched, diagnostics included.

const OPT: &str = "type Opt = enum::<T> { Some(T), None };\n";

// ---- `unsafe fn` — unsafety lives in the function type ------------------

/// A safe function, an `extern` item (`unsafe fn`), and a higher-order
/// function that takes either — the shapes every test below draws from.
const UNSAFE_FN_PRELUDE: &str = "\
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;
static safe_read = fn(buf: u8.&raw mut, len: usize) -> isize { 0 };
";

fn check_unsafe_fn(body: &str, expect: Expect) {
    check_diagnostics(&format!("{UNSAFE_FN_PRELUDE}{body}"), expect);
}

// ---- materialized temporaries (M12) --------------------------------------

/// Every materialized temporary in the fixture: the ROOT expression whose
/// value got storage, as `range 'snippet'`.
fn check_temps(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rows: Vec<(u32, String)> = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (body, source_map) = crate::body_with_source_map(&db, item);
        for (expr, binding) in body.temps.iter() {
            let Some(ptr) = source_map.node_for_expr(expr) else {
                continue;
            };
            let range = ptr.text_range();
            let data = &body.bindings[*binding];
            assert!(
                data.is_temporary() && data.mutable,
                "a temporary reports its kind and is trivially exclusive"
            );
            assert!(
                source_map.node_for_binding(*binding).is_none(),
                "no syntax node IS a temporary's binding"
            );
            rows.push((
                range.start().into(),
                format!("{range:?} '{}'\n", &text[range]),
            ));
        }
    }
    rows.sort();
    expect.assert_eq(&rows.into_iter().map(|(_, row)| row).collect::<String>());
}

/// Renders the diagnostics of every file of a library fixture (see
/// [`base_db::fixture_files`]), each prefixed with its file's path.
fn check_library_diagnostics(fixture: &str, expect: Expect) {
    let mut db = RootDatabase::default();
    let files = base_db::new_library(&mut db, "", base_db::fixture_files(fixture));
    let mut rendered = String::new();
    for file in files {
        for d in crate::file_diagnostics(&db, file) {
            let related = d
                .related
                .iter()
                .map(|r| format!(" ({} at {} {:?})", r.message, r.file.path(&db), r.range))
                .collect::<String>();
            rendered.push_str(&format!(
                "{} {:?}: {}{related}\n",
                file.path(&db),
                d.range,
                d.message
            ));
        }
    }
    expect.assert_eq(&rendered);
}
