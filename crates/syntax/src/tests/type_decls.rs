//! `type` items, enums and variant paths.

use super::*;

#[test]
fn type_item() {
    check(
        "type Foo = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..31
              TYPE_ITEM@0..31
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..30
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..28
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    PATH_TYPE@23..28
                      NAME_REF@23..28
                        IDENT@23..28 "usize"
                  WHITESPACE@28..29 " "
                  R_BRACE@29..30 "}"
                SEMICOLON@30..31 ";"
        "#]],
    );
}

#[test]
fn type_item_with_non_struct_rhs_parses() {
    // Superset parsing: any expression parses as the RHS; hir restricts it
    // to a `struct` literal with its own diagnostic.
    check(
        "type Foo = 5;",
        expect![[r#"
            SOURCE_FILE@0..13
              TYPE_ITEM@0..13
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..12
                  INT_NUMBER@11..12 "5"
                SEMICOLON@12..13 ";"
        "#]],
    );
}

#[test]
fn type_item_inside_block_recovers() {
    // `type` never starts an expression, so a block statement loop breaks
    // out and the item parses at the top level.
    check(
        "static f = fn {\ntype Foo = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..47
              STATIC_ITEM@0..15
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..15
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..15
                    L_BRACE@14..15 "{"
              WHITESPACE@15..16 "\n"
              TYPE_ITEM@16..47
                ITEM_HEAD@16..20
                  TYPE_KW@16..20 "type"
                WHITESPACE@20..21 " "
                NAME@21..24
                  IDENT@21..24 "Foo"
                WHITESPACE@24..25 " "
                EQ@25..26 "="
                WHITESPACE@26..27 " "
                RECORD_EXPR@27..46
                  STRUCT_KW@27..33 "struct"
                  WHITESPACE@33..34 " "
                  L_BRACE@34..35 "{"
                  WHITESPACE@35..36 " "
                  RECORD_EXPR_FIELD@36..44
                    NAME_REF@36..37
                      IDENT@36..37 "x"
                    COLON@37..38 ":"
                    WHITESPACE@38..39 " "
                    PATH_TYPE@39..44
                      NAME_REF@39..44
                        IDENT@39..44 "usize"
                  WHITESPACE@44..45 " "
                  R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
            error 14..15: expected `}`
        "#]],
    );
}

#[test]
fn type_item_annotation_rejected() {
    // The item shape is shared with `static`/`const`, so `: Type` parses;
    // validation rejects it with a removal fix.
    check(
        "type Foo: usize = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..38
              TYPE_ITEM@0..38
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "usize"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                RECORD_EXPR@18..37
                  STRUCT_KW@18..24 "struct"
                  WHITESPACE@24..25 " "
                  L_BRACE@25..26 "{"
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..35
                    NAME_REF@27..28
                      IDENT@27..28 "x"
                    COLON@28..29 ":"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..35
                      NAME_REF@30..35
                        IDENT@30..35 "usize"
                  WHITESPACE@35..36 " "
                  R_BRACE@36..37 "}"
                SEMICOLON@37..38 ";"
            error 8..15: a `type` declaration takes no type annotation
        "#]],
    );
}

#[test]
fn enum_type_declaration() {
    check(
        "type Shape = enum { Circle(usize), Pair(usize, str), Point };",
        expect![[r#"
            SOURCE_FILE@0..61
              TYPE_ITEM@0..61
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..10
                  IDENT@5..10 "Shape"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                ENUM_EXPR@13..60
                  ENUM_KW@13..17 "enum"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  ENUM_VARIANT@20..33
                    NAME@20..26
                      IDENT@20..26 "Circle"
                    L_PAREN@26..27 "("
                    PATH_TYPE@27..32
                      NAME_REF@27..32
                        IDENT@27..32 "usize"
                    R_PAREN@32..33 ")"
                  COMMA@33..34 ","
                  WHITESPACE@34..35 " "
                  ENUM_VARIANT@35..51
                    NAME@35..39
                      IDENT@35..39 "Pair"
                    L_PAREN@39..40 "("
                    PATH_TYPE@40..45
                      NAME_REF@40..45
                        IDENT@40..45 "usize"
                    COMMA@45..46 ","
                    WHITESPACE@46..47 " "
                    PATH_TYPE@47..50
                      NAME_REF@47..50
                        IDENT@47..50 "str"
                    R_PAREN@50..51 ")"
                  COMMA@51..52 ","
                  WHITESPACE@52..53 " "
                  ENUM_VARIANT@53..58
                    NAME@53..58
                      IDENT@53..58 "Point"
                  WHITESPACE@58..59 " "
                  R_BRACE@59..60 "}"
                SEMICOLON@60..61 ";"
        "#]],
    );
}

#[test]
fn enum_literal_outside_type_declaration_is_rejected() {
    check(
        "static x = enum { A };",
        expect![[r#"
            SOURCE_FILE@0..22
              STATIC_ITEM@0..22
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                ENUM_EXPR@11..21
                  ENUM_KW@11..15 "enum"
                  WHITESPACE@15..16 " "
                  L_BRACE@16..17 "{"
                  WHITESPACE@17..18 " "
                  ENUM_VARIANT@18..19
                    NAME@18..19
                      IDENT@18..19 "A"
                  WHITESPACE@19..20 " "
                  R_BRACE@20..21 "}"
                SEMICOLON@21..22 ";"
            error 11..21: an `enum` literal can only appear as a `type` declaration's value
        "#]],
    );
}

#[test]
fn duplicate_enum_variants_are_rejected() {
    check(
        "type Shape = enum { A, A };",
        expect![[r#"
            SOURCE_FILE@0..27
              TYPE_ITEM@0..27
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..10
                  IDENT@5..10 "Shape"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                ENUM_EXPR@13..26
                  ENUM_KW@13..17 "enum"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  ENUM_VARIANT@20..21
                    NAME@20..21
                      IDENT@20..21 "A"
                  COMMA@21..22 ","
                  WHITESPACE@22..23 " "
                  ENUM_VARIANT@23..24
                    NAME@23..24
                      IDENT@23..24 "A"
                  WHITESPACE@24..25 " "
                  R_BRACE@25..26 "}"
                SEMICOLON@26..27 ";"
            error 23..24: duplicate variant `A`
        "#]],
    );
}

#[test]
fn variant_path_expression() {
    check(
        "static s = Shape::Circle(3);",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..27
                  PATH_EXPR@11..24
                    NAME_REF@11..16
                      IDENT@11..16 "Shape"
                    COLON2@16..18 "::"
                    NAME_REF@18..24
                      IDENT@18..24 "Circle"
                  ARG_LIST@24..27
                    L_PAREN@24..25 "("
                    LITERAL@25..26
                      INT_NUMBER@25..26 "3"
                    R_PAREN@26..27 ")"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression() {
    // The mirror of the elided-sigil variant PATTERN, same node shape: one
    // `NameRef`, with the `COLON2` before it — so it can never be confused
    // with a `PATH_EXPR`, which always starts at an `IDENT`.
    check(
        "static s: Shape = ::Point;",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                ELIDED_VARIANT_EXPR@18..25
                  COLON2@18..20 "::"
                  NAME_REF@20..25
                    IDENT@20..25 "Point"
                SEMICOLON@25..26 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_with_payload() {
    // The payload form is left to the ordinary postfix loop, so it is a
    // `CALL_EXPR` over the sigil node exactly as `Shape::Circle(3)` is one
    // over a `PATH_EXPR`.
    check(
        "static s: Shape = ::Circle(3);",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                CALL_EXPR@18..29
                  ELIDED_VARIANT_EXPR@18..26
                    COLON2@18..20 "::"
                    NAME_REF@20..26
                      IDENT@20..26 "Circle"
                  ARG_LIST@26..29
                    L_PAREN@26..27 "("
                    LITERAL@27..28
                      INT_NUMBER@27..28 "3"
                    R_PAREN@28..29 ")"
                SEMICOLON@29..30 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_after_return() {
    // `return`/`break` take a value only when the next token can start one,
    // so `COLON2` has to be an expression starter or the sigil is stranded
    // and the `return` silently becomes valueless.
    check(
        "static f = fn() -> Shape { return ::Point; };",
        expect![[r#"
            SOURCE_FILE@0..45
              STATIC_ITEM@0..45
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..44
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..15
                    L_PAREN@13..14 "("
                    R_PAREN@14..15 ")"
                  WHITESPACE@15..16 " "
                  RET_TYPE@16..24
                    THIN_ARROW@16..18 "->"
                    WHITESPACE@18..19 " "
                    PATH_TYPE@19..24
                      NAME_REF@19..24
                        IDENT@19..24 "Shape"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..44
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..42
                      RETURN_EXPR@27..41
                        RETURN_KW@27..33 "return"
                        WHITESPACE@33..34 " "
                        ELIDED_VARIANT_EXPR@34..41
                          COLON2@34..36 "::"
                          NAME_REF@36..41
                            IDENT@36..41 "Point"
                      SEMICOLON@41..42 ";"
                    WHITESPACE@42..43 " "
                    R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_missing_name() {
    check(
        "static s: Shape = ::;",
        expect![[r#"
            SOURCE_FILE@0..21
              STATIC_ITEM@0..21
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                ELIDED_VARIANT_EXPR@18..20
                  COLON2@18..20 "::"
                SEMICOLON@20..21 ";"
            error 20..21: expected a variant name after `::`
        "#]],
    );
}

#[test]
fn inherent_member_region_binder_parses_without_reservation() {
    // Regions are the ONE kind of member-own binder that is live: a member
    // taking a borrow of `Self` has nowhere else to bind its per-call
    // region. The type and const kinds keep their reservation, now squiggling
    // the individual param rather than the whole list.
    check(
        "type Cell = struct { n: usize } with {\n\
             impl Self { get = fn::<@b>(m: Self.&::<@b>) -> usize { m.*.n }; }\n\
         };",
        expect![[r#"
            SOURCE_FILE@0..107
              TYPE_ITEM@0..107
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..9
                  IDENT@5..9 "Cell"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                RECORD_EXPR@12..31
                  STRUCT_KW@12..18 "struct"
                  WHITESPACE@18..19 " "
                  L_BRACE@19..20 "{"
                  WHITESPACE@20..21 " "
                  RECORD_EXPR_FIELD@21..29
                    NAME_REF@21..22
                      IDENT@21..22 "n"
                    COLON@22..23 ":"
                    WHITESPACE@23..24 " "
                    PATH_TYPE@24..29
                      NAME_REF@24..29
                        IDENT@24..29 "usize"
                  WHITESPACE@29..30 " "
                  R_BRACE@30..31 "}"
                WHITESPACE@31..32 " "
                WITH_GROUP@32..106
                  WITH_KW@32..36 "with"
                  WHITESPACE@36..37 " "
                  L_BRACE@37..38 "{"
                  WHITESPACE@38..39 "\n"
                  IMPL_ELEMENT@39..104
                    IMPL_KW@39..43 "impl"
                    WHITESPACE@43..44 " "
                    PATH_TYPE@44..48
                      NAME_REF@44..48
                        IDENT@44..48 "Self"
                    WHITESPACE@48..49 " "
                    L_BRACE@49..50 "{"
                    WHITESPACE@50..51 " "
                    MEMBER@51..102
                      NAME@51..54
                        IDENT@51..54 "get"
                      WHITESPACE@54..55 " "
                      EQ@55..56 "="
                      WHITESPACE@56..57 " "
                      FN_LITERAL@57..101
                        FN_KW@57..59 "fn"
                        GENERIC_PARAM_LIST@59..65
                          COLON2@59..61 "::"
                          L_ANGLE@61..62 "<"
                          REGION_PARAM@62..64
                            REGION_IDENT@62..64 "@b"
                          R_ANGLE@64..65 ">"
                        PARAM_LIST@65..82
                          L_PAREN@65..66 "("
                          PARAM@66..81
                            BIND_PAT@66..67
                              NAME@66..67
                                IDENT@66..67 "m"
                            COLON@67..68 ":"
                            WHITESPACE@68..69 " "
                            BORROW_TYPE@69..81
                              PATH_TYPE@69..73
                                NAME_REF@69..73
                                  IDENT@69..73 "Self"
                              DOT@73..74 "."
                              AMP@74..75 "&"
                              COLON2@75..77 "::"
                              GENERIC_ARG_LIST@77..81
                                L_ANGLE@77..78 "<"
                                REGION_ARG@78..80
                                  REGION_IDENT@78..80 "@b"
                                R_ANGLE@80..81 ">"
                          R_PAREN@81..82 ")"
                        WHITESPACE@82..83 " "
                        RET_TYPE@83..91
                          THIN_ARROW@83..85 "->"
                          WHITESPACE@85..86 " "
                          PATH_TYPE@86..91
                            NAME_REF@86..91
                              IDENT@86..91 "usize"
                        WHITESPACE@91..92 " "
                        BLOCK_EXPR@92..101
                          L_BRACE@92..93 "{"
                          WHITESPACE@93..94 " "
                          FIELD_EXPR@94..99
                            DEREF_EXPR@94..97
                              PATH_EXPR@94..95
                                NAME_REF@94..95
                                  IDENT@94..95 "m"
                              DOT@95..96 "."
                              STAR@96..97 "*"
                            DOT@97..98 "."
                            NAME_REF@98..99
                              IDENT@98..99 "n"
                          WHITESPACE@99..100 " "
                          R_BRACE@100..101 "}"
                      SEMICOLON@101..102 ";"
                    WHITESPACE@102..103 " "
                    R_BRACE@103..104 "}"
                  WHITESPACE@104..105 "\n"
                  R_BRACE@105..106 "}"
                SEMICOLON@106..107 ";"
        "#]],
    );
}

#[test]
fn variant_path_type_annotation() {
    check(
        "static s: Shape::Circle = c;",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..23
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                  COLON2@15..17 "::"
                  NAME_REF@17..23
                    IDENT@17..23 "Circle"
                WHITESPACE@23..24 " "
                EQ@24..25 "="
                WHITESPACE@25..26 " "
                PATH_EXPR@26..27
                  NAME_REF@26..27
                    IDENT@26..27 "c"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn variant_path_missing_second_segment() {
    check(
        "static s = Shape::;",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                PATH_EXPR@11..18
                  NAME_REF@11..16
                    IDENT@11..16 "Shape"
                  COLON2@16..18 "::"
                SEMICOLON@18..19 ";"
            error 18..19: expected a variant name after `::`
        "#]],
    );
}
