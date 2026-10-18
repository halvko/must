//! `match`, let patterns, parameter patterns and destructuring.

use super::*;

#[test]
fn match_dispatches_on_each_variant() {
    check_run(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static describe = fn (s: Shape) -> str {
    match s {
        ::Circle(r) => "circle",
        ::Pair(n, text) => text,
        ::Point => "point",
    }
};
static main = fn {
    print(describe(Shape::Circle(3)));
    print(describe(Shape::Pair(1, "pair")));
    print(describe(Shape::Point));
};
"#,
        "main()",
        expect![[r#"
            output: "circlepairpoint"
            => ()
        "#]],
    );
}

#[test]
fn match_extracts_payloads_positionally() {
    check_run(
        r#"
type Shape = enum { Pair(usize, usize) };
static sum = fn (s: Shape) -> usize {
    match s {
        ::Pair(a, b) => a + b,
    }
};
"#,
        "sum(Shape::Pair(30, 12))",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn mut_match_bindings_are_assigned_locally() {
    check_run(
        r#"
type Opt = enum { Some(usize), None };
static bump = fn (o: Opt) -> usize {
    match o {
        ::Some(mut n) => {
            n = n + 1;
            n
        },
        mut other => {
            other = Opt::Some(100);
            match other {
                ::Some(n) => n,
                ::None => 0,
            }
        },
    }
};
"#,
        "bump(Opt::Some(41)) + bump(Opt::None)",
        expect![[r#"
            => 142
        "#]],
    );
}

#[test]
fn a_mut_match_binder_can_be_reassigned_to_another_variant() {
    // The binder holds the enum value: it starts as the `Circle` and
    // can be reassigned to `Point`.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (point: bool) -> usize {
    let c = Shape::Circle(1);
    match c {
        mut o => {
            if point { o = Shape::Point; }
            match o {
                ::Circle(n) => n,
                ::Point => 7,
            }
        },
    }
};
"#,
        "f(false) * 10 + f(true)",
        expect![[r#"
            => 17
        "#]],
    );
}

#[test]
fn nested_mut_binders_of_a_variant_value_can_be_reassigned() {
    // Each binder starts as the `Circle` it was given and can be
    // reassigned to `Point`.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
type Pair = enum { Two(Shape, usize) };
static size = fn (s: Shape) -> usize {
    match s {
        ::Circle(n) => n,
        ::Point => 7,
    }
};
static f = fn (point: bool) -> usize {
    let struct { mut a } = struct { a = Shape::Circle(1) };
    let [mut c] = [Shape::Circle(2)];
    if point { a = Shape::Point; c = Shape::Point; }
    match Pair::Two(Shape::Circle(3), 0) {
        ::Two(mut e, _) => {
            if point { e = Shape::Point; }
            size(a) * 100 + size(c) * 10 + size(e)
        },
    }
};
"#,
        "f(false) * 1000 + f(true)",
        expect![[r#"
            => 123777
        "#]],
    );
}

#[test]
fn array_let_patterns_bind_each_element() {
    check_run(
        r#"
static f = fn () -> usize {
    let [a, mut b, _] = [1, 2, 3];
    b = b * 10;
    let [[p, q], [r, s]]: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    a + b + p * 1000 + q * 100 + r * 10 + s
};
"#,
        "f()",
        expect![[r#"
            => 1255
        "#]],
    );
}

#[test]
fn variant_let_patterns_bind_the_payloads() {
    check_run(
        r#"
type Only = enum { One(usize, usize) };
static f = fn (o: Only) -> usize {
    let ::One(x, mut y) = o;
    y = y + 1;
    let Only::One(z, _) = o;
    x * 100 + y * 10 + z
};
"#,
        "f(Only::One(1, 2))",
        expect![[r#"
            => 131
        "#]],
    );
}

#[test]
fn nested_let_patterns_bind_through_every_level() {
    check_run(
        r#"
type Only = enum { One(usize) };
type Pair = struct { lo: usize, hi: usize };
static f = fn (v: Only, w: Only) -> usize {
    let [::One(n), ::One(m)] = [v, w];
    let [Pair(struct { lo, hi }), _] = [Pair(struct { lo = 5, hi = 6 }), Pair(struct { lo = 7, hi = 8 })];
    n * 1000 + m * 100 + lo * 10 + hi
};
"#,
        "f(Only::One(1), Only::One(2))",
        expect![[r#"
            => 1256
        "#]],
    );
}

#[test]
fn a_variant_let_pattern_through_a_mut_borrow_binds_mut_borrows() {
    // Writing through `x` changes `o`.
    check_run(
        r#"
type Only = enum { One(usize, usize) };
static f = fn (mut o: Only) -> usize {
    let r = o.&mut;
    let ::One(x, y) = r;
    x.* = 100;
    let v = y.*;
    match o {
        ::One(a, _) => a + v,
    }
};
"#,
        "f(Only::One(1, 2))",
        expect![[r#"
            => 102
        "#]],
    );
}

#[test]
fn parameter_patterns_bind_the_parts_of_each_argument() {
    check_run(
        r#"
type Only = enum { One(usize, usize) };
static f = fn ([a, b]: [usize; 2], ::One(c, d): Only) -> usize {
    a * 1000 + b * 100 + c * 10 + d
};
"#,
        "f([1, 2], Only::One(3, 4))",
        expect![[r#"
            => 1234
        "#]],
    );
}

#[test]
fn a_refutable_let_pattern_traps_with_the_diagnostic_message() {
    check_run(
        r#"
type Opt = enum { Some(usize), None };
static f = fn (o: Opt) -> usize {
    let ::Some(n) = o;
    n
};
"#,
        "f(Opt::Some(1))",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_let_pattern_naming_another_variant_traps_instead_of_binding() {
    // The `Circle`'s payload is never read as the `Label`'s `str`.
    let text = r#"
type Shape = enum { Circle(usize), Label(str) };
static owned = fn (c: Shape) -> str {
    let ::Label(s) = c;
    s
};
static borrowed = fn (c: Shape) -> str {
    let ::Label(s) = c.&;
    s.*
};
"#;
    check_run(
        text,
        "owned(Shape::Circle(1))",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
    check_run(
        text,
        "borrowed(Shape::Circle(1))",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_let_pattern_on_a_value_of_unknown_type_traps_instead_of_binding() {
    // Error recovery: `Nope` names no type, so nothing says `c` is a
    // `Label`, and the `Circle`'s payload must not be read as a `str`.
    check_run(
        r#"
type Shape = enum { Circle(usize), Label(str) };
static f = fn (c: Shape) -> str {
    let Shape::Label(s): Nope = c;
    s
};
"#,
        "f(Shape::Circle(7))",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_variant_pattern_on_a_value_of_unknown_type_reads_no_payload() {
    // Error recovery: `Nope` names no type, so `g()` and the parameter are
    // `{error}`. The `Circle`'s payload is never read as a `str`.
    let text = r#"
type Shape = enum { Circle(usize), Label(str) };
static g = fn () -> Nope { Shape::Circle(7) };
static from_call = fn () -> str {
    let Shape::Label(s) = g();
    s
};
static from_param = fn (Shape::Label(s): Nope) -> str { s };
"#;
    check_run(
        text,
        "from_call()",
        expect![[r#"
        error[Trap]: cannot use `g`: its type annotation has errors
    "#]],
    );
    check_run(
        text,
        "from_param(Shape::Circle(7))",
        expect![[r#"
        error[Trap]: cannot use `from_param`: its type annotation has errors
    "#]],
    );
}

#[test]
fn a_refutable_parameter_pattern_traps_before_it_reads_the_argument() {
    let text = r#"
type S = enum { A(usize), B(usize, usize) };
static owned = fn (::B(x, y): S) -> usize { x + y };
static borrowed = fn::<@a>(::B(x, y): S.&::<@a>) -> usize { x.* + y.* };
static nested = fn ([::B(x, y)]: [S; 1]) -> usize { x + y };
"#;
    check_run(
        text,
        "owned(S::A(1))",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
    check_run(
        text,
        "{ let s: S = S::A(1); borrowed(s.&) }",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
    check_run(
        text,
        "nested([S::A(1)])",
        expect![[r#"
            error[Trap]: this pattern can fail to match; use `match`
        "#]],
    );
}

#[test]
fn a_parameter_pattern_with_too_many_payloads_traps_before_it_reads_the_argument() {
    check_run(
        r#"
type S = enum { A(usize), B(usize, usize) };
static f = fn (::A(x, y): S) -> usize { x + y };
"#,
        "f(S::A(1))",
        expect![[r#"
            error[Trap]: `A` has 1 payload, this pattern names 2
        "#]],
    );
}

#[test]
fn a_parameter_pattern_naming_a_missing_field_traps_before_it_reads_the_argument() {
    check_run(
        r#"
type R = struct { a: usize };
static f = fn (R(struct { a, b }): R) -> usize { a + b };
"#,
        "f(R(struct { a = 1 }))",
        expect![[r#"
            error[Trap]: no field `b` on `struct { a: usize }`
        "#]],
    );
}

#[test]
fn a_parameter_pattern_longer_than_its_array_traps_before_it_reads_the_argument() {
    check_run(
        r#"
static f = fn ([a, b, c]: [usize; 2]) -> usize { a + b + c };
"#,
        "f([1, 2])",
        expect![[r#"
            error[Trap]: this pattern has 3 elements, but the array has length 2
        "#]],
    );
}

#[test]
fn a_qualified_variant_let_pattern_binds_through_a_borrow() {
    check_run(
        r#"
type Only = enum { One(usize, usize) };
static f = fn (o: Only) -> usize {
    let Only::One(a, b) = o.&;
    a.* + b.*
};
"#,
        "f(Only::One(40, 2))",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn nonexhaustive_match_traps_with_the_diagnostic_message() {
    // Reaching the uncovered variant crashes with exactly the text the
    // squiggle shows; the covered variant still runs fine.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
static main = fn {
    print("covered arm runs");
    f(Shape::Circle(1));
    f(Shape::Point);
};
"#,
        "main()",
        expect![[r#"
            output: "covered arm runs"
            error[Trap]: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn match_on_a_reassigned_value_round_trips() {
    // Construct, reassign another variant, dispatch on the tag — the full
    // round trip.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn () -> usize {
    let mut s = Shape::Point;
    s = Shape::Circle(42);
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        "f()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn match_in_const_context_evaluates() {
    check_const(
        r#"
type Shape = enum { Circle(usize), Point };
static pick = const fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 7,
    }
};
static a = pick(Shape::Circle(3));
static b = pick(Shape::Point);
static c = const { match Shape::Circle(9) { ::Circle(r) => r, ::Point => 0 } };
"#,
        expect![[r#"
            pick = fn
            a = 3
            b = 7
            c = 9
        "#]],
    );
}

#[test]
fn match_runs_the_state_machine_end_to_end() {
    // Construct `Running(5)`, pass it to a function taking the enum,
    // dispatch on it inside, extract the payload.
    check_run(
        r#"
type State = enum { Idle, Running(usize) };
static tick = fn (s: State) -> usize {
    match s {
        ::Running(n) => n + 1,
        ::Idle => 0,
    }
};
"#,
        "tick(State::Running(5))",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn match_binding_arm_receives_the_whole_scrutinee() {
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> Shape {
    match s {
        whole => whole,
    }
};
"#,
        "f(Shape::Circle(8))",
        expect![[r#"
            => Shape::Circle(8)
        "#]],
    );
}

#[test]
fn bind_arm_named_like_payload_variant_binds_the_whole_value() {
    // G25's motivating scenario, end to end: `Circle` (bare, no `::`) is a
    // binding named like a payload-carrying variant. It used to require a
    // `PatArity` error under reinterpretation; now it just binds the whole
    // tagged value — no error, no payload extraction — and the value
    // round-trips as the original variant instance.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> Shape {
    match s {
        Circle => Circle,
    }
};
"#,
        "f(Shape::Circle(8))",
        expect![[r#"
            => Shape::Circle(8)
        "#]],
    );
}

// ---- record destructuring end to end ----

#[test]
fn let_record_destructure_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x, y }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; x + y })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn let_record_destructure_rename_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x as a, y as b }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; a + b })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn let_record_destructure_rest_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x, .. }: struct { x: usize, y: usize, z: usize } = struct { x = 1, y = 2, z = 3 }; x })()"#,
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn param_record_destructure_evaluates() {
    check_run(
        "static add = fn (struct { x, y }: struct { x: usize, y: usize }) -> usize { x + y };",
        "add(struct { x = 4, y = 5 })",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn newtype_destructure_evaluates() {
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static add = fn (Point(struct { x, y })) -> usize { x + y };
"#,
        "add(Point(struct { x = 4, y = 5 }))",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn record_destructure_in_const_context_evaluates() {
    check_const(
        r#"
static p: struct { x: usize, y: usize } = struct { x = 3, y = 4 };
static sum = const fn () -> usize {
    let struct { x, y } = p;
    x + y
}();
"#,
        expect![[r#"
            p = { x = 3, y = 4 }
            sum = 7
        "#]],
    );
}

#[test]
fn per_binding_mut_record_destructure_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { mut x, y }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; x = x + y; x })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}
