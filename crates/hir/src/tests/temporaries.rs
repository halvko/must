//! Materialized temporaries.

use super::*;

/// Each binding reports its own kind and display name; only a written name
/// is something a scope can bind.
#[test]
fn every_binding_reports_its_kind_and_display_name() {
    let db = RootDatabase::default();
    let text = r#"
static mk = fn() -> usize { 5 };
static main = fn() -> () {
    let struct { mut } = struct { x = 2 };
    let struct { y as _ } = struct { y = 1 };
    let p = mk().&raw;
    unsafe { p.*; }
};
"#;
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rows = Vec::new();
    for &item in crate::file_item_ids(&db, file) {
        let (body, _) = crate::body_with_source_map(&db, item);
        for (_, data) in body.bindings.iter() {
            rows.push(format!(
                "{:?} shows as {:?}, written {:?}\n",
                data.kind,
                data.name(),
                data.written_name()
            ));
        }
    }
    expect![[r#"
        Missing shows as "<missing>", written None
        Hole shows as "_", written None
        Temporary shows as "<temporary>", written None
        Named("p") shows as "p", written Some("p")
    "#]]
    .assert_eq(&rows.concat());
}

/// `inner.fmt(Printer(struct {}).&mut)`: a freshly built value borrowed
/// exclusively, straight into a member that writes through it, checks
/// clean.
#[test]
fn the_playground_shape_borrows_a_temporary() {
    check_diagnostics(
        r#"
type Printer = struct { count: usize } with {
    impl Self {
        emit = fn::<@a>(s: str, p: Self.&mut::<@a>) -> () {
            p.*.count = p.*.count + 1;
        };
    }
};
type Inner = struct { label: str } with {
    impl Self {
        fmt = fn::<@a>(p: Printer.&mut::<@a>, i: Self) -> usize {
            p.emit(i.label);
            p.*.count
        };
    }
};
static main = fn() -> usize {
    let inner = Inner(struct { label = "x" });
    inner.fmt(Printer(struct { count = 0 }).&mut)
};
"#,
        expect![""],
    );
}

/// The mark lands on the ROOT of the borrowed place: field and element
/// roots, a receiver-position temporary, and a nested one.
#[test]
fn a_borrow_reaches_into_the_temporary_it_materialized() {
    let text = r#"
type Pair = struct { a: usize, b: usize };
type Cell = struct { v: usize } with {
    impl Self {
        bump = fn::<@a>(by: usize, c: Self.&mut::<@a>) -> usize {
            c.*.v = c.*.v + by;
            c.*.v
        };
    }
};
static mk = fn() -> Pair { Pair(struct { a = 3, b = 4 }) };
static arr = fn() -> [usize; 2] { [1, 2] };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static main = fn() -> usize {
    get(mk().a.&) + get(arr()[0].&) + get(get(mk().b.&).&) + Cell(struct { v = 1 }).&mut.bump(1)
};
"#;
    check_diagnostics(text, expect![""]);
    check_temps(
        text,
        expect![[r#"
            435..439 'mk()'
            451..456 'arr()'
            469..482 'get(mk().b.&)'
            473..477 'mk()'
            488..510 'Cell(struct { v = 1 })'
        "#]],
    );
}

/// A temporary inside a `match` arm, an `if` condition, a loop body and a
/// `const` context checks clean.
#[test]
fn a_temporary_in_a_branch_a_loop_or_a_const_context_is_borrowable() {
    check_diagnostics(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static mk = fn() -> usize { 7 };
static cget = const fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static cmk = const fn() -> usize { 7 };
static folded: usize = cget(cmk().&);
static main = fn(n: usize) -> usize {
    let a = match n { 0 => get(mk().&), _ => 0 };
    let b = if get(mk().&) == 7 { 1 } else { 2 };
    let mut i: usize = 0;
    loop {
        if i == get(mk().&) { break; };
        i = i + 1;
    };
    a + b + i + const { cget(cmk().&) } + folded
};
"#,
        expect![""],
    );
}

/// A deref-rooted place gets no temporary: `p.*.x` names storage the
/// pointer already points at.
#[test]
fn a_deref_rooted_place_is_a_place_and_gets_no_temporary() {
    check_temps(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn::<@a>(p: Pair.&mut::<@a>) -> Pair.&mut::<@a> { p };
static main = fn() -> usize {
    let mut pair = Pair(struct { a = 1, b = 2 });
    let r = mk(pair.&mut).*.a.&;
    r.*
};
"#,
        expect![""],
    );
}

/// `.&raw` of a temporary checks clean and marks the root, so
/// `mkp().a.&raw mut` points into the temporary's own field.
#[test]
fn a_temporary_is_materialized_under_raw_address_of_too() {
    let text = r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> usize { 7 };
static mkp = fn() -> Pair { Pair(struct { a = 1, b = 2 }) };
static main = fn() -> usize {
    let p = mk().&raw;
    let q = mkp().a.&raw mut;
    unsafe { p.* + q.* }
};
"#;
    check_diagnostics(text, expect![""]);
    check_temps(
        text,
        expect![[r#"
            180..184 'mk()'
            203..208 'mkp()'
        "#]],
    );
}

/// A borrowed name is never materialized, whatever it resolves to.
/// TODO: a builtin is a function value and should borrow (halvko/must#31).
#[test]
fn a_name_is_never_materialized_whatever_it_resolves_to() {
    check_diagnostics(
        r#"
static counter: usize = 1;
const limit: usize = 2;
static by_param = fn::<const N: usize>() -> usize { N.& .* };
static by_static = fn() -> () { let p = counter.&mut; };
static by_const = fn() -> () { let p = limit.&mut; };
static by_builtin = fn() -> () { let p = print.&; };
static raw_param = fn::<const N: usize>() -> () { let p = N.&raw; };
static raw_builtin = fn() -> () { let p = print.&raw mut; };
"#,
        expect![[r#"
            104..107: `.&` cannot borrow `N`: it is a const parameter, not a value with storage
            154..161: cannot borrow a `static` as `.&mut`: `static mut` is not supported yet (`counter` is defined here at 8..15)
            210..215: cannot borrow a `const` as `.&mut`: a `const` is copied at every mention, so there is no one place to borrow (`limit` is defined here at 34..39)
            266..273: `.&` cannot borrow `print`: it is a builtin, not a value with storage
            336..342: `.&raw` cannot take the address of `N`: it is a const parameter, not a value with storage
            389..403: `.&raw` cannot take the address of `print`: it is a builtin, not a value with storage
        "#]],
    );
}

/// The temporary's binding is typed as the whole root value (`Pair`, not
/// the borrowed field), displays as `<temporary>`, and has no syntax node,
/// so it never renders.
#[test]
fn a_temporarys_binding_is_typed_and_never_rendered() {
    let text = r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> Pair { Pair(struct { a = 3, b = 4 }) };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static f = fn() -> usize { get(mk().a.&) };
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let f = *crate::file_item_ids(&db, file).last().unwrap();
    let body = crate::body::body(&db, f);
    let infer = crate::infer::infer(&db, f);
    let temps: Vec<_> = body.temps.iter().collect();
    let &[(_, &binding)] = temps.as_slice() else {
        panic!("one temporary, got {temps:?}");
    };
    assert_eq!(infer.type_of_binding[binding].display(), "Pair");
    assert_eq!(body.bindings[binding].name(), "<temporary>");
    check_infer(
        text,
        expect![[r#"
            56..102 'fn() -> Pair { Pa...': fn() -> Pair
            69..102 '{ Pair(struct { a...': Pair
            71..75 'Pair': fn(struct { a: usize, b: usize }) -> Pair
            71..100 'Pair(struct { a =...': Pair
            76..99 'struct { a = 3, b...': struct { a: usize, b: usize }
            89..90 '3': usize
            96..97 '4': usize
            117..160 'fn::<@a>(r: usize...': fn(usize.&::<@a>) -> usize
            126..127 'r': usize.&::<@a>
            153..160 '{ r.* }': usize
            155..156 'r': usize.&::<@a>
            155..158 'r.*': usize
            173..204 'fn() -> usize { g...': fn() -> usize
            187..204 '{ get(mk().a.&) }': usize
            189..192 'get': fn(usize.&) -> usize
            189..202 'get(mk().a.&)': usize
            193..195 'mk': fn() -> Pair
            193..197 'mk()': Pair
            193..199 'mk().a': usize
            193..201 'mk().a.&': usize.&
        "#]],
    );
}
