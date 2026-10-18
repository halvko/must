//! Whole small programs: items, fn literals, precedence.

use super::*;

#[test]
fn empty_file() {
    check(
        "",
        expect![[r#"
        SOURCE_FILE@0..0
    "#]],
    );
}

#[test]
fn hello_annotated() {
    check(
        r#"
static main: fn() -> () = fn() -> () {
    let s: str = "hello";
    (fn () -> () { print(s) })();
}
"#,
        expect![[r#"
            SOURCE_FILE@0..102
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..101
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                COLON@12..13 ":"
                WHITESPACE@13..14 " "
                FN_TYPE@14..24
                  FN_KW@14..16 "fn"
                  PARAM_LIST@16..18
                    L_PAREN@16..17 "("
                    R_PAREN@17..18 ")"
                  WHITESPACE@18..19 " "
                  RET_TYPE@19..24
                    THIN_ARROW@19..21 "->"
                    WHITESPACE@21..22 " "
                    UNIT_TYPE@22..24
                      L_PAREN@22..23 "("
                      R_PAREN@23..24 ")"
                WHITESPACE@24..25 " "
                EQ@25..26 "="
                WHITESPACE@26..27 " "
                FN_LITERAL@27..101
                  FN_KW@27..29 "fn"
                  PARAM_LIST@29..31
                    L_PAREN@29..30 "("
                    R_PAREN@30..31 ")"
                  WHITESPACE@31..32 " "
                  RET_TYPE@32..37
                    THIN_ARROW@32..34 "->"
                    WHITESPACE@34..35 " "
                    UNIT_TYPE@35..37
                      L_PAREN@35..36 "("
                      R_PAREN@36..37 ")"
                  WHITESPACE@37..38 " "
                  BLOCK_EXPR@38..101
                    L_BRACE@38..39 "{"
                    WHITESPACE@39..44 "\n    "
                    LET_STMT@44..65
                      LET_KW@44..47 "let"
                      WHITESPACE@47..48 " "
                      BIND_PAT@48..49
                        NAME@48..49
                          IDENT@48..49 "s"
                      COLON@49..50 ":"
                      WHITESPACE@50..51 " "
                      PATH_TYPE@51..54
                        NAME_REF@51..54
                          IDENT@51..54 "str"
                      WHITESPACE@54..55 " "
                      EQ@55..56 "="
                      WHITESPACE@56..57 " "
                      LITERAL@57..64
                        STRING@57..64 "\"hello\""
                      SEMICOLON@64..65 ";"
                    WHITESPACE@65..70 "\n    "
                    EXPR_STMT@70..99
                      CALL_EXPR@70..98
                        PAREN_EXPR@70..96
                          L_PAREN@70..71 "("
                          FN_LITERAL@71..95
                            FN_KW@71..73 "fn"
                            WHITESPACE@73..74 " "
                            PARAM_LIST@74..76
                              L_PAREN@74..75 "("
                              R_PAREN@75..76 ")"
                            WHITESPACE@76..77 " "
                            RET_TYPE@77..82
                              THIN_ARROW@77..79 "->"
                              WHITESPACE@79..80 " "
                              UNIT_TYPE@80..82
                                L_PAREN@80..81 "("
                                R_PAREN@81..82 ")"
                            WHITESPACE@82..83 " "
                            BLOCK_EXPR@83..95
                              L_BRACE@83..84 "{"
                              WHITESPACE@84..85 " "
                              CALL_EXPR@85..93
                                PATH_EXPR@85..90
                                  NAME_REF@85..90
                                    IDENT@85..90 "print"
                                ARG_LIST@90..93
                                  L_PAREN@90..91 "("
                                  PATH_EXPR@91..92
                                    NAME_REF@91..92
                                      IDENT@91..92 "s"
                                  R_PAREN@92..93 ")"
                              WHITESPACE@93..94 " "
                              R_BRACE@94..95 "}"
                          R_PAREN@95..96 ")"
                        ARG_LIST@96..98
                          L_PAREN@96..97 "("
                          R_PAREN@97..98 ")"
                      SEMICOLON@98..99 ";"
                    WHITESPACE@99..100 "\n"
                    R_BRACE@100..101 "}"
              WHITESPACE@101..102 "\n"
        "#]],
    );
}

#[test]
fn hello_minimal() {
    check(
        r#"
static main = fn {
    let s = "hello";
    (fn { print(s) })();
}
"#,
        expect![[r#"
            SOURCE_FILE@0..68
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..67
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..67
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..67
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..40
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..39
                        STRING@32..39 "\"hello\""
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..45 "\n    "
                    EXPR_STMT@45..65
                      CALL_EXPR@45..64
                        PAREN_EXPR@45..62
                          L_PAREN@45..46 "("
                          FN_LITERAL@46..61
                            FN_KW@46..48 "fn"
                            WHITESPACE@48..49 " "
                            BLOCK_EXPR@49..61
                              L_BRACE@49..50 "{"
                              WHITESPACE@50..51 " "
                              CALL_EXPR@51..59
                                PATH_EXPR@51..56
                                  NAME_REF@51..56
                                    IDENT@51..56 "print"
                                ARG_LIST@56..59
                                  L_PAREN@56..57 "("
                                  PATH_EXPR@57..58
                                    NAME_REF@57..58
                                      IDENT@57..58 "s"
                                  R_PAREN@58..59 ")"
                              WHITESPACE@59..60 " "
                              R_BRACE@60..61 "}"
                          R_PAREN@61..62 ")"
                        ARG_LIST@62..64
                          L_PAREN@62..63 "("
                          R_PAREN@63..64 ")"
                      SEMICOLON@64..65 ";"
                    WHITESPACE@65..66 "\n"
                    R_BRACE@66..67 "}"
              WHITESPACE@67..68 "\n"
        "#]],
    );
}

#[test]
fn curried_bs() {
    check(
        r#"
static main = fn {
    let s = "hello";
    (fn { print })()(s);
}
"#,
        expect![[r#"
            SOURCE_FILE@0..68
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..67
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..67
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..67
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..40
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..39
                        STRING@32..39 "\"hello\""
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..45 "\n    "
                    EXPR_STMT@45..65
                      CALL_EXPR@45..64
                        CALL_EXPR@45..61
                          PAREN_EXPR@45..59
                            L_PAREN@45..46 "("
                            FN_LITERAL@46..58
                              FN_KW@46..48 "fn"
                              WHITESPACE@48..49 " "
                              BLOCK_EXPR@49..58
                                L_BRACE@49..50 "{"
                                WHITESPACE@50..51 " "
                                PATH_EXPR@51..56
                                  NAME_REF@51..56
                                    IDENT@51..56 "print"
                                WHITESPACE@56..57 " "
                                R_BRACE@57..58 "}"
                            R_PAREN@58..59 ")"
                          ARG_LIST@59..61
                            L_PAREN@59..60 "("
                            R_PAREN@60..61 ")"
                        ARG_LIST@61..64
                          L_PAREN@61..62 "("
                          PATH_EXPR@62..63
                            NAME_REF@62..63
                              IDENT@62..63 "s"
                          R_PAREN@63..64 ")"
                      SEMICOLON@64..65 ";"
                    WHITESPACE@65..66 "\n"
                    R_BRACE@66..67 "}"
              WHITESPACE@67..68 "\n"
        "#]],
    );
}

#[test]
fn fn_block_as_arg() {
    check(
        r#"
static example = fn (arg: fn() -> usize) {
    arg();
}

static main = fn {
    example(fn { 42 + 69 })
}
"#,
        expect![[r#"
            SOURCE_FILE@0..107
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..56
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..15
                  IDENT@8..15 "example"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                FN_LITERAL@18..56
                  FN_KW@18..20 "fn"
                  WHITESPACE@20..21 " "
                  PARAM_LIST@21..41
                    L_PAREN@21..22 "("
                    PARAM@22..40
                      BIND_PAT@22..25
                        NAME@22..25
                          IDENT@22..25 "arg"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      FN_TYPE@27..40
                        FN_KW@27..29 "fn"
                        PARAM_LIST@29..31
                          L_PAREN@29..30 "("
                          R_PAREN@30..31 ")"
                        WHITESPACE@31..32 " "
                        RET_TYPE@32..40
                          THIN_ARROW@32..34 "->"
                          WHITESPACE@34..35 " "
                          PATH_TYPE@35..40
                            NAME_REF@35..40
                              IDENT@35..40 "usize"
                    R_PAREN@40..41 ")"
                  WHITESPACE@41..42 " "
                  BLOCK_EXPR@42..56
                    L_BRACE@42..43 "{"
                    WHITESPACE@43..48 "\n    "
                    EXPR_STMT@48..54
                      CALL_EXPR@48..53
                        PATH_EXPR@48..51
                          NAME_REF@48..51
                            IDENT@48..51 "arg"
                        ARG_LIST@51..53
                          L_PAREN@51..52 "("
                          R_PAREN@52..53 ")"
                      SEMICOLON@53..54 ";"
                    WHITESPACE@54..55 "\n"
                    R_BRACE@55..56 "}"
              WHITESPACE@56..58 "\n\n"
              STATIC_ITEM@58..106
                ITEM_HEAD@58..64
                  STATIC_KW@58..64 "static"
                WHITESPACE@64..65 " "
                NAME@65..69
                  IDENT@65..69 "main"
                WHITESPACE@69..70 " "
                EQ@70..71 "="
                WHITESPACE@71..72 " "
                FN_LITERAL@72..106
                  FN_KW@72..74 "fn"
                  WHITESPACE@74..75 " "
                  BLOCK_EXPR@75..106
                    L_BRACE@75..76 "{"
                    WHITESPACE@76..81 "\n    "
                    CALL_EXPR@81..104
                      PATH_EXPR@81..88
                        NAME_REF@81..88
                          IDENT@81..88 "example"
                      ARG_LIST@88..104
                        L_PAREN@88..89 "("
                        FN_LITERAL@89..103
                          FN_KW@89..91 "fn"
                          WHITESPACE@91..92 " "
                          BLOCK_EXPR@92..103
                            L_BRACE@92..93 "{"
                            WHITESPACE@93..94 " "
                            BIN_EXPR@94..101
                              LITERAL@94..96
                                INT_NUMBER@94..96 "42"
                              WHITESPACE@96..97 " "
                              PLUS@97..98 "+"
                              WHITESPACE@98..99 " "
                              LITERAL@99..101
                                INT_NUMBER@99..101 "69"
                            WHITESPACE@101..102 " "
                            R_BRACE@102..103 "}"
                        R_PAREN@103..104 ")"
                    WHITESPACE@104..105 "\n"
                    R_BRACE@105..106 "}"
              WHITESPACE@106..107 "\n"
        "#]],
    );
}

#[test]
fn binary_expr_precedence() {
    check(
        "const x = 1 + 2 * 3 - 4 / 5;",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                ITEM_HEAD@0..5
                  CONST_KW@0..5 "const"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "x"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                BIN_EXPR@10..27
                  BIN_EXPR@10..19
                    LITERAL@10..11
                      INT_NUMBER@10..11 "1"
                    WHITESPACE@11..12 " "
                    PLUS@12..13 "+"
                    WHITESPACE@13..14 " "
                    BIN_EXPR@14..19
                      LITERAL@14..15
                        INT_NUMBER@14..15 "2"
                      WHITESPACE@15..16 " "
                      STAR@16..17 "*"
                      WHITESPACE@17..18 " "
                      LITERAL@18..19
                        INT_NUMBER@18..19 "3"
                  WHITESPACE@19..20 " "
                  MINUS@20..21 "-"
                  WHITESPACE@21..22 " "
                  BIN_EXPR@22..27
                    LITERAL@22..23
                      INT_NUMBER@22..23 "4"
                    WHITESPACE@23..24 " "
                    SLASH@24..25 "/"
                    WHITESPACE@25..26 " "
                    LITERAL@26..27
                      INT_NUMBER@26..27 "5"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn mutual_recursion() {
    check(
        r#"
const a = fn { b() };
static b = fn { a() };
"#,
        expect![[r#"
            SOURCE_FILE@0..46
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..22
                ITEM_HEAD@1..6
                  CONST_KW@1..6 "const"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "a"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..21
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..21
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    CALL_EXPR@16..19
                      PATH_EXPR@16..17
                        NAME_REF@16..17
                          IDENT@16..17 "b"
                      ARG_LIST@17..19
                        L_PAREN@17..18 "("
                        R_PAREN@18..19 ")"
                    WHITESPACE@19..20 " "
                    R_BRACE@20..21 "}"
                SEMICOLON@21..22 ";"
              WHITESPACE@22..23 "\n"
              STATIC_ITEM@23..45
                ITEM_HEAD@23..29
                  STATIC_KW@23..29 "static"
                WHITESPACE@29..30 " "
                NAME@30..31
                  IDENT@30..31 "b"
                WHITESPACE@31..32 " "
                EQ@32..33 "="
                WHITESPACE@33..34 " "
                FN_LITERAL@34..44
                  FN_KW@34..36 "fn"
                  WHITESPACE@36..37 " "
                  BLOCK_EXPR@37..44
                    L_BRACE@37..38 "{"
                    WHITESPACE@38..39 " "
                    CALL_EXPR@39..42
                      PATH_EXPR@39..40
                        NAME_REF@39..40
                          IDENT@39..40 "a"
                      ARG_LIST@40..42
                        L_PAREN@40..41 "("
                        R_PAREN@41..42 ")"
                    WHITESPACE@42..43 " "
                    R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
              WHITESPACE@45..46 "\n"
        "#]],
    );
}

#[test]
fn never_type_annotation() {
    check(
        "static diverges: fn() -> ! = fn (s: str) -> ! { panic(s) }",
        expect![[r#"
            SOURCE_FILE@0..58
              STATIC_ITEM@0..58
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..15
                  IDENT@7..15 "diverges"
                COLON@15..16 ":"
                WHITESPACE@16..17 " "
                FN_TYPE@17..26
                  FN_KW@17..19 "fn"
                  PARAM_LIST@19..21
                    L_PAREN@19..20 "("
                    R_PAREN@20..21 ")"
                  WHITESPACE@21..22 " "
                  RET_TYPE@22..26
                    THIN_ARROW@22..24 "->"
                    WHITESPACE@24..25 " "
                    NEVER_TYPE@25..26
                      BANG@25..26 "!"
                WHITESPACE@26..27 " "
                EQ@27..28 "="
                WHITESPACE@28..29 " "
                FN_LITERAL@29..58
                  FN_KW@29..31 "fn"
                  WHITESPACE@31..32 " "
                  PARAM_LIST@32..40
                    L_PAREN@32..33 "("
                    PARAM@33..39
                      BIND_PAT@33..34
                        NAME@33..34
                          IDENT@33..34 "s"
                      COLON@34..35 ":"
                      WHITESPACE@35..36 " "
                      PATH_TYPE@36..39
                        NAME_REF@36..39
                          IDENT@36..39 "str"
                    R_PAREN@39..40 ")"
                  WHITESPACE@40..41 " "
                  RET_TYPE@41..45
                    THIN_ARROW@41..43 "->"
                    WHITESPACE@43..44 " "
                    NEVER_TYPE@44..45
                      BANG@44..45 "!"
                  WHITESPACE@45..46 " "
                  BLOCK_EXPR@46..58
                    L_BRACE@46..47 "{"
                    WHITESPACE@47..48 " "
                    CALL_EXPR@48..56
                      PATH_EXPR@48..53
                        NAME_REF@48..53
                          IDENT@48..53 "panic"
                      ARG_LIST@53..56
                        L_PAREN@53..54 "("
                        PATH_EXPR@54..55
                          NAME_REF@54..55
                            IDENT@54..55 "s"
                        R_PAREN@55..56 ")"
                    WHITESPACE@56..57 " "
                    R_BRACE@57..58 "}"
        "#]],
    );
}
