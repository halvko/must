//! Error recovery: missing semicolons and braces, junk between items.

use super::*;

// ---- error resilience ----

#[test]
fn item_semicolon_brace_rule() {
    // `;` required unless the value ends in `}`.
    check(
        r#"
static x = 1
static main = fn { }
static y = 2;
"#,
        expect![[r#"
            SOURCE_FILE@0..49
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..13
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "x"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                LITERAL@12..13
                  INT_NUMBER@12..13 "1"
              WHITESPACE@13..14 "\n"
              STATIC_ITEM@14..34
                ITEM_HEAD@14..20
                  STATIC_KW@14..20 "static"
                WHITESPACE@20..21 " "
                NAME@21..25
                  IDENT@21..25 "main"
                WHITESPACE@25..26 " "
                EQ@26..27 "="
                WHITESPACE@27..28 " "
                FN_LITERAL@28..34
                  FN_KW@28..30 "fn"
                  WHITESPACE@30..31 " "
                  BLOCK_EXPR@31..34
                    L_BRACE@31..32 "{"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
              WHITESPACE@34..35 "\n"
              STATIC_ITEM@35..48
                ITEM_HEAD@35..41
                  STATIC_KW@35..41 "static"
                WHITESPACE@41..42 " "
                NAME@42..43
                  IDENT@42..43 "y"
                WHITESPACE@43..44 " "
                EQ@44..45 "="
                WHITESPACE@45..46 " "
                LITERAL@46..47
                  INT_NUMBER@46..47 "2"
                SEMICOLON@47..48 ";"
              WHITESPACE@48..49 "\n"
            error 12..13: expected `;`
        "#]],
    );
}

#[test]
fn bare_static_reports_one_error() {
    // "expected a name" and "expected `=`" both land at the same spot;
    // only the first (most fundamental) survives.
    check(
        "static ",
        expect![[r#"
            SOURCE_FILE@0..7
              STATIC_ITEM@0..6
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
              WHITESPACE@6..7 " "
            error 7..7: expected a name for the item
        "#]],
    );
}

#[test]
fn missing_semicolon_reported_after_previous_token() {
    check(
        r#"
static name = fn {
    let f = fn { 42 }
    f()
}
"#,
        expect![[r#"
            SOURCE_FILE@0..52
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..51
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "name"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..51
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..51
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..41
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "f"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      FN_LITERAL@32..41
                        FN_KW@32..34 "fn"
                        WHITESPACE@34..35 " "
                        BLOCK_EXPR@35..41
                          L_BRACE@35..36 "{"
                          WHITESPACE@36..37 " "
                          LITERAL@37..39
                            INT_NUMBER@37..39 "42"
                          WHITESPACE@39..40 " "
                          R_BRACE@40..41 "}"
                    WHITESPACE@41..46 "\n    "
                    CALL_EXPR@46..49
                      PATH_EXPR@46..47
                        NAME_REF@46..47
                          IDENT@46..47 "f"
                      ARG_LIST@47..49
                        L_PAREN@47..48 "("
                        R_PAREN@48..49 ")"
                    WHITESPACE@49..50 "\n"
                    R_BRACE@50..51 "}"
              WHITESPACE@51..52 "\n"
            error 40..41: expected `;`
        "#]],
    );
}

#[test]
fn unclosed_block_reports_brace_not_semicolon() {
    // The "expected `}`" anchors on the `{`; the item-level missing-`;`
    // lands on the same already-errored token and is suppressed.
    check(
        r#"
static name = fn {
"#,
        expect![[r#"
            SOURCE_FILE@0..20
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..19
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "name"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..19
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..19
                    L_BRACE@18..19 "{"
              WHITESPACE@19..20 "\n"
            error 18..19: expected `}`
        "#]],
    );
}

#[test]
fn item_missing_name_and_body() {
    check(
        "static = fn {",
        expect![[r#"
            SOURCE_FILE@0..13
              STATIC_ITEM@0..13
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                EQ@7..8 "="
                WHITESPACE@8..9 " "
                FN_LITERAL@9..13
                  FN_KW@9..11 "fn"
                  WHITESPACE@11..12 " "
                  BLOCK_EXPR@12..13
                    L_BRACE@12..13 "{"
            error 7..8: expected a name for the item
            error 12..13: expected `}`
        "#]],
    );
}

#[test]
fn junk_between_items() {
    check(
        r#"
static a = fn {};
@ % what
static b = fn {};
"#,
        expect![[r#"
            SOURCE_FILE@0..46
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..18
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "a"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..17
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  BLOCK_EXPR@15..17
                    L_BRACE@15..16 "{"
                    R_BRACE@16..17 "}"
                SEMICOLON@17..18 ";"
              WHITESPACE@18..19 "\n"
              ERROR@19..20
                ERROR_TOKEN@19..20 "@"
              WHITESPACE@20..21 " "
              ERROR@21..22
                ERROR_TOKEN@21..22 "%"
              WHITESPACE@22..23 " "
              ERROR@23..27
                IDENT@23..27 "what"
              WHITESPACE@27..28 "\n"
              STATIC_ITEM@28..45
                ITEM_HEAD@28..34
                  STATIC_KW@28..34 "static"
                WHITESPACE@34..35 " "
                NAME@35..36
                  IDENT@35..36 "b"
                WHITESPACE@36..37 " "
                EQ@37..38 "="
                WHITESPACE@38..39 " "
                FN_LITERAL@39..44
                  FN_KW@39..41 "fn"
                  WHITESPACE@41..42 " "
                  BLOCK_EXPR@42..44
                    L_BRACE@42..43 "{"
                    R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
              WHITESPACE@45..46 "\n"
            error 19..20: expected a region name after `@` (`@a`, or `@_` to infer one)
            error 21..22: unexpected character `%`
            error 23..27: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}

#[test]
fn let_recovers_to_next_stmt() {
    check(
        r#"
static main = fn {
    let = "hello";
    let s2 = ;
    print(s2);
}
"#,
        expect![[r#"
            SOURCE_FILE@0..71
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..70
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..70
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..70
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..38
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      EQ@28..29 "="
                      WHITESPACE@29..30 " "
                      LITERAL@30..37
                        STRING@30..37 "\"hello\""
                      SEMICOLON@37..38 ";"
                    WHITESPACE@38..43 "\n    "
                    LET_STMT@43..53
                      LET_KW@43..46 "let"
                      WHITESPACE@46..47 " "
                      BIND_PAT@47..49
                        NAME@47..49
                          IDENT@47..49 "s2"
                      WHITESPACE@49..50 " "
                      EQ@50..51 "="
                      WHITESPACE@51..52 " "
                      SEMICOLON@52..53 ";"
                    WHITESPACE@53..58 "\n    "
                    EXPR_STMT@58..68
                      CALL_EXPR@58..67
                        PATH_EXPR@58..63
                          NAME_REF@58..63
                            IDENT@58..63 "print"
                        ARG_LIST@63..67
                          L_PAREN@63..64 "("
                          PATH_EXPR@64..66
                            NAME_REF@64..66
                              IDENT@64..66 "s2"
                          R_PAREN@66..67 ")"
                      SEMICOLON@67..68 ";"
                    WHITESPACE@68..69 "\n"
                    R_BRACE@69..70 "}"
              WHITESPACE@70..71 "\n"
            error 28..29: expected a binding name
            error 52..53: expected an expression
        "#]],
    );
}

#[test]
fn item_keyword_inside_block_recovers() {
    check(
        r#"
static a = fn {
    let s = "x";
static b = fn {};
"#,
        expect![[r#"
            SOURCE_FILE@0..52
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..33
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "a"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..33
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  BLOCK_EXPR@15..33
                    L_BRACE@15..16 "{"
                    WHITESPACE@16..21 "\n    "
                    LET_STMT@21..33
                      LET_KW@21..24 "let"
                      WHITESPACE@24..25 " "
                      BIND_PAT@25..26
                        NAME@25..26
                          IDENT@25..26 "s"
                      WHITESPACE@26..27 " "
                      EQ@27..28 "="
                      WHITESPACE@28..29 " "
                      LITERAL@29..32
                        STRING@29..32 "\"x\""
                      SEMICOLON@32..33 ";"
              WHITESPACE@33..34 "\n"
              STATIC_ITEM@34..51
                ITEM_HEAD@34..40
                  STATIC_KW@34..40 "static"
                WHITESPACE@40..41 " "
                NAME@41..42
                  IDENT@41..42 "b"
                WHITESPACE@42..43 " "
                EQ@43..44 "="
                WHITESPACE@44..45 " "
                FN_LITERAL@45..50
                  FN_KW@45..47 "fn"
                  WHITESPACE@47..48 " "
                  BLOCK_EXPR@48..50
                    L_BRACE@48..49 "{"
                    R_BRACE@49..50 "}"
                SEMICOLON@50..51 ";"
              WHITESPACE@51..52 "\n"
            error 32..33: expected `}`
        "#]],
    );
}

#[test]
fn missing_semicolon_between_stmts() {
    check(
        r#"
static main = fn {
    let a = 1
    print(a);
}
"#,
        expect![[r#"
            SOURCE_FILE@0..50
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..49
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..49
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..49
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..33
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "a"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..33
                        INT_NUMBER@32..33 "1"
                    WHITESPACE@33..38 "\n    "
                    EXPR_STMT@38..47
                      CALL_EXPR@38..46
                        PATH_EXPR@38..43
                          NAME_REF@38..43
                            IDENT@38..43 "print"
                        ARG_LIST@43..46
                          L_PAREN@43..44 "("
                          PATH_EXPR@44..45
                            NAME_REF@44..45
                              IDENT@44..45 "a"
                          R_PAREN@45..46 ")"
                      SEMICOLON@46..47 ";"
                    WHITESPACE@47..48 "\n"
                    R_BRACE@48..49 "}"
              WHITESPACE@49..50 "\n"
            error 32..33: expected `;`
        "#]],
    );
}

#[test]
fn fn_requires_block_body() {
    // Bodies are always blocks; `(` after `fn` is always a parameter list.
    check(
        r#"
static takes_param = fn (s) { s }
static shorthand_is_error = fn 42;
static empty_params = fn () { 1 }
"#,
        expect![[r#"
            SOURCE_FILE@0..104
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..34
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..19
                  IDENT@8..19 "takes_param"
                WHITESPACE@19..20 " "
                EQ@20..21 "="
                WHITESPACE@21..22 " "
                FN_LITERAL@22..34
                  FN_KW@22..24 "fn"
                  WHITESPACE@24..25 " "
                  PARAM_LIST@25..28
                    L_PAREN@25..26 "("
                    PARAM@26..27
                      BIND_PAT@26..27
                        NAME@26..27
                          IDENT@26..27 "s"
                    R_PAREN@27..28 ")"
                  WHITESPACE@28..29 " "
                  BLOCK_EXPR@29..34
                    L_BRACE@29..30 "{"
                    WHITESPACE@30..31 " "
                    PATH_EXPR@31..32
                      NAME_REF@31..32
                        IDENT@31..32 "s"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
              WHITESPACE@34..35 "\n"
              STATIC_ITEM@35..69
                ITEM_HEAD@35..41
                  STATIC_KW@35..41 "static"
                WHITESPACE@41..42 " "
                NAME@42..60
                  IDENT@42..60 "shorthand_is_error"
                WHITESPACE@60..61 " "
                EQ@61..62 "="
                WHITESPACE@62..63 " "
                FN_LITERAL@63..68
                  FN_KW@63..65 "fn"
                  WHITESPACE@65..66 " "
                  LITERAL@66..68
                    INT_NUMBER@66..68 "42"
                SEMICOLON@68..69 ";"
              WHITESPACE@69..70 "\n"
              STATIC_ITEM@70..103
                ITEM_HEAD@70..76
                  STATIC_KW@70..76 "static"
                WHITESPACE@76..77 " "
                NAME@77..89
                  IDENT@77..89 "empty_params"
                WHITESPACE@89..90 " "
                EQ@90..91 "="
                WHITESPACE@91..92 " "
                FN_LITERAL@92..103
                  FN_KW@92..94 "fn"
                  WHITESPACE@94..95 " "
                  PARAM_LIST@95..97
                    L_PAREN@95..96 "("
                    R_PAREN@96..97 ")"
                  WHITESPACE@97..98 " "
                  BLOCK_EXPR@98..103
                    L_BRACE@98..99 "{"
                    WHITESPACE@99..100 " "
                    LITERAL@100..101
                      INT_NUMBER@100..101 "1"
                    WHITESPACE@101..102 " "
                    R_BRACE@102..103 "}"
              WHITESPACE@103..104 "\n"
            error 66..68: function bodies are blocks; wrap this expression in `{ }`
        "#]],
    );
}
#[test]
fn fn_body_recovery_semicolon() {
    check(
        "static f = fn;",
        expect![[r#"
            SOURCE_FILE@0..14
              STATIC_ITEM@0..14
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..13
                  FN_KW@11..13 "fn"
                SEMICOLON@13..14 ";"
            error 13..14: expected `{`: function bodies are blocks
        "#]],
    );
}
