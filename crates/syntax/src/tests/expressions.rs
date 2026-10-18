//! Expressions: `if`, comparisons, unary minus, digit groups, unit.

use super::*;

#[test]
fn if_else_chain() {
    check(
        "static x = if a { 1 } else if b { 2 } else { 3 };",
        expect![[r#"
            SOURCE_FILE@0..49
              STATIC_ITEM@0..49
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@17..23
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
fn unary_minus_parses_as_a_neg_expr() {
    check(
        "static x: i32 = -5;",
        expect![[r#"
            SOURCE_FILE@0..19
              STATIC_ITEM@0..19
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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
                ITEM_HEAD@0..6
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

#[test]
fn digit_group_commas_are_one_error_where_a_semicolon_is_owed() {
    check_errors(
        r#"
static big: i64 = 1,000,000;
static f = fn () -> i32 {
    let min: i32 = 2,147,483,647;
    x = 12,345;
    -2,147,483,648
};
"#,
        expect![[r#"
            19..28: digit groups cannot be separated by `,`; use `_`
            75..88: digit groups cannot be separated by `,`; use `_`
            98..104: digit groups cannot be separated by `,`; use `_`
            111..124: digit groups cannot be separated by `,`; use `_`
        "#]],
    );
}

#[test]
fn digit_group_commas_fix_joins_the_groups_with_underscores() {
    let text = "static f = fn () -> () { let min: i32 = 2,147,483,647; };";
    let parse = crate::parse(text);
    let errors = parse.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let fix = errors[0].fix.as_ref().expect("a fix");
    assert_eq!(fix.label, "Write `2_147_483_647`");
    let fixed = apply_fix(text, fix);
    assert_eq!(
        fixed,
        "static f = fn () -> () { let min: i32 = 2_147_483_647; };"
    );
    assert!(crate::parse(&fixed).errors().is_empty());
}

#[test]
fn digit_group_commas_leave_lists_and_other_shapes_alone() {
    // A `,` that separates list elements is legal, and a spaced or
    // non-3-digit group is not read as a digit group.
    check_errors(
        r#"
static f = fn () -> () {
    g(1,234);
    let a = [1,234];
    let b = 1, 234;
    let c = 3,14;
};
"#,
        expect![[r#"
            73..74: expected `;`
            74..75: expected an expression
            93..94: expected `;`
            94..95: expected an expression
        "#]],
    );
}

#[test]
fn digit_group_commas_are_one_error_node() {
    check(
        r#"
static f = fn () -> i32 { -2,147 };
"#,
        expect![[r#"
            SOURCE_FILE@0..37
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
                FN_LITERAL@12..35
                  FN_KW@12..14 "fn"
                  WHITESPACE@14..15 " "
                  PARAM_LIST@15..17
                    L_PAREN@15..16 "("
                    R_PAREN@16..17 ")"
                  WHITESPACE@17..18 " "
                  RET_TYPE@18..24
                    THIN_ARROW@18..20 "->"
                    WHITESPACE@20..21 " "
                    PATH_TYPE@21..24
                      NAME_REF@21..24
                        IDENT@21..24 "i32"
                  WHITESPACE@24..25 " "
                  BLOCK_EXPR@25..35
                    L_BRACE@25..26 "{"
                    WHITESPACE@26..27 " "
                    NEG_EXPR@27..33
                      MINUS@27..28 "-"
                      ERROR@28..33
                        INT_NUMBER@28..29 "2"
                        COMMA@29..30 ","
                        INT_NUMBER@30..33 "147"
                    WHITESPACE@33..34 " "
                    R_BRACE@34..35 "}"
                SEMICOLON@35..36 ";"
              WHITESPACE@36..37 "\n"
            error 28..33: digit groups cannot be separated by `,`; use `_`
        "#]],
    );
}

#[test]
fn unit_value_is_spelled_like_the_unit_type() {
    check(
        "static u: () = ( );",
        expect![[r#"
        SOURCE_FILE@0..19
          STATIC_ITEM@0..19
            ITEM_HEAD@0..6
              STATIC_KW@0..6 "static"
            WHITESPACE@6..7 " "
            NAME@7..8
              IDENT@7..8 "u"
            COLON@8..9 ":"
            WHITESPACE@9..10 " "
            UNIT_TYPE@10..12
              L_PAREN@10..11 "("
              R_PAREN@11..12 ")"
            WHITESPACE@12..13 " "
            EQ@13..14 "="
            WHITESPACE@14..15 " "
            UNIT_EXPR@15..18
              L_PAREN@15..16 "("
              WHITESPACE@16..17 " "
              R_PAREN@17..18 ")"
            SEMICOLON@18..19 ";"
    "#]],
    );
}
