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
            error 31..36: can only assign to a variable
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
