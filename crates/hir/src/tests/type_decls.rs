//! Named type declarations and construction.

use super::*;

// ---- named types (`type Foo = struct { ... };`) ----

#[test]
fn type_decl_alone_is_clean() {
    check_diagnostics("type Foo = struct { x: usize, y: str };", expect![[r#""#]]);
}

#[test]
fn type_and_static_share_the_flat_namespace() {
    // One namespace: a `type` and a `static` of the same name collide via
    // the ordinary duplicate machinery.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static Foo = 5;
"#,
        expect![[r#"
            6..9: `Foo` is defined multiple times (also defined here at 40..43)
            40..43: `Foo` is defined multiple times (also defined here at 6..9)
            46..47: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn type_rhs_must_be_a_struct_literal() {
    check_diagnostics(
        "type Foo = 5;",
        expect![[r#"
            11..12: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

#[test]
fn type_decl_fields_must_be_types() {
    // The retired `a: 5` spelling gets the targeted respell parse error, a
    // shorthand field has no type to read, and an unknown or value name in
    // a field gets the type-position errors.
    check_diagnostics(
        r#"
static five = 5;
type Foo = struct { a: 5, b, c: missing, d: five };
"#,
        expect![[r#"
            15..16: cannot infer the type of this number: it has no defining use — add a type annotation
            41..42: record fields are defined with `=` (`name = value`); `:` annotates a type
            44..45: expected a type for field `b`: `name: Type`
            50..57: unknown type `missing`
            62..66: `five` is not a type
        "#]],
    );
}

#[test]
fn annotation_resolves_named_type_and_field_projects() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo) -> usize { p.x };
"#,
        expect![[r#"
            44..72 'fn (p: Foo) -> us...': fn(Foo) -> usize
            48..49 'p': Foo
            65..72 '{ p.x }': usize
            67..68 'p': Foo
            67..70 'p.x': usize
        "#]],
    );
}

#[test]
fn value_item_in_type_position_is_an_error() {
    check_diagnostics(
        r#"
static double = fn (x: usize) -> usize { x + x };
static f = fn (p: double) { };
"#,
        expect![[r#"
            69..75: `double` is not a type
        "#]],
    );
}

#[test]
fn bare_type_name_is_not_a_value() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static x = Foo;
"#,
        expect![[r#"
            44..47: `Foo` is a type, not a value
        "#]],
    );
}

#[test]
fn construction_call_produces_the_named_type() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = 1 });
"#,
        expect![[r#"
            44..47 'Foo': fn(struct { x: usize }) -> Foo
            44..65 'Foo(struct { x = ...': Foo
            48..64 'struct { x = 1 }': struct { x: usize }
            61..62 '1': usize
        "#]],
    );
}

#[test]
fn construction_field_mismatch_cites_the_field_declaration() {
    // The squiggle lands on the wrong field's value; the hint points at the
    // field's declaration inside the `type` item.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = "s" });
"#,
        expect![[r#"
            61..64: type mismatch: expected `usize`, found `str` (expected `usize` because of this field declaration at 21..29)
        "#]],
    );
}

#[test]
fn construction_non_record_argument_cites_the_declaration() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(5);
"#,
        expect![[r#"
            48..49: type mismatch: expected `struct { x: usize }`, found `{number}` (expected `struct { x: usize }` because of `Foo`'s declaration at 6..9)
        "#]],
    );
}

#[test]
fn construction_takes_exactly_one_argument() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static p = Foo(struct { x = 1 }, 2);
static q = Foo();
"#,
        expect![[r#"
            44..68: `Foo` takes exactly one argument (its underlying `struct` value), found 2 (`Foo` is defined here at 6..9)
            66..67: cannot infer the type of this number: it has no defining use — add a type annotation
            81..86: `Foo` takes exactly one argument (its underlying `struct` value), found 0 (`Foo` is defined here at 6..9)
        "#]],
    );
}

#[test]
fn named_type_is_distinct_from_its_underlying_record() {
    // No implicit nominal↔structural coercion in either direction. The
    // named-expected direction carries the construct-it hint.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static a: Foo = struct { x = 1 };
static b: struct { x: usize } = Foo(struct { x = 1 });
"#,
        expect![[r#"
            49..65: type mismatch: expected `Foo`, found `struct { x: {number} }`; `Foo` is a distinct type — construct it with `Foo(...)` (expected `Foo` because of this annotation at 43..46)
            99..120: type mismatch: expected `struct { x: usize }`, found `Foo` (expected `struct { x: usize }` because of this annotation at 77..96)
        "#]],
    );
}

#[test]
fn named_types_unify_by_declaration_not_shape() {
    // Same shape, different declarations: still different types.
    check_diagnostics(
        r#"
type Meters = struct { value: usize };
type Feet = struct { value: usize };
static len: Meters = Feet(struct { value = 3 });
"#,
        expect![[r#"
            98..124: type mismatch: expected `Meters`, found `Feet` (expected `Meters` because of this annotation at 89..95)
        "#]],
    );
}

#[test]
fn no_such_field_on_named_type_points_at_the_declaration() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo) { p.z };
"#,
        expect![[r#"
            60..61: no field `z` on `Foo` (the fields of `Foo` are declared here at 12..31)
        "#]],
    );
}

#[test]
fn equality_between_named_and_bare_record_is_a_type_error() {
    // Eq/Ne go through unification, so nominal vs structural is rejected
    // like any other mismatch.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static eq = Foo(struct { x = 1 }) == struct { x = 1 };
"#,
        expect![[r#"
            70..86: type mismatch: expected `Foo`, found `struct { x: {number} }`; `Foo` is a distinct type — construct it with `Foo(...)` (this operand has type `Foo` at 45..66)
        "#]],
    );
}

#[test]
fn construction_is_legal_in_const_contexts() {
    // Pure construction, not a user fn call: fine in an item initializer
    // (always a const context) and inside a `const { ... }` block.
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static a = Foo(struct { x = 1 });
const b = const { Foo(struct { x = 2 }) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn recursive_type_declaration_does_not_hang() {
    // `Ty::Named` is identity, not expansion: a self-referential field
    // lowers to `Named(Foo)` and stops.
    check_diagnostics(
        r#"
type Foo = struct { next: Foo };
static f = fn (p: Foo) { p.next.next };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn assigning_to_a_type_name_reports_type_not_value() {
    check_diagnostics(
        r#"
type Foo = struct { x: usize };
static f = fn { Foo = 5; };
"#,
        expect![[r#"
            49..52: `Foo` is a type, not a value
        "#]],
    );
}

#[test]
fn hole_typed_binding_can_hold_a_named_type() {
    check_infer(
        r#"
type Foo = struct { x: usize };
static f = fn { let p = Foo(struct { x = 1 }); p.x };
"#,
        expect![[r#"
            44..85 'fn { let p = Foo(...': fn() -> usize
            47..85 '{ let p = Foo(str...': usize
            53..54 'p': Foo
            57..60 'Foo': fn(struct { x: usize }) -> Foo
            57..78 'Foo(struct { x = ...': Foo
            61..77 'struct { x = 1 }': struct { x: usize }
            74..75 '1': usize
            80..81 'p': Foo
            80..83 'p.x': usize
        "#]],
    );
}
