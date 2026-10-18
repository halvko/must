//! The loan check: liveness, conflicts and where loan traps sit.

use super::*;

/// THE WITNESS: `l1` views the reader's own storage, the second
/// `next_line` moves the reader on, and the read afterwards is a read
/// through an invalidated borrow. Refused at the second use of `m`, naming
/// the other two sites in the interpreter's own vocabulary. Exactly ONE
/// refusal: the `r.deinit()` closing every test in this family is the
/// negative control for the MOVE arm and has to stay silent.
#[test]
fn a_view_read_after_the_reader_moves_on_is_refused() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    m.next_line();
    let out = match l1 { ::Some(line) => line.*, ::None => 0 };
    r.deinit();
    out
};
"#,
        expect![[r#"
            1049..1050: using `m` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 1030..1031: this borrow was created here
              note at 1084..1086: and it is still used here
        "#]],
    );
}

/// The sound twin, and why the rule is LIVENESS and not scope: the view is
/// read before the reader moves on, so it is dead at the second call even
/// though it is still in scope there.
#[test]
fn a_view_read_before_the_reader_moves_on_is_fine() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    let out = match l1 { ::Some(line) => line.*, ::None => 0 };
    m.next_line();
    r.deinit();
    out
};
"#,
        expect![[r#""#]],
    );
}

/// The corpus shape: sequential reads in a loop, each iteration's view
/// dead before the next call. Scope-based liveness refuses this — the
/// previous iteration's binding is in scope at every `next_line` — and it
/// is the single most important program the checker has to accept. Also
/// where the loop and the move rule meet: `m` spans the whole loop and
/// `r.deinit()` after it must still be clean.
#[test]
fn sequential_reads_with_dead_views_check_clean() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let mut total = 0;
    loop {
        match m.next_line() {
            ::Some(line) => { total = total + line.*; },
            ::None => break,
        };
    };
    r.deinit();
    total
};
"#,
        expect![[r#""#]],
    );
}

/// Copying the view OUT survives the next call: the copy has no region, so
/// nothing about it is a loan any more. `string_lib`'s `to_owned` in one
/// line.
#[test]
fn a_value_copied_out_of_a_view_survives_the_next_read() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let text = match m.next_line() { ::Some(line) => line.*, ::None => 0 };
    m.next_line();
    r.deinit();
    text
};
"#,
        expect![[r#""#]],
    );
}

/// The MOVE arm on the shape it exists for: the reader is disposed of
/// while a view into its storage is still needed. The interpreter calls
/// this "moved away" and detects it; now it does not compile.
#[test]
fn disposing_of_the_reader_under_a_live_view_is_refused() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    r.deinit();
    match l1 { ::Some(line) => line.*, ::None => 0 }
};
"#,
        expect![[r#"
            1049..1050: moving `r` here invalidates a borrow of it that is still live: the borrow points into storage this move takes away, and it is used after this point
              note at 1009..1015: this borrow was created here
              note at 1071..1073: and it is still used here
        "#]],
    );
}

/// Two exclusive loans of one local, both live. The oldest refusal there
/// is.
#[test]
fn two_live_exclusive_borrows_of_one_local_are_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    let b = n.&mut;
    bump(b);
    a.*
};
"#,
        expect![[r#"
            156..162: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 136..142: this borrow was created here
              note at 181..182: and it is still used here
        "#]],
    );
}

/// Sequenced, they are fine: the first loan is dead where the second is
/// taken. Reborrow-at-every-use is what makes this the common case.
#[test]
fn sequenced_exclusive_borrows_of_one_local_are_fine() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    bump(a);
    let b = n.&mut;
    b.*
};
"#,
        expect![[r#""#]],
    );
}

/// A `&mut` mint kills SHARED loans too — a write is foreign to every
/// loan it overlaps, whatever that loan's flavour.
#[test]
fn a_live_shared_borrow_is_killed_by_a_later_exclusive_one() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let m = n.&mut;
    bump(m);
    get(s)
};
"#,
        expect![[r#"
            211..217: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 194..197: this borrow was created here
              note at 240..241: and it is still used here
        "#]],
    );
}

/// The READ arm: an exclusive loan is the only way to the value while it
/// lasts, so reading the place around it — by taking a `.&`, or by simply
/// naming the local — invalidates it. Both spellings are the same access
/// and get the same refusal with a different verb.
#[test]
fn reading_the_root_around_a_live_exclusive_borrow_is_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static by_name = fn() -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let copy = n;
    bump(m);
    copy
};
static by_shared_borrow = fn() -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let s = n.&;
    bump(m);
    get(s)
};
"#,
        expect![[r#"
            223..224: reading `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 200..206: this borrow was created here
              note at 235..236: and it is still used here
            351..354: borrowing `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 331..337: this borrow was created here
              note at 365..366: and it is still used here
        "#]],
    );
}

/// A foreign read does NOT disturb a shared loan — any number of readers
/// coexist, which is the interpreter's own rule (a read scans only the
/// exclusive nodes). The negative control for the arm above.
#[test]
fn a_shared_loan_tolerates_a_foreign_read() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let copy = n;
    get(s) + copy
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn many_live_shared_borrows_of_one_local_are_fine() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&;
    let b = n.&;
    get(a) + get(b)
};
"#,
        expect![[r#""#]],
    );
}

/// Path granularity, both directions, matching the aliasing tree's own
/// rule: two disjoint fields never conflict, and a borrow of the WHOLE
/// value conflicts with a borrow of a part.
#[test]
fn disjoint_field_borrows_are_independent_and_a_containing_one_is_not() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static disjoint = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    let b = p.y.&mut;
    bump(a);
    bump(b);
    p.x
};
static contains = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    let whole = p.&mut;
    bump(whole.*.y.&mut);
    a.*
};
"#,
        expect![[r#"
            386..392: using `p` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 360..368: this borrow was created here
              note at 424..425: and it is still used here
        "#]],
    );
}

/// A READ of a sibling field is as path-precise as a write of one: the
/// read is lowered against `w.b`, not against the whole of `w`.
#[test]
fn a_read_of_a_sibling_field_leaves_an_exclusive_loan_alone() {
    check_loans(
        r#"
type W = struct { a: usize, b: usize };
static f = fn() -> usize {
    let mut w = W(struct { a = 1, b = 2 });
    let r = w.a.&mut;
    let v = w.b;
    r.* + v
};
"#,
        expect![[r#""#]],
    );
}

/// The back edge. The invalidating mint sits AFTER the stale use in the
/// text, so nothing about reading the body forwards finds it — the loan
/// is live at the mint only because the loop runs again.
#[test]
fn a_loan_live_across_a_loops_back_edge_is_refused() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let mut i = 0;
    loop {
        i = i + get(s);
        bump(n.&mut);
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            266..272: using `n` mutably here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 194..197: this borrow was created here
              note at 249..250: and the loop brings control back round to this use of it
        "#]],
    );
}

/// And a loan dead before the loop starts is not live inside it: what
/// closes a loan over the back edge is a use on the far side of it, not
/// being declared outside.
#[test]
fn a_loan_dead_before_a_loop_does_not_close_over_it() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let mut i = get(s);
    loop {
        bump(n.&mut);
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#""#]],
    );
}

/// A loan handed back through a `return` leaves, and nothing this body
/// does afterwards can reach it — the mint's block returns, so no later
/// access is reachable from it. A loan PARKED in a slot the caller reads
/// later has not left at all: its region reaches a universal, so it is
/// live everywhere, and the root moving on afterwards is a stale view in
/// the caller's hands. The first stays legal (it is `next_line`); the
/// second is refused.
#[test]
fn a_loan_parked_in_a_borrowed_slot_is_not_a_departure() {
    check_loans(
        r#"
type B = struct { v: usize };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static departs = fn::<@a>(b: B.&mut::<@a>, c: bool) -> usize.&::<@a> {
    if c { return b.*.v.&; };
    bump(b.*.v.&mut);
    b.*.v.&
};
static parks = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>) -> () {
    out.* = b.*.v.&;
    bump(b.*.v.&mut);
};
"#,
        expect![[r#"
            353..363: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 335..342: this borrow was created here
        "#]],
    );
}

/// Parking the loan inside a branch changes nothing: execution carries on
/// past the branch, so the store is reachable from the mint and the mint
/// reaches the later access. Both branch forms.
#[test]
fn a_loan_parked_inside_a_branch_is_still_not_a_departure() {
    check_loans(
        r#"
type B = struct { v: usize };
type Flag = enum { On, Off };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static parks_in_an_if = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>, c: bool) -> () {
    if c { out.* = b.*.v.&; };
    bump(b.*.v.&mut);
};
static parks_in_a_match = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>, f: Flag) -> () {
    match f { ::On => { out.* = b.*.v.&; }, ::Off => {}, };
    bump(b.*.v.&mut);
};
"#,
        expect![[r#"
            273..283: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 252..259: this borrow was created here
            462..472: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 425..432: this borrow was created here
        "#]],
    );
}

/// Re-minting into a slot the previous loan is DEAD in is ordinary code:
/// `s` is not live at the reassignment, so the loan it held is not live at
/// the second mint — with or without a loop. A slot carries one region for
/// every value it ever holds, so the old loan's region is live again as
/// soon as the slot is; what keeps the old loan from coming back is that
/// it left scope at the point it went dead.
#[test]
fn re_minting_a_loan_into_a_dead_slot_is_fine() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static slot_no_loop = fn() -> usize {
    let mut a = 1;
    let mut s = a.&mut;
    let x = s.*;
    s = a.&mut;
    let y = s.*;
    x + y
};
static slot_in_a_loop = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&mut;
    let mut i = 0;
    loop {
        bump(s);
        s = n.&mut;
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#""#]],
    );
}

/// A holder reassigned to a loan of ANOTHER local frees the first: `r`'s
/// region is live again after `r = b.&mut`, but `a`'s loan left scope at
/// the point `r` went dead and does not come back. In a straight line
/// and in a loop the write to `a` is fine; a reassignment on one path
/// only keeps `a`'s loan in scope on the other, and the write is refused
/// with the loan's own mint as the companion. (rustc's answer on all
/// three.)
#[test]
fn reassigning_a_holder_frees_the_loan_it_held() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static straight = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    bump(r);
    r = b.&mut;
    a = 5;
    bump(r);
    a + b
};
static witness = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut s = a.&mut;
    let x = s.*;
    s = b.&mut;
    a = 99;
    x + s.*
};
static in_a_loop = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    let mut i: usize = 0;
    loop {
        bump(r);
        r = b.&mut;
        a = a + 1;
        i = i + 1;
        if i > 3 { break a + b; };
    }
};
static conditional = fn(f: bool) -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    bump(r);
    if f { r = b.&mut; };
    a = 5;
    bump(r);
    a + b
};
"#,
        expect![[r#"
            794..795: writing to `a` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 739..745: this borrow was created here
              note at 806..807: and it is still used here
        "#]],
    );
}

/// One report per access expression and loan. The two payload borrows of
/// one match arm share the arm's origin and report once; `a = a + 1`
/// reads at `a` and writes at the assignment, and reports both; a call
/// that reads two borrowed locals reports each at its own name.
#[test]
fn one_report_per_access_expression_and_loan() {
    check_loans(
        r#"
static take2 = fn(x: usize, y: usize) -> usize { x + y };
static read_modify_write = fn() -> usize {
    let mut a = 1;
    let p = a.&mut;
    a = a + 1;
    p.*
};
static two_loans_one_call = fn() -> usize {
    let mut x = 1;
    let mut y = 2;
    let p = x.&mut;
    let q = y.&mut;
    let s = take2(x, y);
    p.* + q.* + s
};
type E = enum { A(usize, usize), B };
static two_payloads_one_arm = fn::<@a>(e: E.&mut::<@a>) -> usize {
    let s = e.*.&;
    let v: usize = match e { ::A(x, y) => x.* + y.*, ::B => 2 };
    match s { ::A(a, b) => v + a.*, ::B => v }
};
"#,
        expect![[r#"
            149..150: reading `a` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 133..139: this borrow was created here
              note at 160..161: and it is still used here
            149..154: writing to `a` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 133..139: this borrow was created here
              note at 160..161: and it is still used here
            307..308: reading `x` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 261..267: this borrow was created here
              note at 318..319: and it is still used here
            310..311: reading `y` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 281..287: this borrow was created here
              note at 324..325: and it is still used here
            501..510: using `e.*.0` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 452..457: this borrow was created here
              note at 534..535: and it is still used here
        "#]],
    );
}

#[test]
fn the_same_two_loans_in_two_locals_are_fine() {
    check_loans(
        r#"
static two_locals = fn() -> usize {
    let mut a = 1;
    let p = a.&mut;
    let x = p.*;
    let q = a.&mut;
    let y = q.*;
    x + y
};
"#,
        expect![[r#""#]],
    );
}

/// Two mints into one slot from the two arms of an `if`: neither arm's
/// mint is reachable from the other's, so nothing conflicts.
#[test]
fn one_slot_filled_from_two_arms_is_fine() {
    check_loans(
        r#"
static f = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let r: usize.&mut::<@_> = if c { n.&mut } else { n.&mut };
    r.*
};
"#,
        expect![[r#""#]],
    );
}

/// The WRITE arm. An assignment goes around every borrow of anything it
/// overlaps, and `let b = n.&; n = 99; b.*` is the shape it exists for:
/// the borrow reads storage the assignment has already overwritten. A bare
/// local and a place, because they are one rule.
#[test]
fn writing_to_the_root_under_a_live_loan_is_refused() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static to_a_local = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    n = 99;
    b.*
};
static through_a_place = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    p.x = 7;
    a.*
};
"#,
        expect![[r#"
            128..130: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 115..118: this borrow was created here
              note at 136..137: and it is still used here
            260..261: writing to `p.x` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 240..248: this borrow was created here
              note at 267..268: and it is still used here
        "#]],
    );
}

/// Its two negative controls: a write to a DISJOINT field is not foreign
/// to the loan at all, and a write after the loan is dead is ordinary code.
#[test]
fn a_write_beside_a_loan_rather_than_over_it_is_fine() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static disjoint_field = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    p.y = 7;
    a.*
};
static after_the_loan_is_dead = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    let out = get(b);
    n = 99;
    out + n
};
"#,
        expect![[r#""#]],
    );
}

/// An assignment to a POINTER is shallow: it replaces what `p` holds and
/// touches nothing `p` pointed at, so a loan through the old pointee is
/// not invalidated — it is ended. Reading through `p` afterwards sees the
/// new pointee and no stale loan.
#[test]
fn reassigning_a_pointer_ends_the_loans_through_it_without_conflict() {
    check_loans(
        r#"
static f = fn() -> usize {
    let mut a: usize = 1;
    let mut b: usize = 2;
    let mut p = a.&mut;
    let r = p.*.&mut;
    p = b.&mut;
    r.* + p.*
};
"#,
        expect![[r#""#]],
    );
}

/// `string_lib.must`'s main loop in miniature, the write arm's corpus
/// negative control. An owner is borrowed, disposed of and REASSIGNED in
/// one iteration, over and over: the move, the assignment and the loop's
/// back edge over both. Clean, because the owner is region-free: a
/// `String` owns its bytes, so the loan taken in the condition is finished
/// by the time the condition is answered.
#[test]
fn reassigning_an_owner_beside_its_dead_loans_is_fine() {
    check_loans(
        r#"
type Text = struct { buf: u8.&raw mut, cap: usize } only move with {
    impl Self {
        len = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.cap };
        drop = fn(s: Self) -> () {
            let Text(struct { buf, cap, .. }) = s;
            unsafe { dealloc_array(buf, cap); };
        };
    }
};
static text_new = fn(n: usize) -> Text {
    let buf = match alloc_array::<u8>(n) { ::Ok(p) => p, ::Err => panic("out of memory") };
    Text(struct { buf, cap = n })
};
static main = fn() -> usize {
    let mut longest = text_new(1);
    let mut i = 1;
    loop {
        if longest.&.len() < i {
            longest.drop();
            longest = text_new(i);
        };
        i = i + 1;
        if i > 3 { break; };
    };
    let out = longest.&.len();
    longest.drop();
    out
};
"#,
        expect![[r#""#]],
    );
}

/// A dot-call's receiver binds LAST (the self-last rule), and MIR lowers
/// it last, so the receiver's inserted reborrow is an access that comes
/// after every argument's mint. `c.bump(c.*.a.&mut)` and its hoisted
/// spelling are one program and get one answer.
#[test]
fn a_receiver_reborrow_comes_after_the_arguments() {
    check_loans(
        r#"
type Cell = struct { a: usize } with {
    impl Self {
        bump = fn::<@b>(v: usize.&mut::<@b>, c: Self.&mut::<@b>) -> usize { c.*.a + v.* };
    }
};
static inline_arg = fn::<@a>(c: Cell.&mut::<@a>) -> usize { c.bump(c.*.a.&mut) };
static hoisted_arg = fn::<@a>(c: Cell.&mut::<@a>) -> usize {
    let v = c.*.a.&mut;
    c.bump(v)
};
"#,
        expect![[r#"
            216..217: using `c` mutably here invalidates a borrow of it that is still live: the borrow is still needed by this very operation
              note at 223..233: this borrow was created here
            327..328: using `c` mutably here invalidates a borrow of it that is still live: the borrow is still needed by this very operation
              note at 311..321: this borrow was created here
        "#]],
    );
}

/// Two arms of one branch never both run, so a use in the `else` arm
/// cannot keep a loan live across a mint in the `then` arm: the use is
/// not reachable from the mint. Both arm orders, both accepted.
#[test]
fn a_use_and_a_mint_in_sibling_arms_do_not_conflict() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static use_in_else = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { bump(n.&mut); 0 } else { get(s) }
};
static use_in_then = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { get(s) } else { bump(n.&mut); 0 }
};
"#,
        expect![[r#""#]],
    );
}

/// A borrow taken THROUGH a borrow is a reborrow of it, and the parent's
/// region covers the child's — so invalidating the root reaches the
/// child. One level, three levels, shared flavour, and the field form.
#[test]
fn a_borrow_through_a_borrow_is_tied_to_its_parent() {
    check_loans(
        r#"
type P = struct { a: usize, b: usize };
static one_level = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&mut;
    let c = b.*.&mut;
    n = 99;
    c.*
};
static three_deep = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&mut;
    let c = b.*.&mut;
    let d = c.*.&mut;
    n = 99;
    d.*
};
static shared_flavor = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    let c = b.*.&;
    n = 99;
    c.*
};
static field_form = fn() -> usize {
    let mut p = P(struct { a = 1, b = 2 });
    let m = p.&mut;
    let c = m.*.a.&mut;
    p.a = 99;
    c.*
};
"#,
        expect![[r#"
            152..154: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 114..120: this borrow was created here
              note at 160..161: and it is still used here
            301..303: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 241..247: this borrow was created here
              note at 309..310: and it is still used here
            425..427: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 393..396: this borrow was created here
              note at 433..434: and it is still used here
            574..576: writing to `p.a` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 532..538: this borrow was created here
              note at 582..583: and it is still used here
        "#]],
    );
}

/// A nested pointer place is spelled in MIR as a copy of the inner
/// pointer into a temp; the loan and the later access are both rooted
/// back at the outer pointer, so they meet.
#[test]
fn a_loan_through_a_nested_pointer_place_is_rooted_at_the_outer_pointer() {
    check_loans(
        r#"
static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize {
    let c = bb.*.*.&mut;
    let d = bb.*.*.&mut;
    c.* + d.*
};
"#,
        expect![[r#"
            106..117: using `bb.*.*` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 81..92: this borrow was created here
              note at 123..124: and it is still used here
        "#]],
    );
}

/// A sibling access through a NESTED pointer. The copy of `bb.*` into
/// the temp that `bb.*.*` is spelled through performs no access of its
/// own, so a loan of `bb.*.*.g` sees only the sibling's own path and
/// stays untouched — read, write, second borrow, or a raw address of the
/// whole record (minting one touches nothing, M08). Under a loan that
/// does overlap the sibling — of `bb.*.*.f` itself, of `bb.*`, of `bb` —
/// the access is refused at the expression the user wrote.
#[test]
fn a_sibling_access_through_a_nested_pointer_is_accepted() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static read = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; let y = bb.*.*.f; x.* = 1; y };
static write = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; bb.*.*.f = 3; x.* = 1; x.* };
static borrow = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; let y = bb.*.*.f.&mut; x.* = 1; y.* = 2; x.* };
static address = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> P.&raw { let x = bb.*.*.g.&mut; let p = bb.*.*.&raw; x.* = 1; p };
"#,
        expect![[""]],
    );
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static under_field = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.f.&mut; let y = bb.*.*.f; x.* = 1; y };
static under_inner = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.&mut; let y = bb.*.*.f; x.*.*.f = 1; y };
static under_outer = fn::<@a, @b>(mut bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.&mut; let y = bb.*.*.f; x.*.*.*.f = 1; y };
"#,
        expect![[r#"
            146..154: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 123..136: this borrow was created here
              note at 156..157: and it is still used here
            271..279: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 252..261: this borrow was created here
              note at 281..282: and it is still used here
            402..410: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 385..392: this borrow was created here
              note at 412..413: and it is still used here
        "#]],
    );
}

/// A write THROUGH a place temp still reads what it writes. The arm that
/// drops a defining copy's access covers the temp ITSELF — `t = copy p`,
/// the copy that makes `t` stand for `p` — and nothing else: once the
/// destination is projected (`bb.*.* = v` writes through the temp `bb.*`
/// stands for), the statement is an ordinary write whose right-hand side
/// is read like any other operand. Drop the `dest.projection.is_empty()`
/// half of that guard and this program is silently accepted.
#[test]
fn a_write_through_a_place_temp_still_reads_its_source() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static f = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>, mut v: P) -> usize {
    let y = v.f.&mut;
    bb.*.* = v;
    y.* = 1;
    y.*
};
"#,
        expect![[r#"
            150..151: reading `v` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 127..135: this borrow was created here
              note at 157..158: and it is still used here
        "#]],
    );
}

/// A borrowed `match` reads its scrutinee's DEREF, not the borrow itself.
/// The tag lives behind `p`, so dispatching on `p` while a `.&mut` of
/// `p.*` is live is a read of `p.*`, and the refusal says so — naming a
/// move of `p` instead would name an operation the program never performs
/// and point the writer at the wrong thing to change.
#[test]
fn a_borrowed_match_reads_the_scrutinee_through_its_deref() {
    check_loans(
        r#"
type E = enum { A, B };
static f = fn::<@a>(p: E.&mut::<@a>) -> usize {
    let s = p.*.&mut;
    let v: usize = match p { ::A => 1, ::B => 2 };
    s.* = ::A;
    v
};
"#,
        expect![[r#"
            120..121: reading `p.*` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 85..93: this borrow was created here
              note at 150..151: and it is still used here
        "#]],
    );
}

/// Where the blame sits: an argument is read into a temp at its own
/// expression, projected or bare, and the read is squiggled.
#[test]
fn an_argument_blames_its_own_read() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static take = fn(v: usize) -> usize { v };
static projected = fn::<@a>(p: P.&mut::<@a>) -> usize { let x = p.*.f.&mut; let n = take(p.*.f); x.* = n; 0 };
static bare = fn() -> usize { let mut a: usize = 1; let x = a.&mut; let n = take(a); x.* = n; 0 };
"#,
        expect![[r#"
            173..178: reading `p.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 148..158: this borrow was created here
              note at 181..182: and it is still used here
            276..277: reading `a` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 255..261: this borrow was created here
              note at 280..281: and it is still used here
        "#]],
    );
}

/// A tail-position read through a borrow is one access, reported once
/// at the read the user wrote. The temp it is read into, and the return
/// slot that temp is copied to, are values, not places: no loan is rooted
/// in either, so neither the copy nor the return is a second access.
#[test]
fn a_tail_read_through_a_borrow_reports_once_at_the_read() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static g1 = fn::<@a, @b>(out: usize.&mut::<@a>.&mut::<@b>, r: usize.&mut::<@a>) -> usize { out.* = r; r.* };
static g2 = fn::<@a, @b>(out: P.&mut::<@a>.&mut::<@b>, r: P.&mut::<@a>) -> usize { out.* = r; r.*.f };
static g3 = fn::<@a, @b>(out: P.&mut::<@a>.&mut::<@b>, r: P.&mut::<@a>) -> P { out.* = r; r.* };
static h = fn::<@a, @b>(out: usize.&mut::<@a>.&mut::<@b>, r: usize.&mut::<@a>) -> usize { out.* = r; let v = r.*; v };
"#,
        expect![[r#"
            143..146: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 140..141: this borrow was created here
            244..249: reading `r.*.f` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 241..242: this borrow was created here
            343..346: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 340..341: this borrow was created here
            459..462: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 448..449: this borrow was created here
        "#]],
    );
}

/// The companion names why the loan is live. A flow into a universal
/// reaching the access is always the true reason, so it wins over a
/// holder's next use: after `r = q.*.g.&mut`, `r.*` reads `q`'s loan,
/// not `m`'s, and the note must not send the reader there.
#[test]
fn a_loan_handed_back_says_so_over_a_reassigned_holders_use() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static reassigned = fn::<@a, @b>(m: P.&mut::<@a>, out: usize.&::<@a>.&mut::<@b>, q: P.&::<@a>) -> usize {
    let mut r = m.*.f.&;
    out.* = r;
    r = q.*.g.&;
    m.*.f = 5;
    r.*
};
static held = fn::<@a, @b>(m: P.&mut::<@a>, out: usize.&::<@a>.&mut::<@b>) -> usize {
    let r = m.*.f.&;
    out.* = r;
    m.*.f = 5;
    r.*
};
"#,
        expect![[r#"
            216..217: writing to `m.*.f` here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 163..170: this borrow was created here
            364..365: writing to `m.*.f` here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 328..335: this borrow was created here
        "#]],
    );
}

/// A field that HOLDS a function is a place, not a member path: `b.f(4)`
/// reads `b.f` and nothing else, so it leaves a loan of `b.v` alone.
#[test]
fn a_call_through_a_function_valued_field_reads_only_that_field() {
    check_loans(
        r#"
type Box = struct { v: usize, f: fn(usize) -> usize };
static twice = fn(n: usize) -> usize { n + n };
static main = fn() -> usize {
    let mut b = Box(struct { v = 1, f = twice });
    let r = b.v.&mut;
    let y = b.f(4);
    r.* + y
};
"#,
        expect![[r#""#]],
    );
}

/// A loop-carried slot initialised from a DIFFERENT root: no loan of `n`
/// exists before the loop, and the in-loop mint is followed by the write
/// in the same iteration, with the slot read on the next one.
#[test]
fn a_loop_carried_slot_initialised_from_another_root_is_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let mut other: usize = 7;
    let mut s = other.&mut;
    let mut i = 0;
    loop {
        bump(s);
        s = n.&mut;
        n = 99;
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            261..263: writing to `n` here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 241..247: this borrow was created here
              note at 225..226: and the loop brings control back round to this use of it
        "#]],
    );
}

/// The same loop-carried shape with a NON-borrow pre-loop init: an `Opt`
/// slot that starts `::None` and only ever holds a borrow from inside the
/// loop. The loan sits inside the slot's payload, and the slot is live
/// across the back edge.
#[test]
fn a_loop_carried_option_slot_starting_none_is_refused() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn() -> usize {
    let mut n: usize = 1;
    let mut s: Opt::<usize.&::<@_>> = Opt::<usize.&::<@_>>::None;
    let mut i = 0;
    loop {
        i = i + match s { ::Some(r) => r.*, ::None => 0 };
        s = Opt::<usize.&::<@_>>::Some(n.&);
        n = 99;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            306..308: writing to `n` here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 288..291: this borrow was created here
              note at 212..213: and the loop brings control back round to this use of it
        "#]],
    );
}

/// The back edge makes two SIBLING ARMS both run, in different iterations:
/// an iteration taking the minting arm hands the loan to an iteration
/// taking the invalidating one. Both arm orders and the `match` spelling.
#[test]
fn sibling_arms_inside_a_loop_are_not_alternatives_across_iterations() {
    check_loans(
        r#"
type Flag = enum { On, Off };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static then_mint = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        if c { s = n.&; } else { n = 99; };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static else_mint = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        if c { n = 99; } else { s = n.&; };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static match_mint = fn(f: Flag) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        match f { ::On => { s = n.&; }, ::Off => { n = 99; }, };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            245..247: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 173..176: this borrow was created here
              note at 272..273: and it is still used here
            454..456: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 400..403: this borrow was created here
              note at 499..500: and it is still used here
            718..720: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 628..631: this borrow was created here
              note at 748..749: and it is still used here
        "#]],
    );
}

/// The two controls that shape needs: with no arms at all the loop shape
/// is refused (nothing about arms is what catches it), and with no loop
/// the arms really are alternatives and the write in one is fine.
#[test]
fn sibling_arms_are_alternatives_only_outside_a_loop() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static no_arms = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        s = n.&;
        n = 99;
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static no_loop = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { n = 99; 0 } else { get(s) }
};
"#,
        expect![[r#"
            198..200: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 181..184: this borrow was created here
              note at 222..223: and it is still used here
        "#]],
    );
}

/// Across a back edge the mint precedes the access in TIME however the
/// text reads: iteration `i` mints, iteration `i+1` invalidates and then
/// reads. Three ways the loan reaches the slot — directly, through a
/// call, and through a member dot-call.
#[test]
fn a_mint_below_its_invalidator_still_conflicts_across_the_back_edge() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static id = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> { x };
type W = struct { v: usize } with {
    impl Self {
        lend = fn::<@b>(w: Self.&::<@b>) -> usize.&::<@b> { w.*.v.& };
    }
};
static plain = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        n = 99;
        i = i + get(s);
        s = n.&;
        if i > 3 { break i; };
    }
};
static via_call = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        n = 99;
        i = i + get(s);
        s = id(n.&);
        if i > 3 { break i; };
    }
};
static via_member = fn() -> usize {
    let mut w = W(struct { v = 1 });
    let mut s = w.&.lend();
    let mut i = 0;
    loop {
        w.v = 99;
        i = i + get(s);
        s = w.&.lend();
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            374..376: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 327..330: this borrow was created here
              note at 398..399: and it is still used here
            582..584: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 535..538: this borrow was created here
              note at 606..607: and it is still used here
            816..818: writing to `w.v` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 760..763: this borrow was created here
              note at 840..841: and it is still used here
        "#]],
    );
}

/// A CALL is a use of every loan flowing into it: the temps holding the
/// arguments are live until the call. `two(n.&mut, n.&mut)` is the
/// textbook two-exclusive-borrows error, refused whether the parameters
/// take independent regions or one, and however the mints nest.
#[test]
fn a_call_is_a_use_of_every_loan_passed_to_it() {
    check_loans(
        r#"
static two = fn::<@a, @b>(x: usize.&mut::<@a>, y: usize.&mut::<@b>) -> usize { x.* + y.* };
static one = fn::<@a>(x: usize.&mut::<@a>, y: usize.&mut::<@a>) -> usize { x.* + y.* };
static id = fn::<@a>(x: usize.&mut::<@a>) -> usize.&mut::<@a> { x };
static flat = fn() -> usize { let mut n: usize = 1; two(n.&mut, n.&mut) };
static nested_first = fn() -> usize { let mut n: usize = 1; two(id(n.&mut), n.&mut) };
static nested_second = fn() -> usize { let mut n: usize = 1; two(n.&mut, id(n.&mut)) };
static both_nested = fn() -> usize { let mut n: usize = 1; two(id(n.&mut), id(n.&mut)) };
static nested_let = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    two(a, n.&mut)
};
static shared_binder = fn() -> usize { let mut n: usize = 1; one(n.&mut, n.&mut) };
"#,
        expect![[r#"
            314..320: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 306..312: this borrow was created here
              note at 302..321: and it is still used here
            401..407: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 392..398: this borrow was created here
              note at 385..408: and it is still used here
            488..494: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 477..483: this borrow was created here
              note at 473..496: and it is still used here
            578..584: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 566..572: this borrow was created here
              note at 559..586: and it is still used here
            683..689: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 664..670: this borrow was created here
              note at 676..690: and it is still used here
            767..773: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 759..765: this borrow was created here
              note at 755..774: and it is still used here
        "#]],
    );
}

/// `examples/borrows.must` says of its join fixture that borrowing `n`
/// both ways at once is exactly the exclusivity violation the static
/// checker rejects. The example itself borrows two DIFFERENT locals; this
/// is the claim, verified.
#[test]
fn the_borrows_example_prose_claim_holds() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static pick_flavors = fn::<@a>(x: usize.&::<@a>, m: usize.&mut::<@a>, c: bool) -> usize {
    get(if c { x } else { m })
};
static both_ways = fn() -> usize {
    let mut n: usize = 1;
    pick_flavors(n.&, n.&mut, true)
};
"#,
        expect![[r#"
            266..272: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 261..264: this borrow was created here
              note at 248..279: and it is still used here
        "#]],
    );
}

/// Matching a borrowed scrutinee binds BORROWS of the payloads (M13): each
/// binder is a loan rooted at the scrutinee's borrow, so writing to the
/// matched value while a binder is still needed is refused, and finishing
/// with the binder first is fine.
#[test]
fn a_payload_borrow_dies_when_the_matched_value_is_written() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static stale = fn() -> usize {
    let mut o = Opt::<usize>::Some(1);
    match o.&mut {
        ::Some(x) => { o = Opt::<usize>::None; x.* },
        ::None => 0,
    }
};
static finished = fn() -> usize {
    let mut o = Opt::<usize>::Some(1);
    match o.&mut {
        ::Some(x) => { let v = x.*; o = Opt::<usize>::None; v },
        ::None => 0,
    }
};
"#,
        expect![[r#"
            157..175: writing to `o` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 121..127: this borrow was created here
              note at 177..178: and it is still used here
        "#]],
    );
}

/// `Return` is the storage end of every local. A nested literal returning
/// a borrow of its own local at `@_` reaches no universal of the
/// enclosing item, so the outlives check is silent; the loan is live
/// where the literal's body returns, which is what escapes.
#[test]
fn a_nested_literal_borrow_of_its_own_local_cannot_escape() {
    check_loans(
        "static main = fn () -> usize {\n\
             let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };\n\
             f().*\n\
         };",
        expect![[r#"
            79..82: borrowed value does not live long enough: this borrows a local, but the borrow is still live when the body returns and the local is gone by then
              note at 83..84: the storage ends here, where the body returns
        "#]],
    );
}

/// The conditional-return case. `map.get(key)`'s loan is handed back
/// from the `::Some` arm, which ties it to `@a` — at that point, and not
/// before: the `::None` arm is not reachable from the flow, so the loan is
/// not live there and `insert` may touch the map. Location-insensitive
/// liveness (NLL) refuses this program; `examples/reborrow.must` is built
/// on it.
#[test]
fn a_loan_handed_back_from_one_arm_is_not_live_in_the_other() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
type Map = struct { k: usize, v: usize, used: bool } with {
    impl Self {
        get = fn::<@b>(key: usize, m: Self.&mut::<@b>) -> Opt::<usize.&mut::<@b>> {
            if m.*.used == true { if m.*.k == key { return ::Some(m.*.v.&mut); }; };
            ::None
        };
        insert = fn::<@b>(key: usize, val: usize, m: Self.&mut::<@b>) -> () {
            m.*.k = key;
            m.*.v = val;
            m.*.used = true;
        };
    }
};
static get_or_default = fn::<@a>(key: usize, val: usize, map: Map.&mut::<@a>) -> usize.&mut::<@a> {
    match map.get(key) {
        ::Some(v) => v,
        ::None => {
            map.insert(key, val);
            match map.get(key) { ::Some(v) => v, ::None => panic("just inserted") }
        },
    }
};
"#,
        expect![""],
    );
}

/// The same shape flowing into a SLOT with a body-local region instead of
/// into the signature: the slot is live in both arms (it is read after the
/// match) and its region is covered wherever it is live, so the loan
/// stored in one arm counts as live in the other, on a path where it was
/// never stored. Refused — the hop into a body region is not dated, only
/// the hop into a universal is; this is the over-refusal M19 records.
#[test]
fn a_loan_stored_in_a_live_slot_in_one_arm_is_live_in_the_other() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
type Map = struct { k: usize, v: usize, used: bool } with {
    impl Self {
        get = fn::<@b>(key: usize, m: Self.&mut::<@b>) -> Opt::<usize.&mut::<@b>> {
            if m.*.used == true { if m.*.k == key { return ::Some(m.*.v.&mut); }; };
            ::None
        };
        insert = fn::<@b>(key: usize, val: usize, m: Self.&mut::<@b>) -> () {
            m.*.k = key;
            m.*.v = val;
            m.*.used = true;
        };
    }
};
static f = fn::<@a>(key: usize, val: usize, map: Map.&mut::<@a>, slot: usize.&mut::<@a>) -> usize {
    let mut r: usize.&mut::<@_> = slot;
    match map.get(key) {
        ::Some(v) => { r = v; },
        ::None => { map.insert(key, val); },
    };
    r.*
};
"#,
        expect![[r#"
            711..714: using `map` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 643..646: this borrow was created here
              note at 747..748: and it is still used here
        "#]],
    );
}

/// G24: a name is read where it stands, so the read of `x` comes before
/// the write the right operand makes.
#[test]
fn a_name_is_read_before_a_later_operand_writes_it() {
    check_mir(
        r#"
static f = fn() -> usize {
    let mut x: usize = 1;
    let y = x + { x = 5; 1 };
    y
};
"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize
              _3: usize
              _4: usize  // y
              _5: usize
              bb0:
                _1 = 1
                _2 = _1
                _1 = 5
                _3 = Add(_2, 1)
                _4 = _3
                _5 = _4
                _0 = _5
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

/// A deref goes through the temp that read the pointer's name, and the
/// temp stands for the name: the loan and the access both root in `p`.
#[test]
fn a_name_temp_does_not_hide_a_loan() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static read = fn::<@a>(p: P.&mut::<@a>) -> usize { let x = p.*.f.&mut; let y = p.*.f; x.* = 1; y };
static write = fn::<@a>(p: P.&mut::<@a>) -> usize { let x = p.*.f.&mut; p.*.f = 3; x.* = 1; x.* };
static whole = fn() -> usize { let mut n: usize = 1; let x = n.&mut; let y = n; x.* = 2; y };
"#,
        expect![[r#"
            120..125: reading `p.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 100..110: this borrow was created here
              note at 127..128: and it is still used here
            221..222: writing to `p.*.f` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 201..211: this borrow was created here
              note at 224..225: and it is still used here
            317..318: reading `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 301..307: this borrow was created here
              note at 320..321: and it is still used here
        "#]],
    );
}

#[test]
fn a_name_temp_does_not_invent_a_loan() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static set = fn::<@a>(v: usize, m: usize.&mut::<@a>) -> () { m.* = v; };
static sibling = fn::<@a>(p: P.&mut::<@a>) -> usize { let x = p.*.g.&mut; let y = p.*.f; x.* = 1; y };
static copied = fn() -> usize { let mut x: usize = 1; let y = x; x = 2; x + y };
static beside = fn() -> usize { let a: usize = 1; let mut b: usize = 2; set(a, b.&mut); a + b };
"#,
        expect![[""]],
    );
}

/// A join with one exit copies the name temp whole, and a deref through
/// the join still roots in the name.
#[test]
fn a_single_exit_join_under_a_deref_roots_in_the_name() {
    check_loans(
        r#"
static looped = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize {
    let x = loop { break bb; }.*.*.&mut;
    let y = bb.*.*;
    x.* = 1;
    y
};
static matched = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>, k: usize) -> usize {
    let x = match k { _ => bb }.*.*.&mut;
    let y = bb.*.*;
    x.* = 1;
    y
};
static nested = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize {
    let x = bb.*.*.&mut;
    let y = loop { break loop { break bb; }; }.*.*;
    x.* = 1;
    y
};
"#,
        expect![[r#"
            127..133: reading `bb.*.*` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 86..113: this borrow was created here
              note at 139..140: and it is still used here
            295..301: reading `bb.*.*` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 253..281: this borrow was created here
              note at 307..308: and it is still used here
            435..473: reading `bb.*.*` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 410..421: this borrow was created here
              note at 479..480: and it is still used here
        "#]],
    );
}

/// The read of `x` stands between the mint of `r` and its use in the
/// next argument, and is refused at the name.
#[test]
fn an_argument_read_under_a_loan_a_later_argument_uses_is_refused() {
    check_loans(
        r#"
static pack = fn(a: usize, b: usize) -> usize { a * 10 + b };
static f = fn() -> usize {
    let mut x: usize = 1;
    let r = x.&mut;
    pack(x, { r.* = 5; 1 })
};
"#,
        expect![[r#"
            145..146: reading `x` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 128..134: this borrow was created here
              note at 150..151: and it is still used here
        "#]],
    );
}

/// A borrow of a local that was moved out, in a later argument or a later
/// statement, is refused as a use of the consumed binding.
#[test]
fn a_borrow_of_a_moved_out_local_is_refused() {
    check_loans(
        r#"
type Tok = struct { n: usize } only move with {
    impl Self {
        done = fn(t: Self) -> usize { let Tok(struct { n }) = t; n };
    }
};
static take = fn::<@a>(v: Tok, r: Tok.&::<@a>) -> usize { let k = r.*.n; v.done() + k };
static argument = fn() -> usize { let v = Tok(struct { n = 3 }); take(v, v.&) };
static statement = fn() -> usize { let v = Tok(struct { n = 3 }); let w = v; let r = v.&; take(w, r) };
static pair = fn::<@a, T>(v: T, r: T.&::<@a>) -> T { v };
static generic_argument = fn::<T>(m: T) -> T { pair(m, m.&) };
static generic_statement = fn::<T>(m: T) -> T { let w = m; let r = m.&; pair(w, r) };
"#,
        expect![[r#"
            306..307: `v` was already consumed
              note at 271..272: `v` is born here and must be consumed
              note at 303..304: first consumed here
            399..400: `v` was already consumed
              note at 353..354: `v` is born here and must be consumed
              note at 388..389: first consumed here
            531..532: `m` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use
              note at 510..511: `m` is born here and must be consumed
              note at 528..529: first consumed here
            606..607: `m` was already consumed: a value of `T` may not be duplicated, and no bound grants copying — borrow it at the first use
              note at 574..575: `m` is born here and must be consumed
              note at 595..596: first consumed here
        "#]],
    );
}

/// An index operand that writes through the pointer, or beside it, leaves
/// the pointer the place was read through in place, and is accepted.
#[test]
fn an_index_operand_that_leaves_the_pointer_in_place_is_accepted() {
    check_loans(
        r#"
static through = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut p = a.&mut;
    p.*[{ p.*[1] = 5; 0 }]
};
static beside = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut s = struct { p = a.&mut, q: usize = 0 };
    s.p.*[{ s.q = 1; 0 }]
};
static after = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0];
    p = b.&mut;
    x + p.*[0]
};
"#,
        expect![[""]],
    );
}

/// An index operand that reassigns the pointer runs after the pointer was
/// read, so the reassignment is refused, in the read, the write target and
/// the address-of walks, and one deref deeper.
#[test]
fn an_access_through_a_pointer_its_index_reassigns_is_refused() {
    check_loans(
        r#"
static read = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0].&mut;
    let q = b.&mut;
    let y = p.*[{ p = q; 0 }];
    x.* = 7;
    y
};
static write = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0].&mut;
    let q = b.&mut;
    p.*[{ p = q; 0 }] = 9;
    x.* = 7;
    x.*
};
static address = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0].&mut;
    let q = b.&mut;
    let z = p.*[{ p = q; 0 }].&mut;
    z.* = 9;
    x.* = 7;
    x.*
};
static double = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut c: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let mut bb = p.&mut;
    let r = bb.*.*.&mut;
    let v = bb.*.*[{ bb.* = c.&mut; 0 }];
    r.*[0] = 9;
    v
};
"#,
        expect![[r#"
            194..195: writing to `p` here replaces a pointer the place around it has already been read through: the access that place makes afterwards would go through the old pointer
              note at 184..201: the place is accessed here, through the old pointer
            411..412: writing to `p` here replaces a pointer the place around it has already been read through: the access that place makes afterwards would go through the old pointer
              note at 421..422: the place is accessed here, through the old pointer
            644..645: writing to `p` here replaces a pointer the place around it has already been read through: the access that place makes afterwards would go through the old pointer
              note at 634..656: the place is accessed here, through the old pointer
            901..907: writing to `bb.*` here replaces a pointer the place around it has already been read through: the access that place makes afterwards would go through the old pointer
              note at 885..913: the place is accessed here, through the old pointer
        "#]],
    );
}

#[test]
fn loan_traps_sit_before_the_refused_operation() {
    // Two refused reads in one block, which splits it twice.
    check_loan_traps(
        r#"
static f = fn() -> usize {
    let mut n: usize = 1;
    let r = n.&mut;
    let a = n;
    let b = n;
    r.* = 2;
    a + b
};
"#,
        expect![[r#"
            Some(86..87) statement: reading `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
            Some(101..102) statement: reading `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
        "#]],
    );
    // An escape, trapped at the borrow.
    check_loan_traps(
        r#"
static main = fn () -> usize {
    let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };
    f().*
};
"#,
        expect![[r#"
            Some(84..87) statement: borrowed value does not live long enough: this borrows a local, but the borrow is still live when the body returns and the local is gone by then
        "#]],
    );
}
