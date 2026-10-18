//! Generic binders on fns and declarations.

use super::*;

// ---- generics: fn binders ----

#[test]
fn generic_fn_binder_type_params_only() {
    check(
        "static id = fn::<T>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..9
                  IDENT@7..9 "id"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..36
                  FN_KW@12..14 "fn"
                  GENERIC_PARAM_LIST@14..19
                    COLON2@14..16 "::"
                    L_ANGLE@16..17 "<"
                    TYPE_PARAM@17..18
                      NAME@17..18
                        IDENT@17..18 "T"
                    R_ANGLE@18..19 ">"
                  PARAM_LIST@19..25
                    L_PAREN@19..20 "("
                    PARAM@20..24
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "x"
                      COLON@21..22 ":"
                      WHITESPACE@22..23 " "
                      PATH_TYPE@23..24
                        NAME_REF@23..24
                          IDENT@23..24 "T"
                    R_PAREN@24..25 ")"
                  WHITESPACE@25..26 " "
                  RET_TYPE@26..30
                    THIN_ARROW@26..28 "->"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..30
                      NAME_REF@29..30
                        IDENT@29..30 "T"
                  WHITESPACE@30..31 " "
                  BLOCK_EXPR@31..36
                    L_BRACE@31..32 "{"
                    WHITESPACE@32..33 " "
                    PATH_EXPR@33..34
                      NAME_REF@33..34
                        IDENT@33..34 "x"
                    WHITESPACE@34..35 " "
                    R_BRACE@35..36 "}"
                SEMICOLON@36..37 ";"
        "#]],
    );
}

#[test]
fn generic_fn_binder_const_param_only() {
    check(
        "static make = fn::<const N: usize>() -> usize { N };",
        expect![[r#"
            SOURCE_FILE@0..52
              STATIC_ITEM@0..52
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "make"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                FN_LITERAL@14..51
                  FN_KW@14..16 "fn"
                  GENERIC_PARAM_LIST@16..34
                    COLON2@16..18 "::"
                    L_ANGLE@18..19 "<"
                    CONST_PARAM@19..33
                      CONST_KW@19..24 "const"
                      WHITESPACE@24..25 " "
                      NAME@25..26
                        IDENT@25..26 "N"
                      COLON@26..27 ":"
                      WHITESPACE@27..28 " "
                      PATH_TYPE@28..33
                        NAME_REF@28..33
                          IDENT@28..33 "usize"
                    R_ANGLE@33..34 ">"
                  PARAM_LIST@34..36
                    L_PAREN@34..35 "("
                    R_PAREN@35..36 ")"
                  WHITESPACE@36..37 " "
                  RET_TYPE@37..45
                    THIN_ARROW@37..39 "->"
                    WHITESPACE@39..40 " "
                    PATH_TYPE@40..45
                      NAME_REF@40..45
                        IDENT@40..45 "usize"
                  WHITESPACE@45..46 " "
                  BLOCK_EXPR@46..51
                    L_BRACE@46..47 "{"
                    WHITESPACE@47..48 " "
                    PATH_EXPR@48..49
                      NAME_REF@48..49
                        IDENT@48..49 "N"
                    WHITESPACE@49..50 " "
                    R_BRACE@50..51 "}"
                SEMICOLON@51..52 ";"
        "#]],
    );
}

#[test]
fn generic_fn_binder_mixed_with_trailing_comma() {
    check(
        "static f = fn::<T, const N: usize,>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..53
              STATIC_ITEM@0..53
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..52
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..35
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    TYPE_PARAM@16..17
                      NAME@16..17
                        IDENT@16..17 "T"
                    COMMA@17..18 ","
                    WHITESPACE@18..19 " "
                    CONST_PARAM@19..33
                      CONST_KW@19..24 "const"
                      WHITESPACE@24..25 " "
                      NAME@25..26
                        IDENT@25..26 "N"
                      COLON@26..27 ":"
                      WHITESPACE@27..28 " "
                      PATH_TYPE@28..33
                        NAME_REF@28..33
                          IDENT@28..33 "usize"
                    COMMA@33..34 ","
                    R_ANGLE@34..35 ">"
                  PARAM_LIST@35..41
                    L_PAREN@35..36 "("
                    PARAM@36..40
                      BIND_PAT@36..37
                        NAME@36..37
                          IDENT@36..37 "x"
                      COLON@37..38 ":"
                      WHITESPACE@38..39 " "
                      PATH_TYPE@39..40
                        NAME_REF@39..40
                          IDENT@39..40 "T"
                    R_PAREN@40..41 ")"
                  WHITESPACE@41..42 " "
                  RET_TYPE@42..46
                    THIN_ARROW@42..44 "->"
                    WHITESPACE@44..45 " "
                    PATH_TYPE@45..46
                      NAME_REF@45..46
                        IDENT@45..46 "T"
                  WHITESPACE@46..47 " "
                  BLOCK_EXPR@47..52
                    L_BRACE@47..48 "{"
                    WHITESPACE@48..49 " "
                    PATH_EXPR@49..50
                      NAME_REF@49..50
                        IDENT@49..50 "x"
                    WHITESPACE@50..51 " "
                    R_BRACE@51..52 "}"
                SEMICOLON@52..53 ";"
        "#]],
    );
}

#[test]
fn generic_fn_binder_on_const_fn() {
    check(
        "static f = const fn::<const N: usize>() -> usize { N };",
        expect![[r#"
            SOURCE_FILE@0..55
              STATIC_ITEM@0..55
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..54
                  CONST_KW@11..16 "const"
                  WHITESPACE@16..17 " "
                  FN_KW@17..19 "fn"
                  GENERIC_PARAM_LIST@19..37
                    COLON2@19..21 "::"
                    L_ANGLE@21..22 "<"
                    CONST_PARAM@22..36
                      CONST_KW@22..27 "const"
                      WHITESPACE@27..28 " "
                      NAME@28..29
                        IDENT@28..29 "N"
                      COLON@29..30 ":"
                      WHITESPACE@30..31 " "
                      PATH_TYPE@31..36
                        NAME_REF@31..36
                          IDENT@31..36 "usize"
                    R_ANGLE@36..37 ">"
                  PARAM_LIST@37..39
                    L_PAREN@37..38 "("
                    R_PAREN@38..39 ")"
                  WHITESPACE@39..40 " "
                  RET_TYPE@40..48
                    THIN_ARROW@40..42 "->"
                    WHITESPACE@42..43 " "
                    PATH_TYPE@43..48
                      NAME_REF@43..48
                        IDENT@43..48 "usize"
                  WHITESPACE@48..49 " "
                  BLOCK_EXPR@49..54
                    L_BRACE@49..50 "{"
                    WHITESPACE@50..51 " "
                    PATH_EXPR@51..52
                      NAME_REF@51..52
                        IDENT@51..52 "N"
                    WHITESPACE@52..53 " "
                    R_BRACE@53..54 "}"
                SEMICOLON@54..55 ";"
        "#]],
    );
}

#[test]
fn generic_fn_binder_unclosed_angle_recovers() {
    check(
        "static f = fn::<T, const N: usize;",
        expect![[r#"
            SOURCE_FILE@0..34
              STATIC_ITEM@0..34
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..34
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..34
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    TYPE_PARAM@16..17
                      NAME@16..17
                        IDENT@16..17 "T"
                    COMMA@17..18 ","
                    WHITESPACE@18..19 " "
                    CONST_PARAM@19..33
                      CONST_KW@19..24 "const"
                      WHITESPACE@24..25 " "
                      NAME@25..26
                        IDENT@25..26 "N"
                      COLON@26..27 ":"
                      WHITESPACE@27..28 " "
                      PATH_TYPE@28..33
                        NAME_REF@28..33
                          IDENT@28..33 "usize"
                    ERROR@33..34
                      SEMICOLON@33..34 ";"
            error 33..34: expected `,` or `>`
            error 34..34: expected `{`: function bodies are blocks
        "#]],
    );
}

#[test]
fn generic_fn_binder_missing_param_name_recovers() {
    check(
        "static f = fn::<, const N: usize>() -> usize { N };",
        expect![[r#"
            SOURCE_FILE@0..51
              STATIC_ITEM@0..51
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..50
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..33
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    COMMA@16..17 ","
                    WHITESPACE@17..18 " "
                    CONST_PARAM@18..32
                      CONST_KW@18..23 "const"
                      WHITESPACE@23..24 " "
                      NAME@24..25
                        IDENT@24..25 "N"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      PATH_TYPE@27..32
                        NAME_REF@27..32
                          IDENT@27..32 "usize"
                    R_ANGLE@32..33 ">"
                  PARAM_LIST@33..35
                    L_PAREN@33..34 "("
                    R_PAREN@34..35 ")"
                  WHITESPACE@35..36 " "
                  RET_TYPE@36..44
                    THIN_ARROW@36..38 "->"
                    WHITESPACE@38..39 " "
                    PATH_TYPE@39..44
                      NAME_REF@39..44
                        IDENT@39..44 "usize"
                  WHITESPACE@44..45 " "
                  BLOCK_EXPR@45..50
                    L_BRACE@45..46 "{"
                    WHITESPACE@46..47 " "
                    PATH_EXPR@47..48
                      NAME_REF@47..48
                        IDENT@47..48 "N"
                    WHITESPACE@48..49 " "
                    R_BRACE@49..50 "}"
                SEMICOLON@50..51 ";"
            error 16..17: expected a generic parameter
        "#]],
    );
}

#[test]
fn generic_fn_binder_const_param_without_type_recovers() {
    check(
        "static f = fn::<const N>() -> usize { N };",
        expect![[r#"
            SOURCE_FILE@0..42
              STATIC_ITEM@0..42
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..41
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..24
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    CONST_PARAM@16..23
                      CONST_KW@16..21 "const"
                      WHITESPACE@21..22 " "
                      NAME@22..23
                        IDENT@22..23 "N"
                    R_ANGLE@23..24 ">"
                  PARAM_LIST@24..26
                    L_PAREN@24..25 "("
                    R_PAREN@25..26 ")"
                  WHITESPACE@26..27 " "
                  RET_TYPE@27..35
                    THIN_ARROW@27..29 "->"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..35
                      NAME_REF@30..35
                        IDENT@30..35 "usize"
                  WHITESPACE@35..36 " "
                  BLOCK_EXPR@36..41
                    L_BRACE@36..37 "{"
                    WHITESPACE@37..38 " "
                    PATH_EXPR@38..39
                      NAME_REF@38..39
                        IDENT@38..39 "N"
                    WHITESPACE@39..40 " "
                    R_BRACE@40..41 "}"
                SEMICOLON@41..42 ";"
            error 23..24: expected `:` followed by the const parameter's type
        "#]],
    );
}

// ---- generics: type-declaration binders ----

#[test]
fn generic_binder_on_struct_literal() {
    check(
        "type Pair = struct::<T> { a: T, b: T };",
        expect![[r#"
            SOURCE_FILE@0..39
              TYPE_ITEM@0..39
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..9
                  IDENT@5..9 "Pair"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                RECORD_EXPR@12..38
                  STRUCT_KW@12..18 "struct"
                  GENERIC_PARAM_LIST@18..23
                    COLON2@18..20 "::"
                    L_ANGLE@20..21 "<"
                    TYPE_PARAM@21..22
                      NAME@21..22
                        IDENT@21..22 "T"
                    R_ANGLE@22..23 ">"
                  WHITESPACE@23..24 " "
                  L_BRACE@24..25 "{"
                  WHITESPACE@25..26 " "
                  RECORD_EXPR_FIELD@26..30
                    NAME_REF@26..27
                      IDENT@26..27 "a"
                    COLON@27..28 ":"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..30
                      NAME_REF@29..30
                        IDENT@29..30 "T"
                  COMMA@30..31 ","
                  WHITESPACE@31..32 " "
                  RECORD_EXPR_FIELD@32..36
                    NAME_REF@32..33
                      IDENT@32..33 "b"
                    COLON@33..34 ":"
                    WHITESPACE@34..35 " "
                    PATH_TYPE@35..36
                      NAME_REF@35..36
                        IDENT@35..36 "T"
                  WHITESPACE@36..37 " "
                  R_BRACE@37..38 "}"
                SEMICOLON@38..39 ";"
        "#]],
    );
}

#[test]
fn generic_binder_on_enum_literal() {
    check(
        "type Option = enum::<T> { Some(T), None };",
        expect![[r#"
            SOURCE_FILE@0..42
              TYPE_ITEM@0..42
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..11
                  IDENT@5..11 "Option"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                ENUM_EXPR@14..41
                  ENUM_KW@14..18 "enum"
                  GENERIC_PARAM_LIST@18..23
                    COLON2@18..20 "::"
                    L_ANGLE@20..21 "<"
                    TYPE_PARAM@21..22
                      NAME@21..22
                        IDENT@21..22 "T"
                    R_ANGLE@22..23 ">"
                  WHITESPACE@23..24 " "
                  L_BRACE@24..25 "{"
                  WHITESPACE@25..26 " "
                  ENUM_VARIANT@26..33
                    NAME@26..30
                      IDENT@26..30 "Some"
                    L_PAREN@30..31 "("
                    PATH_TYPE@31..32
                      NAME_REF@31..32
                        IDENT@31..32 "T"
                    R_PAREN@32..33 ")"
                  COMMA@33..34 ","
                  WHITESPACE@34..35 " "
                  ENUM_VARIANT@35..39
                    NAME@35..39
                      IDENT@35..39 "None"
                  WHITESPACE@39..40 " "
                  R_BRACE@40..41 "}"
                SEMICOLON@41..42 ";"
        "#]],
    );
}

#[test]
fn generic_binder_with_const_param_on_struct_literal() {
    check(
        "type Buf = struct::<T, const N: usize> { x: T };",
        expect![[r#"
            SOURCE_FILE@0..48
              TYPE_ITEM@0..48
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Buf"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..47
                  STRUCT_KW@11..17 "struct"
                  GENERIC_PARAM_LIST@17..38
                    COLON2@17..19 "::"
                    L_ANGLE@19..20 "<"
                    TYPE_PARAM@20..21
                      NAME@20..21
                        IDENT@20..21 "T"
                    COMMA@21..22 ","
                    WHITESPACE@22..23 " "
                    CONST_PARAM@23..37
                      CONST_KW@23..28 "const"
                      WHITESPACE@28..29 " "
                      NAME@29..30
                        IDENT@29..30 "N"
                      COLON@30..31 ":"
                      WHITESPACE@31..32 " "
                      PATH_TYPE@32..37
                        NAME_REF@32..37
                          IDENT@32..37 "usize"
                    R_ANGLE@37..38 ">"
                  WHITESPACE@38..39 " "
                  L_BRACE@39..40 "{"
                  WHITESPACE@40..41 " "
                  RECORD_EXPR_FIELD@41..45
                    NAME_REF@41..42
                      IDENT@41..42 "x"
                    COLON@42..43 ":"
                    WHITESPACE@43..44 " "
                    PATH_TYPE@44..45
                      NAME_REF@44..45
                        IDENT@44..45 "T"
                  WHITESPACE@45..46 " "
                  R_BRACE@46..47 "}"
                SEMICOLON@47..48 ";"
        "#]],
    );
}

#[test]
fn generic_enum_variant_path_expr() {
    check(
        "static x = Option::<usize>::Some(3);",
        expect![[r#"
            SOURCE_FILE@0..36
              STATIC_ITEM@0..36
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..35
                  PATH_EXPR@11..32
                    NAME_REF@11..17
                      IDENT@11..17 "Option"
                    COLON2@17..19 "::"
                    GENERIC_ARG_LIST@19..26
                      L_ANGLE@19..20 "<"
                      TYPE_ARG@20..25
                        PATH_TYPE@20..25
                          NAME_REF@20..25
                            IDENT@20..25 "usize"
                      R_ANGLE@25..26 ">"
                    COLON2@26..28 "::"
                    NAME_REF@28..32
                      IDENT@28..32 "Some"
                  ARG_LIST@32..35
                    L_PAREN@32..33 "("
                    LITERAL@33..34
                      INT_NUMBER@33..34 "3"
                    R_PAREN@34..35 ")"
                SEMICOLON@35..36 ";"
        "#]],
    );
}

#[test]
fn construction_turbofish_expr() {
    check(
        "static p = Pair::<usize>(struct { a = 1, b = 2 });",
        expect![[r#"
            SOURCE_FILE@0..50
              STATIC_ITEM@0..50
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..49
                  PATH_EXPR@11..24
                    NAME_REF@11..15
                      IDENT@11..15 "Pair"
                    COLON2@15..17 "::"
                    GENERIC_ARG_LIST@17..24
                      L_ANGLE@17..18 "<"
                      TYPE_ARG@18..23
                        PATH_TYPE@18..23
                          NAME_REF@18..23
                            IDENT@18..23 "usize"
                      R_ANGLE@23..24 ">"
                  ARG_LIST@24..49
                    L_PAREN@24..25 "("
                    RECORD_EXPR@25..48
                      STRUCT_KW@25..31 "struct"
                      WHITESPACE@31..32 " "
                      L_BRACE@32..33 "{"
                      WHITESPACE@33..34 " "
                      RECORD_EXPR_FIELD@34..39
                        NAME_REF@34..35
                          IDENT@34..35 "a"
                        WHITESPACE@35..36 " "
                        EQ@36..37 "="
                        WHITESPACE@37..38 " "
                        LITERAL@38..39
                          INT_NUMBER@38..39 "1"
                      COMMA@39..40 ","
                      WHITESPACE@40..41 " "
                      RECORD_EXPR_FIELD@41..46
                        NAME_REF@41..42
                          IDENT@41..42 "b"
                        WHITESPACE@42..43 " "
                        EQ@43..44 "="
                        WHITESPACE@44..45 " "
                        LITERAL@45..46
                          INT_NUMBER@45..46 "2"
                      WHITESPACE@46..47 " "
                      R_BRACE@47..48 "}"
                    R_PAREN@48..49 ")"
                SEMICOLON@49..50 ";"
        "#]],
    );
}
