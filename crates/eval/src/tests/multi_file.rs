//! Programs across the files of a library.

use super::*;

#[test]
fn a_program_runs_across_the_files_of_its_library() {
    check_run(
        r#"
//- b.must
static double = fn(n: usize) -> usize { n + n };
//- a.must
use b::double;
static main = fn() -> usize { double(2) };
"#,
        "main()",
        expect![[r#"
            => 4
        "#]],
    );
}

/// A fully typed caller fills the holes of a callee in another file.
#[test]
fn a_fully_typed_caller_fills_a_callee_in_another_file() {
    check_run(
        r#"
//- b.must
static id = fn(x) { x };
//- a.must
use b::id;
static main = fn() -> usize { id(1) + 1 };
"#,
        "main()",
        expect![[r#"
            => 2
        "#]],
    );
}

/// A `ReadLineResult` made in one file matches the patterns of another: both
/// name the prelude's declaration.
#[test]
fn a_compiler_provided_enum_is_one_type_across_files() {
    check_run(
        r#"
//- b.must
static line = fn() -> ReadLineResult { read_line() };
//- a.must
use b::line;
static main = fn() -> usize {
    match line() { ReadLineResult::Line(s) => 1, ReadLineResult::End => 2 }
};
"#,
        "main()",
        expect![[r#"
            => 2
        "#]],
    );
}

/// A generic callee in another file gets its dictionary from a caller that
/// does not import the bound's trait.
#[test]
fn a_bound_from_another_file_passes_its_dictionary() {
    check_run(
        r#"
//- b.must
trait D = requires { m: fn(Self) -> usize; };
type P = struct { a: usize } with {
    impl D { m = fn(x: Self) -> usize { x.a + 40 }; }
};
static f = fn::<T: D>(x: T) -> usize { x.m() };
//- a.must
use b::{f, P};
static main = fn() -> usize { f::<P>(P(struct { a = 2 })) };
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

/// A dot-call's impl names its trait in the impl's file: a namesake trait of
/// the caller's file, broken or not, has nothing to do with the call.
#[test]
fn a_dot_call_is_not_blamed_on_a_namesake_trait_of_the_caller() {
    check_run(
        r#"
//- b.must
trait Show = requires { show: fn(Self) -> usize; };
type P = struct { a: usize } with {
    impl Show { show = fn(x: Self) -> usize { x.a + 1 }; }
};
//- a.must
use b::P;
trait Show = requires { other: fn(Self) -> Nope; };
static main = fn() -> usize { P(struct { a = 5 }).show() };
"#,
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

/// A broken import traps nothing that does not name what it imports.
#[test]
fn a_broken_import_leaves_an_item_that_does_not_name_it_running() {
    check_run(
        r#"
//- b.must
static g = fn() -> usize { 1 };
//- a.must
use b::missing;
static main = fn() -> usize { print("a;"); 2 };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            => 2
        "#]],
    );
}

/// A name a broken import fails to bring in traps where it is named, with
/// the import's error.
#[test]
fn a_broken_import_traps_where_its_name_is_used() {
    check_run(
        r#"
//- b.must
static g = fn() -> usize { 1 };
//- a.must
use b::missing;
static main = fn() -> usize { print("a;"); missing() };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `b` declares no `missing`
        "#]],
    );
}

/// An import that clashes with the file's own name traps where the name is
/// used, though the name still reads the file's own item.
#[test]
fn a_clashing_import_traps_where_its_name_is_used() {
    check_run(
        r#"
//- b.must
static g = fn() -> usize { 1 };
//- a.must
use b::g;
static g = fn() -> usize { 2 };
static main = fn() -> usize { print("a;"); g() };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `g` is defined multiple times
        "#]],
    );
}

/// A clash is an error only in the file that has it: another file that
/// imports the clashing item sees one name, and its call runs.
#[test]
fn a_clash_does_not_trap_a_file_that_imports_the_item() {
    check_run(
        r#"
//- b.must
static f = fn() -> usize { 1 };
//- a.must
use b::f;
static f = fn() -> usize { 4 };
//- c.must
use a::f;
static main = fn() -> usize { f() };
"#,
        "main()",
        expect![[r#"
            => 4
        "#]],
    );
}

/// An import of a name its module declares twice traps where the importer
/// names it, with the error at the `use`.
#[test]
fn an_import_of_a_twice_declared_name_traps_where_it_is_used() {
    check_run(
        r#"
//- b.must
static d: usize = 1;
static d: usize = 2;
//- a.must
use b::d;
static main = fn() -> usize { print("a;"); d };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `b` declares `d` more than once
        "#]],
    );
}

/// An error in the syntax of a `use` traps where its names are used.
#[test]
fn a_malformed_import_traps_where_its_name_is_used() {
    check_run(
        r#"
//- b.must
static g = fn() -> usize { 1 };
//- a.must
use b::{g h};
static main = fn() -> usize { print("a;"); g() };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: expected `,` or `}`
        "#]],
    );
}

/// An error in a bodiless declaration traps where another file names it.
#[test]
fn a_broken_declaration_traps_where_another_file_names_it() {
    check_run(
        BROKEN_DECLARATION_IN_ANOTHER_FILE,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: unknown type `Bogus`
        "#]],
    );
}

/// An item that does not name the broken declaration runs.
#[test]
fn a_broken_declaration_leaves_an_item_that_does_not_name_it_running() {
    check_run(
        BROKEN_DECLARATION_IN_ANOTHER_FILE,
        "other()",
        expect![[r#"
            => 3
        "#]],
    );
}

/// A dot-call names the trait its impl implements, though the caller
/// imports neither: a broken requirement of that trait traps the call.
#[test]
fn a_dot_call_traps_on_a_broken_trait_the_caller_does_not_import() {
    check_run(
        r#"
//- c.must
trait Show = requires { show: fn(Self) -> usize; hide: fn(Self) -> Nope; };
//- b.must
use c::Show;
type P = struct { a: usize } with {
    impl Show { show = fn(x: Self) -> usize { x.a }; hide = fn(x: Self) -> Nope { x }; }
};
//- a.must
use b::P;
static main = fn() -> usize { print("a;"); P(struct { a = 1 }).show() };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: unknown type `Nope`
        "#]],
    );
}

/// An ambiguous name names nothing, so a declaration or bound that spells
/// one is broken: the clash, or the import of a name declared twice, traps
/// where that code is named, not only where the name itself is.
#[test]
fn a_clash_traps_where_a_declaration_spelling_it_is_named() {
    check_run(
        r#"
//- b.must
type T = struct { x: usize };
//- a.must
use b::T;
type T = struct { y: bool };
type U = struct { a: T, b: usize };
static main = fn() -> usize {
    print("a;");
    let u = U(struct { a = true, b = 2 });
    let v: usize = u.a;
    v + 1
};
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `T` is defined multiple times
        "#]],
    );
    check_run(
        r#"
//- b.must
type T = struct { x: usize };
type T = struct { y: bool };
//- a.must
use b::T;
type U = struct { a: T, b: usize };
static main = fn() -> usize {
    print("a;");
    let u = U(struct { a = true, b = 2 });
    let v: usize = u.a;
    v + 1
};
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `T` is defined multiple times
        "#]],
    );
    check_run(
        r#"
//- b.must
trait Get = requires { get: fn(Self) -> usize; };
//- a.must
use b::Get;
trait Get = requires { get: fn(Self) -> usize; };
static f = fn::<X: Get>(x: X) -> usize { 1 };
static main = fn() -> usize { print("a;"); f::<bool>(true) };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `Get` is defined multiple times
        "#]],
    );
    check_run(
        r#"
//- b.must
trait Get = requires { get: fn(Self) -> usize; };
//- a.must
use b::Get;
trait Get = requires { get: fn(Self) -> usize; };
type Counter = struct { count: usize } with {
    impl Get { get = fn(c: Self) -> usize { c.count }; }
};
static main = fn() -> usize { print("a;"); Counter(struct { count = 3 }).count };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: `Get` is defined multiple times
        "#]],
    );
}

/// A name that only looks like a clashing one, a generic parameter or a
/// field, is not a spelling of it: the declaration runs.
#[test]
fn a_clash_leaves_a_declaration_that_only_resembles_its_name_running() {
    check_run(
        r#"
//- a.must
type T = struct { x: usize };
type T = struct { y: bool };
type W = struct::<T> { a: T };
static main = fn() -> usize {
    let w = W::<usize>(struct { a = 2 });
    w.a
};
"#,
        "main()",
        expect![[r#"
            => 2
        "#]],
    );
    check_run(
        r#"
//- b.must
static count: usize = 1;
//- a.must
use b::count;
static count: usize = 2;
trait Get = requires { get: fn(Self) -> usize; };
type Counter = struct { count: usize } with {
    impl Get { get = fn(c: Self) -> usize { c.count }; }
};
static main = fn() -> usize { Counter(struct { count = 3 }).get() };
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

/// An error outside every item traps every item of its file, and another
/// file's code where it names one of the file's items. Code that names
/// nothing from the file runs.
#[test]
fn an_error_outside_every_item_traps_its_file_and_what_names_it() {
    let own_file = r#"
//- a.must
static one: usize = 1;
\
static main = fn() -> usize { print("a;"); 2 };
"#;
    check_run(
        own_file,
        "main()",
        expect![[r#"
            error[Trap]: unexpected character `\`
        "#]],
    );
    check_run(
        own_file,
        "one",
        expect![[r#"
            error[Trap]: unexpected character `\`
        "#]],
    );
    check_run(
        r#"
//- a.must
static one: usize = 1;
\
//- b.must
static main = fn() -> usize { print("a;"); 2 };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            => 2
        "#]],
    );
    check_run(
        r#"
//- a.must
static one: usize = 1;
\
//- c.must
use a::one;
static main = fn() -> usize { print("a;"); one };
"#,
        "main()",
        expect![[r#"
            output: "a;"
            error[Trap]: unexpected character `\`
        "#]],
    );
}
