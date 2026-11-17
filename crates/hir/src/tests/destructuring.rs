//! Record and newtype destructuring, and field assignment.

use super::*;

// ---- record destructuring, `pub` reservation, field assignment ----

#[test]
fn let_record_destructure_binds_correct_types() {
    check_infer(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = "s" }; };"#,
        expect![[r#"
            11..66 'fn { let struct {...': fn()
            14..66 '{ let struct { x,...': ()
            29..30 'x': {number}
            32..33 'y': str
            38..63 'struct { x = 1, y...': struct { x: {number}, y: str }
            51..52 '1': {number}
            58..61 '"s"': str
        "#]],
    );
}

#[test]
fn let_record_destructure_rename_binds_only_the_new_name() {
    // `x` is not bound under its own name; only the rename `a` is.
    check_diagnostics(
        r#"static f = fn { let struct { x as a } = struct { x = 1 }; let b = a; let c = x; };"#,
        expect![[r#"
            53..54: cannot infer the type of this number: it has no defining use — add a type annotation
            77..78: unresolved name `x`
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_field_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            20..32: pattern does not mention field `y`; add `..` to ignore it
            48..49: cannot infer the type of this number: it has no defining use — add a type annotation
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn a_record_pattern_missing_fields_offers_the_rest_marker() {
    let fixed = apply_fix(
        "static f = fn (p: struct { x: usize, y: usize }) -> usize { let struct { x } = p; x };",
        "Add `..`",
    );
    assert!(
        fixed.contains("let struct { x, .. } = p;"),
        "fixed: {fixed}"
    );
    check_diagnostics(&fixed, expect![[r#""#]]);
    // After a trailing comma the marker goes behind it; an empty pattern
    // gets the marker alone.
    let fixed = apply_fix(
        "static f = fn (p: struct { x: usize, y: usize }) -> usize { let struct { x, } = p; x };",
        "Add `..`",
    );
    assert!(
        fixed.contains("let struct { x, .. } = p;"),
        "fixed: {fixed}"
    );
    check_diagnostics(&fixed, expect![[r#""#]]);
    let fixed = apply_fix(
        "static f = fn (p: struct { x: usize }) -> () { let struct {} = p; };",
        "Add `..`",
    );
    assert!(fixed.contains("let struct { .. } = p;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_record_pattern_missing_a_field_that_must_be_consumed_binds_it() {
    // `..` may not skip a field that must be consumed: bind it.
    let source = r#"
static f = fn::<T>(p: struct { t: T, x: usize, y: usize }) -> usize {
    let struct { x } = p;
    x
};
"#;
    check_diagnostics(
        source,
        expect![[r#"
            79..91: pattern does not mention fields `t`, `y`; bind `t` and add `..` to ignore the rest: `T` may be a type that must be consumed, or write `T: forget` to require one that can be discarded
        "#]],
    );
    let fixed = apply_fix(source, "Bind `t` and add `..`");
    assert!(
        fixed.contains("let struct { x, t, .. } = p;"),
        "fixed: {fixed}"
    );
    check_diagnostics(&fixed, expect![[r#""#]]);
    // Every unmentioned field must be consumed: bound, with no `..`.
    let source = r#"
static f = fn::<T>(p: struct { t: T, x: usize }) -> usize {
    let struct { x } = p;
    x
};
"#;
    check_diagnostics(
        source,
        expect![[r#"
            69..81: pattern does not mention field `t`; bind `t`: `T` may be a type that must be consumed, or write `T: forget` to require one that can be discarded
        "#]],
    );
    let fixed = apply_fix(source, "Bind `t`");
    assert!(fixed.contains("let struct { x, t } = p;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // Fields declared `only move` must be consumed, said once for both.
    let source = r#"
type Lin = struct { n: usize } only move;
static f = fn(p: struct { a: Lin, b: Lin, x: usize, y: usize }) -> usize {
    let struct { x } = p;
    x
};
"#;
    check_diagnostics(
        source,
        expect![[r#"
            126..138: pattern does not mention fields `a`, `b`, `y`; bind `a`, `b` and add `..` to ignore the rest: `a`, `b` must be consumed
        "#]],
    );
    let fixed = apply_fix(source, "Bind `a`, `b` and add `..`");
    assert!(
        fixed.contains("let struct { x, a, b, .. } = p;"),
        "fixed: {fixed}"
    );
    // The fixed source is clean: binding the fields is all the pattern owes.
    check_diagnostics(&fixed, expect![[r#""#]]);
    // Each parameter's reason once, in field order.
    check_diagnostics(
        r#"
static f = fn::<T, U>(p: struct { a: T, b: U, c: T, x: usize }) -> usize {
    let struct { x } = p;
    x
};
"#,
        expect![[r#"
            84..96: pattern does not mention fields `a`, `b`, `c`; bind `a`, `b`, `c`: `T` may be a type that must be consumed, or write `T: forget` to require one that can be discarded; `U` may be a type that must be consumed, or write `U: forget` to require one that can be discarded
        "#]],
    );
}

#[test]
fn let_record_destructure_missing_several_fields_without_rest_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x } = struct { x = 1, y = 2, z = 3 }; };"#,
        expect![[r#"
            20..32: pattern does not mention fields `y`, `z`; add `..` to ignore them
            48..49: cannot infer the type of this number: it has no defining use — add a type annotation
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
            62..63: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_record_destructure_with_rest_ignores_missing_fields() {
    check_diagnostics(
        r#"static f = fn { let struct { x, .. } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            52..53: cannot infer the type of this number: it has no defining use — add a type annotation
            59..60: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_record_destructure_unknown_field_errors() {
    check_diagnostics(
        r#"static f = fn { let struct { x, z } = struct { x = 1, y = 2 }; };"#,
        expect![[r#"
            20..35: no field `z` on `struct { x: {number}, y: {number} }`
            20..35: pattern does not mention field `y`; add `..` to ignore it
            51..52: cannot infer the type of this number: it has no defining use — add a type annotation
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn let_bare_record_pattern_needs_annotation_without_one() {
    // No annotation and no newtype wrapper: nothing pins the initializer's
    // type down before the pattern must be checked against it.
    check_diagnostics(
        r#"static f = fn (mk: fn() -> struct { x: usize }) { let struct { x } = mk(); };"#,
        expect![[r#""#]],
    );
    // `x` is a free parameter of `f` itself (no call site anywhere pins its
    // type — unlike a group-inferred callee, nothing forces it concrete),
    // so it is still genuinely undetermined when the pattern is checked.
    check_diagnostics(
        r#"static f = fn (x) { let struct { y } = x; };"#,
        expect![[r#"
            24..36: cannot destructure this pattern: its type is not known here; add a type annotation
        "#]],
    );
}

#[test]
fn param_record_destructure_binds_correct_types() {
    check_infer(
        r#"static f = fn (struct { x, y }: struct { x: usize, y: str }) { x }; "#,
        expect![[r#"
            11..66 'fn (struct { x, y...': fn(struct { x: usize, y: str }) -> usize
            24..25 'x': usize
            27..28 'y': str
            61..66 '{ x }': usize
            63..64 'x': usize
        "#]],
    );
}

#[test]
fn param_bare_record_destructure_needs_annotation() {
    check_diagnostics(
        r#"static f = fn (struct { x }) { x };"#,
        expect![[r#"
            15..27: cannot destructure this pattern: its type is not known here; add a type annotation
        "#]],
    );
}

#[test]
fn let_newtype_destructure_binds_underlying_fields() {
    check_infer(
        r#"
type Foo = struct { x: usize, y: str };
static f = fn (v: Foo) { let Foo(struct { x, y }) = v; x };
"#,
        expect![[r#"
            52..99 'fn (v: Foo) { let...': fn(Foo) -> usize
            56..57 'v': Foo
            64..99 '{ let Foo(struct ...': usize
            83..84 'x': usize
            86..87 'y': str
            93..94 'v': Foo
            96..97 'x': usize
        "#]],
    );
}

#[test]
fn param_newtype_destructure_infers_param_type_without_annotation() {
    // `Foo(...)` names its own type — no annotation needed, mirroring
    // construction's callee.
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn (Foo(struct { x })) { x };
"#,
        expect![[r#"
            44..72 'fn (Foo(struct { ...': fn(Foo) -> usize
            61..62 'x': usize
            67..72 '{ x }': usize
            69..70 'x': usize
        "#]],
    );
}

#[test]
fn newtype_destructure_wrong_named_type_errors() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
type Bar = struct { x: usize };
static f = fn (v: Bar) { let Foo(struct { x }) = v; };
"#,
        expect![[r#"
            114..115: type mismatch: expected `Foo`, found `Bar`
        "#]],
    );
}

#[test]
fn newtype_destructure_unknown_type_errors() {
    check_diagnostics(
        r#"static f = fn { let Bogus(struct { x }) = 1; };"#,
        expect![[r#"
            20..39: `Bogus` does not name a type
            42..43: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_destructure_unknown_field_still_types_it_as_error_not_cascading() {
    // The unknown field's binding recovers as `{error}` (infectious and
    // silent) rather than blocking the rest of the pattern from checking.
    check_diagnostics(
        r#"static f = fn { let struct { x, z, .. } = struct { x = 1 }; let n: usize = z; };"#,
        expect![[r#"
            20..39: no field `z` on `struct { x: {number} }`
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn per_binding_mut_in_record_pattern_allows_assignment() {
    check_diagnostics(
        r#"static f = fn { let struct { mut x, y } = struct { x = 1, y = 2 }; x = 3; };"#,
        expect![[r#"
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
            62..63: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_pattern_binding_without_mut_is_immutable() {
    check_diagnostics(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = 2 }; x = 3; };"#,
        expect![[r#"
            51..52: cannot infer the type of this number: it has no defining use — add a type annotation
            58..59: cannot infer the type of this number: it has no defining use — add a type annotation
            63..64: cannot assign to `x`: it is not declared `mut` (`x` is declared without `mut` here at 29..30)
        "#]],
    );
}

#[test]
fn record_pattern_field_offers_the_make_mut_fix() {
    // A record-pattern field carries its own `mut` (`struct { mut x }`),
    // so `ast::Name::mut_slot` puts the fix right before the field name.
    let text = "static f = fn { let struct { x } = struct { x = 1 }; x = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diag = diagnostics
        .iter()
        .find(|d| d.message.starts_with("cannot assign to `x`"))
        .unwrap_or_else(|| panic!("no assign-to-immutable diagnostic in {diagnostics:?}"));
    let fix = diag.fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `x` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the field's own name `x` in `struct { x }`
    // (offset 29): applying it yields `struct { mut x }`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 29);
}

#[test]
fn renamed_record_pattern_field_puts_mut_before_the_field_name() {
    // `record_pat_field` is `mut? field (as rename)?`: the one `mut` goes
    // before the FIELD name, not the bound rename, so the fix for `a`
    // inserts before `x` — `struct { x as mut a }` would not parse.
    let text = "static f = fn { let struct { x as a } = struct { x = 1 }; a = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diag = diagnostics
        .iter()
        .find(|d| d.message.starts_with("cannot assign to `a`"))
        .unwrap_or_else(|| panic!("no assign-to-immutable diagnostic in {diagnostics:?}"));
    let fix = diag.fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `a` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Before `x` in `struct { x as a }` (offset 29): applying it yields
    // `struct { mut x as a }`. The related note still points at `a`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 29);
    assert_eq!(diag.related.len(), 1, "related: {:?}", diag.related);
    assert_eq!(u32::from(diag.related[0].range.start()), 34);
}

#[test]
fn let_mut_on_a_destructuring_pattern_is_a_syntax_error() {
    check_diagnostics(
        r#"static f = fn { let mut struct { x } = struct { x = 1 }; };"#,
        expect![[r#"
            20..36: `mut` applies to individual bindings in a destructuring pattern
            52..53: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn field_assign_on_a_mut_root_is_clean() {
    // Mutability is transitive from the binding: a `mut` root makes every
    // field of it assignable — no per-field `mut` exists.
    check_diagnostics(
        r#"static f = fn (mut p: struct { x: usize }) -> usize { p.x = 1; p.x };"#,
        expect![[r#""#]],
    );
}

#[test]
fn nested_field_assign_on_a_mut_root_is_clean() {
    check_diagnostics(
        r#"static f = fn { let mut p = struct { a = struct { b = 1 } }; p.a.b = 2; };"#,
        expect![[r#"
            54..55: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn field_assign_on_an_immutable_root_blames_the_root() {
    // The squiggle sits on the root name inside the place — the fix
    // (adding `mut`) belongs to the binding, not the field — and the
    // message spells both the place and the root out.
    check_diagnostics(
        r#"static f = fn (p: struct { x: usize }) { p.x = 1; };"#,
        expect![[r#"
            41..42: cannot assign to `p.x`: `p` is not declared `mut` (`p` is declared without `mut` here at 15..16)
        "#]],
    );
}

#[test]
fn field_assign_to_immutable_root_offers_the_make_mut_fix() {
    // Same machinery as a plain assignment to an immutable binding: the
    // insert-`mut` fix anchors at the binding's declaration.
    let text = "static f = fn { let p: struct { x: usize } = struct { x = 1 }; p.x = 2; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "cannot assign to `p.x`: `p` is not declared `mut`"
    );
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `p` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Right before `p`'s declaration (offset 20 is the `p` in `let p`).
    assert_eq!(u32::from(fix.edits[0].range.start()), 20);
}

#[test]
fn field_assign_through_a_named_type_is_clean() {
    // A named type projects through its declared record for writes the
    // same as for reads.
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (mut p: Point) -> usize { p.x = 3; p.x + p.y };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn unknown_field_in_an_assign_target_reports_no_such_field() {
    // The target lowers as an ordinary field read for typing, so the
    // existing `NoSuchField` fires at the target position — no
    // assignment-specific wording needed.
    check_diagnostics(
        r#"static f = fn { let mut p = struct { x = 1 }; p.y = 2; };"#,
        expect![[r#"
            41..42: cannot infer the type of this number: it has no defining use — add a type annotation
            48..49: no field `y` on `struct { x: {number} }`
        "#]],
    );
}

#[test]
fn field_assign_through_a_non_record_reports_no_such_field() {
    check_diagnostics(
        r#"static f = fn { let mut n: usize = 1; n.x = 2; };"#,
        expect![[r#"
            40..41: no field `x` on `usize`
        "#]],
    );
}

#[test]
fn field_assign_rhs_mismatch_blames_the_annotated_root() {
    // The RHS is checked against the *field's* type; the root binding's
    // annotation is the axiom cited (its record type spells the field's
    // type out — the same flow record-literal field checking uses).
    check_diagnostics(
        r#"static f = fn (mut p: struct { x: usize }) { p.x = "one"; };"#,
        expect![[r#"
            51..56: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 22..41)
        "#]],
    );
}

#[test]
fn field_assign_in_a_const_fn_is_clean() {
    // Field assignment is as legal in a const context as local assignment:
    // mutating a private local record is not an observable effect.
    check_diagnostics(
        r#"
static bump = const fn (mut p: struct { x: usize }) -> usize { p.x = p.x + 1; p.x };
static two: usize = bump(struct { x = 1 });
"#,
        expect![[r#""#]],
    );
}

#[test]
fn pub_field_on_type_decl_is_reserved() {
    check_diagnostics(
        r#"type Foo = struct { pub x: usize };"#,
        expect![[r#"
            20..23: field visibility is not supported yet
        "#]],
    );
}
