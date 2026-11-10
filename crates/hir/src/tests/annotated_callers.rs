//! Fully typed callers filling their callees' holes, and big-group warnings.

use super::*;

/// A fully typed caller's call fills a hole in its callee's signature; the
/// caller's own signature stays as written.
#[test]
fn fully_typed_caller_fills_its_callees_holes() {
    check_signatures(
        r#"
type Option = enum::<T> { Some(T), None };
static g = fn (x) -> Option::<_> { ::Some(x) };
static main = fn () -> () { let o: Option::<usize> = g(1); };
"#,
        expect![[r#"
            g: fn(usize) -> Option::<usize>
            main: fn()
        "#]],
    );
}

/// A caller that ties two callees together (`g(f(1))`) puts them in one
/// group, so the result doesn't depend on declaration order.
#[test]
fn fully_typed_caller_merges_the_callees_its_body_ties_together() {
    let text = r#"
static f = fn (x) { x };
static g = fn (y) { y };
static main = fn () -> usize { g(f(1)) };
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let library = crate::library::library_of(&db, file);
    let groups = crate::groups::inference_groups(&db, library);
    let items = crate::scopes::library_items(&db, library);
    let group_of = |index: usize| groups.slot.get(&items[index].0).map(|&(group, _)| group);
    assert_eq!(group_of(2), None, "the fully typed caller is no member");
    assert_eq!(
        group_of(0),
        group_of(1),
        "one caller ties both callees into one group"
    );
    check_signatures(
        text,
        expect![[r#"
            f: fn(usize) -> usize
            g: fn(usize) -> usize
            main: fn() -> usize
        "#]],
    );
    check_signatures(
        r#"
static main = fn () -> usize { g(f(1)) };
static g = fn (y) { y };
static f = fn (x) { x };
"#,
        expect![[r#"
            main: fn() -> usize
            g: fn(usize) -> usize
            f: fn(usize) -> usize
        "#]],
    );
}

/// Editing a fully typed caller re-runs the group it feeds, which checks
/// every body in it once; when the other bodies' results come out the
/// same, nothing downstream of them re-runs.
#[test]
fn fully_typed_caller_edit_backdates_behind_its_group() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = r#"
static id = fn (x) { x };
static main = fn () -> usize { id(1) };
static other = fn () -> usize { id(2) + 1 };
"#;
    let text_v2 = r#"
static id = fn (x) { x };
static main = fn () -> usize { id(1) + 0 };
static other = fn () -> usize { id(2) + 1 };
"#;

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::linear_check::linear_check(&db, item);
    }
    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::linear_check::linear_check(&db, item);
    }
    let log = log.lock().unwrap();
    // Each member's `infer` re-runs to read the group's new result; the
    // unchanged ones come out equal and backdate.
    assert_eq!(
        log.iter()
            .filter(|entry| entry.starts_with("infer_group("))
            .count(),
        1,
        "the edited caller feeds `id`'s group, which runs once: {log:#?}"
    );
    assert_eq!(
        log.iter()
            .filter(|entry| entry.starts_with("linear_check("))
            .count(),
        1,
        "only the edited item's checks may re-run; executed: {log:#?}"
    );
}

/// A group past the size setting warns once per body, at its first use of
/// an unannotated member, pointing at every member it uses. Recursion
/// doesn't count: it is the item's own business. A group exactly at the
/// setting is fine, and `None` turns the warning off.
#[test]
fn big_inference_group_warns_at_uses_of_its_unannotated_items() {
    let text = r#"
static f = fn (x) { if x > 10 { x } else { f(x + 1) } };
static g = fn (y) { y + f(y) };
static main = fn () -> usize { g(1) + g(2) + f(3) };
"#;
    expect![[r#"
        Warning 82..83: `f` is missing a complete type signature, and it is inferred in a group of 3 items (with 34 expressions); inference this large may slow down your editor (`f` is defined here at 8..9)
        Warning 121..122: `g` and 1 other item used here are missing complete type signatures, and they are inferred in a group of 3 items (with 34 expressions); inference this large may slow down your editor (`g` is defined here at 65..66) (`f` is defined here at 8..9)
    "#]]
    .assert_eq(&render_diagnostics_with_big_group(text, Some(33)));
    expect![[r#""#]].assert_eq(&render_diagnostics_with_big_group(text, Some(34)));
    expect![[r#""#]].assert_eq(&render_diagnostics_with_big_group(text, None));
}

/// Mutually recursive unannotated items are one group, each body inferred
/// once against the shared signatures, so each warns at its use of the
/// other: annotating either one breaks the cycle.
#[test]
fn big_inference_group_warns_across_mutual_recursion() {
    let text = r#"
static even = fn (n: usize) { if n == 0 { true } else { odd(n - 1) } };
static odd = fn (n: usize) { if n == 0 { false } else { even(n - 1) } };
"#;
    expect![[r#"
        Warning 57..60: `odd` is missing a complete type signature, and it is inferred in a group of 2 items (with 28 expressions); inference this large may slow down your editor (`odd` is defined here at 80..83)
        Warning 129..133: `even` is missing a complete type signature, and it is inferred in a group of 2 items (with 28 expressions); inference this large may slow down your editor (`even` is defined here at 8..12)
    "#]].assert_eq(&render_diagnostics_with_big_group(text, Some(0)));
}

/// A callee's own body decides its signature; a fully typed caller only
/// fills what the body left free. A caller that disagrees is reported once,
/// at the caller, against the callee's published signature.
#[test]
fn fully_typed_caller_cannot_override_its_callees_own_body() {
    check_diagnostics(
        r#"
static g = fn () { true };
static main = fn () -> usize { g() };
"#,
        expect![[r#"
            59..62: type mismatch: expected `usize`, found `bool` (expected `usize` because of this return type at 48..56)
        "#]],
    );
}

/// Two fully typed callers that disagree on a hole are blamed equally,
/// whatever their order: the callee is poisoned and each use names both.
#[test]
fn fully_typed_callers_that_disagree_are_blamed_equally() {
    let a_first = r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { id(true) };
"#;
    check_signatures(
        a_first,
        expect![[r#"
            id: {error}
            a: fn() -> usize
            b: fn() -> bool
        "#]],
    );
    check_diagnostics(
        a_first,
        expect![[r#"
        55..57: callers disagree on the type of `id`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 71..72)
        91..93: callers disagree on the type of `id`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 71..72)
    "#]],
    );
    let b_first = r#"
static id = fn (x) { x };
static b = fn () -> bool { id(true) };
static a = fn () -> usize { id(1) };
"#;
    check_signatures(
        b_first,
        expect![[r#"
            id: {error}
            b: fn() -> bool
            a: fn() -> usize
        "#]],
    );
    check_diagnostics(
        b_first,
        expect![[r#"
        54..56: callers disagree on the type of `id`: `b` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 34..35) (`a` is defined here at 73..74)
        94..96: callers disagree on the type of `id`: `b` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 34..35) (`a` is defined here at 73..74)
    "#]],
    );
}

/// Callers vote on a hole once per use: a body's three uses outvote two
/// callers with one each, and the losing uses are outvoted at each use,
/// naming the body that decided.
#[test]
fn three_uses_in_one_body_outvote_two_callers() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { if id(true) { id(false) } else { id(true) } };
static c = fn () -> usize { id(2) };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn(bool) -> bool
        a: fn() -> usize
        b: fn() -> bool
        c: fn() -> usize
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        55..57: `id` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 71..72)
        166..168: `id` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 71..72)
    "#]],
    );
}

/// Three callers with one use each outvote a body with two uses.
#[test]
fn three_single_use_callers_outvote_a_body_with_two_uses() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { if id(true) { id(false) } else { true } };
static c = fn () -> usize { id(2) };
static d = fn () -> usize { id(3) };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn(usize) -> usize
        a: fn() -> usize
        b: fn() -> bool
        c: fn() -> usize
        d: fn() -> usize
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        94..96: `id` is `fn(usize) -> usize` here, as `a`, `c` and `d` make it, but this use makes it `fn(bool) -> bool` (`a` is defined here at 34..35) (`c` is defined here at 141..142) (`d` is defined here at 178..179)
        105..107: `id` is `fn(usize) -> usize` here, as `a`, `c` and `d` make it, but this use makes it `fn(bool) -> bool` (`a` is defined here at 34..35) (`c` is defined here at 141..142) (`d` is defined here at 178..179)
    "#]],
    );
}

/// Two bodies, one of them using the member twice: the uses decide, not
/// the bodies, so there is no tie.
#[test]
fn use_count_decides_between_two_bodies() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { if id(true) { id(false) } else { true } };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn(bool) -> bool
        a: fn() -> usize
        b: fn() -> bool
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        55..57: `id` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 71..72)
    "#]],
    );
}

/// Uses count per member, not across a variable two members share: `b`'s
/// two uses of `h` outvote `c`'s one, while its one use of `g` ties `a`'s.
#[test]
fn uses_count_per_member_not_across_a_link() {
    let src = r#"
static g = fn (x) { h(x) };
static h = fn (y) { y };
static a = fn () -> usize { g(1) };
static b = fn () -> bool { if g(true) { h(false) } else { h(true) } };
static c = fn () -> usize { h(2) };
"#;
    check_signatures(
        src,
        expect![[r#"
            g: {error}
            h: fn(bool) -> bool
            a: fn() -> usize
            b: fn() -> bool
            c: fn() -> usize
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            82..83: callers disagree on the type of `g`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 61..62) (`b` is defined here at 97..98)
            120..121: callers disagree on the type of `g`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 61..62) (`b` is defined here at 97..98)
            189..190: `h` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 97..98)
        "#]],
    );
}

/// A member body and a caller that use `g` once each tie.
#[test]
fn member_body_and_a_caller_with_one_use_each_tie() {
    let src = r#"
static k = fn () { k() };
static g = fn (y) { y };
static f = fn (x) { let _: bool = g(true); };
static c = fn () { f(g(k())) };
static a = fn () { let _: usize = g(1); };
"#;
    check_signatures(
        src,
        expect![[r#"
            k: fn() -> {error}
            g: {error}
            f: fn({error})
            c: fn()
            a: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            20..21: not enough information to infer the type of `k`; add a type annotation to its definition (defined here at 8..9)
            86..87: callers disagree on the type of `g`: `f` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`f` is defined here at 59..60) (`a` is defined here at 137..138)
            117..118: not enough information to infer the type of `f`; add a type annotation to its definition (defined here at 59..60)
            119..120: callers disagree on the type of `g`: `f` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`f` is defined here at 59..60) (`a` is defined here at 137..138)
            121..122: not enough information to infer the type of `k`; add a type annotation to its definition (defined here at 8..9)
            164..165: callers disagree on the type of `g`: `f` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`f` is defined here at 59..60) (`a` is defined here at 137..138)
        "#]],
    );
}

/// A body's mentions all vote with its one copy: `b`'s `id(3)` errors in
/// `b`, yet counts for the `bool` `b` gives `id`, so `b` wins 3 to 2.
#[test]
fn mention_that_errors_in_its_body_still_votes_with_it() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static c = fn () -> usize { id(2) };
static b = fn () -> () { let _ = id(true); let _ = id(false); let _ = id(3); };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn(bool) -> bool
        a: fn() -> usize
        c: fn() -> usize
        b: fn()
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        55..57: `id` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 108..109)
        92..94: `id` is `fn(bool) -> bool` here, as `b` makes it, but this use makes it `fn(usize) -> usize` (`b` is defined here at 108..109)
        174..175: type mismatch: expected `bool`, found `{number}`
    "#]],
    );
}

/// A caller that leaves a literal unpinned still votes, its number matching
/// any number type: against a `bool` it ties.
#[test]
fn unpinned_number_ties_with_a_bool() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> () { id(1); };
static b = fn () -> bool { id(true) };
"#;
    check_diagnostics(
        src,
        expect![[r#"
            52..54: callers disagree on the type of `id`: `a` makes it `fn({number}) -> {number}`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 69..70)
            89..91: callers disagree on the type of `id`: `a` makes it `fn({number}) -> {number}`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 69..70)
        "#]],
    );
}

/// Whether a literal votes doesn't depend on which uses were unified before
/// it.
#[test]
fn literal_votes_whatever_was_unified_before() {
    let src = r#"
static id = fn (x) { x };
static wrap = fn (y) { id(y) };
static a = fn () -> bool { id(true) };
static b = fn () -> () { let _ = id(1); };
static c = fn () -> () { let _ = id(2); };
"#;
    // Nothing settles `b`'s and `c`'s number, so `id` stays `{error}`, and
    // `a`'s use lost to nothing: every use names the three bodies.
    check_signatures(
        src,
        expect![[r#"
            id: fn({error}) -> {error}
            wrap: fn({error}) -> {error}
            a: fn() -> bool
            b: fn()
            c: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            50..52: `a`, `b` and `c` disagree on the type of `id`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 66..67) (`b` is defined here at 105..106) (`c` is defined here at 148..149)
            86..88: `a`, `b` and `c` disagree on the type of `id`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 66..67) (`b` is defined here at 105..106) (`c` is defined here at 148..149)
            131..133: `a`, `b` and `c` disagree on the type of `id`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 66..67) (`b` is defined here at 105..106) (`c` is defined here at 148..149)
            174..176: `a`, `b` and `c` disagree on the type of `id`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 66..67) (`b` is defined here at 105..106) (`c` is defined here at 148..149)
        "#]],
    );
}

/// Callers whose literals leave a number open tie with a fully typed
/// caller.
#[test]
fn callers_leaving_numbers_open_tie_with_a_typed_caller() {
    let src = r#"
static pair = fn (x, y) { x };
static a = fn () -> bool { pair(true, 1) };
static b = fn () -> bool { pair(false, 2) };
static c = fn () -> usize { pair(3, true) };
"#;
    // Nothing settles `y`'s number, so `pair`'s second parameter stays
    // `{error}`, and `c`'s use lost to nothing: every use names the three
    // bodies.
    check_signatures(
        src,
        expect![[r#"
        pair: fn(bool, {error}) -> bool
        a: fn() -> bool
        b: fn() -> bool
        c: fn() -> usize
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            59..63: `a`, `b` and `c` disagree on the type of `pair`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 83..84) (`c` is defined here at 128..129)
            103..107: `a`, `b` and `c` disagree on the type of `pair`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 83..84) (`c` is defined here at 128..129)
            149..153: `a`, `b` and `c` disagree on the type of `pair`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 83..84) (`c` is defined here at 128..129)
        "#]],
    );
}

/// Two callers that make one signature outvote a third that contradicts
/// it on one parameter, and the callee stays decided.
#[test]
fn caller_contradicting_the_others_on_one_parameter_is_outvoted() {
    let src = r#"
static pair = fn (x, y) { x };
static a = fn () -> usize { pair(1, true) };
static b = fn () -> usize { pair(2, 3) };
static c = fn () -> usize { pair(4, false) };
"#;
    check_signatures(
        src,
        expect![[r#"
            pair: fn(usize, bool) -> usize
            a: fn() -> usize
            b: fn() -> usize
            c: fn() -> usize
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            105..109: `pair` is `fn(usize, bool) -> usize` here, as `a` and `c` make it, but this use makes it `fn(usize, {number}) -> usize` (`a` is defined here at 39..40) (`c` is defined here at 126..127)
        "#]],
    );
}

/// A use that contradicts what a member's own body decided loses to it,
/// but when the member's callers then tie, nothing decided it, and the
/// use is one of the disagreeing ones.
#[test]
fn use_outvoted_on_a_member_a_tie_leaves_undecided_joins_the_tie() {
    let src = r#"
static f = fn (x, y) { let n: usize = x; if true { y } else { g() } };
static g = fn () { f(true, 1) };
static b = fn () -> bool { f(1, true) };
static c = fn (n: usize) -> usize { f(2, n) };
"#;
    check_signatures(
        src,
        expect![[r#"
        f: {error}
        g: fn() -> {error}
        b: fn() -> bool
        c: fn(usize) -> usize
    "#]],
    );
    // A `_` in a message is a part of the member that nothing determines.
    check_diagnostics(
        src,
        expect![[r#"
        63..64: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 79..80)
        91..92: callers disagree on the type of `f`: `g` makes it `fn(bool, {number}) -> _`, `b` makes it `fn(usize, bool) -> bool`, `c` makes it `fn(usize, usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`g` is defined here at 79..80) (`b` is defined here at 112..113) (`c` is defined here at 153..154)
        132..133: callers disagree on the type of `f`: `g` makes it `fn(bool, {number}) -> _`, `b` makes it `fn(usize, bool) -> bool`, `c` makes it `fn(usize, usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`g` is defined here at 79..80) (`b` is defined here at 112..113) (`c` is defined here at 153..154)
        182..183: callers disagree on the type of `f`: `g` makes it `fn(bool, {number}) -> _`, `b` makes it `fn(usize, bool) -> bool`, `c` makes it `fn(usize, usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`g` is defined here at 79..80) (`b` is defined here at 112..113) (`c` is defined here at 153..154)
    "#]],
    );
}

/// Uses that each leave the member open cast no vote; when they
/// contradict each other, they tie.
#[test]
fn open_uses_that_contradict_each_other_tie() {
    let src = r#"
static pair = fn (x, y) { x };
static a = fn (t) { pair(1, t) };
static b = fn (s) { pair(true, s) };
"#;
    check_signatures(
        src,
        expect![[r#"
        pair: {error}
        a: fn({error}) -> {error}
        b: fn({error}) -> bool
    "#]],
    );
    // A `_` in a message is a part of the member that nothing determines.
    check_diagnostics(
        src,
        expect![[r#"
        52..56: callers disagree on the type of `pair`: `a` makes it `fn({number}, _) -> {number}`, `b` makes it `fn(bool, _) -> bool`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 73..74)
        86..90: callers disagree on the type of `pair`: `a` makes it `fn({number}, _) -> {number}`, `b` makes it `fn(bool, _) -> bool`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 73..74)
    "#]],
    );
}

/// A member votes as a whole signature: callers that agree on part of it
/// still tie on all of it.
#[test]
fn callers_agreeing_on_part_of_a_signature_still_tie() {
    // TODO: halvko/must#233: the diagnostics should name the part each
    // caller disagrees on (`x` and `y` agree two to one).
    let src = r#"
static pair = fn (x, y) { x };
static a = fn () -> usize { pair(1, true) };
static b = fn (n: usize) -> usize { pair(n, n) };
static c = fn () -> bool { pair(false, true) };
"#;
    check_signatures(
        src,
        expect![[r#"
        pair: {error}
        a: fn() -> usize
        b: fn(usize) -> usize
        c: fn() -> bool
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        60..64: callers disagree on the type of `pair`: `a` makes it `fn(usize, bool) -> usize`, `b` makes it `fn(usize, usize) -> usize`, `c` makes it `fn(bool, bool) -> bool`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 84..85) (`c` is defined here at 134..135)
        113..117: callers disagree on the type of `pair`: `a` makes it `fn(usize, bool) -> usize`, `b` makes it `fn(usize, usize) -> usize`, `c` makes it `fn(bool, bool) -> bool`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 84..85) (`c` is defined here at 134..135)
        154..158: callers disagree on the type of `pair`: `a` makes it `fn(usize, bool) -> usize`, `b` makes it `fn(usize, usize) -> usize`, `c` makes it `fn(bool, bool) -> bool`; add a type annotation to its definition (defined here at 8..12) (`a` is defined here at 39..40) (`b` is defined here at 84..85) (`c` is defined here at 134..135)
    "#]],
    );
}

/// A caller passing an `unsafe fn` makes the callee's place `unsafe fn`;
/// the callers passing a safe fn still vote for its shape.
#[test]
fn unsafe_caller_joins_with_safe_callers_of_the_same_shape() {
    let src = r#"
unsafe extern static u: unsafe fn(usize) -> usize;
static k = fn (n: usize) -> usize { n };
static id = fn (x) { x };
static a = fn () -> () { let _ = id(k); };
static b = fn () -> () { let _ = id(k); };
static c = fn () -> () { let _ = id(u); };
static d = fn () -> () { let _ = id(true); };
"#;
    check_signatures(
        src,
        expect![[r#"
        u: unsafe fn(usize) -> usize
        k: fn(usize) -> usize
        id: fn(unsafe fn(usize) -> usize) -> unsafe fn(usize) -> usize
        a: fn()
        b: fn()
        c: fn()
        d: fn()
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        281..283: `id` is `fn(unsafe fn(usize) -> usize) -> unsafe fn(usize) -> usize` here, as `a`, `b` and `c` make it, but this use makes it `fn(bool) -> bool` (`a` is defined here at 126..127) (`b` is defined here at 169..170) (`c` is defined here at 212..213)
    "#]],
    );
}

/// Callers that leave a safety open in different places agree.
#[test]
fn open_safeties_in_different_places_agree() {
    let src = r#"
static k = fn (n: usize) -> usize { n };
static s: fn(usize) -> usize = fn (n) { n };
static id = fn (x) { x };
static a = fn () -> () { let _ = id(struct { f = k, g = s }); };
static b = fn () -> () { let _ = id(struct { f = s, g = k }); };
"#;
    check_signatures(
        src,
        expect![[r#"
        k: fn(usize) -> usize
        s: fn(usize) -> usize
        id: fn(struct { f: fn(usize) -> usize, g: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize, g: fn(usize) -> usize }
        a: fn()
        b: fn()
    "#]],
    );
    check_diagnostics(src, expect![[r#""#]]);
}

/// Two votes of one shape outvote a third shape, whatever safeties each
/// leaves open.
#[test]
fn votes_of_one_shape_win_whatever_safety_they_leave_open() {
    let src = r#"
static k = fn (n: usize) -> usize { n };
static s: fn(usize) -> usize = fn (n) { n };
static id = fn (x) { x };
static a = fn () -> () { let _ = id(struct { p = s, q = fn (n: usize) -> usize { n } }); };
static b = fn () -> () { let _ = id(struct { p = fn (n: usize) -> usize { n }, q = s }); };
static d = fn () -> () { let _ = id(true); };
"#;
    check_signatures(
        src,
        expect![[r#"
        k: fn(usize) -> usize
        s: fn(usize) -> usize
        id: fn(struct { p: fn(usize) -> usize, q: fn(usize) -> usize }) -> struct { p: fn(usize) -> usize, q: fn(usize) -> usize }
        a: fn()
        b: fn()
        d: fn()
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        330..332: `id` is `fn(struct { p: fn(usize) -> usize, q: fn(usize) -> usize }) -> struct { p: fn(usize) -> usize, q: fn(usize) -> usize }` here, as `a` and `b` make it, but this use makes it `fn(bool) -> bool` (`a` is defined here at 120..121) (`b` is defined here at 212..213)
    "#]],
    );
}

/// Votes join their safeties as a join's branches do: a caller passing an
/// `unsafe fn` makes the callee's place `unsafe fn`, and the callers that
/// call it as a safe fn lose their use to that caller.
#[test]
fn safe_and_unsafe_votes_join_to_unsafe() {
    let src = r#"
unsafe extern static u: unsafe fn(usize) -> usize;
static k: fn(usize) -> usize = fn (n) { n };
static id = fn (x) { x };
static a = fn () -> usize { id(k)(1) };
static b = fn () -> usize { id(k)(2) };
static c = fn () -> () { let _ = id(u); };
"#;
    check_signatures(
        src,
        expect![[r#"
            u: unsafe fn(usize) -> usize
            k: fn(usize) -> usize
            id: fn(unsafe fn(usize) -> usize) -> unsafe fn(usize) -> usize
            a: fn() -> usize
            b: fn() -> usize
            c: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            151..153: `id` is `fn(unsafe fn(usize) -> usize) -> unsafe fn(usize) -> usize` here, as `c` makes it, but this use makes it `fn(fn(usize) -> usize) -> fn(usize) -> usize` (`c` is defined here at 210..211)
            191..193: `id` is `fn(unsafe fn(usize) -> usize) -> unsafe fn(usize) -> usize` here, as `c` makes it, but this use makes it `fn(fn(usize) -> usize) -> fn(usize) -> usize` (`c` is defined here at 210..211)
        "#]],
    );
}

/// Below the top a function's safety is invariant (T13), so winners of one
/// shape that differ there don't join: the member ties, whatever order the
/// callers come in.
#[test]
fn votes_differing_in_a_nested_safety_tie() {
    let src = r#"
unsafe extern static u: unsafe fn(usize) -> usize;
static k: fn(usize) -> usize = fn (n) { n };
static id = fn (x) { x };
static a = fn () -> () { let p: struct { f: fn(usize) -> usize } = struct { f = k }; let _ = id(p); };
static b = fn () -> () { let p: struct { f: fn(usize) -> usize } = struct { f = k }; let _ = id(p); };
static c = fn () -> () { let p: struct { f: unsafe fn(usize) -> usize } = struct { f = u }; let _ = id(p); };
"#;
    check_signatures(
        src,
        expect![[r#"
            u: unsafe fn(usize) -> usize
            k: fn(usize) -> usize
            id: {error}
            a: fn()
            b: fn()
            c: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            216..218: callers disagree on the type of `id`: `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`a` is defined here at 130..131) (`b` is defined here at 233..234) (`c` is defined here at 336..337)
            319..321: callers disagree on the type of `id`: `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`a` is defined here at 130..131) (`b` is defined here at 233..234) (`c` is defined here at 336..337)
            429..431: callers disagree on the type of `id`: `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`a` is defined here at 130..131) (`b` is defined here at 233..234) (`c` is defined here at 336..337)
        "#]],
    );
    let swapped = r#"
unsafe extern static u: unsafe fn(usize) -> usize;
static k: fn(usize) -> usize = fn (n) { n };
static id = fn (x) { x };
static c = fn () -> () { let p: struct { f: unsafe fn(usize) -> usize } = struct { f = u }; let _ = id(p); };
static a = fn () -> () { let p: struct { f: fn(usize) -> usize } = struct { f = k }; let _ = id(p); };
static b = fn () -> () { let p: struct { f: fn(usize) -> usize } = struct { f = k }; let _ = id(p); };
"#;
    check_signatures(
        swapped,
        expect![[r#"
            u: unsafe fn(usize) -> usize
            k: fn(usize) -> usize
            id: {error}
            c: fn()
            a: fn()
            b: fn()
        "#]],
    );
    check_diagnostics(
        swapped,
        expect![[r#"
            223..225: callers disagree on the type of `id`: `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`, `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`c` is defined here at 130..131) (`a` is defined here at 240..241) (`b` is defined here at 343..344)
            326..328: callers disagree on the type of `id`: `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`, `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`c` is defined here at 130..131) (`a` is defined here at 240..241) (`b` is defined here at 343..344)
            429..431: callers disagree on the type of `id`: `c` makes it `fn(struct { f: unsafe fn(usize) -> usize }) -> struct { f: unsafe fn(usize) -> usize }`, `a` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`, `b` makes it `fn(struct { f: fn(usize) -> usize }) -> struct { f: fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..106) (`c` is defined here at 130..131) (`a` is defined here at 240..241) (`b` is defined here at 343..344)
        "#]],
    );
}

/// `a` and `b` each call as a safe fn what the other passes an `unsafe fn`
/// for, and `c` passing one value for both makes them one vote; the join
/// of the winners contradicts both, so `pick` is `{error}` and every use
/// reports the callers' disagreement. `c`'s `{error}`s follow from `pick`
/// being undecided.
#[test]
fn join_contradicting_every_winner_leaves_the_member_undecided() {
    let src = r#"
unsafe extern static u: unsafe fn(usize) -> usize;
static k: fn(usize) -> usize = fn (n) { n };
static pick = fn (x, y) { struct { p = x, q = y } };
static a = fn () { pick(u, k).q(1) };
static b = fn () { pick(k, u).p(2) };
static c = fn (z) { pick(z, z) };
"#;
    check_signatures(
        src,
        expect![[r#"
            u: unsafe fn(usize) -> usize
            k: fn(usize) -> usize
            pick: {error}
            a: fn() -> usize
            b: fn() -> usize
            c: fn({error}) -> struct { p: {error}, q: {error} }
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            169..173: callers disagree on the type of `pick`: `a` makes it `fn(unsafe fn(usize) -> usize, fn(usize) -> usize) -> struct { p: unsafe fn(usize) -> usize, q: fn(usize) -> usize }`, `b` makes it `fn(fn(usize) -> usize, unsafe fn(usize) -> usize) -> struct { p: fn(usize) -> usize, q: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..108) (`a` is defined here at 157..158) (`b` is defined here at 195..196)
            207..211: callers disagree on the type of `pick`: `a` makes it `fn(unsafe fn(usize) -> usize, fn(usize) -> usize) -> struct { p: unsafe fn(usize) -> usize, q: fn(usize) -> usize }`, `b` makes it `fn(fn(usize) -> usize, unsafe fn(usize) -> usize) -> struct { p: fn(usize) -> usize, q: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..108) (`a` is defined here at 157..158) (`b` is defined here at 195..196)
            246..250: callers disagree on the type of `pick`: `a` makes it `fn(unsafe fn(usize) -> usize, fn(usize) -> usize) -> struct { p: unsafe fn(usize) -> usize, q: fn(usize) -> usize }`, `b` makes it `fn(fn(usize) -> usize, unsafe fn(usize) -> usize) -> struct { p: fn(usize) -> usize, q: unsafe fn(usize) -> usize }`; add a type annotation to its definition (defined here at 104..108) (`a` is defined here at 157..158) (`b` is defined here at 195..196)
        "#]],
    );
}

/// An outvoted use no single body decided names no decider.
#[test]
fn outvoted_use_with_no_single_decider_names_none() {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), String::new());
    let diag = crate::infer::InferenceDiagnostic::OutvotedUse {
        expr: la_arena::Idx::from_raw(la_arena::RawIdx::from(0)),
        item: crate::ItemLoc::top_level(file, "id".into(), 0),
        ty: crate::ty::Ty::fn_type_with(false, vec![crate::ty::Ty::Bool], crate::ty::Ty::Bool),
        outvoted: crate::groups::Outvoted {
            use_ty: crate::ty::Ty::fn_type_with(
                false,
                vec![crate::ty::Ty::Unit],
                crate::ty::Ty::Unit,
            ),
            deciders: Vec::new(),
        },
    };
    expect!["`id` is `fn(bool) -> bool` here, but this use makes it `fn(())`"]
        .assert_eq(&diag.message());
}

/// `a`'s `g` and `b`'s `h` each win their own vote, but `g`'s body links
/// the two, so the winners contradict each other: both members tie, and each
/// use names the bodies on both sides, whatever order the items come in.
#[test]
fn callers_contradicting_through_a_shared_variable_tie_both_members() {
    let src = r#"
static g = fn (x) { h(x) };
static h = fn (y) { y };
static a = fn () -> usize { g(1) };
static b = fn () -> bool { h(true) };
"#;
    check_signatures(
        src,
        expect![[r#"
            g: {error}
            h: {error}
            a: fn() -> usize
            b: fn() -> bool
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            21..22: callers disagree on the type of `h`, which shares a type with `g`: `b` makes `h` `fn(bool) -> bool`, `a` makes `g` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 36..37) (`b` is defined here at 97..98) (`g` is defined here at 8..9) (`a` is defined here at 61..62)
            82..83: callers disagree on the type of `g`, which shares a type with `h`: `a` makes `g` `fn(usize) -> usize`, `b` makes `h` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 61..62) (`h` is defined here at 36..37) (`b` is defined here at 97..98)
            117..118: callers disagree on the type of `h`, which shares a type with `g`: `b` makes `h` `fn(bool) -> bool`, `a` makes `g` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 36..37) (`b` is defined here at 97..98) (`g` is defined here at 8..9) (`a` is defined here at 61..62)
        "#]],
    );
    let swapped = r#"
static h = fn (y) { y };
static g = fn (x) { h(x) };
static a = fn () -> usize { g(1) };
static b = fn () -> bool { h(true) };
"#;
    check_signatures(
        swapped,
        expect![[r#"
            h: {error}
            g: {error}
            a: fn() -> usize
            b: fn() -> bool
        "#]],
    );
    check_diagnostics(
        swapped,
        expect![[r#"
            46..47: callers disagree on the type of `h`, which shares a type with `g`: `b` makes `h` `fn(bool) -> bool`, `a` makes `g` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`b` is defined here at 97..98) (`g` is defined here at 33..34) (`a` is defined here at 61..62)
            82..83: callers disagree on the type of `g`, which shares a type with `h`: `a` makes `g` `fn(usize) -> usize`, `b` makes `h` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 33..34) (`a` is defined here at 61..62) (`h` is defined here at 8..9) (`b` is defined here at 97..98)
            117..118: callers disagree on the type of `h`, which shares a type with `g`: `b` makes `h` `fn(bool) -> bool`, `a` makes `g` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`b` is defined here at 97..98) (`g` is defined here at 33..34) (`a` is defined here at 61..62)
        "#]],
    );
}

/// A caller that links two members ties them when their winners contradict
/// each other through it, whatever order the items come in.
#[test]
fn callers_disagreeing_through_a_linking_caller_are_blamed_in_any_order() {
    let f_first = r#"
static f = fn (x) { };
static g = fn (y) { y };
static h = fn () { h() };
static a = fn (n: usize) -> () { f(n) };
static b = fn () -> bool { g(true) };
static c = fn () -> () { f(g(h())) };
"#;
    check_signatures(
        f_first,
        expect![[r#"
            f: {error}
            g: {error}
            h: {error}
            a: fn(usize)
            b: fn() -> bool
            c: fn()
        "#]],
    );
    // A `_` in a message is a part of the member that nothing determines.
    check_diagnostics(
        f_first,
        expect![[r#"
            68..69: callers disagree on the type of `h`, which shares a type with `f` and `g`: `c` makes `h` `fn() -> _`, `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 56..57) (`c` is defined here at 161..162) (`f` is defined here at 8..9) (`a` is defined here at 82..83) (`g` is defined here at 31..32) (`b` is defined here at 123..124)
            108..109: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 82..83) (`g` is defined here at 31..32) (`b` is defined here at 123..124)
            143..144: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 31..32) (`b` is defined here at 123..124) (`f` is defined here at 8..9) (`a` is defined here at 82..83)
            179..180: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 82..83) (`g` is defined here at 31..32) (`b` is defined here at 123..124)
            181..182: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 31..32) (`b` is defined here at 123..124) (`f` is defined here at 8..9) (`a` is defined here at 82..83)
            183..184: callers disagree on the type of `h`, which shares a type with `f` and `g`: `c` makes `h` `fn() -> _`, `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 56..57) (`c` is defined here at 161..162) (`f` is defined here at 8..9) (`a` is defined here at 82..83) (`g` is defined here at 31..32) (`b` is defined here at 123..124)
        "#]],
    );
    let g_first = r#"
static g = fn (y) { y };
static f = fn (x) { };
static h = fn () { h() };
static a = fn (n: usize) -> () { f(n) };
static b = fn () -> bool { g(true) };
static c = fn () -> () { f(g(h())) };
"#;
    check_signatures(
        g_first,
        expect![[r#"
            g: {error}
            f: {error}
            h: {error}
            a: fn(usize)
            b: fn() -> bool
            c: fn()
        "#]],
    );
    check_diagnostics(
        g_first,
        expect![[r#"
            68..69: callers disagree on the type of `h`, which shares a type with `g` and `f`: `c` makes `h` `fn() -> _`, `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 56..57) (`c` is defined here at 161..162) (`g` is defined here at 8..9) (`b` is defined here at 123..124) (`f` is defined here at 33..34) (`a` is defined here at 82..83)
            108..109: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 33..34) (`a` is defined here at 82..83) (`g` is defined here at 8..9) (`b` is defined here at 123..124)
            143..144: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 8..9) (`b` is defined here at 123..124) (`f` is defined here at 33..34) (`a` is defined here at 82..83)
            179..180: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize)`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 33..34) (`a` is defined here at 82..83) (`g` is defined here at 8..9) (`b` is defined here at 123..124)
            181..182: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 8..9) (`b` is defined here at 123..124) (`f` is defined here at 33..34) (`a` is defined here at 82..83)
            183..184: callers disagree on the type of `h`, which shares a type with `g` and `f`: `c` makes `h` `fn() -> _`, `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize)`; add a type annotation to its definition (defined here at 56..57) (`c` is defined here at 161..162) (`g` is defined here at 8..9) (`b` is defined here at 123..124) (`f` is defined here at 33..34) (`a` is defined here at 82..83)
        "#]],
    );
}

/// `f`'s callers tie, so nothing fills `f`'s `z`, nor `g`'s parameter,
/// which `f`'s body links to it, though both callers agree on it.
#[test]
fn member_linked_to_a_tied_member_stays_open() {
    // TODO: halvko/must#233: `g` should be `fn(bool)`, which both callers
    // agree on.
    let src = r#"
static g = fn (w) { };
static f = fn (x, z) { g(z); x };
static a = fn () -> () { let _: usize = f(1, true); };
static b = fn () -> () { let _: bool = f(true, true); };
"#;
    check_signatures(
        src,
        expect![[r#"
            g: fn({error})
            f: {error}
            a: fn()
            b: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            47..48: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 8..9)
            98..99: callers disagree on the type of `f`: `a` makes it `fn(usize, bool) -> usize`, `b` makes it `fn(bool, bool) -> bool`; add a type annotation to its definition (defined here at 31..32) (`a` is defined here at 65..66) (`b` is defined here at 120..121)
            152..153: callers disagree on the type of `f`: `a` makes it `fn(usize, bool) -> usize`, `b` makes it `fn(bool, bool) -> bool`; add a type annotation to its definition (defined here at 31..32) (`a` is defined here at 65..66) (`b` is defined here at 120..121)
        "#]],
    );
}

/// A caller that contradicts what a member's own body decided fills
/// nothing and reports the mismatch itself.
#[test]
fn fully_typed_caller_contradicting_a_decided_part_is_not_a_disagreement() {
    let src = r#"
static f = fn (x, y) { let n: usize = x; y };
static a = fn () -> usize { f(true, 1) };
static b = fn () -> usize { f(1, 2) };
"#;
    check_signatures(
        src,
        expect![[r#"
        f: fn(usize, usize) -> usize
        a: fn() -> usize
        b: fn() -> usize
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        77..81: type mismatch: expected `usize`, found `bool`
    "#]],
    );
}

/// A caller that leaves a literal unpinned agrees with one that pins it.
#[test]
fn fully_typed_callers_with_a_number_and_a_usize_agree() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> () { id(1); };
static b = fn () -> usize { id(2) };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn(usize) -> usize
        a: fn()
        b: fn() -> usize
    "#]],
    );
    check_diagnostics(src, expect![[r#""#]]);
}

/// Callers that both leave a literal unpinned agree; the hole stays open,
/// so `id` is undetermined, not disputed.
#[test]
fn fully_typed_callers_that_leave_numbers_unpinned_agree() {
    let src = r#"
static id = fn (x) { x };
static a = fn () -> () { id(1); };
static b = fn () -> () { id(2); };
"#;
    check_signatures(
        src,
        expect![[r#"
        id: fn({error}) -> {error}
        a: fn()
        b: fn()
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
        52..54: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        87..89: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
    "#]],
    );
}

/// Fully typed callers that agree each fill their part of the hole.
#[test]
fn fully_typed_callers_that_agree_fill_together() {
    let src = r#"
static pair = fn (x, y) { x };
static a = fn () -> usize { pair(1, true) };
static b = fn () -> usize { pair(2, false) };
"#;
    check_signatures(
        src,
        expect![[r#"
        pair: fn(usize, bool) -> usize
        a: fn() -> usize
        b: fn() -> usize
    "#]],
    );
    check_diagnostics(src, expect![[r#""#]]);
}

/// A tie between callers that are group members names each with the type
/// it gives the callee.
#[test]
fn member_callers_that_disagree_are_named() {
    check_diagnostics(
        r#"
static id = fn (x) { x };
static a = fn () { let n: usize = id(1); n };
static b = fn () { let t: bool = id(true); t };
"#,
        expect![[r#"
            61..63: callers disagree on the type of `id`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 80..81)
            106..108: callers disagree on the type of `id`: `a` makes it `fn(usize) -> usize`, `b` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 34..35) (`b` is defined here at 80..81)
        "#]],
    );
}

/// In a cycle, a caller that contradicts the callee's own body loses to
/// it, as a caller outside the cycle would.
#[test]
fn caller_contradicting_its_callees_body_in_a_cycle_loses_to_it() {
    let src = r#"
static f = fn (x) { let n: usize = x; if true { n } else { g() } };
static g = fn () { f(true) };
"#;
    check_signatures(
        src,
        expect![[r#"
        f: fn(usize) -> usize
        g: fn() -> usize
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            88..89: `f` is `fn(usize) -> usize` here, as its own body makes it, but this use makes it `fn(bool) -> usize` (`f` is defined here at 8..9)
        "#]],
    );
    check_diagnostics(
        r#"
static id = fn (x) { x };
static b = fn () -> bool { id(true) };
static a = fn () -> usize { id(1) };
"#,
        expect![[r#"
            54..56: callers disagree on the type of `id`: `b` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 34..35) (`a` is defined here at 73..74)
            94..96: callers disagree on the type of `id`: `b` makes it `fn(bool) -> bool`, `a` makes it `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 34..35) (`a` is defined here at 73..74)
        "#]],
    );
}

/// A use in a call cycle that contradicts what the callee's annotation
/// writes is reported at the use, as outside a cycle.
#[test]
fn cycle_use_contradicting_a_written_annotation_is_a_mismatch() {
    let text = r#"
static f = fn () { g(1) };
static g = fn (x: bool) -> _ { f(); x };
"#;
    check_signatures(
        text,
        expect![[r#"
        f: fn() -> bool
        g: fn(bool) -> bool
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        22..23: type mismatch: expected `bool`, found `{number}`
    "#]],
    );
}

/// A member linked to conflicting ones ties with them.
// TODO: halvko/must#234: `m` agrees with every use and should stay decided.
#[test]
fn member_linked_beside_a_conflict_ties_with_it() {
    let text = r#"
static id = fn (x) { x };
static k = fn (y, w) { w; y };
static m = fn (z) { z };
static tie = fn (p, s) { id(p); k(p, s); m(s) };
static a = fn () -> usize { id(1) };
static c = fn (w: usize) -> bool { k(true, w) };
static d = fn () -> usize { m(3) };
"#;
    check_signatures(
        text,
        expect![[r#"
            id: {error}
            k: {error}
            m: {error}
            tie: fn({error}, {error}) -> {error}
            a: fn() -> usize
            c: fn(usize) -> bool
            d: fn() -> usize
        "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            108..110: callers disagree on the type of `id`, which shares a type with `k` and `m`: `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 139..140) (`k` is defined here at 34..35) (`c` is defined here at 176..177) (`m` is defined here at 65..66) (`d` is defined here at 225..226)
            115..116: callers disagree on the type of `k`, which shares a type with `id` and `m`: `c` makes `k` `fn(bool, usize) -> bool`, `a` makes `id` `fn(usize) -> usize`, `d` makes `m` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`c` is defined here at 176..177) (`id` is defined here at 8..10) (`a` is defined here at 139..140) (`m` is defined here at 65..66) (`d` is defined here at 225..226)
            124..125: callers disagree on the type of `m`, which shares a type with `id` and `k`: `d` makes `m` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize) -> bool`; add a type annotation to its definition (defined here at 65..66) (`d` is defined here at 225..226) (`id` is defined here at 8..10) (`a` is defined here at 139..140) (`k` is defined here at 34..35) (`c` is defined here at 176..177)
            160..162: callers disagree on the type of `id`, which shares a type with `k` and `m`: `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 139..140) (`k` is defined here at 34..35) (`c` is defined here at 176..177) (`m` is defined here at 65..66) (`d` is defined here at 225..226)
            204..205: callers disagree on the type of `k`, which shares a type with `id` and `m`: `c` makes `k` `fn(bool, usize) -> bool`, `a` makes `id` `fn(usize) -> usize`, `d` makes `m` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`c` is defined here at 176..177) (`id` is defined here at 8..10) (`a` is defined here at 139..140) (`m` is defined here at 65..66) (`d` is defined here at 225..226)
            246..247: callers disagree on the type of `m`, which shares a type with `id` and `k`: `d` makes `m` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize) -> bool`; add a type annotation to its definition (defined here at 65..66) (`d` is defined here at 225..226) (`id` is defined here at 8..10) (`a` is defined here at 139..140) (`k` is defined here at 34..35) (`c` is defined here at 176..177)
        "#]],
    );
}

/// Two conflicts linked through one body tie as one group, with every
/// member linked to them.
// TODO: halvko/must#234: `o` agrees with every use and should stay decided.
#[test]
fn conflicts_linked_through_one_body_tie_as_one_group() {
    let text = r#"
static id = fn (x) { x };
static k = fn (y, w, u) { w; u; y };
static m = fn (z) { z };
static n = fn (v) { v };
static o = fn (r) { r };
static tie = fn (p, s, t) { id(p); k(p, s, t); m(s); n(s); o(t) };
static a = fn () -> usize { id(1) };
static c = fn (w: usize, u: usize) -> bool { k(true, w, u) };
static d = fn () -> usize { m(4) };
static e = fn () -> bool { n(false) };
static f = fn () -> usize { o(5) };
"#;
    check_signatures(
        text,
        expect![[r#"
            id: {error}
            k: {error}
            m: {error}
            n: {error}
            o: {error}
            tie: fn({error}, {error}, {error}) -> {error}
            a: fn() -> usize
            c: fn(usize, usize) -> bool
            d: fn() -> usize
            e: fn() -> bool
            f: fn() -> usize
        "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            167..169: callers disagree on the type of `id`, which shares a type with `k`, `m`, `n` and `o`: `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            174..175: callers disagree on the type of `k`, which shares a type with `id`, `m`, `n` and `o`: `c` makes `k` `fn(bool, usize, usize) -> bool`, `a` makes `id` `fn(usize) -> usize`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`c` is defined here at 250..251) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            186..187: callers disagree on the type of `m`, which shares a type with `id`, `k`, `n` and `o`: `d` makes `m` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 71..72) (`d` is defined here at 312..313) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            192..193: callers disagree on the type of `n`, which shares a type with `id`, `k`, `m` and `o`: `e` makes `n` `fn(bool) -> bool`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 96..97) (`e` is defined here at 348..349) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            198..199: callers disagree on the type of `o`, which shares a type with `id`, `k`, `m` and `n`: `f` makes `o` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 121..122) (`f` is defined here at 387..388) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349)
            234..236: callers disagree on the type of `id`, which shares a type with `k`, `m`, `n` and `o`: `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            288..289: callers disagree on the type of `k`, which shares a type with `id`, `m`, `n` and `o`: `c` makes `k` `fn(bool, usize, usize) -> bool`, `a` makes `id` `fn(usize) -> usize`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 34..35) (`c` is defined here at 250..251) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            333..334: callers disagree on the type of `m`, which shares a type with `id`, `k`, `n` and `o`: `d` makes `m` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `e` makes `n` `fn(bool) -> bool`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 71..72) (`d` is defined here at 312..313) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`n` is defined here at 96..97) (`e` is defined here at 348..349) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            368..369: callers disagree on the type of `n`, which shares a type with `id`, `k`, `m` and `o`: `e` makes `n` `fn(bool) -> bool`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `f` makes `o` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 96..97) (`e` is defined here at 348..349) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`o` is defined here at 121..122) (`f` is defined here at 387..388)
            408..409: callers disagree on the type of `o`, which shares a type with `id`, `k`, `m` and `n`: `f` makes `o` `fn(usize) -> usize`, `a` makes `id` `fn(usize) -> usize`, `c` makes `k` `fn(bool, usize, usize) -> bool`, `d` makes `m` `fn(usize) -> usize`, `e` makes `n` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 121..122) (`f` is defined here at 387..388) (`id` is defined here at 8..10) (`a` is defined here at 213..214) (`k` is defined here at 34..35) (`c` is defined here at 250..251) (`m` is defined here at 71..72) (`d` is defined here at 312..313) (`n` is defined here at 96..97) (`e` is defined here at 348..349)
        "#]],
    );
}

/// Members that share a variable through a third member tie; a member
/// linked to none of them stays decided.
#[test]
fn tie_through_a_linking_member_leaves_the_bystander_decided() {
    let text = r#"
static c = true;
static f = fn (x) { x };
static g = fn (x) { x };
static h = fn (x, y) { if c { x } else { y } };
static o = fn (r) { r };
static tie = fn (p, q, t) { f(p); g(q); h(p, q); o(t) };
static a = fn () { let _: usize = f(1); };
static b = fn () { let _: bool = g(true); };
static d = fn () { let _: usize = o(5); };
"#;
    check_signatures(
        text,
        expect![[r#"
            c: bool
            f: {error}
            g: {error}
            h: {error}
            o: fn(usize) -> usize
            tie: fn({error}, {error}, usize) -> usize
            a: fn()
            b: fn()
            d: fn()
        "#]],
    );
    // A `_` in a message is a part of the member that nothing determines.
    check_diagnostics(
        text,
        expect![[r#"
            169..170: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 25..26) (`a` is defined here at 205..206) (`g` is defined here at 50..51) (`b` is defined here at 248..249)
            175..176: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 50..51) (`b` is defined here at 248..249) (`f` is defined here at 25..26) (`a` is defined here at 205..206)
            181..182: callers disagree on the type of `h`, which shares a type with `f` and `g`: `tie` makes `h` `fn(_, _) -> _`, `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 75..76) (`tie` is defined here at 148..151) (`f` is defined here at 25..26) (`a` is defined here at 205..206) (`g` is defined here at 50..51) (`b` is defined here at 248..249)
            232..233: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 25..26) (`a` is defined here at 205..206) (`g` is defined here at 50..51) (`b` is defined here at 248..249)
            274..275: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 50..51) (`b` is defined here at 248..249) (`f` is defined here at 25..26) (`a` is defined here at 205..206)
        "#]],
    );
}

/// Members linked only through an open fn safety tie together, whichever
/// is declared first.
#[test]
fn members_linked_through_an_open_safety_tie_in_any_order() {
    let k_first = r#"
static k = fn (n) { n };
static g = fn () { k };
static a = fn () -> usize { k(1) };
static c = fn () -> () { let p: fn() -> unsafe fn(usize) -> usize = g; };
"#;
    check_signatures(
        k_first,
        expect![[r#"
        k: {error}
        g: {error}
        a: fn() -> usize
        c: fn()
    "#]],
    );
    check_diagnostics(
        k_first,
        expect![[r#"
            45..46: callers disagree on the type of `k`, which shares a type with `g`: `a` makes `k` `fn(usize) -> usize`, `c` makes `g` `fn() -> unsafe fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 57..58) (`g` is defined here at 33..34) (`c` is defined here at 93..94)
            78..79: callers disagree on the type of `k`, which shares a type with `g`: `a` makes `k` `fn(usize) -> usize`, `c` makes `g` `fn() -> unsafe fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`a` is defined here at 57..58) (`g` is defined here at 33..34) (`c` is defined here at 93..94)
            154..155: callers disagree on the type of `g`, which shares a type with `k`: `c` makes `g` `fn() -> unsafe fn(usize) -> usize`, `a` makes `k` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 33..34) (`c` is defined here at 93..94) (`k` is defined here at 8..9) (`a` is defined here at 57..58)
        "#]],
    );
    let g_first = r#"
static g = fn () { k };
static k = fn (n) { n };
static a = fn () -> usize { k(1) };
static c = fn () -> () { let p: fn() -> unsafe fn(usize) -> usize = g; };
"#;
    check_signatures(
        g_first,
        expect![[r#"
        g: {error}
        k: {error}
        a: fn() -> usize
        c: fn()
    "#]],
    );
    check_diagnostics(
        g_first,
        expect![[r#"
            20..21: callers disagree on the type of `k`, which shares a type with `g`: `a` makes `k` `fn(usize) -> usize`, `c` makes `g` `fn() -> unsafe fn(usize) -> usize`; add a type annotation to its definition (defined here at 32..33) (`a` is defined here at 57..58) (`g` is defined here at 8..9) (`c` is defined here at 93..94)
            78..79: callers disagree on the type of `k`, which shares a type with `g`: `a` makes `k` `fn(usize) -> usize`, `c` makes `g` `fn() -> unsafe fn(usize) -> usize`; add a type annotation to its definition (defined here at 32..33) (`a` is defined here at 57..58) (`g` is defined here at 8..9) (`c` is defined here at 93..94)
            154..155: callers disagree on the type of `g`, which shares a type with `k`: `c` makes `g` `fn() -> unsafe fn(usize) -> usize`, `a` makes `k` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 8..9) (`c` is defined here at 93..94) (`k` is defined here at 32..33) (`a` is defined here at 57..58)
        "#]],
    );
}

/// Two separate conflicts in one unit each tie on their own, and a member
/// linked to neither stays decided.
#[test]
fn separate_conflicts_leave_an_unlinked_member_decided() {
    let text = r#"
static c = true;
static f = fn (x) { x };
static g = fn (x) { x };
static h = fn (x, y) { if c { x } else { y } };
static f2 = fn (x) { x };
static g2 = fn (x) { x };
static h2 = fn (x, y) { if c { x } else { y } };
static o = fn (r) { r };
static tie = fn (p, q, p2, q2, t) { f(p); g(q); h(p, q); f2(p2); g2(q2); h2(p2, q2); o(t) };
static a = fn () { let _: usize = f(1); let _: usize = f2(1); };
static b = fn () { let _: bool = g(true); let _: bool = g2(true); };
static d = fn () { let _: usize = o(5); };
"#;
    check_signatures(
        text,
        expect![[r#"
        c: bool
        f: {error}
        g: {error}
        h: {error}
        f2: {error}
        g2: {error}
        h2: {error}
        o: fn(usize) -> usize
        tie: fn({error}, {error}, {error}, {error}, usize) -> usize
        a: fn()
        b: fn()
        d: fn()
    "#]],
    );
    // A `_` in a message is a part of the member that nothing determines.
    check_diagnostics(
        text,
        expect![[r#"
        278..279: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 25..26) (`a` is defined here at 342..343) (`g` is defined here at 50..51) (`b` is defined here at 407..408)
        284..285: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 50..51) (`b` is defined here at 407..408) (`f` is defined here at 25..26) (`a` is defined here at 342..343)
        290..291: callers disagree on the type of `h`, which shares a type with `f` and `g`: `tie` makes `h` `fn(_, _) -> _`, `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 75..76) (`tie` is defined here at 249..252) (`f` is defined here at 25..26) (`a` is defined here at 342..343) (`g` is defined here at 50..51) (`b` is defined here at 407..408)
        299..301: callers disagree on the type of `f2`, which shares a type with `g2`: `a` makes `f2` `fn(usize) -> usize`, `b` makes `g2` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 123..125) (`a` is defined here at 342..343) (`g2` is defined here at 149..151) (`b` is defined here at 407..408)
        307..309: callers disagree on the type of `g2`, which shares a type with `f2`: `b` makes `g2` `fn(bool) -> bool`, `a` makes `f2` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 149..151) (`b` is defined here at 407..408) (`f2` is defined here at 123..125) (`a` is defined here at 342..343)
        315..317: callers disagree on the type of `h2`, which shares a type with `f2` and `g2`: `tie` makes `h2` `fn(_, _) -> _`, `a` makes `f2` `fn(usize) -> usize`, `b` makes `g2` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 175..177) (`tie` is defined here at 249..252) (`f2` is defined here at 123..125) (`a` is defined here at 342..343) (`g2` is defined here at 149..151) (`b` is defined here at 407..408)
        369..370: callers disagree on the type of `f`, which shares a type with `g`: `a` makes `f` `fn(usize) -> usize`, `b` makes `g` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 25..26) (`a` is defined here at 342..343) (`g` is defined here at 50..51) (`b` is defined here at 407..408)
        390..392: callers disagree on the type of `f2`, which shares a type with `g2`: `a` makes `f2` `fn(usize) -> usize`, `b` makes `g2` `fn(bool) -> bool`; add a type annotation to its definition (defined here at 123..125) (`a` is defined here at 342..343) (`g2` is defined here at 149..151) (`b` is defined here at 407..408)
        433..434: callers disagree on the type of `g`, which shares a type with `f`: `b` makes `g` `fn(bool) -> bool`, `a` makes `f` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 50..51) (`b` is defined here at 407..408) (`f` is defined here at 25..26) (`a` is defined here at 342..343)
        456..458: callers disagree on the type of `g2`, which shares a type with `f2`: `b` makes `g2` `fn(bool) -> bool`, `a` makes `f2` `fn(usize) -> usize`; add a type annotation to its definition (defined here at 149..151) (`b` is defined here at 407..408) (`f2` is defined here at 123..125) (`a` is defined here at 342..343)
    "#]],
    );
}

/// A destructuring parameter writes its type for the cycle's uses too.
#[test]
fn cycle_use_contradicting_a_destructuring_parameter_is_a_mismatch() {
    let text = r#"
type Foo = struct { n: usize };
type Bar = struct { m: usize };
static f = fn (Foo(struct { n })) -> usize { if n == 0 { 0 } else { g(n) } };
static g = fn (k) { f(Bar(struct { m = k })) };
"#;
    check_signatures(
        text,
        expect![[r#"
        f: fn(Foo) -> usize
        g: fn(usize) -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        165..186: type mismatch: expected `Foo`, found `Bar`
    "#]],
    );
}

/// A hole in the item annotation does not hide what the fn literal writes.
#[test]
fn cycle_use_contradicting_a_literal_under_a_holed_annotation_is_a_mismatch() {
    let text = r#"
static even: _ = fn (n: usize) -> bool { if n == 0 { true } else { odd(n - 1) } };
static odd = fn (n) { even(true) };
"#;
    check_signatures(
        text,
        expect![[r#"
        even: fn(usize) -> bool
        odd: fn(usize) -> bool
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        111..115: type mismatch: expected `usize`, found `bool`
    "#]],
    );
}

/// Uses that disagree on one member leave only it undecided, not the
/// members it shares variables with.
#[test]
fn disagreeing_uses_leave_only_their_member_undecided() {
    let text = r#"
static id = fn (x) { x };
static k = fn (y) { y };
static pair = fn (p, q) { k(q); id(p) };
static a = fn () -> usize { k(1) };
static b = fn () -> usize { id(2) };
static c = fn () -> bool { id(true) };
"#;
    check_signatures(
        text,
        expect![[r#"
        id: {error}
        k: fn(usize) -> usize
        pair: fn({error}, usize) -> {error}
        a: fn() -> usize
        b: fn() -> usize
        c: fn() -> bool
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            84..86: callers disagree on the type of `id`: `b` makes it `fn(usize) -> usize`, `c` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 136..137) (`c` is defined here at 173..174)
            157..159: callers disagree on the type of `id`: `b` makes it `fn(usize) -> usize`, `c` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 136..137) (`c` is defined here at 173..174)
            193..195: callers disagree on the type of `id`: `b` makes it `fn(usize) -> usize`, `c` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 8..10) (`b` is defined here at 136..137) (`c` is defined here at 173..174)
        "#]],
    );
}

/// A safe member used where an `unsafe fn` is expected coerces in a call
/// cycle too, in either order.
#[test]
fn safe_member_used_as_unsafe_fn_in_a_cycle_coerces() {
    let q_first = r#"
static apply = fn (f: unsafe fn()) -> () { };
static q = fn () { p() };
static p = fn () { apply(q) };
"#;
    check_signatures(
        q_first,
        expect![[r#"
        apply: fn(unsafe fn())
        q: fn()
        p: fn()
    "#]],
    );
    check_diagnostics(q_first, expect![[r#""#]]);
    let p_first = r#"
static apply = fn (f: unsafe fn()) -> () { };
static p = fn () { apply(q) };
static q = fn () { p() };
"#;
    check_signatures(
        p_first,
        expect![[r#"
        apply: fn(unsafe fn())
        p: fn()
        q: fn()
    "#]],
    );
    check_diagnostics(p_first, expect![[r#""#]]);
}

/// A generic caller is a hard firewall: its body sees `T` as a rigid
/// param, and filling a monotype hole with it would leak `T` out of its
/// binder.
#[test]
fn generic_caller_does_not_fill_its_callees_holes() {
    check_diagnostics(
        r#"
static id = fn (x) { x };
static wrap = fn::<T>(t: T) -> T { id(t) };
"#,
        expect![[r#"
            62..64: not enough information to infer the type of `id`; add a type annotation to its definition (calls from a generic function don't fill in its holes) (defined here at 8..10)
        "#]],
    );
}

/// The same for a member of a generic type: its `Self` mentions the
/// owner's params.
#[test]
fn generic_types_member_does_not_fill_its_callees_holes() {
    check_diagnostics(
        r#"
static id = fn (x) { x };
type W = struct::<T> { t: T } with {
    impl Self {
        get = fn(w: Self) -> Self { id(w) };
    }
};
"#,
        expect![[r#"
            116..118: not enough information to infer the type of `id`; add a type annotation to its definition (calls from a generic function don't fill in its holes) (defined here at 8..10)
        "#]],
    );
}
