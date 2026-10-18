//! The heap builtins and the library types built on them.

use super::*;

// ---- the heap builtins --------------------------------------------------

#[test]
fn heap_alloc_write_read_roundtrip() {
    // The result-shaped alloc: run mode never produces `Err` (the
    // interpreter cannot meaningfully OOM), so the `Ok` arm is the one
    // that runs — but the match is mandatory (the call's TYPE is the
    // result enum).
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(3) {
        AllocResult::Ok(p) => {
            unsafe {
                p.* = 10;
                let p1 = add(p, 1);
                p1.* = 20;
                let p2 = add(p, 2);
                p2.* = 30;
            };
            let sum = unsafe { p.* + add(p, 1).* + add(p, 2).* };
            unsafe { dealloc_array(p, 3); };
            sum
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 60
        "#]],
    );
}

#[test]
fn heap_alloc_result_is_a_tagged_enum_value() {
    // What the builtin actually returns: a tagged `AllocResult` value —
    // displayable, matchable, ordinary.
    check_run(
        r#"
static main = fn () -> AllocResult::<usize> { alloc_array::<usize>(1) };
"#,
        "main()",
        expect![[r#"
            => AllocResult::Ok(&raw <opaque>)
        "#]],
    );
}

#[test]
fn heap_uninit_read_is_detected_ub() {
    // Fresh elements are tracked-uninit (A04): reading one before its
    // first write is detected UB, not a zero.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { p.* },
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn heap_uninit_read_through_aggregate_copy_is_detected_ub() {
    // The read gate covers AGGREGATE copies too: `copy` plants poison in
    // a local array's element (silently — that is `copy`'s license), and
    // then reading the WHOLE array as a value trips the same UB.
    check_run(
        r#"
static main = fn () -> bool {
    let mut a = [1, 2];
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => {
            unsafe { copy(p, a[0].&raw mut, 2) };
            let b = a;
            b == b
        }
        AllocResult::Err => false,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn heap_write_initializes_and_partial_writes_track_per_element() {
    // Per-ELEMENT tracking: writing element 0 makes element 0 readable;
    // element 1 stays poison until its own write.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => {
            unsafe { p.* = 5; };
            let first = unsafe { p.* };
            unsafe { dealloc_array(p, 2); };
            first
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn heap_double_free_is_detected_ub_with_allocation_origin() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe {
            dealloc_array(p, 2);
            dealloc_array(p, 2);
        },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: double free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_use_after_free_is_detected_ub_with_allocation_origin() {
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(1) {
        AllocResult::Ok(p) => {
            unsafe { p.* = 3; };
            unsafe { dealloc_array(p, 1); };
            unsafe { p.* }
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_count_mismatch_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { dealloc_array(p, 3) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` with the wrong element count — this allocation has 2 element(s), but 3 were passed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_of_interior_pointer_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { dealloc_array(add(p, 1), 2) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` of a pointer that is not the head of its allocation — it points inside it
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_of_local_is_detected_ub() {
    // A promoted local is memory, but not HEAP memory: freeing it is a
    // category error whatever its liveness.
    check_run(
        r#"
static main = fn () -> () {
    let mut x: usize = 4;
    unsafe { dealloc_array(x.&raw mut, 1) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` of a pointer that does not point to a heap allocation
        "#]],
    );
}

#[test]
fn heap_zero_element_alloc_is_a_defined_trap() {
    // A05: allocators are NOT required to handle zero-size
    // requests — `alloc_array(0)` is a defined trap (never UB),
    // restrictive now and loosenable later. Corollary: no zero-size
    // allocation can exist, so `dealloc_array` never legally sees `n == 0`.
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(0) {
        AllocResult::Ok(p) => unsafe { dealloc_array(p, 0) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[Runtime]: cannot allocate zero elements: zero-size allocation support is reserved
        "#]],
    );
}

#[test]
fn dangling_deref_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> usize {
    let p = dangling::<usize>();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — this pointer was never valid
        "#]],
    );
}

#[test]
fn dangling_pointers_compare_equal() {
    // One reserved allocation per machine: every `dangling()` is the same
    // address.
    check_run(
        r#"
static main = fn () -> bool {
    dangling::<usize>() == dangling::<usize>()
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn add_stays_within_allocation_and_oob_deref_is_detected_ub() {
    // Minting past the end is silent (validity is a deref-time
    // judgement, same as `a[i].&raw mut`); the deref is where it traps.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { add(p, 5).* },
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn add_works_on_local_array_element_pointers() {
    // `add` is not heap-only: any pointer addressing an array element
    // supports it — the stack-buffer story.
    check_run(
        r#"
static main = fn () -> usize {
    let mut a = [10, 20, 30];
    let p = a[0].&raw;
    unsafe { add(p, 2).* }
};
"#,
        "main()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn add_zero_is_identity_on_any_pointer() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut x = 7;
    let p = x.&raw mut;
    unsafe { add(p, 0).* }
};
"#,
        "main()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn add_of_non_element_pointer_is_detected_ub() {
    // The one shape the abstract machine cannot represent: advancing a
    // pointer that doesn't address an array element (a lone local).
    check_run(
        r#"
static main = fn () -> usize {
    let mut x = 7;
    let p = x.&raw mut;
    unsafe { add(p, 1).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `add` of a pointer that does not address an array element
        "#]],
    );
}

#[test]
fn copy_moves_elements_between_allocations() {
    check_run(
        r#"
static main = fn () -> usize {
    let a = [1, 2, 3];
    match alloc_array::<usize>(3) {
        AllocResult::Ok(p) => {
            unsafe { copy(a[0].&raw, p, 3) };
            let sum = unsafe { p.* + add(p, 1).* + add(p, 2).* };
            unsafe { dealloc_array(p, 3) };
            sum
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn copy_overlap_is_defined_memmove_semantics() {
    // Overlapping forward copy: the source range is read out in full
    // before the destination is written — `[1,2,3,4,5]` shifted right by
    // one is `[1,1,2,3,4]`, never the memcpy smear `[1,1,1,1,1]`.
    check_run(
        r#"
static main = fn () -> bool {
    let mut a: [usize; 5] = [1, 2, 3, 4, 5];
    let p = a[0].&raw mut;
    unsafe { copy(p, add(p, 1), 4) };
    a == [1, 1, 2, 3, 4]
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn copy_propagates_uninit_silently() {
    // `copy` is the ONE mover licensed to transport poison: copying a
    // partially initialized buffer works; only a later value-read of the
    // still-uninit element traps.
    check_run(
        r#"
static main = fn () -> usize {
    let p = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let q = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = 9; };
    unsafe { copy(p, q, 2) };
    print("copy of a half-written buffer did not trap");
    let ok = unsafe { q.* };
    print("the initialized element arrived");
    unsafe { add(q, 1).* }
};
"#,
        "main()",
        expect![[r#"
            output: "copy of a half-written buffer did not trapthe initialized element arrived"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn copy_out_of_bounds_source_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a: [usize; 2] = [1, 2];
    let mut b: [usize; 3] = [0, 0, 0];
    unsafe { copy(a[0].&raw, b[0].&raw mut, 3) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `copy` out of bounds — the source names 3 element(s) from index 0, but the array has 2
        "#]],
    );
}

#[test]
fn copy_out_of_bounds_destination_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a: [usize; 3] = [1, 2, 3];
    let mut b: [usize; 2] = [0, 0];
    unsafe { copy(a[0].&raw, b[0].&raw mut, 3) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `copy` out of bounds — the destination names 3 element(s) from index 0, but the array has 2
        "#]],
    );
}

#[test]
fn copy_of_zero_elements_is_legal_through_any_pointer() {
    // Rust's rule, adopted: zero-length copies are valid through any
    // pointer, `dangling()` included — what lets a growing container copy
    // its 0 elements out of the never-allocated buffer.
    check_run(
        r#"
static main = fn () -> () {
    let mut a = [1];
    unsafe { copy(dangling::<usize>(), a[0].&raw mut, 0) };
    unsafe { copy(a[0].&raw, dangling::<usize>(), 0) };
};
"#,
        "main()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn copy_of_zero_elements_is_legal_through_a_pointer_to_a_scalar() {
    // The zero-length shortcut must hold for a pointer whose path does not
    // address an array element at all, not only for `dangling()` (which
    // happens to be shaped as element 0 of a registered, never-live
    // array) and array-element pointers: `write_range`'s effect must not
    // run when there is nothing to store, or this panics the interpreter
    // instead of answering the copy's rule.
    check_run(
        r#"
static main = fn () -> () {
    let mut a: usize = 1;
    let mut b: usize = 2;
    unsafe { copy(a.&raw, b.&raw mut, 0) };
    unsafe { copy(dangling::<usize>(), a.&raw mut, 0) };
};
"#,
        "main()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn copy_into_freed_allocation_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a = [1, 2];
    let p = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { dealloc_array(p, 2) };
    unsafe { copy(a[0].&raw, p, 2) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_alloc_is_refused_in_const_contexts() {
    // The eager const fence (C04): const-check plants the editor's exact
    // message; the machine's own `NotConst` refusal stands behind it as
    // defense in depth.
    check_const(
        r#"
static x = match alloc_array::<usize>(1) {
    AllocResult::Ok(p) => 1,
    AllocResult::Err => 2,
};
"#,
        expect![[r#"
            x = error[Trap]: cannot allocate during compile-time evaluation: const-built heap values wait for an interning design
        "#]],
    );
}

#[test]
fn heap_dealloc_is_refused_in_const_contexts() {
    check_const(
        r#"
static x = const fn () -> () { unsafe { dealloc_array(dangling::<usize>(), 1) } };
static y: usize = { x(); 1 };
"#,
        expect![[r#"
            x = fn
            y = error[Trap]: cannot deallocate during compile-time evaluation: const-built heap values wait for an interning design
        "#]],
    );
}

#[test]
fn add_copy_and_dangling_are_const_legal_on_locals() {
    // NOT fenced (they allocate nothing): pointer arithmetic and copies
    // over const-local storage work at compile time — the pointer-escape
    // rule stays the backstop for anything trying to leave.
    check_const(
        r#"
static x: usize = const {
    let mut a = [1, 2, 3];
    let p = a[0].&raw mut;
    unsafe { copy(p, add(p, 1), 2); };
    let d = dangling::<usize>();
    unsafe { add(p, 1).* }
};
"#,
        expect![[r#"
            x = 1
        "#]],
    );
}

#[test]
fn heap_pointer_cannot_escape_const_evaluation() {
    // The shipped escape rule, untouched, as the backstop: even if a heap
    // pointer were built during const eval (here: `dangling`, which is
    // not fenced), it may not reach the memoized result.
    check_const(
        "static p = dangling::<usize>();",
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn heapvec_push_growth_get_and_deinit_roundtrip() {
    // The proof program's core, as a machine test: growth (alloc + copy +
    // dealloc old) preserves elements across reallocation, checked get
    // reads through `add`, and one deinit frees the one live buffer.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_new = fn::<T>() -> HeapVec::<T> {
    HeapVec::<T>(struct { ptr = dangling::<T>(), len = 0, cap = 0 })
};
static heapvec_push = fn::<T>(mut v: HeapVec::<T>, x: T) -> HeapVec::<T> {
    if v.len == v.cap {
        let new_cap = if v.cap == 0 { 4 } else { v.cap * 2 };
        let fresh = match alloc_array::<T>(new_cap) {
            AllocResult::Ok(p) => p,
            AllocResult::Err => panic("heapvec_push: out of memory"),
        };
        unsafe {
            copy(v.ptr, fresh, v.len);
            if v.cap != 0 {
                dealloc_array(v.ptr, v.cap);
            };
        };
        v.ptr = fresh;
        v.cap = new_cap;
    };
    let slot = unsafe { add(v.ptr, v.len) };
    unsafe { slot.* = x; };
    v.len = v.len + 1;
    v
};
static heapvec_get = fn::<T>(v: HeapVec::<T>, i: usize) -> T {
    if i < v.len {
        unsafe { add(v.ptr, i).* }
    } else {
        panic("index out of bounds")
    }
};
static heapvec_deinit = fn::<T>(v: HeapVec::<T>) -> () {
    if v.cap != 0 {
        unsafe { dealloc_array(v.ptr, v.cap); };
    };
};
static main = fn () -> usize {
    let mut v = heapvec_new::<usize>();
    let mut i = 0;
    loop {
        if i == 6 { break; };
        v = heapvec_push(v, i * i);
        i = i + 1;
    };
    let picked = heapvec_get(v, 5) + heapvec_get(v, 1);
    heapvec_deinit(v);
    picked
};
"#,
        "main()",
        expect![[r#"
            => 26
        "#]],
    );
}

#[test]
fn heapvec_get_out_of_bounds_panics_like_the_future_index_desugar() {
    // `heapvec_get` is written as EXACTLY the future `a[i]` desugar
    // (check + panic + add + deref) — past `len` it panics, an
    // ordinary trap, never UB.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_get = fn::<T>(v: HeapVec::<T>, i: usize) -> T {
    if i < v.len {
        unsafe { add(v.ptr, i).* }
    } else {
        panic("index out of bounds")
    }
};
static main = fn () -> usize {
    let p = match alloc_array::<usize>(4) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = 1; };
    let v = HeapVec::<usize>(struct { ptr = p, len = 1, cap = 4 });
    heapvec_get(v, 3)
};
"#,
        "main()",
        expect![[r#"
            error[Panic]: index out of bounds
        "#]],
    );
}

#[test]
fn heapvec_double_deinit_is_detected_double_free() {
    // The v1 handle posture, honestly: copying the handle copies nothing
    // but the pointer, so a second deinit is a double free — detected,
    // deterministically, with the allocation's birth site.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_deinit = fn::<T>(v: HeapVec::<T>) -> () {
    if v.cap != 0 {
        unsafe { dealloc_array(v.ptr, v.cap); };
    };
};
static main = fn () -> () {
    let p = match alloc_array::<usize>(4) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let v = HeapVec::<usize>(struct { ptr = p, len = 0, cap = 4 });
    let w = v;
    heapvec_deinit(v);
    heapvec_deinit(w);
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: double free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn arena_carves_and_exhaustion_produces_err() {
    // The typed arena: ONE backing allocation carved by a bump cursor,
    // and a REAL `Err` producer for the result-shaped interface —
    // exhaustion is a value, not a trap.
    check_run(
        r#"
type ArenaState = struct::<T> { base: T.&raw mut, cap: usize, cursor: usize };
type Arena = struct::<T> { state: ArenaState::<T>.&raw mut };
static arena_new = fn::<T>(cap: usize) -> Arena::<T> {
    let state = match alloc_array::<ArenaState::<T>>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("arena_new: out of memory"),
    };
    let base = match alloc_array::<T>(cap) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("arena_new: out of memory"),
    };
    unsafe { state.* = ArenaState::<T>(struct { base = base, cap = cap, cursor = 0 }); };
    Arena::<T>(struct { state = state })
};
static arena_alloc = fn::<T>(a: Arena::<T>, n: usize) -> AllocResult::<T> {
    let cap = unsafe { a.state.*.cap };
    let cursor = unsafe { a.state.*.cursor };
    if cap - cursor < n {
        AllocResult::<T>::Err
    } else {
        unsafe {
            let p = add(a.state.*.base, cursor);
            a.state.*.cursor = cursor + n;
            AllocResult::<T>::Ok(p)
        }
    }
};
static arena_deinit = fn::<T>(a: Arena::<T>) -> () {
    unsafe {
        dealloc_array(a.state.*.base, a.state.*.cap);
        dealloc_array(a.state, 1);
    };
};
static main = fn () -> () {
    let a = arena_new::<usize>(4);
    match arena_alloc(a, 3) {
        AllocResult::Ok(p) => {
            let last = unsafe { add(p, 2) };
            unsafe { last.* = 7; };
            print("carved 3 of 4");
        }
        AllocResult::Err => print("unexpected exhaustion"),
    };
    match arena_alloc::<usize>(a, 2) {
        AllocResult::Ok(p) => print("unexpected fit"),
        AllocResult::Err => print("exhausted: Err, by value"),
    };
    match arena_alloc(a, 1) {
        AllocResult::Ok(p) => print("the last element still fits"),
        AllocResult::Err => print("unexpected exhaustion"),
    };
    arena_deinit(a);
};
"#,
        "main()",
        expect![[r#"
            output: "carved 3 of 4exhausted: Err, by valuethe last element still fits"
            => ()
        "#]],
    );
}

#[test]
fn arena_use_after_deinit_is_detected_ub() {
    // Carved pointers are paths into the ONE backing allocation, so they
    // all die together at `arena_deinit` — with the backing's birth site
    // in the note.
    check_run(
        r#"
static main = fn () -> usize {
    let base = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let carved = unsafe { add(base, 1) };
    unsafe { dealloc_array(base, 2) };
    unsafe { carved.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_allocations_of_records_project_like_any_memory() {
    // Heap elements are typed values: records allocated behind a pointer
    // support field writes through paths, exactly like promoted locals.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> usize {
    let p = match alloc_array::<Point>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = Point(struct { x = 1, y = 2 }); };
    unsafe { p.*.y = 40; };
    let got = unsafe { p.*.x + p.*.y };
    unsafe { dealloc_array(p, 1) };
    got
};
"#,
        "main()",
        expect![[r#"
            => 41
        "#]],
    );
}

#[test]
fn heap_field_write_through_uninit_element_is_detected_ub() {
    // Projecting INTO a never-written element is UB even as a write
    // target: the element's structure doesn't exist yet — write the whole
    // element first.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> () {
    let p = match alloc_array::<Point>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.*.x = 1; };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}
