//! Temporaries and storage that ends with its block, arm or iteration.

use super::*;

// ---- materialized temporaries (M12) --------------------------------------

/// A materialized temporary lowers to one nameless local (`_2`), assigned
/// once, and the borrow points into its own field (`& _2.1`).
#[test]
fn a_borrowed_temporary_lowers_to_one_addressable_local() {
    check_mir(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> Pair { Pair(struct { a = 1, b = 2 }) };
static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };
static f = fn() -> usize { get(mk().b.&) };
"#,
        expect![[r#"
            item Pair:
            item mk:
            fn b0() -> Pair {
              _0: Pair  // return
              _1: struct { a: usize, b: usize }
              bb0:
                _1 = { a: 1, b: 2 }
                _0 = _1
                return
            }
            fn b1() -> fn() -> Pair {
              _0: fn() -> Pair  // return
              bb0:
                _0 = fn b0
                return
            }
            item get:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param r
              _2: usize.&
              _3: usize
              bb0:
                _2 = _1
                _3 = _2.*
                _0 = _3
                return
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: Pair
              _2: Pair
              _3: usize.&
              _4: usize
              bb0:
                _1 = call item mk() -> bb1
              bb1:
                _2 = _1
                _3 = _2.1.&
                _4 = call item get(_3) -> bb2
              bb2:
                _0 = _4
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

/// `.&raw` of a temporary lowers to one nameless local, assigned once,
/// addressed by `&raw _1`.
#[test]
fn a_raw_address_of_a_temporary_lands_on_the_nameless_addressable_local() {
    check_mir(
        r#"
static mk = fn() -> usize { 7 };
static main = fn() -> usize { unsafe { mk().&raw.* } };
"#,
        expect![[r#"
            item mk:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 7
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item main:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              _2: usize
              _3: usize.&raw
              _4: usize
              bb0:
                _1 = call item mk() -> bb1
              bb1:
                _2 = _1
                _3 = _2.&raw
                _4 = _3.*
                storage_dead _2
                _0 = _4
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

/// A loan of a loop-body temporary cannot survive the back edge: its
/// storage ends with the body block while the loan (through `r`) is live.
/// The blame sits on the body's closing brace and says "this temporary".
#[test]
fn a_loan_of_a_loop_body_temporary_cannot_survive_the_back_edge() {
    check_loans(
        r#"
static mk = fn() -> usize { 7 };
static keep = fn::<@a>(r: usize.&::<@a>, s: usize.&::<@a>) -> usize.&::<@a> { s };
static f = fn() -> usize {
    let n: usize = 1;
    let mut r = n.&;
    let mut i: usize = 0;
    loop {
        if i == 2 { break; };
        r = keep(r, mk().&);
        i = i + 1;
    };
    r.*
};
"#,
        expect![[r#"
            306..307: leaving this block ends the storage of this temporary, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 274..280: this borrow was created here
              note at 313..314: and it is still used here
        "#]],
    );
}

/// A temporary holding a borrow of `n` keeps `n` loaned until the
/// temporary's last use: a write to `n` in between is refused, naming `n`.
#[test]
fn a_loan_reaching_through_a_temporary_is_still_a_loan_of_its_source() {
    check_loans(
        r#"
static id = fn::<@b>(r: usize.&::<@b>) -> usize.&::<@b> { r };
static f = fn() -> usize {
    let mut n: usize = 1;
    let rr = id(n.&).&;
    n = 5;
    rr.*.*
};
"#,
        expect![[r#"
            149..150: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 133..136: this borrow was created here
              note at 156..158: and it is still used here
        "#]],
    );
}

/// A `fn` literal returning a borrow of a temporary is refused, and the
/// message says "this temporary".
#[test]
fn a_nested_literal_returning_a_borrow_of_a_temporary_names_the_temporary() {
    check_loans(
        r#"
static mk = fn() -> usize { 7 };
static f = fn() -> usize {
    let g = fn() -> usize.&::<@_> { mk().& };
    g().*
};
"#,
        expect![[r#"
            97..103: borrowed value does not live long enough: this borrows a temporary, which lives no longer than the block that creates it, but the borrow is still live when the body returns
              note at 104..105: the storage ends here, where the body returns
        "#]],
    );
}

/// A borrow of a nested block's `let` that escapes into an outer holder
/// is refused where the block is left: the storage of `x` ends at the
/// closing brace, and `r` is read after it.
#[test]
fn a_borrow_of_a_block_local_does_not_survive_the_block() {
    check_loans(
        r#"
static f = fn() -> usize {
    let a: usize = 1;
    let mut r = a.&;
    {
        let x: usize = 2;
        r = x.&;
    };
    r.*
};
"#,
        expect![[r#"
            124..125: leaving this block ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 115..118: this borrow was created here
              note at 131..132: and it is still used here
        "#]],
    );
}

/// The same escape through a materialized temporary: the temporary's
/// storage ends with the block that created it, and the blame names it
/// as "this temporary".
#[test]
fn a_borrow_of_a_block_temporary_does_not_survive_the_block() {
    check_loans(
        r#"
static mk = fn() -> usize { 2 };
static f = fn() -> usize {
    let a: usize = 1;
    let mut r = a.&;
    {
        r = mk().&;
    };
    r.*
};
"#,
        expect![[r#"
            134..135: leaving this block ends the storage of this temporary, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 122..128: this borrow was created here
              note at 141..142: and it is still used here
        "#]],
    );
}

/// A `break` is an exit of the loop body's block too: the body local's
/// storage ends on that path, and the blame sits on the `break`.
#[test]
fn a_break_ends_the_storage_of_the_loop_body_on_its_path() {
    check_loans(
        r#"
static f = fn() -> usize {
    let a: usize = 1;
    let mut r = a.&;
    loop {
        let x: usize = 2;
        r = x.&;
        break;
    };
    r.*
};
"#,
        expect![[r#"
            133..138: leaving this block ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 120..123: this borrow was created here
              note at 151..152: and it is still used here
        "#]],
    );
}

/// The accepting twin: the holder is reassigned inside the block and not
/// read after it, so no loan of `x` is live where its storage ends.
#[test]
fn a_borrow_of_a_block_local_used_only_inside_the_block_is_accepted() {
    check_loans(
        r#"
static f = fn() -> usize {
    let a: usize = 1;
    let mut r = a.&;
    let mut total: usize = 0;
    {
        let x: usize = 2;
        r = x.&;
        total = r.*;
    };
    total
};
"#,
        expect![[""]],
    );
}

/// A loop that reads through the holder at its top and rebinds it to a
/// body local below: the body's end kills the storage while the next
/// iteration's read still needs it, and the message says the storage is
/// gone rather than that the borrow was invalidated.
#[test]
fn a_borrow_of_a_loop_body_local_does_not_survive_to_the_next_iteration() {
    check_loans(
        r#"
static f = fn() -> usize {
    let a: usize = 1;
    let mut r = a.&;
    let mut total: usize = 0;
    let mut i: usize = 0;
    loop {
        if i == 3 { break; };
        total = total + r.*;
        let x: usize = i;
        r = x.&;
        i = i + 1;
    };
    total
};
"#,
        expect![[r#"
            263..264: leaving this block ends the storage of `x`, which invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read storage that no longer exists
              note at 235..238: this borrow was created here
              note at 192..193: and the loop brings control back round to this use of it
        "#]],
    );
}

/// A match arm's pattern binder dies with the arm: a borrow of it still
/// live after the `match` is refused at the arm body's closing brace,
/// not at the block around the `match`.
#[test]
fn a_match_binder_dies_with_its_arm() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let mut r = a.&;
    match o {
        ::Some(x) => { r = x.&; },
        ::None => {},
    };
    r.*
};
"#,
        expect![[r#"
            162..163: leaving this arm ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 157..160: this borrow was created here
              note at 198..199: and it is still used here
        "#]],
    );
}

/// A braceless arm has no brace to end at: the refusal sits on the arm's
/// expression.
#[test]
fn a_braceless_arms_binder_dies_at_the_arm_expression() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let r: usize.&::<@_> = match o {
        ::Some(x) => x.&,
        ::None => a.&,
    };
    r.*
};
"#,
        expect![[r#"
            153..156: leaving this arm ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 153..156: this borrow was created here
              note at 153..156: and it is still used here
        "#]],
    );
}

/// An unannotated assignment whose arms both yield borrows reaches the
/// checker (#26): the join's result is already the place's borrow type,
/// and re-binding it to its own resolution used to trip the region-safety
/// assertion on `adopt` instead of reporting the arm's dead binder.
#[test]
fn an_unannotated_match_of_borrows_into_a_borrow_place() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(v: Opt) -> usize {
    let a: usize = 1;
    let mut r = a.&;
    r = match v { ::Some(x) => x.&, ::None => a.& };
    r.*
};
"#,
        expect![[r#"
            147..150: leaving this arm ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 147..150: this borrow was created here
              note at 147..150: and it is still used here
        "#]],
    );
}

/// The same join with arms borrowing locals that outlive the `match` is
/// accepted.
#[test]
fn an_unannotated_match_of_live_borrows_is_accepted() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(v: Opt) -> usize {
    let a: usize = 1;
    let b: usize = 2;
    let mut r = a.&;
    r = match v { ::Some(x) => b.&, ::None => a.& };
    r.*
};
"#,
        expect![[r#""#]],
    );
}

/// A temporary a braceless arm body materializes dies with the arm too:
/// borrowing a call's result in the arm and reading the borrow after
/// the `match` is refused at the arm's expression, and the blame names it
/// as "this temporary".
#[test]
fn a_braceless_arms_temporary_dies_with_the_arm() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static mk = fn() -> usize { 2 };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let r: usize.&::<@_> = match o {
        ::Some(x) => mk().&,
        ::None => a.&,
    };
    r.*
};
"#,
        expect![[r#"
            186..192: leaving this arm ends the storage of this temporary, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 186..192: this borrow was created here
              note at 186..192: and it is still used here
        "#]],
    );
}

/// A `break` from inside an arm is an exit of the arm too: the binder's
/// storage ends on that path, and the blame sits on the `break`.
#[test]
fn a_break_from_inside_an_arm_ends_the_binders_storage() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let mut r = a.&;
    loop {
        match o {
            ::Some(x) => { r = x.&; break; },
            ::None => { break; },
        };
    };
    r.*
};
"#,
        expect![[r#"
            181..186: leaving this arm ends the storage of `x`, which invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read storage that no longer exists
              note at 176..179: this borrow was created here
              note at 247..248: and it is still used here
        "#]],
    );
}

/// The accepting twin: the borrow of the binder is used inside the arm
/// and the holder is not read after the `match`.
#[test]
fn a_borrow_of_a_match_binder_used_only_inside_the_arm_is_accepted() {
    check_loans(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    let a: usize = 1;
    let mut r = a.&;
    let mut total: usize = 0;
    match o {
        ::Some(x) => { r = x.&; total = r.*; },
        ::None => {},
    };
    total
};
"#,
        expect![[""]],
    );
}

/// A raw pointer to a block's temporary gets no static help: the program
/// checks clean, and the dangling deref is the interpreter's to detect.
#[test]
fn a_raw_pointer_to_ended_block_storage_is_not_refused() {
    check_loans(
        r#"
static mk = fn() -> usize { 2 };
static f = fn() -> usize {
    let p = { mk().&raw };
    unsafe { p.* }
};
"#,
        expect![[""]],
    );
}

/// The markers a nested block lowers to: its `let` dies at the block's
/// end, after the tail has been read where it stands, so the block's
/// value never reads dead storage. The outer `let` is the frame's and
/// gets no marker.
#[test]
fn a_nested_block_ends_the_storage_of_its_locals_after_reading_its_tail() {
    check_mir(
        r#"
static f = fn() -> usize {
    let a: usize = 1;
    let b = {
        let x: usize = a;
        x
    };
    b
};
"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // a
              _2: usize
              _3: usize  // x
              _4: usize
              _5: usize  // b
              _6: usize
              bb0:
                _1 = 1
                _2 = _1
                _3 = _2
                _4 = _3
                storage_dead _3
                _5 = _4
                _6 = _5
                _0 = _6
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
