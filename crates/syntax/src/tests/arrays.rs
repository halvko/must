//! Array types, literals, repeats and indexing.

use super::*;

// ---- fixed-size arrays ----

#[test]
fn array_type_positions() {
    check(
        "static x: [usize; 3] = y;",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..20
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  SEMICOLON@16..17 ";"
                  WHITESPACE@17..18 " "
                  CONST_ARG@18..19
                    LITERAL@18..19
                      INT_NUMBER@18..19 "3"
                  R_BRACKET@19..20 "]"
                WHITESPACE@20..21 " "
                EQ@21..22 "="
                WHITESPACE@22..23 " "
                PATH_EXPR@23..24
                  NAME_REF@23..24
                    IDENT@23..24 "y"
                SEMICOLON@24..25 ";"
        "#]],
    );
}

#[test]
fn array_type_nested() {
    check(
        "static m: [[usize; 2]; 3] = y;",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "m"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..25
                  L_BRACKET@10..11 "["
                  ARRAY_TYPE@11..21
                    L_BRACKET@11..12 "["
                    PATH_TYPE@12..17
                      NAME_REF@12..17
                        IDENT@12..17 "usize"
                    SEMICOLON@17..18 ";"
                    WHITESPACE@18..19 " "
                    CONST_ARG@19..20
                      LITERAL@19..20
                        INT_NUMBER@19..20 "2"
                    R_BRACKET@20..21 "]"
                  SEMICOLON@21..22 ";"
                  WHITESPACE@22..23 " "
                  CONST_ARG@23..24
                    LITERAL@23..24
                      INT_NUMBER@23..24 "3"
                  R_BRACKET@24..25 "]"
                WHITESPACE@25..26 " "
                EQ@26..27 "="
                WHITESPACE@27..28 " "
                PATH_EXPR@28..29
                  NAME_REF@28..29
                    IDENT@28..29 "y"
                SEMICOLON@29..30 ";"
        "#]],
    );
}

#[test]
fn array_type_const_param_length() {
    check(
        "static f = fn::<const N: usize>(b: [usize; N]) -> usize { 0 };",
        expect![[r#"
            SOURCE_FILE@0..62
              STATIC_ITEM@0..62
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..61
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..31
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    CONST_PARAM@16..30
                      CONST_KW@16..21 "const"
                      WHITESPACE@21..22 " "
                      NAME@22..23
                        IDENT@22..23 "N"
                      COLON@23..24 ":"
                      WHITESPACE@24..25 " "
                      PATH_TYPE@25..30
                        NAME_REF@25..30
                          IDENT@25..30 "usize"
                    R_ANGLE@30..31 ">"
                  PARAM_LIST@31..46
                    L_PAREN@31..32 "("
                    PARAM@32..45
                      BIND_PAT@32..33
                        NAME@32..33
                          IDENT@32..33 "b"
                      COLON@33..34 ":"
                      WHITESPACE@34..35 " "
                      ARRAY_TYPE@35..45
                        L_BRACKET@35..36 "["
                        PATH_TYPE@36..41
                          NAME_REF@36..41
                            IDENT@36..41 "usize"
                        SEMICOLON@41..42 ";"
                        WHITESPACE@42..43 " "
                        CONST_ARG@43..44
                          PATH_EXPR@43..44
                            NAME_REF@43..44
                              IDENT@43..44 "N"
                        R_BRACKET@44..45 "]"
                    R_PAREN@45..46 ")"
                  WHITESPACE@46..47 " "
                  RET_TYPE@47..55
                    THIN_ARROW@47..49 "->"
                    WHITESPACE@49..50 " "
                    PATH_TYPE@50..55
                      NAME_REF@50..55
                        IDENT@50..55 "usize"
                  WHITESPACE@55..56 " "
                  BLOCK_EXPR@56..61
                    L_BRACE@56..57 "{"
                    WHITESPACE@57..58 " "
                    LITERAL@58..59
                      INT_NUMBER@58..59 "0"
                    WHITESPACE@59..60 " "
                    R_BRACE@60..61 "}"
                SEMICOLON@61..62 ";"
        "#]],
    );
}

#[test]
fn array_type_const_block_length_parses() {
    // `const { ... }` parses as the length (resilience); hir rejects it.
    check(
        "static x: [usize; const { 3 }] = y;",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..30
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  SEMICOLON@16..17 ";"
                  WHITESPACE@17..18 " "
                  CONST_ARG@18..29
                    CONST_BLOCK_EXPR@18..29
                      CONST_KW@18..23 "const"
                      WHITESPACE@23..24 " "
                      BLOCK_EXPR@24..29
                        L_BRACE@24..25 "{"
                        WHITESPACE@25..26 " "
                        LITERAL@26..27
                          INT_NUMBER@26..27 "3"
                        WHITESPACE@27..28 " "
                        R_BRACE@28..29 "}"
                  R_BRACKET@29..30 "]"
                WHITESPACE@30..31 " "
                EQ@31..32 "="
                WHITESPACE@32..33 " "
                PATH_EXPR@33..34
                  NAME_REF@33..34
                    IDENT@33..34 "y"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn array_type_missing_semicolon() {
    check(
        "static x: [usize] = y;",
        expect![[r#"
            SOURCE_FILE@0..22
              STATIC_ITEM@0..22
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..17
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  R_BRACKET@16..17 "]"
                WHITESPACE@17..18 " "
                EQ@18..19 "="
                WHITESPACE@19..20 " "
                PATH_EXPR@20..21
                  NAME_REF@20..21
                    IDENT@20..21 "y"
                SEMICOLON@21..22 ";"
            error 16..17: expected `;` (array types are written `[T; N]`)
        "#]],
    );
}

#[test]
fn array_literal_and_repeat() {
    check(
        "static f = fn { let a = [1, 2, 3]; let b = [0; 4]; let c = []; };",
        expect![[r#"
            SOURCE_FILE@0..65
              STATIC_ITEM@0..65
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..64
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..64
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..34
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      ARRAY_EXPR@24..33
                        L_BRACKET@24..25 "["
                        LITERAL@25..26
                          INT_NUMBER@25..26 "1"
                        COMMA@26..27 ","
                        WHITESPACE@27..28 " "
                        LITERAL@28..29
                          INT_NUMBER@28..29 "2"
                        COMMA@29..30 ","
                        WHITESPACE@30..31 " "
                        LITERAL@31..32
                          INT_NUMBER@31..32 "3"
                        R_BRACKET@32..33 "]"
                      SEMICOLON@33..34 ";"
                    WHITESPACE@34..35 " "
                    LET_STMT@35..50
                      LET_KW@35..38 "let"
                      WHITESPACE@38..39 " "
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "b"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      ARRAY_EXPR@43..49
                        L_BRACKET@43..44 "["
                        LITERAL@44..45
                          INT_NUMBER@44..45 "0"
                        SEMICOLON@45..46 ";"
                        WHITESPACE@46..47 " "
                        LITERAL@47..48
                          INT_NUMBER@47..48 "4"
                        R_BRACKET@48..49 "]"
                      SEMICOLON@49..50 ";"
                    WHITESPACE@50..51 " "
                    LET_STMT@51..62
                      LET_KW@51..54 "let"
                      WHITESPACE@54..55 " "
                      BIND_PAT@55..56
                        NAME@55..56
                          IDENT@55..56 "c"
                      WHITESPACE@56..57 " "
                      EQ@57..58 "="
                      WHITESPACE@58..59 " "
                      ARRAY_EXPR@59..61
                        L_BRACKET@59..60 "["
                        R_BRACKET@60..61 "]"
                      SEMICOLON@61..62 ";"
                    WHITESPACE@62..63 " "
                    R_BRACE@63..64 "}"
                SEMICOLON@64..65 ";"
        "#]],
    );
}

#[test]
fn array_literal_trailing_comma() {
    check(
        "static a = [1, 2,];",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "a"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                ARRAY_EXPR@11..18
                  L_BRACKET@11..12 "["
                  LITERAL@12..13
                    INT_NUMBER@12..13 "1"
                  COMMA@13..14 ","
                  WHITESPACE@14..15 " "
                  LITERAL@15..16
                    INT_NUMBER@15..16 "2"
                  COMMA@16..17 ","
                  R_BRACKET@17..18 "]"
                SEMICOLON@18..19 ";"
        "#]],
    );
}

#[test]
fn index_chains() {
    check(
        "static f = fn { m[0][1] + p.buf[i].x };",
        expect![[r#"
            SOURCE_FILE@0..39
              STATIC_ITEM@0..39
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..38
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..38
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    BIN_EXPR@16..36
                      INDEX_EXPR@16..23
                        INDEX_EXPR@16..20
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "m"
                          L_BRACKET@17..18 "["
                          LITERAL@18..19
                            INT_NUMBER@18..19 "0"
                          R_BRACKET@19..20 "]"
                        L_BRACKET@20..21 "["
                        LITERAL@21..22
                          INT_NUMBER@21..22 "1"
                        R_BRACKET@22..23 "]"
                      WHITESPACE@23..24 " "
                      PLUS@24..25 "+"
                      WHITESPACE@25..26 " "
                      FIELD_EXPR@26..36
                        INDEX_EXPR@26..34
                          FIELD_EXPR@26..31
                            PATH_EXPR@26..27
                              NAME_REF@26..27
                                IDENT@26..27 "p"
                            DOT@27..28 "."
                            NAME_REF@28..31
                              IDENT@28..31 "buf"
                          L_BRACKET@31..32 "["
                          PATH_EXPR@32..33
                            NAME_REF@32..33
                              IDENT@32..33 "i"
                          R_BRACKET@33..34 "]"
                        DOT@34..35 "."
                        NAME_REF@35..36
                          IDENT@35..36 "x"
                    WHITESPACE@36..37 " "
                    R_BRACE@37..38 "}"
                SEMICOLON@38..39 ";"
        "#]],
    );
}

#[test]
fn index_assign_statement() {
    check(
        "static f = fn { a[0] = 5; m[0][1] = 2; };",
        expect![[r#"
            SOURCE_FILE@0..41
              STATIC_ITEM@0..41
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..40
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..40
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..25
                      INDEX_EXPR@16..20
                        PATH_EXPR@16..17
                          NAME_REF@16..17
                            IDENT@16..17 "a"
                        L_BRACKET@17..18 "["
                        LITERAL@18..19
                          INT_NUMBER@18..19 "0"
                        R_BRACKET@19..20 "]"
                      WHITESPACE@20..21 " "
                      EQ@21..22 "="
                      WHITESPACE@22..23 " "
                      LITERAL@23..24
                        INT_NUMBER@23..24 "5"
                      SEMICOLON@24..25 ";"
                    WHITESPACE@25..26 " "
                    ASSIGN_STMT@26..38
                      INDEX_EXPR@26..33
                        INDEX_EXPR@26..30
                          PATH_EXPR@26..27
                            NAME_REF@26..27
                              IDENT@26..27 "m"
                          L_BRACKET@27..28 "["
                          LITERAL@28..29
                            INT_NUMBER@28..29 "0"
                          R_BRACKET@29..30 "]"
                        L_BRACKET@30..31 "["
                        LITERAL@31..32
                          INT_NUMBER@31..32 "1"
                        R_BRACKET@32..33 "]"
                      WHITESPACE@33..34 " "
                      EQ@34..35 "="
                      WHITESPACE@35..36 " "
                      LITERAL@36..37
                        INT_NUMBER@36..37 "2"
                      SEMICOLON@37..38 ";"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}
