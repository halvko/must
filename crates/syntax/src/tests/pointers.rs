//! Raw pointers, safe borrows and the retired prefix spellings.

use super::*;

#[test]
fn raw_pointer_types_parse() {
    check(
        "static f = fn(p: usize.&raw, q: usize.&raw mut) -> usize.&raw mut { q };",
        expect![[r#"
            SOURCE_FILE@0..72
              STATIC_ITEM@0..72
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..71
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..47
                    L_PAREN@13..14 "("
                    PARAM@14..27
                      BIND_PAT@14..15
                        NAME@14..15
                          IDENT@14..15 "p"
                      COLON@15..16 ":"
                      WHITESPACE@16..17 " "
                      RAW_PTR_TYPE@17..27
                        PATH_TYPE@17..22
                          NAME_REF@17..22
                            IDENT@17..22 "usize"
                        DOT@22..23 "."
                        AMP@23..24 "&"
                        RAW_KW@24..27 "raw"
                    COMMA@27..28 ","
                    WHITESPACE@28..29 " "
                    PARAM@29..46
                      BIND_PAT@29..30
                        NAME@29..30
                          IDENT@29..30 "q"
                      COLON@30..31 ":"
                      WHITESPACE@31..32 " "
                      RAW_PTR_TYPE@32..46
                        PATH_TYPE@32..37
                          NAME_REF@32..37
                            IDENT@32..37 "usize"
                        DOT@37..38 "."
                        AMP@38..39 "&"
                        RAW_KW@39..42 "raw"
                        WHITESPACE@42..43 " "
                        MUT_KW@43..46 "mut"
                    R_PAREN@46..47 ")"
                  WHITESPACE@47..48 " "
                  RET_TYPE@48..65
                    THIN_ARROW@48..50 "->"
                    WHITESPACE@50..51 " "
                    RAW_PTR_TYPE@51..65
                      PATH_TYPE@51..56
                        NAME_REF@51..56
                          IDENT@51..56 "usize"
                      DOT@56..57 "."
                      AMP@57..58 "&"
                      RAW_KW@58..61 "raw"
                      WHITESPACE@61..62 " "
                      MUT_KW@62..65 "mut"
                  WHITESPACE@65..66 " "
                  BLOCK_EXPR@66..71
                    L_BRACE@66..67 "{"
                    WHITESPACE@67..68 " "
                    PATH_EXPR@68..69
                      NAME_REF@68..69
                        IDENT@68..69 "q"
                    WHITESPACE@69..70 " "
                    R_BRACE@70..71 "}"
                SEMICOLON@71..72 ";"
        "#]],
    );
}

#[test]
fn raw_pointer_type_in_generic_argument_parses() {
    check(
        "static x: Pair::<usize.&raw mut> = y;",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..32
                  NAME_REF@10..14
                    IDENT@10..14 "Pair"
                  COLON2@14..16 "::"
                  GENERIC_ARG_LIST@16..32
                    L_ANGLE@16..17 "<"
                    TYPE_ARG@17..31
                      RAW_PTR_TYPE@17..31
                        PATH_TYPE@17..22
                          NAME_REF@17..22
                            IDENT@17..22 "usize"
                        DOT@22..23 "."
                        AMP@23..24 "&"
                        RAW_KW@24..27 "raw"
                        WHITESPACE@27..28 " "
                        MUT_KW@28..31 "mut"
                    R_ANGLE@31..32 ">"
                WHITESPACE@32..33 " "
                EQ@33..34 "="
                WHITESPACE@34..35 " "
                PATH_EXPR@35..36
                  NAME_REF@35..36
                    IDENT@35..36 "y"
                SEMICOLON@36..37 ";"
        "#]],
    );
}

#[test]
fn raw_pointer_type_in_array_element_parses() {
    check(
        "static x: [usize.&raw mut; 2] = y;",
        expect![[r#"
            SOURCE_FILE@0..34
              STATIC_ITEM@0..34
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..29
                  L_BRACKET@10..11 "["
                  RAW_PTR_TYPE@11..25
                    PATH_TYPE@11..16
                      NAME_REF@11..16
                        IDENT@11..16 "usize"
                    DOT@16..17 "."
                    AMP@17..18 "&"
                    RAW_KW@18..21 "raw"
                    WHITESPACE@21..22 " "
                    MUT_KW@22..25 "mut"
                  SEMICOLON@25..26 ";"
                  WHITESPACE@26..27 " "
                  CONST_ARG@27..28
                    LITERAL@27..28
                      INT_NUMBER@27..28 "2"
                  R_BRACKET@28..29 "]"
                WHITESPACE@29..30 " "
                EQ@30..31 "="
                WHITESPACE@31..32 " "
                PATH_EXPR@32..33
                  NAME_REF@32..33
                    IDENT@32..33 "y"
                SEMICOLON@33..34 ";"
        "#]],
    );
}

#[test]
fn addr_of_both_flavors_parse() {
    check(
        "static f = fn { let a = x.&raw; let b = y.z.&raw mut; };",
        expect![[r#"
            SOURCE_FILE@0..56
              STATIC_ITEM@0..56
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..55
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..55
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..31
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      ADDR_OF_EXPR@24..30
                        PATH_EXPR@24..25
                          NAME_REF@24..25
                            IDENT@24..25 "x"
                        DOT@25..26 "."
                        AMP@26..27 "&"
                        RAW_KW@27..30 "raw"
                      SEMICOLON@30..31 ";"
                    WHITESPACE@31..32 " "
                    LET_STMT@32..53
                      LET_KW@32..35 "let"
                      WHITESPACE@35..36 " "
                      BIND_PAT@36..37
                        NAME@36..37
                          IDENT@36..37 "b"
                      WHITESPACE@37..38 " "
                      EQ@38..39 "="
                      WHITESPACE@39..40 " "
                      ADDR_OF_EXPR@40..52
                        FIELD_EXPR@40..43
                          PATH_EXPR@40..41
                            NAME_REF@40..41
                              IDENT@40..41 "y"
                          DOT@41..42 "."
                          NAME_REF@42..43
                            IDENT@42..43 "z"
                        DOT@43..44 "."
                        AMP@44..45 "&"
                        RAW_KW@45..48 "raw"
                        WHITESPACE@48..49 " "
                        MUT_KW@49..52 "mut"
                      SEMICOLON@52..53 ";"
                    WHITESPACE@53..54 " "
                    R_BRACE@54..55 "}"
                SEMICOLON@55..56 ";"
        "#]],
    );
}

#[test]
fn addr_of_binds_tighter_than_comparison() {
    check(
        "static f = fn { x.&raw == x.&raw };",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
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
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..34
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    BIN_EXPR@16..32
                      ADDR_OF_EXPR@16..22
                        PATH_EXPR@16..17
                          NAME_REF@16..17
                            IDENT@16..17 "x"
                        DOT@17..18 "."
                        AMP@18..19 "&"
                        RAW_KW@19..22 "raw"
                      WHITESPACE@22..23 " "
                      EQ2@23..25 "=="
                      WHITESPACE@25..26 " "
                      ADDR_OF_EXPR@26..32
                        PATH_EXPR@26..27
                          NAME_REF@26..27
                            IDENT@26..27 "x"
                        DOT@27..28 "."
                        AMP@28..29 "&"
                        RAW_KW@29..32 "raw"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn postfix_deref_chains_in_larger_expressions() {
    check(
        "static f = fn { p.*.a + q.*.buf.* };",
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
                    BIN_EXPR@16..33
                      FIELD_EXPR@16..21
                        DEREF_EXPR@16..19
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "p"
                          DOT@17..18 "."
                          STAR@18..19 "*"
                        DOT@19..20 "."
                        NAME_REF@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      PLUS@22..23 "+"
                      WHITESPACE@23..24 " "
                      DEREF_EXPR@24..33
                        FIELD_EXPR@24..31
                          DEREF_EXPR@24..27
                            PATH_EXPR@24..25
                              NAME_REF@24..25
                                IDENT@24..25 "q"
                            DOT@25..26 "."
                            STAR@26..27 "*"
                          DOT@27..28 "."
                          NAME_REF@28..31
                            IDENT@28..31 "buf"
                        DOT@31..32 "."
                        STAR@32..33 "*"
                    WHITESPACE@33..34 " "
                    R_BRACE@34..35 "}"
                SEMICOLON@35..36 ";"
        "#]],
    );
}

#[test]
fn deref_write_target_parses() {
    check(
        "static f = fn { p.* = 5; };",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..26
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..26
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..24
                      DEREF_EXPR@16..19
                        PATH_EXPR@16..17
                          NAME_REF@16..17
                            IDENT@16..17 "p"
                        DOT@17..18 "."
                        STAR@18..19 "*"
                      WHITESPACE@19..20 " "
                      EQ@20..21 "="
                      WHITESPACE@21..22 " "
                      LITERAL@22..23
                        INT_NUMBER@22..23 "5"
                      SEMICOLON@23..24 ";"
                    WHITESPACE@24..25 " "
                    R_BRACE@25..26 "}"
                SEMICOLON@26..27 ";"
        "#]],
    );
}

#[test]
fn deref_projected_write_target_parses() {
    check(
        "static f = fn { p.*.a = 5; };",
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
                        DEREF_EXPR@16..19
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "p"
                          DOT@17..18 "."
                          STAR@18..19 "*"
                        DOT@19..20 "."
                        NAME_REF@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      LITERAL@24..25
                        INT_NUMBER@24..25 "5"
                      SEMICOLON@25..26 ";"
                    WHITESPACE@26..27 " "
                    R_BRACE@27..28 "}"
                SEMICOLON@28..29 ";"
        "#]],
    );
}

#[test]
fn addr_of_element_and_through_deref_places_parse() {
    check(
        "static f = fn { a[0].&raw mut == p.*.x.&raw mut };",
        expect![[r#"
            SOURCE_FILE@0..50
              STATIC_ITEM@0..50
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..49
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..49
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    BIN_EXPR@16..47
                      ADDR_OF_EXPR@16..29
                        INDEX_EXPR@16..20
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "a"
                          L_BRACKET@17..18 "["
                          LITERAL@18..19
                            INT_NUMBER@18..19 "0"
                          R_BRACKET@19..20 "]"
                        DOT@20..21 "."
                        AMP@21..22 "&"
                        RAW_KW@22..25 "raw"
                        WHITESPACE@25..26 " "
                        MUT_KW@26..29 "mut"
                      WHITESPACE@29..30 " "
                      EQ2@30..32 "=="
                      WHITESPACE@32..33 " "
                      ADDR_OF_EXPR@33..47
                        FIELD_EXPR@33..38
                          DEREF_EXPR@33..36
                            PATH_EXPR@33..34
                              NAME_REF@33..34
                                IDENT@33..34 "p"
                            DOT@34..35 "."
                            STAR@35..36 "*"
                          DOT@36..37 "."
                          NAME_REF@37..38
                            IDENT@37..38 "x"
                        DOT@38..39 "."
                        AMP@39..40 "&"
                        RAW_KW@40..43 "raw"
                        WHITESPACE@43..44 " "
                        MUT_KW@44..47 "mut"
                    WHITESPACE@47..48 " "
                    R_BRACE@48..49 "}"
                SEMICOLON@49..50 ";"
        "#]],
    );
}

#[test]
fn postfix_raw_borrow_chains_through_deref() {
    // Expression-first greedy postfix: `x.&raw mut.*` chains the deref onto
    // the address-of (the pointer is formed, then immediately followed), and
    // `p.*.&raw mut` takes the address through a deref — both single postfix
    // chains, the spaced `mut` never shattering the chain (the whole point of
    // moving raw borrows postfix). The `= 9` then meets the unchanged
    // assign-target rule: the deref's receiver is an address-of, not a
    // variable, so the place isn't variable-rooted (the prefix form could not
    // even spell this inline, so it is new territory — a future deref-of-
    // address-of cancellation is a borrow-round call, not this migration's).
    check(
        "static f = fn { x.&raw mut.* = 9; p.*.&raw mut };",
        expect![[r#"
            SOURCE_FILE@0..49
              STATIC_ITEM@0..49
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..48
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..48
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..33
                      DEREF_EXPR@16..28
                        ADDR_OF_EXPR@16..26
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "x"
                          DOT@17..18 "."
                          AMP@18..19 "&"
                          RAW_KW@19..22 "raw"
                          WHITESPACE@22..23 " "
                          MUT_KW@23..26 "mut"
                        DOT@26..27 "."
                        STAR@27..28 "*"
                      WHITESPACE@28..29 " "
                      EQ@29..30 "="
                      WHITESPACE@30..31 " "
                      LITERAL@31..32
                        INT_NUMBER@31..32 "9"
                      SEMICOLON@32..33 ";"
                    WHITESPACE@33..34 " "
                    ADDR_OF_EXPR@34..46
                      DEREF_EXPR@34..37
                        PATH_EXPR@34..35
                          NAME_REF@34..35
                            IDENT@34..35 "p"
                        DOT@35..36 "."
                        STAR@36..37 "*"
                      DOT@37..38 "."
                      AMP@38..39 "&"
                      RAW_KW@39..42 "raw"
                      WHITESPACE@42..43 " "
                      MUT_KW@43..46 "mut"
                    WHITESPACE@46..47 " "
                    R_BRACE@47..48 "}"
                SEMICOLON@48..49 ";"
            error 16..28: can only assign to a variable or its fields
        "#]],
    );
}

#[test]
fn postfix_raw_borrow_precedence_vs_unary_minus() {
    // Postfix binds tighter than unary minus: `-x.&raw` negates the pointer
    // (address first, then the prefix `-`), exactly as `-a.b` negates the
    // field — the postfix chain is part of the same operand tier.
    check(
        "static f = fn { -x.&raw };",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..25
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..25
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    NEG_EXPR@16..23
                      MINUS@16..17 "-"
                      ADDR_OF_EXPR@17..23
                        PATH_EXPR@17..18
                          NAME_REF@17..18
                            IDENT@17..18 "x"
                        DOT@18..19 "."
                        AMP@19..20 "&"
                        RAW_KW@20..23 "raw"
                    WHITESPACE@23..24 " "
                    R_BRACE@24..25 "}"
                SEMICOLON@25..26 ";"
        "#]],
    );
}

#[test]
fn retired_prefix_raw_borrow_expr_migration() {
    // The retired prefix spelling superset-parses into the same node with a
    // targeted migration diagnostic (never a silent reinterpretation).
    check(
        "static f = fn { let a = &raw x; let b = &raw mut y.z; };",
        expect![[r#"
            SOURCE_FILE@0..56
              STATIC_ITEM@0..56
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..55
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..55
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..31
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      ADDR_OF_EXPR@24..30
                        AMP@24..25 "&"
                        RAW_KW@25..28 "raw"
                        WHITESPACE@28..29 " "
                        PATH_EXPR@29..30
                          NAME_REF@29..30
                            IDENT@29..30 "x"
                      SEMICOLON@30..31 ";"
                    WHITESPACE@31..32 " "
                    LET_STMT@32..53
                      LET_KW@32..35 "let"
                      WHITESPACE@35..36 " "
                      BIND_PAT@36..37
                        NAME@36..37
                          IDENT@36..37 "b"
                      WHITESPACE@37..38 " "
                      EQ@38..39 "="
                      WHITESPACE@39..40 " "
                      ADDR_OF_EXPR@40..52
                        AMP@40..41 "&"
                        RAW_KW@41..44 "raw"
                        WHITESPACE@44..45 " "
                        MUT_KW@45..48 "mut"
                        WHITESPACE@48..49 " "
                        FIELD_EXPR@49..52
                          PATH_EXPR@49..50
                            NAME_REF@49..50
                              IDENT@49..50 "y"
                          DOT@50..51 "."
                          NAME_REF@51..52
                            IDENT@51..52 "z"
                      SEMICOLON@52..53 ";"
                    WHITESPACE@53..54 " "
                    R_BRACE@54..55 "}"
                SEMICOLON@55..56 ";"
            error 24..25: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
            error 40..41: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
        "#]],
    );
}

#[test]
fn retired_prefix_raw_ptr_type_migration() {
    check(
        "static f = fn(p: &raw mut usize) -> () { };",
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
                  PARAM_LIST@13..32
                    L_PAREN@13..14 "("
                    PARAM@14..31
                      BIND_PAT@14..15
                        NAME@14..15
                          IDENT@14..15 "p"
                      COLON@15..16 ":"
                      WHITESPACE@16..17 " "
                      RAW_PTR_TYPE@17..31
                        AMP@17..18 "&"
                        RAW_KW@18..21 "raw"
                        WHITESPACE@21..22 " "
                        MUT_KW@22..25 "mut"
                        WHITESPACE@25..26 " "
                        PATH_TYPE@26..31
                          NAME_REF@26..31
                            IDENT@26..31 "usize"
                    R_PAREN@31..32 ")"
                  WHITESPACE@32..33 " "
                  RET_TYPE@33..38
                    THIN_ARROW@33..35 "->"
                    WHITESPACE@35..36 " "
                    UNIT_TYPE@36..38
                      L_PAREN@36..37 "("
                      R_PAREN@37..38 ")"
                  WHITESPACE@38..39 " "
                  BLOCK_EXPR@39..42
                    L_BRACE@39..40 "{"
                    WHITESPACE@40..41 " "
                    R_BRACE@41..42 "}"
                SEMICOLON@42..43 ";"
            error 17..18: raw pointer types are spelled postfix: `T.&raw` / `T.&raw mut`
        "#]],
    );
}

#[test]
fn retired_prefix_borrow_expr_migration() {
    // The retired prefix safe-borrow spelling superset-parses into the same
    // `BORROW_EXPR` node the postfix form produces — a targeted migration
    // diagnostic, never a silent reinterpretation, mirroring the `&raw`
    // precedent above.
    check(
        "static f = fn { let a = &x; let b = &mut y; };",
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
                    LET_STMT@16..27
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      BORROW_EXPR@24..26
                        AMP@24..25 "&"
                        PATH_EXPR@25..26
                          NAME_REF@25..26
                            IDENT@25..26 "x"
                      SEMICOLON@26..27 ";"
                    WHITESPACE@27..28 " "
                    LET_STMT@28..43
                      LET_KW@28..31 "let"
                      WHITESPACE@31..32 " "
                      BIND_PAT@32..33
                        NAME@32..33
                          IDENT@32..33 "b"
                      WHITESPACE@33..34 " "
                      EQ@34..35 "="
                      WHITESPACE@35..36 " "
                      BORROW_EXPR@36..42
                        AMP@36..37 "&"
                        MUT_KW@37..40 "mut"
                        WHITESPACE@40..41 " "
                        PATH_EXPR@41..42
                          NAME_REF@41..42
                            IDENT@41..42 "y"
                      SEMICOLON@42..43 ";"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
            error 24..26: borrows are spelled postfix: `x.&` / `x.&mut`
            error 36..42: borrows are spelled postfix: `x.&` / `x.&mut`
        "#]],
    );
}

#[test]
fn retired_prefix_borrow_expr_fix_rewrites_to_postfix() {
    // The fix is two edits (delete the leading `&`/`&mut`, append the
    // postfix operator after the operand) — the "wrap in `{ }`"-style
    // multi-edit `Fix`, not a single parser-level insertion.
    let non_mut = "static f = fn { &x; };";
    let parse = crate::parse(non_mut);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrows are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(fix.label, "Rewrite as postfix");
    assert_eq!(apply_fix(non_mut, fix), "static f = fn { x.&; };");

    let mutable = "static f = fn { &mut x; };";
    let parse = crate::parse(mutable);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrows are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(apply_fix(mutable, fix), "static f = fn { x.&mut; };");
}

#[test]
fn retired_prefix_borrow_type_migration() {
    // Type position's mirror of the expression-side migration above.
    check(
        "static f = fn(a: &str, b: &mut str) -> () { };",
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
                  PARAM_LIST@13..35
                    L_PAREN@13..14 "("
                    PARAM@14..21
                      BIND_PAT@14..15
                        NAME@14..15
                          IDENT@14..15 "a"
                      COLON@15..16 ":"
                      WHITESPACE@16..17 " "
                      BORROW_TYPE@17..21
                        AMP@17..18 "&"
                        PATH_TYPE@18..21
                          NAME_REF@18..21
                            IDENT@18..21 "str"
                    COMMA@21..22 ","
                    WHITESPACE@22..23 " "
                    PARAM@23..34
                      BIND_PAT@23..24
                        NAME@23..24
                          IDENT@23..24 "b"
                      COLON@24..25 ":"
                      WHITESPACE@25..26 " "
                      BORROW_TYPE@26..34
                        AMP@26..27 "&"
                        MUT_KW@27..30 "mut"
                        WHITESPACE@30..31 " "
                        PATH_TYPE@31..34
                          NAME_REF@31..34
                            IDENT@31..34 "str"
                    R_PAREN@34..35 ")"
                  WHITESPACE@35..36 " "
                  RET_TYPE@36..41
                    THIN_ARROW@36..38 "->"
                    WHITESPACE@38..39 " "
                    UNIT_TYPE@39..41
                      L_PAREN@39..40 "("
                      R_PAREN@40..41 ")"
                  WHITESPACE@41..42 " "
                  BLOCK_EXPR@42..45
                    L_BRACE@42..43 "{"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
            error 17..21: borrow types are spelled postfix: `T.&` / `T.&mut`
            error 26..34: borrow types are spelled postfix: `T.&` / `T.&mut`
        "#]],
    );
}

#[test]
fn retired_prefix_borrow_type_fix_rewrites_to_postfix() {
    let plain = "static f = fn(s: &str) {};";
    let parse = crate::parse(plain);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrow types are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(fix.label, "Rewrite as postfix");
    // No region turbofish: `T.&` requires one everywhere and hir's own
    // "must name its region" diagnostic says so — inventing a name here
    // would be a guess.
    assert_eq!(apply_fix(plain, fix), "static f = fn(s: str.&) {};");

    let mutable = "static f = fn(s: &mut str) {};";
    let parse = crate::parse(mutable);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrow types are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(apply_fix(mutable, fix), "static f = fn(s: str.&mut) {};");
}

#[test]
fn a_prefix_borrow_type_postfix_would_rebind_gets_no_fix() {
    // Postfix `.&` binds to whatever the type ENDS in, and there is no type
    // grouping to override it (G08). So for a `fn(..) -> T` referent the
    // rewrite would land on the RETURN type (`&fn() -> usize` is a borrow OF
    // a fn; `fn() -> usize.&` is a fn returning one), for a retired prefix
    // raw pointer on the pointee, and for a retired prefix BORROW on that
    // borrow's own referent — which for `&&mut T` swaps the two borrows'
    // mutability (`&mut usize.&` is `usize.&.&mut`, not `usize.&mut.&`).
    // Each migrates; none offers a fix that silently changes the type.
    // Validation walks in preorder, so the first diagnostic is the OUTER
    // borrow's — the one whose rewrite would rebind.
    for text in [
        "static f = fn(a: &fn() -> usize) {};",
        "static f = fn(a: &&raw usize) {};",
        "static f = fn(a: &&mut usize) {};",
        "static f = fn(a: &mut &usize) {};",
        "static f = fn(a: &&fn() -> usize) {};",
        "static f = fn(a: &&&raw usize) {};",
    ] {
        let parse = crate::parse(text);
        let err = parse
            .errors()
            .iter()
            .find(|e| e.message.starts_with("borrow types are spelled postfix"))
            .expect("expected the migration diagnostic");
        assert!(err.fix.is_none(), "postfix would rebind: {text}");
    }
    // A `fn` type with no return type ends in its own `)`, so the rewrite is
    // sound and the fix stays on offer.
    let text = "static f = fn(a: &fn(usize)) {};";
    let parse = crate::parse(text);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrow types are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(apply_fix(text, fix), "static f = fn(a: fn(usize).&) {};");
    // And the inner spelling migrating first is what unblocks the outer one:
    // over a referent that is ALREADY postfix, the outer fix is sound again.
    let text = "static f = fn(a: &usize.&mut::<@a>) {};";
    let parse = crate::parse(text);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrow types are spelled postfix"))
        .expect("expected the migration diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(
        apply_fix(text, fix),
        "static f = fn(a: usize.&mut::<@a>.&) {};"
    );
}

#[test]
fn a_prefix_borrow_with_no_operand_migrates_without_a_fix() {
    // The prefix spelling can lose its operand — the postfix form never
    // could, since it always precedes one. The migration still reports;
    // the rewrite has nothing to move, so no fix is offered.
    let text = "static f = fn { let r = &; };";
    let parse = crate::parse(text);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrows are spelled postfix"))
        .expect("expected the migration diagnostic");
    assert!(
        err.fix.is_none(),
        "an operandless borrow has nothing to move"
    );
}

#[test]
fn retired_prefix_borrow_type_does_not_chase_a_retired_lifetime() {
    // The parser does not chase a retired spelling across token kinds
    // (G26). In legacy `&'a T` the `&` fires its own migration and stops;
    // the freed `'` opens an unterminated character literal (the scan is
    // bounded to the line, so it costs one token), and the referent never
    // reaches the borrow — so no rewrite is offered and
    // what followed is recovered as further (garbage) parameters. Making
    // sense of the wreckage is diagnostics-layer work, not the base
    // parser's; what is pinned here is that the parse terminates, covers
    // the whole input, and stays honest about the `&`.
    let text = "static f = fn(s: &'a str) {};";
    let parse = crate::parse(text);
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("borrow types are spelled postfix")),
        "the `&` still migrates: {messages:?}"
    );
    assert!(
        messages.contains(&"unterminated character literal: expected a closing `'`"),
        "the freed `'` is a plain lexer error: {messages:?}"
    );
    let root = parse.syntax_node();
    let borrow = root
        .descendants()
        .find(|n| n.kind() == crate::SyntaxKind::BORROW_TYPE)
        .expect("the `&` still opens a borrow type");
    assert_eq!(
        borrow.text_range(),
        crate::TextRange::new(17.into(), 18.into())
    );
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("borrow types are spelled postfix"))
        .expect("expected the migration diagnostic");
    assert!(err.fix.is_none(), "no referent, nothing to rewrite");
    assert_eq!(
        usize::from(root.text_range().end()),
        text.len(),
        "recovery still covers the whole input"
    );
}

#[test]
fn retired_lifetime_in_return_type_desyncs_the_rest_of_the_item() {
    // Param position recovers (see the sibling test above): `param_list`'s
    // loop consumes the stranded `'`/name/referent as bogus extra params
    // and re-syncs at the closing `)`. Return-type position has no such
    // loop — `ret_type` calls `type_` once — so when the recursive call
    // bails on the CHAR without consuming it, `fn_literal` falls into its
    // "superset-parse the body as an expression" branch, swallowing the
    // lone `'` as a garbage one-token body. The `static` item closes right
    // there, before its trailing `;` — that error is suppressed in
    // `Builder::error`, because the lexer's error on the `'` already
    // intersects the token it would anchor to. Everything after is then
    // read by `source_file`'s item loop, token by token, as "expected an
    // item" until the next item keyword — here, end of input.
    check_errors(
        "static f = fn(s: str) -> &'a str { s };",
        expect![[r#"
            25..26: borrow types are spelled postfix: `T.&` / `T.&mut`
            26..27: unterminated character literal: expected a closing `'`
            27..28: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            29..32: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            33..34: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            35..36: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            37..38: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            38..39: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
    // Not lifetime-specific: any type token `type_` refuses desyncs the
    // item the same way — only the leading errors differ. `)` (unlike the
    // CHAR above) is in `at_expr_recovery`'s gate set, so `fn_literal`
    // takes its "expected `{`" branch instead of superset-parsing a body;
    // no body is added, so the item's own "expected `;`" is left
    // un-suppressed (nothing else has flagged the `->` it anchors to), and
    // `type_`'s "expected a type" survives instead of being deduped
    // against a same-range lexer error the way the CHAR case's was.
    check_errors(
        "static f = fn(s: str) -> ) str { s };",
        expect![[r#"
            23..24: expected `;`
            25..26: expected a type
            27..30: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            31..32: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            33..34: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            35..36: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            36..37: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}

#[test]
fn retired_lifetime_in_struct_field_desyncs_the_rest_of_the_item() {
    // Struct-field position's version of the case above, via a different
    // mechanism: `x`'s name and `:` parse fine, but the field's type hits
    // the same "AMP consumed, referent bails on the CHAR" shape, leaving
    // `record_type`'s loop stuck at the CHAR. It reruns
    // `record_type_field`, which this time takes its no-name branch
    // (`p.at(IDENT)` is false) and also consumes nothing, so the loop's
    // zero-progress check breaks it. The `RECORD_TYPE` closes without its
    // own `}`, the enclosing `static` item closes without `=`/`;`, and
    // `source_file`'s item loop reads everything from there on, token by
    // token, as "expected an item" until the next item keyword — here, end
    // of input (a following item resyncs normally, same as any other bad
    // item — this isn't special to the cascade).
    //
    // This is a genuine type-position gap, not just ordinary item-boundary
    // resync: `type_core`'s catch-all never consumes a token, so
    // `record_type_field` can't see a type here no matter what follows. A
    // boundary-gated catch-all there — bump one token when `type_` refuses
    // and the parser isn't sitting at an `at_expr_recovery` boundary,
    // mirroring `primary_expr`'s own gate — would let this snippet's item
    // resync at its real `}`/`;` instead of cascading; not built here.
    check_errors(
        "static p: struct { x: &'a str } = 0;",
        expect![[r#"
            22..23: borrow types are spelled postfix: `T.&` / `T.&mut`
            23..24: unterminated character literal: expected a closing `'`
            24..25: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            26..29: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            30..31: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            32..33: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            34..35: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            35..36: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
    // The tail cascade is identical, and both snippets actually run the
    // same two `record_type_field` passes: a with-name pass on `x` whose
    // type fails, then a zero-progress no-name pass that breaks the loop.
    // The leading errors differ only in where that pile of same-range
    // errors lands and which of them survives. Here nothing is consumed
    // past `:`, so the pile sits at 22..23 (the `)`) where `type_`'s
    // "expected a type" is first in and wins `parse`'s range-equal dedup,
    // and the missing `}` anchors on the untouched `:` at 20..21 and
    // survives. With the retired lifetime the `&` is consumed as a
    // `BORROW_TYPE`, so the pile lands on the CHAR the lexer already
    // flagged at 23..24 — the lexer error wins that dedup instead — and
    // the missing `}` now anchors on the `&` (22..23), which touches that
    // pile, so `Builder::error`'s after-prev suppression drops it before
    // it is ever pushed.
    check_errors(
        "static p: struct { x: ) str } = 0;",
        expect![[r#"
            20..21: expected `}`
            22..23: expected a type
            24..27: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            28..29: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            30..31: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            32..33: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            33..34: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
        "#]],
    );
}

#[test]
fn bare_amp_between_expressions_is_not_a_binary_operator() {
    // `&` has never bound as a binary operator (it is absent from
    // `expr_bp`'s precedence table — this sweep doesn't add it there
    // either, only to `primary_expr`'s prefix dispatch), so `a & b` is not
    // "bitwise and": `a` ends the first statement (missing `;`, with the
    // usual insert fix), and `& b` starts a fresh one — which the retired
    // prefix borrow now claims, exactly as `&b` on its own would. The
    // first statement's error is unaffected by this sweep; the second's
    // used to be "expected an expression" and is now the migration
    // diagnostic, because bare `&` had to start becoming a valid
    // expression for the retired prefix borrow to superset-parse at all.
    check(
        "static f = fn { a & b; };",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..24
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..24
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..17
                      PATH_EXPR@16..17
                        NAME_REF@16..17
                          IDENT@16..17 "a"
                    WHITESPACE@17..18 " "
                    EXPR_STMT@18..22
                      BORROW_EXPR@18..21
                        AMP@18..19 "&"
                        WHITESPACE@19..20 " "
                        PATH_EXPR@20..21
                          NAME_REF@20..21
                            IDENT@20..21 "b"
                      SEMICOLON@21..22 ";"
                    WHITESPACE@22..23 " "
                    R_BRACE@23..24 "}"
                SEMICOLON@24..25 ";"
            error 16..17: expected `;`
            error 18..21: borrows are spelled postfix: `x.&` / `x.&mut`
        "#]],
    );
}

#[test]
fn a_bare_quote_is_an_unterminated_character_literal() {
    // The `'`-prefixed lifetime kind is retired (G26) and `'` now opens a
    // character literal. A lone one is therefore an UNTERMINATED literal,
    // not an unknown character — and the scan is bounded to the line
    // (unlike a string's), so this single odd quote costs exactly one token
    // and the identifier after it lexes and parses on its own, exactly as
    // it did when `'` was a stray character. The bound survives a trailing
    // backslash and does not survive a SECOND odd quote on the same line;
    // both cases have tests of their own.
    check(
        "static x = fn { 'a; };",
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
                FN_LITERAL@11..21
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..21
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..17
                      LITERAL@16..17
                        CHAR@16..17 "'"
                    EXPR_STMT@17..19
                      PATH_EXPR@17..18
                        NAME_REF@17..18
                          IDENT@17..18 "a"
                      SEMICOLON@18..19 ";"
                    WHITESPACE@19..20 " "
                    R_BRACE@20..21 "}"
                SEMICOLON@21..22 ";"
            error 16..17: unterminated character literal: expected a closing `'`
        "#]],
    );
}

#[test]
fn safe_borrow_expr() {
    // `x.&` / `x.&mut` (DOT AMP without `raw`) — the postfix safe borrows.
    // Their own node, no reservation error; hir owns them from here.
    check(
        "static f = fn { let a = x.&; let b = y.&mut; };",
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
                    LET_STMT@16..28
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      BORROW_EXPR@24..27
                        PATH_EXPR@24..25
                          NAME_REF@24..25
                            IDENT@24..25 "x"
                        DOT@25..26 "."
                        AMP@26..27 "&"
                      SEMICOLON@27..28 ";"
                    WHITESPACE@28..29 " "
                    LET_STMT@29..44
                      LET_KW@29..32 "let"
                      WHITESPACE@32..33 " "
                      BIND_PAT@33..34
                        NAME@33..34
                          IDENT@33..34 "b"
                      WHITESPACE@34..35 " "
                      EQ@35..36 "="
                      WHITESPACE@36..37 " "
                      BORROW_EXPR@37..43
                        PATH_EXPR@37..38
                          NAME_REF@37..38
                            IDENT@37..38 "y"
                        DOT@38..39 "."
                        AMP@39..40 "&"
                        MUT_KW@40..43 "mut"
                      SEMICOLON@43..44 ";"
                    WHITESPACE@44..45 " "
                    R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
        "#]],
    );
}

#[test]
fn safe_borrow_type() {
    check(
        "static f = fn(a: usize.&::<@a>, b: usize.&mut::<@_>) -> () { };",
        expect![[r#"
            SOURCE_FILE@0..63
              STATIC_ITEM@0..63
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..62
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..52
                    L_PAREN@13..14 "("
                    PARAM@14..30
                      BIND_PAT@14..15
                        NAME@14..15
                          IDENT@14..15 "a"
                      COLON@15..16 ":"
                      WHITESPACE@16..17 " "
                      BORROW_TYPE@17..30
                        PATH_TYPE@17..22
                          NAME_REF@17..22
                            IDENT@17..22 "usize"
                        DOT@22..23 "."
                        AMP@23..24 "&"
                        COLON2@24..26 "::"
                        GENERIC_ARG_LIST@26..30
                          L_ANGLE@26..27 "<"
                          REGION_ARG@27..29
                            REGION_IDENT@27..29 "@a"
                          R_ANGLE@29..30 ">"
                    COMMA@30..31 ","
                    WHITESPACE@31..32 " "
                    PARAM@32..51
                      BIND_PAT@32..33
                        NAME@32..33
                          IDENT@32..33 "b"
                      COLON@33..34 ":"
                      WHITESPACE@34..35 " "
                      BORROW_TYPE@35..51
                        PATH_TYPE@35..40
                          NAME_REF@35..40
                            IDENT@35..40 "usize"
                        DOT@40..41 "."
                        AMP@41..42 "&"
                        MUT_KW@42..45 "mut"
                        COLON2@45..47 "::"
                        GENERIC_ARG_LIST@47..51
                          L_ANGLE@47..48 "<"
                          REGION_ARG@48..50
                            REGION_IDENT@48..50 "@_"
                          R_ANGLE@50..51 ">"
                    R_PAREN@51..52 ")"
                  WHITESPACE@52..53 " "
                  RET_TYPE@53..58
                    THIN_ARROW@53..55 "->"
                    WHITESPACE@55..56 " "
                    UNIT_TYPE@56..58
                      L_PAREN@56..57 "("
                      R_PAREN@57..58 ")"
                  WHITESPACE@58..59 " "
                  BLOCK_EXPR@59..62
                    L_BRACE@59..60 "{"
                    WHITESPACE@60..61 " "
                    R_BRACE@61..62 "}"
                SEMICOLON@62..63 ";"
        "#]],
    );
}
