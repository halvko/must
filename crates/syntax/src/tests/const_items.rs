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
    // At the START of a statement, where `block_expr` must tell `const {`
    // (an expression) from a misplaced `const` item it bails on.
    check(
        "static f = fn { const { 2 }; 1 };",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..33
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..32
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..32
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..28
                      CONST_BLOCK_EXPR@16..27
                        CONST_KW@16..21 "const"
                        WHITESPACE@21..22 " "
                        BLOCK_EXPR@22..27
                          L_BRACE@22..23 "{"
                          WHITESPACE@23..24 " "
                          LITERAL@24..25
                            INT_NUMBER@24..25 "2"
                          WHITESPACE@25..26 " "
                          R_BRACE@26..27 "}"
                      SEMICOLON@27..28 ";"
                    WHITESPACE@28..29 " "
                    LITERAL@29..30
                      INT_NUMBER@29..30 "1"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
        "#]],
    );
}

#[test]
fn const_fn_as_expression_inside_block() {
    // Statement start again: `const fn` is a literal, not a misplaced item.
    check(
        "static f = fn { const fn { 1 }; 2 };",
        expect![[r#"
            SOURCE_FILE@0..36
              STATIC_ITEM@0..36
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..35
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..35
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..31
                      FN_LITERAL@16..30
                        CONST_KW@16..21 "const"
                        WHITESPACE@21..22 " "
                        FN_KW@22..24 "fn"
                        WHITESPACE@24..25 " "
                        BLOCK_EXPR@25..30
                          L_BRACE@25..26 "{"
                          WHITESPACE@26..27 " "
                          LITERAL@27..28
                            INT_NUMBER@27..28 "1"
                          WHITESPACE@28..29 " "
                          R_BRACE@29..30 "}"
                      SEMICOLON@30..31 ";"
                    WHITESPACE@31..32 " "
                    LITERAL@32..33
                      INT_NUMBER@32..33 "2"
                    WHITESPACE@33..34 " "
                    R_BRACE@34..35 "}"
                SEMICOLON@35..36 ";"
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
