//! Assignment and mutability, with the make-mutable fixes.

use super::*;

#[test]
fn assignment_infers_value_against_the_bindings_type() {
    check_infer(
        "static f = fn { let mut x = 1; x = 2; x };",
        expect![[r#"
            11..41 'fn { let mut x = ...': fn() -> {number}
            14..41 '{ let mut x = 1; ...': {number}
            24..25 'x': {number}
            28..29 '1': {number}
            31..32 'x': {number}
            35..36 '2': {number}
            38..39 'x': {number}
        "#]],
    );
}

#[test]
fn assignment_value_must_match_the_bindings_type() {
    // No annotation on `x`, so the hint falls back to the binding's name:
    // that's where its inferred type became attached.
    check_diagnostics(
        r#"static f = fn { let mut x = 1; x = "no"; };"#,
        expect![[r#"
            35..39: type mismatch: expected `{number}`, found `str` (`x` was inferred to have type `{number}` from its initializer at 24..25)
        "#]],
    );
}

#[test]
fn assignment_mismatch_cites_the_bindings_annotation_when_present() {
    check_diagnostics(
        r#"static f = fn { let mut x: usize = 1; x = "no"; };"#,
        expect![[r#"
            42..46: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 27..32)
        "#]],
    );
}

#[test]
fn assignment_to_an_immutable_let_is_rejected() {
    check_diagnostics(
        "static f = fn { let x = 1; x = 2; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            27..28: cannot assign to `x`: it is not declared `mut` (`x` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn assignment_to_a_mut_param_is_allowed() {
    // Regression guard: the happy path must stay diagnostic-free.
    check_diagnostics(
        "static f = fn (mut n: usize) -> usize { n = n + 1; n };",
        expect![[""]],
    );
}

#[test]
fn assignment_to_an_immutable_param_is_rejected() {
    check_diagnostics(
        "static f = fn (n: usize) { n = 2; };",
        expect![[r#"
            27..28: cannot assign to `n`: it is not declared `mut` (`n` is declared without `mut` here at 15..16)
        "#]],
    );
}

#[test]
fn assignment_to_a_static_item_is_rejected() {
    check_diagnostics(
        "static x: usize = 1;\nstatic f = fn { x = 2; };",
        expect![[r#"
            37..38: cannot assign to `x`: `static` items cannot be reassigned (`x` is defined here at 7..8)
        "#]],
    );
}

#[test]
fn assignment_to_a_const_item_is_rejected() {
    check_diagnostics(
        "const x: usize = 1;\nstatic f = fn { x = 2; };",
        expect![[r#"
            36..37: cannot assign to `x`: a `const` is copied into each use, so there is no single place to assign to (`x` is defined here at 6..7)
        "#]],
    );
}

#[test]
fn assignment_to_a_builtin_is_rejected() {
    // The value side reads `print` (same type), so the target diagnostic
    // is the only one.
    check_diagnostics(
        "static f = fn { print = print; };",
        expect![[r#"
            16..21: cannot assign to `print`: it is a builtin function
        "#]],
    );
}

#[test]
fn mutation_is_allowed_in_const_contexts() {
    // No const-check rule rejects local mutation; only calls are restricted.
    check_diagnostics(
        "static x = const { let mut n = 1; n = n + 1; n };",
        expect![[r#"
            31..32: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn assign_to_immutable_let_offers_a_make_mutable_fix() {
    let text = "static f = fn { let x: usize = 1; x = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `x` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    assert!(fix.edits[0].range.is_empty());
    // Immediately before the binding's name `x` in `let x` (offset 20):
    // applying it yields `let mut x = 1;`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 20);
}

#[test]
fn assign_to_immutable_param_offers_a_make_mutable_fix() {
    let text = "static f = fn (n: usize) { n = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `n` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the parameter's name `n` (offset 15): applying it
    // yields `fn (mut n: usize)` — params take `mut` before the name.
    assert_eq!(u32::from(fix.edits[0].range.start()), 15);
}

#[test]
fn variant_payload_binding_assign_offers_a_make_mutable_fix() {
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => { r = 2; r },
        ::Point => 0,
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diag = diagnostics
        .iter()
        .find(|d| d.message.starts_with("cannot assign to `r`"))
        .unwrap_or_else(|| panic!("no assign-to-immutable diagnostic in {diagnostics:?}"));
    let fix = diag.fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `r` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the payload's name: `::Circle(mut r)`.
    let r = text.find("Circle(r)").unwrap() + "Circle(".len();
    assert_eq!(usize::from(fix.edits[0].range.start()), r);
}

#[test]
fn match_arm_bind_assign_offers_a_make_mutable_fix() {
    let text = r#"
static f = fn (x: usize) -> usize {
    match x {
        n => { n = 2; n },
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diag = diagnostics
        .iter()
        .find(|d| d.message.starts_with("cannot assign to `n`"))
        .unwrap_or_else(|| panic!("no assign-to-immutable diagnostic in {diagnostics:?}"));
    let fix = diag.fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `n` mutable");
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the arm's name: `mut n => ...`.
    let n = text.find("n =>").unwrap();
    assert_eq!(usize::from(fix.edits[0].range.start()), n);
}

#[test]
fn mut_match_bindings_are_assignable() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(mut r) => { r = r + 1; r },
        mut other => { other = Shape::Point; 0 },
    }
};
"#,
        expect![[""]],
    );
}

#[test]
fn mut_on_a_match_hole_is_refused() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(mut _) => 1,
        mut _ => 0,
    }
};
"#,
        expect![[r#"
            112..117: `mut` has no effect on `_`: a hole can never be assigned
            133..138: `mut` has no effect on `_`: a hole can never be assigned
        "#]],
    );
}

#[test]
fn newtype_nested_binding_assign_offers_a_make_mutable_fix() {
    let text = r#"
type Foo = struct { n: usize };
static f = fn (v: Foo) -> usize {
    let Foo(inner) = v;
    inner = 2;
    inner
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diag = diagnostics
        .iter()
        .find(|d| d.message.starts_with("cannot assign to `inner`"))
        .unwrap_or_else(|| panic!("no assign-to-immutable diagnostic in {diagnostics:?}"));
    let fix = diag.fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `inner` mutable");
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the inner name: `Foo(mut inner)`.
    let inner = text.find("Foo(inner)").unwrap() + "Foo(".len();
    assert_eq!(usize::from(fix.edits[0].range.start()), inner);
}
