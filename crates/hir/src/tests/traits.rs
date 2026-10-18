//! Traits: declarations, impls, bounds, resolution and named `Self`.

use super::*;

#[test]
fn trait_fixture_is_clean() {
    check_diagnostics(TRAIT_FIXTURE, expect![[r#""#]]);
}

#[test]
fn duplicate_impl_names_both_sites() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { 1 }; }
    impl usize { m = fn(x: usize) -> usize { 2 }; }
};
"#,
        expect![[r#"
            114..119: duplicate impl of `D` for `usize` (first implemented here at 62..67)
            122..123: duplicate member `m` (first defined here at 70..71)
        "#]],
    );
}

#[test]
fn duplicate_impl_across_homes_names_both_sites() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl P { m = fn(x: Self) -> usize { 1 }; }
};
type P = struct { a: usize } with {
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
"#,
        expect![[r#"
            148..149: duplicate impl of `D` for `P` (first implemented here at 62..63)
        "#]],
    );
}

#[test]
fn impl_missing_and_extra_members() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { extra = fn(x: usize) -> usize { 1 }; }
};
"#,
        expect![[r#"
            62..67: this impl of `D` is missing the member `m` (required by the trait here at 22..23)
            70..75: `D` has no requirement `extra` (declared here at 7..8)
        "#]],
    );
}

#[test]
fn impl_member_signature_mismatch() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> str { "s" }; }
};
"#,
        expect![[r#"
            70..71: member `m` does not match `D`'s requirement: expected `fn(usize) -> usize`, found `fn(usize) -> str` (required by the trait here at 22..23)
        "#]],
    );
}

#[test]
fn impl_member_binder_mismatch() {
    check_diagnostics(
        r#"
trait W = requires { p: fn(Self) -> usize; } with {
    impl usize { p = fn(x: usize) -> usize { 1 }; }
};
trait D = requires { m: fn::<X: W>(X, Self) -> usize; } with {
    impl usize { m = fn::<X: forget>(v: X, x: usize) -> usize { 1 }; }
};
"#,
        // TODO: halvko/must#163: bounds may come in any order; the message stops naming one.
        expect![[r#"
            188..189: member `m`'s generic binder does not match `D`'s requirement (arity, kinds and bounds must agree, the bounds in the requirement's order) (required by the trait here at 129..130)
        "#]],
    );
}

/// Bounds pass their dictionaries in written order, so a member that
/// reorders its requirement's bounds would read each other's dictionaries.
#[test]
fn impl_member_binder_with_reordered_bounds() {
    check_diagnostics(
        r#"
trait A = requires { a: fn(Self) -> usize; };
trait B = requires { b: fn(Self) -> usize; };
trait D = requires { m: fn::<X: A + B>(X, Self) -> usize; } with {
    impl usize { m = fn::<X: B + A>(v: X, x: usize) -> usize { 1 }; }
};
"#,
        // TODO: halvko/must#163: bounds may come in any order, and this impl is accepted.
        expect![[r#"
            177..178: member `m`'s generic binder does not match `D`'s requirement (arity, kinds and bounds must agree, the bounds in the requirement's order) (required by the trait here at 114..115)
        "#]],
    );
}

#[test]
fn unknown_trait_in_bound_and_impl_head() {
    check_diagnostics(
        r#"
static f = fn::<T: Nope + forget>(x: T) -> usize { 1 };
type P = struct { a: usize } with {
    impl Missing { m = fn(x: Self) -> usize { 1 }; }
};
"#,
        expect![[r#"
            20..24: unknown trait `Nope`
            102..109: unknown trait `Missing`
        "#]],
    );
}

#[test]
fn bound_naming_a_type_is_not_a_trait() {
    check_diagnostics(
        r#"
type P = struct { a: usize };
static f = fn::<T: P + forget>(x: T) -> usize { 1 };
static g = fn::<T: usize + forget>(x: T) -> usize { 1 };
"#,
        expect![[r#"
            50..51: `P` is not a trait
            103..108: `usize` is not a trait
        "#]],
    );
}

#[test]
fn trait_side_impl_head_resolution_errors() {
    check_diagnostics(
        r#"
trait D = requires { } with {
    impl Unknown { };
    impl D { };
};
type V = struct::<T> { a: T };
trait E = requires { } with {
    impl V { };
};
"#,
        expect![[r#"
            40..47: unknown type `Unknown`
            62..63: `D` is a trait; an impl in a trait's `with`-chain names the IMPLEMENTING type
            142..143: impls for generic types are not supported yet
        "#]],
    );
}

#[test]
fn unsatisfied_bound_at_instantiation() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let a = f::<P>(P(struct { a = 1 }));
    let _ = a;
};
"#,
        expect![[r#"
            164..170: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

#[test]
fn unsatisfied_bound_on_structural_type() {
    // Structural types implement nothing (TR03: impls attach to nominal
    // types only), so a record can never satisfy a bound.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let a = f(struct { q: usize = 1 });
    let _ = a;
};
"#,
        expect![[r#"
            134..135: the bound `T: D` is not satisfied here: `struct { q: usize }` does not implement `D`
        "#]],
    );
}

#[test]
fn bound_forwarding_requires_the_bound() {
    // `g` forwards its unbounded `T` into `f`'s bounded param: the bound
    // is unsatisfied AT g's call of f — bodies check against bounds.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { x.m() };
static g = fn::<T>(x: T) -> usize { f(x) };
"#,
        expect![[r#"
            192..193: the bound `T: D` is not satisfied here: `T` does not implement `D`
        "#]],
    );
}

#[test]
fn ambiguous_member_and_qualified_disambiguation() {
    check_diagnostics(
        r#"
trait A = requires { m: fn(Self) -> usize; } with { impl usize { m = fn(x: usize) -> usize { 1 }; } };
trait B = requires { m: fn(Self) -> usize; } with { impl usize { m = fn(x: usize) -> usize { 2 }; } };
static main = fn() -> () {
    let n: usize = 4;
    let x = n.m();
    let y = A::m(n);
    let _ = x + y;
};
"#,
        expect![[r#"
            268..273: `m` is ambiguous on `usize`: it could be `A`'s member (`A::m(value)`) or `B`'s member (`B::m(value)`) — spell the one you mean (`A::m` for `usize` is defined here at 66..67) (`B::m` for `usize` is defined here at 169..170)
        "#]],
    );
}

#[test]
fn trait_member_shadows_field_under_call_syntax() {
    // `s.m()` picks the trait member; bare `s.m` stays the field.
    check_infer(
        r#"
trait D = requires { m: fn(Self) -> str; };
type S = struct { m: usize } with {
    impl D { m = fn(x: Self) -> str { "member" }; }
};
static f = fn(s: S) -> str { let a = s.m; s.m() };
"#,
        expect![[r#"
            147..185 'fn(s: S) -> str {...': fn(S) -> str
            150..151 's': S
            163..185 '{ let a = s.m; s....': str
            169..170 'a': usize
            173..174 's': S
            173..176 's.m': usize
            178..179 's': S
            178..181 's.m': fn(S) -> str
            178..183 's.m()': str
        "#]],
    );
}

#[test]
fn trait_names_are_not_values_or_types() {
    check_diagnostics(
        r#"
trait D = requires { };
static x = fn() -> usize { let a = D; 1 };
static y = fn(p: D) -> usize { 1 };
"#,
        expect![[r#"
            60..61: `D` is a trait, not a value
            85..86: `D` is a trait; traits are bounds, not types — did you mean a bounded generic param (`T: D`)?
        "#]],
    );
}

#[test]
fn bare_trait_member_value_needs_the_implementer() {
    // A member VALUE is impl-specific, so a bare `D::m` names no one
    // function — the fix is the named-Self form. A trait's own POSITIONAL
    // arguments are reserved (generic traits): a non-generic trait takes none.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static a = fn() -> usize { let f = D::m; 1 };
static b = fn() -> usize { let n: usize = 1; D::<usize>::m(n) };
"#,
        expect![[r#"
            143..147: a member value is impl-specific, so it must name the implementer: `D::<Self = Type>::m`
            199..212: `D` takes no generic arguments
        "#]],
    );
}

#[test]
fn qualified_call_self_resolution_errors() {
    // A rigid Self without the bound is an unsatisfied bound; a Self no
    // argument determines cannot be inferred by the SHORT form (the
    // named-Self form spells it — see `named_self_call_pins_self`).
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; n: fn(usize) -> Self; } with {
    impl usize { m = fn(x: usize) -> usize { x }; n = fn(k: usize) -> usize { k }; }
};
static a = fn::<T>(x: T) -> usize { D::m(x) };
static b = fn() -> usize { let v = D::n(3); 1 };
"#,
        expect![[r#"
            199..206: the bound `T: D` is not satisfied here: `T` does not implement `D`
            245..252: cannot infer `Self` for `D::n`: no argument determines the implementing type — annotate an argument
        "#]],
    );
}

#[test]
fn bound_fn_value_reserved() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let g = f::<usize>;
    let _ = g(1);
};
"#,
        expect![[r#"
            195..205: `f` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn requirement_params_are_bare_types() {
    // A requirement's signature is a type position with bare parameter
    // types. Impl conformance, `Self`-parameter receiver detection (both the
    // concrete and the bound-directed dot-call) and the region binder all
    // read the types alone.
    check_diagnostics(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; };
trait Display = requires {
    fmt: fn::<@a, @b, W: Write>(W.&mut::<@a>, Self.&::<@b>) -> ();
} with {
    impl usize {
        fmt = fn::<@a, @b, W: Write>(w: W.&mut::<@a>, x: usize.&::<@b>) -> () {};
    }
};
type Sink = struct { n: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self { Sink(struct { n = w.n + 1 }) };
    }
};
static direct = fn(s: Sink) -> Sink { s.push("x") };
static generic = fn::<W: Write>(w: W) -> W { Write::push("x", w) };
static shown = fn::<@z, @y, T: Display>(w: Sink.&mut::<@z>, t: T.&::<@y>) -> () {
    t.fmt(w)
};
"#,
        expect![[r#""#]],
    );
    // Conformance compares the types position by position.
    check_diagnostics(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; };
type Sink = struct { n: usize } with {
    impl Write { push = fn(s: usize, w: Self) -> Self { w }; }
};
"#,
        expect![[r#"
            114..118: member `push` does not match `Write`'s requirement: expected `fn(str, Sink) -> Sink`, found `fn(usize, Sink) -> Sink` (required by the trait here at 26..30)
        "#]],
    );
}

#[test]
fn requirement_rules() {
    check_diagnostics(
        r#"
trait D = requires {
    m: fn(Self) -> usize;
    m: fn(Self) -> str;
    n: fn(_, usize) -> usize;
    o: fn(Self);
    p: fn -> usize;
};
"#,
        expect![[r#"
            52..53: duplicate requirement `m`
            76..77: requirement `n` must spell its full signature: every parameter's type and the return type
            106..107: requirement `o` must spell its full signature: every parameter's type and the return type
            129..131: expected `(`: function types are written `fn(...) -> ...`
        "#]],
    );
    // A parameter with no type is reported by syntax, not again here.
    check_diagnostics(
        r#"
trait D = requires {
    m: fn(x: ) -> usize;
};
"#,
        expect![[r#"
            32..33: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
            35..36: expected a type
        "#]],
    );
}

#[test]
fn no_member_in_bounds_on_rigid_receiver() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
static f = fn::<T: D>(x: T) -> usize { x.q() };
"#,
        expect![[r#"
            86..91: no field or member `q` on `T`
        "#]],
    );
}

#[test]
fn trait_member_call_types_flow() {
    // Bound-directed and impl-directed calls carry the requirement's
    // instantiated types (the generic requirement's W pins from the
    // argument).
    check_infer(
        r#"
trait W = requires { push: fn(str, Self) -> Self; };
trait D = requires { fmt: fn::<X: W>(X, Self) -> X; } with {
    impl usize { fmt = fn::<X: W>(w: X, v: usize) -> X { w.push("n") }; }
};
type S = struct { c: usize } with {
    impl W { push = fn(s: str, w: Self) -> Self { w }; }
};
static f = fn(n: usize) -> S { n.fmt(S(struct { c = 0 })) };
"#,
        expect![[r#"
            299..347 'fn(n: usize) -> S...': fn(usize) -> S
            302..303 'n': usize
            317..347 '{ n.fmt(S(struct ...': S
            319..320 'n': usize
            319..324 'n.fmt': fn(S, usize) -> S
            319..345 'n.fmt(S(struct { ...': S
            325..326 'S': fn(struct { c: usize }) -> S
            325..344 'S(struct { c = 0 })': S
            327..343 'struct { c = 0 }': struct { c: usize }
            340..341 '0': usize
        "#]],
    );
}

// ---- traits: nested-body reservation, variant Self, ambiguity errors --

#[test]
fn nested_fn_literal_cannot_use_enclosing_bounds() {
    // The dictionary lives in the ROOT body: a nested literal would have
    // to capture it — reserved (both the bound-directed call and the
    // forwarding call forms).
    check_diagnostics(
        r#"
trait Size = requires { size: fn(Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static bounded = fn::<T: Size>(x: T) -> usize { x.size() };
static outer = fn::<T: Size>(x: T, z: T) -> usize {
    let f = fn(y: T) -> usize { y.size() };
    let g = fn(y: T) -> usize { bounded(y) };
    f(x) + g(z)
};
"#,
        expect![[r#"
            261..269: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
            305..312: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}

#[test]
fn const_block_cannot_use_enclosing_bounds() {
    // A `const` block lowers to a separate body — the same
    // captured-dictionary wall as a nested literal (the const-check
    // rejection fires alongside).
    check_diagnostics(
        r#"
trait Size = requires { size: fn(Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static outer = fn::<T: Size>(x: T) -> usize { const { x.size() } };
"#,
        expect![[r#"
            171..177: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const` block is a const context at 163..168)
            171..179: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}

#[test]
fn qualified_variant_self_resolves_through_the_enum() {
    // A constructed variant at a `Self` position determines `Self` as its
    // ENUM — the return type is the enum too, so a single-variant match is
    // honestly non-exhaustive.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point } with {
    impl Id { id = fn(x: Self) -> Self { Shape::Point }; }
};
trait Id = requires { id: fn(Self) -> Self; };
static main = fn() -> () {
    let c = Id::id(Shape::Circle(3));
    let r = match c { ::Circle(n) => n };
    let _ = r;
};
"#,
        expect![[r#"
            237..242: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn reserved_generic_trait_is_fully_non_live() {
    // The reservation is real everywhere: the bound, the impl and the
    // qualified use each carry their own diagnostic, and nothing
    // dispatches through the reserved trait.
    check_diagnostics(
        r#"
trait Gen = requires::<T> { get: fn(Self) -> usize; };
type X = struct { a: usize } with {
    impl Gen { get = fn(x: Self) -> usize { x.a }; }
};
static f = fn::<B: Gen>(x: B) -> usize { x.get() };
static g = fn(x: X) -> usize { Gen::get(x) };
"#,
        expect![[r#"
            21..26: generic traits are not supported yet
            101..104: `Gen` is a reserved generic trait (generic traits are not supported yet); it cannot be implemented
            167..170: `Gen` is a reserved generic trait (generic traits are not supported yet); it cannot be a bound
            189..196: no field or member `get` on `B`
            231..239: `Gen` is a reserved generic trait (generic traits are not supported yet) and cannot be used
        "#]],
    );
}

#[test]
fn trait_member_vs_fn_field_call_is_ambiguous() {
    // G13: a dot-callable member beside an fn-typed field is an
    // ambiguity ERROR under call syntax — silent shadowing would let a
    // distant impl reroute existing field calls. Both escapes work:
    // `(b.get)()` reaches the field, `Get::get(b)` the trait member. Bare
    // access stays the field; a NON-fn field keeps the sealed
    // member-shadows-field rule.
    check_diagnostics(
        r#"
trait Get = requires { get: fn(Self) -> usize; };
type B = struct { get: fn() -> usize } with {
    impl Get { get = fn(x: Self) -> usize { 100 }; }
};
type C = struct { get: usize } with {
    impl Get { get = fn(x: Self) -> usize { 7 }; }
};
static f = fn(b: B) -> usize { b.get() };
static escapes = fn(b: B, c: C) -> usize {
    let field = (b.get)();
    let member = Get::get(b);
    let bare = b.get;
    let nonfn = c.get();
    field + member + bare() + nonfn
};
"#,
        expect![[r#"
            276..283: `get` is ambiguous on `B`: it could be `Get`'s member (`Get::get(value)`) or the fn-typed field (`(value.get)(...)`) — spell the one you mean (`Get::get` for `B` is defined here at 112..115)
        "#]],
    );
}

#[test]
fn inherent_member_vs_fn_field_call_is_ambiguous() {
    check_diagnostics(
        r#"
type B = struct { get: fn() -> usize } with {
    impl Self { get = fn(x: Self) -> usize { 100 }; }
};
static f = fn(b: B) -> usize { b.get() };
"#,
        expect![[r#"
            135..142: `get` is ambiguous on `B`: it could be the inherent member (`B::get(value)`) or the fn-typed field (`(value.get)(...)`) — spell the one you mean (`B::get` is defined here at 63..66)
        "#]],
    );
}

// ---- G13: member collisions and their escapes ----

#[test]
fn inherent_member_vs_trait_member_call_is_ambiguous() {
    // G13: an inherent member no longer silently shadows a trait
    // member — the impl may be added in the TRAIT's chain, nowhere near
    // the type, so either winner would be action at a distance. Both
    // escapes are named, and both work.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { v: usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static collides = fn(p: P) -> usize { p.m() };
static escapes = fn(p: P) -> usize { P::m(p) + D::m(p) };
"#,
        expect![[r#"
            221..226: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`) or `D`'s member (`D::m(value)`) — spell the one you mean (`P::m` is defined here at 99..100) (`D::m` for `P` is defined here at 146..147)
        "#]],
    );
}

#[test]
fn inherent_trait_and_field_call_is_ambiguous() {
    // All three namespaces at once: the message lists one escape per
    // candidate, in resolution order (inherent, traits, field).
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { m: fn() -> usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static collides = fn(p: P) -> usize { p.m() };
static escapes = fn(p: P) -> usize { P::m(p) + D::m(p) + (p.m)() };
"#,
        expect![[r#"
            229..234: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`), `D`'s member (`D::m(value)`), or the fn-typed field (`(value.m)(...)`) — spell the one you mean (`P::m` is defined here at 107..108) (`D::m` for `P` is defined here at 154..155)
        "#]],
    );
}

#[test]
fn collision_leaves_bare_access_alone() {
    // Bare `p.m` is the FIELD, collision or not — only call syntax has
    // more than one candidate to choose between.
    check_infer(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { m: fn() -> usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static bare = fn(p: P) -> fn() -> usize { p.m };
"#,
        expect![[r#"
            205..238 'fn(p: P) -> fn() ...': fn(P) -> fn() -> usize
            208..209 'p': P
            231..238 '{ p.m }': fn() -> usize
            233..234 'p': P
            233..236 'p.m': fn() -> usize
        "#]],
    );
}

#[test]
fn bound_directed_collision_is_ambiguous() {
    // Two bounds providing the same name on a RIGID receiver: the same
    // ambiguity, with the escapes naming the param.
    check_diagnostics(
        r#"
trait A = requires { m: fn(Self) -> usize; };
trait B = requires { m: fn(Self) -> usize; };
static collides = fn::<T: A + B>(x: T) -> usize { x.m() };
static escapes = fn::<T: A + B>(x: T, y: T) -> usize { A::m(x) + B::<Self = T>::m(y) };
"#,
        expect![[r#"
            143..148: `m` is ambiguous on `T`: it could be `A`'s member (`A::m(value)`) or `B`'s member (`B::m(value)`) — spell the one you mean (required by the trait here at 22..23) (required by the trait here at 68..69)
        "#]],
    );
}

#[test]
fn qualified_inherent_member_is_a_value() {
    // An inherent member is an ordinary fn (its `Self` is just the last
    // parameter), so the qualified reference is its plain fn VALUE — no
    // dictionary is involved anywhere.
    check_infer(
        r#"
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static main = fn(p: P) -> usize { let f = P::len; f(p) };
"#,
        expect![[r#"
            108..150 'fn(p: P) -> usize...': fn(P) -> usize
            111..112 'p': P
            126..150 '{ let f = P::len;...': usize
            132..133 'f': fn(P) -> usize
            136..142 'P::len': fn(P) -> usize
            144..145 'f': fn(P) -> usize
            144..148 'f(p)': usize
            146..147 'p': P
        "#]],
    );
}

#[test]
fn qualified_path_naming_a_field_says_so_and_names_the_escape() {
    // The field/member namespace split puts fields on VALUES: `P::len` is
    // looking in the type's namespace, where only variants and members
    // live. The old answer ("`P` has no variants") was true and useless —
    // the name IS declared, one namespace over. Genuinely-unknown names
    // keep that answer, and an enum is untouched (it has no fields to
    // confuse anything with).
    check_diagnostics(
        r#"
type P = struct { len: usize } with {
    impl Self { size = fn(p: Self) -> usize { p.len }; }
};
type Shape = enum { Circle, Point };
static a = fn() -> () { let i = P::len; };
static b = fn() -> () { let j = P::nosuch; };
static c = fn(p: P) -> usize { P::size(p) };
static d = fn() -> () { let m = Shape::Nope; };
"#,
        expect![[r#"
            168..174: `len` is a field of `P`, not a member — fields are reached through a value: `value.len` (`P` is defined here at 6..7)
            211..220: `P` has no variants (it is a `struct` type) (`P` is defined here at 6..7)
            309..313: `Shape` has no variant `Nope` (`Shape` is defined here at 104..109)
        "#]],
    );
}

#[test]
fn qualified_member_path_reaches_inherent_members_only() {
    // Each spelling names exactly one thing: `Type::m` is the type's OWN
    // member, and a trait member is spelled through its trait.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
type P = struct { v: usize } with {
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static main = fn(p: P) -> usize { P::m(p) };
"#,
        expect![[r#"
            167..171: `m` is not a member of `P` itself: `D` provides it — write `D::m(value)` (a trait member is spelled through its trait)
        "#]],
    );
}

#[test]
fn self_qualified_member_inside_a_member_body() {
    // Expression-`Self` is RIGID (TR01), and the qualified
    // path reads it like any type name: `Self::one(p)` is the enclosing
    // type's own member at the body's own instantiation.
    check_diagnostics(
        r#"
type P = struct { v: usize } with {
    impl Self {
        one = fn(p: Self) -> usize { 1 };
        two = fn(p: Self) -> usize { Self::one(p) + 1 };
    }
};
static main = fn(p: P) -> usize { P::two(p) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn qualified_member_path_on_an_enum_keeps_variants_first() {
    // An enum's second segment is its VARIANT's home; a member of the same
    // name would be shadowed there, and any other name reaches the members.
    // The qualified path also ignores dot-callability — it passes `Self`
    // like any other argument, so a member the dot cannot reach is still
    // callable here.
    check_diagnostics(
        r#"
type Shape = enum { Circle, Point } with {
    impl Self {
        area = fn(s: Self) -> usize { 1 };
        scaled = fn(s: Self, by: usize) -> usize { by };
    }
};
static main = fn() -> usize {
    let c = Shape::Circle;
    Shape::area(c) + Shape::scaled(c, 2)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn qualified_member_on_a_generic_type_takes_the_type_args() {
    // An inherent member's binder IS the owner's, so the turbofish sits on
    // the TYPE — exactly the arguments a dot-call reads off the receiver.
    check_infer(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
static main = fn() -> usize {
    let p = Pair::<usize>(struct { a = 1, b = 2 });
    Pair::<usize>::first(p)
};
"#,
        expect![[r#"
            116..213 'fn() -> usize {  ...': fn() -> usize
            130..213 '{     let p = Pai...': usize
            140..141 'p': Pair::<usize>
            144..157 'Pair::<usize>': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            144..182 'Pair::<usize>(str...': Pair::<usize>
            158..181 'struct { a = 1, b...': struct { a: usize, b: usize }
            171..172 '1': usize
            178..179 '2': usize
            188..208 'Pair::<usize>::first': fn(Pair::<usize>) -> usize
            188..211 'Pair::<usize>::fi...': usize
            209..210 'p': Pair::<usize>
        "#]],
    );
}

#[test]
fn named_self_call_pins_an_uninferable_self() {
    // The short form cannot infer `Self` when no argument mentions it;
    // the named-Self form states it — much of its point.
    check_diagnostics(
        r#"
trait D = requires { n: fn(usize) -> Self; } with {
    impl usize { n = fn(k: usize) -> usize { k }; }
};
static main = fn() -> usize { D::<Self = usize>::n(3) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn named_self_call_on_a_rigid_self_dispatches_through_the_dictionary() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static f = fn::<T: D>(x: T) -> usize { D::<Self = T>::m(x) };
static main = fn() -> usize { f::<usize>(7) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn named_self_member_value_is_impl_specific() {
    // TR01's impl-specific fn value: one impl's member, usable as a value.
    // A RIGID `Self` would read the enclosing dictionary — the same
    // capture wall as `BoundFnValue`.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static ok = fn() -> usize { let f = D::<Self = usize>::m; f(4) };
static reserved = fn::<T: D>(x: T) -> usize { let f = D::<Self = T>::m; f(x) };
"#,
        expect![[r#"
            228..244: `D::m` on a rigid `Self` comes from the enclosing dictionary, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn named_self_argument_list_rules() {
    // Only `Self` is nameable (TR01), it may be given once, and a
    // non-generic trait takes no positional arguments of its own.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static a = fn() -> usize { D::<Self = usize, Self = usize>::m(1) };
static b = fn() -> usize { D::<W = usize, Self = usize>::m(1) };
static c = fn() -> usize { D::<usize, Self = usize>::m(1) };
static d = fn(x: usize) -> usize { id::<Self = usize>(x) };
static id = fn::<T>(x: T) -> T { x };
type Pair = struct::<T> { a: T, b: T };
static e = fn(p: Pair::<Self = usize>) -> usize { 1 };
"#,
        expect![[r#"
            135..169: `Self` is given more than once
            203..234: `W` cannot be supplied by name: `Self` is the only nameable generic argument (`Trait::<Self = Type>::member`)
            268..295: `D` takes no generic arguments
            337..355: only a trait has a `Self` argument to name
            464..476: only a trait has a `Self` argument to name
        "#]],
    );
}

#[test]
fn named_self_argument_may_not_be_a_hole() {
    // `Self` NAMES the implementer — it is what decides which impl the
    // path denotes — so a `_` there declines to answer the only question
    // the spelling asks. Refused structurally at lowering, in BOTH
    // positions and at ANY depth, so no inference variable ever reaches
    // trait resolution (the old shape rendered `_ does not implement D`
    // and `Pair::<_> does not implement E`).
    //
    // Uniform on purpose: in call position a top-level hole IS inferable
    // from the arguments, but it adds nothing over the short form
    // `D::m(v)` — and both short and full spellings still work below.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
type Pair = struct::<T> { a: T, b: T };
trait E = requires { n: fn(Self) -> usize; };
static a = fn(v: usize) -> usize { D::<Self = _>::m(v) };
static b = fn() -> () { let f = D::<Self = _>::m; };
static c = fn() -> () { let f = E::<Self = Pair::<_>>::n; };
static ok = fn(v: usize) -> usize { D::<Self = usize>::m(v) + D::m(v) };
"#,
        expect![[r#"
            229..245: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
            284..300: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
            337..361: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
        "#]],
    );
}

#[test]
fn named_self_reservations_survive() {
    // Associated types (reserved), generic traits (reserved) and the
    // bounded-value capture wall are untouched by the named-Self form.
    check_diagnostics(
        r#"
trait Gen = requires::<T> { g: fn(Self) -> T; };
trait D = requires {
    type Item;
    m: fn::<W: D + forget>(W, Self) -> usize;
} with {
    impl usize { m = fn::<W: D + forget>(w: W, x: usize) -> usize { x }; }
};
static a = fn(x: usize) -> usize { D::<Self = usize>::Item };
static b = fn(x: usize) -> usize { Gen::<Self = usize>::g(x) };
static c = fn(x: usize) -> usize { let f = D::<Self = usize>::m; 1 };
"#,
        expect![[r#"
            21..26: generic traits are not supported yet
            75..79: associated types are not supported yet
            254..277: `D::Item` is an associated type; associated types are not supported yet
            316..338: `Gen` is a reserved generic trait (generic traits are not supported yet) and cannot be used
            388..408: `D::m` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn bounded_fn_as_value_reports_once() {
    // The reservation is the whole story — no cannot-infer sibling.
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
static bounded = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> () {
    let v = bounded;
    let _ = v;
};
"#,
        expect![[r#"
            201..208: `bounded` has bounds on its type parameters, so it cannot be used as a value yet; call it directly
        "#]],
    );
}

#[test]
fn builtin_type_as_type_side_impl_head_is_not_a_trait() {
    check_diagnostics(
        r#"
type P = struct { a: usize } with {
    impl usize { m = fn(x: Self) -> usize { 1 }; }
};
"#,
        expect![[r#"
            46..51: `usize` is not a trait
        "#]],
    );
}
