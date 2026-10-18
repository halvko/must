//! Broken programs trap where the checker's error lands, CFG checkers included.

use super::*;

/// A loan error traps where the refused operation runs, with the checker's
/// message: here the write to `n`, between the two prints.
#[test]
fn a_stale_borrow_traps_at_the_access_that_invalidates_it() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut n: usize = 1;
    let r = n.&;
    print("before;");
    n = 2;
    print("after;");
    r.*
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: writing to `n` here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
        "#]],
    );
}

/// An escape traps at the borrow: the nested literal's `n.&` is live
/// where the literal returns.
#[test]
fn an_escaping_borrow_traps_at_the_borrow() {
    check_run(
        r#"
static main = fn() -> usize {
    let f = fn() -> usize.&::<@_> {
        let n = 7;
        print("before;");
        n.&
    };
    f().*
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: borrowed value does not live long enough: this borrows a local, but the borrow is still live when the body returns and the local is gone by then
        "#]],
    );
}

/// A repointed place traps at the write that replaces its pointer.
#[test]
fn a_repointed_place_traps_at_the_write() {
    check_run(
        r#"
static main = fn() -> usize {
    let a = [1, 2];
    let b = [3, 4];
    let mut p = a.&;
    let q = b.&;
    print("before;");
    p.*[{ p = q; print("after;"); 0 }]
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: writing to `p` here replaces a pointer the place around it has already been read through: the access that place makes afterwards would go through the old pointer
        "#]],
    );
}

/// A pattern the lowering refuses (a string literal) traps the whole `match`
/// before it runs, instead of dropping the arm.
#[test]
fn an_unsupported_literal_pattern_traps_the_match() {
    check_run(
        r#"
static f = fn() -> usize {
    print("before;");
    match "x" { "x" => 1, _ => 2 }
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: string literal patterns are not supported yet; only character (`'x'`) and integer (`0`) literals can be matched
        "#]],
    );
}

/// An unknown type in a `let` annotation traps when the `let` runs.
#[test]
fn an_unknown_type_in_a_let_annotation_traps_at_the_let() {
    check_run(
        r#"
static f = fn() -> usize {
    print("before;");
    let x: Foo = 5;
    x + 1
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: unknown type `Foo`
        "#]],
    );
}

/// A `let` destructuring that names a field the value lacks traps at the
/// `let`.
#[test]
fn a_broken_let_destructuring_traps_at_the_let() {
    check_run(
        r#"
static f = fn() -> usize {
    print("before;");
    let struct { y } = struct { x: usize = 1 };
    y
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: no field `y` on `struct { x: usize }`
        "#]],
    );
}

/// A parameter destructuring that names a field the argument lacks traps
/// when the function is created.
#[test]
fn a_broken_parameter_destructuring_traps() {
    check_run(
        r#"
static f = fn(struct { y }: struct { x: usize }) -> usize { y };
"#,
        r#"f(struct { x = 1 })"#,
        expect![[r#"
            error[Trap]: no field `y` on `struct { x: usize }`
        "#]],
    );
}

/// An impl member whose signature does not match its requirement traps
/// when called through a bound, instead of handing back the wrong type.
#[test]
fn a_mismatched_impl_member_traps_through_a_bound() {
    check_run(
        r#"
trait D = requires { f: fn(Self) -> usize; } with {
    impl usize { f = fn(x: usize) -> str { "oops" }; }
};
static g = fn::<T: D>(x: T) -> usize { x.f() + 1 };
static main = fn() -> usize {
    print("before;");
    let n: usize = 1;
    g(n)
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: member `f` does not match `D`'s requirement: expected `fn(usize) -> usize`, found `fn(usize) -> str`
        "#]],
    );
}

/// A call through a bound traps with the trait's error when an `unsafe fn`
/// requirement leaves the impl without its member.
#[test]
fn a_reserved_unsafe_requirement_traps_through_a_bound() {
    check_run(
        r#"
trait D = requires { f: unsafe fn(Self) -> usize; } with {
    impl usize { f = unsafe fn(x: usize) -> usize { x }; }
};
static g = fn::<T: D>(x: T) -> usize { unsafe { x.f() } + 1 };
static main = fn() -> usize {
    print("before;");
    let n: usize = 1;
    g(n)
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: `unsafe` trait members are not supported yet
        "#]],
    );
}

/// A call through a bound traps with the trait's first error when both the
/// trait and the impl are broken.
#[test]
fn a_non_fn_requirement_traps_through_a_bound() {
    check_run(
        r#"
trait D = requires { f: usize; } with {
    impl usize { f = 3; }
};
static g = fn::<T: D>(x: T) -> usize { x.f() + 1 };
static main = fn() -> usize {
    print("before;");
    let n: usize = 1;
    g(n)
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: a requirement's signature must be an `fn` signature
        "#]],
    );
}

/// An impl missing a member traps a call that reaches it through a bound,
/// even when nothing in the body names the impl's home.
#[test]
fn a_missing_impl_member_traps_through_a_bound() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; } with { impl usize { } };
static g = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> usize {
    print("before;");
    let n: usize = 1;
    g(n)
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: this impl of `D` is missing the member `m`
        "#]],
    );
}

/// A member without its full signature traps when a dot-call reaches it.
#[test]
fn a_member_without_a_full_signature_traps_at_the_call() {
    check_run(
        r#"
type T = struct { a: usize } with {
    impl Self { f = fn(p: Self) { print("in f;"); }; }
};
static main = fn() {
    print("before;");
    let t = T(struct { a = 1 });
    t.f();
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: member `f` must spell its full signature: every parameter and the return type
        "#]],
    );
}

/// A linear value left live on one branch traps where its storage ends,
/// after everything that path runs.
#[test]
fn a_linear_left_live_on_one_branch_traps_where_its_scope_ends() {
    check_run(
        r#"
type L = struct { x: usize } only move;
static eat = fn(l: L) { let L(struct { x }) = l; };
static f = fn(c: bool) {
    let t = L(struct { x = 1 });
    if c { eat(t); } else { print("leak;"); }
    print("after;");
};
"#,
        "f(false)",
        expect![[r#"
            output: "leak;after;"
            error[Trap]: `t` is not consumed on this path; its type has no `forget` capability, so every path must consume it
        "#]],
    );
}

/// A refused `t;` on one branch leaves the other branch's leak trapped.
#[test]
fn a_linear_skipping_a_refused_discard_traps_where_its_scope_ends() {
    check_run(
        r#"
type L = struct { x: usize } only move;
static f = fn(c: bool) {
    let t = L(struct { x = 1 });
    if c { t; } else { print("leak;"); }
    print("after;");
};
"#,
        "f(false)",
        expect![[r#"
            output: "leak;after;"
            error[Trap]: `t` is not consumed on this path; its type has no `forget` capability, so every path must consume it
        "#]],
    );
}

/// A value a non-exhaustive match produced on a covered arm leaks there.
#[test]
fn a_linear_from_a_covered_arm_of_a_nonexhaustive_match_traps_where_its_scope_ends() {
    check_run(
        r#"
type L = struct { x: usize } only move;
type Shape = enum { Circle(usize), Point };
static f = fn(s: Shape) {
    let r = match s { ::Circle(n) => L(struct { x = n }) };
    print("after;");
};
"#,
        "f(Shape::Circle(1))",
        expect![[r#"
            output: "after;"
            error[Trap]: `r` is not consumed on this path; its type has no `forget` capability, so every path must consume it
        "#]],
    );
}

/// A `return` that cuts a call short abandons the argument already made.
#[test]
fn a_linear_argument_abandoned_by_a_return_traps_where_the_frame_ends() {
    check_run(
        r#"
type L = struct { x: usize } only move;
static make = fn() -> L { print("made;"); L(struct { x = 1 }) };
static take = fn(l: L, n: usize) { let L(struct { x }) = l; };
static f = fn() {
    take(make(), return);
};
"#,
        "f()",
        expect![[r#"
            output: "made;"
            error[Trap]: this value is not consumed on this path; its type has no `forget` capability, so every path must consume it
        "#]],
    );
}

/// A borrow of a local that escapes its body traps before it is returned.
#[test]
fn an_escaping_borrow_traps_before_it_leaves() {
    check_run(
        r#"
static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> {
    let n: usize = 7;
    n.&
};
static main = fn() -> usize {
    print("before;");
    let m: usize = 1;
    f(m.&).*
};
"#,
        "main()",
        expect![[r#"
            output: "before;"
            error[Trap]: borrowed value does not live long enough: this borrows a local, but the borrow has to last for `@a`, which outlives the body
        "#]],
    );
}

/// A missing `;` traps the statement it belongs to, before it runs.
#[test]
fn a_missing_semicolon_traps_the_statement() {
    check_run(
        r#"
static f = fn() {
    print("a;");
    print("b;") print("c;");
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: expected `;`
        "#]],
    );
}

/// A stray character between statements traps before the next one runs.
#[test]
fn a_stray_character_traps_before_the_next_statement() {
    check_run(
        r#"
static f = fn() {
    print("a;");
    $ print("b;");
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: unexpected character `$`
        "#]],
    );
}

/// An error in a `type` declaration traps where the type is named, not
/// before.
#[test]
fn a_broken_type_traps_where_it_is_named() {
    check_run(
        r#"
type P = struct { x: usize, pub y: usize };
static f = fn() -> usize {
    print("before;");
    let p = P(struct { x = 1, y = 2 });
    p.x
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: field visibility is not supported yet
        "#]],
    );
}

/// An item that never names a broken declaration runs normally.
#[test]
fn an_item_that_does_not_name_a_broken_type_runs() {
    check_run(
        r#"
type P = struct { x: usize, pub y: usize };
static f = fn() -> usize { 3 };
"#,
        "f()",
        expect![[r#"
            => 3
        "#]],
    );
}

/// An error in a trait traps a function bounded by it when the function's
/// value is read.
#[test]
fn a_broken_trait_traps_a_function_bounded_by_it() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static g = fn::<T: D>(t: T) -> usize { 1 };
static f = fn() -> usize {
    print("before;");
    let n: usize = 5;
    g(n)
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: duplicate requirement `m`
        "#]],
    );
}

/// A missing impl member traps where the implementing type is named.
#[test]
fn a_missing_impl_member_traps_where_the_type_is_named() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { x: usize } with { impl D { } };
static f = fn() -> usize {
    print("before;");
    P(struct { x = 1 }).x
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: this impl of `D` is missing the member `m`
        "#]],
    );
}

/// An error outside every item traps every item's value.
#[test]
fn an_error_outside_every_item_traps_every_item() {
    check_run(
        r#"
static f = fn() -> usize { 3 };
\
"#,
        "f()",
        expect![[r#"
            error[Trap]: unexpected character `\`
        "#]],
    );
}

/// A `static` with no value traps where it is read.
#[test]
fn a_static_without_a_value_traps_where_it_is_read() {
    check_run(
        r#"
static s: usize;
static f = fn() -> usize {
    print("before;");
    s + 1
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: expected `=` followed by the item's value
        "#]],
    );
}

/// A broken enum traps at a variant path naming it.
#[test]
fn a_broken_enum_traps_at_a_variant_path() {
    check_run(
        r#"
type E = enum { A, B(_) };
static f = fn() -> usize {
    print("before;");
    let e = E::A;
    1
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: a variant payload must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

/// A broken trait traps at a qualified call through it.
#[test]
fn a_broken_trait_traps_at_a_qualified_call() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn() -> usize {
    print("before;");
    let n: usize = 5;
    D::m(n)
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: duplicate requirement `m`
        "#]],
    );
}

/// A broken trait traps at a dot-call that reaches one of its impls.
#[test]
fn a_broken_trait_traps_at_a_dot_call() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn() -> usize {
    print("before;");
    let n: usize = 5;
    n.m()
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: duplicate requirement `m`
        "#]],
    );
}

/// A broken trait traps at a dot-call on a type that implements it.
#[test]
fn a_broken_trait_traps_at_a_dot_call_on_its_implementer() {
    check_run(
        r#"
trait D = requires { m: fn(Self) -> usize; m: fn(Self) -> usize; };
type P = struct { v: usize } with {
    impl D { m = fn(x: P) -> usize { 1 }; }
};
static f = fn() -> usize {
    print("before;");
    P(struct { v = 2 }).m()
};
"#,
        "f()",
        expect![[r#"
            output: "before;"
            error[Trap]: duplicate requirement `m`
        "#]],
    );
}

/// An unknown type in a construction's turbofish traps at the
/// construction.
#[test]
fn an_unknown_type_in_a_construction_turbofish_traps() {
    check_run(
        r#"
type Pair = struct::<T> { a: T, b: T };
static f = fn() -> usize {
    print("a;");
    let p = Pair::<Bogus>(struct { a = 1, b = 2 });
    print("b;");
    3
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: unknown type `Bogus`
        "#]],
    );
}

/// An unknown type in a variant path's turbofish traps at the variant.
#[test]
fn an_unknown_type_in_a_variant_turbofish_traps() {
    check_run(
        r#"
type O = enum::<T> { Some(T), None };
static f = fn() -> usize {
    print("a;");
    let o = O::<Bogus>::Some(1);
    print("b;");
    3
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: unknown type `Bogus`
        "#]],
    );
}

/// A parse error on a link of a borrowed place traps at the borrow.
#[test]
fn a_broken_field_in_a_borrowed_place_traps() {
    check_run(
        r#"
type P = struct { a: usize, b: usize };
static f = fn() -> usize {
    let p = P(struct { a = 1, b = 2 });
    print("a;");
    let r = (p.).&;
    print("b;");
    3
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: expected a field name after `.`
        "#]],
    );
}

/// A parse error on a link of an assignment target traps at the write.
#[test]
fn a_broken_field_in_an_assignment_target_traps() {
    check_run(
        r#"
static f = fn() -> usize {
    let mut p = struct { a: usize = 1 };
    print("a;");
    p. = 5;
    print("b;");
    3
};
"#,
        "f()",
        expect![[r#"
            output: "a;"
            error[Trap]: expected a field name after `.`
        "#]],
    );
}
