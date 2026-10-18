//! Region binders, outlives clauses and region arguments.

use super::*;

// ---- regions -------------------------------------------------------------

#[test]
fn region_binders_are_a_third_kind_in_one_list() {
    // ONE binder list, THREE kinds. Regions ride the same slot as types and
    // consts, told apart by their sigil alone — no ordering rule, no
    // separate list, and `@` is unclaimed everywhere else so the lexer
    // needs no lookahead to know one.
    check(
        "static f = fn::<@a, T, const N: usize>(x: T) -> T { x };",
        expect![[r#"
            SOURCE_FILE@0..56
              STATIC_ITEM@0..56
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..55
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..38
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..18
                      REGION_IDENT@16..18 "@a"
                    COMMA@18..19 ","
                    WHITESPACE@19..20 " "
                    TYPE_PARAM@20..21
                      NAME@20..21
                        IDENT@20..21 "T"
                    COMMA@21..22 ","
                    WHITESPACE@22..23 " "
                    CONST_PARAM@23..37
                      CONST_KW@23..28 "const"
                      WHITESPACE@28..29 " "
                      NAME@29..30
                        IDENT@29..30 "N"
                      COLON@30..31 ":"
                      WHITESPACE@31..32 " "
                      PATH_TYPE@32..37
                        NAME_REF@32..37
                          IDENT@32..37 "usize"
                    R_ANGLE@37..38 ">"
                  PARAM_LIST@38..44
                    L_PAREN@38..39 "("
                    PARAM@39..43
                      BIND_PAT@39..40
                        NAME@39..40
                          IDENT@39..40 "x"
                      COLON@40..41 ":"
                      WHITESPACE@41..42 " "
                      PATH_TYPE@42..43
                        NAME_REF@42..43
                          IDENT@42..43 "T"
                    R_PAREN@43..44 ")"
                  WHITESPACE@44..45 " "
                  RET_TYPE@45..49
                    THIN_ARROW@45..47 "->"
                    WHITESPACE@47..48 " "
                    PATH_TYPE@48..49
                      NAME_REF@48..49
                        IDENT@48..49 "T"
                  WHITESPACE@49..50 " "
                  BLOCK_EXPR@50..55
                    L_BRACE@50..51 "{"
                    WHITESPACE@51..52 " "
                    PATH_EXPR@52..53
                      NAME_REF@52..53
                        IDENT@52..53 "x"
                    WHITESPACE@53..54 " "
                    R_BRACE@54..55 "}"
                SEMICOLON@55..56 ";"
        "#]],
    );
}

#[test]
fn region_params_carry_outlives_bounds() {
    // Bounds live where the param is born, exactly as a type param's trait
    // bounds do — and a region's bounds are regions, never traits.
    check(
        "static f = fn::<@a: @b + @c, @b, @c>() -> () {};",
        expect![[r#"
            SOURCE_FILE@0..48
              STATIC_ITEM@0..48
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..47
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..36
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..27
                      REGION_IDENT@16..18 "@a"
                      COLON@18..19 ":"
                      WHITESPACE@19..20 " "
                      REGION_IDENT@20..22 "@b"
                      WHITESPACE@22..23 " "
                      PLUS@23..24 "+"
                      WHITESPACE@24..25 " "
                      REGION_IDENT@25..27 "@c"
                    COMMA@27..28 ","
                    WHITESPACE@28..29 " "
                    REGION_PARAM@29..31
                      REGION_IDENT@29..31 "@b"
                    COMMA@31..32 ","
                    WHITESPACE@32..33 " "
                    REGION_PARAM@33..35
                      REGION_IDENT@33..35 "@c"
                    R_ANGLE@35..36 ">"
                  PARAM_LIST@36..38
                    L_PAREN@36..37 "("
                    R_PAREN@37..38 ")"
                  WHITESPACE@38..39 " "
                  RET_TYPE@39..44
                    THIN_ARROW@39..41 "->"
                    WHITESPACE@41..42 " "
                    UNIT_TYPE@42..44
                      L_PAREN@42..43 "("
                      R_PAREN@43..44 ")"
                  WHITESPACE@44..45 " "
                  BLOCK_EXPR@45..47
                    L_BRACE@45..46 "{"
                    R_BRACE@46..47 "}"
                SEMICOLON@47..48 ";"
        "#]],
    );
}

#[test]
fn the_region_wildcard_and_the_join() {
    check(
        "static f = fn::<@a, @b>(x: usize.&::<@a + @b>) -> () { let r: usize.&::<@_> = x; };",
        expect![[r#"
            SOURCE_FILE@0..83
              STATIC_ITEM@0..83
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..82
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..23
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    REGION_PARAM@16..18
                      REGION_IDENT@16..18 "@a"
                    COMMA@18..19 ","
                    WHITESPACE@19..20 " "
                    REGION_PARAM@20..22
                      REGION_IDENT@20..22 "@b"
                    R_ANGLE@22..23 ">"
                  PARAM_LIST@23..46
                    L_PAREN@23..24 "("
                    PARAM@24..45
                      BIND_PAT@24..25
                        NAME@24..25
                          IDENT@24..25 "x"
                      COLON@25..26 ":"
                      WHITESPACE@26..27 " "
                      BORROW_TYPE@27..45
                        PATH_TYPE@27..32
                          NAME_REF@27..32
                            IDENT@27..32 "usize"
                        DOT@32..33 "."
                        AMP@33..34 "&"
                        COLON2@34..36 "::"
                        GENERIC_ARG_LIST@36..45
                          L_ANGLE@36..37 "<"
                          REGION_ARG@37..44
                            REGION_IDENT@37..39 "@a"
                            WHITESPACE@39..40 " "
                            PLUS@40..41 "+"
                            WHITESPACE@41..42 " "
                            REGION_IDENT@42..44 "@b"
                          R_ANGLE@44..45 ">"
                    R_PAREN@45..46 ")"
                  WHITESPACE@46..47 " "
                  RET_TYPE@47..52
                    THIN_ARROW@47..49 "->"
                    WHITESPACE@49..50 " "
                    UNIT_TYPE@50..52
                      L_PAREN@50..51 "("
                      R_PAREN@51..52 ")"
                  WHITESPACE@52..53 " "
                  BLOCK_EXPR@53..82
                    L_BRACE@53..54 "{"
                    WHITESPACE@54..55 " "
                    LET_STMT@55..80
                      LET_KW@55..58 "let"
                      WHITESPACE@58..59 " "
                      BIND_PAT@59..60
                        NAME@59..60
                          IDENT@59..60 "r"
                      COLON@60..61 ":"
                      WHITESPACE@61..62 " "
                      BORROW_TYPE@62..75
                        PATH_TYPE@62..67
                          NAME_REF@62..67
                            IDENT@62..67 "usize"
                        DOT@67..68 "."
                        AMP@68..69 "&"
                        COLON2@69..71 "::"
                        GENERIC_ARG_LIST@71..75
                          L_ANGLE@71..72 "<"
                          REGION_ARG@72..74
                            REGION_IDENT@72..74 "@_"
                          R_ANGLE@74..75 ">"
                      WHITESPACE@75..76 " "
                      EQ@76..77 "="
                      WHITESPACE@77..78 " "
                      PATH_EXPR@78..79
                        NAME_REF@78..79
                          IDENT@78..79 "x"
                      SEMICOLON@79..80 ";"
                    WHITESPACE@80..81 " "
                    R_BRACE@81..82 "}"
                SEMICOLON@82..83 ";"
        "#]],
    );
}

#[test]
fn a_bare_at_sign_is_an_error_that_names_the_spelling() {
    check(
        "static f = fn::<@>() -> () {};",
        expect![[r#"
            SOURCE_FILE@0..30
              STATIC_ITEM@0..30
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..29
                  FN_KW@11..13 "fn"
                  GENERIC_PARAM_LIST@13..18
                    COLON2@13..15 "::"
                    L_ANGLE@15..16 "<"
                    ERROR@16..17
                      ERROR_TOKEN@16..17 "@"
                    R_ANGLE@17..18 ">"
                  PARAM_LIST@18..20
                    L_PAREN@18..19 "("
                    R_PAREN@19..20 ")"
                  WHITESPACE@20..21 " "
                  RET_TYPE@21..26
                    THIN_ARROW@21..23 "->"
                    WHITESPACE@23..24 " "
                    UNIT_TYPE@24..26
                      L_PAREN@24..25 "("
                      R_PAREN@25..26 ")"
                  WHITESPACE@26..27 " "
                  BLOCK_EXPR@27..29
                    L_BRACE@27..28 "{"
                    R_BRACE@28..29 "}"
                SEMICOLON@29..30 ";"
            error 16..17: expected a region name after `@` (`@a`, or `@_` to infer one)
        "#]],
    );
}

#[test]
fn region_params_come_first_in_a_binder() {
    // Regions are elided at every call site, so what a turbofish spells is
    // the binder MINUS its regions. That subtraction is only legible when
    // the regions are a contiguous PREFIX: `fn::<T, @a>` called `f::<usize>`
    // would read as a list with a slot collapsed out of the middle of it.
    check_errors(
        "static f = fn::<T, @a>(x: T.&::<@a>) -> () {};",
        expect![[r#"
            19..21: region parameters come first in a binder; move `@a` before `T`
        "#]],
    );
    // Regions among themselves, and before everything spellable: fine.
    check_errors(
        "static f = fn::<@a, @b: @a, T, const N: usize>(x: T.&::<@b>) -> () {};",
        expect![[r#""#]],
    );
    // The rule is about KINDS, so a const parameter in front counts too.
    check_errors(
        "static f = fn::<const N: usize, @a>(x: usize.&::<@a>) -> () {};",
        expect![[r#"
            32..34: region parameters come first in a binder; move `@a` before `N`
        "#]],
    );
    // One binder rule, at every binder position — a trait requirement's own
    // is checked by the same node, and nothing else reserves that shape, so
    // the ordering error is the only one it produces.
    check_errors(
        "trait D = requires { size: fn::<T, @a>(T.&::<@a>, Self) -> usize; };",
        expect![[r#"
            35..37: region parameters come first in a binder; move `@a` before `T`
        "#]],
    );
}

#[test]
fn a_borrow_expressions_turbofish_parses_and_is_refused() {
    // The borrow OPERATOR's turbofish still parses on the expression side —
    // one `GENERIC_ARG_LIST` hanging off the `BORROW_EXPR`, exactly where
    // the type-side spelling puts it. That is deliberate superset-parsing,
    // and the refusal below is what it buys: the argument is in the tree,
    // so validation can name it and hand back the one-token fix instead of
    // the parser guessing at a recovery.
    check(
        "static f = fn () -> () { let a = x.&; let b = y.&mut::<@r>; };",
        expect![[r#"
            SOURCE_FILE@0..62
              STATIC_ITEM@0..62
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..61
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
                  BLOCK_EXPR@23..61
                    L_BRACE@23..24 "{"
                    WHITESPACE@24..25 " "
                    LET_STMT@25..37
                      LET_KW@25..28 "let"
                      WHITESPACE@28..29 " "
                      BIND_PAT@29..30
                        NAME@29..30
                          IDENT@29..30 "a"
                      WHITESPACE@30..31 " "
                      EQ@31..32 "="
                      WHITESPACE@32..33 " "
                      BORROW_EXPR@33..36
                        PATH_EXPR@33..34
                          NAME_REF@33..34
                            IDENT@33..34 "x"
                        DOT@34..35 "."
                        AMP@35..36 "&"
                      SEMICOLON@36..37 ";"
                    WHITESPACE@37..38 " "
                    LET_STMT@38..59
                      LET_KW@38..41 "let"
                      WHITESPACE@41..42 " "
                      BIND_PAT@42..43
                        NAME@42..43
                          IDENT@42..43 "b"
                      WHITESPACE@43..44 " "
                      EQ@44..45 "="
                      WHITESPACE@45..46 " "
                      BORROW_EXPR@46..58
                        PATH_EXPR@46..47
                          NAME_REF@46..47
                            IDENT@46..47 "y"
                        DOT@47..48 "."
                        AMP@48..49 "&"
                        MUT_KW@49..52 "mut"
                        COLON2@52..54 "::"
                        GENERIC_ARG_LIST@54..58
                          L_ANGLE@54..55 "<"
                          REGION_ARG@55..57
                            REGION_IDENT@55..57 "@r"
                          R_ANGLE@57..58 ">"
                      SEMICOLON@58..59 ";"
                    WHITESPACE@59..60 " "
                    R_BRACE@60..61 "}"
                SEMICOLON@61..62 ";"
            error 52..58: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&mut::<@a> = x.&mut;`)
        "#]],
    );
}

/// **Regions are written only in TYPE positions; operations never carry
/// them.** A borrow is an operation, so its turbofish is refused — in every
/// form the grammar can build it. `@_` goes with the named forms: an
/// omitted turbofish already means "infer it", so the wildcard is a token
/// that buys nothing, and letting it survive would leave the rule reading
/// "regions are inferred at a borrow, except when you say so".
#[test]
fn a_region_argument_on_a_borrow_expression_is_refused() {
    // Shared and exclusive, named and wildcard.
    check_errors(
        "static f = fn::<@a>(p: usize.&::<@a>) -> () {\n\
             let q = p.*.&::<@a>;\n\
             let r = p.*.&mut::<@a>;\n\
             let s = p.*.&::<@_>;\n\
             let t = p.*.&mut::<@_>;\n\
         };",
        expect![[r#"
            59..65: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
            83..89: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&mut::<@a> = x.&mut;`)
            104..110: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
            128..134: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&mut::<@a> = x.&mut;`)
        "#]],
    );
    // A wrong-kind argument and a two-element list get the same one
    // sentence: an operation has no slot for a kind or an arity message
    // to presuppose (the TYPE side keeps both).
    check_errors(
        "static f = fn::<@a, @b>(p: usize.&::<@a>) -> () {\n\
             let q = p.*.&::<usize>;\n\
             let r = p.*.&::<@a, @b>;\n\
         };",
        expect![[r#"
            63..72: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
            87..97: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
        "#]],
    );
}

/// The fix is the one-token delete, `::` through `>`, leaving the bare
/// operator; the message's example follows the operator, so copying it
/// keeps an exclusive borrow exclusive.
#[test]
fn the_region_argument_fix_drops_the_turbofish() {
    let src = "static f = fn::<@a>(p: usize.&::<@a>) -> () { let r = p.*.&mut::<@a>; };";
    let parse = crate::parse(src);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("regions are inferred at a borrow"))
        .expect("expected the refusal");
    assert!(err.message.ends_with("(`let r: _.&mut::<@a> = x.&mut;`)"));
    let fix = err.fix.as_ref().expect("expected a fix");
    assert_eq!(fix.label, "Drop the region argument");
    assert_eq!(
        apply_fix(src, fix),
        "static f = fn::<@a>(p: usize.&::<@a>) -> () { let r = p.*.&mut; };"
    );
}

/// The refusal reaches a borrow wherever the postfix tier can chain one —
/// it is a property of the OPERATOR, not of the shape of its operand.
#[test]
fn a_region_argument_is_refused_on_a_nested_borrow_too() {
    check_errors(
        "static f = fn::<@a>(p: usize.&::<@a>, arr: [usize; 2].&::<@a>) -> () {\n\
             let q = p.*.&::<@a>;\n\
             let r = arr.*[0].&::<@a>;\n\
         };",
        expect![[r#"
            84..90: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
            110..116: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
        "#]],
    );
}

#[test]
fn outlives_clauses_ride_the_with_clause_grammar() {
    // A region-headed constrain clause, the third `with_clause` shape.
    // Parse-and-reserve — regions on type declarations are not decided yet,
    // so this parses its real tree and validation says "not yet".
    check(
        "type Slice = struct::<@a, T> { n: usize } with @a: @b { impl Self {} };",
        expect![[r#"
            SOURCE_FILE@0..71
              TYPE_ITEM@0..71
                ITEM_HEAD@0..4
                  TYPE_KW@0..4 "type"
                WHITESPACE@4..5 " "
                NAME@5..10
                  IDENT@5..10 "Slice"
                WHITESPACE@10..11 " "
                EQ@11..12 "="
                WHITESPACE@12..13 " "
                RECORD_EXPR@13..41
                  STRUCT_KW@13..19 "struct"
                  GENERIC_PARAM_LIST@19..28
                    COLON2@19..21 "::"
                    L_ANGLE@21..22 "<"
                    REGION_PARAM@22..24
                      REGION_IDENT@22..24 "@a"
                    COMMA@24..25 ","
                    WHITESPACE@25..26 " "
                    TYPE_PARAM@26..27
                      NAME@26..27
                        IDENT@26..27 "T"
                    R_ANGLE@27..28 ">"
                  WHITESPACE@28..29 " "
                  L_BRACE@29..30 "{"
                  WHITESPACE@30..31 " "
                  RECORD_EXPR_FIELD@31..39
                    NAME_REF@31..32
                      IDENT@31..32 "n"
                    COLON@32..33 ":"
                    WHITESPACE@33..34 " "
                    PATH_TYPE@34..39
                      NAME_REF@34..39
                        IDENT@34..39 "usize"
                  WHITESPACE@39..40 " "
                  R_BRACE@40..41 "}"
                WHITESPACE@41..42 " "
                WITH_GROUP@42..70
                  WITH_KW@42..46 "with"
                  WHITESPACE@46..47 " "
                  WITH_CLAUSE@47..53
                    REGION_IDENT@47..49 "@a"
                    COLON@49..50 ":"
                    WHITESPACE@50..51 " "
                    REGION_IDENT@51..53 "@b"
                  WHITESPACE@53..54 " "
                  L_BRACE@54..55 "{"
                  WHITESPACE@55..56 " "
                  IMPL_ELEMENT@56..68
                    IMPL_KW@56..60 "impl"
                    WHITESPACE@60..61 " "
                    PATH_TYPE@61..65
                      NAME_REF@61..65
                        IDENT@61..65 "Self"
                    WHITESPACE@65..66 " "
                    L_BRACE@66..67 "{"
                    R_BRACE@67..68 "}"
                  WHITESPACE@68..69 " "
                  R_BRACE@69..70 "}"
                SEMICOLON@70..71 ";"
            error 47..53: `with @a: ...` outlives groups are not supported yet
        "#]],
    );
}
