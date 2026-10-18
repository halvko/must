//! Raw pointers, arrays and the heap builtins.

use super::*;

#[test]
fn addr_of_deref_and_deref_store_lower_to_place_ops() {
    check_mir(
        r#"
static s: usize = 7;
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    let q = s.&raw;
    unsafe {
        p.* = 2;
        p.* + q.*
    }
};
"#,
        expect![[r#"
            item s:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 7
                return
            }
            item main:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize.&raw mut
              _3: usize.&raw mut  // p
              _4: usize.&raw
              _5: usize.&raw  // q
              _6: usize.&raw mut
              _7: usize.&raw mut
              _8: usize
              _9: usize.&raw
              _10: usize
              _11: usize
              bb0:
                _1 = 1
                _2 = _1.&raw mut
                _3 = _2
                _4 = static s.&raw
                _5 = _4
                _6 = _3
                _6.* = 2
                _7 = _3
                _8 = _7.*
                _9 = _5
                _10 = _9.*
                _11 = Add(_8, _10)
                _0 = _11
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

#[test]
fn deref_outside_unsafe_lowers_to_a_trap_not_a_load() {
    check_mir(
        r#"
static main = fn() -> usize {
    let mut x: usize = 1;
    let p = x.&raw mut;
    p.*
};
"#,
        expect![[r#"
            item main:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize.&raw mut
              _3: usize.&raw mut  // p
              _4: usize.&raw mut
              _5: usize
              bb0:
                _1 = 1
                _2 = _1.&raw mut
                _3 = _2
                _4 = _3
                _5 = trap "dereferencing a raw pointer requires an `unsafe { ... }` block" -> bb1
              bb1:
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

#[test]
fn through_pointer_writes_lower_to_deref_projected_places() {
    check_mir(
        r#"
static main = fn() {
    let mut r: struct { x: usize, buf: [usize; 2] } = struct { x = 1, buf = [1, 2] };
    let p = r.&raw mut;
    let i = 1;
    unsafe {
        p.*.x = 2;
        p.*.buf[i] = 3;
    }
};
"#,
        expect![[r#"
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: [usize; 2]
              _2: struct { buf: [usize; 2], x: usize }
              _3: struct { buf: [usize; 2], x: usize }  // r
              _4: struct { buf: [usize; 2], x: usize }.&raw mut
              _5: struct { buf: [usize; 2], x: usize }.&raw mut  // p
              _6: usize  // i
              _7: struct { buf: [usize; 2], x: usize }.&raw mut
              _8: struct { buf: [usize; 2], x: usize }.&raw mut
              _9: usize
              bb0:
                _1 = [1, 2]
                _2 = { buf: _1, x: 1 }
                _3 = _2
                _4 = _3.&raw mut
                _5 = _4
                _6 = 1
                _7 = _5
                _7.*.1 = 2
                _8 = _5
                _9 = _6
                _8.*.0[_9] = 3
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn place_shaped_reads_lower_to_one_projection() {
    // The read twin of `through_pointer_writes_lower_to_deref_projected_places`:
    // a chain that names a PLACE is read through one projection, however
    // long it is and whether or not a deref roots it — never as a copy of
    // its root followed by a field/index extraction. `eval`'s aliasing
    // tree checks each read against the path it sees here, so a wide read
    // would be a foreign access to every borrow underneath it (see
    // `lower_place_read`).
    check_mir(
        r#"
static main = fn() {
    let mut r: struct { x: usize, buf: [usize; 2] } = struct { x = 1, buf = [1, 2] };
    let p = r.&raw mut;
    let i = 1;
    let a = r.buf[i];
    unsafe {
        let b = p.*.x;
        let c = p.*.buf[i];
    }
};
"#,
        expect![[r#"
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: [usize; 2]
              _2: struct { buf: [usize; 2], x: usize }
              _3: struct { buf: [usize; 2], x: usize }  // r
              _4: struct { buf: [usize; 2], x: usize }.&raw mut
              _5: struct { buf: [usize; 2], x: usize }.&raw mut  // p
              _6: usize  // i
              _7: usize
              _8: usize
              _9: usize  // a
              _10: struct { buf: [usize; 2], x: usize }.&raw mut
              _11: usize
              _12: usize  // b
              _13: struct { buf: [usize; 2], x: usize }.&raw mut
              _14: usize
              _15: usize
              _16: usize  // c
              bb0:
                _1 = [1, 2]
                _2 = { buf: _1, x: 1 }
                _3 = _2
                _4 = _3.&raw mut
                _5 = _4
                _6 = 1
                _7 = _6
                _8 = _3.0[_7]
                _9 = _8
                _10 = _5
                _11 = _10.*.1
                _12 = _11
                _13 = _5
                _14 = _6
                _15 = _13.*.0[_14]
                _16 = _15
                storage_dead _16
                storage_dead _12
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn mid_chain_deref_write_reads_the_inner_pointer_then_stores() {
    check_mir(
        r#"
static main = fn() {
    let mut x: usize = 1;
    let mut p = x.&raw mut;
    let pp = p.&raw mut;
    unsafe { pp.*.* = 7; }
};
"#,
        expect![[r#"
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: usize  // x
              _2: usize.&raw mut
              _3: usize.&raw mut  // p
              _4: usize.&raw mut.&raw mut
              _5: usize.&raw mut.&raw mut  // pp
              _6: usize.&raw mut.&raw mut
              _7: usize.&raw mut
              bb0:
                _1 = 1
                _2 = _1.&raw mut
                _3 = _2
                _4 = _3.&raw mut
                _5 = _4
                _6 = _5
                _7 = _6.*
                _7.* = 7
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn addr_of_array_element_and_through_deref_lower_without_promotion_of_the_pointer() {
    check_mir(
        r#"
static main = fn() -> usize {
    let mut a: [usize; 2] = [1, 2];
    let e = a[0].&raw mut;
    let mut r = struct { x = 1 };
    let p = r.&raw mut;
    unsafe {
        let q = p.*.x.&raw mut;
        q.*
    }
};
"#,
        expect![[r#"
            item main:
            fn b0() -> usize {
              _0: usize  // return
              _1: [usize; 2]
              _2: [usize; 2]  // a
              _3: usize.&raw mut
              _4: usize.&raw mut  // e
              _5: struct { x: usize }
              _6: struct { x: usize }  // r
              _7: struct { x: usize }.&raw mut
              _8: struct { x: usize }.&raw mut  // p
              _9: struct { x: usize }.&raw mut
              _10: usize.&raw mut
              _11: usize.&raw mut  // q
              _12: usize.&raw mut
              _13: usize
              bb0:
                _1 = [1, 2]
                _2 = _1
                _3 = _2[0].&raw mut
                _4 = _3
                _5 = { x: 1 }
                _6 = _5
                _7 = _6.&raw mut
                _8 = _7
                _9 = _8
                _10 = _9.*.0.&raw mut
                _11 = _10
                _12 = _11
                _13 = _12.*
                storage_dead _11
                _0 = _13
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

// ---- fixed-size arrays ----

#[test]
fn array_literal_index_and_element_assign_lower() {
    check_mir(
        r#"
static f = fn {
    let mut a: [usize; 3] = [1, 2, 3];
    let x = a[1];
    a[2] = x;
};
"#,
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: [usize; 3]
              _2: [usize; 3]  // a
              _3: usize
              _4: usize  // x
              _5: usize
              bb0:
                _1 = [1, 2, 3]
                _2 = _1
                _3 = _2[1]
                _4 = _3
                _5 = _4
                _2[2] = _5
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn array_repeat_and_nested_index_assign_lower() {
    check_mir(
        r#"
static f = fn {
    let mut m: [[usize; 2]; 2] = [[0; 2]; 2];
    m[0][1] = 5;
};
"#,
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: [usize; 2]
              _2: [[usize; 2]; 2]
              _3: [[usize; 2]; 2]  // m
              bb0:
                _1 = [0; 2]
                _2 = [_1; 2]
                _3 = _2
                _3[0][1] = 5
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn compile_time_out_of_bounds_lowers_to_a_trap() {
    check_mir(
        "static f = fn { let a: [usize; 2] = [1, 2]; let x = a[2]; };",
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: [usize; 2]
              _2: [usize; 2]  // a
              _3: usize
              _4: usize  // x
              bb0:
                _1 = [1, 2]
                _2 = _1
                _3 = trap "index out of bounds: the length is 2 but the index is 2" -> bb1
              bb1:
                _4 = _3
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- the heap builtins --------------------------------------------------

#[test]
fn heap_builtin_calls_lower_as_plain_builtin_calls() {
    // Erasure at work: `alloc_array::<usize>`'s MIR carries NO type
    // argument — the callee is the bare builtin constant, the machine
    // allocates `n` uninit elements whatever `T` was.
    check_mir(
        r#"
static f = fn (p: usize.&raw mut) -> () {
    unsafe { dealloc_array(p, 1) };
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize.&raw mut) -> () {
              _0: ()  // return
              _1: usize.&raw mut  // param p
              _2: usize.&raw mut
              _3: ()
              bb0:
                _2 = _1
                _3 = call builtin dealloc_array(_2, 1) -> bb1
              bb1:
                _0 = ()
                return
            }
            fn b1() -> fn(usize.&raw mut) {
              _0: fn(usize.&raw mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn unsafe_builtin_call_outside_unsafe_lowers_to_a_trap() {
    // The call must not execute at all: it traps with exactly the
    // squiggle's message (arguments still evaluated for their effects).
    check_mir(
        r#"
static f = fn (p: usize.&raw mut) -> () {
    dealloc_array(p, 1);
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize.&raw mut) -> () {
              _0: ()  // return
              _1: usize.&raw mut  // param p
              _2: usize.&raw mut
              _3: ()
              bb0:
                _2 = _1
                _3 = trap "calling `dealloc_array` requires an `unsafe { ... }` block" -> bb1
              bb1:
                _0 = ()
                return
            }
            fn b1() -> fn(usize.&raw mut) {
              _0: fn(usize.&raw mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn heap_alloc_in_initializer_gets_a_conditional_const_trap() {
    // Initializer level is the one const context with a runtime escape,
    // so the fence is a ConstTrap: forcing the item traps, the runner's
    // synthetic entry may proceed.
    check_mir(
        "static x = alloc_array::<usize>(1);",
        expect![[r#"
            item x:
            fn b0() -> AllocResult::<usize> {
              _0: AllocResult::<usize>  // return
              _1: AllocResult::<usize>
              bb0:
                const trap "cannot allocate during compile-time evaluation: const-built heap values wait for an interning design" -> bb1
              bb1:
                _1 = call builtin alloc_array(1) -> bb2
              bb2:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn unary_minus_on_a_literal_folds_and_on_a_value_lowers_to_a_neg() {
    // `-n` is a real runtime operation (`Neg`); `-128` folds into the
    // typed constant — the positive magnitude never materializes, which
    // is what lets `i8::MIN` exist at all.
    check_mir(
        "static f = fn (n: i8) -> i8 { if n == -128 { -n } else { n } };",
        expect![[r#"
            item f:
            fn b0(_1: i8) -> i8 {
              _0: i8  // return
              _1: i8  // param n
              _2: i8
              _3: bool
              _4: i8
              _5: i8
              _6: i8
              _7: i8
              bb0:
                _2 = _1
                _3 = Eq(_2, -128)
                if _3 -> [then: bb1, else: bb2]
              bb1:
                _5 = _1
                _6 = Neg(_5)
                _4 = _6
                goto -> bb3
              bb2:
                _7 = _1
                _4 = _7
                goto -> bb3
              bb3:
                _0 = _4
                return
            }
            fn b1() -> fn(i8) -> i8 {
              _0: fn(i8) -> i8  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}
