//! Enums and variants: construction, paths and joins.

use super::*;

// ---- enums and variants ----

#[test]
fn variant_construction_has_the_enum_type() {
    // A constructed value has the enum's type, `Shape`; a payload-less
    // variant path IS the value, and a payload-carrying one is the
    // constructor `fn(payload) -> Shape`.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static c = Shape::Circle(3);
static p = Shape::Point;
"#,
        expect![[r#"
            56..69 'Shape::Circle': fn(usize) -> Shape
            56..72 'Shape::Circle(3)': Shape
            70..71 '3': usize
            85..97 'Shape::Point': Shape
        "#]],
    );
}

#[test]
fn variant_constructor_is_a_first_class_function() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let make = Shape::Circle;
    make(3)
};
"#,
        expect![[r#"
            56..104 'fn {     let make...': fn() -> Shape
            59..104 '{     let make = ...': Shape
            69..73 'make': fn(usize) -> Shape
            76..89 'Shape::Circle': fn(usize) -> Shape
            95..99 'make': fn(usize) -> Shape
            95..102 'make(3)': Shape
            100..101 '3': usize
        "#]],
    );
}

#[test]
fn mixed_variant_join_has_the_enum_type() {
    // Both branches construct a variant of `Shape`, so both are `Shape`
    // and the join is plain agreement.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool) {
    let s = if a { Shape::Circle(1) } else { Shape::Point };
    s
};
"#,
        expect![[r#"
            56..139 'fn (a: bool) {   ...': fn(bool) -> Shape
            60..61 'a': bool
            69..139 '{     let s = if ...': Shape
            79..80 's': Shape
            83..130 'if a { Shape::Cir...': Shape
            86..87 'a': bool
            88..108 '{ Shape::Circle(1) }': Shape
            90..103 'Shape::Circle': fn(usize) -> Shape
            90..106 'Shape::Circle(1)': Shape
            104..105 '1': usize
            114..130 '{ Shape::Point }': Shape
            116..128 'Shape::Point': Shape
            136..137 's': Shape
        "#]],
    );
}

#[test]
fn cross_enum_join_is_still_incompatible() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
type Color = enum { Red };
static f = fn (a: bool) {
    let s = if a { Shape::Circle(1) } else { Color::Red };
    s
};
"#,
        expect![[r#"
            136..146: `if` branches have incompatible types: `Shape` vs `Color`; add a type annotation to decide between them (this branch has type `Shape` at 110..126)
        "#]],
    );
}

#[test]
fn enum_annotation_accepts_any_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = Shape::Circle(3);
static p: Shape = Shape::Point;
static f = fn (a: bool) -> Shape {
    if a { Shape::Circle(1) } else { Shape::Point }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_variant_is_not_an_annotation_type() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Circle = Shape::Point;
"#,
        expect![[r#"
            55..68: `Shape::Circle` is a variant, not a type; use `Shape`
        "#]],
    );
}

#[test]
fn a_variant_is_not_a_parameter_type() {
    // A variant is a value of its enum's type, so an API cannot demand
    // one specific state through a parameter type.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static wants_circle = fn (s: Shape::Circle) {};
"#,
        expect![[r#"
            74..87: `Shape::Circle` is a variant, not a type; use `Shape`
        "#]],
    );
}

#[test]
fn let_and_let_mut_of_a_variant_have_the_enum_type() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let p = Shape::Point;
    let mut s = Shape::Point;
    s = Shape::Circle(1);
};
"#,
        expect![[r#"
            56..144 'fn {     let p = ...': fn()
            59..144 '{     let p = Sha...': ()
            69..70 'p': Shape
            73..85 'Shape::Point': Shape
            99..100 's': Shape
            103..115 'Shape::Point': Shape
            121..122 's': Shape
            125..138 'Shape::Circle': fn(usize) -> Shape
            125..141 'Shape::Circle(1)': Shape
            139..140 '1': usize
        "#]],
    );
}

#[test]
fn unknown_variant_is_reported_with_the_declaration() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Missing;
"#,
        expect![[r#"
            63..70: `Shape` has no variant `Missing` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn variant_path_on_a_struct_type_item() {
    // `x` IS declared — as a field, one namespace over — so the answer
    // names the namespace and the escape rather than the generic
    // no-variants line. See
    // `qualified_path_naming_a_field_says_so_and_names_the_escape` for the
    // names that genuinely aren't there, which keep it.
    check_diagnostics(
        r#"
type Point = struct { x: usize };
static p = Point::x;
"#,
        expect![[r#"
            46..54: `x` is a field of `Point`, not a member — fields are reached through a value: `value.x` (`Point` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn variant_path_on_a_value_base() {
    check_diagnostics(
        r#"
static five = 5;
static x = five::Circle;
"#,
        expect![[r#"
            15..16: cannot infer the type of this number: it has no defining use — add a type annotation
            29..41: `five` is not a type; only an `enum` type has `::` variants
        "#]],
    );
}

#[test]
fn bare_enum_name_is_a_type_not_a_value() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static s = Shape;
"#,
        expect![[r#"
            49..54: `Shape` is a type, not a value
        "#]],
    );
}

#[test]
fn enum_cannot_be_constructed_directly() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static s = Shape(3);
"#,
        expect![[r#"
            49..57: `Shape` is an `enum`; construct it through one of its variants (`Shape::<variant>(...)`) (`Shape` is defined here at 6..11)
            55..56: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn variant_constructor_arity_is_checked() {
    // Constructors are plain functions: wrong arity is the ordinary
    // ArgCountMismatch, wrong payload type the ordinary call-site mismatch.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static a = Shape::Circle(1, 2);
static b = Shape::Circle("no");
"#,
        expect![[r#"
            56..75: expected 1 argument(s), found 2
            102..106: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn payload_less_variant_is_not_callable() {
    check_diagnostics(
        r#"
type Shape = enum { Point };
static p = Shape::Point();
"#,
        expect![[r#"
            41..53: expression of type `Shape` is not callable
        "#]],
    );
}

#[test]
fn variant_path_annotation_errors_explain_the_path() {
    // A variant path in type position always lowers to a silent
    // `Ty::Error`; when it names no variant at all, the diagnostic says
    // what is wrong with it: unknown variant, struct base, builtin base.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
type Point = struct { x: usize };
static a: Shape::Missing = 1;
static b: Point::Circle = 2;
static c: usize::Circle = 3;
"#,
        expect![[r#"
            82..96: `Shape` has no variant `Missing`
            112..125: `Point` has no variants (it is a `struct` type)
            141..154: `usize` has no variants (it is a builtin type)
        "#]],
    );
}

#[test]
fn enum_payloads_must_be_fully_written() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(_) };
"#,
        expect![[r#"
            28..29: a variant payload must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn field_access_on_an_enum_value_is_an_error() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize) };
static f = fn (s: Shape) { s.x };
"#,
        expect![[r#"
            67..68: no field `x` on `Shape` (`Shape` is an `enum`, declared here — it has variants, not fields at 14..36)
        "#]],
    );
}

#[test]
fn let_mut_of_a_variant_explains_a_later_mismatch() {
    // When the binding bites later, the mismatch carries the
    // binding hint ("inferred from its initializer") — which the ide layer
    // surfaces as an info-severity companion at the `let mut`.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s = Shape::Point;
    s = 5;
};
"#,
        expect![[r#"
            99..100: type mismatch: expected `Shape`, found `{number}` (`s` was inferred to have type `Shape` from its initializer at 73..74)
        "#]],
    );
}

// ---- `::Variant` in expression position --------------------------------

#[test]
fn an_elided_variant_expression_resolves_from_the_expected_type() {
    // The mirror of the elided-sigil variant PATTERN: the pattern reads the
    // scrutinee's enum, the expression reads the position's expected type.
    // Every axiom position works — an item annotation, a return type, a
    // call argument, a `let` annotation — including a generic enum, whose
    // ARGS come from the expectation rather than from a fresh mention.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
type Opt = enum::<T> { Some(T), None };
static annotated: Shape = ::Circle(3);
static returned = fn() -> Shape { ::Point };
static taken = fn(s: Shape) -> usize { 1 };
static passed = fn() -> usize { taken(::Point) };
static bound = fn() -> usize {
    let s: Shape = ::Circle(2);
    1
};
static generic = fn::<@a>(p: usize.&::<@a>) -> Opt::<usize.&::<@a>> { ::Some(p) };
static empty = fn::<@a>(p: usize.&::<@a>) -> Opt::<usize.&::<@a>> { ::None };
static returned_by_keyword = fn() -> Shape { return ::Point; };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_elided_variant_expression_is_reject_only_sugar() {
    // It reads `expected` and NOTHING else: no backwards inference, no
    // sibling scan, no deferral. Where no enum is in view the qualified
    // spelling is named, and it always works — so this only ever removes a
    // rejection, and the qualified form stays canonical.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static unpinned = fn() -> usize {
    let s = ::Point;
    1
};
static not_an_enum = fn() -> usize { ::Point };
static no_such_variant = fn() -> Shape { ::Square };
"#,
        expect![[r#"
            91..98: cannot resolve `::Point` without an expected type — write `Enum::Point`
            146..153: cannot resolve `::Point`: the expected type `usize` is not an enum — write `Enum::Point`
            198..206: `Shape` has no variant `Square` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn an_elided_variant_expression_is_refused_in_a_join_position() {
    // FLAGGED, and deliberate. A match arm's body and an `if` branch are
    // JOIN leaves: each is inferred against a fresh variable so "outer
    // expectation pressure never leaks in" (the join's own comment), and
    // the construct's expectation is applied to the RESULT after every leaf
    // has been visited. So there is genuinely no enum in view at the leaf,
    // and the sigil is refused with the escape named.
    //
    // Lifting this is a second expectation channel through the join — an
    // inference-shape decision, not sugar — and it is purely additive: it
    // would only ever turn these errors into acceptances.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static in_arm = fn(c: bool) -> Shape { match c { _ => ::Point } };
static in_branch = fn(c: bool) -> Shape { if c { ::Point } else { ::Point } };
"#,
        expect![[r#"
            99..106: cannot resolve `::Point` without an expected type — write `Enum::Point`
            161..168: cannot resolve `::Point` without an expected type — write `Enum::Point`
            178..185: cannot resolve `::Point` without an expected type — write `Enum::Point`
        "#]],
    );
}
