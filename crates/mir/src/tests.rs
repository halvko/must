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
fn assignment_to_a_non_variable_traps_with_the_validation_message() {
    // Validation squiggled the LHS with "can only assign to a variable";
    // the trap borrows that exact text via the shared constant.
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
                _2 = trap "can only assign to a variable" -> bb1
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
