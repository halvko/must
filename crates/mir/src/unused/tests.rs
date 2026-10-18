//! Unused bindings and values never read, with the text each squiggles.

use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

/// Every warning of this pass, one per line: the squiggled text, then the
/// message.
fn check(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for item in hir::all_checkable_items(&db, file) {
        let (body, source_map) = hir::body_with_source_map(&db, item);
        for diag in crate::unused_check(&db, item) {
            let range = diag.range(source_map).expect("every warning has a range");
            let squiggled = &text[range];
            rendered.push_str(&format!("`{squiggled}`: {}\n", diag.message(body)));
        }
    }
    expect.assert_eq(&rendered);
}

#[test]
fn a_let_nothing_reads_is_unused() {
    check(
        "static f = fn () -> usize { let x: usize = 1; 2 };",
        expect![[r#"
            `x`: unused binding `x`
        "#]],
    );
}

#[test]
fn a_leading_underscore_says_unused_on_purpose() {
    check(
        "static f = fn (_n: usize) -> usize { let _x: usize = 1; 2 };",
        expect![""],
    );
}

#[test]
fn a_binding_only_ever_assigned_is_unused() {
    check(
        "static f = fn () -> () { let mut x: usize = 1; x = 2; };",
        expect![[r#"
            `x`: unused binding `x`
        "#]],
    );
}

#[test]
fn a_parameter_nothing_reads_is_unused() {
    check(
        "static f = fn (a: usize, b: usize) -> usize { a };",
        expect![[r#"
            `b`: unused binding `b`
        "#]],
    );
}

#[test]
fn a_payload_binding_nothing_reads_is_unused() {
    check(
        r#"
type Shape = enum { Circle(usize), Square(usize) };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => 1,
        ::Square(side) => side,
    }
};
"#,
        expect![[r#"
            `r`: unused binding `r`
        "#]],
    );
}

#[test]
fn an_initializer_overwritten_before_a_read_is_never_read() {
    check(
        "static f = fn () -> usize { let mut x = 1; x = 2; x };",
        expect![[r#"
            `1`: value written to `x` is never read
        "#]],
    );
}

#[test]
fn a_value_overwritten_on_every_branch_is_never_read() {
    check(
        r#"
static f = fn (c: bool) -> usize {
    let mut x = 10;
    if c { x = 1; } else { x = 2; }
    x
};
"#,
        expect![[r#"
            `10`: value written to `x` is never read
        "#]],
    );
}

#[test]
fn a_value_one_branch_reads_is_read() {
    check(
        r#"
static f = fn (c: bool) -> usize {
    let mut x = 10;
    if c { x = 1; }
    x
};
"#,
        expect![""],
    );
}

#[test]
fn the_last_assignment_in_a_loop_is_read_by_the_next_iteration() {
    check(
        r#"
static f = fn (n: usize) -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == n { break acc; }
        acc = acc + i;
        i = i + 1;
    }
};
"#,
        expect![""],
    );
}

#[test]
fn a_store_after_the_last_read_is_never_read() {
    check(
        "static f = fn () -> usize { let mut x = 1; let y = x; x = 5; y };",
        expect![[r#"
            `5`: value written to `x` is never read
        "#]],
    );
}

#[test]
fn a_parameter_overwritten_before_a_read_never_reads_its_argument() {
    check(
        "static f = fn (mut n: usize) -> usize { n = 1; n };",
        expect![[r#"
            `n`: the argument passed as `n` is never read
        "#]],
    );
}

/// A raw pointer can read the local where liveness says it is dead.
#[test]
fn an_addressable_local_is_not_judged() {
    check(
        r#"
static f = fn () -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    x = 2;
    unsafe { p.* }
};
"#,
        expect![""],
    );
}

/// Losing a linear value is the leak error's to report, not a warning's.
#[test]
fn a_linear_local_is_left_to_the_leak_check() {
    check(
        r#"
type Res = struct { id: usize } only move;
static f = fn () -> () { let r = Res(struct { id = 1 }); };
"#,
        expect![""],
    );
}

/// A broken expression's reads lower into a trap, so the binding it
/// reads would look unused beside the real error.
#[test]
fn an_item_with_an_error_is_not_judged() {
    check(
        r#"static f = fn () -> usize { let x = 1; x + "two" };"#,
        expect![""],
    );
}

/// A capture's read traps in the nested body, so the whole item, `b`
/// included, gets no warning.
#[test]
fn an_item_with_a_capture_is_not_judged() {
    check(
        r#"
static f = fn () -> usize {
    let a: usize = 1;
    let b: usize = 2;
    let g = fn () -> usize { a };
    g()
};
"#,
        expect![""],
    );
}

/// A read after a `return` keeps `x` used, and the write `3` after it is
/// not reported; the `1` is never read.
#[test]
fn unreachable_code_is_not_judged() {
    check(
        "static f = fn () -> usize { let mut x: usize = 1; return 2; x = 3; x };",
        expect![[r#"
            `1`: value written to `x` is never read
        "#]],
    );
}

#[test]
fn an_element_write_nothing_reads_afterwards_is_never_read() {
    check(
        r#"
static f = fn (i: usize) -> usize {
    let mut a = [1, 2, 3];
    let first = a[0];
    a[i] = 7;
    first
};
"#,
        expect![[r#"
            `7`: value written to `a` is never read
        "#]],
    );
}

/// A binding only ever written into is unused, not read by its writes.
#[test]
fn a_binding_only_written_field_by_field_is_unused() {
    check(
        r#"
static f = fn () -> () {
    let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    p.x = 10;
};
"#,
        expect![[r#"
            `p`: unused binding `p`
        "#]],
    );
}

/// Liveness is whole-local: reading another field keeps the write.
#[test]
fn a_field_write_before_a_read_of_another_field_is_kept() {
    check(
        r#"
static f = fn () -> usize {
    let mut p = struct { x = 1, y = 2 };
    p.x = 10;
    p.y
};
"#,
        expect![""],
    );
}

/// `p.x = p.x + 1` reads `p` too; the write after it is the dead one.
#[test]
fn a_field_write_that_reads_its_own_root_counts_as_a_read() {
    check(
        r#"
static f = fn () -> usize {
    let mut p = struct { x = 1, y = 2 };
    p.x = p.x + 1;
    p.x
};
"#,
        expect![""],
    );
}

/// A write through a `&mut` is not judged.
#[test]
fn a_write_through_a_borrow_is_not_judged() {
    check(
        r#"
static f = fn () -> () {
    let mut x: usize = 1;
    let r = x.&mut;
    r.* = 2;
};
"#,
        expect![""],
    );
}

#[test]
fn every_field_write_nothing_reads_afterwards_is_never_read() {
    check(
        r#"
static f = fn () -> usize {
    let mut p = struct { x = 1, y = 2 };
    let sum = p.x + p.y;
    p.x = 10;
    p.y = 20;
    sum
};
"#,
        expect![[r#"
            `10`: value written to `p` is never read
            `20`: value written to `p` is never read
        "#]],
    );
}

#[test]
fn a_destructured_value_overwritten_before_a_read_squiggles_the_name() {
    check(
        r#"
static f = fn () -> usize {
    let [mut a, mut b] = [1, 2];
    a = 3;
    b = 4;
    a + b
};
"#,
        expect![[r#"
            `a`: value written to `a` is never read
            `b`: value written to `b` is never read
        "#]],
    );
}

#[test]
fn a_destructured_parameter_overwritten_before_a_read_squiggles_the_name() {
    check(
        r#"
static f = fn (struct { a, mut b }: struct { a: usize, b: usize }) -> usize {
    b = 1;
    a + b
};
"#,
        expect![[r#"
            `b`: value written to `b` is never read
        "#]],
    );
}

#[test]
fn a_binding_that_shadows_a_variant_is_left_to_that_warning() {
    check(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        Point => 0,
    }
};
"#,
        expect![""],
    );
}
