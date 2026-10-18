//! Record and newtype destructuring, `pub` fields, field assignment.

use super::*;

// ---- record destructuring, `pub` reservation, field assignment ----

#[test]
fn let_record_destructure() {
    check(
        "static f = fn { let struct { x, y } = p; };",
        expect![[r#"
            SOURCE_FILE@0..43
              STATIC_ITEM@0..43
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..42
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..42
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..40
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      RECORD_PAT@20..35
                        STRUCT_KW@20..26 "struct"
                        WHITESPACE@26..27 " "
                        L_BRACE@27..28 "{"
                        WHITESPACE@28..29 " "
                        RECORD_PAT_FIELD@29..30
                          NAME@29..30
                            IDENT@29..30 "x"
                        COMMA@30..31 ","
                        WHITESPACE@31..32 " "
                        RECORD_PAT_FIELD@32..33
                          NAME@32..33
                            IDENT@32..33 "y"
                        WHITESPACE@33..34 " "
                        R_BRACE@34..35 "}"
                      WHITESPACE@35..36 " "
                      EQ@36..37 "="
                      WHITESPACE@37..38 " "
                      PATH_EXPR@38..39
                        NAME_REF@38..39
                          IDENT@38..39 "p"
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..41 " "
                    R_BRACE@41..42 "}"
                SEMICOLON@42..43 ";"
        "#]],
    );
}

#[test]
fn let_record_destructure_rename() {
    check(
        "static f = fn { let struct { x as a, y } = p; };",
        expect![[r#"
            SOURCE_FILE@0..48
              STATIC_ITEM@0..48
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..47
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..47
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..45
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      RECORD_PAT@20..40
                        STRUCT_KW@20..26 "struct"
                        WHITESPACE@26..27 " "
                        L_BRACE@27..28 "{"
                        WHITESPACE@28..29 " "
                        RECORD_PAT_FIELD@29..35
                          NAME@29..30
                            IDENT@29..30 "x"
                          WHITESPACE@30..31 " "
                          AS_KW@31..33 "as"
                          WHITESPACE@33..34 " "
                          NAME@34..35
                            IDENT@34..35 "a"
                        COMMA@35..36 ","
                        WHITESPACE@36..37 " "
                        RECORD_PAT_FIELD@37..38
                          NAME@37..38
                            IDENT@37..38 "y"
                        WHITESPACE@38..39 " "
                        R_BRACE@39..40 "}"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      PATH_EXPR@43..44
                        NAME_REF@43..44
                          IDENT@43..44 "p"
                      SEMICOLON@44..45 ";"
                    WHITESPACE@45..46 " "
                    R_BRACE@46..47 "}"
                SEMICOLON@47..48 ";"
        "#]],
    );
}

#[test]
fn let_record_destructure_rest() {
    check(
        "static f = fn { let struct { x, .. } = p; };",
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
                    LET_STMT@16..41
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      RECORD_PAT@20..36
                        STRUCT_KW@20..26 "struct"
                        WHITESPACE@26..27 " "
                        L_BRACE@27..28 "{"
                        WHITESPACE@28..29 " "
                        RECORD_PAT_FIELD@29..30
                          NAME@29..30
                            IDENT@29..30 "x"
                        COMMA@30..31 ","
                        WHITESPACE@31..32 " "
                        REST_PAT@32..34
                          DOT2@32..34 ".."
                        WHITESPACE@34..35 " "
                        R_BRACE@35..36 "}"
                      WHITESPACE@36..37 " "
                      EQ@37..38 "="
                      WHITESPACE@38..39 " "
                      PATH_EXPR@39..40
                        NAME_REF@39..40
                          IDENT@39..40 "p"
                      SEMICOLON@40..41 ";"
                    WHITESPACE@41..42 " "
                    R_BRACE@42..43 "}"
                SEMICOLON@43..44 ";"
        "#]],
    );
}

#[test]
fn let_record_destructure_per_binding_mut() {
    check(
        "static f = fn { let struct { mut x, y } = p; };",
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
                    LET_STMT@16..44
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      RECORD_PAT@20..39
                        STRUCT_KW@20..26 "struct"
                        WHITESPACE@26..27 " "
                        L_BRACE@27..28 "{"
                        WHITESPACE@28..29 " "
                        RECORD_PAT_FIELD@29..34
                          MUT_KW@29..32 "mut"
                          WHITESPACE@32..33 " "
                          NAME@33..34
                            IDENT@33..34 "x"
                        COMMA@34..35 ","
                        WHITESPACE@35..36 " "
                        RECORD_PAT_FIELD@36..37
                          NAME@36..37
                            IDENT@36..37 "y"
                        WHITESPACE@37..38 " "
                        R_BRACE@38..39 "}"
                      WHITESPACE@39..40 " "
                      EQ@40..41 "="
                      WHITESPACE@41..42 " "
                      PATH_EXPR@42..43
                        NAME_REF@42..43
                          IDENT@42..43 "p"
                      SEMICOLON@43..44 ";"
                    WHITESPACE@44..45 " "
                    R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
        "#]],
    );
}

#[test]
fn let_mut_on_destructuring_pattern_is_rejected() {
    check(
        "static f = fn { let mut struct { x } = p; };",
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
                    LET_STMT@16..41
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      MUT_KW@20..23 "mut"
                      WHITESPACE@23..24 " "
                      RECORD_PAT@24..36
                        STRUCT_KW@24..30 "struct"
                        WHITESPACE@30..31 " "
                        L_BRACE@31..32 "{"
                        WHITESPACE@32..33 " "
                        RECORD_PAT_FIELD@33..34
                          NAME@33..34
                            IDENT@33..34 "x"
                        WHITESPACE@34..35 " "
                        R_BRACE@35..36 "}"
                      WHITESPACE@36..37 " "
                      EQ@37..38 "="
                      WHITESPACE@38..39 " "
                      PATH_EXPR@39..40
                        NAME_REF@39..40
                          IDENT@39..40 "p"
                      SEMICOLON@40..41 ";"
                    WHITESPACE@41..42 " "
                    R_BRACE@42..43 "}"
                SEMICOLON@43..44 ";"
            error 20..36: `mut` applies to individual bindings in a destructuring pattern
        "#]],
    );
}

#[test]
fn param_record_destructure() {
    check(
        "static f = fn (struct { x, y }: struct { x: usize, y: usize }) { };",
        expect![[r#"
            SOURCE_FILE@0..67
              STATIC_ITEM@0..67
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..66
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..62
                    L_PAREN@14..15 "("
                    PARAM@15..61
                      RECORD_PAT@15..30
                        STRUCT_KW@15..21 "struct"
                        WHITESPACE@21..22 " "
                        L_BRACE@22..23 "{"
                        WHITESPACE@23..24 " "
                        RECORD_PAT_FIELD@24..25
                          NAME@24..25
                            IDENT@24..25 "x"
                        COMMA@25..26 ","
                        WHITESPACE@26..27 " "
                        RECORD_PAT_FIELD@27..28
                          NAME@27..28
                            IDENT@27..28 "y"
                        WHITESPACE@28..29 " "
                        R_BRACE@29..30 "}"
                      COLON@30..31 ":"
                      WHITESPACE@31..32 " "
                      RECORD_TYPE@32..61
                        STRUCT_KW@32..38 "struct"
                        WHITESPACE@38..39 " "
                        L_BRACE@39..40 "{"
                        WHITESPACE@40..41 " "
                        RECORD_TYPE_FIELD@41..49
                          NAME@41..42
                            IDENT@41..42 "x"
                          COLON@42..43 ":"
                          WHITESPACE@43..44 " "
                          PATH_TYPE@44..49
                            NAME_REF@44..49
                              IDENT@44..49 "usize"
                        COMMA@49..50 ","
                        WHITESPACE@50..51 " "
                        RECORD_TYPE_FIELD@51..59
                          NAME@51..52
                            IDENT@51..52 "y"
                          COLON@52..53 ":"
                          WHITESPACE@53..54 " "
                          PATH_TYPE@54..59
                            NAME_REF@54..59
                              IDENT@54..59 "usize"
                        WHITESPACE@59..60 " "
                        R_BRACE@60..61 "}"
                    R_PAREN@61..62 ")"
                  WHITESPACE@62..63 " "
                  BLOCK_EXPR@63..66
                    L_BRACE@63..64 "{"
                    WHITESPACE@64..65 " "
                    R_BRACE@65..66 "}"
                SEMICOLON@66..67 ";"
        "#]],
    );
}

#[test]
fn let_newtype_destructure() {
    check(
        "static f = fn { let Foo(struct { x, y }) = p; };",
        expect![[r#"
            SOURCE_FILE@0..48
              STATIC_ITEM@0..48
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..47
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..47
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..45
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      NEWTYPE_PAT@20..40
                        NAME_REF@20..23
                          IDENT@20..23 "Foo"
                        L_PAREN@23..24 "("
                        RECORD_PAT@24..39
                          STRUCT_KW@24..30 "struct"
                          WHITESPACE@30..31 " "
                          L_BRACE@31..32 "{"
                          WHITESPACE@32..33 " "
                          RECORD_PAT_FIELD@33..34
                            NAME@33..34
                              IDENT@33..34 "x"
                          COMMA@34..35 ","
                          WHITESPACE@35..36 " "
                          RECORD_PAT_FIELD@36..37
                            NAME@36..37
                              IDENT@36..37 "y"
                          WHITESPACE@37..38 " "
                          R_BRACE@38..39 "}"
                        R_PAREN@39..40 ")"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      PATH_EXPR@43..44
                        NAME_REF@43..44
                          IDENT@43..44 "p"
                      SEMICOLON@44..45 ";"
                    WHITESPACE@45..46 " "
                    R_BRACE@46..47 "}"
                SEMICOLON@47..48 ";"
        "#]],
    );
}

#[test]
fn param_newtype_destructure() {
    check(
        "static f = fn (Foo(struct { x, y })) { };",
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
                  PARAM_LIST@14..36
                    L_PAREN@14..15 "("
                    PARAM@15..35
                      NEWTYPE_PAT@15..35
                        NAME_REF@15..18
                          IDENT@15..18 "Foo"
                        L_PAREN@18..19 "("
                        RECORD_PAT@19..34
                          STRUCT_KW@19..25 "struct"
                          WHITESPACE@25..26 " "
                          L_BRACE@26..27 "{"
                          WHITESPACE@27..28 " "
                          RECORD_PAT_FIELD@28..29
                            NAME@28..29
                              IDENT@28..29 "x"
                          COMMA@29..30 ","
                          WHITESPACE@30..31 " "
                          RECORD_PAT_FIELD@31..32
                            NAME@31..32
                              IDENT@31..32 "y"
                          WHITESPACE@32..33 " "
                          R_BRACE@33..34 "}"
                        R_PAREN@34..35 ")"
                    R_PAREN@35..36 ")"
                  WHITESPACE@36..37 " "
                  BLOCK_EXPR@37..40
                    L_BRACE@37..38 "{"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn newtype_destructure_bare_bind_inner() {
    check(
        "static f = fn { let Foo(inner) = p; };",
        expect![[r#"
            SOURCE_FILE@0..38
              STATIC_ITEM@0..38
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..37
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..37
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..35
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      NEWTYPE_PAT@20..30
                        NAME_REF@20..23
                          IDENT@20..23 "Foo"
                        L_PAREN@23..24 "("
                        BIND_PAT@24..29
                          NAME@24..29
                            IDENT@24..29 "inner"
                        R_PAREN@29..30 ")"
                      WHITESPACE@30..31 " "
                      EQ@31..32 "="
                      WHITESPACE@32..33 " "
                      PATH_EXPR@33..34
                        NAME_REF@33..34
                          IDENT@33..34 "p"
                      SEMICOLON@34..35 ";"
                    WHITESPACE@35..36 " "
                    R_BRACE@36..37 "}"
                SEMICOLON@37..38 ";"
        "#]],
    );
}

#[test]
fn pub_reserved_on_record_type_field() {
    check(
        "static f: struct { pub x: usize } = p;",
        expect![[r#"
            SOURCE_FILE@0..38
              STATIC_ITEM@0..38
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                RECORD_TYPE@10..33
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_TYPE_FIELD@19..31
                    PUB_KW@19..22 "pub"
                    WHITESPACE@22..23 " "
                    NAME@23..24
                      IDENT@23..24 "x"
                    COLON@24..25 ":"
                    WHITESPACE@25..26 " "
                    PATH_TYPE@26..31
                      NAME_REF@26..31
                        IDENT@26..31 "usize"
                  WHITESPACE@31..32 " "
                  R_BRACE@32..33 "}"
                WHITESPACE@33..34 " "
                EQ@34..35 "="
                WHITESPACE@35..36 " "
                PATH_EXPR@36..37
                  NAME_REF@36..37
                    IDENT@36..37 "p"
                SEMICOLON@37..38 ";"
            error 19..22: field visibility is not supported yet
        "#]],
    );
}

#[test]
fn pub_reserved_on_type_decl_field() {
    check(
        "type Foo = struct { pub bar: usize };",
        expect![[r#"
            SOURCE_FILE@0..37
              TYPE_ITEM@0..37
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..36
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..34
                    PUB_KW@20..23 "pub"
                    WHITESPACE@23..24 " "
                    NAME_REF@24..27
                      IDENT@24..27 "bar"
                    COLON@27..28 ":"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..34
                      NAME_REF@29..34
                        IDENT@29..34 "usize"
                  WHITESPACE@34..35 " "
                  R_BRACE@35..36 "}"
                SEMICOLON@36..37 ";"
            error 20..23: field visibility is not supported yet
        "#]],
    );
}

#[test]
fn pub_rejected_on_record_literal_field() {
    check(
        "static f = struct { pub x = 1 };",
        expect![[r#"
            SOURCE_FILE@0..32
              STATIC_ITEM@0..32
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..31
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..29
                    PUB_KW@20..23 "pub"
                    WHITESPACE@23..24 " "
                    NAME_REF@24..25
                      IDENT@24..25 "x"
                    WHITESPACE@25..26 " "
                    EQ@26..27 "="
                    WHITESPACE@27..28 " "
                    LITERAL@28..29
                      INT_NUMBER@28..29 "1"
                  WHITESPACE@29..30 " "
                  R_BRACE@30..31 "}"
                SEMICOLON@31..32 ";"
            error 20..23: field visibility is not supported yet
        "#]],
    );
}

#[test]
fn field_assignment_is_a_legal_target() {
    // A field access is a place: it parses as the assignment's LHS and
    // validation has nothing to say (whether the root is `mut` is
    // semantic — inference's call).
    check(
        "static f = fn (p: struct { x: usize }) { p.x = 1; };",
        expect![[r#"
            SOURCE_FILE@0..52
              STATIC_ITEM@0..52
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..51
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..38
                    L_PAREN@14..15 "("
                    PARAM@15..37
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "p"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      RECORD_TYPE@18..37
                        STRUCT_KW@18..24 "struct"
                        WHITESPACE@24..25 " "
                        L_BRACE@25..26 "{"
                        WHITESPACE@26..27 " "
                        RECORD_TYPE_FIELD@27..35
                          NAME@27..28
                            IDENT@27..28 "x"
                          COLON@28..29 ":"
                          WHITESPACE@29..30 " "
                          PATH_TYPE@30..35
                            NAME_REF@30..35
                              IDENT@30..35 "usize"
                        WHITESPACE@35..36 " "
                        R_BRACE@36..37 "}"
                    R_PAREN@37..38 ")"
                  WHITESPACE@38..39 " "
                  BLOCK_EXPR@39..51
                    L_BRACE@39..40 "{"
                    WHITESPACE@40..41 " "
                    ASSIGN_STMT@41..49
                      FIELD_EXPR@41..44
                        PATH_EXPR@41..42
                          NAME_REF@41..42
                            IDENT@41..42 "p"
                        DOT@42..43 "."
                        NAME_REF@43..44
                          IDENT@43..44 "x"
                      WHITESPACE@44..45 " "
                      EQ@45..46 "="
                      WHITESPACE@46..47 " "
                      LITERAL@47..48
                        INT_NUMBER@47..48 "1"
                      SEMICOLON@48..49 ";"
                    WHITESPACE@49..50 " "
                    R_BRACE@50..51 "}"
                SEMICOLON@51..52 ";"
        "#]],
    );
}

#[test]
fn nested_field_assignment_parses_as_a_field_chain_target() {
    // `p.a.b = 2;` — the LHS is a field access whose receiver is another
    // field access, rooted at a plain variable: a legal place.
    check(
        "static f = fn { p.a.b = 2; };",
        expect![[r#"
            SOURCE_FILE@0..29
              STATIC_ITEM@0..29
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..28
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..28
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..26
                      FIELD_EXPR@16..21
                        FIELD_EXPR@16..19
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "p"
                          DOT@17..18 "."
                          NAME_REF@18..19
                            IDENT@18..19 "a"
                        DOT@19..20 "."
                        NAME_REF@20..21
                          IDENT@20..21 "b"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      LITERAL@24..25
                        INT_NUMBER@24..25 "2"
                      SEMICOLON@25..26 ";"
                    WHITESPACE@26..27 " "
                    R_BRACE@27..28 "}"
                SEMICOLON@28..29 ";"
        "#]],
    );
}

#[test]
fn field_chain_rooted_at_a_call_is_rejected() {
    // A chain is a place only when it roots at a variable; `f().x` has no
    // stable location to write to. The whole LHS carries the (widened)
    // non-place message.
    check(
        "static g = fn { f().x = 1; };",
        expect![[r#"
            SOURCE_FILE@0..29
              STATIC_ITEM@0..29
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "g"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..28
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..28
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..26
                      FIELD_EXPR@16..21
                        CALL_EXPR@16..19
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "f"
                          ARG_LIST@17..19
                            L_PAREN@17..18 "("
                            R_PAREN@18..19 ")"
                        DOT@19..20 "."
                        NAME_REF@20..21
                          IDENT@20..21 "x"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      LITERAL@24..25
                        INT_NUMBER@24..25 "1"
                      SEMICOLON@25..26 ";"
                    WHITESPACE@26..27 " "
                    R_BRACE@27..28 "}"
                SEMICOLON@28..29 ";"
            error 16..21: can only assign to a variable or its fields
        "#]],
    );
}
