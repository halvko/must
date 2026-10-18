//! `with` chains: inherent impls and their reserved forms.

use super::*;

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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@0..4
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
                ITEM_HEAD@88..94
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
                ITEM_HEAD@1..5
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
        len: fn(Self) -> usize;
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..94
              WHITESPACE@0..1 "\n"
              TYPE_ITEM@1..93
                ITEM_HEAD@1..5
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
                WITH_GROUP@30..92
                  WITH_KW@30..34 "with"
                  WHITESPACE@34..35 " "
                  L_BRACE@35..36 "{"
                  WHITESPACE@36..41 "\n    "
                  IMPL_ELEMENT@41..90
                    IMPL_KW@41..45 "impl"
                    WHITESPACE@45..46 " "
                    PATH_TYPE@46..50
                      NAME_REF@46..50
                        IDENT@46..50 "Self"
                    WHITESPACE@50..51 " "
                    L_BRACE@51..52 "{"
                    WHITESPACE@52..61 "\n        "
                    MEMBER@61..84
                      NAME@61..64
                        IDENT@61..64 "len"
                      COLON@64..65 ":"
                      WHITESPACE@65..66 " "
                      FN_TYPE@66..83
                        FN_KW@66..68 "fn"
                        PARAM_LIST@68..74
                          L_PAREN@68..69 "("
                          PARAM@69..73
                            PATH_TYPE@69..73
                              NAME_REF@69..73
                                IDENT@69..73 "Self"
                          R_PAREN@73..74 ")"
                        WHITESPACE@74..75 " "
                        RET_TYPE@75..83
                          THIN_ARROW@75..77 "->"
                          WHITESPACE@77..78 " "
                          PATH_TYPE@78..83
                            NAME_REF@78..83
                              IDENT@78..83 "usize"
                      SEMICOLON@83..84 ";"
                    WHITESPACE@84..89 "\n    "
                    R_BRACE@89..90 "}"
                  WHITESPACE@90..91 "\n"
                  R_BRACE@91..92 "}"
                SEMICOLON@92..93 ";"
              WHITESPACE@93..94 "\n"
            error 61..84: a declare-only inherent member is an unimplementable promise; define it: `name = fn(...) -> ... { ... };`
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..4
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@236..242
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
                ITEM_HEAD@1..5
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
                ITEM_HEAD@95..101
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
