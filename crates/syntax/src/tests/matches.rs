//! `match` expressions, arms and their recovery.

use super::*;

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
                ITEM_HEAD@1..7
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
                          BIND_PAT@73..74
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
                          BIND_PAT@97..98
                            NAME@97..98
                              IDENT@97..98 "a"
                          COMMA@98..99 ","
                          WHITESPACE@99..100 " "
                          BIND_PAT@100..101
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
                ITEM_HEAD@1..7
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
fn dotdot_is_no_match_pattern_and_no_payload_binding() {
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
                ITEM_HEAD@1..7
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
                          BIND_PAT@68..69
                            NAME@68..69
                              IDENT@68..69 "r"
                          COMMA@69..70 ","
                          WHITESPACE@70..71 " "
                          ERROR@71..73
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
                        ERROR@89..91
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
            error 71..73: `..` is only allowed in a record pattern (`struct { x, .. }`)
            error 89..91: `..` is only allowed in a record pattern (`struct { x, .. }`)
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
                ITEM_HEAD@1..7
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
                          BIND_PAT@66..67
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
fn match_arm_semicolon_before_comma_is_one_error() {
    check(
        r#"
static f = fn (s: Shape) -> usize {
    match s {
        ::Point => 0;,
        _ => 1,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..98
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..97
                ITEM_HEAD@1..7
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
                        VARIANT_PAT@59..66
                          COLON2@59..61 "::"
                          NAME_REF@61..66
                            IDENT@61..66 "Point"
                        WHITESPACE@66..67 " "
                        FAT_ARROW@67..69 "=>"
                        WHITESPACE@69..70 " "
                        LITERAL@70..71
                          INT_NUMBER@70..71 "0"
                        ERROR@71..72
                          SEMICOLON@71..72 ";"
                        COMMA@72..73 ","
                      WHITESPACE@73..82 "\n        "
                      MATCH_ARM@82..89
                        WILDCARD_PAT@82..83
                          HOLE@82..83 "_"
                        WHITESPACE@83..84 " "
                        FAT_ARROW@84..86 "=>"
                        WHITESPACE@86..87 " "
                        LITERAL@87..88
                          INT_NUMBER@87..88 "1"
                        COMMA@88..89 ","
                      WHITESPACE@89..94 "\n    "
                      R_BRACE@94..95 "}"
                    WHITESPACE@95..96 "\n"
                    R_BRACE@96..97 "}"
              WHITESPACE@97..98 "\n"
            error 71..72: remove this `;`: a match arm's body is an expression, not a statement
        "#]],
    );
}

/// Each `;` after an arm's body is exactly one error, on the `;`, whose fix
/// leaves a program that parses cleanly.
#[test]
fn match_arm_semicolon_fix_leaves_a_clean_parse() {
    let cases = [
        (
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0;, _ => 1 } }",
            "Remove `;`",
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0, _ => 1 } }",
        ),
        (
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0, _ => 1; } }",
            "Remove `;`",
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0, _ => 1 } }",
        ),
        (
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0; _ => 1 } }",
            "Replace `;` with `,`",
            "static f = fn (s: Shape) -> usize { match s { ::Point => 0, _ => 1 } }",
        ),
        (
            "static f = fn (s: Shape) -> usize { match s { ::Point => { 0 }; _ => 1 } }",
            "Replace `;` with `,`",
            "static f = fn (s: Shape) -> usize { match s { ::Point => { 0 }, _ => 1 } }",
        ),
    ];
    for (text, label, fixed) in cases {
        let parse = crate::parse(text);
        let [err] = parse.errors() else {
            panic!("expected one error for {text:?}: {:?}", parse.errors());
        };
        assert_eq!(&text[err.range], ";", "{text:?}");
        let fix = err.fix.as_ref().expect("expected a fix");
        assert_eq!(fix.label, label);
        assert_eq!(apply_fix(text, fix), fixed);
        assert!(crate::parse(fixed).errors().is_empty(), "{fixed:?}");
    }
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
                ITEM_HEAD@1..7
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
    // `-x` is not a pattern (only an integer literal takes a sign), and
    // the refusal consumes nothing. Reading a body after it would take `1` as
    // the arm's value and judge it against the match's RESULT type — a
    // second error about a type the user never wrote — so the rest of the
    // arm is ERROR and the next arm is where parsing resumes.
    check(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        -x => 1,
        _ => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..92
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..91
                ITEM_HEAD@1..7
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
                          IDENT@60..61 "x"
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
        -x => { 1 },
        _ => 0,
    }
}
"#,
        expect![[r#"
            SOURCE_FILE@0..96
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..95
                ITEM_HEAD@1..7
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
                          IDENT@60..61 "x"
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
        crate::parse("static f = fn (n: usize) -> usize { match n { -x => [1, 2], _ => 0, } }");
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
    -x => 1
type T = usize;
"#,
        expect![[r#"
            SOURCE_FILE@0..75
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..58
                ITEM_HEAD@1..7
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
                            IDENT@52..53 "x"
                            WHITESPACE@53..54 " "
                            FAT_ARROW@54..56 "=>"
                            WHITESPACE@56..57 " "
                            INT_NUMBER@57..58 "1"
              WHITESPACE@58..59 "\n"
              TYPE_ITEM@59..74
                ITEM_HEAD@59..63
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
        "static f = fn (n: usize) -> usize { match n {\n    -x => 1\ntrait T = requires { };\n",
    );
    let msgs: Vec<_> = parse.errors().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs, ["expected a pattern", "expected `}`"], "{msgs:?}");
    assert!(
        parse.debug_dump().contains("TRAIT_ITEM@"),
        "the trait declaration must survive the skip: {}",
        parse.debug_dump()
    );
    // Same for the `extern` marker, which leads its item.
    let parse = crate::parse(
        "static f = fn (n: usize) -> usize { match n {\n    -x => 1\nunsafe extern static g: unsafe fn() -> usize;\n",
    );
    let msgs: Vec<_> = parse.errors().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs, ["expected a pattern", "expected `}`"], "{msgs:?}");
    assert!(
        parse.debug_dump().contains("EXTERN_KW@65..71"),
        "the `extern` declaration must survive the skip: {}",
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
                ITEM_HEAD@1..7
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
                ITEM_HEAD@38..44
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
