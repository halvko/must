//! Record types, record literals and field access.

use super::*;

#[test]
fn record_type_annotation() {
    check(
        "static p: struct { x: usize, y: usize } = 0;",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                RECORD_TYPE@10..39
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_TYPE_FIELD@19..27
                    NAME@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  COMMA@27..28 ","
                  WHITESPACE@28..29 " "
                  RECORD_TYPE_FIELD@29..37
                    NAME@29..30
                      IDENT@29..30 "y"
                    COLON@30..31 ":"
                    WHITESPACE@31..32 " "
                    PATH_TYPE@32..37
                      NAME_REF@32..37
                        IDENT@32..37 "usize"
                  WHITESPACE@37..38 " "
                  R_BRACE@38..39 "}"
                WHITESPACE@39..40 " "
                EQ@40..41 "="
                WHITESPACE@41..42 " "
                LITERAL@42..43
                  INT_NUMBER@42..43 "0"
                SEMICOLON@43..44 ";"
        "#]],
    );
}

#[test]
fn record_expr_mixed_explicit_and_shorthand_trailing_comma() {
    check(
        "static p = struct { x = 1, y, };",
        expect![[r#"
            SOURCE_FILE@0..32
              STATIC_ITEM@0..32
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..31
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..25
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    WHITESPACE@21..22 " "
                    EQ@22..23 "="
                    WHITESPACE@23..24 " "
                    LITERAL@24..25
                      INT_NUMBER@24..25 "1"
                  COMMA@25..26 ","
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..28
                    NAME_REF@27..28
                      IDENT@27..28 "y"
                  COMMA@28..29 ","
                  WHITESPACE@29..30 " "
                  R_BRACE@30..31 "}"
                SEMICOLON@31..32 ";"
        "#]],
    );
}

#[test]
fn nested_record_literal() {
    check(
        "static p = struct { outer = struct { x = 1 } };",
        expect![[r#"
            SOURCE_FILE@0..47
              STATIC_ITEM@0..47
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..46
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..44
                    NAME_REF@20..25
                      IDENT@20..25 "outer"
                    WHITESPACE@25..26 " "
                    EQ@26..27 "="
                    WHITESPACE@27..28 " "
                    RECORD_EXPR@28..44
                      STRUCT_KW@28..34 "struct"
                      WHITESPACE@34..35 " "
                      L_BRACE@35..36 "{"
                      WHITESPACE@36..37 " "
                      RECORD_EXPR_FIELD@37..42
                        NAME_REF@37..38
                          IDENT@37..38 "x"
                        WHITESPACE@38..39 " "
                        EQ@39..40 "="
                        WHITESPACE@40..41 " "
                        LITERAL@41..42
                          INT_NUMBER@41..42 "1"
                      WHITESPACE@42..43 " "
                      R_BRACE@43..44 "}"
                  WHITESPACE@44..45 " "
                  R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
        "#]],
    );
}

#[test]
fn single_ident_brace_stays_block() {
    check(
        "static p = { x };",
        expect![[r#"
            SOURCE_FILE@0..17
              STATIC_ITEM@0..17
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..16
                  L_BRACE@11..12 "{"
                  WHITESPACE@12..13 " "
                  PATH_EXPR@13..14
                    NAME_REF@13..14
                      IDENT@13..14 "x"
                  WHITESPACE@14..15 " "
                  R_BRACE@15..16 "}"
                SEMICOLON@16..17 ";"
        "#]],
    );
}

#[test]
fn empty_brace_stays_block() {
    check(
        "static p = {};",
        expect![[r#"
            SOURCE_FILE@0..14
              STATIC_ITEM@0..14
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..13
                  L_BRACE@11..12 "{"
                  R_BRACE@12..13 "}"
                SEMICOLON@13..14 ";"
        "#]],
    );
}

// With records now requiring the `struct` keyword, there is no comma-triggered
// lookahead any more: `{ x, y }` is an ordinary block (with parse errors), not
// a record literal.
#[test]
fn two_idents_comma_separated_stays_block() {
    check(
        "static p = { x, y };",
        expect![[r#"
            SOURCE_FILE@0..20
              STATIC_ITEM@0..20
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..19
                  L_BRACE@11..12 "{"
                  WHITESPACE@12..13 " "
                  EXPR_STMT@13..14
                    PATH_EXPR@13..14
                      NAME_REF@13..14
                        IDENT@13..14 "x"
                  ERROR@14..15
                    COMMA@14..15 ","
                  WHITESPACE@15..16 " "
                  PATH_EXPR@16..17
                    NAME_REF@16..17
                      IDENT@16..17 "y"
                  WHITESPACE@17..18 " "
                  R_BRACE@18..19 "}"
                SEMICOLON@19..20 ";"
            error 13..14: expected `;`
            error 14..15: expected an expression
        "#]],
    );
}

// Likewise `{ x, }`: without the `struct` keyword there is no lookahead, so
// this is a block (with parse errors), not a single-shorthand-field record.
#[test]
fn single_ident_trailing_comma_stays_block() {
    check(
        "static p = { x, };",
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..17
                  L_BRACE@11..12 "{"
                  WHITESPACE@12..13 " "
                  EXPR_STMT@13..14
                    PATH_EXPR@13..14
                      NAME_REF@13..14
                        IDENT@13..14 "x"
                  ERROR@14..15
                    COMMA@14..15 ","
                  WHITESPACE@15..16 " "
                  R_BRACE@16..17 "}"
                SEMICOLON@17..18 ";"
            error 13..14: expected `;`
            error 14..15: expected an expression
        "#]],
    );
}

#[test]
fn field_access_chain() {
    check(
        "static p = a.x.y;",
        expect![[r#"
            SOURCE_FILE@0..17
              STATIC_ITEM@0..17
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FIELD_EXPR@11..16
                  FIELD_EXPR@11..14
                    PATH_EXPR@11..12
                      NAME_REF@11..12
                        IDENT@11..12 "a"
                    DOT@12..13 "."
                    NAME_REF@13..14
                      IDENT@13..14 "x"
                  DOT@14..15 "."
                  NAME_REF@15..16
                    IDENT@15..16 "y"
                SEMICOLON@16..17 ";"
        "#]],
    );
}

#[test]
fn field_access_on_call_result() {
    check(
        "static p = f().x;",
        expect![[r#"
            SOURCE_FILE@0..17
              STATIC_ITEM@0..17
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FIELD_EXPR@11..16
                  CALL_EXPR@11..14
                    PATH_EXPR@11..12
                      NAME_REF@11..12
                        IDENT@11..12 "f"
                    ARG_LIST@12..14
                      L_PAREN@12..13 "("
                      R_PAREN@13..14 ")"
                  DOT@14..15 "."
                  NAME_REF@15..16
                    IDENT@15..16 "x"
                SEMICOLON@16..17 ";"
        "#]],
    );
}

#[test]
fn record_literal_as_call_argument() {
    check(
        "static p = f(struct { x = 1 });",
        expect![[r#"
            SOURCE_FILE@0..31
              STATIC_ITEM@0..31
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..30
                  PATH_EXPR@11..12
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                  ARG_LIST@12..30
                    L_PAREN@12..13 "("
                    RECORD_EXPR@13..29
                      STRUCT_KW@13..19 "struct"
                      WHITESPACE@19..20 " "
                      L_BRACE@20..21 "{"
                      WHITESPACE@21..22 " "
                      RECORD_EXPR_FIELD@22..27
                        NAME_REF@22..23
                          IDENT@22..23 "x"
                        WHITESPACE@23..24 " "
                        EQ@24..25 "="
                        WHITESPACE@25..26 " "
                        LITERAL@26..27
                          INT_NUMBER@26..27 "1"
                      WHITESPACE@27..28 " "
                      R_BRACE@28..29 "}"
                    R_PAREN@29..30 ")"
                SEMICOLON@30..31 ";"
        "#]],
    );
}

#[test]
fn duplicate_field_in_record_type() {
    check(
        "static p: struct { x: usize, x: usize } = 0;",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                RECORD_TYPE@10..39
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_TYPE_FIELD@19..27
                    NAME@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  COMMA@27..28 ","
                  WHITESPACE@28..29 " "
                  RECORD_TYPE_FIELD@29..37
                    NAME@29..30
                      IDENT@29..30 "x"
                    COLON@30..31 ":"
                    WHITESPACE@31..32 " "
                    PATH_TYPE@32..37
                      NAME_REF@32..37
                        IDENT@32..37 "usize"
                  WHITESPACE@37..38 " "
                  R_BRACE@38..39 "}"
                WHITESPACE@39..40 " "
                EQ@40..41 "="
                WHITESPACE@41..42 " "
                LITERAL@42..43
                  INT_NUMBER@42..43 "0"
                SEMICOLON@43..44 ";"
            error 29..30: duplicate field `x`
        "#]],
    );
}

#[test]
fn duplicate_field_in_record_literal() {
    check(
        "static p = struct { x = 1, x = 2 };",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..34
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..25
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    WHITESPACE@21..22 " "
                    EQ@22..23 "="
                    WHITESPACE@23..24 " "
                    LITERAL@24..25
                      INT_NUMBER@24..25 "1"
                  COMMA@25..26 ","
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..32
                    NAME_REF@27..28
                      IDENT@27..28 "x"
                    WHITESPACE@28..29 " "
                    EQ@29..30 "="
                    WHITESPACE@30..31 " "
                    LITERAL@31..32
                      INT_NUMBER@31..32 "2"
                  WHITESPACE@32..33 " "
                  R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
            error 27..28: duplicate field `x`
        "#]],
    );
}

#[test]
fn open_record_type_is_rejected() {
    check(
        "static p: struct { x: usize, ... } = 0;",
        expect![[r#"
            SOURCE_FILE@0..39
              STATIC_ITEM@0..39
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                RECORD_TYPE@10..34
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_TYPE_FIELD@19..27
                    NAME@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  COMMA@27..28 ","
                  WHITESPACE@28..29 " "
                  DOT3@29..32 "..."
                  WHITESPACE@32..33 " "
                  R_BRACE@33..34 "}"
                WHITESPACE@34..35 " "
                EQ@35..36 "="
                WHITESPACE@36..37 " "
                LITERAL@37..38
                  INT_NUMBER@37..38 "0"
                SEMICOLON@38..39 ";"
            error 29..32: open record types are not supported yet
        "#]],
    );
}

#[test]
fn open_record_literal_is_rejected() {
    check(
        "static p = struct { x = 1, ... };",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..33
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..32
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..25
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    WHITESPACE@21..22 " "
                    EQ@22..23 "="
                    WHITESPACE@23..24 " "
                    LITERAL@24..25
                      INT_NUMBER@24..25 "1"
                  COMMA@25..26 ","
                  WHITESPACE@26..27 " "
                  DOT3@27..30 "..."
                  WHITESPACE@30..31 " "
                  R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
            error 27..30: open record types are not supported yet
        "#]],
    );
}

#[test]
fn semicolon_after_ident_stays_block() {
    check(
        "static p = { x; y };",
        expect![[r#"
            SOURCE_FILE@0..20
              STATIC_ITEM@0..20
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..19
                  L_BRACE@11..12 "{"
                  WHITESPACE@12..13 " "
                  EXPR_STMT@13..15
                    PATH_EXPR@13..14
                      NAME_REF@13..14
                        IDENT@13..14 "x"
                    SEMICOLON@14..15 ";"
                  WHITESPACE@15..16 " "
                  PATH_EXPR@16..17
                    NAME_REF@16..17
                      IDENT@16..17 "y"
                  WHITESPACE@17..18 " "
                  R_BRACE@18..19 "}"
                SEMICOLON@19..20 ";"
        "#]],
    );
}

#[test]
fn record_literal_where_block_required_in_if() {
    check(
        "static p = if c { x: 1 } {};",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..24
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                IF_EXPR@11..24
                  IF_KW@11..13 "if"
                  WHITESPACE@13..14 " "
                  PATH_EXPR@14..15
                    NAME_REF@14..15
                      IDENT@14..15 "c"
                  WHITESPACE@15..16 " "
                  BLOCK_EXPR@16..24
                    L_BRACE@16..17 "{"
                    WHITESPACE@17..18 " "
                    EXPR_STMT@18..19
                      PATH_EXPR@18..19
                        NAME_REF@18..19
                          IDENT@18..19 "x"
                    ERROR@19..20
                      COLON@19..20 ":"
                    WHITESPACE@20..21 " "
                    LITERAL@21..22
                      INT_NUMBER@21..22 "1"
                    WHITESPACE@22..23 " "
                    R_BRACE@23..24 "}"
              WHITESPACE@24..25 " "
              ERROR@25..26
                L_BRACE@25..26 "{"
              ERROR@26..27
                R_BRACE@26..27 "}"
              ERROR@27..28
                SEMICOLON@27..28 ";"
            error 18..19: expected `;`
            error 19..20: expected an expression
            error 25..26: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 26..27: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 27..28: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}

// Generalizes `record_literal_where_block_required_in_if` to every position:
// bare `{ x: 1 }` is no longer special-cased into a record anywhere, so in an
// ordinary expression position it too parses as a block with a stray-colon error.
#[test]
fn bare_brace_with_colon_is_block_with_error() {
    check(
        "static p = { x: 1 };",
        expect![[r#"
            SOURCE_FILE@0..20
              STATIC_ITEM@0..20
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BLOCK_EXPR@11..19
                  L_BRACE@11..12 "{"
                  WHITESPACE@12..13 " "
                  EXPR_STMT@13..14
                    PATH_EXPR@13..14
                      NAME_REF@13..14
                        IDENT@13..14 "x"
                  ERROR@14..15
                    COLON@14..15 ":"
                  WHITESPACE@15..16 " "
                  LITERAL@16..17
                    INT_NUMBER@16..17 "1"
                  WHITESPACE@17..18 " "
                  R_BRACE@18..19 "}"
                SEMICOLON@19..20 ";"
            error 13..14: expected `;`
            error 14..15: expected an expression
        "#]],
    );
}

// A `struct` not followed by `{` has no useful interpretation, so it is
// consumed by the "expected an expression" catch-all (wrapped in ERROR) and
// parsing continues cleanly — the `;` is still consumed by the item parser.
#[test]
fn struct_without_brace_errors_gracefully() {
    check(
        "static p = struct;",
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                ERROR@11..17
                  STRUCT_KW@11..17 "struct"
                SEMICOLON@17..18 ";"
            error 11..17: expected an expression
        "#]],
    );
}

// `struct { ... }` used as a statement parses as an EXPR_STMT wrapping a
// RECORD_EXPR, with no errors.
#[test]
fn struct_literal_as_statement() {
    check(
        "static f = fn { struct { x = 1 }; };",
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
                    EXPR_STMT@16..33
                      RECORD_EXPR@16..32
                        STRUCT_KW@16..22 "struct"
                        WHITESPACE@22..23 " "
                        L_BRACE@23..24 "{"
                        WHITESPACE@24..25 " "
                        RECORD_EXPR_FIELD@25..30
                          NAME_REF@25..26
                            IDENT@25..26 "x"
                          WHITESPACE@26..27 " "
                          EQ@27..28 "="
                          WHITESPACE@28..29 " "
                          LITERAL@29..30
                            INT_NUMBER@29..30 "1"
                        WHITESPACE@30..31 " "
                        R_BRACE@31..32 "}"
                      SEMICOLON@32..33 ";"
                    WHITESPACE@33..34 " "
                    R_BRACE@34..35 "}"
                SEMICOLON@35..36 ";"
        "#]],
    );
}

// The "Rust wart" case: because `struct` unambiguously owns exactly one
// `{...}` and stops, an `if` condition containing a struct literal parses
// fully and the following `{}` is unambiguously the (empty) branch — no parens
// needed, no parse errors.
#[test]
fn if_condition_with_struct_literal_before_branch() {
    check(
        "static c = if p == struct { x = 1 } {};",
        expect![[r#"
            SOURCE_FILE@0..39
              STATIC_ITEM@0..39
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "c"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                IF_EXPR@11..38
                  IF_KW@11..13 "if"
                  WHITESPACE@13..14 " "
                  BIN_EXPR@14..35
                    PATH_EXPR@14..15
                      NAME_REF@14..15
                        IDENT@14..15 "p"
                    WHITESPACE@15..16 " "
                    EQ2@16..18 "=="
                    WHITESPACE@18..19 " "
                    RECORD_EXPR@19..35
                      STRUCT_KW@19..25 "struct"
                      WHITESPACE@25..26 " "
                      L_BRACE@26..27 "{"
                      WHITESPACE@27..28 " "
                      RECORD_EXPR_FIELD@28..33
                        NAME_REF@28..29
                          IDENT@28..29 "x"
                        WHITESPACE@29..30 " "
                        EQ@30..31 "="
                        WHITESPACE@31..32 " "
                        LITERAL@32..33
                          INT_NUMBER@32..33 "1"
                      WHITESPACE@33..34 " "
                      R_BRACE@34..35 "}"
                  WHITESPACE@35..36 " "
                  BLOCK_EXPR@36..38
                    L_BRACE@36..37 "{"
                    R_BRACE@37..38 "}"
                SEMICOLON@38..39 ";"
        "#]],
    );
}

// ---- the record-literal equals-defines respell --------------------------

#[test]
fn record_field_equals_defines() {
    check(
        "static p = struct { x = 1, y: usize = 2, z };",
        expect![[r#"
            SOURCE_FILE@0..45
              STATIC_ITEM@0..45
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..44
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..25
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    WHITESPACE@21..22 " "
                    EQ@22..23 "="
                    WHITESPACE@23..24 " "
                    LITERAL@24..25
                      INT_NUMBER@24..25 "1"
                  COMMA@25..26 ","
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..39
                    NAME_REF@27..28
                      IDENT@27..28 "y"
                    COLON@28..29 ":"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..35
                      NAME_REF@30..35
                        IDENT@30..35 "usize"
                    WHITESPACE@35..36 " "
                    EQ@36..37 "="
                    WHITESPACE@37..38 " "
                    LITERAL@38..39
                      INT_NUMBER@38..39 "2"
                  COMMA@39..40 ","
                  WHITESPACE@40..41 " "
                  RECORD_EXPR_FIELD@41..42
                    NAME_REF@41..42
                      IDENT@41..42 "z"
                  WHITESPACE@42..43 " "
                  R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
        "#]],
    );
}

#[test]
fn record_field_old_colon_value_is_a_targeted_parse_error() {
    check(
        "static p = struct { x: 1 };",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..26
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..24
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    LITERAL@23..24
                      INT_NUMBER@23..24 "1"
                  WHITESPACE@24..25 " "
                  R_BRACE@25..26 "}"
                SEMICOLON@26..27 ";"
            error 23..24: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}
