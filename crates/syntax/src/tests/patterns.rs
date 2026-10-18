//! Patterns: holes, `mut` bindings, arrays, characters and negative integers.

use super::*;

#[test]
fn let_hole_pattern() {
    check(
        "static f = fn { let _ = 5; };",
        expect![[r#"
            SOURCE_FILE@0..29
              STATIC_ITEM@0..29
                ITEM_HEAD@0..6
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
                      WILDCARD_PAT@20..21
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
                ITEM_HEAD@0..6
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
                      WILDCARD_PAT@15..16
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
fn match_pattern_bindings_take_mut() {
    check(
        "static f = fn { match s { ::Char(mut c, _) => c, mut o => o } };",
        expect![[r#"
            SOURCE_FILE@0..64
              STATIC_ITEM@0..64
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..63
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..63
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    MATCH_EXPR@16..61
                      MATCH_KW@16..21 "match"
                      WHITESPACE@21..22 " "
                      PATH_EXPR@22..23
                        NAME_REF@22..23
                          IDENT@22..23 "s"
                      WHITESPACE@23..24 " "
                      L_BRACE@24..25 "{"
                      WHITESPACE@25..26 " "
                      MATCH_ARM@26..48
                        VARIANT_PAT@26..42
                          COLON2@26..28 "::"
                          NAME_REF@28..32
                            IDENT@28..32 "Char"
                          L_PAREN@32..33 "("
                          BIND_PAT@33..38
                            MUT_KW@33..36 "mut"
                            WHITESPACE@36..37 " "
                            NAME@37..38
                              IDENT@37..38 "c"
                          COMMA@38..39 ","
                          WHITESPACE@39..40 " "
                          BIND_PAT@40..41
                            NAME@40..41
                              HOLE@40..41 "_"
                          R_PAREN@41..42 ")"
                        WHITESPACE@42..43 " "
                        FAT_ARROW@43..45 "=>"
                        WHITESPACE@45..46 " "
                        PATH_EXPR@46..47
                          NAME_REF@46..47
                            IDENT@46..47 "c"
                        COMMA@47..48 ","
                      WHITESPACE@48..49 " "
                      MATCH_ARM@49..59
                        BIND_PAT@49..54
                          MUT_KW@49..52 "mut"
                          WHITESPACE@52..53 " "
                          NAME@53..54
                            IDENT@53..54 "o"
                        WHITESPACE@54..55 " "
                        FAT_ARROW@55..57 "=>"
                        WHITESPACE@57..58 " "
                        PATH_EXPR@58..59
                          NAME_REF@58..59
                            IDENT@58..59 "o"
                      WHITESPACE@59..60 " "
                      R_BRACE@60..61 "}"
                    WHITESPACE@61..62 " "
                    R_BRACE@62..63 "}"
                SEMICOLON@63..64 ";"
        "#]],
    );
}

#[test]
fn mut_without_a_name_in_a_match_pattern() {
    check_errors(
        "static f = fn { match s { ::Char(mut) => 0, mut => 1 } };",
        expect![[r#"
            36..37: expected a binding name
            48..50: expected a binding name
        "#]],
    );
}

#[test]
fn doubled_comma_in_a_variant_pattern_is_one_error() {
    // The list goes on past the empty slot, so `b` and the arm's body are
    // not read as further arms.
    check_errors(
        "static f = fn(o) { match o { ::Some(a, , b) => 1 } };",
        expect![[r#"
            39..40: expected a binding name
        "#]],
    );
}

#[test]
fn array_literal_missing_a_comma_goes_on() {
    // One error: `3` is read as the next element.
    check_errors(
        "static f = fn { let x = [1, 2 3]; x };",
        expect![[r#"
            30..31: expected `,` or `]`
        "#]],
    );
}

#[test]
fn array_literal_missing_the_comma_after_its_first_element() {
    check_errors(
        "static f = [1 2];",
        expect![[r#"
            14..15: expected `,` or `]`
        "#]],
    );
}

#[test]
fn array_literal_missing_its_first_element_is_one_error() {
    check_errors(
        "static f = [, a];",
        expect![[r#"
            12..13: expected an expression
        "#]],
    );
}

#[test]
fn array_literal_unclosed_at_end_of_file_demands_the_bracket() {
    check_errors(
        "static f = [a, b",
        expect![[r#"
            15..16: expected `]`
        "#]],
    );
}

#[test]
fn repeat_array_unclosed_demands_the_bracket() {
    check_errors(
        "static f = [a; 3",
        expect![[r#"
            15..16: expected `]`
        "#]],
    );
}

#[test]
fn variant_pattern_missing_its_close_paren_stops_at_the_arrow() {
    // The arrow is not taken into the pattern, so the arm's body and the
    // next arm parse as written; the missing `,` is reported at the arrow.
    check_errors(
        "static f = fn(o) { match o { ::Some(a => 1, _ => 2 } };",
        expect![[r#"
            36..37: expected `)`
            38..40: expected `,` or `)`
        "#]],
    );
}

#[test]
fn let_takes_a_pattern() {
    check(
        "static f = fn { let [a, [mut b, _]] = v; let ::Char(c, _) = s; let 0 = n; };",
        expect![[r#"
            SOURCE_FILE@0..76
              STATIC_ITEM@0..76
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..75
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..75
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..40
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      ARRAY_PAT@20..35
                        L_BRACKET@20..21 "["
                        BIND_PAT@21..22
                          NAME@21..22
                            IDENT@21..22 "a"
                        COMMA@22..23 ","
                        WHITESPACE@23..24 " "
                        ARRAY_PAT@24..34
                          L_BRACKET@24..25 "["
                          BIND_PAT@25..30
                            MUT_KW@25..28 "mut"
                            WHITESPACE@28..29 " "
                            NAME@29..30
                              IDENT@29..30 "b"
                          COMMA@30..31 ","
                          WHITESPACE@31..32 " "
                          WILDCARD_PAT@32..33
                            HOLE@32..33 "_"
                          R_BRACKET@33..34 "]"
                        R_BRACKET@34..35 "]"
                      WHITESPACE@35..36 " "
                      EQ@36..37 "="
                      WHITESPACE@37..38 " "
                      PATH_EXPR@38..39
                        NAME_REF@38..39
                          IDENT@38..39 "v"
                      SEMICOLON@39..40 ";"
                    WHITESPACE@40..41 " "
                    LET_STMT@41..62
                      LET_KW@41..44 "let"
                      WHITESPACE@44..45 " "
                      VARIANT_PAT@45..57
                        COLON2@45..47 "::"
                        NAME_REF@47..51
                          IDENT@47..51 "Char"
                        L_PAREN@51..52 "("
                        BIND_PAT@52..53
                          NAME@52..53
                            IDENT@52..53 "c"
                        COMMA@53..54 ","
                        WHITESPACE@54..55 " "
                        BIND_PAT@55..56
                          NAME@55..56
                            HOLE@55..56 "_"
                        R_PAREN@56..57 ")"
                      WHITESPACE@57..58 " "
                      EQ@58..59 "="
                      WHITESPACE@59..60 " "
                      PATH_EXPR@60..61
                        NAME_REF@60..61
                          IDENT@60..61 "s"
                      SEMICOLON@61..62 ";"
                    WHITESPACE@62..63 " "
                    LET_STMT@63..73
                      LET_KW@63..66 "let"
                      WHITESPACE@66..67 " "
                      LITERAL_PAT@67..68
                        LITERAL@67..68
                          INT_NUMBER@67..68 "0"
                      WHITESPACE@68..69 " "
                      EQ@69..70 "="
                      WHITESPACE@70..71 " "
                      PATH_EXPR@71..72
                        NAME_REF@71..72
                          IDENT@71..72 "n"
                      SEMICOLON@72..73 ";"
                    WHITESPACE@73..74 " "
                    R_BRACE@74..75 "}"
                SEMICOLON@75..76 ";"
        "#]],
    );
}

#[test]
fn a_broken_array_pattern_recovers_at_the_let() {
    check_errors(
        "static f = fn { let [a b] = v; let [a, = w; let ok = 1; };",
        expect![[r#"
            23..24: expected `,` or `]`
            37..38: expected `]`
            39..40: expected a pattern
        "#]],
    );
}

#[test]
fn character_literals_lex_as_one_token_each() {
    // Every well-formed shape: plain ASCII, an escape, the escaped quote,
    // and a multi-byte scalar value (one token whose LENGTH is its UTF-8
    // width — `'æ'` is four bytes, not three).
    check(
        r"static x = fn { let a = 'a'; let b = '\n'; let c = '\''; let d = 'æ'; };",
        expect![[r#"
            SOURCE_FILE@0..73
              STATIC_ITEM@0..73
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..72
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..72
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..28
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      LITERAL@24..27
                        CHAR@24..27 "'a'"
                      SEMICOLON@27..28 ";"
                    WHITESPACE@28..29 " "
                    LET_STMT@29..42
                      LET_KW@29..32 "let"
                      WHITESPACE@32..33 " "
                      BIND_PAT@33..34
                        NAME@33..34
                          IDENT@33..34 "b"
                      WHITESPACE@34..35 " "
                      EQ@35..36 "="
                      WHITESPACE@36..37 " "
                      LITERAL@37..41
                        CHAR@37..41 "'\\n'"
                      SEMICOLON@41..42 ";"
                    WHITESPACE@42..43 " "
                    LET_STMT@43..56
                      LET_KW@43..46 "let"
                      WHITESPACE@46..47 " "
                      BIND_PAT@47..48
                        NAME@47..48
                          IDENT@47..48 "c"
                      WHITESPACE@48..49 " "
                      EQ@49..50 "="
                      WHITESPACE@50..51 " "
                      LITERAL@51..55
                        CHAR@51..55 "'\\''"
                      SEMICOLON@55..56 ";"
                    WHITESPACE@56..57 " "
                    LET_STMT@57..70
                      LET_KW@57..60 "let"
                      WHITESPACE@60..61 " "
                      BIND_PAT@61..62
                        NAME@61..62
                          IDENT@61..62 "d"
                      WHITESPACE@62..63 " "
                      EQ@63..64 "="
                      WHITESPACE@64..65 " "
                      LITERAL@65..69
                        CHAR@65..69 "'æ'"
                      SEMICOLON@69..70 ";"
                    WHITESPACE@70..71 " "
                    R_BRACE@71..72 "}"
                SEMICOLON@72..73 ";"
        "#]],
    );
}

#[test]
fn malformed_character_literals_say_which_shape_is_wrong() {
    // Each diagnostic names the specific mistake — an empty literal, one
    // holding a whole word (the "you meant a string" case), and an escape
    // the table doesn't have.
    check_errors(
        r"static x = fn { let a = ''; let b = 'ab'; let c = '\q'; };",
        expect![[r#"
            24..26: empty character literal: a character literal holds exactly one character
            36..40: a character literal holds exactly one character; use a string (`"..."`) to hold more
            50..54: unknown escape sequence `\q`
        "#]],
    );
}

#[test]
fn a_character_literal_is_a_match_pattern() {
    check(
        "static x = fn (c) { match c { '(' => 1, _ => 0, } };",
        expect![[r#"
            SOURCE_FILE@0..52
              STATIC_ITEM@0..52
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..51
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..17
                    L_PAREN@14..15 "("
                    PARAM@15..16
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "c"
                    R_PAREN@16..17 ")"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..51
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..20 " "
                    MATCH_EXPR@20..49
                      MATCH_KW@20..25 "match"
                      WHITESPACE@25..26 " "
                      PATH_EXPR@26..27
                        NAME_REF@26..27
                          IDENT@26..27 "c"
                      WHITESPACE@27..28 " "
                      L_BRACE@28..29 "{"
                      WHITESPACE@29..30 " "
                      MATCH_ARM@30..39
                        LITERAL_PAT@30..33
                          LITERAL@30..33
                            CHAR@30..33 "'('"
                        WHITESPACE@33..34 " "
                        FAT_ARROW@34..36 "=>"
                        WHITESPACE@36..37 " "
                        LITERAL@37..38
                          INT_NUMBER@37..38 "1"
                        COMMA@38..39 ","
                      WHITESPACE@39..40 " "
                      MATCH_ARM@40..47
                        WILDCARD_PAT@40..41
                          HOLE@40..41 "_"
                        WHITESPACE@41..42 " "
                        FAT_ARROW@42..44 "=>"
                        WHITESPACE@44..45 " "
                        LITERAL@45..46
                          INT_NUMBER@45..46 "0"
                        COMMA@46..47 ","
                      WHITESPACE@47..48 " "
                      R_BRACE@48..49 "}"
                    WHITESPACE@49..50 " "
                    R_BRACE@50..51 "}"
                SEMICOLON@51..52 ";"
        "#]],
    );
}

#[test]
fn a_backslash_at_end_of_line_does_not_escape_the_newline() {
    // The line bound has to survive the escape rule, or it is not a bound:
    // a backslash consumes the character after it, and consuming a NEWLINE
    // would carry the scan onto the next line and eat a whole innocent
    // statement. `let d = 'y';` below must lex exactly as it would with no
    // broken literal above it — the errors stay on line one.
    check_errors(
        "static x = fn { let c = '\\\n    let d = 'y'; };",
        expect![[r#"
            24..25: unterminated character literal: expected a closing `'`
            25..26: unexpected character `\`
        "#]],
    );
}

#[test]
fn two_retired_lifetimes_on_one_line_pair_up() {
    // The line bound is on the LINE, not on the token count: two odd
    // quotes pair into one literal. Two retired `'a` lifetimes in one
    // parameter list therefore recover poorly — the first quote closes on
    // the second, swallowing the parameter between them, and what is left
    // is recovered as further (garbage) parameters.
    //
    // Pinned rather than fixed. Every cheap heuristic that breaks the
    // pairing (stop at whitespace, cap the body length) costs the "you
    // meant a string" message for `'hello world'` — a mistake people
    // actually make — to improve a signature that is doubly retired syntax
    // and is being told to rewrite anyway.
    check_errors(
        "static f = fn(a: &'a str, b: &'b str) -> () { };",
        expect![[r#"
            17..18: borrow types are spelled postfix: `T.&` / `T.&mut`
            18..31: a character literal holds exactly one character; use a string (`"..."`) to hold more
            31..32: expected `,` or `)`
            33..36: expected `,` or `)`
        "#]],
    );
}

#[test]
fn reserved_escapes_say_reserved_not_unknown() {
    // `\u{...}` and byte escapes have design room held for them, so
    // "unknown escape sequence" would be a lie: it tells the user to go
    // find another spelling when this one is already spoken for. Both
    // literal forms answer identically — one table, one answer.
    check_errors(
        r#"static x = fn { let a = '\u{41}'; let b = "\u{41}"; let c = '\x41'; };"#,
        expect![[r#"
            24..32: `\u{...}` escapes are not supported yet
            43..45: `\u{...}` escapes are not supported yet
            60..66: byte escapes (`\xNN`) are not supported yet
        "#]],
    );
}

#[test]
fn non_scalar_literal_patterns_are_refused_by_kind() {
    // The grammar takes every literal kind so this message can name the
    // one that was written; the two SCALARS (`char`, integers) have
    // pattern semantics — `0 => 1` is silent here now — and the other two
    // still do not.
    check_errors(
        "static x = fn (n) { match n { 0 => 1, \"s\" => 2, true => 3, _ => 0, } };",
        expect![[r#"
            38..41: string literal patterns are not supported yet; only character (`'x'`) and integer (`0`) literals can be matched
            48..52: boolean literal patterns are not supported yet; only character (`'x'`) and integer (`0`) literals can be matched
        "#]],
    );
}

#[test]
fn a_negative_integer_is_a_literal_pattern() {
    // The `-` belongs to the pattern, beside its literal, not to an
    // operator expression around it.
    check(
        "static x = fn (n) { match n { -1 => 1, _ => 0, } };",
        expect![[r#"
            SOURCE_FILE@0..51
              STATIC_ITEM@0..51
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..50
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..17
                    L_PAREN@14..15 "("
                    PARAM@15..16
                      BIND_PAT@15..16
                        NAME@15..16
                          IDENT@15..16 "n"
                    R_PAREN@16..17 ")"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..50
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..20 " "
                    MATCH_EXPR@20..48
                      MATCH_KW@20..25 "match"
                      WHITESPACE@25..26 " "
                      PATH_EXPR@26..27
                        NAME_REF@26..27
                          IDENT@26..27 "n"
                      WHITESPACE@27..28 " "
                      L_BRACE@28..29 "{"
                      WHITESPACE@29..30 " "
                      MATCH_ARM@30..38
                        LITERAL_PAT@30..32
                          MINUS@30..31 "-"
                          LITERAL@31..32
                            INT_NUMBER@31..32 "1"
                        WHITESPACE@32..33 " "
                        FAT_ARROW@33..35 "=>"
                        WHITESPACE@35..36 " "
                        LITERAL@36..37
                          INT_NUMBER@36..37 "1"
                        COMMA@37..38 ","
                      WHITESPACE@38..39 " "
                      MATCH_ARM@39..46
                        WILDCARD_PAT@39..40
                          HOLE@39..40 "_"
                        WHITESPACE@40..41 " "
                        FAT_ARROW@41..43 "=>"
                        WHITESPACE@43..44 " "
                        LITERAL@44..45
                          INT_NUMBER@44..45 "0"
                        COMMA@45..46 ","
                      WHITESPACE@46..47 " "
                      R_BRACE@47..48 "}"
                    WHITESPACE@48..49 " "
                    R_BRACE@49..50 "}"
                SEMICOLON@50..51 ";"
        "#]],
    );
}

#[test]
fn a_negative_integer_binding_pattern_parses() {
    // `let`/parameter patterns share the literal pattern.
    check_errors("static x = fn () { let -1 = 0; };", expect![[r#""#]]);
}

#[test]
fn only_an_integer_takes_a_minus_in_a_pattern() {
    // A sign before anything else is still no pattern, so the parser's own
    // "expected a pattern" is the whole answer.
    check_errors(
        "static x = fn (n) { match n { -'a' => 1, -x => 2, _ => 0, } };",
        expect![[r#"
            30..31: expected a pattern
            41..42: expected a pattern
        "#]],
    );
}

#[test]
fn a_range_pattern_does_not_parse() {
    // Ranges are reserved for their own reason (G19: exhaustiveness, not
    // spelling) and are not in the superset parse either, so `0..=9`
    // cascades into the parser's ordinary recovery.
    check_errors(
        "static x = fn (n) { match n { 0..=9 => 1, _ => 0, } };",
        expect![[r#"
            31..33: expected `=>`
            33..34: expected `,`
        "#]],
    );
}
