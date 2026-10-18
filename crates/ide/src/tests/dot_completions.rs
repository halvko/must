//! Completions after a dot: fields, members and bound requirements.

use super::*;

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
    x: Counter.&raw mut,
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
        "static f = fn(p: struct { x: usize }.&raw mut) -> () { p.$0 };",
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
fn a_name_two_bounds_carry_is_offered_by_neither() {
    // A bound member call is decided by NAME before G14 is asked, so this
    // `push` is refused permanently even though exactly one candidate fits
    // the receiver's shape. Offering the shape-viable one would put a row
    // in the list that cannot be accepted — so completion suppresses the
    // name, which is resolution's own verdict (`narrow_by_name` returning
    // two) read straight through.
    //
    // The suppression is per NAME, not per bound set: `grow`, which only
    // `A` declares, is offered as usual.
    let source = format!(
        "{AMBIGUOUS_SHAPES_HEAD}\
         static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () {{ t.$0 }};"
    );
    check_no_completion(&source, "push");
    check_has_completion(&source, "grow");
}

#[test]
fn the_ambiguity_diagnostic_still_names_both_traits() {
    // P15: ambiguity is name-only, so the suppressed completion above is
    // only defensible while the diagnostic itself says which two traits
    // collided and how to spell each — the message, not the list, is what
    // tells the user what to write.
    let (analysis, file, _) = fixture(&format!(
        "{AMBIGUOUS_SHAPES_HEAD}\
         static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () {{ t.push('x') }};$0"
    ));
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
fn two_bounds_of_the_same_shape_do_not_double_a_row() {
    // The same ruling, with the shapes IDENTICAL: resolution refuses just
    // as permanently, and the list must not show two indistinguishable
    // rows for a name that resolves to nothing. Counted, not just
    // `check_no_completion` — one row and two rows are different bugs.
    let (analysis, _file, pos) = fixture(
        r#"
trait A = requires { push: fn(char, Self) -> (); };
trait B = requires { push: fn(char, Self) -> (); };
static f = fn::<T: A + B>(t: T) -> () { t.$0 };
"#,
    );
    assert_eq!(
        analysis
            .completions(pos)
            .iter()
            .filter(|item| item.label == "push")
            .count(),
        0,
    );
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
