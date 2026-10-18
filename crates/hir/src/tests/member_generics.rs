//! A member's own generic arguments, turbofish and region arguments.

use super::*;

// ---- second-segment generic arguments -----------------------------------
//
// A turbofish on a path's SECOND segment parses into a real node of its
// own (`syntax`'s `MEMBER_GENERIC_ARGS`) and is judged HERE, by what the
// segment names: a MEMBER's own binder SPENDS it, a VARIANT's arguments
// are the owner's and misplaced. The split cannot live in the parser — it
// does not know which is which.

#[test]
fn member_own_generic_arguments_are_matched_against_the_members_own_binder() {
    // A member with no binder of its own takes no arguments — the same
    // arity report a non-generic item's turbofish gets, naming the member.
    // (The list is not the OWNER's and never falls back to it; that claim
    // is `member_own_generic_arguments_do_not_instantiate_the_owner`.)
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
static main = fn() -> usize {
    let f = Measured::size::<usize>;
    0
};
"#,
        expect![[r#"
            144..167: `Measured::size` takes no generic arguments
        "#]],
    );
}

#[test]
fn member_turbofish_on_an_unknown_base_is_still_one_diagnostic() {
    // The cascade this whole area exists to prevent, checked end to end:
    // `P::len::<usize>` on an undeclared `P` reports the unresolved NAME
    // and nothing else — no parse errors (the list is real syntax now), no
    // second complaint about the argument, and the statements around it
    // are untouched.
    check_diagnostics(
        r#"
static main = fn() -> usize {
    let g = P::len::<usize>;
    let n = 1;
    n
};
"#,
        expect![[r#"
            43..44: unresolved name `P`
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_do_not_instantiate_the_owner() {
    // The failure mode the ERROR node used to prevent structurally, now
    // prevented by the tree itself: were `::<usize>` read as the OWNER's
    // list, this program would be exactly `Pair::<usize>::first(p)` and
    // would check CLEAN. It must not — `first` declares no binder of its
    // own, so its own list is empty and one argument is one too many. The
    // OWNER's arguments still come from inference (from `p`), which is
    // what the type side below pins.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: usize } with {
    impl Self { first = fn(p: Self) -> T { let Pair(struct { a, b }) = p; a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::first::<usize>(p)
};
"#,
        expect![[r#"
            221..241: `Pair::first` takes no generic arguments; if these are meant for `Pair`, write `Pair::<...>::first`
        "#]],
    );
    // And the type side of the same claim: the OWNER's argument came from
    // `p`, never from the written list.
    check_infer(
        r#"
type Pair = struct::<T> { a: T, b: usize } with {
    impl Self { first = fn(p: Self) -> T { let Pair(struct { a, b }) = p; a }; }
};
static main = fn(p: Pair::<usize>) -> usize { Pair::first::<usize>(p) };
"#,
        expect![[r#"
            149..206 'fn(p: Pair::<usiz...': fn(Pair::<usize>) -> usize
            152..153 'p': Pair::<usize>
            179..206 '{ Pair::first::<u...': usize
            181..201 'Pair::first::<usize>': fn(Pair::<usize>) -> usize
            181..204 'Pair::first::<usi...': usize
            202..203 'p': Pair::<usize>
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_no_hint_when_the_owner_list_is_already_written() {
    // `Pair::<usize>::first::<usize>` already writes the owner's list — the
    // `{owner}::<...>::{member}` hint has nothing left to suggest, so it
    // must not repeat the user's own spelling back at them.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: usize } with {
    impl Self { first = fn(p: Self) -> T { let Pair(struct { a, b }) = p; a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::<usize>::first::<usize>(p)
};
"#,
        expect![[r#"
            221..250: `Pair::first` takes no generic arguments
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_belong_to_the_owner() {
    // A variant is a case of its enum and never gets a binder of its own,
    // so this one is a CORRECTION (move them), not a reservation — the
    // same tree, a different message, decided by what the segment names.
    check_diagnostics(
        r#"
type Shape = enum::<T> { Circle(T), Point };
static main = fn() -> usize {
    let c = Shape::Circle::<usize>;
    0
};
"#,
        expect![[r#"
            88..110: a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_no_hint_on_a_non_generic_owner() {
    // `Shape` has no binder at all, so `Shape::<...>::Circle` is not a fix —
    // it is a second error ("Shape takes no generic arguments"). The hint
    // must not steer the reader into it.
    check_diagnostics(
        r#"
type Shape = enum { Circle, Point };
static main = fn() -> usize {
    let c = Shape::Circle::<usize>;
    0
};
"#,
        expect![[r#"
            80..102: a variant has no generic arguments of its own: arguments written on `Shape::Circle` cannot be applied here
        "#]],
    );
}

#[test]
fn variant_own_generic_arguments_no_hint_when_the_owner_list_is_already_written() {
    // `Option::<usize>::Some::<bool>` already writes the owner's list — the
    // hint has nothing left to suggest, so it must not echo the user's own
    // spelling back.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static main = fn() -> usize {
    let o = Option::<usize>::Some::<bool>;
    0
};
"#,
        expect![[r#"
            86..115: a variant has no generic arguments of its own: arguments written on `Option::Some` cannot be applied here
        "#]],
    );
}

#[test]
fn trait_member_own_type_generic_arguments_are_spendable_in_both_forms() {
    // A trait requirement has always been allowed its own binder; what
    // changed is that the use site may now SPELL its type arguments — in
    // the value form and in the TR01 short/named call forms alike, through
    // the one `matched_member_args` every member path shares.
    check_diagnostics(
        r#"
trait D = requires { n: fn::<T: forget>(T, Self) -> usize; } with {
    impl usize { n = fn::<T: forget>(t: T, s: usize) -> usize { s }; }
};
static value = fn() -> usize {
    let f = D::<Self = usize>::n::<usize>;
    0
};
static called = fn(s: usize) -> usize { D::n::<usize>(1, s) };
"#,
        expect![""],
    );
    // And the binder-less report on a requirement with none of its own —
    // the same sentence an inherent member gets, named for the trait whose
    // requirement it is, and with NO `{owner}::<...>::{member}` hint: that
    // spelling collides with the separately reserved generic-trait form.
    check_diagnostics(
        r#"
trait D = requires { n: fn(Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static value = fn() -> usize {
    let f = D::<Self = usize>::n::<usize>;
    0
};
static called = fn(s: usize) -> usize { D::n::<usize>(s) };
"#,
        expect![[r#"
            151..180: `D::n` takes no generic arguments
            231..247: `D::n` takes no generic arguments
        "#]],
    );
}

#[test]
fn a_member_reached_through_a_borrow_is_named_by_its_impl_head() {
    // Every message about a member's own arguments names a spelling a
    // program could CONTAIN. A trait-impl member's item name is already the
    // written `head::member` form, so it is passed through — naming it from
    // the RECEIVER's rendered type instead would report `usize.&::peek` for
    // the ordinary `Self.&` member, which is not a path.
    check_diagnostics(
        r#"
trait Peek = requires {
    none: fn::<@a>(Self.&::<@a>) -> usize;
    one: fn::<@a, U: forget>(U, Self.&mut::<@a>) -> usize;
    counted: fn::<@a, const N: usize>(Self.&::<@a>) -> usize;
} with {
    impl usize {
        none = fn::<@a>(s: usize.&::<@a>) -> usize { 1 };
        one = fn::<@a, U: forget>(u: U, s: usize.&mut::<@a>) -> usize { 1 };
        counted = fn::<@a, const N: usize>(s: usize.&::<@a>) -> usize { N };
    }
};
static f = fn::<@a>(n: usize.&::<@a>, m: usize.&mut::<@a>) -> usize {
    n.none::<bool>() + m.one::<bool, str>(true) + n.counted::<3>()
};
"#,
        expect![[r#"
            510..526: `usize::none` takes no generic arguments
            529..553: `usize::one` takes 1 generic argument, found 2
            556..572: `usize::counted` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_on_a_trait_declaring_its_own_binder() {
    // The requirement's own BOUND rides its binder to the use site: a
    // spelled argument is checked against it, exactly as a free generic
    // fn's turbofish argument is.
    check_diagnostics(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; } with {
    impl usize { push = fn(s: str, w: usize) -> usize { w }; }
};
trait Display = requires { fmt: fn::<W: Write>(W, Self) -> W; } with {
    impl bool { fmt = fn::<W: Write>(w: W, x: bool) -> W { w }; }
};
static ok = fn(w: usize, x: bool) -> usize { Display::fmt::<usize>(w, x) };
static bad = fn(w: str, x: bool) -> str { Display::fmt::<str>(w, x) };
"#,
        expect![[r#"
            388..413: the bound `W: Write` is not satisfied here: `str` does not implement `Write`
        "#]],
    );
}

#[test]
fn member_own_generic_arguments_infer_the_owners_const_args_exactly_once() {
    // Regression: the owner's turbofish const args used to be inferred
    // TWICE on the trait path — `trait_path_self_arg` types every const in
    // `vp_args` regardless of whether it finds `Self`, so nothing under
    // `infer_qualified_trait_call` may re-infer them. One type-mismatch,
    // not two, in both the value and called forms.
    check_diagnostics(
        r#"
trait D = requires { n: fn(usize) -> Self; } with {
    impl usize { n = fn(k: usize) -> usize { k }; }
};
static value = fn() -> usize {
    let f = D::<const { 1 + true }>::n::<usize>;
    0
};
static called = fn() -> usize { D::<const { 1 + true }>::n::<usize>(3) };
"#,
        expect![[r#"
            151..186: `D` takes no generic arguments
            151..186: a member value is impl-specific, so it must name the implementer: `D::<Self = Type>::n`
            167..171: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 165..166)
            229..264: `D` takes no generic arguments
            229..267: `D::n` takes no generic arguments
            229..267: cannot infer `Self` for `D::n`: no argument determines the implementing type — annotate an argument
            245..249: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 243..244)
        "#]],
    );
}

#[test]
fn the_member_generic_program_the_future_made_legal() {
    // The program the reservation was written against, now checking
    // clean — the whole grant, in one example: the DECLARATION's binder is
    // live (`syntax::validation` no longer refuses the type half) and the
    // USE site's arguments are spent on it.
    //
    // Neither half ever needed a grammar change: both were always
    // diagnostics over a REAL parse, and deleting them was the grant.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self {
        size = fn::<T: forget>(m: Self, t: T) -> usize { m.n };
    }
};
static main = fn() -> usize {
    let f = Measured::size::<usize>;
    0
};
"#,
        expect![""],
    );
}

// ---- member-own TYPE binders --------------------------------------------

#[test]
fn a_constructed_variant_reaches_its_enums_borrowing_member() {
    // `Option::<usize>::Some(11)` has the enum's type, so `o.&` is an
    // `Option::<usize>.&` and the `Self.&` member takes it as written —
    // no annotation on `o` needed.
    let text = r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        is_some = fn::<@local>(s: Self.&::<@local>) -> bool {
            match s { ::Some(_) => true, ::None => false, }
        }
    }
};
static f = fn () -> () {
    let o = Option::<usize>::Some(11);
    if o.&.is_some() { }
};
"#;
    check_diagnostics(text, expect![[""]]);
    check_infer(
        text,
        expect![[r#"
            218..297 'fn () -> () {    ...': fn()
            230..297 '{     let o = Opt...': ()
            240..241 'o': Option::<usize>
            244..265 'Option::<usize>::...': fn(usize) -> Option::<usize>
            244..269 'Option::<usize>::...': Option::<usize>
            266..268 '11': usize
            275..295 'if o.&.is_some() { }': ()
            278..279 'o': Option::<usize>
            278..281 'o.&': Option::<usize>.&
            278..289 'o.&.is_some': fn(Option::<usize>.&) -> bool
            278..291 'o.&.is_some()': bool
            292..295 '{ }': ()
        "#]],
    );
}

#[test]
fn a_member_own_type_param_is_inferred_from_the_call() {
    // No turbofish: `U` is an ordinary inference variable at the call site,
    // pinned by the argument exactly as a free generic fn's would be.
    check_infer(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static main = fn(o: Option::<usize>) -> bool { o.map_to(width) };
"#,
        expect![[r#"
            223..253 'fn(n: usize) -> b...': fn(usize) -> bool
            226..227 'n': usize
            244..253 '{ n > 2 }': bool
            246..247 'n': usize
            246..251 'n > 2': bool
            250..251 '2': usize
            269..319 'fn(o: Option::<us...': fn(Option::<usize>) -> bool
            272..273 'o': Option::<usize>
            300..319 '{ o.map_to(width) }': bool
            302..303 'o': Option::<usize>
            302..310 'o.map_to': fn(fn(usize) -> bool, Option::<usize>) -> bool
            302..317 'o.map_to(width)': bool
            311..316 'width': fn(usize) -> bool
        "#]],
    );
}

#[test]
fn a_member_own_type_param_is_spellable_at_both_call_spellings() {
    // The two use-site spellings of the same instantiation: the dot-call's
    // own turbofish (a NEW grammar position — see `syntax`'s
    // `member_turbofish_on_a_dot_call`) and the qualified path's, which
    // has parsed into `MEMBER_GENERIC_ARGS` since the reservation.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static dotted = fn(o: Option::<usize>) -> bool { o.map_to::<bool>(width) };
static pathed = fn(o: Option::<usize>) -> bool { Option::map_to::<bool>(width, o) };
static owner_and_member = fn(o: Option::<usize>) -> bool {
    Option::<usize>::map_to::<bool>(width, o)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_written_member_type_argument_is_checked_against_the_call() {
    // The turbofish PINS the parameter, so a disagreeing argument is an
    // ordinary mismatch at the argument — the same thing a free generic
    // fn's turbofish does.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static wrong = fn(o: Option::<usize>) -> usize { o.map_to::<usize>(width) };
"#,
        expect![[r#"
            322..327: type mismatch: expected `fn(usize) -> usize`, found `fn(usize) -> bool`
        "#]],
    );
}

#[test]
fn a_member_own_type_param_is_rigid_inside_the_body() {
    // Rigid-parameter checking (TR07), unchanged by whose binder the param
    // came from: the body is checked ONCE against a rigid `U`, so an
    // operation `U` does not support is refused at the DEFINITION, not at
    // some instantiation.
    check_diagnostics(
        r#"
type Wrap = struct { n: usize } with {
    impl Self {
        bad = fn::<U>(u: U, w: Self) -> usize { u + w.n };
    }
};
"#,
        expect![[r#"
            104..105: type mismatch: expected `{number}`, found `U` (`+` requires `{number}` operands at 106..107)
        "#]],
    );
}

#[test]
fn a_member_own_type_param_takes_the_forget_bound_and_it_is_checked_at_the_call() {
    // A member's `U` is a type parameter like any other: it asks for
    // nothing unless the body needs something. `take` DISCARDS its `u`, so
    // it has to say `U: forget` — and saying it is a promise, checked at
    // the instantiation edge the member call is. `hand_back` needs no
    // bound at all: passing a value on is not discarding it.
    check_diagnostics(
        r#"
type Wrap = struct { n: usize } with {
    impl Self {
        take = fn::<U: forget>(u: U, w: Self) -> usize { w.n };
        hand_back = fn::<U>(u: U, w: Self) -> U { u };
    }
};
type Lin = struct { n: usize } only move with {
    impl Self { sink = fn(l: Self) -> usize { let Lin(struct { n }) = l; n }; }
};
static refused = fn(w: Wrap, l: Lin) -> usize { w.take(l) };
static allowed = fn(w: Wrap, l: Lin) -> usize { w.hand_back(l).sink() };
"#,
        expect![[r#"
            363..372: `Lin` cannot be a `U`: `Lin` is declared `only move`, and `U: forget` asks for a type whose values may be dropped on the floor
        "#]],
    );
}

#[test]
fn a_member_own_type_binder_composes_with_its_region_binder() {
    // Both halves of the member's own binder at once, in the order the
    // owner's binder is extended: regions stay always-inferred (no
    // spelling at the call), the type is the one the turbofish spells.
    check_diagnostics(
        r#"
type Cell = struct::<T> { v: T } with {
    impl Self {
        pick = fn::<@b, U>(alt: U, c: Self.&::<@b>) -> U { alt };
    }
};
static f = fn::<@a>(c: Cell::<usize>.&::<@a>) -> bool { c.pick::<bool>(true) };
static g = fn::<@a>(c: Cell::<usize>.&::<@a>) -> bool { c.pick(true) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_type_param_shadows_a_same_named_owner_param() {
    // Two `T`s in one binder — the owner's and the member's. The member's
    // is written LAST and wins inside the member, which is the rule
    // `scopes` already applies to a binder's const params (and the rule
    // local shadowing follows). `Self` is unaffected: it is the owner at
    // the OWNER's binder, whatever the member renamed on top of it.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        m = fn::<@r, T>(t: T, p: Self.&::<@r>) -> T { t };
    }
};
static main = fn(p: P::<usize>) -> bool { p.&.m::<bool>(true) };
static plain = fn(p: P::<usize>) -> usize { p.a };
"#,
        expect![""],
    );
}

#[test]
fn every_refused_member_turbofish_still_types_its_const_arguments() {
    // THE INVARIANT: every expression in a body gets a type, and a const
    // argument inside a member turbofish is an ordinary expression. It is
    // easy to lose on the exits that refuse the PATH for some other
    // reason, because those return before anything looks at the list — so
    // the freeing is structural (one wrapper per entry point, the spend
    // sites TAKE), and this pins one probe per family of exit.
    //
    // `const { 1 + true }` is the probe: if the argument were never
    // inferred, its own mismatch would go missing.
    check_diagnostics(
        r#"
type Shape = enum::<T> { Circle(T), Point } with {
    impl Self { keep = fn(s: Self) -> Self { s }; }
};
trait D = requires { n: fn(Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static nope = fn() -> usize { Shape::missing::<const { 1 + true }>(1) };
static variant = fn() -> usize { let c = Shape::Circle::<const { 1 + true }>; 0 };
static wrong_ns = fn() -> usize { let f = D::gone::<const { 1 + true }>; 0 };
static no_impl = fn() -> usize { let f = D::<Self = bool>::n::<const { 1 + true }>; 0 };
static on_a_value = fn(n: usize) -> usize { n.oops::<const { 1 + true }>(); 0 };
"#,
        expect![[r#"
            251..258: `Shape` has no variant `missing` (`Shape` is defined here at 6..11)
            273..277: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 271..272)
            328..363: a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`
            356..360: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 354..355)
            412..441: `D` has no requirement `gone`
            434..438: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 432..433)
            489..530: `bool` does not implement `D`
            523..527: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 521..522)
            581..611: no field or member `oops` on `usize`
            602..606: type mismatch: expected `{number}`, found `bool` (`+` requires `{number}` operands at 600..601)
        "#]],
    );
}

#[test]
fn a_refused_member_turbofish_says_nothing_it_cannot_back_up() {
    // The other half of the invariant: the const arguments are consumed
    // SILENTLY on every path that is already refusing the call. "`map_to`
    // takes no generic arguments" is a lie about a member that HAS a
    // binder, and piling it on an unrelated refusal is the noise the
    // infectious-and-silent rule exists to prevent. Not one of the
    // programs below may produce it.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s {
                ::Some(t) => f(t),
                ::None => panic("x"),
            }
        };
    }
};
type Holder = struct { go: fn(usize) -> usize };
static half_typed = fn(o: Option::<usize>) -> usize { o.map_to::<>; 0 };
static wrong_name = fn(o: Option::<usize>) -> usize { o.map_two::<usize>(1) };
static on_broken = fn(o: Nope) -> usize { o.map_to::<usize>(1) };
// A BARE INTEGER is the only const-arg form that can produce the
// no-defining-use diagnostic, so it is the only probe that can catch the
// noise coming back: the list is dropped, and a number nobody kept must
// not be told to annotate itself. (`const { ... }` cannot show this — a
// block's contents are inferred whatever happens to the list.)
static dropped_number = fn(h: Holder) -> usize { h.go::<3>(1) };
static unknown_number = fn(h: Holder) -> usize { h.nope::<3>(1) };
"#,
        expect![[r#"
            354..366: `map_to` is a member fn, not a field; call it: `.map_to(...)`
            427..448: no field or member `map_two` on `Option::<usize>`
            477..481: unknown type `Nope`
            919..928: `go` takes no generic arguments
            984..998: no field or member `nope` on `Holder`
        "#]],
    );
}

#[test]
fn a_binderless_member_refuses_even_an_empty_turbofish() {
    // `::<>` is a WRITTEN list: accepting it silently would make the
    // spelling mean two things. One sentence for every binder-less
    // turbofish, the same one a non-generic item's gets.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
trait D = requires { n: fn(Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static empty_inherent = fn(m: Measured) -> usize { m.size::<>() };
static empty_impl = fn(s: usize) -> usize { s.n::<>() };
static empty_path = fn(m: Measured) -> usize { Measured::size::<>(m) };
"#,
        expect![[r#"
            260..272: `Measured::size` takes no generic arguments
            320..329: `usize::n` takes no generic arguments
            380..398: `Measured::size` takes no generic arguments
        "#]],
    );
}

#[test]
fn an_item_turbofish_elides_regions_exactly_as_a_member_one_does() {
    // One rule: the written list spells TYPE and CONST arguments,
    // everywhere. `fn::<@b, U>` is called `f::<usize>` whether it is
    // reached as an item, through a dot, or through the qualified form.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None } with {
    impl Self { pick = fn::<@b, U>(alt: U, s: Self.&::<@b>) -> U { alt }; }
};
static free = fn::<@b, U>(alt: U, x: usize.&::<@b>) -> U { alt };
static counted = fn::<@b, T, const N: usize>(x: T.&::<@b>) -> usize { N };
static main = fn::<@a>(p: usize.&::<@a>, o: Opt::<usize>.&::<@a>) -> usize {
    let a: bool = free::<bool>(true, p);
    let b: bool = o.pick::<bool>(true);
    let c: bool = Opt::<usize>::pick::<bool>(true, o);
    counted::<usize, 3>(p)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_written_region_argument_is_refused_at_every_mention() {
    // Regions are inferred at calls, never written — named or wildcard,
    // and the SAME sentence in both lists, because there is no longer a
    // rule to tell apart. The rest of the list still spends: a use site
    // that got only the elision wrong gets exactly one message.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None } with {
    impl Self { pick = fn::<@b, U>(alt: U, s: Self.&::<@b>) -> U { alt }; }
};
static free = fn::<@b, U>(alt: U, x: usize.&::<@b>) -> U { alt };
static main = fn::<@a>(p: usize.&::<@a>, o: Opt::<usize>.&::<@a>) -> usize {
    let a: bool = free::<@_, bool>(true, p);
    let b: bool = free::<@a, bool>(true, p);
    let c: bool = o.pick::<@_, bool>(true);
    let d: bool = Opt::<usize>::pick::<@a, bool>(true, o);
    0
};
"#,
        expect![[r#"
            294..296: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            339..341: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            386..388: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            442..444: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
        "#]],
    );
}

#[test]
fn a_written_region_is_refused_on_a_requirement_and_a_builtin() {
    // The remaining spellings of "everywhere": a requirement reached
    // through a bound and through the qualified trait path, and a
    // scheme-shaped builtin. Same helper, same sentence, and the type
    // argument beside the region still spends.
    check_diagnostics(
        r#"
type Counter = struct { n: usize };
trait Counted = requires {
    count: fn::<@b, U>(U, Self.&::<@b>) -> U;
} with {
    impl Counter { count = fn::<@b, U>(alt: U, s: Counter.&::<@b>) -> U { alt }; }
};
static bounded = fn::<@z, T: Counted>(t: T.&::<@z>) -> usize {
    t.count::<@z, usize>(1)
};
static qualified = fn::<@z>(c: Counter.&::<@z>) -> usize {
    let r = alloc_array::<@z, usize>(2);
    Counted::count::<@_, usize>(1, c)
};
"#,
        expect![[r#"
            282..284: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            384..386: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            420..422: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
        "#]],
    );
}

#[test]
fn a_written_region_on_a_binderless_name_is_refused_the_same_way() {
    // A list its target has NOTHING to spend on — no binder at all, a
    // local, a field, a monomorphic builtin, a trait's own list — refuses
    // a region with the same sentence as a binder would, and owes "takes
    // no generic arguments" only when something spellable is left beside
    // it (`owes_not_generic`): the regions-only binder and the binder-less
    // item answer `::<@_>` and `::<@_, usize>` identically.
    check_diagnostics(
        r#"
type Holder = struct { go: fn(usize) -> usize };
trait D = requires { n: fn(Self) -> usize; } with {
    impl usize { n = fn(s: usize) -> usize { s }; }
};
static k = fn(x: usize) -> usize { x };
static regionless = fn::<@b>(x: usize.&::<@b>) -> usize { 1 };
static main = fn::<@a>(p: usize.&::<@a>, h: Holder) -> usize {
    let a: usize = k::<@_>(1);
    let b: usize = k::<@_, usize>(1);
    let c: usize = regionless::<@a, usize>(p);
    let d: usize = h.go::<@_>(1);
    let e: usize = a::<@_>;
    let f: usize = D::<@_, Self = usize>::n(3);
    print::<@_>("x");
    0
};
"#,
        expect![[r#"
            346..348: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            373..387: `k` takes no generic arguments
            377..379: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            411..434: `regionless` takes no generic arguments
            424..426: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            465..467: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            496..498: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            524..526: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            561..563: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
        "#]],
    );
}

#[test]
fn a_refused_region_argument_does_not_also_miscount_the_list() {
    // The DISPLACING shape: a region written where the binder has none at
    // all. Dropping it leaves nothing to spend, and "takes 1 generic
    // argument, found 0" would be a false sentence about a list the reader
    // can see one argument in. The refusal is the whole story, in both
    // positions. The suppression is deliberately TOTAL, not
    // count-conditional: the third case writes two spellable arguments for
    // one slot, an accurate arity error on its own, and still gets the
    // refusal alone — a reader who wrote a region cannot trust ANY count
    // about that list until the region is dropped. One message, then
    // re-check.
    check_diagnostics(
        r#"
type Opt = enum { Some(usize), None } with {
    impl Self { pick = fn::<U>(alt: U, s: Self) -> U { alt }; }
};
static id = fn::<T>(x: T) -> T { x };
static z1 = fn::<@a, T>(x: T, p: usize.&::<@a>) -> T { x };
static main = fn::<@b>(o: Opt, q: usize.&::<@b>) -> usize {
    let a: usize = id::<@z>(1);
    let b: bool = o.pick::<@a>(true);
    let c: usize = z1::<@_, usize, bool>(1, q);
    0
};
"#,
        expect![[r#"
            295..297: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            330..332: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
            365..367: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
        "#]],
    );
}

#[test]
fn a_type_mentions_turbofish_argument_is_named_in_the_blame() {
    // `Cause::GenericArg` is recorded by the TYPE-mention path too
    // (`Pair::<usize>(...)`), and its hint was silently dropped: the
    // renderer resolved the mention's base as `Resolution::Item` only, and
    // a type name resolves to `TypeItem`.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T };
static p = Pair::<usize>(struct { a = 1, b = true });
"#,
        expect![[r#"
            86..90: type mismatch: expected `usize`, found `bool` (because `T` was instantiated to `usize` by this argument at 59..64)
        "#]],
    );
}

#[test]
fn wrong_arity_counts_only_the_spellable_slots() {
    // A binder's regions are not positions a caller may write, so they are
    // not positions a caller may be told they got wrong: "`free` takes 2"
    // for `fn::<@b, U>` would be counting something unspellable.
    //
    // The zero case: a REGIONS-ONLY binder is generic, so it is not "not
    // generic" — but it has nothing spellable, and "takes no generic
    // arguments" is the true sentence about what may be written. An item
    // with no binder at all already says exactly that, so the two agree
    // rather than inventing a second wording — region written or not, see
    // `a_written_region_on_a_binderless_name_is_refused_the_same_way`.
    check_diagnostics(
        r#"
static free = fn::<@b, U>(alt: U, x: usize.&::<@b>) -> U { alt };
static regionless = fn::<@b>(x: usize.&::<@b>) -> usize { 1 };
static main = fn::<@a>(p: usize.&::<@a>) -> usize {
    let a: bool = free::<bool, usize>(true, p);
    let b: usize = regionless::<usize>(p);
    let c: usize = regionless::<>(p);
    0
};
"#,
        expect![[r#"
            200..219: `free` takes 1 generic argument, found 2 (declared here at 8..12)
            249..268: `regionless` takes no generic arguments
            292..306: `regionless` takes no generic arguments
        "#]],
    );
}

#[test]
fn a_member_turbofish_has_no_nameable_argument() {
    // TR01 gives v1 exactly ONE nameable argument, a trait's `Self`, and it
    // is written on the OWNER's list. A member's own arguments are
    // positional; a name here is refused rather than swallowed.
    //
    // WHICH refusal is the owner's to decide, and both sentences below are
    // true of their own program. An INHERENT owner (the first two) has no
    // `Self` argument at all, so it gets the same not-a-trait sentence any
    // other non-trait list gets. A TRAIT's member (the last three) does
    // have one, one segment to the left, so denying the trait-ness would
    // be false about what the reader can see — there the wrong thing is
    // the POSITION, and the message names the spelling that has it.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s {
                ::Some(t) => f(t),
                ::None => panic("x"),
            }
        };
    }
};
trait Pk = requires { pick: fn::<U>(U, Self) -> U; } with {
    impl usize { pick = fn::<U>(alt: U, s: usize) -> U { alt }; }
};
static width = fn(n: usize) -> bool { n > 2 };
static dotted = fn(o: Option::<usize>) -> bool { o.map_to::<Self = bool>(width) };
static pathed = fn(o: Option::<usize>) -> bool { Option::map_to::<Self = bool>(width, o) };
static impl_dot = fn(n: usize) -> bool { n.pick::<Self = bool>(true) };
static trait_path = fn(n: usize) -> bool { Pk::pick::<Self = bool>(true, n) };
static bound = fn::<T: Pk>(x: T) -> bool { x.pick::<Self = bool>(true) };
"#,
        expect![[r#"
            476..506: only a trait has a `Self` argument to name
            559..588: only a trait has a `Self` argument to name
            643..670: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
            717..749: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
            796..823: a member's own generic arguments are positional: `Self` is the owner's, one segment to the left (`Trait::<Self = Type>::member`)
        "#]],
    );
}

#[test]
fn cannot_infer_a_member_own_param_names_the_spelling_that_pins_it() {
    // The one prompt that exists to send a reader to the member
    // turbofish, so it has to name a turbofish the reader can WRITE — and
    // that is the SITE's answer, not the member's. At a dot-call the
    // member stands alone (`.fresh::<...>(...)`); at a path the qualified
    // name carries the list (`Option::fresh::<...>`, `Mk::mk::<...>`); and
    // a trait member VALUE is impl-specific, so there the bare path is
    // itself refused and the implementer naming is part of the spelling
    // (`Mk::<Self = usize>::mk::<...>`). Blame follows the site too — a
    // requirement's own `U` is `Mk::mk`'s, not `Mk`'s. Every spelling
    // below is checked clean when written out.
    check_diagnostics(
        r#"
type Option = enum { Some(usize), None } with {
    impl Self { fresh = fn::<U>(s: Self) -> U { panic("x") }; }
};
trait Mk = requires { mk: fn::<U>(Self) -> U; } with {
    impl usize { mk = fn::<U>(s: usize) -> U { panic("x") }; }
};
static a = fn(o: Option) -> usize { o.fresh(); 1 };
static b = fn(o: Option) -> usize { Option::fresh(o); 1 };
static c = fn(n: usize) -> usize { n.mk(); 1 };
static d = fn(n: usize) -> usize { Mk::mk(n); 1 };
static e = fn::<T: Mk>(x: T) -> usize { x.mk(); 1 };
static f = fn(n: usize) -> usize { let g = Mk::<Self = usize>::mk; g(n); 1 };
"#,
        expect![[r#"
            273..282: cannot infer the type parameter `U` of `Option::fresh`; write `.fresh::<...>(...)` to specify it
            325..338: cannot infer the type parameter `U` of `Option::fresh`; write `Option::fresh::<...>` to specify it
            383..389: cannot infer the type parameter `U` of `usize::mk`; write `.mk::<...>(...)` to specify it
            431..440: cannot infer the type parameter `U` of `Mk::mk`; write `Mk::mk::<...>` to specify it (defined here at 122..124)
            487..493: cannot infer the type parameter `U` of `Mk::mk`; write `.mk::<...>(...)` to specify it (defined here at 122..124)
            543..565: cannot infer the type parameter `U` of `Mk::mk`; write `Mk::<Self = usize>::mk::<...>` to specify it
        "#]],
    );
}

#[test]
fn a_bound_directed_dot_call_spends_a_member_turbofish() {
    // The THIRD spend site, and the one no test reached: a rigid receiver
    // resolves through the enclosing dictionary, and the REQUIREMENT's own
    // binder is instantiated there — so its type arguments are spellable
    // exactly as a concrete receiver's are.
    check_diagnostics(
        r#"
trait D = requires { pick: fn::<U>(U, Self) -> U; } with {
    impl usize { pick = fn::<U>(alt: U, s: usize) -> U { alt }; }
};
static generic = fn::<T: D>(x: T) -> bool { x.pick::<bool>(true) };
static inferred = fn::<T: D>(x: T) -> bool { x.pick(true) };
static main = fn() -> bool { generic::<usize>(1) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_turbofish_argument_is_named_in_the_blame() {
    // `Cause::MemberGenericArg`: the hint reads the MEMBER's list, which
    // hangs one level down inside `MEMBER_GENERIC_ARGS` — under a dot-call
    // callee here, under a path in the second program. Sharing
    // `Cause::GenericArg`'s renderer would have pointed at the OWNER's
    // argument instead, which is the wrong node and the wrong `usize`.
    check_diagnostics(
        r#"
type Cell = struct { v: usize } with {
    impl Self { pick = fn::<U>(alt: U, c: Self) -> U { alt }; }
};
static dotted = fn(c: Cell) -> bool { c.pick::<bool>(1) };
static pathed = fn(c: Cell) -> bool { Cell::pick::<bool>(1, c) };
"#,
        expect![[r#"
            160..161: type mismatch: expected `bool`, found `{number}` (because this member argument instantiated the parameter to `bool` at 154..158)
            223..224: type mismatch: expected `bool`, found `{number}` (because this member argument instantiated the parameter to `bool` at 217..221)
        "#]],
    );
}

#[test]
fn two_same_named_parameters_are_told_apart_in_the_message() {
    // `shadowed_param_note`, both branches. ONE binder declaring the name
    // twice (a member shadowing its owner's `T`) is the blessed case, so
    // its mismatch has to be actionable — "expected `T`, found `T`" is not.
    // Indices count every kind, which is why the member's `T` here is 1 and
    // not 0, and why the message says so.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        bad = fn::<T: forget>(t: T, p: Self) -> T { let P(struct { a }) = p; a };
    }
};
"#,
        expect![[r#"
            131..132: type mismatch: expected `T`, found `T` — `P::bad` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 1, the value has the one at index 0 — rename one of them (expected `T` because of this return type at 99..103)
        "#]],
    );
    // The pair need not be the WHOLE type: the shape the blessing invites
    // most is `Self` against `Owner::<T>`, where the two `T`s sit one
    // constructor down and the plain message reads "expected `P::<T>`,
    // found `P::<T>`". A borrow of one is the same case one more
    // constructor down, and pins that regions count in the index (the
    // member's `T` is 2 there, not 1).
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        nested = fn::<T: forget>(t: T, p: Self) -> P::<T> { p };
        deep = fn::<@x, T: forget>(t: T, p: Self.&::<@x>) -> P::<T>.&::<@x> { p };
    }
};
"#,
        expect![[r#"
            114..115: type mismatch: expected `P::<T>`, found `P::<T>` — `P::nested` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 1, the value has the one at index 0 — rename one of them (expected `P::<T>` because of this return type at 102..111)
            197..198: type mismatch: expected `P::<T>.&::<@x>`, found `P::<T>.&::<@x>` — `P::deep` declares `T` twice (the owner's parameters come first, then the member's own, and every kind counts — regions included): this position wants the one at binder index 2, the value has the one at index 0 — rename one of them (expected `P::<T>.&::<@x>` because of this return type at 177..194)
        "#]],
    );
    // The note's OTHER branch (two different items each declaring the
    // name) is UNREACHABLE today and is not pinned as behaviour, because
    // there is no program that produces it: a param cannot escape its
    // body, so every cross-item mention instantiates to fresh variables
    // and two rigid params only ever meet inside ONE binder. Both shapes
    // below are the near-misses, and both check clean — which is the
    // claim, and what would break first if that invariant ever moved.
    check_diagnostics(
        r#"
type P = struct::<T> { a: T } with {
    impl Self {
        one = fn::<T: forget>(t: T, p: Self) -> Self { P::two(t, p) };
        two = fn::<T: forget>(t: T, p: Self) -> Self { p };
    }
};
static f = fn::<T>(x: T) -> T { g(x) };
static g = fn::<T>(x: T) -> T { x };
"#,
        expect![""],
    );
}

#[test]
fn a_member_own_const_argument_is_the_one_reservation_left() {
    // What `MemberOwnConstArgs` still says, and the only shape that can
    // still say it: an INHERENT member cannot declare a const parameter at
    // all (`syntax::validation` refuses it), so this needs a trait
    // requirement — whose own binder has always been live — and the
    // refusal is of the WHOLE list, in both the called and the dot forms.
    check_diagnostics(
        r#"
trait D = requires { n: fn::<const N: usize>(Self) -> usize; } with {
    impl usize { n = fn::<const N: usize>(s: usize) -> usize { N }; }
};
static called = fn(s: usize) -> usize { D::n::<3>(s) };
static dotted = fn(s: usize) -> usize { s.n::<3>() };
"#,
        expect![[r#"
            184..196: `D::n` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
            240..250: `usize::n` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
        "#]],
    );
}

#[test]
fn a_member_turbofish_lands_only_on_a_member() {
    // The dot's new turbofish position is spendable on a MEMBER and on
    // nothing else the dot can reach: an fn-typed field carries the call
    // as a VALUE (no binder), and a builtin member has no binder either.
    // One refusal for both, stated on the name.
    check_diagnostics(
        r#"
type Holder = struct { go: fn(usize) -> usize };
static field_call = fn(h: Holder) -> usize { h.go::<usize>(1) };
static builtin_call = fn(s: str, i: usize) -> usize { let c = s.next_char::<usize>(i); i };
static plain_field = fn(h: Holder) -> fn(usize) -> usize { h.go::<usize> };
"#,
        expect![[r#"
            95..108: `go` takes no generic arguments
            177..197: `next_char` takes no generic arguments
            266..279: `go` takes no generic arguments
        "#]],
    );
}
