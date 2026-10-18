//! Libraries of several files: imports, paths and cross-file inference.

use super::*;

/// Renders the diagnostics of every file of a library fixture (see
/// [`base_db::fixture_files`]), each prefixed with its file's path.
fn check_library_diagnostics(fixture: &str, expect: Expect) {
    let mut db = RootDatabase::default();
    let files = base_db::new_library(&mut db, "", base_db::fixture_files(fixture));
    let mut rendered = String::new();
    for file in files {
        for d in crate::file_diagnostics(&db, file) {
            let related = d
                .related
                .iter()
                .map(|r| format!(" ({} at {} {:?})", r.message, r.file.path(&db), r.range))
                .collect::<String>();
            rendered.push_str(&format!(
                "{} {:?}: {}{related}\n",
                file.path(&db),
                d.range,
                d.message
            ));
        }
    }
    expect.assert_eq(&rendered);
}

// ---- place chains ---------------------------------------------------------

/// Every field and index expression as [`crate::Body::place_chain`] takes
/// it apart: `range 'snippet': kind 'root' [steps, outermost first]`.
fn check_place_chains(text: &str, expect: Expect) {
    use crate::body::{ExprData, PlaceChain, PlaceRoot};
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rows: Vec<(u32, String)> = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (body, source_map) = crate::body_with_source_map(&db, item);
        let range = |expr| source_map.node_for_expr(expr).unwrap().text_range();
        let snippet = |expr| &text[range(expr)];
        for (expr, data) in body.exprs.iter() {
            if !matches!(data, ExprData::Field { .. } | ExprData::Index { .. }) {
                continue;
            }
            let PlaceChain { root, kind, steps } = body.place_chain(expr);
            assert_eq!(body.place_root(expr), (root, kind));
            let kind = match kind {
                PlaceRoot::Deref { receiver } => format!("deref of '{}'", snippet(receiver)),
                PlaceRoot::Name(name) => format!("name `{name}`"),
                PlaceRoot::Temporary(_) => "temporary".to_owned(),
                PlaceRoot::Missing => "missing".to_owned(),
                PlaceRoot::Value => "value".to_owned(),
            };
            let steps: Vec<_> = steps.iter().map(|&step| snippet(step)).collect();
            rows.push((
                range(expr).start().into(),
                format!(
                    "{:?} '{}': {kind} '{}' {steps:?}\n",
                    range(expr),
                    snippet(expr),
                    snippet(root)
                ),
            ));
        }
    }
    rows.sort();
    expect.assert_eq(&rows.into_iter().map(|(_, row)| row).collect::<String>());
}

/// The source for the turbofish tests: `h` reads `p.{field}::<usize>`,
/// with `p` annotated as written in `param`, and `main` calls it with a `P`.
fn turbofish_on_a_field_access(param: &str, field: &str) -> String {
    format!(
        r#"
type P = struct {{ x: usize }} with {{ impl Self {{ m = fn (a: Self) -> usize {{ a.x }}; }} }};
static h = fn ({param}) -> usize {{ p.{field}::<usize> }};
static main = fn () -> usize {{ h(P(struct {{ x = 1 }})) }};
"#
    )
}

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
            c.must 105..106: member `m`'s generic binder does not match `D`'s requirement (arity, kinds and bounds must agree, the bounds in the requirement's order) (`m` is required by `D` here at b.must 67..68)
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
            b.must 50..56: duplicate impl of `D` for `P` (first implemented here at a.must 66..72)
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
static x = fn(f, ptr: u8.&raw, n: usize) -> () { let r: isize = f(ptr, n); };
static y = fn(ptr: u8.&raw, n: usize) { p(ptr, n); read };
static p = fn(ptr: u8.&raw, n: usize) -> () { x(y(ptr, n), ptr, n); };"#,
        expect![[r#"
            314..323: type mismatch: expected `fn(u8.&raw, usize) -> isize`, found `unsafe fn(u8.&raw, usize) -> isize`
        "#]],
    );
}

#[test]
fn unfinished_body_squiggles_only_its_closing_brace() {
    // The diagnostic inside the body keeps its own squiggle.
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize } with {
    impl Self {
        d = fn::<@a>(other: Self.&::<@a>, self: Self.&::<@a>) -> usize {
            let n = other.z;
        }
    }
};
"#,
        expect![[r#"
            160..167: `Point.&::<@a>` is a borrow, so `.z` does not reach through it — there is no auto-deref; write `.*.z`
            177..178: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 129..137)
        "#]],
    );
}

#[test]
fn unit_valued_block_squiggles_only_its_closing_brace() {
    // A plain, `const` and `unsafe` block, and the empty `then` branch of
    // an `if`/`else`.
    check_diagnostics(
        r#"
static x: usize = { };
static y: usize = const { };
static f = fn (b: bool) -> usize { if b { } else { 3 } };
static g = fn () -> usize { unsafe { } };
"#,
        expect![[r#"
            21..22: type mismatch: expected `usize`, found `()` (expected `usize` because of this annotation at 11..16)
            50..51: type mismatch: expected `usize`, found `()` (expected `usize` because of this annotation at 34..39)
            95..96: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 77..85)
            148..149: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 128..136)
        "#]],
    );
}

#[test]
fn mismatched_block_with_a_tail_squiggles_the_tail() {
    // The pointer check blames the `{ p }` argument; the squiggle lands on
    // its tail.
    check_diagnostics(
        r#"static f = fn (p: u16.&raw) -> () { unsafe { str_from_utf8({ p }, 1); }; };"#,
        expect![[r#"
            61..62: type mismatch: expected `u8.&raw`, found `u16.&raw`
        "#]],
    );
}

#[test]
fn unclosed_block_squiggles_the_whole_block() {
    check_diagnostics(
        r#"static f = fn () -> usize { let x = 1;"#,
        expect![[r#"
            26..38: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 17..25)
            36..37: cannot infer the type of this number: it has no defining use — add a type annotation
            37..38: expected `}`
        "#]],
    );
}

#[test]
fn if_without_else_squiggles_its_last_closing_brace() {
    // The `()` comes from the missing final `else`, after the last branch.
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { if n > 0 { 1 } else if n > 1 { 2 } };"#,
        expect![[r#"
            67..68: type mismatch: expected `()`, found `{number}` (this `if` has no `else`, so its value is `()` at 56..58)
            69..70: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn mismatched_if_with_a_diverging_else_squiggles_the_other_branch() {
    // The `if`'s value is the `then` branch's: the `else` never produces one.
    check_diagnostics(
        r#"static f = fn (b: bool) -> usize { if b { true } else { return 1; } };"#,
        expect![[r#"
            42..46: type mismatch: expected `usize`, found `bool` (expected `usize` because of this return type at 24..32)
        "#]],
    );
}

#[test]
fn mismatched_match_with_diverging_arms_squiggles_the_live_arm() {
    // The `match`'s value is the `_` arm's: the other arm never produces one.
    check_diagnostics(
        r#"static f = fn (n: usize) -> usize { match n { 0 => return 1, _ => "x" } };"#,
        expect![[r#"
            66..69: type mismatch: expected `usize`, found `str` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn mismatch_every_branch_shares_squiggles_the_keyword() {
    // No one branch is to blame when they all agree with each other.
    check_diagnostics(
        r#"
static f = fn (b: bool) -> usize { if b { "a" } else { "b" } };
static g = fn (n: usize) -> usize { match n { 0 => "a", _ => "b" } };
static h = fn (b: bool) -> usize { loop { if b { break "a"; }; break "b"; } };
"#,
        expect![[r#"
            36..38: every branch produces `str`, but `usize` is needed (expected `usize` because of this return type at 25..33)
            101..106: every branch produces `str`, but `usize` is needed (expected `usize` because of this return type at 90..98)
            170..174: every branch produces `str`, but `usize` is needed (expected `usize` because of this return type at 159..167)
        "#]],
    );
}

#[test]
fn dropped_value_squiggles_its_semicolon() {
    // The `;` after `x` drops the value the body was meant to return.
    check_diagnostics(
        r#"static f = fn (x: usize) -> usize { x; };"#,
        expect![[r#"
            37..38: type mismatch: expected `usize`, found `()` (expected `usize` because of this return type at 25..33)
        "#]],
    );
}

#[test]
fn block_around_an_if_with_one_live_branch_squiggles_that_branch() {
    // The pointer check blames the `{ .. }` argument whole; its value is
    // `p`, since the `else` diverges.
    check_diagnostics(
        r#"static f = fn (p: u16.&raw, b: bool) -> () { unsafe { str_from_utf8({ if b { p } else { return; } }, 1); }; };"#,
        expect![[r#"
            77..78: type mismatch: expected `u8.&raw`, found `u16.&raw`
        "#]],
    );
}

/// A use that loses to a decision the group then leaves `{error}` lost to
/// nothing: the members' bodies contradict each other.
#[test]
fn members_whose_bodies_contradict_each_other_disagree() {
    let src = r#"
static c = true;
static x = if c { 1 } else { y() };
static y = fn () { x(2) };
"#;
    check_signatures(
        src,
        expect![[r#"
            c: bool
            x: {error}
            y: fn() -> {error}
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            47..48: `y` and `x` disagree on each other's types; add type annotations to their definitions (defined here at 61..62) (`x` is defined here at 25..26)
            47..48: cannot call `y` in a const context; marking it `const fn` would allow this (`y` is defined here at 61..62) (this item's initializer is a const context at 18..24)
            73..74: `x` and `y` disagree on each other's types; add type annotations to their definitions (defined here at 25..26) (`y` is defined here at 61..62)
        "#]],
    );
}

/// A chain stops at its outermost deref, and its root is classified once
/// for every layer: a name, a deref, a borrowed value's temporary, or a
/// value read with no storage of its own.
#[test]
fn place_chains_stop_at_the_outermost_deref_and_classify_the_root() {
    let text = r#"
type In = struct { b: [usize; 2] };
type Out = struct { a: In };
static mk = fn() -> Out { Out(struct { a = In(struct { b = [1, 2] }) }) };
static f = fn::<@a, @b>(o: Out, r: Out.&mut::<@a>.&mut::<@b>) -> usize {
    let x = o.a.b[0];
    let y = r.*.*.a.b[1].&;
    mk().a.b[0].& .* + mk().a.b[1] + x + y.*
};
"#;
    check_diagnostics(text, expect![""]);
    check_place_chains(
        text,
        expect![[r#"
            226..229 'o.a': name `o` 'o' ["o.a"]
            226..231 'o.a.b': name `o` 'o' ["o.a.b", "o.a"]
            226..234 'o.a.b[0]': name `o` 'o' ["o.a.b[0]", "o.a.b", "o.a"]
            248..255 'r.*.*.a': deref of 'r.*' 'r.*.*' ["r.*.*.a"]
            248..257 'r.*.*.a.b': deref of 'r.*' 'r.*.*' ["r.*.*.a.b", "r.*.*.a"]
            248..260 'r.*.*.a.b[1]': deref of 'r.*' 'r.*.*' ["r.*.*.a.b[1]", "r.*.*.a.b", "r.*.*.a"]
            268..274 'mk().a': temporary 'mk()' ["mk().a"]
            268..276 'mk().a.b': temporary 'mk()' ["mk().a.b", "mk().a"]
            268..279 'mk().a.b[0]': temporary 'mk()' ["mk().a.b[0]", "mk().a.b", "mk().a"]
            287..293 'mk().a': value 'mk()' ["mk().a"]
            287..295 'mk().a.b': value 'mk()' ["mk().a.b", "mk().a"]
            287..298 'mk().a.b[1]': value 'mk()' ["mk().a.b[1]", "mk().a.b", "mk().a"]
        "#]],
    );
}

/// A use whose member waits on a deferred projection votes with what the
/// projection gives, so callers' plurality decides: two callers make
/// `f` take an `A`, one a `B`, and the `B` one is outvoted.
#[test]
fn uses_vote_with_what_the_member_projects() {
    let src = r#"
type A = struct { x: usize };
type B = struct { x: bool };
static a = A(struct { x = 1 });
static b = B(struct { x = true });
static f = fn (p) { p.x };
static m1 = fn () { f(a); };
static m2 = fn () { f(a); };
static m3 = fn () { f(b); };
"#;
    check_signatures(
        src,
        expect![[r#"
        a: A
        b: B
        f: fn(A) -> usize
        m1: fn()
        m2: fn()
        m3: fn()
    "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            232..233: `f` is `fn(A) -> usize` here, as `m1` and `m2` make it, but this use makes it `fn(B) -> bool` (`m1` is defined here at 161..163) (`m2` is defined here at 190..192)
        "#]],
    );
}

/// One caller each: a tie, which asks for the annotation at the uses; the
/// projection in `f` doesn't ask again.
#[test]
fn a_tie_on_a_member_with_a_deferred_projection_reports_once() {
    let src = r#"
type A = struct { x: usize };
type B = struct { x: bool };
static a = A(struct { x = 1 });
static b = B(struct { x = true });
static f = fn (p) { p.x };
static g = fn () -> usize { f(a) };
static h = fn () -> bool { f(b) };
"#;
    check_signatures(
        src,
        expect![[r#"
            a: A
            b: B
            f: {error}
            g: fn() -> usize
            h: fn() -> bool
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            182..183: callers disagree on the type of `f`: `g` makes it `fn(A) -> usize`, `h` makes it `fn(B) -> bool`; add a type annotation to its definition (defined here at 134..135) (`g` is defined here at 161..162) (`h` is defined here at 197..198)
            217..218: callers disagree on the type of `f`: `g` makes it `fn(A) -> usize`, `h` makes it `fn(B) -> bool`; add a type annotation to its definition (defined here at 134..135) (`g` is defined here at 161..162) (`h` is defined here at 197..198)
        "#]],
    );
}

/// `f`'s use of `k` waits for `f`'s receiver, which `g` determines: `k` is
/// decided after `f`, so the use votes as `fn(bool) -> bool` and ties
/// with `u`'s, exactly as with `p: B` written.
#[test]
fn a_use_through_a_deferred_projection_votes_as_if_annotated() {
    let src = |param: &str| {
        format!(
            r#"
type B = struct {{ x: bool }};
static b = B(struct {{ x = true }});
static k = fn (y) {{ y }};
static u = fn () {{ let _: usize = k(1); }};
static f = fn ({param}) {{ k(p.x) }};
static g = fn () {{ f(b); }};
"#
        )
    };
    check_signatures(
        &src("p"),
        expect![[r#"
        b: B
        k: {error}
        u: fn()
        f: fn(B) -> bool
        g: fn()
    "#]],
    );
    check_signatures(
        &src("p: B"),
        expect![[r#"
        b: B
        k: {error}
        u: fn()
        f: fn(B) -> bool
        g: fn()
    "#]],
    );
    check_diagnostics(
        &src("p"),
        expect![[r#"
            124..125: callers disagree on the type of `k`: `u` makes it `fn(usize) -> usize`, `f` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 72..73) (`u` is defined here at 97..98) (`f` is defined here at 140..141)
            153..154: callers disagree on the type of `k`: `u` makes it `fn(usize) -> usize`, `f` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 72..73) (`u` is defined here at 97..98) (`f` is defined here at 140..141)
        "#]],
    );
    check_diagnostics(
        &src("p: B"),
        expect![[r#"
            124..125: callers disagree on the type of `k`: `u` makes it `fn(usize) -> usize`, `f` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 72..73) (`u` is defined here at 97..98) (`f` is defined here at 140..141)
            156..157: callers disagree on the type of `k`: `u` makes it `fn(usize) -> usize`, `f` makes it `fn(bool) -> bool`; add a type annotation to its definition (defined here at 72..73) (`u` is defined here at 97..98) (`f` is defined here at 140..141)
        "#]],
    );
}

/// A body with a deferred projection and a use that lost: the group
/// decides it like any other body.
#[test]
fn a_deferred_projection_beside_an_outvoted_use() {
    let src = r#"
type B = struct { x: bool };
static b = B(struct { x = true });
static k = fn (y) { y };
static u1 = fn () { let _: usize = k(1); };
static u2 = fn () { let _: usize = k(2); };
static f = fn (p) { let _: bool = k(true); p.x };
static g = fn () { f(b); };
"#;
    check_signatures(
        src,
        expect![[r#"
            b: B
            k: fn(usize) -> usize
            u1: fn()
            u2: fn()
            f: fn(B) -> bool
            g: fn()
        "#]],
    );
    check_diagnostics(
        src,
        expect![[r#"
            212..213: `k` is `fn(usize) -> usize` here, as `u1` and `u2` make it, but this use makes it `fn(bool) -> bool` (`u1` is defined here at 97..99) (`u2` is defined here at 141..143)
        "#]],
    );
}

/// A deferred field whose declared type is broken projects `{error}`,
/// which binds nothing: the group still finishes, and the broken type is
/// reported at its declaration.
#[test]
fn a_deferred_field_of_a_broken_type_terminates() {
    let text = r#"
type P = struct { x: Nope };
static h = fn (p) { p.x };
static main = fn (q: P) { h(q); };
"#;
    check_signatures(
        text,
        expect![[r#"
        h: fn(P) -> {error}
        main: fn(P)
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        22..26: unknown type `Nope`
    "#]],
    );
}

/// A turbofish on a field access whose receiver is determined later is
/// refused on a field, as with a determined receiver.
#[test]
fn a_turbofish_on_a_deferred_field_is_refused_as_on_a_known_one() {
    check_diagnostics(
        &turbofish_on_a_field_access("p", "x"),
        expect![[r#"
        118..130: `x` takes no generic arguments
    "#]],
    );
    check_diagnostics(
        &turbofish_on_a_field_access("p: P", "x"),
        expect![[r#"
        121..133: `x` takes no generic arguments
    "#]],
    );
}

/// A turbofish on a member read as a field, with its receiver determined
/// later, adds no second diagnostic: the member's own is the same one a
/// determined receiver gets.
#[test]
fn a_turbofish_on_a_deferred_member_adds_nothing_to_the_members_error() {
    check_diagnostics(
        &turbofish_on_a_field_access("p", "m"),
        expect![[r#"
        118..130: `m` is a member fn, not a field; call it: `.m(...)`
    "#]],
    );
    check_diagnostics(
        &turbofish_on_a_field_access("p: P", "m"),
        expect![[r#"
        121..133: `m` is a member fn, not a field; call it: `.m(...)`
    "#]],
    );
}

/// A field access inside the const argument of a deferred field access's
/// turbofish is checked like any expression, once its own receiver is
/// determined: a missing field there is reported beside the refusal.
#[test]
fn a_field_access_in_a_deferred_turbofish_is_checked() {
    check_diagnostics(
        r#"
type P = struct { x: usize, y: usize };
static h = fn (p) -> usize { p.x::<const { p.z }> };
static main = fn () -> usize { h(P(struct { x = 1, y = 2 })) };
"#,
        expect![[r#"
            70..90: `x` takes no generic arguments
            86..87: no field `z` on `P` (the fields of `P` are declared here at 10..39)
        "#]],
    );
}

/// A field access inside the const argument of a deferred field access's
/// turbofish waits on its own receiver: with nothing to determine `p`,
/// both accesses ask for its annotation.
#[test]
fn a_field_access_in_a_deferred_turbofish_waits_for_its_receiver() {
    check_diagnostics(
        r#"
static h = fn (p) -> usize { p.x::<const { p.y }> };
"#,
        expect![[r#"
            30..31: cannot determine the type of this expression; add a type annotation
            44..45: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

/// A turbofish refused on a field still has its const arguments checked:
/// an error inside is reported beside the refusal.
#[test]
fn a_refused_field_turbofish_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type P = struct { x: usize, y: usize };
static h = fn (p: P) -> usize { p.x::<const { p.z }> };
"#,
        expect![[r#"
            73..93: `x` takes no generic arguments
            89..90: no field `z` on `P` (the fields of `P` are declared here at 10..39)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_inside_a_longer_place_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type Q = struct { y: usize };
type P = struct { x: Q };
static a = fn (p: P) -> usize { p.x::<const { p.z }>.y };
"#,
        expect![[r#"
            89..109: `x` takes no generic arguments
            105..106: no field `z` on `P` (the fields of `P` are declared here at 40..55)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_on_an_assignment_target_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type P = struct { w: usize };
static b = fn (mut p: P) { p.w::<const { p.z }> = 1; };
"#,
        expect![[r#"
            58..78: `w` takes no generic arguments
            74..75: no field `z` on `P` (the fields of `P` are declared here at 10..29)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_on_a_linear_assignment_target_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type Res = struct { id: usize } only move with {
    impl Self { drop = fn(r: Self) -> () { let Res(struct { id }) = r; }; };
};
type Holder = struct { res: Res, tag: usize };
static make = fn (id: usize) -> Res { Res(struct { id = id }) };
static f = fn (mut h: Holder) -> () { h.res::<const { h.nope }> = make(2); let Holder(struct { res, tag }) = h; let Res(struct { id }) = res; };
"#,
        expect![[r#"
            280..305: `res` takes no generic arguments
            280..305: this place still holds a value that must be consumed; assigning here would lose it
            298..302: no field `nope` on `Holder` (the fields of `Holder` are declared here at 144..175)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_in_a_borrowed_place_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type P = struct { w: usize };
static c = fn (p: P) { let r = p.w::<const { p.z }>.&; };
"#,
        expect![[r#"
            62..82: `w` takes no generic arguments
            78..79: no field `z` on `P` (the fields of `P` are declared here at 10..29)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_behind_a_refused_deref_checks_its_const_arguments() {
    check_diagnostics(
        r#"
type P = struct { w: usize };
static d = fn (p: P.&raw) -> usize { p.*.w::<const { p.*.z }> };
"#,
        expect![[r#"
            68..71: dereferencing a raw pointer requires an `unsafe { ... }` block
            68..92: `w` takes no generic arguments
            84..87: dereferencing a raw pointer requires an `unsafe { ... }` block
            88..89: no field `z` on `P` (the fields of `P` are declared here at 10..29)
        "#]],
    );
}

#[test]
fn a_refused_turbofish_behind_a_broken_deref_checks_its_const_arguments() {
    check_diagnostics(
        r#"
static e = fn (p: Nope) -> usize { p.*.w::<const { nope }> };
"#,
        expect![[r#"
            19..23: unknown type `Nope`
            52..56: unresolved name `nope`
        "#]],
    );
}
