//! Errors in one body do not poison what it is joined to.

use super::*;

/// Member bodies are NOT a second class of call site. A member is required
/// to spell its full signature, which makes it a fully-typed item — and a
/// fully-typed item's body feeds the groups of the items it calls (T11):
/// a member's call fills an unannotated static's holes exactly as an
/// annotated static's call does.
#[test]
fn a_member_body_fills_its_callees_holes_like_an_annotated_static() {
    check_signatures(
        r#"
static id = fn (x) { x };
type A = struct { n: usize } with {
    impl Self {
        use_it = fn(a: Self) -> usize { id(a.n) };
    }
};
"#,
        expect![[r#"
            id: fn(usize) -> usize
        "#]],
    );
    check_signatures(
        r#"
static id = fn (x) { x };
static use_it = fn (n: usize) -> usize { id(n) };
"#,
        expect![[r#"
            id: fn(usize) -> usize
            use_it: fn(usize) -> usize
        "#]],
    );
}

/// A trait impl's member has the concrete `Self` its impl head names, so
/// its body fills its callees' holes like any fully typed item.
#[test]
fn trait_impl_member_fills_its_callees_holes() {
    check_signatures(
        r#"
static id = fn (x) { x };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl usize {
        size = fn(n: usize) -> usize { id(n) };
    }
};
"#,
        expect![[r#"
            id: fn(usize) -> usize
        "#]],
    );
    check_signatures(
        r#"
static id = fn (x) { x };
type A = struct { n: usize };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl A {
        size = fn(a: Self) -> usize { id(a).n };
    }
};
"#,
        expect![[r#"
            id: fn(A) -> A
        "#]],
    );
}

/// An impl whose head names no type fills its callees' holes: its `Self`
/// is the silent `Ty::Error`, so only the head is reported, in either
/// source order.
#[test]
fn unresolved_trait_impl_member_fills_its_callees_holes() {
    let uses_n = r#"
static id = fn (x) { x };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl Nope {
        size = fn(n: usize) -> usize { id(n) };
    }
};
"#;
    check_signatures(
        uses_n,
        expect![[r#"
        id: fn(usize) -> usize
    "#]],
    );
    check_diagnostics(
        uses_n,
        expect![[r#"
        94..98: unknown type `Nope`
    "#]],
    );
    let main_first = r#"
static id = fn (x) { x };
static main = fn () -> usize { id(1) };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl Nope {
        size = fn(s: Self) -> usize { id(s); 0 };
    }
};
"#;
    check_signatures(
        main_first,
        expect![[r#"
        id: fn(usize) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        main_first,
        expect![[r#"
        134..138: unknown type `Nope`
    "#]],
    );
    let trait_first = r#"
static id = fn (x) { x };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl Nope {
        size = fn(s: Self) -> usize { id(s); 0 };
    }
};
static main = fn () -> usize { id(1) };
"#;
    check_signatures(
        trait_first,
        expect![[r#"
        id: fn(usize) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        trait_first,
        expect![[r#"
        94..98: unknown type `Nope`
    "#]],
    );
}

/// A fully typed caller with an unknown type in its signature fills
/// nothing with it: the callee takes what the sound calls say, and the
/// error is reported once.
#[test]
fn callers_broken_signature_does_not_poison_its_callee() {
    let text = r#"
static id = fn (x) { x };
static broken = fn (x: Nope) -> usize { id(x); 0 };
static main = fn () -> usize { id(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
        id: fn(usize) -> usize
        broken: fn({error}) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        50..54: unknown type `Nope`
    "#]],
    );
}

/// An unresolved name in a fully typed caller's body fills nothing, but its
/// sound calls fill their callees. A callee whose only use is the broken one stays
/// undetermined.
#[test]
fn callers_broken_body_does_not_poison_its_callees() {
    let text = r#"
static id = fn (x) { x };
static show = fn (y) { y };
static main = fn (n: usize) -> usize { id(n); show(missing); 0 };
"#;
    check_signatures(
        text,
        expect![[r#"
        id: fn(usize) -> usize
        show: fn({error}) -> {error}
        main: fn(usize) -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            101..105: not enough information to infer the type of `show`; add a type annotation to its definition (defined here at 34..38)
            106..113: unresolved name `missing`
        "#]],
    );
}

/// An impl member under an unknown head type fills nothing with its
/// `Self`; its sound calls fill their callees.
#[test]
fn callers_broken_impl_head_does_not_poison_its_callees() {
    let text = r#"
static id = fn (x) { x };
static show = fn (y) { y };
trait Size = requires { size: fn(Self) -> usize; } with {
    impl Nope {
        size = fn(s: Self, n: usize) -> usize { id(n); show(s); 0 };
    }
};
"#;
    check_signatures(
        text,
        expect![[r#"
        id: fn(usize) -> usize
        show: fn({error}) -> {error}
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            122..126: unknown type `Nope`
            184..188: not enough information to infer the type of `show`; add a type annotation to its definition (defined here at 34..38)
        "#]],
    );
}

/// An unannotated caller's mistake fills nothing either: the callee takes
/// the type the sound call gives it, whichever caller is annotated.
#[test]
fn unannotated_callers_broken_body_does_not_poison_its_callee() {
    let text = r#"
static show = fn (y) { y };
static a = fn () -> () { show(missing); };
static main = fn () -> usize { show(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
        show: fn(usize) -> usize
        a: fn()
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        59..66: unresolved name `missing`
    "#]],
    );
}

/// A member whose own value is erroneous gets an erroneous signature, so
/// its uses neither decide its type nor disagree about it, in either order.
#[test]
fn erroneous_static_member_is_not_decided_by_its_uses() {
    let b_first = r#"
static a = missing;
static b = fn () -> () { print(a) };
static c = fn () -> usize { a };
"#;
    check_signatures(
        b_first,
        expect![[r#"
            a: {error}
            b: fn()
            c: fn() -> usize
        "#]],
    );
    check_diagnostics(
        b_first,
        expect![[r#"
            12..19: unresolved name `missing`
        "#]],
    );
    let c_first = r#"
static a = missing;
static c = fn () -> usize { a };
static b = fn () -> () { print(a) };
"#;
    check_signatures(
        c_first,
        expect![[r#"
            a: {error}
            c: fn() -> usize
            b: fn()
        "#]],
    );
    check_diagnostics(
        c_first,
        expect![[r#"
            12..19: unresolved name `missing`
        "#]],
    );
}

/// A function member whose own type is erroneous gets an erroneous
/// signature, so its calls neither decide it nor disagree about it, in
/// either order.
#[test]
fn erroneous_fn_member_is_not_decided_by_its_calls() {
    let g_first = r#"
static f = fn () { missing };
static g = fn () -> usize { f() };
static h = fn () -> str { f() };
"#;
    check_signatures(
        g_first,
        expect![[r#"
        f: fn() -> {error}
        g: fn() -> usize
        h: fn() -> str
    "#]],
    );
    check_diagnostics(
        g_first,
        expect![[r#"
            20..27: unresolved name `missing`
        "#]],
    );
    let h_first = r#"
static f = fn () { missing };
static h = fn () -> str { f() };
static g = fn () -> usize { f() };
"#;
    check_signatures(
        h_first,
        expect![[r#"
        f: fn() -> {error}
        h: fn() -> str
        g: fn() -> usize
    "#]],
    );
    check_diagnostics(
        h_first,
        expect![[r#"
            20..27: unresolved name `missing`
        "#]],
    );
}

/// An error that meets only a parameter leaves it open: uses decide
/// parameters, so `main` decides `g`'s, which `f` shares.
#[test]
fn members_own_error_leaves_a_parameter_it_shares_open() {
    let text = r#"
static g = fn (y) { y };
static f = fn (x) { g(x); x == missing };
static main = fn () -> usize { g(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
            g: fn(usize) -> usize
            f: fn(usize) -> bool
            main: fn() -> usize
        "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            57..64: unresolved name `missing`
        "#]],
    );
}

/// The same holds for the parameter of a function a member returns.
#[test]
fn members_own_error_leaves_the_parameter_of_a_returned_fn_open() {
    let curried = r#"
static f = fn () { fn (x) { x == missing; 0 } };
static main = fn () -> usize { let n: usize = 1; f()(n) };
"#;
    check_signatures(
        curried,
        expect![[r#"
            f: fn() -> fn(usize) -> usize
            main: fn() -> usize
        "#]],
    );
    check_diagnostics(
        curried,
        expect![[r#"
            34..41: unresolved name `missing`
        "#]],
    );
}

/// A variable in what a member produces that met an error is poisoned
/// even where another member shares it: that member's type depends on the
/// broken body too, so `g`'s uses (`main`'s included) say "not enough
/// information".
#[test]
fn members_own_mismatch_poisons_a_result_it_shares() {
    let text = r#"
static g = fn (y) { y };
static f = fn (x) { g(x); x == 1; let b: bool = x; x };
static main = fn () -> usize { g(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
        g: fn({error}) -> {error}
        f: fn({error}) -> {error}
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            46..47: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 8..9)
            74..75: type mismatch: expected `bool`, found `{number}` (expected `bool` because of this annotation at 67..71)
            113..114: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );
}

/// A member that returns an erroneous member's result is erroneous too,
/// whichever of the two runs first, so its uses don't disagree about it.
#[test]
fn member_returning_an_erroneous_members_result_is_not_decided_by_its_uses() {
    let caller_first = r#"
static a = fn () { f() };
static f = fn () { missing };
static g = fn () -> usize { a() };
static h = fn () -> str { a() };
"#;
    check_signatures(
        caller_first,
        expect![[r#"
        a: fn() -> {error}
        f: fn() -> {error}
        g: fn() -> usize
        h: fn() -> str
    "#]],
    );
    check_diagnostics(
        caller_first,
        expect![[r#"
            46..53: unresolved name `missing`
            85..86: not enough information to infer the type of `a`; add a type annotation to its definition (defined here at 8..9)
            118..119: not enough information to infer the type of `a`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );
    let callee_first = r#"
static f = fn () { missing };
static a = fn () { f() };
static g = fn () -> usize { a() };
static h = fn () -> str { a() };
"#;
    check_signatures(
        callee_first,
        expect![[r#"
        f: fn() -> {error}
        a: fn() -> {error}
        g: fn() -> usize
        h: fn() -> str
    "#]],
    );
    check_diagnostics(
        callee_first,
        expect![[r#"
            20..27: unresolved name `missing`
            85..86: not enough information to infer the type of `a`; add a type annotation to its definition (defined here at 38..39)
            118..119: not enough information to infer the type of `a`; add a type annotation to its definition (defined here at 38..39)
        "#]],
    );
}

/// An error that reaches only a member's parameter leaves the rest of its
/// signature as written, so a call is still checked against its arity and
/// return type.
#[test]
fn erroneous_parameter_keeps_a_members_arity_and_return() {
    let text = r#"
static f = fn (x) { x == missing; true };
static g = fn () -> str { f(1, 2) };
"#;
    check_signatures(
        text,
        expect![[r#"
            f: fn({error}) -> bool
            g: fn() -> str
        "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            26..33: unresolved name `missing`
            69..70: not enough information to infer the type of `f`; add a type annotation to its definition (defined here at 8..9)
            69..76: expected 1 argument(s), found 2 (`f` is defined here at 8..9)
            69..76: type mismatch: expected `str`, found `bool` (expected `str` because of this return type at 60..66)
        "#]],
    );
}

/// A member's own error doesn't decide a parameter it shares with a later
/// member, so that member's own body decides it, in either order.
#[test]
fn members_own_error_leaves_a_shared_parameter_to_its_other_member() {
    let f_first = r#"
static f = fn (x) { g(x); x == missing };
static g = fn (y) { let z: usize = y; y };
static main = fn () -> usize { g(1) };
"#;
    check_signatures(
        f_first,
        expect![[r#"
        f: fn(usize) -> bool
        g: fn(usize) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        f_first,
        expect![[r#"
        32..39: unresolved name `missing`
    "#]],
    );
    let g_first = r#"
static g = fn (y) { let z: usize = y; y };
static f = fn (x) { g(x); x == missing };
static main = fn () -> usize { g(1) };
"#;
    check_signatures(
        g_first,
        expect![[r#"
        g: fn(usize) -> usize
        f: fn(usize) -> bool
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        g_first,
        expect![[r#"
        75..82: unresolved name `missing`
    "#]],
    );
}

/// A parameter that met an error and that a later member's body joins to
/// the result is poisoned in either order.
#[test]
fn parameter_joined_to_the_result_later_is_poisoned_in_either_order() {
    let f_first = r#"
static f = fn (x) { x == missing; h(x) };
static h = fn (y) { y };
static main = fn () -> usize { f(1) };
"#;
    check_signatures(
        f_first,
        expect![[r#"
        f: fn({error}) -> {error}
        h: fn({error}) -> {error}
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        f_first,
        expect![[r#"
            26..33: unresolved name `missing`
            35..36: not enough information to infer the type of `h`; add a type annotation to its definition (defined here at 50..51)
        "#]],
    );
    let h_first = r#"
static h = fn (y) { y };
static f = fn (x) { x == missing; h(x) };
static main = fn () -> usize { f(1) };
"#;
    check_signatures(
        h_first,
        expect![[r#"
        h: fn({error}) -> {error}
        f: fn({error}) -> {error}
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        h_first,
        expect![[r#"
            51..58: unresolved name `missing`
            60..61: not enough information to infer the type of `h`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );
}

/// A member's own error doesn't decide a result it shares with a later
/// member, so that member's own body decides it, in either order.
#[test]
fn members_own_error_leaves_a_shared_result_to_its_other_member() {
    let f_first = r#"
static f = fn () { let r = g(); r == missing; r };
static g = fn () { let z: usize = 1; z };
static main = fn () -> usize { g() };
"#;
    check_signatures(
        f_first,
        expect![[r#"
        f: fn() -> usize
        g: fn() -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        f_first,
        expect![[r#"
        38..45: unresolved name `missing`
    "#]],
    );
    let g_first = r#"
static g = fn () { let z: usize = 1; z };
static f = fn () { let r = g(); r == missing; r };
static main = fn () -> usize { g() };
"#;
    check_signatures(
        g_first,
        expect![[r#"
        g: fn() -> usize
        f: fn() -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        g_first,
        expect![[r#"
        80..87: unresolved name `missing`
    "#]],
    );
}

/// A member's use decides an erroneous member's result, as it fills any
/// part a member's body left open, in either order.
#[test]
fn members_use_decides_an_erroneous_members_result() {
    let use_first = r#"
static b = fn () { print(a()) };
static a = fn () { missing };
"#;
    check_signatures(
        use_first,
        expect![[r#"
        b: fn()
        a: fn() -> str
    "#]],
    );
    check_diagnostics(
        use_first,
        expect![[r#"
        53..60: unresolved name `missing`
    "#]],
    );
    let a_first = r#"
static a = fn () { missing };
static b = fn () { print(a()) };
"#;
    check_signatures(
        a_first,
        expect![[r#"
        a: fn() -> str
        b: fn()
    "#]],
    );
    check_diagnostics(
        a_first,
        expect![[r#"
        20..27: unresolved name `missing`
    "#]],
    );
}

/// Two members' uses that disagree on an erroneous member's result leave
/// it `{error}`, in either order. Its body carries the error, so the uses
/// say nothing.
#[test]
fn members_uses_disagreeing_on_an_erroneous_result_leave_it_undecided() {
    let k_first = r#"
static f = fn () { missing };
static k = fn () { print(f()) };
static m = fn () { let n: usize = f(); n };
"#;
    check_signatures(
        k_first,
        expect![[r#"
            f: {error}
            k: fn()
            m: fn() -> usize
        "#]],
    );
    check_diagnostics(
        k_first,
        expect![[r#"
            20..27: unresolved name `missing`
        "#]],
    );
    let m_first = r#"
static f = fn () { missing };
static m = fn () { let n: usize = f(); n };
static k = fn () { print(f()) };
"#;
    check_signatures(
        m_first,
        expect![[r#"
            f: {error}
            m: fn() -> usize
            k: fn()
        "#]],
    );
    check_diagnostics(
        m_first,
        expect![[r#"
            20..27: unresolved name `missing`
        "#]],
    );
}

/// A static whose value is `{error}` decides nothing: a body that joins
/// it to another member (`a == name()`) leaves that member to its own
/// body, in either order, and the uses of the broken static say nothing.
#[test]
fn erroneous_static_does_not_poison_what_a_body_joins_it_to() {
    let check_first = r#"
static check = fn () { a == name() };
static a = missing;
static name = fn () { "bob" };
"#;
    check_signatures(
        check_first,
        expect![[r#"
            check: fn() -> bool
            a: {error}
            name: fn() -> str
        "#]],
    );
    check_diagnostics(
        check_first,
        expect![[r#"
            50..57: unresolved name `missing`
        "#]],
    );
    let a_first = r#"
static a = missing;
static check = fn () { a == name() };
static name = fn () { "bob" };
"#;
    check_signatures(
        a_first,
        expect![[r#"
        a: {error}
        check: fn() -> bool
        name: fn() -> str
    "#]],
    );
    check_diagnostics(
        a_first,
        expect![[r#"
            12..19: unresolved name `missing`
        "#]],
    );
}

/// An error unrelated to a member's own type leaves its signature to its
/// uses.
#[test]
fn error_outside_a_members_type_leaves_its_signature_open() {
    let text = r#"
static f = fn (x) { let z = missing; x };
static main = fn () -> usize { f(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
        f: fn(usize) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        29..36: unresolved name `missing`
    "#]],
    );
}

/// A mismatch in a member's own body that doesn't reach its type leaves
/// its parameter open for a caller to fill.
#[test]
fn mismatch_in_a_members_body_leaves_its_parameter_open() {
    let text = r#"
static f = fn (x) { let y: bool = x.&; x };
static main = fn () -> usize { f(1) };
"#;
    check_signatures(
        text,
        expect![[r#"
            f: fn(usize) -> usize
            main: fn() -> usize
        "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            35..38: type mismatch: expected `bool`, found `usize.&` (expected `bool` because of this annotation at 28..32)
        "#]],
    );
}

/// Calling a member that isn't a function is the caller's mistake: it is
/// reported at the call, and the member keeps the type its other uses give
/// it.
#[test]
fn calling_a_non_function_member_keeps_its_type() {
    let text = r#"
static five = 5;
static main = fn () -> usize { five(1) };
static other = fn () -> usize { five };
"#;
    check_signatures(
        text,
        expect![[r#"
        five: usize
        main: fn() -> usize
        other: fn() -> usize
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
            49..53: expression of type `usize` is not callable
        "#]],
    );
}

/// A caller whose use of a member mismatches doesn't poison the member,
/// whichever caller comes first: the other caller's use decides it, and the
/// mismatch is reported once, at the mismatching use, with the type the
/// member's own body gave it.
#[test]
fn mismatching_caller_does_not_poison_a_number_member() {
    let main_first = r#"
static five = 5;
static main = fn () -> bool { five };
static other = fn () -> usize { five };
"#;
    check_signatures(
        main_first,
        expect![[r#"
            five: usize
            main: fn() -> bool
            other: fn() -> usize
        "#]],
    );
    check_diagnostics(
        main_first,
        expect![[r#"
            48..52: type mismatch: expected `bool`, found `usize` (expected `bool` because of this return type at 38..45)
        "#]],
    );
    let other_first = r#"
static five = 5;
static other = fn () -> usize { five };
static main = fn () -> bool { five };
"#;
    check_signatures(
        other_first,
        expect![[r#"
            five: usize
            other: fn() -> usize
            main: fn() -> bool
        "#]],
    );
    check_diagnostics(
        other_first,
        expect![[r#"
            88..92: type mismatch: expected `bool`, found `usize` (expected `bool` because of this return type at 78..85)
        "#]],
    );
}

#[test]
fn dot_call_on_broken_declaration_with_members_stays_silent() {
    // The `with`-chain parses independently of a broken RHS, so the member
    // resolves even though the type does not. Errors are infectious and
    // SILENT: the declaration carries its own diagnostic, and a dot-call
    // must not additionally be told to call what it already called.
    check_diagnostics(
        r#"
type A = 5 with {
    impl Self {
        get = fn(a: Self) -> usize { 0 };
    }
};
static main = fn (a: A) -> usize { a.get() };
"#,
        expect![[r#"
            10..11: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

#[test]
fn dot_call_on_broken_declaration_stays_silent() {
    // Errors are infectious and SILENT: a broken type declaration carries
    // its own diagnostic; a dot-call on a value of that type must not
    // cascade a "no field or member" on top (same posture as plain field
    // access).
    check_diagnostics(
        r#"
type A = 5;
static main = fn (a: A) -> usize { a.get() };
"#,
        expect![[r#"
            10..11: only a `struct` or `enum` literal can declare a type
        "#]],
    );
}

/// An annotated `let` whose initializer is poisoned is typed by its
/// annotation, so later uses are judged against `char`.
#[test]
fn annotated_let_with_poisoned_initializer_uses_its_annotation() {
    check_diagnostics(
        r#"
static main = fn () -> () {
    let c: char = match input.next_char(0) {
        ::Char(ch, _) => ch,
        ::End => panic("no input"),
    };
    let d = c.&.to_i32();
};
"#,
        expect![[r#"
            53..58: unresolved name `input`
            158..170: no field or member `to_i32` on `char`
        "#]],
    );
}

/// An unannotated `let` with a poisoned initializer reports nothing past
/// the original error.
#[test]
fn unannotated_let_with_poisoned_initializer_stays_silent() {
    check_diagnostics(
        r#"
static main = fn () -> () {
    let c = match input.next_char(0) {
        ::Char(ch, _) => ch,
        ::End => panic("no input"),
    };
    let d = c.&.to_i32();
    let e = c.&.x;
    let f = c.&();
};
"#,
        expect![[r#"
            47..52: unresolved name `input`
        "#]],
    );
}

/// A diagnostic on a poisoned operand is reported when it is its own error
/// (`.b` on a record, an annotation mismatch).
#[test]
fn an_error_beside_the_poison_is_reported() {
    check_diagnostics(
        r#"
static main = fn () -> () {
    let y = nope;
    let g = struct { a = y }.b;
    let x: usize = struct { a = y };
};
"#,
        expect![[r#"
            41..45: unresolved name `nope`
            76..77: no field `b` on `struct { a: {error} }`
            98..114: type mismatch: expected `usize`, found `struct { a: {error} }` (expected `usize` because of this annotation at 90..95)
        "#]],
    );
}

/// A branch join whose types hold a nested `{error}` reports nothing past
/// the original error.
#[test]
fn branch_join_with_nested_poison_stays_silent() {
    check_diagnostics(
        r#"
static f = fn (c: bool) -> () {
    let y: Nope = 5;
    let r = if c { struct { a = y } } else { struct { a = "s" } };
};
"#,
        expect![[r#"
            44..48: unknown type `Nope`
        "#]],
    );
}

/// Branch leaves with a poisoned part disagree when their shapes do:
/// with each other, or with a clean leaf.
#[test]
fn branch_join_with_poisoned_leaves_of_different_shapes_reports() {
    check_diagnostics(
        r#"
static f = fn (c: bool) -> () {
    let y: Nope = 5;
    let r = if c { struct { a = y } } else { struct { b = y } };
    let s = if c { struct { a = y } } else { struct { b = "s" } };
};
"#,
        expect![[r#"
            44..48: unknown type `Nope`
            99..115: `if` branches have incompatible types: `struct { a: {error} }` vs `struct { b: {error} }`; add a type annotation to decide between them (this branch has type `struct { a: {error} }` at 73..89)
            138..154: type mismatch: expected `struct { b: str }`, found `struct { a: {error} }` (this branch has type `struct { b: str }` at 164..182)
        "#]],
    );
}

/// A destructuring `let` with a poisoned initializer is typed by its
/// annotation.
#[test]
fn annotated_destructuring_let_with_poisoned_initializer_uses_its_annotation() {
    check_infer(
        r#"
static main = fn () -> () {
    let struct { x }: struct { x: usize } = nope;
    let y = x;
};
"#,
        expect![[r#"
            15..95 'fn () -> () {    ...': fn()
            27..95 '{     let struct ...': ()
            46..47 'x': usize
            73..77 'nope': {error}
            87..88 'y': usize
            91..92 'x': usize
        "#]],
    );
}
