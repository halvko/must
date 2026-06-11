use expect_test::{Expect, expect};

fn check(input: &str, expect: Expect) {
    let parse = crate::parse(input);
    expect.assert_eq(&parse.debug_dump());
}

#[test]
fn empty_file() {
    check("", expect![[r#"
        SOURCE_FILE@0..0
    "#]]);
}

#[test]
fn hello_annotated() {
    check(
        r#"
static main: fn() -> () = fn() -> () {
    let s: &'static str = "hello";
    (fn () -> () print(s))();
}
"#,
        expect![[r#"
            SOURCE_FILE@0..107
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..106
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                COLON@12..13 ":"
                WHITESPACE@13..14 " "
                FN_TYPE@14..24
                  FN_KW@14..16 "fn"
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
                FN_LITERAL@27..106
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
                  BLOCK_EXPR@38..106
                    L_BRACE@38..39 "{"
                    WHITESPACE@39..44 "\n    "
                    LET_STMT@44..74
                      LET_KW@44..47 "let"
                      WHITESPACE@47..48 " "
                      NAME@48..49
                        IDENT@48..49 "s"
                      COLON@49..50 ":"
                      WHITESPACE@50..51 " "
                      REF_TYPE@51..63
                        AMP@51..52 "&"
                        LIFETIME_IDENT@52..59 "'static"
                        WHITESPACE@59..60 " "
                        PATH_TYPE@60..63
                          NAME_REF@60..63
                            IDENT@60..63 "str"
                      WHITESPACE@63..64 " "
                      EQ@64..65 "="
                      WHITESPACE@65..66 " "
                      LITERAL@66..73
                        STRING@66..73 "\"hello\""
                      SEMICOLON@73..74 ";"
                    WHITESPACE@74..79 "\n    "
                    EXPR_STMT@79..104
                      CALL_EXPR@79..103
                        PAREN_EXPR@79..101
                          L_PAREN@79..80 "("
                          FN_LITERAL@80..100
                            FN_KW@80..82 "fn"
                            WHITESPACE@82..83 " "
                            PARAM_LIST@83..85
                              L_PAREN@83..84 "("
                              R_PAREN@84..85 ")"
                            WHITESPACE@85..86 " "
                            RET_TYPE@86..91
                              THIN_ARROW@86..88 "->"
                              WHITESPACE@88..89 " "
                              UNIT_TYPE@89..91
                                L_PAREN@89..90 "("
                                R_PAREN@90..91 ")"
                            WHITESPACE@91..92 " "
                            CALL_EXPR@92..100
                              PATH_EXPR@92..97
                                NAME_REF@92..97
                                  IDENT@92..97 "print"
                              ARG_LIST@97..100
                                L_PAREN@97..98 "("
                                PATH_EXPR@98..99
                                  NAME_REF@98..99
                                    IDENT@98..99 "s"
                                R_PAREN@99..100 ")"
                          R_PAREN@100..101 ")"
                        ARG_LIST@101..103
                          L_PAREN@101..102 "("
                          R_PAREN@102..103 ")"
                      SEMICOLON@103..104 ";"
                    WHITESPACE@104..105 "\n"
                    R_BRACE@105..106 "}"
              WHITESPACE@106..107 "\n"
        "#]],
    );
}

#[test]
fn hello_minimal() {
    check(
        r#"
static main = fn {
    let s = "hello";
    (fn print(s))();
}
"#,
        expect![[r#"
            SOURCE_FILE@0..64
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..63
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..63
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..63
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..40
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      NAME@28..29
                        IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..39
                        STRING@32..39 "\"hello\""
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..45 "\n    "
                    EXPR_STMT@45..61
                      CALL_EXPR@45..60
                        PAREN_EXPR@45..58
                          L_PAREN@45..46 "("
                          FN_LITERAL@46..57
                            FN_KW@46..48 "fn"
                            WHITESPACE@48..49 " "
                            CALL_EXPR@49..57
                              PATH_EXPR@49..54
                                NAME_REF@49..54
                                  IDENT@49..54 "print"
                              ARG_LIST@54..57
                                L_PAREN@54..55 "("
                                PATH_EXPR@55..56
                                  NAME_REF@55..56
                                    IDENT@55..56 "s"
                                R_PAREN@56..57 ")"
                          R_PAREN@57..58 ")"
                        ARG_LIST@58..60
                          L_PAREN@58..59 "("
                          R_PAREN@59..60 ")"
                      SEMICOLON@60..61 ";"
                    WHITESPACE@61..62 "\n"
                    R_BRACE@62..63 "}"
              WHITESPACE@63..64 "\n"
        "#]],
    );
}

#[test]
fn curried_bs() {
    check(
        r#"
static main = fn {
    let s = "hello";
    (fn print)()(s);
}
"#,
        expect![[r#"
            SOURCE_FILE@0..64
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..63
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "main"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..63
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..63
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..40
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      NAME@28..29
                        IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..39
                        STRING@32..39 "\"hello\""
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..45 "\n    "
                    EXPR_STMT@45..61
                      CALL_EXPR@45..60
                        CALL_EXPR@45..57
                          PAREN_EXPR@45..55
                            L_PAREN@45..46 "("
                            FN_LITERAL@46..54
                              FN_KW@46..48 "fn"
                              WHITESPACE@48..49 " "
                              PATH_EXPR@49..54
                                NAME_REF@49..54
                                  IDENT@49..54 "print"
                            R_PAREN@54..55 ")"
                          ARG_LIST@55..57
                            L_PAREN@55..56 "("
                            R_PAREN@56..57 ")"
                        ARG_LIST@57..60
                          L_PAREN@57..58 "("
                          PATH_EXPR@58..59
                            NAME_REF@58..59
                              IDENT@58..59 "s"
                          R_PAREN@59..60 ")"
                      SEMICOLON@60..61 ";"
                    WHITESPACE@61..62 "\n"
                    R_BRACE@62..63 "}"
              WHITESPACE@63..64 "\n"
        "#]],
    );
}

#[test]
fn fn_shorthand_as_arg() {
    check(
        r#"
static example = fn (arg: fn -> usize) {
    arg();
}

static main = fn {
    example(fn 42 + 69)
}
"#,
        expect![[r#"
            SOURCE_FILE@0..101
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..54
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..15
                  IDENT@8..15 "example"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                FN_LITERAL@18..54
                  FN_KW@18..20 "fn"
                  WHITESPACE@20..21 " "
                  PARAM_LIST@21..39
                    L_PAREN@21..22 "("
                    PARAM@22..38
                      NAME@22..25
                        IDENT@22..25 "arg"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      FN_TYPE@27..38
                        FN_KW@27..29 "fn"
                        WHITESPACE@29..30 " "
                        RET_TYPE@30..38
                          THIN_ARROW@30..32 "->"
                          WHITESPACE@32..33 " "
                          PATH_TYPE@33..38
                            NAME_REF@33..38
                              IDENT@33..38 "usize"
                    R_PAREN@38..39 ")"
                  WHITESPACE@39..40 " "
                  BLOCK_EXPR@40..54
                    L_BRACE@40..41 "{"
                    WHITESPACE@41..46 "\n    "
                    EXPR_STMT@46..52
                      CALL_EXPR@46..51
                        PATH_EXPR@46..49
                          NAME_REF@46..49
                            IDENT@46..49 "arg"
                        ARG_LIST@49..51
                          L_PAREN@49..50 "("
                          R_PAREN@50..51 ")"
                      SEMICOLON@51..52 ";"
                    WHITESPACE@52..53 "\n"
                    R_BRACE@53..54 "}"
              WHITESPACE@54..56 "\n\n"
              STATIC_ITEM@56..100
                STATIC_KW@56..62 "static"
                WHITESPACE@62..63 " "
                NAME@63..67
                  IDENT@63..67 "main"
                WHITESPACE@67..68 " "
                EQ@68..69 "="
                WHITESPACE@69..70 " "
                FN_LITERAL@70..100
                  FN_KW@70..72 "fn"
                  WHITESPACE@72..73 " "
                  BLOCK_EXPR@73..100
                    L_BRACE@73..74 "{"
                    WHITESPACE@74..79 "\n    "
                    CALL_EXPR@79..98
                      PATH_EXPR@79..86
                        NAME_REF@79..86
                          IDENT@79..86 "example"
                      ARG_LIST@86..98
                        L_PAREN@86..87 "("
                        FN_LITERAL@87..97
                          FN_KW@87..89 "fn"
                          WHITESPACE@89..90 " "
                          BIN_EXPR@90..97
                            LITERAL@90..92
                              INT_NUMBER@90..92 "42"
                            WHITESPACE@92..93 " "
                            PLUS@93..94 "+"
                            WHITESPACE@94..95 " "
                            LITERAL@95..97
                              INT_NUMBER@95..97 "69"
                        R_PAREN@97..98 ")"
                    WHITESPACE@98..99 "\n"
                    R_BRACE@99..100 "}"
              WHITESPACE@100..101 "\n"
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
        "static diverges: fn() -> ! = fn (s: str) -> ! panic(s);",
        expect![[r#"
            SOURCE_FILE@0..55
              STATIC_ITEM@0..55
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..15
                  IDENT@7..15 "diverges"
                COLON@15..16 ":"
                WHITESPACE@16..17 " "
                FN_TYPE@17..26
                  FN_KW@17..19 "fn"
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
                FN_LITERAL@29..54
                  FN_KW@29..31 "fn"
                  WHITESPACE@31..32 " "
                  PARAM_LIST@32..40
                    L_PAREN@32..33 "("
                    PARAM@33..39
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
                  CALL_EXPR@46..54
                    PATH_EXPR@46..51
                      NAME_REF@46..51
                        IDENT@46..51 "panic"
                    ARG_LIST@51..54
                      L_PAREN@51..52 "("
                      PATH_EXPR@52..53
                        NAME_REF@52..53
                          IDENT@52..53 "s"
                      R_PAREN@53..54 ")"
                SEMICOLON@54..55 ";"
        "#]],
    );
}

// ---- error resilience ----

#[test]
fn item_missing_name_and_body() {
    check(
        "static = fn {",
        expect![[r#"
            SOURCE_FILE@0..13
              STATIC_ITEM@0..13
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
            error 13..13: expected `}`
        "#]],
    );
}

#[test]
fn unterminated_string() {
    check(
        r#"static s = "hello;"#,
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..18
                  STRING@11..18 "\"hello;"
            error 11..18: unterminated string
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
            error 19..20: unexpected character `@`
            error 21..22: unexpected character `%`
            error 19..20: expected an item (`static` or `const`)
            error 21..22: expected an item (`static` or `const`)
            error 23..27: expected an item (`static` or `const`)
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
            error 34..40: expected `}`
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
            error 38..43: expected `;`
        "#]],
    );
}

#[test]
fn fn_paren_ambiguity() {
    // `fn (s)` → parameter list; `fn (s + 1)` → shorthand body.
    check(
        r#"
static takes_param = fn (s) s;
static paren_body = fn (1 + 2);
static empty_params = fn () 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..95
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..31
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..19
                  IDENT@8..19 "takes_param"
                WHITESPACE@19..20 " "
                EQ@20..21 "="
                WHITESPACE@21..22 " "
                FN_LITERAL@22..30
                  FN_KW@22..24 "fn"
                  WHITESPACE@24..25 " "
                  PARAM_LIST@25..28
                    L_PAREN@25..26 "("
                    PARAM@26..27
                      NAME@26..27
                        IDENT@26..27 "s"
                    R_PAREN@27..28 ")"
                  WHITESPACE@28..29 " "
                  PATH_EXPR@29..30
                    NAME_REF@29..30
                      IDENT@29..30 "s"
                SEMICOLON@30..31 ";"
              WHITESPACE@31..32 "\n"
              STATIC_ITEM@32..63
                STATIC_KW@32..38 "static"
                WHITESPACE@38..39 " "
                NAME@39..49
                  IDENT@39..49 "paren_body"
                WHITESPACE@49..50 " "
                EQ@50..51 "="
                WHITESPACE@51..52 " "
                FN_LITERAL@52..62
                  FN_KW@52..54 "fn"
                  WHITESPACE@54..55 " "
                  PAREN_EXPR@55..62
                    L_PAREN@55..56 "("
                    BIN_EXPR@56..61
                      LITERAL@56..57
                        INT_NUMBER@56..57 "1"
                      WHITESPACE@57..58 " "
                      PLUS@58..59 "+"
                      WHITESPACE@59..60 " "
                      LITERAL@60..61
                        INT_NUMBER@60..61 "2"
                    R_PAREN@61..62 ")"
                SEMICOLON@62..63 ";"
              WHITESPACE@63..64 "\n"
              STATIC_ITEM@64..94
                STATIC_KW@64..70 "static"
                WHITESPACE@70..71 " "
                NAME@71..83
                  IDENT@71..83 "empty_params"
                WHITESPACE@83..84 " "
                EQ@84..85 "="
                WHITESPACE@85..86 " "
                FN_LITERAL@86..93
                  FN_KW@86..88 "fn"
                  WHITESPACE@88..89 " "
                  PARAM_LIST@89..91
                    L_PAREN@89..90 "("
                    R_PAREN@90..91 ")"
                  WHITESPACE@91..92 " "
                  LITERAL@92..93
                    INT_NUMBER@92..93 "1"
                SEMICOLON@93..94 ";"
              WHITESPACE@94..95 "\n"
        "#]],
    );
}
