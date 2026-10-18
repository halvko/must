//! Turbofish arguments, member turbofish and named `Self` paths.

use super::*;

// ---- generics: turbofish ----

#[test]
fn turbofish_call_expr() {
    check(
        "static x = f::<usize, 42>();",
        expect![[r#"
            SOURCE_FILE@0..28
              STATIC_ITEM@0..28
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                  UNIT_EXPR@27..29
                    L_PAREN@27..28 "("
                    R_PAREN@28..29 ")"
                SEMICOLON@29..30 ";"
            error 21..22: expected `>`
            error 23..24: expected `,` or `>`
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
                ITEM_HEAD@0..6
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
            error 26..27: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 27..28: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 28..29: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 29..30: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 30..31: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
            error 31..32: expected an item (`static`, `const`, `type`, `trait`, `extern`, `unsafe` or `use`)
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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

#[test]
fn member_turbofish_on_a_dot_call() {
    // The second position for a member's own turbofish: the DOT spelling.
    // It parses into the very same `MEMBER_GENERIC_ARGS` node the qualified
    // spelling produces, one level inside the `FIELD_EXPR` — so
    // everything downstream sees one thing, and a field access (which has
    // no binder) still cannot be confused with the arguments written on it.
    check(
        "static s = fn () -> () { o.flat_map::<usize>(f); };",
        expect![[r#"
            SOURCE_FILE@0..51
              STATIC_ITEM@0..51
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..50
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
                  BLOCK_EXPR@23..50
                    L_BRACE@23..24 "{"
                    WHITESPACE@24..25 " "
                    EXPR_STMT@25..48
                      CALL_EXPR@25..47
                        FIELD_EXPR@25..44
                          PATH_EXPR@25..26
                            NAME_REF@25..26
                              IDENT@25..26 "o"
                          DOT@26..27 "."
                          NAME_REF@27..35
                            IDENT@27..35 "flat_map"
                          MEMBER_GENERIC_ARGS@35..44
                            COLON2@35..37 "::"
                            GENERIC_ARG_LIST@37..44
                              L_ANGLE@37..38 "<"
                              TYPE_ARG@38..43
                                PATH_TYPE@38..43
                                  NAME_REF@38..43
                                    IDENT@38..43 "usize"
                              R_ANGLE@43..44 ">"
                        ARG_LIST@44..47
                          L_PAREN@44..45 "("
                          PATH_EXPR@45..46
                            NAME_REF@45..46
                              IDENT@45..46 "f"
                          R_PAREN@46..47 ")"
                      SEMICOLON@47..48 ";"
                    WHITESPACE@48..49 " "
                    R_BRACE@49..50 "}"
                SEMICOLON@50..51 ";"
        "#]],
    );
}

#[test]
fn the_dot_member_turbofish_has_its_own_accessor() {
    // The AST half: `FieldExpr::member_generic_arg_list()` is the dot
    // spelling's twin of `PathExpr::member_generic_arg_list()`, and a plain
    // field access has none.
    use crate::ast::{self, AstNode};
    let field_of = |text: &str| {
        crate::parse(text)
            .syntax_node()
            .descendants()
            .find_map(ast::FieldExpr::cast)
            .expect("a field expression")
    };
    let with_args = field_of("static s = fn () -> () { o.map::<bool>(f); };");
    assert_eq!(
        with_args
            .member_generic_arg_list()
            .map(|list| list.syntax().text().to_string()),
        Some("<bool>".to_owned())
    );
    assert_eq!(
        with_args.name_ref().map(|n| n.text()),
        Some("map".to_owned())
    );

    let plain = field_of("static s = fn () -> () { o.map(f); };");
    assert!(plain.member_generic_arg_list().is_none());
}
