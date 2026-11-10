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
            89..90 'p': usize.&raw
            95..109 'unsafe { p.* }': usize
            102..109 '{ p.* }': usize
            104..105 'p': usize.&raw
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
static f = fn (p: str.&raw) {
    unsafe { dealloc_array(p, 1) };
};
"#,
        expect![[r#"
            12..68 'fn (p: str.&raw) ...': fn(str.&raw)
            16..17 'p': str.&raw
            29..68 '{     unsafe { de...': ()
            35..65 'unsafe { dealloc_...': ()
            42..65 '{ dealloc_array(p...': ()
            44..57 'dealloc_array': unsafe fn(str.&raw, usize)
            44..63 'dealloc_array(p, 1)': ()
            58..59 'p': str.&raw
            61..62 '1': usize
        "#]],
    );
}

#[test]
fn dealloc_array_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw) {
    dealloc_array(p, 1);
};
"#,
        expect![[r#"
            37..56: calling `dealloc_array` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn copy_outside_unsafe_is_rejected() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, q: usize.&raw) {
    copy(p, q, 1);
};
"#,
        expect![[r#"
            52..65: calling `copy` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn turbofished_dealloc_outside_unsafe_is_rejected_too() {
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw) {
    dealloc_array::<usize>(p, 1);
};
"#,
        expect![[r#"
            37..65: calling `dealloc_array` requires an `unsafe { ... }` block
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
static f = fn (p: usize.&raw) {
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
static f = fn (p: usize.&raw) {
    let q = add(p, 1);
};
"#,
        expect![[r#"
            45..54: calling `add` requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn add_preserves_the_pointee() {
    check_infer(
        r#"
static f = fn (s: usize.&raw, m: str.&raw) {
    let a = unsafe { add(s, 1) };
    let b = unsafe { add(m, 1) };
};
"#,
        expect![[r#"
            12..115 'fn (s: usize.&raw...': fn(usize.&raw, str.&raw)
            16..17 's': usize.&raw
            31..32 'm': str.&raw
            44..115 '{     let a = uns...': ()
            54..55 'a': usize.&raw
            58..78 'unsafe { add(s, 1) }': usize.&raw
            65..78 '{ add(s, 1) }': usize.&raw
            67..70 'add': unsafe fn(usize.&raw, usize) -> usize.&raw
            67..76 'add(s, 1)': usize.&raw
            71..72 's': usize.&raw
            74..75 '1': usize
            88..89 'b': str.&raw
            92..112 'unsafe { add(m, 1) }': str.&raw
            99..112 '{ add(m, 1) }': str.&raw
            101..104 'add': unsafe fn(str.&raw, usize) -> str.&raw
            101..110 'add(m, 1)': str.&raw
            105..106 'm': str.&raw
            108..109 '1': usize
        "#]],
    );
}

#[test]
fn copy_pointees_must_agree() {
    check_diagnostics(
        r#"
static f = fn (p: str.&raw, q: usize.&raw) {
    unsafe { copy(p, q, 1) };
};
"#,
        expect![[r#"
            67..68: type mismatch: expected `str.&raw`, found `usize.&raw`
        "#]],
    );
}

#[test]
fn add_of_a_non_pointer_is_rejected() {
    check_diagnostics(
        r#"
static f = fn {
    let a = unsafe { add::<usize>(4, 1) };
};
"#,
        expect![[r#"
            51..52: type mismatch: expected `usize.&raw`, found `{number}`
        "#]],
    );
}

#[test]
fn offset_is_a_free_identifier_after_the_rename() {
    // A user item named `offset` shadows the builtin like any other: it is
    // an ordinary item, first-class, and needs no `unsafe` to call.
    check_diagnostics(
        r#"
static offset = fn (p: usize.&raw) { };
static main = fn (p: usize.&raw) {
    let g = offset;
    offset(p);
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn pointer_builtins_are_first_class_values() {
    // `add`, `offset`, `copy` and the blesses have ordinary fn types, so
    // they bind like any generic or monomorphic fn; calling the binding
    // needs `unsafe`.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw, b: u8.&raw) {
    let g = add::<usize>;
    let h = offset::<usize>;
    let c = copy::<usize>;
    let s = str_from_utf8;
    let u = str_from_utf8_unchecked;
    let q: usize.&raw = unsafe { g(p, 1) };
    let r: usize.&raw = unsafe { h(q, -1) };
    unsafe { c(p, r, 1) };
    let t: str = unsafe { u(b, 0) };
    let w = unsafe { s(b, 0) };
};
"#,
        expect![[r#""#]],
    );
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw) {
    let g = add::<usize>;
    let q = g(p, 1);
};
"#,
        expect![[r#"
            71..78: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_bare_add_needs_its_type_argument() {
    check_diagnostics(
        r#"
static f = fn {
    let g = add;
};
"#,
        expect![[r#"
            29..32: cannot infer the type parameter `T` of `add`; write `add::<...>` to specify it
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
    // The declaration side: `T.&raw` is a legal field type in
    // a `type` declaration, rigid params included.
    check_infer(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw, len: usize, cap: usize };
static f = fn (v: HeapVec::<str>) {
    let p = v.ptr;
};
"#,
        expect![[r#"
            80..125 'fn (v: HeapVec::<...': fn(HeapVec::<str>)
            84..85 'v': HeapVec::<str>
            103..125 '{     let p = v.p...': ()
            113..114 'p': str.&raw
            117..118 'v': HeapVec::<str>
            117..122 'v.ptr': str.&raw
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
static maybe = fn::<T>(p: T.&raw, full: bool) -> AllocResult::<T> {
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
fn user_declarations_shadow_every_compiler_provided_declaration() {
    // The `print` precedent, applied to the type namespace: a file
    // declaring its own `AllocResult` sees its own everywhere. Shadowing
    // goes by name alone, so every row of the table is checked the same
    // way.
    let table = crate::scopes::synthetic_decls();
    assert!(!table.is_empty(), "the table is what this test is about");
    for decl in table {
        let name = decl.name;
        let text = format!(
            r#"
type {name} = struct {{ tag: usize }};
static f = fn () -> {name} {{
    {name}(struct {{ tag = 1 }})
}};
"#
        );
        assert_eq!(render_diagnostics(&text), "", "`{name}` shadowed");
        let types: Vec<String> = infer_pairs(&text).into_iter().map(|(_, ty)| ty).collect();
        assert_eq!(
            types,
            [
                format!("fn() -> {name}"),
                name.to_owned(),
                format!("fn(struct {{ tag: usize }}) -> {name}"),
                name.to_owned(),
                "struct { tag: usize }".to_owned(),
                "usize".to_owned(),
            ],
            "`{name}` shadowed"
        );
    }
}
