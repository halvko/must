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

/// Two fully typed callers that disagree leave the hole undecided, in
/// either order: neither wins by coming first (T11).
#[test]
fn disagreeing_fully_typed_callers_leave_the_hole_undecided() {
    check_signatures(
        r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { id(true) };
"#,
        expect![[r#"
            id: {error}
            a: fn() -> usize
            b: fn() -> bool
        "#]],
    );
    check_signatures(
        r#"
static id = fn (x) { x };
static b = fn () -> bool { id(true) };
static a = fn () -> usize { id(1) };
"#,
        expect![[r#"
            id: {error}
            b: fn() -> bool
            a: fn() -> usize
        "#]],
    );
    check_diagnostics(
        r#"
static id = fn (x) { x };
static a = fn () -> usize { id(1) };
static b = fn () -> bool { id(true) };
"#,
        expect![[r#"
            55..57: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
            91..93: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        "#]],
    );
    check_diagnostics(
        r#"
static id = fn (x) { x };
static b = fn () -> bool { id(true) };
static a = fn () -> usize { id(1) };
"#,
        expect![[r#"
            54..56: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
            94..96: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
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

/// When uses conflict only through shared variables, a member linked to
/// the conflict by another variable stays decided.
#[test]
fn member_linked_beside_a_conflict_stays_decided() {
    let text = r#"
static id = fn (x) { x };
static k = fn (y, w) { w; y };
static m = fn (z) { z };
static tie = fn (p, s) { id(p); k(p, s); m(s) };
static a = fn () -> usize { id(1) };
static c = fn () -> bool { k(true, 2) };
static d = fn () -> usize { m(3) };
"#;
    check_signatures(
        text,
        expect![[r#"
        id: {error}
        k: {error}
        m: fn(usize) -> usize
        tie: fn({error}, usize) -> usize
        a: fn() -> usize
        c: fn() -> bool
        d: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        108..110: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        115..116: not enough information to infer the type of `k`; add a type annotation to its definition (defined here at 34..35)
        160..162: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        196..197: not enough information to infer the type of `k`; add a type annotation to its definition (defined here at 34..35)
    "#]],
    );
}

/// Two conflicts in one unit leave every member of it undecided, bystanders
/// included: no single member's removal lets the rest agree (#172).
#[test]
fn two_conflicts_in_one_unit_leave_all_of_it_undecided() {
    let text = r#"
static id = fn (x) { x };
static k = fn (y, w, u) { w; u; y };
static m = fn (z) { z };
static n = fn (v) { v };
static o = fn (r) { r };
static tie = fn (p, s, t) { id(p); k(p, s, t); m(s); n(s); o(t) };
static a = fn () -> usize { id(1) };
static c = fn () -> bool { k(true, 2, 3) };
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
        c: fn() -> bool
        d: fn() -> usize
        e: fn() -> bool
        f: fn() -> usize
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
        84..86: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        157..159: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
        193..195: not enough information to infer the type of `id`; add a type annotation to its definition (defined here at 8..10)
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
