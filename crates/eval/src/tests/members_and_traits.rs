//! Dot-calls, members and trait dispatch.

use super::*;

// ---- inherent members and dot-calls -------------------------------------

#[test]
fn dot_call_runs_as_a_direct_call() {
    check_run(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Counter(struct { n = c.n + by }) };
    }
};
"#,
        "Counter(struct { n = 3 }).bump(4).get()",
        expect![[r#"
            => 7
        "#]],
    );
}

// THE TR01 evaluation-order pin: `recv.name(a)` desugars to `name(a, recv)`,
// and arguments evaluate left to right — so the ARGUMENT runs BEFORE the
// receiver expression binds its value. `bump` mutates the cell through a
// raw pointer while the receiver expression reads through the same
// pointer: argument-first order sees the bumped value (99), receiver-first
// would have seen 1. This order is WHY self is the last parameter.
#[test]
fn dot_call_arguments_evaluate_before_the_receiver_binds() {
    check_run(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        plus = fn(extra: usize, c: Self) -> usize { c.v + extra };
    }
};
static bump = fn(p: Cell.&raw mut) -> usize {
    unsafe { p.* = Cell(struct { v = 99 }); };
    0
};
static main = fn() -> usize {
    let mut c = Cell(struct { v = 1 });
    let p = c.&raw mut;
    unsafe { p.* }.plus(bump(p))
};
"#,
        "main()",
        expect![[r#"
            => 99
        "#]],
    );
}

#[test]
fn generic_member_dispatches_at_the_receiver_args() {
    check_run(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { let Box2(struct { v }) = b; v };
        rewrap = fn(b: Self) -> Self { let Box2(struct { v }) = b; Box2::<T>(struct { v = v }) };
    }
};
"#,
        r#"Box2(struct { v = "ho" }).rewrap().get()"#,
        expect![[r#"
            => "ho"
        "#]],
    );
}

// A const-generic owner: the member reads the binder's `N`, supplied by
// the RECEIVER's type (the owner's arguments, which a dot-call never
// spells) — and one member forwards it to another through a dot-call on
// `Self`.
#[test]
fn const_generic_member_reads_the_receivers_const_arg() {
    check_run(
        r#"
type Buf = struct::<const N: usize> { used: usize } with {
    impl Self {
        cap = fn(b: Self) -> usize { N };
        free = fn(b: Self) -> usize { b.cap() - b.used };
    }
};
"#,
        "Buf::<8>(struct { used = 3 }).free()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn a_constructed_variant_receiver_reaches_the_member() {
    check_run(
        r#"
type Light = enum { Red, Green } with {
    impl Self {
        flip = fn(l: Self) -> Light {
            match l {
                ::Red => Light::Green,
                ::Green => Light::Red,
            }
        };
    }
};
"#,
        "Light::Red.flip()",
        expect![[r#"
            => Light::Green
        "#]],
    );
}

#[test]
fn a_constructed_variant_reaches_a_borrowing_member() {
    // `o` is unannotated: `Option::<usize>::Some(11)` has the enum's type,
    // so `o.&` is exactly the `Self.&` the member takes.
    check_run(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        is_some = fn::<@local>(s: Self.&::<@local>) -> bool {
            match s { ::Some(_) => true, ::None => false, }
        }
    }
};
static main = fn () -> () {
    let o = Option::<usize>::Some(11);
    if o.&.is_some() { print("some"); }
};
"#,
        "main()",
        expect![[r#"
            output: "some"
            => ()
        "#]],
    );
}

// A `const fn` member is const-callable; a plain member is rejected in a
// const context with the ordinary const-check story.
#[test]
fn const_fn_members_run_at_compile_time() {
    check_const(
        r#"
type Sq = struct { n: usize } with {
    impl Self {
        area = const fn(s: Self) -> usize { s.n * s.n };
    }
};
static a = const { Sq(struct { n = 5 }).area() };
"#,
        expect![[r#"
            a = 25
        "#]],
    );
}

#[test]
fn plain_member_call_rejected_in_const_context() {
    check_const(
        r#"
type Sq = struct { n: usize } with {
    impl Self {
        area = fn(s: Self) -> usize { s.n * s.n };
    }
};
static a = const { Sq(struct { n = 5 }).area() };
"#,
        expect![[r#"
            a = error[Trap]: cannot call `area` in a const context; marking it `const fn` would allow this
        "#]],
    );
}

// SEPARATE NAMESPACES (G13): call syntax runs the MEMBER, bare
// access reads the FIELD — the getter idiom end to end at runtime.
#[test]
fn member_shadowing_a_field_dispatches_by_syntax() {
    check_run(
        r#"
type Vecish = struct { len: usize } with {
    impl Self {
        len = fn(v: Self) -> usize { v.len + 10 };
    }
};
"#,
        "Vecish(struct { len = 3 }).len() + Vecish(struct { len = 3 }).len",
        expect![[r#"
            => 16
        "#]],
    );
}

// ---- trait declarations: dictionary dispatch ----------------------------

#[test]
fn trait_dispatch_through_generic_fn() {
    // One bounded generic fn, three implementers: trait-side impls (str,
    // usize) and a type-side impl (Point) behave identically — the
    // dictionary resolves per call site at the instantiation edge.
    check_run(
        r#"
trait Write = requires {
    push: fn(str, Self) -> Self;
};
trait Display = requires {
    fmt: fn::<W: Write>(W, Self) -> W;
} with {
    impl str {
        fmt = fn::<W: Write>(w: W, x: str) -> W { w.push(x) };
    }
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("num") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self {
            print(s);
            Sink(struct { pushes = w.pushes + 1 })
        };
    }
};
type Point = struct { x: usize, y: usize } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, p: Self) -> W {
            let w = Display::fmt(w, p.x);
            let w = w.push(",");
            Display::fmt(w, p.y)
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn() -> usize {
    let a = show::<str>("hi");
    let b = show::<usize>(7);
    let c = show::<Point>(Point(struct { x = 1, y = 2 }));
    a + b + c
};
"#,
        "main()",
        expect![[r#"
            output: "hinumnum,num"
            => 5
        "#]],
    );
}

#[test]
fn trait_dispatch_through_requirements_with_bare_param_types() {
    check_run(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; };
trait Display = requires { fmt: fn::<W: Write>(W, Self) -> W; } with {
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("num") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self {
            print(s);
            Sink(struct { pushes = w.pushes + 1 })
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = x.fmt(Sink(struct { pushes = 0 }));
    s.pushes
};
static main = fn() -> usize {
    let n: usize = 2;
    show::<usize>(7) + Display::fmt(Sink(struct { pushes = 1 }), n).pushes
};
"#,
        "main()",
        expect![[r#"
            output: "numnum"
            => 3
        "#]],
    );
}

#[test]
fn dictionary_forwarding_through_recursion() {
    // `fmt_usize` recurses (forwarding its own dictionary) and is called
    // from an impl member (forwarding the member's binder dictionary).
    check_run(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; };
trait Display = requires { fmt: fn::<W: Write>(W, Self) -> W; } with {
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { fmt_usize(w, x) };
    }
};
static digit = fn(d: usize) -> str {
    if d == 0 { "0" } else if d == 1 { "1" } else if d == 2 { "2" }
    else if d == 3 { "3" } else if d == 4 { "4" } else { "5+" }
};
static fmt_usize = fn::<W: Write>(w: W, n: usize) -> W {
    if n < 10 {
        w.push(digit(n))
    } else {
        let w = fmt_usize(w, n / 10);
        w.push(digit(n - (n / 10) * 10))
    }
};
type Out = struct { c: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self { print(s); Out(struct { c = w.c + 1 }) };
    }
};
static main = fn() -> usize {
    let n: usize = 431;
    let o = n.fmt(Out(struct { c = 0 }));
    o.c
};
"#,
        "main()",
        expect![[r#"
            output: "431"
            => 3
        "#]],
    );
}

#[test]
fn qualified_short_form_dispatches() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "int" }; }
    impl str { m = fn(x: str) -> str { x }; }
};
static main = fn() -> () {
    let n: usize = 3;
    print(D::m(n));
    print(D::m("qq"));
    print(n.m());
};
"#,
        "main()",
        expect![[r#"
            output: "intqqint"
            => ()
        "#]],
    );
}

#[test]
fn collision_escapes_each_run_to_their_own_member() {
    // G13: every escape the ambiguity error names must actually reach the
    // thing it names — inherent, trait impl, fn-typed field.
    check_run(
        r#"
trait D = requires { m: fn(Self) -> str; };
type P = struct { m: fn() -> str } with {
    impl Self { m = fn(x: Self) -> str { "inherent" }; }
    impl D { m = fn(x: Self) -> str { "trait" }; }
};
static main = fn() -> () {
    let p = P(struct { m = fn() -> str { "field" } });
    print(P::m(p));
    print(D::m(p));
    print(D::<Self = P>::m(p));
    print((p.m)());
};
"#,
        "main()",
        expect![[r#"
            output: "inherenttraittraitfield"
            => ()
        "#]],
    );
}

#[test]
fn collision_call_traps_with_the_ambiguity_message() {
    // Deferred-error mode: the refused call carries the squiggle's text.
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { v: usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static main = fn() -> usize { P(struct { v = 0 }).m() };
"#,
        "main()",
        expect![[r#"
            error[Trap]: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`) or `D`'s member (`D::m(value)`) — spell the one you mean
        "#]],
    );
}

#[test]
fn qualified_inherent_member_runs_as_a_plain_fn_value() {
    // `Type::member` is an ordinary fn value: callable directly, bindable,
    // and instantiated at the TYPE's arguments when the owner is generic.
    check_run(
        r#"
type Pair = struct::<T> { a: T, b: usize } with {
    impl Self { first = fn(p: Self) -> T { let Pair(struct { a, b }) = p; a }; }
};
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static main = fn() -> usize {
    let p = P(struct { v = 7 });
    let f = P::len;
    let q = Pair::<usize>(struct { a = 5, b = 6 });
    f(p) + P::len(p) + Pair::<usize>::first(q)
};
"#,
        "main()",
        expect![[r#"
            => 19
        "#]],
    );
}

#[test]
fn a_member_own_type_param_runs_at_every_spelling() {
    // The member-own TYPE binder end to end: inferred at the call,
    // spelled on the dot, and spelled on the qualified path — the same
    // instantiation, three ways, all of them running.
    check_run(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        unwrap = const fn(s: Self) -> T {
            match s { ::Some(t) => t, ::None => panic("none") }
        }
        flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self) -> Option::<U> {
            match s {
                ::Some(v) => { f(v) }
                ::None => { Option::None }
            }
        }
    }
};
static twice = fn(n: usize) -> Option::<usize> { Option::Some(n + n) };
static label = fn(n: usize) -> Option::<str> { Option::Some("hit") };
static main = fn() -> usize {
    let inferred = Option::Some(3).flat_map(twice).unwrap();
    let dotted = Option::Some(4).flat_map::<usize>(twice).unwrap();
    let pathed = Option::unwrap(Option::flat_map::<usize>(twice, Option::Some(5)));
    // A SECOND instantiation of the same member, at a different `U`: the
    // member's own binder varies per call, which is the whole point.
    print(Option::Some(1).flat_map(label).unwrap());
    inferred + dotted + pathed
};
"#,
        "main()",
        expect![[r#"
            output: "hit"
            => 24
        "#]],
    );
}

#[test]
fn named_self_call_and_value_dispatch() {
    // The full named-Self form states `Self` where nothing infers it, and
    // its member VALUE is one impl's fn.
    check_run(
        r#"
trait D = requires { n: fn(usize) -> Self; m: fn(Self) -> str; } with {
    impl usize { n = fn(k: usize) -> usize { k + 1 }; m = fn(x: usize) -> str { "int" }; }
    impl str { n = fn(k: usize) -> str { "s" }; m = fn(x: str) -> str { x }; }
};
static generic = fn::<T: D>(x: T) -> str { D::<Self = T>::m(x) };
static main = fn() -> () {
    print(D::<Self = str>::n(1));
    let f = D::<Self = usize>::m;
    print(f(D::<Self = usize>::n(1)));
    print(generic("rigid"));
};
"#,
        "main()",
        expect![[r#"
            output: "sintrigid"
            => ()
        "#]],
    );
}

#[test]
fn unsatisfied_bound_traps_at_runtime() {
    // Deferred-error mode: the broken call traps with the squiggle's text
    // when it actually runs.
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> usize { f::<P>(P(struct { a = 1 })) };
"#,
        "main()",
        expect![[r#"
            error[Trap]: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

#[test]
fn trait_impls_on_enums_and_constructed_variant_receivers() {
    // A constructed variant is a value of its ENUM, so it reaches the
    // enum's trait impl — dot-form and qualified short form alike.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
trait D = requires { m: fn(Self) -> usize; } with {
    impl Shape {
        m = fn(x: Self) -> usize {
            match x { ::Circle(r) => r, ::Point => 0 }
        };
    }
};
static main = fn() -> () {
    let c = Shape::Circle(3);
    if c.m() == 3 { print("dot ok") } else { print("bad") };
    if D::m(Shape::Point) == 0 { print("qualified ok") } else { print("bad") };
};
"#,
        "main()",
        expect![[r#"
            output: "dot okqualified ok"
            => ()
        "#]],
    );
}

#[test]
fn qualified_variant_self_resolves_to_the_enum_and_dispatches() {
    // A constructed variant at a `Self` position has its enum's type, so
    // the impl's returned tag is real and the match dispatches honestly.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point } with {
    impl Id { id = fn(x: Self) -> Self { Shape::Point }; }
};
trait Id = requires { id: fn(Self) -> Self; };
static main = fn() -> () {
    let c = Id::id(Shape::Circle(3));
    match c {
        ::Circle(n) => { let _ = n; print("circle") },
        ::Point => print("point — correct"),
    };
};
"#,
        "main()",
        expect![[r#"
            output: "point — correct"
            => ()
        "#]],
    );
}

#[test]
fn nested_bound_use_traps_with_the_reservation() {
    // A check-time reservation and its deferred trap carry the same
    // text at runtime (never an internal error).
    check_run(
        r#"
trait Size = requires { size: fn(Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static outer = fn::<T: Size>(x: T) -> usize {
    let f = fn(y: T) -> usize { y.size() };
    f(x)
};
static main = fn() -> usize { outer::<usize>(4) };
"#,
        "main()",
        expect![[r#"
            error[Trap]: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}
