//! Arrays: literals, indexing, lengths and repeats.

use super::*;

// ---- fixed-size arrays ----

#[test]
fn array_literal_and_index_infer() {
    check_infer(
        "static f = fn { let a = [1, 2, 3]; let x = a[0]; };",
        expect![[r#"
            11..50 'fn { let a = [1, ...': fn()
            14..50 '{ let a = [1, 2, ...': ()
            20..21 'a': [{number}; 3]
            24..33 '[1, 2, 3]': [{number}; 3]
            25..26 '1': {number}
            28..29 '2': {number}
            31..32 '3': {number}
            39..40 'x': {number}
            43..44 'a': [{number}; 3]
            43..47 'a[0]': {number}
            45..46 '0': usize
        "#]],
    );
}

#[test]
fn arrays_in_records_and_records_in_arrays_infer() {
    // `a` carries an annotation: like an `if`-joined record binding, a
    // record-element type decided by a multi-witness join is not available
    // to `.x` *during* traversal (joins solve after it) — the annotation is
    // the same way out `field_access_on_unannotated_param` names.
    check_infer(
        r#"
static f = fn {
    let r = struct { data = [1, 2], len = 2 };
    let a: [struct { x: usize }; 2] = [struct { x = 1 }, struct { x = 2 }];
    let n = r.data[0] + a[1].x;
};
"#,
        expect![[r#"
            12..173 'fn {     let r = ...': fn()
            15..173 '{     let r = str...': ()
            25..26 'r': struct { data: [usize; 2], len: {number} }
            29..62 'struct { data = [...': struct { data: [usize; 2], len: {number} }
            45..51 '[1, 2]': [usize; 2]
            46..47 '1': usize
            49..50 '2': usize
            59..60 '2': {number}
            72..73 'a': [struct { x: usize }; 2]
            102..138 '[struct { x = 1 }...': [struct { x: usize }; 2]
            103..119 'struct { x = 1 }': struct { x: usize }
            116..117 '1': usize
            121..137 'struct { x = 2 }': struct { x: usize }
            134..135 '2': usize
            148..149 'n': usize
            152..153 'r': struct { data: [usize; 2], len: {number} }
            152..158 'r.data': [usize; 2]
            152..161 'r.data[0]': usize
            152..170 'r.data[0] + a[1].x': usize
            159..160 '0': usize
            164..165 'a': [struct { x: usize }; 2]
            164..168 'a[1]': struct { x: usize }
            164..170 'a[1].x': usize
            166..167 '1': usize
        "#]],
    );
}

#[test]
fn array_length_mismatch_blames_the_literal() {
    check_diagnostics(
        "static f = fn { let a: [usize; 3] = [1, 2]; };",
        expect![[r#"
            36..42: type mismatch: expected `[usize; 3]`, found `[usize; 2]` (expected `[usize; 3]` because of this annotation at 23..33)
        "#]],
    );
}

#[test]
fn array_lengths_never_unify_across_a_call() {
    check_diagnostics(
        r#"
static g = fn (a: [usize; 2]) {};
static f = fn { g([1, 2, 3]); };
"#,
        expect![[r#"
            53..62: type mismatch: expected `[usize; 2]`, found `[usize; 3]`
        "#]],
    );
}

#[test]
fn array_element_mismatch_blames_the_element() {
    check_diagnostics(
        r#"static f = fn { let a: [usize; 2] = [1, "two"]; };"#,
        expect![[r#"
            40..45: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..33)
        "#]],
    );
}

#[test]
fn array_elements_vote_without_an_annotation() {
    check_diagnostics(
        r#"static f = fn { let a = [1, 2, "three"]; };"#,
        expect![[r#"
            25..26: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 31..38)
            28..29: type mismatch: expected `str`, found `{number}` (this branch has type `str` at 31..38)
        "#]],
    );
}

#[test]
fn empty_array_needs_annotation() {
    check_diagnostics(
        r#"static f = fn { let a = []; };"#,
        expect![[r#"
            24..26: cannot infer the element type of an empty array; add a type annotation
        "#]],
    );
}

#[test]
fn empty_array_with_annotation_is_fine() {
    check_diagnostics(
        "static f = fn { let a: [usize; 0] = []; };",
        expect![[r#""#]],
    );
}

#[test]
fn index_must_be_usize() {
    check_diagnostics(
        r#"static f = fn { let a = [1, 2]; let x = a["nope"]; };"#,
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            42..48: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn compile_time_out_of_bounds_is_reported() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; let x = a[2]; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            40..44: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn index_on_non_array_is_reported() {
    check_diagnostics(
        "static f = fn { let x = 5; let y = x[0]; };",
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            35..36: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn index_assign_requires_mut_root() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; a[0] = 5; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            32..33: cannot assign to `a[_]`: `a` is not declared `mut` (`a` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn index_assign_to_mut_binding_is_fine() {
    check_diagnostics(
        "static f = fn { let mut a = [1, 2]; a[0] = 5; print(\"\"); };",
        expect![[r#"
            29..30: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn const_param_array_length_end_to_end() {
    check_diagnostics(
        r#"
static sum2 = const fn::<const N: usize>(b: [usize; N]) -> usize { b[0] + b[1] };
static r = sum2::<2>([1, 2]);
"#,
        expect![""],
    );
}

#[test]
fn const_param_array_length_mismatch_at_instantiation() {
    check_diagnostics(
        r#"
static sum2 = const fn::<const N: usize>(b: [usize; N]) -> usize { b[0] + b[1] };
static r = sum2::<3>([1, 2]);
"#,
        expect![[r#"
            104..110: type mismatch: expected `[usize; 3]`, found `[usize; 2]`
        "#]],
    );
}

#[test]
fn generic_type_with_array_field_end_to_end() {
    check_diagnostics(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
static b = Buf::<2>(struct { data = [1, 2], len = 2 });
static first = fn (buf: Buf::<2>) -> usize { buf.data[0] };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn const_block_array_length_rejected_in_type_position() {
    check_diagnostics(
        "static x: [usize; const { 3 }] = [1, 2, 3];",
        expect![[r#"
            18..29: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn non_const_array_length_name_rejected_in_type_position() {
    check_diagnostics(
        "static x: [usize; huh] = [1];",
        expect![[r#"
            18..21: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn str_array_length_rejected_in_type_position() {
    check_diagnostics(
        r#"static x: [usize; "two"] = [1, 2];"#,
        expect![[r#"
            18..23: type mismatch: expected `usize`, found `str`
            27..33: type mismatch: expected `[usize; "two"]`, found `[usize; 2]` (expected `[usize; "two"]` because of this annotation at 10..24)
        "#]],
    );
}

#[test]
fn array_repeat_infers_and_repeat_count_forms() {
    check_infer(
        "static f = fn { let a = [0; 4]; };",
        expect![[r#"
            11..33 'fn { let a = [0; ...': fn()
            14..33 '{ let a = [0; 4]; }': ()
            20..21 'a': [{number}; 4]
            24..30 '[0; 4]': [{number}; 4]
            25..26 '0': {number}
            28..29 '4': usize
        "#]],
    );
}

#[test]
fn array_repeat_with_variable_count_rejected() {
    check_diagnostics(
        "static f = fn (n: usize) { let a = [0; n]; };",
        expect![[r#"
            36..37: cannot infer the type of this number: it has no defining use — add a type annotation
            39..40: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn array_repeat_with_const_block_count_rejected() {
    check_diagnostics(
        "static f = fn { let a = [0; const { 2 }]; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            28..39: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn array_repeat_with_const_param_count() {
    check_diagnostics(
        r#"
static rep = const fn::<const N: usize>(v: usize) -> [usize; N] { [v; N] };
static r = rep::<3>(7);
"#,
        expect![""],
    );
}

#[test]
fn array_valued_const_param_rejected_at_declaration() {
    check_diagnostics(
        "static f = fn::<const N: [usize; 2]>() -> usize { 0 };",
        expect![[r#"
            25..35: an array value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn addr_of_whole_array_and_element_work() {
    check_diagnostics(
        r#"
static f = fn {
    let mut a = [1, 2];
    let p = unsafe { (a.&raw mut).* };
    let q = a[0].&raw mut;
};
"#,
        expect![[r#"
            34..35: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn addr_of_array_element_types_as_element_pointer() {
    check_infer(
        r#"
static f = fn {
    let mut a = [1, 2];
    let q = a[0].&raw mut;
};
"#,
        expect![[r#"
            12..69 'fn {     let mut ...': fn()
            15..69 '{     let mut a =...': ()
            29..30 'a': [{number}; 2]
            33..39 '[1, 2]': [{number}; 2]
            34..35 '1': {number}
            37..38 '2': {number}
            49..50 'q': {number}.&raw mut
            53..54 'a': [{number}; 2]
            53..57 'a[0]': {number}
            53..66 'a[0].&raw mut': {number}.&raw mut
            55..56 '0': usize
        "#]],
    );
}

#[test]
fn addr_of_mut_of_an_element_still_requires_a_mut_root() {
    check_diagnostics(
        "static f = fn { let a = [1, 2]; let q = a[0].&raw mut; };",
        expect![[r#"
            25..26: cannot infer the type of this number: it has no defining use — add a type annotation
            40..41: cannot take `.&raw mut` of `a[_]`: `a` is not declared `mut` (`a` is declared without `mut` here at 20..21)
        "#]],
    );
}

#[test]
fn match_on_array_scrutinee_rejects_variant_patterns() {
    check_diagnostics(
        r#"
type Shape = enum { Point };
static f = fn (a: [usize; 2]) -> usize {
    match a {
        ::Point => 1,
        _ => 2,
    }
};
"#,
        expect![[r#"
            93..100: a variant pattern needs an enum scrutinee; only `_` or a binding can match a `[usize; 2]`
        "#]],
    );
}

#[test]
fn type_declaration_with_array_field() {
    check_diagnostics(
        r#"
type Buf = struct { data: [usize; 2] };
static b = Buf(struct { data = [1, 2] });
"#,
        expect![[r#""#]],
    );
}

#[test]
fn type_declaration_array_field_bad_lengths() {
    check_diagnostics(
        r#"
type A = struct { data: [usize; const { 2 }] };
type B = struct { data: [usize; nope] };
"#,
        expect![[r#"
            33..44: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
            81..85: a type's const argument must be a literal or a const parameter name
        "#]],
    );
}

#[test]
fn mixed_variant_array_elements_have_the_enum_type() {
    check_infer(
        r#"
type Shape = enum { Point, Circle(usize) };
static f = fn {
    let shapes = [Shape::Circle(1), Shape::Point];
};
"#,
        expect![[r#"
            56..113 'fn {     let shap...': fn()
            59..113 '{     let shapes ...': ()
            69..75 'shapes': [Shape; 2]
            78..110 '[Shape::Circle(1)...': [Shape; 2]
            79..92 'Shape::Circle': fn(usize) -> Shape
            79..95 'Shape::Circle(1)': Shape
            93..94 '1': usize
            97..109 'Shape::Point': Shape
        "#]],
    );
}
