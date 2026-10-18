//! Libraries of several files: imports, paths and cross-file inference.

use super::*;

/// A file sees another file's items only through `use`: values and types,
/// renamed or not.
#[test]
fn a_file_sees_what_it_imports() {
    check_library_diagnostics(
        r#"
//- a.must
use b::{g, Pair as P};
static f = fn() -> usize { g(1) + P(struct { a = 1, b = 2 }).a };
//- b.must
static g = fn(n: usize) -> usize { n };
type Pair = struct { a: usize, b: usize };
"#,
        expect![[""]],
    );
}

/// A name another module declares is unresolved until imported; the error
/// points at every module that declares it.
#[test]
fn a_file_does_not_see_what_it_does_not_import() {
    check_library_diagnostics(
        r#"
//- a.must
static f = fn() -> usize { g(1) + h() };
//- b.must
static g = fn(n: usize) -> usize { n };
static h = fn() -> usize { 1 };
//- sub/c.must
static h = fn() -> usize { 2 };
"#,
        expect![[r#"
            a.must 27..28: unresolved name `g` (`b` declares `g` at b.must 7..8)
            a.must 34..35: unresolved name `h` (`b` declares `h` at b.must 47..48) (`sub::c` declares `h` at sub/c.must 7..8)
        "#]],
    );
}

/// Each file is its own namespace: the same name in two files is two items.
#[test]
fn a_name_declared_in_two_files_is_two_items() {
    check_library_diagnostics(
        r#"
//- a.must
static f = fn() -> usize { 1 };
type P = struct { a: usize };
//- b.must
static f = fn() -> usize { 2 };
type P = struct { b: usize };
"#,
        expect![[""]],
    );
}

/// An import clashes with the file's own item of that name, and with an
/// earlier import of it; every side carries the error, each pointing at the
/// others.
#[test]
fn an_import_clashes_with_a_name_the_file_already_has() {
    check_library_diagnostics(
        r#"
//- a.must
use b::f;
use c::{g, f as g};
use c::f;
static f = fn() -> usize { 1 };
//- b.must
static f = fn() -> usize { 2 };
//- c.must
static f = fn() -> usize { 3 };
static g = fn() -> usize { 4 };
"#,
        expect![[r#"
            a.must 7..8: `f` is defined multiple times (also defined here at a.must 47..48) (also imported here at a.must 37..38)
            a.must 18..19: `g` is defined multiple times (also imported here at a.must 26..27)
            a.must 26..27: `g` is defined multiple times (also imported here at a.must 18..19)
            a.must 37..38: `f` is defined multiple times (also defined here at a.must 47..48) (also imported here at a.must 7..8)
            a.must 47..48: `f` is defined multiple times (also imported here at a.must 7..8) (also imported here at a.must 37..38)
        "#]],
    );
}

/// A bad path is one error at the `use`, however many names it carries; a
/// missing name, or one its module declares twice, is an error at that name.
/// A name imported from two declarations is ambiguous, not unknown, so its
/// mention carries no second error.
#[test]
fn an_import_from_a_missing_module_or_of_a_missing_name() {
    check_library_diagnostics(
        r#"
//- a.must
use nowhere::{f, h};
use sub::b::{g, k};
use b::{g, f, d};
static e = fn() -> usize { d };
//- b.must
static f = fn() -> usize { 2 };
static d: usize = 1;
static d: usize = 2;
"#,
        expect![[r#"
            a.must 4..11: no module `nowhere` in this library to import from
            a.must 25..31: no module `sub::b` in this library to import from
            a.must 49..50: `b` declares no `g`
            a.must 55..56: `b` declares `d` more than once (declared here at b.must 39..40) (declared here at b.must 60..61)
            b.must 39..40: `d` is defined multiple times (also defined here at b.must 60..61)
            b.must 60..61: `d` is defined multiple times (also defined here at b.must 39..40)
        "#]],
    );
}

/// An unresolved name exactly one other module declares offers the `use`
/// that imports it.
#[test]
fn an_unresolved_name_offers_its_import() {
    let mut db = RootDatabase::default();
    let files = base_db::new_library(
        &mut db,
        "",
        base_db::fixture_files(
            r#"
//- a.must
static f = fn() -> usize { g(1) };
//- sub/b.must
static g = fn(n: usize) -> usize { n };
"#,
        ),
    );
    let fixes: Vec<_> = crate::file_diagnostics(&db, files[0])
        .into_iter()
        .filter_map(|d| d.fix)
        .map(|fix| (fix.label, fix.edits[0].insert.clone()))
        .collect();
    assert_eq!(
        fixes,
        [(
            "Import `g` from `sub::b`".to_owned(),
            "use sub::b::g;\n".to_owned()
        )]
    );
}

/// A type or trait the file lacks points at the modules that declare one,
/// wherever it is written: an annotation, a variant's base, a bound, an
/// impl head, a `::` path or a constructor in a body, a pattern.
/// Only declarations that fit the position count: `Shape` the static is no
/// type to offer.
#[test]
fn an_unknown_type_or_trait_offers_its_import() {
    check_library_diagnostics(
        r#"
//- a.must
static area = fn(s: Shape) -> usize { 1 };
static dot = fn(s: Shape::Dot) -> usize { 1 };
static size = fn::<T: Size>(x: T) -> usize { 1 };
type Box = struct { n: usize } with {
    impl Size {
        size = fn(b: Self) -> usize { b.n };
    }
};
trait Grow = requires { grow: fn(Self) -> usize; } with {
    impl Shape {
        grow = fn(s: Self) -> usize { 1 };
    }
};
static body = fn() -> usize {
    let s = Shape::Dot;
    let Pair(p) = Pair(struct { n = 1 });
    match s { Shape::Dot => p.n }
};
//- shapes.must
type Shape = enum { Dot };
type Pair = struct { n: usize };
trait Size = requires { size: fn(Self) -> usize; };
//- other.must
static Shape: usize = 1;
"#,
        expect![[r#"
            a.must 20..25: unknown type `Shape` (`shapes` declares `Shape` at shapes.must 5..10)
            a.must 62..72: unknown type `Shape` (`shapes` declares `Shape` at shapes.must 5..10)
            a.must 112..116: unknown trait `Size` (`shapes` declares `Size` at shapes.must 66..70)
            a.must 187..191: unknown trait `Size` (`shapes` declares `Size` at shapes.must 66..70)
            a.must 315..320: unknown type `Shape` (`shapes` declares `Shape` at shapes.must 5..10)
            a.must 417..422: unresolved name `Shape` (`shapes` declares `Shape` at shapes.must 5..10)
            a.must 437..444: `Pair` does not name a type (`shapes` declares `Pair` at shapes.must 32..36)
            a.must 447..451: unresolved name `Pair` (`shapes` declares `Pair` at shapes.must 32..36)
            a.must 485..495: `Shape` does not name an `enum` type (`shapes` declares `Shape` at shapes.must 5..10)
        "#]],
    );
}

/// Two modules declaring the name leave the choice to the reader: both are
/// pointed at, and no fix picks one.
#[test]
fn an_unresolved_name_two_modules_declare_offers_no_import() {
    check_library_diagnostics(
        r#"
//- a.must
static f = fn() -> usize { g(1) };
//- b.must
static g = fn(n: usize) -> usize { n };
//- c.must
static g = fn(n: usize) -> usize { n };
"#,
        expect![[r#"
            a.must 27..28: unresolved name `g` (`b` declares `g` at b.must 7..8) (`c` declares `g` at c.must 7..8)
        "#]],
    );
    let mut db = RootDatabase::default();
    let files = base_db::new_library(
        &mut db,
        "",
        base_db::fixture_files(
            r#"
//- a.must
static f = fn() -> usize { g(1) };
//- b.must
static g = fn(n: usize) -> usize { n };
//- c.must
static g = fn(n: usize) -> usize { n };
"#,
        ),
    );
    let fixes = (crate::file_diagnostics(&db, files[0]).into_iter()).filter(|d| d.fix.is_some());
    assert_eq!(fixes.count(), 0);
}

/// A `use` path names a module by its path from the library's root, the
/// same from every file: `sub::b` is `sub/b.must` whether the importer sits
/// at the root or beside `b.must`.
#[test]
fn a_nested_path_names_a_module_from_the_library_root() {
    check_library_diagnostics(
        r#"
//- a.must
use sub::b::g;
static x = fn() -> usize { g() };
//- sub/b.must
static g = fn() -> usize { 2 };
//- sub/c.must
use sub::b::g;
use b::g as h;
static y = fn() -> usize { g() };
"#,
        expect![[r#"
            sub/c.must 19..20: no module `b` in this library to import from
        "#]],
    );
}

/// A path starting with `self` reads from the importer's children, and each
/// leading `super` goes one module up; past the root there is no module.
#[test]
fn a_relative_path_reads_from_the_importer() {
    check_library_diagnostics(
        r#"
//- sub.must
use self::b::g;
static x = fn() -> usize { g() };
//- sub/b.must
use super::c::h;
use super::super::top::t;
use super::super::super::top::t as u;
static g = fn() -> usize { h() + t() };
//- sub/c.must
static h = fn() -> usize { 2 };
//- top.must
static t = fn() -> usize { 3 };
"#,
        expect![[r#"
            sub/b.must 47..71: no module `super::super::super::top` in this library to import from
        "#]],
    );
}

/// A file loaded without its directory has no library to import from, and
/// the error says so rather than that the file is missing.
#[test]
fn a_file_loaded_on_its_own_cannot_import() {
    check_diagnostics(
        r#"
use b::f;
"#,
        expect![[r#"
            5..6: this file is loaded on its own, so it cannot import from `b`
        "#]],
    );
}

/// An impl member's binder is checked with each side's bounds resolved
/// where they are written: the requirement's in the trait's file, the
/// member's in the impl's. A renamed import of the bound's trait matches,
/// and a namesake trait of the impl's file does not.
#[test]
fn an_impl_binder_resolves_each_side_in_its_own_file() {
    check_library_diagnostics(
        r#"
//- b.must
trait W = requires { p: fn(Self) -> usize; };
trait D = requires { m: fn::<X: W>(X, Self) -> usize; };
//- a.must
use b::{D, W as V};
type P = struct { a: usize } with {
    impl D { m = fn::<X: V>(v: X, x: P) -> usize { 1 }; }
};
//- c.must
use b::D;
trait W = requires { q: fn(Self) -> usize; };
type Q = struct { a: usize } with {
    impl D { m = fn::<X: W>(v: X, x: Q) -> usize { 1 }; }
};
"#,
        // TODO: halvko/must#163: bounds may come in any order; the message stops naming one.
        expect![[r#"
            c.must 105..106: member `m`'s generic binder does not match `D`'s requirement (arity, kinds and bounds must agree, the bounds in the requirement's order) (required by the trait here at b.must 67..68)
        "#]],
    );
}

/// The trait's impl in one file and the type's in another implement the
/// same pair: the later one is the duplicate, and it names the first.
#[test]
fn an_impl_duplicated_across_files_of_a_library_names_the_first() {
    check_library_diagnostics(
        r#"
//- a.must
use b::P;
trait D = requires { m: fn(Self) -> usize; } with {
    impl P { m = fn(x: Self) -> usize { 1 }; }
};
//- b.must
use a::D;
type P = struct { a: usize } with {
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
"#,
        expect![[r#"
            b.must 55..56: duplicate impl of `D` for `P` (first implemented here at a.must 71..72)
        "#]],
    );
}

/// Binding groups span the library: mutual recursion across files infers,
/// and a caller in one file pins the params of a callee in another.
#[test]
fn unannotated_items_infer_across_files() {
    check_library_diagnostics(
        r#"
//- a.must
use b::is_odd;
static is_even = fn (n: usize) {
    if n == 0 { true } else { is_odd(n - 1) }
};
static apply = fn (f, a) { f(a) };
//- b.must
use a::{is_even, apply};
static is_odd = fn (n: usize) {
    if n == 0 { false } else { is_even(n - 1) }
};
static double = fn (n: usize) { n + n };
static main = fn { is_even(apply(double, 2)) };
"#,
        expect![[""]],
    );
}

/// A dot-call needs no import of the trait that answers it.
#[test]
fn a_trait_impl_in_another_file_answers_a_dot_call() {
    check_library_diagnostics(
        r#"
//- a.must
use b::Marker;
static f = fn() -> usize { Marker(struct { n = 2 }).twice() };
//- b.must
use c::Twice;
type Marker = struct { n: usize } with {
    impl Twice { twice = fn(x: Self) -> usize { x.n + x.n }; }
};
//- c.must
trait Twice = requires { twice: fn(Self) -> usize; };
"#,
        expect![[""]],
    );
}

/// A callee's bounds name traits in the callee's file: a caller that does
/// not import the trait still owes the obligation.
#[test]
fn a_bound_is_owed_where_the_caller_does_not_see_its_trait() {
    check_library_diagnostics(
        r#"
//- a.must
use b::{f, P};
static main = fn() -> usize { f::<P>(P(struct { a = 1 })) };
//- b.must
trait D = requires { m: fn(Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
"#,
        expect![[r#"
            a.must 45..51: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

/// A trait of the caller's own that shares the bound's name does not
/// answer it.
#[test]
fn a_bound_is_not_answered_by_a_namesake_trait_of_the_caller() {
    check_library_diagnostics(
        r#"
//- a.must
use b::{f, P};
trait D = requires { m: fn(Self) -> str; } with {
    impl P { m = fn(x: Self) -> str { "no" }; }
};
static main = fn() -> usize { f::<P>(P(struct { a = 1 })) };
//- b.must
trait D = requires { m: fn(Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
"#,
        expect![[r#"
            a.must 146..152: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

#[test]
fn files_outside_a_library_do_not_see_each_other() {
    let db = RootDatabase::default();
    let _a = SourceFile::new(&db, "a.must".to_owned(), "static g = 1;".to_owned());
    let b = SourceFile::new(
        &db,
        "b.must".to_owned(),
        "static f = fn() -> usize { g };".to_owned(),
    );
    let messages: Vec<String> = crate::file_diagnostics(&db, b)
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(messages, ["unresolved name `g`"]);
}

/// An error outside every item traps only its own library: a file in no
/// library is one by itself.
#[test]
fn an_error_outside_every_item_does_not_trap_another_library() {
    let db = RootDatabase::default();
    let _a = SourceFile::new(
        &db,
        "a.must".to_owned(),
        r#"
static one = 1;
\
"#
        .to_owned(),
    );
    let b = SourceFile::new(
        &db,
        "b.must".to_owned(),
        "static f = fn() -> usize { 2 };".to_owned(),
    );
    let item = crate::file_item_ids(&db, b)[0];
    assert_eq!(crate::trap_sites::range_traps(&db, item), &[]);
}

#[test]
fn a_safety_edge_out_of_a_cycle_reaches_the_callee_it_feeds() {
    // `p` passes `y`'s result, whose safety is open while the `p`-`y`
    // cycle is checked, to `x`'s parameter. `y`'s body makes it unsafe;
    // `x` already called its parameter outside `unsafe { }`, so the
    // argument is refused at `p`.
    check_unsafe_fn(
        r#"
static x = fn(f, ptr: u8.&raw mut, n: usize) -> () { let r: isize = f(ptr, n); };
static y = fn(ptr: u8.&raw mut, n: usize) { p(ptr, n); read };
static p = fn(ptr: u8.&raw mut, n: usize) -> () { x(y(ptr, n), ptr, n); };"#,
        expect![[r#"
            333..342: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize`
        "#]],
    );
}
