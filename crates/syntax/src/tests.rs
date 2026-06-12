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
    (fn () -> () { print(s) })();
}
"#,
        expect![[r#"
            SOURCE_FILE@0..111
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..110
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
                FN_LITERAL@27..110
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
                  BLOCK_EXPR@38..110
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
                    EXPR_STMT@79..108
                      CALL_EXPR@79..107
                        PAREN_EXPR@79..105
                          L_PAREN@79..80 "("
                          FN_LITERAL@80..104
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
                            BLOCK_EXPR@92..104
                              L_BRACE@92..93 "{"
                              WHITESPACE@93..94 " "
                              CALL_EXPR@94..102
                                PATH_EXPR@94..99
                                  NAME_REF@94..99
                                    IDENT@94..99 "print"
                                ARG_LIST@99..102
                                  L_PAREN@99..100 "("
                                  PATH_EXPR@100..101
                                    NAME_REF@100..101
                                      IDENT@100..101 "s"
                                  R_PAREN@101..102 ")"
                              WHITESPACE@102..103 " "
                              R_BRACE@103..104 "}"
                          R_PAREN@104..105 ")"
                        ARG_LIST@105..107
                          L_PAREN@105..106 "("
                          R_PAREN@106..107 ")"
                      SEMICOLON@107..108 ";"
                    WHITESPACE@108..109 "\n"
                    R_BRACE@109..110 "}"
              WHITESPACE@110..111 "\n"
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
                      NAME@22..25
                        IDENT@22..25 "arg"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      FN_TYPE@27..40
                        FN_KW@27..29 "fn"
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
        "static diverges: fn() -> ! = fn (s: str) -> ! { panic(s) }",
        expect![[r#"
            SOURCE_FILE@0..58
              STATIC_ITEM@0..58
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
                FN_LITERAL@29..58
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
            error 14..20: expected `;`
        "#]],
    );
}

#[test]
fn fn_type_requires_parens() {
    check(
        "static f: fn -> usize = fn () -> usize { 1 }",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                FN_TYPE@10..21
                  FN_KW@10..12 "fn"
                  WHITESPACE@12..13 " "
                  RET_TYPE@13..21
                    THIN_ARROW@13..15 "->"
                    WHITESPACE@15..16 " "
                    PATH_TYPE@16..21
                      NAME_REF@16..21
                        IDENT@16..21 "usize"
                WHITESPACE@21..22 " "
                EQ@22..23 "="
                WHITESPACE@23..24 " "
                FN_LITERAL@24..44
                  FN_KW@24..26 "fn"
                  WHITESPACE@26..27 " "
                  PARAM_LIST@27..29
                    L_PAREN@27..28 "("
                    R_PAREN@28..29 ")"
                  WHITESPACE@29..30 " "
                  RET_TYPE@30..38
                    THIN_ARROW@30..32 "->"
                    WHITESPACE@32..33 " "
                    PATH_TYPE@33..38
                      NAME_REF@33..38
                        IDENT@33..38 "usize"
                  WHITESPACE@38..39 " "
                  BLOCK_EXPR@39..44
                    L_BRACE@39..40 "{"
                    WHITESPACE@40..41 " "
                    LITERAL@41..42
                      INT_NUMBER@41..42 "1"
                    WHITESPACE@42..43 " "
                    R_BRACE@43..44 "}"
            error 13..15: expected `(`: function types are written `fn(...) -> ...`
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
            error 13..13: expected `;`
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
            error 18..18: expected `;`
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
            error 34..40: expected `;`
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
              STATIC_ITEM@35..65
                STATIC_KW@35..41 "static"
                WHITESPACE@41..42 " "
                NAME@42..60
                  IDENT@42..60 "shorthand_is_error"
                WHITESPACE@60..61 " "
                EQ@61..62 "="
                WHITESPACE@62..63 " "
                FN_LITERAL@63..65
                  FN_KW@63..65 "fn"
              WHITESPACE@65..66 " "
              ERROR@66..68
                INT_NUMBER@66..68 "42"
              ERROR@68..69
                SEMICOLON@68..69 ";"
              WHITESPACE@69..70 "\n"
              STATIC_ITEM@70..103
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
            error 66..68: expected `{`: function bodies are blocks
            error 66..68: expected `;`
            error 66..68: expected an item (`static` or `const`)
            error 68..69: expected an item (`static` or `const`)
        "#]],
    );
}
