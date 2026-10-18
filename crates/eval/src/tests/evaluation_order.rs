//! Operand evaluation order.

use super::*;

// ---- a name is read where it stands (G24) --------------------------------

/// Evaluation is left to right: the left operand is read before the right
/// operand assigns the local.
#[test]
fn a_binary_operand_is_read_before_a_later_operand_writes_it() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut x: usize = 1;
    let y = x + { x = 5; 1 };
    y
};
"#,
        "f()",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn a_const_context_reads_a_binary_operand_where_it_stands() {
    check_const(
        r#"
static v: usize = const {
    let mut x: usize = 1;
    x + { x = 5; 1 }
};
"#,
        expect![[r#"
            v = 2
        "#]],
    );
}

#[test]
fn a_call_argument_is_read_before_a_later_argument_writes_it() {
    check_run(
        r#"
static pack = fn(a: usize, b: usize) -> usize { a * 10 + b };
static f = fn() -> usize {
    let mut x: usize = 1;
    pack(x, { x = 5; 2 })
};
"#,
        "f()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn a_constructor_element_is_read_before_a_later_element_writes_it() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut x: usize = 1;
    let r = struct { a = x, b = { x = 5; 2 } };
    let mut y: usize = 3;
    let arr = [y, { y = 7; 4 }];
    r.a * 1000 + r.b * 100 + arr[0] * 10 + arr[1]
};
"#,
        "f()",
        expect![[r#"
            => 1234
        "#]],
    );
}

#[test]
fn an_index_operand_is_read_before_a_later_index_writes_it() {
    check_run(
        r#"
static f = fn() -> usize {
    let grid: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    let mut i: usize = 1;
    grid[i][{ i = 0; 1 }]
};
"#,
        "f()",
        expect![[r#"
            => 4
        "#]],
    );
}

/// The base of `arr[..]` names a place: the element is read after the index
/// operand has run.
#[test]
fn an_indexed_name_is_a_place_not_a_read() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut arr: [usize; 2] = [10, 20];
    arr[{ arr = [30, 40]; 1 }]
};
"#,
        "f()",
        expect![[r#"
            => 40
        "#]],
    );
}

#[test]
fn an_assigned_value_reads_its_operands_where_they_stand() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut x: usize = 1;
    let mut y: usize = 0;
    y = x + { x = 5; 1 };
    y
};
"#,
        "f()",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn a_match_binder_holds_the_scrutinee_as_it_was_read() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut x: usize = 1;
    match x { 5 => 0, v => { x = 5; v * 10 + x } }
};
"#,
        "f()",
        expect![[r#"
            => 15
        "#]],
    );
}

#[test]
fn a_must_consume_argument_is_moved_before_a_later_argument_replaces_it() {
    check_run(
        r#"
type Lin = struct { id: usize } only move with {
    impl Self {
        eat = fn(s: Self) -> usize { let Lin(struct { id }) = s; id };
    }
};
static both = fn(a: Lin, b: usize) -> usize { a.eat() * 10 + b };
static f = fn() -> usize {
    let mut s = Lin(struct { id = 1 });
    let out = both(s, { s = Lin(struct { id = 5 }); 2 });
    out * 10 + s.eat()
};
"#,
        "f()",
        expect![[r#"
            => 125
        "#]],
    );
}

/// A dot-call evaluates its receiver last (M10), so the receiver sees a
/// write an argument made; the arguments themselves go left to right.
#[test]
fn a_receiver_is_read_after_its_arguments() {
    check_run(
        r#"
type N = struct { n: usize } with {
    impl Self {
        m = fn(a: usize, b: usize, s: Self) -> usize { s.n * 100 + a * 10 + b };
    }
};
static f = fn() -> usize {
    let mut x = N(struct { n = 1 });
    let mut k: usize = 2;
    x.m(k, { x = N(struct { n = 5 }); k = 3; 4 })
};
"#,
        "f()",
        expect![[r#"
            => 524
        "#]],
    );
}

/// The read of `x` stands between the mint of `r` and the write through
/// it, so the write goes through a suspended borrow.
#[test]
fn an_argument_read_under_a_live_exclusive_borrow_suspends_it() {
    check_run_past_the_cfg_checkers(
        r#"
static pack = fn(a: usize, b: usize) -> usize { a * 10 + b };
static f = fn() -> usize {
    let mut x: usize = 1;
    let r = x.&mut;
    pack(x, { r.* = 5; 1 })
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

/// `p` is read before the index operand reassigns it, so the element
/// read goes through the old pointer, under the live borrow `x`.
#[test]
fn a_read_through_a_pointer_reassigned_by_its_index_uses_the_old_pointer() {
    check_run_past_the_cfg_checkers(
        r#"
static f = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0].&mut;
    let q = b.&mut;
    let y = p.*[{ p = q; 0 }];
    x.* = 7;
    y
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn a_write_through_a_pointer_reassigned_by_its_index_uses_the_old_pointer() {
    check_run_past_the_cfg_checkers(
        r#"
static f = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut b: [usize; 2] = [3, 4];
    let mut p = a.&mut;
    let x = p.*[0].&mut;
    let q = b.&mut;
    p.*[{ p = q; 0 }] = 9;
    x.* = 7;
    x.*
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}
