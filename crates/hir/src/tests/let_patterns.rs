//! Let and parameter patterns, unit, and deferred `let`.

use super::*;

#[test]
fn a_let_array_pattern_binds_each_element() {
    check_diagnostics(
        r#"
static f = fn () -> usize {
    let [a, b, c] = [0; 3];
    let [[p, mut q], _]: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    q = p;
    a + b + c + q
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_refutable_let_pattern_is_refused_once() {
    check_diagnostics(
        r#"
static f = fn () -> () {
    let s = "abc";
    let ::Char(c, _) = s.next_char(0);
};
static g = fn (n: usize) -> () {
    let 0 = n;
};
"#,
        expect![[r#"
            53..65: this pattern can fail to match; use `match`
            128..129: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_variant_let_pattern_must_cover_its_type() {
    check_diagnostics(
        r#"
type Only = enum { One(usize, usize) };
type Shape = enum { Circle(usize), Point };
static f = fn (o: Only) -> usize {
    let ::One(x, mut y) = o;
    y = y + 1;
    let Only::One(z, _) = o;
    x + y + z
};
static g = fn (s: Shape) -> usize {
    let ::Circle(r) = s;
    r
};
"#,
        expect![[r#"
            254..265: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_let_pattern_naming_one_of_several_variants_is_refused() {
    // A variant of a multi-variant enum may not be what the value holds:
    // one error, owned or through a borrow.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Label(str) };
static f = fn (c: Shape) -> str {
    let ::Label(s) = c;
    s
};
static g = fn (c: Shape) -> str {
    let ::Label(s) = c.&;
    s.*
};
"#,
        expect![[r#"
            92..102: this pattern can fail to match; use `match`
            159..169: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_qualified_variant_let_pattern_on_a_diverging_or_broken_value_adds_no_pattern_error() {
    // The value never arrives, so the pattern can't fail to match.
    check_diagnostics(
        r#"
type Only = enum { One(usize) };
static f = fn () -> usize {
    let Only::One(a) = return 0;
    a
};
static g = fn () -> usize {
    let Only::One(a) = nope;
    a
};
"#,
        expect![[r#"
            155..159: unresolved name `nope`
        "#]],
    );
}

#[test]
fn a_qualified_variant_let_pattern_binds_through_a_borrow() {
    check_diagnostics(
        r#"
type Only = enum { One(usize, usize) };
static f = fn (o: Only) -> usize {
    let Only::One(a, b) = o.&;
    a.* + b.*
};
"#,
        expect![[""]],
    );
}

#[test]
fn a_qualified_let_pattern_naming_one_of_several_variants_is_refused() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Label(str) };
static f = fn (c: Shape) -> str {
    let Shape::Label(s) = c;
    s
};
"#,
        expect![[r#"
            92..107: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn an_array_pattern_must_match_the_array_length_and_kind() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    let [a, b] = [1, 2, 3];
    let [c] = n;
    a + b + c
};
"#,
        expect![[r#"
            45..51: this pattern has 2 elements, but the array has length 3
            73..76: this pattern only matches an array; found `usize`
        "#]],
    );
}

#[test]
fn array_and_variant_parameter_patterns_bind() {
    check_diagnostics(
        r#"
type Only = enum { One(usize, usize) };
static f = fn ([a, mut b]: [usize; 2], ::One(c, d): Only) -> usize {
    b = b + 1;
    a + b + c + d
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn mut_on_hole_offers_a_remove_mut_fix() {
    // Validation's "`mut` has no effect on `_`" error carries a fix that
    // deletes the `mut` keyword and the whitespace up to the hole.
    let text = "static f = fn { let mut _: usize = 1; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Remove `mut`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "");
    // Deletes `mut ` (offsets 20..24), leaving `let _ = 1;`.
    assert_eq!(&text[fix.edits[0].range], "mut ");
}

#[test]
fn an_immutable_deferred_let_offers_an_insert_mut_fix() {
    let text = "static f = fn { let x: usize; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `x` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    assert_eq!(
        usize::from(fix.edits[0].range.start()),
        text.find('x').unwrap()
    );
}

#[test]
fn assignment_to_an_unresolved_name_does_not_panic() {
    // The target is a plain variable syntactically (so validation has
    // nothing to say), but it doesn't resolve to any binding — reported
    // exactly like an unresolved name anywhere else, and lowering stays
    // total downstream (see `mir`'s equivalent capture-write coverage).
    check_diagnostics(
        "static f = fn { y = 2; };",
        expect![[r#"
            16..17: unresolved name `y`
        "#]],
    );
}

#[test]
fn unit_value_has_the_unit_type() {
    check_infer(
        "static f = fn (u: ()) -> bool { let v: () = (); u == () };",
        expect![[r#"
            11..57 'fn (u: ()) -> boo...': fn(()) -> bool
            15..16 'u': ()
            30..57 '{ let v: () = ();...': bool
            36..37 'v': ()
            44..46 '()': ()
            48..49 'u': ()
            48..55 'u == ()': bool
            53..55 '()': ()
        "#]],
    );
}

// ---- deferred initialization (`let mut x;`) ---------------------------------

#[test]
fn a_deferred_binding_takes_its_type_from_the_annotation_or_the_assignment() {
    check_infer(
        r#"
static main = fn () -> usize {
    let mut x: usize;
    let mut y;
    y = 'c';
    x = 1;
    x
};
"#,
        expect![[r#"
            15..100 'fn () -> usize { ...': fn() -> usize
            30..100 '{     let mut x: ...': usize
            44..45 'x': usize
            66..67 'y': char
            73..74 'y': char
            77..80 ''c'': char
            86..87 'x': usize
            90..91 '1': usize
            97..98 'x': usize
        "#]],
    );
}

#[test]
fn a_hole_let_without_an_initializer_is_accepted() {
    check_diagnostics(
        r#"
static main = fn () -> usize {
    let _;
    0
};
"#,
        expect![""],
    );
}

#[test]
fn a_destructuring_let_without_an_initializer_is_refused() {
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> () {
    let struct { x, y };
    let Point(struct { x as a, y as b }): Point;
};
"#,
        expect![[r#"
            81..96: a destructuring `let` needs a value: only a single name can be declared without one
            106..138: a destructuring `let` needs a value: only a single name can be declared without one
        "#]],
    );
}

#[test]
fn an_unannotated_deferred_binding_takes_its_assignments_type() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static assigned = fn () -> Shape {
    let mut s;
    s = Shape::Circle(1);
    s = ::Point;
    s
};
static after_never = fn (c: bool) -> Shape {
    let mut s;
    if c { s = panic("no"); } else { s = Shape::Point; }
    s = Shape::Circle(1);
    s
};
static after_error = fn () -> Shape {
    let mut s;
    s = nope;
    s = Shape::Circle(1);
    s = ::Point;
    s
};
"#,
        expect![[r#"
            360..364: unresolved name `nope`
        "#]],
    );
}
