//! The heap builtins: alloc, dealloc, copy and add.

use super::*;

// ---- the heap builtins --------------------------------------------------

#[test]
fn alloc_array_types_as_the_result_enum() {
    check_infer(
        r#"
static f = fn {
    let r = alloc_array::<usize>(2);
};
"#,
        expect![[r#"
            12..55 'fn {     let r = ...': fn()
            15..55 '{     let r = all...': ()
            25..26 'r': AllocResult::<usize>
            29..49 'alloc_array::<usize>': fn(usize) -> AllocResult::<usize>
            29..52 'alloc_array::<usi...': AllocResult::<usize>
            50..51 '2': usize
        "#]],
    );
}

#[test]
fn alloc_result_ok_payload_binds_the_pointer_type() {
    check_infer(
        r#"
static f = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { p.* },
        AllocResult::Err => 0,
    }
};
"#,
        expect![[r#"
            12..149 'fn () -> usize { ...': fn() -> usize
            27..149 '{     match alloc...': usize
            33..147 'match alloc_array...': usize
            39..59 'alloc_array::<usize>': fn(usize) -> AllocResult::<usize>
            39..62 'alloc_array::<usi...': AllocResult::<usize>
            60..61 '2': usize
            89..90 'p': usize.&raw mut
            95..109 'unsafe { p.* }': usize
            102..109 '{ p.* }': usize
            104..105 'p': usize.&raw mut
            104..107 'p.*': usize
            139..140 '0': usize
        "#]],
    );
}

#[test]
fn match_on_alloc_result_must_cover_the_err_arm() {
    // The result shape has teeth: the `Err` arm exists in the type even
    // though the interpreter never produces it.
    check_diagnostics(
        r#"
static f = fn () -> () {
    match alloc_array::<usize>(1) {
        AllocResult::Ok(p) => { },
    }
};
"#,
        expect![[r#"
            30..35: this `match` does not cover `AllocResult::Err`
        "#]],
    );
}

#[test]
fn dealloc_array_infers_its_type_argument_from_the_pointer() {
    check_infer(
        r#"
static f = fn (p: str.&raw mut) {
    unsafe { dealloc_array(p, 1) };
};
"#,
        expect![[r#"
            12..72 'fn (p: str.&raw m...': fn(str.&raw mut)
            16..17 'p': str.&raw mut
            33..72 '{     unsafe { de...': ()
            39..69 'unsafe { dealloc_...': ()
            46..69 '{ dealloc_array(p...': ()
            48..61 'dealloc_array': unsafe fn(str.&raw mut, usize)
            48..67 'dealloc_array(p, 1)': ()
            62..63 'p': str.&raw mut
            65..66 '1': usize
        "#]],
    );
}

#[test]
fn dealloc_array_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    dealloc_array(p, 1);
};
"#,
        expect![[r#"
            41..60: calling `dealloc_array` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn copy_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, q: usize.&raw mut) {
    copy(p, q, 1);
};
"#,
        expect![[r#"
            56..69: calling `copy` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn turbofished_dealloc_outside_unsafe_is_rejected_too() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    dealloc_array::<usize>(p, 1);
};
"#,
        expect![[r#"
            41..69: calling `dealloc_array` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn alloc_and_dangling_need_no_unsafe() {
    // Allocating cannot UB and a dangling pointer only hurts when
    // dereferenced — neither needs the marker. `add`, by contrast, now
    // carries a real precondition, so it is wrapped in `unsafe` here.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    let r = alloc_array::<usize>(1);
    let q = unsafe { add(p, 1) };
    let d = dangling::<usize>();
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn add_outside_unsafe_is_rejected() {
    // `add` joins `dealloc_array`/`copy` as an unsafe builtin: advancing a
    // pointer that does not address an array element (with `i > 0`) is
    // detected UB at the call, so the call needs the marker.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw mut) {
    let q = add(p, 1);
};
"#,
        expect![[r#"
            49..58: calling `add` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn add_preserves_the_pointer_flavor() {
    check_infer(
        r#"
static f = fn (s: usize.&raw, m: usize.&raw mut) {
    let a = unsafe { add(s, 1) };
    let b = unsafe { add(m, 1) };
};
"#,
        expect![[r#"
            12..121 'fn (s: usize.&raw...': fn(usize.&raw, usize.&raw mut)
            16..17 's': usize.&raw
            31..32 'm': usize.&raw mut
            50..121 '{     let a = uns...': ()
            60..61 'a': usize.&raw
            64..84 'unsafe { add(s, 1) }': usize.&raw
            71..84 '{ add(s, 1) }': usize.&raw
            73..82 'add(s, 1)': usize.&raw
            77..78 's': usize.&raw
            80..81 '1': usize
            94..95 'b': usize.&raw mut
            98..118 'unsafe { add(m, 1) }': usize.&raw mut
            105..118 '{ add(m, 1) }': usize.&raw mut
            107..116 'add(m, 1)': usize.&raw mut
            111..112 'm': usize.&raw mut
            114..115 '1': usize
        "#]],
    );
}

#[test]
fn copy_requires_a_mutable_destination() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, q: usize.&raw) {
    unsafe { copy(p, q, 1) };
};
"#,
        expect![[r#"
            69..70: type mismatch: expected `usize.&raw mut`, found `usize.&raw`
        "#]],
    );
}

#[test]
fn copy_source_may_be_either_flavor_but_pointees_must_agree() {
    check_diagnostics(
        r#"
static f = fn (p: str.&raw, q: usize.&raw mut) {
    unsafe { copy(p, q, 1) };
};
"#,
        expect![[r#"
            71..72: type mismatch: expected `str.&raw mut`, found `usize.&raw mut`
        "#]],
    );
}

#[test]
fn add_of_a_non_pointer_is_rejected() {
    check_diagnostics(
        r#"
static f = fn {
    let a = unsafe { add(4, 1) };
};
"#,
        expect![[r#"
            42..43: `add` expects a raw pointer (`T.&raw` or `T.&raw mut`) here, found `{number}`
            42..43: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn offset_is_a_free_identifier_after_the_rename() {
    // `offset` is reserved for a future signed (`isize`) variant but binds
    // NOTHING builtin today: a user item named `offset` is an ordinary
    // item. It even works as a first-class value and needs no `unsafe` —
    // neither of which the flavor-polymorphic `add` builtin could do —
    // proving `offset` no longer resolves to a builtin.
    check_diagnostics(
        r#"
static offset = fn (p: usize.&raw mut) { };
static main = fn (p: usize.&raw mut) {
    let g = offset;
    offset(p);
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn add_is_not_a_first_class_value() {
    // Its pointer parameter accepts both flavors, so there is no one fn
    // type for a `let` to bind.
    check_diagnostics(
        r#"
static f = fn {
    let g = add;
};
"#,
        expect![[r#"
            29..32: `add` must be called directly; its pointer parameter accepts both `T.&raw` and `T.&raw mut`, so it has no one function type to be a value at
        "#]],
    );
}

#[test]
fn heap_calls_are_rejected_in_const_contexts() {
    // The eager const fence (C04): both heap builtins refuse at check
    // time; `add`/`copy`/`dangling` are deliberately not fenced.
    check_diagnostics(
        r#"
static a = alloc_array::<usize>(1);
static b = const fn () -> () { unsafe { dealloc_array(dangling::<usize>(), 0) } };
static ok = const {
    let d = dangling::<usize>();
    unsafe { add(d, 1) };
    1
};
"#,
        expect![[r#"
            12..32: cannot allocate during compile-time evaluation: const-built heap values wait for an interning design (this item's initializer is a const context at 1..7)
            77..90: cannot deallocate during compile-time evaluation: const-built heap values wait for an interning design (this `const fn` is always a const context at 48..53)
            203..204: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn raw_pointer_fields_in_type_declarations_lower() {
    // The declaration side: `T.&raw mut` is a legal field type in
    // a `type` declaration, rigid params included.
    check_infer(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static f = fn (v: HeapVec::<str>) {
    let p = v.ptr;
};
"#,
        expect![[r#"
            84..129 'fn (v: HeapVec::<...': fn(HeapVec::<str>)
            88..89 'v': HeapVec::<str>
            107..129 '{     let p = v.p...': ()
            117..118 'p': str.&raw mut
            121..122 'v': HeapVec::<str>
            121..126 'v.ptr': str.&raw mut
        "#]],
    );
}

#[test]
fn retired_prefix_borrow_type_field_migrates_and_still_needs_a_region() {
    // The retired prefix `&x` superset-parses into the same `BORROW_TYPE`
    // node the postfix form produces (a syntax-level migration diagnostic,
    // see `syntax::tests`), so it flows into hir exactly like a real safe
    // borrow — including the requirement that it name a region. Two
    // genuinely independent complaints, both surviving: the migration
    // (syntax) and the missing region (semantic) — plus `x` naming a value,
    // not a type, unrelated to either.
    check_diagnostics(
        r#"
static x = 4;
type Bad = struct { r: &x };
"#,
        expect![[r#"
            12..13: cannot infer the type of this number: it has no defining use — add a type annotation
            38..39: a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet
            38..40: borrow types are spelled postfix: `T.&` / `T.&mut`
            39..40: `x` is not a type
        "#]],
    );
}

#[test]
fn alloc_result_can_be_named_constructed_and_returned() {
    // The compiler-provided enum is an ordinary nominal enum: nameable in
    // annotations, constructible through variant paths, returned as the
    // enum it is.
    check_diagnostics(
        r#"
static maybe = fn::<T>(p: T.&raw mut, full: bool) -> AllocResult::<T> {
    if full {
        AllocResult::<T>::Err
    } else {
        AllocResult::<T>::Ok(p)
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_alloc_result() {
    // The `print` precedent, applied to the type namespace: a file
    // declaring its own `AllocResult` sees its own everywhere.
    check_infer(
        r#"
type AllocResult = struct { tag: usize };
static f = fn () -> AllocResult {
    AllocResult(struct { tag = 1 })
};
"#,
        expect![[r#"
            54..114 'fn () -> AllocRes...': fn() -> AllocResult
            75..114 '{     AllocResult...': AllocResult
            81..92 'AllocResult': fn(struct { tag: usize }) -> AllocResult
            81..112 'AllocResult(struc...': AllocResult
            93..111 'struct { tag = 1 }': struct { tag: usize }
            108..109 '1': usize
        "#]],
    );
}
