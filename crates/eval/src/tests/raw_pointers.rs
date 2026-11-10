//! Raw pointers: reads, writes, dangling and out-of-bounds detection.

use super::*;

#[test]
fn write_through_raw_mut_is_visible_through_the_local() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw;
    unsafe { p.* = 42; }
    x
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn pointer_to_a_field_reads_and_writes_that_element() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw;
    unsafe { pa.* = 10; }
    r.a + r.b
};
"#,
        "main()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn interior_pointer_survives_whole_value_overwrite() {
    // Overwriting the whole record writes INTO the allocation, so an
    // interior pointer minted before the overwrite sees the new field —
    // exactly real-memory behavior.
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw;
    r = struct { a = 3, b = 4 };
    unsafe { pa.* }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn pointee_field_reads_chain() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw;
    unsafe { p.*.a + p.*.b }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn pointer_copies_alias_the_same_place() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw;
    let q = p;
    unsafe { q.* = 9; p.* }
};
"#,
        "main()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn two_addr_of_the_same_static_are_the_same_address() {
    check_run(
        r#"
static s: usize = 7;
static main = fn() -> bool {
    let a = s.&raw;
    let b = s.&raw;
    a == b
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn deref_of_a_static_pointer_reads_the_static() {
    check_run(
        r#"
static s: struct { a: usize, b: usize } = struct { a = 40, b = 2 };
static main = fn() -> usize {
    let pa = s.a.&raw;
    let pb = s.b.&raw;
    unsafe { pa.* + pb.* }
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn addr_of_a_const_takes_the_address_of_each_use_copy() {
    // const=copied: the interpreter happens to give each `c.&raw` mention
    // its own temporary, so two of them compare unequal. Const-mention
    // identity is deliberately unspecified — this pins today's interpreter
    // behaviour, not a language promise.
    check_run(
        r#"
const c: usize = 7;
static main = fn() -> bool { c.&raw == c.&raw };
"#,
        "main()",
        expect![[r#"
            => false
        "#]],
    );
}

#[test]
fn two_addr_of_the_same_local_are_equal() {
    check_run(
        r#"
static main = fn() -> bool {
    let mut x: usize = 1;
    x.&raw == x.&raw
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn dangling_deref_after_frame_return_is_detected_ub() {
    check_run(
        r#"
static make = fn() -> usize.&raw {
    let mut x = 5;
    x.&raw
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn pointers_work_inside_const_evaluation() {
    // Unsafe (and pointers) are legal in const contexts: the whole
    // computation runs at compile time, the pointer dies inside it.
    check_const(
        r#"
static v = {
    let mut x: usize = 1;
    let p = x.&raw;
    unsafe { p.* = 41; }
    x + 1
};
"#,
        expect![[r#"
            v = 42
        "#]],
    );
}

#[test]
fn a_pointer_cannot_leave_const_evaluation() {
    check_const(
        r#"
static p = {
    let mut x: usize = 1;
    x.&raw
};
"#,
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn a_pointer_inside_a_record_cannot_leave_const_evaluation_either() {
    check_const(
        r#"
static p = {
    let mut x: usize = 1;
    struct { ptr = x.&raw }
};
"#,
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn a_pointer_cannot_leave_a_const_block() {
    check_const_blocks(
        r#"
static main = fn() -> usize {
    const { let mut y: usize = 1; let p = y.&raw; unsafe { p.* } }
};
static bad = fn() {
    const { let mut y: usize = 1; y.&raw };
};
"#,
        expect![[r#"
            main#0 = 1
            bad#0 = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn deref_display_is_opaque_never_a_number() {
    check_run(
        r#"
static main = fn() -> usize.&raw {
    let mut x = 1;
    x.&raw
};
"#,
        "main()",
        expect![[r#"
            => &raw <opaque>
        "#]],
    );
}

// ---- raw pointers, through-pointer places ----

#[test]
fn through_pointer_field_chain_writes_the_nested_field() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r = struct { inner = struct { v = 1 }, other = 2 };
    let p = r.&raw;
    unsafe { p.*.inner.v = 40; }
    r.inner.v + r.other
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn through_pointer_element_reads_and_writes() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2, 3];
    let p = a.&raw;
    unsafe { p.*[1] = 9; }
    unsafe { a[0] + p.*[1] }
};
"#,
        "main()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn through_pointer_element_write_out_of_bounds_is_an_ordinary_trap() {
    // The `[i]` step is the program's own checked indexing — an ordinary
    // runtime trap with the same message as `a[i]`, symmetric with the
    // `p.*[i]` read. (Pointers MINTED out of bounds are the UB case; a
    // compile-time-known OOB index would be the planted squiggle trap
    // instead — same message either way.)
    check_run(
        r#"
static main = fn() {
    let mut a: [usize; 2] = [1, 2];
    let p = a.&raw;
    let i: usize = 5;
    unsafe { p.*[i] = 0; }
};
"#,
        "main()",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 5
        "#]],
    );
}

#[test]
fn chained_deref_writes_through_a_pointer_to_a_pointer() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let mut p = x.&raw;
    let pp = p.&raw;
    unsafe { pp.*.* = 7; }
    x
};
"#,
        "main()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn deref_in_the_middle_of_a_write_chain_composes() {
    // `p.*.q.*.v = 5;` — the OUTERMOST deref governs the store; the inner
    // one is an ordinary read that fetches the interior pointer.
    check_run(
        r#"
static main = fn() -> usize {
    let mut inner = struct { v = 1 };
    let mut outer = struct { q = inner.&raw };
    let p = outer.&raw;
    unsafe { p.*.q.*.v = 5; }
    inner.v
};
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn through_pointer_mixed_chain_with_elements_and_fields() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { buf: [struct { v: usize }; 2] } = struct { buf = [struct { v = 1 }, struct { v = 2 }] };
    let p = r.&raw;
    unsafe { p.*.buf[1].v = 9; }
    unsafe { p.*.buf[0].v + p.*.buf[1].v }
};
"#,
        "main()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn addr_of_array_element_writes_through_to_the_array() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2, 3];
    let p = a[1].&raw;
    unsafe { p.* = 20; }
    a[0] + a[1] + a[2]
};
"#,
        "main()",
        expect![[r#"
            => 24
        "#]],
    );
}

#[test]
fn addr_of_out_of_bounds_element_mints_silently_and_derefs_as_ub() {
    // Address-taking never bounds-checks (validity is a deref-time
    // judgement): the out-of-range address mints fine, the deref is
    // detected UB.
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2];
    let i = 5;
    let p = a[i].&raw;
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn a_write_through_an_out_of_bounds_pointer_is_detected_ub() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2];
    let i = 5;
    let p = a[i].&raw;
    unsafe { p.* = 9; a[0] }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_is_double_indirection_free() {
    // `p.*.a.&raw` carries the ORIGINAL allocation's identity with an
    // extended path — no intermediate materialization, so it is the same
    // address `r.a.&raw` mints.
    check_run(
        r#"
static main = fn() -> bool {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw;
    unsafe { p.*.a.&raw == r.a.&raw }
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_writes_the_original_place() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw };
    unsafe { q.* = 40; }
    r.a + r.b
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn dangling_interior_pointers_are_detected_ub() {
    // Interior pointers die with their frame's allocation — the liveness
    // story covers extended paths with no extra machinery.
    check_run(
        r#"
static make = fn() -> usize.&raw {
    let mut a = [1, 2];
    a[0].&raw
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
    // A field pointer minted through a deref dies with its frame too.
    check_run(
        r#"
static make = fn() -> usize.&raw {
    let mut r: struct { a: usize } = struct { a = 1 };
    let p = r.&raw;
    unsafe { p.*.a.&raw }
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn interior_pointer_survives_through_pointer_whole_value_overwrite() {
    // Direct overwrites (`r = ...`) pinned this first; the same
    // invariant holds when the overwrite itself goes through a pointer:
    // `p.* = ...` writes INTO the allocation, so the interior pointer
    // sees the new field.
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw;
    let p = r.&raw;
    unsafe { p.* = struct { a = 3, b = 4 }; }
    unsafe { pa.* }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn through_pointer_write_into_a_static_is_detected_ub() {
    // `s.&raw` is legal and the checker does not judge the write, so the
    // read-only allocation does — extended paths do not open a route
    // around it.
    check_run(
        r#"
static s: struct { a: usize } = struct { a = 1 };
static main = fn() {
    let p = s.&raw;
    unsafe { p.*.a = 2; }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: write through a pointer into read-only memory (a `static`)
        "#]],
    );
}

#[test]
fn through_pointer_write_into_a_non_mut_local_is_detected_ub() {
    // A non-`mut` binding is minted read-only the first time its address
    // is taken, like a static, so a write through the pointer — whole,
    // into a field, or as a range (`copy`) — is detected UB (#183).
    for write in [
        "let p = x.&raw; unsafe { p.* = [5, 6]; }",
        "let p = x[1].&raw; unsafe { p.* = 5; }",
        "unsafe { copy(q[0].&raw, x[0].&raw, 2); }",
    ] {
        check_run(
            &format!(
                r#"
static main = fn() -> usize {{
    let x: [usize; 2] = [1, 2];
    let mut q: [usize; 2] = [3, 4];
    {write}
    x[0]
}};
"#
            ),
            "main()",
            expect![[r#"
                error[UndefinedBehavior]: write through a pointer into read-only memory (the local `x`, which is not declared `mut`)
            "#]],
        );
    }
}

#[test]
fn through_pointer_write_into_a_non_mut_parameter_is_detected_ub() {
    check_run(
        r#"
static f = fn(n: usize) -> usize {
    let p = n.&raw;
    unsafe { p.* = 2; }
    n
};
"#,
        "f(1)",
        expect![[r#"
            error[UndefinedBehavior]: write through a pointer into read-only memory (the local `n`, which is not declared `mut`)
        "#]],
    );
}

#[test]
fn through_pointer_write_into_a_const_copy_is_detected_ub() {
    // `c.&raw` points at this use's own copy of the `const`, which stays
    // one value at every use, so it is read-only too.
    check_run(
        r#"
const c: usize = 1;
static main = fn() -> usize {
    let p = c.&raw;
    unsafe { p.* = 2; }
    c
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: write through a pointer into read-only memory (a `const`'s copy)
        "#]],
    );
}

#[test]
fn reads_through_a_pointer_to_a_non_mut_local_are_fine() {
    // Read-only means pointer WRITES are refused, nothing else: reads
    // work, and a binding re-initialized by name on each loop iteration
    // keeps working after its address was taken.
    check_run(
        r#"
static main = fn() -> usize {
    let mut total: usize = 0;
    let mut i: usize = 0;
    loop {
        if i == 3 { break; }
        let x: usize = i + 1;
        let p = x.&raw;
        total = total + unsafe { p.* };
        i = i + 1;
    }
    total
};
"#,
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn through_pointer_write_under_a_shared_borrow_is_detected_ub() {
    // `.&raw` through a `.&` mints no write permission of its own: the
    // pointer inherits the borrow's frozen node, and a write through it
    // is caught there.
    check_run(
        r#"
static main = fn() -> usize {
    let mut x: usize = 1;
    let b = x.&;
    let p = b.*.&raw;
    unsafe { p.* = 2; }
    x
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: write through a shared borrow — only `.&mut` may write through a borrow
              note: this borrow was created here
        "#]],
    );
}

#[test]
fn through_pointer_field_write_outside_unsafe_traps() {
    check_run(
        r#"
static main = fn() {
    let mut r: struct { a: usize } = struct { a = 1 };
    let p = r.&raw;
    p.*.a = 2;
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn mid_chain_deref_outside_unsafe_traps() {
    // The INNER deref of `pp.*.* = 7;` is an ordinary read — it needs
    // `unsafe` like every deref site, even when the outer one is the
    // store.
    check_run(
        r#"
static main = fn() {
    let mut x: usize = 1;
    let mut p = x.&raw;
    let pp = p.&raw;
    pp.*.* = 7;
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn through_pointer_writes_work_inside_const_evaluation() {
    check_const(
        r#"
static v = {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw;
    unsafe { p.*.a = 40; }
    r.a + r.b
};
"#,
        expect![[r#"
            v = 42
        "#]],
    );
}
