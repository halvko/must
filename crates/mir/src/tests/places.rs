//! Assignment, records and field projections.

use super::*;

#[test]
fn assignment_reuses_the_lets_local() {
    // `x`'s local (`_1`) is allocated once, by the `let`; the assignment
    // writes into that same slot rather than minting a new one.
    check_mir(
        "static f = fn () -> usize { let mut x = 1; x = 2; x };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize
              bb0:
                _1 = 1
                _1 = 2
                _2 = _1
                _0 = _2
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
fn assignment_to_a_captured_local_is_diagnosed_and_trapped() {
    // Mirrors `capture_is_diagnosed_and_trapped`: writing to a local of an
    // enclosing function is exactly as unsupported as reading it.
    check_mir(
        "static f = fn () -> usize { let mut a: usize = 1; let g = fn () -> usize { a = 2; 0 }; g() };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                _1 = trap "`a` is a local of an enclosing function; captures are not supported yet" -> bb1
              bb1:
                _0 = 0
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: usize  // a
              _2: fn() -> usize  // g
              _3: fn() -> usize
              _4: usize
              bb0:
                _1 = 1
                _2 = fn b0
                _3 = _2
                _4 = call _3() -> bb1
              bb1:
                _0 = _4
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
            mir diagnostic at 75..76: `a` is a local of an enclosing function; captures are not supported yet
        "#]],
    );
}

#[test]
fn assignment_to_an_immutable_binding_traps_with_the_diagnostic_message() {
    // The RHS still evaluates (the CFG keeps everything); the write itself
    // is replaced by a trap carrying inference's exact squiggle text.
    check_mir(
        "static f = fn () -> usize { let x = 1; x = 2; x };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize
              _3: usize
              bb0:
                _1 = 1
                _2 = trap "cannot assign to `x`: it is not declared `mut`" -> bb1
              bb1:
                _3 = _1
                _0 = _3
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
fn record_literal_evaluates_fields_in_source_order_then_aggregates_in_sorted_order() {
    // Field initializers evaluate in the order they're written — `y` calls
    // `g` before `x` calls `f` — but the aggregate itself lists operands in
    // the type's canonical (sorted) order: `_3`/`_2` are just referenced in
    // `x, y` order once both have already been computed.
    check_mir(
        r#"
static f = fn () -> usize { 1 }
static g = fn () -> usize { 2 }
static r = fn { struct { y = g(), x = f() } };
"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 1
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item g:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 2
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item r:
            fn b0() -> struct { x: usize, y: usize } {
              _0: struct { x: usize, y: usize }  // return
              _1: usize
              _2: usize
              _3: struct { x: usize, y: usize }
              bb0:
                _1 = call item g() -> bb1
              bb1:
                _2 = call item f() -> bb2
              bb2:
                _3 = { x: _2, y: _1 }
                _0 = _3
                return
            }
            fn b1() -> fn() -> struct { x: usize, y: usize } {
              _0: fn() -> struct { x: usize, y: usize }  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn record_field_access_lowers_to_a_positional_projection() {
    check_mir(
        r#"static f = fn { let p: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; p.y };"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: struct { x: usize, y: usize }
              _2: struct { x: usize, y: usize }  // p
              _3: usize
              bb0:
                _1 = { x: 1, y: 2 }
                _2 = _1
                _3 = _2.1
                _0 = _3
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
fn field_access_on_a_nonexistent_field_still_traps() {
    // Records have a real MIR story now, but a field inference rejected is
    // exactly as trapped as before — the diagnostic just isn't
    // `UnsupportedRecord` anymore.
    check_mir(
        r#"static f = fn { let p = struct { x = "s" }; p.y };"#,
        expect![[r#"
            item f:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: struct { x: str }
              _2: struct { x: str }  // p
              _3: {error}
              bb0:
                _1 = { x: "s" }
                _2 = _1
                _3 = trap "no field `y` on `struct { x: str }`" -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b1() -> fn() -> {error} {
              _0: fn() -> {error}  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn nested_field_assign_lowers_to_a_place_projection() {
    // `p.a.b = 2;` writes through a Place: the root's local plus the
    // field-index path (innermost first — `a`'s index in `p`, then `b`'s
    // in `p.a`), rendered `_3.0.0`. The RHS is evaluated before the write,
    // like every assignment. The trailing READ of the same chain is the
    // SAME projection (`_4 = _3.0.0`, not a copy of `p.a` followed by a
    // field extraction) — see `lower_place_read`.
    check_mir(
        "static f = fn () -> usize { let mut p = struct { a = struct { b = 1 } }; p.a.b = 2; p.a.b };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: struct { b: usize }
              _2: struct { a: struct { b: usize } }
              _3: struct { a: struct { b: usize } }  // p
              _4: usize
              bb0:
                _1 = { b: 1 }
                _2 = { a: _1 }
                _3 = _2
                _3.0.0 = 2
                _4 = _3.0.0
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

#[test]
fn field_assign_on_an_immutable_root_traps() {
    // Inference rejected the root; the write is replaced by a trap with
    // the squiggle's exact text (squiggle-equals-crash).
    check_mir(
        "static f = fn () -> usize { let p = struct { x = 1 }; p.x = 2; p.x };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: struct { x: usize }
              _2: struct { x: usize }  // p
              _3: struct { x: usize }
              _4: usize
              bb0:
                _1 = { x: 1 }
                _2 = _1
                _3 = trap "cannot assign to `p.x`: `p` is not declared `mut`" -> bb1
              bb1:
                _4 = _2.0
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

#[test]
fn assignment_to_a_non_variable_traps_with_the_validation_message() {
    // Validation squiggled the LHS with "can only assign to a variable or
    // its fields"; the trap borrows that exact text via the shared
    // constant.
    check_mir(
        "static f = fn () -> usize { let mut x = 1; x + 1 = 2; x };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize
              _3: usize
              bb0:
                _1 = 1
                _2 = trap "can only assign to a variable or its fields" -> bb1
              bb1:
                _3 = _1
                _0 = _3
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
fn named_type_constructor_erases_in_mir() {
    // `Foo(struct { x = 1 })` lowers to the record aggregate alone — no
    // call, no tag: nominal types exist only in the static type system.
    // The `type` item itself lowers to nothing (no root body).
    check_mir(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = 1 });
"#,
        expect![[r#"
            item Foo:
            item p:
            fn b0() -> Foo {
              _0: Foo  // return
              _1: struct { x: usize }
              bb0:
                _1 = { x: 1 }
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn field_access_through_named_type_projects_by_declared_order() {
    check_mir(
        r#"
type Pair = struct { b: usize, a: usize };
static second = fn (p: Pair) -> usize { p.b };
"#,
        expect![[r#"
            item Pair:
            item second:
            fn b0(_1: Pair) -> usize {
              _0: usize  // return
              _1: Pair  // param p
              _2: usize
              bb0:
                _2 = _1.1
                _0 = _2
                return
            }
            fn b1() -> fn(Pair) -> usize {
              _0: fn(Pair) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}
