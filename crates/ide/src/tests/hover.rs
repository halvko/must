//! Hover.

use super::*;

#[test]
fn hover_hole_pattern_is_none() {
    check_no_hover("static f = fn { let _$0 = 1; };");
}

#[test]
fn hover_local_use() {
    check_hover(
        r#"static main = fn { let s = "hello"; print(s$0); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_binding_definition() {
    check_hover(
        r#"static main = fn { let s$0 = "hello"; print(s); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_mut_binding_definition_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s$0 = "hello"; s = "bye"; };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_binding_use_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s = "hello"; print(s$0); };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_param_shows_mut() {
    check_hover(
        "static f = fn (mut n$0: usize) { n = n + 1; };",
        "```must\nmut n: usize\n```",
    );
}

#[test]
fn hover_item_name_shows_inferred_fn_type() {
    check_hover(
        "static ma$0in = fn { print(\"hi\"); };",
        "```must\nmain: fn()\n```",
    );
}

#[test]
fn hover_shows_the_unsafe_marker_on_a_fn_type() {
    // The one-token difference is the whole of what a reader needs to know
    // about a value here, so hover shows it exactly where it is written.
    check_hover(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;\n\
         static main = fn { let g$0 = read; };",
        "```must\ng: unsafe fn(u8.&raw mut, usize) -> isize\n```",
    );
}

#[test]
fn hover_on_an_extern_declaration_shows_its_type() {
    // The declaration has no value expression at all, so hover must answer
    // from the DECLARATION — which is the whole contract anyway.
    check_hover(
        "unsafe extern static read$0: unsafe fn(u8.&raw mut, usize) -> isize;",
        "```must\nread: unsafe fn(u8.&raw mut, usize) -> isize\n```",
    );
}

#[test]
fn hover_static_use_shows_signature() {
    check_hover(
        r#"
static f = fn (n: usize) -> usize { n }
static main = fn { f$0(1); };
"#,
        "```must\nf: fn(usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_definition_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static high$0er_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    higher_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_call_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static higher_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    high$0er_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn hover_const_block_shows_its_value() {
    check_hover(
        "static f = fn () -> usize { con$0st { 2 + 3 } };",
        "```must\nconst { … }: usize = 5\n```",
    );
}

#[test]
fn hover_item_shows_const_value() {
    check_hover(
        "static exa$0mple: usize = 4 + 5;",
        "```must\nexample: usize = 9\n```",
    );
}

#[test]
fn hover_item_shows_record_const_value() {
    // Records const-evaluate: `Value::Record` displays like the type
    // does, field by field, in the same canonical (sorted) order.
    check_hover(
        "static po$0int: struct { x: usize, y: usize } = struct { y = 2, x = 1 };",
        "```must\npoint: struct { x: usize, y: usize } = { x = 1, y = 2 }\n```",
    );
}

#[test]
fn hover_use_shows_const_value_of_the_target() {
    check_hover(
        r#"
static greeting = "hi";
static main = fn { print(gree$0ting); };
"#,
        "```must\ngreeting: str = \"hi\"\n```",
    );
}

#[test]
fn hover_record_typed_binding() {
    // The record type renders canonically (sorted fields) on the binding.
    check_hover(
        r#"static f = fn { let p$0 = struct { y = "s", x = { let n: usize = 1; n } }; };"#,
        "```must\np: struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_field_access_shows_the_field_type() {
    check_hover(
        r#"static f = fn { let p: struct { x: usize } = struct { x = 1 }; let y = p.x$0; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_chained_field_access_intermediate_step() {
    // Hovering `b` in `a.b.c` shows the intermediate record's type.
    check_hover(
        r#"static f = fn { let a = struct { b = struct { c = "deep" } }; a.b$0.c; };"#,
        "```must\nb: struct { c: str }\n```",
    );
}

#[test]
fn hover_type_item_declaration() {
    check_hover(
        "type Foo$0 = struct { y: str, x: usize };",
        "```must\ntype Foo = struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_type_name_in_annotation() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_type_name_in_construction_call() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x = 1 });
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_named_typed_binding_shows_the_name() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn { let p$0 = Foo(struct { x = 1 }); p.x };
"#,
        "```must\np: Foo\n```",
    );
}

#[test]
fn hover_variant_value_shows_the_enum_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let c = Shape::Circle(3);
    c$0;
};
"#,
        r#"
```must
c: Shape
type Shape = enum { Circle(usize), Point }
```
"#,
    );
}

#[test]
fn hover_mut_binding_of_a_variant_shows_the_enum() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s = Shape::Point;
    s$0;
};
"#,
        r#"
```must
mut s: Shape
type Shape = enum { Circle(usize), Point }
```
"#,
    );
}

#[test]
fn hover_builtin_enum_binding_shows_its_variants() {
    check_hover(
        r#"
static f = fn (s: str) {
    let c$0 = s.next_char(0);
};
"#,
        r#"
```must
c: NextChar
type NextChar = enum { Char(char, usize), End }
```
"#,
    );
}

#[test]
fn hover_enum_name_shows_the_declaration() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Sha$0pe::Point;
"#,
        "```must\ntype Shape = enum { Circle(usize), Point }\n```",
    );
}

#[test]
fn hover_variant_segment_shows_its_enum_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        r#"
```must
Point: Shape
type Shape = enum { Circle(usize), Point }
```
"#,
    );
}

#[test]
fn hover_variant_in_type_position_shows_the_declaration() {
    // Not a type (the checker says so), but the segment names the
    // variant: the hover shows what it declares.
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn hover_the_expression_sigil_shows_the_variant() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = ::Poi$0nt;
"#,
        "```must\nShape::Point\n```",
    );
}

#[test]
fn hover_pattern_binding_shows_payload_type() {
    check_hover(
        r#"
type Shape = enum { Pair(usize, str) };
static f = fn (s: Shape) {
    match s {
        ::Pair(n, text$0) => text,
        _ => "",
    }
};
"#,
        "```must\ntext: str\n```",
    );
}

#[test]
fn hover_variant_name_in_pattern_shows_the_variant() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn hover_bind_arm_named_like_variant_shows_whole_scrutinee_type() {
    // Bare `Point` (no `::`) is a binding of the whole scrutinee, not a
    // variant match — hover shows the enum type, not the narrowed variant.
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
        r#"
```must
Point: Shape
type Shape = enum { Circle(usize), Point }
```
"#,
    );
}

#[test]
fn hover_on_loop_shows_its_type() {
    check_hover(
        r#"
static f = fn () -> usize {
    lo$0op {
        break 42;
    }
};
"#,
        "```must\nloop { … }: usize\n```",
    );
}

#[test]
fn hover_on_breakless_loop_shows_never() {
    check_hover(
        "static f = fn { lo$0op { } };",
        "```must\nloop { … }: !\n```",
    );
}

#[test]
fn hover_on_break_keyword_is_none() {
    // `break` names nothing; hover stays quiet (and doesn't panic) on it
    // and on its dangling outside-a-loop form.
    let (analysis, _file, pos) = fixture("static f = fn { loop { bre$0ak 1; } };");
    assert_eq!(analysis.hover(pos), None);
    let (analysis, _file, pos) = fixture("static f = fn { bre$0ak; };");
    assert_eq!(analysis.hover(pos), None);
}

// ---- record destructuring — hover and goto-def ----

#[test]
fn hover_record_destructured_binding_definition() {
    check_hover(
        r#"static f = fn { let struct { x$0, y }: struct { x: usize, y: str } = struct { x = 1, y = "s" }; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_record_destructured_binding_use() {
    check_hover(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = "s" }; print(y$0); };"#,
        "```must\ny: str\n```",
    );
}

#[test]
fn hover_record_destructure_rename_shows_the_new_name() {
    check_hover(
        r#"static f = fn { let struct { x as alpha }: struct { x: usize } = struct { x = 1 }; let b = alpha$0; };"#,
        "```must\nalpha: usize\n```",
    );
}

#[test]
fn hover_mut_field_binding_shows_mut() {
    check_hover(
        r#"static f = fn { let struct { mut x$0 }: struct { x: usize } = struct { x = 1 }; x = 2; };"#,
        "```must\nmut x: usize\n```",
    );
}

#[test]
fn hover_param_record_destructured_binding() {
    check_hover(
        "static f = fn (struct { n$0 }: struct { n: usize }) { n };",
        "```must\nn: usize\n```",
    );
}

#[test]
fn hover_newtype_destructured_binding() {
    check_hover(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) { let Point(struct { x$0, y }) = p; x };
"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_does_not_panic_near_a_turbofish() {
    let (analysis, _file, pos) = fixture(
        "static f = fn (x: usize) -> usize { x };\nstatic g = fn () -> usize { f::<usize$0>(1) };",
    );
    let _ = analysis.hover(pos);
}

#[test]
fn hover_does_not_panic_on_a_rigid_param_annotation() {
    // Hover on the `T` of `x: T` inside a generic body.
    let (analysis, _file, pos) = fixture("static id = fn::<T>(x: T$0) -> T { x };");
    let _ = analysis.hover(pos);
}

#[test]
fn hover_shows_the_rigid_param_through_the_bound_value() {
    // Hover on `x` in the body: its type is the rigid param, displayed by
    // its binder name.
    let (analysis, _file, pos) = fixture("static id = fn::<T>(x: T) -> T { x$0 };");
    if let Some(hover) = analysis.hover(pos) {
        assert!(
            hover.markup.contains('T'),
            "hover should mention the param: {:?}",
            hover.markup
        );
    }
}

#[test]
fn hover_shows_generic_type_instance() {
    check_hover(
        "type Pair = struct::<T> { a: T, b: T };\nstatic main = fn { let p$0 = Pair::<usize>(struct { a = 1, b = 2 }); };",
        "```must\np: Pair::<usize>\n```",
    );
}

#[test]
fn hover_shows_the_generic_enum_instance_of_a_constructed_variant() {
    check_hover(
        r#"
type Option = enum::<T> { Some(T), None };
static main = fn { let o$0 = Option::<usize>::Some(3); };
"#,
        r#"
```must
o: Option::<usize>
type Option = enum { Some(T), None }
```
"#,
    );
}

#[test]
fn hover_raw_pointer_binding_shows_the_pointer_type() {
    check_hover(
        "static main = fn { let mut x: usize = 1; let p$0 = x.&raw mut; };",
        "```must\np: usize.&raw mut\n```",
    );
}

#[test]
fn hover_deref_receiver_shows_the_pointer_type() {
    check_hover(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; unsafe { p$0.* } };",
        "```must\np: usize.&raw mut\n```",
    );
}

// ---- fixed-size arrays ----

#[test]
fn hover_array_local() {
    check_hover(
        r#"static main = fn { let a: [usize; 3] = [1, 2, 3]; let x = a$0[0]; };"#,
        "```must\na: [usize; 3]\n```",
    );
}

#[test]
fn hover_array_binding_definition() {
    check_hover(
        r#"static main = fn { let m$0: [[usize; 2]; 2] = [[1, 2], [3, 4]]; let x = m[0][1]; };"#,
        "```must\nm: [[usize; 2]; 2]\n```",
    );
}

#[test]
fn hover_unpinned_number_renders_as_number() {
    // A literal no defining use ever pinned hovers as `{number}` — never a
    // silently-defaulted type.
    check_hover(
        "static f = fn { let n$0 = 3; };",
        "```must\nn: {number}\n```",
    );
}

#[test]
fn hover_sized_integer_binding_shows_its_width() {
    check_hover("static f = fn { let n$0: u8 = 3; };", "```must\nn: u8\n```");
}

#[test]
fn hover_on_member_call_name_shows_the_member_signature() {
    check_hover(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).get$0() }};"
        ),
        "```must\nget: fn(Counter) -> usize\n```",
    );
}

#[test]
fn hover_on_member_name_declaration_shows_its_signature() {
    check_hover(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get$0 = fn(c: Self) -> usize { c.n };
    }
};
"#,
        "```must\nget: fn(Counter) -> usize\n```",
    );
}

#[test]
fn hover_on_a_member_with_its_own_type_binder_shows_the_binder() {
    // A member's OWN type parameter is part of its signature, and hover
    // renders it rigid — `U`, not a resolved type — because that is what
    // the DECLARATION says. (The owner's `T` reads the same way; the two
    // halves of the binder are indistinguishable here, deliberately.)
    check_hover(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to$0 = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
"#,
        "```must
map_to: fn(fn(T) -> U, Option::<T>) -> U
```",
    );
}

#[test]
fn hover_on_a_member_turbofish_call_shows_the_instantiated_signature() {
    // At the USE site the same member reads instantiated — the owner's
    // argument from the receiver, the member's own from the turbofish.
    check_hover(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static main = fn(o: Option::<usize>) -> bool { o.map_to$0::<bool>(width) };
"#,
        "```must
map_to: fn(fn(usize) -> bool, Option::<usize>) -> bool
```",
    );
}

#[test]
fn hover_inside_member_body_resolves_locals() {
    check_hover(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c$0.n };
    }
};
"#,
        "```must\nc: Counter\n```",
    );
}

// ---- trait declarations: hover and goto ---------------------------------

#[test]
fn hover_on_bound_shows_the_trait() {
    check_hover(
        r#"
trait Display = requires { fmt: fn(Self) -> str; };
static f = fn::<T: Disp$0lay>(x: T) -> usize { 1 };
"#,
        "```must\ntrait Display = requires { fmt: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_decl_name() {
    check_hover(
        r#"
trait Disp$0lay = requires { fmt: fn(Self) -> str; };
"#,
        "```must\ntrait Display = requires { fmt: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_impl_head_shows_the_trait() {
    check_hover(
        r#"
trait Show = requires { show: fn(Self) -> str; };
type P = struct { a: usize } with {
    impl Sh$0ow { show = fn(x: Self) -> str { "p" }; }
};
"#,
        "```must\ntrait Show = requires { show: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_member_dot_call_shows_instantiated_signature() {
    check_hover(
        r#"
trait D = requires { m: fn(Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { n.m$0() };
"#,
        "```must\nm: fn(usize) -> str\n```",
    );
}

#[test]
fn hover_on_a_requirement_with_bare_param_types() {
    check_hover(
        r#"
trait Sh$0ow = requires { show: fn(usize, Self) -> str; };
"#,
        r#"```must
trait Show = requires { show: fn(usize, Self) -> str; }
```"#,
    );
    check_hover(
        r#"
trait D = requires { m: fn(Self, usize) -> str; } with {
    impl usize { m = fn(x: usize, y: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { n.m$0(1) };
"#,
        r#"```must
m: fn(usize, usize) -> str
```"#,
    );
}

#[test]
fn hover_on_qualified_call_base_shows_the_trait() {
    check_hover(
        r#"
trait D = requires { m: fn(Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D$0::m(n) };
"#,
        "```must\ntrait D = requires { m: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_impl_member_name_shows_signature() {
    check_hover(
        r#"
trait D = requires { m: fn(Self) -> str; } with {
    impl usize { m$0 = fn(x: usize) -> str { "n" }; }
};
"#,
        "```must\nm: fn(usize) -> str\n```",
    );
}

// ---- match projects through borrows: what the editor shows --------------

#[test]
fn hover_on_a_borrowed_matchs_payload_binding_shows_the_borrow() {
    // The binding IS a borrow, and hover says so — including the flavor,
    // which is the thing a reader needs to know before writing `t.*`.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn::<@a>(s: Opt::<usize>.&mut::<@a>) -> () {
    match s { ::Some(t$0) => { t.* = 1; }, ::None => {} }
};
"#,
        "```must\nt: usize.&mut\n```",
    );
}

#[test]
fn hover_on_an_owned_matchs_payload_binding_still_shows_the_value() {
    // The control: the same enum matched by value binds the value.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn(s: Opt::<usize>) -> usize {
    match s { ::Some(t$0) => t, ::None => 0 }
};
"#,
        "```must\nt: usize\n```",
    );
}

#[test]
fn hover_on_a_whole_value_binder_through_a_borrow_shows_the_borrow() {
    // A whole-value binder is not a projection: it names the same place, so
    // it gets the scrutinee's own borrow back, region and all.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {
    match s { whole$0 => 0 }
};
"#,
        "```must\nwhole: Opt::<usize>.&::<@a>\n```",
    );
}

#[test]
fn hover_shows_a_region_in_the_declaration_and_not_at_the_call() {
    // The elision, as a reader sees it. A DECLARATION's regions are its
    // parameters and are rendered — `@a` is what a caller's borrow has to
    // satisfy. At a MENTION they have been instantiated to this call's own
    // existentials, which have no spelling and print as a bare borrow: the
    // hover never suggests a token the turbofish would refuse.
    check_hover(
        "static ge$0t = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };",
        "```must\nget: fn(T.&::<@a>) -> T\n```",
    );
    check_hover(
        "static get = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };\n\
         static main = fn::<@b>(p: usize.&::<@b>) -> usize { ge$0t(p) };",
        "```must\nget: fn(usize.&) -> usize\n```",
    );
}
