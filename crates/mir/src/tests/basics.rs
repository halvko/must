//! Lowering basics: items, `if`, calls and the traps broken code lowers to.

use super::*;

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
              _2: usize
              _3: bool
              _4: str
              _5: str  // label
              _6: str
              bb0:
                _2 = _1
                _3 = Lt(_2, 10)
                if _3 -> [then: bb1, else: bb2]
              bb1:
                _4 = "small"
                goto -> bb3
              bb2:
                _4 = "big"
                goto -> bb3
              bb3:
                _5 = _4
                _6 = _5
                _0 = _6
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
              _2: bool
              _3: ()
              _4: ()
              bb0:
                _2 = _1
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _4 = call builtin print("y") -> bb3
              bb2:
                _3 = ()
                goto -> bb4
              bb3:
                _3 = _4
                goto -> bb4
              bb4:
                _0 = _3
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
              _3: usize
              _4: usize  // x
              _5: usize
              bb0:
                _1 = 1
                _2 = _1
                _3 = Add(_2, 1)
                _4 = _3
                _5 = _4
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
              _2: bool
              _3: usize
              _4: !
              _5: usize  // v
              _6: usize
              bb0:
                _2 = _1
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _4 = call builtin panic("x") -> !
              bb2:
                _3 = 1
                goto -> bb4
              bb3:
                _3 = _4
                goto -> bb4
              bb4:
                _5 = _3
                _6 = _5
                _0 = _6
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
              _2: usize
              bb0:
                _2 = _1
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
              _3: {error}
              bb0:
                _1 = trap "expected an expression" -> bb1
              bb1:
                _2 = trap "syntax error: missing expression" -> bb2
              bb2:
                _3 = Add((), _2)
                _0 = _3
                return
            }
        "#]],
    );
}

#[test]
fn digit_group_commas_trap_instead_of_running_the_first_group() {
    check_mir(
        r#"
static a: i64 = 1,000;
static b = fn () -> i64 { -2,147 };
static c = fn () -> i64 { 2,147 };
"#,
        expect![[r#"
            item a:
            item b:
            fn b0() -> i64 {
              _0: i64  // return
              _1: {error}
              _2: {error}
              bb0:
                _1 = trap "digit groups cannot be separated by `,`; use `_`" -> bb1
              bb1:
                _2 = trap "syntax error: missing expression" -> bb2
              bb2:
                _0 = ()
                return
            }
            fn b1() -> fn() -> i64 {
              _0: fn() -> i64  // return
              bb0:
                _0 = fn b0
                return
            }
            item c:
            fn b0() -> i64 {
              _0: i64  // return
              _1: {error}
              _2: {error}
              bb0:
                _1 = trap "digit groups cannot be separated by `,`; use `_`" -> bb1
              bb1:
                _2 = trap "syntax error: missing expression" -> bb2
              bb2:
                _0 = _2
                return
            }
            fn b1() -> fn() -> i64 {
              _0: fn() -> i64  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn digit_group_commas_in_a_return_or_break_value_trap() {
    check_mir(
        r#"
static f = fn () -> () { return 1,000; };
static g = fn () -> () { loop { break 1,000; } };
"#,
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: !
              _2: {error}
              bb0:
                _1 = trap "digit groups cannot be separated by `,`; use `_`" -> bb1
              bb1:
                _2 = trap "syntax error: missing expression" -> bb2
              bb2:
                _0 = _2
                return
              bb3:
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
            item g:
            fn b0() -> () {
              _0: ()  // return
              _1: {error}
              _2: !
              _3: {error}
              bb0:
                goto -> bb1
              bb1:
                _2 = trap "digit groups cannot be separated by `,`; use `_`" -> bb3
              bb2:
                _0 = _1
                return
              bb3:
                _3 = trap "syntax error: missing expression" -> bb4
              bb4:
                _1 = _3
                goto -> bb2
              bb5:
                goto -> bb1
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
fn a_stray_token_at_a_block_end_leaves_no_tail() {
    check_mir(
        r#"
static f = fn () -> () {};
static g = fn () -> () { f(); ) };
"#,
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              bb0:
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
            item g:
            fn b0() -> () {
              _0: ()  // return
              _1: ()
              _2: ()
              bb0:
                _1 = call item f() -> bb1
              bb1:
                _2 = trap "expected an expression" -> bb2
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
              _3: usize
              bb0:
                _2 = _1
                _3 = Mul(_2, 2)
                _0 = _3
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
              _2: _
              bb0:
                _2 = _1
                _0 = _2
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
                _1 = trap "not enough information to infer the type of `id`; add a type annotation to its definition" -> bb1
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
fn wildcard_patterns_bind_nothing_but_still_evaluate() {
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
              bb0:
                _2 = Add(4, 5)
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
              _3: usize
              bb0:
                _2 = _1
                _3 = Mul(_2, 2)
                _0 = _3
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
              _2: fn() -> usize
              _3: usize
              bb0:
                _2 = _1
                _3 = trap "cannot call a value in a const context; whether it is a `const fn` is not known from its type" -> bb1
              bb1:
                _0 = _3
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
