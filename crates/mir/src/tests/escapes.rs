//! Borrows escaping the body, and region assertions.

use super::*;

// ---- escapes -----------------------------------------------------------
//
// A borrow of a body-local that a caller's region needs is blamed at the
// borrow, with the exit that ends the storage as its note.

/// A region on a borrow expression is refused; an annotation asserts the
/// same region, and the escape is reported there.
#[test]
fn an_annotation_is_what_pins_a_borrows_region() {
    // Refused, and the region is not read: no escape is reported.
    check_loans(
        "static f = fn::<@a>() -> () { let n: usize = 1; let r = n.&::<@a>; };",
        expect![[r#"
            59..65: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
        "#]],
    );
    // The annotation carries the obligation.
    check_loans(
        "static f = fn::<@a>() -> () { let n: usize = 1; let r: usize.&::<@a> = n.&; };",
        expect![[r#"
            71..74: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 76..77: the storage ends here, where the body returns
        "#]],
    );
    // With no assertion at either site, the borrow is just a borrow.
    check_loans(
        "static f = fn::<@a>() -> () { let n: usize = 1; let r = n.&; };",
        expect![[r#""#]],
    );
}

/// `_.&::<@a>`, a hole referent under a written region, asserts the region
/// for any referent: a plain local, an fn-typed value (whose type cannot
/// be spelled before `.&`), and an argument through a block.
#[test]
fn a_hole_referent_is_the_general_region_assertion() {
    check_loans(
        "static f = fn::<@a>() -> () { let n: usize = 1; let q: _.&::<@a> = n.&; };",
        expect![[r#"
            67..70: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 72..73: the storage ends here, where the body returns
        "#]],
    );
    check_loans(
        r#"
static f = fn::<@a>() -> () {
    let g = fn() -> usize { 1 };
    let q: _.&::<@a> = g.&;
};
"#,
        expect![[r#"
            87..90: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 92..93: the storage ends here, where the body returns
        "#]],
    );
    // `fn() -> usize.&::<@a>` is a fn returning a borrow, so it mismatches.
    check_loans(
        r#"
static f = fn::<@a>() -> () {
    let g = fn() -> usize { 1 };
    let q: fn() -> usize.&::<@a> = g.&;
};
"#,
        expect![[r#"
            99..102: type mismatch: expected `fn() -> usize.&::<@a>`, found `fn() -> usize.&`
        "#]],
    );
    check_loans(
        r#"
static take = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn::<@a>() -> usize {
    let n: usize = 1;
    take({ let q: _.&::<@a> = n.&; q })
};
"#,
        expect![[r#"
            145..148: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 155..156: the storage ends here, where the body returns
        "#]],
    );
}

#[test]
fn a_borrow_of_a_local_cannot_escape_the_body() {
    check_loans(
        "static f = fn::<@a>() -> usize.&::<@a> { let n: usize = 5; n.& };",
        expect![[r#"
            59..62: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 63..64: the storage ends here, where the body returns
        "#]],
    );
}

/// `r.*.&` reborrows through `r`, so `n`'s loan reaches the universal the
/// return reaches.
#[test]
fn a_two_step_escape_through_an_explicit_reborrow_is_rejected() {
    check_loans(
        r#"
static f = fn::<@a>() -> usize.&::<@a> {
    let n: usize = 5;
    let r = n.&;
    r.*.&
};
"#,
        expect![[r#"
            76..79: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 91..92: the storage ends here, where the body returns
        "#]],
    );
}

/// A `T.&::<@z>` receiver reaches `T`'s bounds, and the region obligation
/// survives the dictionary call: `leak` escapes.
#[test]
fn a_borrowed_rigid_receiver_reaches_its_bounds() {
    check_loans(
        r#"
type Cell = struct { n: usize };
trait Pass = requires {
    pass: fn::<@b>(usize.&::<@b>, Self.&::<@b>) -> usize.&::<@b>;
} with {
    impl Cell { pass = fn::<@b>(p: usize.&::<@b>, s: Self.&::<@b>) -> usize.&::<@b> { p }; }
};
static ok = fn::<@z, T: Pass>(c: T.&::<@z>, p: usize.&::<@z>) -> usize.&::<@z> { c.pass(p) };
static leak = fn::<@z, T: Pass>(c: T.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    c.pass(n.&)
};
"#,
        expect![[r#"
            421..424: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
              note at 426..427: the storage ends here, where the body returns
        "#]],
    );
}

/// A requirement's own outlives bound (`@c: @b`) holds at the call, through
/// a bound-directed call and a qualified one alike.
#[test]
fn a_requirements_own_region_bound_travels_to_the_call_site() {
    check_loans(
        r#"
type Cell = struct { n: usize };
trait Widen = requires {
    widen: fn::<@b, @c: @b>(usize.&::<@c>, Self.&::<@b>) -> usize.&::<@b>;
} with {
    impl Cell {
        widen = fn::<@b, @c: @b>(p: usize.&::<@c>, s: Cell.&::<@b>) -> usize.&::<@b> { p };
    }
};
static bound_directed = fn::<@z, T: Widen>(t: T.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    t.widen(n.&)
};
static qualified = fn::<@z>(c: Cell.&::<@z>) -> usize.&::<@z> {
    let n: usize = 7;
    Widen::widen(n.&, c)
};
"#,
        expect![[r#"
            370..373: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
              note at 375..376: the storage ends here, where the body returns
            481..484: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
              note at 489..490: the storage ends here, where the body returns
        "#]],
    );
}

/// Both branches of the join return a borrow of a body-local.
#[test]
fn a_join_cannot_launder_an_escaping_borrow() {
    check_loans(
        r#"
static leak = fn::<@a>(c: bool) -> usize.&::<@a> {
    let x: usize = 7;
    if c { x.& } else { x.& }
};
"#,
        expect![[r#"
            85..88: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 104..105: the storage ends here, where the body returns
            98..101: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 104..105: the storage ends here, where the body returns
        "#]],
    );
}

/// A borrow returned at a join region `@b + @c` escapes as at `@b`.
#[test]
fn an_escape_at_a_join_region_is_refused() {
    check_loans(
        r#"
static leak = fn::<@b, @c>() -> usize.&::<@b + @c> {
    let n: usize = 7;
    n.&
};
"#,
        expect![[r#"
            80..83: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@b`, which outlives the body
              note at 84..85: the storage ends here, where the body returns
        "#]],
    );
}

#[test]
fn an_array_literal_of_borrows_cannot_launder_an_escape() {
    check_loans(
        r#"
static leak = fn::<@a>(p: usize.&::<@a>) -> usize.&::<@a> {
    let n: usize = 7;
    let arr = [p, n.&];
    arr[1]
};
"#,
        expect![[r#"
            101..104: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 118..119: the storage ends here, where the body returns
        "#]],
    );
}

#[test]
fn a_joined_record_of_borrows_cannot_launder_an_escape() {
    check_loans(
        r#"
static getx = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x };
static leak = fn::<@z>(c: bool) -> usize.&::<@z> {
    let n: usize = 7;
    getx(if c { struct { x = n.& } } else { struct { x = n.& } })
};
"#,
        expect![[r#"
            184..187: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
              note at 221..222: the storage ends here, where the body returns
            212..215: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@z`, which outlives the body
              note at 221..222: the storage ends here, where the body returns
        "#]],
    );
}

/// Handing back a borrow a parameter's field holds borrows no local.
#[test]
fn a_borrow_held_in_a_parameters_field_can_be_handed_back() {
    check_loans(
        r#"
static getx = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x };
static deref = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x.*.& };
"#,
        expect![[r#""#]],
    );
}

/// The scrutinee borrows a body-local and the payload borrow is returned,
/// so the local's loan reaches the universal through the payload.
#[test]
fn a_payload_borrow_of_a_local_cannot_escape_the_body() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static leak = fn::<@a>() -> usize.&::<@a> {
    let o: Opt::<usize> = Opt::<usize>::Some(5);
    match o.& { ::Some(t) => t, ::None => panic("none") }
};
"#,
        expect![[r#"
            144..147: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 192..193: the storage ends here, where the body returns
        "#]],
    );
}

/// A body handing back a borrow of a temporary it built is refused, and
/// the message says "temporary".
#[test]
fn a_borrow_of_a_temporary_that_outlives_the_body_is_refused() {
    check_loans(
        r#"
static mk = fn() -> usize { 7 };
static leak = fn::<@a>() -> usize.&::<@a> { mk().& };
static leak_named = fn::<@a>() -> usize.&::<@a> { let n = mk(); n.& };
"#,
        expect![[r#"
            78..84: borrowed value does not live long enough: this borrows a temporary, which lives no longer than the block that creates it, but the borrow has to last for `@a`, which outlives the body
              note at 85..86: the storage ends here, where the body returns
            152..155: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 156..157: the storage ends here, where the body returns
        "#]],
    );
}

/// A member call's `@b` is a fresh region at the call, so passing `local.&`
/// with a receiver at `@a` escapes, called by path or by dot alike.
#[test]
fn a_member_call_cannot_launder_an_escape() {
    check_loans(
        r#"
type Cell = struct { n: usize } with {
    impl Self {
        pass = fn::<@b>(p: usize.&::<@b>, m: Self.&::<@b>) -> usize.&::<@b> { p };
    }
};
static by_path = fn::<@a>(c: Cell.&::<@a>) -> usize.&::<@a> {
    let local: usize = 5;
    Cell::pass(local.&, c)
};
static by_dot = fn::<@a>(c: Cell.&::<@a>) -> usize.&::<@a> {
    let local: usize = 5;
    c.pass(local.&)
};
"#,
        expect![[r#"
            251..258: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 263..264: the storage ends here, where the body returns
            364..371: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 373..374: the storage ends here, where the body returns
        "#]],
    );
}

/// A member's own body is checked too.
#[test]
fn a_member_cannot_hand_back_a_borrow_of_its_local() {
    check_loans(
        r#"
type Cell = struct { n: usize } with {
    impl Self { leak = fn::<@a>() -> usize.&::<@a> { let n: usize = 5; n.& }; }
};
"#,
        expect![[r#"
            111..114: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 115..116: the storage ends here, where the body returns
        "#]],
    );
}

/// A nested block's local, handed to the caller: the storage ends where
/// the block is left, and that exit is the note.
#[test]
fn a_block_local_handed_to_the_caller_is_blamed_at_the_borrow() {
    check_loans(
        "static f = fn::<@a>() -> usize.&::<@a> { { let n: usize = 5; n.& } };",
        expect![[r#"
            61..64: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 65..66: the storage ends here, where the block is left
        "#]],
    );
}

/// A temporary built in a nested block and handed to the caller: blamed at
/// the borrow, with the block's exit as the note.
#[test]
fn a_block_temporary_handed_to_the_caller_is_blamed_at_the_borrow() {
    check_loans(
        r#"
static mk = fn() -> usize { 7 };
static leak = fn::<@a>() -> usize.&::<@a> { { mk().& } };
"#,
        expect![[r#"
            80..86: borrowed value does not live long enough: this borrows a temporary, which lives no longer than the block that creates it, but the borrow has to last for `@a`, which outlives the body
              note at 87..88: the storage ends here, where the block is left
        "#]],
    );
}

/// An arm's binding, handed to the caller: the note is where the arm is
/// left.
#[test]
fn an_arm_local_handed_to_the_caller_is_blamed_at_the_borrow() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn::<@a>(o: Opt::<usize>) -> usize.&::<@a> {
    match o { ::Some(n) => { n.& } ::None => panic("none") }
};
"#,
        expect![[r#"
            126..129: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 130..131: the storage ends here, where the arm is left
        "#]],
    );
}

/// An early `return` from inside a nested block leaves the frame too.
#[test]
fn an_early_return_of_a_block_local_escapes() {
    check_loans(
        r#"
static f = fn::<@a>(c: bool) -> usize.&::<@a> {
    if c { let n: usize = 5; return n.&; }
    panic("no")
};
"#,
        expect![[r#"
            85..88: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
              note at 78..88: the storage ends here, where the body returns
        "#]],
    );
}
