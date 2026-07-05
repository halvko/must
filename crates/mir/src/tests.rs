use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

/// Renders every item's lowered MIR, followed by MIR's own diagnostics with
/// ranges attached the way the aggregator does it (via the source map).
fn check_mir(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        rendered.push_str(&format!(
            "item {}:\n",
            if name.is_empty() { "<unnamed>" } else { name }
        ));
        let lowered = crate::mir_lowered(&db, item);
        rendered.push_str(&crate::pretty::render(lowered));
        let (_, source_map) = hir::body_with_source_map(&db, item);
        for diag in &lowered.diagnostics {
            if let Some(ptr) = source_map.node_for_expr(diag.expr()) {
                rendered.push_str(&format!(
                    "mir diagnostic at {:?}: {}\n",
                    ptr.text_range(),
                    diag.message()
                ));
            }
        }
    }
    expect.assert_eq(&rendered);
}

#[test]
fn const_initializer_lowers_to_a_const_body() {
    check_mir(
        "static example = 4 + 5;",
        expect![[r#"
        item example:
        fn b0() -> usize {
          _0: usize  // return
          _1: usize
          bb0:
            _1 = Add(4, 5)
            _0 = _1
            return
        }
    "#]],
    );
}

#[test]
fn fn_items_lower_root_as_const_fn_value() {
    check_mir(
        r#"static main = fn { print("hi"); };"#,
        expect![[r#"
        item main:
        fn b0() -> () {
          _0: ()  // return
          _1: ()
          bb0:
            _1 = call builtin print("hi") -> bb1
          bb1:
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
fn if_else_keeps_the_condition_materialized() {
    check_mir(
        r#"
static classify = fn (n: usize) -> str {
    let label = if n < 10 { "small" } else { "big" };
    label
};
"#,
        expect![[r#"
            item classify:
            fn b0(_1: usize) -> str {
              _0: str  // return
              _1: usize  // param n
              _2: bool
              _3: str
              _4: str  // label
              bb0:
                _2 = Lt(_1, 10)
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _3 = "small"
                goto -> bb3
              bb2:
                _3 = "big"
                goto -> bb3
              bb3:
                _4 = _3
                _0 = _4
                return
            }
            fn b1() -> fn(usize) -> str {
              _0: fn(usize) -> str  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn if_without_else_produces_unit_on_the_false_edge() {
    check_mir(
        r#"static f = fn (c: bool) { if c { print("y") } };"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> () {
              _0: ()  // return
              _1: bool  // param c
              _2: ()
              _3: ()
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _3 = call builtin print("y") -> bb3
              bb2:
                _2 = ()
                goto -> bb4
              bb3:
                _2 = _3
                goto -> bb4
              bb4:
                _0 = _2
                return
            }
            fn b1() -> fn(bool) {
              _0: fn(bool)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn let_shadowing_gets_distinct_locals() {
    check_mir(
        "static f = fn () -> usize { let x = 1; let x = x + 1; x };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: usize
              _3: usize  // x
              bb0:
                _1 = 1
                _2 = Add(_1, 1)
                _3 = _2
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
fn diverging_call_has_no_return_target() {
    check_mir(
        r#"static f = fn () -> usize { panic("boom"); 1 };"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: !
              bb0:
                _1 = call builtin panic("boom") -> !
              bb1:
                _0 = 1
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
fn diverging_call_in_if_branch_leaves_a_dead_continuation() {
    // The `panic` call ends bb1 with no return target: its continuation bb3
    // has no predecessors, and the join bb4 keeps a single live one (bb2).
    check_mir(
        r#"static f = fn (c: bool) -> usize { let v = if c { panic("x") } else { 1 }; v };"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> usize {
              _0: usize  // return
              _1: bool  // param c
              _2: usize
              _3: !
              _4: usize  // v
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _3 = call builtin panic("x") -> !
              bb2:
                _2 = 1
                goto -> bb4
              bb3:
                _2 = _3
                goto -> bb4
              bb4:
                _4 = _2
                _0 = _4
                return
            }
            fn b1() -> fn(bool) -> usize {
              _0: fn(bool) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn unresolved_name_traps_and_lowering_continues() {
    // The trap structurally continues: the `print` call after the broken
    // `let` is still in the CFG.
    check_mir(
        r#"static main = fn { let v = missing; print("after"); };"#,
        expect![[r#"
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: {error}
              _2: {error}  // v
              _3: ()
              bb0:
                _1 = trap "unresolved name `missing`" -> bb1
              bb1:
                _2 = _1
                _3 = call builtin print("after") -> bb2
              bb2:
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
fn use_of_a_duplicated_name_traps() {
    check_mir(
        r#"
static name = 1;
static name = 2;
static use_it = fn { let v = name; };
"#,
        expect![[r#"
            item name:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 1
                return
            }
            item name:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 2
                return
            }
            item use_it:
            fn b0() -> () {
              _0: ()  // return
              _1: {error}
              _2: {error}  // v
              bb0:
                _1 = trap "`name` is defined multiple times" -> bb1
              bb1:
                _2 = _1
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
fn type_mismatch_evaluates_the_value_then_traps() {
    check_mir(
        r#"static f = fn { let x: usize = "s"; print("after"); };"#,
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: usize
              _2: usize  // x
              _3: ()
              bb0:
                _1 = trap "type mismatch: expected `usize`, found `str`" -> bb1
              bb1:
                _2 = _1
                _3 = call builtin print("after") -> bb2
              bb2:
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
fn wrong_arg_count_traps_instead_of_calling() {
    check_mir(
        r#"
static f = fn (n: usize) -> usize { n }
static main = fn { f(1, 2); };
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              bb0:
                _0 = _1
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: usize
              bb0:
                _1 = trap "expected 1 argument(s), found 2" -> bb1
              bb1:
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
fn not_callable_traps_instead_of_calling() {
    check_mir(
        "static main = fn { 5(1); };",
        expect![[r#"
        item main:
        fn b0() -> () {
          _0: ()  // return
          _1: {error}
          bb0:
            _1 = trap "expression of type `usize` is not callable" -> bb1
          bb1:
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
fn capture_is_diagnosed_and_trapped() {
    check_mir(
        "static f = fn () -> usize { let a = 1; let g = fn () -> usize { a }; g() };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                _1 = trap "`a` is a local of an enclosing function; captures are not supported yet" -> bb1
              bb1:
                _0 = _1
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: usize  // a
              _2: fn() -> usize  // g
              _3: usize
              bb0:
                _1 = 1
                _2 = fn b0
                _3 = call _2() -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
            mir diagnostic at 64..65: `a` is a local of an enclosing function; captures are not supported yet
        "#]],
    );
}

#[test]
fn missing_operand_traps() {
    check_mir(
        "static x = 1 + ;",
        expect![[r#"
        item x:
        fn b0() -> usize {
          _0: usize  // return
          _1: {error}
          _2: usize
          bb0:
            _1 = trap "syntax error: missing expression" -> bb1
          bb1:
            _2 = Add(1, _1)
            _0 = _2
            return
        }
    "#]],
    );
}

#[test]
fn annotation_recovered_never_does_not_make_a_returning_call_diverge() {
    // Inference recovers `g()` as `!` (trusting the annotation); the call
    // must still get a return target — the mismatch is the trap after it.
    // `g` is `const fn` so the call passes const-check and lowers as a
    // plain call, which is the subject here.
    check_mir(
        r#"
static g = const fn () -> usize { 1 }
static f: ! = g();
"#,
        expect![[r#"
            item g:
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
            item f:
            fn b0() -> ! {
              _0: !  // return
              _1: !
              _2: !
              bb0:
                _1 = call item g() -> bb1
              bb1:
                _2 = trap "type mismatch: expected `!`, found `usize`" -> bb2
              bb2:
                _0 = _2
                return
            }
        "#]],
    );
}

#[test]
fn unannotated_items_lower_with_inferred_signatures() {
    // Interprocedural inference: `double` types as fn(usize) -> usize from
    // its body; the use lowers to a direct call, no trap.
    check_mir(
        r#"
static double = fn (n) { n * 2 }
static main = fn { double(2); };
"#,
        expect![[r#"
            item double:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              bb0:
                _2 = Mul(_1, 2)
                _0 = _2
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: usize
              bb0:
                _1 = call item double(2) -> bb1
              bb1:
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
fn use_of_underdetermined_item_traps_with_the_annotation_message() {
    // `id` is never used concretely, so even interprocedural inference
    // can't determine it; uses trap with the needs-annotation diagnostic.
    check_mir(
        r#"
static id = fn (x) { x }
static main = fn { let f = id; };
"#,
        expect![[r#"
            item id:
            fn b0(_1: _) -> _ {
              _0: _  // return
              _1: _  // param x
              bb0:
                _0 = _1
                return
            }
            fn b1() -> fn(_) -> _ {
              _0: fn(_) -> _  // return
              bb0:
                _0 = fn b0
                return
            }
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: fn({error}) -> {error}
              _2: fn({error}) -> {error}  // f
              bb0:
                _1 = trap "cannot infer the type of `id` across items; add a type annotation to its definition" -> bb1
              bb1:
                _2 = _1
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
fn hole_pattern_locals_are_unnamed_but_still_assigned() {
    // `_` binds nothing (no ` // <name>` comment), but the initializer and
    // the argument are still lowered and evaluated for their effects.
    check_mir(
        r#"static f = fn (_: usize) { let _ = 4 + 5; };"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> () {
              _0: ()  // return
              _1: usize  // param _
              _2: usize
              _3: usize
              bb0:
                _2 = Add(4, 5)
                _3 = _2
                _0 = ()
                return
            }
            fn b1() -> fn(usize) {
              _0: fn(usize)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn const_block_lowers_to_its_own_body() {
    // The `const { … }` gets a separate zero-parameter body (b0, completed
    // innermost-first) and the surrounding function references it through a
    // `const` operand — the machine forces that body at compile time
    // instead of executing the block inline.
    check_mir(
        "static f = fn () -> usize { const { 2 + 3 } };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                _1 = Add(2, 3)
                _0 = _1
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              bb0:
                _0 = const b0
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn const_check_violations_plant_traps_carrying_the_diagnostic() {
    // Two shapes, both borrowing the const-check message the editor shows.
    // At initializer level (`x`) the trap is a *conditional* `const trap`
    // barrier before the intact call: forcing an item is a const context
    // (it fires), but the runner's synthetic entry evaluates its
    // initializer as run-mode code (it falls through and the call runs).
    // Inside a `const fn` body (`apply`) the code is a const context under
    // every execution, so the call is *replaced* by an unconditional trap.
    check_mir(
        r#"
static double = fn (n: usize) -> usize { n * 2 }
static x = double(2);
static apply = const fn (f: fn() -> usize) -> usize { f() };
"#,
        expect![[r#"
            item double:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              bb0:
                _2 = Mul(_1, 2)
                _0 = _2
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item x:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                const trap "cannot call `double` in a const context; marking it `const fn` would allow this" -> bb1
              bb1:
                _1 = call item double(2) -> bb2
              bb2:
                _0 = _1
                return
            }
            item apply:
            fn b0(_1: fn() -> usize) -> usize {
              _0: usize  // return
              _1: fn() -> usize  // param f
              _2: usize
              bb0:
                _2 = trap "cannot call a value in a const context; whether it is a `const fn` is not known from its type" -> bb1
              bb1:
                _0 = _2
                return
            }
            fn b1() -> fn(fn() -> usize) -> usize {
              _0: fn(fn() -> usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

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
              bb0:
                _1 = 1
                _1 = 2
                _0 = _1
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
        "static f = fn () -> usize { let mut a = 1; let g = fn () -> usize { a = 2; 0 }; g() };",
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
              _3: usize
              bb0:
                _1 = 1
                _2 = fn b0
                _3 = call _2() -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b2() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
            mir diagnostic at 68..69: `a` is a local of an enclosing function; captures are not supported yet
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
              bb0:
                _1 = 1
                _2 = trap "cannot assign to `x`: it is not declared `mut`" -> bb1
              bb1:
                _0 = _1
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
static r = fn { struct { y: g(), x: f() } };
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
        r#"static f = fn { let p = struct { x: 1, y: 2 }; p.y };"#,
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
        r#"static f = fn { let p = struct { x: 1 }; p.y };"#,
        expect![[r#"
            item f:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: struct { x: usize }
              _2: struct { x: usize }  // p
              _3: {error}
              bb0:
                _1 = { x: 1 }
                _2 = _1
                _3 = trap "no field `y` on `struct { x: usize }`" -> bb1
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
    // in `p.a`), rendered `_1.0.0`. The RHS is evaluated before the write,
    // like every assignment.
    check_mir(
        "static f = fn () -> usize { let mut p = struct { a: struct { b: 1 } }; p.a.b = 2; p.a.b };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: struct { b: usize }
              _2: struct { a: struct { b: usize } }
              _3: struct { a: struct { b: usize } }  // p
              _4: struct { b: usize }
              _5: usize
              bb0:
                _1 = { b: 1 }
                _2 = { a: _1 }
                _3 = _2
                _3.0.0 = 2
                _4 = _3.0
                _5 = _4.0
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
fn field_assign_on_an_immutable_root_traps() {
    // Inference rejected the root; the write is replaced by a trap with
    // the squiggle's exact text (squiggle-equals-crash).
    check_mir(
        "static f = fn () -> usize { let p = struct { x: 1 }; p.x = 2; p.x };",
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
              bb0:
                _1 = 1
                _2 = trap "can only assign to a variable or its fields" -> bb1
              bb1:
                _0 = _1
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
    // `Foo(struct { x: 1 })` lowers to the record aggregate alone — no
    // call, no tag: nominal types exist only in the static type system.
    // The `type` item itself lowers to nothing (no root body).
    check_mir(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x: 1 });
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

#[test]
fn variant_construction_is_tag_free() {
    // Constructing and passing a variant-typed value produces only the
    // payload aggregate — no tag, no `widen` op anywhere on this path.
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static c = Shape::Circle(3);
static p = Shape::Point;
"#,
        expect![[r#"
            item Shape:
            item c:
            fn b0() -> Shape::Circle {
              _0: Shape::Circle  // return
              _1: Shape::Circle
              bb0:
                _1 = payload(3)
                _0 = _1
                return
            }
            item p:
            fn b0() -> Shape::Point {
              _0: Shape::Point  // return
              _1: Shape::Point
              bb0:
                _1 = payload()
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn widening_happens_exactly_at_the_conversion_edge() {
    // The enum annotation is the edge: `widen` appears once, on the
    // initializer; the same-variant `keep` path has none.
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static widened: Shape = Shape::Point;
static keep: Shape::Point = Shape::Point;
"#,
        expect![[r#"
            item Shape:
            item widened:
            fn b0() -> Shape {
              _0: Shape  // return
              _1: Shape
              _2: Shape
              bb0:
                _1 = payload()
                _2 = widen _1 to Shape::Point
                _0 = _2
                return
            }
            item keep:
            fn b0() -> Shape::Point {
              _0: Shape::Point  // return
              _1: Shape::Point
              bb0:
                _1 = payload()
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn mixed_variant_join_widens_each_edge_only() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool) -> Shape {
    if a { Shape::Circle(1) } else { Shape::Point }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: bool) -> Shape {
              _0: Shape  // return
              _1: bool  // param a
              _2: Shape
              _3: Shape::Circle
              _4: Shape
              _5: Shape::Point
              _6: Shape
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _3 = payload(1)
                _4 = widen _3 to Shape::Circle
                _2 = _4
                goto -> bb3
              bb2:
                _5 = payload()
                _6 = widen _5 to Shape::Point
                _2 = _6
                goto -> bb3
              bb3:
                _0 = _2
                return
            }
            fn b1() -> fn(bool) -> Shape {
              _0: fn(bool) -> Shape  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn same_variant_join_has_no_widening() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool) {
    if a { Shape::Point } else { Shape::Point }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: bool) -> Shape::Point {
              _0: Shape::Point  // return
              _1: bool  // param a
              _2: Shape::Point
              _3: Shape::Point
              _4: Shape::Point
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _3 = payload()
                _2 = _3
                goto -> bb3
              bb2:
                _4 = payload()
                _2 = _4
                goto -> bb3
              bb3:
                _0 = _2
                return
            }
            fn b1() -> fn(bool) -> Shape::Point {
              _0: fn(bool) -> Shape::Point  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn first_class_constructor_synthesizes_a_body() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let make = Shape::Circle;
    make(3)
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: usize) -> Shape::Circle {
              _0: Shape::Circle  // return
              _1: usize  // param _
              bb0:
                _0 = payload(_1)
                return
            }
            fn b1() -> Shape::Circle {
              _0: Shape::Circle  // return
              _1: fn(usize) -> Shape::Circle  // make
              _2: Shape::Circle
              bb0:
                _1 = fn b0
                _2 = call _1(3) -> bb1
              bb1:
                _0 = _2
                return
            }
            fn b2() -> fn() -> Shape::Circle {
              _0: fn() -> Shape::Circle  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn unknown_variant_traps_with_the_diagnostic() {
    check_mir(
        r#"
type Shape = enum { Point };
static s = Shape::Missing;
"#,
        expect![[r#"
            item Shape:
            item s:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "`Shape` has no variant `Missing`" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn match_on_enum_lowers_to_switch_variant() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: Shape) -> usize {
              _0: usize  // return
              _1: Shape  // param s
              _2: Shape
              _3: usize
              _4: usize  // r
              bb0:
                _2 = _1
                switch _2 on Shape -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = _2.0
                _3 = _4
                goto -> bb4
              bb2:
                _3 = 0
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _3
                return
            }
            fn b1() -> fn(Shape) -> usize {
              _0: fn(Shape) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn nonexhaustive_match_traps_in_the_otherwise_arm() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: Shape) -> usize {
              _0: usize  // return
              _1: Shape  // param s
              _2: Shape
              _3: usize
              _4: usize  // r
              bb0:
                _2 = _1
                switch _2 on Shape -> [0: bb1, otherwise: bb2]
              bb1:
                _4 = _2.0
                _3 = _4
                goto -> bb3
              bb2:
                _3 = trap "this `match` does not cover `Shape::Point`" -> bb3
              bb3:
                _0 = _3
                return
            }
            fn b1() -> fn(Shape) -> usize {
              _0: fn(Shape) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn variant_typed_scrutinee_match_compiles_to_no_switch() {
    // The state-machine payoff: the scrutinee can only be `State::Running`,
    // so the match is a straight-line payload destructure — no switch, no
    // tag read; the other arms are dead blocks nothing jumps to.
    check_mir(
        r#"
type State = enum { Idle, Running(usize) };
static step = fn (s: State::Running) -> usize {
    match s {
        ::Running(n) => n + 1,
        ::Idle => 0,
    }
};
"#,
        expect![[r#"
            item State:
            item step:
            fn b0(_1: State::Running) -> usize {
              _0: usize  // return
              _1: State::Running  // param s
              _2: State::Running
              _3: usize
              _4: usize  // n
              _5: usize
              bb0:
                _2 = _1
                _4 = _2.0
                _5 = Add(_4, 1)
                _3 = _5
                goto -> bb1
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(State::Running) -> usize {
              _0: fn(State::Running) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn widen_then_match_round_trip() {
    // Construct a variant (tag-free), widen it into the enum (`let mut`
    // injects the tag), then match: the tag written by `WidenToEnum` is
    // exactly what `SwitchVariant` dispatches on.
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn () -> usize {
    let mut s = Shape::Circle(3);
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: Shape::Circle
              _2: Shape
              _3: Shape  // s
              _4: Shape
              _5: usize
              _6: usize  // r
              bb0:
                _1 = payload(3)
                _2 = widen _1 to Shape::Circle
                _3 = _2
                _4 = _3
                switch _4 on Shape -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _6 = _4.0
                _5 = _6
                goto -> bb4
              bb2:
                _5 = 0
                goto -> bb4
              bb3:
                unreachable
              bb4:
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
fn accumulator_loop_lowers_with_a_back_edge() {
    // The first cyclic CFG: bb-header re-entered by the back edge at the
    // body's end, break edges to the exit carrying the loop's value in a
    // dedicated result local.
    check_mir(
        r#"
static sum = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
"#,
        expect![[r#"
            item sum:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // acc
              _2: usize  // i
              _3: usize
              _4: bool
              _5: ()
              _6: usize
              _7: usize
              bb0:
                _1 = 0
                _2 = 0
                goto -> bb1
              bb1:
                _4 = Eq(_2, 10)
                if _4 -> [then: bb3, else: bb4]
              bb2:
                _0 = _3
                return
              bb3:
                _3 = _1
                goto -> bb2
              bb4:
                _5 = ()
                goto -> bb6
              bb5:
                _5 = ()
                goto -> bb6
              bb6:
                _6 = Add(_1, _2)
                _1 = _6
                _7 = Add(_2, 1)
                _2 = _7
                goto -> bb1
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
fn break_with_value_stores_into_the_loop_result() {
    check_mir(
        "static f = fn () -> usize { loop { break 5; } };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                goto -> bb1
              bb1:
                _1 = 5
                goto -> bb2
              bb2:
                _0 = _1
                return
              bb3:
                goto -> bb1
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
fn breakless_loop_has_an_unreachable_exit() {
    // No break ever targets the exit block: it stays predecessor-less
    // (like the continuation after a diverging call), and the code after
    // the loop lowers into it so the CFG stays total.
    check_mir(
        "static f = fn { loop { }; 1 };",
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: !
              bb0:
                goto -> bb1
              bb1:
                goto -> bb1
              bb2:
                _0 = 1
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
fn continue_jumps_to_the_header() {
    check_mir(
        r#"
static f = fn (skip: bool) -> usize {
    loop {
        if skip { continue; };
        break 1;
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> usize {
              _0: usize  // return
              _1: bool  // param skip
              _2: usize
              _3: ()
              bb0:
                goto -> bb1
              bb1:
                if _1 -> [then: bb3, else: bb4]
              bb2:
                _0 = _2
                return
              bb3:
                goto -> bb1
              bb4:
                _3 = ()
                goto -> bb6
              bb5:
                _3 = ()
                goto -> bb6
              bb6:
                _2 = 1
                goto -> bb2
              bb7:
                goto -> bb1
            }
            fn b1() -> fn(bool) -> usize {
              _0: fn(bool) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn break_outside_loop_traps_with_the_diagnostic() {
    check_mir(
        "static f = fn { break 1; };",
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: {error}
              bb0:
                _1 = trap "`break` outside of a loop: there is no enclosing `loop` to exit" -> bb1
              bb1:
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

// ---- generics: real instances, no staging traps ----
//
// These replace the earlier `generic_mention_lowers_to_a_staging_trap`:
// the same fixture now lowers to a real instantiation — the staging
// refusal (and `TerminatorKind::StagingTrap` itself) is gone.

#[test]
fn generic_call_site_constructs_the_instance_and_calls_it() {
    // The generic body reads its const param as a real `Const::ConstParam`
    // operand (dense const-only index); the call site in `v` lowers the
    // const argument `3` to a compile-time body of its own (the same
    // machinery as a `const` block) and constructs the instance's fn value
    // with `instantiate`, then calls it like any ordinary fn value — no
    // traps anywhere.
    check_mir(
        "static rep = const fn::<const N: usize>() -> usize { N };\nstatic v: usize = rep::<3>();",
        expect![[r#"
            item rep:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = const param 0
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item v:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 3
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: fn() -> usize
              _2: usize
              bb0:
                _1 = instantiate rep(const b0)
                _2 = call _1() -> bb1
              bb1:
                _0 = _2
                return
            }
        "#]],
    );
}

#[test]
fn type_only_generic_mention_lowers_to_the_plain_item_value() {
    // Type args need nothing at runtime (TR06): with no const params there
    // is no `instantiate` — the mention lowers to the item's own value,
    // exactly like a non-generic mention.
    check_mir(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(4) };",
        expect![[r#"
            item id:
            fn b0(_1: T) -> T {
              _0: T  // return
              _1: T  // param x
              bb0:
                _0 = _1
                return
            }
            fn b1() -> fn(T) -> T {
              _0: fn(T) -> T  // return
              bb0:
                _0 = fn b0
                return
            }
            item g:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                _1 = call item id(4) -> bb1
              bb1:
                _0 = _1
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
fn forwarded_const_param_lowers_to_a_const_param_body() {
    // The composition case: inside `rep2`, the const argument `const N` is
    // itself a compile-time body reading the enclosing binder's param —
    // forced per instance when the mention executes, the value passing
    // straight through to `rep`'s instance.
    check_mir(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };",
        expect![[r#"
            item rep:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param x
              _2: usize
              bb0:
                _2 = Mul(_1, const param 0)
                _0 = _2
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item rep2:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = const param 0
                return
            }
            fn b1(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param x
              _2: fn(usize) -> usize
              _3: usize
              bb0:
                _2 = instantiate rep(const b0)
                _3 = call _2(_1) -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b2() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn generic_enum_match_switches_on_the_declaration() {
    check_mir(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn (o: Option::<usize>) -> usize {\n\
             match o { ::Some(x) => x, ::None => 0, }\n\
         };",
        expect![[r#"
            item Option:
            item main:
            fn b0(_1: Option::<usize>) -> usize {
              _0: usize  // return
              _1: Option::<usize>  // param o
              _2: Option::<usize>
              _3: usize
              _4: usize  // x
              bb0:
                _2 = _1
                switch _2 on Option -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = _2.0
                _3 = _4
                goto -> bb4
              bb2:
                _3 = 0
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _3
                return
            }
            fn b1() -> fn(Option::<usize>) -> usize {
              _0: fn(Option::<usize>) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn generic_widening_injects_the_tag() {
    check_mir(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::<usize>::None };",
        expect![[r#"
            item Option:
            item main:
            fn b0() -> Option::<usize> {
              _0: Option::<usize>  // return
              _1: Option::<usize>
              _2: Option::<usize>
              bb0:
                _1 = payload()
                _2 = widen _1 to Option::None
                _0 = _2
                return
            }
            fn b1() -> fn() -> Option::<usize> {
              _0: fn() -> Option::<usize>  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn addr_of_deref_and_deref_store_lower_to_place_ops() {
    check_mir(
        r#"
static s = 7;
static main = fn() -> usize {
    let mut x = 1;
    let p = &raw mut x;
    let q = &raw s;
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
              _2: &raw mut usize
              _3: &raw mut usize  // p
              _4: &raw usize
              _5: &raw usize  // q
              _6: usize
              _7: usize
              _8: usize
              bb0:
                _1 = 1
                _2 = &raw mut _1
                _3 = _2
                _4 = &raw static s
                _5 = _4
                _3.* = 2
                _6 = _3.*
                _7 = _5.*
                _8 = Add(_6, _7)
                _0 = _8
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
    let mut x = 1;
    let p = &raw mut x;
    p.*
};
"#,
        expect![[r#"
            item main:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize  // x
              _2: &raw mut usize
              _3: &raw mut usize  // p
              _4: usize
              bb0:
                _1 = 1
                _2 = &raw mut _1
                _3 = _2
                _4 = trap "dereferencing a raw pointer requires an `unsafe { ... }` block" -> bb1
              bb1:
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
fn through_pointer_writes_lower_to_deref_projected_places() {
    check_mir(
        r#"
static main = fn() {
    let mut r = struct { x: 1, buf: [1, 2] };
    let p = &raw mut r;
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
              _4: &raw mut struct { buf: [usize; 2], x: usize }
              _5: &raw mut struct { buf: [usize; 2], x: usize }  // p
              _6: usize  // i
              bb0:
                _1 = [1, 2]
                _2 = { buf: _1, x: 1 }
                _3 = _2
                _4 = &raw mut _3
                _5 = _4
                _6 = 1
                _5.*.1 = 2
                _5.*.0[_6] = 3
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
    let mut x = 1;
    let mut p = &raw mut x;
    let pp = &raw mut p;
    unsafe { pp.*.* = 7; }
};
"#,
        expect![[r#"
            item main:
            fn b0() -> () {
              _0: ()  // return
              _1: usize  // x
              _2: &raw mut usize
              _3: &raw mut usize  // p
              _4: &raw mut &raw mut usize
              _5: &raw mut &raw mut usize  // pp
              _6: &raw mut usize
              bb0:
                _1 = 1
                _2 = &raw mut _1
                _3 = _2
                _4 = &raw mut _3
                _5 = _4
                _6 = _5.*
                _6.* = 7
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
    let mut a = [1, 2];
    let e = &raw mut a[0];
    let mut r = struct { x: 1 };
    let p = &raw mut r;
    unsafe {
        let q = &raw mut p.*.x;
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
              _3: &raw mut usize
              _4: &raw mut usize  // e
              _5: struct { x: usize }
              _6: struct { x: usize }  // r
              _7: &raw mut struct { x: usize }
              _8: &raw mut struct { x: usize }  // p
              _9: &raw mut usize
              _10: &raw mut usize  // q
              _11: usize
              bb0:
                _1 = [1, 2]
                _2 = _1
                _3 = &raw mut _2[0]
                _4 = _3
                _5 = { x: 1 }
                _6 = _5
                _7 = &raw mut _6
                _8 = _7
                _9 = &raw mut _8.*.0
                _10 = _9
                _11 = _10.*
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

// ---- fixed-size arrays ----

#[test]
fn array_literal_index_and_element_assign_lower() {
    check_mir(
        r#"
static f = fn {
    let mut a = [1, 2, 3];
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
              bb0:
                _1 = [1, 2, 3]
                _2 = _1
                _3 = _2[1]
                _4 = _3
                _2[2] = _4
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
    let mut m = [[0; 2]; 2];
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
        "static f = fn { let a = [1, 2]; let x = a[2]; };",
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
static f = fn (p: &raw mut usize) -> () {
    unsafe { dealloc_array(p, 1) };
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: &raw mut usize) -> () {
              _0: ()  // return
              _1: &raw mut usize  // param p
              _2: ()
              bb0:
                _2 = call builtin dealloc_array(_1, 1) -> bb1
              bb1:
                _0 = ()
                return
            }
            fn b1() -> fn(&raw mut usize) {
              _0: fn(&raw mut usize)  // return
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
static f = fn (p: &raw mut usize) -> () {
    dealloc_array(p, 1);
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: &raw mut usize) -> () {
              _0: ()  // return
              _1: &raw mut usize  // param p
              _2: ()
              bb0:
                _2 = trap "calling `dealloc_array` requires an `unsafe { ... }` block" -> bb1
              bb1:
                _0 = ()
                return
            }
            fn b1() -> fn(&raw mut usize) {
              _0: fn(&raw mut usize)  // return
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
