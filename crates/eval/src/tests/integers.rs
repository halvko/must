//! Sized integers: overflow, negation and `offset`.

use super::*;

// --- Integer types: typed arithmetic, overflow traps, unary minus, offset ---

#[test]
fn typed_arithmetic_overflow_traps_name_the_operation_and_type() {
    // Const contexts get the eager error (div-by-zero precedent); the
    // message names the operation and the type.
    check_const(
        r#"
static add8: u8 = 250 + 10;
static sub8: u8 = 3 - 5;
static mul8: u8 = 16 * 16;
static ok8: u8 = 250 + 5;
"#,
        expect![[r#"
            add8 = error[Runtime]: arithmetic overflow: `250 + 10` does not fit in `u8`
            sub8 = error[Runtime]: arithmetic overflow: `3 - 5` does not fit in `u8`
            mul8 = error[Runtime]: arithmetic overflow: `16 * 16` does not fit in `u8`
            ok8 = 255
        "#]],
    );
}

#[test]
fn signed_overflow_edges_trap() {
    check_const(
        r#"
static div_min: i8 = -128 / -1;
static neg_min: i8 = const { let m: i8 = -128; -m };
static ok: i8 = -128 / 1;
"#,
        expect![[r#"
            div_min = error[Runtime]: arithmetic overflow: `-128 / -1` does not fit in `i8`
            neg_min = error[Runtime]: arithmetic overflow: `-(-128)` does not fit in `i8`
            ok = -128
        "#]],
    );
}

#[test]
fn arithmetic_overflow_traps_at_runtime_too() {
    check_run(
        r#"
static bump = fn (n: u8) -> u8 { n + 10 };
"#,
        "bump(250)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `250 + 10` does not fit in `u8`
        "#]],
    );
}

#[test]
fn unary_minus_evaluates_on_signed_and_traps_on_unsigned_nonzero() {
    check_run(
        r#"
static negate = fn (n: i32) -> i32 { -n };
"#,
        "negate(41) + 83",
        expect![[r#"
            => 42
        "#]],
    );
    check_run(
        r#"
static negate = fn (n: u8) -> u8 { -n };
"#,
        "negate(0)",
        expect![[r#"
            => 0
        "#]],
    );
    check_run(
        r#"
static negate = fn (n: u8) -> u8 { -n };
"#,
        "negate(1)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `-1` does not fit in `u8`
        "#]],
    );
}

#[test]
fn i64_min_negation_traps() {
    check_run(
        r#"
static negate = fn (n: i64) -> i64 { -n };
static min: i64 = -9223372036854775808;
"#,
        "negate(min)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `-(-9223372036854775808)` does not fit in `i64`
        "#]],
    );
}

#[test]
fn each_width_wraps_its_own_range() {
    // The same value overflows a narrow type and fits a wide one.
    check_const(
        r#"
static narrow: i16 = 300 * 300;
static wide: i32 = 300 * 300;
"#,
        expect![[r#"
            narrow = error[Runtime]: arithmetic overflow: `300 * 300` does not fit in `i16`
            wide = 90000
        "#]],
    );
}

#[test]
fn offset_moves_both_directions() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [10, 20, 30];
    let p = a[1].&raw mut;
    unsafe {
        let forward = offset(p, 1);
        let back = offset(p, -1);
        forward.* + back.*
    }
};
"#,
        "main()",
        expect![[r#"
            => 40
        "#]],
    );
}

#[test]
fn offset_below_the_start_is_detected_ub_at_the_call() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [10, 20, 30];
    let p = a[1].&raw mut;
    unsafe { offset(p, -2).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `offset` result points below the start of the allocation
        "#]],
    );
}

#[test]
fn offset_past_the_end_mints_silently_and_derefs_as_ub_like_add() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 2] = [1, 2];
    let p = a[0].&raw mut;
    unsafe { offset(p, 5).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn offset_by_zero_is_the_identity() {
    check_run(
        r#"
static main = fn () -> bool {
    let mut x: u8 = 7;
    let p = x.&raw mut;
    unsafe { offset(p, 0) == p }
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn typed_values_freeze_into_statics_per_width() {
    check_const(
        r#"
static a: u8 = 200;
static b: i8 = -100;
static c: isize = -5;
"#,
        expect![[r#"
            a = 200
            b = -100
            c = -5
        "#]],
    );
}

#[test]
fn wrong_kind_builtin_count_is_intercepted_at_the_argument() {
    // Defense-in-depth: the count/index builtins now require the exact
    // checker-pinned kind (`usize` for the alloc/copy/`add` counts, `isize`
    // for `offset`), trapping a wrong kind ill-typed rather than laundering
    // it. That trap is not reachable from source, though: the checker pins
    // each of these argument positions to its kind, so a wrong-kind
    // argument (here a `u8` where `alloc_array` wants `usize`) is
    // value-trapped AT THE ARGUMENT — with the ordinary type-mismatch
    // message — before the builtin ever runs. This test pins that
    // interception (the same reason the sibling `ProjElem::Index`
    // tightening ships without a builtin-arm test): a mistyped builtin
    // count traps cleanly, never launders and never panics the machine.
    check_run(
        r#"
static main = fn () -> () {
    let n: u8 = 3;
    match alloc_array::<usize>(n) {
        AllocResult::Ok(p) => { unsafe { dealloc_array(p, 3); }; }
        AllocResult::Err => {}
    };
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `usize`, found `u8`
        "#]],
    );
}

#[test]
fn mixed_kind_integer_equality_is_intercepted_at_the_operand() {
    // The same no-laundering discipline as the arithmetic ops: the checker
    // requires `==`/`!=` operands to agree, so a mixed-kind pairing (a `u8`
    // against a `usize`) is an ordinary mismatch on the culprit operand, and
    // the machine replays it as a value trap before the operands ever meet.
    // The operator's own same-kind check (the machine arms for `eval_bin_op`
    // equality) stays behind it as defense in depth, unreachable from source
    // — the same reason the sibling builtin-count tightening ships with an
    // interception test and no builtin-arm test.
    check_run(
        r#"
static main = fn () -> bool {
    let x: u8 = 1;
    let y: usize = 1;
    x == y
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `u8`, found `usize`
        "#]],
    );
    check_run(
        r#"
static main = fn () -> bool {
    let x: u8 = 1;
    let y: usize = 1;
    x != y
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `u8`, found `usize`
        "#]],
    );
}
