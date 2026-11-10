//! The bare-angle `f<T>` heuristic and its `::` fix.

use super::*;

// ---- bare-angle generics (the missing-turbofish typo) -------------------
//
// `Option<T>` for `Option::<T>` (Rust muscle memory) is the most
// predictable typo the language has, and a permanent one: G06 rules bare
// angles illegal for good. X03 then says what to do with a
// never-legal-but-recognizable form — parse it into its real tree shape and
// CORRECT it later — so `Option<T>` builds the very `GENERIC_ARG_LIST`
// `Option::<T>` builds, hover and inference keep working inside the
// arguments, and `validation::reject_bare_angle_generic_args` names the
// missing `::` with a one-token fix. Type position takes the form
// unconditionally; expression position gates on
// `grammar::bare_angle_generic_args_expr`, since `<` is a real operator
// there.

#[test]
fn bare_angle_return_type_parses_as_a_turbofish() {
    // The whole shape, in return-type position: a plain `GENERIC_ARG_LIST`
    // where the turbofish's would be, one diagnostic, and the enclosing
    // `fn`'s body parsed on undisturbed.
    check(
        "static f = fn () -> Option<usize> { 0 };",
        expect![[r#"
            SOURCE_FILE@0..40
              STATIC_ITEM@0..40
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "f"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..39
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  PARAM_LIST@14..16
                    L_PAREN@14..15 "("
                    R_PAREN@15..16 ")"
                  WHITESPACE@16..17 " "
                  RET_TYPE@17..33
                    THIN_ARROW@17..19 "->"
                    WHITESPACE@19..20 " "
                    PATH_TYPE@20..33
                      NAME_REF@20..26
                        IDENT@20..26 "Option"
                      GENERIC_ARG_LIST@26..33
                        L_ANGLE@26..27 "<"
                        TYPE_ARG@27..32
                          PATH_TYPE@27..32
                            NAME_REF@27..32
                              IDENT@27..32 "usize"
                        R_ANGLE@32..33 ">"
                  WHITESPACE@33..34 " "
                  BLOCK_EXPR@34..39
                    L_BRACE@34..35 "{"
                    WHITESPACE@35..36 " "
                    LITERAL@36..37
                      INT_NUMBER@36..37 "0"
                    WHITESPACE@37..38 " "
                    R_BRACE@38..39 "}"
                SEMICOLON@39..40 ";"
            error 20..33: generic arguments use the turbofish: write `Option::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_type_position_every_slot_gets_one_diagnostic() {
    // After `->`, after `:` in an item annotation, a field, a param, and
    // inside a correctly-spelled turbofish's own `TYPE_ARG`. Every slot
    // funnels through the same `type_core` arm, so one rule covers all of
    // them; each of these used to cascade from the stray `<`.
    for input in [
        "static f = fn () -> Option<usize> { 0 };",
        "static f = fn (x: Option<usize>) -> usize { 0 };",
        "type R = struct { x: Option<usize> };",
        "static x: Option<usize> = y;",
        "static x: Pair::<Option<usize>> = y;",
    ] {
        let parse = crate::parse(input);
        let errors = parse.errors();
        assert_eq!(
            errors.len(),
            1,
            "`{input}` should get exactly one diagnostic, got {errors:?}"
        );
        assert_eq!(
            errors[0].message, "generic arguments use the turbofish: write `Option::<...>`",
            "`{input}`"
        );
    }
}

#[test]
fn bare_angle_offers_an_insert_colon2_fix() {
    let parse = crate::parse("static x: Option<usize> = y;");
    let errors = parse.errors();
    assert_eq!(errors.len(), 1);
    let fix = errors[0].fix.as_ref().expect("a fix");
    assert_eq!(fix.label, "Insert `::`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "::");
    // Empty range right after the name, before the bad `<`: a pure
    // insertion, `Option` -> `Option::`.
    assert_eq!(
        fix.edits[0].range,
        crate::TextRange::empty(crate::TextSize::from(16))
    );
}

#[test]
fn bare_angle_nested_generics_two_independent_hints() {
    // `Vec<Vec<T>>` is two typos, so it earns two hints: the arguments are
    // parsed for real, so the inner `Vec<T>` reaches the same `type_core`
    // arm as a `TYPE_ARG` of its own. Each hint carries its own one-token
    // fix and the two apply independently — the user is never told to fix
    // one, re-run, and discover the other.
    check(
        "static x: Vec<Vec<T>> = y;",
        expect![[r#"
            SOURCE_FILE@0..26
              STATIC_ITEM@0..26
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                COLON@8..9 ":"
                WHITESPACE@9..10 " "
                PATH_TYPE@10..21
                  NAME_REF@10..13
                    IDENT@10..13 "Vec"
                  GENERIC_ARG_LIST@13..21
                    L_ANGLE@13..14 "<"
                    TYPE_ARG@14..20
                      PATH_TYPE@14..20
                        NAME_REF@14..17
                          IDENT@14..17 "Vec"
                        GENERIC_ARG_LIST@17..20
                          L_ANGLE@17..18 "<"
                          TYPE_ARG@18..19
                            PATH_TYPE@18..19
                              NAME_REF@18..19
                                IDENT@18..19 "T"
                          R_ANGLE@19..20 ">"
                    R_ANGLE@20..21 ">"
                WHITESPACE@21..22 " "
                EQ@22..23 "="
                WHITESPACE@23..24 " "
                PATH_EXPR@24..25
                  NAME_REF@24..25
                    IDENT@24..25 "y"
                SEMICOLON@25..26 ";"
            error 10..21: generic arguments use the turbofish: write `Vec::<...>`
            error 14..20: generic arguments use the turbofish: write `Vec::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_expr_call_heuristic_fires() {
    // Expression position, positive: `f<T>(...)` — a balanced group
    // immediately followed by `(`. The tree is exactly `f::<usize>(3)`'s,
    // minus the `COLON2`.
    check(
        "static x = f<usize>(3);",
        expect![[r#"
            SOURCE_FILE@0..23
              STATIC_ITEM@0..23
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                CALL_EXPR@11..22
                  PATH_EXPR@11..19
                    NAME_REF@11..12
                      IDENT@11..12 "f"
                    GENERIC_ARG_LIST@12..19
                      L_ANGLE@12..13 "<"
                      TYPE_ARG@13..18
                        PATH_TYPE@13..18
                          NAME_REF@13..18
                            IDENT@13..18 "usize"
                      R_ANGLE@18..19 ">"
                  ARG_LIST@19..22
                    L_PAREN@19..20 "("
                    LITERAL@20..21
                      INT_NUMBER@20..21 "3"
                    R_PAREN@21..22 ")"
                SEMICOLON@22..23 ";"
            error 11..19: generic arguments use the turbofish: write `f::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_expr_assoc_heuristic_keeps_the_segment() {
    // Expression position, positive: `f<T>::assoc` — a balanced group
    // followed by `::`, the other trusted shape. The trailing segment gets
    // the same handling `f::<usize>::assoc` gets (its own `NAME_REF` and
    // its own possible turbofish), and is left out of the hint's range.
    check(
        "static x = f<usize>::assoc;",
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
                PATH_EXPR@11..26
                  NAME_REF@11..12
                    IDENT@11..12 "f"
                  GENERIC_ARG_LIST@12..19
                    L_ANGLE@12..13 "<"
                    TYPE_ARG@13..18
                      PATH_TYPE@13..18
                        NAME_REF@13..18
                          IDENT@13..18 "usize"
                    R_ANGLE@18..19 ">"
                  COLON2@19..21 "::"
                  NAME_REF@21..26
                    IDENT@21..26 "assoc"
                SEMICOLON@26..27 ";"
            error 11..19: generic arguments use the turbofish: write `f::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_comparison_untouched() {
    // Expression position, negative: a plain `a < b` must never get the
    // hint — `scan_bare_angle_group` never even confirms a balanced group
    // (the statement's `;` bails it), so this is a pure no-op.
    check(
        "static x = fn { let y = a < b; y };",
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
                FN_LITERAL@11..34
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..34
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..30
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "y"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      BIN_EXPR@24..29
                        PATH_EXPR@24..25
                          NAME_REF@24..25
                            IDENT@24..25 "a"
                        WHITESPACE@25..26 " "
                        L_ANGLE@26..27 "<"
                        WHITESPACE@27..28 " "
                        PATH_EXPR@28..29
                          NAME_REF@28..29
                            IDENT@28..29 "b"
                      SEMICOLON@29..30 ";"
                    WHITESPACE@30..31 " "
                    PATH_EXPR@31..32
                      NAME_REF@31..32
                        IDENT@31..32 "y"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn bare_angle_call_comparison_untouched() {
    // Expression position, negative: `f(x) < g(y)`. The gate only fires
    // right after a bare NAME in `primary_expr`'s `IDENT` arm, before any
    // call parens are parsed, so this `<` is never even offered to it.
    check(
        "static x = fn { let z = f(x) < g(y); z };",
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
                FN_LITERAL@11..40
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..40
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..36
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "z"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      BIN_EXPR@24..35
                        CALL_EXPR@24..28
                          PATH_EXPR@24..25
                            NAME_REF@24..25
                              IDENT@24..25 "f"
                          ARG_LIST@25..28
                            L_PAREN@25..26 "("
                            PATH_EXPR@26..27
                              NAME_REF@26..27
                                IDENT@26..27 "x"
                            R_PAREN@27..28 ")"
                        WHITESPACE@28..29 " "
                        L_ANGLE@29..30 "<"
                        WHITESPACE@30..31 " "
                        CALL_EXPR@31..35
                          PATH_EXPR@31..32
                            NAME_REF@31..32
                              IDENT@31..32 "g"
                          ARG_LIST@32..35
                            L_PAREN@32..33 "("
                            PATH_EXPR@33..34
                              NAME_REF@33..34
                                IDENT@33..34 "y"
                            R_PAREN@34..35 ")"
                      SEMICOLON@35..36 ";"
                    WHITESPACE@36..37 " "
                    PATH_EXPR@37..38
                      NAME_REF@37..38
                        IDENT@37..38 "z"
                    WHITESPACE@38..39 " "
                    R_BRACE@39..40 "}"
                SEMICOLON@40..41 ";"
        "#]],
    );
}

#[test]
fn bare_angle_comparison_chain_untouched() {
    // Expression position, negative: `a < b == c > (d)` closes an angle
    // group and IS followed by `(`, so shape alone would fire. The
    // equality/ordering operators can never appear in a generic argument,
    // so finding one at nest 0 says "comparison chain" outright.
    check(
        "static x = fn { let z = a < b == c > (d); z };",
        expect![[r#"
            SOURCE_FILE@0..46
              STATIC_ITEM@0..46
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..45
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..45
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..41
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "z"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      BIN_EXPR@24..40
                        BIN_EXPR@24..34
                          BIN_EXPR@24..29
                            PATH_EXPR@24..25
                              NAME_REF@24..25
                                IDENT@24..25 "a"
                            WHITESPACE@25..26 " "
                            L_ANGLE@26..27 "<"
                            WHITESPACE@27..28 " "
                            PATH_EXPR@28..29
                              NAME_REF@28..29
                                IDENT@28..29 "b"
                          WHITESPACE@29..30 " "
                          EQ2@30..32 "=="
                          WHITESPACE@32..33 " "
                          PATH_EXPR@33..34
                            NAME_REF@33..34
                              IDENT@33..34 "c"
                        WHITESPACE@34..35 " "
                        R_ANGLE@35..36 ">"
                        WHITESPACE@36..37 " "
                        PAREN_EXPR@37..40
                          L_PAREN@37..38 "("
                          PATH_EXPR@38..39
                            NAME_REF@38..39
                              IDENT@38..39 "d"
                          R_PAREN@39..40 ")"
                      SEMICOLON@40..41 ";"
                    WHITESPACE@41..42 " "
                    PATH_EXPR@42..43
                      NAME_REF@42..43
                        IDENT@42..43 "z"
                    WHITESPACE@43..44 " "
                    R_BRACE@44..45 "}"
                SEMICOLON@45..46 ";"
        "#]],
    );
}

#[test]
fn bare_angle_ordering_chain_with_parens_is_read_as_a_call() {
    // The residual of the heuristic, and the only shape a comma cannot
    // separate: `a < b > (d)` is token-for-token `f::<T>(d)`. It is read as
    // the call because no well-typed program is spelled that way (`a < b`
    // is a `bool`, and `>` wants numbers), so nothing that compiles is at
    // stake — but the misread does cost the whole expression, not just this
    // message: see `hir`'s `bare_angle_ordering_chain_is_read_as_a_call`.
    assert_eq!(
        messages("static x = fn { let z = a < b > (d); z };"),
        ["generic arguments use the turbofish: write `a::<...>`"]
    );
}

#[test]
fn bare_angle_top_level_comma_keeps_the_comparison_reading() {
    // Why the call shape refuses a top-level comma: `g(a < b, c > (d))` is
    // two comparisons passed as arguments — a program that compiles — and
    // is otherwise token-for-token a one-argument call on `a::<b, c>`. The
    // price is that a MULTI-argument bare-angle call goes uncorrected;
    // that shape is exactly the ambiguous one.
    let valid =
        "static n = fn (a: usize, b: usize, c: usize, d: usize) -> bool { g(a < b, c > (d)) };";
    assert_eq!(messages(valid), Vec::<String>::new());
    assert!(!node_kinds(valid).contains(&"GENERIC_ARG_LIST".to_owned()));
    assert_eq!(
        messages("static u = fn { p<usize, usize>(1, 2) };")
            .iter()
            .filter(|message| message.contains("turbofish"))
            .count(),
        0
    );
}

#[test]
fn bare_angle_angles_inside_a_pair_close_nothing() {
    // The other half of the same rule: a `>` inside a pair opened after the
    // `<` is a comparison in someone else's expression, not this group's
    // close. Counting it would read `a < f(b >` as the group and correct a
    // program that compiles — commas cannot help, since there is none.
    let valid = "static n = fn (a: usize, b: usize, c: usize) -> bool { a < f(b > (c)) };";
    assert_eq!(messages(valid), Vec::<String>::new());
    assert!(!node_kinds(valid).contains(&"GENERIC_ARG_LIST".to_owned()));
}

#[test]
fn bare_angle_match_arms_untouched() {
    // Match arms need both tells. The call shape is caught by the arm
    // separator being a top-level comma; the `::` shape, which accepts
    // commas, is caught by the `=>` itself — an arm arrow can never appear
    // in an argument list.
    for input in [
        "static m = fn (k: usize, a: usize, b: usize) -> bool { match k { 0 => a < b, _ => a > (b + 1) } };",
        "static m = fn (k: usize, a: usize, b: usize) -> usize { match k { 0 => a < b, _ => b > ::Foo } };",
    ] {
        assert!(!node_kinds(input).contains(&"GENERIC_ARG_LIST".to_owned()));
    }
}

#[test]
fn bare_angle_assoc_shape_takes_a_comma() {
    // The `::` follow-shape has no comparison reading at all (nothing valid
    // puts `::` after one), so it corrects the multi-argument lists the
    // call shape has to leave alone.
    assert_eq!(
        messages("static x = Pair<A, B>::first;"),
        ["generic arguments use the turbofish: write `Pair::<...>`"]
    );
}

#[test]
fn bare_angle_unclosed_group_parses_as_the_turbofish_would() {
    // The typo is committed to, not bailed out of, when the group never
    // closes: `generic_arg_list`'s own no-progress break and
    // `expect_after_prev(R_ANGLE)` bound `Boxed<usize` exactly where they
    // bound `Boxed::<usize`, so mid-typing behaves the same on both
    // spellings — the same tree and the same errors, plus the correction.
    assert_eq!(
        node_kinds("static f = fn (b: Boxed<usize) -> usize { b.value };"),
        without_colon2(node_kinds(
            "static f = fn (b: Boxed::<usize) -> usize { b.value };"
        ))
    );
    assert_eq!(
        messages("static f = fn (b: Boxed<usize) -> usize { b.value };"),
        prepend(
            "generic arguments use the turbofish: write `Boxed::<...>`",
            messages("static f = fn (b: Boxed::<usize) -> usize { b.value };")
        )
    );
}

#[test]
fn bare_angle_junk_arguments_report_as_the_turbofish_would() {
    // Same trade in the other direction: because the arguments are parsed
    // rather than swallowed, junk inside them is reported exactly as the
    // turbofish spelling reports it, instead of hiding behind the typo.
    assert_eq!(
        node_kinds("static x: Boxed<N + 1> = y;"),
        without_colon2(node_kinds("static x: Boxed::<N + 1> = y;"))
    );
    assert_eq!(
        messages("static x: Boxed<N + 1> = y;"),
        prepend(
            "generic arguments use the turbofish: write `Boxed::<...>`",
            messages("static x: Boxed::<N + 1> = y;")
        )
    );
}

#[test]
fn bare_angle_typo_survives_at_the_member_boundary() {
    // A bad member's typo must not fall the enclosing `impl` out of the
    // parser: the member after it parses clean.
    let parse = crate::parse(
        "type Pair = struct { a: usize } with { \
         impl Self { \
         bad = fn () -> Option<usize> { 0 }; \
         after = fn (s: Self) -> usize { s.a }; \
         } };",
    );
    assert_eq!(
        parse.errors().len(),
        1,
        "expected only the turbofish hint, got {:?}",
        parse.errors()
    );
    assert_eq!(
        parse.errors()[0].message,
        "generic arguments use the turbofish: write `Option::<...>`"
    );
    let dump = parse.debug_dump();
    assert_eq!(
        dump.matches("MEMBER@").count(),
        2,
        "`after` did not survive as its own member:\n{dump}"
    );
    assert!(
        dump.contains("\"after\""),
        "`after`'s name did not survive:\n{dump}"
    );
}

#[test]
fn bare_angle_typo_survives_at_the_item_boundary() {
    // One level up: a bad ITEM's typo must not fall the rest of the file
    // out of the parser.
    let parse = crate::parse(
        "type Bad = struct { x: Option<usize> };\n\
         static after = 1;\n",
    );
    assert_eq!(
        parse.errors().len(),
        1,
        "expected only the turbofish hint, got {:?}",
        parse.errors()
    );
    let dump = parse.debug_dump();
    assert!(
        dump.contains("STATIC_ITEM") && dump.contains("\"after\""),
        "the item after the bad one did not survive:\n{dump}"
    );
}
