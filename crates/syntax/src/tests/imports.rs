//! `use` items and their recovery.

use super::*;

// ---- `use` — imports from another file -----------------------------------

#[test]
fn use_imports_one_name() {
    check(
        "use sub::helpers::double;",
        expect![[r#"
        SOURCE_FILE@0..25
          USE_ITEM@0..25
            USE_KW@0..3 "use"
            WHITESPACE@3..4 " "
            NAME_REF@4..7
              IDENT@4..7 "sub"
            COLON2@7..9 "::"
            NAME_REF@9..16
              IDENT@9..16 "helpers"
            COLON2@16..18 "::"
            USE_NAME@18..24
              NAME_REF@18..24
                IDENT@18..24 "double"
            SEMICOLON@24..25 ";"
    "#]],
    );
}

#[test]
fn use_imports_a_braced_list_with_a_rename() {
    check(
        "use b::{double, Pair as P,};",
        expect![[r#"
            SOURCE_FILE@0..28
              USE_ITEM@0..28
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..14
                  NAME_REF@8..14
                    IDENT@8..14 "double"
                COMMA@14..15 ","
                WHITESPACE@15..16 " "
                USE_NAME@16..25
                  NAME_REF@16..20
                    IDENT@16..20 "Pair"
                  WHITESPACE@20..21 " "
                  AS_KW@21..23 "as"
                  WHITESPACE@23..24 " "
                  NAME@24..25
                    IDENT@24..25 "P"
                COMMA@25..26 ","
                R_BRACE@26..27 "}"
                SEMICOLON@27..28 ";"
        "#]],
    );
}

#[test]
fn use_without_a_file() {
    check(
        "use double;",
        expect![[r#"
            SOURCE_FILE@0..11
              USE_ITEM@0..11
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                USE_NAME@4..10
                  NAME_REF@4..10
                    IDENT@4..10 "double"
                SEMICOLON@10..11 ";"
            error 4..10: expected a file to import from, followed by `::`
        "#]],
    );
}

#[test]
fn use_without_a_name() {
    check(
        "use b::;",
        expect![[r#"
            SOURCE_FILE@0..8
              USE_ITEM@0..8
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                SEMICOLON@7..8 ";"
            error 7..8: expected a name to import, or `{` and a list of them
        "#]],
    );
}

#[test]
fn use_with_an_unclosed_list_before_an_item() {
    check(
        r#"
use b::{double static f = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..30
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..15
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                L_BRACE@8..9 "{"
                USE_NAME@9..15
                  NAME_REF@9..15
                    IDENT@9..15 "double"
              WHITESPACE@15..16 " "
              STATIC_ITEM@16..29
                ITEM_HEAD@16..22
                  STATIC_KW@16..22 "static"
                WHITESPACE@22..23 " "
                NAME@23..24
                  IDENT@23..24 "f"
                WHITESPACE@24..25 " "
                EQ@25..26 "="
                WHITESPACE@26..27 " "
                LITERAL@27..28
                  INT_NUMBER@27..28 "1"
                SEMICOLON@28..29 ";"
              WHITESPACE@29..30 "\n"
            error 14..15: expected `}`
        "#]],
    );
}

#[test]
fn use_with_nothing_after_as() {
    check(
        "use b::{a as};",
        expect![[r#"
            SOURCE_FILE@0..14
              USE_ITEM@0..14
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..12
                  NAME_REF@8..9
                    IDENT@8..9 "a"
                  WHITESPACE@9..10 " "
                  AS_KW@10..12 "as"
                R_BRACE@12..13 "}"
                SEMICOLON@13..14 ";"
            error 12..13: expected a name after `as`
        "#]],
    );
}

#[test]
fn use_with_a_number_in_its_list() {
    check(
        "use b::{1};",
        expect![[r#"
            SOURCE_FILE@0..11
              USE_ITEM@0..11
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                ERROR@8..9
                  INT_NUMBER@8..9 "1"
                R_BRACE@9..10 "}"
                SEMICOLON@10..11 ";"
            error 8..9: expected a name to import
        "#]],
    );
}

#[test]
fn use_with_a_missing_comma() {
    check(
        "use b::{a c};",
        expect![[r#"
            SOURCE_FILE@0..13
              USE_ITEM@0..13
                USE_KW@0..3 "use"
                WHITESPACE@3..4 " "
                NAME_REF@4..5
                  IDENT@4..5 "b"
                COLON2@5..7 "::"
                L_BRACE@7..8 "{"
                USE_NAME@8..9
                  NAME_REF@8..9
                    IDENT@8..9 "a"
                WHITESPACE@9..10 " "
                USE_NAME@10..11
                  NAME_REF@10..11
                    IDENT@10..11 "c"
                R_BRACE@11..12 "}"
                SEMICOLON@12..13 ";"
            error 10..11: expected `,` or `}`
        "#]],
    );
}

#[test]
fn use_without_a_semicolon() {
    check(
        r#"
use b::a
static f = 1;
"#,
        expect![[r#"
            SOURCE_FILE@0..24
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..9
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                USE_NAME@8..9
                  NAME_REF@8..9
                    IDENT@8..9 "a"
              WHITESPACE@9..10 "\n"
              STATIC_ITEM@10..23
                ITEM_HEAD@10..16
                  STATIC_KW@10..16 "static"
                WHITESPACE@16..17 " "
                NAME@17..18
                  IDENT@17..18 "f"
                WHITESPACE@18..19 " "
                EQ@19..20 "="
                WHITESPACE@20..21 " "
                LITERAL@21..22
                  INT_NUMBER@21..22 "1"
                SEMICOLON@22..23 ";"
              WHITESPACE@23..24 "\n"
            error 8..9: expected `;`
        "#]],
    );
}

#[test]
fn use_after_a_broken_expression() {
    check(
        r#"
static f = fn { 1 +
use b::a;
"#,
        expect![[r#"
            SOURCE_FILE@0..31
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..20
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..20
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  BLOCK_EXPR@15..20
                    L_BRACE@15..16 "{"
                    WHITESPACE@16..17 " "
                    EXPR_STMT@17..20
                      BIN_EXPR@17..20
                        LITERAL@17..18
                          INT_NUMBER@17..18 "1"
                        WHITESPACE@18..19 " "
                        PLUS@19..20 "+"
              WHITESPACE@20..21 "\n"
              USE_ITEM@21..30
                USE_KW@21..24 "use"
                WHITESPACE@24..25 " "
                NAME_REF@25..26
                  IDENT@25..26 "b"
                COLON2@26..28 "::"
                USE_NAME@28..29
                  NAME_REF@28..29
                    IDENT@28..29 "a"
                SEMICOLON@29..30 ";"
              WHITESPACE@30..31 "\n"
            error 19..20: expected `;`
            error 21..24: expected an expression
        "#]],
    );
}

#[test]
fn use_after_an_unclosed_match() {
    check(
        r#"
static f = fn(s: usize) { match s {
use b::a;
"#,
        expect![[r#"
            SOURCE_FILE@0..47
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..36
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..9
                  IDENT@8..9 "f"
                WHITESPACE@9..10 " "
                EQ@10..11 "="
                WHITESPACE@11..12 " "
                FN_LITERAL@12..36
                  FN_KW@12..14 "fn"
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
                          IDENT@18..23 "usize"
                    R_PAREN@23..24 ")"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..36
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..36
                      MATCH_EXPR@27..36
                        MATCH_KW@27..32 "match"
                        WHITESPACE@32..33 " "
                        PATH_EXPR@33..34
                          NAME_REF@33..34
                            IDENT@33..34 "s"
                        WHITESPACE@34..35 " "
                        L_BRACE@35..36 "{"
              WHITESPACE@36..37 "\n"
              USE_ITEM@37..46
                USE_KW@37..40 "use"
                WHITESPACE@40..41 " "
                NAME_REF@41..42
                  IDENT@41..42 "b"
                COLON2@42..44 "::"
                USE_NAME@44..45
                  NAME_REF@44..45
                    IDENT@44..45 "a"
                SEMICOLON@45..46 ";"
              WHITESPACE@46..47 "\n"
            error 35..36: expected `}`
        "#]],
    );
}

#[test]
fn use_inside_an_unclosed_list() {
    check(
        r#"
use b::{a,
use c::d;
"#,
        expect![[r#"
            SOURCE_FILE@0..22
              WHITESPACE@0..1 "\n"
              USE_ITEM@1..11
                USE_KW@1..4 "use"
                WHITESPACE@4..5 " "
                NAME_REF@5..6
                  IDENT@5..6 "b"
                COLON2@6..8 "::"
                L_BRACE@8..9 "{"
                USE_NAME@9..10
                  NAME_REF@9..10
                    IDENT@9..10 "a"
                COMMA@10..11 ","
              WHITESPACE@11..12 "\n"
              USE_ITEM@12..21
                USE_KW@12..15 "use"
                WHITESPACE@15..16 " "
                NAME_REF@16..17
                  IDENT@16..17 "c"
                COLON2@17..19 "::"
                USE_NAME@19..20
                  NAME_REF@19..20
                    IDENT@19..20 "d"
                SEMICOLON@20..21 ";"
              WHITESPACE@21..22 "\n"
            error 10..11: expected `}`
        "#]],
    );
}

#[test]
fn use_item_accessors() {
    let parse = crate::parse(
        r#"
use a::b::{x, y as z};
use a::x;
"#,
    );
    let shown: Vec<String> = parse
        .tree()
        .use_items()
        .map(|item| {
            let path: Vec<String> = item.segments().map(|s| s.text()).collect();
            let names: Vec<String> = item
                .names()
                .map(|n| {
                    let name = n.name_ref().unwrap().text();
                    match n.rename() {
                        Some(rename) => format!("{name} as {}", rename.text()),
                        None => name,
                    }
                })
                .collect();
            format!("{}: {}", path.join("::"), names.join(", "))
        })
        .collect();
    assert_eq!(shown, ["a::b: x, y as z", "a: x"]);
}
