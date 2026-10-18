//! Arrays: building, indexing and bounds traps.

use super::*;

// ---- fixed-size arrays ----

#[test]
fn arrays_build_read_and_write() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [1, 2, 3];
    a[0] = 10;
    let m: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    a[0] + a[2] + m[1][0]
};
"#,
        "main()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn arrays_const_evaluate_and_freeze_into_statics() {
    check_const(
        r#"
static table = const {
    let mut t: [usize; 4] = [0; 4];
    t[0] = 1;
    t[1] = 2;
    t[3] = t[0] + t[1];
    t
};
static row: struct { name: str, cells: [usize; 3] } = struct { name = "row", cells = [1, 2, 3] };
static grid: [struct { x: usize }; 2] = [struct { x = 1 }, struct { x = 2 }];
"#,
        expect![[r#"
            table = [1, 2, 0, 3]
            row = { cells = [1, 2, 3], name = "row" }
            grid = [{ x = 1 }, { x = 2 }]
        "#]],
    );
}

#[test]
fn compile_time_out_of_bounds_traps_with_the_squiggle_text() {
    // The trap MIR planted for the compile-time-known OOB — the exact text
    // of the editor squiggle (single render), reached at runtime.
    check_run(
        r#"
static main = fn () -> usize {
    let a = [1, 2];
    a[2]
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn runtime_out_of_bounds_read_traps_deterministically() {
    check_run(
        r#"
static get = fn (a: [usize; 2], i: usize) -> usize { a[i] };
static main = fn () -> usize { get([1, 2], 5) };
"#,
        "main()",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 5
        "#]],
    );
}

#[test]
fn runtime_out_of_bounds_write_traps() {
    check_run(
        r#"
static set = fn (i: usize) -> usize {
    let mut a = [1, 2];
    a[i] = 9;
    a[0]
};
"#,
        "set(2)",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn array_repeat_with_const_param_length() {
    check_run(
        r#"
static rep = const fn::<const N: usize>(v: usize) -> [usize; N] { [v; N] };
static main = fn () -> usize {
    let a = rep::<3>(7);
    a[0] + a[1] + a[2]
};
"#,
        "main()",
        expect![[r#"
            => 21
        "#]],
    );
}

#[test]
fn generic_buffer_type_with_const_length_runs() {
    check_run(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
static first = fn (b: Buf::<2>) -> usize { b.data[0] + b.len };
static main = fn () -> usize { first(Buf::<2>(struct { data = [40, 1], len = 2 })) };
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn mixed_variant_array_elements_dispatch_through_match() {
    check_run(
        r#"
type Shape = enum { Point, Circle(usize) };
static area = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r * r,
        ::Point => 0,
    }
};
static main = fn () -> usize {
    let shapes = [Shape::Circle(3), Shape::Point];
    area(shapes[0]) + area(shapes[1])
};
"#,
        "main()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn arrays_of_records_mutate_in_place() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut pts: [struct { x: usize, y: usize }; 2] = [struct { x = 1, y = 2 }, struct { x = 3, y = 4 }];
    pts[1].x = 30;
    pts[1].x + pts[0].y
};
"#,
        "main()",
        expect![[r#"
            => 32
        "#]],
    );
}

#[test]
fn huge_const_array_repeat_runs_out_of_fuel() {
    check_const(
        "static big: [usize; 4_000_000_000] = [0; 4_000_000_000];",
        expect![[r#"
            big = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

#[test]
fn empty_array_and_equality() {
    check_run(
        r#"
static main = fn () -> bool {
    let a: [usize; 0] = [];
    let b: [usize; 0] = [];
    let c: [usize; 2] = [1, 2];
    let d: [usize; 2] = [1, 2];
    a == b == (c == d)
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}
