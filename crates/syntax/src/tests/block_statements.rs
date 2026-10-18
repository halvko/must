//! Block-shaped statements and when they owe a semicolon.

use super::*;

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

// ---- the brace rule in statement position -----------------------------
//
// A statement whose expression ends in `}` closes itself. The tests below
// pin the carve-out and each of its edges: the shapes it covers, the `;`
// staying legal, the greedy continuation that is its other half, and the
// positions it deliberately leaves alone.

#[test]
fn else_if_chain_as_a_statement_needs_no_semicolon() {
    // The shape that provoked the ruling: a branch run for effect, followed
    // by an ordinary statement. Two EXPR_STMTs, no error.
    check(
        "static f = fn (c: usize) { if c { a() } else if c { b() } else { d() } e(); };",
        expect![[r#"
            SOURCE_FILE@0..78
              STATIC_ITEM@0..78
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..77
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..24
                    L_PAREN@14..15 "("
                    PARAM@15..23
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "c"
                      COLON@16..17 ":"
                      WHITESPACE@17..18 " "
                      PATH_TYPE@18..23
                        NAME_REF@18..23
                          IDENT@18..23 "usize"
                    R_PAREN@23..24 ")"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..77
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    EXPR_STMT@27..70
                      IF_EXPR@27..70
                        IF_KW@27..29 "if"
                        WHITESPACE@29..30 " "
                        PATH_EXPR@30..31
                          NAME_REF@30..31
                            IDENT@30..31 "c"
                        WHITESPACE@31..32 " "
                        BLOCK_EXPR@32..39
                          L_BRACE@32..33 "{"
                          WHITESPACE@33..34 " "
                          CALL_EXPR@34..37
                            PATH_EXPR@34..35
                              NAME_REF@34..35
                                IDENT@34..35 "a"
                            ARG_LIST@35..37
                              L_PAREN@35..36 "("
                              R_PAREN@36..37 ")"
                          WHITESPACE@37..38 " "
                          R_BRACE@38..39 "}"
                        WHITESPACE@39..40 " "
                        ELSE_KW@40..44 "else"
                        WHITESPACE@44..45 " "
                        IF_EXPR@45..70
                          IF_KW@45..47 "if"
                          WHITESPACE@47..48 " "
                          PATH_EXPR@48..49
                            NAME_REF@48..49
                              IDENT@48..49 "c"
                          WHITESPACE@49..50 " "
                          BLOCK_EXPR@50..57
                            L_BRACE@50..51 "{"
                            WHITESPACE@51..52 " "
                            CALL_EXPR@52..55
                              PATH_EXPR@52..53
                                NAME_REF@52..53
                                  IDENT@52..53 "b"
                              ARG_LIST@53..55
                                L_PAREN@53..54 "("
                                R_PAREN@54..55 ")"
                            WHITESPACE@55..56 " "
                            R_BRACE@56..57 "}"
                          WHITESPACE@57..58 " "
                          ELSE_KW@58..62 "else"
                          WHITESPACE@62..63 " "
                          BLOCK_EXPR@63..70
                            L_BRACE@63..64 "{"
                            WHITESPACE@64..65 " "
                            CALL_EXPR@65..68
                              PATH_EXPR@65..66
                                NAME_REF@65..66
                                  IDENT@65..66 "d"
                              ARG_LIST@66..68
                                L_PAREN@66..67 "("
                                R_PAREN@67..68 ")"
                            WHITESPACE@68..69 " "
                            R_BRACE@69..70 "}"
                    WHITESPACE@70..71 " "
                    EXPR_STMT@71..75
                      CALL_EXPR@71..74
                        PATH_EXPR@71..72
                          NAME_REF@71..72
                            IDENT@71..72 "e"
                        ARG_LIST@72..74
                          L_PAREN@72..73 "("
                          R_PAREN@73..74 ")"
                      SEMICOLON@74..75 ";"
                    WHITESPACE@75..76 " "
                    R_BRACE@76..77 "}"
                SEMICOLON@77..78 ";"
        "#]],
    );
}

#[test]
fn every_block_shaped_statement_self_terminates() {
    // `match`, `loop`, `unsafe { }`, `const { }` and a bare block — all of
    // them end in `}`, so all of them stand alone as statements. Not a list
    // the parser holds: see the sibling below, which is the actual rule.
    check(
        "static f = fn { { } match x { } loop { } unsafe { } const { } g(); };",
        expect![[r#"
            SOURCE_FILE@0..69
              STATIC_ITEM@0..69
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..68
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..68
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..19
                      BLOCK_EXPR@16..19
                        L_BRACE@16..17 "{"
                        WHITESPACE@17..18 " "
                        R_BRACE@18..19 "}"
                    WHITESPACE@19..20 " "
                    EXPR_STMT@20..31
                      MATCH_EXPR@20..31
                        MATCH_KW@20..25 "match"
                        WHITESPACE@25..26 " "
                        PATH_EXPR@26..27
                          NAME_REF@26..27
                            IDENT@26..27 "x"
                        WHITESPACE@27..28 " "
                        L_BRACE@28..29 "{"
                        WHITESPACE@29..30 " "
                        R_BRACE@30..31 "}"
                    WHITESPACE@31..32 " "
                    EXPR_STMT@32..40
                      LOOP_EXPR@32..40
                        LOOP_KW@32..36 "loop"
                        WHITESPACE@36..37 " "
                        BLOCK_EXPR@37..40
                          L_BRACE@37..38 "{"
                          WHITESPACE@38..39 " "
                          R_BRACE@39..40 "}"
                    WHITESPACE@40..41 " "
                    EXPR_STMT@41..51
                      UNSAFE_BLOCK_EXPR@41..51
                        UNSAFE_KW@41..47 "unsafe"
                        WHITESPACE@47..48 " "
                        BLOCK_EXPR@48..51
                          L_BRACE@48..49 "{"
                          WHITESPACE@49..50 " "
                          R_BRACE@50..51 "}"
                    WHITESPACE@51..52 " "
                    EXPR_STMT@52..61
                      CONST_BLOCK_EXPR@52..61
                        CONST_KW@52..57 "const"
                        WHITESPACE@57..58 " "
                        BLOCK_EXPR@58..61
                          L_BRACE@58..59 "{"
                          WHITESPACE@59..60 " "
                          R_BRACE@60..61 "}"
                    WHITESPACE@61..62 " "
                    EXPR_STMT@62..66
                      CALL_EXPR@62..65
                        PATH_EXPR@62..63
                          NAME_REF@62..63
                            IDENT@62..63 "g"
                        ARG_LIST@63..65
                          L_PAREN@63..64 "("
                          R_PAREN@64..65 ")"
                      SEMICOLON@65..66 ";"
                    WHITESPACE@66..67 " "
                    R_BRACE@67..68 "}"
                SEMICOLON@68..69 ";"
        "#]],
    );
}

#[test]
fn the_rule_is_asked_of_the_token_not_of_a_list_of_kinds() {
    // The parser holds no list of block-shaped expressions: it asks whether
    // the statement's LAST TOKEN was a `}`. So the forms that are not
    // "block-shaped" at all but still end in a brace — a `fn` literal, a
    // record literal, an enum literal — self-terminate too, and so will
    // anything brace-ended added later, with no edit here. (As statements
    // all three are dead code; what matters is that no shape has to be
    // enrolled one at a time.)
    check(
        "static f = fn { fn () { } struct { a = 1 } enum { A } g(); };",
        expect![[r#"
            SOURCE_FILE@0..61
              STATIC_ITEM@0..61
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..60
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..60
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..25
                      FN_LITERAL@16..25
                        FN_KW@16..18 "fn"
                        WHITESPACE@18..19 " "
                        PARAM_LIST@19..21
                          L_PAREN@19..20 "("
                          R_PAREN@20..21 ")"
                        WHITESPACE@21..22 " "
                        BLOCK_EXPR@22..25
                          L_BRACE@22..23 "{"
                          WHITESPACE@23..24 " "
                          R_BRACE@24..25 "}"
                    WHITESPACE@25..26 " "
                    EXPR_STMT@26..42
                      RECORD_EXPR@26..42
                        STRUCT_KW@26..32 "struct"
                        WHITESPACE@32..33 " "
                        L_BRACE@33..34 "{"
                        WHITESPACE@34..35 " "
                        RECORD_EXPR_FIELD@35..40
                          NAME_REF@35..36
                            IDENT@35..36 "a"
                          WHITESPACE@36..37 " "
                          EQ@37..38 "="
                          WHITESPACE@38..39 " "
                          LITERAL@39..40
                            INT_NUMBER@39..40 "1"
                        WHITESPACE@40..41 " "
                        R_BRACE@41..42 "}"
                    WHITESPACE@42..43 " "
                    EXPR_STMT@43..53
                      ENUM_EXPR@43..53
                        ENUM_KW@43..47 "enum"
                        WHITESPACE@47..48 " "
                        L_BRACE@48..49 "{"
                        WHITESPACE@49..50 " "
                        ENUM_VARIANT@50..51
                          NAME@50..51
                            IDENT@50..51 "A"
                        WHITESPACE@51..52 " "
                        R_BRACE@52..53 "}"
                    WHITESPACE@53..54 " "
                    EXPR_STMT@54..58
                      CALL_EXPR@54..57
                        PATH_EXPR@54..55
                          NAME_REF@54..55
                            IDENT@54..55 "g"
                        ARG_LIST@55..57
                          L_PAREN@55..56 "("
                          R_PAREN@56..57 ")"
                      SEMICOLON@57..58 ";"
                    WHITESPACE@58..59 " "
                    R_BRACE@59..60 "}"
                SEMICOLON@60..61 ";"
            error 43..53: an `enum` literal can only appear as a `type` declaration's value
        "#]],
    );
}

#[test]
fn a_block_tail_statement_before_an_item_keyword_recovers_on_the_brace() {
    // A RECOVERY IMPROVEMENT that came with the carve-out, pinned so it is
    // not mistaken for a regression. The block-tail statement no longer
    // reports a missing `;`, which un-suppresses the report the enclosing
    // block was already making: the message at this position changes from
    // "expected `;`" to "expected `}`" — one error either way, and the `}`
    // is the one that is actually missing.
    check(
        "static f = fn { { } static g = 1;",
        expect![[r#"
            SOURCE_FILE@0..33
              STATIC_ITEM@0..19
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..19
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..19
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..19
                      BLOCK_EXPR@16..19
                        L_BRACE@16..17 "{"
                        WHITESPACE@17..18 " "
                        R_BRACE@18..19 "}"
              WHITESPACE@19..20 " "
              STATIC_ITEM@20..33
                ITEM_HEAD@20..26
                  STATIC_KW@20..26 "static"
                WHITESPACE@26..27 " "
                NAME@27..28
                  IDENT@27..28 "g"
                WHITESPACE@28..29 " "
                EQ@29..30 "="
                WHITESPACE@30..31 " "
                LITERAL@31..32
                  INT_NUMBER@31..32 "1"
                SEMICOLON@32..33 ";"
            error 18..19: expected `}`
        "#]],
    );
}

#[test]
fn broken_nesting_that_swallowed_a_semicolon_reports_the_brace_once() {
    // The `SEMICOLON` half of `value_closed_itself`'s territory: the
    // unclosed inner block consumed a `;`, so `p.prev()` is that `;` when
    // the statement rule runs. One "expected `}`", not a `;` demand on top
    // of it — see that helper's doc for why it holds either way.
    check(
        "static f = fn { { g(); static h = 1;",
        expect![[r#"
            SOURCE_FILE@0..36
              STATIC_ITEM@0..22
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..22
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..22
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    EXPR_STMT@16..22
                      BLOCK_EXPR@16..22
                        L_BRACE@16..17 "{"
                        WHITESPACE@17..18 " "
                        EXPR_STMT@18..22
                          CALL_EXPR@18..21
                            PATH_EXPR@18..19
                              NAME_REF@18..19
                                IDENT@18..19 "g"
                            ARG_LIST@19..21
                              L_PAREN@19..20 "("
                              R_PAREN@20..21 ")"
                          SEMICOLON@21..22 ";"
              WHITESPACE@22..23 " "
              STATIC_ITEM@23..36
                ITEM_HEAD@23..29
                  STATIC_KW@23..29 "static"
                WHITESPACE@29..30 " "
                NAME@30..31
                  IDENT@30..31 "h"
                WHITESPACE@31..32 " "
                EQ@32..33 "="
                WHITESPACE@33..34 " "
                LITERAL@34..35
                  INT_NUMBER@34..35 "1"
                SEMICOLON@35..36 ";"
            error 21..22: expected `}`
        "#]],
    );
}

#[test]
fn semicolon_after_a_block_tail_statement_stays_legal() {
    // The carve-out makes the `;` OPTIONAL, not wrong: the old spelling
    // keeps parsing, and to exactly the same shape plus its token.
    check(
        "static f = fn { if c { }; g(); };",
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
                    EXPR_STMT@16..25
                      IF_EXPR@16..24
                        IF_KW@16..18 "if"
                        WHITESPACE@18..19 " "
                        PATH_EXPR@19..20
                          NAME_REF@19..20
                            IDENT@19..20 "c"
                        WHITESPACE@20..21 " "
                        BLOCK_EXPR@21..24
                          L_BRACE@21..22 "{"
                          WHITESPACE@22..23 " "
                          R_BRACE@23..24 "}"
                      SEMICOLON@24..25 ";"
                    WHITESPACE@25..26 " "
                    EXPR_STMT@26..30
                      CALL_EXPR@26..29
                        PATH_EXPR@26..27
                          NAME_REF@26..27
                            IDENT@26..27 "g"
                        ARG_LIST@27..29
                          L_PAREN@27..28 "("
                          R_PAREN@28..29 ")"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    R_BRACE@31..32 "}"
                SEMICOLON@32..33 ";"
        "#]],
    );
}

#[test]
fn a_continuation_token_after_a_block_tail_statement_keeps_the_expression() {
    // The greedy half of the rule, at statement level rather than tail
    // level (see `statement_position_block_minus_operator_is_one_bin_expr`):
    // `-` continues, so this is ONE subtraction statement, and its `;` is
    // owed as usual because the statement no longer ends in `}`.
    check(
        "static f = fn { if c { } - 1; g(); };",
        expect![[r#"
            SOURCE_FILE@0..37
              STATIC_ITEM@0..37
                ITEM_HEAD@0..6
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
                    EXPR_STMT@16..29
                      BIN_EXPR@16..28
                        IF_EXPR@16..24
                          IF_KW@16..18 "if"
                          WHITESPACE@18..19 " "
                          PATH_EXPR@19..20
                            NAME_REF@19..20
                              IDENT@19..20 "c"
                          WHITESPACE@20..21 " "
                          BLOCK_EXPR@21..24
                            L_BRACE@21..22 "{"
                            WHITESPACE@22..23 " "
                            R_BRACE@23..24 "}"
                        WHITESPACE@24..25 " "
                        MINUS@25..26 "-"
                        WHITESPACE@26..27 " "
                        LITERAL@27..28
                          INT_NUMBER@27..28 "1"
                      SEMICOLON@28..29 ";"
                    WHITESPACE@29..30 " "
                    EXPR_STMT@30..34
                      CALL_EXPR@30..33
                        PATH_EXPR@30..31
                          NAME_REF@30..31
                            IDENT@30..31 "g"
                        ARG_LIST@31..33
                          L_PAREN@31..32 "("
                          R_PAREN@32..33 ")"
                      SEMICOLON@33..34 ";"
                    WHITESPACE@34..35 " "
                    R_BRACE@35..36 "}"
                SEMICOLON@36..37 ";"
        "#]],
    );
}

#[test]
fn an_explicit_semicolon_splits_a_block_tail_from_a_continuation_shaped_statement() {
    // The accepted price, and the way out of it: `;` after the `}` forces
    // the split the greedy reading would otherwise deny — two statements,
    // the second a negation.
    check(
        "static f = fn { if c { }; -1; };",
        expect![[r#"
            SOURCE_FILE@0..32
              STATIC_ITEM@0..32
                ITEM_HEAD@0..6
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
                    EXPR_STMT@16..25
                      IF_EXPR@16..24
                        IF_KW@16..18 "if"
                        WHITESPACE@18..19 " "
                        PATH_EXPR@19..20
                          NAME_REF@19..20
                            IDENT@19..20 "c"
                        WHITESPACE@20..21 " "
                        BLOCK_EXPR@21..24
                          L_BRACE@21..22 "{"
                          WHITESPACE@22..23 " "
                          R_BRACE@23..24 "}"
                      SEMICOLON@24..25 ";"
                    WHITESPACE@25..26 " "
                    EXPR_STMT@26..29
                      NEG_EXPR@26..28
                        MINUS@26..27 "-"
                        LITERAL@27..28
                          INT_NUMBER@27..28 "1"
                      SEMICOLON@28..29 ";"
                    WHITESPACE@29..30 " "
                    R_BRACE@30..31 "}"
                SEMICOLON@31..32 ";"
        "#]],
    );
}

#[test]
fn a_non_block_expression_statement_still_owes_its_semicolon() {
    // The carve-out is for brace-ended shapes only; nothing else changed.
    check(
        "static f = fn { g() h(); };",
        expect![[r#"
            SOURCE_FILE@0..27
              STATIC_ITEM@0..27
                ITEM_HEAD@0..6
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
                    EXPR_STMT@16..19
                      CALL_EXPR@16..19
                        PATH_EXPR@16..17
                          NAME_REF@16..17
                            IDENT@16..17 "g"
                        ARG_LIST@17..19
                          L_PAREN@17..18 "("
                          R_PAREN@18..19 ")"
                    WHITESPACE@19..20 " "
                    EXPR_STMT@20..24
                      CALL_EXPR@20..23
                        PATH_EXPR@20..21
                          NAME_REF@20..21
                            IDENT@20..21 "h"
                        ARG_LIST@21..23
                          L_PAREN@21..22 "("
                          R_PAREN@22..23 ")"
                      SEMICOLON@23..24 ";"
                    WHITESPACE@24..25 " "
                    R_BRACE@25..26 "}"
                SEMICOLON@26..27 ";"
            error 18..19: expected `;`
        "#]],
    );
}

#[test]
fn a_block_tail_in_value_position_still_owes_its_semicolon() {
    // A `let`'s initializer is a VALUE, not a statement — the brace rule
    // never reaches it, so the `let` still ends with `;`.
    check(
        "static f = fn { let x = if c { 1 } else { 2 } g(); };",
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
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..52
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..45
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "x"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      IF_EXPR@24..45
                        IF_KW@24..26 "if"
                        WHITESPACE@26..27 " "
                        PATH_EXPR@27..28
                          NAME_REF@27..28
                            IDENT@27..28 "c"
                        WHITESPACE@28..29 " "
                        BLOCK_EXPR@29..34
                          L_BRACE@29..30 "{"
                          WHITESPACE@30..31 " "
                          LITERAL@31..32
                            INT_NUMBER@31..32 "1"
                          WHITESPACE@32..33 " "
                          R_BRACE@33..34 "}"
                        WHITESPACE@34..35 " "
                        ELSE_KW@35..39 "else"
                        WHITESPACE@39..40 " "
                        BLOCK_EXPR@40..45
                          L_BRACE@40..41 "{"
                          WHITESPACE@41..42 " "
                          LITERAL@42..43
                            INT_NUMBER@42..43 "2"
                          WHITESPACE@43..44 " "
                          R_BRACE@44..45 "}"
                    WHITESPACE@45..46 " "
                    EXPR_STMT@46..50
                      CALL_EXPR@46..49
                        PATH_EXPR@46..47
                          NAME_REF@46..47
                            IDENT@46..47 "g"
                        ARG_LIST@47..49
                          L_PAREN@47..48 "("
                          R_PAREN@48..49 ")"
                      SEMICOLON@49..50 ";"
                    WHITESPACE@50..51 " "
                    R_BRACE@51..52 "}"
                SEMICOLON@52..53 ";"
            error 44..45: expected `;`
        "#]],
    );
}
