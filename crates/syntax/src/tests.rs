use expect_test::{Expect, expect};

fn check(input: &str, expect: Expect) {
    let parse = crate::parse(input);
    expect.assert_eq(&parse.debug_dump());
}

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
                      BIND_PAT@48..49
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
            error 51..63: references are not supported yet
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
            error 12..13: expected `;`
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
fn bare_static_reports_one_error() {
    // "expected a name" and "expected `=`" both land at the same spot;
    // only the first (most fundamental) survives.
    check(
        "static ",
        expect![[r#"
        SOURCE_FILE@0..7
          STATIC_ITEM@0..6
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
fn multiline_string_literal() {
    // Strings are multiline, Rust-style: a closing quote on a later line is
    // a valid literal, not two unterminated-string errors.
    check(
        r#"
static name = fn {
    let s = "haha
    ";
}
"#,
        expect![[r#"
            SOURCE_FILE@0..47
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..46
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "name"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..46
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..46
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..44
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..43
                        STRING@32..43 "\"haha\n    \""
                      SEMICOLON@43..44 ";"
                    WHITESPACE@44..45 "\n"
                    R_BRACE@45..46 "}"
              WHITESPACE@46..47 "\n"
        "#]],
    );
}

#[test]
fn escaped_quote_does_not_end_the_string() {
    // The token boundary is what escape handling is load-bearing for at the
    // lexer level: `\"` keeps the literal open, so this is ONE string.
    check(
        r#"static s = "a\"b";"#,
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
                LITERAL@11..17
                  STRING@11..17 "\"a\\\"b\""
                SEMICOLON@17..18 ";"
        "#]],
    );
}

#[test]
fn every_known_escape_is_accepted() {
    // The whole conventional set, in one literal: no diagnostics.
    let parse = crate::parse(r#"static s = "\n\t\r\0\\\"";"#);
    assert_eq!(parse.errors(), &[], "known escapes must not be diagnosed");
}

#[test]
fn unknown_escape_is_an_error_at_the_escape() {
    // Anchored at the two-character escape, NOT at the whole literal — the
    // rest of the string is fine and the squiggle should say so.
    check(
        r#"static s = "a\qb";"#,
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
                LITERAL@11..17
                  STRING@11..17 "\"a\\qb\""
                SEMICOLON@17..18 ";"
            error 13..15: unknown escape sequence `\q`
        "#]],
    );
}

#[test]
fn each_bad_escape_in_a_literal_is_reported() {
    let parse = crate::parse(r#"static s = "\q and \z";"#);
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "unknown escape sequence `\\q`",
            "unknown escape sequence `\\z`"
        ]
    );
}

#[test]
fn control_characters_in_the_escape_message_are_rendered_not_embedded() {
    // A backslash before a LITERAL newline (the C line-continuation idiom,
    // which Must does not have) must not split the diagnostic across two
    // lines, and a CR from a CRLF file must not travel raw inside an LSP
    // message. The range still covers the two source characters. Rendered
    // as a codepoint (`\u{a}`), never as `\n` — that two-character spelling
    // IS a valid escape, so naming it here would call something legal
    // unknown.
    let parse = crate::parse("static s = \"a\\\nb\";");
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["unknown escape sequence `\\u{a}`"]);
    assert_eq!(
        parse.errors()[0].range,
        crate::TextRange::new(13.into(), 15.into())
    );

    let parse = crate::parse("static s = \"a\\\r\nb\";");
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["unknown escape sequence `\\u{d}`"]);
    for err in parse.errors() {
        assert!(
            !err.message.contains('\n') && !err.message.contains('\r'),
            "a diagnostic must stay on one line: {:?}",
            err.message
        );
    }
}

#[test]
fn trailing_lone_backslash_is_an_error() {
    // Only reachable at end of input: before a closing quote the backslash
    // would have escaped the quote. Both errors are honest — the string is
    // unterminated *and* ends on a dangling escape.
    check(
        r#"static s = "abc\"#,
        expect![[r#"
            SOURCE_FILE@0..16
              STATIC_ITEM@0..16
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..16
                  STRING@11..16 "\"abc\\"
            error 11..16: unterminated string
            error 15..16: a string cannot end with a lone `\`
        "#]],
    );
}

#[test]
fn escapes_are_legal_inside_a_multiline_string() {
    // Strings stay multiline (unchanged ruling); a literal newline and an
    // escape coexist in one literal, and neither is diagnosed.
    let parse = crate::parse("static s = \"line one\\n\nline two\\t\";");
    assert_eq!(parse.errors(), &[]);
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
            error 19..20: expected a region name after `@` (`@a`, or `@_` to infer one)
            error 21..22: unexpected character `%`
            error 23..27: expected an item (`static`, `const`, `type` or `trait`)
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

#[test]
fn if_else_chain() {
    check(
        "static x = if a { 1 } else if b { 2 } else { 3 };",
        expect![[r#"
            SOURCE_FILE@0..49
              STATIC_ITEM@0..49
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                IF_EXPR@11..48
                  IF_KW@11..13 "if"
                  WHITESPACE@13..14 " "
                  PATH_EXPR@14..15
                    NAME_REF@14..15
                      IDENT@14..15 "a"
                  WHITESPACE@15..16 " "
                  BLOCK_EXPR@16..21
                    L_BRACE@16..17 "{"
                    WHITESPACE@17..18 " "
                    LITERAL@18..19
                      INT_NUMBER@18..19 "1"
                    WHITESPACE@19..20 " "
                    R_BRACE@20..21 "}"
                  WHITESPACE@21..22 " "
                  ELSE_KW@22..26 "else"
                  WHITESPACE@26..27 " "
                  IF_EXPR@27..48
                    IF_KW@27..29 "if"
                    WHITESPACE@29..30 " "
                    PATH_EXPR@30..31
                      NAME_REF@30..31
                        IDENT@30..31 "b"
                    WHITESPACE@31..32 " "
                    BLOCK_EXPR@32..37
                      L_BRACE@32..33 "{"
                      WHITESPACE@33..34 " "
                      LITERAL@34..35
                        INT_NUMBER@34..35 "2"
                      WHITESPACE@35..36 " "
                      R_BRACE@36..37 "}"
                    WHITESPACE@37..38 " "
                    ELSE_KW@38..42 "else"
                    WHITESPACE@42..43 " "
                    BLOCK_EXPR@43..48
                      L_BRACE@43..44 "{"
                      WHITESPACE@44..45 " "
                      LITERAL@45..46
                        INT_NUMBER@45..46 "3"
                      WHITESPACE@46..47 " "
                      R_BRACE@47..48 "}"
                SEMICOLON@48..49 ";"
        "#]],
    );
}

#[test]
fn comparisons_bind_looser_than_arithmetic() {
    check(
        "static x = 1 + 2 == 3 * 4;",
        expect![[r#"
        SOURCE_FILE@0..26
          STATIC_ITEM@0..26
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            BIN_EXPR@11..25
              BIN_EXPR@11..16
                LITERAL@11..12
                  INT_NUMBER@11..12 "1"
                WHITESPACE@12..13 " "
                PLUS@13..14 "+"
                WHITESPACE@14..15 " "
                LITERAL@15..16
                  INT_NUMBER@15..16 "2"
              WHITESPACE@16..17 " "
              EQ2@17..19 "=="
              WHITESPACE@19..20 " "
              BIN_EXPR@20..25
                LITERAL@20..21
                  INT_NUMBER@20..21 "3"
                WHITESPACE@21..22 " "
                STAR@22..23 "*"
                WHITESPACE@23..24 " "
                LITERAL@24..25
                  INT_NUMBER@24..25 "4"
            SEMICOLON@25..26 ";"
    "#]],
    );
}

#[test]
fn bool_literals() {
    check(
        "static x = true; static y = false;",
        expect![[r#"
        SOURCE_FILE@0..34
          STATIC_ITEM@0..16
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            LITERAL@11..15
              TRUE_KW@11..15 "true"
            SEMICOLON@15..16 ";"
          WHITESPACE@16..17 " "
          STATIC_ITEM@17..34
            STATIC_KW@17..23 "static"
            WHITESPACE@23..24 " "
            NAME@24..25
              IDENT@24..25 "y"
            WHITESPACE@25..26 " "
            EQ@26..27 "="
            WHITESPACE@27..28 " "
            LITERAL@28..33
              FALSE_KW@28..33 "false"
            SEMICOLON@33..34 ";"
    "#]],
    );
}

#[test]
fn if_branches_require_blocks_with_wrap_fix() {
    check(
        "static x = if c 1 else 2;",
        expect![[r#"
        SOURCE_FILE@0..25
          STATIC_ITEM@0..25
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
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
              LITERAL@16..17
                INT_NUMBER@16..17 "1"
              WHITESPACE@17..18 " "
              ELSE_KW@18..22 "else"
              WHITESPACE@22..23 " "
              LITERAL@23..24
                INT_NUMBER@23..24 "2"
            SEMICOLON@24..25 ";"
        error 16..17: `if` branches are blocks; wrap this expression in `{ }`
        error 23..24: `else` branches are blocks; wrap this expression in `{ }`
    "#]],
    );
}

#[test]
fn if_with_missing_then_block_before_else_recovers() {
    check(
        "static x = if c else { 2 };",
        expect![[r#"
        SOURCE_FILE@0..27
          STATIC_ITEM@0..27
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            IF_EXPR@11..26
              IF_KW@11..13 "if"
              WHITESPACE@13..14 " "
              PATH_EXPR@14..15
                NAME_REF@14..15
                  IDENT@14..15 "c"
              WHITESPACE@15..16 " "
              ELSE_KW@16..20 "else"
              WHITESPACE@20..21 " "
              BLOCK_EXPR@21..26
                L_BRACE@21..22 "{"
                WHITESPACE@22..23 " "
                LITERAL@23..24
                  INT_NUMBER@23..24 "2"
                WHITESPACE@24..25 " "
                R_BRACE@25..26 "}"
            SEMICOLON@26..27 ";"
        error 16..20: expected `{`: `if` branches are blocks
    "#]],
    );
}

#[test]
fn let_hole_pattern() {
    check(
        "static f = fn { let _ = 5; };",
        expect![[r#"
            SOURCE_FILE@0..29
              STATIC_ITEM@0..29
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
                    LET_STMT@16..26
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          HOLE@20..21 "_"
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
fn param_hole_pattern() {
    check(
        "static f = fn (_: usize) { };",
        expect![[r#"
            SOURCE_FILE@0..29
              STATIC_ITEM@0..29
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
                  PARAM_LIST@14..24
                    L_PAREN@14..15 "("
                    PARAM@15..23
                      BIND_PAT@15..16
                        NAME@15..16
                          HOLE@15..16 "_"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      PATH_TYPE@18..23
                        NAME_REF@18..23
                          IDENT@18..23 "usize"
                    R_PAREN@23..24 ")"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..28
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    R_BRACE@27..28 "}"
                SEMICOLON@28..29 ";"
        "#]],
    );
}

#[test]
fn const_fn_literal_with_params_and_ret_type() {
    check(
        "static f = const fn (n: usize) -> usize { n };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..45
                  CONST_KW@11..16 "const"
                  WHITESPACE@16..17 " "
                  FN_KW@17..19 "fn"
                  WHITESPACE@19..20 " "
                  PARAM_LIST@20..30
                    L_PAREN@20..21 "("
                    PARAM@21..29
                      BIND_PAT@21..22
                        NAME@21..22
                          IDENT@21..22 "n"
                      COLON@22..23 ":"
                      WHITESPACE@23..24 " "
                      PATH_TYPE@24..29
                        NAME_REF@24..29
                          IDENT@24..29 "usize"
                    R_PAREN@29..30 ")"
                  WHITESPACE@30..31 " "
                  RET_TYPE@31..39
                    THIN_ARROW@31..33 "->"
                    WHITESPACE@33..34 " "
                    PATH_TYPE@34..39
                      NAME_REF@34..39
                        IDENT@34..39 "usize"
                  WHITESPACE@39..40 " "
                  BLOCK_EXPR@40..45
                    L_BRACE@40..41 "{"
                    WHITESPACE@41..42 " "
                    PATH_EXPR@42..43
                      NAME_REF@42..43
                        IDENT@42..43 "n"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn const_item_with_bare_const_fn() {
    check(
        "const g = const fn { 1 };",
        expect![[r#"
        SOURCE_FILE@0..25
          STATIC_ITEM@0..25
            CONST_KW@0..5 "const"
            WHITESPACE@5..6 " "
            NAME@6..7
              IDENT@6..7 "g"
            WHITESPACE@7..8 " "
            EQ@8..9 "="
            WHITESPACE@9..10 " "
            FN_LITERAL@10..24
              CONST_KW@10..15 "const"
              WHITESPACE@15..16 " "
              FN_KW@16..18 "fn"
              WHITESPACE@18..19 " "
              BLOCK_EXPR@19..24
                L_BRACE@19..20 "{"
                WHITESPACE@20..21 " "
                LITERAL@21..22
                  INT_NUMBER@21..22 "1"
                WHITESPACE@22..23 " "
                R_BRACE@23..24 "}"
            SEMICOLON@24..25 ";"
    "#]],
    );
}

#[test]
fn const_block_as_initializer() {
    check(
        "static x = const { 1 + 2 };",
        expect![[r#"
        SOURCE_FILE@0..27
          STATIC_ITEM@0..27
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            CONST_BLOCK_EXPR@11..26
              CONST_KW@11..16 "const"
              WHITESPACE@16..17 " "
              BLOCK_EXPR@17..26
                L_BRACE@17..18 "{"
                WHITESPACE@18..19 " "
                BIN_EXPR@19..24
                  LITERAL@19..20
                    INT_NUMBER@19..20 "1"
                  WHITESPACE@20..21 " "
                  PLUS@21..22 "+"
                  WHITESPACE@22..23 " "
                  LITERAL@23..24
                    INT_NUMBER@23..24 "2"
                WHITESPACE@24..25 " "
                R_BRACE@25..26 "}"
            SEMICOLON@26..27 ";"
    "#]],
    );
}

#[test]
fn const_block_nested_in_fn_body() {
    check(
        "static f = fn { let y = const { 2 }; y };",
        expect![[r#"
            SOURCE_FILE@0..41
              STATIC_ITEM@0..41
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
                  BLOCK_EXPR@14..40
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..36
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "y"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      CONST_BLOCK_EXPR@24..35
                        CONST_KW@24..29 "const"
                        WHITESPACE@29..30 " "
                        BLOCK_EXPR@30..35
                          L_BRACE@30..31 "{"
                          WHITESPACE@31..32 " "
                          LITERAL@32..33
                            INT_NUMBER@32..33 "2"
                          WHITESPACE@33..34 " "
                          R_BRACE@34..35 "}"
                      SEMICOLON@35..36 ";"
                    WHITESPACE@36..37 " "
                    PATH_EXPR@37..38
                      NAME_REF@37..38
                        IDENT@37..38 "y"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn const_fn_as_expression_inside_block() {
    check(
        "static f = fn { let g = const fn { 1 }; g() };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
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
                    LET_STMT@16..39
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "g"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      FN_LITERAL@24..38
                        CONST_KW@24..29 "const"
                        WHITESPACE@29..30 " "
                        FN_KW@30..32 "fn"
                        WHITESPACE@32..33 " "
                        BLOCK_EXPR@33..38
                          L_BRACE@33..34 "{"
                          WHITESPACE@34..35 " "
                          LITERAL@35..36
                            INT_NUMBER@35..36 "1"
                          WHITESPACE@36..37 " "
                          R_BRACE@37..38 "}"
                      SEMICOLON@38..39 ";"
                    WHITESPACE@39..40 " "
                    CALL_EXPR@40..43
                      PATH_EXPR@40..41
                        NAME_REF@40..41
                          IDENT@40..41 "g"
                      ARG_LIST@41..43
                        L_PAREN@41..42 "("
                        R_PAREN@42..43 ")"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn const_item_inside_block_still_recovers() {
    check(
        "static f = fn { const x = 5; };",
        expect![[r#"
            SOURCE_FILE@0..31
              STATIC_ITEM@0..15
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..15
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..15
                    L_BRACE@14..15 "{"
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..28
                CONST_KW@16..21 "const"
                WHITESPACE@21..22 " "
                NAME@22..23
                  IDENT@22..23 "x"
                WHITESPACE@23..24 " "
                EQ@24..25 "="
                WHITESPACE@25..26 " "
                LITERAL@26..27
                  INT_NUMBER@26..27 "5"
                SEMICOLON@27..28 ";"
              WHITESPACE@28..29 " "
              ERROR@29..30
                R_BRACE@29..30 "}"
              ERROR@30..31
                SEMICOLON@30..31 ";"
            error 14..15: expected `}`
            error 29..30: expected an item (`static`, `const`, `type` or `trait`)
            error 30..31: expected an item (`static`, `const`, `type` or `trait`)
        "#]],
    );
}

#[test]
fn dangling_const_at_block_end_recovers() {
    check(
        "static f = fn { const };",
        expect![[r#"
            SOURCE_FILE@0..24
              STATIC_ITEM@0..15
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..15
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..15
                    L_BRACE@14..15 "{"
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..21
                CONST_KW@16..21 "const"
              WHITESPACE@21..22 " "
              ERROR@22..23
                R_BRACE@22..23 "}"
              ERROR@23..24
                SEMICOLON@23..24 ";"
            error 14..15: expected `}`
            error 22..23: expected a name for the item
            error 23..24: expected an item (`static`, `const`, `type` or `trait`)
        "#]],
    );
}

#[test]
fn let_mut() {
    check(
        "static f = fn { let mut x = 1; };",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..33
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
fn equality_comparison_is_still_an_expr_stmt() {
    // Regression guard: `==` must not be mistaken for the assignment `=`.
    check(
        "static f = fn { let x = 1; x == 2; };",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
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

#[test]
fn record_type_annotation() {
    check(
        "static p: struct { x: usize, y: usize } = 0;",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
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
            error 25..26: expected an item (`static`, `const`, `type` or `trait`)
            error 26..27: expected an item (`static`, `const`, `type` or `trait`)
            error 27..28: expected an item (`static`, `const`, `type` or `trait`)
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

#[test]
fn type_item() {
    check(
        "type Foo = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..31
              TYPE_ITEM@0..31
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                RECORD_EXPR@11..30
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..28
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    PATH_TYPE@23..28
                      NAME_REF@23..28
                        IDENT@23..28 "usize"
                  WHITESPACE@28..29 " "
                  R_BRACE@29..30 "}"
                SEMICOLON@30..31 ";"
        "#]],
    );
}

#[test]
fn type_item_with_non_struct_rhs_parses() {
    // Superset parsing: any expression parses as the RHS; hir restricts it
    // to a `struct` literal with its own diagnostic.
    check(
        "type Foo = 5;",
        expect![[r#"
            SOURCE_FILE@0..13
              TYPE_ITEM@0..13
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..12
                  INT_NUMBER@11..12 "5"
                SEMICOLON@12..13 ";"
        "#]],
    );
}

#[test]
fn type_item_inside_block_recovers() {
    // `type` never starts an expression, so a block statement loop breaks
    // out and the item parses at the top level.
    check(
        "static f = fn {\ntype Foo = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..47
              STATIC_ITEM@0..15
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..15
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..15
                    L_BRACE@14..15 "{"
              WHITESPACE@15..16 "\n"
              TYPE_ITEM@16..47
                TYPE_KW@16..20 "type"
                WHITESPACE@20..21 " "
                NAME@21..24
                  IDENT@21..24 "Foo"
                WHITESPACE@24..25 " "
                EQ@25..26 "="
                WHITESPACE@26..27 " "
                RECORD_EXPR@27..46
                  STRUCT_KW@27..33 "struct"
                  WHITESPACE@33..34 " "
                  L_BRACE@34..35 "{"
                  WHITESPACE@35..36 " "
                  RECORD_EXPR_FIELD@36..44
                    NAME_REF@36..37
                      IDENT@36..37 "x"
                    COLON@37..38 ":"
                    WHITESPACE@38..39 " "
                    PATH_TYPE@39..44
                      NAME_REF@39..44
                        IDENT@39..44 "usize"
                  WHITESPACE@44..45 " "
                  R_BRACE@45..46 "}"
                SEMICOLON@46..47 ";"
            error 14..15: expected `}`
        "#]],
    );
}

#[test]
fn type_item_annotation_rejected() {
    // The item shape is shared with `static`/`const`, so `: Type` parses;
    // validation rejects it with a removal fix.
    check(
        "type Foo: usize = struct { x: usize };",
        expect![[r#"
            SOURCE_FILE@0..38
              TYPE_ITEM@0..38
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..8
                  IDENT@5..8 "Foo"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "usize"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                RECORD_EXPR@18..37
                  STRUCT_KW@18..24 "struct"
                  WHITESPACE@24..25 " "
                  L_BRACE@25..26 "{"
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..35
                    NAME_REF@27..28
                      IDENT@27..28 "x"
                    COLON@28..29 ":"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..35
                      NAME_REF@30..35
                        IDENT@30..35 "usize"
                  WHITESPACE@35..36 " "
                  R_BRACE@36..37 "}"
                SEMICOLON@37..38 ";"
            error 8..15: a `type` declaration takes no type annotation
        "#]],
    );
}

#[test]
fn enum_type_declaration() {
    check(
        "type Shape = enum { Circle(usize), Pair(usize, str), Point };",
        expect![[r#"
            SOURCE_FILE@0..61
              TYPE_ITEM@0..61
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..10
                  IDENT@5..10 "Shape"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                ENUM_EXPR@13..60
                  ENUM_KW@13..17 "enum"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  ENUM_VARIANT@20..33
                    NAME@20..26
                      IDENT@20..26 "Circle"
                    L_PAREN@26..27 "("
                    PATH_TYPE@27..32
                      NAME_REF@27..32
                        IDENT@27..32 "usize"
                    R_PAREN@32..33 ")"
                  COMMA@33..34 ","
                  WHITESPACE@34..35 " "
                  ENUM_VARIANT@35..51
                    NAME@35..39
                      IDENT@35..39 "Pair"
                    L_PAREN@39..40 "("
                    PATH_TYPE@40..45
                      NAME_REF@40..45
                        IDENT@40..45 "usize"
                    COMMA@45..46 ","
                    WHITESPACE@46..47 " "
                    PATH_TYPE@47..50
                      NAME_REF@47..50
                        IDENT@47..50 "str"
                    R_PAREN@50..51 ")"
                  COMMA@51..52 ","
                  WHITESPACE@52..53 " "
                  ENUM_VARIANT@53..58
                    NAME@53..58
                      IDENT@53..58 "Point"
                  WHITESPACE@58..59 " "
                  R_BRACE@59..60 "}"
                SEMICOLON@60..61 ";"
        "#]],
    );
}

#[test]
fn enum_literal_outside_type_declaration_is_rejected() {
    check(
        "static x = enum { A };",
        expect![[r#"
        SOURCE_FILE@0..22
          STATIC_ITEM@0..22
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            ENUM_EXPR@11..21
              ENUM_KW@11..15 "enum"
              WHITESPACE@15..16 " "
              L_BRACE@16..17 "{"
              WHITESPACE@17..18 " "
              ENUM_VARIANT@18..19
                NAME@18..19
                  IDENT@18..19 "A"
              WHITESPACE@19..20 " "
              R_BRACE@20..21 "}"
            SEMICOLON@21..22 ";"
        error 11..21: an `enum` literal can only appear as a `type` declaration's value
    "#]],
    );
}

#[test]
fn duplicate_enum_variants_are_rejected() {
    check(
        "type Shape = enum { A, A };",
        expect![[r#"
        SOURCE_FILE@0..27
          TYPE_ITEM@0..27
            TYPE_KW@0..4 "type"
            WHITESPACE@4..5 " "
            NAME@5..10
              IDENT@5..10 "Shape"
            WHITESPACE@10..11 " "
            EQ@11..12 "="
            WHITESPACE@12..13 " "
            ENUM_EXPR@13..26
              ENUM_KW@13..17 "enum"
              WHITESPACE@17..18 " "
              L_BRACE@18..19 "{"
              WHITESPACE@19..20 " "
              ENUM_VARIANT@20..21
                NAME@20..21
                  IDENT@20..21 "A"
              COMMA@21..22 ","
              WHITESPACE@22..23 " "
              ENUM_VARIANT@23..24
                NAME@23..24
                  IDENT@23..24 "A"
              WHITESPACE@24..25 " "
              R_BRACE@25..26 "}"
            SEMICOLON@26..27 ";"
        error 23..24: duplicate variant `A`
    "#]],
    );
}

#[test]
fn variant_path_expression() {
    check(
        "static s = Shape::Circle(3);",
        expect![[r#"
        SOURCE_FILE@0..28
          STATIC_ITEM@0..28
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "s"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            CALL_EXPR@11..27
              PATH_EXPR@11..24
                NAME_REF@11..16
                  IDENT@11..16 "Shape"
                COLON2@16..18 "::"
                NAME_REF@18..24
                  IDENT@18..24 "Circle"
              ARG_LIST@24..27
                L_PAREN@24..25 "("
                LITERAL@25..26
                  INT_NUMBER@25..26 "3"
                R_PAREN@26..27 ")"
            SEMICOLON@27..28 ";"
    "#]],
    );
}

#[test]
fn elided_variant_expression() {
    // The mirror of the elided-sigil variant PATTERN, same node shape: one
    // `NameRef`, with the `COLON2` before it — so it can never be confused
    // with a `PATH_EXPR`, which always starts at an `IDENT`.
    check(
        "static s: Shape = ::Point;",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                ELIDED_VARIANT_EXPR@18..25
                  COLON2@18..20 "::"
                  NAME_REF@20..25
                    IDENT@20..25 "Point"
                SEMICOLON@25..26 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_with_payload() {
    // The payload form is left to the ordinary postfix loop, so it is a
    // `CALL_EXPR` over the sigil node exactly as `Shape::Circle(3)` is one
    // over a `PATH_EXPR`.
    check(
        "static s: Shape = ::Circle(3);",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                CALL_EXPR@18..29
                  ELIDED_VARIANT_EXPR@18..26
                    COLON2@18..20 "::"
                    NAME_REF@20..26
                      IDENT@20..26 "Circle"
                  ARG_LIST@26..29
                    L_PAREN@26..27 "("
                    LITERAL@27..28
                      INT_NUMBER@27..28 "3"
                    R_PAREN@28..29 ")"
                SEMICOLON@29..30 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_after_return() {
    // `return`/`break` take a value only when the next token can start one,
    // so `COLON2` has to be an expression starter or the sigil is stranded
    // and the `return` silently becomes valueless.
    check(
        "static f = fn() -> Shape { return ::Point; };",
        expect![[r#"
            SOURCE_FILE@0..45
              STATIC_ITEM@0..45
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..44
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..15
                    L_PAREN@13..14 "("
                    R_PAREN@14..15 ")"
                  WHITESPACE@15..16 " "
                  RET_TYPE@16..24
                    THIN_ARROW@16..18 "->"
                    WHITESPACE@18..19 " "
                    PATH_TYPE@19..24
                      NAME_REF@19..24
                        IDENT@19..24 "Shape"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..44
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..42
                      RETURN_EXPR@27..41
                        RETURN_KW@27..33 "return"
                        WHITESPACE@33..34 " "
                        ELIDED_VARIANT_EXPR@34..41
                          COLON2@34..36 "::"
                          NAME_REF@36..41
                            IDENT@36..41 "Point"
                      SEMICOLON@41..42 ";"
                    WHITESPACE@42..43 " "
                    R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
        "#]],
    );
}

#[test]
fn elided_variant_expression_missing_name() {
    check(
        "static s: Shape = ::;",
        expect![[r#"
            SOURCE_FILE@0..21
              STATIC_ITEM@0..21
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..15
                  NAME_REF@10..15
                    IDENT@10..15 "Shape"
                WHITESPACE@15..16 " "
                EQ@16..17 "="
                WHITESPACE@17..18 " "
                ELIDED_VARIANT_EXPR@18..20
                  COLON2@18..20 "::"
                SEMICOLON@20..21 ";"
            error 20..21: expected a variant name after `::`
        "#]],
    );
}

#[test]
fn inherent_member_region_binder_parses_without_reservation() {
    // Regions are the ONE kind of member-own binder that is live: a member
    // taking a borrow of `Self` has nowhere else to bind its per-call
    // region. The type and const kinds keep their reservation, now squiggling
    // the individual param rather than the whole list.
    check(
        "type Cell = struct { n: usize } with {\n\
             impl Self { get = fn::<@b>(m: Self.&::<@b>) -> usize { m.*.n }; }\n\
         };",
        expect![[r#"
            SOURCE_FILE@0..107
              TYPE_ITEM@0..107
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..9
                  IDENT@5..9 "Cell"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                RECORD_EXPR@12..31
                  STRUCT_KW@12..18 "struct"
                  WHITESPACE@18..19 " "
                  L_BRACE@19..20 "{"
                  WHITESPACE@20..21 " "
                  RECORD_EXPR_FIELD@21..29
                    NAME_REF@21..22
                      IDENT@21..22 "n"
                    COLON@22..23 ":"
                    WHITESPACE@23..24 " "
                    PATH_TYPE@24..29
                      NAME_REF@24..29
                        IDENT@24..29 "usize"
                  WHITESPACE@29..30 " "
                  R_BRACE@30..31 "}"
                WHITESPACE@31..32 " "
                WITH_GROUP@32..106
                  WITH_KW@32..36 "with"
                  WHITESPACE@36..37 " "
                  L_BRACE@37..38 "{"
                  WHITESPACE@38..39 "\n"
                  IMPL_ELEMENT@39..104
                    IMPL_KW@39..43 "impl"
                    WHITESPACE@43..44 " "
                    PATH_TYPE@44..48
                      NAME_REF@44..48
                        IDENT@44..48 "Self"
                    WHITESPACE@48..49 " "
                    L_BRACE@49..50 "{"
                    WHITESPACE@50..51 " "
                    MEMBER@51..102
                      NAME@51..54
                        IDENT@51..54 "get"
                      WHITESPACE@54..55 " "
                      EQ@55..56 "="
                      WHITESPACE@56..57 " "
                      FN_LITERAL@57..101
                        FN_KW@57..59 "fn"
                        GENERIC_PARAM_LIST@59..65
                          COLON2@59..61 "::"
                          L_ANGLE@61..62 "<"
                          REGION_PARAM@62..64
                            REGION_IDENT@62..64 "@b"
                          R_ANGLE@64..65 ">"
                        PARAM_LIST@65..82
                          L_PAREN@65..66 "("
                          PARAM@66..81
                            BIND_PAT@66..67
                              NAME@66..67
                                IDENT@66..67 "m"
                            COLON@67..68 ":"
                            WHITESPACE@68..69 " "
                            BORROW_TYPE@69..81
                              PATH_TYPE@69..73
                                NAME_REF@69..73
                                  IDENT@69..73 "Self"
                              DOT@73..74 "."
                              AMP@74..75 "&"
                              COLON2@75..77 "::"
                              GENERIC_ARG_LIST@77..81
                                L_ANGLE@77..78 "<"
                                REGION_ARG@78..80
                                  REGION_IDENT@78..80 "@b"
                                R_ANGLE@80..81 ">"
                          R_PAREN@81..82 ")"
                        WHITESPACE@82..83 " "
                        RET_TYPE@83..91
                          THIN_ARROW@83..85 "->"
                          WHITESPACE@85..86 " "
                          PATH_TYPE@86..91
                            NAME_REF@86..91
                              IDENT@86..91 "usize"
                        WHITESPACE@91..92 " "
                        BLOCK_EXPR@92..101
                          L_BRACE@92..93 "{"
                          WHITESPACE@93..94 " "
                          FIELD_EXPR@94..99
                            DEREF_EXPR@94..97
                              PATH_EXPR@94..95
                                NAME_REF@94..95
                                  IDENT@94..95 "m"
                              DOT@95..96 "."
                              STAR@96..97 "*"
                            DOT@97..98 "."
                            NAME_REF@98..99
                              IDENT@98..99 "n"
                          WHITESPACE@99..100 " "
                          R_BRACE@100..101 "}"
                      SEMICOLON@101..102 ";"
                    WHITESPACE@102..103 " "
                    R_BRACE@103..104 "}"
                  WHITESPACE@104..105 "\n"
                  R_BRACE@105..106 "}"
                SEMICOLON@106..107 ";"
        "#]],
    );
}

#[test]
fn variant_path_type_annotation() {
    check(
        "static s: Shape::Circle = c;",
        expect![[r#"
        SOURCE_FILE@0..28
          STATIC_ITEM@0..28
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "s"
            COLON@8..9 ":"
            WHITESPACE@9..10 " "
            PATH_TYPE@10..23
              NAME_REF@10..15
                IDENT@10..15 "Shape"
              COLON2@15..17 "::"
              NAME_REF@17..23
                IDENT@17..23 "Circle"
            WHITESPACE@23..24 " "
            EQ@24..25 "="
            WHITESPACE@25..26 " "
            PATH_EXPR@26..27
              NAME_REF@26..27
                IDENT@26..27 "c"
            SEMICOLON@27..28 ";"
    "#]],
    );
}

#[test]
fn variant_path_missing_second_segment() {
    check(
        "static s = Shape::;",
        expect![[r#"
        SOURCE_FILE@0..19
          STATIC_ITEM@0..19
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "s"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            PATH_EXPR@11..18
              NAME_REF@11..16
                IDENT@11..16 "Shape"
              COLON2@16..18 "::"
            SEMICOLON@18..19 ";"
        error 18..19: expected a variant name after `::`
    "#]],
    );
}

#[test]
fn match_expr_all_pattern_kinds() {
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        ::Pair(a, _) => a,
        Point => 0,
        other => 1,
        _ => 2,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..173
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..172
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..172
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
                  BLOCK_EXPR@35..172
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..170
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "s"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..81
                        VARIANT_PAT@59..75
                          NAME_REF@59..64
                            IDENT@59..64 "Shape"
                          COLON2@64..66 "::"
                          NAME_REF@66..72
                            IDENT@66..72 "Circle"
                          L_PAREN@72..73 "("
                          NAME@73..74
                            IDENT@73..74 "r"
                          R_PAREN@74..75 ")"
                        WHITESPACE@75..76 " "
                        FAT_ARROW@76..78 "=>"
                        WHITESPACE@78..79 " "
                        PATH_EXPR@79..80
                          NAME_REF@79..80
                            IDENT@79..80 "r"
                        COMMA@80..81 ","
                      WHITESPACE@81..90 "\n        "
                      MATCH_ARM@90..108
                        VARIANT_PAT@90..102
                          COLON2@90..92 "::"
                          NAME_REF@92..96
                            IDENT@92..96 "Pair"
                          L_PAREN@96..97 "("
                          NAME@97..98
                            IDENT@97..98 "a"
                          COMMA@98..99 ","
                          WHITESPACE@99..100 " "
                          NAME@100..101
                            HOLE@100..101 "_"
                          R_PAREN@101..102 ")"
                        WHITESPACE@102..103 " "
                        FAT_ARROW@103..105 "=>"
                        WHITESPACE@105..106 " "
                        PATH_EXPR@106..107
                          NAME_REF@106..107
                            IDENT@106..107 "a"
                        COMMA@107..108 ","
                      WHITESPACE@108..117 "\n        "
                      MATCH_ARM@117..128
                        BIND_PAT@117..122
                          NAME@117..122
                            IDENT@117..122 "Point"
                        WHITESPACE@122..123 " "
                        FAT_ARROW@123..125 "=>"
                        WHITESPACE@125..126 " "
                        LITERAL@126..127
                          INT_NUMBER@126..127 "0"
                        COMMA@127..128 ","
                      WHITESPACE@128..137 "\n        "
                      MATCH_ARM@137..148
                        BIND_PAT@137..142
                          NAME@137..142
                            IDENT@137..142 "other"
                        WHITESPACE@142..143 " "
                        FAT_ARROW@143..145 "=>"
                        WHITESPACE@145..146 " "
                        LITERAL@146..147
                          INT_NUMBER@146..147 "1"
                        COMMA@147..148 ","
                      WHITESPACE@148..157 "\n        "
                      MATCH_ARM@157..164
                        WILDCARD_PAT@157..158
                          HOLE@157..158 "_"
                        WHITESPACE@158..159 " "
                        FAT_ARROW@159..161 "=>"
                        WHITESPACE@161..162 " "
                        LITERAL@162..163
                          INT_NUMBER@162..163 "2"
                        COMMA@163..164 ","
                      WHITESPACE@164..169 "\n    "
                      R_BRACE@169..170 "}"
                    WHITESPACE@170..171 "\n"
                    R_BRACE@171..172 "}"
              WHITESPACE@172..173 "\n"
        "#]],
    );
}

#[test]
fn match_arm_brace_rule_and_trailing_comma() {
    // An arm body ending in `}` needs no comma; a trailing comma before
    // the closing brace is fine.
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        Point => { 0 }
        _ => 2,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..98
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..97
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..97
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
                  BLOCK_EXPR@35..97
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..95
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "s"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..73
                        BIND_PAT@59..64
                          NAME@59..64
                            IDENT@59..64 "Point"
                        WHITESPACE@64..65 " "
                        FAT_ARROW@65..67 "=>"
                        WHITESPACE@67..68 " "
                        BLOCK_EXPR@68..73
                          L_BRACE@68..69 "{"
                          WHITESPACE@69..70 " "
                          LITERAL@70..71
                            INT_NUMBER@70..71 "0"
                          WHITESPACE@71..72 " "
                          R_BRACE@72..73 "}"
                      WHITESPACE@73..82 "\n        "
                      MATCH_ARM@82..89
                        WILDCARD_PAT@82..83
                          HOLE@82..83 "_"
                        WHITESPACE@83..84 " "
                        FAT_ARROW@84..86 "=>"
                        WHITESPACE@86..87 " "
                        LITERAL@87..88
                          INT_NUMBER@87..88 "2"
                        COMMA@88..89 ","
                      WHITESPACE@89..94 "\n    "
                      R_BRACE@94..95 "}"
                    WHITESPACE@95..96 "\n"
                    R_BRACE@96..97 "}"
              WHITESPACE@97..98 "\n"
        "#]],
    );
}

#[test]
fn match_rest_pattern_parses_and_is_rejected() {
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r, ..) => r,
        .. => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..106
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..105
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..105
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
                  BLOCK_EXPR@35..105
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..103
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "s"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..80
                        VARIANT_PAT@59..74
                          COLON2@59..61 "::"
                          NAME_REF@61..67
                            IDENT@61..67 "Circle"
                          L_PAREN@67..68 "("
                          NAME@68..69
                            IDENT@68..69 "r"
                          COMMA@69..70 ","
                          WHITESPACE@70..71 " "
                          REST_PAT@71..73
                            DOT2@71..73 ".."
                          R_PAREN@73..74 ")"
                        WHITESPACE@74..75 " "
                        FAT_ARROW@75..77 "=>"
                        WHITESPACE@77..78 " "
                        PATH_EXPR@78..79
                          NAME_REF@78..79
                            IDENT@78..79 "r"
                        COMMA@79..80 ","
                      WHITESPACE@80..89 "\n        "
                      MATCH_ARM@89..97
                        REST_PAT@89..91
                          DOT2@89..91 ".."
                        WHITESPACE@91..92 " "
                        FAT_ARROW@92..94 "=>"
                        WHITESPACE@94..95 " "
                        LITERAL@95..96
                          INT_NUMBER@95..96 "0"
                        COMMA@96..97 ","
                      WHITESPACE@97..102 "\n    "
                      R_BRACE@102..103 "}"
                    WHITESPACE@103..104 "\n"
                    R_BRACE@104..105 "}"
              WHITESPACE@105..106 "\n"
            error 71..73: `..` in patterns is not supported yet
            error 89..91: `..` in patterns is not supported yet
        "#]],
    );
}

#[test]
fn match_retired_bare_variant_pat_still_parses() {
    // `Circle(r)` (no `::`, no qualifying enum) is the retired v1
    // shorthand. It still parses as a `VARIANT_PAT` with no `COLON2` (the
    // tree keeps the user's intent), and validation reports the honest
    // "write `::Circle(...)`" error.
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        Circle(r) => r,
        _ => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..99
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..98
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..98
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
                  BLOCK_EXPR@35..98
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..96
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "s"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..74
                        VARIANT_PAT@59..68
                          NAME_REF@59..65
                            IDENT@59..65 "Circle"
                          L_PAREN@65..66 "("
                          NAME@66..67
                            IDENT@66..67 "r"
                          R_PAREN@67..68 ")"
                        WHITESPACE@68..69 " "
                        FAT_ARROW@69..71 "=>"
                        WHITESPACE@71..72 " "
                        PATH_EXPR@72..73
                          NAME_REF@72..73
                            IDENT@72..73 "r"
                        COMMA@73..74 ","
                      WHITESPACE@74..83 "\n        "
                      MATCH_ARM@83..90
                        WILDCARD_PAT@83..84
                          HOLE@83..84 "_"
                        WHITESPACE@84..85 " "
                        FAT_ARROW@85..87 "=>"
                        WHITESPACE@87..88 " "
                        LITERAL@88..89
                          INT_NUMBER@88..89 "0"
                        COMMA@89..90 ","
                      WHITESPACE@90..95 "\n    "
                      R_BRACE@95..96 "}"
                    WHITESPACE@96..97 "\n"
                    R_BRACE@97..98 "}"
              WHITESPACE@98..99 "\n"
            error 59..68: write `::Circle(...)` to match a variant, or remove `(...)` to bind
        "#]],
    );
}

#[test]
fn match_retired_bare_variant_pat_fix_inserts_colon2() {
    // The retired `Circle(r)` shape's fix inserts `::` right before the
    // variant name, turning it into the elided sigil spelling.
    let parse = crate::parse(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        Circle(r) => r,
        _ => 0,
    }
}
"#,
    );
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("write `::Circle"))
        .expect("expected the retired-bare-variant error");
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(fix.label, "Insert `::`");
    assert_eq!(fix.edits.len(), 1);
    let edit = &fix.edits[0];
    assert!(edit.range.is_empty(), "the fix is a pure insertion");
    assert_eq!(edit.insert, "::");
}

#[test]
fn match_recovery_missing_fat_arrow() {
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        Point 0,
        _ => 1,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..92
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..91
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..91
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
                  BLOCK_EXPR@35..91
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..89
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "s"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..67
                        BIND_PAT@59..64
                          NAME@59..64
                            IDENT@59..64 "Point"
                        WHITESPACE@64..65 " "
                        LITERAL@65..66
                          INT_NUMBER@65..66 "0"
                        COMMA@66..67 ","
                      WHITESPACE@67..76 "\n        "
                      MATCH_ARM@76..83
                        WILDCARD_PAT@76..77
                          HOLE@76..77 "_"
                        WHITESPACE@77..78 " "
                        FAT_ARROW@78..80 "=>"
                        WHITESPACE@80..81 " "
                        LITERAL@81..82
                          INT_NUMBER@81..82 "1"
                        COMMA@82..83 ","
                      WHITESPACE@83..88 "\n    "
                      R_BRACE@88..89 "}"
                    WHITESPACE@89..90 "\n"
                    R_BRACE@90..91 "}"
              WHITESPACE@91..92 "\n"
            error 65..66: expected `=>`
        "#]],
    );
}

#[test]
fn match_arm_whose_pattern_reads_nothing_skips_its_body() {
    // `-1` is not a pattern (a negative literal is an operator applied to
    // a literal, reserved with the rest of the pattern language), and the
    // refusal consumes nothing. Reading a body after it would take `1` as
    // the arm's value and judge it against the match's RESULT type — a
    // second error about a type the user never wrote — so the rest of the
    // arm is ERROR and the next arm is where parsing resumes.
    check(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        -1 => 1,
        _ => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..92
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..91
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..91
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  PARAM_LIST@15..25
                    L_PAREN@15..16 "("
                    PARAM@16..24
                      BIND_PAT@16..17
                        NAME@16..17
                          IDENT@16..17 "n"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..24
                        NAME_REF@19..24
                          IDENT@19..24 "usize"
                    R_PAREN@24..25 ")"
                  WHITESPACE@25..26 " "
                  RET_TYPE@26..34
                    THIN_ARROW@26..28 "->"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..34
                      NAME_REF@29..34
                        IDENT@29..34 "usize"
                  WHITESPACE@34..35 " "
                  BLOCK_EXPR@35..91
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..89
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "n"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..67
                        ERROR@59..66
                          MINUS@59..60 "-"
                          INT_NUMBER@60..61 "1"
                          WHITESPACE@61..62 " "
                          FAT_ARROW@62..64 "=>"
                          WHITESPACE@64..65 " "
                          INT_NUMBER@65..66 "1"
                        COMMA@66..67 ","
                      WHITESPACE@67..76 "\n        "
                      MATCH_ARM@76..83
                        WILDCARD_PAT@76..77
                          HOLE@76..77 "_"
                        WHITESPACE@77..78 " "
                        FAT_ARROW@78..80 "=>"
                        WHITESPACE@80..81 " "
                        LITERAL@81..82
                          INT_NUMBER@81..82 "0"
                        COMMA@82..83 ","
                      WHITESPACE@83..88 "\n    "
                      R_BRACE@88..89 "}"
                    WHITESPACE@89..90 "\n"
                    R_BRACE@90..91 "}"
              WHITESPACE@91..92 "\n"
            error 59..60: expected a pattern
        "#]],
    );
}

#[test]
fn a_skipped_arm_body_does_not_close_the_arm_list_on_its_own_brace() {
    // The skip counts bracket depth: the body's `}` and its `,` belong to
    // the body, not to the arm list. Stopping at the first `}` would end
    // the match on the body's brace and cascade every token after it
    // through the item parser — worse than the diagnostic the skip exists
    // to remove. One error here, and `_ => 0` is still an arm.
    check(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        -1 => { 1 },
        _ => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..96
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..95
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..95
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  PARAM_LIST@15..25
                    L_PAREN@15..16 "("
                    PARAM@16..24
                      BIND_PAT@16..17
                        NAME@16..17
                          IDENT@16..17 "n"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..24
                        NAME_REF@19..24
                          IDENT@19..24 "usize"
                    R_PAREN@24..25 ")"
                  WHITESPACE@25..26 " "
                  RET_TYPE@26..34
                    THIN_ARROW@26..28 "->"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..34
                      NAME_REF@29..34
                        IDENT@29..34 "usize"
                  WHITESPACE@34..35 " "
                  BLOCK_EXPR@35..95
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..41 "\n    "
                    MATCH_EXPR@41..93
                      MATCH_KW@41..46 "match"
                      WHITESPACE@46..47 " "
                      PATH_EXPR@47..48
                        NAME_REF@47..48
                          IDENT@47..48 "n"
                      WHITESPACE@48..49 " "
                      L_BRACE@49..50 "{"
                      WHITESPACE@50..59 "\n        "
                      MATCH_ARM@59..71
                        ERROR@59..70
                          MINUS@59..60 "-"
                          INT_NUMBER@60..61 "1"
                          WHITESPACE@61..62 " "
                          FAT_ARROW@62..64 "=>"
                          WHITESPACE@64..65 " "
                          L_BRACE@65..66 "{"
                          WHITESPACE@66..67 " "
                          INT_NUMBER@67..68 "1"
                          WHITESPACE@68..69 " "
                          R_BRACE@69..70 "}"
                        COMMA@70..71 ","
                      WHITESPACE@71..80 "\n        "
                      MATCH_ARM@80..87
                        WILDCARD_PAT@80..81
                          HOLE@80..81 "_"
                        WHITESPACE@81..82 " "
                        FAT_ARROW@82..84 "=>"
                        WHITESPACE@84..85 " "
                        LITERAL@85..86
                          INT_NUMBER@85..86 "0"
                        COMMA@86..87 ","
                      WHITESPACE@87..92 "\n    "
                      R_BRACE@92..93 "}"
                    WHITESPACE@93..94 "\n"
                    R_BRACE@94..95 "}"
              WHITESPACE@95..96 "\n"
            error 59..60: expected a pattern
        "#]],
    );
    // Arrays bring the third bracket pair, and its `,` separates ELEMENTS:
    // counted, `[1, 2]` is one body; uncounted, the arm would end at the
    // array's own comma and `2]` would be handed to the arm list.
    let parse =
        crate::parse("static f = fn (n: usize) -> usize { match n { -1 => [1, 2], _ => 0, } }");
    let msgs: Vec<_> = parse.errors().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs, ["expected a pattern"], "{msgs:?}");
}

#[test]
fn a_skipped_arm_body_stops_at_an_item_keyword() {
    // The skip's depth-0 stop set IS the item-recovery predicate, not a
    // second copy of it. An unclosed arm list whose pattern read nothing
    // must still hand the next item to the item parser whole: a skip that
    // ran past `type` would swallow the declaration and turn one pattern
    // error into a file's worth.
    check(
        r#"
static f = fn (n: usize) -> usize { match n {
    -1 => 1
type T = usize;
"#,
        expect![[r#"
            SOURCE_FILE@0..75
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..58
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..58
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  PARAM_LIST@15..25
                    L_PAREN@15..16 "("
                    PARAM@16..24
                      BIND_PAT@16..17
                        NAME@16..17
                          IDENT@16..17 "n"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..24
                        NAME_REF@19..24
                          IDENT@19..24 "usize"
                    R_PAREN@24..25 ")"
                  WHITESPACE@25..26 " "
                  RET_TYPE@26..34
                    THIN_ARROW@26..28 "->"
                    WHITESPACE@28..29 " "
                    PATH_TYPE@29..34
                      NAME_REF@29..34
                        IDENT@29..34 "usize"
                  WHITESPACE@34..35 " "
                  BLOCK_EXPR@35..58
                    L_BRACE@35..36 "{"
                    WHITESPACE@36..37 " "
                    EXPR_STMT@37..58
                      MATCH_EXPR@37..58
                        MATCH_KW@37..42 "match"
                        WHITESPACE@42..43 " "
                        PATH_EXPR@43..44
                          NAME_REF@43..44
                            IDENT@43..44 "n"
                        WHITESPACE@44..45 " "
                        L_BRACE@45..46 "{"
                        WHITESPACE@46..51 "\n    "
                        MATCH_ARM@51..58
                          ERROR@51..58
                            MINUS@51..52 "-"
                            INT_NUMBER@52..53 "1"
                            WHITESPACE@53..54 " "
                            FAT_ARROW@54..56 "=>"
                            WHITESPACE@56..57 " "
                            INT_NUMBER@57..58 "1"
              WHITESPACE@58..59 "\n"
              TYPE_ITEM@59..74
                TYPE_KW@59..63 "type"
                WHITESPACE@63..64 " "
                NAME@64..65
                  IDENT@64..65 "T"
                WHITESPACE@65..66 " "
                EQ@66..67 "="
                WHITESPACE@67..68 " "
                PATH_EXPR@68..73
                  NAME_REF@68..73
                    IDENT@68..73 "usize"
                SEMICOLON@73..74 ";"
              WHITESPACE@74..75 "\n"
            error 51..52: expected a pattern
            error 57..58: expected `}`
        "#]],
    );
    // Trait declarations join the item set, and the skip follows with no
    // second edit of its own — that is the point of asking the predicate.
    let parse = crate::parse(
        "static f = fn (n: usize) -> usize { match n {\n    -1 => 1\ntrait T = requires { };\n",
    );
    let msgs: Vec<_> = parse.errors().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs, ["expected a pattern", "expected `}`"], "{msgs:?}");
    assert!(
        parse.debug_dump().contains("TRAIT_ITEM@"),
        "the trait declaration must survive the skip: {}",
        parse.debug_dump()
    );
}

#[test]
fn match_recovery_missing_arm_list() {
    check(
        "static f = fn (s: Shape) -> usize { match s }",
        expect![[r#"
            SOURCE_FILE@0..45
              STATIC_ITEM@0..45
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
                  PARAM_LIST@14..24
                    L_PAREN@14..15 "("
                    PARAM@15..23
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "s"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      PATH_TYPE@18..23
                        NAME_REF@18..23
                          IDENT@18..23 "Shape"
                    R_PAREN@23..24 ")"
                  WHITESPACE@24..25 " "
                  RET_TYPE@25..33
                    THIN_ARROW@25..27 "->"
                    WHITESPACE@27..28 " "
                    PATH_TYPE@28..33
                      NAME_REF@28..33
                        IDENT@28..33 "usize"
                  WHITESPACE@33..34 " "
                  BLOCK_EXPR@34..45
                    L_BRACE@34..35 "{"
                    WHITESPACE@35..36 " "
                    MATCH_EXPR@36..43
                      MATCH_KW@36..41 "match"
                      WHITESPACE@41..42 " "
                      PATH_EXPR@42..43
                        NAME_REF@42..43
                          IDENT@42..43 "s"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
            error 44..45: expected `{` followed by the match arms
        "#]],
    );
}

#[test]
fn match_recovery_unclosed_before_item() {
    // An item keyword inside the arm list means the `}` is missing: the
    // parser stops the match and lets the item parse.
    check(
        r#"
static f = fn (s: Shape) { match s {
static g = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..52
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..37
                STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..37
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
                  BLOCK_EXPR@26..37
                    L_BRACE@26..27 "{"
                    WHITESPACE@27..28 " "
                    EXPR_STMT@28..37
                      MATCH_EXPR@28..37
                        MATCH_KW@28..33 "match"
                        WHITESPACE@33..34 " "
                        PATH_EXPR@34..35
                          NAME_REF@34..35
                            IDENT@34..35 "s"
                        WHITESPACE@35..36 " "
                        L_BRACE@36..37 "{"
              WHITESPACE@37..38 "\n"
              STATIC_ITEM@38..51
                STATIC_KW@38..44 "static"
                WHITESPACE@44..45 " "
                NAME@45..46
                  IDENT@45..46 "g"
                WHITESPACE@46..47 " "
                EQ@47..48 "="
                WHITESPACE@48..49 " "
                LITERAL@49..50
                  INT_NUMBER@49..50 "1"
                SEMICOLON@50..51 ";"
              WHITESPACE@51..52 "\n"
            error 36..37: expected `}`
        "#]],
    );
}

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

// ---- record destructuring, `pub` reservation, field assignment ----

#[test]
fn let_record_destructure() {
    check(
        "static f = fn { let struct { x, y } = p; };",
        expect![[r#"
            SOURCE_FILE@0..43
              STATIC_ITEM@0..43
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

// ---- generics: fn binders ----

#[test]
fn generic_fn_binder_type_params_only() {
    check(
        "static id = fn::<T>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
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
            error 33..34: expected `,`
            error 34..34: expected `,`
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

// ---- generics: turbofish ----

#[test]
fn turbofish_call_expr() {
    check(
        "static x = f::<usize, 42>();",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..27
                  PATH_EXPR@11..25
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..25
                      L_ANGLE@14..15 "<"
                      TYPE_ARG@15..20
                        PATH_TYPE@15..20
                          NAME_REF@15..20
                            IDENT@15..20 "usize"
                      COMMA@20..21 ","
                      WHITESPACE@21..22 " "
                      CONST_ARG@22..24
                        LITERAL@22..24
                          INT_NUMBER@22..24 "42"
                      R_ANGLE@24..25 ">"
                  ARG_LIST@25..27
                    L_PAREN@25..26 "("
                    R_PAREN@26..27 ")"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn turbofish_hole_arg() {
    check(
        "static x = f::<_>();",
        expect![[r#"
            SOURCE_FILE@0..20
              STATIC_ITEM@0..20
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..19
                  PATH_EXPR@11..17
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..17
                      L_ANGLE@14..15 "<"
                      TYPE_ARG@15..16
                        HOLE_TYPE@15..16
                          HOLE@15..16 "_"
                      R_ANGLE@16..17 ">"
                  ARG_LIST@17..19
                    L_PAREN@17..18 "("
                    R_PAREN@18..19 ")"
                SEMICOLON@19..20 ";"
        "#]],
    );
}

#[test]
fn turbofish_const_prefixed_arg() {
    check(
        "static x = f::<const LEN>();",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..27
                  PATH_EXPR@11..25
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..25
                      L_ANGLE@14..15 "<"
                      CONST_ARG@15..24
                        CONST_KW@15..20 "const"
                        WHITESPACE@20..21 " "
                        PATH_EXPR@21..24
                          NAME_REF@21..24
                            IDENT@21..24 "LEN"
                      R_ANGLE@24..25 ">"
                  ARG_LIST@25..27
                    L_PAREN@25..26 "("
                    R_PAREN@26..27 ")"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn turbofish_bare_braced_arg_no_longer_parses() {
    // A bare braced block is not a const arg in v1 — the `const` keyword is
    // required. Bare `{ ... }` is a parse error pointing at `const { ... }`.
    check(
        "static x = f::<{ N + 1 }>();",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..27
                  PATH_EXPR@11..25
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..25
                      L_ANGLE@14..15 "<"
                      ERROR@15..24
                        BLOCK_EXPR@15..24
                          L_BRACE@15..16 "{"
                          WHITESPACE@16..17 " "
                          BIN_EXPR@17..22
                            PATH_EXPR@17..18
                              NAME_REF@17..18
                                IDENT@17..18 "N"
                            WHITESPACE@18..19 " "
                            PLUS@19..20 "+"
                            WHITESPACE@20..21 " "
                            LITERAL@21..22
                              INT_NUMBER@21..22 "1"
                          WHITESPACE@22..23 " "
                          R_BRACE@23..24 "}"
                      R_ANGLE@24..25 ">"
                  ARG_LIST@25..27
                    L_PAREN@25..26 "("
                    R_PAREN@26..27 ")"
                SEMICOLON@27..28 ";"
            error 15..16: a braced const argument must be written `const { ... }`
        "#]],
    );
}

#[test]
fn turbofish_const_braced_arg() {
    // `const { ... }` parses as a const-block expression inside the arg.
    check(
        "static x = f::<const { a > b }>();",
        expect![[r#"
            SOURCE_FILE@0..34
              STATIC_ITEM@0..34
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..33
                  PATH_EXPR@11..31
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..31
                      L_ANGLE@14..15 "<"
                      CONST_ARG@15..30
                        CONST_BLOCK_EXPR@15..30
                          CONST_KW@15..20 "const"
                          WHITESPACE@20..21 " "
                          BLOCK_EXPR@21..30
                            L_BRACE@21..22 "{"
                            WHITESPACE@22..23 " "
                            BIN_EXPR@23..28
                              PATH_EXPR@23..24
                                NAME_REF@23..24
                                  IDENT@23..24 "a"
                              WHITESPACE@24..25 " "
                              R_ANGLE@25..26 ">"
                              WHITESPACE@26..27 " "
                              PATH_EXPR@27..28
                                NAME_REF@27..28
                                  IDENT@27..28 "b"
                            WHITESPACE@28..29 " "
                            R_BRACE@29..30 "}"
                      R_ANGLE@30..31 ">"
                  ARG_LIST@31..33
                    L_PAREN@31..32 "("
                    R_PAREN@32..33 ")"
                SEMICOLON@33..34 ";"
        "#]],
    );
}

#[test]
fn turbofish_const_additive_arg_no_longer_parses() {
    // The old additive-precedence escape is gone: `const N + 1` is not one
    // const arg any more — the canonical spelling is `{ N + 1 }`.
    check(
        "static x = f::<const N + 1>();",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BIN_EXPR@11..29
                  BIN_EXPR@11..26
                    PATH_EXPR@11..23
                      NAME_REF@11..12
                        IDENT@11..12 "f"
                      COLON2@12..14 "::"
                      GENERIC_ARG_LIST@14..23
                        L_ANGLE@14..15 "<"
                        CONST_ARG@15..22
                          CONST_KW@15..20 "const"
                          WHITESPACE@20..21 " "
                          PATH_EXPR@21..22
                            NAME_REF@21..22
                              IDENT@21..22 "N"
                        WHITESPACE@22..23 " "
                        TYPE_ARG@23..23
                    PLUS@23..24 "+"
                    WHITESPACE@24..25 " "
                    LITERAL@25..26
                      INT_NUMBER@25..26 "1"
                  R_ANGLE@26..27 ">"
                  PAREN_EXPR@27..29
                    L_PAREN@27..28 "("
                    R_PAREN@28..29 ")"
                SEMICOLON@29..30 ";"
            error 21..22: expected `>`
            error 23..24: expected `,`
            error 28..29: expected an expression
        "#]],
    );
}

#[test]
fn turbofish_const_paren_escape_no_longer_parses() {
    // The old paren escape is gone: a compound expression after `const` must
    // be braced, so `const (a > b)` is a parse error pointing at braces.
    check(
        "static x = f::<const (a > b)>();",
        expect![[r#"
            SOURCE_FILE@0..32
              STATIC_ITEM@0..25
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                PATH_EXPR@11..25
                  NAME_REF@11..12
                    IDENT@11..12 "f"
                  COLON2@12..14 "::"
                  GENERIC_ARG_LIST@14..25
                    L_ANGLE@14..15 "<"
                    CONST_ARG@15..20
                      CONST_KW@15..20 "const"
                    WHITESPACE@20..21 " "
                    TYPE_ARG@21..22
                      UNIT_TYPE@21..22
                        L_PAREN@21..22 "("
                    TYPE_ARG@22..23
                      PATH_TYPE@22..23
                        NAME_REF@22..23
                          IDENT@22..23 "a"
                    WHITESPACE@23..24 " "
                    R_ANGLE@24..25 ">"
              WHITESPACE@25..26 " "
              ERROR@26..27
                IDENT@26..27 "b"
              ERROR@27..28
                R_PAREN@27..28 ")"
              ERROR@28..29
                R_ANGLE@28..29 ">"
              ERROR@29..30
                L_PAREN@29..30 "("
              ERROR@30..31
                R_PAREN@30..31 ")"
              ERROR@31..32
                SEMICOLON@31..32 ";"
            error 21..22: expected a name, literal, or `{ ... }` block after `const`; wrap a compound expression in `const { ... }`
            error 22..23: expected `)` (only the unit type `()` is supported here)
            error 24..25: expected `;`
            error 26..27: expected an item (`static`, `const`, `type` or `trait`)
            error 27..28: expected an item (`static`, `const`, `type` or `trait`)
            error 28..29: expected an item (`static`, `const`, `type` or `trait`)
            error 29..30: expected an item (`static`, `const`, `type` or `trait`)
            error 30..31: expected an item (`static`, `const`, `type` or `trait`)
            error 31..32: expected an item (`static`, `const`, `type` or `trait`)
        "#]],
    );
}

#[test]
fn named_self_qualified_member_path() {
    // TR01's named-Self form: `Self = Type` is one argument of the trait's
    // list (recognized by the `IDENT EQ` form, so the tree is stable under
    // declaration edits), and the member segment follows it.
    check(
        "static x = D::<Self = usize>::m(n);",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..34
                  PATH_EXPR@11..31
                    NAME_REF@11..12
                      IDENT@11..12 "D"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..28
                      L_ANGLE@14..15 "<"
                      NAMED_ARG@15..27
                        NAME_REF@15..19
                          IDENT@15..19 "Self"
                        WHITESPACE@19..20 " "
                        EQ@20..21 "="
                        WHITESPACE@21..22 " "
                        PATH_TYPE@22..27
                          NAME_REF@22..27
                            IDENT@22..27 "usize"
                      R_ANGLE@27..28 ">"
                    COLON2@28..30 "::"
                    NAME_REF@30..31
                      IDENT@30..31 "m"
                  ARG_LIST@31..34
                    L_PAREN@31..32 "("
                    PATH_EXPR@32..33
                      NAME_REF@32..33
                        IDENT@32..33 "n"
                    R_PAREN@33..34 ")"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn named_arg_composes_with_positional_args() {
    // Position-irrelevant (TR01): `From::<u8, Self = T>::from(x)` is the
    // reserved generic-trait shape, and the grammar already carries it.
    check(
        "static x = From::<u8, Self = T>::from(v);",
        expect![[r#"
            SOURCE_FILE@0..41
              STATIC_ITEM@0..41
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..40
                  PATH_EXPR@11..37
                    NAME_REF@11..15
                      IDENT@11..15 "From"
                    COLON2@15..17 "::"
                    GENERIC_ARG_LIST@17..31
                      L_ANGLE@17..18 "<"
                      TYPE_ARG@18..20
                        PATH_TYPE@18..20
                          NAME_REF@18..20
                            IDENT@18..20 "u8"
                      COMMA@20..21 ","
                      WHITESPACE@21..22 " "
                      NAMED_ARG@22..30
                        NAME_REF@22..26
                          IDENT@22..26 "Self"
                        WHITESPACE@26..27 " "
                        EQ@27..28 "="
                        WHITESPACE@28..29 " "
                        PATH_TYPE@29..30
                          NAME_REF@29..30
                            IDENT@29..30 "T"
                      R_ANGLE@30..31 ">"
                    COLON2@31..33 "::"
                    NAME_REF@33..37
                      IDENT@33..37 "from"
                  ARG_LIST@37..40
                    L_PAREN@37..38 "("
                    PATH_EXPR@38..39
                      NAME_REF@38..39
                        IDENT@38..39 "v"
                    R_PAREN@39..40 ")"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn qualified_member_path_with_type_args() {
    // The G13 inherent escape, turbofished: the arguments belong to the
    // TYPE, the last segment names its member.
    check(
        "static x = Pair::<usize>::first(p);",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..34
                  PATH_EXPR@11..31
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
                    COLON2@24..26 "::"
                    NAME_REF@26..31
                      IDENT@26..31 "first"
                  ARG_LIST@31..34
                    L_PAREN@31..32 "("
                    PATH_EXPR@32..33
                      NAME_REF@32..33
                        IDENT@32..33 "p"
                    R_PAREN@33..34 ")"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn turbofish_in_type_position() {
    check(
        "static x: Pair::<usize> = y;",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..23
                  NAME_REF@10..14
                    IDENT@10..14 "Pair"
                  COLON2@14..16 "::"
                  GENERIC_ARG_LIST@16..23
                    L_ANGLE@16..17 "<"
                    TYPE_ARG@17..22
                      PATH_TYPE@17..22
                        NAME_REF@17..22
                          IDENT@17..22 "usize"
                    R_ANGLE@22..23 ">"
                WHITESPACE@23..24 " "
                EQ@24..25 "="
                WHITESPACE@25..26 " "
                PATH_EXPR@26..27
                  NAME_REF@26..27
                    IDENT@26..27 "y"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn turbofish_with_spaces() {
    check(
        "static x = f ::< usize >();",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..26
                  PATH_EXPR@11..24
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    WHITESPACE@12..13 " "
                    COLON2@13..15 "::"
                    GENERIC_ARG_LIST@15..24
                      L_ANGLE@15..16 "<"
                      WHITESPACE@16..17 " "
                      TYPE_ARG@17..22
                        PATH_TYPE@17..22
                          NAME_REF@17..22
                            IDENT@17..22 "usize"
                      WHITESPACE@22..23 " "
                      R_ANGLE@23..24 ">"
                  ARG_LIST@24..26
                    L_PAREN@24..25 "("
                    R_PAREN@25..26 ")"
                SEMICOLON@26..27 ";"
        "#]],
    );
}

#[test]
fn comparison_operator_still_parses_after_generics() {
    // Regression: `<`/`>` remain ordinary comparison operators everywhere
    // that isn't the unambiguous `::<` gate.
    check(
        "static b = a < b;",
        expect![[r#"
            SOURCE_FILE@0..17
              STATIC_ITEM@0..17
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "b"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                BIN_EXPR@11..16
                  PATH_EXPR@11..12
                    NAME_REF@11..12
                      IDENT@11..12 "a"
                  WHITESPACE@12..13 " "
                  L_ANGLE@13..14 "<"
                  WHITESPACE@14..15 " "
                  PATH_EXPR@15..16
                    NAME_REF@15..16
                      IDENT@15..16 "b"
                SEMICOLON@16..17 ";"
        "#]],
    );
}

#[test]
fn variant_path_still_parses_after_generics() {
    // Regression: the existing two-segment variant path (no turbofish)
    // keeps parsing exactly as before.
    check(
        "static s = Shape::Circle;",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                PATH_EXPR@11..24
                  NAME_REF@11..16
                    IDENT@11..16 "Shape"
                  COLON2@16..18 "::"
                  NAME_REF@18..24
                    IDENT@18..24 "Circle"
                SEMICOLON@24..25 ";"
        "#]],
    );
}

#[test]
fn member_own_turbofish_parses_as_the_segments_own_list() {
    // A turbofish on the SECOND segment gets a real node of its own —
    // `MEMBER_GENERIC_ARGS`, holding the `::` and the list — and NO parse
    // error. `P::len::<usize>` is future-legal by declared intent (member
    // binders are reserved, not rejected), so the grammar parses the shape
    // the language will keep and hir states the reservation; granting
    // member generics later deletes a diagnostic and leaves this tree
    // alone.
    //
    // Two regressions are pinned at once. The original bug: the `::` ended
    // the statement and every leftover token became its own parse error —
    // five of them, including a bogus "unresolved name `usize`". Its first
    // fix: the list was consumed under an ERROR node, which said "malformed
    // syntax" about a form that is merely not-yet-meaningful.
    check(
        "static s = fn { let g = P::len::<usize>; };",
        expect![[r#"
            SOURCE_FILE@0..43
              STATIC_ITEM@0..43
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
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
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "g"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      PATH_EXPR@24..39
                        NAME_REF@24..25
                          IDENT@24..25 "P"
                        COLON2@25..27 "::"
                        NAME_REF@27..30
                          IDENT@27..30 "len"
                        MEMBER_GENERIC_ARGS@30..39
                          COLON2@30..32 "::"
                          GENERIC_ARG_LIST@32..39
                            L_ANGLE@32..33 "<"
                            TYPE_ARG@33..38
                              PATH_TYPE@33..38
                                NAME_REF@33..38
                                  IDENT@33..38 "usize"
                            R_ANGLE@38..39 ">"
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..41 " "
                    R_BRACE@41..42 "}"
                SEMICOLON@42..43 ";"
        "#]],
    );
}

#[test]
fn variant_own_turbofish_parses_the_same_shape() {
    // `Shape::Circle::<usize>` is NOT future-legal — a variant never gets
    // a binder of its own — but the parser cannot tell a variant from a
    // member, and does not try: one shape, and hir splits the message by
    // what the segment turned out to name.
    check(
        "static s = Shape::Circle::<usize>;",
        expect![[r#"
            SOURCE_FILE@0..34
              STATIC_ITEM@0..34
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                PATH_EXPR@11..33
                  NAME_REF@11..16
                    IDENT@11..16 "Shape"
                  COLON2@16..18 "::"
                  NAME_REF@18..24
                    IDENT@18..24 "Circle"
                  MEMBER_GENERIC_ARGS@24..33
                    COLON2@24..26 "::"
                    GENERIC_ARG_LIST@26..33
                      L_ANGLE@26..27 "<"
                      TYPE_ARG@27..32
                        PATH_TYPE@27..32
                          NAME_REF@27..32
                            IDENT@27..32 "usize"
                      R_ANGLE@32..33 ">"
                SEMICOLON@33..34 ";"
        "#]],
    );
}

#[test]
fn owner_and_member_turbofish_are_separate_nodes() {
    // Both segments carrying arguments: the OWNER's list is a direct
    // `GENERIC_ARG_LIST` child of the path, the member's hangs inside
    // `MEMBER_GENERIC_ARGS`. That nesting is the whole mechanism — it is
    // what makes `PathExpr::generic_arg_list()` structurally unable to
    // return the member's list (see
    // `path_accessors_keep_the_two_lists_apart`).
    check(
        "static s = Pair::<usize>::first::<T>;",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                PATH_EXPR@11..36
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
                  COLON2@24..26 "::"
                  NAME_REF@26..31
                    IDENT@26..31 "first"
                  MEMBER_GENERIC_ARGS@31..36
                    COLON2@31..33 "::"
                    GENERIC_ARG_LIST@33..36
                      L_ANGLE@33..34 "<"
                      TYPE_ARG@34..35
                        PATH_TYPE@34..35
                          NAME_REF@34..35
                            IDENT@34..35 "T"
                      R_ANGLE@35..36 ">"
                SEMICOLON@36..37 ";"
        "#]],
    );
}

#[test]
fn named_self_path_carries_a_member_turbofish_too() {
    // The trait-qualified form composes: `Self = usize` is the TRAIT's
    // argument (TR01), `::<usize>` after `m` is the member's own.
    check(
        "static x = D::<Self = usize>::m::<usize>(n);",
        expect![[r#"
            SOURCE_FILE@0..44
              STATIC_ITEM@0..44
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..43
                  PATH_EXPR@11..40
                    NAME_REF@11..12
                      IDENT@11..12 "D"
                    COLON2@12..14 "::"
                    GENERIC_ARG_LIST@14..28
                      L_ANGLE@14..15 "<"
                      NAMED_ARG@15..27
                        NAME_REF@15..19
                          IDENT@15..19 "Self"
                        WHITESPACE@19..20 " "
                        EQ@20..21 "="
                        WHITESPACE@21..22 " "
                        PATH_TYPE@22..27
                          NAME_REF@22..27
                            IDENT@22..27 "usize"
                      R_ANGLE@27..28 ">"
                    COLON2@28..30 "::"
                    NAME_REF@30..31
                      IDENT@30..31 "m"
                    MEMBER_GENERIC_ARGS@31..40
                      COLON2@31..33 "::"
                      GENERIC_ARG_LIST@33..40
                        L_ANGLE@33..34 "<"
                        TYPE_ARG@34..39
                          PATH_TYPE@34..39
                            NAME_REF@34..39
                              IDENT@34..39 "usize"
                        R_ANGLE@39..40 ">"
                  ARG_LIST@40..43
                    L_PAREN@40..41 "("
                    PATH_EXPR@41..42
                      NAME_REF@41..42
                        IDENT@41..42 "n"
                    R_PAREN@42..43 ")"
                SEMICOLON@43..44 ";"
        "#]],
    );
}

#[test]
fn member_turbofish_reports_no_parse_error_and_the_rest_parses() {
    // The property the cascade fix bought, kept without the ERROR node: a
    // second-segment turbofish costs ZERO parse errors (it is well-formed
    // syntax — hir is where it is refused), and the statements around it
    // are untouched. If the `::` ever stops being consumed here, this
    // fails long before the snapshots do.
    for input in [
        "static s = fn { let g = P::len::<usize>; let n = 1; };",
        "static s = fn { let g = Shape::Circle::<usize>; let n = 1; };",
        "static s = fn { let g = Pair::<usize>::first::<T>; let n = 1; };",
        "static s = fn { let g = D::<Self = usize>::m::<usize>; let n = 1; };",
        "static s = fn { let g = P::len::<const { 1 + 1 }>; let n = 1; };",
    ] {
        let parse = crate::parse(input);
        assert_eq!(
            parse.errors().len(),
            0,
            "`{input}` should parse clean, got {:?}",
            parse.errors()
        );
        // The trailing `let n = 1;` really is there: recovery did not eat
        // the rest of the block.
        let dump = parse.debug_dump();
        assert_eq!(
            dump.matches("LET_STMT").count(),
            2,
            "`{input}` lost the following statement:\n{dump}"
        );
    }
}

#[test]
fn path_accessors_keep_the_two_lists_apart() {
    // The AST half of the separation: `generic_arg_list()` means the
    // OWNER's list and cannot see a member's, `member_generic_arg_list()`
    // means the second segment's and cannot see the owner's. This is what
    // the ERROR node used to fake — anything that reads a path's arguments
    // gets the owner's, and only code that asks for the member's gets
    // those.
    use crate::ast::{self, AstNode};
    let path_of = |text: &str| {
        crate::parse(text)
            .syntax_node()
            .descendants()
            .find_map(ast::PathExpr::cast)
            .expect("a path expression")
    };
    let owner_text =
        |list: Option<ast::GenericArgList>| list.map(|list| list.syntax().text().to_string());

    let member = path_of("static s = Measured::size::<usize>;");
    assert_eq!(owner_text(member.generic_arg_list()), None);
    assert_eq!(
        owner_text(member.member_generic_arg_list()),
        Some("<usize>".to_owned())
    );

    let owner = path_of("static s = Pair::<usize>::first;");
    assert_eq!(
        owner_text(owner.generic_arg_list()),
        Some("<usize>".to_owned())
    );
    assert_eq!(owner_text(owner.member_generic_arg_list()), None);

    let both = path_of("static s = Pair::<usize>::first::<T>;");
    assert_eq!(
        owner_text(both.generic_arg_list()),
        Some("<usize>".to_owned())
    );
    assert_eq!(
        owner_text(both.member_generic_arg_list()),
        Some("<T>".to_owned())
    );
}

// ---- bare-angle generics (the missing-turbofish typo) -------------------
//
// `Option<T>` for `Option::<T>` (Rust muscle memory) is the most
// predictable typo the language has, and a permanent one: G06 rules bare
// angles illegal for good. X03 then says what to do with a
// never-legal-but-recognizable form — parse it into its real tree shape and
// CORRECT it later — so `Option<T>` builds the very `GENERIC_ARG_LIST`
// `Option::<T>` builds, hover and inference keep working inside the
// arguments, and `validation::reject_bare_angle_generic_args` names the
// missing `::` with a one-token fix. Type position takes the form
// unconditionally; expression position gates on
// `grammar::bare_angle_generic_args_expr`, since `<` is a real operator
// there.

#[test]
fn bare_angle_return_type_parses_as_a_turbofish() {
    // The whole shape, in return-type position: a plain `GENERIC_ARG_LIST`
    // where the turbofish's would be, one diagnostic, and the enclosing
    // `fn`'s body parsed on undisturbed.
    check(
        "static f = fn () -> Option<usize> { 0 };",
        expect![[r#"
        SOURCE_FILE@0..40
          STATIC_ITEM@0..40
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
              PARAM_LIST@14..16
                L_PAREN@14..15 "("
                R_PAREN@15..16 ")"
              WHITESPACE@16..17 " "
              RET_TYPE@17..33
                THIN_ARROW@17..19 "->"
                WHITESPACE@19..20 " "
                PATH_TYPE@20..33
                  NAME_REF@20..26
                    IDENT@20..26 "Option"
                  GENERIC_ARG_LIST@26..33
                    L_ANGLE@26..27 "<"
                    TYPE_ARG@27..32
                      PATH_TYPE@27..32
                        NAME_REF@27..32
                          IDENT@27..32 "usize"
                    R_ANGLE@32..33 ">"
              WHITESPACE@33..34 " "
              BLOCK_EXPR@34..39
                L_BRACE@34..35 "{"
                WHITESPACE@35..36 " "
                LITERAL@36..37
                  INT_NUMBER@36..37 "0"
                WHITESPACE@37..38 " "
                R_BRACE@38..39 "}"
            SEMICOLON@39..40 ";"
        error 20..33: generic arguments use the turbofish: write `Option::<...>`
    "#]],
    );
}

#[test]
fn bare_angle_type_position_every_slot_gets_one_diagnostic() {
    // After `->`, after `:` in an item annotation, a field, a param, and
    // inside a correctly-spelled turbofish's own `TYPE_ARG`. Every slot
    // funnels through the same `type_core` arm, so one rule covers all of
    // them; each of these used to cascade from the stray `<`.
    for input in [
        "static f = fn () -> Option<usize> { 0 };",
        "static f = fn (x: Option<usize>) -> usize { 0 };",
        "type R = struct { x: Option<usize> };",
        "static x: Option<usize> = y;",
        "static x: Pair::<Option<usize>> = y;",
    ] {
        let parse = crate::parse(input);
        let errors = parse.errors();
        assert_eq!(
            errors.len(),
            1,
            "`{input}` should get exactly one diagnostic, got {errors:?}"
        );
        assert_eq!(
            errors[0].message, "generic arguments use the turbofish: write `Option::<...>`",
            "`{input}`"
        );
    }
}

#[test]
fn bare_angle_offers_an_insert_colon2_fix() {
    let parse = crate::parse("static x: Option<usize> = y;");
    let errors = parse.errors();
    assert_eq!(errors.len(), 1);
    let fix = errors[0].fix.as_ref().expect("a fix");
    assert_eq!(fix.label, "Insert `::`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "::");
    // Empty range right after the name, before the bad `<`: a pure
    // insertion, `Option` -> `Option::`.
    assert_eq!(
        fix.edits[0].range,
        crate::TextRange::empty(crate::TextSize::from(16))
    );
}

#[test]
fn bare_angle_nested_generics_two_independent_hints() {
    // `Vec<Vec<T>>` is two typos, so it earns two hints: the arguments are
    // parsed for real, so the inner `Vec<T>` reaches the same `type_core`
    // arm as a `TYPE_ARG` of its own. Each hint carries its own one-token
    // fix and the two apply independently — the user is never told to fix
    // one, re-run, and discover the other.
    check(
        "static x: Vec<Vec<T>> = y;",
        expect![[r#"
        SOURCE_FILE@0..26
          STATIC_ITEM@0..26
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            COLON@8..9 ":"
            WHITESPACE@9..10 " "
            PATH_TYPE@10..21
              NAME_REF@10..13
                IDENT@10..13 "Vec"
              GENERIC_ARG_LIST@13..21
                L_ANGLE@13..14 "<"
                TYPE_ARG@14..20
                  PATH_TYPE@14..20
                    NAME_REF@14..17
                      IDENT@14..17 "Vec"
                    GENERIC_ARG_LIST@17..20
                      L_ANGLE@17..18 "<"
                      TYPE_ARG@18..19
                        PATH_TYPE@18..19
                          NAME_REF@18..19
                            IDENT@18..19 "T"
                      R_ANGLE@19..20 ">"
                R_ANGLE@20..21 ">"
            WHITESPACE@21..22 " "
            EQ@22..23 "="
            WHITESPACE@23..24 " "
            PATH_EXPR@24..25
              NAME_REF@24..25
                IDENT@24..25 "y"
            SEMICOLON@25..26 ";"
        error 10..21: generic arguments use the turbofish: write `Vec::<...>`
        error 14..20: generic arguments use the turbofish: write `Vec::<...>`
    "#]],
    );
}

#[test]
fn bare_angle_expr_call_heuristic_fires() {
    // Expression position, positive: `f<T>(...)` — a balanced group
    // immediately followed by `(`. The tree is exactly `f::<usize>(3)`'s,
    // minus the `COLON2`.
    check(
        "static x = f<usize>(3);",
        expect![[r#"
        SOURCE_FILE@0..23
          STATIC_ITEM@0..23
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            CALL_EXPR@11..22
              PATH_EXPR@11..19
                NAME_REF@11..12
                  IDENT@11..12 "f"
                GENERIC_ARG_LIST@12..19
                  L_ANGLE@12..13 "<"
                  TYPE_ARG@13..18
                    PATH_TYPE@13..18
                      NAME_REF@13..18
                        IDENT@13..18 "usize"
                  R_ANGLE@18..19 ">"
              ARG_LIST@19..22
                L_PAREN@19..20 "("
                LITERAL@20..21
                  INT_NUMBER@20..21 "3"
                R_PAREN@21..22 ")"
            SEMICOLON@22..23 ";"
        error 11..19: generic arguments use the turbofish: write `f::<...>`
    "#]],
    );
}

#[test]
fn bare_angle_expr_assoc_heuristic_keeps_the_segment() {
    // Expression position, positive: `f<T>::assoc` — a balanced group
    // followed by `::`, the other trusted shape. The trailing segment gets
    // the same handling `f::<usize>::assoc` gets (its own `NAME_REF` and
    // its own possible turbofish), and is left out of the hint's range.
    check(
        "static x = f<usize>::assoc;",
        expect![[r#"
        SOURCE_FILE@0..27
          STATIC_ITEM@0..27
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            PATH_EXPR@11..26
              NAME_REF@11..12
                IDENT@11..12 "f"
              GENERIC_ARG_LIST@12..19
                L_ANGLE@12..13 "<"
                TYPE_ARG@13..18
                  PATH_TYPE@13..18
                    NAME_REF@13..18
                      IDENT@13..18 "usize"
                R_ANGLE@18..19 ">"
              COLON2@19..21 "::"
              NAME_REF@21..26
                IDENT@21..26 "assoc"
            SEMICOLON@26..27 ";"
        error 11..19: generic arguments use the turbofish: write `f::<...>`
    "#]],
    );
}

#[test]
fn bare_angle_comparison_untouched() {
    // Expression position, negative: a plain `a < b` must never get the
    // hint — `scan_bare_angle_group` never even confirms a balanced group
    // (the statement's `;` bails it), so this is a pure no-op.
    check(
        "static x = fn { let y = a < b; y };",
        expect![[r#"
        SOURCE_FILE@0..35
          STATIC_ITEM@0..35
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            FN_LITERAL@11..34
              FN_KW@11..13 "fn"
              WHITESPACE@13..14 " "
              BLOCK_EXPR@14..34
                L_BRACE@14..15 "{"
                WHITESPACE@15..16 " "
                LET_STMT@16..30
                  LET_KW@16..19 "let"
                  WHITESPACE@19..20 " "
                  BIND_PAT@20..21
                    NAME@20..21
                      IDENT@20..21 "y"
                  WHITESPACE@21..22 " "
                  EQ@22..23 "="
                  WHITESPACE@23..24 " "
                  BIN_EXPR@24..29
                    PATH_EXPR@24..25
                      NAME_REF@24..25
                        IDENT@24..25 "a"
                    WHITESPACE@25..26 " "
                    L_ANGLE@26..27 "<"
                    WHITESPACE@27..28 " "
                    PATH_EXPR@28..29
                      NAME_REF@28..29
                        IDENT@28..29 "b"
                  SEMICOLON@29..30 ";"
                WHITESPACE@30..31 " "
                PATH_EXPR@31..32
                  NAME_REF@31..32
                    IDENT@31..32 "y"
                WHITESPACE@32..33 " "
                R_BRACE@33..34 "}"
            SEMICOLON@34..35 ";"
    "#]],
    );
}

#[test]
fn bare_angle_call_comparison_untouched() {
    // Expression position, negative: `f(x) < g(y)`. The gate only fires
    // right after a bare NAME in `primary_expr`'s `IDENT` arm, before any
    // call parens are parsed, so this `<` is never even offered to it.
    check(
        "static x = fn { let z = f(x) < g(y); z };",
        expect![[r#"
        SOURCE_FILE@0..41
          STATIC_ITEM@0..41
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            FN_LITERAL@11..40
              FN_KW@11..13 "fn"
              WHITESPACE@13..14 " "
              BLOCK_EXPR@14..40
                L_BRACE@14..15 "{"
                WHITESPACE@15..16 " "
                LET_STMT@16..36
                  LET_KW@16..19 "let"
                  WHITESPACE@19..20 " "
                  BIND_PAT@20..21
                    NAME@20..21
                      IDENT@20..21 "z"
                  WHITESPACE@21..22 " "
                  EQ@22..23 "="
                  WHITESPACE@23..24 " "
                  BIN_EXPR@24..35
                    CALL_EXPR@24..28
                      PATH_EXPR@24..25
                        NAME_REF@24..25
                          IDENT@24..25 "f"
                      ARG_LIST@25..28
                        L_PAREN@25..26 "("
                        PATH_EXPR@26..27
                          NAME_REF@26..27
                            IDENT@26..27 "x"
                        R_PAREN@27..28 ")"
                    WHITESPACE@28..29 " "
                    L_ANGLE@29..30 "<"
                    WHITESPACE@30..31 " "
                    CALL_EXPR@31..35
                      PATH_EXPR@31..32
                        NAME_REF@31..32
                          IDENT@31..32 "g"
                      ARG_LIST@32..35
                        L_PAREN@32..33 "("
                        PATH_EXPR@33..34
                          NAME_REF@33..34
                            IDENT@33..34 "y"
                        R_PAREN@34..35 ")"
                  SEMICOLON@35..36 ";"
                WHITESPACE@36..37 " "
                PATH_EXPR@37..38
                  NAME_REF@37..38
                    IDENT@37..38 "z"
                WHITESPACE@38..39 " "
                R_BRACE@39..40 "}"
            SEMICOLON@40..41 ";"
    "#]],
    );
}

#[test]
fn bare_angle_comparison_chain_untouched() {
    // Expression position, negative: `a < b == c > (d)` closes an angle
    // group and IS followed by `(`, so shape alone would fire. The
    // equality/ordering operators can never appear in a generic argument,
    // so finding one at nest 0 says "comparison chain" outright.
    check(
        "static x = fn { let z = a < b == c > (d); z };",
        expect![[r#"
        SOURCE_FILE@0..46
          STATIC_ITEM@0..46
            STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "x"
            WHITESPACE@8..9 " "
            EQ@9..10 "="
            WHITESPACE@10..11 " "
            FN_LITERAL@11..45
              FN_KW@11..13 "fn"
              WHITESPACE@13..14 " "
              BLOCK_EXPR@14..45
                L_BRACE@14..15 "{"
                WHITESPACE@15..16 " "
                LET_STMT@16..41
                  LET_KW@16..19 "let"
                  WHITESPACE@19..20 " "
                  BIND_PAT@20..21
                    NAME@20..21
                      IDENT@20..21 "z"
                  WHITESPACE@21..22 " "
                  EQ@22..23 "="
                  WHITESPACE@23..24 " "
                  BIN_EXPR@24..40
                    BIN_EXPR@24..34
                      BIN_EXPR@24..29
                        PATH_EXPR@24..25
                          NAME_REF@24..25
                            IDENT@24..25 "a"
                        WHITESPACE@25..26 " "
                        L_ANGLE@26..27 "<"
                        WHITESPACE@27..28 " "
                        PATH_EXPR@28..29
                          NAME_REF@28..29
                            IDENT@28..29 "b"
                      WHITESPACE@29..30 " "
                      EQ2@30..32 "=="
                      WHITESPACE@32..33 " "
                      PATH_EXPR@33..34
                        NAME_REF@33..34
                          IDENT@33..34 "c"
                    WHITESPACE@34..35 " "
                    R_ANGLE@35..36 ">"
                    WHITESPACE@36..37 " "
                    PAREN_EXPR@37..40
                      L_PAREN@37..38 "("
                      PATH_EXPR@38..39
                        NAME_REF@38..39
                          IDENT@38..39 "d"
                      R_PAREN@39..40 ")"
                  SEMICOLON@40..41 ";"
                WHITESPACE@41..42 " "
                PATH_EXPR@42..43
                  NAME_REF@42..43
                    IDENT@42..43 "z"
                WHITESPACE@43..44 " "
                R_BRACE@44..45 "}"
            SEMICOLON@45..46 ";"
    "#]],
    );
}

#[test]
fn bare_angle_ordering_chain_with_parens_is_read_as_a_call() {
    // The residual of the heuristic, and the only shape a comma cannot
    // separate: `a < b > (d)` is token-for-token `f::<T>(d)`. It is read as
    // the call because no well-typed program is spelled that way (`a < b`
    // is a `bool`, and `>` wants numbers), so nothing that compiles is at
    // stake — but the misread does cost the whole expression, not just this
    // message: see `hir`'s `bare_angle_ordering_chain_is_read_as_a_call`.
    assert_eq!(
        messages("static x = fn { let z = a < b > (d); z };"),
        ["generic arguments use the turbofish: write `a::<...>`"]
    );
}

#[test]
fn bare_angle_top_level_comma_keeps_the_comparison_reading() {
    // Why the call shape refuses a top-level comma: `g(a < b, c > (d))` is
    // two comparisons passed as arguments — a program that compiles — and
    // is otherwise token-for-token a one-argument call on `a::<b, c>`. The
    // price is that a MULTI-argument bare-angle call goes uncorrected;
    // that shape is exactly the ambiguous one.
    let valid =
        "static n = fn (a: usize, b: usize, c: usize, d: usize) -> bool { g(a < b, c > (d)) };";
    assert_eq!(messages(valid), Vec::<String>::new());
    assert!(!node_kinds(valid).contains(&"GENERIC_ARG_LIST".to_owned()));
    assert_eq!(
        messages("static u = fn { p<usize, usize>(1, 2) };")
            .iter()
            .filter(|message| message.contains("turbofish"))
            .count(),
        0
    );
}

#[test]
fn bare_angle_angles_inside_a_pair_close_nothing() {
    // The other half of the same rule: a `>` inside a pair opened after the
    // `<` is a comparison in someone else's expression, not this group's
    // close. Counting it would read `a < f(b >` as the group and correct a
    // program that compiles — commas cannot help, since there is none.
    let valid = "static n = fn (a: usize, b: usize, c: usize) -> bool { a < f(b > (c)) };";
    assert_eq!(messages(valid), Vec::<String>::new());
    assert!(!node_kinds(valid).contains(&"GENERIC_ARG_LIST".to_owned()));
}

#[test]
fn bare_angle_match_arms_untouched() {
    // Match arms need both tells. The call shape is caught by the arm
    // separator being a top-level comma; the `::` shape, which accepts
    // commas, is caught by the `=>` itself — an arm arrow can never appear
    // in an argument list.
    for input in [
        "static m = fn (k: usize, a: usize, b: usize) -> bool { match k { 0 => a < b, _ => a > (b + 1) } };",
        "static m = fn (k: usize, a: usize, b: usize) -> usize { match k { 0 => a < b, _ => b > ::Foo } };",
    ] {
        assert!(!node_kinds(input).contains(&"GENERIC_ARG_LIST".to_owned()));
    }
}

#[test]
fn bare_angle_assoc_shape_takes_a_comma() {
    // The `::` follow-shape has no comparison reading at all (nothing valid
    // puts `::` after one), so it corrects the multi-argument lists the
    // call shape has to leave alone.
    assert_eq!(
        messages("static x = Pair<A, B>::first;"),
        ["generic arguments use the turbofish: write `Pair::<...>`"]
    );
}

#[test]
fn bare_angle_unclosed_group_parses_as_the_turbofish_would() {
    // The typo is committed to, not bailed out of, when the group never
    // closes: `generic_arg_list`'s own no-progress break and
    // `expect_after_prev(R_ANGLE)` bound `Boxed<usize` exactly where they
    // bound `Boxed::<usize`, so mid-typing behaves the same on both
    // spellings — the same tree and the same errors, plus the correction.
    assert_eq!(
        node_kinds("static f = fn (b: Boxed<usize) -> usize { b.value };"),
        without_colon2(node_kinds(
            "static f = fn (b: Boxed::<usize) -> usize { b.value };"
        ))
    );
    assert_eq!(
        messages("static f = fn (b: Boxed<usize) -> usize { b.value };"),
        prepend(
            "generic arguments use the turbofish: write `Boxed::<...>`",
            messages("static f = fn (b: Boxed::<usize) -> usize { b.value };")
        )
    );
}

#[test]
fn bare_angle_junk_arguments_report_as_the_turbofish_would() {
    // Same trade in the other direction: because the arguments are parsed
    // rather than swallowed, junk inside them is reported exactly as the
    // turbofish spelling reports it, instead of hiding behind the typo.
    assert_eq!(
        node_kinds("static x: Boxed<N + 1> = y;"),
        without_colon2(node_kinds("static x: Boxed::<N + 1> = y;"))
    );
    assert_eq!(
        messages("static x: Boxed<N + 1> = y;"),
        prepend(
            "generic arguments use the turbofish: write `Boxed::<...>`",
            messages("static x: Boxed::<N + 1> = y;")
        )
    );
}

#[test]
fn bare_angle_typo_survives_at_the_member_boundary() {
    // A bad member's typo must not fall the enclosing `impl` out of the
    // parser: the member after it parses clean.
    let parse = crate::parse(
        "type Pair = struct { a: usize } with { \
         impl Self { \
         bad = fn () -> Option<usize> { 0 }; \
         after = fn (s: Self) -> usize { s.a }; \
         } };",
    );
    assert_eq!(
        parse.errors().len(),
        1,
        "expected only the turbofish hint, got {:?}",
        parse.errors()
    );
    assert_eq!(
        parse.errors()[0].message,
        "generic arguments use the turbofish: write `Option::<...>`"
    );
    let dump = parse.debug_dump();
    assert_eq!(
        dump.matches("MEMBER@").count(),
        2,
        "`after` did not survive as its own member:\n{dump}"
    );
    assert!(
        dump.contains("\"after\""),
        "`after`'s name did not survive:\n{dump}"
    );
}

#[test]
fn bare_angle_typo_survives_at_the_item_boundary() {
    // One level up: a bad ITEM's typo must not fall the rest of the file
    // out of the parser.
    let parse = crate::parse(
        "type Bad = struct { x: Option<usize> };\n\
         static after = 1;\n",
    );
    assert_eq!(
        parse.errors().len(),
        1,
        "expected only the turbofish hint, got {:?}",
        parse.errors()
    );
    let dump = parse.debug_dump();
    assert!(
        dump.contains("STATIC_ITEM") && dump.contains("\"after\""),
        "the item after the bad one did not survive:\n{dump}"
    );
}

/// The tree's node/token kinds in order, with offsets and text dropped —
/// enough to say two parses have the same SHAPE without pinning the
/// offsets a `::` shifts.
fn node_kinds(input: &str) -> Vec<String> {
    crate::parse(input)
        .debug_dump()
        .lines()
        .filter_map(|line| line.split_once('@'))
        .map(|(kind, _)| kind.trim().to_owned())
        .collect()
}

fn without_colon2(kinds: Vec<String>) -> Vec<String> {
    kinds.into_iter().filter(|kind| kind != "COLON2").collect()
}

fn messages(input: &str) -> Vec<String> {
    crate::parse(input)
        .errors()
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

fn prepend(first: &str, rest: Vec<String>) -> Vec<String> {
    std::iter::once(first.to_owned()).chain(rest).collect()
}

// ---- generics: type-declaration binders ----

#[test]
fn generic_binder_on_struct_literal() {
    check(
        "type Pair = struct::<T> { a: T, b: T };",
        expect![[r#"
            SOURCE_FILE@0..39
              TYPE_ITEM@0..39
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

#[test]
fn raw_pointer_types_parse() {
    check(
        "static f = fn(p: usize.&raw, q: usize.&raw mut) -> usize.&raw mut { q };",
        expect![[r#"
            SOURCE_FILE@0..72
              STATIC_ITEM@0..72
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
fn safe_borrow_expr() {
    // `x.&` / `x.&mut` (DOT AMP without `raw`) — the postfix safe borrows.
    // Their own node, no reservation error; hir owns them from here.
    check(
        "static f = fn { let a = x.&; let b = y.&mut; };",
        expect![[r#"
            SOURCE_FILE@0..47
              STATIC_ITEM@0..47
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

#[test]
fn unsafe_block_parses() {
    check(
        "static f = fn { unsafe { p.* } };",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..33
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
                    UNSAFE_BLOCK_EXPR@16..30
                      UNSAFE_KW@16..22 "unsafe"
                      WHITESPACE@22..23 " "
                      BLOCK_EXPR@23..30
                        L_BRACE@23..24 "{"
                        WHITESPACE@24..25 " "
                        DEREF_EXPR@25..28
                          PATH_EXPR@25..26
                            NAME_REF@25..26
                              IDENT@25..26 "p"
                          DOT@26..27 "."
                          STAR@27..28 "*"
                        WHITESPACE@28..29 " "
                        R_BRACE@29..30 "}"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
        "#]],
    );
}

#[test]
fn unsafe_fn_is_reserved() {
    check(
        "static f = unsafe fn() {};",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                UNSAFE_BLOCK_EXPR@11..25
                  UNSAFE_KW@11..17 "unsafe"
                  WHITESPACE@17..18 " "
                  FN_LITERAL@18..25
                    FN_KW@18..20 "fn"
                    PARAM_LIST@20..22
                      L_PAREN@20..21 "("
                      R_PAREN@21..22 ")"
                    WHITESPACE@22..23 " "
                    BLOCK_EXPR@23..25
                      L_BRACE@23..24 "{"
                      R_BRACE@24..25 "}"
                SEMICOLON@25..26 ";"
            error 18..25: `unsafe fn` is not supported yet; use `unsafe { ... }` blocks inside a plain `fn`
        "#]],
    );
}

#[test]
fn plain_reference_type_is_reserved() {
    check(
        "static f = fn(s: &str) {};",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..25
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..22
                    L_PAREN@13..14 "("
                    PARAM@14..21
                      BIND_PAT@14..15
                        NAME@14..15
                          IDENT@14..15 "s"
                      COLON@15..16 ":"
                      WHITESPACE@16..17 " "
                      REF_TYPE@17..21
                        AMP@17..18 "&"
                        PATH_TYPE@18..21
                          NAME_REF@18..21
                            IDENT@18..21 "str"
                    R_PAREN@21..22 ")"
                  WHITESPACE@22..23 " "
                  BLOCK_EXPR@23..25
                    L_BRACE@23..24 "{"
                    R_BRACE@24..25 "}"
                SEMICOLON@25..26 ";"
            error 17..21: references are not supported yet
        "#]],
    );
}

// ---- fixed-size arrays ----

#[test]
fn array_type_positions() {
    check(
        "static x: [usize; 3] = y;",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..20
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  SEMICOLON@16..17 ";"
                  WHITESPACE@17..18 " "
                  CONST_ARG@18..19
                    LITERAL@18..19
                      INT_NUMBER@18..19 "3"
                  R_BRACKET@19..20 "]"
                WHITESPACE@20..21 " "
                EQ@21..22 "="
                WHITESPACE@22..23 " "
                PATH_EXPR@23..24
                  NAME_REF@23..24
                    IDENT@23..24 "y"
                SEMICOLON@24..25 ";"
        "#]],
    );
}

#[test]
fn array_type_nested() {
    check(
        "static m: [[usize; 2]; 3] = y;",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "m"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..25
                  L_BRACKET@10..11 "["
                  ARRAY_TYPE@11..21
                    L_BRACKET@11..12 "["
                    PATH_TYPE@12..17
                      NAME_REF@12..17
                        IDENT@12..17 "usize"
                    SEMICOLON@17..18 ";"
                    WHITESPACE@18..19 " "
                    CONST_ARG@19..20
                      LITERAL@19..20
                        INT_NUMBER@19..20 "2"
                    R_BRACKET@20..21 "]"
                  SEMICOLON@21..22 ";"
                  WHITESPACE@22..23 " "
                  CONST_ARG@23..24
                    LITERAL@23..24
                      INT_NUMBER@23..24 "3"
                  R_BRACKET@24..25 "]"
                WHITESPACE@25..26 " "
                EQ@26..27 "="
                WHITESPACE@27..28 " "
                PATH_EXPR@28..29
                  NAME_REF@28..29
                    IDENT@28..29 "y"
                SEMICOLON@29..30 ";"
        "#]],
    );
}

#[test]
fn array_type_const_param_length() {
    check(
        "static f = fn::<const N: usize>(b: [usize; N]) -> usize { 0 };",
        expect![[r#"
            SOURCE_FILE@0..62
              STATIC_ITEM@0..62
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..61
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..31
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    CONST_PARAM@16..30
                      CONST_KW@16..21 "const"
                      WHITESPACE@21..22 " "
                      NAME@22..23
                        IDENT@22..23 "N"
                      COLON@23..24 ":"
                      WHITESPACE@24..25 " "
                      PATH_TYPE@25..30
                        NAME_REF@25..30
                          IDENT@25..30 "usize"
                    R_ANGLE@30..31 ">"
                  PARAM_LIST@31..46
                    L_PAREN@31..32 "("
                    PARAM@32..45
                      BIND_PAT@32..33
                        NAME@32..33
                          IDENT@32..33 "b"
                      COLON@33..34 ":"
                      WHITESPACE@34..35 " "
                      ARRAY_TYPE@35..45
                        L_BRACKET@35..36 "["
                        PATH_TYPE@36..41
                          NAME_REF@36..41
                            IDENT@36..41 "usize"
                        SEMICOLON@41..42 ";"
                        WHITESPACE@42..43 " "
                        CONST_ARG@43..44
                          PATH_EXPR@43..44
                            NAME_REF@43..44
                              IDENT@43..44 "N"
                        R_BRACKET@44..45 "]"
                    R_PAREN@45..46 ")"
                  WHITESPACE@46..47 " "
                  RET_TYPE@47..55
                    THIN_ARROW@47..49 "->"
                    WHITESPACE@49..50 " "
                    PATH_TYPE@50..55
                      NAME_REF@50..55
                        IDENT@50..55 "usize"
                  WHITESPACE@55..56 " "
                  BLOCK_EXPR@56..61
                    L_BRACE@56..57 "{"
                    WHITESPACE@57..58 " "
                    LITERAL@58..59
                      INT_NUMBER@58..59 "0"
                    WHITESPACE@59..60 " "
                    R_BRACE@60..61 "}"
                SEMICOLON@61..62 ";"
        "#]],
    );
}

#[test]
fn array_type_const_block_length_parses() {
    // `const { ... }` parses as the length (resilience); hir rejects it.
    check(
        "static x: [usize; const { 3 }] = y;",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..30
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  SEMICOLON@16..17 ";"
                  WHITESPACE@17..18 " "
                  CONST_ARG@18..29
                    CONST_BLOCK_EXPR@18..29
                      CONST_KW@18..23 "const"
                      WHITESPACE@23..24 " "
                      BLOCK_EXPR@24..29
                        L_BRACE@24..25 "{"
                        WHITESPACE@25..26 " "
                        LITERAL@26..27
                          INT_NUMBER@26..27 "3"
                        WHITESPACE@27..28 " "
                        R_BRACE@28..29 "}"
                  R_BRACKET@29..30 "]"
                WHITESPACE@30..31 " "
                EQ@31..32 "="
                WHITESPACE@32..33 " "
                PATH_EXPR@33..34
                  NAME_REF@33..34
                    IDENT@33..34 "y"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn array_type_missing_semicolon() {
    check(
        "static x: [usize] = y;",
        expect![[r#"
            SOURCE_FILE@0..22
              STATIC_ITEM@0..22
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                ARRAY_TYPE@10..17
                  L_BRACKET@10..11 "["
                  PATH_TYPE@11..16
                    NAME_REF@11..16
                      IDENT@11..16 "usize"
                  R_BRACKET@16..17 "]"
                WHITESPACE@17..18 " "
                EQ@18..19 "="
                WHITESPACE@19..20 " "
                PATH_EXPR@20..21
                  NAME_REF@20..21
                    IDENT@20..21 "y"
                SEMICOLON@21..22 ";"
            error 16..17: expected `;` (array types are written `[T; N]`)
        "#]],
    );
}

#[test]
fn array_literal_and_repeat() {
    check(
        "static f = fn { let a = [1, 2, 3]; let b = [0; 4]; let c = []; };",
        expect![[r#"
            SOURCE_FILE@0..65
              STATIC_ITEM@0..65
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
                    LET_STMT@16..34
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      ARRAY_EXPR@24..33
                        L_BRACKET@24..25 "["
                        LITERAL@25..26
                          INT_NUMBER@25..26 "1"
                        COMMA@26..27 ","
                        WHITESPACE@27..28 " "
                        LITERAL@28..29
                          INT_NUMBER@28..29 "2"
                        COMMA@29..30 ","
                        WHITESPACE@30..31 " "
                        LITERAL@31..32
                          INT_NUMBER@31..32 "3"
                        R_BRACKET@32..33 "]"
                      SEMICOLON@33..34 ";"
                    WHITESPACE@34..35 " "
                    LET_STMT@35..50
                      LET_KW@35..38 "let"
                      WHITESPACE@38..39 " "
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "b"
                      WHITESPACE@40..41 " "
                      EQ@41..42 "="
                      WHITESPACE@42..43 " "
                      ARRAY_EXPR@43..49
                        L_BRACKET@43..44 "["
                        LITERAL@44..45
                          INT_NUMBER@44..45 "0"
                        SEMICOLON@45..46 ";"
                        WHITESPACE@46..47 " "
                        LITERAL@47..48
                          INT_NUMBER@47..48 "4"
                        R_BRACKET@48..49 "]"
                      SEMICOLON@49..50 ";"
                    WHITESPACE@50..51 " "
                    LET_STMT@51..62
                      LET_KW@51..54 "let"
                      WHITESPACE@54..55 " "
                      BIND_PAT@55..56
                        NAME@55..56
                          IDENT@55..56 "c"
                      WHITESPACE@56..57 " "
                      EQ@57..58 "="
                      WHITESPACE@58..59 " "
                      ARRAY_EXPR@59..61
                        L_BRACKET@59..60 "["
                        R_BRACKET@60..61 "]"
                      SEMICOLON@61..62 ";"
                    WHITESPACE@62..63 " "
                    R_BRACE@63..64 "}"
                SEMICOLON@64..65 ";"
        "#]],
    );
}

#[test]
fn array_literal_trailing_comma() {
    check(
        "static a = [1, 2,];",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "a"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                ARRAY_EXPR@11..18
                  L_BRACKET@11..12 "["
                  LITERAL@12..13
                    INT_NUMBER@12..13 "1"
                  COMMA@13..14 ","
                  WHITESPACE@14..15 " "
                  LITERAL@15..16
                    INT_NUMBER@15..16 "2"
                  COMMA@16..17 ","
                  R_BRACKET@17..18 "]"
                SEMICOLON@18..19 ";"
        "#]],
    );
}

#[test]
fn index_chains() {
    check(
        "static f = fn { m[0][1] + p.buf[i].x };",
        expect![[r#"
            SOURCE_FILE@0..39
              STATIC_ITEM@0..39
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..38
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..38
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    BIN_EXPR@16..36
                      INDEX_EXPR@16..23
                        INDEX_EXPR@16..20
                          PATH_EXPR@16..17
                            NAME_REF@16..17
                              IDENT@16..17 "m"
                          L_BRACKET@17..18 "["
                          LITERAL@18..19
                            INT_NUMBER@18..19 "0"
                          R_BRACKET@19..20 "]"
                        L_BRACKET@20..21 "["
                        LITERAL@21..22
                          INT_NUMBER@21..22 "1"
                        R_BRACKET@22..23 "]"
                      WHITESPACE@23..24 " "
                      PLUS@24..25 "+"
                      WHITESPACE@25..26 " "
                      FIELD_EXPR@26..36
                        INDEX_EXPR@26..34
                          FIELD_EXPR@26..31
                            PATH_EXPR@26..27
                              NAME_REF@26..27
                                IDENT@26..27 "p"
                            DOT@27..28 "."
                            NAME_REF@28..31
                              IDENT@28..31 "buf"
                          L_BRACKET@31..32 "["
                          PATH_EXPR@32..33
                            NAME_REF@32..33
                              IDENT@32..33 "i"
                          R_BRACKET@33..34 "]"
                        DOT@34..35 "."
                        NAME_REF@35..36
                          IDENT@35..36 "x"
                    WHITESPACE@36..37 " "
                    R_BRACE@37..38 "}"
                SEMICOLON@38..39 ";"
        "#]],
    );
}

#[test]
fn index_assign_statement() {
    check(
        "static f = fn { a[0] = 5; m[0][1] = 2; };",
        expect![[r#"
            SOURCE_FILE@0..41
              STATIC_ITEM@0..41
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
                  BLOCK_EXPR@14..40
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    ASSIGN_STMT@16..25
                      INDEX_EXPR@16..20
                        PATH_EXPR@16..17
                          NAME_REF@16..17
                            IDENT@16..17 "a"
                        L_BRACKET@17..18 "["
                        LITERAL@18..19
                          INT_NUMBER@18..19 "0"
                        R_BRACKET@19..20 "]"
                      WHITESPACE@20..21 " "
                      EQ@21..22 "="
                      WHITESPACE@22..23 " "
                      LITERAL@23..24
                        INT_NUMBER@23..24 "5"
                      SEMICOLON@24..25 ";"
                    WHITESPACE@25..26 " "
                    ASSIGN_STMT@26..38
                      INDEX_EXPR@26..33
                        INDEX_EXPR@26..30
                          PATH_EXPR@26..27
                            NAME_REF@26..27
                              IDENT@26..27 "m"
                          L_BRACKET@27..28 "["
                          LITERAL@28..29
                            INT_NUMBER@28..29 "0"
                          R_BRACKET@29..30 "]"
                        L_BRACKET@30..31 "["
                        LITERAL@31..32
                          INT_NUMBER@31..32 "1"
                        R_BRACKET@32..33 "]"
                      WHITESPACE@33..34 " "
                      EQ@34..35 "="
                      WHITESPACE@35..36 " "
                      LITERAL@36..37
                        INT_NUMBER@36..37 "2"
                      SEMICOLON@37..38 ";"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn unary_minus_parses_as_a_neg_expr() {
    check(
        "static x: i32 = -5;",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..13
                  NAME_REF@10..13
                    IDENT@10..13 "i32"
                WHITESPACE@13..14 " "
                EQ@14..15 "="
                WHITESPACE@15..16 " "
                NEG_EXPR@16..18
                  MINUS@16..17 "-"
                  LITERAL@17..18
                    INT_NUMBER@17..18 "5"
                SEMICOLON@18..19 ";"
        "#]],
    );
}

#[test]
fn unary_minus_binds_tighter_than_binary_operators_but_looser_than_postfix() {
    // `-a.b + c` is `(-(a.b)) + c`: the operand is a primary expression
    // plus its postfix chain, and the negation is an ordinary operand of
    // the sum.
    check(
        "static x: i32 = -a.b + c;",
        expect![[r#"
            SOURCE_FILE@0..25
              STATIC_ITEM@0..25
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..13
                  NAME_REF@10..13
                    IDENT@10..13 "i32"
                WHITESPACE@13..14 " "
                EQ@14..15 "="
                WHITESPACE@15..16 " "
                BIN_EXPR@16..24
                  NEG_EXPR@16..20
                    MINUS@16..17 "-"
                    FIELD_EXPR@17..20
                      PATH_EXPR@17..18
                        NAME_REF@17..18
                          IDENT@17..18 "a"
                      DOT@18..19 "."
                      NAME_REF@19..20
                        IDENT@19..20 "b"
                  WHITESPACE@20..21 " "
                  PLUS@21..22 "+"
                  WHITESPACE@22..23 " "
                  PATH_EXPR@23..24
                    NAME_REF@23..24
                      IDENT@23..24 "c"
                SEMICOLON@24..25 ";"
        "#]],
    );
}

#[test]
fn unary_minus_can_carry_a_break_value() {
    check(
        "static x: i32 = loop { break -1; };",
        expect![[r#"
            SOURCE_FILE@0..35
              STATIC_ITEM@0..35
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..13
                  NAME_REF@10..13
                    IDENT@10..13 "i32"
                WHITESPACE@13..14 " "
                EQ@14..15 "="
                WHITESPACE@15..16 " "
                LOOP_EXPR@16..34
                  LOOP_KW@16..20 "loop"
                  WHITESPACE@20..21 " "
                  BLOCK_EXPR@21..34
                    L_BRACE@21..22 "{"
                    WHITESPACE@22..23 " "
                    EXPR_STMT@23..32
                      BREAK_EXPR@23..31
                        BREAK_KW@23..28 "break"
                        WHITESPACE@28..29 " "
                        NEG_EXPR@29..31
                          MINUS@29..30 "-"
                          LITERAL@30..31
                            INT_NUMBER@30..31 "1"
                      SEMICOLON@31..32 ";"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

// ---- attachment `with`-chains -------------------------------------------

#[test]
fn with_chain_inherent_impl() {
    check(
        r#"
type Point = struct { x: usize, y: usize } with {
    impl Self {
        len = fn(p: Self) -> usize { p.x };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..120
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..119
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..11
                  IDENT@6..11 "Point"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                RECORD_EXPR@14..43
                  STRUCT_KW@14..20 "struct"
                  WHITESPACE@20..21 " "
                  L_BRACE@21..22 "{"
                  WHITESPACE@22..23 " "
                  RECORD_EXPR_FIELD@23..31
                    NAME_REF@23..24
                      IDENT@23..24 "x"
                    COLON@24..25 ":"
                    WHITESPACE@25..26 " "
                    PATH_TYPE@26..31
                      NAME_REF@26..31
                        IDENT@26..31 "usize"
                  COMMA@31..32 ","
                  WHITESPACE@32..33 " "
                  RECORD_EXPR_FIELD@33..41
                    NAME_REF@33..34
                      IDENT@33..34 "y"
                    COLON@34..35 ":"
                    WHITESPACE@35..36 " "
                    PATH_TYPE@36..41
                      NAME_REF@36..41
                        IDENT@36..41 "usize"
                  WHITESPACE@41..42 " "
                  R_BRACE@42..43 "}"
                WHITESPACE@43..44 " "
                WITH_GROUP@44..118
                  WITH_KW@44..48 "with"
                  WHITESPACE@48..49 " "
                  L_BRACE@49..50 "{"
                  WHITESPACE@50..55 "\n    "
                  IMPL_ELEMENT@55..116
                    IMPL_KW@55..59 "impl"
                    WHITESPACE@59..60 " "
                    PATH_TYPE@60..64
                      NAME_REF@60..64
                        IDENT@60..64 "Self"
                    WHITESPACE@64..65 " "
                    L_BRACE@65..66 "{"
                    WHITESPACE@66..75 "\n        "
                    MEMBER@75..110
                      NAME@75..78
                        IDENT@75..78 "len"
                      WHITESPACE@78..79 " "
                      EQ@79..80 "="
                      WHITESPACE@80..81 " "
                      FN_LITERAL@81..109
                        FN_KW@81..83 "fn"
                        PARAM_LIST@83..92
                          L_PAREN@83..84 "("
                          PARAM@84..91
                            BIND_PAT@84..85
                              NAME@84..85
                                IDENT@84..85 "p"
                            COLON@85..86 ":"
                            WHITESPACE@86..87 " "
                            PATH_TYPE@87..91
                              NAME_REF@87..91
                                IDENT@87..91 "Self"
                          R_PAREN@91..92 ")"
                        WHITESPACE@92..93 " "
                        RET_TYPE@93..101
                          THIN_ARROW@93..95 "->"
                          WHITESPACE@95..96 " "
                          PATH_TYPE@96..101
                            NAME_REF@96..101
                              IDENT@96..101 "usize"
                        WHITESPACE@101..102 " "
                        BLOCK_EXPR@102..109
                          L_BRACE@102..103 "{"
                          WHITESPACE@103..104 " "
                          FIELD_EXPR@104..107
                            PATH_EXPR@104..105
                              NAME_REF@104..105
                                IDENT@104..105 "p"
                            DOT@105..106 "."
                            NAME_REF@106..107
                              IDENT@106..107 "x"
                          WHITESPACE@107..108 " "
                          R_BRACE@108..109 "}"
                      SEMICOLON@109..110 ";"
                    WHITESPACE@110..115 "\n    "
                    R_BRACE@115..116 "}"
                  WHITESPACE@116..117 "\n"
                  R_BRACE@117..118 "}"
                SEMICOLON@118..119 ";"
              WHITESPACE@119..120 "\n"
        "#]],
    );
}

#[test]
fn with_chain_on_generic_type() {
    check(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self {
        first = fn(p: Self) -> T { p.a };
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..113
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..112
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..10
                  IDENT@6..10 "Pair"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                RECORD_EXPR@13..39
                  STRUCT_KW@13..19 "struct"
                  GENERIC_PARAM_LIST@19..24
                    COLON2@19..21 "::"
                    L_ANGLE@21..22 "<"
                    TYPE_PARAM@22..23
                      NAME@22..23
                        IDENT@22..23 "T"
                    R_ANGLE@23..24 ">"
                  WHITESPACE@24..25 " "
                  L_BRACE@25..26 "{"
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..31
                    NAME_REF@27..28
                      IDENT@27..28 "a"
                    COLON@28..29 ":"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..31
                      NAME_REF@30..31
                        IDENT@30..31 "T"
                  COMMA@31..32 ","
                  WHITESPACE@32..33 " "
                  RECORD_EXPR_FIELD@33..37
                    NAME_REF@33..34
                      IDENT@33..34 "b"
                    COLON@34..35 ":"
                    WHITESPACE@35..36 " "
                    PATH_TYPE@36..37
                      NAME_REF@36..37
                        IDENT@36..37 "T"
                  WHITESPACE@37..38 " "
                  R_BRACE@38..39 "}"
                WHITESPACE@39..40 " "
                WITH_GROUP@40..112
                  WITH_KW@40..44 "with"
                  WHITESPACE@44..45 " "
                  L_BRACE@45..46 "{"
                  WHITESPACE@46..51 "\n    "
                  IMPL_ELEMENT@51..110
                    IMPL_KW@51..55 "impl"
                    WHITESPACE@55..56 " "
                    PATH_TYPE@56..60
                      NAME_REF@56..60
                        IDENT@56..60 "Self"
                    WHITESPACE@60..61 " "
                    L_BRACE@61..62 "{"
                    WHITESPACE@62..71 "\n        "
                    MEMBER@71..104
                      NAME@71..76
                        IDENT@71..76 "first"
                      WHITESPACE@76..77 " "
                      EQ@77..78 "="
                      WHITESPACE@78..79 " "
                      FN_LITERAL@79..103
                        FN_KW@79..81 "fn"
                        PARAM_LIST@81..90
                          L_PAREN@81..82 "("
                          PARAM@82..89
                            BIND_PAT@82..83
                              NAME@82..83
                                IDENT@82..83 "p"
                            COLON@83..84 ":"
                            WHITESPACE@84..85 " "
                            PATH_TYPE@85..89
                              NAME_REF@85..89
                                IDENT@85..89 "Self"
                          R_PAREN@89..90 ")"
                        WHITESPACE@90..91 " "
                        RET_TYPE@91..95
                          THIN_ARROW@91..93 "->"
                          WHITESPACE@93..94 " "
                          PATH_TYPE@94..95
                            NAME_REF@94..95
                              IDENT@94..95 "T"
                        WHITESPACE@95..96 " "
                        BLOCK_EXPR@96..103
                          L_BRACE@96..97 "{"
                          WHITESPACE@97..98 " "
                          FIELD_EXPR@98..101
                            PATH_EXPR@98..99
                              NAME_REF@98..99
                                IDENT@98..99 "p"
                            DOT@99..100 "."
                            NAME_REF@100..101
                              IDENT@100..101 "a"
                          WHITESPACE@101..102 " "
                          R_BRACE@102..103 "}"
                      SEMICOLON@103..104 ";"
                    WHITESPACE@104..109 "\n    "
                    R_BRACE@109..110 "}"
                  WHITESPACE@110..111 "\n"
                  R_BRACE@111..112 "}"
              WHITESPACE@112..113 "\n"
        "#]],
    );
}

#[test]
fn with_chain_semicolon_optional_after_group_brace() {
    check(
        "type A = struct { x: usize } with { impl Self { get = fn(a: Self) -> usize { a.x }; } }\nstatic n = 1;",
        expect![[r#"
            SOURCE_FILE@0..101
              TYPE_ITEM@0..87
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..6
                  IDENT@5..6 "A"
                WHITESPACE@6..7 " "
                EQ@7..8 "="
                WHITESPACE@8..9 " "
                RECORD_EXPR@9..28
                  STRUCT_KW@9..15 "struct"
                  WHITESPACE@15..16 " "
                  L_BRACE@16..17 "{"
                  WHITESPACE@17..18 " "
                  RECORD_EXPR_FIELD@18..26
                    NAME_REF@18..19
                      IDENT@18..19 "x"
                    COLON@19..20 ":"
                    WHITESPACE@20..21 " "
                    PATH_TYPE@21..26
                      NAME_REF@21..26
                        IDENT@21..26 "usize"
                  WHITESPACE@26..27 " "
                  R_BRACE@27..28 "}"
                WHITESPACE@28..29 " "
                WITH_GROUP@29..87
                  WITH_KW@29..33 "with"
                  WHITESPACE@33..34 " "
                  L_BRACE@34..35 "{"
                  WHITESPACE@35..36 " "
                  IMPL_ELEMENT@36..85
                    IMPL_KW@36..40 "impl"
                    WHITESPACE@40..41 " "
                    PATH_TYPE@41..45
                      NAME_REF@41..45
                        IDENT@41..45 "Self"
                    WHITESPACE@45..46 " "
                    L_BRACE@46..47 "{"
                    WHITESPACE@47..48 " "
                    MEMBER@48..83
                      NAME@48..51
                        IDENT@48..51 "get"
                      WHITESPACE@51..52 " "
                      EQ@52..53 "="
                      WHITESPACE@53..54 " "
                      FN_LITERAL@54..82
                        FN_KW@54..56 "fn"
                        PARAM_LIST@56..65
                          L_PAREN@56..57 "("
                          PARAM@57..64
                            BIND_PAT@57..58
                              NAME@57..58
                                IDENT@57..58 "a"
                            COLON@58..59 ":"
                            WHITESPACE@59..60 " "
                            PATH_TYPE@60..64
                              NAME_REF@60..64
                                IDENT@60..64 "Self"
                          R_PAREN@64..65 ")"
                        WHITESPACE@65..66 " "
                        RET_TYPE@66..74
                          THIN_ARROW@66..68 "->"
                          WHITESPACE@68..69 " "
                          PATH_TYPE@69..74
                            NAME_REF@69..74
                              IDENT@69..74 "usize"
                        WHITESPACE@74..75 " "
                        BLOCK_EXPR@75..82
                          L_BRACE@75..76 "{"
                          WHITESPACE@76..77 " "
                          FIELD_EXPR@77..80
                            PATH_EXPR@77..78
                              NAME_REF@77..78
                                IDENT@77..78 "a"
                            DOT@78..79 "."
                            NAME_REF@79..80
                              IDENT@79..80 "x"
                          WHITESPACE@80..81 " "
                          R_BRACE@81..82 "}"
                      SEMICOLON@82..83 ";"
                    WHITESPACE@83..84 " "
                    R_BRACE@84..85 "}"
                  WHITESPACE@85..86 " "
                  R_BRACE@86..87 "}"
              WHITESPACE@87..88 "\n"
              STATIC_ITEM@88..101
                STATIC_KW@88..94 "static"
                WHITESPACE@94..95 " "
                NAME@95..96
                  IDENT@95..96 "n"
                WHITESPACE@96..97 " "
                EQ@97..98 "="
                WHITESPACE@98..99 " "
                LITERAL@99..100
                  INT_NUMBER@99..100 "1"
                SEMICOLON@100..101 ";"
        "#]],
    );
}

#[test]
fn with_chain_trait_impl_parses_assoc_type_reserved() {
    check(
        r#"
type Range = struct { at: usize } with {
    impl Iterator {
        type Item = usize;
        next = fn(r: Self) -> usize { r.at };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..144
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..143
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..11
                  IDENT@6..11 "Range"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                RECORD_EXPR@14..34
                  STRUCT_KW@14..20 "struct"
                  WHITESPACE@20..21 " "
                  L_BRACE@21..22 "{"
                  WHITESPACE@22..23 " "
                  RECORD_EXPR_FIELD@23..32
                    NAME_REF@23..25
                      IDENT@23..25 "at"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    PATH_TYPE@27..32
                      NAME_REF@27..32
                        IDENT@27..32 "usize"
                  WHITESPACE@32..33 " "
                  R_BRACE@33..34 "}"
                WHITESPACE@34..35 " "
                WITH_GROUP@35..142
                  WITH_KW@35..39 "with"
                  WHITESPACE@39..40 " "
                  L_BRACE@40..41 "{"
                  WHITESPACE@41..46 "\n    "
                  IMPL_ELEMENT@46..140
                    IMPL_KW@46..50 "impl"
                    WHITESPACE@50..51 " "
                    PATH_TYPE@51..59
                      NAME_REF@51..59
                        IDENT@51..59 "Iterator"
                    WHITESPACE@59..60 " "
                    L_BRACE@60..61 "{"
                    WHITESPACE@61..70 "\n        "
                    MEMBER@70..88
                      TYPE_KW@70..74 "type"
                      WHITESPACE@74..75 " "
                      NAME@75..79
                        IDENT@75..79 "Item"
                      WHITESPACE@79..80 " "
                      EQ@80..81 "="
                      WHITESPACE@81..82 " "
                      PATH_EXPR@82..87
                        NAME_REF@82..87
                          IDENT@82..87 "usize"
                      SEMICOLON@87..88 ";"
                    WHITESPACE@88..97 "\n        "
                    MEMBER@97..134
                      NAME@97..101
                        IDENT@97..101 "next"
                      WHITESPACE@101..102 " "
                      EQ@102..103 "="
                      WHITESPACE@103..104 " "
                      FN_LITERAL@104..133
                        FN_KW@104..106 "fn"
                        PARAM_LIST@106..115
                          L_PAREN@106..107 "("
                          PARAM@107..114
                            BIND_PAT@107..108
                              NAME@107..108
                                IDENT@107..108 "r"
                            COLON@108..109 ":"
                            WHITESPACE@109..110 " "
                            PATH_TYPE@110..114
                              NAME_REF@110..114
                                IDENT@110..114 "Self"
                          R_PAREN@114..115 ")"
                        WHITESPACE@115..116 " "
                        RET_TYPE@116..124
                          THIN_ARROW@116..118 "->"
                          WHITESPACE@118..119 " "
                          PATH_TYPE@119..124
                            NAME_REF@119..124
                              IDENT@119..124 "usize"
                        WHITESPACE@124..125 " "
                        BLOCK_EXPR@125..133
                          L_BRACE@125..126 "{"
                          WHITESPACE@126..127 " "
                          FIELD_EXPR@127..131
                            PATH_EXPR@127..128
                              NAME_REF@127..128
                                IDENT@127..128 "r"
                            DOT@128..129 "."
                            NAME_REF@129..131
                              IDENT@129..131 "at"
                          WHITESPACE@131..132 " "
                          R_BRACE@132..133 "}"
                      SEMICOLON@133..134 ";"
                    WHITESPACE@134..139 "\n    "
                    R_BRACE@139..140 "}"
                  WHITESPACE@140..141 "\n"
                  R_BRACE@141..142 "}"
                SEMICOLON@142..143 ";"
              WHITESPACE@143..144 "\n"
            error 70..74: associated types are not supported yet
        "#]],
    );
}

#[test]
fn colon_declared_member_rejected() {
    check(
        r#"
type A = struct { x: usize } with {
    impl Self {
        len: fn(a: Self) -> usize;
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..97
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..96
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..95
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  IMPL_ELEMENT@41..93
                    IMPL_KW@41..45 "impl"
                    WHITESPACE@45..46 " "
                    PATH_TYPE@46..50
                      NAME_REF@46..50
                        IDENT@46..50 "Self"
                    WHITESPACE@50..51 " "
                    L_BRACE@51..52 "{"
                    WHITESPACE@52..61 "\n        "
                    MEMBER@61..87
                      NAME@61..64
                        IDENT@61..64 "len"
                      COLON@64..65 ":"
                      WHITESPACE@65..66 " "
                      FN_TYPE@66..86
                        FN_KW@66..68 "fn"
                        PARAM_LIST@68..77
                          L_PAREN@68..69 "("
                          PARAM@69..76
                            BIND_PAT@69..70
                              NAME@69..70
                                IDENT@69..70 "a"
                            COLON@70..71 ":"
                            WHITESPACE@71..72 " "
                            PATH_TYPE@72..76
                              NAME_REF@72..76
                                IDENT@72..76 "Self"
                          R_PAREN@76..77 ")"
                        WHITESPACE@77..78 " "
                        RET_TYPE@78..86
                          THIN_ARROW@78..80 "->"
                          WHITESPACE@80..81 " "
                          PATH_TYPE@81..86
                            NAME_REF@81..86
                              IDENT@81..86 "usize"
                      SEMICOLON@86..87 ";"
                    WHITESPACE@87..92 "\n    "
                    R_BRACE@92..93 "}"
                  WHITESPACE@93..94 "\n"
                  R_BRACE@94..95 "}"
                SEMICOLON@95..96 ";"
              WHITESPACE@96..97 "\n"
            error 61..87: a declare-only inherent member is an unimplementable promise; define it: `name = fn(...) -> ... { ... };`
        "#]],
    );
}

#[test]
fn member_reserved_forms() {
    check(
        r#"
type A = struct { x: usize } with {
    impl Self {
        const N: usize;
        v = 5;
        g = fn::<T>(x: T, a: Self) -> T { x };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..148
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..147
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..146
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  IMPL_ELEMENT@41..144
                    IMPL_KW@41..45 "impl"
                    WHITESPACE@45..46 " "
                    PATH_TYPE@46..50
                      NAME_REF@46..50
                        IDENT@46..50 "Self"
                    WHITESPACE@50..51 " "
                    L_BRACE@51..52 "{"
                    WHITESPACE@52..61 "\n        "
                    MEMBER@61..76
                      CONST_KW@61..66 "const"
                      WHITESPACE@66..67 " "
                      NAME@67..68
                        IDENT@67..68 "N"
                      COLON@68..69 ":"
                      WHITESPACE@69..70 " "
                      PATH_TYPE@70..75
                        NAME_REF@70..75
                          IDENT@70..75 "usize"
                      SEMICOLON@75..76 ";"
                    WHITESPACE@76..85 "\n        "
                    MEMBER@85..91
                      NAME@85..86
                        IDENT@85..86 "v"
                      WHITESPACE@86..87 " "
                      EQ@87..88 "="
                      WHITESPACE@88..89 " "
                      LITERAL@89..90
                        INT_NUMBER@89..90 "5"
                      SEMICOLON@90..91 ";"
                    WHITESPACE@91..100 "\n        "
                    MEMBER@100..138
                      NAME@100..101
                        IDENT@100..101 "g"
                      WHITESPACE@101..102 " "
                      EQ@102..103 "="
                      WHITESPACE@103..104 " "
                      FN_LITERAL@104..137
                        FN_KW@104..106 "fn"
                        GENERIC_PARAM_LIST@106..111
                          COLON2@106..108 "::"
                          L_ANGLE@108..109 "<"
                          TYPE_PARAM@109..110
                            NAME@109..110
                              IDENT@109..110 "T"
                          R_ANGLE@110..111 ">"
                        PARAM_LIST@111..126
                          L_PAREN@111..112 "("
                          PARAM@112..116
                            BIND_PAT@112..113
                              NAME@112..113
                                IDENT@112..113 "x"
                            COLON@113..114 ":"
                            WHITESPACE@114..115 " "
                            PATH_TYPE@115..116
                              NAME_REF@115..116
                                IDENT@115..116 "T"
                          COMMA@116..117 ","
                          WHITESPACE@117..118 " "
                          PARAM@118..125
                            BIND_PAT@118..119
                              NAME@118..119
                                IDENT@118..119 "a"
                            COLON@119..120 ":"
                            WHITESPACE@120..121 " "
                            PATH_TYPE@121..125
                              NAME_REF@121..125
                                IDENT@121..125 "Self"
                          R_PAREN@125..126 ")"
                        WHITESPACE@126..127 " "
                        RET_TYPE@127..131
                          THIN_ARROW@127..129 "->"
                          WHITESPACE@129..130 " "
                          PATH_TYPE@130..131
                            NAME_REF@130..131
                              IDENT@130..131 "T"
                        WHITESPACE@131..132 " "
                        BLOCK_EXPR@132..137
                          L_BRACE@132..133 "{"
                          WHITESPACE@133..134 " "
                          PATH_EXPR@134..135
                            NAME_REF@134..135
                              IDENT@134..135 "x"
                          WHITESPACE@135..136 " "
                          R_BRACE@136..137 "}"
                      SEMICOLON@137..138 ";"
                    WHITESPACE@138..143 "\n    "
                    R_BRACE@143..144 "}"
                  WHITESPACE@144..145 "\n"
                  R_BRACE@145..146 "}"
                SEMICOLON@146..147 ";"
              WHITESPACE@147..148 "\n"
            error 61..66: associated consts are not supported yet
            error 89..90: a member must be defined as an `fn` literal
            error 109..110: a member's own type parameters are not supported yet (the type's own binders are already in scope)
        "#]],
    );
}

#[test]
fn with_chain_marker_and_unsafe_reserved() {
    check(
        r#"
type A = struct { x: usize } with {
    unsafe impl send;
    unsafe { impl send; impl sync; }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..99
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..98
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..97
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  UNSAFE_ELEMENT@41..58
                    UNSAFE_KW@41..47 "unsafe"
                    WHITESPACE@47..48 " "
                    IMPL_ELEMENT@48..58
                      IMPL_KW@48..52 "impl"
                      WHITESPACE@52..53 " "
                      PATH_TYPE@53..57
                        NAME_REF@53..57
                          IDENT@53..57 "send"
                      SEMICOLON@57..58 ";"
                  WHITESPACE@58..63 "\n    "
                  UNSAFE_ELEMENT@63..95
                    UNSAFE_KW@63..69 "unsafe"
                    WHITESPACE@69..70 " "
                    L_BRACE@70..71 "{"
                    WHITESPACE@71..72 " "
                    IMPL_ELEMENT@72..82
                      IMPL_KW@72..76 "impl"
                      WHITESPACE@76..77 " "
                      PATH_TYPE@77..81
                        NAME_REF@77..81
                          IDENT@77..81 "send"
                      SEMICOLON@81..82 ";"
                    WHITESPACE@82..83 " "
                    IMPL_ELEMENT@83..93
                      IMPL_KW@83..87 "impl"
                      WHITESPACE@87..88 " "
                      PATH_TYPE@88..92
                        NAME_REF@88..92
                          IDENT@88..92 "sync"
                      SEMICOLON@92..93 ";"
                    WHITESPACE@93..94 " "
                    R_BRACE@94..95 "}"
                  WHITESPACE@95..96 "\n"
                  R_BRACE@96..97 "}"
                SEMICOLON@97..98 ";"
              WHITESPACE@98..99 "\n"
            error 41..47: `unsafe` impl elements are not supported yet
            error 63..69: `unsafe` impl elements are not supported yet
        "#]],
    );
}

#[test]
fn with_chain_for_heads_reserved() {
    check(
        r#"
type A = struct { x: usize } with {
    for Box::<Self> impl Display { fmt = fn(a: Self) -> usize { 1 }; }
    for Vec::<Self> {
        impl Display { fmt = fn(a: Self) -> usize { 1 }; }
        impl Iterator { next = fn(a: Self) -> usize { 2 }; }
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..259
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..258
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..257
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  FOR_ELEMENT@41..107
                    FOR_KW@41..44 "for"
                    WHITESPACE@44..45 " "
                    PATH_TYPE@45..56
                      NAME_REF@45..48
                        IDENT@45..48 "Box"
                      COLON2@48..50 "::"
                      GENERIC_ARG_LIST@50..56
                        L_ANGLE@50..51 "<"
                        TYPE_ARG@51..55
                          PATH_TYPE@51..55
                            NAME_REF@51..55
                              IDENT@51..55 "Self"
                        R_ANGLE@55..56 ">"
                    WHITESPACE@56..57 " "
                    IMPL_ELEMENT@57..107
                      IMPL_KW@57..61 "impl"
                      WHITESPACE@61..62 " "
                      PATH_TYPE@62..69
                        NAME_REF@62..69
                          IDENT@62..69 "Display"
                      WHITESPACE@69..70 " "
                      L_BRACE@70..71 "{"
                      WHITESPACE@71..72 " "
                      MEMBER@72..105
                        NAME@72..75
                          IDENT@72..75 "fmt"
                        WHITESPACE@75..76 " "
                        EQ@76..77 "="
                        WHITESPACE@77..78 " "
                        FN_LITERAL@78..104
                          FN_KW@78..80 "fn"
                          PARAM_LIST@80..89
                            L_PAREN@80..81 "("
                            PARAM@81..88
                              BIND_PAT@81..82
                                NAME@81..82
                                  IDENT@81..82 "a"
                              COLON@82..83 ":"
                              WHITESPACE@83..84 " "
                              PATH_TYPE@84..88
                                NAME_REF@84..88
                                  IDENT@84..88 "Self"
                            R_PAREN@88..89 ")"
                          WHITESPACE@89..90 " "
                          RET_TYPE@90..98
                            THIN_ARROW@90..92 "->"
                            WHITESPACE@92..93 " "
                            PATH_TYPE@93..98
                              NAME_REF@93..98
                                IDENT@93..98 "usize"
                          WHITESPACE@98..99 " "
                          BLOCK_EXPR@99..104
                            L_BRACE@99..100 "{"
                            WHITESPACE@100..101 " "
                            LITERAL@101..102
                              INT_NUMBER@101..102 "1"
                            WHITESPACE@102..103 " "
                            R_BRACE@103..104 "}"
                        SEMICOLON@104..105 ";"
                      WHITESPACE@105..106 " "
                      R_BRACE@106..107 "}"
                  WHITESPACE@107..112 "\n    "
                  FOR_ELEMENT@112..255
                    FOR_KW@112..115 "for"
                    WHITESPACE@115..116 " "
                    PATH_TYPE@116..127
                      NAME_REF@116..119
                        IDENT@116..119 "Vec"
                      COLON2@119..121 "::"
                      GENERIC_ARG_LIST@121..127
                        L_ANGLE@121..122 "<"
                        TYPE_ARG@122..126
                          PATH_TYPE@122..126
                            NAME_REF@122..126
                              IDENT@122..126 "Self"
                        R_ANGLE@126..127 ">"
                    WHITESPACE@127..128 " "
                    L_BRACE@128..129 "{"
                    WHITESPACE@129..138 "\n        "
                    IMPL_ELEMENT@138..188
                      IMPL_KW@138..142 "impl"
                      WHITESPACE@142..143 " "
                      PATH_TYPE@143..150
                        NAME_REF@143..150
                          IDENT@143..150 "Display"
                      WHITESPACE@150..151 " "
                      L_BRACE@151..152 "{"
                      WHITESPACE@152..153 " "
                      MEMBER@153..186
                        NAME@153..156
                          IDENT@153..156 "fmt"
                        WHITESPACE@156..157 " "
                        EQ@157..158 "="
                        WHITESPACE@158..159 " "
                        FN_LITERAL@159..185
                          FN_KW@159..161 "fn"
                          PARAM_LIST@161..170
                            L_PAREN@161..162 "("
                            PARAM@162..169
                              BIND_PAT@162..163
                                NAME@162..163
                                  IDENT@162..163 "a"
                              COLON@163..164 ":"
                              WHITESPACE@164..165 " "
                              PATH_TYPE@165..169
                                NAME_REF@165..169
                                  IDENT@165..169 "Self"
                            R_PAREN@169..170 ")"
                          WHITESPACE@170..171 " "
                          RET_TYPE@171..179
                            THIN_ARROW@171..173 "->"
                            WHITESPACE@173..174 " "
                            PATH_TYPE@174..179
                              NAME_REF@174..179
                                IDENT@174..179 "usize"
                          WHITESPACE@179..180 " "
                          BLOCK_EXPR@180..185
                            L_BRACE@180..181 "{"
                            WHITESPACE@181..182 " "
                            LITERAL@182..183
                              INT_NUMBER@182..183 "1"
                            WHITESPACE@183..184 " "
                            R_BRACE@184..185 "}"
                        SEMICOLON@185..186 ";"
                      WHITESPACE@186..187 " "
                      R_BRACE@187..188 "}"
                    WHITESPACE@188..197 "\n        "
                    IMPL_ELEMENT@197..249
                      IMPL_KW@197..201 "impl"
                      WHITESPACE@201..202 " "
                      PATH_TYPE@202..210
                        NAME_REF@202..210
                          IDENT@202..210 "Iterator"
                      WHITESPACE@210..211 " "
                      L_BRACE@211..212 "{"
                      WHITESPACE@212..213 " "
                      MEMBER@213..247
                        NAME@213..217
                          IDENT@213..217 "next"
                        WHITESPACE@217..218 " "
                        EQ@218..219 "="
                        WHITESPACE@219..220 " "
                        FN_LITERAL@220..246
                          FN_KW@220..222 "fn"
                          PARAM_LIST@222..231
                            L_PAREN@222..223 "("
                            PARAM@223..230
                              BIND_PAT@223..224
                                NAME@223..224
                                  IDENT@223..224 "a"
                              COLON@224..225 ":"
                              WHITESPACE@225..226 " "
                              PATH_TYPE@226..230
                                NAME_REF@226..230
                                  IDENT@226..230 "Self"
                            R_PAREN@230..231 ")"
                          WHITESPACE@231..232 " "
                          RET_TYPE@232..240
                            THIN_ARROW@232..234 "->"
                            WHITESPACE@234..235 " "
                            PATH_TYPE@235..240
                              NAME_REF@235..240
                                IDENT@235..240 "usize"
                          WHITESPACE@240..241 " "
                          BLOCK_EXPR@241..246
                            L_BRACE@241..242 "{"
                            WHITESPACE@242..243 " "
                            LITERAL@243..244
                              INT_NUMBER@243..244 "2"
                            WHITESPACE@244..245 " "
                            R_BRACE@245..246 "}"
                        SEMICOLON@246..247 ";"
                      WHITESPACE@247..248 " "
                      R_BRACE@248..249 "}"
                    WHITESPACE@249..254 "\n    "
                    R_BRACE@254..255 "}"
                  WHITESPACE@255..256 "\n"
                  R_BRACE@256..257 "}"
                SEMICOLON@257..258 ";"
              WHITESPACE@258..259 "\n"
            error 41..44: `for` (covered) impl elements are not supported yet
            error 112..115: `for` (covered) impl elements are not supported yet
        "#]],
    );
}

#[test]
fn with_group_clause_lists_reserved() {
    check(
        r#"
type Pool = struct::<T> { x: T }
with T: copy {
    impl Self { get = fn(p: Self) -> T { p.x }; }
}
with T = usize {
    impl UsizeThing { m = fn(p: Self) -> usize { 0 }; }
}
with::<U> T: From::<U> {
    impl Extend::<U> { extend = fn(x: U, p: Self) -> Self { p }; }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..271
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..270
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..10
                  IDENT@6..10 "Pool"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                RECORD_EXPR@13..33
                  STRUCT_KW@13..19 "struct"
                  GENERIC_PARAM_LIST@19..24
                    COLON2@19..21 "::"
                    L_ANGLE@21..22 "<"
                    TYPE_PARAM@22..23
                      NAME@22..23
                        IDENT@22..23 "T"
                    R_ANGLE@23..24 ">"
                  WHITESPACE@24..25 " "
                  L_BRACE@25..26 "{"
                  WHITESPACE@26..27 " "
                  RECORD_EXPR_FIELD@27..31
                    NAME_REF@27..28
                      IDENT@27..28 "x"
                    COLON@28..29 ":"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..31
                      NAME_REF@30..31
                        IDENT@30..31 "T"
                  WHITESPACE@31..32 " "
                  R_BRACE@32..33 "}"
                WHITESPACE@33..34 "\n"
                WITH_GROUP@34..100
                  WITH_KW@34..38 "with"
                  WHITESPACE@38..39 " "
                  WITH_CLAUSE@39..46
                    NAME_REF@39..40
                      IDENT@39..40 "T"
                    COLON@40..41 ":"
                    WHITESPACE@41..42 " "
                    PATH_TYPE@42..46
                      NAME_REF@42..46
                        IDENT@42..46 "copy"
                  WHITESPACE@46..47 " "
                  L_BRACE@47..48 "{"
                  WHITESPACE@48..53 "\n    "
                  IMPL_ELEMENT@53..98
                    IMPL_KW@53..57 "impl"
                    WHITESPACE@57..58 " "
                    PATH_TYPE@58..62
                      NAME_REF@58..62
                        IDENT@58..62 "Self"
                    WHITESPACE@62..63 " "
                    L_BRACE@63..64 "{"
                    WHITESPACE@64..65 " "
                    MEMBER@65..96
                      NAME@65..68
                        IDENT@65..68 "get"
                      WHITESPACE@68..69 " "
                      EQ@69..70 "="
                      WHITESPACE@70..71 " "
                      FN_LITERAL@71..95
                        FN_KW@71..73 "fn"
                        PARAM_LIST@73..82
                          L_PAREN@73..74 "("
                          PARAM@74..81
                            BIND_PAT@74..75
                              NAME@74..75
                                IDENT@74..75 "p"
                            COLON@75..76 ":"
                            WHITESPACE@76..77 " "
                            PATH_TYPE@77..81
                              NAME_REF@77..81
                                IDENT@77..81 "Self"
                          R_PAREN@81..82 ")"
                        WHITESPACE@82..83 " "
                        RET_TYPE@83..87
                          THIN_ARROW@83..85 "->"
                          WHITESPACE@85..86 " "
                          PATH_TYPE@86..87
                            NAME_REF@86..87
                              IDENT@86..87 "T"
                        WHITESPACE@87..88 " "
                        BLOCK_EXPR@88..95
                          L_BRACE@88..89 "{"
                          WHITESPACE@89..90 " "
                          FIELD_EXPR@90..93
                            PATH_EXPR@90..91
                              NAME_REF@90..91
                                IDENT@90..91 "p"
                            DOT@91..92 "."
                            NAME_REF@92..93
                              IDENT@92..93 "x"
                          WHITESPACE@93..94 " "
                          R_BRACE@94..95 "}"
                      SEMICOLON@95..96 ";"
                    WHITESPACE@96..97 " "
                    R_BRACE@97..98 "}"
                  WHITESPACE@98..99 "\n"
                  R_BRACE@99..100 "}"
                WHITESPACE@100..101 "\n"
                WITH_GROUP@101..175
                  WITH_KW@101..105 "with"
                  WHITESPACE@105..106 " "
                  WITH_CLAUSE@106..115
                    NAME_REF@106..107
                      IDENT@106..107 "T"
                    WHITESPACE@107..108 " "
                    EQ@108..109 "="
                    WHITESPACE@109..110 " "
                    PATH_TYPE@110..115
                      NAME_REF@110..115
                        IDENT@110..115 "usize"
                  WHITESPACE@115..116 " "
                  L_BRACE@116..117 "{"
                  WHITESPACE@117..122 "\n    "
                  IMPL_ELEMENT@122..173
                    IMPL_KW@122..126 "impl"
                    WHITESPACE@126..127 " "
                    PATH_TYPE@127..137
                      NAME_REF@127..137
                        IDENT@127..137 "UsizeThing"
                    WHITESPACE@137..138 " "
                    L_BRACE@138..139 "{"
                    WHITESPACE@139..140 " "
                    MEMBER@140..171
                      NAME@140..141
                        IDENT@140..141 "m"
                      WHITESPACE@141..142 " "
                      EQ@142..143 "="
                      WHITESPACE@143..144 " "
                      FN_LITERAL@144..170
                        FN_KW@144..146 "fn"
                        PARAM_LIST@146..155
                          L_PAREN@146..147 "("
                          PARAM@147..154
                            BIND_PAT@147..148
                              NAME@147..148
                                IDENT@147..148 "p"
                            COLON@148..149 ":"
                            WHITESPACE@149..150 " "
                            PATH_TYPE@150..154
                              NAME_REF@150..154
                                IDENT@150..154 "Self"
                          R_PAREN@154..155 ")"
                        WHITESPACE@155..156 " "
                        RET_TYPE@156..164
                          THIN_ARROW@156..158 "->"
                          WHITESPACE@158..159 " "
                          PATH_TYPE@159..164
                            NAME_REF@159..164
                              IDENT@159..164 "usize"
                        WHITESPACE@164..165 " "
                        BLOCK_EXPR@165..170
                          L_BRACE@165..166 "{"
                          WHITESPACE@166..167 " "
                          LITERAL@167..168
                            INT_NUMBER@167..168 "0"
                          WHITESPACE@168..169 " "
                          R_BRACE@169..170 "}"
                      SEMICOLON@170..171 ";"
                    WHITESPACE@171..172 " "
                    R_BRACE@172..173 "}"
                  WHITESPACE@173..174 "\n"
                  R_BRACE@174..175 "}"
                WHITESPACE@175..176 "\n"
                WITH_GROUP@176..269
                  WITH_KW@176..180 "with"
                  GENERIC_PARAM_LIST@180..185
                    COLON2@180..182 "::"
                    L_ANGLE@182..183 "<"
                    TYPE_PARAM@183..184
                      NAME@183..184
                        IDENT@183..184 "U"
                    R_ANGLE@184..185 ">"
                  WHITESPACE@185..186 " "
                  WITH_CLAUSE@186..198
                    NAME_REF@186..187
                      IDENT@186..187 "T"
                    COLON@187..188 ":"
                    WHITESPACE@188..189 " "
                    PATH_TYPE@189..198
                      NAME_REF@189..193
                        IDENT@189..193 "From"
                      COLON2@193..195 "::"
                      GENERIC_ARG_LIST@195..198
                        L_ANGLE@195..196 "<"
                        TYPE_ARG@196..197
                          PATH_TYPE@196..197
                            NAME_REF@196..197
                              IDENT@196..197 "U"
                        R_ANGLE@197..198 ">"
                  WHITESPACE@198..199 " "
                  L_BRACE@199..200 "{"
                  WHITESPACE@200..205 "\n    "
                  IMPL_ELEMENT@205..267
                    IMPL_KW@205..209 "impl"
                    WHITESPACE@209..210 " "
                    PATH_TYPE@210..221
                      NAME_REF@210..216
                        IDENT@210..216 "Extend"
                      COLON2@216..218 "::"
                      GENERIC_ARG_LIST@218..221
                        L_ANGLE@218..219 "<"
                        TYPE_ARG@219..220
                          PATH_TYPE@219..220
                            NAME_REF@219..220
                              IDENT@219..220 "U"
                        R_ANGLE@220..221 ">"
                    WHITESPACE@221..222 " "
                    L_BRACE@222..223 "{"
                    WHITESPACE@223..224 " "
                    MEMBER@224..265
                      NAME@224..230
                        IDENT@224..230 "extend"
                      WHITESPACE@230..231 " "
                      EQ@231..232 "="
                      WHITESPACE@232..233 " "
                      FN_LITERAL@233..264
                        FN_KW@233..235 "fn"
                        PARAM_LIST@235..250
                          L_PAREN@235..236 "("
                          PARAM@236..240
                            BIND_PAT@236..237
                              NAME@236..237
                                IDENT@236..237 "x"
                            COLON@237..238 ":"
                            WHITESPACE@238..239 " "
                            PATH_TYPE@239..240
                              NAME_REF@239..240
                                IDENT@239..240 "U"
                          COMMA@240..241 ","
                          WHITESPACE@241..242 " "
                          PARAM@242..249
                            BIND_PAT@242..243
                              NAME@242..243
                                IDENT@242..243 "p"
                            COLON@243..244 ":"
                            WHITESPACE@244..245 " "
                            PATH_TYPE@245..249
                              NAME_REF@245..249
                                IDENT@245..249 "Self"
                          R_PAREN@249..250 ")"
                        WHITESPACE@250..251 " "
                        RET_TYPE@251..258
                          THIN_ARROW@251..253 "->"
                          WHITESPACE@253..254 " "
                          PATH_TYPE@254..258
                            NAME_REF@254..258
                              IDENT@254..258 "Self"
                        WHITESPACE@258..259 " "
                        BLOCK_EXPR@259..264
                          L_BRACE@259..260 "{"
                          WHITESPACE@260..261 " "
                          PATH_EXPR@261..262
                            NAME_REF@261..262
                              IDENT@261..262 "p"
                          WHITESPACE@262..263 " "
                          R_BRACE@263..264 "}"
                      SEMICOLON@264..265 ";"
                    WHITESPACE@265..266 " "
                    R_BRACE@266..267 "}"
                  WHITESPACE@267..268 "\n"
                  R_BRACE@268..269 "}"
                SEMICOLON@269..270 ";"
              WHITESPACE@270..271 "\n"
            error 39..46: `with T: ...` constrained groups are not supported yet
            error 106..115: `with T = ...` pin groups are not supported yet
            error 180..185: `with::<...>` binder groups are not supported yet
            error 186..198: `with T: ...` constrained groups are not supported yet
        "#]],
    );
}

#[test]
fn with_chain_on_static_rejected() {
    check(
        "static x = 5 with { impl Self { f = fn(s: Self) -> usize { 1 }; } };",
        expect![[r#"
            SOURCE_FILE@0..68
              STATIC_ITEM@0..68
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..12
                  INT_NUMBER@11..12 "5"
                WHITESPACE@12..13 " "
                WITH_GROUP@13..67
                  WITH_KW@13..17 "with"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  IMPL_ELEMENT@20..65
                    IMPL_KW@20..24 "impl"
                    WHITESPACE@24..25 " "
                    PATH_TYPE@25..29
                      NAME_REF@25..29
                        IDENT@25..29 "Self"
                    WHITESPACE@29..30 " "
                    L_BRACE@30..31 "{"
                    WHITESPACE@31..32 " "
                    MEMBER@32..63
                      NAME@32..33
                        IDENT@32..33 "f"
                      WHITESPACE@33..34 " "
                      EQ@34..35 "="
                      WHITESPACE@35..36 " "
                      FN_LITERAL@36..62
                        FN_KW@36..38 "fn"
                        PARAM_LIST@38..47
                          L_PAREN@38..39 "("
                          PARAM@39..46
                            BIND_PAT@39..40
                              NAME@39..40
                                IDENT@39..40 "s"
                            COLON@40..41 ":"
                            WHITESPACE@41..42 " "
                            PATH_TYPE@42..46
                              NAME_REF@42..46
                                IDENT@42..46 "Self"
                          R_PAREN@46..47 ")"
                        WHITESPACE@47..48 " "
                        RET_TYPE@48..56
                          THIN_ARROW@48..50 "->"
                          WHITESPACE@50..51 " "
                          PATH_TYPE@51..56
                            NAME_REF@51..56
                              IDENT@51..56 "usize"
                        WHITESPACE@56..57 " "
                        BLOCK_EXPR@57..62
                          L_BRACE@57..58 "{"
                          WHITESPACE@58..59 " "
                          LITERAL@59..60
                            INT_NUMBER@59..60 "1"
                          WHITESPACE@60..61 " "
                          R_BRACE@61..62 "}"
                      SEMICOLON@62..63 ";"
                    WHITESPACE@63..64 " "
                    R_BRACE@64..65 "}"
                  WHITESPACE@65..66 " "
                  R_BRACE@66..67 "}"
                SEMICOLON@67..68 ";"
            error 13..17: `with` attachment groups do not belong on a `static` item
        "#]],
    );
}

#[test]
fn impl_self_requires_body() {
    check(
        "type A = struct { x: usize } with { impl Self; };",
        expect![[r#"
            SOURCE_FILE@0..49
              TYPE_ITEM@0..49
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..6
                  IDENT@5..6 "A"
                WHITESPACE@6..7 " "
                EQ@7..8 "="
                WHITESPACE@8..9 " "
                RECORD_EXPR@9..28
                  STRUCT_KW@9..15 "struct"
                  WHITESPACE@15..16 " "
                  L_BRACE@16..17 "{"
                  WHITESPACE@17..18 " "
                  RECORD_EXPR_FIELD@18..26
                    NAME_REF@18..19
                      IDENT@18..19 "x"
                    COLON@19..20 ":"
                    WHITESPACE@20..21 " "
                    PATH_TYPE@21..26
                      NAME_REF@21..26
                        IDENT@21..26 "usize"
                  WHITESPACE@26..27 " "
                  R_BRACE@27..28 "}"
                WHITESPACE@28..29 " "
                WITH_GROUP@29..48
                  WITH_KW@29..33 "with"
                  WHITESPACE@33..34 " "
                  L_BRACE@34..35 "{"
                  WHITESPACE@35..36 " "
                  IMPL_ELEMENT@36..46
                    IMPL_KW@36..40 "impl"
                    WHITESPACE@40..41 " "
                    PATH_TYPE@41..45
                      NAME_REF@41..45
                        IDENT@41..45 "Self"
                    SEMICOLON@45..46 ";"
                  WHITESPACE@46..47 " "
                  R_BRACE@47..48 "}"
                SEMICOLON@48..49 ";"
            error 41..45: `impl Self` requires a member body (`{ ... }`)
        "#]],
    );
}

// Boundary invariant: element and member boundaries stay token-recognizable even
// with nested braces in member bodies, and the chain ends exactly at the
// first token that is neither `with` nor part of a group.
#[test]
fn with_chain_boundaries_with_nested_braces() {
    check(
        r#"
type A = struct { x: usize } with {
    impl Self {
        f = fn(a: Self) -> usize { if a.x == 0 { 1 } else { a.x } };
        g = fn(a: Self) -> usize { a.x };
    }
} with {
    impl Display { fmt = fn(a: Self) -> usize { 0 }; }
}
static after = 3;
"#,
        expect![[r#"
            SOURCE_FILE@0..254
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..235
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..171
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  IMPL_ELEMENT@41..169
                    IMPL_KW@41..45 "impl"
                    WHITESPACE@45..46 " "
                    PATH_TYPE@46..50
                      NAME_REF@46..50
                        IDENT@46..50 "Self"
                    WHITESPACE@50..51 " "
                    L_BRACE@51..52 "{"
                    WHITESPACE@52..61 "\n        "
                    MEMBER@61..121
                      NAME@61..62
                        IDENT@61..62 "f"
                      WHITESPACE@62..63 " "
                      EQ@63..64 "="
                      WHITESPACE@64..65 " "
                      FN_LITERAL@65..120
                        FN_KW@65..67 "fn"
                        PARAM_LIST@67..76
                          L_PAREN@67..68 "("
                          PARAM@68..75
                            BIND_PAT@68..69
                              NAME@68..69
                                IDENT@68..69 "a"
                            COLON@69..70 ":"
                            WHITESPACE@70..71 " "
                            PATH_TYPE@71..75
                              NAME_REF@71..75
                                IDENT@71..75 "Self"
                          R_PAREN@75..76 ")"
                        WHITESPACE@76..77 " "
                        RET_TYPE@77..85
                          THIN_ARROW@77..79 "->"
                          WHITESPACE@79..80 " "
                          PATH_TYPE@80..85
                            NAME_REF@80..85
                              IDENT@80..85 "usize"
                        WHITESPACE@85..86 " "
                        BLOCK_EXPR@86..120
                          L_BRACE@86..87 "{"
                          WHITESPACE@87..88 " "
                          IF_EXPR@88..118
                            IF_KW@88..90 "if"
                            WHITESPACE@90..91 " "
                            BIN_EXPR@91..99
                              FIELD_EXPR@91..94
                                PATH_EXPR@91..92
                                  NAME_REF@91..92
                                    IDENT@91..92 "a"
                                DOT@92..93 "."
                                NAME_REF@93..94
                                  IDENT@93..94 "x"
                              WHITESPACE@94..95 " "
                              EQ2@95..97 "=="
                              WHITESPACE@97..98 " "
                              LITERAL@98..99
                                INT_NUMBER@98..99 "0"
                            WHITESPACE@99..100 " "
                            BLOCK_EXPR@100..105
                              L_BRACE@100..101 "{"
                              WHITESPACE@101..102 " "
                              LITERAL@102..103
                                INT_NUMBER@102..103 "1"
                              WHITESPACE@103..104 " "
                              R_BRACE@104..105 "}"
                            WHITESPACE@105..106 " "
                            ELSE_KW@106..110 "else"
                            WHITESPACE@110..111 " "
                            BLOCK_EXPR@111..118
                              L_BRACE@111..112 "{"
                              WHITESPACE@112..113 " "
                              FIELD_EXPR@113..116
                                PATH_EXPR@113..114
                                  NAME_REF@113..114
                                    IDENT@113..114 "a"
                                DOT@114..115 "."
                                NAME_REF@115..116
                                  IDENT@115..116 "x"
                              WHITESPACE@116..117 " "
                              R_BRACE@117..118 "}"
                          WHITESPACE@118..119 " "
                          R_BRACE@119..120 "}"
                      SEMICOLON@120..121 ";"
                    WHITESPACE@121..130 "\n        "
                    MEMBER@130..163
                      NAME@130..131
                        IDENT@130..131 "g"
                      WHITESPACE@131..132 " "
                      EQ@132..133 "="
                      WHITESPACE@133..134 " "
                      FN_LITERAL@134..162
                        FN_KW@134..136 "fn"
                        PARAM_LIST@136..145
                          L_PAREN@136..137 "("
                          PARAM@137..144
                            BIND_PAT@137..138
                              NAME@137..138
                                IDENT@137..138 "a"
                            COLON@138..139 ":"
                            WHITESPACE@139..140 " "
                            PATH_TYPE@140..144
                              NAME_REF@140..144
                                IDENT@140..144 "Self"
                          R_PAREN@144..145 ")"
                        WHITESPACE@145..146 " "
                        RET_TYPE@146..154
                          THIN_ARROW@146..148 "->"
                          WHITESPACE@148..149 " "
                          PATH_TYPE@149..154
                            NAME_REF@149..154
                              IDENT@149..154 "usize"
                        WHITESPACE@154..155 " "
                        BLOCK_EXPR@155..162
                          L_BRACE@155..156 "{"
                          WHITESPACE@156..157 " "
                          FIELD_EXPR@157..160
                            PATH_EXPR@157..158
                              NAME_REF@157..158
                                IDENT@157..158 "a"
                            DOT@158..159 "."
                            NAME_REF@159..160
                              IDENT@159..160 "x"
                          WHITESPACE@160..161 " "
                          R_BRACE@161..162 "}"
                      SEMICOLON@162..163 ";"
                    WHITESPACE@163..168 "\n    "
                    R_BRACE@168..169 "}"
                  WHITESPACE@169..170 "\n"
                  R_BRACE@170..171 "}"
                WHITESPACE@171..172 " "
                WITH_GROUP@172..235
                  WITH_KW@172..176 "with"
                  WHITESPACE@176..177 " "
                  L_BRACE@177..178 "{"
                  WHITESPACE@178..183 "\n    "
                  IMPL_ELEMENT@183..233
                    IMPL_KW@183..187 "impl"
                    WHITESPACE@187..188 " "
                    PATH_TYPE@188..195
                      NAME_REF@188..195
                        IDENT@188..195 "Display"
                    WHITESPACE@195..196 " "
                    L_BRACE@196..197 "{"
                    WHITESPACE@197..198 " "
                    MEMBER@198..231
                      NAME@198..201
                        IDENT@198..201 "fmt"
                      WHITESPACE@201..202 " "
                      EQ@202..203 "="
                      WHITESPACE@203..204 " "
                      FN_LITERAL@204..230
                        FN_KW@204..206 "fn"
                        PARAM_LIST@206..215
                          L_PAREN@206..207 "("
                          PARAM@207..214
                            BIND_PAT@207..208
                              NAME@207..208
                                IDENT@207..208 "a"
                            COLON@208..209 ":"
                            WHITESPACE@209..210 " "
                            PATH_TYPE@210..214
                              NAME_REF@210..214
                                IDENT@210..214 "Self"
                          R_PAREN@214..215 ")"
                        WHITESPACE@215..216 " "
                        RET_TYPE@216..224
                          THIN_ARROW@216..218 "->"
                          WHITESPACE@218..219 " "
                          PATH_TYPE@219..224
                            NAME_REF@219..224
                              IDENT@219..224 "usize"
                        WHITESPACE@224..225 " "
                        BLOCK_EXPR@225..230
                          L_BRACE@225..226 "{"
                          WHITESPACE@226..227 " "
                          LITERAL@227..228
                            INT_NUMBER@227..228 "0"
                          WHITESPACE@228..229 " "
                          R_BRACE@229..230 "}"
                      SEMICOLON@230..231 ";"
                    WHITESPACE@231..232 " "
                    R_BRACE@232..233 "}"
                  WHITESPACE@233..234 "\n"
                  R_BRACE@234..235 "}"
              WHITESPACE@235..236 "\n"
              STATIC_ITEM@236..253
                STATIC_KW@236..242 "static"
                WHITESPACE@242..243 " "
                NAME@243..248
                  IDENT@243..248 "after"
                WHITESPACE@248..249 " "
                EQ@249..250 "="
                WHITESPACE@250..251 " "
                LITERAL@251..252
                  INT_NUMBER@251..252 "3"
                SEMICOLON@252..253 ";"
              WHITESPACE@253..254 "\n"
        "#]],
    );
}

#[test]
fn with_group_recovers_at_next_item() {
    check(
        r#"
type A = struct { x: usize } with {
    impl Self {
        f = fn(a: Self) -> usize { a.x };
static next = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..112
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..94
                TYPE_KW@1..5 "type"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "A"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                RECORD_EXPR@10..29
                  STRUCT_KW@10..16 "struct"
                  WHITESPACE@16..17 " "
                  L_BRACE@17..18 "{"
                  WHITESPACE@18..19 " "
                  RECORD_EXPR_FIELD@19..27
                    NAME_REF@19..20
                      IDENT@19..20 "x"
                    COLON@20..21 ":"
                    WHITESPACE@21..22 " "
                    PATH_TYPE@22..27
                      NAME_REF@22..27
                        IDENT@22..27 "usize"
                  WHITESPACE@27..28 " "
                  R_BRACE@28..29 "}"
                WHITESPACE@29..30 " "
                WITH_GROUP@30..94
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  IMPL_ELEMENT@41..94
                    IMPL_KW@41..45 "impl"
                    WHITESPACE@45..46 " "
                    PATH_TYPE@46..50
                      NAME_REF@46..50
                        IDENT@46..50 "Self"
                    WHITESPACE@50..51 " "
                    L_BRACE@51..52 "{"
                    WHITESPACE@52..61 "\n        "
                    MEMBER@61..94
                      NAME@61..62
                        IDENT@61..62 "f"
                      WHITESPACE@62..63 " "
                      EQ@63..64 "="
                      WHITESPACE@64..65 " "
                      FN_LITERAL@65..93
                        FN_KW@65..67 "fn"
                        PARAM_LIST@67..76
                          L_PAREN@67..68 "("
                          PARAM@68..75
                            BIND_PAT@68..69
                              NAME@68..69
                                IDENT@68..69 "a"
                            COLON@69..70 ":"
                            WHITESPACE@70..71 " "
                            PATH_TYPE@71..75
                              NAME_REF@71..75
                                IDENT@71..75 "Self"
                          R_PAREN@75..76 ")"
                        WHITESPACE@76..77 " "
                        RET_TYPE@77..85
                          THIN_ARROW@77..79 "->"
                          WHITESPACE@79..80 " "
                          PATH_TYPE@80..85
                            NAME_REF@80..85
                              IDENT@80..85 "usize"
                        WHITESPACE@85..86 " "
                        BLOCK_EXPR@86..93
                          L_BRACE@86..87 "{"
                          WHITESPACE@87..88 " "
                          FIELD_EXPR@88..91
                            PATH_EXPR@88..89
                              NAME_REF@88..89
                                IDENT@88..89 "a"
                            DOT@89..90 "."
                            NAME_REF@90..91
                              IDENT@90..91 "x"
                          WHITESPACE@91..92 " "
                          R_BRACE@92..93 "}"
                      SEMICOLON@93..94 ";"
              WHITESPACE@94..95 "\n"
              STATIC_ITEM@95..111
                STATIC_KW@95..101 "static"
                WHITESPACE@101..102 " "
                NAME@102..106
                  IDENT@102..106 "next"
                WHITESPACE@106..107 " "
                EQ@107..108 "="
                WHITESPACE@108..109 " "
                LITERAL@109..110
                  INT_NUMBER@109..110 "1"
                SEMICOLON@110..111 ";"
              WHITESPACE@111..112 "\n"
            error 93..94: expected `}`
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

// ---- trait declarations: bounds, impl homes -----------------------------

#[test]
fn trait_decl_requires_parses() {
    check(
        r#"
trait Write = requires {
    push: fn(s: str, w: Self) -> Self;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..68
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..67
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..12
                  IDENT@7..12 "Write"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                REQUIRES_DEF@15..66
                  REQUIRES_KW@15..23 "requires"
                  WHITESPACE@23..24 " "
                  L_BRACE@24..25 "{"
                  WHITESPACE@25..30 "\n    "
                  MEMBER@30..64
                    NAME@30..34
                      IDENT@30..34 "push"
                    COLON@34..35 ":"
                    WHITESPACE@35..36 " "
                    FN_TYPE@36..63
                      FN_KW@36..38 "fn"
                      PARAM_LIST@38..55
                        L_PAREN@38..39 "("
                        PARAM@39..45
                          BIND_PAT@39..40
                            NAME@39..40
                              IDENT@39..40 "s"
                          COLON@40..41 ":"
                          WHITESPACE@41..42 " "
                          PATH_TYPE@42..45
                            NAME_REF@42..45
                              IDENT@42..45 "str"
                        COMMA@45..46 ","
                        WHITESPACE@46..47 " "
                        PARAM@47..54
                          BIND_PAT@47..48
                            NAME@47..48
                              IDENT@47..48 "w"
                          COLON@48..49 ":"
                          WHITESPACE@49..50 " "
                          PATH_TYPE@50..54
                            NAME_REF@50..54
                              IDENT@50..54 "Self"
                        R_PAREN@54..55 ")"
                      WHITESPACE@55..56 " "
                      RET_TYPE@56..63
                        THIN_ARROW@56..58 "->"
                        WHITESPACE@58..59 " "
                        PATH_TYPE@59..63
                          NAME_REF@59..63
                            IDENT@59..63 "Self"
                    SEMICOLON@63..64 ";"
                  WHITESPACE@64..65 "\n"
                  R_BRACE@65..66 "}"
                SEMICOLON@66..67 ";"
              WHITESPACE@67..68 "\n"
        "#]],
    );
}

#[test]
fn trait_decl_generic_requirement_with_bound_parses() {
    check(
        r#"
trait Display = requires {
    fmt: fn::<W: Write>(w: W, x: Self) -> W;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..76
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..75
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..14
                  IDENT@7..14 "Display"
                WHITESPACE@14..15 " "
                EQ@15..16 "="
                WHITESPACE@16..17 " "
                REQUIRES_DEF@17..74
                  REQUIRES_KW@17..25 "requires"
                  WHITESPACE@25..26 " "
                  L_BRACE@26..27 "{"
                  WHITESPACE@27..32 "\n    "
                  MEMBER@32..72
                    NAME@32..35
                      IDENT@32..35 "fmt"
                    COLON@35..36 ":"
                    WHITESPACE@36..37 " "
                    FN_TYPE@37..71
                      FN_KW@37..39 "fn"
                      GENERIC_PARAM_LIST@39..51
                        COLON2@39..41 "::"
                        L_ANGLE@41..42 "<"
                        TYPE_PARAM@42..50
                          NAME@42..43
                            IDENT@42..43 "W"
                          COLON@43..44 ":"
                          WHITESPACE@44..45 " "
                          PATH_TYPE@45..50
                            NAME_REF@45..50
                              IDENT@45..50 "Write"
                        R_ANGLE@50..51 ">"
                      PARAM_LIST@51..66
                        L_PAREN@51..52 "("
                        PARAM@52..56
                          BIND_PAT@52..53
                            NAME@52..53
                              IDENT@52..53 "w"
                          COLON@53..54 ":"
                          WHITESPACE@54..55 " "
                          PATH_TYPE@55..56
                            NAME_REF@55..56
                              IDENT@55..56 "W"
                        COMMA@56..57 ","
                        WHITESPACE@57..58 " "
                        PARAM@58..65
                          BIND_PAT@58..59
                            NAME@58..59
                              IDENT@58..59 "x"
                          COLON@59..60 ":"
                          WHITESPACE@60..61 " "
                          PATH_TYPE@61..65
                            NAME_REF@61..65
                              IDENT@61..65 "Self"
                        R_PAREN@65..66 ")"
                      WHITESPACE@66..67 " "
                      RET_TYPE@67..71
                        THIN_ARROW@67..69 "->"
                        WHITESPACE@69..70 " "
                        PATH_TYPE@70..71
                          NAME_REF@70..71
                            IDENT@70..71 "W"
                    SEMICOLON@71..72 ";"
                  WHITESPACE@72..73 "\n"
                  R_BRACE@73..74 "}"
                SEMICOLON@74..75 ";"
              WHITESPACE@75..76 "\n"
        "#]],
    );
}

#[test]
fn trait_alias_parses_and_is_reserved() {
    check(
        "trait Ord = Eq + PartialOrd;",
        expect![[r#"
            SOURCE_FILE@0..28
              TRAIT_ITEM@0..28
                TRAIT_KW@0..5 "trait"
                WHITESPACE@5..6 " "
                NAME@6..9
                  IDENT@6..9 "Ord"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                TRAIT_ALIAS@12..27
                  PATH_TYPE@12..14
                    NAME_REF@12..14
                      IDENT@12..14 "Eq"
                  WHITESPACE@14..15 " "
                  PLUS@15..16 "+"
                  WHITESPACE@16..17 " "
                  PATH_TYPE@17..27
                    NAME_REF@17..27
                      IDENT@17..27 "PartialOrd"
                SEMICOLON@27..28 ";"
            error 12..27: trait aliases are not supported yet; declare the trait with `requires { ... }`
        "#]],
    );
}

#[test]
fn trait_requires_reserved_forms() {
    // Generic binder, unsafe head and supertrait clause all parse cleanly
    // and carry precise reservations.
    check(
        r#"
trait Alloc = requires::<T> { alloc: fn(n: usize, s: Self) -> usize; };
trait TrustedLen = unsafe requires Self: Iterator { };
"#,
        expect![[r#"
            SOURCE_FILE@0..128
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..72
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..12
                  IDENT@7..12 "Alloc"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                REQUIRES_DEF@15..71
                  REQUIRES_KW@15..23 "requires"
                  GENERIC_PARAM_LIST@23..28
                    COLON2@23..25 "::"
                    L_ANGLE@25..26 "<"
                    TYPE_PARAM@26..27
                      NAME@26..27
                        IDENT@26..27 "T"
                    R_ANGLE@27..28 ">"
                  WHITESPACE@28..29 " "
                  L_BRACE@29..30 "{"
                  WHITESPACE@30..31 " "
                  MEMBER@31..69
                    NAME@31..36
                      IDENT@31..36 "alloc"
                    COLON@36..37 ":"
                    WHITESPACE@37..38 " "
                    FN_TYPE@38..68
                      FN_KW@38..40 "fn"
                      PARAM_LIST@40..59
                        L_PAREN@40..41 "("
                        PARAM@41..49
                          BIND_PAT@41..42
                            NAME@41..42
                              IDENT@41..42 "n"
                          COLON@42..43 ":"
                          WHITESPACE@43..44 " "
                          PATH_TYPE@44..49
                            NAME_REF@44..49
                              IDENT@44..49 "usize"
                        COMMA@49..50 ","
                        WHITESPACE@50..51 " "
                        PARAM@51..58
                          BIND_PAT@51..52
                            NAME@51..52
                              IDENT@51..52 "s"
                          COLON@52..53 ":"
                          WHITESPACE@53..54 " "
                          PATH_TYPE@54..58
                            NAME_REF@54..58
                              IDENT@54..58 "Self"
                        R_PAREN@58..59 ")"
                      WHITESPACE@59..60 " "
                      RET_TYPE@60..68
                        THIN_ARROW@60..62 "->"
                        WHITESPACE@62..63 " "
                        PATH_TYPE@63..68
                          NAME_REF@63..68
                            IDENT@63..68 "usize"
                    SEMICOLON@68..69 ";"
                  WHITESPACE@69..70 " "
                  R_BRACE@70..71 "}"
                SEMICOLON@71..72 ";"
              WHITESPACE@72..73 "\n"
              TRAIT_ITEM@73..127
                TRAIT_KW@73..78 "trait"
                WHITESPACE@78..79 " "
                NAME@79..89
                  IDENT@79..89 "TrustedLen"
                WHITESPACE@89..90 " "
                EQ@90..91 "="
                WHITESPACE@91..92 " "
                REQUIRES_DEF@92..126
                  UNSAFE_KW@92..98 "unsafe"
                  WHITESPACE@98..99 " "
                  REQUIRES_KW@99..107 "requires"
                  WHITESPACE@107..108 " "
                  REQUIRES_CLAUSE@108..122
                    NAME_REF@108..112
                      IDENT@108..112 "Self"
                    COLON@112..113 ":"
                    WHITESPACE@113..114 " "
                    PATH_TYPE@114..122
                      NAME_REF@114..122
                        IDENT@114..122 "Iterator"
                  WHITESPACE@122..123 " "
                  L_BRACE@123..124 "{"
                  WHITESPACE@124..125 " "
                  R_BRACE@125..126 "}"
                SEMICOLON@126..127 ";"
              WHITESPACE@127..128 "\n"
            error 23..28: generic traits are not supported yet
            error 92..98: `unsafe` traits are not supported yet
            error 108..122: supertrait clauses are not supported yet
        "#]],
    );
}

#[test]
fn trait_requirement_reserved_member_forms() {
    // Defaults (equals-defined), associated types/consts and `unsafe fn`
    // signatures are parse-and-reserve inside a `requires` body.
    check(
        r#"
trait T = requires {
    d = fn(x: usize) -> usize { 1 };
    type Item;
    const N: usize;
    f: unsafe fn(x: Self) -> usize;
    g;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..140
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..139
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "T"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..138
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..26 "\n    "
                  MEMBER@26..58
                    NAME@26..27
                      IDENT@26..27 "d"
                    WHITESPACE@27..28 " "
                    EQ@28..29 "="
                    WHITESPACE@29..30 " "
                    FN_LITERAL@30..57
                      FN_KW@30..32 "fn"
                      PARAM_LIST@32..42
                        L_PAREN@32..33 "("
                        PARAM@33..41
                          BIND_PAT@33..34
                            NAME@33..34
                              IDENT@33..34 "x"
                          COLON@34..35 ":"
                          WHITESPACE@35..36 " "
                          PATH_TYPE@36..41
                            NAME_REF@36..41
                              IDENT@36..41 "usize"
                        R_PAREN@41..42 ")"
                      WHITESPACE@42..43 " "
                      RET_TYPE@43..51
                        THIN_ARROW@43..45 "->"
                        WHITESPACE@45..46 " "
                        PATH_TYPE@46..51
                          NAME_REF@46..51
                            IDENT@46..51 "usize"
                      WHITESPACE@51..52 " "
                      BLOCK_EXPR@52..57
                        L_BRACE@52..53 "{"
                        WHITESPACE@53..54 " "
                        LITERAL@54..55
                          INT_NUMBER@54..55 "1"
                        WHITESPACE@55..56 " "
                        R_BRACE@56..57 "}"
                    SEMICOLON@57..58 ";"
                  WHITESPACE@58..63 "\n    "
                  MEMBER@63..73
                    TYPE_KW@63..67 "type"
                    WHITESPACE@67..68 " "
                    NAME@68..72
                      IDENT@68..72 "Item"
                    SEMICOLON@72..73 ";"
                  WHITESPACE@73..78 "\n    "
                  MEMBER@78..93
                    CONST_KW@78..83 "const"
                    WHITESPACE@83..84 " "
                    NAME@84..85
                      IDENT@84..85 "N"
                    COLON@85..86 ":"
                    WHITESPACE@86..87 " "
                    PATH_TYPE@87..92
                      NAME_REF@87..92
                        IDENT@87..92 "usize"
                    SEMICOLON@92..93 ";"
                  WHITESPACE@93..98 "\n    "
                  MEMBER@98..129
                    NAME@98..99
                      IDENT@98..99 "f"
                    COLON@99..100 ":"
                    WHITESPACE@100..101 " "
                    FN_TYPE@101..128
                      UNSAFE_KW@101..107 "unsafe"
                      WHITESPACE@107..108 " "
                      FN_KW@108..110 "fn"
                      PARAM_LIST@110..119
                        L_PAREN@110..111 "("
                        PARAM@111..118
                          BIND_PAT@111..112
                            NAME@111..112
                              IDENT@111..112 "x"
                          COLON@112..113 ":"
                          WHITESPACE@113..114 " "
                          PATH_TYPE@114..118
                            NAME_REF@114..118
                              IDENT@114..118 "Self"
                        R_PAREN@118..119 ")"
                      WHITESPACE@119..120 " "
                      RET_TYPE@120..128
                        THIN_ARROW@120..122 "->"
                        WHITESPACE@122..123 " "
                        PATH_TYPE@123..128
                          NAME_REF@123..128
                            IDENT@123..128 "usize"
                    SEMICOLON@128..129 ";"
                  WHITESPACE@129..134 "\n    "
                  MEMBER@134..136
                    NAME@134..135
                      IDENT@134..135 "g"
                    SEMICOLON@135..136 ";"
                  WHITESPACE@136..137 "\n"
                  R_BRACE@137..138 "}"
                SEMICOLON@138..139 ";"
              WHITESPACE@139..140 "\n"
            error 28..57: default members are not supported yet; a trait declares requirements (`name: fn(...) -> ...;`)
            error 63..67: associated types are not supported yet
            error 78..83: associated consts are not supported yet
            error 101..107: `unsafe` trait members are not supported yet
            error 134..136: a requirement declares its signature: `name: fn(...) -> ...;`
        "#]],
    );
}

#[test]
fn trait_requirement_param_is_not_mut() {
    // A requirement declares a signature; `mut` is a binding mode, which
    // only a body has.
    check(
        "trait T = requires { m: fn(mut n: usize) -> usize; };",
        expect![[r#"
            SOURCE_FILE@0..53
              TRAIT_ITEM@0..53
                TRAIT_KW@0..5 "trait"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "T"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                REQUIRES_DEF@10..52
                  REQUIRES_KW@10..18 "requires"
                  WHITESPACE@18..19 " "
                  L_BRACE@19..20 "{"
                  WHITESPACE@20..21 " "
                  MEMBER@21..50
                    NAME@21..22
                      IDENT@21..22 "m"
                    COLON@22..23 ":"
                    WHITESPACE@23..24 " "
                    FN_TYPE@24..49
                      FN_KW@24..26 "fn"
                      PARAM_LIST@26..40
                        L_PAREN@26..27 "("
                        PARAM@27..39
                          MUT_KW@27..30 "mut"
                          WHITESPACE@30..31 " "
                          BIND_PAT@31..32
                            NAME@31..32
                              IDENT@31..32 "n"
                          COLON@32..33 ":"
                          WHITESPACE@33..34 " "
                          PATH_TYPE@34..39
                            NAME_REF@34..39
                              IDENT@34..39 "usize"
                        R_PAREN@39..40 ")"
                      WHITESPACE@40..41 " "
                      RET_TYPE@41..49
                        THIN_ARROW@41..43 "->"
                        WHITESPACE@43..44 " "
                        PATH_TYPE@44..49
                          NAME_REF@44..49
                            IDENT@44..49 "usize"
                    SEMICOLON@49..50 ";"
                  WHITESPACE@50..51 " "
                  R_BRACE@51..52 "}"
                SEMICOLON@52..53 ";"
            error 27..32: a requirement's parameter is a plain `name: Type`
        "#]],
    );
}

#[test]
fn trait_requirement_param_is_not_a_pattern() {
    // Same rule for a destructuring parameter: which parts an
    // implementation picks apart is its own business, one per impl.
    check(
        "trait T = requires { m: fn(struct { a }: P) -> usize; };",
        expect![[r#"
            SOURCE_FILE@0..56
              TRAIT_ITEM@0..56
                TRAIT_KW@0..5 "trait"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "T"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                REQUIRES_DEF@10..55
                  REQUIRES_KW@10..18 "requires"
                  WHITESPACE@18..19 " "
                  L_BRACE@19..20 "{"
                  WHITESPACE@20..21 " "
                  MEMBER@21..53
                    NAME@21..22
                      IDENT@21..22 "m"
                    COLON@22..23 ":"
                    WHITESPACE@23..24 " "
                    FN_TYPE@24..52
                      FN_KW@24..26 "fn"
                      PARAM_LIST@26..43
                        L_PAREN@26..27 "("
                        PARAM@27..42
                          RECORD_PAT@27..39
                            STRUCT_KW@27..33 "struct"
                            WHITESPACE@33..34 " "
                            L_BRACE@34..35 "{"
                            WHITESPACE@35..36 " "
                            RECORD_PAT_FIELD@36..37
                              NAME@36..37
                                IDENT@36..37 "a"
                            WHITESPACE@37..38 " "
                            R_BRACE@38..39 "}"
                          COLON@39..40 ":"
                          WHITESPACE@40..41 " "
                          PATH_TYPE@41..42
                            NAME_REF@41..42
                              IDENT@41..42 "P"
                        R_PAREN@42..43 ")"
                      WHITESPACE@43..44 " "
                      RET_TYPE@44..52
                        THIN_ARROW@44..46 "->"
                        WHITESPACE@46..47 " "
                        PATH_TYPE@47..52
                          NAME_REF@47..52
                            IDENT@47..52 "usize"
                    SEMICOLON@52..53 ";"
                  WHITESPACE@53..54 " "
                  R_BRACE@54..55 "}"
                SEMICOLON@55..56 ";"
            error 27..39: a requirement's parameter is a plain `name: Type`
        "#]],
    );
}

#[test]
fn fn_binder_bounds_parse() {
    check(
        "static f = fn::<T: Display + Write>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..53
              STATIC_ITEM@0..53
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
                    TYPE_PARAM@16..34
                      NAME@16..17
                        IDENT@16..17 "T"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..26
                        NAME_REF@19..26
                          IDENT@19..26 "Display"
                      WHITESPACE@26..27 " "
                      PLUS@27..28 "+"
                      WHITESPACE@28..29 " "
                      PATH_TYPE@29..34
                        NAME_REF@29..34
                          IDENT@29..34 "Write"
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
fn type_decl_binder_bounds_reserved() {
    check(
        "type V = struct::<T: Display> { a: T };",
        expect![[r#"
            SOURCE_FILE@0..39
              TYPE_ITEM@0..39
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..6
                  IDENT@5..6 "V"
                WHITESPACE@6..7 " "
                EQ@7..8 "="
                WHITESPACE@8..9 " "
                RECORD_EXPR@9..38
                  STRUCT_KW@9..15 "struct"
                  GENERIC_PARAM_LIST@15..29
                    COLON2@15..17 "::"
                    L_ANGLE@17..18 "<"
                    TYPE_PARAM@18..28
                      NAME@18..19
                        IDENT@18..19 "T"
                      COLON@19..20 ":"
                      WHITESPACE@20..21 " "
                      PATH_TYPE@21..28
                        NAME_REF@21..28
                          IDENT@21..28 "Display"
                    R_ANGLE@28..29 ">"
                  WHITESPACE@29..30 " "
                  L_BRACE@30..31 "{"
                  WHITESPACE@31..32 " "
                  RECORD_EXPR_FIELD@32..36
                    NAME_REF@32..33
                      IDENT@32..33 "a"
                    COLON@33..34 ":"
                    WHITESPACE@34..35 " "
                    PATH_TYPE@35..36
                      NAME_REF@35..36
                        IDENT@35..36 "T"
                  WHITESPACE@36..37 " "
                  R_BRACE@37..38 "}"
                SEMICOLON@38..39 ";"
            error 18..28: bounds on a `type` declaration's binder are not supported yet
        "#]],
    );
}

#[test]
fn bound_shape_rules() {
    // A turbofished bound reserves (generic traits); a non-path bound is
    // rejected outright.
    check(
        "static f = fn::<T: Alloc::<usize>, U: fn() -> usize>(x: T) -> usize { 1 };",
        expect![[r#"
            SOURCE_FILE@0..74
              STATIC_ITEM@0..74
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..73
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..52
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    TYPE_PARAM@16..33
                      NAME@16..17
                        IDENT@16..17 "T"
                      COLON@17..18 ":"
                      WHITESPACE@18..19 " "
                      PATH_TYPE@19..33
                        NAME_REF@19..24
                          IDENT@19..24 "Alloc"
                        COLON2@24..26 "::"
                        GENERIC_ARG_LIST@26..33
                          L_ANGLE@26..27 "<"
                          TYPE_ARG@27..32
                            PATH_TYPE@27..32
                              NAME_REF@27..32
                                IDENT@27..32 "usize"
                          R_ANGLE@32..33 ">"
                    COMMA@33..34 ","
                    WHITESPACE@34..35 " "
                    TYPE_PARAM@35..51
                      NAME@35..36
                        IDENT@35..36 "U"
                      COLON@36..37 ":"
                      WHITESPACE@37..38 " "
                      FN_TYPE@38..51
                        FN_KW@38..40 "fn"
                        L_PAREN@40..41 "("
                        R_PAREN@41..42 ")"
                        WHITESPACE@42..43 " "
                        RET_TYPE@43..51
                          THIN_ARROW@43..45 "->"
                          WHITESPACE@45..46 " "
                          PATH_TYPE@46..51
                            NAME_REF@46..51
                              IDENT@46..51 "usize"
                    R_ANGLE@51..52 ">"
                  PARAM_LIST@52..58
                    L_PAREN@52..53 "("
                    PARAM@53..57
                      BIND_PAT@53..54
                        NAME@53..54
                          IDENT@53..54 "x"
                      COLON@54..55 ":"
                      WHITESPACE@55..56 " "
                      PATH_TYPE@56..57
                        NAME_REF@56..57
                          IDENT@56..57 "T"
                    R_PAREN@57..58 ")"
                  WHITESPACE@58..59 " "
                  RET_TYPE@59..67
                    THIN_ARROW@59..61 "->"
                    WHITESPACE@61..62 " "
                    PATH_TYPE@62..67
                      NAME_REF@62..67
                        IDENT@62..67 "usize"
                  WHITESPACE@67..68 " "
                  BLOCK_EXPR@68..73
                    L_BRACE@68..69 "{"
                    WHITESPACE@69..70 " "
                    LITERAL@70..71
                      INT_NUMBER@70..71 "1"
                    WHITESPACE@71..72 " "
                    R_BRACE@72..73 "}"
                SEMICOLON@73..74 ";"
            error 19..33: generic traits are not supported yet; a bound is a bare trait name
            error 38..51: only a trait name can be a bound
        "#]],
    );
}

#[test]
fn type_side_trait_impl_is_live() {
    check(
        r#"
trait Show = requires { show: fn(x: Self) -> str; };
type P = struct { a: usize } with {
    impl Show {
        show = fn(x: Self) -> str { "p" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..158
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..53
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..52
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..50
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..49
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..42
                        L_PAREN@33..34 "("
                        PARAM@34..41
                          BIND_PAT@34..35
                            NAME@34..35
                              IDENT@34..35 "x"
                          COLON@35..36 ":"
                          WHITESPACE@36..37 " "
                          PATH_TYPE@37..41
                            NAME_REF@37..41
                              IDENT@37..41 "Self"
                        R_PAREN@41..42 ")"
                      WHITESPACE@42..43 " "
                      RET_TYPE@43..49
                        THIN_ARROW@43..45 "->"
                        WHITESPACE@45..46 " "
                        PATH_TYPE@46..49
                          NAME_REF@46..49
                            IDENT@46..49 "str"
                    SEMICOLON@49..50 ";"
                  WHITESPACE@50..51 " "
                  R_BRACE@51..52 "}"
                SEMICOLON@52..53 ";"
              WHITESPACE@53..54 "\n"
              TYPE_ITEM@54..157
                TYPE_KW@54..58 "type"
                WHITESPACE@58..59 " "
                NAME@59..60
                  IDENT@59..60 "P"
                WHITESPACE@60..61 " "
                EQ@61..62 "="
                WHITESPACE@62..63 " "
                RECORD_EXPR@63..82
                  STRUCT_KW@63..69 "struct"
                  WHITESPACE@69..70 " "
                  L_BRACE@70..71 "{"
                  WHITESPACE@71..72 " "
                  RECORD_EXPR_FIELD@72..80
                    NAME_REF@72..73
                      IDENT@72..73 "a"
                    COLON@73..74 ":"
                    WHITESPACE@74..75 " "
                    PATH_TYPE@75..80
                      NAME_REF@75..80
                        IDENT@75..80 "usize"
                  WHITESPACE@80..81 " "
                  R_BRACE@81..82 "}"
                WHITESPACE@82..83 " "
                WITH_GROUP@83..156
                  WITH_KW@83..87 "with"
                  WHITESPACE@87..88 " "
                  L_BRACE@88..89 "{"
                  WHITESPACE@89..94 "\n    "
                  IMPL_ELEMENT@94..154
                    IMPL_KW@94..98 "impl"
                    WHITESPACE@98..99 " "
                    PATH_TYPE@99..103
                      NAME_REF@99..103
                        IDENT@99..103 "Show"
                    WHITESPACE@103..104 " "
                    L_BRACE@104..105 "{"
                    WHITESPACE@105..114 "\n        "
                    MEMBER@114..148
                      NAME@114..118
                        IDENT@114..118 "show"
                      WHITESPACE@118..119 " "
                      EQ@119..120 "="
                      WHITESPACE@120..121 " "
                      FN_LITERAL@121..147
                        FN_KW@121..123 "fn"
                        PARAM_LIST@123..132
                          L_PAREN@123..124 "("
                          PARAM@124..131
                            BIND_PAT@124..125
                              NAME@124..125
                                IDENT@124..125 "x"
                            COLON@125..126 ":"
                            WHITESPACE@126..127 " "
                            PATH_TYPE@127..131
                              NAME_REF@127..131
                                IDENT@127..131 "Self"
                          R_PAREN@131..132 ")"
                        WHITESPACE@132..133 " "
                        RET_TYPE@133..139
                          THIN_ARROW@133..135 "->"
                          WHITESPACE@135..136 " "
                          PATH_TYPE@136..139
                            NAME_REF@136..139
                              IDENT@136..139 "str"
                        WHITESPACE@139..140 " "
                        BLOCK_EXPR@140..147
                          L_BRACE@140..141 "{"
                          WHITESPACE@141..142 " "
                          LITERAL@142..145
                            STRING@142..145 "\"p\""
                          WHITESPACE@145..146 " "
                          R_BRACE@146..147 "}"
                      SEMICOLON@147..148 ";"
                    WHITESPACE@148..153 "\n    "
                    R_BRACE@153..154 "}"
                  WHITESPACE@154..155 "\n"
                  R_BRACE@155..156 "}"
                SEMICOLON@156..157 ";"
              WHITESPACE@157..158 "\n"
        "#]],
    );
}

#[test]
fn trait_side_impl_is_live() {
    check(
        r#"
trait Show = requires { show: fn(x: Self) -> str; } with {
    impl usize {
        show = fn(x: usize) -> str { "n" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..130
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..129
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..52
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..50
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..49
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..42
                        L_PAREN@33..34 "("
                        PARAM@34..41
                          BIND_PAT@34..35
                            NAME@34..35
                              IDENT@34..35 "x"
                          COLON@35..36 ":"
                          WHITESPACE@36..37 " "
                          PATH_TYPE@37..41
                            NAME_REF@37..41
                              IDENT@37..41 "Self"
                        R_PAREN@41..42 ")"
                      WHITESPACE@42..43 " "
                      RET_TYPE@43..49
                        THIN_ARROW@43..45 "->"
                        WHITESPACE@45..46 " "
                        PATH_TYPE@46..49
                          NAME_REF@46..49
                            IDENT@46..49 "str"
                    SEMICOLON@49..50 ";"
                  WHITESPACE@50..51 " "
                  R_BRACE@51..52 "}"
                WHITESPACE@52..53 " "
                WITH_GROUP@53..128
                  WITH_KW@53..57 "with"
                  WHITESPACE@57..58 " "
                  L_BRACE@58..59 "{"
                  WHITESPACE@59..64 "\n    "
                  IMPL_ELEMENT@64..126
                    IMPL_KW@64..68 "impl"
                    WHITESPACE@68..69 " "
                    PATH_TYPE@69..74
                      NAME_REF@69..74
                        IDENT@69..74 "usize"
                    WHITESPACE@74..75 " "
                    L_BRACE@75..76 "{"
                    WHITESPACE@76..85 "\n        "
                    MEMBER@85..120
                      NAME@85..89
                        IDENT@85..89 "show"
                      WHITESPACE@89..90 " "
                      EQ@90..91 "="
                      WHITESPACE@91..92 " "
                      FN_LITERAL@92..119
                        FN_KW@92..94 "fn"
                        PARAM_LIST@94..104
                          L_PAREN@94..95 "("
                          PARAM@95..103
                            BIND_PAT@95..96
                              NAME@95..96
                                IDENT@95..96 "x"
                            COLON@96..97 ":"
                            WHITESPACE@97..98 " "
                            PATH_TYPE@98..103
                              NAME_REF@98..103
                                IDENT@98..103 "usize"
                          R_PAREN@103..104 ")"
                        WHITESPACE@104..105 " "
                        RET_TYPE@105..111
                          THIN_ARROW@105..107 "->"
                          WHITESPACE@107..108 " "
                          PATH_TYPE@108..111
                            NAME_REF@108..111
                              IDENT@108..111 "str"
                        WHITESPACE@111..112 " "
                        BLOCK_EXPR@112..119
                          L_BRACE@112..113 "{"
                          WHITESPACE@113..114 " "
                          LITERAL@114..117
                            STRING@114..117 "\"n\""
                          WHITESPACE@117..118 " "
                          R_BRACE@118..119 "}"
                      SEMICOLON@119..120 ";"
                    WHITESPACE@120..125 "\n    "
                    R_BRACE@125..126 "}"
                  WHITESPACE@126..127 "\n"
                  R_BRACE@127..128 "}"
                SEMICOLON@128..129 ";"
              WHITESPACE@129..130 "\n"
        "#]],
    );
}

#[test]
fn marker_impl_reserved_and_impl_self_in_trait_chain_rejected() {
    check(
        r#"
trait M = requires { } with {
    impl send;
    impl Self { };
};
type P = struct { a: usize } with {
    impl send;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..122
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..67
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "M"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..23
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..22 " "
                  R_BRACE@22..23 "}"
                WHITESPACE@23..24 " "
                WITH_GROUP@24..66
                  WITH_KW@24..28 "with"
                  WHITESPACE@28..29 " "
                  L_BRACE@29..30 "{"
                  WHITESPACE@30..35 "\n    "
                  IMPL_ELEMENT@35..45
                    IMPL_KW@35..39 "impl"
                    WHITESPACE@39..40 " "
                    PATH_TYPE@40..44
                      NAME_REF@40..44
                        IDENT@40..44 "send"
                    SEMICOLON@44..45 ";"
                  WHITESPACE@45..50 "\n    "
                  IMPL_ELEMENT@50..63
                    IMPL_KW@50..54 "impl"
                    WHITESPACE@54..55 " "
                    PATH_TYPE@55..59
                      NAME_REF@55..59
                        IDENT@55..59 "Self"
                    WHITESPACE@59..60 " "
                    L_BRACE@60..61 "{"
                    WHITESPACE@61..62 " "
                    R_BRACE@62..63 "}"
                  SEMICOLON@63..64 ";"
                  WHITESPACE@64..65 "\n"
                  R_BRACE@65..66 "}"
                SEMICOLON@66..67 ";"
              WHITESPACE@67..68 "\n"
              TYPE_ITEM@68..121
                TYPE_KW@68..72 "type"
                WHITESPACE@72..73 " "
                NAME@73..74
                  IDENT@73..74 "P"
                WHITESPACE@74..75 " "
                EQ@75..76 "="
                WHITESPACE@76..77 " "
                RECORD_EXPR@77..96
                  STRUCT_KW@77..83 "struct"
                  WHITESPACE@83..84 " "
                  L_BRACE@84..85 "{"
                  WHITESPACE@85..86 " "
                  RECORD_EXPR_FIELD@86..94
                    NAME_REF@86..87
                      IDENT@86..87 "a"
                    COLON@87..88 ":"
                    WHITESPACE@88..89 " "
                    PATH_TYPE@89..94
                      NAME_REF@89..94
                        IDENT@89..94 "usize"
                  WHITESPACE@94..95 " "
                  R_BRACE@95..96 "}"
                WHITESPACE@96..97 " "
                WITH_GROUP@97..120
                  WITH_KW@97..101 "with"
                  WHITESPACE@101..102 " "
                  L_BRACE@102..103 "{"
                  WHITESPACE@103..108 "\n    "
                  IMPL_ELEMENT@108..118
                    IMPL_KW@108..112 "impl"
                    WHITESPACE@112..113 " "
                    PATH_TYPE@113..117
                      NAME_REF@113..117
                        IDENT@113..117 "send"
                    SEMICOLON@117..118 ";"
                  WHITESPACE@118..119 "\n"
                  R_BRACE@119..120 "}"
                SEMICOLON@120..121 ";"
              WHITESPACE@121..122 "\n"
            error 40..44: marker impls (`impl name;`) are not supported yet
            error 55..59: an impl in a trait's `with`-chain names the IMPLEMENTING type, not `Self`
            error 113..117: marker impls (`impl name;`) are not supported yet
        "#]],
    );
}

#[test]
fn trait_impl_on_generic_type_reserved() {
    check(
        r#"
trait Show = requires { show: fn(x: Self) -> str; };
type V = struct::<T> { a: T } with {
    impl Show {
        show = fn(x: Self) -> str { "v" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..159
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..53
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..52
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..50
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..49
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..42
                        L_PAREN@33..34 "("
                        PARAM@34..41
                          BIND_PAT@34..35
                            NAME@34..35
                              IDENT@34..35 "x"
                          COLON@35..36 ":"
                          WHITESPACE@36..37 " "
                          PATH_TYPE@37..41
                            NAME_REF@37..41
                              IDENT@37..41 "Self"
                        R_PAREN@41..42 ")"
                      WHITESPACE@42..43 " "
                      RET_TYPE@43..49
                        THIN_ARROW@43..45 "->"
                        WHITESPACE@45..46 " "
                        PATH_TYPE@46..49
                          NAME_REF@46..49
                            IDENT@46..49 "str"
                    SEMICOLON@49..50 ";"
                  WHITESPACE@50..51 " "
                  R_BRACE@51..52 "}"
                SEMICOLON@52..53 ";"
              WHITESPACE@53..54 "\n"
              TYPE_ITEM@54..158
                TYPE_KW@54..58 "type"
                WHITESPACE@58..59 " "
                NAME@59..60
                  IDENT@59..60 "V"
                WHITESPACE@60..61 " "
                EQ@61..62 "="
                WHITESPACE@62..63 " "
                RECORD_EXPR@63..83
                  STRUCT_KW@63..69 "struct"
                  GENERIC_PARAM_LIST@69..74
                    COLON2@69..71 "::"
                    L_ANGLE@71..72 "<"
                    TYPE_PARAM@72..73
                      NAME@72..73
                        IDENT@72..73 "T"
                    R_ANGLE@73..74 ">"
                  WHITESPACE@74..75 " "
                  L_BRACE@75..76 "{"
                  WHITESPACE@76..77 " "
                  RECORD_EXPR_FIELD@77..81
                    NAME_REF@77..78
                      IDENT@77..78 "a"
                    COLON@78..79 ":"
                    WHITESPACE@79..80 " "
                    PATH_TYPE@80..81
                      NAME_REF@80..81
                        IDENT@80..81 "T"
                  WHITESPACE@81..82 " "
                  R_BRACE@82..83 "}"
                WHITESPACE@83..84 " "
                WITH_GROUP@84..157
                  WITH_KW@84..88 "with"
                  WHITESPACE@88..89 " "
                  L_BRACE@89..90 "{"
                  WHITESPACE@90..95 "\n    "
                  IMPL_ELEMENT@95..155
                    IMPL_KW@95..99 "impl"
                    WHITESPACE@99..100 " "
                    PATH_TYPE@100..104
                      NAME_REF@100..104
                        IDENT@100..104 "Show"
                    WHITESPACE@104..105 " "
                    L_BRACE@105..106 "{"
                    WHITESPACE@106..115 "\n        "
                    MEMBER@115..149
                      NAME@115..119
                        IDENT@115..119 "show"
                      WHITESPACE@119..120 " "
                      EQ@120..121 "="
                      WHITESPACE@121..122 " "
                      FN_LITERAL@122..148
                        FN_KW@122..124 "fn"
                        PARAM_LIST@124..133
                          L_PAREN@124..125 "("
                          PARAM@125..132
                            BIND_PAT@125..126
                              NAME@125..126
                                IDENT@125..126 "x"
                            COLON@126..127 ":"
                            WHITESPACE@127..128 " "
                            PATH_TYPE@128..132
                              NAME_REF@128..132
                                IDENT@128..132 "Self"
                          R_PAREN@132..133 ")"
                        WHITESPACE@133..134 " "
                        RET_TYPE@134..140
                          THIN_ARROW@134..136 "->"
                          WHITESPACE@136..137 " "
                          PATH_TYPE@137..140
                            NAME_REF@137..140
                              IDENT@137..140 "str"
                        WHITESPACE@140..141 " "
                        BLOCK_EXPR@141..148
                          L_BRACE@141..142 "{"
                          WHITESPACE@142..143 " "
                          LITERAL@143..146
                            STRING@143..146 "\"v\""
                          WHITESPACE@146..147 " "
                          R_BRACE@147..148 "}"
                      SEMICOLON@148..149 ";"
                    WHITESPACE@149..154 "\n    "
                    R_BRACE@154..155 "}"
                  WHITESPACE@155..156 "\n"
                  R_BRACE@156..157 "}"
                SEMICOLON@157..158 ";"
              WHITESPACE@158..159 "\n"
            error 100..104: trait impls on generic types are not supported yet
        "#]],
    );
}

#[test]
fn generic_binder_allowed_on_trait_impl_member() {
    check(
        r#"
trait D = requires { fmt: fn::<W>(w: W, x: Self) -> W; };
type P = struct { a: usize } with {
    impl D {
        fmt = fn::<W>(w: W, x: Self) -> W { w };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..166
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..58
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "D"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..57
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..22 " "
                  MEMBER@22..55
                    NAME@22..25
                      IDENT@22..25 "fmt"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    FN_TYPE@27..54
                      FN_KW@27..29 "fn"
                      GENERIC_PARAM_LIST@29..34
                        COLON2@29..31 "::"
                        L_ANGLE@31..32 "<"
                        TYPE_PARAM@32..33
                          NAME@32..33
                            IDENT@32..33 "W"
                        R_ANGLE@33..34 ">"
                      PARAM_LIST@34..49
                        L_PAREN@34..35 "("
                        PARAM@35..39
                          BIND_PAT@35..36
                            NAME@35..36
                              IDENT@35..36 "w"
                          COLON@36..37 ":"
                          WHITESPACE@37..38 " "
                          PATH_TYPE@38..39
                            NAME_REF@38..39
                              IDENT@38..39 "W"
                        COMMA@39..40 ","
                        WHITESPACE@40..41 " "
                        PARAM@41..48
                          BIND_PAT@41..42
                            NAME@41..42
                              IDENT@41..42 "x"
                          COLON@42..43 ":"
                          WHITESPACE@43..44 " "
                          PATH_TYPE@44..48
                            NAME_REF@44..48
                              IDENT@44..48 "Self"
                        R_PAREN@48..49 ")"
                      WHITESPACE@49..50 " "
                      RET_TYPE@50..54
                        THIN_ARROW@50..52 "->"
                        WHITESPACE@52..53 " "
                        PATH_TYPE@53..54
                          NAME_REF@53..54
                            IDENT@53..54 "W"
                    SEMICOLON@54..55 ";"
                  WHITESPACE@55..56 " "
                  R_BRACE@56..57 "}"
                SEMICOLON@57..58 ";"
              WHITESPACE@58..59 "\n"
              TYPE_ITEM@59..165
                TYPE_KW@59..63 "type"
                WHITESPACE@63..64 " "
                NAME@64..65
                  IDENT@64..65 "P"
                WHITESPACE@65..66 " "
                EQ@66..67 "="
                WHITESPACE@67..68 " "
                RECORD_EXPR@68..87
                  STRUCT_KW@68..74 "struct"
                  WHITESPACE@74..75 " "
                  L_BRACE@75..76 "{"
                  WHITESPACE@76..77 " "
                  RECORD_EXPR_FIELD@77..85
                    NAME_REF@77..78
                      IDENT@77..78 "a"
                    COLON@78..79 ":"
                    WHITESPACE@79..80 " "
                    PATH_TYPE@80..85
                      NAME_REF@80..85
                        IDENT@80..85 "usize"
                  WHITESPACE@85..86 " "
                  R_BRACE@86..87 "}"
                WHITESPACE@87..88 " "
                WITH_GROUP@88..164
                  WITH_KW@88..92 "with"
                  WHITESPACE@92..93 " "
                  L_BRACE@93..94 "{"
                  WHITESPACE@94..99 "\n    "
                  IMPL_ELEMENT@99..162
                    IMPL_KW@99..103 "impl"
                    WHITESPACE@103..104 " "
                    PATH_TYPE@104..105
                      NAME_REF@104..105
                        IDENT@104..105 "D"
                    WHITESPACE@105..106 " "
                    L_BRACE@106..107 "{"
                    WHITESPACE@107..116 "\n        "
                    MEMBER@116..156
                      NAME@116..119
                        IDENT@116..119 "fmt"
                      WHITESPACE@119..120 " "
                      EQ@120..121 "="
                      WHITESPACE@121..122 " "
                      FN_LITERAL@122..155
                        FN_KW@122..124 "fn"
                        GENERIC_PARAM_LIST@124..129
                          COLON2@124..126 "::"
                          L_ANGLE@126..127 "<"
                          TYPE_PARAM@127..128
                            NAME@127..128
                              IDENT@127..128 "W"
                          R_ANGLE@128..129 ">"
                        PARAM_LIST@129..144
                          L_PAREN@129..130 "("
                          PARAM@130..134
                            BIND_PAT@130..131
                              NAME@130..131
                                IDENT@130..131 "w"
                            COLON@131..132 ":"
                            WHITESPACE@132..133 " "
                            PATH_TYPE@133..134
                              NAME_REF@133..134
                                IDENT@133..134 "W"
                          COMMA@134..135 ","
                          WHITESPACE@135..136 " "
                          PARAM@136..143
                            BIND_PAT@136..137
                              NAME@136..137
                                IDENT@136..137 "x"
                            COLON@137..138 ":"
                            WHITESPACE@138..139 " "
                            PATH_TYPE@139..143
                              NAME_REF@139..143
                                IDENT@139..143 "Self"
                          R_PAREN@143..144 ")"
                        WHITESPACE@144..145 " "
                        RET_TYPE@145..149
                          THIN_ARROW@145..147 "->"
                          WHITESPACE@147..148 " "
                          PATH_TYPE@148..149
                            NAME_REF@148..149
                              IDENT@148..149 "W"
                        WHITESPACE@149..150 " "
                        BLOCK_EXPR@150..155
                          L_BRACE@150..151 "{"
                          WHITESPACE@151..152 " "
                          PATH_EXPR@152..153
                            NAME_REF@152..153
                              IDENT@152..153 "w"
                          WHITESPACE@153..154 " "
                          R_BRACE@154..155 "}"
                      SEMICOLON@155..156 ";"
                    WHITESPACE@156..161 "\n    "
                    R_BRACE@161..162 "}"
                  WHITESPACE@162..163 "\n"
                  R_BRACE@163..164 "}"
                SEMICOLON@164..165 ";"
              WHITESPACE@165..166 "\n"
        "#]],
    );
}

#[test]
fn colon_declared_member_in_impl_rejected() {
    check(
        r#"
trait D = requires { fmt: fn(x: Self) -> str; };
type P = struct { a: usize } with {
    impl D {
        fmt: fn(x: Self) -> str;
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..141
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..49
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "D"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..48
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..22 " "
                  MEMBER@22..46
                    NAME@22..25
                      IDENT@22..25 "fmt"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    FN_TYPE@27..45
                      FN_KW@27..29 "fn"
                      PARAM_LIST@29..38
                        L_PAREN@29..30 "("
                        PARAM@30..37
                          BIND_PAT@30..31
                            NAME@30..31
                              IDENT@30..31 "x"
                          COLON@31..32 ":"
                          WHITESPACE@32..33 " "
                          PATH_TYPE@33..37
                            NAME_REF@33..37
                              IDENT@33..37 "Self"
                        R_PAREN@37..38 ")"
                      WHITESPACE@38..39 " "
                      RET_TYPE@39..45
                        THIN_ARROW@39..41 "->"
                        WHITESPACE@41..42 " "
                        PATH_TYPE@42..45
                          NAME_REF@42..45
                            IDENT@42..45 "str"
                    SEMICOLON@45..46 ";"
                  WHITESPACE@46..47 " "
                  R_BRACE@47..48 "}"
                SEMICOLON@48..49 ";"
              WHITESPACE@49..50 "\n"
              TYPE_ITEM@50..140
                TYPE_KW@50..54 "type"
                WHITESPACE@54..55 " "
                NAME@55..56
                  IDENT@55..56 "P"
                WHITESPACE@56..57 " "
                EQ@57..58 "="
                WHITESPACE@58..59 " "
                RECORD_EXPR@59..78
                  STRUCT_KW@59..65 "struct"
                  WHITESPACE@65..66 " "
                  L_BRACE@66..67 "{"
                  WHITESPACE@67..68 " "
                  RECORD_EXPR_FIELD@68..76
                    NAME_REF@68..69
                      IDENT@68..69 "a"
                    COLON@69..70 ":"
                    WHITESPACE@70..71 " "
                    PATH_TYPE@71..76
                      NAME_REF@71..76
                        IDENT@71..76 "usize"
                  WHITESPACE@76..77 " "
                  R_BRACE@77..78 "}"
                WHITESPACE@78..79 " "
                WITH_GROUP@79..139
                  WITH_KW@79..83 "with"
                  WHITESPACE@83..84 " "
                  L_BRACE@84..85 "{"
                  WHITESPACE@85..90 "\n    "
                  IMPL_ELEMENT@90..137
                    IMPL_KW@90..94 "impl"
                    WHITESPACE@94..95 " "
                    PATH_TYPE@95..96
                      NAME_REF@95..96
                        IDENT@95..96 "D"
                    WHITESPACE@96..97 " "
                    L_BRACE@97..98 "{"
                    WHITESPACE@98..107 "\n        "
                    MEMBER@107..131
                      NAME@107..110
                        IDENT@107..110 "fmt"
                      COLON@110..111 ":"
                      WHITESPACE@111..112 " "
                      FN_TYPE@112..130
                        FN_KW@112..114 "fn"
                        PARAM_LIST@114..123
                          L_PAREN@114..115 "("
                          PARAM@115..122
                            BIND_PAT@115..116
                              NAME@115..116
                                IDENT@115..116 "x"
                            COLON@116..117 ":"
                            WHITESPACE@117..118 " "
                            PATH_TYPE@118..122
                              NAME_REF@118..122
                                IDENT@118..122 "Self"
                          R_PAREN@122..123 ")"
                        WHITESPACE@123..124 " "
                        RET_TYPE@124..130
                          THIN_ARROW@124..126 "->"
                          WHITESPACE@126..127 " "
                          PATH_TYPE@127..130
                            NAME_REF@127..130
                              IDENT@127..130 "str"
                      SEMICOLON@130..131 ";"
                    WHITESPACE@131..136 "\n    "
                    R_BRACE@136..137 "}"
                  WHITESPACE@137..138 "\n"
                  R_BRACE@138..139 "}"
                SEMICOLON@139..140 ";"
              WHITESPACE@140..141 "\n"
            error 107..131: an impl member is defined with `=`; the colon-declared requirement form belongs in the trait declaration
        "#]],
    );
}

#[test]
fn statement_position_block_minus_operator_is_one_bin_expr() {
    // Expression-first statement grammar (G01): a
    // block-valued form in statement position does NOT terminate the
    // statement — `unsafe { 3 } - 2` as a fn tail is ONE BIN_EXPR (the
    // opposite of Rust's block-terminates-statement rule). Pinned so the
    // rule can't silently regress.
    check(
        "static f = fn() -> usize { unsafe { 3 } - 2 };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..45
                  FN_KW@11..13 "fn"
                  PARAM_LIST@13..15
                    L_PAREN@13..14 "("
                    R_PAREN@14..15 ")"
                  WHITESPACE@15..16 " "
                  RET_TYPE@16..24
                    THIN_ARROW@16..18 "->"
                    WHITESPACE@18..19 " "
                    PATH_TYPE@19..24
                      NAME_REF@19..24
                        IDENT@19..24 "usize"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..45
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    BIN_EXPR@27..43
                      UNSAFE_BLOCK_EXPR@27..39
                        UNSAFE_KW@27..33 "unsafe"
                        WHITESPACE@33..34 " "
                        BLOCK_EXPR@34..39
                          L_BRACE@34..35 "{"
                          WHITESPACE@35..36 " "
                          LITERAL@36..37
                            INT_NUMBER@36..37 "3"
                          WHITESPACE@37..38 " "
                          R_BRACE@38..39 "}"
                      WHITESPACE@39..40 " "
                      MINUS@40..41 "-"
                      WHITESPACE@41..42 " "
                      LITERAL@42..43
                        INT_NUMBER@42..43 "2"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn impls_in_generic_trait_chain_reserved() {
    // A RESERVED generic trait's chain must not go live: each impl in it
    // carries its own reservation (no silent semantics).
    check(
        r#"
trait Gen = requires::<T> { get: fn(x: Self) -> usize; } with {
    impl usize { get = fn(x: usize) -> usize { x }; }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..122
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..121
                TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..10
                  IDENT@7..10 "Gen"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                REQUIRES_DEF@13..57
                  REQUIRES_KW@13..21 "requires"
                  GENERIC_PARAM_LIST@21..26
                    COLON2@21..23 "::"
                    L_ANGLE@23..24 "<"
                    TYPE_PARAM@24..25
                      NAME@24..25
                        IDENT@24..25 "T"
                    R_ANGLE@25..26 ">"
                  WHITESPACE@26..27 " "
                  L_BRACE@27..28 "{"
                  WHITESPACE@28..29 " "
                  MEMBER@29..55
                    NAME@29..32
                      IDENT@29..32 "get"
                    COLON@32..33 ":"
                    WHITESPACE@33..34 " "
                    FN_TYPE@34..54
                      FN_KW@34..36 "fn"
                      PARAM_LIST@36..45
                        L_PAREN@36..37 "("
                        PARAM@37..44
                          BIND_PAT@37..38
                            NAME@37..38
                              IDENT@37..38 "x"
                          COLON@38..39 ":"
                          WHITESPACE@39..40 " "
                          PATH_TYPE@40..44
                            NAME_REF@40..44
                              IDENT@40..44 "Self"
                        R_PAREN@44..45 ")"
                      WHITESPACE@45..46 " "
                      RET_TYPE@46..54
                        THIN_ARROW@46..48 "->"
                        WHITESPACE@48..49 " "
                        PATH_TYPE@49..54
                          NAME_REF@49..54
                            IDENT@49..54 "usize"
                    SEMICOLON@54..55 ";"
                  WHITESPACE@55..56 " "
                  R_BRACE@56..57 "}"
                WHITESPACE@57..58 " "
                WITH_GROUP@58..120
                  WITH_KW@58..62 "with"
                  WHITESPACE@62..63 " "
                  L_BRACE@63..64 "{"
                  WHITESPACE@64..69 "\n    "
                  IMPL_ELEMENT@69..118
                    IMPL_KW@69..73 "impl"
                    WHITESPACE@73..74 " "
                    PATH_TYPE@74..79
                      NAME_REF@74..79
                        IDENT@74..79 "usize"
                    WHITESPACE@79..80 " "
                    L_BRACE@80..81 "{"
                    WHITESPACE@81..82 " "
                    MEMBER@82..116
                      NAME@82..85
                        IDENT@82..85 "get"
                      WHITESPACE@85..86 " "
                      EQ@86..87 "="
                      WHITESPACE@87..88 " "
                      FN_LITERAL@88..115
                        FN_KW@88..90 "fn"
                        PARAM_LIST@90..100
                          L_PAREN@90..91 "("
                          PARAM@91..99
                            BIND_PAT@91..92
                              NAME@91..92
                                IDENT@91..92 "x"
                            COLON@92..93 ":"
                            WHITESPACE@93..94 " "
                            PATH_TYPE@94..99
                              NAME_REF@94..99
                                IDENT@94..99 "usize"
                          R_PAREN@99..100 ")"
                        WHITESPACE@100..101 " "
                        RET_TYPE@101..109
                          THIN_ARROW@101..103 "->"
                          WHITESPACE@103..104 " "
                          PATH_TYPE@104..109
                            NAME_REF@104..109
                              IDENT@104..109 "usize"
                        WHITESPACE@109..110 " "
                        BLOCK_EXPR@110..115
                          L_BRACE@110..111 "{"
                          WHITESPACE@111..112 " "
                          PATH_EXPR@112..113
                            NAME_REF@112..113
                              IDENT@112..113 "x"
                          WHITESPACE@113..114 " "
                          R_BRACE@114..115 "}"
                      SEMICOLON@115..116 ";"
                    WHITESPACE@116..117 " "
                    R_BRACE@117..118 "}"
                  WHITESPACE@118..119 "\n"
                  R_BRACE@119..120 "}"
                SEMICOLON@120..121 ";"
              WHITESPACE@121..122 "\n"
            error 21..26: generic traits are not supported yet
            error 74..79: impls in a generic trait's `with`-chain are not supported yet (generic traits are reserved)
        "#]],
    );
}

// ---- keyword-table drift guards ---------------------------------------
//
// One canonical table (`syntax_kind::KEYWORDS`) generates `from_keyword`
// and `is_keyword`; these tests pin that nothing can drift away from it —
// including a `*_KW` kind added to the enum but forgotten in the table,
// which is exactly how `trait`/`requires` ended up unhighlighted.

#[test]
fn every_keyword_in_the_table_lexes_and_answers_is_keyword() {
    for &(text, kind) in crate::KEYWORDS {
        assert_eq!(
            crate::SyntaxKind::from_keyword(text),
            Some(kind),
            "`{text}` does not round-trip through `from_keyword`"
        );
        assert!(kind.is_keyword(), "{kind:?} is not `is_keyword`");
        // The lexer must actually produce the kind for that spelling.
        let (tokens, _) = crate::tokenize(text);
        assert_eq!(
            tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            vec![kind],
            "lexing `{text}` did not yield {kind:?}"
        );
        assert!(
            format!("{kind:?}").ends_with("_KW"),
            "{kind:?} is in the keyword table but is not a `*_KW` kind"
        );
    }
}

#[test]
fn every_kw_kind_is_in_the_canonical_keyword_table() {
    // Walks the whole `SyntaxKind` enum by discriminant: any variant whose
    // name ends in `_KW` must be reachable from the table, so adding a
    // keyword kind without a table entry fails here rather than silently
    // losing its highlighting.
    for raw in 0..=(crate::SyntaxKind::ERROR as u16) {
        let kind = crate::SyntaxKind::from(rowan::SyntaxKind(raw));
        if !format!("{kind:?}").ends_with("_KW") {
            continue;
        }
        assert!(
            crate::KEYWORDS.iter().any(|&(_, k)| k == kind),
            "{kind:?} is missing from the canonical keyword table \
             (`syntax_kind::keywords!`) — add its spelling there"
        );
        assert!(kind.is_keyword(), "{kind:?} is not `is_keyword`");
    }
}

#[test]
fn non_keyword_kinds_are_not_keywords() {
    for kind in [
        crate::SyntaxKind::IDENT,
        crate::SyntaxKind::COMMENT,
        crate::SyntaxKind::INT_NUMBER,
        crate::SyntaxKind::L_BRACE,
        crate::SyntaxKind::TRAIT_ITEM,
        crate::SyntaxKind::ERROR,
    ] {
        assert!(!kind.is_keyword(), "{kind:?} must not be `is_keyword`");
    }
    assert_eq!(crate::SyntaxKind::from_keyword("nonsense"), None);
    assert_eq!(crate::SyntaxKind::from_keyword("Self"), None);
}

// ---- regions -------------------------------------------------------------

#[test]
fn region_binders_are_a_third_kind_in_one_list() {
    // ONE binder list, THREE kinds. Regions ride the same slot as types and
    // consts, told apart by their sigil alone — no ordering rule, no
    // separate list, and `@` is unclaimed everywhere else so the lexer
    // needs no lookahead to know one.
    check(
        "static f = fn::<@a, T, const N: usize>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..56
              STATIC_ITEM@0..56
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..55
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..38
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..18
                      REGION_IDENT@16..18 "@a"
                    COMMA@18..19 ","
                    WHITESPACE@19..20 " "
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
                  PARAM_LIST@38..44
                    L_PAREN@38..39 "("
                    PARAM@39..43
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "x"
                      COLON@40..41 ":"
                      WHITESPACE@41..42 " "
                      PATH_TYPE@42..43
                        NAME_REF@42..43
                          IDENT@42..43 "T"
                    R_PAREN@43..44 ")"
                  WHITESPACE@44..45 " "
                  RET_TYPE@45..49
                    THIN_ARROW@45..47 "->"
                    WHITESPACE@47..48 " "
                    PATH_TYPE@48..49
                      NAME_REF@48..49
                        IDENT@48..49 "T"
                  WHITESPACE@49..50 " "
                  BLOCK_EXPR@50..55
                    L_BRACE@50..51 "{"
                    WHITESPACE@51..52 " "
                    PATH_EXPR@52..53
                      NAME_REF@52..53
                        IDENT@52..53 "x"
                    WHITESPACE@53..54 " "
                    R_BRACE@54..55 "}"
                SEMICOLON@55..56 ";"
        "#]],
    );
}

#[test]
fn region_params_carry_outlives_bounds() {
    // Bounds live where the param is born, exactly as a type param's trait
    // bounds do — and a region's bounds are regions, never traits.
    check(
        "static f = fn::<@a: @b + @c, @b, @c>() -> () {};",
        expect![[r#"
            SOURCE_FILE@0..48
              STATIC_ITEM@0..48
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..47
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..36
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..27
                      REGION_IDENT@16..18 "@a"
                      COLON@18..19 ":"
                      WHITESPACE@19..20 " "
                      REGION_IDENT@20..22 "@b"
                      WHITESPACE@22..23 " "
                      PLUS@23..24 "+"
                      WHITESPACE@24..25 " "
                      REGION_IDENT@25..27 "@c"
                    COMMA@27..28 ","
                    WHITESPACE@28..29 " "
                    REGION_PARAM@29..31
                      REGION_IDENT@29..31 "@b"
                    COMMA@31..32 ","
                    WHITESPACE@32..33 " "
                    REGION_PARAM@33..35
                      REGION_IDENT@33..35 "@c"
                    R_ANGLE@35..36 ">"
                  PARAM_LIST@36..38
                    L_PAREN@36..37 "("
                    R_PAREN@37..38 ")"
                  WHITESPACE@38..39 " "
                  RET_TYPE@39..44
                    THIN_ARROW@39..41 "->"
                    WHITESPACE@41..42 " "
                    UNIT_TYPE@42..44
                      L_PAREN@42..43 "("
                      R_PAREN@43..44 ")"
                  WHITESPACE@44..45 " "
                  BLOCK_EXPR@45..47
                    L_BRACE@45..46 "{"
                    R_BRACE@46..47 "}"
                SEMICOLON@47..48 ";"
        "#]],
    );
}

#[test]
fn the_region_wildcard_and_the_join() {
    check(
        "static f = fn::<@a, @b>(x: usize.&::<@a + @b>) -> () { let r: usize.&::<@_> = x; };",
        expect![[r#"
            SOURCE_FILE@0..83
              STATIC_ITEM@0..83
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..82
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..23
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..18
                      REGION_IDENT@16..18 "@a"
                    COMMA@18..19 ","
                    WHITESPACE@19..20 " "
                    REGION_PARAM@20..22
                      REGION_IDENT@20..22 "@b"
                    R_ANGLE@22..23 ">"
                  PARAM_LIST@23..46
                    L_PAREN@23..24 "("
                    PARAM@24..45
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      BORROW_TYPE@27..45
                        PATH_TYPE@27..32
                          NAME_REF@27..32
                            IDENT@27..32 "usize"
                        DOT@32..33 "."
                        AMP@33..34 "&"
                        COLON2@34..36 "::"
                        GENERIC_ARG_LIST@36..45
                          L_ANGLE@36..37 "<"
                          REGION_ARG@37..44
                            REGION_IDENT@37..39 "@a"
                            WHITESPACE@39..40 " "
                            PLUS@40..41 "+"
                            WHITESPACE@41..42 " "
                            REGION_IDENT@42..44 "@b"
                          R_ANGLE@44..45 ">"
                    R_PAREN@45..46 ")"
                  WHITESPACE@46..47 " "
                  RET_TYPE@47..52
                    THIN_ARROW@47..49 "->"
                    WHITESPACE@49..50 " "
                    UNIT_TYPE@50..52
                      L_PAREN@50..51 "("
                      R_PAREN@51..52 ")"
                  WHITESPACE@52..53 " "
                  BLOCK_EXPR@53..82
                    L_BRACE@53..54 "{"
                    WHITESPACE@54..55 " "
                    LET_STMT@55..80
                      LET_KW@55..58 "let"
                      WHITESPACE@58..59 " "
                      BIND_PAT@59..60
                        NAME@59..60
                          IDENT@59..60 "r"
                      COLON@60..61 ":"
                      WHITESPACE@61..62 " "
                      BORROW_TYPE@62..75
                        PATH_TYPE@62..67
                          NAME_REF@62..67
                            IDENT@62..67 "usize"
                        DOT@67..68 "."
                        AMP@68..69 "&"
                        COLON2@69..71 "::"
                        GENERIC_ARG_LIST@71..75
                          L_ANGLE@71..72 "<"
                          REGION_ARG@72..74
                            REGION_IDENT@72..74 "@_"
                          R_ANGLE@74..75 ">"
                      WHITESPACE@75..76 " "
                      EQ@76..77 "="
                      WHITESPACE@77..78 " "
                      PATH_EXPR@78..79
                        NAME_REF@78..79
                          IDENT@78..79 "x"
                      SEMICOLON@79..80 ";"
                    WHITESPACE@80..81 " "
                    R_BRACE@81..82 "}"
                SEMICOLON@82..83 ";"
        "#]],
    );
}

#[test]
fn a_bare_at_sign_is_an_error_that_names_the_spelling() {
    check(
        "static f = fn::<@>() -> () {};",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..29
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..18
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    ERROR@16..17
                      ERROR_TOKEN@16..17 "@"
                    R_ANGLE@17..18 ">"
                  PARAM_LIST@18..20
                    L_PAREN@18..19 "("
                    R_PAREN@19..20 ")"
                  WHITESPACE@20..21 " "
                  RET_TYPE@21..26
                    THIN_ARROW@21..23 "->"
                    WHITESPACE@23..24 " "
                    UNIT_TYPE@24..26
                      L_PAREN@24..25 "("
                      R_PAREN@25..26 ")"
                  WHITESPACE@26..27 " "
                  BLOCK_EXPR@27..29
                    L_BRACE@27..28 "{"
                    R_BRACE@28..29 "}"
                SEMICOLON@29..30 ";"
            error 16..17: expected a region name after `@` (`@a`, or `@_` to infer one)
        "#]],
    );
}

#[test]
fn borrow_expressions_take_their_own_turbofish() {
    // The region rides the BORROW OPERATOR's turbofish, not the referent
    // type's — `x.&mut::<@a>`, one list hanging off the borrow node, so no
    // consumer can read it as anything else's arguments.
    check(
        "static f = fn () -> () { let a = x.&; let b = y.&mut::<@r>; };",
        expect![[r#"
            SOURCE_FILE@0..62
              STATIC_ITEM@0..62
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..61
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..16
                    L_PAREN@14..15 "("
                    R_PAREN@15..16 ")"
                  WHITESPACE@16..17 " "
                  RET_TYPE@17..22
                    THIN_ARROW@17..19 "->"
                    WHITESPACE@19..20 " "
                    UNIT_TYPE@20..22
                      L_PAREN@20..21 "("
                      R_PAREN@21..22 ")"
                  WHITESPACE@22..23 " "
                  BLOCK_EXPR@23..61
                    L_BRACE@23..24 "{"
                    WHITESPACE@24..25 " "
                    LET_STMT@25..37
                      LET_KW@25..28 "let"
                      WHITESPACE@28..29 " "
                      BIND_PAT@29..30
                        NAME@29..30
                          IDENT@29..30 "a"
                      WHITESPACE@30..31 " "
                      EQ@31..32 "="
                      WHITESPACE@32..33 " "
                      BORROW_EXPR@33..36
                        PATH_EXPR@33..34
                          NAME_REF@33..34
                            IDENT@33..34 "x"
                        DOT@34..35 "."
                        AMP@35..36 "&"
                      SEMICOLON@36..37 ";"
                    WHITESPACE@37..38 " "
                    LET_STMT@38..59
                      LET_KW@38..41 "let"
                      WHITESPACE@41..42 " "
                      BIND_PAT@42..43
                        NAME@42..43
                          IDENT@42..43 "b"
                      WHITESPACE@43..44 " "
                      EQ@44..45 "="
                      WHITESPACE@45..46 " "
                      BORROW_EXPR@46..58
                        PATH_EXPR@46..47
                          NAME_REF@46..47
                            IDENT@46..47 "y"
                        DOT@47..48 "."
                        AMP@48..49 "&"
                        MUT_KW@49..52 "mut"
                        COLON2@52..54 "::"
                        GENERIC_ARG_LIST@54..58
                          L_ANGLE@54..55 "<"
                          REGION_ARG@55..57
                            REGION_IDENT@55..57 "@r"
                          R_ANGLE@57..58 ">"
                      SEMICOLON@58..59 ";"
                    WHITESPACE@59..60 " "
                    R_BRACE@60..61 "}"
                SEMICOLON@61..62 ";"
        "#]],
    );
}

#[test]
fn outlives_clauses_ride_the_with_clause_grammar() {
    // A region-headed constrain clause, the third `with_clause` shape.
    // Parse-and-reserve — regions on type declarations are not decided yet,
    // so this parses its real tree and validation says "not yet".
    check(
        "type Slice = struct::<@a, T> { n: usize } with @a: @b { impl Self {} };",
        expect![[r#"
            SOURCE_FILE@0..71
              TYPE_ITEM@0..71
                TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..10
                  IDENT@5..10 "Slice"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                RECORD_EXPR@13..41
                  STRUCT_KW@13..19 "struct"
                  GENERIC_PARAM_LIST@19..28
                    COLON2@19..21 "::"
                    L_ANGLE@21..22 "<"
                    REGION_PARAM@22..24
                      REGION_IDENT@22..24 "@a"
                    COMMA@24..25 ","
                    WHITESPACE@25..26 " "
                    TYPE_PARAM@26..27
                      NAME@26..27
                        IDENT@26..27 "T"
                    R_ANGLE@27..28 ">"
                  WHITESPACE@28..29 " "
                  L_BRACE@29..30 "{"
                  WHITESPACE@30..31 " "
                  RECORD_EXPR_FIELD@31..39
                    NAME_REF@31..32
                      IDENT@31..32 "n"
                    COLON@32..33 ":"
                    WHITESPACE@33..34 " "
                    PATH_TYPE@34..39
                      NAME_REF@34..39
                        IDENT@34..39 "usize"
                  WHITESPACE@39..40 " "
                  R_BRACE@40..41 "}"
                WHITESPACE@41..42 " "
                WITH_GROUP@42..70
                  WITH_KW@42..46 "with"
                  WHITESPACE@46..47 " "
                  WITH_CLAUSE@47..53
                    REGION_IDENT@47..49 "@a"
                    COLON@49..50 ":"
                    WHITESPACE@50..51 " "
                    REGION_IDENT@51..53 "@b"
                  WHITESPACE@53..54 " "
                  L_BRACE@54..55 "{"
                  WHITESPACE@55..56 " "
                  IMPL_ELEMENT@56..68
                    IMPL_KW@56..60 "impl"
                    WHITESPACE@60..61 " "
                    PATH_TYPE@61..65
                      NAME_REF@61..65
                        IDENT@61..65 "Self"
                    WHITESPACE@65..66 " "
                    L_BRACE@66..67 "{"
                    R_BRACE@67..68 "}"
                  WHITESPACE@68..69 " "
                  R_BRACE@69..70 "}"
                SEMICOLON@70..71 ";"
            error 47..53: `with @a: ...` outlives groups are not supported yet
        "#]],
    );
}
