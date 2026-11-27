//! Completions after a dot: fields, members and bound requirements.

use super::*;

/// The item is a call the checker refuses: sorted in the refused tier (P16).
fn is_refused(item: &crate::CompletionItem) -> bool {
    item.sort_text
        .starts_with(&crate::completions::TYPE_TIER_REFUSED.to_string())
}

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
    expect.assert_eq(&format!(
        r#"{}
filter: {}
"#,
        applied_line(&analysis, file, item),
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

const ORD_FIXTURE: &str = r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
} with {
    impl usize {
        ord = fn::<@a>(other: Self.&::<@a>, self: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
};
"#;

/// Applies the completion labeled `label`, with `argument` at its tab
/// stop, and asserts the written code checks without errors.
fn check_applied_checks_clean(text: &str, label: &str, argument: &str) {
    let (analysis, file, pos) = fixture(text);
    let items = analysis.completions(pos);
    let item = items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"));
    let insert = match &item.text_edit.insert {
        crate::InsertText::Plain(text) => text.clone(),
        crate::InsertText::Snippet { snippet, .. } => snippet.replace("$1", argument),
    };
    let mut written = analysis.file_text(file).to_string();
    let range = item.text_edit.range;
    written.replace_range(
        usize::from(range.start())..usize::from(range.end()),
        &insert,
    );
    let mut host = AnalysisHost::new();
    let written_file = host.create_file("test.must".to_owned(), written.clone());
    let errors: Vec<_> = host
        .snapshot()
        .diagnostics(written_file)
        .into_iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .map(|d| d.message)
        .collect();
    assert!(
        errors.is_empty(),
        r#"{label:?} in
{written}
errors: {errors:?}"#
    );
}

/// The analysis of a library of the fixture's files, and the position of
/// `$0` in its first file.
fn library_fixture(fixture_text: &str) -> (crate::Analysis, Vec<SourceFile>, FilePosition) {
    let mut db = base_db::RootDatabase::default();
    let files = base_db::fixture_files(fixture_text);
    let cursor = files[0].1.find("$0").unwrap_or(0);
    let files: Vec<_> = files
        .into_iter()
        .map(|(path, text)| (path, text.replace("$0", "")))
        .collect();
    let sources = base_db::new_library(&mut db, "", files);
    let pos = FilePosition {
        file: sources[0],
        offset: TextSize::new(cursor as u32),
    };
    (crate::Analysis::new(db), sources, pos)
}

/// Completion labels at `$0` in the fixture's first file, in a library of
/// the fixture's files.
fn library_completion_labels(fixture_text: &str) -> Vec<String> {
    let (analysis, _, pos) = library_fixture(fixture_text);
    analysis
        .completions(pos)
        .into_iter()
        .map(|c| c.label)
        .collect()
}

/// The errors in the fixture's first file once the completion labeled
/// `label` is applied there, with `argument` at its tab stop.
fn library_applied_errors(fixture_text: &str, label: &str, argument: &str) -> Vec<String> {
    let (analysis, sources, pos) = library_fixture(fixture_text);
    let item = analysis
        .completions(pos)
        .into_iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}"));
    let insert = match &item.text_edit.insert {
        crate::InsertText::Plain(text) => text.clone(),
        crate::InsertText::Snippet { snippet, .. } => snippet.replace("$1", argument),
    };
    let mut written = analysis.file_text(sources[0]).to_string();
    let range = item.text_edit.range;
    written.replace_range(
        usize::from(range.start())..usize::from(range.end()),
        &insert,
    );
    let mut fixture = String::new();
    for (k, (path, text)) in base_db::fixture_files(fixture_text).into_iter().enumerate() {
        let text = if k == 0 { written.clone() } else { text };
        fixture.push_str(&format!(
            r#"//- {path}
{text}
"#
        ));
    }
    let (analysis, sources, _) = library_fixture(&fixture);
    analysis
        .diagnostics(sources[0])
        .into_iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .map(|d| d.message)
        .collect()
}

#[test]
fn dot_completions_offer_members_next_to_fields() {
    check_has_completion(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).$0 }};"
        ),
        "bump",
    );
    check_has_completion(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).$0 }};"
        ),
        "n",
    );
}

#[test]
fn dot_completions_on_a_borrow_receiver_follow_the_receiver_shape() {
    // Through a `&mut` receiver the dot offers both borrow members and not
    // the value one; through a shared receiver the exclusive one is offered
    // refused (P16). Fields come through a written `.*`, never bare:
    // projecting through a borrow would be auto-deref.
    const BORROW_FIXTURE: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        read = fn::<@r>(c: Self.&::<@r>) -> usize { c.*.n };
        bump = fn::<@r>(c: Self.&mut::<@r>) -> () { c.*.n = 1; };
    }
};
"#;
    let exclusive =
        format!("{BORROW_FIXTURE}static main = fn::<@a>(c: Counter.&mut::<@a>) -> () {{ c.$0 }};");
    check_has_completion(&exclusive, "read");
    check_has_completion(&exclusive, "bump");
    check_no_completion(&exclusive, "get");
    check_no_completion(&exclusive, "n");
    check_has_completion(&exclusive, "*.n");
    let shared =
        format!("{BORROW_FIXTURE}static main = fn::<@a>(c: Counter.&::<@a>) -> () {{ c.$0 }};");
    check_has_completion(&shared, "read");
    let (analysis, _file, pos) = fixture(&shared);
    let items = analysis.completions(pos);
    let bump = items.iter().find(|c| c.label == "bump");
    assert!(bump.is_some_and(is_refused), "`bump` is offered refused");
}

#[test]
fn dot_completions_on_an_owned_receiver_write_the_borrow_a_member_takes() {
    check_completion_applied(
        &borrowed_member_main("let v = c.$0;"),
        "get",
        expect_test::expect![[r#"
            let v = c.&.get();
            filter: .get
        "#]],
    );
    check_completion_applied(
        &borrowed_member_main("c.$0;"),
        "add",
        expect_test::expect![[r#"
            c.&mut.add($1);
            filter: .add
        "#]],
    );
}

#[test]
fn dot_completions_written_borrow_replaces_a_typed_prefix() {
    check_completion_applied(
        &borrowed_member_main("let v = c.ge$0;"),
        "get",
        expect_test::expect![[r#"
            let v = c.&.get();
            filter: .get
        "#]],
    );
    // The scrutinee of a brace-less `match` ends in a space.
    assert_eq!(
        completion_insert(&borrowed_member_main("match c.$0"), "get"),
        crate::InsertText::Plain(".&.get() ".to_owned()),
    );
}

#[test]
fn dot_completions_reach_a_value_self_member_as_written() {
    check_completion_applied(
        &borrowed_member_main("let v = c.$0;"),
        "take",
        expect_test::expect![[r#"
            let v = c.take();
            filter: take
        "#]],
    );
}

#[test]
fn dot_completions_rank_a_refused_mut_borrow_last() {
    const MAIN: &str = r#"
static k: Counter = Counter(struct { n = 1 });
static main = fn::<@a>(
    s: Counter.&::<@a>,
    m: Counter.&mut::<@a>,
    x: Counter.&raw,
) -> () {
    let c: Counter = Counter(struct { n = 1 });
    let mut d: Counter = Counter(struct { n = 1 });
    let p: struct { c: Counter } = struct { c = Counter(struct { n = 1 }) };
    let mut q: struct { c: Counter } = struct { c = Counter(struct { n = 1 }) };
    let mut t: Counter.&::<@a> = s;
    RECEIVER.$0;
};
"#;
    // Whether `add` (a `Self.&mut` member) sorts after `get` and `take`.
    let add_last = |receiver: &str| {
        let text = MAIN.replace("RECEIVER", receiver);
        let (analysis, _file, pos) = fixture(&format!("{BORROWED_MEMBER_FIXTURE}{text}"));
        let items = analysis.completions(pos);
        let sort_text = |label: &str| {
            items
                .iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("{label} is offered on {receiver}"))
                .sort_text
                .clone()
        };
        let add = sort_text("add");
        add > sort_text("get") && add > sort_text("take")
    };
    assert!(add_last("c"), "an immutable local");
    assert!(!add_last("d"), "a `mut` local");
    assert!(add_last("p.c"), "a field of an immutable local");
    assert!(!add_last("q.c"), "a field of a `mut` local");
    assert!(add_last("k"), "a `static`");
    assert!(add_last("s.*"), "through a shared borrow");
    assert!(
        add_last("t.*"),
        "through a shared borrow held by a `mut` local"
    );
    assert!(
        !add_last("m.*"),
        "through an exclusive borrow held by an immutable local"
    );
    assert!(add_last("x.*"), "through a raw pointer");
}

#[test]
fn dot_completions_on_a_borrow_receiver_offer_the_referents_fields_through_a_deref() {
    check_completions(
        r#"
static main = fn {
    let a = struct {a = 8, b = "11"}.&;
    a.$0
}
"#,
        expect_test::expect![[r#"
            *.a Field ({number})
            *.b Field (str)
        "#]],
    );
}

#[test]
fn dot_completions_deref_field_replaces_a_typed_prefix() {
    check_completion_applied(
        r#"
static main = fn::<@a>(p: struct { x: usize, y: usize }.&mut::<@a>) -> usize {
    p.y$0
};
"#,
        "*.y",
        expect_test::expect![[r#"
            p.*.y
            filter: y
        "#]],
    );
}

#[test]
fn dot_completions_deref_once_per_borrow_layer() {
    check_completion_applied(
        r#"
type Point = struct { x: usize };
static f = fn::<@a, @b>(p: Point.&::<@a>.&::<@b>) -> usize {
    p.$0
};
"#,
        "*.*.x",
        expect_test::expect![[r#"
            p.*.*.x
            filter: x
        "#]],
    );
}

#[test]
fn dot_completions_reach_members_behind_the_innermost_borrow() {
    const F: &str = r#"
static f = fn::<@a, @b>(c: Counter.&::<@a>.&::<@b>) -> usize {
    c.$0
};
"#;
    check_completion_applied(
        &format!("{BORROWED_MEMBER_FIXTURE}{F}"),
        "*.get",
        expect_test::expect![[r#"
            c.*.get()
            filter: get
        "#]],
    );
    check_has_completion(&format!("{BORROWED_MEMBER_FIXTURE}{F}"), "*.*.n");
    check_no_completion(&format!("{BORROWED_MEMBER_FIXTURE}{F}"), "*.take");
}

#[test]
fn dot_completions_refuse_a_mut_member_behind_a_shared_layer() {
    // `c.*` on a `Counter.&mut.&` reborrows through the shared layer, so
    // `*.add()` is offered, sorted last and marked refused.
    let text = format!(
        "{BORROWED_MEMBER_FIXTURE}{}",
        r#"
static f = fn::<@a, @b>(c: Counter.&mut::<@a>.&::<@b>) -> () { c.$0 };
"#
    );
    let (analysis, _file, pos) = fixture(&text);
    let items = analysis.completions(pos);
    let item = |label: &str| {
        items
            .iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label} is offered"))
    };
    assert!(!is_refused(item("*.get")));
    assert!(is_refused(item("*.add")));
    assert!(item("*.add").sort_text > item("*.get").sort_text);
}

#[test]
fn dot_completions_refuse_a_mut_member_on_a_place_read_through_a_shared_borrow() {
    // `s.*.c.*` reborrows through `s`'s shared layer, so `*.add()` is
    // refused although `c` is a `Counter.&mut`.
    let text = format!(
        "{BORROWED_MEMBER_FIXTURE}{}",
        r#"
static f = fn::<@a, @b>(s: struct { c: Counter.&mut::<@a> }.&::<@b>) -> () { s.*.c.$0 };
"#
    );
    let (analysis, _file, pos) = fixture(&text);
    let items = analysis.completions(pos);
    let item = |label: &str| {
        items
            .iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label} is offered"))
    };
    assert!(!is_refused(item("get")));
    assert!(is_refused(item("add")));
    assert!(item("add").sort_text > item("get").sort_text);
}

#[test]
fn dot_completions_offer_a_mut_member_on_a_place_read_through_exclusive_borrows() {
    let text = format!(
        "{BORROWED_MEMBER_FIXTURE}{}",
        r#"
static f = fn::<@a, @b>(s: struct { c: Counter.&mut::<@a> }.&mut::<@b>) -> () { s.*.c.$0 };
"#
    );
    let (analysis, _file, pos) = fixture(&text);
    let items = analysis.completions(pos);
    let add = items
        .iter()
        .find(|c| c.label == "add")
        .expect("add is offered");
    assert!(!is_refused(add));
}

#[test]
fn dot_completions_reach_bound_requirements_behind_the_innermost_borrow() {
    check_completions(
        &format!(
            "{BOUND_FIXTURE_HEAD}{}",
            r#"
static f = fn::<@a, @b, W: Write>(w: W.&mut::<@a>.&mut::<@b>) -> () { w.$0 };
"#
        ),
        expect_test::expect![[r#"
            *.push Function (fn(char, W.&mut::<@r>))
        "#]],
    );
}

#[test]
fn dot_completions_refuse_a_mut_bound_requirement_behind_a_shared_layer() {
    let (analysis, _file, pos) = fixture(&format!(
        "{BOUND_FIXTURE_HEAD}{}",
        r#"
static f = fn::<@a, @b, W: Write>(w: W.&mut::<@a>.&::<@b>) -> () { w.$0 };
"#
    ));
    let items = analysis.completions(pos);
    let push = items.iter().find(|c| c.label == "*.push");
    assert!(push.is_some_and(is_refused));
}

#[test]
fn dot_completions_offer_no_deref_fields_through_a_raw_pointer() {
    // TODO: halvko/must#181: a raw-pointer receiver offers no referent fields.
    check_no_completion(
        "static f = fn(p: struct { x: usize }.&raw) -> () { p.$0 };",
        "*.x",
    );
}

#[test]
fn dot_completions_on_a_borrow_receiver_write_no_borrow() {
    const F: &str = r#"
static f = fn::<@a>(c: Counter.&mut::<@a>) -> usize {
    c.$0
};
"#;
    check_completion_applied(
        &format!("{BORROWED_MEMBER_FIXTURE}{F}"),
        "get",
        expect_test::expect![[r#"
            c.get()
            filter: get
        "#]],
    );
}

#[test]
fn dot_completions_on_a_rigid_receiver_offer_its_bounds_requirements() {
    // `w.push(x)` has compiled since bound-directed resolution landed, and
    // `w.` offered nothing: the receiver is rigid, so it has no fields and
    // no impl to look in — its BOUNDS are the only thing that re-opens the
    // dot, and now they are read from the same enumeration resolution
    // picks from. Detail renders the requirement at the receiver's own
    // param (`Self` → `W`), its own binder as written — the shape the
    // trait's hover shows.
    check_completions(
        &format!(
            "{BOUND_FIXTURE_HEAD}\
             static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
        ),
        expect_test::expect![[r#"
            push Function (fn(char, W.&mut::<@r>))
        "#]],
    );
}

#[test]
fn dot_completions_snippet_bound_requirement_call_with_params() {
    // A requirement is fn-shaped like a member, so accepting it inserts the
    // call, not the bare name — the same idiom `field_items` uses for an
    // inherent member. The receiver (`Self`) is not a written argument, so
    // it does not count toward the tab stops.
    assert_eq!(
        completion_insert(
            &format!(
                "{BOUND_FIXTURE_HEAD}\
                 static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
            ),
            "push",
        ),
        crate::InsertText::Snippet {
            snippet: "push($1)".to_owned(),
            plain: "push()".to_owned(),
        }
    );
}

#[test]
fn dot_completions_snippet_bound_requirement_call_without_params() {
    // A NULLARY requirement (past its receiver) inserts a plain call, no
    // tab stop — pinned separately because the receiver-only arity is the
    // one a written-arity miscount would get wrong silently (a snippet
    // with an empty placeholder reads the same as this until a tab stop
    // is expected).
    assert_eq!(
        completion_insert(
            r#"
trait Show = requires { show: fn(Self) -> str; };
static f = fn::<T: Show>(t: T) -> str { t.$0 };
"#,
            "show",
        ),
        crate::InsertText::Plain("show()".to_owned())
    );
}

#[test]
fn a_bound_requirement_sorts_like_an_inherent_member() {
    // A requirement is rendered through the same [`Provenance::Item`] band
    // as an inherent member — not a band of its own — so a bound-directed
    // offer and a field or member candidate interleave by type match, not
    // by which mechanism produced them.
    assert_eq!(
        completion_sort_text(
            &format!(
                "{BOUND_FIXTURE_HEAD}\
                 static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
            ),
            "push",
        ),
        "2_20_push",
    );
}

#[test]
fn a_bound_receivers_offer_is_exactly_what_resolves() {
    // The pin on the one-home claim: the offered name is the name that
    // compiles, on the very same receiver. If the enumeration and the
    // resolver ever part company, one of these two halves fails.
    let source = format!(
        "{BOUND_FIXTURE_HEAD}\
         static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, _n: usize) -> () {{ w.push('x') }};"
    );
    let (analysis, file, _) = fixture(&format!("{source}$0"));
    assert_eq!(analysis.diagnostics(file), Vec::new());
    check_has_completion(
        &format!(
            "{BOUND_FIXTURE_HEAD}\
             static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
        ),
        "push",
    );
}

#[test]
fn multiple_bounds_union_their_requirements_on_the_dot() {
    // `T: A + B` offers both traits' requirements — the union, not the
    // first bound that answers. (The rendered order is the list's own
    // ranking, which sorts equally-ranked candidates by name; the
    // enumeration itself walks bound order then declaration order, the
    // same order resolution walks when it looks for a name.)
    check_completions(
        r#"
trait Show = requires { show: fn(Self) -> str; };
trait Size = requires { size: fn(Self) -> usize; len: fn(Self) -> usize; };
static f = fn::<T: Show + Size>(t: T) -> str { t.$0 };
"#,
        expect_test::expect![[r#"
            len Function (fn(T) -> usize)
            show Function (fn(T) -> str)
            size Function (fn(T) -> usize)
        "#]],
    );
}

#[test]
fn a_requirement_without_the_dot_callable_shape_is_not_offered() {
    // G14 is structural: the receiver sits in the LAST parameter. A
    // requirement whose `Self` is anywhere else (or nowhere) is callable
    // only by its qualified spelling, so the dot does not offer it — the
    // same test `member_takes_receiver` admits calls by.
    check_completions(
        r#"
trait Mk = requires {
    make: fn(usize) -> Self;
    first: fn(Self, usize) -> usize;
    take: fn(usize, Self) -> usize;
};
static f = fn::<T: Mk>(t: T) -> usize { t.$0 };
"#,
        expect_test::expect![[r#"
            take Function (fn(usize, T) -> usize)
        "#]],
    );
}

#[test]
fn the_receiver_shape_decides_the_bounds_offer() {
    // One table for offers and calls alike. An OWNED receiver reaches a
    // `Self.&mut` requirement through a written `.&mut.` (the edit writes
    // it), a SHARED borrow is offered an exclusive one refused (P16), and
    // an exclusive borrow takes both — `dot_reach`, read through the
    // completion list.
    let owned = format!("{BOUND_FIXTURE_HEAD}static f = fn::<W: Write>(w: W) -> () {{ w.$0 }};");
    check_completion_applied(
        &owned,
        "push",
        expect_test::expect![[r#"
            static f = fn::<W: Write>(w: W) -> () { w.&mut.push($1) };
            filter: .push
        "#]],
    );
    let shared = format!(
        "{BOUND_FIXTURE_HEAD}\
         static f = fn::<@a, W: Write>(w: W.&::<@a>) -> () {{ w.$0 }};"
    );
    let (analysis, _file, pos) = fixture(&shared);
    let items = analysis.completions(pos);
    let push = items.iter().find(|c| c.label == "push");
    assert!(push.is_some_and(is_refused), "`push` is offered refused");
    // A value-`Self` requirement is the mirror: the owned receiver takes
    // it, and the borrowed one does not (that would be auto-deref).
    let by_value = r#"
trait Show = requires { show: fn(Self) -> str; };
static f = fn::<S: Show>(s: S) -> str { s.$0 };
"#;
    check_has_completion(by_value, "show");
    check_no_completion(
        r#"
trait Show = requires { show: fn(Self) -> str; };
static f = fn::<@a, S: Show>(s: S.&::<@a>) -> str { s.$0 };
"#,
        "show",
    );
}

#[test]
fn the_forget_capability_bound_contributes_nothing_to_the_dot() {
    // `forget` is a CAPABILITY, not a trait: it has no members, resolves
    // to no dictionary, and never enters the bound list at all. The dot of
    // a `T: forget` param is therefore empty — cleanly, with no candidate
    // and no complaint.
    check_completions(
        "static f = fn::<T: forget>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
    // And a capability written NEXT to a trait leaves the trait's
    // requirements exactly as they were.
    check_completions(
        r#"
trait Show = requires { show: fn(Self) -> str; };
static f = fn::<T: Show + forget>(t: T) -> str { t.$0 };
"#,
        expect_test::expect![[r#"
            show Function (fn(T) -> str)
        "#]],
    );
}

#[test]
fn an_unbounded_rigid_receiver_offers_nothing() {
    // Nothing is assumed of a parameter that writes no bound: it has no
    // fields, no impl and no requirements, and the dot says so by offering
    // nothing rather than guessing at some type's members.
    check_completions(
        "static f = fn::<T>(t: T) -> T { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn an_unresolvable_bound_offers_nothing() {
    // `Nope` names nothing: the bound's own declaration carries that
    // diagnostic, and the enumeration contributes no candidate for it —
    // no panic, no guess.
    check_completions(
        "static f = fn::<T: Nope>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn a_bound_naming_a_non_trait_offers_nothing() {
    // `usize` resolves to something, but not to a trait: `bound_trait`
    // rejects it the same way a call site's bound resolution does, so it
    // contributes no candidate either.
    check_completions(
        "static f = fn::<T: usize>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn a_requirement_without_a_full_signature_is_not_offered() {
    // `n`'s first parameter is a hole, so its signature isn't
    // fully written — the trait declaration carries that diagnostic, and
    // `lower_requirement_sig` returns `None` for it. `Self` is deliberately
    // placed in `n`'s SECOND parameter (not dropped) so this isolates the
    // `sig: None` path from `dot_callable`'s shape refusal: if `n` were
    // offered because it merely lacked a `Self` parameter, this fixture
    // would still catch it. The dot offers `m` (fully written) and skips
    // `n` rather than guessing at its shape.
    check_completions(
        r#"
trait D = requires {
    m: fn(Self) -> usize;
    n: fn(_, Self) -> usize;
};
static f = fn::<T: D>(t: T) -> () { t.$0 };
"#,
        expect_test::expect![[r#"
            m Function (fn(T) -> usize)
        "#]],
    );
}

#[test]
fn a_name_two_bounds_carry_writes_its_escape() {
    // A bound member call is decided by NAME before G14 is asked, so
    // `t.push(c)` is ambiguous even though only `A::push` fits the
    // receiver's shape. That row writes its G13 escape; `B::push` takes
    // `Self` by value, which `t` can't give. `grow`, which only `A`
    // declares, is offered as usual.
    let source = [
        AMBIGUOUS_SHAPES_HEAD,
        r#"
static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () { let y = t.$0; };
"#,
    ]
    .concat();
    check_no_completion(&source, "push");
    check_no_completion(&source, "B::push");
    check_has_completion(&source, "grow");
    check_completion_applied(
        &source,
        "A::push",
        expect_test::expect![[r#"
            static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () { let y = A::push($1, t); };
            filter: t.push
        "#]],
    );
    check_applied_checks_clean(&source, "A::push", "'c'");
}

#[test]
fn the_ambiguity_diagnostic_names_both_traits() {
    // P15: ambiguity is name-only, and the diagnostic names both traits
    // with the escapes completion writes for them.
    let (analysis, file, _) = fixture(
        &[
            AMBIGUOUS_SHAPES_HEAD,
            r#"
static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () { t.push('x') };$0
"#,
        ]
        .concat(),
    );
    let message = analysis
        .diagnostics(file)
        .into_iter()
        .map(|d| d.message)
        .find(|m| m.contains("is ambiguous"))
        .expect("the collision must be diagnosed");
    assert!(
        message.contains("`A`'s member (`A::push(value)`)")
            && message.contains("`B`'s member (`B::push(value)`)"),
        "the hint must name both traits and their spellings: {message}"
    );
}

#[test]
fn two_bounds_of_the_same_shape_offer_one_escape_each() {
    // With the shapes IDENTICAL the dot-call is just as ambiguous: each
    // trait gets one escape row, and no bare `push` row resolves to nothing.
    let (analysis, _file, pos) = fixture(
        r#"
trait A = requires { push: fn(char, Self) -> (); };
trait B = requires { push: fn(char, Self) -> (); };
static f = fn::<T: A + B>(t: T) -> () { t.$0 };
"#,
    );
    let mut labels: Vec<_> = analysis
        .completions(pos)
        .into_iter()
        .map(|item| item.label)
        .filter(|label| label.ends_with("push"))
        .collect();
    labels.sort();
    assert_eq!(labels, ["A::push", "B::push"]);
}

#[test]
fn a_nested_body_is_still_offered_its_enclosing_bounds() {
    // A bound-directed call here is refused — "cannot use the enclosing
    // bounds YET (it would have to capture the dictionary)" — but that
    // wall is a RESERVATION and a body-lowering fact (`in_nested_body`),
    // not a fact about what the bound declares, so the item-tree-driven
    // offers query does not re-derive it. The refusal explains itself at
    // the call, and the day the wall lifts these offers are already
    // correct.
    check_has_completion(
        r#"
trait Show = requires { show: fn(Self) -> str; };
static f = fn::<T: Show>(t: T) -> str {
    let g = fn(x: T) -> str { x.$0 };
    g(t)
};
"#,
        "show",
    );
}

#[test]
fn dot_completions_offer_trait_impl_members_inside_the_impl() {
    // The trait's own chain implements it for `usize`, so a `usize.&`
    // receiver in a sibling member's body reaches `ord` through that impl.
    check_completion_applied(
        r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
    min: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Self.&::<@a>;
} with {
    impl usize {
        ord = fn::<@a>(other: Self.&::<@a>, self: Self.&::<@a>) -> Cmp { Cmp::Eq };
        min = fn::<@a>(other: Self.&::<@a>, self: Self.&::<@a>) -> Self.&::<@a> {
            match self.$0
        };
    }
};
"#,
        "ord",
        expect_test::expect![[r#"
            match self.ord($1)
            filter: ord
        "#]],
    );
}

#[test]
fn dot_completions_offer_trait_impl_members_on_builtin_receivers() {
    check_completions(
        &format!(
            "{ORD_FIXTURE}{}",
            r#"
static g = fn::<@a>(x: usize.&::<@a>) -> () { let y = x.$0; };
"#
        ),
        expect_test::expect![[r#"
            ord Function (fn(usize.&::<@a>, usize.&::<@a>) -> Cmp)
        "#]],
    );
    // An owned receiver reaches the borrow-taking member through a written
    // borrow.
    check_completion_applied(
        &format!(
            "{ORD_FIXTURE}{}",
            r#"
static g = fn (x: usize) -> () { let y = x.$0; };
"#
        ),
        "ord",
        expect_test::expect![[r#"
            static g = fn (x: usize) -> () { let y = x.&.ord($1); };
            filter: .ord
        "#]],
    );
}

#[test]
fn dot_completions_name_a_type_side_trait_member_by_its_requirement() {
    // `impl Ord` in a type's chain names its members `Ord::ord`; the dot
    // reaches them as `ord`, through the trait, not as inherent members.
    check_completions(
        r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
};
type P = struct { n: usize } with {
    impl Ord {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
};
static g = fn::<@a>(x: P.&::<@a>) -> () { let y = x.$0; };
"#,
        expect_test::expect![[r#"
            *.n Field (usize)
            ord Function (fn(P.&::<@a>, P.&::<@a>) -> Cmp)
        "#]],
    );
}

#[test]
fn dot_completions_write_the_escape_for_a_name_two_impls_carry() {
    // `x.ord(y)` is ambiguous between `Ord` and `Ord2`, so each row writes
    // its escape.
    let text = format!(
        "{ORD_FIXTURE}{}",
        r#"
trait Ord2 = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
    other: fn::<@a>(Self.&::<@a>) -> ();
} with {
    impl usize {
        ord = fn::<@a>(a: Self.&::<@a>, b: Self.&::<@a>) -> Cmp { Cmp::Eq };
        other = fn::<@a>(a: Self.&::<@a>) -> () { () };
    }
};
static g = fn::<@a>(x: usize.&::<@a>) -> () { let y = x.$0; };
"#
    );
    check_no_completion(&text, "ord");
    check_has_completion(&text, "other");
    check_applied_checks_clean(&text, "Ord::ord", "x");
    check_applied_checks_clean(&text, "Ord2::ord", "x");
}

#[test]
fn dot_completions_judge_a_shared_name_per_spelling() {
    // On an owned `x`, `x.ord()` resolves to `Ord2` (value `Self`) and
    // `x.&.ord()` to `Ord` (borrowed `Self`): each spelling has one carrier,
    // so both are offered.
    let text = format!(
        "{ORD_FIXTURE}{}",
        r#"
trait Ord2 = requires {
    ord: fn(Self, Self) -> Cmp;
} with {
    impl usize {
        ord = fn(a: Self, b: Self) -> Cmp { Cmp::Eq };
    }
};
static g = fn (x: usize) -> () { let y = x.$0; };
"#
    );
    check_completions_applied(
        &text,
        expect_test::expect![[r#"
        fn(usize.&::<@a>, usize.&::<@a>) -> Cmp: static g = fn (x: usize) -> () { let y = x.&.ord($1); };
        fn(usize, usize) -> Cmp: static g = fn (x: usize) -> () { let y = x.ord($1); };
    "#]],
    );
}

#[test]
fn dot_completions_offer_a_name_only_one_member_takes_from_the_receiver() {
    // `Zero::make` takes no `Self`, so `x.make()` can only mean `Mk::make`.
    check_completions(
        r#"
trait Zero = requires {
    make: fn() -> Self;
} with {
    impl usize {
        make = fn() -> Self { 0 };
    }
};
trait Mk = requires {
    make: fn::<@a>(Self.&::<@a>) -> usize;
} with {
    impl usize {
        make = fn::<@a>(s: Self.&::<@a>) -> usize { 0 };
    }
};
static g = fn::<@a>(x: usize.&::<@a>) -> () { let y = x.$0; };
"#,
        expect_test::expect![[r#"
            make Function (fn(usize.&::<@a>) -> usize)
        "#]],
    );
}

#[test]
fn dot_completions_write_the_escape_for_a_name_an_inherent_and_a_trait_member_share() {
    // `x.ord(y)` is ambiguous between the inherent `ord` and `Ord`'s, so
    // each row writes the path-call that names its member (G13).
    const TEXT: &str = r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
};
type P = struct { n: usize } with {
    impl Self {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
    impl Ord {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
};
static g = fn::<@a>(x: P.&::<@a>) -> () { let y = x.$0; };
"#;
    check_completions(
        TEXT,
        expect_test::expect![[r#"
            *.n Field (usize)
            P::ord Function (fn(P.&::<@a>, P.&::<@a>) -> Cmp)
            Ord::ord Function (fn(P.&::<@a>, P.&::<@a>) -> Cmp)
        "#]],
    );
    check_completions_applied(
        TEXT,
        expect_test::expect![[r#"
            usize: static g = fn::<@a>(x: P.&::<@a>) -> () { let y = x.*.n; };
            fn(P.&::<@a>, P.&::<@a>) -> Cmp: static g = fn::<@a>(x: P.&::<@a>) -> () { let y = P::ord($1, x); };
            fn(P.&::<@a>, P.&::<@a>) -> Cmp: static g = fn::<@a>(x: P.&::<@a>) -> () { let y = Ord::ord($1, x); };
        "#]],
    );
    check_applied_checks_clean(TEXT, "P::ord", "x");
    check_applied_checks_clean(TEXT, "Ord::ord", "x");
}

#[test]
fn an_escape_row_ranks_by_what_its_call_returns() {
    // The escape is always a call, so it ranks as one: a match on the call's
    // `Cmp`, never the top band a position expecting the fn value would give
    // the member itself.
    const TEXT: &str = r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires { ord: fn(Self, Self) -> Cmp; };
type P = struct { n: usize } with {
    impl Self {
        ord = fn(o: Self, s: Self) -> Cmp { Cmp::Eq };
    }
    impl Ord {
        ord = fn(o: Self, s: Self) -> Cmp { Cmp::Eq };
    }
};
static g = fn(x: P) -> EXPECTED { x.o$0 };
"#;
    let sort_text =
        |expected: &str| completion_sort_text(&TEXT.replace("EXPECTED", expected), "Ord::ord");
    assert_eq!(sort_text("fn(P, P) -> Cmp"), "2_20_ord");
    assert_eq!(sort_text("Cmp"), "1_20_ord");
}

#[test]
fn dot_completions_write_the_escape_with_the_receiver_as_the_dot_reaches_it() {
    // The escape passes the receiver as the row would have reached it: an
    // owned one borrowed, one behind two borrows dereferenced once.
    const TEXT: &str = r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
};
type P = struct { n: usize } with {
    impl Self {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
    impl Ord {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Cmp { Cmp::Eq };
    }
};
static g = fn::<@a, @b>(RECEIVER) -> () { let y = x.$0; };
"#;
    let owned = TEXT.replace("RECEIVER", "x: P");
    check_completion_applied(
        &owned,
        "Ord::ord",
        expect_test::expect![[r#"
            static g = fn::<@a, @b>(x: P) -> () { let y = Ord::ord($1, x.&); };
            filter: x.ord
        "#]],
    );
    check_applied_checks_clean(&owned, "Ord::ord", "x.&");
    let layered = TEXT.replace("RECEIVER", "x: P.&::<@a>.&::<@b>");
    check_completion_applied(
        &layered,
        "P::ord",
        expect_test::expect![[r#"
            static g = fn::<@a, @b>(x: P.&::<@a>.&::<@b>) -> () { let y = P::ord($1, x.*); };
            filter: x.ord
        "#]],
    );
    check_applied_checks_clean(&layered, "P::ord", "x.*");
}

#[test]
fn dot_completions_write_the_escape_for_a_trait_member_a_fn_field_shares() {
    // `x.ord(y)` on an owned `P` is ambiguous between `Ord`'s member and the
    // fn-typed field: the field reads as written, the member writes its
    // escape.
    const TEXT: &str = r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn(Self, Self) -> Cmp;
};
type P = struct { ord: fn(usize) -> Cmp } with {
    impl Ord {
        ord = fn(o: Self, s: Self) -> Cmp { Cmp::Eq };
    }
};
static g = fn (x: P) -> () { let y = x.$0; };
"#;
    check_completions_applied(
        TEXT,
        expect_test::expect![[r#"
            fn(usize) -> Cmp: static g = fn (x: P) -> () { let y = x.ord; };
            fn(P, P) -> Cmp: static g = fn (x: P) -> () { let y = Ord::ord($1, x); };
        "#]],
    );
}

#[test]
fn dot_completions_skip_a_trait_member_with_an_unwritten_signature() {
    check_no_completion(
        r#"
type Cmp = enum { Less, Eq, Greater };
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> Cmp;
} with {
    impl usize {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> Nope { Cmp::Eq };
    }
};
static g = fn::<@a>(x: usize.&::<@a>) -> () { let y = x.$0; };
"#,
        "ord",
    );
}

#[test]
fn dot_completions_spell_the_escape_by_the_name_the_file_imports() {
    // `Ord` lives in `b.must`; `a.must` knows it only as `O`, so the escape
    // is `O::ord`. Without the import it can't be named, and the row is left
    // out rather than writing a path that doesn't resolve.
    const FIXTURE: &str = r#"
//- a.must
IMPORT
type Cmp = enum { Less, Eq, Greater };
type P = struct { n: usize } with {
    impl Self {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> usize { 0 };
    }
};
static g = fn::<@a>(x: P.&::<@a>) -> () { let y = x.$0; };
//- b.must
use a::P;
trait Ord = requires {
    ord: fn::<@a>(Self.&::<@a>, Self.&::<@a>) -> usize;
} with {
    impl P {
        ord = fn::<@a>(o: Self.&::<@a>, s: Self.&::<@a>) -> usize { 0 };
    }
};
"#;
    let imported = library_completion_labels(&FIXTURE.replace("IMPORT", "use b::{Ord as O};"));
    assert!(imported.contains(&"O::ord".to_owned()), "{imported:?}");
    assert!(imported.contains(&"P::ord".to_owned()), "{imported:?}");
    let imported = FIXTURE.replace("IMPORT", "use b::{Ord as O};");
    for label in ["O::ord", "P::ord"] {
        let errors = library_applied_errors(&imported, label, "x");
        assert!(errors.is_empty(), "{label}: {errors:?}");
    }
    // The ambiguity error spells the escape the same way.
    let call = imported.replace("let y = x.$0;", "let y = x.ord(x);$0");
    let (analysis, sources, _) = library_fixture(&call);
    let message = (analysis.diagnostics(sources[0]).into_iter())
        .map(|d| d.message)
        .find(|m| m.contains("is ambiguous"))
        .expect("the collision is diagnosed");
    assert!(message.contains("`O::ord(value)`"), "{message}");
    // The check sees the written code: a wrong argument is an error.
    assert!(!library_applied_errors(&imported, "O::ord", "nope").is_empty());
    let unimported = library_completion_labels(&FIXTURE.replace("IMPORT", ""));
    // Ambiguous without the import, so the inherent member writes its own
    // escape.
    assert!(unimported.contains(&"P::ord".to_owned()), "{unimported:?}");
    assert!(
        !unimported
            .iter()
            .any(|l| l.ends_with("::ord") && l != "P::ord"),
        "{unimported:?}"
    );
}
