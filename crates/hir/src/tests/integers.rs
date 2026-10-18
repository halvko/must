//! Integer types: literal pinning, ranges and mixed widths.

use super::*;

// --- Integer types: the i/u × 8/16/32/64 menu plus usize/isize ---

#[test]
fn every_integer_type_annotates_and_infers() {
    check_infer(
        r#"
static f = fn (
    a: i8, b: i16, c: i32, d: i64,
    e: u8, g: u16, h: u32, i: u64,
    j: usize, k: isize,
) {};
"#,
        expect![[r#"
            12..115 'fn (     a: i8, b...': fn(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize)
            21..22 'a': i8
            28..29 'b': i16
            36..37 'c': i32
            44..45 'd': i64
            56..57 'e': u8
            63..64 'g': u16
            71..72 'h': u32
            79..80 'i': u64
            91..92 'j': usize
            101..102 'k': isize
            113..115 '{}': ()
        "#]],
    );
}

#[test]
fn literal_pins_through_an_annotation() {
    check_infer(
        "static f = fn { let x: u8 = 7; };",
        expect![[r#"
            11..32 'fn { let x: u8 = ...': fn()
            14..32 '{ let x: u8 = 7; }': ()
            20..21 'x': u8
            28..29 '7': u8
        "#]],
    );
}

#[test]
fn literal_pins_through_a_parameter_type() {
    check_infer(
        r#"
static g = fn (n: u16) {};
static f = fn { g(3); };
"#,
        expect![[r#"
            12..26 'fn (n: u16) {}': fn(u16)
            16..17 'n': u16
            24..26 '{}': ()
            39..51 'fn { g(3); }': fn()
            42..51 '{ g(3); }': ()
            44..45 'g': fn(u16)
            44..48 'g(3)': ()
            46..47 '3': u16
        "#]],
    );
}

#[test]
fn index_position_pins_usize() {
    check_infer(
        r#"static f = fn (a: [u8; 4]) -> u8 { a[1] };"#,
        expect![[r#"
            11..41 'fn (a: [u8; 4]) -...': fn([u8; 4]) -> u8
            15..16 'a': [u8; 4]
            33..41 '{ a[1] }': u8
            35..36 'a': [u8; 4]
            35..39 'a[1]': u8
            37..38 '1': usize
        "#]],
    );
}

#[test]
fn repeat_count_pins_usize_and_the_element_pins_through_the_annotation() {
    check_infer(
        r#"static f = fn { let a: [u8; 3] = [0; 3]; };"#,
        expect![[r#"
            11..42 'fn { let a: [u8; ...': fn()
            14..42 '{ let a: [u8; 3] ...': ()
            20..21 'a': [u8; 3]
            33..39 '[0; 3]': [u8; 3]
            34..35 '0': u8
            37..38 '3': usize
        "#]],
    );
}

#[test]
fn alloc_count_pins_usize() {
    check_infer(
        r#"static f = fn { let r = alloc_array::<u8>(4); };"#,
        expect![[r#"
            11..47 'fn { let r = allo...': fn()
            14..47 '{ let r = alloc_a...': ()
            20..21 'r': AllocResult::<u8>
            24..41 'alloc_array::<u8>': fn(usize) -> AllocResult::<u8>
            24..44 'alloc_array::<u8>(4)': AllocResult::<u8>
            42..43 '4': usize
        "#]],
    );
}

#[test]
fn typed_operand_pins_the_other_side() {
    check_infer(
        r#"static f = fn (n: u32) -> u32 { n + 1 };"#,
        expect![[r#"
            11..39 'fn (n: u32) -> u3...': fn(u32) -> u32
            15..16 'n': u32
            30..39 '{ n + 1 }': u32
            32..33 'n': u32
            32..37 'n + 1': u32
            36..37 '1': u32
        "#]],
    );
}

#[test]
fn number_variables_merge_then_a_late_use_pins_the_whole_chain() {
    // `1`, `2` and the intermediates share one NUMBER variable; the final
    // `i64` annotation is the one defining use — it pins them all.
    check_infer(
        r#"
static f = fn {
    let a = 1;
    let b = a + 2;
    let c: i64 = b;
};
"#,
        expect![[r#"
            12..72 'fn {     let a = ...': fn()
            15..72 '{     let a = 1; ...': ()
            25..26 'a': i64
            29..30 '1': i64
            40..41 'b': i64
            44..45 'a': i64
            44..49 'a + 2': i64
            48..49 '2': i64
            59..60 'c': i64
            68..69 'b': i64
        "#]],
    );
}

#[test]
fn unpinned_number_renders_as_number_and_reports_no_defining_use() {
    check_infer(
        "static f = fn { let n = 3; };",
        expect![[r#"
            11..28 'fn { let n = 3; }': fn()
            14..28 '{ let n = 3; }': ()
            20..21 'n': {number}
            24..25 '3': {number}
        "#]],
    );
    check_diagnostics(
        "static f = fn { let n = 3; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn unannotated_static_number_reports_no_defining_use_too() {
    // The same never-default rule at item level: the definition carries
    // the number diagnostic; uses see an undetermined signature.
    check_diagnostics(
        "static x = 3;",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
    check_diagnostics(
        "static x = 3;\nstatic f = fn { let y = x; };",
        expect![[r#"
            11..12: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: not enough information to infer the type of `x`; add a type annotation to its definition (defined here at 7..8)
        "#]],
    );
}

#[test]
fn literal_out_of_range_for_its_resolved_type() {
    check_diagnostics(
        "static a: u8 = 300;",
        expect![[r#"
            15..18: `300` does not fit in `u8`
        "#]],
    );
    check_diagnostics("static b: u8 = 255;", expect![[r#""#]]);
}

#[test]
fn negative_literal_in_an_unsigned_type_is_a_range_error() {
    check_diagnostics(
        "static a: u32 = -1;",
        expect![[r#"
            17..18: `-1` does not fit in `u32`
        "#]],
    );
    // `-0` is the one negation every unsigned type holds.
    check_diagnostics("static b: u32 = -0;", expect![[r#""#]]);
}

#[test]
fn i64_min_fits_and_one_below_does_not() {
    check_diagnostics("static min: i64 = -9223372036854775808;", expect![[r#""#]]);
    check_diagnostics(
        "static below: i64 = -9223372036854775809;",
        expect![[r#"
            21..40: `-9223372036854775809` does not fit in `i64`
        "#]],
    );
    check_diagnostics(
        "static above: i64 = 9223372036854775808;",
        expect![[r#"
            20..39: `9223372036854775808` does not fit in `i64`
        "#]],
    );
}

#[test]
fn i8_range_edges_respect_the_sign() {
    check_diagnostics("static min: i8 = -128;", expect![[r#""#]]);
    check_diagnostics(
        "static plus: i8 = 128;",
        expect![[r#"
            18..21: `128` does not fit in `i8`
        "#]],
    );
}

#[test]
fn mixed_integer_types_are_an_ordinary_mismatch() {
    // No implicit conversions: `u8 + u32` doesn't unify, and no cast
    // syntax exists to bridge them.
    check_diagnostics(
        "static f = fn (a: u8, b: u32) -> u8 { a + b };",
        expect![[r#"
            42..43: type mismatch: expected `u8`, found `u32` (`+` requires `u8` operands at 40..41)
        "#]],
    );
}

#[test]
fn unary_minus_types_as_its_operand() {
    check_infer(
        "static f = fn (n: i32) -> i32 { -n };",
        expect![[r#"
            11..36 'fn (n: i32) -> i3...': fn(i32) -> i32
            15..16 'n': i32
            30..36 '{ -n }': i32
            32..34 '-n': i32
            33..34 'n': i32
        "#]],
    );
    // Legal syntax on unsigned operands (traps at runtime unless zero).
    check_diagnostics("static f = fn (n: u8) -> u8 { -n };", expect![[r#""#]]);
}

#[test]
fn offset_builtin_takes_an_isize_and_preserves_the_flavor() {
    check_infer(
        r#"
static f = fn (p: u8.&raw mut, i: isize) -> u8.&raw mut {
    unsafe { offset(p, i) }
};
"#,
        expect![[r#"
            12..88 'fn (p: u8.&raw mu...': fn(u8.&raw mut, isize) -> u8.&raw mut
            16..17 'p': u8.&raw mut
            32..33 'i': isize
            57..88 '{     unsafe { of...': u8.&raw mut
            63..86 'unsafe { offset(p...': u8.&raw mut
            70..86 '{ offset(p, i) }': u8.&raw mut
            72..84 'offset(p, i)': u8.&raw mut
            79..80 'p': u8.&raw mut
            82..83 'i': isize
        "#]],
    );
    // The literal pins to `isize` through the parameter position.
    check_infer(
        r#"
static f = fn (p: u8.&raw) -> u8.&raw {
    unsafe { offset(p, 1) }
};
"#,
        expect![[r#"
            12..70 'fn (p: u8.&raw) -...': fn(u8.&raw) -> u8.&raw
            16..17 'p': u8.&raw
            39..70 '{     unsafe { of...': u8.&raw
            45..68 'unsafe { offset(p...': u8.&raw
            52..68 '{ offset(p, 1) }': u8.&raw
            54..66 'offset(p, 1)': u8.&raw
            61..62 'p': u8.&raw
            64..65 '1': isize
        "#]],
    );
}

#[test]
fn offset_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: u8.&raw mut, i: isize) {
    let q = offset(p, i);
};
"#,
        expect![[r#"
            56..68: calling `offset` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn offset_index_must_be_isize() {
    check_diagnostics(
        r#"
static f = fn (p: u8.&raw mut, n: usize) {
    let q = unsafe { offset(p, n) };
};
"#,
        expect![[r#"
            75..76: type mismatch: expected `isize`, found `usize`
        "#]],
    );
}

#[test]
fn generic_call_with_only_a_bare_literal_reports_the_number_not_the_param() {
    // The only information about `T` is an unpinned literal: an unresolved
    // number is not a type, and the actionable diagnostic is the literal's
    // own annotate-me error — not a cannot-infer-`T` on top.
    check_diagnostics(
        r#"
static id = fn::<T>(x: T) -> T { x };
static f = fn { let y = id(3); };
"#,
        expect![[r#"
            66..67: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn turbofish_pins_a_literal_argument_through_the_type_param() {
    check_infer(
        r#"
static id = fn::<T>(x: T) -> T { x };
static f = fn { let y = id::<u8>(3); };
"#,
        expect![[r#"
            13..37 'fn::<T>(x: T) -> ...': fn(T) -> T
            21..22 'x': T
            32..37 '{ x }': T
            34..35 'x': T
            50..77 'fn { let y = id::...': fn()
            53..77 '{ let y = id::<u8...': ()
            59..60 'y': u8
            63..71 'id::<u8>': fn(u8) -> u8
            63..74 'id::<u8>(3)': u8
            72..73 '3': u8
        "#]],
    );
}

#[test]
fn const_param_declared_with_a_sized_type_checks_literal_range() {
    // A const param may use any integer type; the written argument is
    // range-checked against it like any pinned literal.
    check_diagnostics(
        r#"
type Buf = struct::<const N: u8> { len: usize };
static b: Buf::<300> = Buf::<300>(struct { len = 1 });
"#,
        expect![[r#"
            66..69: `300` does not fit in `u8`
            79..82: `300` does not fit in `u8`
        "#]],
    );
}

#[test]
fn match_on_an_integer_scrutinee_requires_a_catch_all_uniformly() {
    // Integers are open-domain for every width — same rule `usize` always
    // had, extended uniformly.
    check_diagnostics(
        r#"
static f = fn (n: u8) -> str {
    match n {
        _ => "something",
    }
};
"#,
        expect![[r#""#]],
    );
    check_diagnostics(
        r#"
static f = fn (n: i16) -> str {
    match n {
    }
};
"#,
        expect![[r#"
            37..42: this `match` does not cover every possible `i16`; add a `_` arm
        "#]],
    );
}
