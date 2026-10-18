//! Semantic highlighting.

use super::*;

#[test]
fn highlights_classify_names_semantically() {
    check_highlights(
        r#"// the entry point
static greeting: str = "hi";
static main = fn (count: usize) {
    let next = count - 1;
    print(greeting);
    main(next);
};"#,
        expect_test::expect![[r#"
            0..18 "// the entry point" Comment
            19..25 "static" Keyword
            26..34 "greeting" Variable.declaration.static
            36..39 "str" Type.defaultLibrary
            40..41 "=" Operator
            42..46 "\"hi\"" String
            48..54 "static" Keyword
            55..59 "main" Function.declaration.static
            60..61 "=" Operator
            62..64 "fn" Keyword
            66..71 "count" Parameter.declaration
            73..78 "usize" Type.defaultLibrary
            86..89 "let" Keyword
            90..94 "next" Variable.declaration
            95..96 "=" Operator
            97..102 "count" Parameter
            103..104 "-" Operator
            105..106 "1" Number
            112..117 "print" Function.defaultLibrary
            118..126 "greeting" Variable.static
            133..137 "main" Function.static
            138..142 "next" Variable
        "#]],
    );
}

#[test]
fn highlights_block_comments_as_comments() {
    // `/* ... */` shares `COMMENT`'s one lexical tag with `//` (see
    // `lexer::scan_block_comment`), so it needs no highlighter code of its
    // own — pinned here so that stays true. Nested, to also confirm the
    // highlighter sees ONE token spanning the whole thing rather than
    // splitting at the first (inner) `*/`.
    check_highlights(
        "/* a /* nested */ comment */\nstatic x = 1;",
        expect_test::expect![[r#"
            0..28 "/* a /* nested */ comment */" Comment
            29..35 "static" Keyword
            36..37 "x" Variable.declaration.static
            38..39 "=" Operator
            40..41 "1" Number
        "#]],
    );
}

#[test]
fn highlights_split_multiline_block_comments_per_line() {
    // `push_line_split` already existed for multi-line STRING tokens (LSP
    // clients aren't required to handle a highlight range that spans
    // lines) — `/* */` is the second token kind to go through that same
    // function, since `//` never contained a newline of its own to split
    // on. Two ranges, not one, and the `\n` between them is in neither.
    check_highlights(
        "static s = 1; /* one\ntwo */",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "s" Variable.declaration.static
            9..10 "=" Operator
            11..12 "1" Number
            14..20 "/* one" Comment
            21..27 "two */" Comment
        "#]],
    );
}

#[test]
fn highlights_hole_pattern_unstyled() {
    // `_` isn't an IDENT token, so it's left unclassified (no panic, no
    // bogus Variable highlight).
    check_highlights(
        "static f = fn { let _ = 1; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            22..23 "=" Operator
            24..25 "1" Number
        "#]],
    );
}

#[test]
fn highlights_const_fn_and_const_block_keywords() {
    check_highlights(
        "static f = const fn { const { 1 } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..16 "const" Keyword
            17..19 "fn" Keyword
            22..27 "const" Keyword
            30..31 "1" Number
        "#]],
    );
}

#[test]
fn highlights_struct_keyword() {
    check_highlights(
        "static p = struct { x = 1 };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "p" Variable.declaration.static
            9..10 "=" Operator
            11..17 "struct" Keyword
            22..23 "=" Operator
            24..25 "1" Number
        "#]],
    );
}

#[test]
fn highlights_mut_keyword() {
    check_highlights(
        "static f = fn { let mut x = 1; x = 2; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            20..23 "mut" Keyword
            24..25 "x" Variable.declaration.mutable
            26..27 "=" Operator
            28..29 "1" Number
            31..32 "x" Variable.mutable
            33..34 "=" Operator
            35..36 "2" Number
        "#]],
    );
}

#[test]
fn highlights_mutable_param_declaration_and_uses() {
    // The `mutable` modifier follows the binding, not just its declaration:
    // both the `mut` parameter's name and its later reads/assignment carry
    // it, while an ordinary immutable `let` next to it does not.
    check_highlights(
        "static f = fn (mut n: usize) { let y = n; n = y; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..18 "mut" Keyword
            19..20 "n" Parameter.declaration.mutable
            22..27 "usize" Type.defaultLibrary
            31..34 "let" Keyword
            35..36 "y" Variable.declaration
            37..38 "=" Operator
            39..40 "n" Parameter.mutable
            42..43 "n" Parameter.mutable
            44..45 "=" Operator
            46..47 "y" Variable
        "#]],
    );
}

#[test]
fn highlights_a_fn_types_parameters_as_types() {
    check_highlights(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;",
        expect_test::expect![[r#"
            0..6 "unsafe" Keyword
            7..13 "extern" Keyword
            14..20 "static" Keyword
            21..25 "read" Function.declaration.static
            27..33 "unsafe" Keyword
            34..36 "fn" Keyword
            37..39 "u8" Type.defaultLibrary
            40..41 "&" Operator
            41..44 "raw" Keyword
            45..48 "mut" Keyword
            50..55 "usize" Type.defaultLibrary
            57..59 "->" Operator
            60..65 "isize" Type.defaultLibrary
        "#]],
    );
}

#[test]
fn highlights_split_multiline_strings_per_line() {
    check_highlights(
        "static s = \"one\ntwo\";",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "s" Variable.declaration.static
            9..10 "=" Operator
            11..15 "\"one" String
            16..20 "two\"" String
        "#]],
    );
}

#[test]
fn highlights_character_literals_as_strings() {
    // A character literal is string-like and colored like one: the palette
    // has no character class, and every theme already paints quoted text
    // the same way whichever quote it is. A literal PATTERN gets the same
    // tag as an expression — the token kind decides, and there is only one
    // token kind for both positions.
    check_highlights(
        "static c = 'x';\nstatic f = fn (c: char) -> usize { match c { '\\n' => 1, _ => 0 } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "c" Variable.declaration.static
            9..10 "=" Operator
            11..14 "'x'" String
            16..22 "static" Keyword
            23..24 "f" Function.declaration.static
            25..26 "=" Operator
            27..29 "fn" Keyword
            31..32 "c" Parameter.declaration
            34..38 "char" Type.defaultLibrary
            40..42 "->" Operator
            43..48 "usize" Type.defaultLibrary
            51..56 "match" Keyword
            57..58 "c" Parameter
            61..65 "'\\n'" String
            66..68 "=>" Operator
            69..70 "1" Number
            74..76 "=>" Operator
            77..78 "0" Number
        "#]],
    );
}

#[test]
fn highlights_named_types() {
    // The declaration and both reference positions (annotation,
    // construction head) carry the type highlight class; builtins keep
    // the library modifier, user types don't.
    check_highlights(
        r#"type Foo = struct { x: usize };
static f = fn (p: Foo) { Foo(struct { x = p.x }) };"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..8 "Foo" Type.declaration
            9..10 "=" Operator
            11..17 "struct" Keyword
            23..28 "usize" Type.defaultLibrary
            32..38 "static" Keyword
            39..40 "f" Function.declaration.static
            41..42 "=" Operator
            43..45 "fn" Keyword
            47..48 "p" Parameter.declaration
            50..53 "Foo" Type
            57..60 "Foo" Type
            61..67 "struct" Keyword
            72..73 "=" Operator
            74..75 "p" Parameter
        "#]],
    );
}

#[test]
fn highlights_the_expression_sigil_as_an_enum_member() {
    check_highlights(
        r#"type Shape = enum { Circle(usize), Point };
static s: Shape = ::Point;"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "s" Variable.declaration.static
            54..59 "Shape" Type
            60..61 "=" Operator
            64..69 "Point" EnumMember
        "#]],
    );
}

#[test]
fn highlights_enum_declaration_and_variant_paths() {
    check_highlights(
        r#"type Shape = enum { Circle(usize), Point };
static s: Shape::Circle = Shape::Circle(3);"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "s" Variable.declaration.static
            54..59 "Shape" Type
            61..67 "Circle" EnumMember
            68..69 "=" Operator
            70..75 "Shape" Type
            77..83 "Circle" EnumMember
            84..85 "3" Number
        "#]],
    );
}

#[test]
fn highlights_match_expression() {
    check_highlights(
        // A qualified arm (`Shape::Circle`), an elided sigil arm
        // (`::Point`, whose sole segment must still color as an enum
        // member), a bare bind whose name shadows a variant (`Point` —
        // now a plain `Variable`, never an `EnumMember`), and an ordinary
        // bind (`other`).
        r#"type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        ::Point => 0,
        Point => 1,
        other => 2,
    }
};"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "f" Function.declaration.static
            53..54 "=" Operator
            55..57 "fn" Keyword
            59..60 "s" Parameter.declaration
            62..67 "Shape" Type
            69..71 "->" Operator
            72..77 "usize" Type.defaultLibrary
            84..89 "match" Keyword
            90..91 "s" Parameter
            102..107 "Shape" Type
            109..115 "Circle" EnumMember
            116..117 "r" Variable.declaration
            119..121 "=>" Operator
            122..123 "r" Variable
            135..140 "Point" EnumMember
            141..143 "=>" Operator
            144..145 "0" Number
            155..160 "Point" Variable.declaration
            161..163 "=>" Operator
            164..165 "1" Number
            175..180 "other" Variable.declaration
            181..183 "=>" Operator
            184..185 "2" Number
        "#]],
    );
}

#[test]
fn highlights_loop_break_continue_keywords() {
    check_highlights(
        "static f = fn { loop { if true { break 1; }; continue; } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..20 "loop" Keyword
            23..25 "if" Keyword
            26..30 "true" Keyword
            33..38 "break" Keyword
            39..40 "1" Number
            45..53 "continue" Keyword
        "#]],
    );
}

#[test]
fn highlights_survive_array_syntax() {
    check_highlights(
        r#"static f = fn (i: usize) { let mut a = [1, 2]; a[i] = a[0]; };"#,
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..16 "i" Parameter.declaration
            18..23 "usize" Type.defaultLibrary
            27..30 "let" Keyword
            31..34 "mut" Keyword
            35..36 "a" Variable.declaration.mutable
            37..38 "=" Operator
            40..41 "1" Number
            43..44 "2" Number
            47..48 "a" Variable.mutable
            49..50 "i" Parameter
            52..53 "=" Operator
            54..55 "a" Variable.mutable
            56..57 "0" Number
        "#]],
    );
}

#[test]
fn a_dot_call_with_a_member_turbofish_still_highlights_as_a_function() {
    // The member name keeps its `function` token with the new node sitting
    // beside it — `MEMBER_GENERIC_ARGS` is a CHILD of the `FIELD_EXPR`, so
    // the call is still the field expression's parent and the resolution
    // lookup is unchanged.
    check_highlights(
        r#"type C = struct { n: usize } with {
    impl Self { pick = fn::<U>(alt: U, c: Self) -> U { alt }; }
};
static main = fn(c: C) -> bool { c.pick::<bool>(true) };
"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..6 "C" Type.declaration
            7..8 "=" Operator
            9..15 "struct" Keyword
            21..26 "usize" Type.defaultLibrary
            29..33 "with" Keyword
            40..44 "impl" Keyword
            45..49 "Self" Type
            52..56 "pick" Function.declaration
            57..58 "=" Operator
            59..61 "fn" Keyword
            63..64 "<" Operator
            64..65 "U" TypeParameter.declaration
            65..66 ">" Operator
            67..70 "alt" Parameter.declaration
            72..73 "U" TypeParameter
            75..76 "c" Parameter.declaration
            78..82 "Self" Type
            84..86 "->" Operator
            87..88 "U" TypeParameter
            91..94 "alt" Parameter
            103..109 "static" Keyword
            110..114 "main" Function.declaration.static
            115..116 "=" Operator
            117..119 "fn" Keyword
            120..121 "c" Parameter.declaration
            123..124 "C" Type
            126..128 "->" Operator
            129..133 "bool" Type.defaultLibrary
            136..137 "c" Parameter
            138..142 "pick" Function
            144..145 "<" Operator
            145..149 "bool" Type.defaultLibrary
            149..150 ">" Operator
            151..155 "true" Keyword
        "#]],
    );
}

#[test]
fn member_bodies_highlight_as_code() {
    check_highlights(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static use_it = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect_test::expect![[r#"
            1..5 "type" Keyword
            6..7 "A" Type.declaration
            8..9 "=" Operator
            10..16 "struct" Keyword
            22..27 "usize" Type.defaultLibrary
            30..34 "with" Keyword
            41..45 "impl" Keyword
            46..50 "Self" Type
            61..64 "get" Function.declaration
            65..66 "=" Operator
            67..69 "fn" Keyword
            70..71 "a" Parameter.declaration
            73..77 "Self" Type
            79..81 "->" Operator
            82..87 "usize" Type.defaultLibrary
            90..91 "a" Parameter
            106..112 "static" Keyword
            113..119 "use_it" Function.declaration.static
            120..121 "=" Operator
            122..124 "fn" Keyword
            127..129 "->" Operator
            130..135 "usize" Type.defaultLibrary
            138..139 "A" Type
            140..146 "struct" Keyword
            151..152 "=" Operator
            153..154 "1" Number
            158..161 "get" Function
        "#]],
    );
}

// ---- keyword drift guard ----------------------------------------------

#[test]
fn every_canonical_keyword_classifies_as_a_keyword_highlight() {
    // The architectural guard: keyword-ness reaches the highlighter from
    // the syntax crate's ONE canonical table (`syntax::KEYWORDS`, which
    // also generates `from_keyword`/`is_keyword`), never from a list kept
    // here. A keyword added to the language must therefore highlight with
    // no edit to `syntax_highlighting` — and if anyone reintroduces a
    // hand-written match arm that forgets one, this fails.
    for &(text, kind) in syntax::KEYWORDS {
        assert_eq!(
            syntax::SyntaxKind::from_keyword(text),
            Some(kind),
            "`{text}` does not round-trip through the keyword table"
        );
        assert_eq!(
            crate::syntax_highlighting::lexical_tag(kind),
            Some(crate::HlTag::Keyword),
            "`{text}` ({kind:?}) is a keyword but does not classify as one"
        );
    }
    // Identifiers are classified through hir, not by kind — the keyword
    // branch must not swallow them.
    assert_eq!(
        crate::syntax_highlighting::lexical_tag(syntax::SyntaxKind::IDENT),
        None
    );
}

#[test]
fn return_highlights_as_a_keyword_with_no_ide_change() {
    // The keyword-table architecture, validated on a keyword added AFTER
    // the highlighter was written: `return` reached `syntax_kind.rs`'s
    // `keywords!` table as a one-line entry and NOTHING in this crate was
    // touched to teach the highlighter about it. Rendered here on real
    // code rather than only through the drift guard above, so the claim is
    // legible.
    check_highlights(
        r#"static f = fn (c: bool) -> usize {
    if c { return 1; };
    return 2;
};"#,
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..16 "c" Parameter.declaration
            18..22 "bool" Type.defaultLibrary
            24..26 "->" Operator
            27..32 "usize" Type.defaultLibrary
            39..41 "if" Keyword
            42..43 "c" Parameter
            46..52 "return" Keyword
            53..54 "1" Number
            63..69 "return" Keyword
            70..71 "2" Number
        "#]],
    );
}

// ---- trait-aware classification ---------------------------------------

#[test]
fn highlights_trait_declaration_and_requirements() {
    check_highlights(
        r#"
trait Show = requires {
    show: fn(Self) -> str;
    bare: fn(Self) -> str;
};
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..11 "Show" Trait.declaration
            12..13 "=" Operator
            14..22 "requires" Keyword
            29..33 "show" Function.declaration
            35..37 "fn" Keyword
            38..42 "Self" Type
            44..46 "->" Operator
            47..50 "str" Type.defaultLibrary
            56..60 "bare" Function.declaration
            62..64 "fn" Keyword
            65..69 "Self" Type
            71..73 "->" Operator
            74..77 "str" Type.defaultLibrary
        "#]],
    );
}

#[test]
fn highlights_trait_names_at_every_occurrence() {
    // One trait, every position it can be written in: the declaration, an
    // impl head in the trait's OWN chain (the trait-side home), an impl
    // head in a type's chain (the type-side home), a bound on a binder,
    // and the base of a qualified member call.
    check_highlights(
        r#"
trait Show = requires {
    show: fn(Self) -> str;
} with {
    impl usize {
        show = fn(x: usize) -> str { "n" };
    }
};
type Tag = struct { t: usize } with {
    impl Show {
        show = fn(x: Self) -> str { Show::show(x.t) };
    }
};
static render = fn::<T: Show>(v: T) -> str { v.show() };
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..11 "Show" Trait.declaration
            12..13 "=" Operator
            14..22 "requires" Keyword
            29..33 "show" Function.declaration
            35..37 "fn" Keyword
            38..42 "Self" Type
            44..46 "->" Operator
            47..50 "str" Type.defaultLibrary
            54..58 "with" Keyword
            65..69 "impl" Keyword
            70..75 "usize" Type.defaultLibrary
            86..90 "show" Function.declaration
            91..92 "=" Operator
            93..95 "fn" Keyword
            96..97 "x" Parameter.declaration
            99..104 "usize" Type.defaultLibrary
            106..108 "->" Operator
            109..112 "str" Type.defaultLibrary
            115..118 "\"n\"" String
            131..135 "type" Keyword
            136..139 "Tag" Type.declaration
            140..141 "=" Operator
            142..148 "struct" Keyword
            154..159 "usize" Type.defaultLibrary
            162..166 "with" Keyword
            173..177 "impl" Keyword
            178..182 "Show" Trait
            193..197 "show" Function.declaration
            198..199 "=" Operator
            200..202 "fn" Keyword
            203..204 "x" Parameter.declaration
            206..210 "Self" Type
            212..214 "->" Operator
            215..218 "str" Type.defaultLibrary
            221..225 "Show" Trait
            227..231 "show" Function
            232..233 "x" Parameter
            249..255 "static" Keyword
            256..262 "render" Function.declaration.static
            263..264 "=" Operator
            265..267 "fn" Keyword
            269..270 "<" Operator
            270..271 "T" TypeParameter.declaration
            273..277 "Show" Trait
            277..278 ">" Operator
            279..280 "v" Parameter.declaration
            282..283 "T" TypeParameter
            285..287 "->" Operator
            288..291 "str" Type.defaultLibrary
            294..295 "v" Parameter
            296..300 "show" Function
        "#]],
    );
}

#[test]
fn highlights_generic_binders_and_their_uses() {
    // Type params and const params, at the binder and at every use — in
    // signatures, in bodies, and in a `type` declaration's fields (whose
    // binder sits on the RHS literal, in scope for the whole declaration).
    check_highlights(
        r#"
static id = fn::<T>(x: T) -> T { x };
static rep = const fn::<const N: usize>() -> usize { N + N };
type Pair = struct::<T> { a: T, b: T };
"#,
        expect_test::expect![[r#"
            1..7 "static" Keyword
            8..10 "id" Function.declaration.static
            11..12 "=" Operator
            13..15 "fn" Keyword
            17..18 "<" Operator
            18..19 "T" TypeParameter.declaration
            19..20 ">" Operator
            21..22 "x" Parameter.declaration
            24..25 "T" TypeParameter
            27..29 "->" Operator
            30..31 "T" TypeParameter
            34..35 "x" Parameter
            39..45 "static" Keyword
            46..49 "rep" Function.declaration.static
            50..51 "=" Operator
            52..57 "const" Keyword
            58..60 "fn" Keyword
            62..63 "<" Operator
            63..68 "const" Keyword
            69..70 "N" TypeParameter.declaration
            72..77 "usize" Type.defaultLibrary
            77..78 ">" Operator
            81..83 "->" Operator
            84..89 "usize" Type.defaultLibrary
            92..93 "N" TypeParameter
            94..95 "+" Operator
            96..97 "N" TypeParameter
            101..105 "type" Keyword
            106..110 "Pair" Type.declaration
            111..112 "=" Operator
            113..119 "struct" Keyword
            121..122 "<" Operator
            122..123 "T" TypeParameter.declaration
            123..124 ">" Operator
            130..131 "T" TypeParameter
            136..137 "T" TypeParameter
        "#]],
    );
}

#[test]
fn highlights_type_binder_reaches_its_with_chain_members() {
    // The declaration's binder is in scope inside its `with`-chain, which
    // is a SIBLING of the literal carrying the binder — the one place a
    // naive ancestor walk would lose `T`.
    check_highlights(
        r#"
type Box = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { b.v };
    }
};
"#,
        expect_test::expect![[r#"
            1..5 "type" Keyword
            6..9 "Box" Type.declaration
            10..11 "=" Operator
            12..18 "struct" Keyword
            20..21 "<" Operator
            21..22 "T" TypeParameter.declaration
            22..23 ">" Operator
            29..30 "T" TypeParameter
            33..37 "with" Keyword
            44..48 "impl" Keyword
            49..53 "Self" Type
            64..67 "get" Function.declaration
            68..69 "=" Operator
            70..72 "fn" Keyword
            73..74 "b" Parameter.declaration
            76..80 "Self" Type
            82..84 "->" Operator
            85..86 "T" TypeParameter
            89..90 "b" Parameter
        "#]],
    );
}

/// A const parameter named as an array length in a signature is a generic
/// parameter there, as it is in the body.
#[test]
fn highlights_const_param_in_a_signature_array_length() {
    check_highlights(
        r#"
static rep = const fn::<const N: usize>(v: usize) -> [usize; N] { [v; N] };
"#,
        expect_test::expect![[r#"
            1..7 "static" Keyword
            8..11 "rep" Function.declaration.static
            12..13 "=" Operator
            14..19 "const" Keyword
            20..22 "fn" Keyword
            24..25 "<" Operator
            25..30 "const" Keyword
            31..32 "N" TypeParameter.declaration
            34..39 "usize" Type.defaultLibrary
            39..40 ">" Operator
            41..42 "v" Parameter.declaration
            44..49 "usize" Type.defaultLibrary
            51..53 "->" Operator
            55..60 "usize" Type.defaultLibrary
            62..63 "N" TypeParameter
            68..69 "v" Parameter
            71..72 "N" TypeParameter
        "#]],
    );
}

/// A renamed record-pattern field's first name is left plain in a
/// parameter, as it is in a `let`; only the rename is a binding.
#[test]
fn highlights_leave_a_renamed_pattern_field_name_plain() {
    check_highlights(
        r#"
static f = fn (struct { y as z }: struct { y: usize }) -> usize {
    let struct { y as w } = struct { y = z };
    w
};
"#,
        expect_test::expect![[r#"
            1..7 "static" Keyword
            8..9 "f" Function.declaration.static
            10..11 "=" Operator
            12..14 "fn" Keyword
            16..22 "struct" Keyword
            27..29 "as" Keyword
            30..31 "z" Parameter.declaration
            35..41 "struct" Keyword
            47..52 "usize" Type.defaultLibrary
            56..58 "->" Operator
            59..64 "usize" Type.defaultLibrary
            71..74 "let" Keyword
            75..81 "struct" Keyword
            86..88 "as" Keyword
            89..90 "w" Variable.declaration
            93..94 "=" Operator
            95..101 "struct" Keyword
            106..107 "=" Operator
            108..109 "z" Parameter
            117..118 "w" Variable
        "#]],
    );
}

#[test]
fn qualified_member_paths_highlight_as_functions() {
    // A qualified member path shares the `Name::name` shape with a variant
    // path but names a FUNCTION — after a TYPE (`P::len`) as well as after
    // a TRAIT, here in the named-`Self` form, whose named argument reads
    // like the type it stands for. (The trait short form `D::m` is pinned
    // by `highlights_trait_names_at_every_occurrence`.)
    check_highlights(
        r#"
trait D = requires { m: fn(Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static a = fn(p: P) -> usize { D::<Self = usize>::m(1) + P::len(p) };
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..8 "D" Trait.declaration
            9..10 "=" Operator
            11..19 "requires" Keyword
            22..23 "m" Function.declaration
            25..27 "fn" Keyword
            28..32 "Self" Type
            34..36 "->" Operator
            37..42 "usize" Type.defaultLibrary
            46..50 "with" Keyword
            57..61 "impl" Keyword
            62..67 "usize" Type.defaultLibrary
            70..71 "m" Function.declaration
            72..73 "=" Operator
            74..76 "fn" Keyword
            77..78 "x" Parameter.declaration
            80..85 "usize" Type.defaultLibrary
            87..89 "->" Operator
            90..95 "usize" Type.defaultLibrary
            98..99 "x" Parameter
            108..112 "type" Keyword
            113..114 "P" Type.declaration
            115..116 "=" Operator
            117..123 "struct" Keyword
            129..134 "usize" Type.defaultLibrary
            137..141 "with" Keyword
            148..152 "impl" Keyword
            153..157 "Self" Type
            160..163 "len" Function.declaration
            164..165 "=" Operator
            166..168 "fn" Keyword
            169..170 "p" Parameter.declaration
            172..176 "Self" Type
            178..180 "->" Operator
            181..186 "usize" Type.defaultLibrary
            189..190 "p" Parameter
            201..207 "static" Keyword
            208..209 "a" Function.declaration.static
            210..211 "=" Operator
            212..214 "fn" Keyword
            215..216 "p" Parameter.declaration
            218..219 "P" Type
            221..223 "->" Operator
            224..229 "usize" Type.defaultLibrary
            232..233 "D" Trait
            235..236 "<" Operator
            236..240 "Self" Type
            241..242 "=" Operator
            243..248 "usize" Type.defaultLibrary
            248..249 ">" Operator
            251..252 "m" Function
            253..254 "1" Number
            256..257 "+" Operator
            258..259 "P" Type
            261..264 "len" Function
            265..266 "p" Parameter
        "#]],
    );
}
