//! `loop`, `break`, `continue` and `return`.

use super::*;

#[test]
fn loop_with_break_and_continue_parses() {
    check(
        r#"
static f = fn {
    loop {
        if done { break; };
        continue;
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..82
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..81
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..81
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  BLOCK_EXPR@15..81
                    L_BRACE@15..16 "{"
                    WHITESPACE@16..21 "\n    "
                    LOOP_EXPR@21..79
                      LOOP_KW@21..25 "loop"
                      WHITESPACE@25..26 " "
                      BLOCK_EXPR@26..79
                        L_BRACE@26..27 "{"
                        WHITESPACE@27..36 "\n        "
                        EXPR_STMT@36..55
                          IF_EXPR@36..54
                            IF_KW@36..38 "if"
                            WHITESPACE@38..39 " "
                            PATH_EXPR@39..43
                              NAME_REF@39..43
                                IDENT@39..43 "done"
                            WHITESPACE@43..44 " "
                            BLOCK_EXPR@44..54
                              L_BRACE@44..45 "{"
                              WHITESPACE@45..46 " "
                              EXPR_STMT@46..52
                                BREAK_EXPR@46..51
                                  BREAK_KW@46..51 "break"
                                SEMICOLON@51..52 ";"
                              WHITESPACE@52..53 " "
                              R_BRACE@53..54 "}"
                          SEMICOLON@54..55 ";"
                        WHITESPACE@55..64 "\n        "
                        EXPR_STMT@64..73
                          CONTINUE_EXPR@64..72
                            CONTINUE_KW@64..72 "continue"
                          SEMICOLON@72..73 ";"
                        WHITESPACE@73..78 "\n    "
                        R_BRACE@78..79 "}"
                    WHITESPACE@79..80 "\n"
                    R_BRACE@80..81 "}"
              WHITESPACE@81..82 "\n"
        "#]],
    );
}

#[test]
fn break_with_value_parses() {
    check(
        "static f = fn { loop { break 1 + 2; } }",
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
                FN_LITERAL@11..39
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..39
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LOOP_EXPR@16..37
                      LOOP_KW@16..20 "loop"
                      WHITESPACE@20..21 " "
                      BLOCK_EXPR@21..37
                        L_BRACE@21..22 "{"
                        WHITESPACE@22..23 " "
                        EXPR_STMT@23..35
                          BREAK_EXPR@23..34
                            BREAK_KW@23..28 "break"
                            WHITESPACE@28..29 " "
                            BIN_EXPR@29..34
                              LITERAL@29..30
                                INT_NUMBER@29..30 "1"
                              WHITESPACE@30..31 " "
                              PLUS@31..32 "+"
                              WHITESPACE@32..33 " "
                              LITERAL@33..34
                                INT_NUMBER@33..34 "2"
                          SEMICOLON@34..35 ";"
                        WHITESPACE@35..36 " "
                        R_BRACE@36..37 "}"
                    WHITESPACE@37..38 " "
                    R_BRACE@38..39 "}"
        "#]],
    );
}

#[test]
fn break_loop_pathology_parses() {
    // `break` takes any expression as its value, so `break loop { ... }`
    // falls out of the grammar (the inner loop is the carried value).
    check(
        "static f = fn { loop { break loop { break 1; }; } }",
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
                FN_LITERAL@11..51
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..51
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LOOP_EXPR@16..49
                      LOOP_KW@16..20 "loop"
                      WHITESPACE@20..21 " "
                      BLOCK_EXPR@21..49
                        L_BRACE@21..22 "{"
                        WHITESPACE@22..23 " "
                        EXPR_STMT@23..47
                          BREAK_EXPR@23..46
                            BREAK_KW@23..28 "break"
                            WHITESPACE@28..29 " "
                            LOOP_EXPR@29..46
                              LOOP_KW@29..33 "loop"
                              WHITESPACE@33..34 " "
                              BLOCK_EXPR@34..46
                                L_BRACE@34..35 "{"
                                WHITESPACE@35..36 " "
                                EXPR_STMT@36..44
                                  BREAK_EXPR@36..43
                                    BREAK_KW@36..41 "break"
                                    WHITESPACE@41..42 " "
                                    LITERAL@42..43
                                      INT_NUMBER@42..43 "1"
                                  SEMICOLON@43..44 ";"
                                WHITESPACE@44..45 " "
                                R_BRACE@45..46 "}"
                          SEMICOLON@46..47 ";"
                        WHITESPACE@47..48 " "
                        R_BRACE@48..49 "}"
                    WHITESPACE@49..50 " "
                    R_BRACE@50..51 "}"
        "#]],
    );
}

#[test]
fn dangling_break_at_top_level_parses() {
    // Grammar-clean: `break` outside a loop is hir's error, not a parse
    // error.
    check(
        "static x = break 1;",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BREAK_EXPR@11..18
                  BREAK_KW@11..16 "break"
                  WHITESPACE@16..17 " "
                  LITERAL@17..18
                    INT_NUMBER@17..18 "1"
                SEMICOLON@18..19 ";"
        "#]],
    );
}

// ---- `return` ---------------------------------------------------------

#[test]
fn bare_return_statement_parses() {
    // No value: the `;` must not be swallowed hunting for one (the same
    // `at_expr_start` gate `break` uses).
    check(
        "static f = fn { return; }",
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
                FN_LITERAL@11..25
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..25
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..23
                      RETURN_EXPR@16..22
                        RETURN_KW@16..22 "return"
                      SEMICOLON@22..23 ";"
                    WHITESPACE@23..24 " "
                    R_BRACE@24..25 "}"
        "#]],
    );
}

#[test]
fn return_with_a_value_parses() {
    check(
        "static f = fn { return 1 + 2; }",
        expect![[r#"
            SOURCE_FILE@0..31
              STATIC_ITEM@0..31
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..31
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..31
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..29
                      RETURN_EXPR@16..28
                        RETURN_KW@16..22 "return"
                        WHITESPACE@22..23 " "
                        BIN_EXPR@23..28
                          LITERAL@23..24
                            INT_NUMBER@23..24 "1"
                          WHITESPACE@24..25 " "
                          PLUS@25..26 "+"
                          WHITESPACE@26..27 " "
                          LITERAL@27..28
                            INT_NUMBER@27..28 "2"
                      SEMICOLON@28..29 ";"
                    WHITESPACE@29..30 " "
                    R_BRACE@30..31 "}"
        "#]],
    );
}

#[test]
fn return_as_a_block_tail_parses() {
    // No `;`: `return` is an expression, so it is the block's tail — no
    // EXPR_STMT wrapper.
    check(
        "static f = fn { return 1 }",
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
                FN_LITERAL@11..26
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..26
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    RETURN_EXPR@16..24
                      RETURN_KW@16..22 "return"
                      WHITESPACE@22..23 " "
                      LITERAL@23..24
                        INT_NUMBER@23..24 "1"
                    WHITESPACE@24..25 " "
                    R_BRACE@25..26 "}"
        "#]],
    );
}

#[test]
fn return_inside_if_match_and_loop_parses() {
    check(
        r#"
static f = fn (s: Shape) -> usize {
    loop {
        if done { return 0; };
        match s { ::Circle => return 1, ::Square => 2 };
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..144
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..143
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..143
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  PARAM_LIST@15..25
                    L_PAREN@15..16 "("
                    PARAM@16..24
                      BIND_PAT@16..17
                        NAME@16..17
                          IDENT@16..17 "s"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..24
                        NAME_REF@19..24
                          IDENT@19..24 "Shape"
                    R_PAREN@24..25 ")"
                  WHITESPACE@25..26 " "
                  RET_TYPE@26..34
                    THIN_ARROW@26..28 "->"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..34
                      NAME_REF@29..34
                        IDENT@29..34 "usize"
                  WHITESPACE@34..35 " "
                  BLOCK_EXPR@35..143
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    LOOP_EXPR@41..141
                      LOOP_KW@41..45 "loop"
                      WHITESPACE@45..46 " "
                      BLOCK_EXPR@46..141
                        L_BRACE@46..47 "{"
                        WHITESPACE@47..56 "\n        "
                        EXPR_STMT@56..78
                          IF_EXPR@56..77
                            IF_KW@56..58 "if"
                            WHITESPACE@58..59 " "
                            PATH_EXPR@59..63
                              NAME_REF@59..63
                                IDENT@59..63 "done"
                            WHITESPACE@63..64 " "
                            BLOCK_EXPR@64..77
                              L_BRACE@64..65 "{"
                              WHITESPACE@65..66 " "
                              EXPR_STMT@66..75
                                RETURN_EXPR@66..74
                                  RETURN_KW@66..72 "return"
                                  WHITESPACE@72..73 " "
                                  LITERAL@73..74
                                    INT_NUMBER@73..74 "0"
                                SEMICOLON@74..75 ";"
                              WHITESPACE@75..76 " "
                              R_BRACE@76..77 "}"
                          SEMICOLON@77..78 ";"
                        WHITESPACE@78..87 "\n        "
                        EXPR_STMT@87..135
                          MATCH_EXPR@87..134
                            MATCH_KW@87..92 "match"
                            WHITESPACE@92..93 " "
                            PATH_EXPR@93..94
                              NAME_REF@93..94
                                IDENT@93..94 "s"
                            WHITESPACE@94..95 " "
                            L_BRACE@95..96 "{"
                            WHITESPACE@96..97 " "
                            MATCH_ARM@97..118
                              VARIANT_PAT@97..105
                                COLON2@97..99 "::"
                                NAME_REF@99..105
                                  IDENT@99..105 "Circle"
                              WHITESPACE@105..106 " "
                              FAT_ARROW@106..108 "=>"
                              WHITESPACE@108..109 " "
                              RETURN_EXPR@109..117
                                RETURN_KW@109..115 "return"
                                WHITESPACE@115..116 " "
                                LITERAL@116..117
                                  INT_NUMBER@116..117 "1"
                              COMMA@117..118 ","
                            WHITESPACE@118..119 " "
                            MATCH_ARM@119..132
                              VARIANT_PAT@119..127
                                COLON2@119..121 "::"
                                NAME_REF@121..127
                                  IDENT@121..127 "Square"
                              WHITESPACE@127..128 " "
                              FAT_ARROW@128..130 "=>"
                              WHITESPACE@130..131 " "
                              LITERAL@131..132
                                INT_NUMBER@131..132 "2"
                            WHITESPACE@132..133 " "
                            R_BRACE@133..134 "}"
                          SEMICOLON@134..135 ";"
                        WHITESPACE@135..140 "\n    "
                        R_BRACE@140..141 "}"
                    WHITESPACE@141..142 "\n"
                    R_BRACE@142..143 "}"
              WHITESPACE@143..144 "\n"
        "#]],
    );
}

#[test]
fn return_in_expression_position_parses() {
    // The whole point of `return` being an expression: it composes as an
    // `if` branch's value.
    check(
        "static f = fn (c: bool) -> usize { let x = if c { 1 } else { return 0 }; x }",
        expect![[r#"
            SOURCE_FILE@0..76
              STATIC_ITEM@0..76
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..76
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..23
                    L_PAREN@14..15 "("
                    PARAM@15..22
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "c"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      PATH_TYPE@18..22
                        NAME_REF@18..22
                          IDENT@18..22 "bool"
                    R_PAREN@22..23 ")"
                  WHITESPACE@23..24 " "
                  RET_TYPE@24..32
                    THIN_ARROW@24..26 "->"
                    WHITESPACE@26..27 " "
                    PATH_TYPE@27..32
                      NAME_REF@27..32
                        IDENT@27..32 "usize"
                  WHITESPACE@32..33 " "
                  BLOCK_EXPR@33..76
                    L_BRACE@33..34 "{"
                    WHITESPACE@34..35 " "
                    LET_STMT@35..72
                      LET_KW@35..38 "let"
                      WHITESPACE@38..39 " "
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "x"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      IF_EXPR@43..71
                        IF_KW@43..45 "if"
                        WHITESPACE@45..46 " "
                        PATH_EXPR@46..47
                          NAME_REF@46..47
                            IDENT@46..47 "c"
                        WHITESPACE@47..48 " "
                        BLOCK_EXPR@48..53
                          L_BRACE@48..49 "{"
                          WHITESPACE@49..50 " "
                          LITERAL@50..51
                            INT_NUMBER@50..51 "1"
                          WHITESPACE@51..52 " "
                          R_BRACE@52..53 "}"
                        WHITESPACE@53..54 " "
                        ELSE_KW@54..58 "else"
                        WHITESPACE@58..59 " "
                        BLOCK_EXPR@59..71
                          L_BRACE@59..60 "{"
                          WHITESPACE@60..61 " "
                          RETURN_EXPR@61..69
                            RETURN_KW@61..67 "return"
                            WHITESPACE@67..68 " "
                            LITERAL@68..69
                              INT_NUMBER@68..69 "0"
                          WHITESPACE@69..70 " "
                          R_BRACE@70..71 "}"
                      SEMICOLON@71..72 ";"
                    WHITESPACE@72..73 " "
                    PATH_EXPR@73..74
                      NAME_REF@73..74
                        IDENT@73..74 "x"
                    WHITESPACE@74..75 " "
                    R_BRACE@75..76 "}"
        "#]],
    );
}

#[test]
fn dangling_return_at_top_level_parses() {
    // Grammar-clean, exactly like a dangling `break`: `return` outside any
    // fn body is hir's error, not a parse error.
    check(
        "static x = return 1;",
        expect![[r#"
            SOURCE_FILE@0..20
              STATIC_ITEM@0..20
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RETURN_EXPR@11..19
                  RETURN_KW@11..17 "return"
                  WHITESPACE@17..18 " "
                  LITERAL@18..19
                    INT_NUMBER@18..19 "1"
                SEMICOLON@19..20 ";"
        "#]],
    );
}

#[test]
fn loop_body_must_be_a_block() {
    // Superset parsing, same as `if` branches: the expression body keeps
    // the user's intent in the tree, validation rejects it with a fix.
    check(
        "static f = fn { loop 5 }",
        expect![[r#"
            SOURCE_FILE@0..24
              STATIC_ITEM@0..24
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
                    LOOP_EXPR@16..22
                      LOOP_KW@16..20 "loop"
                      WHITESPACE@20..21 " "
                      LITERAL@21..22
                        INT_NUMBER@21..22 "5"
                    WHITESPACE@22..23 " "
                    R_BRACE@23..24 "}"
            error 21..22: `loop` bodies are blocks; wrap this expression in `{ }`
        "#]],
    );
}
