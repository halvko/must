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
        "static example: usize = 4 + 5;",
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
static name: usize = 1;
static name: usize = 2;
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
                _1 = trap "expression of type `{number}` is not callable" -> bb1
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
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              _2: {error}
              bb0:
                _1 = trap "syntax error: missing expression" -> bb1
              bb1:
                _2 = Add((), _1)
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
static double = fn (n: usize) { n * 2 }
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
        r#"static f = fn (_: usize) { let _: usize = 4 + 5; };"#,
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
fn member_generic_args_the_binder_cannot_receive_trap_at_the_path() {
    // Without these arms in `value_traps`, lowering falls through to the
    // unrelated (and doubly wrong) "cannot use a variant" trap: `size`
    // names a member, not a variant, and `Measured`'s declaration has no
    // errors.
    check_mir(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
static s = Measured::size::<usize>;
"#,
        expect![[r#"
            item Measured:
            item s:
            fn b0() -> fn(Measured) -> usize {
              _0: fn(Measured) -> usize  // return
              _1: fn(Measured) -> usize
              bb0:
                _1 = trap "`Measured::size` takes no generic arguments" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
    // And the kind still reserved: a member whose own binder declares a
    // CONST parameter refuses the whole list. The report is keyed on the
    // CALL (that is the range the squiggle covers), so the trap REPLACES
    // the call rather than following it — otherwise the impl body runs and
    // reports the reserved `N` instead of the refusal the reader was
    // shown.
    check_mir(
        r#"
trait Counted = requires { step: fn::<const N: usize>(s: Self) -> usize; } with {
    impl usize { step = fn::<const N: usize>(s: usize) -> usize { N }; }
};
static s = fn(n: usize) -> usize { Counted::step::<3>(n) };
"#,
        expect![[r#"
            item Counted:
            item s:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              bb0:
                _2 = trap "`Counted::step` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)" -> bb1
              bb1:
                _0 = _2
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn variant_own_generic_args_traps_with_the_correction() {
    check_mir(
        r#"
type Shape = enum::<T> { Circle(T), Point };
static s = Shape::Circle::<usize>;
"#,
        expect![[r#"
            item Shape:
            item s:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`" -> bb1
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
        "static f = fn -> usize { loop { }; 1 };",
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
        "static f = fn { break \"x\"; };",
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

// ---- `return` ---------------------------------------------------------

#[test]
fn return_assigns_the_return_place_and_terminates() {
    // The existing function-exit path, reached early: the same
    // `_0 = <value>; return` pair the body's tail value lowers to. The
    // statements after it lower into a predecessor-less block, so the CFG
    // stays total and no execution can reach them.
    check_mir(
        r#"
static f = fn (c: bool) -> usize {
    if c { return 1; };
    2
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: bool) -> usize {
              _0: usize  // return
              _1: bool  // param c
              _2: ()
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _0 = 1
                return
              bb2:
                _2 = ()
                goto -> bb4
              bb3:
                _2 = ()
                goto -> bb4
              bb4:
                _0 = 2
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
fn bare_return_returns_unit() {
    check_mir(
        "static f = fn (c: bool) -> () { if c { return; }; };",
        expect![[r#"
            item f:
            fn b0(_1: bool) -> () {
              _0: ()  // return
              _1: bool  // param c
              _2: ()
              bb0:
                if _1 -> [then: bb1, else: bb2]
              bb1:
                _0 = ()
                return
              bb2:
                _2 = ()
                goto -> bb4
              bb3:
                _2 = ()
                goto -> bb4
              bb4:
                _0 = ()
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
fn return_inside_a_loop_leaves_the_function_not_the_loop() {
    // A `break` jumps to the loop's exit block; a `return` in the same
    // position terminates outright — the loop's exit is not involved.
    check_mir(
        r#"
static f = fn (n: usize) -> usize {
    loop {
        if n == 0 { return 7; };
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              _3: bool
              _4: ()
              bb0:
                goto -> bb1
              bb1:
                _3 = Eq(_1, 0)
                if _3 -> [then: bb3, else: bb4]
              bb2:
                _0 = _2
                return
              bb3:
                _0 = 7
                return
              bb4:
                _4 = ()
                goto -> bb6
              bb5:
                _4 = ()
                goto -> bb6
              bb6:
                goto -> bb1
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_terminates_that_body() {
    // Structural: `b` is one MIR body's builder, and a `fn` literal builds
    // its own, so the inner `return` can only terminate the inner body.
    check_mir(
        r#"
static f = fn () -> usize {
    let inner = fn () -> usize { return 1; };
    inner()
};
"#,
        expect![[r#"
            item f:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 1
                return
              bb1:
                _0 = ()
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: fn() -> usize  // inner
              _2: usize
              bb0:
                _1 = fn b0
                _2 = call _1() -> bb1
              bb1:
                _0 = _2
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
fn return_outside_a_function_traps_with_the_diagnostic() {
    // The trap has to sit on the path that REACHES the `return`: emitting
    // the exit edge first buries it in the unreachable block after the
    // `return`, and the execution leaves normally instead of trapping.
    check_mir(
        "static x = return 1;",
        expect![[r#"
            item x:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "`return` outside of a function: there is no enclosing `fn` body to return from" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn return_in_a_const_block_traps_instead_of_leaving_the_block() {
    // The reserve's MIR half: the `const` block's own body still lowers
    // whole, but the refused `return` plants a trap where it used to plant
    // the block's exit edge — so nothing produces the block's value early
    // any more.
    check_mir(
        "static x = fn () -> usize { const { if true { return 42; }; 0 } };",
        expect![[r#"
            item x:
            fn b0() -> usize {
              _0: usize  // return
              _1: ()
              _2: !
              bb0:
                if true -> [then: bb1, else: bb2]
              bb1:
                _2 = trap "`return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own" -> bb3
              bb2:
                _1 = ()
                goto -> bb4
              bb3:
                _1 = ()
                goto -> bb4
              bb4:
                _0 = 0
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
    //
    // The `move` on the parameter read is the other half of T22 landing
    // here: an UNBOUNDED `T` has no `forget`, so a read of one is a move
    // like any other non-copyable value's. Nothing about lowering changed
    // — `has_forget` simply answers differently now.
    check_mir(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(4) };",
        expect![[r#"
            item id:
            fn b0(_1: T) -> T {
              _0: T  // return
              _1: T  // param x
              bb0:
                _0 = move _1
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
              _8: usize  // a
              _9: usize
              _10: usize  // b
              _11: usize
              _12: usize  // c
              bb0:
                _1 = [1, 2]
                _2 = { buf: _1, x: 1 }
                _3 = _2
                _4 = &raw mut _3
                _5 = _4
                _6 = 1
                _7 = _3.0[_6]
                _8 = _7
                _9 = _5.*.1
                _10 = _9
                _11 = _5.*.0[_6]
                _12 = _11
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
              _6: usize.&raw mut
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
              _9: usize.&raw mut
              _10: usize.&raw mut  // q
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
              _2: ()
              bb0:
                _2 = call builtin dealloc_array(_1, 1) -> bb1
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
              _2: ()
              bb0:
                _2 = trap "calling `dealloc_array` requires an `unsafe { ... }` block" -> bb1
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
              _2: bool
              _3: i8
              _4: i8
              bb0:
                _2 = Eq(_1, -128)
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _4 = Neg(_1)
                _3 = _4
                goto -> bb3
              bb2:
                _3 = _1
                goto -> bb3
              bb3:
                _0 = _3
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

// ---- regions stop at the MIR boundary -----------------------------------

#[test]
fn mir_is_region_erased() {
    // THE erasure proof. Two functions whose signatures differ ONLY in
    // their regions lower to MIR that is textually identical apart from
    // their names — no `@a`, no `@b`, nothing to tell them apart. That is
    // the specialization law made mechanical: nothing below this boundary
    // can branch on a region, because there is no region to branch on.
    check_mir(
        "static one = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static two = fn::<@x, @y>(r: usize.&::<@x + @y>) -> usize { r.* };",
        expect![[r#"
            item one:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param r
              _2: usize
              bb0:
                _2 = _1.*
                _0 = _2
                return
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item two:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param r
              _2: usize
              bb0:
                _2 = _1.*
                _0 = _2
                return
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_safe_borrow_is_its_own_rvalue() {
    // `.&`/`.&mut` and `.&raw`/`.&raw mut` compute the same address and
    // lower through the same place walk, but they are DIFFERENT rvalues —
    // so every exhaustive consumer has to decide what a borrow means for
    // it instead of inheriting the raw answer. The wasm backend's refusal
    // hangs off exactly this.
    check_mir(
        "static f = fn () -> () {\n\
             let mut n: usize = 1;\n\
             let borrowed = n.&mut;\n\
             let r = n.&raw mut;\n\
         };",
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: usize  // n
              _2: usize.&mut
              _3: usize.&mut  // borrowed
              _4: usize.&raw mut
              _5: usize.&raw mut  // r
              bb0:
                _1 = 1
                _2 = &mut _1
                _3 = _2
                _4 = &raw mut _1
                _5 = _4
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
fn a_reborrow_through_a_borrow_stays_a_borrow() {
    // `m.*.&mut` mints a NEW node (it is `Rvalue::Borrow`), while
    // `m.*.&raw mut` inherits `m`'s (it is `Rvalue::AddrOf`). That split
    // is the ruling that `.&raw` is deliberately not a decayed safe
    // borrow, visible in the IR.
    check_mir(
        "static f = fn::<@a>(m: usize.&mut::<@a>) -> () {\n\
             let child = m.*.&mut;\n\
             let r = m.*.&raw mut;\n\
         };",
        expect![[r#"
            item f:
            fn b0(_1: usize.&mut) -> () {
              _0: ()  // return
              _1: usize.&mut  // param m
              _2: usize.&mut
              _3: usize.&mut  // child
              _4: usize.&raw mut
              _5: usize.&raw mut  // r
              bb0:
                _2 = &mut _1.*
                _3 = _2
                _4 = &raw mut _1.*
                _5 = _4
                _0 = ()
                return
            }
            fn b1() -> fn(usize.&mut) {
              _0: fn(usize.&mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- match projects through borrows -------------------------------------

#[test]
fn a_borrowed_match_reads_the_tag_through_the_deref_and_borrows_its_payloads() {
    // The whole runtime content of the ruling, in one snapshot.
    //
    //   * `switch _2.*` — the tag test is a READ THROUGH THE BORROW, not a
    //     read of a detached copy, so the interpreter's aliasing tree sees
    //     it and an invalidated scrutinee is caught before any arm runs;
    //   * `_4 = &mut _2.*.0` — the payload binding is a BORROW of the
    //     payload's sub-place, `Rvalue::Borrow` over `[Deref, Field(0)]`.
    //     Compare the owned lowering (`match_on_enum_lowers_to_switch_variant`),
    //     which is `_4 = _2.0`: a value read out of a copy.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&mut::<@a>) -> () {
    match s {
        ::Some(t) => { t.* = 1; },
        ::None => {},
    }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&mut) -> () {
              _0: ()  // return
              _1: Opt.&mut  // param s
              _2: Opt.&mut
              _3: ()
              _4: usize.&mut  // t
              bb0:
                _2 = _1
                switch _2.* on Opt -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = &mut _2.*.0
                _4.* = 1
                _3 = ()
                goto -> bb4
              bb2:
                _3 = ()
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt.&mut) {
              _0: fn(Opt.&mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_shared_borrowed_match_borrows_its_payloads_shared() {
    // The flavor rides through: `&` where the scrutinee was `&`.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&::<@a>) -> usize {
    match s {
        ::Some(t) => t.*,
        ::None => 0,
    }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&) -> usize {
              _0: usize  // return
              _1: Opt.&  // param s
              _2: Opt.&
              _3: usize
              _4: usize.&  // t
              _5: usize
              bb0:
                _2 = _1
                switch _2.* on Opt -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = & _2.*.0
                _5 = _4.*
                _3 = _5
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
            fn b1() -> fn(Opt.&) -> usize {
              _0: fn(Opt.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_variant_typed_borrowed_match_borrows_without_a_switch() {
    // No dispatch (the value can only be this one variant), so no tag read
    // — but the payload binding is still a borrow of the sub-place.
    check_mir(
        r#"
type State = enum { Run(usize), Stop };
static f = fn::<@a>(s: State::Run.&mut::<@a>) -> () {
    match s {
        ::Run(n) => { n.* = 1; },
    }
};
"#,
        expect![[r#"
            item State:
            item f:
            fn b0(_1: State::Run.&mut) -> () {
              _0: ()  // return
              _1: State::Run.&mut  // param s
              _2: State::Run.&mut
              _3: ()
              _4: usize.&mut  // n
              bb0:
                _2 = _1
                _4 = &mut _2.*.0
                _4.* = 1
                _3 = ()
                goto -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b1() -> fn(State::Run.&mut) {
              _0: fn(State::Run.&mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_whole_value_binder_through_a_borrow_copies_the_pointer() {
    // A bare bind names the same place the scrutinee does, so it aliases
    // the value exactly as it always did — which through a borrow means
    // copying the pointer, not minting a node. It also decides no tag, so
    // there is no switch and nothing is read through the borrow.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&::<@a>) -> Opt.&::<@a> {
    match s { whole => whole }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&) -> Opt.& {
              _0: Opt.&  // return
              _1: Opt.&  // param s
              _2: Opt.&
              _3: Opt.&
              _4: Opt.&  // whole
              bb0:
                _2 = _1
                goto -> bb1
              bb1:
                _4 = _2
                _3 = _4
                goto -> bb2
              bb2:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt.&) -> Opt.& {
              _0: fn(Opt.&) -> Opt.&  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_owned_match_that_dispatches_on_nothing_has_no_switch() {
    // The dead-switch elision is not a borrowed-scrutinee special case: an
    // owned match whose only arm is a catch-all decides nothing on the tag
    // either, so it lowers to a plain `goto`. Here that drops only a read
    // of the pinned temp, which nothing can borrow into.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    match o { _ => 7 }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt) -> usize {
              _0: usize  // return
              _1: Opt  // param o
              _2: Opt
              _3: usize
              bb0:
                _2 = _1
                goto -> bb1
              bb1:
                _3 = 7
                goto -> bb2
              bb2:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt) -> usize {
              _0: fn(Opt) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- literal patterns: dispatch by equality chain ----------------------

#[test]
fn a_character_match_lowers_to_a_chain_of_equality_tests() {
    // No table, no switch: one `Eq` against the literal per arm, each
    // `SwitchBool`ing into its own arm block and falling through to the
    // next test. That is the whole reason `char` needed no new terminator
    // and no new operation for eval or the backend to learn.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              _5: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, '(')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = Eq(_2, ')')
                if _5 -> [then: bb4, else: bb5]
              bb4:
                _3 = 2
                goto -> bb1
              bb5:
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_integer_match_lowers_to_the_same_equality_chain() {
    // Integer patterns reuse the character lowering WHOLE — no switch, no
    // jump table, no new operation. The one difference is where the
    // constant's width comes from: inference resolved the pattern against
    // the scrutinee, so the `Eq` compares a typed `u8` against a typed
    // `u8` even though nothing in the source wrote a width.
    check_mir(
        r#"
static f = fn (b: u8) -> usize {
    match b {
        0 => 1,
        7 => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: u8) -> usize {
              _0: usize  // return
              _1: u8  // param b
              _2: u8
              _3: usize
              _4: bool
              _5: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = Eq(_2, 7)
                if _5 -> [then: bb4, else: bb5]
              bb4:
                _3 = 2
                goto -> bb1
              bb5:
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(u8) -> usize {
              _0: fn(u8) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_character_match_without_a_catch_all_traps_at_the_fall_through() {
    // The chain runs out, and the last `else` edge is the deferred error —
    // carrying the exact message the editor already showed, like every
    // other non-exhaustive lowering.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = trap "this `match` does not cover every possible `char`; add a `_` arm" -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_integer_match_without_a_catch_all_traps_the_same_way() {
    // The `_` arm is required at EVERY width — a policy, not an
    // arithmetic — so a `u8` match that lists a value still falls through
    // to the deferred trap, carrying the message the editor showed. The
    // character twin's shape, byte for byte apart from the type.
    check_mir(
        r#"
static f = fn (b: u8) -> usize {
    match b {
        0 => 1,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: u8) -> usize {
              _0: usize  // return
              _1: u8  // param b
              _2: u8
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = trap "this `match` does not cover every possible `u8`; add a `_` arm" -> bb1
            }
            fn b1() -> fn(u8) -> usize {
              _0: fn(u8) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn character_arms_after_a_catch_all_lower_as_orphan_blocks() {
    // Everything after a catch-all is dead, but it still LOWERS — as a
    // block no edge targets — so the CFG stays total, exactly as in the
    // switch and straight-line lowerings. A capture error inside one is
    // still reported: MIR lowering is the pass that finds captures.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
        _ => 2,
        'b' => 3,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = 2
                goto -> bb1
              bb4:
                _3 = 3
                goto -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_borrowed_character_match_tests_through_the_deref() {
    // The projection rule reaching literal patterns (M13: the scrutinee's
    // flavour decides, all the way down). The equality test names the
    // POINTEE place, not a detached copy — so the interpreter's aliasing
    // tree sees the access and an invalidated scrutinee is caught at the
    // `match`, before any arm runs. The catch-all's binder still takes the
    // borrow itself: it names the very same place.
    check_mir(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        'a' => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char.&) -> usize {
              _0: usize  // return
              _1: char.&  // param c
              _2: char.&
              _3: usize
              _4: bool
              _5: char.&  // other
              bb0:
                _2 = _1
                _4 = Eq(_2.*, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = _2
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(char.&) -> usize {
              _0: fn(char.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- loan liveness: the static exclusivity fence -------------------------

/// Renders every loan-check finding in the file — range, message, then
/// each companion as a note with its range — after any hir diagnostic, so
/// a program that fails to type-check cannot pass as "accepted".
fn check_loans(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for diag in hir::file_diagnostics(&db, file) {
        rendered.push_str(&format!("{:?}: {}\n", diag.range, diag.message));
    }
    for &item in hir::file_item_ids(&db, file) {
        let (_, source_map) = hir::body_with_source_map(&db, item);
        for diag in crate::loan_check(&db, item) {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            rendered.push_str(&format!("{:?}: {}\n", ptr.text_range(), diag.message()));
            for (expr, note) in diag.related() {
                if let Some(ptr) = source_map.node_for_expr(expr) {
                    rendered.push_str(&format!("  note at {:?}: {note}\n", ptr.text_range()));
                }
            }
        }
    }
    expect.assert_eq(&rendered);
}

/// `stdin_lib.must`'s `Reader` in miniature, the fixture the family is
/// written against. Three things about it are load-bearing:
///
/// * `next_line` takes `Self.&mut` at its OWN region and hands back a
///   borrow tied to that region, so the view a caller holds IS a loan of
///   the reader, and every later `&mut` use of the reader kills it;
/// * its body returns a borrow from one branch with a `&mut` reborrow of
///   the same receiver beside it — `return ::Some(r.*.line.&)` next to
///   `r.refill()`. That stays legal because loans are in scope forward
///   from the mint, and the mint's block returns;
/// * it is `only move` and owns a buffer, so every test has to `deinit`
///   it — the MOVE-under-a-live-loan negative control runs in every test
///   below, not in one of them.
const LOAN_PRELUDE: &str = r#"
type Option = enum::<T> { Some(T), None };
type Reader = struct {
    buf: u8.&raw mut,
    cap: usize,
    line: usize,
    n: usize,
} only move with {
    impl Self {
        next_line = fn::<@b>(r: Self.&mut::<@b>) -> Option::<usize.&::<@b>> {
            if r.*.n == 0 {
                if r.*.line == 0 { return ::None; };
                return ::Some(r.*.line.&);
            };
            r.refill();
            ::Some(r.*.line.&)
        };
        refill = fn::<@b>(r: Self.&mut::<@b>) -> () {
            r.*.n = r.*.n - 1;
            r.*.line = r.*.line + 1;
        };
        deinit = fn(r: Self) -> () {
            let Reader(struct { buf, cap, .. }) = r;
            unsafe { dealloc_array(buf, cap); };
        };
    }
};
static reader_new = fn(n: usize) -> Reader {
    let buf = match alloc_array::<u8>(n) { ::Ok(p) => p, ::Err => panic("out of memory") };
    Reader(struct { buf, cap = n, line = 0, n })
};
"#;

fn check_reader_loans(body: &str, expect: Expect) {
    check_loans(&format!("{LOAN_PRELUDE}{body}"), expect);
}

/// THE WITNESS: `l1` views the reader's own storage, the second
/// `next_line` moves the reader on, and the read afterwards is a read
/// through an invalidated borrow. Refused at the second use of `m`, naming
/// the other two sites in the interpreter's own vocabulary. Exactly ONE
/// refusal: the `r.deinit()` closing every test in this family is the
/// negative control for the MOVE arm and has to stay silent.
#[test]
fn a_view_read_after_the_reader_moves_on_is_refused() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    m.next_line();
    let out = match l1 { ::Some(line) => line.*, ::None => 0 };
    r.deinit();
    out
};
"#,
        expect![[r#"
            1049..1050: using `m` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 1030..1031: this borrow was created here
              note at 1084..1086: and it is still used here
        "#]],
    );
}

/// The sound twin, and why the rule is LIVENESS and not scope: the view is
/// read before the reader moves on, so it is dead at the second call even
/// though it is still in scope there.
#[test]
fn a_view_read_before_the_reader_moves_on_is_fine() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    let out = match l1 { ::Some(line) => line.*, ::None => 0 };
    m.next_line();
    r.deinit();
    out
};
"#,
        expect![[r#""#]],
    );
}

/// The corpus shape: sequential reads in a loop, each iteration's view
/// dead before the next call. Scope-based liveness refuses this — the
/// previous iteration's binding is in scope at every `next_line` — and it
/// is the single most important program the checker has to accept. Also
/// where the loop and the move rule meet: `m` spans the whole loop and
/// `r.deinit()` after it must still be clean.
#[test]
fn sequential_reads_with_dead_views_check_clean() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let mut total = 0;
    loop {
        match m.next_line() {
            ::Some(line) => { total = total + line.*; },
            ::None => break,
        };
    };
    r.deinit();
    total
};
"#,
        expect![[r#""#]],
    );
}

/// Copying the view OUT survives the next call: the copy has no region, so
/// nothing about it is a loan any more. `string_lib`'s `to_owned` in one
/// line.
#[test]
fn a_value_copied_out_of_a_view_survives_the_next_read() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let text = match m.next_line() { ::Some(line) => line.*, ::None => 0 };
    m.next_line();
    r.deinit();
    text
};
"#,
        expect![[r#""#]],
    );
}

/// The MOVE arm on the shape it exists for: the reader is disposed of
/// while a view into its storage is still needed. The interpreter calls
/// this "moved away" and detects it; now it does not compile.
#[test]
fn disposing_of_the_reader_under_a_live_view_is_refused() {
    check_reader_loans(
        r#"
static main = fn() -> usize {
    let mut r = reader_new(3);
    let m = r.&mut;
    let l1 = m.next_line();
    r.deinit();
    match l1 { ::Some(line) => line.*, ::None => 0 }
};
"#,
        expect![[r#"
            1049..1059: moving `r` here invalidates a borrow of it that is still live: the borrow points into storage this move takes away, and it is used after this point
              note at 1009..1015: this borrow was created here
              note at 1071..1073: and it is still used here
        "#]],
    );
}

/// Two exclusive loans of one local, both live. The oldest refusal there
/// is.
#[test]
fn two_live_exclusive_borrows_of_one_local_are_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    let b = n.&mut;
    bump(b);
    a.*
};
"#,
        expect![[r#"
            156..162: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 136..142: this borrow was created here
              note at 181..184: and it is still used here
        "#]],
    );
}

/// Sequenced, they are fine: the first loan is dead where the second is
/// taken. Reborrow-at-every-use is what makes this the common case.
#[test]
fn sequenced_exclusive_borrows_of_one_local_are_fine() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    bump(a);
    let b = n.&mut;
    b.*
};
"#,
        expect![[r#""#]],
    );
}

/// A `&mut` mint kills SHARED loans too — a write is foreign to every
/// loan it overlaps, whatever that loan's flavour.
#[test]
fn a_live_shared_borrow_is_killed_by_a_later_exclusive_one() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let m = n.&mut;
    bump(m);
    get(s)
};
"#,
        expect![[r#"
            211..217: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 194..197: this borrow was created here
              note at 240..241: and it is still used here
        "#]],
    );
}

/// The READ arm: an exclusive loan is the only way to the value while it
/// lasts, so reading the place around it — by taking a `.&`, or by simply
/// naming the local — invalidates it. Both spellings are the same access
/// and get the same refusal with a different verb.
#[test]
fn reading_the_root_around_a_live_exclusive_borrow_is_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static by_name = fn() -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let copy = n;
    bump(m);
    copy
};
static by_shared_borrow = fn() -> usize {
    let mut n: usize = 1;
    let m = n.&mut;
    let s = n.&;
    bump(m);
    get(s)
};
"#,
        expect![[r#"
            223..224: reading `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 200..206: this borrow was created here
              note at 235..236: and it is still used here
            351..354: borrowing `n` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 331..337: this borrow was created here
              note at 365..366: and it is still used here
        "#]],
    );
}

/// A foreign read does NOT disturb a shared loan — any number of readers
/// coexist, which is the interpreter's own rule (a read scans only the
/// exclusive nodes). The negative control for the arm above.
#[test]
fn a_shared_loan_tolerates_a_foreign_read() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let copy = n;
    get(s) + copy
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn many_live_shared_borrows_of_one_local_are_fine() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&;
    let b = n.&;
    get(a) + get(b)
};
"#,
        expect![[r#""#]],
    );
}

/// Path granularity, both directions, matching the aliasing tree's own
/// rule: two disjoint fields never conflict, and a borrow of the WHOLE
/// value conflicts with a borrow of a part.
#[test]
fn disjoint_field_borrows_are_independent_and_a_containing_one_is_not() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static disjoint = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    let b = p.y.&mut;
    bump(a);
    bump(b);
    p.x
};
static contains = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    let whole = p.&mut;
    bump(whole.*.y.&mut);
    a.*
};
"#,
        expect![[r#"
            386..392: using `p` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 360..368: this borrow was created here
              note at 424..427: and it is still used here
        "#]],
    );
}

/// A READ of a sibling field is as path-precise as a write of one: the
/// read is lowered against `w.b`, not against the whole of `w`.
#[test]
fn a_read_of_a_sibling_field_leaves_an_exclusive_loan_alone() {
    check_loans(
        r#"
type W = struct { a: usize, b: usize };
static f = fn() -> usize {
    let mut w = W(struct { a = 1, b = 2 });
    let r = w.a.&mut;
    let v = w.b;
    r.* + v
};
"#,
        expect![[r#""#]],
    );
}

/// The back edge. The invalidating mint sits AFTER the stale use in the
/// text, so nothing about reading the body forwards finds it — the loan
/// is live at the mint only because the loop runs again.
#[test]
fn a_loan_live_across_a_loops_back_edge_is_refused() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let mut i = 0;
    loop {
        i = i + get(s);
        bump(n.&mut);
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            266..272: using `n` mutably here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 194..197: this borrow was created here
              note at 249..250: and the loop brings control back round to this use of it
        "#]],
    );
}

/// And a loan dead before the loop starts is not live inside it: what
/// closes a loan over the back edge is a use on the far side of it, not
/// being declared outside.
#[test]
fn a_loan_dead_before_a_loop_does_not_close_over_it() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let s = n.&;
    let mut i = get(s);
    loop {
        bump(n.&mut);
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#""#]],
    );
}

/// A loan handed back through a `return` leaves, and nothing this body
/// does afterwards can reach it — the mint's block returns, so no later
/// access is reachable from it. A loan PARKED in a slot the caller reads
/// later has not left at all: its region reaches a universal, so it is
/// live everywhere, and the root moving on afterwards is a stale view in
/// the caller's hands. The first stays legal (it is `next_line`); the
/// second is refused.
#[test]
fn a_loan_parked_in_a_borrowed_slot_is_not_a_departure() {
    check_loans(
        r#"
type B = struct { v: usize };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static departs = fn::<@a>(b: B.&mut::<@a>, c: bool) -> usize.&::<@a> {
    if c { return b.*.v.&; };
    bump(b.*.v.&mut);
    b.*.v.&
};
static parks = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>) -> () {
    out.* = b.*.v.&;
    bump(b.*.v.&mut);
};
"#,
        expect![[r#"
            353..363: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 335..342: this borrow was created here
        "#]],
    );
}

/// Parking the loan inside a branch changes nothing: execution carries on
/// past the branch, so the store is reachable from the mint and the mint
/// reaches the later access. Both branch forms.
#[test]
fn a_loan_parked_inside_a_branch_is_still_not_a_departure() {
    check_loans(
        r#"
type B = struct { v: usize };
type Flag = enum { On, Off };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static parks_in_an_if = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>, c: bool) -> () {
    if c { out.* = b.*.v.&; };
    bump(b.*.v.&mut);
};
static parks_in_a_match = fn::<@a, @z>(b: B.&mut::<@a>, out: usize.&::<@a>.&mut::<@z>, f: Flag) -> () {
    match f { ::On => { out.* = b.*.v.&; }, ::Off => {}, };
    bump(b.*.v.&mut);
};
"#,
        expect![[r#"
            273..283: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 252..259: this borrow was created here
            462..472: using `b.*.v` mutably here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 425..432: this borrow was created here
        "#]],
    );
}

/// Re-minting into a slot the previous loan is DEAD in is ordinary code:
/// `s` is not live at the reassignment, so the loan it held is not live at
/// the second mint — with or without a loop. A slot carries one region for
/// every value it ever holds, so the old loan's region is live again as
/// soon as the slot is; what keeps the old loan from coming back is that
/// it left scope at the point it went dead.
#[test]
fn re_minting_a_loan_into_a_dead_slot_is_fine() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static slot_no_loop = fn() -> usize {
    let mut a = 1;
    let mut s = a.&mut;
    let x = s.*;
    s = a.&mut;
    let y = s.*;
    x + y
};
static slot_in_a_loop = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&mut;
    let mut i = 0;
    loop {
        bump(s);
        s = n.&mut;
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#""#]],
    );
}

/// A holder reassigned to a loan of ANOTHER local frees the first: `r`'s
/// region is live again after `r = b.&mut`, but `a`'s loan left scope at
/// the point `r` went dead and does not come back. In a straight line
/// and in a loop the write to `a` is fine; a reassignment on one path
/// only keeps `a`'s loan in scope on the other, and the write is refused
/// with the loan's own mint as the companion. (rustc's answer on all
/// three.)
#[test]
fn reassigning_a_holder_frees_the_loan_it_held() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static straight = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    bump(r);
    r = b.&mut;
    a = 5;
    bump(r);
    a + b
};
static witness = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut s = a.&mut;
    let x = s.*;
    s = b.&mut;
    a = 99;
    x + s.*
};
static in_a_loop = fn() -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    let mut i: usize = 0;
    loop {
        bump(r);
        r = b.&mut;
        a = a + 1;
        i = i + 1;
        if i > 3 { break a + b; };
    }
};
static conditional = fn(f: bool) -> usize {
    let mut a = 1;
    let mut b = 2;
    let mut r = a.&mut;
    bump(r);
    if f { r = b.&mut; };
    a = 5;
    bump(r);
    a + b
};
"#,
        expect![[r#"
            794..795: writing to `a` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 739..745: this borrow was created here
              note at 806..807: and it is still used here
        "#]],
    );
}

/// One report per access expression and loan. `a = a + 1` is two MIR
/// points (the read into a temp, the write from it) with one origin, and
/// reports once; a call that reads two borrowed locals kills two loans
/// at one point, and reports both.
#[test]
fn one_report_per_access_expression_and_loan() {
    check_loans(
        r#"
static take2 = fn(x: usize, y: usize) -> usize { x + y };
static read_modify_write = fn() -> usize {
    let mut a = 1;
    let p = a.&mut;
    a = a + 1;
    p.*
};
static two_loans_one_call = fn() -> usize {
    let mut x = 1;
    let mut y = 2;
    let p = x.&mut;
    let q = y.&mut;
    let s = take2(x, y);
    p.* + q.* + s
};
"#,
        expect![[r#"
            149..154: reading `a` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 133..139: this borrow was created here
              note at 160..163: and it is still used here
            301..312: reading `x` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 261..267: this borrow was created here
              note at 318..321: and it is still used here
            301..312: reading `y` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 281..287: this borrow was created here
              note at 324..327: and it is still used here
        "#]],
    );
}

#[test]
fn the_same_two_loans_in_two_locals_are_fine() {
    check_loans(
        r#"
static two_locals = fn() -> usize {
    let mut a = 1;
    let p = a.&mut;
    let x = p.*;
    let q = a.&mut;
    let y = q.*;
    x + y
};
"#,
        expect![[r#""#]],
    );
}

/// Two mints into one slot from the two arms of an `if`: neither arm's
/// mint is reachable from the other's, so nothing conflicts.
#[test]
fn one_slot_filled_from_two_arms_is_fine() {
    check_loans(
        r#"
static f = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let r: usize.&mut::<@_> = if c { n.&mut } else { n.&mut };
    r.*
};
"#,
        expect![[r#""#]],
    );
}

/// The WRITE arm. An assignment goes around every borrow of anything it
/// overlaps, and `let b = n.&; n = 99; b.*` is the shape it exists for:
/// the borrow reads storage the assignment has already overwritten. A bare
/// local and a place, because they are one rule.
#[test]
fn writing_to_the_root_under_a_live_loan_is_refused() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static to_a_local = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    n = 99;
    b.*
};
static through_a_place = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    p.x = 7;
    a.*
};
"#,
        expect![[r#"
            128..130: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 115..118: this borrow was created here
              note at 136..139: and it is still used here
            260..261: writing to `p.x` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 240..248: this borrow was created here
              note at 267..270: and it is still used here
        "#]],
    );
}

/// Its two negative controls: a write to a DISJOINT field is not foreign
/// to the loan at all, and a write after the loan is dead is ordinary code.
#[test]
fn a_write_beside_a_loan_rather_than_over_it_is_fine() {
    check_loans(
        r#"
type P = struct { x: usize, y: usize };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static disjoint_field = fn() -> usize {
    let mut p = P(struct { x = 1, y = 2 });
    let a = p.x.&mut;
    p.y = 7;
    a.*
};
static after_the_loan_is_dead = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    let out = get(b);
    n = 99;
    out + n
};
"#,
        expect![[r#""#]],
    );
}

/// An assignment to a POINTER is shallow: it replaces what `p` holds and
/// touches nothing `p` pointed at, so a loan through the old pointee is
/// not invalidated — it is ended. Reading through `p` afterwards sees the
/// new pointee and no stale loan.
#[test]
fn reassigning_a_pointer_ends_the_loans_through_it_without_conflict() {
    check_loans(
        r#"
static f = fn() -> usize {
    let mut a: usize = 1;
    let mut b: usize = 2;
    let mut p = a.&mut;
    let r = p.*.&mut;
    p = b.&mut;
    r.* + p.*
};
"#,
        expect![[r#""#]],
    );
}

/// `string_lib.must`'s main loop in miniature, the write arm's corpus
/// negative control. An owner is borrowed, disposed of and REASSIGNED in
/// one iteration, over and over: the move, the assignment and the loop's
/// back edge over both. Clean, because the owner is region-free: a
/// `String` owns its bytes, so the loan taken in the condition is finished
/// by the time the condition is answered.
#[test]
fn reassigning_an_owner_beside_its_dead_loans_is_fine() {
    check_loans(
        r#"
type Text = struct { buf: u8.&raw mut, cap: usize } only move with {
    impl Self {
        len = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.cap };
        drop = fn(s: Self) -> () {
            let Text(struct { buf, cap, .. }) = s;
            unsafe { dealloc_array(buf, cap); };
        };
    }
};
static text_new = fn(n: usize) -> Text {
    let buf = match alloc_array::<u8>(n) { ::Ok(p) => p, ::Err => panic("out of memory") };
    Text(struct { buf, cap = n })
};
static main = fn() -> usize {
    let mut longest = text_new(1);
    let mut i = 1;
    loop {
        if longest.&.len() < i {
            longest.drop();
            longest = text_new(i);
        };
        i = i + 1;
        if i > 3 { break; };
    };
    let out = longest.&.len();
    longest.drop();
    out
};
"#,
        expect![[r#""#]],
    );
}

/// A dot-call's receiver binds LAST (the self-last rule), and MIR lowers
/// it last, so the receiver's inserted reborrow is an access that comes
/// after every argument's mint. `c.bump(c.*.a.&mut)` and its hoisted
/// spelling are one program and get one answer.
#[test]
fn a_receiver_reborrow_comes_after_the_arguments() {
    check_loans(
        r#"
type Cell = struct { a: usize } with {
    impl Self {
        bump = fn::<@b>(v: usize.&mut::<@b>, c: Self.&mut::<@b>) -> usize { c.*.a + v.* };
    }
};
static inline_arg = fn::<@a>(c: Cell.&mut::<@a>) -> usize { c.bump(c.*.a.&mut) };
static hoisted_arg = fn::<@a>(c: Cell.&mut::<@a>) -> usize {
    let v = c.*.a.&mut;
    c.bump(v)
};
"#,
        expect![[r#"
            216..217: using `c` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 223..233: this borrow was created here
              note at 216..234: and it is still used here
            327..328: using `c` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 311..321: this borrow was created here
              note at 327..336: and it is still used here
        "#]],
    );
}

/// Two arms of one branch never both run, so a use in the `else` arm
/// cannot keep a loan live across a mint in the `then` arm: the use is
/// not reachable from the mint. Both arm orders, both accepted.
#[test]
fn a_use_and_a_mint_in_sibling_arms_do_not_conflict() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static use_in_else = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { bump(n.&mut); 0 } else { get(s) }
};
static use_in_then = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { get(s) } else { bump(n.&mut); 0 }
};
"#,
        expect![[r#""#]],
    );
}

/// A borrow taken THROUGH a borrow is a reborrow of it, and the parent's
/// region covers the child's — so invalidating the root reaches the
/// child. One level, three levels, shared flavour, and the field form.
#[test]
fn a_borrow_through_a_borrow_is_tied_to_its_parent() {
    check_loans(
        r#"
type P = struct { a: usize, b: usize };
static one_level = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&mut;
    let c = b.*.&mut;
    n = 99;
    c.*
};
static three_deep = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&mut;
    let c = b.*.&mut;
    let d = c.*.&mut;
    n = 99;
    d.*
};
static shared_flavor = fn() -> usize {
    let mut n: usize = 1;
    let b = n.&;
    let c = b.*.&;
    n = 99;
    c.*
};
static field_form = fn() -> usize {
    let mut p = P(struct { a = 1, b = 2 });
    let m = p.&mut;
    let c = m.*.a.&mut;
    p.a = 99;
    c.*
};
"#,
        expect![[r#"
            152..154: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 114..120: this borrow was created here
              note at 160..163: and it is still used here
            301..303: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 241..247: this borrow was created here
              note at 309..312: and it is still used here
            425..427: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 393..396: this borrow was created here
              note at 433..436: and it is still used here
            574..576: writing to `p.a` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 532..538: this borrow was created here
              note at 582..585: and it is still used here
        "#]],
    );
}

/// A nested pointer place is spelled in MIR as a copy of the inner
/// pointer into a temp; the loan and the later access are both rooted
/// back at the outer pointer, so they meet.
#[test]
fn a_loan_through_a_nested_pointer_place_is_rooted_at_the_outer_pointer() {
    check_loans(
        r#"
static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize {
    let c = bb.*.*.&mut;
    let d = bb.*.*.&mut;
    c.* + d.*
};
"#,
        expect![[r#"
            106..117: using `bb.*.*` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 81..92: this borrow was created here
              note at 123..126: and it is still used here
        "#]],
    );
}

/// A sibling access through a NESTED pointer. The copy of `bb.*` into
/// the temp that `bb.*.*` is spelled through performs no access of its
/// own, so a loan of `bb.*.*.g` sees only the sibling's own path and
/// stays untouched — read, write, second borrow, or a raw address of the
/// whole record (minting one touches nothing, M08). Under a loan that
/// does overlap the sibling — of `bb.*.*.f` itself, of `bb.*`, of `bb` —
/// the access is refused at the expression the user wrote.
#[test]
fn a_sibling_access_through_a_nested_pointer_is_accepted() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static read = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; let y = bb.*.*.f; x.* = 1; y };
static write = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; bb.*.*.f = 3; x.* = 1; x.* };
static borrow = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.g.&mut; let y = bb.*.*.f.&mut; x.* = 1; y.* = 2; x.* };
static address = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> P.&raw { let x = bb.*.*.g.&mut; let p = bb.*.*.&raw; x.* = 1; p };
"#,
        expect![[""]],
    );
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static under_field = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.*.f.&mut; let y = bb.*.*.f; x.* = 1; y };
static under_inner = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.*.&mut; let y = bb.*.*.f; x.*.*.f = 1; y };
static under_outer = fn::<@a, @b>(mut bb: P.&mut::<@a>.&mut::<@b>) -> usize { let x = bb.&mut; let y = bb.*.*.f; x.*.*.*.f = 1; y };
"#,
        expect![[r#"
            146..154: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 123..136: this borrow was created here
              note at 162..163: and it is still used here
            271..279: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 252..261: this borrow was created here
              note at 281..284: and it is still used here
            402..410: reading `bb.*.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 385..392: this borrow was created here
              note at 412..415: and it is still used here
        "#]],
    );
}

/// A write THROUGH a place temp still reads what it writes. The arm that
/// drops a defining copy's access covers the temp ITSELF — `t = copy p`,
/// the copy that makes `t` stand for `p` — and nothing else: once the
/// destination is projected (`bb.*.* = v` writes through the temp `bb.*`
/// stands for), the statement is an ordinary write whose right-hand side
/// is read like any other operand. Drop the `dest.projection.is_empty()`
/// half of that guard and this program is silently accepted.
#[test]
fn a_write_through_a_place_temp_still_reads_its_source() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static f = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>, mut v: P) -> usize {
    let y = v.f.&mut;
    bb.*.* = v;
    y.* = 1;
    y.*
};
"#,
        expect![[r#"
            150..151: reading `v` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 127..135: this borrow was created here
              note at 163..164: and it is still used here
        "#]],
    );
}

/// A borrowed `match` reads its scrutinee's DEREF, not the borrow itself.
/// The tag lives behind `p`, so dispatching on `p` while a `.&mut` of
/// `p.*` is live is a read of `p.*`, and the refusal says so — naming a
/// move of `p` instead would name an operation the program never performs
/// and point the writer at the wrong thing to change.
#[test]
fn a_borrowed_match_reads_the_scrutinee_through_its_deref() {
    check_loans(
        r#"
type E = enum { A, B };
static f = fn::<@a>(p: E.&mut::<@a>) -> usize {
    let s = p.*.&mut;
    let v: usize = match p { ::A => 1, ::B => 2 };
    s.* = ::A;
    v
};
"#,
        expect![[r#"
            120..121: reading `p.*` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 85..93: this borrow was created here
              note at 156..159: and it is still used here
        "#]],
    );
}

/// Where the blame sits. A bare-name argument is an operand of the call
/// with no expression of its own, so the call is squiggled; a projected
/// argument is read into a temp at its own expression, and the read is.
#[test]
fn a_projected_argument_blames_the_read_and_a_bare_name_the_call() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static take = fn(v: usize) -> usize { v };
static projected = fn::<@a>(p: P.&mut::<@a>) -> usize { let x = p.*.f.&mut; let n = take(p.*.f); x.* = n; 0 };
static bare = fn() -> usize { let mut a: usize = 1; let x = a.&mut; let n = take(a); x.* = n; 0 };
"#,
        expect![[r#"
            173..178: reading `p.*.f` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 148..158: this borrow was created here
              note at 187..188: and it is still used here
            271..278: reading `a` here invalidates an exclusive borrow of it that is still live: a `.&mut` is the only way to the value while it lasts, and this one is used after this point
              note at 255..261: this borrow was created here
              note at 286..287: and it is still used here
        "#]],
    );
}

/// A tail-position read through a borrow is one access, reported once
/// at the read the user wrote. The temp it is read into, and the return
/// slot that temp is copied to, are values, not places: no loan is rooted
/// in either, so neither the copy nor the return is a second access.
#[test]
fn a_tail_read_through_a_borrow_reports_once_at_the_read() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static g1 = fn::<@a, @b>(out: usize.&mut::<@a>.&mut::<@b>, r: usize.&mut::<@a>) -> usize { out.* = r; r.* };
static g2 = fn::<@a, @b>(out: P.&mut::<@a>.&mut::<@b>, r: P.&mut::<@a>) -> usize { out.* = r; r.*.f };
static g3 = fn::<@a, @b>(out: P.&mut::<@a>.&mut::<@b>, r: P.&mut::<@a>) -> P { out.* = r; r.* };
static h = fn::<@a, @b>(out: usize.&mut::<@a>.&mut::<@b>, r: usize.&mut::<@a>) -> usize { out.* = r; let v = r.*; v };
"#,
        expect![[r#"
            143..146: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 140..141: this borrow was created here
            244..249: reading `r.*.f` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 241..242: this borrow was created here
            343..346: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 340..341: this borrow was created here
            459..462: reading `r.*` here invalidates an exclusive borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 448..449: this borrow was created here
        "#]],
    );
}

/// The companion names why the loan is live. A flow into a universal
/// reaching the access is always the true reason, so it wins over a
/// holder's next use: after `r = q.*.g.&mut`, `r.*` reads `q`'s loan,
/// not `m`'s, and the note must not send the reader there.
#[test]
fn a_loan_handed_back_says_so_over_a_reassigned_holders_use() {
    check_loans(
        r#"
type P = struct { f: usize, g: usize };
static reassigned = fn::<@a, @b>(m: P.&mut::<@a>, out: usize.&::<@a>.&mut::<@b>, q: P.&::<@a>) -> usize {
    let mut r = m.*.f.&;
    out.* = r;
    r = q.*.g.&;
    m.*.f = 5;
    r.*
};
static held = fn::<@a, @b>(m: P.&mut::<@a>, out: usize.&::<@a>.&mut::<@b>) -> usize {
    let r = m.*.f.&;
    out.* = r;
    m.*.f = 5;
    r.*
};
"#,
        expect![[r#"
            216..217: writing to `m.*.f` here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 163..170: this borrow was created here
            364..365: writing to `m.*.f` here invalidates a borrow of it that is still live: the borrow is handed back to the caller, and this invalidates it before the caller can read it
              note at 328..335: this borrow was created here
        "#]],
    );
}

/// A field that HOLDS a function is a place, not a member path: `b.f(4)`
/// reads `b.f` and nothing else, so it leaves a loan of `b.v` alone.
#[test]
fn a_call_through_a_function_valued_field_reads_only_that_field() {
    check_loans(
        r#"
type Box = struct { v: usize, f: fn(usize) -> usize };
static twice = fn(n: usize) -> usize { n + n };
static main = fn() -> usize {
    let mut b = Box(struct { v = 1, f = twice });
    let r = b.v.&mut;
    let y = b.f(4);
    r.* + y
};
"#,
        expect![[r#""#]],
    );
}

/// A loop-carried slot initialised from a DIFFERENT root: no loan of `n`
/// exists before the loop, and the in-loop mint is followed by the write
/// in the same iteration, with the slot read on the next one.
#[test]
fn a_loop_carried_slot_initialised_from_another_root_is_refused() {
    check_loans(
        r#"
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };
static f = fn() -> usize {
    let mut n: usize = 1;
    let mut other: usize = 7;
    let mut s = other.&mut;
    let mut i = 0;
    loop {
        bump(s);
        s = n.&mut;
        n = 99;
        i = i + 1;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            261..263: writing to `n` here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 241..247: this borrow was created here
              note at 225..226: and the loop brings control back round to this use of it
        "#]],
    );
}

/// The same loop-carried shape with a NON-borrow pre-loop init: an `Opt`
/// slot that starts `::None` and only ever holds a borrow from inside the
/// loop. The loan sits inside the slot's payload, and the slot is live
/// across the back edge.
#[test]
fn a_loop_carried_option_slot_starting_none_is_refused() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn() -> usize {
    let mut n: usize = 1;
    let mut s: Opt::<usize.&::<@_>> = Opt::<usize.&::<@_>>::None;
    let mut i = 0;
    loop {
        i = i + match s { ::Some(r) => r.*, ::None => 0 };
        s = Opt::<usize.&::<@_>>::Some(n.&);
        n = 99;
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            306..308: writing to `n` here invalidates a borrow of it that is still live: the loop brings control back round to a use of the borrow, which would then read through an invalidated borrow
              note at 288..291: this borrow was created here
              note at 212..213: and the loop brings control back round to this use of it
        "#]],
    );
}

/// The back edge makes two SIBLING ARMS both run, in different iterations:
/// an iteration taking the minting arm hands the loan to an iteration
/// taking the invalidating one. Both arm orders and the `match` spelling.
#[test]
fn sibling_arms_inside_a_loop_are_not_alternatives_across_iterations() {
    check_loans(
        r#"
type Flag = enum { On, Off };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static then_mint = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        if c { s = n.&; } else { n = 99; };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static else_mint = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        if c { n = 99; } else { s = n.&; };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static match_mint = fn(f: Flag) -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        match f { ::On => { s = n.&; }, ::Off => { n = 99; }, };
        i = i + get(s);
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            245..247: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 173..176: this borrow was created here
              note at 272..273: and it is still used here
            454..456: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 400..403: this borrow was created here
              note at 499..500: and it is still used here
            718..720: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 628..631: this borrow was created here
              note at 748..749: and it is still used here
        "#]],
    );
}

/// The two controls that shape needs: with no arms at all the loop shape
/// is refused (nothing about arms is what catches it), and with no loop
/// the arms really are alternatives and the write in one is fine.
#[test]
fn sibling_arms_are_alternatives_only_outside_a_loop() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static no_arms = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        s = n.&;
        n = 99;
        i = i + get(s);
        if i > 3 { break i; };
    }
};
static no_loop = fn(c: bool) -> usize {
    let mut n: usize = 1;
    let s = n.&;
    if c { n = 99; 0 } else { get(s) }
};
"#,
        expect![[r#"
            198..200: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 181..184: this borrow was created here
              note at 222..223: and it is still used here
        "#]],
    );
}

/// Across a back edge the mint precedes the access in TIME however the
/// text reads: iteration `i` mints, iteration `i+1` invalidates and then
/// reads. Three ways the loan reaches the slot — directly, through a
/// call, and through a member dot-call.
#[test]
fn a_mint_below_its_invalidator_still_conflicts_across_the_back_edge() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static id = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> { x };
type W = struct { v: usize } with {
    impl Self {
        lend = fn::<@b>(w: Self.&::<@b>) -> usize.&::<@b> { w.*.v.& };
    }
};
static plain = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        n = 99;
        i = i + get(s);
        s = n.&;
        if i > 3 { break i; };
    }
};
static via_call = fn() -> usize {
    let mut n: usize = 1;
    let mut s = n.&;
    let mut i = 0;
    loop {
        n = 99;
        i = i + get(s);
        s = id(n.&);
        if i > 3 { break i; };
    }
};
static via_member = fn() -> usize {
    let mut w = W(struct { v = 1 });
    let mut s = w.&.lend();
    let mut i = 0;
    loop {
        w.v = 99;
        i = i + get(s);
        s = w.&.lend();
        if i > 3 { break i; };
    }
};
"#,
        expect![[r#"
            374..376: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 327..330: this borrow was created here
              note at 398..399: and it is still used here
            582..584: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 535..538: this borrow was created here
              note at 606..607: and it is still used here
            816..818: writing to `w.v` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 760..763: this borrow was created here
              note at 840..841: and it is still used here
        "#]],
    );
}

/// A CALL is a use of every loan flowing into it: the temps holding the
/// arguments are live until the call. `two(n.&mut, n.&mut)` is the
/// textbook two-exclusive-borrows error, refused whether the parameters
/// take independent regions or one, and however the mints nest.
#[test]
fn a_call_is_a_use_of_every_loan_passed_to_it() {
    check_loans(
        r#"
static two = fn::<@a, @b>(x: usize.&mut::<@a>, y: usize.&mut::<@b>) -> usize { x.* + y.* };
static one = fn::<@a>(x: usize.&mut::<@a>, y: usize.&mut::<@a>) -> usize { x.* + y.* };
static id = fn::<@a>(x: usize.&mut::<@a>) -> usize.&mut::<@a> { x };
static flat = fn() -> usize { let mut n: usize = 1; two(n.&mut, n.&mut) };
static nested_first = fn() -> usize { let mut n: usize = 1; two(id(n.&mut), n.&mut) };
static nested_second = fn() -> usize { let mut n: usize = 1; two(n.&mut, id(n.&mut)) };
static both_nested = fn() -> usize { let mut n: usize = 1; two(id(n.&mut), id(n.&mut)) };
static nested_let = fn() -> usize {
    let mut n: usize = 1;
    let a = n.&mut;
    two(a, n.&mut)
};
static shared_binder = fn() -> usize { let mut n: usize = 1; one(n.&mut, n.&mut) };
"#,
        expect![[r#"
            314..320: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 306..312: this borrow was created here
              note at 302..321: and it is still used here
            401..407: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 392..398: this borrow was created here
              note at 385..408: and it is still used here
            488..494: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 477..483: this borrow was created here
              note at 473..496: and it is still used here
            578..584: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 566..572: this borrow was created here
              note at 559..586: and it is still used here
            683..689: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 664..670: this borrow was created here
              note at 676..690: and it is still used here
            767..773: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 759..765: this borrow was created here
              note at 755..774: and it is still used here
        "#]],
    );
}

/// `examples/borrows.must` says of its join fixture that borrowing `n`
/// both ways at once is exactly the exclusivity violation the static
/// checker rejects. The example itself borrows two DIFFERENT locals; this
/// is the claim, verified.
#[test]
fn the_borrows_example_prose_claim_holds() {
    check_loans(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static pick_flavors = fn::<@a>(x: usize.&::<@a>, m: usize.&mut::<@a>, c: bool) -> usize {
    get(if c { x } else { m })
};
static both_ways = fn() -> usize {
    let mut n: usize = 1;
    pick_flavors(n.&, n.&mut, true)
};
"#,
        expect![[r#"
            266..272: using `n` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 261..264: this borrow was created here
              note at 248..279: and it is still used here
        "#]],
    );
}

/// Matching a borrowed scrutinee binds BORROWS of the payloads (M13): each
/// binder is a loan rooted at the scrutinee's borrow, so writing to the
/// matched value while a binder is still needed is refused, and finishing
/// with the binder first is fine.
#[test]
fn a_payload_borrow_dies_when_the_matched_value_is_written() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
static stale = fn() -> usize {
    let mut o = Opt::<usize>::Some(1);
    match o.&mut {
        ::Some(x) => { o = Opt::<usize>::None; x.* },
        ::None => 0,
    }
};
static finished = fn() -> usize {
    let mut o = Opt::<usize>::Some(1);
    match o.&mut {
        ::Some(x) => { let v = x.*; o = Opt::<usize>::None; v },
        ::None => 0,
    }
};
"#,
        expect![[r#"
            157..175: writing to `o` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 121..127: this borrow was created here
              note at 177..180: and it is still used here
        "#]],
    );
}

/// `Return` is the storage end of every local. A nested literal returning
/// a borrow of its own local at `@_` reaches no universal of the
/// enclosing item, so the outlives check is silent; the loan is live
/// where the literal's body returns, which is what escapes.
#[test]
fn a_nested_literal_borrow_of_its_own_local_cannot_escape() {
    check_loans(
        "static main = fn () -> usize {\n\
             let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };\n\
             f().*\n\
         };",
        expect![[r#"
            79..82: borrowed value does not live long enough: this borrows a local, but the borrow is still live when the body returns and the local is gone by then
        "#]],
    );
}

/// The conditional-return case. `map.get(key)`'s loan is handed back
/// from the `::Some` arm, which ties it to `@a` — at that point, and not
/// before: the `::None` arm is not reachable from the flow, so the loan is
/// not live there and `insert` may touch the map. Location-insensitive
/// liveness (NLL) refuses this program; `examples/reborrow.must` is built
/// on it.
#[test]
fn a_loan_handed_back_from_one_arm_is_not_live_in_the_other() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
type Map = struct { k: usize, v: usize, used: bool } with {
    impl Self {
        get = fn::<@b>(key: usize, m: Self.&mut::<@b>) -> Opt::<usize.&mut::<@b>> {
            if m.*.used == true { if m.*.k == key { return ::Some(m.*.v.&mut); }; };
            ::None
        };
        insert = fn::<@b>(key: usize, val: usize, m: Self.&mut::<@b>) -> () {
            m.*.k = key;
            m.*.v = val;
            m.*.used = true;
        };
    }
};
static get_or_default = fn::<@a>(key: usize, val: usize, map: Map.&mut::<@a>) -> usize.&mut::<@a> {
    match map.get(key) {
        ::Some(v) => v,
        ::None => {
            map.insert(key, val);
            match map.get(key) { ::Some(v) => v, ::None => panic("just inserted") }
        },
    }
};
"#,
        expect![""],
    );
}

/// The same shape flowing into a SLOT with a body-local region instead of
/// into the signature: the slot is live in both arms (it is read after the
/// match) and its region is covered wherever it is live, so the loan
/// stored in one arm counts as live in the other, on a path where it was
/// never stored. Refused — the hop into a body region is not dated, only
/// the hop into a universal is; this is the over-refusal M19 records.
#[test]
fn a_loan_stored_in_a_live_slot_in_one_arm_is_live_in_the_other() {
    check_loans(
        r#"
type Opt = enum::<T> { Some(T), None };
type Map = struct { k: usize, v: usize, used: bool } with {
    impl Self {
        get = fn::<@b>(key: usize, m: Self.&mut::<@b>) -> Opt::<usize.&mut::<@b>> {
            if m.*.used == true { if m.*.k == key { return ::Some(m.*.v.&mut); }; };
            ::None
        };
        insert = fn::<@b>(key: usize, val: usize, m: Self.&mut::<@b>) -> () {
            m.*.k = key;
            m.*.v = val;
            m.*.used = true;
        };
    }
};
static f = fn::<@a>(key: usize, val: usize, map: Map.&mut::<@a>, slot: usize.&mut::<@a>) -> usize {
    let mut r: usize.&mut::<@_> = slot;
    match map.get(key) {
        ::Some(v) => { r = v; },
        ::None => { map.insert(key, val); },
    };
    r.*
};
"#,
        expect![[r#"
            711..714: using `map` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 643..646: this borrow was created here
              note at 747..750: and it is still used here
        "#]],
    );
}

#[test]
fn a_borrowed_integer_match_tests_through_the_deref_too() {
    // The lens generalises with the pattern kind: an integer referent
    // dispatches by equality exactly as a character one does, so the test
    // reads THROUGH the borrow here for the same reason, and the binder
    // still takes the borrow itself.
    check_mir(
        r#"
static f = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param d
              _2: usize.&
              _3: usize
              _4: bool
              _5: usize.&  // other
              bb0:
                _2 = _1
                _4 = Eq(_2.*, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = _2
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_refused_literal_arm_lowers_no_test() {
    // A literal arm inference REFUSED is dead, the same way a wrong-enum
    // variant arm is: no equality test between mismatched types is
    // planted, and the arm body lowers into an orphan block nothing
    // targets (`bb2`). Both literal kinds answer this identically —
    // `'a'` against a `usize` used to emit a live `Eq(_2, 'a')` in the
    // ENTRY block, ill-typed MIR behind nothing. The match's own value
    // trap is unchanged: the refusal is still what the program dies of.
    check_mir(
        r#"
static f = fn (d: usize) -> usize {
    match d {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param d
              _2: usize
              _3: usize
              _4: usize
              bb0:
                _2 = _1
                _3 = 0
                goto -> bb1
              bb1:
                _4 = trap "type mismatch: this `match` is on a `usize`, and `char` cannot match one" -> bb3
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _0 = _4
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- materialized temporaries (M12) --------------------------------------

/// A MATERIALIZED TEMPORARY lowers to one local, assigned once and
/// addressed once — `_2` below, with no name comment beside it, which is
/// the whole of what makes it a temporary down here: a named local renders
/// its name, and the debugger's variables panel shows only those. The
/// borrow is an ordinary [`crate::Rvalue::Borrow`] into the temporary's own
/// field (`& _2.1`), and the local is marked addressable exactly as a
/// borrowed `let`'s is — which this dump does not render.
#[test]
fn a_borrowed_temporary_lowers_to_one_addressable_local() {
    check_mir(
        "type Pair = struct { a: usize, b: usize };\n\
         static mk = fn() -> Pair { Pair(struct { a = 1, b = 2 }) };\n\
         static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static f = fn() -> usize { get(mk().b.&) };",
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
              _2: usize
              bb0:
                _2 = _1.*
                _0 = _2
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
                _3 = & _2.1
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

/// `.&raw` reaches the SAME arm as `.&`: `lower_addr_of_flavored` handles
/// both flavors through one `body.temp_local(root)` check
/// ([`crate::lower::LowerCtx::lower_addr_of_flavored`]), so a raw
/// address-of a non-place operand lowers to the identical shape — one
/// nameless addressable local, assigned once, addressed once — with only
/// the `Rvalue` (`&raw _1` rather than `& _1`) telling the two apart.
#[test]
fn a_raw_address_of_a_temporary_lands_on_the_nameless_addressable_local() {
    check_mir(
        "static mk = fn() -> usize { 7 };\n\
         static main = fn() -> usize { unsafe { mk().&raw.* } };",
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
                _3 = &raw _2
                _4 = _3.*
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

/// A loan of a loop-body temporary cannot survive the back edge: the next
/// iteration's store into the temporary is a write to the loan's root,
/// and a loan carried across the back edge (through `r`) is still live
/// there. This is the one piece of M12's block rule the checker enforces
/// today, and it enforces it for free — no storage liveness is involved,
/// only the ordinary write-kills-loan rule. The blame names the root as
/// "this temporary": there is no name to quote and no second mention to
/// disambiguate from.
#[test]
fn a_loan_of_a_loop_body_temporary_cannot_survive_the_back_edge() {
    check_loans(
        "static mk = fn() -> usize { 7 };\n\
         static keep = fn::<@a>(r: usize.&::<@a>, s: usize.&::<@a>) -> usize.&::<@a> { s };\n\
         static f = fn() -> usize {\n\
             let n: usize = 1;\n\
             let mut r = n.&;\n\
             let mut i: usize = 0;\n\
             loop {\n\
                 if i == 2 { break; };\n\
                 r = keep(r, mk().&);\n\
                 i = i + 1;\n\
             };\n\
             r.*\n\
         };",
        expect![[r#"
            241..245: writing to this temporary here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 241..247: this borrow was created here
              note at 233..248: and it is still used here
        "#]],
    );
}

/// A loan that reaches THROUGH a temporary is still a loan of what it came
/// from: the temporary holds a borrow of `n`, so `n` is spoken for until
/// the last use of the temporary, and writing to it in between is refused
/// — named as `n`, because that is the storage the loan is of. The loan
/// stays live across a local that no name mentions.
#[test]
fn a_loan_reaching_through_a_temporary_is_still_a_loan_of_its_source() {
    check_loans(
        "static id = fn::<@b>(r: usize.&::<@b>) -> usize.&::<@b> { r };\n\
         static f = fn() -> usize {\n\
             let mut n: usize = 1;\n\
             let rr = id(n.&).&;\n\
             n = 5;\n\
             rr.*.*\n\
         };",
        expect![[r#"
            136..137: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
              note at 124..127: this borrow was created here
              note at 139..143: and it is still used here
        "#]],
    );
}

/// The nested-literal escape — a borrow still live where a `fn` literal's
/// body returns, at `@_`, which reaches no universal of the enclosing item
/// — names the temporary too, so the two escape fences (this one and
/// `hir::outlives`'s universal-region one) read the same either way.
#[test]
fn a_nested_literal_returning_a_borrow_of_a_temporary_names_the_temporary() {
    check_loans(
        "static mk = fn() -> usize { 7 };\n\
         static f = fn() -> usize {\n\
             let g = fn() -> usize.&::<@_> { mk().& };\n\
             g().*\n\
         };",
        expect![[r#"
            92..98: borrowed value does not live long enough: this borrows a temporary, which lives no longer than the block that creates it, but the borrow is still live when the body returns
        "#]],
    );
}
