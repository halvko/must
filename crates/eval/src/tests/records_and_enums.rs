//! Records, named types and enum values.

use super::*;

#[test]
fn record_construction_and_field_access() {
    check_run(
        "",
        "(fn { let p: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; p.x + p.y })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn nested_records_construct_and_project() {
    check_run(
        "",
        r#"(fn { let a: struct { b: struct { c: usize } } = struct { b = struct { c = 5 } }; a.b.c })()"#,
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn records_const_evaluate() {
    check_const(
        r#"
static p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
static sum = p.x + p.y;
"#,
        expect![[r#"
            p = { x = 1, y = 2 }
            sum = 3
        "#]],
    );
}

#[test]
fn shorthand_fields_evaluate() {
    check_run(
        "",
        r#"(fn { let x: usize = 5; let y: usize = 6; let p = struct { x, y }; p.x + p.y })()"#,
        expect![[r#"
            => 11
        "#]],
    );
}

#[test]
fn record_equality_is_structural_both_ways() {
    check_run(
        "",
        r#"(fn { let one: usize = 1; struct { x = one } == struct { x = one } })()"#,
        expect![[r#"
            => true
        "#]],
    );
    check_run(
        "",
        r#"(fn { let one: usize = 1; let two: usize = 2; struct { x = one } == struct { x = two } })()"#,
        expect![[r#"
            => false
        "#]],
    );
    check_run(
        "",
        r#"(fn { let one: usize = 1; let two: usize = 2; struct { x = one } != struct { x = two } })()"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn field_access_on_a_nonexistent_field_still_traps_at_runtime() {
    // Records have a real MIR/eval story now, but a field inference
    // rejected is exactly as trapped as before records had an eval story
    // (the earlier message, unchanged).
    check_run(
        "",
        r#"(fn { let p: struct { x: usize } = struct { x = 1 }; p.y })()"#,
        expect![[r#"
            error[Trap]: no field `y` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn named_type_construction_erases_to_its_record() {
    // Full erasure: `Foo(v)` is `v` at runtime — the const value of a
    // Foo-typed item is a plain record value. Forcing the `type` item
    // itself (this helper forces every item) yields a Trap-kind error,
    // the kind `ide` never surfaces as a diagnostic: a type has no value,
    // and reads of `Foo` already trap with their own type-not-a-value
    // message.
    check_const(
        r#"
type Foo = struct { x: usize, y: str };
static p = Foo(struct { x = 1, y = "s" });
static x = p.x;
"#,
        expect![[r#"
            p = { x = 1, y = "s" }
            x = 1
        "#]],
    );
}

#[test]
fn named_type_equality_is_structural_under_the_hood() {
    // The type system keeps `Foo` and bare records apart; between two
    // `Foo`s, equality is the underlying records' structural equality.
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x = 1 }) == Foo(struct { x = 1 }))"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn named_type_inequality_observes_field_values() {
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x = 1 }) == Foo(struct { x = 2 }))"#,
        expect![[r#"
            => false
        "#]],
    );
}

#[test]
fn variant_values_round_trip_and_render_as_their_enum() {
    // Round-trip through a fn: the value stays equal to a freshly
    // constructed one, and the run's result renders as the enum value it
    // is, variant and payload.
    check_run(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static through = fn (c: Shape) -> Shape { c };
static main = fn -> Shape {
    if through(Shape::Circle(3)) == Shape::Circle(3) {
        print("round-tripped intact");
    };
    Shape::Pair(1, "a")
};
"#,
        "main()",
        expect![[r#"
            output: "round-tripped intact"
            => Shape::Pair(1, "a")
        "#]],
    );
}

#[test]
fn variant_values_const_evaluate() {
    // Every arity const-evaluates to its enum value, tag and payload, with
    // or without an annotation.
    check_const(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static circle = Shape::Circle(3);
static pair = Shape::Pair(1, "a");
static point = Shape::Point;
static annotated_circle: Shape = Shape::Circle(3);
"#,
        expect![[r#"
            circle = Shape::Circle(3)
            pair = Shape::Pair(1, "a")
            point = Shape::Point
            annotated_circle = Shape::Circle(3)
        "#]],
    );
}

#[test]
fn enum_values_compare_by_tag_and_payload() {
    check_const(
        r#"
type Shape = enum { Circle(usize), Point };
static a: Shape = Shape::Point;
static b: Shape = Shape::Point;
static c: Shape = Shape::Circle(1);
static same = a == b;
static different = a == c;
"#,
        expect![[r#"
            a = Shape::Point
            b = Shape::Point
            c = Shape::Circle(1)
            same = true
            different = false
        "#]],
    );
}

#[test]
fn first_class_constructor_runs() {
    // Called at runtime: a first-class constructor is an ordinary fn
    // *value*, so const contexts reject calling it (the conservative
    // value-call rule, same as any fn value).
    check_run(
        r#"
type Shape = enum { Circle(usize) };
static make: fn(usize) -> Shape = Shape::Circle;
static main = fn -> bool { make(3) == Shape::Circle(3) };
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn const_context_construction() {
    // Constructors are const-legal, with or without an annotation.
    check_const_blocks(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let a = const { Shape::Circle(2) };
    let b: Shape = const { Shape::Point };
};
"#,
        expect![[r#"
            f#0 = Shape::Circle(2)
            f#1 = Shape::Point
        "#]],
    );
}
