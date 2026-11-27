//! Assignment, field assignment, unit and deferred `let`.

use super::*;

#[test]
fn mutation_of_a_let_binding_is_visible_to_later_reads() {
    check_run(
        "",
        "(fn { let mut x: usize = 1; x = x + 2; x })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn mutating_a_mut_param_is_visible_to_later_reads() {
    check_run(
        "static f = fn (mut n: usize) -> usize { n = n + 1; n };",
        "f(41)",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn assigning_to_an_immutable_binding_traps_at_runtime() {
    // The executed path runs into the assignment: it crashes with exactly
    // the message the editor shows as a squiggle.
    check_run(
        "",
        "(fn { let x: usize = 1; x = 2; x })()",
        expect![[r#"
            error[Trap]: cannot assign to `x`: it is not declared `mut`
        "#]],
    );
}

#[test]
fn illegal_assignment_in_an_unevaluated_branch_does_not_crash() {
    // The dead branch's assignment is squiggled (tested in hir) and
    // trapped, but only the evaluated path crashes: with the condition
    // true the function returns normally.
    check_run(
        "",
        "(fn () -> usize { let x = 1; if true { x } else { x = 2; x } })()",
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn field_assignment_mutates_the_record() {
    check_run(
        "",
        "(fn { let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; p.x = 10; p.x + p.y })()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn nested_field_assignment_writes_through_both_levels() {
    // The projection navigates `outer.inner` then `inner.b`; the sibling
    // field and the outer record's other field are untouched.
    check_run(
        "",
        "(fn {\n            let mut p: struct { inner: struct { a: usize, b: usize }, c: usize } =\n                struct { inner = struct { a = 1, b = 2 }, c = 3 };\n            p.inner.b = 20;\n            p.inner.a + p.inner.b + p.c\n        })()",
        expect![[r#"
            => 24
        "#]],
    );
}

#[test]
fn field_assignment_through_a_named_type() {
    // Erasure: a `Point` is its underlying record at runtime; the write
    // projects through the declaration's field order.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> usize {
    let mut p = Point(struct { x = 1, y = 2 });
    p.x = 40;
    p.x + p.y
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn field_assignment_in_a_const_fn_works_at_compile_time() {
    // The field-projection twin of local mutation in a const context:
    // forcing `x` runs the write at check time.
    check_const(
        r#"
static bump = const fn (mut p: struct { n: usize }) -> usize {
    p.n = p.n + 1;
    p.n
};
static x = bump(struct { n = 41 });
"#,
        expect![[r#"
            bump = fn
            x = 42
        "#]],
    );
}

#[test]
fn field_assignment_to_an_immutable_root_traps_at_runtime() {
    // Squiggle-equals-crash, field edition: the message blames the root
    // binding, exactly as the editor shows it.
    check_run(
        "",
        "(fn { let p: struct { x: usize } = struct { x = 1 }; p.x = 2; p.x })()",
        expect![[r#"
            error[Trap]: cannot assign to `p.x`: `p` is not declared `mut`
        "#]],
    );
}

#[test]
fn illegal_field_assign_in_an_unevaluated_branch_does_not_crash() {
    // Culprit-branch idiom: the dead branch's unknown-field write is
    // squiggled (tested in hir) and trapped, but only the evaluated path
    // decides the run.
    check_run(
        "",
        "(fn () -> usize {\n            let mut p = struct { x = 1 };\n            if true { p.x } else { p.bogus = 2; p.x }\n        })()",
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn mutation_inside_a_const_fn_body_works_at_compile_time() {
    // `double` mutates a local of its own body; forcing `x` through it at
    // check time exercises mutation in a const context end to end.
    check_const(
        r#"
static double = const fn (n: usize) -> usize {
    let mut r = n;
    r = r + r;
    r
};
static x = double(21);
"#,
        expect![[r#"
            double = fn
            x = 42
        "#]],
    );
}

#[test]
fn unit_value_runs() {
    check_run(
        r#"
static id = fn (u: ()) -> () { u };
static main = fn () -> () {
    if id(()) == () { print("unit") };
    ()
};
"#,
        "main()",
        expect![[r#"
            output: "unit"
            => ()
        "#]],
    );
}

#[test]
fn a_deferred_binding_assigned_in_a_match_arm_runs() {
    check_run(
        r#"
static main = fn () -> () {
    let s = "abc";
    let mut i = 0;
    loop {
        let mut current;
        match s.next_char(i) {
            ::Char(c, next) => {
                current = c;
                i = next;
            },
            ::End => {
                break;
            },
        };
        if current == 'a' { print("found a\n"); }
    }
};
"#,
        "main()",
        expect![[r#"
            output: "found a\n"
            => ()
        "#]],
    );
}

#[test]
fn a_deferred_binding_holds_what_was_assigned() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut x: usize;
    let mut greeting;
    x = 1;
    greeting = "hi";
    print(greeting);
    x
};
"#,
        "main()",
        expect![[r#"
            output: "hi"
            => 1
        "#]],
    );
}

#[test]
fn a_deferred_binding_assigned_before_break_holds_the_last_value() {
    check_run(
        r#"
static looped = fn (n: usize) -> usize {
    let mut x;
    let mut i = 0;
    loop {
        x = i * 2;
        i = i + 1;
        if i > n { break; }
    }
    x
};
"#,
        "looped(3)",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn an_unannotated_deferred_binding_holds_every_variant_of_its_enum() {
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (c: bool) -> Shape {
    let mut s;
    s = Shape::Circle(1);
    if c { s = ::Point; }
    s
};
"#,
        "f(true) == Shape::Point",
        expect![[r#"
            => true
        "#]],
    );
}

/// The checker refuses the read; run anyway, the read traps with that
/// refusal rather than seeing a stale or zero value.
#[test]
fn a_refused_read_of_a_deferred_binding_traps_at_the_read() {
    check_run(
        r#"
static f = fn (c: bool) -> usize {
    let mut x;
    if c { x = 1; }
    x
};
"#,
        "f(true) + f(false)",
        expect![[r#"
            error[Trap]: `x` is used here, but not every path to here assigns it a value
        "#]],
    );
    check_run(
        r#"
static f = fn (n: usize) -> usize {
    let mut total = 0;
    let mut i = 0;
    loop {
        let mut x: usize;
        if i == 0 { x = 5; }
        total = total + x;
        i = i + 1;
        if i == n { break; }
    }
    total
};
"#,
        "f(3)",
        expect![[r#"
            error[Trap]: `x` is used here, but not every path to here assigns it a value
        "#]],
    );
}

/// Assigning a field does not assign the deferred binding itself, so `p`
/// is unassigned and the program traps.
#[test]
fn a_field_write_to_a_deferred_binding_traps() {
    check_run(
        r#"
static f = fn () -> usize {
    let mut p: struct { a: usize };
    p.a = 1;
    p.a
};
"#,
        "f()",
        expect![[r#"
            error[Trap]: `p` is used before it is assigned a value
        "#]],
    );
}
