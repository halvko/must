//! `mut`, assignment targets and `let` without an initializer.

use super::*;

#[test]
fn let_mut() {
    check(
        "static f = fn { let mut x = 1; };",
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
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
        "#]],
    );
}

#[test]
fn mut_param() {
    check(
        "static f = fn (mut n: usize) { };",
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
                  PARAM_LIST@14..28
                    L_PAREN@14..15 "("
                    PARAM@15..27
                      MUT_KW@15..18 "mut"
                      WHITESPACE@18..19 " "
                      BIND_PAT@19..20
                        NAME@19..20
                          IDENT@19..20 "n"
                      COLON@20..21 ":"
                      WHITESPACE@21..22 " "
                      PATH_TYPE@22..27
                        NAME_REF@22..27
                          IDENT@22..27 "usize"
                    R_PAREN@27..28 ")"
                  WHITESPACE@28..29 " "
                  BLOCK_EXPR@29..32
                    L_BRACE@29..30 "{"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
        "#]],
    );
}

#[test]
fn simple_assignment() {
    check(
        "static f = fn { let mut x = 1; x = 2; };",
        expect![[r#"
            SOURCE_FILE@0..40
              STATIC_ITEM@0..40
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..39
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..39
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    ASSIGN_STMT@31..37
                      PATH_EXPR@31..32
                        NAME_REF@31..32
                          IDENT@31..32 "x"
                      WHITESPACE@32..33 " "
                      EQ@33..34 "="
                      WHITESPACE@34..35 " "
                      LITERAL@35..36
                        INT_NUMBER@35..36 "2"
                      SEMICOLON@36..37 ";"
                    WHITESPACE@37..38 " "
                    R_BRACE@38..39 "}"
                SEMICOLON@39..40 ";"
        "#]],
    );
}

#[test]
fn assignment_with_complex_rhs() {
    check(
        "static f = fn { let mut x = 1; x = if x == 1 { 2 } else { 3 }; };",
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
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    ASSIGN_STMT@31..62
                      PATH_EXPR@31..32
                        NAME_REF@31..32
                          IDENT@31..32 "x"
                      WHITESPACE@32..33 " "
                      EQ@33..34 "="
                      WHITESPACE@34..35 " "
                      IF_EXPR@35..61
                        IF_KW@35..37 "if"
                        WHITESPACE@37..38 " "
                        BIN_EXPR@38..44
                          PATH_EXPR@38..39
                            NAME_REF@38..39
                              IDENT@38..39 "x"
                          WHITESPACE@39..40 " "
                          EQ2@40..42 "=="
                          WHITESPACE@42..43 " "
                          LITERAL@43..44
                            INT_NUMBER@43..44 "1"
                        WHITESPACE@44..45 " "
                        BLOCK_EXPR@45..50
                          L_BRACE@45..46 "{"
                          WHITESPACE@46..47 " "
                          LITERAL@47..48
                            INT_NUMBER@47..48 "2"
                          WHITESPACE@48..49 " "
                          R_BRACE@49..50 "}"
                        WHITESPACE@50..51 " "
                        ELSE_KW@51..55 "else"
                        WHITESPACE@55..56 " "
                        BLOCK_EXPR@56..61
                          L_BRACE@56..57 "{"
                          WHITESPACE@57..58 " "
                          LITERAL@58..59
                            INT_NUMBER@58..59 "3"
                          WHITESPACE@59..60 " "
                          R_BRACE@60..61 "}"
                      SEMICOLON@61..62 ";"
                    WHITESPACE@62..63 " "
                    R_BRACE@63..64 "}"
                SEMICOLON@64..65 ";"
        "#]],
    );
}

#[test]
fn assignment_to_non_name_is_rejected() {
    check(
        "static f = fn { let mut x = 1; x + 1 = 2; };",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..43
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..43
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    ASSIGN_STMT@31..41
                      BIN_EXPR@31..36
                        PATH_EXPR@31..32
                          NAME_REF@31..32
                            IDENT@31..32 "x"
                        WHITESPACE@32..33 " "
                        PLUS@33..34 "+"
                        WHITESPACE@34..35 " "
                        LITERAL@35..36
                          INT_NUMBER@35..36 "1"
                      WHITESPACE@36..37 " "
                      EQ@37..38 "="
                      WHITESPACE@38..39 " "
                      LITERAL@39..40
                        INT_NUMBER@39..40 "2"
                      SEMICOLON@40..41 ";"
                    WHITESPACE@41..42 " "
                    R_BRACE@42..43 "}"
                SEMICOLON@43..44 ";"
            error 31..36: can only assign to a variable or its fields
        "#]],
    );
}

#[test]
fn chained_assignment_is_rejected() {
    check(
        "static f = fn { let mut x = 1; let mut y = 1; let mut z = 1; x = y = z; };",
        expect![[r#"
            SOURCE_FILE@0..74
              STATIC_ITEM@0..74
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..73
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..73
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    LET_STMT@31..45
                      LET_KW@31..34 "let"
                      WHITESPACE@34..35 " "
                      MUT_KW@35..38 "mut"
                      WHITESPACE@38..39 " "
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "y"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      LITERAL@43..44
                        INT_NUMBER@43..44 "1"
                      SEMICOLON@44..45 ";"
                    WHITESPACE@45..46 " "
                    LET_STMT@46..60
                      LET_KW@46..49 "let"
                      WHITESPACE@49..50 " "
                      MUT_KW@50..53 "mut"
                      WHITESPACE@53..54 " "
                      BIND_PAT@54..55
                        NAME@54..55
                          IDENT@54..55 "z"
                      WHITESPACE@55..56 " "
                      EQ@56..57 "="
                      WHITESPACE@57..58 " "
                      LITERAL@58..59
                        INT_NUMBER@58..59 "1"
                      SEMICOLON@59..60 ";"
                    WHITESPACE@60..61 " "
                    ASSIGN_STMT@61..66
                      PATH_EXPR@61..62
                        NAME_REF@61..62
                          IDENT@61..62 "x"
                      WHITESPACE@62..63 " "
                      EQ@63..64 "="
                      WHITESPACE@64..65 " "
                      PATH_EXPR@65..66
                        NAME_REF@65..66
                          IDENT@65..66 "y"
                    WHITESPACE@66..67 " "
                    ERROR@67..68
                      EQ@67..68 "="
                    WHITESPACE@68..69 " "
                    EXPR_STMT@69..71
                      PATH_EXPR@69..70
                        NAME_REF@69..70
                          IDENT@69..70 "z"
                      SEMICOLON@70..71 ";"
                    WHITESPACE@71..72 " "
                    R_BRACE@72..73 "}"
                SEMICOLON@73..74 ";"
            error 65..66: expected `;`
            error 67..68: expected an expression
        "#]],
    );
}

#[test]
fn let_mut_hole_pattern_is_rejected() {
    check(
        "static f = fn { let mut _ = 1; };",
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
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      WILDCARD_PAT@24..25
                        HOLE@24..25 "_"
                      WHITESPACE@25..26 " "
                      EQ@26..27 "="
                      WHITESPACE@27..28 " "
                      LITERAL@28..29
                        INT_NUMBER@28..29 "1"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
            error 20..25: `mut` has no effect on `_`: a hole can never be assigned
        "#]],
    );
}

#[test]
fn mut_hole_param_is_rejected() {
    check(
        "static f = fn (mut _: usize) { };",
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
                  PARAM_LIST@14..28
                    L_PAREN@14..15 "("
                    PARAM@15..27
                      MUT_KW@15..18 "mut"
                      WHITESPACE@18..19 " "
                      WILDCARD_PAT@19..20
                        HOLE@19..20 "_"
                      COLON@20..21 ":"
                      WHITESPACE@21..22 " "
                      PATH_TYPE@22..27
                        NAME_REF@22..27
                          IDENT@22..27 "usize"
                    R_PAREN@27..28 ")"
                  WHITESPACE@28..29 " "
                  BLOCK_EXPR@29..32
                    L_BRACE@29..30 "{"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
            error 15..20: `mut` has no effect on `_`: a hole can never be assigned
        "#]],
    );
}

#[test]
fn a_let_without_an_initializer_parses() {
    check(
        "static f = fn { let mut x: usize; let mut y; };",
        expect![[r#"
            SOURCE_FILE@0..47
              STATIC_ITEM@0..47
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..46
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..46
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..33
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      PATH_TYPE@27..32
                        NAME_REF@27..32
                          IDENT@27..32 "usize"
                      SEMICOLON@32..33 ";"
                    WHITESPACE@33..34 " "
                    LET_STMT@34..44
                      LET_KW@34..37 "let"
                      WHITESPACE@37..38 " "
                      MUT_KW@38..41 "mut"
                      WHITESPACE@41..42 " "
                      BIND_PAT@42..43
                        NAME@42..43
                          IDENT@42..43 "y"
                      SEMICOLON@43..44 ";"
                    WHITESPACE@44..45 " "
                    R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
        "#]],
    );
}

#[test]
fn an_immutable_let_without_a_value_is_reserved() {
    check_errors(
        "static f = fn { let y; };",
        expect![[r#"
            16..22: an immutable `let` without a value is not supported yet; write `let mut y;`
        "#]],
    );
}

#[test]
fn equality_comparison_is_still_an_expr_stmt() {
    // Regression guard: `==` must not be mistaken for the assignment `=`.
    check(
        "static f = fn { let x = 1; x == 2; };",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..36
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..36
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..26
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "x"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      LITERAL@24..25
                        INT_NUMBER@24..25 "1"
                      SEMICOLON@25..26 ";"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..34
                      BIN_EXPR@27..33
                        PATH_EXPR@27..28
                          NAME_REF@27..28
                            IDENT@27..28 "x"
                        WHITESPACE@28..29 " "
                        EQ2@29..31 "=="
                        WHITESPACE@31..32 " "
                        LITERAL@32..33
                          INT_NUMBER@32..33 "2"
                      SEMICOLON@33..34 ";"
                    WHITESPACE@34..35 " "
                    R_BRACE@35..36 "}"
                SEMICOLON@36..37 ";"
        "#]],
    );
}
