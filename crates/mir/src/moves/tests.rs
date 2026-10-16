//! The linear check end to end: hir's half and this pass's, as the editor
//! reports them.

use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};
use syntax::TextRange;

/// A rendered finding: its range, its message, and its notes.
type Rendered = (TextRange, String, Vec<(TextRange, String)>);

/// Every hir diagnostic, then every finding of this pass, each with its
/// notes — the order the aggregator reports them in.
fn diagnostics(text: &str) -> Vec<Rendered> {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut found: Vec<_> = hir::file_diagnostics(&db, file)
        .into_iter()
        .map(|diag| {
            let notes = diag.related.iter().map(|r| (r.range, r.message.clone()));
            (diag.range, diag.message, notes.collect())
        })
        .collect();
    for item in hir::all_checkable_items(&db, file) {
        let (_, source_map) = hir::body_with_source_map(&db, item);
        for diag in crate::move_check(&db, item) {
            let Some(range) = diag.range(source_map) else {
                continue;
            };
            let notes = diag.related(&db, item, source_map);
            found.push((range, diag.message(&db, item), notes));
        }
    }
    found
}

fn check_diagnostics(text: &str, expect: Expect) {
    let rendered: String = diagnostics(text)
        .into_iter()
        .map(|(range, message, notes)| {
            let notes: String = notes
                .iter()
                .map(|(range, note)| format!(" ({note} at {range:?})"))
                .collect();
            format!("{range:?}: {message}{notes}\n")
        })
        .collect();
    expect.assert_eq(&rendered);
}

fn diagnostic_messages(text: &str) -> Vec<String> {
    diagnostics(text)
        .into_iter()
        .map(|(_, message, _)| message)
        .collect()
}

// ---- linear types: the `forget` capability and must-consume checking ----

/// The shape every test below builds on: a nominal type that has shed
/// `forget`, plus a consuming `drop` that takes it apart. `drop` needs no
/// compiler support — destructuring hands the obligation to the parts, and
/// the parts are two integers.
const LINEAR_PRELUDE: &str = r#"
type Res = struct { id: usize, size: usize } only move with {
    impl Self {
        drop = fn(r: Self) -> () {
            let Res(struct { id, size }) = r;
            print_two(id, size);
        };
    }
};
static print_two = fn(a: usize, b: usize) -> () { };
static make = fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
"#;

fn check_linear(body: &str, expect: Expect) {
    check_diagnostics(&format!("{LINEAR_PRELUDE}{body}"), expect);
}

#[test]
fn a_linear_consumed_on_the_one_path_is_clean() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn a_linear_left_alive_at_the_end_of_a_scope_is_a_leak() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
};
"#,
        expect![[r#"
            361..385: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 371..372)
        "#]],
    );
}

#[test]
fn consuming_a_linear_twice_is_the_use_after_move_error() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    r.drop();
    r.drop();
};
"#,
        expect![[r#"
            402..403: `r` was already consumed (`r` is born here and must be consumed at 371..372) (first consumed here at 388..389)
        "#]],
    );
}

/// Every mention of a consumed binding but a whole assignment is a use
/// after move: a borrow, a raw borrow, a field read, a field write.
#[test]
fn a_place_use_of_a_consumed_linear_is_the_use_after_move_error() {
    check_linear(
        r#"
static borrow = fn() -> usize {
    let r = make(1);
    r.drop();
    let b = r.&;
    b.*.id
};
static raw_borrow = fn() -> () {
    let r = make(1);
    r.drop();
    let p = r.&raw;
};
static field = fn() -> usize {
    let r = make(1);
    r.drop();
    r.id
};
static field_write = fn() -> () {
    let mut r = make(1);
    r.drop();
    r.id = 2;
};
"#,
        expect![[r#"
            415..416: `r` was already consumed (`r` is born here and must be consumed at 376..377) (first consumed here at 393..394)
            514..515: `r` was already consumed (`r` is born here and must be consumed at 475..476) (first consumed here at 492..493)
            595..596: `r` was already consumed (`r` is born here and must be consumed at 564..565) (first consumed here at 581..582)
            680..681: `r` was already consumed (`r` is born here and must be consumed at 649..650) (first consumed here at 666..667)
        "#]],
    );
}

/// A whole assignment gives a consumed binding its value back, so a
/// borrow after it is clean.
#[test]
fn a_consumed_linear_assigned_whole_can_be_borrowed_again() {
    check_linear(
        r#"
static main = fn() -> usize {
    let mut r = make(1);
    r.drop();
    r = make(2);
    let id = r.&.*.id;
    r.drop();
    id
};
"#,
        expect![""],
    );
}

/// A place use of a moved rigid parameter is refused like a by-value one.
#[test]
fn a_place_use_of_a_moved_type_parameter_is_the_use_after_move_error() {
    check_diagnostics(
        r#"
static pair = fn::<@a, T>(v: T, r: T.&::<@a>) -> T { v };
static f = fn::<T: forget>(m: T) -> T {
    let w = m;
    let r = m.&;
    pair(w, r)
};
"#,
        expect![[r#"
            126..127: `m` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use (`m` is born here, and there is only one of it at 86..87) (first consumed here at 111..112)
        "#]],
    );
}

/// The leak is on the path that leaks — the empty `else`, out through the
/// body's end — with the move on the other path as the note.
#[test]
fn a_linear_consumed_in_one_arm_only_leaks_on_the_other_path() {
    check_linear(
        r#"
static main = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { };
};
"#,
        expect![[r#"
            368..425: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 378..379) (consumed here, on another path at 402..403)
        "#]],
    );
}

#[test]
fn both_arms_consuming_is_clean_and_a_diverging_arm_needs_nothing() {
    check_linear(
        r#"
static both = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { r.drop(); };
};
static diverging = fn(c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { panic("no"); };
};
"#,
        expect![""],
    );
}

#[test]
fn a_diverging_tail_owes_nothing_and_a_falling_one_still_does() {
    check_linear(
        r#"
static nope = fn() -> ! { panic("n") };
static panicking = fn() -> usize {
    let r = make(1);
    panic("gone")
};
static returning = fn() -> usize {
    let r = make(1);
    return panic("gone")
};
static nested = fn() -> usize {
    let r = make(1);
    { panic("gone") }
};
static by_name = fn() -> usize {
    let r = make(1);
    nope()
};
static branching = fn(c: bool) -> usize {
    let r = make(1);
    if c { panic("a") } else { panic("b") }
};
static looping = fn() -> usize {
    let r = make(1);
    loop {}
};
static falling = fn() -> usize {
    let r = make(1);
    1
};
"#,
        expect![[r#"
            893..923: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 903..904)
        "#]],
    );
}

#[test]
fn returning_a_linear_consumes_it_and_leaving_one_behind_does_not() {
    check_linear(
        r#"
static handed_back = fn() -> Res {
    let r = make(1);
    r
};
static returned_early = fn(c: bool) -> () {
    let r = make(1);
    if c { return; };
    r.drop();
};
"#,
        expect![[r#"
            477..483: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 453..454)
        "#]],
    );
}

#[test]
fn a_discarded_linear_temporary_is_reported_without_a_name() {
    check_linear(
        r#"
static main = fn() -> () {
    make(1);
};
"#,
        expect![[r#"
            367..374: this value must be consumed; its type has no `forget` capability, so it cannot be discarded
        "#]],
    );
}

#[test]
fn a_return_mid_call_abandons_an_argument_already_made() {
    // `make(1)` is evaluated into a temporary before `return` runs, and
    // the call that would consume it never happens.
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn() -> () {
    take2(make(1), return);
};
"#,
        expect![[r#"
            439..445: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it (this value is born here and must be consumed at 430..437)
        "#]],
    );
}

#[test]
fn a_break_mid_call_abandons_an_argument_already_made() {
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn() -> () {
    loop { take2(make(1), break); };
};
"#,
        expect![[r#"
            446..451: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it (this value is born here and must be consumed at 437..444)
        "#]],
    );
}

#[test]
fn a_continue_mid_call_abandons_an_argument_already_made() {
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn(c: bool) -> () {
    loop {
        take2(make(1), if c { continue } else { 1 });
        break;
    };
};
"#,
        expect![[r#"
            468..476: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it (this value is born here and must be consumed at 452..459)
        "#]],
    );
}

#[test]
fn an_argument_made_on_either_arm_is_abandoned_by_a_later_return() {
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn(c: bool) -> () {
    take2(if c { make(1) } else { make(2) }, return);
};
"#,
        expect![[r#"
            472..478: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it (this value is born here and must be consumed at 459..470)
        "#]],
    );
}

#[test]
fn a_diverging_argument_abandons_nothing() {
    // `panic` ends the program: no frame survives to leak from.
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn() -> () {
    take2(make(1), panic("no"));
};
"#,
        expect![[""]],
    );
}

#[test]
fn a_return_in_a_later_record_field_abandons_an_earlier_one() {
    check_linear(
        r#"
type Pair = struct { r: Res, n: usize } only move;
static main = fn() -> () {
    let Pair(struct { r, n }) = Pair(struct { r = make(1), n = return });
    r.drop();
};
"#,
        expect![[r#"
            477..483: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it (this value is born here and must be consumed at 464..471)
        "#]],
    );
}

#[test]
fn a_temporary_that_reaches_its_consumer_on_every_path_is_quiet() {
    check_linear(
        r#"
static take2 = fn(r: Res, n: usize) -> () { r.drop(); };
static main = fn(c: bool) -> () {
    take2(make(1), if c { 1 } else { 2 });
};
"#,
        expect![[""]],
    );
}

#[test]
fn a_static_cannot_hold_a_linear() {
    check_linear(
        r#"
static held = Res(struct { id = 1, size = 2 });
"#,
        expect![[r#"
            350..382: an item's value must have the `forget` capability: a `static` is never destroyed, so nothing could ever consume this
        "#]],
    );
}

#[test]
fn assigning_over_a_live_linear_loses_it() {
    check_linear(
        r#"
static main = fn() -> () {
    let mut r = make(1);
    r = make(2);
    r.drop();
};
"#,
        expect![[r#"
            392..393: `r` still holds a value that must be consumed; assigning here would lose it (`r` is born here and must be consumed at 375..376)
        "#]],
    );
}

#[test]
fn assigning_over_a_linear_place_with_no_name_loses_it_too() {
    // The same loss where no binding names the place — a field, an
    // element, a `.&mut` referent. Liveness cannot say the old value is
    // already gone (there is nothing to track it on) and the TYPE says one
    // is there, so the write always loses one.
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static overwrite_field = fn::<@a>(h: Holder.&mut::<@a>) -> () { h.*.res = make(2); };
static overwrite_elem = fn::<@a>(a: [Res; 3].&mut::<@a>) -> () { a.*[0] = make(2); };
static overwrite_referent = fn::<@a>(m: Res.&mut::<@a>) -> () { m.* = make(2); };
"#,
        expect![[r#"
            447..454: this place still holds a value that must be consumed; assigning here would lose it
            534..540: this place still holds a value that must be consumed; assigning here would lose it
            619..622: this place still holds a value that must be consumed; assigning here would lose it
        "#]],
    );
    // The raw hatch is the one way past it, where it always was: writing
    // through a raw pointer is `unsafe`, and what it does to an obligation
    // is the caller's word.
    check_linear(
        r#"
static overwrite_raw = fn(p: Res.&raw mut) -> () { unsafe { p.* = make(2); }; };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_const_block_may_not_produce_a_linear() {
    // A `const { ... }` is a CONSTANT — computed once and copied into
    // every evaluation — so a linear one is one obligation per evaluation
    // and no path discharges it once. `ItemHoldsLinear`'s reason, one
    // nesting level down, and not covered by the leaked-INSIDE-the-block
    // case: here the block itself hands the value out.
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static main = fn() -> () {
    let r = const { const_make(1) };
    r.drop();
};
"#,
        expect![[r#"
            456..479: a `const` block's value must have the `forget` capability: it is computed once and copied into every evaluation, so no single path could consume it
        "#]],
    );
}

#[test]
fn a_diverging_const_block_answers_for_the_type_it_was_pinned_to() {
    // The block's TYPE decides this, not a path through it — the same
    // question the item-level twin asks. A diverging body is no excuse:
    // pinned by an annotation the block still promises a `Res` per
    // evaluation, while unpinned its own type is `!`, which anyone may
    // forget. The binding is not reported either way: nothing reaches it.
    check_linear(
        r#"
static main = fn() -> usize {
    let pinned: Res = const { panic("x") };
    1
};
static free = fn() -> usize {
    let loose = const { panic("y") };
    1
};
"#,
        expect![[r#"
            388..408: a `const` block's value must have the `forget` capability: it is computed once and copied into every evaluation, so no single path could consume it
        "#]],
    );
}

#[test]
fn a_static_holding_a_const_block_says_it_once() {
    // Both rules fire on the same range here — the block's and the item's,
    // which is the block's one level up. One mistake gets one sentence, and
    // it is the item's: a `static` is never destroyed, which is the reason
    // a reader needs.
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static held = const { const_make(1) };
"#,
        expect![[r#"
            431..454: an item's value must have the `forget` capability: a `static` is never destroyed, so nothing could ever consume this
        "#]],
    );
}

#[test]
fn a_borrow_of_a_linear_consumes_nothing_and_a_copy_out_is_refused() {
    check_linear(
        r#"
static peek = fn::<@a>(r: Res.&::<@a>) -> usize { r.*.id };
static main = fn() -> () {
    let r = make(1);
    let n = peek(r.&);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn containment_infects_a_container_and_a_rest_pattern_cannot_skip_it() {
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static take = fn(h: Holder) -> () {
    let Holder(struct { res, tag }) = h;
    res.drop();
};
static skipped = fn(h: Holder) -> () {
    let Holder(struct { tag, .. }) = h;
};
"#,
        expect![[r#"
            533..551: `..` would skip `res`, which must be consumed; name it in the pattern so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_field_read_of_a_linear_field_is_a_copy_and_is_refused() {
    check_linear(
        r#"
type Holder = struct { res: Res, tag: usize };
static main = fn(h: Holder) -> () {
    let r = h.res;
    r.drop();
};
"#,
        expect![[r#"
            431..436: cannot copy a value that must be consumed out of a place; take the whole value apart instead (`let Name(struct { .. }) = value;`)
        "#]],
    );
}

#[test]
fn matching_an_owned_linear_enum_consumes_it_through_its_payloads() {
    check_linear(
        r#"
type Maybe = enum { One(Res), Nothing };
static main = fn(m: Maybe) -> () {
    match m {
        ::One(r) => { r.drop(); },
        ::Nothing => { },
    };
};
static leaky = fn(m: Maybe) -> () {
    match m {
        ::One(r) => { },
        ::Nothing => { },
    };
};
"#,
        expect![[r#"
            567..570: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 561..562)
        "#]],
    );
}

#[test]
fn a_loop_may_not_consume_a_linear_born_outside_it() {
    check_linear(
        r#"
static main = fn(n: usize) -> () {
    let r = make(1);
    loop {
        r.drop();
    };
};
"#,
        expect![[r#"
            411..412: `r` was already consumed (`r` is born here and must be consumed at 379..380) (consumed here, on an earlier iteration of the loop at 411..412)
        "#]],
    );
}

#[test]
fn a_loop_that_replaces_what_it_consumed_is_clean() {
    check_linear(
        r#"
static main = fn(n: usize) -> () {
    let mut r = make(1);
    loop {
        r.drop();
        r = make(2);
        if n == 0 { break; };
    };
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn a_linear_born_and_consumed_inside_a_loop_is_clean_and_a_break_checks_it() {
    check_linear(
        r#"
static clean = fn(n: usize) -> () {
    loop {
        let r = make(1);
        r.drop();
        if n == 0 { break; };
    };
};
static broken = fn(n: usize) -> () {
    loop {
        let r = make(1);
        if n == 0 { break; };
        r.drop();
    };
};
"#,
        expect![[r#"
            559..564: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 526..527)
        "#]],
    );
}

#[test]
fn a_linear_cannot_be_repeated_into_an_array() {
    check_linear(
        r#"
static main = fn() -> () {
    let r = make(1);
    let a = [r; 3];
};
"#,
        expect![[r#"
            397..398: cannot repeat a value that must be consumed: the copies would each have to be consumed, and there is only one value
        "#]],
    );
}

#[test]
fn a_linear_born_in_a_const_context_must_be_consumed_there() {
    check_linear(
        r#"
static const_make = const fn(id: usize) -> Res { Res(struct { id, size = 1 }) };
static n: usize = const { let r = const_make(1); 5 };
"#,
        expect![[r#"
            441..469: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 447..448)
        "#]],
    );
}

#[test]
fn a_container_holds_a_linear_with_nothing_written_on_its_binder() {
    // The ruling, at its smallest (T22): `Box` says nothing about `T`, and
    // `Box::<Res>` is linear anyway — CONTAINMENT decided, which is what
    // it always did. The one diagnostic left is the leak the program
    // really has; the refusal that used to sit beside it was bookkeeping
    // about a bound nobody needed.
    check_linear(
        r#"
type Box = enum::<T> { Full(T), Empty };
static main = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
};
"#,
        expect![[r#"
            402..452: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 433..434)
        "#]],
    );
}

#[test]
fn a_container_parameter_carries_a_linear_and_hands_it_back() {
    // `unwrap`'s shape is the reason rigid checking is bearable: inside the
    // body the only things you can do with a `T` you may not forget are
    // hand it back and pass it on — and handing it back is what a container
    // is for.
    check_linear(
        r#"
type Box = enum::<T> { Full(T), Empty } with {
    impl Self {
        unwrap = fn(b: Self) -> T {
            match b {
                ::Full(t) => t,
                ::Empty => panic("empty box"),
            }
        };
    }
};
static clean = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
    let back = b.unwrap();
    back.drop();
};
static leaky = fn() -> () {
    let r = make(1);
    let b = Box::Full(r);
};
static forgettable = fn() -> usize {
    let b = Box::<usize>::Full(5);
    b.unwrap()
};
"#,
        expect![[r#"
            718..768: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 749..750)
        "#]],
    );
}

#[test]
fn an_unbounded_parameter_is_checked_rigidly_inside_the_generic_body() {
    // The caller may hand it a linear, so the body may not assume it can
    // drop one on the floor — even though `T` might be `usize`. The
    // message names the binder, because that is where the fix goes: this
    // body discarded a `T`, and nothing about `T` said it could.
    // `hand_back` needs nothing: passing a value on is not discarding it.
    check_linear(
        r#"
static ignore = fn::<T>(t: T) -> () { };
static hand_back = fn::<T>(t: T) -> T { t };
"#,
        expect![[r#"
            372..375: `t` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`t` is born here and must be consumed at 360..361)
        "#]],
    );
}

#[test]
fn the_forget_bound_lets_a_generic_body_discard_its_parameter() {
    // The body-side dual, written: `T: forget` is what a body says when it
    // genuinely drops a `T` on the floor. Its price is at the call — the
    // instantiation must supply a type that can be dropped, so the linear
    // one is refused there instead of leaking here.
    check_linear(
        r#"
static ignore = fn::<T: forget>(t: T) -> () { };
static fine = fn() -> () { ignore::<usize>(5) };
static refused = fn() -> () { ignore::<Res>(make(1)) };
"#,
        expect![[r#"
            464..477: `Res` cannot be a `T`: `Res` is declared `only move`, and `T: forget` asks for a type whose values may be dropped on the floor
        "#]],
    );
}

#[test]
fn a_trait_named_after_a_capability_is_declarable_and_unreachable() {
    // Capability names are not reserved words, so this declaration is
    // fine — but a bound naming one asks the LANGUAGE for the capability
    // and never performs a scope lookup, so nothing can reach this trait
    // through `T: forget`. A warning, because the declaration is legal and
    // only its reachability is surprising.
    check_diagnostics(
        r#"
trait forget = requires { m: fn(Self) -> usize; };
static f = fn::<T: forget>(t: T) -> usize { 1 };
"#,
        expect![[r#"
            7..13: `forget` is the name of a capability, so `T: forget` asks for the capability and never for this trait; nothing can reach it through a bound
        "#]],
    );
}

#[test]
fn the_forget_bound_is_not_permission_to_copy() {
    // The soundness line the bound must not cross, in both of the places
    // it could have been crossed (TR11). `forget` answers what may be
    // LOST; duplication asks what may be COPIED, and the language has its
    // own witness that the two come apart: `T.&mut` HAS `forget` (losing a
    // borrow loses no obligation) and is refused copying anyway. So a
    // bounded `T` at `usize.&mut` would hand out two exclusive borrows of
    // one place — through `r.*` in the first body, and through a second
    // READ of one binding in the second.
    //
    // Both are refused: every rigid parameter is non-copyable, and every
    // one is tracked by the must-consume check, bound or not. What the
    // bound buys is the LAST body — the obligation to consume before the
    // scope ends is waived, which is the whole of what it was asked for.
    // Strict-first: if a capability is ever ruled to grant duplication,
    // granting it is the relaxation.
    //
    // `in_enum` is the same question one CONTAINMENT level down, and it is
    // pinned because the answer used to depend on which declaration form
    // held the parameter: the copy walk reads a mention's components, and
    // an enum's are its variant payloads exactly as a struct's are its
    // fields.
    check_diagnostics(
        r#"
type Opt = enum::<T> { S(T), N };
static deref = fn::<@a, T: forget>(r: T.&::<@a>) -> T { r.* };
static in_enum = fn::<@a, T: forget>(o: Opt::<T>.&::<@a>) -> Opt::<T> { o.* };
static pair = fn::<T: forget>(x: T) -> struct { a: T, b: T } { struct { a = x, b = x } };
static lend = fn::<@a, T: forget>(r: T.&::<@a>) -> T.&::<@a> { r };
static discards = fn::<T: forget>(x: T) -> usize { 1 };
"#,
        expect![[r#"
            91..94: cannot move out of a borrow: `T` cannot be copied
            170..173: cannot move out of a borrow: `Opt::<T>` cannot be copied
            260..261: `x` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use (`x` is born here, and there is only one of it at 207..208) (first consumed here at 253..254)
        "#]],
    );
}

#[test]
fn lending_a_payload_out_of_a_container_works_and_reading_one_out_does_not() {
    // The lending shape a generic container needs, pinned as a fact
    // rather than a memory: `b.*.v.&` takes a borrow of a field through a
    // borrow of the whole, COPIES NOTHING, and is accepted — for an
    // unbounded `T` and for a concrete linear payload alike, so nothing
    // about it is a capability-bound question.
    //
    // The controls are the two shapes that genuinely duplicate: reading
    // `b.*.v` out through a borrow is a copy, and so is reading it out of
    // an owned box. That is what makes a container's `replace`/`take`
    // unwritable today (TR11's Re-evaluate) — not lending.
    check_diagnostics(
        r#"
type Box = struct::<T> { v: T } with {
    impl Self {
        lend = fn::<@a>(b: Self.&::<@a>) -> T.&::<@a> { b.*.v.& };
        read = fn::<@a>(b: Self.&::<@a>) -> T { b.*.v };
    }
};
type Lin = struct { n: usize } only move with {
    impl Self { drop = fn(l: Self) -> () { let Lin(struct { n }) = l; }; }
};
type LinBox = struct { v: Lin } with {
    impl Self {
        lend = fn::<@a>(b: Self.&::<@a>) -> Lin.&::<@a> { b.*.v.& };
        read = fn::<@a>(b: Self.&::<@a>) -> Lin { b.*.v };
    }
};
"#,
        expect![[r#"
            171..176: cannot move out of a borrow: `T` cannot be copied
            489..494: cannot move out of a borrow: `Lin` cannot be copied
        "#]],
    );
}

#[test]
fn the_rest_of_the_linear_family_names_the_binder_too() {
    // The families that used to compute a hint and drop it. Two are
    // LEAK-shaped (an assignment over a live value, a value consumed on one
    // branch only) and may offer the bound, because they fire only where an
    // unbounded parameter is why the value had to be consumed. Two are
    // DUPLICATION-shaped (a repeat, a loop that consumes what it found)
    // and must not: a bounded parameter reaches them, and for the repeat
    // below consumption has already been waived — "a value that must be
    // consumed" would be a false sentence about it.
    check_diagnostics(
        r#"
static keep = fn::<U>(u: U) -> U { u };
static repeated = fn::<T: forget>(t: T) -> usize { let a = [t; 3]; 1 };
static assigned = fn::<T>(mut t: T, u: T) -> T { t = u; t };
static joined = fn::<T>(t: T, c: bool) -> usize { if c { let x = keep::<T>(t); 0 } else { 0 } };
static looped = fn::<T: forget>(t: T) -> usize { loop { let x = t; }; };
"#,
        expect![[r#"
            101..102: cannot repeat a value of `T`: a value of `T` may not be duplicated, and no bound grants copying — there is only one value
            162..163: `t` still holds a value that must be consumed, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`t` is born here and must be consumed at 143..144)
            229..256: `x` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`x` is born here and must be consumed at 235..236)
            222..269: `t` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`t` is born here and must be consumed at 198..199) (consumed here, on another path at 249..250)
            335..336: `t` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use (`t` is born here, and there is only one of it at 303..304) (consumed here, on an earlier iteration of the loop at 335..336)
        "#]],
    );
}

#[test]
fn a_place_written_over_inside_a_generic_body_names_the_binder() {
    // The one finding with no BINDING to name: a place written over (a
    // field, an element, a `.&mut` referent). LEAK family — the old value
    // goes nowhere — so it may offer the bound, and it names the binder
    // because there is no declaration in sight to point at.
    //
    // The control is the same shape with `T: forget`: a bounded `T` is
    // forgettable, so nothing is lost and nothing fires.
    check_diagnostics(
        r#"
type Box = struct::<T> { v: T };
static over_field = fn::<T>(mut b: Box::<T>, t: T) -> usize { b.v = t; 1 };
static over_field_bounded = fn::<T: forget>(mut b: Box::<T>, t: T) -> usize { b.v = t; 1 };
"#,
        expect![[r#"
            96..99: this place still holds a value that must be consumed, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded
        "#]],
    );
}

#[test]
fn the_pattern_shaped_refusals_name_the_binder_too() {
    // The two refusals that land on a PATTERN rather than on a binding or
    // an expression: a `..` that skips a field, and a match arm `_` that
    // swallows the scrutinee. Both are LEAK-shaped — the value was
    // produced and given nowhere to go — so both offer the bound, and both
    // name the binder for the reason every other twin does.
    //
    // Neither type is a binding's or an expression's, which is why the
    // walk carries the blame instead of the render site looking one up:
    // the `..` answers for the FIELD it skipped, not for the record that
    // holds it, and the second declaration is the control that proves it —
    // `Two`'s skipped field is the concrete `Res`, so its refusal names no
    // binder even though a `T` sits right beside it in the same pattern.
    check_linear(
        r#"
type Box = struct::<T> { v: T, n: usize };
type Two = struct::<T> { r: Res, t: T };
type Opt = enum::<T> { S(T), N };
static skips = fn::<T>(b: Box::<T>) -> usize { let Box(struct { n, .. }) = b; n };
static skips_decl = fn::<T>(x: Two::<T>) -> T { let Two(struct { t, .. }) = x; t };
static swallows = fn::<T>(o: Opt::<T>) -> usize { match o { _ => 1, } };
"#,
        expect![[r#"
            509..525: `..` would skip `v`, and `T` may be a type that must be consumed: name it in the pattern so it has somewhere to go, or write `T: forget` to require one that can be discarded
            593..609: `..` would skip `r`, which must be consumed; name it in the pattern so it has somewhere to go
            681..682: `_` matches the value without binding it, and `T` may be a type that must be consumed: give it a name so it has somewhere to go, or write `T: forget` to require one that can be discarded
        "#]],
    );
}

#[test]
fn a_const_block_typed_as_a_parameter_names_the_binder() {
    // A const context cannot READ a runtime binding, but it can be typed
    // by one: `let x: T = const { ... }` takes its expected type from the
    // annotation while a diverging body supplies the value. So the
    // const-block refusal does reach a generic body, and it names the
    // binder like its neighbours.
    //
    // Its hint is the one leak hint that does not offer consumption: the
    // block's value is computed once and copied into every evaluation, so
    // there is no path to consume it on and the bound is all there is.
    // The bounded body is the control — a forgettable `T` needs no path.
    //
    // `x` itself is not reported: the block diverges, so nothing reaches
    // the binding. The block still answers for the type it was pinned to.
    check_diagnostics(
        r#"
static held = fn::<T>(t: T) -> T { let x: T = const { panic("x") }; t };
static held_bounded = fn::<T: forget>(t: T) -> T { let x: T = const { panic("x") }; t };
"#,
        expect![[r#"
            47..67: a `const` block's value must have the `forget` capability, and `T` may be a type that must be consumed — no path here could consume it, so write `T: forget` to require one that can be discarded
        "#]],
    );
}

#[test]
fn a_self_referential_declaration_does_not_hang_the_copy_walk() {
    // The copy walk descends into a mention's components, so a
    // declaration that mentions itself is a cycle. It contributes
    // nothing — a declaration reached twice adds no component the walk has
    // not judged — and the non-recursive parts decide, which for both of
    // these is "copyable".
    check_diagnostics(
        r#"
type S = struct { next: S };
type L = enum { C(L), N };
static s = fn::<@a>(r: S.&::<@a>) -> S { r.* };
static l = fn::<@a>(r: L.&::<@a>) -> L { r.* };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn nesting_a_declaration_inside_itself_is_not_a_cycle() {
    // A declaration is summarized ONCE — whether its own components cost it
    // the capability, and which of its parameters reach a value position of
    // it — and an instance is judged from that summary and its arguments.
    // So `B::<B::<X>>` names one declaration twice and neither mention is a
    // cycle: `B`'s own component is its own `T`, which holds nothing by
    // itself, and the answer comes from the argument, one level at a time,
    // however many levels there are. A walk that read the repeated
    // declaration as a cycle and answered "found nothing" would copy the
    // exclusive borrow one level down out of a shared borrow, and leak the
    // payload one level down.
    check_diagnostics(
        r#"
type B = struct::<T> { v: T };
type Opt = enum::<T> { S(T), N };
static nested = fn::<@a, @b>(r: B::<B::<usize.&mut::<@b>>>.&::<@a>) -> B::<B::<usize.&mut::<@b>>> { r.* };
static flat = fn::<@a, @b>(r: B::<usize.&mut::<@b>>.&::<@a>) -> B::<usize.&mut::<@b>> { r.* };
static leaks = fn::<T>(x: Opt::<Opt::<T>>) -> usize { 1 };
"#,
        expect![[r#"
            166..169: cannot move out of a borrow: `B::<B::<usize.&mut::<@b>>>` cannot be copied
            261..264: cannot move out of a borrow: `B::<usize.&mut::<@b>>` cannot be copied
            320..325: `x` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`x` is born here and must be consumed at 291..292)
        "#]],
    );
}

#[test]
fn a_declaration_that_names_itself_is_summarized_not_expanded() {
    // The summary is a property of the DECLARATION graph, which is finite,
    // so a declaration that mentions itself with LARGER arguments is
    // answered rather than bounded: nothing limits how deep or how large a
    // type the check will read, and no inhabited type is refused for want of
    // such a limit.
    //
    // `Nest` is the classic nested datatype. Its `T` reaches a value
    // position (a `::Cons` holds one), so `Nest::<usize>` may be forgotten
    // and copied and `Nest::<Res>` may not — and the `Nest::<Pair::<T>>`
    // beside it never has to be expanded to say either.
    //
    // `P` and `Pe` are the same shape with nothing reaching a value
    // position: every value position of a `P` is another `P`, so no `T` is
    // ever held. No finite value inhabits `P` at all; `Pe` has a base
    // variant, so `Pe::<Res>` IS inhabited — by `::N`, and by `::S` of a
    // `Pe` of something bigger — and not one of its inhabitants holds a
    // `Res`. Both are clean, which is the exact answer, and it is the least
    // fixpoint that gives it: assume a parameter flows and it flows
    // forever, taking every argument any `Pe` was ever given with it.
    //
    // `L` and `M` swap their parameters through each other, which is why
    // the fixpoint is over the whole graph rather than one declaration at a
    // time: `L::<usize, Res>` holds the `Res`, `L::<Res, usize>` never does.
    check_diagnostics(
        r#"
type Res = struct { id: usize } only move with {
    impl Self { drop = fn(r: Self) -> () { let Res(struct { id }) = r; }; };
};
type W = struct::<T> { v: T };
type Pair = struct::<T> { l: T, r: T };
type Nest = enum::<T> { Cons(T, Nest::<Pair::<T>>), Nil };
type P = struct::<T> { v: P::<W::<T>> };
type Pe = enum::<T> { S(Pe::<W::<T>>), N };
type L = enum::<S, T> { C(T, M::<T, S>), N };
type M = enum::<S, T> { C(S, L::<T, S>), N };
static holds = fn(n: Nest::<usize>) -> usize { 1 };
static copies = fn::<@a>(r: Nest::<usize>.&::<@a>) -> Nest::<usize> { r.* };
static leaks = fn(n: Nest::<Res>) -> usize { 1 };
static empty = fn(p: P::<usize>) -> usize { 1 };
static copies_empty = fn::<@a>(r: P::<usize>.&::<@a>) -> P::<usize> { r.* };
static based = fn(p: Pe::<Res>) -> usize { 1 };
static swapped = fn(x: L::<usize, Res>) -> usize { 1 };
static unswapped = fn(x: L::<Res, usize>) -> usize { 1 };
"#,
        expect![[r#"
            609..614: `n` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`n` is born here and must be consumed at 584..585)
            839..844: `x` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`x` is born here and must be consumed at 810..811)
        "#]],
    );
}

#[test]
fn ordinary_nested_generics_are_judged_from_their_arguments() {
    // Nesting one generic inside another is the everyday case and costs
    // nothing: `Row` holds an `Opt::<Pair::<usize, usize>>`, so an
    // `Opt::<Row>` holds no obligation and may be both dropped and copied
    // out of a shared borrow. The same shape with a linear leaf anywhere in
    // it is refused — containment does not care how many mentions it
    // travelled through — and so is a mention of a declaration whose OWN
    // component is linear whatever its arguments are (`Row2`), which is the
    // half of a summary no argument can change. The message walks back out
    // through both halves: the declaration's own containment steps, and the
    // parameter each argument was given for.
    check_diagnostics(
        r#"
type Res = struct { id: usize } only move with {
    impl Self { drop = fn(r: Self) -> () { let Res(struct { id }) = r; }; };
};
type Opt = enum::<T> { S(T), N };
type Pair = struct::<A, B> { a: A, b: B };
type Row = struct { cell: Opt::<Pair::<usize, usize>> };
type Row2 = struct { cell: Opt::<Res> };
static ignore = fn::<T: forget>(t: T) -> () { };
static plain = fn(x: Opt::<Row>) -> usize { 1 };
static copies = fn::<@a>(r: Opt::<Row>.&::<@a>) -> Opt::<Row> { r.* };
static deep = fn(x: Opt::<Pair::<usize, Res>>) -> usize { 1 };
static owned = fn(x: Opt::<Row2>) -> usize { 1 };
static asks = fn(x: Opt::<Row2>) -> () { ignore(x) };
"#,
        expect![[r#"
            628..634: `Opt::<Row2>` cannot be a `T`: `Res` is declared `only move`, reached through `Opt`'s `T`, through field `cell`, through `Opt`'s `T`, and `T: forget` asks for a type whose values may be dropped on the floor
            530..535: `x` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`x` is born here and must be consumed at 491..492)
            580..585: `x` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`x` is born here and must be consumed at 555..556)
        "#]],
    );
}

#[test]
fn a_deep_chain_of_wrappers_is_judged_at_the_bottom() {
    // Containment infects however far it has to travel. Eighty wrappers
    // are an ordinary type, a leak eighty levels down is an ordinary leak,
    // and nothing in a program would say which side of a cutoff it sat on
    // — so there is no depth at which the walk stops looking and answers
    // that it found nothing.
    //
    // `D30` is the other half of the same walk: each level holds the
    // level below it twice, so a query that forgot which declarations it
    // had already looked inside would walk `D0` 2^30 times — once for the
    // disposal answer and again for the blame that names `T`. It answers
    // instantly, or this test hangs.
    let mut src = String::from("type L0 = struct::<T> { v: T };\n");
    for level in 1..=80 {
        src += &format!(
            "type L{level} = struct::<T> {{ v: L{}::<T> }};\n",
            level - 1
        );
    }
    src += "type D0 = struct::<T> { v: T };\n";
    for level in 1..=30 {
        src += &format!(
            "type D{level} = struct::<T> {{ a: D{below}::<T>, b: D{below}::<T> }};\n",
            below = level - 1
        );
    }
    src += "static leaks = fn::<T>(x: L80::<T>) -> usize { 1 };\n";
    src += "static copies = fn::<@a, @b>(r: L80::<usize.&mut::<@b>>.&::<@a>) \
            -> L80::<usize.&mut::<@b>> { r.* };\n";
    src += "static doubled = fn(x: D30::<usize>) -> usize { 1 };\n";
    src += "static doubled_leaks = fn::<T>(x: D30::<T>) -> usize { 1 };\n";
    let messages = diagnostic_messages(&src);
    assert_eq!(messages.len(), 3, "{messages:#?}");
    assert!(
        messages[0].starts_with("cannot move out of a borrow"),
        "{messages:#?}"
    );
    assert!(
        messages[1].starts_with("`x` is not consumed on this path"),
        "{messages:#?}"
    );
    assert!(
        messages[2].starts_with("`x` is not consumed on this path"),
        "{messages:#?}"
    );
}

#[test]
fn a_value_holding_an_exclusive_borrow_is_read_once() {
    // The instantiation TR11 names, written out concretely. A bounded `T`
    // may be `usize.&mut`, so the check's second obligation has to hold for
    // the borrow itself — otherwise the generic body is refused and the
    // program it stands for is not, which is the wrong way round.
    //
    // Both would run: two handles on one loan, interleaved writes, and
    // nothing in the aliasing tree to see it, because each read is one
    // `Move` of the same pointer. The refusal is the check's, and its root
    // is neither a declaration nor a binder.
    //
    // A BARE borrow is the control, and M07 is why: a mention in a
    // borrow-wanting position reborrows, and one in any other position
    // copies invisibly — a typing question that ruling leaves open, not
    // this checker's to answer.
    check_diagnostics(
        r#"
type W = struct::<T> { m: T };
type Opt = enum::<T> { S(T), N };
static held = fn::<@a>(w: W::<usize.&mut::<@a>>) -> usize { let a = w; let b = w; 1 };
static wrapped = fn::<@a>(o: Opt::<usize.&mut::<@a>>) -> usize { let a = o; let b = o; 1 };
static bare = fn::<@a>(q: usize.&mut::<@a>) -> usize { let x = q; let y = q; 0 };
"#,
        expect![[r#"
            145..146: `w` was already consumed: an exclusive borrow may not be duplicated: the two would name one place — borrow it at the first use (`w` is born here, and there is only one of it at 89..90) (first consumed here at 134..135)
            237..238: `o` was already consumed: an exclusive borrow may not be duplicated: the two would name one place — borrow it at the first use (`o` is born here, and there is only one of it at 179..180) (first consumed here at 226..227)
        "#]],
    );
}

#[test]
fn a_capability_bound_is_part_of_a_requirements_contract() {
    // The one place a capability bound crosses into the trait system. It
    // takes no dictionary slot — it resolves to no trait — but it is part
    // of what the requirement promised its callers, so an impl that asks
    // `W: forget` where the requirement did not demands more of every
    // caller than the requirement did.
    //
    // The OTHER direction is refused too, and not because the impl is
    // stricter (it is the reverse). Binder matching here is an EQUALITY,
    // deliberately: there is no variance story for binders, and the
    // neighbours compare the same way — trait bounds as an exact set,
    // `outlives` positionally. Strict-first, so loosening stays additive.
    check_diagnostics(
        r#"
trait Adds = requires { m: fn::<W>(W, Self) -> W; } with { impl usize { m = fn::<W: forget>(w: W, x: usize) -> W { w }; } };
trait Omits = requires { m: fn::<W: forget>(W, Self) -> W; } with { impl usize { m = fn::<W>(w: W, x: usize) -> W { w }; } };
trait Agrees = requires { m: fn::<W: forget>(W, Self) -> W; } with { impl usize { m = fn::<W: forget>(w: W, x: usize) -> W { w }; } };
"#,
        expect![[r#"
            73..74: member `m`'s generic binder does not match `Adds`'s requirement (arity, kinds and bounds must agree) (required by the trait here at 25..26)
            207..208: member `m`'s generic binder does not match `Omits`'s requirement (arity, kinds and bounds must agree) (required by the trait here at 151..152)
        "#]],
    );
}

#[test]
fn only_forget_can_be_required_of_a_parameter() {
    // The bound side of the capability vocabulary, read off the one table
    // rather than a second enumeration. `forget` is the only capability a
    // body can require of its caller, and the three ways of not being it
    // are three different facts. A LIVE capability that is not a bound
    // (`move`) is refused for what it would mean — nothing, since every
    // value can be moved; a RESERVED name is refused as reserved, so a
    // reservation reads as a reservation and not as a typo; a name that is
    // no capability at all stays an ordinary trait bound and gets the
    // ordinary unresolved answer.
    check_diagnostics(
        r#"
static ok = fn::<T: forget>(t: T) -> usize { 1 };
static live = fn::<T: move>(t: T) -> T { t };
static reserved = fn::<T: send>(t: T) -> T { t };
static ordinary = fn::<T: Display>(t: T) -> T { t };
"#,
        expect![[r#"
            73..77: every value can be moved, so `move` is not worth requiring; an unbounded parameter can already be moved, returned and passed on
            123..127: the `send` capability does not exist yet; `forget` is the only one a body can require
            173..180: unknown trait `Display`
        "#]],
    );
}

#[test]
fn a_value_linear_for_two_reasons_is_not_offered_the_binder_fix() {
    // The advice has to survive being TAKEN. `Mixed` is linear twice over
    // — a field whose declaration shed the capability, and a payload whose
    // binder promised nothing — and only one of those is fixed by writing
    // `T: forget`. So the hint is the blame walk's OWN root rather than a
    // second search with its own preference order: name the parameter when
    // the parameter is the reason, and the declaration otherwise.
    //
    // The second body is the control: one reason, and it is the binder.
    check_linear(
        r#"
type Mixed = struct::<T> { a: Res, b: T };
type Box = struct::<T> { v: T };
static two_reasons = fn::<T>(r: Res, t: T) -> usize {
    let m = Mixed::<T>(struct { a = r, b = t });
    1
};
static one_reason = fn::<T>(t: T) -> usize {
    let b = Box::<T>(struct { v = t });
    1
};
"#,
        expect![[r#"
            464..522: `m` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`m` is born here and must be consumed at 474..475)
            567..616: `b` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`b` is born here and must be consumed at 577..578)
        "#]],
    );
}

#[test]
fn every_linear_refusal_in_a_generic_body_names_the_binder() {
    // A generic body has no declaration for a reader to go look at, so
    // "its type has no `forget` capability" is both unhelpful and false —
    // nothing declared anything. All four families name the BINDER
    // instead, including the one that reaches it through containment
    // (`Box::<T>` is linear only because its payload might be).
    //
    // The two that are about discarding name `T: forget` as the way out.
    // The two that are about duplicating name BORROWING, because the bound
    // does not grant copying (see `the_forget_bound_is_not_permission_to_copy`).
    check_diagnostics(
        r#"
type Box = struct::<T> { v: T };
static leaks = fn::<T>(x: T) -> usize { let b = Box::<T>(struct { v = x }); 1 };
static twice = fn::<T>(x: T) -> T { let y: T = x; x };
static copies_out = fn::<T>(b: Box::<T>) -> T { b.v };
static discards = fn::<T>(t: T) -> usize { t; 1 };
"#,
        expect![[r#"
            218..221: cannot copy a value of `T` out of a place: a value of `T` may not be duplicated, and no bound grants copying — borrow the place, or move the whole value
            268..269: this value is discarded here, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded
            72..113: `b` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`b` is born here and must be consumed at 78..79)
            165..166: `x` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use (`x` is born here and must be consumed at 138..139) (first consumed here at 162..163)
            149..168: `y` is not consumed on this path, and `T` may be a type that must be consumed; consume it, or write `T: forget` to require one that can be discarded (`y` is born here and must be consumed at 155..156)
        "#]],
    );
}

#[test]
fn a_match_arm_wildcard_cannot_swallow_an_owned_linear() {
    // The match CONSUMED the scrutinee; `_` bound nothing to consume it
    // with. Its neighbours (`let _ = s`, `::Some(_)`) were caught from the
    // start — this is the same sentence at the third position, and it
    // composes: the nested `match x { _ => {} }` is the same mistake one
    // level down.
    check_linear(
        r#"
type Maybe = enum::<T> { One(T), Nothing };
static direct = fn() -> () {
    let r = make(1);
    match r {
        _ => { },
    };
};
static catch_all = fn(m: Maybe::<Res>) -> () {
    match m {
        _ => { },
    };
};
static nested = fn(m: Maybe::<Res>) -> () {
    match m {
        ::One(x) => {
            match x {
                _ => { },
            };
        },
        ::Nothing => { },
    };
};
"#,
        expect![[r#"
            452..453: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
            541..542: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
            679..680: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_match_arm_wildcard_through_a_borrow_owes_nothing() {
    // Nothing was consumed, so nothing is owed — the scrutinee decides,
    // exactly as match projection already rules.
    check_linear(
        r#"
static peek = fn::<@a>(r: Res.&::<@a>) -> usize {
    match r {
        _ => { 0 },
    }
};
static main = fn() -> () {
    let r = make(1);
    let n = peek(r.&);
    r.drop();
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_break_carries_its_state_out_and_only_the_back_edge_runs_the_body_again() {
    // A break is not a back edge: it LEAVES. So the take-ownership-and-stop
    // search loop — the shape every search is written in — checks clean,
    // while falling off the body's end runs it again, and the next
    // iteration consumes what this one already did.
    check_linear(
        r#"
static takes_and_stops = fn(n: usize) -> () {
    let r = make(1);
    let mut i = 0;
    loop {
        i = i + 1;
        if i > n {
            r.drop();
            break;
        };
    };
};
static nested_break = fn(n: usize) -> () {
    let r = make(1);
    let mut i = 0;
    loop {
        loop {
            i = i + 1;
            if i > n { break; };
        };
        r.drop();
        break;
    };
};
static falls_off_the_end = fn() -> () {
    let r = make(1);
    loop {
        r.drop();
    };
};
"#,
        expect![[r#"
            832..833: `r` was already consumed (`r` is born here and must be consumed at 800..801) (consumed here, on an earlier iteration of the loop at 832..833)
        "#]],
    );
}

#[test]
fn a_broken_consumption_site_reports_no_leak_beside_it() {
    // Half-typed (`s.`) and unresolvable (`s.nope()`) consumptions are
    // consumptions in progress. Reporting a leak beside the real error
    // would accuse the writer of the opposite of what they are doing.
    check_linear(
        r#"
static half_typed = fn() -> () {
    let r = make(1);
    r.
};
static unresolvable = fn() -> () {
    let r = make(1);
    r.nope();
};
"#,
        expect![[r#"
            397..398: expected a field name after `.`
            460..468: no field or member `nope` on `Res`
        "#]],
    );
}

#[test]
fn a_broken_site_on_one_branch_leaves_the_other_branch_leaking() {
    // A path through the refused `t;` never runs on; the path that skips
    // it still leaks `t` at the scope's end.
    check_linear(
        r#"
static main = fn(c: bool) -> () {
    let t = make(1);
    if c { t; } else { };
};
"#,
        expect![[r#"
            402..403: this value must be consumed; its type has no `forget` capability, so it cannot be discarded
            368..418: `t` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`t` is born here and must be consumed at 378..379)
        "#]],
    );
}

#[test]
fn a_value_that_may_come_from_a_trap_still_leaks_on_the_path_that_did_not() {
    // A non-exhaustive match traps in its `otherwise` arm into the same
    // result the covered arm writes: `r` is a real value on that arm.
    check_linear(
        r#"
type Shape = enum { Circle(usize), Point };
static main = fn(s: Shape) -> () {
    let r = match s { ::Circle(n) => make(n) };
};
"#,
        expect![[r#"
            427..432: this `match` does not cover `Shape::Point`
            413..464: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 423..424)
        "#]],
    );
}

#[test]
fn a_copy_out_of_a_borrow_is_diagnosed_once() {
    // `r.*[0]` of a linear element is BOTH "cannot move out of a borrow"
    // and "cannot copy a value that must be consumed". One mistake, one
    // message — the borrow-side one, which names the place.
    check_linear(
        r#"
static elem = fn::<@a>(r: [Res; 3].&::<@a>) -> Res { r.*[0] };
"#,
        expect![[r#"
            389..395: cannot move out of a borrow: `Res` cannot be copied
        "#]],
    );
}

#[test]
fn the_heap_builtins_take_a_linear_element_type() {
    // `alloc_array::<T>` hands out a POINTER to storage and never holds a
    // `T`, so its binder asks for nothing — and `AllocResult::<T>`, whose
    // payload is that pointer, is forgettable for the same reason. The two
    // have to agree: accepting the result type while refusing the call
    // that produces it is not a rule, it is a bug.
    check_linear(
        r#"
static main = fn() -> () {
    let p = match alloc_array::<Res>(4) {
        ::Ok(p) => p,
        ::Err => panic("oom"),
    };
    unsafe { p.* = make(7); };
    unsafe { dealloc_array(p, 4); };
};
static result_shape = fn(r: AllocResult::<Res>) -> () {
    match r {
        ::Ok(p) => { },
        ::Err => { },
    };
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_data_side_parameter_takes_a_linear_in_annotation_position() {
    // What used to be refused here, and where the annotation-position hole
    // was: a signature is an instantiation edge no expression ever
    // crosses, so the data-side bound needed a second checker there — one
    // an argument lowering to `{error}` could launder past. The hole
    // closes BY CONSTRUCTION (T22): there is no data-side bound left to
    // check, so `fn(b: Box::<Res>)` is an ordinary signature and the body
    // that takes the box apart and drops its payload is clean.
    check_linear(
        r#"
type Box = struct::<T> { v: T };
static take = fn(b: Box::<Res>) -> () {
    let Box(struct { v }) = b;
    v.drop();
};
"#,
        expect![""],
    );
}

// ---- `String`: the first linear type, as a library ----------------------

/// The acid test: an OWNED, allocation-backed string, declared as a library
/// type and not as a compiler special case. Nothing in `hir` knows the name;
/// the whole of what makes it work is the clause and the members below, and
/// the refusals that follow are the ones a user of it would meet.
///
/// This mirrors `examples/string_lib.must`'s declaration exactly, but is
/// pinned here rather than shared with it: that example must check clean,
/// so the refusal cases below have nowhere to live except a copy.
const STRING_PRELUDE: &str = r#"
type String = struct {
    ptr: u8.&raw mut,
    len: usize,
    cap: usize,
} only move with {
    impl Self {
        as_str = fn::<@a>(s: Self.&::<@a>) -> str {
            unsafe { str_from_utf8_unchecked(s.*.ptr, s.*.len) }
        };
        len = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.len };
        drop = fn(s: Self) -> () {
            let String(struct { ptr, cap, .. }) = s;
            if cap > 0 {
                unsafe { dealloc_array(ptr, cap); };
            };
        };
    }
};
static empty_string = fn() -> String {
    String(struct { ptr = unsafe { dangling::<u8>() }, len = 0, cap = 0 })
};
static to_owned = fn::<@a>(s: str.&::<@a>) -> String {
    let text = s.*;
    let n = text.len();
    if n == 0 { return empty_string(); };
    let p = match alloc_array::<u8>(n) {
        ::Ok(p) => p,
        ::Err => panic("out of memory"),
    };
    unsafe { str_bytes(text, p); };
    String(struct { ptr = p, len = n, cap = n })
};
"#;

fn check_string(body: &str, expect: Expect) {
    check_diagnostics(&format!("{STRING_PRELUDE}{body}"), expect);
}

#[test]
fn the_string_library_itself_checks_clean() {
    check_string(
        r#"
static main = fn(text: str) -> usize {
    let s = to_owned(text.&);
    let n = s.&.len();
    print(s.&.as_str());
    s.drop();
    n
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn not_dropping_a_string_is_a_check_error() {
    check_string(
        r#"
static leak = fn(text: str) -> () {
    let s = to_owned(text.&);
    print(s.&.as_str());
};
"#,
        expect![[r#"
            996..1054: `s` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`s` is born here and must be consumed at 1006..1007)
        "#]],
    );
}

#[test]
fn dropping_a_string_twice_is_a_check_error() {
    check_string(
        r#"
static double = fn(text: str) -> () {
    let s = to_owned(text.&);
    s.drop();
    s.drop();
};
"#,
        expect![[r#"
            1048..1049: `s` was already consumed (`s` is born here and must be consumed at 1008..1009) (first consumed here at 1034..1035)
        "#]],
    );
}

#[test]
fn a_string_cannot_be_copied_out_of_a_borrow_or_a_container() {
    check_string(
        r#"
type Pair = struct { left: String, right: String };
static steal = fn::<@a>(s: String.&::<@a>) -> String { s.* };
static halve = fn(p: Pair) -> String {
    let Pair(struct { left, right }) = p;
    right.drop();
    left
};
static half_only = fn(p: Pair) -> String { p.left };
"#,
        expect![[r#"
            1069..1072: cannot move out of a borrow: `String` cannot be copied
            1230..1236: cannot copy a value that must be consumed out of a place; take the whole value apart instead (`let Name(struct { .. }) = value;`)
        "#]],
    );
}

#[test]
fn an_option_of_string_is_the_container_shape_that_has_to_work() {
    // Three of `examples/option.must`'s five members — a borrowing
    // `is_some`, a consuming `unwrap`, a region-projecting `as_ref` — over
    // a payload that must be consumed. The binder says nothing about
    // capabilities and did not have to (T22): `Option::<String>` is linear
    // because `String` is. Nothing here is `String`-specific either — it is
    // the acid test that the capability machinery composes with the generic
    // machinery.
    check_string(
        r#"
type Option = enum::<T> {
    Some(T),
    None,
} with {
    impl Self {
        is_some = const fn::<@local>(s: Self.&::<@local>) -> bool {
            match s {
                ::Some(_) => true,
                ::None => false,
            }
        }
        unwrap = const fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
        as_ref = fn::<@a>(s: Self.&::<@a>) -> Option::<T.&::<@a>> {
            match s {
                ::Some(t) => Option::Some(t),
                ::None => Option::None,
            }
        }
    }
};
static main = fn(text: str) -> () {
    let o: Option::<String> = Option::Some(to_owned(text.&));
    let present = o.&.is_some();
    // `as_ref` projects the payload as a BORROW, so the `Option` it hands
    // back is FORGETTABLE even though the one it came from is not — the
    // containment rule read through indirection, and the reason `as_ref`
    // needs no consuming of its own.
    let borrowed = o.&.as_ref();
    let still_there = borrowed.&.is_some();
    let s = o.unwrap();
    s.drop();
};
static leaks_the_payload = fn(text: str) -> () {
    let o: Option::<String> = Option::Some(to_owned(text.&));
    match o {
        ::Some(s) => { },
        ::None => { },
    };
};
"#,
        expect![[r#"
            2276..2279: `s` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`s` is born here and must be consumed at 2270..2271)
        "#]],
    );
}

#[test]
fn a_wildcard_let_cannot_swallow_a_linear() {
    // `let _ = <linear>` is refused with the wildcard message.
    check_linear(
        r#"
static main = fn() -> () {
    let _ = make(1);
};
"#,
        expect![[r#"
            371..372: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_nested_wildcard_cannot_swallow_a_linear() {
    // A `_` inside a destructuring `let` pattern is refused too.
    check_linear(
        r#"
static main = fn() -> () {
    let [r, _] = [make(1), make(2)];
    r.drop();
};
"#,
        expect![[r#"
            375..376: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_wildcard_parameter_cannot_swallow_a_linear() {
    // A `_` parameter of a linear type is refused at the parameter.
    check_linear(
        r#"
static take = fn(_: Res) -> () {};
"#,
        expect![[r#"
            353..354: `_` matches the value without binding it, and it must be consumed; give it a name so it has somewhere to go
        "#]],
    );
}

#[test]
fn a_hole_binding_cannot_swallow_a_linear_and_says_so_without_a_name() {
    // A hole in a binding position (here a record field renamed to `_`)
    // binds nothing to quote, and quoting the empty string reads as a
    // compiler bug — so the message names the position instead.
    check_linear(
        r#"
static main = fn() -> () {
    let struct { r as _ } = struct { r = make(1) };
};
"#,
        expect![[r#"
            361..416: the value swallowed by `_` is not consumed on this path; its type has no `forget` capability, so every path must consume it (the value swallowed by `_` is born here and must be consumed at 385..386)
        "#]],
    );
}

#[test]
fn a_join_that_produces_a_fresh_linear_still_has_to_answer_for_the_old_one() {
    // Affine would let the untaken branch's value fall on the floor. Linear
    // does not: the `else` path never consumed `r`, and it leaks where that
    // path leaves the body.
    check_linear(
        r#"
static main = fn(c: bool) -> Res {
    let r = make(1);
    if c { r } else { make(2) }
};
"#,
        expect![[r#"
            369..425: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 379..380) (consumed here, on another path at 403..404)
        "#]],
    );
}

// ---- `Reader`: a heap-owning type declared `only move` ------------------

/// The shipped `examples/stdin_lib.must` `Reader`, trimmed to what the
/// checker sees — the buffering/line-scanning machinery is not the point
/// here, only that the type owns a heap allocation, is declared `without
/// forget`, and disposes of itself by being taken apart, exactly like
/// `String.drop` above. `eof` stays in the trimmed type, unused by any test
/// here, so `..` in `drop` has a forgettable field to actually skip.
const READER_PRELUDE: &str = r#"
type Reader = struct {
    buf: u8.&raw mut,
    cap: usize,
    eof: bool,
} only move with {
    impl Self {
        drop = fn(r: Self) -> () {
            let Reader(struct { buf, cap, .. }) = r;
            unsafe { dealloc_array(buf, cap); };
        };
    }
};
static reader_new = fn(cap: usize) -> Reader {
    let buf = match alloc_array::<u8>(cap) {
        ::Ok(p) => p,
        ::Err => panic("out of memory"),
    };
    Reader(struct { buf, cap, eof = false })
};
"#;

fn check_reader(body: &str, expect: Expect) {
    check_diagnostics(&format!("{READER_PRELUDE}{body}"), expect);
}

#[test]
fn a_reader_dropped_on_the_one_path_is_clean() {
    check_reader(
        r#"
static main = fn() -> () {
    let r = reader_new(16);
    r.drop();
};
"#,
        expect![""],
    );
}

#[test]
fn not_dropping_a_reader_is_a_check_error() {
    // `Reader` owns an allocation and is `only move`, so a path that
    // never calls `drop` is refused (T20, M16).
    check_reader(
        r#"
static leak = fn() -> () {
    let r = reader_new(16);
};
"#,
        expect![[r#"
            505..536: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 515..516)
        "#]],
    );
}

#[test]
fn a_reader_drop_that_only_reads_a_field_is_refused() {
    // The obvious `drop` reads `r.buf`/`r.cap` and frees them — but a
    // plain field read leaves `r` itself untouched by the checker, so `r`
    // is still owed a consume. This is why `drop` above takes `r` apart
    // instead of reading out of it.
    check_diagnostics(
        r#"
type Reader = struct {
    buf: u8.&raw mut,
    cap: usize,
} only move with {
    impl Self {
        drop = fn(r: Self) -> () {
            unsafe { dealloc_array(r.buf, r.cap); };
        };
    }
};
"#,
        expect![[r#"
            130..194: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 115..116)
        "#]],
    );
}

// ---- patterns hand each binding its obligation --------------------------

#[test]
fn a_let_pattern_hands_each_linear_element_its_obligation() {
    check_linear(
        r#"
type Box = enum { Holds(Res) };
static clean = fn() -> () {
    let [a, b] = [make(1), make(2)];
    a.drop();
    b.drop();
    let ::Holds(r) = Box::Holds(make(3));
    r.drop();
};
static leak = fn() -> () {
    let [a, b] = [make(1), make(2)];
    a.drop();
};
static twice = fn() -> () {
    let arr = [make(1), make(2)];
    let [a, b] = arr;
    a.drop();
    b.drop();
    let [c, d] = arr;
    c.drop();
    d.drop();
};
"#,
        expect![[r#"
            545..599: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`b` is born here and must be consumed at 559..560)
            730..733: `arr` was already consumed (`arr` is born here and must be consumed at 637..640) (first consumed here at 680..683)
        "#]],
    );
}

// ---- the five `Option` members ------------------------------------------
//
// The acid test for member-own type binders: the five `Option` members —
// two of which (`flat_map`, `map`) cannot be written at all without one —
// over a forgettable payload and a linear one. The two programs differ by
// exactly one type-level clause.

/// The five members, shared so the payload cases below cannot drift apart.
const OPTION_MEMBERS: &str = r#"    impl Self {
        is_some = const fn::<@local>(s: Self.&::<@local>) -> bool {
            match s {
                ::Some(_) => true,
                ::None => false,
            }
        }
        unwrap = const fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
        flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self) -> Option::<U> {
            match s {
                ::Some(v) => { f(v) }
                ::None => { Option::None }
            }
        }
        map = fn::<U>(f: fn(T) -> U, s: Self) -> Option::<U> {
            match s {
                ::Some(t) => Option::Some(f(t)),
                ::None => Option::None,
            }
        }
        as_ref = fn::<@a>(s: Self.&::<@a>) -> Option::<T.&::<@a>> {
            match s {
                ::Some(t) => Option::Some(t),
                ::None => Option::None,
            }
        }
    }"#;

#[test]
fn the_five_option_members_check_clean_over_any_payload() {
    // ONE source for both payload kinds, which is the whole of the ruling
    // (T22): the binder says nothing about capabilities, so there is no
    // second spelling of `Option` for the linear case to check separately.
    // Nothing in `flat_map` or `map` moves a `T` twice or drops one, so
    // nothing fires here for either — and
    // `the_five_option_members_check_clean_over_option_of_string` runs the
    // same five bodies over a payload that must be consumed.
    check_diagnostics(
        &format!(
            "\ntype Option = enum::<T> {{\n    Some(T),\n    None,\n}} with {{\n{OPTION_MEMBERS}\n}}\n"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_five_option_members_check_clean_over_option_of_string() {
    // The acid test's point, exercised on the real linear type: `String`
    // is `only move`, and `Option` is CLAUSE-FREE — `Option::<String>`
    // is linear because `String` is, and nobody had to tell the enum. All
    // five members, and `flat_map`/`map` over a payload that must be
    // consumed.
    check_string(
        &format!(
            r#"
type Option = enum::<T> {{
    Some(T),
    None,
}} with {{
{OPTION_MEMBERS}
}}
static length = fn(s: String) -> usize {{ let n = s.&.len(); s.drop(); n }};
static wrap_length = fn(s: String) -> Option::<usize> {{ Option::Some(length(s)) }};
static via_map = fn(text: str) -> usize {{
    let o: Option::<String> = Option::Some(to_owned(text.&));
    o.map(length).unwrap()
}};
static via_flat_map = fn(text: str) -> usize {{
    let o: Option::<String> = Option::Some(to_owned(text.&));
    o.flat_map::<usize>(wrap_length).unwrap()
}};
"#
        ),
        expect![[r#""#]],
    );
}

// ---- materialized temporaries (M12) -------------------------------------

/// A must-consume value cannot be borrowed as a temporary: a temporary
/// has no name, so nothing could ever consume it.
#[test]
fn a_value_that_must_be_consumed_cannot_be_borrowed_as_a_temporary() {
    check_linear(
        r#"
static peek = fn::<@a>(r: Res.&::<@a>) -> usize { r.*.id };
static main = fn() -> usize {
    peek(make(1).&)
};
"#,
        expect![[r#"
            435..442: this value must be consumed, so it cannot be borrowed as a temporary: a temporary has no name, and nothing could ever consume it — bind it with `let` first, then borrow the binding
        "#]],
    );
}

/// `.&raw` of a must-consume temporary gets the bind-it-with-`let`-first
/// refusal.
#[test]
fn an_addr_of_of_a_must_consume_temporary_is_refused_with_the_bind_first_message() {
    check_linear(
        r#"
static main = fn() -> () {
    let p = make(1).&raw;
};
"#,
        expect![[r#"
            375..382: this value must be consumed, so it cannot be borrowed as a temporary: a temporary has no name, and nothing could ever consume it — bind it with `let` first, then borrow the binding
        "#]],
    );
}

// ---- deferred initialization (`let mut x;`) ---------------------------------

#[test]
fn a_deferred_binding_assigned_on_every_path_before_its_read_is_clean() {
    check_diagnostics(
        r#"
static main = fn () -> () {
    let s = "abc";
    let mut i = 0;
    loop {
        let mut current;
        match s.next_char(i) {
            ::Char(c, next) => {
                current = c;
                i = next;
            },
            ::End => {
                break;
            },
        };
        if current == 'a' { print("found a\n"); }
    }
};
"#,
        expect![""],
    );
}

#[test]
fn a_deferred_binding_assigned_in_a_loop_before_its_break_is_clean() {
    check_diagnostics(
        r#"
static main = fn (n: usize) -> usize {
    let mut x;
    let mut i = 0;
    loop {
        x = i * 2;
        i = i + 1;
        if i > n { break; }
    }
    x
};
"#,
        expect![""],
    );
}

#[test]
fn reading_a_deferred_binding_before_any_assignment_is_refused() {
    check_diagnostics(
        r#"
static main = fn () -> usize {
    let mut x: usize;
    let y = x;
    let p = x.&;
    x = 1;
    y + x
};
"#,
        expect![[r#"
            66..67: `x` is used before it is assigned a value (`x` is declared here without a value at 44..45)
            81..82: `x` is used before it is assigned a value (`x` is declared here without a value at 44..45)
        "#]],
    );
}

#[test]
fn reading_a_deferred_binding_assigned_on_some_paths_only_is_refused() {
    check_diagnostics(
        r#"
static branch = fn (c: bool) -> usize {
    let mut x;
    if c { x = 1; }
    x
};
static after_loop = fn (n: usize) -> usize {
    let mut x;
    let mut i = 0;
    loop {
        if i > n { break; }
        x = i;
        i = i + 1;
    }
    x
};
static diverging = fn (c: bool) -> usize {
    let mut x;
    if c { x = 1; } else { return 0; }
    x
};
"#,
        expect![[r#"
            80..81: `x` is used here, but not every path to here assigns it a value (`x` is declared here without a value at 53..54)
            247..248: `x` is used here, but not every path to here assigns it a value (`x` is declared here without a value at 142..143)
        "#]],
    );
}

#[test]
fn a_deferred_linear_owes_its_consumption_once_assigned() {
    check_linear(
        r#"
static clean = fn () -> () {
    let mut r;
    r = make(1);
    r.drop();
};
static leaked = fn () -> () {
    let mut r: Res;
    r = make(1);
};
static some_paths = fn (c: bool) -> () {
    let mut r;
    if c { r = make(1); }
    r.drop();
};
static every_iteration = fn (c: bool) -> () {
    let mut r;
    loop {
        r = make(1);
        if c { break; }
    }
    r.drop();
};
"#,
        expect![[r#"
            442..482: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it (`r` is born here and must be consumed at 456..457)
            570..571: `r` is used here, but not every path to here assigns it a value (`r` is declared here without a value at 537..538)
            663..664: `r` still holds a value that must be consumed; assigning here would lose it (`r` is born here and must be consumed at 641..642)
        "#]],
    );
}

// ---- traps -----------------------------------------------------------------

fn check_move_traps(body: &str, expect: Expect) {
    fn move_traps<'db>(
        db: &'db dyn base_db::Db,
        item: hir::ItemId<'db>,
    ) -> &'db [crate::CheckTrap] {
        &crate::move_findings(db, item).traps
    }
    crate::tests::check_traps(&format!("{LINEAR_PRELUDE}{body}"), move_traps, expect);
}

/// Every finding traps where its refused operation runs, with its own
/// message: a use after a move before the use, an assignment over a live
/// value before the write, a leak before the storage end that leaks it —
/// a block's, a `break`'s, or the frame's `return`.
#[test]
fn move_traps_sit_before_the_refused_operation() {
    check_move_traps(
        r#"
static twice = fn() -> () {
    let r = make(1);
    r.drop();
    r.drop();
};
static over = fn() -> () {
    let mut r = make(1);
    r = make(2);
    r.drop();
};
static leaks = fn(c: bool) -> () {
    {
        let a = make(1);
    };
    loop {
        let b = make(2);
        if c { break; };
        b.drop();
    };
    let d = make(3);
    if c { return; };
    d.drop();
};
static unassigned = fn() -> usize {
    let mut n: usize;
    n
};
"#,
        expect![[r#"
            Some(403..404) statement: `r` was already consumed
            Some(476..483) statement: `r` still holds a value that must be consumed; assigning here would lose it
            Some(541..573) statement: `a` is not consumed on this path; its type has no `forget` capability, so every path must consume it
            Some(626..631) statement: `b` is not consumed on this path; its type has no `forget` capability, so every path must consume it
            Some(693..699) terminator: `d` is not consumed on this path; its type has no `forget` capability, so every path must consume it
            Some(783..784) statement: `n` is used before it is assigned a value
        "#]],
    );
}
