//! Raw pointers: address-of, deref and pointer unification.

use super::*;

#[test]
fn raw_pointer_types_infer_and_display() {
    check_infer(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    let q = x.&raw;
    unsafe { p.* }
};
"#,
        expect![[r#"
            15..114 'fn() -> usize {  ...': fn() -> usize
            29..114 '{     let mut x =...': usize
            43..44 'x': usize
            47..48 '1': usize
            58..59 'p': usize.&raw mut
            62..63 'x': usize
            62..72 'x.&raw mut': usize.&raw mut
            82..83 'q': usize.&raw
            86..87 'x': usize
            86..92 'x.&raw': usize.&raw
            98..112 'unsafe { p.* }': usize
            105..112 '{ p.* }': usize
            107..108 'p': usize.&raw mut
            107..110 'p.*': usize
        "#]],
    );
}

#[test]
fn addr_of_mut_requires_a_mut_root() {
    check_diagnostics(
        "static main = fn { let x = 1; let p = x.&raw mut; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: cannot take `.&raw mut` of `x`: it is not declared `mut` (`x` is declared without `mut` here at 23..24)
        "#]],
    );
}

#[test]
fn addr_of_mut_immutable_offers_the_make_mut_fix() {
    // Same machinery as `assign_to_immutable_let_offers_a_make_mutable_fix`:
    // `.&raw mut` of a non-`mut` root anchors the insert-`mut` fix at the
    // binding's declaration, not the address-of.
    let text = "static f = fn { let x: usize = 1; let p = x.&raw mut; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `x` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the binding's name `x` in `let x` (offset 20):
    // applying it yields `let mut x = 1;`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 20);
}

#[test]
fn addr_of_mut_of_a_field_blames_the_root() {
    check_diagnostics(
        "static main = fn { let r = struct { a = 1 }; let p = r.a.&raw mut; };",
        expect![[r#"
            40..41: cannot infer the type of this number: it has no defining use — add a type annotation
            53..54: cannot take `.&raw mut` of `r.a`: `r` is not declared `mut` (`r` is declared without `mut` here at 23..24)
        "#]],
    );
}

#[test]
fn addr_of_shared_needs_no_mut() {
    check_diagnostics(
        "static main = fn { let x = 1; let p = x.&raw; };",
        expect![[r#"
            27..28: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_mut_of_a_static_is_reserved() {
    check_diagnostics(
        "static s = 7;\nstatic main = fn { let p = s.&raw mut; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            41..42: not enough information to infer the type of `s`; add a type annotation to its definition (defined here at 7..8)
            41..42: cannot take `.&raw mut` of `s`: `static mut` is not supported yet (`s` is defined here at 7..8)
        "#]],
    );
}

#[test]
fn addr_of_shared_of_items_is_fine() {
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
        "static main = fn { let mut x = 1; let p = x.&raw mut; let q = unsafe { p.*.&raw mut }; };",
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
        "static main = fn { let mut x = 1; let p = x.&raw mut; let q = p.*.&raw mut; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            62..65: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn addr_of_mut_through_a_shared_pointer_errors() {
    check_diagnostics(
        r#"
static main = fn {
    let mut r = struct { a = 1 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw mut };
};
"#,
        expect![[r#"
            49..50: cannot infer the type of this number: it has no defining use — add a type annotation
            95..109: cannot take `.&raw mut` through `struct { a: {number} }.&raw`: minting a mutating address needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn addr_of_shared_through_any_pointer_is_fine() {
    check_diagnostics(
        r#"
static main = fn {
    let mut r = struct { a = 1 };
    let p = r.&raw;
    let q = unsafe { p.*.a.&raw };
};
"#,
        expect![[r#"
            49..50: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_types_as_the_projected_pointee() {
    check_infer(
        r#"
static main = fn {
    let mut r = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    let q = unsafe { p.*.a.&raw mut };
};
"#,
        expect![[r#"
            15..125 'fn {     let mut ...': fn()
            18..125 '{     let mut r =...': ()
            32..33 'r': struct { a: {number}, b: {number} }
            36..59 'struct { a = 1, b...': struct { a: {number}, b: {number} }
            49..50 '1': {number}
            56..57 '2': {number}
            69..70 'p': struct { a: {number}, b: {number} }.&raw mut
            73..74 'r': struct { a: {number}, b: {number} }
            73..83 'r.&raw mut': struct { a: {number}, b: {number} }.&raw mut
            93..94 'q': {number}.&raw mut
            97..122 'unsafe { p.*.a.&r...': {number}.&raw mut
            104..122 '{ p.*.a.&raw mut }': {number}.&raw mut
            106..107 'p': struct { a: {number}, b: {number} }.&raw mut
            106..109 'p.*': struct { a: {number}, b: {number} }
            106..111 'p.*.a': {number}
            106..120 'p.*.a.&raw mut': {number}.&raw mut
        "#]],
    );
}

#[test]
fn deref_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; p.* };",
        expect![[r#"
            65..68: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_inside_unsafe_is_clean() {
    check_diagnostics(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; unsafe { p.* } };",
        expect![[r#""#]],
    );
}

#[test]
fn deref_write_outside_unsafe_errors() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw mut; p.* = 2; };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            54..57: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_through_a_shared_pointer_errors() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let p = x.&raw; unsafe { p.* = 2; } };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
            59..62: cannot assign through `{number}.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_works() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; unsafe { p.*.a = 2; } };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_through_a_shared_pointer_errors() {
    // A shared step anywhere in the chain refuses the write: `p` is
    // shared, so the field write is refused at the pointer's flavor —
    // same message as `p.* = v;`.
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw; unsafe { p.*.a = 2; } };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
            74..77: cannot assign through `struct { a: {number} }.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn deref_write_into_a_pointee_field_still_requires_unsafe() {
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; p.*.a = 2; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
            69..72: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn deref_write_needs_no_mut_binding_on_the_pointer() {
    // `p` itself is not `mut` — writing through it does not reassign it,
    // for projected targets exactly like for `p.* = v;`.
    check_diagnostics(
        "static main = fn { let mut r = struct { a = 1 }; let p = r.&raw mut; unsafe { p.*.a = 2; }; let x = p; };",
        expect![[r#"
            44..45: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn pointee_field_reads_work() {
    check_diagnostics(
        "static main = fn() -> usize { let mut r = struct { a = 1, b = 2 }; let p = r.&raw mut; unsafe { p.*.a + p.*.b } };",
        expect![[r#""#]],
    );
}

#[test]
fn raw_pointer_unification_is_exact_no_mut_mixing() {
    check_diagnostics(
        "static f = fn(p: usize.&raw) {};\nstatic main = fn { let mut x = 1; f(x.&raw mut); };",
        expect![[r#"
            69..79: type mismatch: expected `usize.&raw`, found `{number}.&raw mut`
        "#]],
    );
}

#[test]
fn raw_pointer_unification_is_exact_no_shared_to_mut() {
    check_diagnostics(
        "static f = fn(p: usize.&raw mut) {};\nstatic main = fn { let x = 1; f(x.&raw); };",
        expect![[r#"
            69..75: type mismatch: expected `usize.&raw mut`, found `{number}.&raw`
        "#]],
    );
}

#[test]
fn raw_pointer_pointee_must_match_exactly() {
    check_diagnostics(
        r#"static main = fn { let mut x = 1; let p: str.&raw mut = x.&raw mut; };"#,
        expect![[r#"
            56..66: type mismatch: expected `str.&raw mut`, found `{number}.&raw mut` (expected `str.&raw mut` because of this annotation at 41..53)
        "#]],
    );
}

#[test]
fn raw_pointer_does_not_coerce_to_pointee() {
    check_diagnostics(
        "static main = fn { let mut x = 1; let y: usize = x.&raw mut; };",
        expect![[r#"
            49..59: type mismatch: expected `usize`, found `{number}.&raw mut` (expected `usize` because of this annotation at 41..46)
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
        "static main = fn() -> bool { let mut x = 1; x.&raw mut == x.&raw mut };",
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}
