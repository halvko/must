//! Raw pointers: address-of, deref and pointer unification.

use super::*;

#[test]
fn raw_pointer_types_infer_and_display() {
    check_infer(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw;
    let q = x.&raw;
    unsafe { p.* }
};
"#,
        expect![[r#"
            15..110 'fn() -> usize {  ...': fn() -> usize
            29..110 '{     let mut x =...': usize
            43..44 'x': usize
            47..48 '1': usize
            58..59 'p': usize.&raw
            62..63 'x': usize
            62..68 'x.&raw': usize.&raw
            78..79 'q': usize.&raw
            82..83 'x': usize
            82..88 'x.&raw': usize.&raw
            94..108 'unsafe { p.* }': usize
            101..108 '{ p.* }': usize
            103..104 'p': usize.&raw
            103..106 'p.*': usize
        "#]],
    );
}

#[test]
fn addr_of_needs_no_mut_root() {
    // `.&raw` of a non-`mut` local, or of a field of one, is legal; a write
    // through it is UB the interpreter detects, not a checker error.
    check_diagnostics(
        "static main = fn { let x: usize = 1; let r = struct { a: usize = 1 }; let p = x.&raw; let q = r.a.&raw; };",
        expect![[r#""#]],
    );
}

#[test]
fn addr_of_items_is_fine() {
    check_diagnostics(
        "static s = 7;\nconst c = 8;\nstatic main = fn { let p = s.&raw; let q = c.&raw; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            54..55: not enough information to infer the type of `s`; add a type annotation to its definition (defined here at 7..8)
            70..71: not enough information to infer the type of `c`; add a type annotation to its definition (defined here at 20..21)
        "#]],
    );
}

/// A non-place `.&raw` operand materializes (M12) rather than erroring —
/// the only diagnostic left here is unrelated, the literal's own type
/// needing an annotation.
#[test]
fn addr_of_a_non_place_materializes() {
    check_diagnostics(
        "static main = fn { let p = 3.&raw; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn safe_borrow_expr_lowers_and_infers() {
    // `.&`/`.&mut` in expression position mint their own region variable
    // (no turbofish needed) and lower cleanly.
    check_diagnostics(
        "static main = fn { let mut x = 1; let a = x.&; let b = x.&mut; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn safe_borrow_type_without_a_region_is_reported() {
    // `T.&`/`T.&mut` in a field type is a SIGNATURE position: elision is
    // deferred, so an unannotated region is reported, not guessed.
    check_diagnostics(
        "type Bad = struct { r: usize.& };",
        expect![[r#"
            29..30: a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_works_inside_unsafe() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw; let q = unsafe { p.*.&raw }; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_still_requires_unsafe() {
    // The deref rule is uniform: the deref inside a `.&raw` place needs
    // `unsafe` like any other deref site.
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw; let q = p.*.&raw; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            58..61: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn addr_of_through_any_pointer_is_fine() {
    // Through a raw pointer, and through a SHARED borrow: the address
    // carries no write permission of its own. A write through it lands
    // on the borrow's frozen aliasing node, which the interpreter
    // detects.
    check_diagnostics(
        r#"
static f = fn::<@a>(b: struct { a: usize }.&::<@a>) -> usize.&raw { b.*.a.&raw };
static main = fn {
    let mut r = struct { a = 1 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw };
};
"#,
        expect![[r#"
            131..132: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_types_as_the_projected_pointee() {
    check_infer(
        r#"
static main = fn {
    let mut r = struct { a = 1, b = 2 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw };
};
"#,
        expect![[r#"
            15..117 'fn {     let mut ...': fn()
            18..117 '{     let mut r =...': ()
            32..33 'r': struct { a: {number}, b: {number} }
            36..59 'struct { a = 1, b...': struct { a: {number}, b: {number} }
            49..50 '1': {number}
            56..57 '2': {number}
            69..70 'p': struct { a: {number}, b: {number} }.&raw
            73..74 'r': struct { a: {number}, b: {number} }
            73..79 'r.&raw': struct { a: {number}, b: {number} }.&raw
            89..90 'q': {number}.&raw
            93..114 'unsafe { p.*.a.&r...': {number}.&raw
            100..114 '{ p.*.a.&raw }': {number}.&raw
            102..103 'p': struct { a: {number}, b: {number} }.&raw
            102..105 'p.*': struct { a: {number}, b: {number} }
            102..107 'p.*.a': {number}
            102..112 'p.*.a.&raw': {number}.&raw
        "#]],
    );
}

#[test]
fn deref_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw; p.* };",
        expect![[r#"
            61..64: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_inside_unsafe_is_clean() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw; unsafe { p.* } };",
        expect![[r#""#]],
    );
}

#[test]
fn deref_write_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw; p.* = 2; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            50..53: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_through_a_pointer_to_a_non_mut_local_is_accepted() {
    // The checker does not judge a raw write's permission (#183): the
    // write is UB, detected by the interpreter, which mints `x` read-only.
    check_diagnostics(
        "static main = fn { let x: usize = 1; let p = x.&raw; unsafe { p.* = 2; } };",
        expect![[r#""#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_works() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw; unsafe { p.*.a = 2; } };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_still_requires_unsafe() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw; p.*.a = 2; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
            65..68: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_needs_no_mut_binding_on_the_pointer() {
    // `p` itself is not `mut` — writing through it does not reassign it,
    // for projected targets exactly like for `p.* = v;`.
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw; unsafe { p.*.a = 2; }; let x = p; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn pointee_field_reads_work() {
    check_diagnostics(
        "static main = fn() -> usize { let mut r = struct { a = 1, b = 2 }; let p = r.&raw; unsafe { p.*.a + p.*.b } };",
        expect![[r#""#]],
    );
}

#[test]
fn raw_pointers_have_one_kind() {
    // `x.&raw` of a `mut` and a non-`mut` root are the same type.
    check_diagnostics(
        "static f = fn(p: usize.&raw) {};\nstatic main = fn { let mut x: usize = 1; let y: usize = 2; f(x.&raw); f(y.&raw); };",
        expect![""],
    );
}

#[test]
fn raw_pointer_pointee_must_match_exactly() {
    check_diagnostics(
        r#"static main = fn { let mut x = 1; let p: str.&raw = x.&raw; };"#,
        expect![[r#"
            52..58: type mismatch: expected `str.&raw`, found `{number}.&raw` (expected `str.&raw` because of this annotation at 41..49)
        "#]],
    );
}

#[test]
fn raw_pointer_does_not_coerce_to_pointee() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let y: usize = x.&raw; };",
        expect![[r#"
            49..55: type mismatch: expected `usize`, found `{number}.&raw` (expected `usize` because of this annotation at 41..46)
        "#]],
    );
}

#[test]
fn deref_of_a_non_pointer_errors() {
    check_diagnostics(
        "static main = fn { let x = 1; unsafe { x.*; } };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
            39..40: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn pointer_equality_is_legal_and_safe() {
    check_diagnostics(
        "static main = fn() -> bool { let mut x = 1; x.&raw == x.&raw };",
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}
