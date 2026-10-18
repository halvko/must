//! Name resolution, holes, duplicates and dead-code warnings.

use super::*;

#[test]
fn unresolved_name() {
    check_diagnostics(
        "static main = fn { missing() };",
        expect![[r#"
            19..26: unresolved name `missing`
        "#]],
    );
}

#[test]
fn locals_params_and_shadowing_resolve() {
    check_diagnostics(
        r#"
static f = fn (a: str) {
    let b = a;
    let b = b;
    print(b);
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn let_initializer_does_not_see_its_own_binding() {
    check_diagnostics(
        "static f = fn { let x = x; };",
        expect![[r#"
            24..25: unresolved name `x`
        "#]],
    );
}

#[test]
fn let_hole_pattern_binds_nothing() {
    // `_` is not a name: a later use of `_` doesn't resolve to the binding
    // (it doesn't even parse as a reference — `_` only lexes as a hole).
    check_diagnostics(
        "static f = fn { let _ = 5; let x = _; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            35..36: expected an expression
        "#]],
    );
}

#[test]
fn let_hole_pattern_initializer_is_still_type_checked() {
    // The binding is discarded, but the initializer is fully inferred, so a
    // type error inside it is still reported.
    check_diagnostics(
        r#"static f = fn { let _: usize = "hello"; };"#,
        expect![[r#"
            31..38: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..28)
        "#]],
    );
}

#[test]
fn param_hole_pattern_binds_nothing() {
    check_diagnostics("static f = fn (_: usize) { };", expect![[r#""#]]);
}

#[test]
fn hole_named_static_item_gets_dead_code_warning() {
    // `_` binds nothing, so `5` can never be referenced — squiggle on `_`.
    check_diagnostics(
        "static _ = 5;",
        expect![[r#"
            7..8: this item binds nothing and its value cannot be used
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn hole_named_const_item_gets_dead_code_warning() {
    check_diagnostics(
        "const _ = 5;",
        expect![[r#"
            6..7: this item binds nothing and its value cannot be used
            10..11: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn named_item_gets_no_dead_code_warning() {
    check_diagnostics("static x: usize = 5;", expect![[r#""#]]);
}

#[test]
fn broken_item_with_missing_name_gets_no_dead_code_warning() {
    // No `NAME` node at all (not even a hole one) — distinct from
    // `static _ = ...`, so this must not also get the dead-code warning on
    // top of the parse error.
    check_diagnostics(
        "static = 5;",
        expect![[r#"
            7..8: expected a name for the item
            9..10: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn infer_let_hole_pattern() {
    check_infer(
        r#"static f = fn { let _ = 5; };"#,
        expect![[r#"
            11..28 'fn { let _ = 5; }': fn()
            14..28 '{ let _ = 5; }': ()
            24..25 '5': {number}
        "#]],
    );
}

#[test]
fn mutual_recursion_and_self_reference_resolve() {
    check_diagnostics(
        r#"
const fib1 = fn (n: usize) -> usize { fib2(n-1) + fib2(n-2) }
static fib2 = fn (n: usize) -> usize { fib1(n-1) + fib2(n-2) }
"#,
        expect![[r#""#]],
    );
}

#[test]
fn builtins_resolve() {
    check_diagnostics(
        r#"static main = fn { print("hi"); panic("boom"); };"#,
        expect![[r#""#]],
    );
}
