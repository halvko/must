//! `const` items, `const fn` literals and `const` blocks.

use super::*;

#[test]
fn const_fn_literal_with_params_and_ret_type() {
    check(
        "static f = const fn (n: usize) -> usize { n };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..45
                  CONST_KW@11..16 "const"
                  WHITESPACE@16..17 " "
                  FN_KW@17..19 "fn"
                  WHITESPACE@19..20 " "
                  PARAM_LIST@20..30
                    L_PAREN@20..21 "("
                    PARAM@21..29
                      BIND_PAT@21..22
                        NAME@21..22
                          IDENT@21..22 "n"
                      COLON@22..23 ":"
                      WHITESPACE@23..24 " "
                      PATH_TYPE@24..29
                        NAME_REF@24..29
                          IDENT@24..29 "usize"
                    R_PAREN@29..30 ")"
                  WHITESPACE@30..31 " "
                  RET_TYPE@31..39
                    THIN_ARROW@31..33 "->"
                    WHITESPACE@33..34 " "
                    PATH_TYPE@34..39
                      NAME_REF@34..39
                        IDENT@34..39 "usize"
                  WHITESPACE@39..40 " "
                  BLOCK_EXPR@40..45
                    L_BRACE@40..41 "{"
                    WHITESPACE@41..42 " "
                    PATH_EXPR@42..43
                      NAME_REF@42..43
                        IDENT@42..43 "n"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn const_item_with_bare_const_fn() {
    check(
        "const g = const fn { 1 };",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                ITEM_HEAD@0..5
                  CONST_KW@0..5 "const"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "g"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                FN_LITERAL@10..24
                  CONST_KW@10..15 "const"
                  WHITESPACE@15..16 " "
                  FN_KW@16..18 "fn"
                  WHITESPACE@18..19 " "
                  BLOCK_EXPR@19..24
                    L_BRACE@19..20 "{"
                    WHITESPACE@20..21 " "
                    LITERAL@21..22
                      INT_NUMBER@21..22 "1"
                    WHITESPACE@22..23 " "
                    R_BRACE@23..24 "}"
                SEMICOLON@24..25 ";"
        "#]],
    );
}

#[test]
fn const_block_as_initializer() {
    check(
        "static x = const { 1 + 2 };",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CONST_BLOCK_EXPR@11..26
                  CONST_KW@11..16 "const"
                  WHITESPACE@16..17 " "
                  BLOCK_EXPR@17..26
                    L_BRACE@17..18 "{"
                    WHITESPACE@18..19 " "
                    BIN_EXPR@19..24
                      LITERAL@19..20
                        INT_NUMBER@19..20 "1"
                      WHITESPACE@20..21 " "
                      PLUS@21..22 "+"
                      WHITESPACE@22..23 " "
                      LITERAL@23..24
                        INT_NUMBER@23..24 "2"
                    WHITESPACE@24..25 " "
                    R_BRACE@25..26 "}"
                SEMICOLON@26..27 ";"
        "#]],
    );
}

#[test]
fn const_block_nested_in_fn_body() {
    check(
        "static f = fn { let y = const { 2 }; y };",
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
                    LET_STMT@16..36
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "y"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      CONST_BLOCK_EXPR@24..35
                        CONST_KW@24..29 "const"
                        WHITESPACE@29..30 " "
                        BLOCK_EXPR@30..35
                          L_BRACE@30..31 "{"
                          WHITESPACE@31..32 " "
                          LITERAL@32..33
                            INT_NUMBER@32..33 "2"
                          WHITESPACE@33..34 " "
                          R_BRACE@34..35 "}"
                      SEMICOLON@35..36 ";"
                    WHITESPACE@36..37 " "
                    PATH_EXPR@37..38
                      NAME_REF@37..38
                        IDENT@37..38 "y"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn const_fn_as_expression_inside_block() {
    check(
        "static f = fn { let g = const fn { 1 }; g() };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..45
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..45
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..39
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "g"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      FN_LITERAL@24..38
                        CONST_KW@24..29 "const"
                        WHITESPACE@29..30 " "
                        FN_KW@30..32 "fn"
                        WHITESPACE@32..33 " "
                        BLOCK_EXPR@33..38
                          L_BRACE@33..34 "{"
                          WHITESPACE@34..35 " "
                          LITERAL@35..36
                            INT_NUMBER@35..36 "1"
                          WHITESPACE@36..37 " "
                          R_BRACE@37..38 "}"
                      SEMICOLON@38..39 ";"
                    WHITESPACE@39..40 " "
                    CALL_EXPR@40..43
                      PATH_EXPR@40..41
                        NAME_REF@40..41
                          IDENT@40..41 "g"
                      ARG_LIST@41..43
                        L_PAREN@41..42 "("
                        R_PAREN@42..43 ")"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn const_item_inside_block_still_recovers() {
    check(
        "static f = fn { const x = 5; };",
        expect![[r#"
            SOURCE_FILE@0..31
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
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..28
                ITEM_HEAD@16..21
                  CONST_KW@16..21 "const"
                WHITESPACE@21..22 " "
                NAME@22..23
                  IDENT@22..23 "x"
                WHITESPACE@23..24 " "
                EQ@24..25 "="
                WHITESPACE@25..26 " "
                LITERAL@26..27
                  INT_NUMBER@26..27 "5"
                SEMICOLON@27..28 ";"
              WHITESPACE@28..29 " "
              ERROR@29..30
                R_BRACE@29..30 "}"
              ERROR@30..31
                SEMICOLON@30..31 ";"
            error 14..15: expected `}`
            error 29..30: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 30..31: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}

#[test]
fn dangling_const_at_block_end_recovers() {
    check(
        "static f = fn { const };",
        expect![[r#"
            SOURCE_FILE@0..24
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
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..21
                ITEM_HEAD@16..21
                  CONST_KW@16..21 "const"
              WHITESPACE@21..22 " "
              ERROR@22..23
                R_BRACE@22..23 "}"
              ERROR@23..24
                SEMICOLON@23..24 ";"
            error 14..15: expected `}`
            error 22..23: expected a name for the item
            error 23..24: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}
