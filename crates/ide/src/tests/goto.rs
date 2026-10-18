//! Goto definition.

use super::*;

#[test]
fn goto_local_picks_nearest_shadowing_binding() {
    check_goto(
        r#"
static f = fn {
    let x = 1;
    let x = 2;
    print(x$0);
}
"#,
        "x",
        1,
    );
}

#[test]
fn goto_param() {
    check_goto("static f = fn (count: usize) { count$0 + 1 }", "count", 0);
}

#[test]
fn goto_static_across_mutual_recursion() {
    check_goto(
        r#"
const a = fn { b$0() };
static b = fn { a() };
"#,
        // Occurrence 0 of `b` is the reference itself; 1 is the definition.
        "b",
        1,
    );
}

#[test]
fn goto_static_from_its_own_body() {
    check_goto("static rec = fn { rec$0() };", "rec", 0);
}

#[test]
fn goto_builtin_is_none() {
    check_no_goto(r#"static f = fn { print$0("hi") };"#);
}

#[test]
fn goto_assignment_lhs() {
    // The LHS of an assignment lowers as a normal expression, so goto-def
    // works on it exactly like any other read of the binding.
    check_goto(
        r#"
static f = fn {
    let mut x = 1;
    x$0 = 5;
};
"#,
        "x",
        0,
    );
}

#[test]
fn goto_hole_pattern_is_none() {
    // `_` isn't a name, so there's nothing to jump to — and nothing panics.
    check_no_goto("static f = fn { let _ = 1; _$0; };");
}

// ---- named types ----

#[test]
fn goto_type_item_from_annotation() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_type_item_from_construction_call() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x = 1 });
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_builtin_type_in_annotation_is_none() {
    check_no_goto("static f = fn (n: usize$0) { n };");
}

// ---- enums and variants ----

#[test]
fn goto_enum_from_variant_path_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize) };
static s = Sha$0pe::Circle(3);
"#,
        "Shape",
        0,
    );
}

#[test]
fn goto_variant_from_variant_path() {
    // The second segment lands on the variant inside the declaration.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        "Point",
        0,
    );
}

#[test]
fn goto_variant_from_type_annotation() {
    // `Shape::Circle` is not a type (the checker says so); the segment
    // names the variant, so it navigates like the value spelling.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "Circle",
        0,
    );
}

#[test]
fn goto_variant_from_the_expression_sigil() {
    // `::Point` in expression position resolves to one variant of one
    // enum, so it jumps exactly where the qualified spelling does.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = ::Poi$0nt;
"#,
        "Point",
        0,
    );
}

// ---- match ----

#[test]
fn goto_variant_from_sigil_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 is the declaration inside the enum literal.
        "Circle",
        0,
    );
}

#[test]
fn goto_variant_from_qualified_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "Circle",
        0,
    );
}

#[test]
fn goto_enum_from_qualified_pattern_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape$0::Circle(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 of `Shape` is the `type` item's name.
        "Shape",
        0,
    );
}

#[test]
fn goto_on_bare_bind_named_like_variant_has_no_target() {
    // Bare `Point` (no `::`) is just a binding — its own declaration — even
    // though it spells a variant's name. Goto on it has nothing to jump to.
    check_no_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
    );
}

#[test]
fn goto_pattern_binding_use_in_arm_body() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r$0,
        _ => 0,
    }
};
"#,
        // `r` also occurs inside the two `Circle`s; occurrence 2 is the
        // pattern binding itself.
        "r",
        2,
    );
}

#[test]
fn goto_record_destructured_binding_use() {
    check_goto(
        r#"
static f = fn {
    let struct { x, y } = struct { x = 1, y = 2 };
    print(x$0);
}
"#,
        "x",
        // Occurrence 0 is the field/binding name in the pattern (its
        // declaration site — where the use inside `print` should jump to);
        // occurrence 1 is the record literal's field name.
        0,
    );
}

#[test]
fn goto_record_destructure_rename_use_lands_on_the_rename() {
    check_goto(
        r#"
static f = fn {
    let struct { x as alpha } = struct { x = 1 };
    print(alpha$0);
}
"#,
        "alpha",
        0,
    );
}

#[test]
fn goto_param_record_destructured_binding_use() {
    check_goto(
        "static f = fn (struct { num }: struct { num: usize }) { num$0 };",
        "num",
        0,
    );
}

#[test]
fn goto_definition_on_a_const_param_jumps_to_the_binder() {
    check_goto(
        "static f = fn::<const N: usize>() -> usize { N$0 + 1 };",
        "N",
        0,
    );
}

#[test]
fn goto_through_index_chain() {
    check_goto(
        r#"
static f = fn {
    let buf = [1, 2];
    let x = buf$0[0];
}
"#,
        "buf",
        0,
    );
}

#[test]
fn goto_definition_on_member_call_name_lands_on_the_member() {
    check_goto(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).get$0() }};"
        ),
        "get",
        0,
    );
}

#[test]
fn goto_definition_on_a_qualified_member_path_lands_on_the_member() {
    // The G13 escape `Type::member` names exactly one member — one click
    // away, like the dot-call's name.
    check_goto(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter::get$0(Counter(struct {{ n = 1 }})) }};"
        ),
        "get",
        0,
    );
}

#[test]
fn goto_bound_lands_on_the_trait_decl() {
    check_goto(
        r#"
trait Display = requires { fmt: fn(Self) -> str; };
static f = fn::<T: Displ$0ay>(x: T) -> usize { 1 };
"#,
        "Display",
        0,
    );
}

#[test]
fn goto_impl_head_lands_on_the_trait_decl() {
    check_goto(
        r#"
trait Show = requires { show: fn(Self) -> str; };
type P = struct { a: usize } with {
    impl Sh$0ow { show = fn(x: Self) -> str { "p" }; }
};
"#,
        "Show",
        0,
    );
}

#[test]
fn goto_impl_directed_dot_call_lands_on_the_impl_member() {
    check_goto(
        r#"
trait D = requires { mem: fn(Self) -> str; } with {
    impl usize { mem = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { n.mem$0() };
"#,
        // Occurrence 0 is the requirement, 1 is the impl member.
        "mem",
        1,
    );
}

#[test]
fn goto_bound_directed_dot_call_lands_on_the_requirement() {
    check_goto(
        r#"
trait D = requires { m: fn(Self) -> str; };
static f = fn::<T: D>(x: T) -> str { x.m$0() };
"#,
        "m",
        0,
    );
}

#[test]
fn goto_qualified_member_lands_on_the_impl_member() {
    check_goto(
        r#"
trait D = requires { mem: fn(Self) -> str; } with {
    impl usize { mem = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D::mem$0(n) };
"#,
        "mem",
        1,
    );
}

#[test]
fn goto_qualified_call_base_lands_on_the_trait() {
    check_goto(
        r#"
trait D = requires { m: fn(Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D$0::m(n) };
"#,
        "D",
        0,
    );
}
