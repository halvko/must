//! Trait declarations, requirements, bounds and impls.

use super::*;

// ---- trait declarations: bounds, impl homes -----------------------------

#[test]
fn trait_decl_requires_parses() {
    check(
        r#"
trait Write = requires {
    push: fn(str, Self) -> Self;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..62
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..61
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..12
                  IDENT@7..12 "Write"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                REQUIRES_DEF@15..60
                  REQUIRES_KW@15..23 "requires"
                  WHITESPACE@23..24 " "
                  L_BRACE@24..25 "{"
                  WHITESPACE@25..30 "\n    "
                  MEMBER@30..58
                    NAME@30..34
                      IDENT@30..34 "push"
                    COLON@34..35 ":"
                    WHITESPACE@35..36 " "
                    FN_TYPE@36..57
                      FN_KW@36..38 "fn"
                      PARAM_LIST@38..49
                        L_PAREN@38..39 "("
                        PARAM@39..42
                          PATH_TYPE@39..42
                            NAME_REF@39..42
                              IDENT@39..42 "str"
                        COMMA@42..43 ","
                        WHITESPACE@43..44 " "
                        PARAM@44..48
                          PATH_TYPE@44..48
                            NAME_REF@44..48
                              IDENT@44..48 "Self"
                        R_PAREN@48..49 ")"
                      WHITESPACE@49..50 " "
                      RET_TYPE@50..57
                        THIN_ARROW@50..52 "->"
                        WHITESPACE@52..53 " "
                        PATH_TYPE@53..57
                          NAME_REF@53..57
                            IDENT@53..57 "Self"
                    SEMICOLON@57..58 ";"
                  WHITESPACE@58..59 "\n"
                  R_BRACE@59..60 "}"
                SEMICOLON@60..61 ";"
              WHITESPACE@61..62 "\n"
        "#]],
    );
}

#[test]
fn trait_decl_generic_requirement_with_bound_parses() {
    check(
        r#"
trait Display = requires {
    fmt: fn::<W: Write>(W, Self) -> W;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..70
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..69
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..14
                  IDENT@7..14 "Display"
                WHITESPACE@14..15 " "
                EQ@15..16 "="
                WHITESPACE@16..17 " "
                REQUIRES_DEF@17..68
                  REQUIRES_KW@17..25 "requires"
                  WHITESPACE@25..26 " "
                  L_BRACE@26..27 "{"
                  WHITESPACE@27..32 "\n    "
                  MEMBER@32..66
                    NAME@32..35
                      IDENT@32..35 "fmt"
                    COLON@35..36 ":"
                    WHITESPACE@36..37 " "
                    FN_TYPE@37..65
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
                      PARAM_LIST@51..60
                        L_PAREN@51..52 "("
                        PARAM@52..53
                          PATH_TYPE@52..53
                            NAME_REF@52..53
                              IDENT@52..53 "W"
                        COMMA@53..54 ","
                        WHITESPACE@54..55 " "
                        PARAM@55..59
                          PATH_TYPE@55..59
                            NAME_REF@55..59
                              IDENT@55..59 "Self"
                        R_PAREN@59..60 ")"
                      WHITESPACE@60..61 " "
                      RET_TYPE@61..65
                        THIN_ARROW@61..63 "->"
                        WHITESPACE@63..64 " "
                        PATH_TYPE@64..65
                          NAME_REF@64..65
                            IDENT@64..65 "W"
                    SEMICOLON@65..66 ";"
                  WHITESPACE@66..67 "\n"
                  R_BRACE@67..68 "}"
                SEMICOLON@68..69 ";"
              WHITESPACE@69..70 "\n"
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
                ITEM_HEAD@0..5
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
trait Alloc = requires::<T> { alloc: fn(usize, Self) -> usize; };
trait TrustedLen = unsafe requires Self: Iterator { };
"#,
        expect![[r#"
            SOURCE_FILE@0..122
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..66
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..12
                  IDENT@7..12 "Alloc"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                REQUIRES_DEF@15..65
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
                  MEMBER@31..63
                    NAME@31..36
                      IDENT@31..36 "alloc"
                    COLON@36..37 ":"
                    WHITESPACE@37..38 " "
                    FN_TYPE@38..62
                      FN_KW@38..40 "fn"
                      PARAM_LIST@40..53
                        L_PAREN@40..41 "("
                        PARAM@41..46
                          PATH_TYPE@41..46
                            NAME_REF@41..46
                              IDENT@41..46 "usize"
                        COMMA@46..47 ","
                        WHITESPACE@47..48 " "
                        PARAM@48..52
                          PATH_TYPE@48..52
                            NAME_REF@48..52
                              IDENT@48..52 "Self"
                        R_PAREN@52..53 ")"
                      WHITESPACE@53..54 " "
                      RET_TYPE@54..62
                        THIN_ARROW@54..56 "->"
                        WHITESPACE@56..57 " "
                        PATH_TYPE@57..62
                          NAME_REF@57..62
                            IDENT@57..62 "usize"
                    SEMICOLON@62..63 ";"
                  WHITESPACE@63..64 " "
                  R_BRACE@64..65 "}"
                SEMICOLON@65..66 ";"
              WHITESPACE@66..67 "\n"
              TRAIT_ITEM@67..121
                ITEM_HEAD@67..72
                  TRAIT_KW@67..72 "trait"
                WHITESPACE@72..73 " "
                NAME@73..83
                  IDENT@73..83 "TrustedLen"
                WHITESPACE@83..84 " "
                EQ@84..85 "="
                WHITESPACE@85..86 " "
                REQUIRES_DEF@86..120
                  UNSAFE_KW@86..92 "unsafe"
                  WHITESPACE@92..93 " "
                  REQUIRES_KW@93..101 "requires"
                  WHITESPACE@101..102 " "
                  REQUIRES_CLAUSE@102..116
                    NAME_REF@102..106
                      IDENT@102..106 "Self"
                    COLON@106..107 ":"
                    WHITESPACE@107..108 " "
                    PATH_TYPE@108..116
                      NAME_REF@108..116
                        IDENT@108..116 "Iterator"
                  WHITESPACE@116..117 " "
                  L_BRACE@117..118 "{"
                  WHITESPACE@118..119 " "
                  R_BRACE@119..120 "}"
                SEMICOLON@120..121 ";"
              WHITESPACE@121..122 "\n"
            error 23..28: generic traits are not supported yet
            error 86..92: `unsafe` traits are not supported yet
            error 102..116: supertrait clauses are not supported yet
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
    f: unsafe fn(Self) -> usize;
    g;
};
"#,
        expect![[r#"
            SOURCE_FILE@0..137
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..136
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "T"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..135
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
                  MEMBER@98..126
                    NAME@98..99
                      IDENT@98..99 "f"
                    COLON@99..100 ":"
                    WHITESPACE@100..101 " "
                    FN_TYPE@101..125
                      UNSAFE_KW@101..107 "unsafe"
                      WHITESPACE@107..108 " "
                      FN_KW@108..110 "fn"
                      PARAM_LIST@110..116
                        L_PAREN@110..111 "("
                        PARAM@111..115
                          PATH_TYPE@111..115
                            NAME_REF@111..115
                              IDENT@111..115 "Self"
                        R_PAREN@115..116 ")"
                      WHITESPACE@116..117 " "
                      RET_TYPE@117..125
                        THIN_ARROW@117..119 "->"
                        WHITESPACE@119..120 " "
                        PATH_TYPE@120..125
                          NAME_REF@120..125
                            IDENT@120..125 "usize"
                    SEMICOLON@125..126 ";"
                  WHITESPACE@126..131 "\n    "
                  MEMBER@131..133
                    NAME@131..132
                      IDENT@131..132 "g"
                    SEMICOLON@132..133 ";"
                  WHITESPACE@133..134 "\n"
                  R_BRACE@134..135 "}"
                SEMICOLON@135..136 ";"
              WHITESPACE@136..137 "\n"
            error 28..57: default members are not supported yet; a trait declares requirements (`name: fn(...) -> ...;`)
            error 63..67: associated types are not supported yet
            error 78..83: associated consts are not supported yet
            error 101..107: `unsafe` trait members are not supported yet
            error 131..133: a requirement declares its signature: `name: fn(...) -> ...;`
        "#]],
    );
}

#[test]
fn trait_requirement_param_is_not_mut() {
    // A requirement declares a signature; `mut` is a binding mode, which
    // only a body has. Its parameters are a fn TYPE's, so the fn type's
    // own refusal says so.
    check(
        "trait T = requires { m: fn(mut n: usize) -> usize; };",
        expect![[r#"
            SOURCE_FILE@0..53
              TRAIT_ITEM@0..53
                ITEM_HEAD@0..5
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
            error 27..30: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
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
                ITEM_HEAD@0..5
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
                          RECORD_TYPE@27..39
                            STRUCT_KW@27..33 "struct"
                            WHITESPACE@33..34 " "
                            L_BRACE@34..35 "{"
                            WHITESPACE@35..36 " "
                            RECORD_TYPE_FIELD@36..37
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
            error 38..39: expected `:` followed by the field's type
            error 39..40: a function type's parameters are bare types, not patterns: write `fn(usize, str)`
        "#]],
    );
}

#[test]
fn trait_requirement_params_are_bare_types() {
    // A requirement's bare parameter types and its binder, regions
    // included, parse clean.
    check_errors(
        r#"
trait Write = requires { push: fn(str, Self) -> Self; };
trait Display = requires {
    fmt: fn::<@a, @b, W: Write>(W.&mut::<@a>, Self.&::<@b>) -> ();
};
"#,
        expect![[r#""#]],
    );
    check(
        "trait T = requires { m: fn(usize) -> usize; };",
        expect![[r#"
            SOURCE_FILE@0..46
              TRAIT_ITEM@0..46
                ITEM_HEAD@0..5
                  TRAIT_KW@0..5 "trait"
                WHITESPACE@5..6 " "
                NAME@6..7
                  IDENT@6..7 "T"
                WHITESPACE@7..8 " "
                EQ@8..9 "="
                WHITESPACE@9..10 " "
                REQUIRES_DEF@10..45
                  REQUIRES_KW@10..18 "requires"
                  WHITESPACE@18..19 " "
                  L_BRACE@19..20 "{"
                  WHITESPACE@20..21 " "
                  MEMBER@21..43
                    NAME@21..22
                      IDENT@21..22 "m"
                    COLON@22..23 ":"
                    WHITESPACE@23..24 " "
                    FN_TYPE@24..42
                      FN_KW@24..26 "fn"
                      PARAM_LIST@26..33
                        L_PAREN@26..27 "("
                        PARAM@27..32
                          PATH_TYPE@27..32
                            NAME_REF@27..32
                              IDENT@27..32 "usize"
                        R_PAREN@32..33 ")"
                      WHITESPACE@33..34 " "
                      RET_TYPE@34..42
                        THIN_ARROW@34..36 "->"
                        WHITESPACE@36..37 " "
                        PATH_TYPE@37..42
                          NAME_REF@37..42
                            IDENT@37..42 "usize"
                    SEMICOLON@42..43 ";"
                  WHITESPACE@43..44 " "
                  R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn an_unsafe_fn_type_nested_in_a_requirement_is_live() {
    // The reservation is about a requirement's OWN top-level signature —
    // an unsafe-to-call MEMBER is still reserved — and not about the
    // `unsafe fn` type, which is live in every other type position. A
    // requirement may therefore take one as a parameter.
    check_errors(
        "trait T = requires { go: fn(unsafe fn()) -> (); };",
        expect![[r#""#]],
    );
    check_errors(
        "trait T = requires { go: unsafe fn(); };",
        expect![[r#"
            25..31: `unsafe` trait members are not supported yet
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..4
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
                ITEM_HEAD@0..6
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
                        PARAM_LIST@40..42
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
trait Show = requires { show: fn(Self) -> str; };
type P = struct { a: usize } with {
    impl Show {
        show = fn(x: Self) -> str { "p" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..155
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..50
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..49
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..47
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..46
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..39
                        L_PAREN@33..34 "("
                        PARAM@34..38
                          PATH_TYPE@34..38
                            NAME_REF@34..38
                              IDENT@34..38 "Self"
                        R_PAREN@38..39 ")"
                      WHITESPACE@39..40 " "
                      RET_TYPE@40..46
                        THIN_ARROW@40..42 "->"
                        WHITESPACE@42..43 " "
                        PATH_TYPE@43..46
                          NAME_REF@43..46
                            IDENT@43..46 "str"
                    SEMICOLON@46..47 ";"
                  WHITESPACE@47..48 " "
                  R_BRACE@48..49 "}"
                SEMICOLON@49..50 ";"
              WHITESPACE@50..51 "\n"
              TYPE_ITEM@51..154
                ITEM_HEAD@51..55
                  TYPE_KW@51..55 "type"
                WHITESPACE@55..56 " "
                NAME@56..57
                  IDENT@56..57 "P"
                WHITESPACE@57..58 " "
                EQ@58..59 "="
                WHITESPACE@59..60 " "
                RECORD_EXPR@60..79
                  STRUCT_KW@60..66 "struct"
                  WHITESPACE@66..67 " "
                  L_BRACE@67..68 "{"
                  WHITESPACE@68..69 " "
                  RECORD_EXPR_FIELD@69..77
                    NAME_REF@69..70
                      IDENT@69..70 "a"
                    COLON@70..71 ":"
                    WHITESPACE@71..72 " "
                    PATH_TYPE@72..77
                      NAME_REF@72..77
                        IDENT@72..77 "usize"
                  WHITESPACE@77..78 " "
                  R_BRACE@78..79 "}"
                WHITESPACE@79..80 " "
                WITH_GROUP@80..153
                  WITH_KW@80..84 "with"
                  WHITESPACE@84..85 " "
                  L_BRACE@85..86 "{"
                  WHITESPACE@86..91 "\n    "
                  IMPL_ELEMENT@91..151
                    IMPL_KW@91..95 "impl"
                    WHITESPACE@95..96 " "
                    PATH_TYPE@96..100
                      NAME_REF@96..100
                        IDENT@96..100 "Show"
                    WHITESPACE@100..101 " "
                    L_BRACE@101..102 "{"
                    WHITESPACE@102..111 "\n        "
                    MEMBER@111..145
                      NAME@111..115
                        IDENT@111..115 "show"
                      WHITESPACE@115..116 " "
                      EQ@116..117 "="
                      WHITESPACE@117..118 " "
                      FN_LITERAL@118..144
                        FN_KW@118..120 "fn"
                        PARAM_LIST@120..129
                          L_PAREN@120..121 "("
                          PARAM@121..128
                            BIND_PAT@121..122
                              NAME@121..122
                                IDENT@121..122 "x"
                            COLON@122..123 ":"
                            WHITESPACE@123..124 " "
                            PATH_TYPE@124..128
                              NAME_REF@124..128
                                IDENT@124..128 "Self"
                          R_PAREN@128..129 ")"
                        WHITESPACE@129..130 " "
                        RET_TYPE@130..136
                          THIN_ARROW@130..132 "->"
                          WHITESPACE@132..133 " "
                          PATH_TYPE@133..136
                            NAME_REF@133..136
                              IDENT@133..136 "str"
                        WHITESPACE@136..137 " "
                        BLOCK_EXPR@137..144
                          L_BRACE@137..138 "{"
                          WHITESPACE@138..139 " "
                          LITERAL@139..142
                            STRING@139..142 "\"p\""
                          WHITESPACE@142..143 " "
                          R_BRACE@143..144 "}"
                      SEMICOLON@144..145 ";"
                    WHITESPACE@145..150 "\n    "
                    R_BRACE@150..151 "}"
                  WHITESPACE@151..152 "\n"
                  R_BRACE@152..153 "}"
                SEMICOLON@153..154 ";"
              WHITESPACE@154..155 "\n"
        "#]],
    );
}

#[test]
fn trait_side_impl_is_live() {
    check(
        r#"
trait Show = requires { show: fn(Self) -> str; } with {
    impl usize {
        show = fn(x: usize) -> str { "n" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..127
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..126
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..49
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..47
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..46
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..39
                        L_PAREN@33..34 "("
                        PARAM@34..38
                          PATH_TYPE@34..38
                            NAME_REF@34..38
                              IDENT@34..38 "Self"
                        R_PAREN@38..39 ")"
                      WHITESPACE@39..40 " "
                      RET_TYPE@40..46
                        THIN_ARROW@40..42 "->"
                        WHITESPACE@42..43 " "
                        PATH_TYPE@43..46
                          NAME_REF@43..46
                            IDENT@43..46 "str"
                    SEMICOLON@46..47 ";"
                  WHITESPACE@47..48 " "
                  R_BRACE@48..49 "}"
                WHITESPACE@49..50 " "
                WITH_GROUP@50..125
                  WITH_KW@50..54 "with"
                  WHITESPACE@54..55 " "
                  L_BRACE@55..56 "{"
                  WHITESPACE@56..61 "\n    "
                  IMPL_ELEMENT@61..123
                    IMPL_KW@61..65 "impl"
                    WHITESPACE@65..66 " "
                    PATH_TYPE@66..71
                      NAME_REF@66..71
                        IDENT@66..71 "usize"
                    WHITESPACE@71..72 " "
                    L_BRACE@72..73 "{"
                    WHITESPACE@73..82 "\n        "
                    MEMBER@82..117
                      NAME@82..86
                        IDENT@82..86 "show"
                      WHITESPACE@86..87 " "
                      EQ@87..88 "="
                      WHITESPACE@88..89 " "
                      FN_LITERAL@89..116
                        FN_KW@89..91 "fn"
                        PARAM_LIST@91..101
                          L_PAREN@91..92 "("
                          PARAM@92..100
                            BIND_PAT@92..93
                              NAME@92..93
                                IDENT@92..93 "x"
                            COLON@93..94 ":"
                            WHITESPACE@94..95 " "
                            PATH_TYPE@95..100
                              NAME_REF@95..100
                                IDENT@95..100 "usize"
                          R_PAREN@100..101 ")"
                        WHITESPACE@101..102 " "
                        RET_TYPE@102..108
                          THIN_ARROW@102..104 "->"
                          WHITESPACE@104..105 " "
                          PATH_TYPE@105..108
                            NAME_REF@105..108
                              IDENT@105..108 "str"
                        WHITESPACE@108..109 " "
                        BLOCK_EXPR@109..116
                          L_BRACE@109..110 "{"
                          WHITESPACE@110..111 " "
                          LITERAL@111..114
                            STRING@111..114 "\"n\""
                          WHITESPACE@114..115 " "
                          R_BRACE@115..116 "}"
                      SEMICOLON@116..117 ";"
                    WHITESPACE@117..122 "\n    "
                    R_BRACE@122..123 "}"
                  WHITESPACE@123..124 "\n"
                  R_BRACE@124..125 "}"
                SEMICOLON@125..126 ";"
              WHITESPACE@126..127 "\n"
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
                ITEM_HEAD@1..6
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
                ITEM_HEAD@68..72
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
trait Show = requires { show: fn(Self) -> str; };
type V = struct::<T> { a: T } with {
    impl Show {
        show = fn(x: Self) -> str { "v" };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..156
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..50
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..11
                  IDENT@7..11 "Show"
                WHITESPACE@11..12 " "
                EQ@12..13 "="
                WHITESPACE@13..14 " "
                REQUIRES_DEF@14..49
                  REQUIRES_KW@14..22 "requires"
                  WHITESPACE@22..23 " "
                  L_BRACE@23..24 "{"
                  WHITESPACE@24..25 " "
                  MEMBER@25..47
                    NAME@25..29
                      IDENT@25..29 "show"
                    COLON@29..30 ":"
                    WHITESPACE@30..31 " "
                    FN_TYPE@31..46
                      FN_KW@31..33 "fn"
                      PARAM_LIST@33..39
                        L_PAREN@33..34 "("
                        PARAM@34..38
                          PATH_TYPE@34..38
                            NAME_REF@34..38
                              IDENT@34..38 "Self"
                        R_PAREN@38..39 ")"
                      WHITESPACE@39..40 " "
                      RET_TYPE@40..46
                        THIN_ARROW@40..42 "->"
                        WHITESPACE@42..43 " "
                        PATH_TYPE@43..46
                          NAME_REF@43..46
                            IDENT@43..46 "str"
                    SEMICOLON@46..47 ";"
                  WHITESPACE@47..48 " "
                  R_BRACE@48..49 "}"
                SEMICOLON@49..50 ";"
              WHITESPACE@50..51 "\n"
              TYPE_ITEM@51..155
                ITEM_HEAD@51..55
                  TYPE_KW@51..55 "type"
                WHITESPACE@55..56 " "
                NAME@56..57
                  IDENT@56..57 "V"
                WHITESPACE@57..58 " "
                EQ@58..59 "="
                WHITESPACE@59..60 " "
                RECORD_EXPR@60..80
                  STRUCT_KW@60..66 "struct"
                  GENERIC_PARAM_LIST@66..71
                    COLON2@66..68 "::"
                    L_ANGLE@68..69 "<"
                    TYPE_PARAM@69..70
                      NAME@69..70
                        IDENT@69..70 "T"
                    R_ANGLE@70..71 ">"
                  WHITESPACE@71..72 " "
                  L_BRACE@72..73 "{"
                  WHITESPACE@73..74 " "
                  RECORD_EXPR_FIELD@74..78
                    NAME_REF@74..75
                      IDENT@74..75 "a"
                    COLON@75..76 ":"
                    WHITESPACE@76..77 " "
                    PATH_TYPE@77..78
                      NAME_REF@77..78
                        IDENT@77..78 "T"
                  WHITESPACE@78..79 " "
                  R_BRACE@79..80 "}"
                WHITESPACE@80..81 " "
                WITH_GROUP@81..154
                  WITH_KW@81..85 "with"
                  WHITESPACE@85..86 " "
                  L_BRACE@86..87 "{"
                  WHITESPACE@87..92 "\n    "
                  IMPL_ELEMENT@92..152
                    IMPL_KW@92..96 "impl"
                    WHITESPACE@96..97 " "
                    PATH_TYPE@97..101
                      NAME_REF@97..101
                        IDENT@97..101 "Show"
                    WHITESPACE@101..102 " "
                    L_BRACE@102..103 "{"
                    WHITESPACE@103..112 "\n        "
                    MEMBER@112..146
                      NAME@112..116
                        IDENT@112..116 "show"
                      WHITESPACE@116..117 " "
                      EQ@117..118 "="
                      WHITESPACE@118..119 " "
                      FN_LITERAL@119..145
                        FN_KW@119..121 "fn"
                        PARAM_LIST@121..130
                          L_PAREN@121..122 "("
                          PARAM@122..129
                            BIND_PAT@122..123
                              NAME@122..123
                                IDENT@122..123 "x"
                            COLON@123..124 ":"
                            WHITESPACE@124..125 " "
                            PATH_TYPE@125..129
                              NAME_REF@125..129
                                IDENT@125..129 "Self"
                          R_PAREN@129..130 ")"
                        WHITESPACE@130..131 " "
                        RET_TYPE@131..137
                          THIN_ARROW@131..133 "->"
                          WHITESPACE@133..134 " "
                          PATH_TYPE@134..137
                            NAME_REF@134..137
                              IDENT@134..137 "str"
                        WHITESPACE@137..138 " "
                        BLOCK_EXPR@138..145
                          L_BRACE@138..139 "{"
                          WHITESPACE@139..140 " "
                          LITERAL@140..143
                            STRING@140..143 "\"v\""
                          WHITESPACE@143..144 " "
                          R_BRACE@144..145 "}"
                      SEMICOLON@145..146 ";"
                    WHITESPACE@146..151 "\n    "
                    R_BRACE@151..152 "}"
                  WHITESPACE@152..153 "\n"
                  R_BRACE@153..154 "}"
                SEMICOLON@154..155 ";"
              WHITESPACE@155..156 "\n"
            error 97..101: trait impls on generic types are not supported yet
        "#]],
    );
}

#[test]
fn generic_binder_allowed_on_trait_impl_member() {
    check(
        r#"
trait D = requires { fmt: fn::<W>(W, Self) -> W; };
type P = struct { a: usize } with {
    impl D {
        fmt = fn::<W>(w: W, x: Self) -> W { w };
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..160
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..52
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "D"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..51
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..22 " "
                  MEMBER@22..49
                    NAME@22..25
                      IDENT@22..25 "fmt"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    FN_TYPE@27..48
                      FN_KW@27..29 "fn"
                      GENERIC_PARAM_LIST@29..34
                        COLON2@29..31 "::"
                        L_ANGLE@31..32 "<"
                        TYPE_PARAM@32..33
                          NAME@32..33
                            IDENT@32..33 "W"
                        R_ANGLE@33..34 ">"
                      PARAM_LIST@34..43
                        L_PAREN@34..35 "("
                        PARAM@35..36
                          PATH_TYPE@35..36
                            NAME_REF@35..36
                              IDENT@35..36 "W"
                        COMMA@36..37 ","
                        WHITESPACE@37..38 " "
                        PARAM@38..42
                          PATH_TYPE@38..42
                            NAME_REF@38..42
                              IDENT@38..42 "Self"
                        R_PAREN@42..43 ")"
                      WHITESPACE@43..44 " "
                      RET_TYPE@44..48
                        THIN_ARROW@44..46 "->"
                        WHITESPACE@46..47 " "
                        PATH_TYPE@47..48
                          NAME_REF@47..48
                            IDENT@47..48 "W"
                    SEMICOLON@48..49 ";"
                  WHITESPACE@49..50 " "
                  R_BRACE@50..51 "}"
                SEMICOLON@51..52 ";"
              WHITESPACE@52..53 "\n"
              TYPE_ITEM@53..159
                ITEM_HEAD@53..57
                  TYPE_KW@53..57 "type"
                WHITESPACE@57..58 " "
                NAME@58..59
                  IDENT@58..59 "P"
                WHITESPACE@59..60 " "
                EQ@60..61 "="
                WHITESPACE@61..62 " "
                RECORD_EXPR@62..81
                  STRUCT_KW@62..68 "struct"
                  WHITESPACE@68..69 " "
                  L_BRACE@69..70 "{"
                  WHITESPACE@70..71 " "
                  RECORD_EXPR_FIELD@71..79
                    NAME_REF@71..72
                      IDENT@71..72 "a"
                    COLON@72..73 ":"
                    WHITESPACE@73..74 " "
                    PATH_TYPE@74..79
                      NAME_REF@74..79
                        IDENT@74..79 "usize"
                  WHITESPACE@79..80 " "
                  R_BRACE@80..81 "}"
                WHITESPACE@81..82 " "
                WITH_GROUP@82..158
                  WITH_KW@82..86 "with"
                  WHITESPACE@86..87 " "
                  L_BRACE@87..88 "{"
                  WHITESPACE@88..93 "\n    "
                  IMPL_ELEMENT@93..156
                    IMPL_KW@93..97 "impl"
                    WHITESPACE@97..98 " "
                    PATH_TYPE@98..99
                      NAME_REF@98..99
                        IDENT@98..99 "D"
                    WHITESPACE@99..100 " "
                    L_BRACE@100..101 "{"
                    WHITESPACE@101..110 "\n        "
                    MEMBER@110..150
                      NAME@110..113
                        IDENT@110..113 "fmt"
                      WHITESPACE@113..114 " "
                      EQ@114..115 "="
                      WHITESPACE@115..116 " "
                      FN_LITERAL@116..149
                        FN_KW@116..118 "fn"
                        GENERIC_PARAM_LIST@118..123
                          COLON2@118..120 "::"
                          L_ANGLE@120..121 "<"
                          TYPE_PARAM@121..122
                            NAME@121..122
                              IDENT@121..122 "W"
                          R_ANGLE@122..123 ">"
                        PARAM_LIST@123..138
                          L_PAREN@123..124 "("
                          PARAM@124..128
                            BIND_PAT@124..125
                              NAME@124..125
                                IDENT@124..125 "w"
                            COLON@125..126 ":"
                            WHITESPACE@126..127 " "
                            PATH_TYPE@127..128
                              NAME_REF@127..128
                                IDENT@127..128 "W"
                          COMMA@128..129 ","
                          WHITESPACE@129..130 " "
                          PARAM@130..137
                            BIND_PAT@130..131
                              NAME@130..131
                                IDENT@130..131 "x"
                            COLON@131..132 ":"
                            WHITESPACE@132..133 " "
                            PATH_TYPE@133..137
                              NAME_REF@133..137
                                IDENT@133..137 "Self"
                          R_PAREN@137..138 ")"
                        WHITESPACE@138..139 " "
                        RET_TYPE@139..143
                          THIN_ARROW@139..141 "->"
                          WHITESPACE@141..142 " "
                          PATH_TYPE@142..143
                            NAME_REF@142..143
                              IDENT@142..143 "W"
                        WHITESPACE@143..144 " "
                        BLOCK_EXPR@144..149
                          L_BRACE@144..145 "{"
                          WHITESPACE@145..146 " "
                          PATH_EXPR@146..147
                            NAME_REF@146..147
                              IDENT@146..147 "w"
                          WHITESPACE@147..148 " "
                          R_BRACE@148..149 "}"
                      SEMICOLON@149..150 ";"
                    WHITESPACE@150..155 "\n    "
                    R_BRACE@155..156 "}"
                  WHITESPACE@156..157 "\n"
                  R_BRACE@157..158 "}"
                SEMICOLON@158..159 ";"
              WHITESPACE@159..160 "\n"
        "#]],
    );
}

#[test]
fn colon_declared_member_in_impl_rejected() {
    check(
        r#"
trait D = requires { fmt: fn(Self) -> str; };
type P = struct { a: usize } with {
    impl D {
        fmt: fn(Self) -> str;
    }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..135
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..46
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "D"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                REQUIRES_DEF@11..45
                  REQUIRES_KW@11..19 "requires"
                  WHITESPACE@19..20 " "
                  L_BRACE@20..21 "{"
                  WHITESPACE@21..22 " "
                  MEMBER@22..43
                    NAME@22..25
                      IDENT@22..25 "fmt"
                    COLON@25..26 ":"
                    WHITESPACE@26..27 " "
                    FN_TYPE@27..42
                      FN_KW@27..29 "fn"
                      PARAM_LIST@29..35
                        L_PAREN@29..30 "("
                        PARAM@30..34
                          PATH_TYPE@30..34
                            NAME_REF@30..34
                              IDENT@30..34 "Self"
                        R_PAREN@34..35 ")"
                      WHITESPACE@35..36 " "
                      RET_TYPE@36..42
                        THIN_ARROW@36..38 "->"
                        WHITESPACE@38..39 " "
                        PATH_TYPE@39..42
                          NAME_REF@39..42
                            IDENT@39..42 "str"
                    SEMICOLON@42..43 ";"
                  WHITESPACE@43..44 " "
                  R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
              WHITESPACE@46..47 "\n"
              TYPE_ITEM@47..134
                ITEM_HEAD@47..51
                  TYPE_KW@47..51 "type"
                WHITESPACE@51..52 " "
                NAME@52..53
                  IDENT@52..53 "P"
                WHITESPACE@53..54 " "
                EQ@54..55 "="
                WHITESPACE@55..56 " "
                RECORD_EXPR@56..75
                  STRUCT_KW@56..62 "struct"
                  WHITESPACE@62..63 " "
                  L_BRACE@63..64 "{"
                  WHITESPACE@64..65 " "
                  RECORD_EXPR_FIELD@65..73
                    NAME_REF@65..66
                      IDENT@65..66 "a"
                    COLON@66..67 ":"
                    WHITESPACE@67..68 " "
                    PATH_TYPE@68..73
                      NAME_REF@68..73
                        IDENT@68..73 "usize"
                  WHITESPACE@73..74 " "
                  R_BRACE@74..75 "}"
                WHITESPACE@75..76 " "
                WITH_GROUP@76..133
                  WITH_KW@76..80 "with"
                  WHITESPACE@80..81 " "
                  L_BRACE@81..82 "{"
                  WHITESPACE@82..87 "\n    "
                  IMPL_ELEMENT@87..131
                    IMPL_KW@87..91 "impl"
                    WHITESPACE@91..92 " "
                    PATH_TYPE@92..93
                      NAME_REF@92..93
                        IDENT@92..93 "D"
                    WHITESPACE@93..94 " "
                    L_BRACE@94..95 "{"
                    WHITESPACE@95..104 "\n        "
                    MEMBER@104..125
                      NAME@104..107
                        IDENT@104..107 "fmt"
                      COLON@107..108 ":"
                      WHITESPACE@108..109 " "
                      FN_TYPE@109..124
                        FN_KW@109..111 "fn"
                        PARAM_LIST@111..117
                          L_PAREN@111..112 "("
                          PARAM@112..116
                            PATH_TYPE@112..116
                              NAME_REF@112..116
                                IDENT@112..116 "Self"
                          R_PAREN@116..117 ")"
                        WHITESPACE@117..118 " "
                        RET_TYPE@118..124
                          THIN_ARROW@118..120 "->"
                          WHITESPACE@120..121 " "
                          PATH_TYPE@121..124
                            NAME_REF@121..124
                              IDENT@121..124 "str"
                      SEMICOLON@124..125 ";"
                    WHITESPACE@125..130 "\n    "
                    R_BRACE@130..131 "}"
                  WHITESPACE@131..132 "\n"
                  R_BRACE@132..133 "}"
                SEMICOLON@133..134 ";"
              WHITESPACE@134..135 "\n"
            error 104..125: an impl member is defined with `=`; the colon-declared requirement form belongs in the trait declaration
        "#]],
    );
}

#[test]
fn impls_in_generic_trait_chain_reserved() {
    // A RESERVED generic trait's chain must not go live: each impl in it
    // carries its own reservation (no silent semantics).
    check(
        r#"
trait Gen = requires::<T> { get: fn(Self) -> usize; } with {
    impl usize { get = fn(x: usize) -> usize { x }; }
};
"#,
        expect![[r#"
            SOURCE_FILE@0..119
              WHITESPACE@0..1 "\n"
              TRAIT_ITEM@1..118
                ITEM_HEAD@1..6
                  TRAIT_KW@1..6 "trait"
                WHITESPACE@6..7 " "
                NAME@7..10
                  IDENT@7..10 "Gen"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                REQUIRES_DEF@13..54
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
                  MEMBER@29..52
                    NAME@29..32
                      IDENT@29..32 "get"
                    COLON@32..33 ":"
                    WHITESPACE@33..34 " "
                    FN_TYPE@34..51
                      FN_KW@34..36 "fn"
                      PARAM_LIST@36..42
                        L_PAREN@36..37 "("
                        PARAM@37..41
                          PATH_TYPE@37..41
                            NAME_REF@37..41
                              IDENT@37..41 "Self"
                        R_PAREN@41..42 ")"
                      WHITESPACE@42..43 " "
                      RET_TYPE@43..51
                        THIN_ARROW@43..45 "->"
                        WHITESPACE@45..46 " "
                        PATH_TYPE@46..51
                          NAME_REF@46..51
                            IDENT@46..51 "usize"
                    SEMICOLON@51..52 ";"
                  WHITESPACE@52..53 " "
                  R_BRACE@53..54 "}"
                WHITESPACE@54..55 " "
                WITH_GROUP@55..117
                  WITH_KW@55..59 "with"
                  WHITESPACE@59..60 " "
                  L_BRACE@60..61 "{"
                  WHITESPACE@61..66 "\n    "
                  IMPL_ELEMENT@66..115
                    IMPL_KW@66..70 "impl"
                    WHITESPACE@70..71 " "
                    PATH_TYPE@71..76
                      NAME_REF@71..76
                        IDENT@71..76 "usize"
                    WHITESPACE@76..77 " "
                    L_BRACE@77..78 "{"
                    WHITESPACE@78..79 " "
                    MEMBER@79..113
                      NAME@79..82
                        IDENT@79..82 "get"
                      WHITESPACE@82..83 " "
                      EQ@83..84 "="
                      WHITESPACE@84..85 " "
                      FN_LITERAL@85..112
                        FN_KW@85..87 "fn"
                        PARAM_LIST@87..97
                          L_PAREN@87..88 "("
                          PARAM@88..96
                            BIND_PAT@88..89
                              NAME@88..89
                                IDENT@88..89 "x"
                            COLON@89..90 ":"
                            WHITESPACE@90..91 " "
                            PATH_TYPE@91..96
                              NAME_REF@91..96
                                IDENT@91..96 "usize"
                          R_PAREN@96..97 ")"
                        WHITESPACE@97..98 " "
                        RET_TYPE@98..106
                          THIN_ARROW@98..100 "->"
                          WHITESPACE@100..101 " "
                          PATH_TYPE@101..106
                            NAME_REF@101..106
                              IDENT@101..106 "usize"
                        WHITESPACE@106..107 " "
                        BLOCK_EXPR@107..112
                          L_BRACE@107..108 "{"
                          WHITESPACE@108..109 " "
                          PATH_EXPR@109..110
                            NAME_REF@109..110
                              IDENT@109..110 "x"
                          WHITESPACE@110..111 " "
                          R_BRACE@111..112 "}"
                      SEMICOLON@112..113 ";"
                    WHITESPACE@113..114 " "
                    R_BRACE@114..115 "}"
                  WHITESPACE@115..116 "\n"
                  R_BRACE@116..117 "}"
                SEMICOLON@117..118 ";"
              WHITESPACE@118..119 "\n"
            error 21..26: generic traits are not supported yet
            error 71..76: impls in a generic trait's `with`-chain are not supported yet (generic traits are reserved)
        "#]],
    );
}
