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
            error 23..27: expected an item (`static`, `const` or `type`)
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
            error 29..30: expected an item (`static`, `const` or `type`)
            error 30..31: expected an item (`static`, `const` or `type`)
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
            error 23..24: expected an item (`static`, `const` or `type`)
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
        "static p = struct { x: 1, y, };",
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
                RECORD_EXPR@11..30
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
                  COMMA@24..25 ","
                  WHITESPACE@25..26 " "
                  RECORD_EXPR_FIELD@26..27
                    NAME_REF@26..27
                      IDENT@26..27 "y"
                  COMMA@27..28 ","
                  WHITESPACE@28..29 " "
                  R_BRACE@29..30 "}"
                SEMICOLON@30..31 ";"
        "#]],
    );
}

#[test]
fn nested_record_literal() {
    check(
        "static p = struct { outer: struct { x: 1 } };",
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
                  RECORD_EXPR_FIELD@20..42
                    NAME_REF@20..25
                      IDENT@20..25 "outer"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    RECORD_EXPR@27..42
                      STRUCT_KW@27..33 "struct"
                      WHITESPACE@33..34 " "
                      L_BRACE@34..35 "{"
                      WHITESPACE@35..36 " "
                      RECORD_EXPR_FIELD@36..40
                        NAME_REF@36..37
                          IDENT@36..37 "x"
                        COLON@37..38 ":"
                        WHITESPACE@38..39 " "
                        LITERAL@39..40
                          INT_NUMBER@39..40 "1"
                      WHITESPACE@40..41 " "
                      R_BRACE@41..42 "}"
                  WHITESPACE@42..43 " "
                  R_BRACE@43..44 "}"
                SEMICOLON@44..45 ";"
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
        "static p = f(struct { x: 1 });",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "p"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..29
                  PATH_EXPR@11..12
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                  ARG_LIST@12..29
                    L_PAREN@12..13 "("
                    RECORD_EXPR@13..28
                      STRUCT_KW@13..19 "struct"
                      WHITESPACE@19..20 " "
                      L_BRACE@20..21 "{"
                      WHITESPACE@21..22 " "
                      RECORD_EXPR_FIELD@22..26
                        NAME_REF@22..23
                          IDENT@22..23 "x"
                        COLON@23..24 ":"
                        WHITESPACE@24..25 " "
                        LITERAL@25..26
                          INT_NUMBER@25..26 "1"
                      WHITESPACE@26..27 " "
                      R_BRACE@27..28 "}"
                    R_PAREN@28..29 ")"
                SEMICOLON@29..30 ";"
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
        "static p = struct { x: 1, x: 2 };",
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
                  RECORD_EXPR_FIELD@20..24
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    LITERAL@23..24
                      INT_NUMBER@23..24 "1"
                  COMMA@24..25 ","
                  WHITESPACE@25..26 " "
                  RECORD_EXPR_FIELD@26..30
                    NAME_REF@26..27
                      IDENT@26..27 "x"
                    COLON@27..28 ":"
                    WHITESPACE@28..29 " "
                    LITERAL@29..30
                      INT_NUMBER@29..30 "2"
                  WHITESPACE@30..31 " "
                  R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
            error 26..27: duplicate field `x`
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
        "static p = struct { x: 1, ... };",
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
                  RECORD_EXPR_FIELD@20..24
                    NAME_REF@20..21
                      IDENT@20..21 "x"
                    COLON@21..22 ":"
                    WHITESPACE@22..23 " "
                    LITERAL@23..24
                      INT_NUMBER@23..24 "1"
                  COMMA@24..25 ","
                  WHITESPACE@25..26 " "
                  DOT3@26..29 "..."
                  WHITESPACE@29..30 " "
                  R_BRACE@30..31 "}"
                SEMICOLON@31..32 ";"
            error 26..29: open record types are not supported yet
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
            error 25..26: expected an item (`static`, `const` or `type`)
            error 26..27: expected an item (`static`, `const` or `type`)
            error 27..28: expected an item (`static`, `const` or `type`)
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
        "static f = fn { struct { x: 1 }; };",
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
                    EXPR_STMT@16..32
                      RECORD_EXPR@16..31
                        STRUCT_KW@16..22 "struct"
                        WHITESPACE@22..23 " "
                        L_BRACE@23..24 "{"
                        WHITESPACE@24..25 " "
                        RECORD_EXPR_FIELD@25..29
                          NAME_REF@25..26
                            IDENT@25..26 "x"
                          COLON@26..27 ":"
                          WHITESPACE@27..28 " "
                          LITERAL@28..29
                            INT_NUMBER@28..29 "1"
                        WHITESPACE@29..30 " "
                        R_BRACE@30..31 "}"
                      SEMICOLON@31..32 ";"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
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
        "static c = if p == struct { x: 1 } {};",
        expect![[r#"
            SOURCE_FILE@0..38
              STATIC_ITEM@0..38
                STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "c"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                IF_EXPR@11..37
                  IF_KW@11..13 "if"
                  WHITESPACE@13..14 " "
                  BIN_EXPR@14..34
                    PATH_EXPR@14..15
                      NAME_REF@14..15
                        IDENT@14..15 "p"
                    WHITESPACE@15..16 " "
                    EQ2@16..18 "=="
                    WHITESPACE@18..19 " "
                    RECORD_EXPR@19..34
                      STRUCT_KW@19..25 "struct"
                      WHITESPACE@25..26 " "
                      L_BRACE@26..27 "{"
                      WHITESPACE@27..28 " "
                      RECORD_EXPR_FIELD@28..32
                        NAME_REF@28..29
                          IDENT@28..29 "x"
                        COLON@29..30 ":"
                        WHITESPACE@30..31 " "
                        LITERAL@31..32
                          INT_NUMBER@31..32 "1"
                      WHITESPACE@32..33 " "
                      R_BRACE@33..34 "}"
                  WHITESPACE@34..35 " "
                  BLOCK_EXPR@35..37
                    L_BRACE@35..36 "{"
                    R_BRACE@36..37 "}"
                SEMICOLON@37..38 ";"
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
                    PATH_EXPR@23..28
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
                    PATH_EXPR@39..44
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
                    PATH_EXPR@30..35
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
                    PATH_EXPR@29..34
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
        "static f = struct { pub x: 1 };",
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
                RECORD_EXPR@11..30
                  STRUCT_KW@11..17 "struct"
                  WHITESPACE@17..18 " "
                  L_BRACE@18..19 "{"
                  WHITESPACE@19..20 " "
                  RECORD_EXPR_FIELD@20..28
                    PUB_KW@20..23 "pub"
                    WHITESPACE@23..24 " "
                    NAME_REF@24..25
                      IDENT@24..25 "x"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    LITERAL@27..28
                      INT_NUMBER@27..28 "1"
                  WHITESPACE@28..29 " "
                  R_BRACE@29..30 "}"
                SEMICOLON@30..31 ";"
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
