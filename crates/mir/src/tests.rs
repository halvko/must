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
    check_mir(
        r#"
static g = fn () -> usize { 1 }
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
