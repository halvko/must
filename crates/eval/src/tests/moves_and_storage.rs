//! Moves, temporaries and ended storage.

use super::*;

// ---- moving is an invalidation event ------------------------------------

#[test]
fn moving_a_value_invalidates_borrows_of_it_exactly_as_writing_does() {
    // The aliasing model's event is "the storage no longer holds what the
    // borrow was taken of", and a MOVE is that event as surely as a write
    // is. Without this, a borrow of a value that was then consumed read
    // stale bytes with nothing to say about it — while its write-shaped
    // twin (below) was caught, which is what made the gap a bug rather
    // than a limit.
    check_run_past_the_cfg_checkers(
        r#"
type Lin = struct { id: usize } only move with {
    impl Self {
        eat = fn(s: Self) -> () { let Lin(struct { id }) = s; };
    }
};
static f = fn() -> usize {
    let s = Lin(struct { id = 1 });
    let b = s.&;
    s.eat();
    b.*.id
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_write_shaped_twin_of_a_move_invalidation_says_the_same_thing() {
    check_run_past_the_cfg_checkers(
        r#"
type Id = struct { id: usize };
static f = fn() -> usize {
    let mut s = Id(struct { id = 1 });
    let b = s.&;
    s = Id(struct { id = 2 });
    b.*.id
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_move_with_no_borrow_outstanding_costs_nothing_and_still_runs() {
    // The invalidation is one map lookup on an address-taken local, so a
    // body that never borrows pays for the distinction with nothing.
    check_run(
        r#"
type Lin = struct { id: usize } only move with {
    impl Self {
        into_id = fn(s: Self) -> usize { let Lin(struct { id }) = s; id };
    }
};
static f = fn() -> usize {
    let s = Lin(struct { id = 7 });
    let t = s;
    t.into_id()
};
"#,
        "f()",
        expect![[r#"
            => 7
        "#]],
    );
}

/// A raw read of a local whose value was moved out reads uninitialized
/// storage: detected UB, with the move as the note.
#[test]
fn a_raw_read_of_moved_out_storage_is_detected() {
    check_run(
        r#"
type Tok = struct { n: usize } only move with {
    impl Self {
        done = fn(t: Self) -> usize { let Tok(struct { n }) = t; n };
    }
};
static f = fn() -> usize {
    let v = Tok(struct { n = 3 });
    let p = v.&raw;
    let w = v;
    let k = w.done();
    k + unsafe { p.*.n }
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — the local's value was moved out
              note: its value was moved out here
        "#]],
    );
}

/// A borrow taken after the move, which the checker refuses, traps before
/// it can read the moved-out storage.
#[test]
fn a_borrow_of_moved_out_storage_traps_before_it_is_read() {
    check_run(
        r#"
type Tok = struct { n: usize } only move with {
    impl Self {
        done = fn(t: Self) -> usize { let Tok(struct { n }) = t; n };
    }
};
static f = fn() -> usize {
    let v = Tok(struct { n = 3 });
    let w = v;
    let r = v.&;
    let k = w.done();
    k + r.*.n
};
"#,
        "f()",
        expect![[r#"
            error[Trap]: `v` was already consumed
        "#]],
    );
}

/// A whole write gives moved-out storage a value again, by name or
/// through a pointer to the whole local.
#[test]
fn a_whole_write_gives_moved_out_storage_its_value_back() {
    check_run(
        r#"
type Tok = struct { n: usize } only move with {
    impl Self {
        done = fn(t: Self) -> usize { let Tok(struct { n }) = t; n };
    }
};
static by_name = fn() -> usize {
    let mut v = Tok(struct { n = 3 });
    let p = v.&raw;
    let k = v.done();
    v = Tok(struct { n = 4 });
    let j = unsafe { p.*.n };
    k + j + v.done()
};
static through = fn() -> usize {
    let mut v = Tok(struct { n = 3 });
    let p = v.&raw mut;
    let k = v.done();
    unsafe { p.* = Tok(struct { n = 5 }); };
    k + unsafe { p.*.n }
};
static both = fn() -> usize { by_name() * 100 + through() };
"#,
        "both()",
        expect![[r#"
            => 1108
        "#]],
    );
}

/// A write into part of moved-out storage has no whole value to land in:
/// detected UB.
#[test]
fn a_field_write_into_moved_out_storage_is_detected() {
    check_run(
        r#"
type Tok = struct { n: usize } only move with {
    impl Self {
        done = fn(t: Self) -> usize { let Tok(struct { n }) = t; n };
    }
};
static f = fn() -> usize {
    let mut v = Tok(struct { n = 3 });
    let p = v.&raw mut;
    let k = v.done();
    unsafe { p.*.n = 5; };
    k
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — the local's value was moved out
              note: its value was moved out here
        "#]],
    );
}

// ---- materialized temporaries (M12) --------------------------------------

/// A write through a `.&mut` of a temporary is visible to the code holding
/// the borrow.
#[test]
fn a_temporary_borrowed_exclusively_is_written_through() {
    check_run(
        r#"
type Printer = struct { count: usize } with {
    impl Self {
        emit = fn::<@a>(s: str, p: Self.&mut::<@a>) -> () {
            print(s);
            p.*.count = p.*.count + 1;
        };
    }
};
type Inner = struct { label: str } with {
    impl Self {
        fmt = fn::<@a>(p: Printer.&mut::<@a>, i: Self) -> usize {
            p.emit(i.label);
            p.emit(i.label);
            p.*.count
        };
    }
};
static f = fn() -> usize {
    let inner = Inner(struct { label = "x" });
    inner.fmt(Printer(struct { count = 0 }).&mut)
};
"#,
        "f()",
        expect![[r#"
            output: "xx"
            => 2
        "#]],
    );
}

/// A branch's temporary is created only when that branch runs, and a
/// loop's is created afresh every iteration.
#[test]
fn a_temporary_in_a_branch_or_a_loop_is_created_per_path() {
    check_run(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        bump = fn::<@a>(by: usize, c: Self.&mut::<@a>) -> usize {
            c.*.v = c.*.v + by;
            c.*.v
        };
    }
};
static pick = fn(c: bool) -> usize {
    if c { Cell(struct { v = 10 }).&mut.bump(1) }
    else { Cell(struct { v = 20 }).&mut.bump(2) }
};
static f = fn() -> usize {
    let mut total = 0;
    let mut i = 0;
    loop {
        if i == 3 { break; };
        total = total + Cell(struct { v = i }).&mut.bump(1);
        i = i + 1;
    };
    total + pick(true) + pick(false)
};
"#,
        "f()",
        expect![[r#"
            => 39
        "#]],
    );
}

/// A borrow of a temporary's FIELD is written through and read back like
/// a borrow of the whole value.
#[test]
fn a_borrow_of_a_temporarys_field_names_the_temporarys_storage() {
    check_run(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> Pair { Pair(struct { a = 3, b = 4 }) };
static set = fn::<@a>(v: usize, r: usize.&mut::<@a>) -> usize {
    r.* = v;
    r.*
};
static f = fn() -> usize { set(9, mk().a.&mut) };
"#,
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

/// A temporary is evaluated where it is written: `a()` runs before
/// `mk()`, so the program prints `am`.
#[test]
fn materializing_a_temporary_does_not_reorder_the_operands_around_it() {
    check_run(
        r#"
static a = fn() -> usize { print("a"); 1 };
static mk = fn() -> usize { print("m"); 2 };
static add = fn::<@x>(l: usize, r: usize.&::<@x>) -> usize { l + r.* };
static f = fn() -> usize { add(a(), mk().&) };
"#,
        "f()",
        expect![[r#"
            output: "am"
            => 3
        "#]],
    );
}

/// A temporary in a CONST context is an ordinary local there too: the
/// compile-time evaluator allocates it, borrows it and reads through the
/// borrow, in a `static`'s initializer and in a `const` block alike.
#[test]
fn a_temporary_can_be_borrowed_at_compile_time() {
    check_const(
        r#"
static get = const fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static mk = const fn() -> usize { 7 };
static folded: usize = get(mk().&);
static in_const_block = const { get(mk().&) };
"#,
        expect![[r#"
            get = fn
            mk = fn
            folded = 7
            in_const_block = 7
        "#]],
    );
}

/// `.&raw` of a temporary runs against the same storage `.&` would get:
/// `mkp().a.&raw mut` writes into the temporary's own field, not a copy
/// of it, and the read through `p` sees the materialized value.
#[test]
fn a_raw_pointer_to_a_temporary_reads_and_writes_its_storage() {
    check_run(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> usize { 7 };
static mkp = fn() -> Pair { Pair(struct { a = 1, b = 2 }) };
static main = fn() -> usize {
    let p = mk().&raw;
    let q = mkp().a.&raw mut;
    unsafe {
        q.* = 9;
        p.* + q.*
    }
};
"#,
        "main()",
        expect![[r#"
            => 16
        "#]],
    );
}

/// A raw pointer to a temporary of a block that has ended is dangling:
/// the deref is detected UB, with the block exit that ended the storage
/// as its note.
#[test]
fn a_raw_pointer_to_a_temporary_of_an_ended_block_is_detected_dangling() {
    check_run(
        r#"
static mk = fn() -> usize { 2 };
static f = fn() -> usize {
    let p = { mk().&raw };
    unsafe { p.* }
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its block has ended)
              note: its storage ended here, when its block was left
        "#]],
    );
}

/// The named-local twin: a block's `let` dies with the block exactly as
/// a temporary does, and a raw pointer to it dangles the same way.
#[test]
fn a_raw_pointer_to_a_local_of_an_ended_block_is_detected_dangling() {
    check_run(
        r#"
static f = fn() -> usize {
    let p = {
        let x: usize = 2;
        x.&raw
    };
    unsafe { p.* }
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its block has ended)
              note: its storage ended here, when its block was left
        "#]],
    );
}

/// A match arm's binder dies with the arm: a raw pointer to it, kept
/// past the `match`, is dangling at the deref.
#[test]
fn a_raw_pointer_to_a_match_binder_dangles_after_the_match() {
    check_run(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let mut p = a.&raw;
    match o {
        ::Some(x) => { p = x.&raw; },
        ::None => {},
    };
    unsafe { p.* }
};
"#,
        "f(Opt::Some(2))",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its arm has ended)
              note: its storage ended here, when its arm was left
        "#]],
    );
}

/// A loop body's local is fresh storage every iteration: each iteration
/// takes the address of its own `x` and reads it back before the body
/// ends, so the total is the sum of the iterations' values.
#[test]
fn a_loop_body_local_gets_fresh_storage_each_iteration() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut i: usize = 0;
    let mut total: usize = 0;
    loop {
        if i == 3 { break; };
        let x: usize = i;
        let p = x.&raw;
        total = total + unsafe { p.* };
        i = i + 1;
    };
    total
};
"#,
        "f()",
        expect![[r#"
            => 3
        "#]],
    );
}

/// Fresh storage means fresh identity: a raw pointer to one iteration's
/// `x`, kept across the `continue`, dangles once the next iteration has
/// initialized its own `x` — the old allocation is not reused.
#[test]
fn a_raw_pointer_kept_across_a_continue_dangles_in_the_next_iteration() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut i: usize = 0;
    let mut keep: usize = 0;
    let mut p = keep.&raw;
    loop {
        if i == 2 { break; };
        let x: usize = i;
        if i == 1 {
            keep = unsafe { p.* };
        };
        p = x.&raw;
        i = i + 1;
        continue;
    };
    keep
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its block has ended)
              note: its storage ended here, when its block was left
        "#]],
    );
}

/// The const evaluator follows the same rule: a deref through a raw
/// pointer to a block's ended storage is UB at compile time too.
#[test]
fn a_const_context_detects_a_raw_pointer_to_ended_block_storage() {
    check_const(
        r#"
static mk = const fn() -> usize { 2 };
static v: usize = const {
    let p = { mk().&raw };
    unsafe { p.* }
};
"#,
        expect![[r#"
            mk = fn
            v = error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its block has ended)
        "#]],
    );
}
