//! Dot-calls through borrows: receiver shapes and inserted borrows.

use super::*;

#[test]
fn a_dot_through_a_borrow_is_reserved_with_its_escape_named() {
    // G14's exception is one-directional: the compiler may insert a
    // borrow of `x.*`, never a deref of `x`. A member whose own `Self` is
    // a borrow is reached through a borrow receiver; a VALUE `Self` is
    // not — that would be the deref — and the message names the
    // one-character escape.
    check_diagnostics(
        "type Counter = struct { n: usize } with {\n\
             impl Self { get = fn (c: Self) -> usize { c.n }; }\n\
         };\n\
         static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.get() };",
        expect![[r#"
            147..152: `Counter.&::<@a>` is a borrow, so `.get` does not reach through it — there is no auto-deref; write `.*.get`
        "#]],
    );
    // The escape, working.
    check_diagnostics(
        "type Counter = struct { n: usize } with {\n\
             impl Self { get = fn (c: Self) -> usize { c.n }; }\n\
         };\n\
         static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.*.get() };",
        expect![[r#""#]],
    );
}

// ---- members that borrow `Self` ----------------------------------------

#[test]
fn an_inherent_members_own_region_binder_is_live() {
    // A member-own REGION was the first half of the binder to go live, and
    // it is the half with no alternative at all: regions on type
    // declarations are themselves reserved, so a member taking a borrow of
    // `Self` has nowhere else to bind the per-call region it needs, and a
    // signature elides nothing, so it may not decline to name one. (The
    // TYPE half is live too — TR10 — and spelled at the use site; only
    // CONSTS are still reserved.)
    check_diagnostics(
        r#"
type Map = struct::<K, V> { n: usize } with {
    impl Self {
        peek = fn::<@b>(m: Self.&mut::<@b>) -> usize { m.*.n };
    }
};
static f = fn::<@a>(m: Map::<usize, usize>.&mut::<@a>) -> usize { m.peek() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_region_is_per_call_never_the_callers_universal() {
    // The member's binder is the OWNER's followed by its own, so a
    // member-own region sits at `owner_arity + i`. Left rigid, it survives
    // into the CALLER's body, where the outlives solver reads a region
    // param's binder index as a NODE NUMBER — a member's `@b` at index 0
    // silently BECOMES the caller's universal at index 0, and two calls in
    // one body share one region. Both spellings mint a fresh existential
    // instead, so this program (which relates nothing to `@a`) checks.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        pass = fn::<@b>(p: usize.&::<@b>, m: Self.&::<@b>) -> usize.&::<@b> { p };
    }
};
static dot = fn::<@a>(c: Cell.&::<@a>) -> usize {
    let short: usize = 5;
    c.pass(short.&).*
};
static qualified = fn::<@a>(c: Cell.&::<@a>) -> usize {
    let short: usize = 5;
    Cell::pass(short.&, c).*
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_region_still_carries_its_declared_outlives_bounds() {
    // `fn::<@b, @c: @b>` at a member is the same instantiation a free fn
    // gets: the declared bound becomes an obligation between the two fresh
    // variables, so the body may rely on it and the caller must supply it.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        widen = fn::<@b, @c: @b>(p: usize.&::<@c>, m: Self.&::<@b>) -> usize.&::<@b> { p };
    }
};
static f = fn::<@a>(c: Cell.&::<@a>, p: usize.&::<@a>) -> usize { c.widen(p).* };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_member_own_const_binder_stays_reserved_precisely() {
    // The one kind that keeps its reservation, still stated per kind and
    // squiggling the individual param rather than the whole list — so a
    // mixed binder grants the region and the type and refuses only the
    // const.
    check_diagnostics(
        r#"
type Measured = struct { n: usize } with {
    impl Self {
        size = fn::<@b, T: forget, const N: usize>(m: Self.&::<@b>, t: T) -> usize { m.*.n };
    }
};
"#,
        expect![[r#"
            95..109: a member's own const parameters are not supported yet (a const argument is part of an instance's identity, and a member's arguments are read off the receiver's own type)
        "#]],
    );
}

#[test]
fn a_borrow_receiver_dot_call_reborrows_into_the_members_region() {
    // G14 and TR01 say the receiver IS the last argument, so a member whose last
    // parameter is a BORROW of `Self` takes a borrow receiver — through
    // exactly the reborrow every other argument position gets. This is G14's
    // exception clause 1: the inserted borrow is of `map.*`, where `map` is
    // already a borrow. Exclusive receivers reach shared members too, by
    // the ordinary degradation reborrow.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        get = fn::<@b>(m: Self.&::<@b>) -> usize { m.*.n };
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn::<@a>(c: Cell.&mut::<@a>) -> usize {
    c.bump();
    c.get()
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_owned_receiver_never_gets_a_borrow_inserted_for_it() {
    // NOT auto-ref, and the reason is clause 2 of the G14 exception: the
    // borrow the compiler would have to insert is a borrow of the LOCAL
    // itself, which the exception forbids outright. So the escape is
    // spelled, and the diagnostic spells it.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn() -> usize {
    let mut c = Cell(struct { n = 1 });
    c.bump();
    c.n
};
"#,
        expect![[r#"
            210..218: `bump` takes `Self.&mut`, and a borrow is never inserted for an owned receiver — write `.&mut.bump(...)` (`bump` is defined here at 64..68)
        "#]],
    );
    // The escape, working: `c.&mut.bump()` is an ordinary postfix chain
    // whose receiver is then a borrow, so it comes back through the
    // licensed case.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn() -> usize {
    let mut c = Cell(struct { n = 1 });
    c.&mut.bump();
    c.n
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_owned_receiver_offers_the_insert_borrow_fix() {
    let fixed = apply_insertion_fix(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.n };
    }
};
static f = fn () -> usize {
    let c: Counter = Counter(struct { n = 1 });
    c.get()
};
"#,
        "Insert `.&` before `.get`",
    );
    assert!(fixed.contains("    c.&.get()\n"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn an_owned_receiver_of_an_exclusive_member_offers_the_insert_borrow_mut_fix() {
    // The borrow lands right before the call's own `.`, so a call chained
    // onto a new line keeps its layout and a temporary receiver works too.
    let fixed = apply_insertion_fix(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn () -> () {
    Cell(struct { n = 1 })
        .bump();
};
"#,
        "Insert `.&mut` before `.bump`",
    );
    assert!(
        fixed.contains("    Cell(struct { n = 1 })\n        .&mut.bump();\n"),
        "fixed: {fixed}"
    );
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_shared_receiver_cannot_reach_an_exclusive_member() {
    // Shared never sharpens to exclusive — the same rule `try_reborrow`
    // enforces at every other argument position, stated at the receiver,
    // where the place is still nameable.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};
static f = fn::<@a>(c: Cell.&::<@a>) -> () { c.bump() };
"#,
        expect![[r#"
            184..192: `bump` takes `Self.&mut`, but `Cell.&::<@a>` is a shared borrow — a shared borrow never becomes exclusive (`bump` is defined here at 64..68)
        "#]],
    );
}

#[test]
fn the_receiver_shape_disambiguates_a_member_against_a_same_named_field() {
    // A pleasant consequence of the rule being a TABLE rather than a
    // filter: a fn-typed field and a `Self.&`-taking member of the same
    // name are never both candidates, so neither spelling is ambiguous and
    // each names exactly one thing.
    //
    // Through a BORROW the field is not reachable at all (that would be
    // auto-deref), so the member wins; on an OWNED receiver the member is
    // not reachable (clause 2 forbids borrowing the local), so the field
    // wins — and the reader who wanted the member writes `b.&.get()`.
    check_diagnostics(
        r#"
type B = struct { get: fn() -> usize } with {
    impl Self { get = fn::<@b>(x: Self.&::<@b>) -> usize { 100 }; }
};
static through_a_borrow = fn::<@a>(b: B.&::<@a>) -> usize { b.get() };
static owned = fn(b: B) -> usize { b.get() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_trait_requirement_may_carry_a_region_binder() {
    // `binders_match` had arms for Type and Const and none for Region, so
    // a region param in a requirement's binder made EVERY textually
    // identical impl member "not match". A `Self.&`-taking requirement
    // must declare a region (a signature elides nothing), so that one
    // guard was what made the trait half of borrow-`Self` members
    // unreachable.
    //
    // All three kinds side by side, each with a conforming impl.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait A = requires { f: fn(usize, Self) -> usize; }
  with { impl Cell { f = fn(x: usize, s: Self) -> usize { x }; } };
trait B = requires { g: fn::<T: forget>(T, Self) -> usize; }
  with { impl Cell { g = fn::<T: forget>(x: T, s: Self) -> usize { 1 }; } };
trait C = requires { h: fn::<@b>(usize.&::<@b>, Self) -> usize; }
  with { impl Cell { h = fn::<@b>(x: usize.&::<@b>, s: Self) -> usize { 1 }; } };
"#,
        expect![""],
    );
}

#[test]
fn a_region_binders_outlives_bounds_must_agree_positionally() {
    // Region outlives bounds name SIBLING params, so they compare by
    // position, not by spelling — the requirement's `@a`/`@b` and the
    // impl's `@x`/`@y` are the same binder. An impl that declares a
    // DIFFERENT bound set does not match, the same exact-set rule type
    // bounds already follow.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Renamed = requires { f: fn::<@a, @b: @a>(usize.&::<@b>, Self) -> usize; }
  with { impl Cell { f = fn::<@x, @y: @x>(p: usize.&::<@y>, s: Self) -> usize { 1 }; } };
"#,
        expect![[r#"
            135..136: member `f` does not match `Renamed`'s requirement: expected `fn(usize.&::<@b>, Cell) -> usize`, found `fn(usize.&::<@y>, Cell) -> usize` (required by the trait here at 61..62)
        "#]],
    );
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Dropped = requires { f: fn::<@a, @b: @a>(usize.&::<@b>, Self) -> usize; }
  with { impl Cell { f = fn::<@x, @y>(p: usize.&::<@y>, s: Self) -> usize { 1 }; } };
"#,
        // TODO: halvko/must#163: bounds may come in any order; the message stops naming one.
        expect![[r#"
            135..136: member `f`'s generic binder does not match `Dropped`'s requirement (arity, kinds and bounds must agree, the bounds in the requirement's order) (required by the trait here at 61..62)
        "#]],
    );
}

#[test]
fn a_borrow_self_requirement_is_reachable_both_ways() {
    // The trait half, end to end: declared, implemented, called on a
    // CONCRETE borrow receiver (impl-directed) and on a BORROWED RIGID one
    // (bound-directed, through the hidden dictionary). `examples/` has the
    // runnable twin — this same shape returns 82.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Readable = requires {
    read: fn::<@r>(Self.&::<@r>) -> usize;
} with {
    impl Cell { read = fn::<@r>(s: Cell.&::<@r>) -> usize { s.*.n }; }
};
static direct = fn::<@a>(c: Cell.&::<@a>) -> usize { c.read() };
static generic = fn::<@z, T: Readable>(t: T.&::<@z>) -> usize { t.read() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn the_receiver_shape_table_is_the_same_on_a_rigid_receiver() {
    // One table, one set of messages, whether `Self` is nominal or rigid —
    // which is the point of routing both paths through
    // `receiver_shape_refusal`. A rigid receiver used to get a different
    // (and, for the borrow cases, wrong) story.
    check_diagnostics(
        r#"
type Cell = struct { n: usize };
trait Bump = requires {
    bump: fn::<@r>(Self.&mut::<@r>) -> ();
} with {
    impl Cell { bump = fn::<@r>(s: Cell.&mut::<@r>) -> () { s.*.n = 1; }; }
};
static owned = fn::<T: Bump>(t: T) -> () { t.bump() };
static shared = fn::<@z, T: Bump>(t: T.&::<@z>) -> () { t.bump() };
static exclusive = fn::<@z, T: Bump>(t: T.&mut::<@z>) -> () { t.bump() };
"#,
        expect![[r#"
            232..240: `bump` takes `Self.&mut`, and a borrow is never inserted for an owned receiver — write `.&mut.bump(...)`
            300..308: `bump` takes `Self.&mut`, but `T.&::<@z>` is a shared borrow — a shared borrow never becomes exclusive
        "#]],
    );
}

// ---- region walkers: generic arguments and borrows --------------------

#[test]
fn a_type_param_under_a_borrow_is_instantiated() {
    // `instantiate_scheme` had no `Ty::Borrow` arm, so `T` in
    // `T.&mut::<@b>` stayed RIGID at every call: inference could not learn
    // `T` from a borrowed argument, and an explicit turbofish substituted
    // everywhere EXCEPT under the borrow, producing the impossible
    // "expected `T.&mut`, found `usize.&mut`" — a mismatch against a
    // parameter the caller has no way to name. One arm, both symptoms.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static pick = fn::<@b, T>(v: T.&mut::<@b>, c: bool) -> Opt::<T.&mut::<@b>> {
    Opt::<T.&mut::<@b>>::None
};
static inferred = fn() -> usize {
    let mut x: usize = 3;
    let r = pick(x.&mut, true);
    0
};
static explicit = fn() -> usize {
    let mut x: usize = 3;
    let r = pick::<usize>(x.&mut, true);
    0
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_region_wildcard_inside_a_generic_argument_is_an_inference_variable() {
    // `@_` is a written token meaning "there is a region here, infer it".
    // `mint_wildcard_regions` replaced the placeholder with a fresh
    // variable for a borrow, a raw pointer, an array, an fn and a record —
    // and not for a generic type's ARGUMENTS, the same hole closed four
    // times elsewhere. `Opt::<V.&mut::<@_>>` is a shape users write.
    //
    // Both symptoms were rendering failures, not laundering: a type
    // spelling nobody may write (`@{error}`), and the compiler accusing
    // itself. The obligations were always emitted — which is why the
    // `leaks` case below was rejected before this fix and still is.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static mk = fn::<@b>(p: usize.&::<@b>) -> Opt::<usize.&::<@b>> { Opt::<usize.&::<@b>>::Some(p) };
static annotated = fn::<@a>(p: usize.&::<@a>) -> usize {
    let o: Opt::<usize.&::<@_>> = mk(p);
    0
};
"#,
        expect![[r#""#]],
    );
    // The mismatch case names a type the reader may actually write.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None };
static mk = fn::<@b>(p: usize.&::<@b>) -> Opt::<usize.&::<@b>> { Opt::<usize.&::<@b>>::Some(p) };
static wrong = fn() -> usize {
    let n: usize = 7;
    let o: Opt::<bool.&::<@_>> = mk(n.&);
    0
};
"#,
        expect![[r#"
            225..232: type mismatch: expected `Opt::<bool.&>`, found `Opt::<usize.&>` (expected `Opt::<bool.&>` because of this annotation at 203..222)
        "#]],
    );
    // And the obligation still lands: an intermediate annotated with `@_`
    // relates the two regions, so an undeclared relation is still reported
    // at the return, with the same message the un-annotated control gets,
    // not an internal error.
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None } with {
    impl Self { unwrap = fn(s: Self) -> T { match s { ::Some(t) => t, ::None => panic("no"), } } }
};
static leaks = fn::<@a, @z>(p: usize.&::<@a>) -> usize.&::<@z> {
    let o: Opt::<usize.&::<@_>> = Opt::<usize.&::<@a>>::Some(p);
    o.unwrap()
};
"#,
        expect![[r#"
            283..293: using this borrow where a longer-lived one is expected needs `@a` to outlive `@z`, which this signature does not declare; add `@a: @z` to the binder
        "#]],
    );
    check_diagnostics(
        r#"
type Opt = enum::<T> { Some(T), None } with {
    impl Self { unwrap = fn(s: Self) -> T { match s { ::Some(t) => t, ::None => panic("no"), } } }
};
static leaks = fn::<@a, @z>(p: usize.&::<@a>) -> usize.&::<@z> {
    let o = Opt::<usize.&::<@a>>::Some(p);
    o.unwrap()
};
"#,
        expect![[r#"
            261..271: using this borrow where a longer-lived one is expected needs `@a` to outlive `@z`, which this signature does not declare; add `@a: @z` to the binder
        "#]],
    );
}

#[test]
fn a_broken_receiver_does_not_also_get_accused_of_a_raw_deref() {
    // `derefs_a_raw_pointer` read "not a safe borrow" as "a raw pointer",
    // which is the right fail-safe default for an UNRESOLVED type and the
    // wrong one for an already-diagnosed one. Errors are infectious and
    // SILENT, and a broken receiver in a postfix chain is where that
    // shows.
    check_diagnostics(
        r#"
type Cell = struct { n: usize } with {
    impl Self { take = fn(x: usize, s: Self) -> usize { 1 }; }
};
static f = fn(c: Cell) -> usize { c.nosuch().* };
"#,
        expect![[r#"
            140..150: no field or member `nosuch` on `Cell`
        "#]],
    );
    // The real thing is still caught.
    check_diagnostics(
        "static f = fn(p: usize.&raw) -> usize { p.* };",
        expect![[r#"
            40..43: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}
