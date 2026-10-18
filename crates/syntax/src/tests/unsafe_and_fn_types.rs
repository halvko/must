//! Fn types, `unsafe fn` types and `unsafe` blocks.

use super::*;

#[test]
fn an_unsafe_fn_type_parses_into_the_fn_type_node() {
    // One node, one extra token: `unsafe` rides the same modifier slot on a
    // fn TYPE that it does on a fn literal, so nothing downstream has to
    // learn a second shape — the flag is read off the token.
    check(
        "static f: unsafe fn(usize) -> usize = g;",
        expect![[r#"
            SOURCE_FILE@0..40
              STATIC_ITEM@0..40
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                FN_TYPE@10..35
                  UNSAFE_KW@10..16 "unsafe"
                  WHITESPACE@16..17 " "
                  FN_KW@17..19 "fn"
                  PARAM_LIST@19..26
                    L_PAREN@19..20 "("
                    PARAM@20..25
                      PATH_TYPE@20..25
                        NAME_REF@20..25
                          IDENT@20..25 "usize"
                    R_PAREN@25..26 ")"
                  WHITESPACE@26..27 " "
                  RET_TYPE@27..35
                    THIN_ARROW@27..29 "->"
                    WHITESPACE@29..30 " "
                    PATH_TYPE@30..35
                      NAME_REF@30..35
                        IDENT@30..35 "usize"
                WHITESPACE@35..36 " "
                EQ@36..37 "="
                WHITESPACE@37..38 " "
                PATH_EXPR@38..39
                  NAME_REF@38..39
                    IDENT@38..39 "g"
                SEMICOLON@39..40 ";"
        "#]],
    );
}

#[test]
fn unsafe_in_type_position_without_fn_says_so() {
    // `unsafe` marks a FUNCTION type and nothing else, so a bare one gets
    // that sentence once — and the annotation after it is still parsed as
    // the real type it is, so a wrong marker costs the reader no more of
    // the item than the marker itself.
    check(
        "static f: unsafe usize = g;",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                UNSAFE_KW@10..16 "unsafe"
                WHITESPACE@16..17 " "
                PATH_TYPE@17..22
                  NAME_REF@17..22
                    IDENT@17..22 "usize"
                WHITESPACE@22..23 " "
                EQ@23..24 "="
                WHITESPACE@24..25 " "
                PATH_EXPR@25..26
                  NAME_REF@25..26
                    IDENT@25..26 "g"
                SEMICOLON@26..27 ";"
            error 10..16: `unsafe` marks a FUNCTION type: `unsafe fn(...) -> ...`
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
                ITEM_HEAD@0..6
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
fn unsafe_block_parses() {
    check(
        "static f = fn { unsafe { p.* } };",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..33
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
            error 18..25: an `unsafe fn` literal is not supported yet; the `unsafe fn(...)` type is live, so annotate the value and write a plain `fn`
        "#]],
    );
}
