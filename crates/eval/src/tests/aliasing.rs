//! The dynamic aliasing tree: borrows detected as UB at run time.

use super::*;

// ---- safe borrows: the dynamic aliasing check ---------------------------
//
// Static exclusivity is the loan checker's (`mir::loans`); the interpreter
// answers the same question dynamically, as depth. Every SAFE program here
// that traps is one the checker refuses too, and the trap is what stops it
// when it is run regardless; the ones that mint a raw pointer check clean
// (minting is no access, M08) and trap only here — that is the depth the
// fence does not reach. The launch configuration is the ruled one: Tree
// Borrows' structure with NO `Reserved` phase, so `&mut` starts `Unique`.

#[test]
fn a_borrow_reads_and_writes_the_place_it_borrows() {
    check_run(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize { let mut n: usize = 1; set(n.&mut, 9); n };
"#,
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn two_exclusive_borrows_of_one_place_is_detected_ub() {
    // The exclusive-write violation. Minting `b` is a write through the
    // parent, which disables the sibling `a`; writing through `a`
    // afterwards is the use of an invalidated borrow.
    check_run_past_the_cfg_checkers(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    let b = n.&mut;
    set(b, 2);
    set(a, 3);
    n
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

#[test]
fn using_a_child_after_writing_through_its_parent_is_detected_ub() {
    // Use-after-parent-invalidated. `child` is a reborrow of `a`; writing
    // through `a` is foreign to `child`, so `child` is disabled — and
    // disabling is transitive, which is what makes one check cover a whole
    // subtree.
    check_run_past_the_cfg_checkers(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn::<@a>(m: usize.&mut::<@a>) -> () {
    let child = m.*.&mut;
    set(m, 5);
    set(child, 9);
};
static g = fn () -> usize { let mut n: usize = 1; f(n.&mut); n };
"#,
        "g()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn many_shared_borrows_of_one_place_are_fine() {
    // The shared regime: any number of readers, no invalidation. A shared
    // creation is a READ through the parent, which freezes rather than
    // disables.
    check_run(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn () -> usize {
    let n: usize = 4;
    let a = n.&;
    let b = n.&;
    get(a) + get(b)
};
"#,
        "f()",
        expect![[r#"
            => 8
        "#]],
    );
}

#[test]
fn a_nested_literal_frame_escape_is_caught_dynamically() {
    // A borrow returned at `@_` from a nested literal never reaches a
    // universal of the ENCLOSING item; the loan checker refuses it because
    // the loan is live where the literal's body returns (mir's
    // `a_nested_literal_borrow_of_its_own_local_cannot_escape`). Run
    // regardless, the interpreter catches it too: the borrowed local's
    // storage really is gone once the nested literal's own frame returns.
    check_run_past_the_cfg_checkers(
        r#"
static main = fn () -> usize {
    let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };
    f().*
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn a_raw_only_program_never_touches_the_aliasing_tree() {
    // Scope, pinned: a raw-only allocation (never covered by a safe
    // borrow) has no root to inherit, so its raw pointers resolve to
    // nothing and never touch the tree. So the aliasing model costs
    // raw-pointer-only programs exactly nothing, and cannot have changed
    // one — which is what makes this an addition rather than a
    // semantics change.
    check_run(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let p = n.&raw mut;
    let q = n.&raw mut;
    unsafe { p.* = 2; q.* = 3; p.* }
};
"#,
        "f()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn disjoint_field_borrows_of_one_struct_coexist() {
    // `p.x.&mut` and `p.y.&mut` are minted straight off the local's own
    // name (no deref): each node's path is exactly its own field, and
    // `paths_overlap` says disjoint fields never interact — so writing
    // through one, then the other, is not exclusivity violation, unlike
    // two borrows of the SAME field (see
    // `two_exclusive_borrows_of_one_place_is_detected_ub`).
    check_run(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let bx = p.x.&mut;
    let by = p.y.&mut;
    set(bx, 10);
    set(by, 20);
    p.x + p.y
};
"#,
        "f()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn writing_a_disjoint_sibling_field_does_not_disturb_a_borrow() {
    // The direct-write-by-name route (`write_place`'s promoted-local
    // branch) must carry the WRITE's own path too: `p.y = 5;` writes
    // through the root with path `[y]`, foreign to a borrow of `[x]` only
    // because the paths are disjoint.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let bx = p.x.&mut;
    p.y = 50;
    bx.* = 9;
    p.x + p.y
};
"#,
        "f()",
        expect![[r#"
            => 59
        "#]],
    );
}

#[test]
fn a_sibling_field_access_through_a_nested_pointer_runs_clean() {
    // The static fence accepts this shape (mir's loan tests: the copy of
    // `bb.*` into the temp `bb.*.*` is spelled through is no access);
    // the tree agrees, `bb.*.*.f` and `bb.*.*.g` being disjoint paths
    // through the same root.
    check_run(
        r#"
type P = struct { f: usize, g: usize };
static f = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize {
    let x = bb.*.*.g.&mut;
    let y = bb.*.*.f;
    bb.*.*.f = 3;
    x.* = x.* + y;
    x.* + bb.*.*.f
};
static main = fn () -> usize {
    let mut p: P = P(struct { f = 1, g = 2 });
    let mut q = p.&mut;
    f(q.&mut)
};
"#,
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn two_exclusive_borrows_of_the_same_field_is_still_detected_ub() {
    // Regression for the per-location fix: SAME field, not disjoint ones
    // — `paths_overlap` says `[x]`/`[x]` overlap, so this must still be
    // the ordinary exclusivity violation.
    check_run_past_the_cfg_checkers(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let a = p.x.&mut;
    let b = p.x.&mut;
    set(b, 2);
    set(a, 3);
    p.x
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

#[test]
fn a_whole_struct_borrow_still_dominates_every_field() {
    // Regression: a borrow of path `[]` (the whole struct) must still be
    // foreign to (and disable, and be disabled by) a borrow of any one
    // field — `paths_overlap` treats the empty path as a prefix of
    // everything.
    check_run_past_the_cfg_checkers(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let whole = p.&mut;
    let field = p.x.&mut;
    set(field, 9);
    whole.* = struct { x = 0, y = 0 };
    p.x
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

#[test]
fn disjoint_array_element_borrows_coexist() {
    check_run_past_the_cfg_checkers(
        r#"
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn () -> usize {
    let mut a: [usize; 2] = [1, 2];
    let b0 = a[0].&mut;
    let b1 = a[1].&mut;
    set(b0, 10);
    set(b1, 20);
    a[0] + a[1]
};
"#,
        "f()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn reading_one_field_through_a_wide_borrow_does_not_kill_a_sibling_field_child() {
    // The read-side per-location fix: `b` is a WIDE reborrow (path `[]`,
    // the whole struct); reading `b.*.x` must be checked with path `[x]`
    // — the field the read actually touches, extended past the deref —
    // not `b`'s own `[]` path, which would make every read through `b`
    // foreign to every child of `b`, however disjoint. Rust accepts the
    // equivalent (`let c = &mut b.y; let v = b.x; *c = 5;`).
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let b = p.&mut;
    let c = b.*.y.&mut;
    let v = b.*.x;
    c.* = 5;
    v + p.y
};
"#,
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_one_element_through_a_wide_borrow_does_not_kill_a_sibling_element_child() {
    check_run_past_the_cfg_checkers(
        r#"
static f = fn () -> usize {
    let mut a: [usize; 2] = [1, 2];
    let b = a.&mut;
    let c = b.*[1].&mut;
    let v = b.*[0];
    c.* = 9;
    v + a[1]
};
"#,
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn reading_a_nested_field_through_a_wide_borrow_does_not_kill_a_sibling_child() {
    // Two derefs deep: `by` is a reborrow of `p.y` (path `[y]`), `cw` a
    // child of `by` over `w` (path `[y, w]`). Reading `by.*.u` must be
    // checked with path `[y, u]` — disjoint from `[y, w]` — not `by`'s
    // own `[y]`.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { y: struct { u: usize, w: usize } } =
    struct { y = struct { u = 1, w = 2 } };
    let by = p.y.&mut;
    let cw = by.*.w.&mut;
    let v = by.*.u;
    cw.* = 9;
    v + p.y.w
};
"#,
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn reading_a_sibling_field_by_name_does_not_kill_a_field_child() {
    // The plainest shape in the language: a two-field struct, one field
    // borrowed, the OTHER read by name. `p.y` reads through the
    // allocation's root, so the path it carries has to be `[y]` — which
    // means the read must lower as one projected place
    // (`lower_place_read`), not as a copy of the whole `p` followed by a
    // field extraction. Rust accepts the equivalent.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let bx = p.x.&mut;
    let v = p.y;
    bx.* = 9;
    p.x + p.y + v
};
"#,
        "f()",
        expect![[r#"
            => 13
        "#]],
    );
}

#[test]
fn reading_a_sibling_element_by_name_does_not_kill_an_element_child() {
    check_run_past_the_cfg_checkers(
        r#"
static f = fn () -> usize {
    let mut a: [usize; 3] = [1, 2, 3];
    let b0 = a[0].&mut;
    let v = a[1];
    b0.* = 9;
    a[0] + v
};
"#,
        "f()",
        expect![[r#"
            => 11
        "#]],
    );
}

#[test]
fn reading_a_nested_field_by_name_does_not_kill_a_sibling_child() {
    // Two steps deep, no deref anywhere: `p.x.y` must carry `[x, y]`,
    // disjoint from the borrow's `[x, z]`.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: struct { y: usize, z: usize }, w: usize } =
    struct { x = struct { y = 1, z = 2 }, w = 3 };
    let c = p.x.z.&mut;
    let v = p.x.y;
    c.* = 5;
    v + p.x.z
};
"#,
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_a_disjoint_top_level_field_does_not_kill_a_deeper_child() {
    // The read is SHALLOWER than the borrow: `p.z` carries `[z]`, the
    // borrow `[a, q]`. Neither is a prefix of the other.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { a: struct { q: usize, r: usize }, z: usize } =
    struct { a = struct { q = 1, r = 2 }, z = 3 };
    let c = p.a.q.&mut;
    let v = p.z;
    c.* = 9;
    v + p.a.q
};
"#,
        "f()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn reading_a_field_of_an_element_by_name_does_not_kill_its_sibling() {
    // An index step then a field step: `a[0].x` carries `[0, x]`.
    check_run(
        r#"
static f = fn () -> usize {
    let mut a: [struct { x: usize, y: usize }; 2] =
    [struct { x = 1, y = 2 }, struct { x = 3, y = 4 }];
    let c = a[0].y.&mut;
    let v = a[0].x;
    c.* = 5;
    v + a[0].y
};
"#,
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_two_steps_past_a_deref_narrows_to_the_field_it_touches() {
    // The deref-rooted twin of the tests above: the chain past `b.*` is
    // TWO steps long, so the path is `[x, y]` — a chain does not stop
    // narrowing one step after the deref.
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: struct { y: usize, z: usize }, w: usize } =
    struct { x = struct { y = 1, z = 2 }, w = 3 };
    let b = p.&mut;
    let c = b.*.x.z.&mut;
    let v = b.*.x.y;
    c.* = 5;
    v + p.x.z
};
"#,
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_the_same_field_by_name_still_suspends_an_exclusive_borrow() {
    // The other side of the narrowing: `p.x` read by name carries `[x]`,
    // which OVERLAPS the borrow's `[x]` — a shared use of a place with a
    // live exclusive borrow of it, and writing through that borrow
    // afterwards is the ordinary violation.
    check_run_past_the_cfg_checkers(
        r#"
static f = fn () -> usize {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    let bx = p.x.&mut;
    let v = p.x;
    bx.* = 9;
    p.x + v
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
fn a_borrow_behind_a_borrow_reads_and_writes_the_root() {
    // Two borrow steps: the inner `bb.*` is an ordinary pointer LOAD, and
    // the loaded value carries the tag `b.&mut` minted, so the write
    // through it lands on `n`'s own allocation as a CHILD of that node
    // rather than as a stranger to it.
    check_run(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let mut b = n.&mut;
    let bb = b.&mut;
    bb.*.* = 5;
    bb.*.* + n
};
"#,
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn a_raw_write_past_a_live_safe_borrow_is_detected_ub() {
    // `n.&raw mut` is minted from a bare local's name AFTER a safe borrow
    // already covers `n` — it inherits `n`'s existing root (not a fresh
    // inert node), so the raw write is exactly as foreign to `r` as
    // `n = 9;` would be.
    check_run(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let r = n.&mut;
    let p = n.&raw mut;
    unsafe { p.* = 9; }
    r.*
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
fn a_raw_pointer_minted_before_the_safe_borrow_still_shares_its_root() {
    // The raw pointer is minted FIRST, while `Provenance(None)` still has
    // no root to resolve against; the safe borrow minted after it is what
    // creates the root. The raw write must still resolve lazily, at the
    // moment of the access, against whatever root exists then.
    check_run(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let p = n.&raw mut;
    let r = n.&mut;
    unsafe { p.* = 9; }
    r.*
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
fn a_raw_reborrow_through_a_live_mut_borrow_stays_clean() {
    // A raw reborrow ROUTED THROUGH the safe borrow (`r.*.&raw mut`, not
    // straight off `n`'s name) inherits `r`'s own node rather than the
    // root, per the `.&raw`-is-not-a-decayed-borrow rule — so it does not
    // disturb `r` at all.
    check_run(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let r = n.&mut;
    let p = unsafe { r.*.&raw mut };
    unsafe { p.* = 9; }
    r.*
};
"#,
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn copy_through_the_destination_is_foreign_to_a_live_borrow_of_an_element() {
    // `copy` is memmove, not a route around the aliasing tree: a live
    // safe borrow of an element the copy overwrites must be invalidated
    // exactly as `a[0] = 7;` would invalidate it.
    check_run(
        r#"
static f = fn () -> usize {
    let mut src: [usize; 1] = [7];
    let mut a: [usize; 2] = [1, 2];
    let m = a[0].&mut;
    unsafe { copy(src[0].&raw, a[0].&raw mut, 1); }
    m.*
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
fn copy_through_the_source_is_foreign_to_a_live_borrow_of_an_element() {
    // The other half of the same claim: the SOURCE range is read, element
    // by element, so a live exclusive borrow of an element the copy reads
    // is suspended exactly as `let v = a[0];` would suspend it.
    check_run(
        r#"
static f = fn () -> usize {
    let mut a: [usize; 2] = [1, 2];
    let mut dst: [usize; 1] = [0];
    let m = a[0].&mut;
    unsafe { copy(a[0].&raw, dst[0].&raw mut, 1); }
    m.* = 5;
    a[0]
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
fn copy_reports_a_dead_source_tag_before_a_destination_bounds_fault() {
    // The observable half of `builtin_copy`'s "source before destination"
    // order: a source whose OWN TAG is no longer valid — not merely
    // foreign to a still-live borrow, which would only suspend it — faults
    // at the read itself, so it is reported even though the destination
    // (one element, asked to receive two) is also out of bounds.
    check_run(
        r#"
static f = fn () -> u8 {
    let mut a: [u8; 2] = [1, 2];
    let mut b: [u8; 1] = [0];
    let m = a.&mut;
    let q = m.*[0].&raw;
    a[0] = 7;
    unsafe { copy(q, b[0].&raw mut, 2); }
    b[0]
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
fn a_borrow_tag_is_not_part_of_pointer_equality() {
    // Two pointers to the same place must compare EQUAL however they were
    // derived — one through a `.&mut` (so it carries that borrow's
    // aliasing node) and one straight off the local (so it carries the
    // untracked root). A tag is aliasing bookkeeping, and letting it reach
    // `==` would turn bookkeeping into an observable program result,
    // which is exactly what an aliasing model must never do.
    check_run(
        r#"
static f = fn () -> bool {
    let mut n: usize = 1;
    let borrowed = n.&mut;
    let via_borrow = borrowed.*.&raw mut;
    let direct = n.&raw mut;
    via_borrow == direct
};
"#,
        "f()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn a_direct_write_to_a_borrowed_local_invalidates_the_borrow() {
    // A write to a local BY ITS OWN NAME must reach the aliasing tree
    // exactly as a write through a deref-rooted place does — the most
    // ordinary exclusivity violation in the language, not a special case.
    check_run_past_the_cfg_checkers(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let r = n.&;
    n = 99;
    r.*
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
fn a_direct_read_of_a_borrowed_local_freezes_an_exclusive_borrow() {
    // A foreign read through the root demotes an exclusive child to
    // shared, so a later write through it is caught. And the message
    // must name the STATE that rejected the write (`Frozen`, from a
    // foreign read), not the borrow's own flavor — `m` here IS a
    // `.&mut`, so "write through a shared borrow" would be false of it.
    check_run_past_the_cfg_checkers(
        r#"
static f = fn () -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let copy: usize = n;
    m.* = 5;
    copy
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
fn an_implicit_degradation_mints_a_real_child_node() {
    // M07 says degradation is not spelled `v.*.&` "because the explicit
    // form produces exactly the same child node" — a claim about the IR
    // that an implicit reborrow must actually make true: it needs its own
    // MIR operation, so `s` and `m` mint distinct tags with a real
    // parent/child relation to violate.
    check_run_past_the_cfg_checkers(
        r#"
static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };
static f = fn () -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let s: usize.&::<@_> = m;
    m.* = 7;
    get(s)
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
fn dynamic_ub_names_the_borrow_site_and_the_invalidating_site() {
    // The report must name both interesting sites even when the trap
    // fires inside a callee that has no borrow sites of its own — both
    // the borrow site and the invalidating site live in the caller here.
    check_run_past_the_cfg_checkers(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn () -> () {
    let mut n: usize = 1;
    let a = n.&mut;
    let b = n.&mut;
    bump(b);
    bump(a);
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

#[test]
fn a_read_only_borrow_loop_does_not_accumulate_scan_cost() {
    // A read can only demote `Unique`, so it never has to look at a node
    // that is already settled — a read-only borrow loop must not rescan
    // every borrow the program has ever made. Pinned behaviorally: the
    // loop must still produce the right answer with several thousand
    // live shared borrows outstanding.
    check_run(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn () -> usize {
    let n: usize = 3;
    let mut acc: usize = 0;
    let mut i: usize = 0;
    loop {
        acc = acc + get(n.&);
        i = i + 1;
        if i == 2000 { break } else { }
    };
    acc
};
"#,
        "f()",
        expect![[r#"
            => 6000
        "#]],
    );
}

// ---- match projects through borrows: aliasing and the tag read ----------
//
// The static story is in `hir`; what runs here is the two things only the
// interpreter can show. A payload binding really points INTO the matched
// value (so a write through it is visible to its owner), and the tag test
// really is an access through the scrutinee's node (so an invalidated
// scrutinee is caught at the `match`, not at the first arm that uses a
// binding).

#[test]
fn a_mut_payload_binding_aliases_the_matched_value() {
    // THE aliasing proof. `bump` never sees the option by value; its write
    // lands in `main`'s storage, and the owner reads it back. Any answer
    // other than 101 would mean the binding was a copy.
    check_run(
        r#"
type Opt = enum::<T> { Some(T), None };
static bump = fn::<@a>(o: Opt::<usize>.&mut::<@a>) -> () {
    match o { ::Some(t) => { t.* = t.* + 100; }, ::None => {} }
};
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    bump(o.&mut);
    match o { ::Some(n) => n, ::None => 0 }
};
"#,
        "f()",
        expect![[r#"
            => 101
        "#]],
    );
}

#[test]
fn two_mut_payload_bindings_of_one_variant_are_independent() {
    // Two exclusive borrows of two payload SLOTS, live at once. This is
    // only sound — and only accepted — because the aliasing tree is
    // path-granular: minting `y` is a write at the second slot, which the
    // borrow of the first cannot see.
    check_run(
        r#"
type Pair = enum { Both(usize, usize), Neither };
static go = fn::<@a>(p: Pair.&mut::<@a>) -> () {
    match p { ::Both(x, y) => { x.* = 7; y.* = 9; }, ::Neither => {} }
};
static f = fn() -> usize {
    let mut p: Pair = ::Both(1, 2);
    go(p.&mut);
    match p { ::Both(a, b) => a + b, ::Neither => 0 }
};
"#,
        "f()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn a_containing_borrow_still_conflicts_with_a_payload_borrow() {
    // Paths OVERLAP when one contains the other, so a borrow of the whole
    // option and a borrow of its payload are not disjoint: minting the
    // second is a write the first can see. (`[]` — the allocation root —
    // contains everything, which is what keeps a write by the local's own
    // name reaching every borrow into it.)
    check_run_past_the_cfg_checkers(
        r#"
type Opt = enum::<T> { Some(T), None };
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    let inner = match o.&mut { ::Some(t) => t, ::None => panic("none") };
    o = Opt::<usize>::Some(5);
    set(inner, 9);
    0
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

#[test]
fn the_tag_read_of_a_shared_borrowed_match_goes_through_the_tree() {
    // The tag test is an access, and this is it firing. `r` was
    // invalidated by a direct write to the local; the `match` itself is
    // the undefined behavior, before any arm body runs. Lowering the tag
    // read as a copy of a detached value — which is what the owned path
    // does — would have let this run clean.
    check_run_past_the_cfg_checkers(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    let r = o.&;
    o = Opt::<usize>::None;
    match r { ::Some(_) => 1, ::None => 0 }
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
fn the_tag_read_of_an_exclusive_borrowed_match_is_a_shared_freeze() {
    // The `.&mut` flavor, ISOLATED: the second `match` dispatches on the
    // tag but binds nothing (`::None` carries no payload, `_` binds the
    // pointer), so the only access it performs is the tag read.
    //
    // That read goes through `m`, and `inner` — a payload borrow minted
    // under `m` — is foreign to it: a foreign read FREEZES an exclusive
    // node rather than disabling it, so writing through `inner` afterwards
    // is the suspended-by-a-read case, which gets its own message. Reading
    // the tag off a detached copy would have done none of this.
    check_run_past_the_cfg_checkers(
        r#"
type Opt = enum::<T> { Some(T), None };
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    let m = o.&mut;
    let inner = match m { ::Some(t) => t, ::None => panic("none") };
    let tag = match m { ::None => 0, _ => 7 };
    set(inner, 9);
    tag
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
fn a_borrowed_match_that_dispatches_on_nothing_reads_nothing() {
    // The control for the test above, and the other half of the rule: a
    // `match` whose arms make no tag decision does not read the tag, so it
    // is not an access at all. Same program, with `_` back in place of the
    // two variant arms — `inner` survives it and the write lands.
    check_run_past_the_cfg_checkers(
        r#"
type Opt = enum::<T> { Some(T), None };
static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    let m = o.&mut;
    let inner = match m { ::Some(t) => t, ::None => panic("none") };
    let tag = match m { _ => 7 };
    set(inner, 9);
    tag + match o { ::Some(n) => n, ::None => 0 }
};
"#,
        "f()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn projecting_twice_reaches_the_inner_payload() {
    // Transitivity, running: the binding is a borrow, so matching IT
    // projects again, and the write lands two levels down in the owner.
    check_run(
        r#"
type Opt = enum::<T> { Some(T), None };
static bump = fn::<@a>(o: Opt::<Opt::<usize>>.&mut::<@a>) -> () {
    match o {
        ::Some(inner) => match inner {
            ::Some(n) => { n.* = n.* + 1; },
            ::None => {},
        },
        ::None => {},
    }
};
static f = fn() -> usize {
    let mut o: Opt::<Opt::<usize>> =
    Opt::<Opt::<usize>>::Some(Opt::<usize>::Some(6));
    bump(o.&mut);
    match o {
        ::Some(inner) => match inner { ::Some(n) => n, ::None => 0 },
        ::None => 0,
    }
};
"#,
        "f()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn an_owned_match_still_copies_its_payloads_out() {
    // The owned path, running unchanged: the binding is the VALUE, so
    // writing to a mutable copy of it cannot touch the scrutinee.
    check_run(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn() -> usize {
    let o: Opt::<usize> = Opt::<usize>::Some(1);
    let copied = match o { ::Some(t) => t, ::None => 0 };
    let again = match o { ::Some(t) => t, ::None => 0 };
    copied + again
};
"#,
        "f()",
        expect![[r#"
            => 2
        "#]],
    );
}

// ---- containment, from the borrows a projection makes ------------------
//
// Reads are path-exact (a disjoint field read leaves a sibling borrow
// alone), and the other half of that rule is that CONTAINMENT in either
// direction is still a conflict. These pin it over the shapes only a
// borrowed match produces — a payload borrow under a whole-value read —
// plus the two containment cases the sibling-read tests above do not
// reach.

#[test]
fn reading_the_whole_value_still_freezes_a_payload_borrow() {
    // CONTAINMENT, the read covering the borrow: `m.*` reads all of the
    // option, which includes the payload slot `t` points at. The borrow is
    // frozen by it, so the later write through `t` is the
    // suspended-by-a-read case. Path exactness must not have turned this
    // off — a shorter path covers more, and `[]` covers everything.
    check_run_past_the_cfg_checkers(
        r#"
type Opt = enum::<T> { Some(T), None };
static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };
static f = fn() -> usize {
    let mut o: Opt::<usize> = Opt::<usize>::Some(1);
    let m = o.&mut;
    let t = match m { ::Some(t) => t, ::None => panic("none") };
    let whole = m.*;
    set(t, 9);
    match whole { ::Some(n) => n, ::None => 0 }
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
fn reading_through_a_prefix_still_freezes_the_borrow_below_it() {
    // CONTAINMENT one level in: the borrow is of `w.*.inner.b`, the read is
    // of `w.*.inner` — a strict PREFIX of the borrow's path, so it contains
    // it and the freeze fires. Overlap is prefix-either-way, not equality.
    check_run_past_the_cfg_checkers(
        r#"
type Inner = struct { a: usize, b: usize };
type W = struct { inner: Inner, other: usize };
static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };
static f = fn::<@a>(w: W.&mut::<@a>) -> usize {
    let bb = w.*.inner.b.&mut;
    let seen = w.*.inner;
    set(bb, 3);
    seen.a
};
static g = fn() -> usize {
    let mut w: W = W(struct { inner = Inner(struct { a = 1, b = 2 }), other = 0 });
    f(w.&mut)
};
"#,
        "g()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn reading_a_local_by_its_bare_name_still_reaches_every_borrow_into_it() {
    // The root read, unchanged: a place with no element steps at all is the
    // empty path, which contains everything. `let r = n.&; n = 99; r.*` —
    // the ordinary exclusivity bug — must still be caught, and a bare-name
    // read of a borrowed STRUCT must still reach a borrow of one field.
    check_run_past_the_cfg_checkers(
        r#"
type W = struct { a: usize, b: usize };
static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };
static f = fn() -> usize {
    let mut w: W = W(struct { a = 1, b = 2 });
    let bb = w.b.&mut;
    let whole = w;
    set(bb, 3);
    whole.a
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
fn a_raw_pointer_minted_from_a_borrow_is_path_filtered_too() {
    // M08's raw-pointer consequence, under path-granular nodes: a
    // `.&raw mut` minted through a borrow inherits the borrow's NODE
    // (`.&raw` is not a decayed safe borrow), and its PATH comes along
    // with it — so a raw write at a disjoint slot does not kill a sibling
    // borrow. Correct and consistent: the raw pointer speaks through the
    // same node, at its own location.
    check_run(
        r#"
type W = struct { a: usize, b: usize };
static f = fn::<@a>(w: W.&mut::<@a>) -> usize {
    let bb = w.*.b.&mut;
    let ra = w.*.a.&raw mut;
    unsafe { ra.* = 7; };
    bb.* = 3;
    w.*.a + w.*.b
};
static g = fn() -> usize {
    let mut w: W = W(struct { a = 1, b = 2 });
    f(w.&mut)
};
"#,
        "g()",
        expect![[r#"
            => 10
        "#]],
    );
}
