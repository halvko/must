//! Generic type declarations and their instances.

use super::*;

// ---- generic type declarations ----

#[test]
fn generic_record_construction_with_explicit_args() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { let p = Pair::<usize>(struct { a = 1, b = 2 }); p.a + p.b };",
        expect![[r#"
            65..141 'fn () -> usize { ...': fn() -> usize
            80..141 '{ let p = Pair::<...': usize
            86..87 'p': Pair::<usize>
            90..103 'Pair::<usize>': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            90..128 'Pair::<usize>(str...': Pair::<usize>
            104..127 'struct { a = 1, b...': struct { a: usize, b: usize }
            117..118 '1': usize
            124..125 '2': usize
            130..131 'p': Pair::<usize>
            130..133 'p.a': usize
            130..139 'p.a + p.b': usize
            136..137 'p': Pair::<usize>
            136..139 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_record_construction_infers_args_from_payload() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { let p = Pair(struct { a = 1, b = 2 }); p.b };",
        expect![[r#"
            65..126 'fn () -> usize { ...': fn() -> usize
            80..126 '{ let p = Pair(st...': usize
            86..87 'p': Pair::<usize>
            90..94 'Pair': fn(struct { a: usize, b: usize }) -> Pair::<usize>
            90..119 'Pair(struct { a =...': Pair::<usize>
            95..118 'struct { a = 1, b...': struct { a: usize, b: usize }
            108..109 '1': usize
            115..116 '2': usize
            121..122 'p': Pair::<usize>
            121..124 'p.b': usize
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p = Pair::<usize, str>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            81..99: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
            113..114: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn generic_type_arity_mismatch_in_annotation_blames_the_use_site() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<usize, str> = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            80..98: `Pair` takes 1 generic argument, found 2 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn bare_generic_type_in_annotation_requires_the_turbofish() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            80..84: `Pair` takes 1 generic argument, found 0 (declared here at 5..9)
        "#]],
    );
}

#[test]
fn annotation_hole_arg_is_pinned_by_the_initializer() {
    check_infer(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<_> = Pair(struct { a = 1, b = 2 }); };",
        expect![[r#"
            65..124 'fn () { let p: Pa...': fn()
            71..124 '{ let p: Pair::<_...': ()
            77..78 'p': Pair::<{number}>
            92..96 'Pair': fn(struct { a: {number}, b: {number} }) -> Pair::<{number}>
            92..121 'Pair(struct { a =...': Pair::<{number}>
            97..120 'struct { a = 1, b...': struct { a: {number}, b: {number} }
            110..111 '1': {number}
            117..118 '2': {number}
        "#]],
    );
}

#[test]
fn different_type_args_do_not_unify() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () { let p: Pair::<str> = Pair::<usize>(struct { a = 1, b = 2 }); };",
        expect![[r#"
            40..41: unexpected character `\`
            94..132: type mismatch: expected `Pair::<str>`, found `Pair::<usize>` (expected `Pair::<str>` because of this annotation at 80..91)
        "#]],
    );
}

#[test]
fn same_args_unify_across_bodies() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static mk = fn () -> Pair::<usize> { Pair::<usize>(struct { a = 1, b = 2 }) };\n\\\n         static use_it = fn (p: Pair::<usize>) -> usize { p.a };\n\\\n         static main = fn () -> usize { use_it(mk()) };",
        expect![[r#"
            40..41: unexpected character `\`
            130..131: unexpected character `\`
            197..198: unexpected character `\`
        "#]],
    );
}

#[test]
fn generic_variant_construction_preserves_args() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             let mut o = Option::<usize>::Some(3);\n\
             o = Option::<usize>::None;\n\
             1\n\
         };",
        expect![[r#"
            57..142 'fn () -> usize { ...': fn() -> usize
            72..142 '{ let mut o = Opt...': usize
            82..83 'o': Option::<usize>
            86..107 'Option::<usize>::...': fn(usize) -> Option::<usize>
            86..110 'Option::<usize>::...': Option::<usize>
            108..109 '3': usize
            112..113 'o': Option::<usize>
            116..137 'Option::<usize>::...': Option::<usize>
            139..140 '1': usize
        "#]],
    );
}

#[test]
fn generic_variant_args_infer_from_the_payload() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::Some(3) };",
        expect![[r#"
            57..101 'fn () -> Option::...': fn() -> Option::<usize>
            82..101 '{ Option::Some(3) }': Option::<usize>
            84..96 'Option::Some': fn(usize) -> Option::<usize>
            84..99 'Option::Some(3)': Option::<usize>
            97..98 '3': usize
        "#]],
    );
}

/// A hole inside a type argument of a group signature that no body pins
/// reaches other items as `{error}`, and a call from another item asks for
/// an annotation (`g(1)` never says which integer).
#[test]
fn signature_hole_in_a_type_arg_needs_annotation_across_items() {
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static g = fn (x) -> Option::<_> { ::Some(x) };
static main = fn () -> () { g(1); };
"#,
        expect![[r#"
            120..121: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 51..52)
        "#]],
    );
}

/// A hole under a raw pointer in a group signature that no body pins
/// reaches other items as `{error}`, and a use from another item asks for
/// an annotation.
#[test]
fn signature_hole_under_a_raw_pointer_needs_annotation_across_items() {
    check_diagnostics(
        r#"
static g = fn (p: _.&raw) -> () { };
static main = fn () -> () {
    let h = g;
};
"#,
        expect![[r#"
            78..79: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );
}

/// An elided region in a group signature reaches other items as
/// `@{error}`, and a call from another item asks for an annotation.
#[test]
fn elided_region_in_a_group_signature_needs_annotation_across_items() {
    check_diagnostics(
        r#"
static g = fn (r: _.&) -> () { };
static main = fn () -> () {
    let n: usize = 1;
    g(n.&)
};
"#,
        expect![[r#"
            21..22: a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet
            89..90: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );
}

/// A qualified variant mention whose type parameter only the signature's
/// hole leaves open reports nothing: the hole is reported where the item
/// is used by another item.
#[test]
fn signature_hole_shared_with_a_qualified_mention_is_not_blamed_on_it() {
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static some = fn (x) -> Option::<_> { Option::Some(x) };
static none = fn () -> Option::<_> { Option::None };
static main = fn () -> () { some(1); };
"#,
        expect![[r#"
            182..186: not enough information to infer the type of `some`; add a type annotation to its definition (defined here at 51..55)
        "#]],
    );
}

/// An unannotated item's whole type is a hole: a qualified variant mention
/// whose type parameter only it leaves open reports nothing.
#[test]
fn unannotated_signature_hole_is_not_blamed_on_a_qualified_mention() {
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static g = fn (x) { Option::Some(x) };
static main = fn () -> usize {
    g(1);
    0
};
"#,
        expect![[r#"
            118..119: not enough information to infer the type of `g`; add a type annotation to its definition (defined here at 51..52)
        "#]],
    );
}

/// A mention whose type parameter nothing ties to the signature reports.
#[test]
fn mention_the_body_never_pins_still_reports() {
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static g = fn (x) -> Option::<_> {
    let n = Option::None;
    Option::Some(x)
};
"#,
        expect![[r#"
            91..103: cannot infer the type parameter `T` of `Option`; write `Option::<...>` to specify it (defined here at 6..12)
        "#]],
    );
}

/// A `forget`-bounded type parameter left open by the signature reports at
/// the mention, the only place the bound is checked.
#[test]
fn forget_bound_left_open_by_the_signature_still_reports() {
    check_diagnostics(
        r#"
static ignore = fn::<T: forget>(t: T) -> () { };
static f = fn (x) { ignore(x) };
"#,
        expect![[r#"
            70..76: cannot infer the type parameter `T` of `ignore`; write `ignore::<...>` to specify it (defined here at 8..14)
        "#]],
    );
}

/// A trait-bounded type parameter its caller's use fills checks the bound
/// at the mention, the only place it is checked.
#[test]
fn trait_bound_on_a_use_filled_parameter_reports() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
static d = fn::<T: D>(x: T) -> usize { x.m() };
static f = fn (x) -> usize { d(x) };
static g = fn () { f(true) };
"#,
        expect![[r#"
            124..125: the bound `T: D` is not satisfied here: `bool` does not implement `D`
        "#]],
    );
}

/// A trait-bounded type parameter nothing fills reports at the mention.
#[test]
fn trait_bound_left_open_by_the_signature_still_reports() {
    check_diagnostics(
        r#"
trait D = requires { m: fn(Self) -> usize; };
static d = fn::<T: D>(x: T) -> usize { x.m() };
static f = fn (x) -> usize { d(x) };
"#,
        expect![[r#"
            124..125: cannot infer the type parameter `T` of `d`; write `d::<...>` to specify it (defined here at 54..55)
        "#]],
    );
}

#[test]
fn constructing_with_the_wrong_args_is_a_mismatch() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::<str>::Some(\"x\") };",
        expect![[r#"
            84..108: type mismatch: expected `Option::<usize>`, found `Option::<str>` (expected `Option::<usize>` because of this return type at 63..81)
        "#]],
    );
}

#[test]
fn match_over_a_generic_enum_is_exhaustive_and_typed() {
    check_infer(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             match Option::<usize>::Some(3) { ::Some(x) => x, ::None => 0, }\n\
         };",
        expect![[r#"
            57..139 'fn () -> usize { ...': fn() -> usize
            72..139 '{ match Option::<...': usize
            74..137 'match Option::<us...': usize
            80..101 'Option::<usize>::...': fn(usize) -> Option::<usize>
            80..104 'Option::<usize>::...': Option::<usize>
            102..103 '3': usize
            114..115 'x': usize
            120..121 'x': usize
            133..134 '0': usize
        "#]],
    );
}

#[test]
fn non_exhaustive_match_over_a_generic_enum_is_reported() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> usize {\n\
             let o: Option::<usize> = Option::<usize>::Some(3);\n\
             match o { ::Some(x) => x, }\n\
         };",
        expect![[r#"
            125..130: this `match` does not cover `Option::None`
        "#]],
    );
}

#[test]
fn const_params_on_types_distinguish_instances() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b: Buf::<8> = Buf::<9>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            103..131: type mismatch: expected `Buf::<8>`, found `Buf::<9>` (expected `Buf::<8>` because of this annotation at 92..100)
        "#]],
    );
}

#[test]
fn const_param_type_annotation_and_construction_agree() {
    check_infer(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () -> usize { let b: Buf::<8> = Buf::<8>(struct { len = 3 }); b.len };",
        expect![[r#"
            77..149 'fn () -> usize { ...': fn() -> usize
            92..149 '{ let b: Buf::<8>...': usize
            98..99 'b': Buf::<8>
            112..120 'Buf::<8>': fn(struct { len: usize }) -> Buf::<8>
            112..140 'Buf::<8>(struct {...': Buf::<8>
            118..119 '8': usize
            121..139 'struct { len = 3 }': struct { len: usize }
            136..137 '3': usize
            142..143 'b': Buf::<8>
            142..147 'b.len': usize
        "#]],
    );
}

#[test]
fn const_block_in_annotation_position_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b: Buf::<const { 4 + 4 }> = Buf::<8>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            98..113: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn const_block_in_type_construction_turbofish_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () { let b = Buf::<const { 4 + 4 }>(struct { len = 1 }); };",
        expect![[r#"
            52..53: unexpected character `\`
            93..115: a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        "#]],
    );
}

#[test]
fn const_param_as_a_field_type_is_rejected() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: N };",
        expect![[r#"
            43..44: `N` is a const parameter, not a type
        "#]],
    );
}

#[test]
fn const_param_forwards_into_a_generic_type_inside_a_generic_fn() {
    check_diagnostics(
        "type Buf = struct::<const N: usize> { len: usize };\n\
         static mk = fn::<const N: usize>(len: usize) -> Buf::<N> { Buf::<N>(struct { len }) };\n\
         static main = fn () { let b: Buf::<8> = mk::<8>(3); };",
        expect![[""]],
    );
}

#[test]
fn self_referential_generic_type_does_not_hang() {
    check_diagnostics(
        "type List = struct::<T> { next: List::<T> };",
        expect![[""]],
    );
}

#[test]
fn unpinned_generic_type_param_is_reported_at_the_mention() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () { let o = Option::None; };",
        expect![[r#"
            73..85: cannot infer the type parameter `T` of `Option`; write `Option::<...>` to specify it (defined here at 5..11)
        "#]],
    );
}

#[test]
fn generic_type_display_in_diagnostics() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { Pair::<usize>(struct { a = 1, b = 2 }) };",
        expect![[r#"
            40..41: unexpected character `\`
            82..120: type mismatch: expected `usize`, found `Pair::<usize>` (expected `usize` because of this return type at 71..79)
        "#]],
    );
}

#[test]
fn generic_type_name_is_not_a_value() {
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         static main = fn () { let x = Pair::<usize>; };",
        expect![[r#"
            70..83: `Pair` is a type, not a value
        "#]],
    );
}

#[test]
fn kind_mismatches_on_a_generic_type_mention() {
    check_diagnostics(
        "type Buf = struct::<T, const N: usize> { x: T };\n\\\n         static main = fn () { let b: Buf::<8, usize> = Buf::<8, usize>(struct { x = 1 }); };",
        expect![[r#"
            49..50: unexpected character `\`
            95..96: `T` is a type parameter; write a type
            98..103: `N` is a const parameter; write a value (a literal, or `const <expr>`)
            107..122: `T` is a type parameter; write a type
            107..122: `N` is a const parameter; write a value (a literal, or `const <expr>`)
            113..114: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn generic_mention_in_a_declaration_field_mirrors_lowering() {
    // Unknown arg name inside a declaration's generic mention.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair::<Missing> };",
        expect![[r#"
            73..80: unknown type `Missing`
        "#]],
    );
    // A bare generic mention in a field needs its args spelled.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair };",
        expect![[r#"
            66..70: `Pair` takes 1 generic argument, found 0 (declared here at 5..9)
        "#]],
    );
    // A hole arg has nothing to infer from in a declaration.
    check_diagnostics(
        "type Pair = struct::<T> { a: T, b: T };\n\
         type Holder = struct { p: Pair::<_> };",
        expect![[r#"
            66..75: a field's type must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn a_variant_in_type_position_offers_the_use_enum_fix() {
    let text = r#"
type Shape = enum { Circle(usize), Point };
static wants_circle = fn (s: Shape::Circle) {};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Use `Shape`");
    assert_eq!(fix.edits.len(), 1);
    let mut fixed = text.to_owned();
    fixed.replace_range(
        std::ops::Range::<usize>::from(fix.edits[0].range),
        &fix.edits[0].insert,
    );
    expect![[r#"

        type Shape = enum { Circle(usize), Point };
        static wants_circle = fn (s: Shape) {};
    "#]]
    .assert_eq(&fixed);
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_generic_enum_variant_is_not_a_type_either() {
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn (o: Option::Some) {};",
        expect![[r#"
            64..76: `Option::Some` is a variant, not a type; use `Option`
        "#]],
    );
}

#[test]
fn generic_enum_payload_mentions_check_in_declarations() {
    // A generic mention inside another enum's payload.
    check_diagnostics(
        "type Option = enum::<T> { Some(T), None };\n\
         type Holder = enum { Held(Option::<usize>), Empty };\n\
         static main = fn () -> Holder { Holder::Held(Option::<usize>::Some(1)) };",
        expect![[""]],
    );
}
