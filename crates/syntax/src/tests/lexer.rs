//! Tokens: strings and escapes, block comments, the keyword tables.

use super::*;

#[test]
fn unterminated_string() {
    check(
        r#"static s = "hello;"#,
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..18
                  STRING@11..18 "\"hello;"
            error 11..18: unterminated string
        "#]],
    );
}

#[test]
fn multiline_string_literal() {
    // Strings are multiline, Rust-style: a closing quote on a later line is
    // a valid literal, not two unterminated-string errors.
    check(
        r#"
static name = fn {
    let s = "haha
    ";
}
"#,
        expect![[r#"
            SOURCE_FILE@0..47
              WHITESPACE@0..1 "\n"
              STATIC_ITEM@1..46
                ITEM_HEAD@1..7
                  STATIC_KW@1..7 "static"
                WHITESPACE@7..8 " "
                NAME@8..12
                  IDENT@8..12 "name"
                WHITESPACE@12..13 " "
                EQ@13..14 "="
                WHITESPACE@14..15 " "
                FN_LITERAL@15..46
                  FN_KW@15..17 "fn"
                  WHITESPACE@17..18 " "
                  BLOCK_EXPR@18..46
                    L_BRACE@18..19 "{"
                    WHITESPACE@19..24 "\n    "
                    LET_STMT@24..44
                      LET_KW@24..27 "let"
                      WHITESPACE@27..28 " "
                      BIND_PAT@28..29
                        NAME@28..29
                          IDENT@28..29 "s"
                      WHITESPACE@29..30 " "
                      EQ@30..31 "="
                      WHITESPACE@31..32 " "
                      LITERAL@32..43
                        STRING@32..43 "\"haha\n    \""
                      SEMICOLON@43..44 ";"
                    WHITESPACE@44..45 "\n"
                    R_BRACE@45..46 "}"
              WHITESPACE@46..47 "\n"
        "#]],
    );
}

#[test]
fn escaped_quote_does_not_end_the_string() {
    // The token boundary is what escape handling is load-bearing for at the
    // lexer level: `\"` keeps the literal open, so this is ONE string.
    check(
        r#"static s = "a\"b";"#,
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..17
                  STRING@11..17 "\"a\\\"b\""
                SEMICOLON@17..18 ";"
        "#]],
    );
}

#[test]
fn every_known_escape_is_accepted() {
    // The whole conventional set, in one literal: no diagnostics.
    let parse = crate::parse(r#"static s = "\n\t\r\0\\\"";"#);
    assert_eq!(parse.errors(), &[], "known escapes must not be diagnosed");
}

#[test]
fn unknown_escape_is_an_error_at_the_escape() {
    // Anchored at the two-character escape, NOT at the whole literal — the
    // rest of the string is fine and the squiggle should say so.
    check(
        r#"static s = "a\qb";"#,
        expect![[r#"
            SOURCE_FILE@0..18
              STATIC_ITEM@0..18
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..17
                  STRING@11..17 "\"a\\qb\""
                SEMICOLON@17..18 ";"
            error 13..15: unknown escape sequence `\q`
        "#]],
    );
}

#[test]
fn each_bad_escape_in_a_literal_is_reported() {
    let parse = crate::parse(r#"static s = "\q and \z";"#);
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "unknown escape sequence `\\q`",
            "unknown escape sequence `\\z`"
        ]
    );
}

#[test]
fn control_characters_in_the_escape_message_are_rendered_not_embedded() {
    // A backslash before a LITERAL newline (the C line-continuation idiom,
    // which Must does not have) must not split the diagnostic across two
    // lines, and a CR from a CRLF file must not travel raw inside an LSP
    // message. The range still covers the two source characters. Rendered
    // as a codepoint (`\u{a}`), never as `\n` — that two-character spelling
    // IS a valid escape, so naming it here would call something legal
    // unknown.
    let parse = crate::parse("static s = \"a\\\nb\";");
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["unknown escape sequence `\\u{a}`"]);
    assert_eq!(
        parse.errors()[0].range,
        crate::TextRange::new(13.into(), 15.into())
    );

    let parse = crate::parse("static s = \"a\\\r\nb\";");
    let messages: Vec<&str> = parse.errors().iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["unknown escape sequence `\\u{d}`"]);
    for err in parse.errors() {
        assert!(
            !err.message.contains('\n') && !err.message.contains('\r'),
            "a diagnostic must stay on one line: {:?}",
            err.message
        );
    }
}

#[test]
fn trailing_lone_backslash_is_an_error() {
    // Only reachable at end of input: before a closing quote the backslash
    // would have escaped the quote. Both errors are honest — the string is
    // unterminated *and* ends on a dangling escape.
    check(
        r#"static s = "abc\"#,
        expect![[r#"
            SOURCE_FILE@0..16
              STATIC_ITEM@0..16
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "s"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                LITERAL@11..16
                  STRING@11..16 "\"abc\\"
            error 11..16: unterminated string
            error 15..16: a string cannot end with a lone `\`
        "#]],
    );
}

#[test]
fn escapes_are_legal_inside_a_multiline_string() {
    // Strings stay multiline (unchanged ruling); a literal newline and an
    // escape coexist in one literal, and neither is diagnosed.
    let parse = crate::parse("static s = \"line one\\n\nline two\\t\";");
    assert_eq!(parse.errors(), &[]);
}

// ---- block comments (nesting, Rust's rule) -------------------------------
//
// `/* ... */` is a second spelling for the same `COMMENT` token kind `//`
// already produces (`scan_block_comment` in `lexer.rs`) — one kind, so
// trivia classification, semantic highlighting and "no completions inside a
// comment" all cover it with no edit of their own; see that function's doc
// comment for the nesting/unterminated rationale.

#[test]
fn a_block_comment_lexes_as_one_comment_token() {
    check(
        "/* hello */",
        expect![[r#"
            SOURCE_FILE@0..11
              COMMENT@0..11 "/* hello */"
        "#]],
    );
}

#[test]
fn nested_block_comments_close_at_the_matching_depth() {
    // The FIRST `*/` (after "inner") must not end the comment — it only
    // closes the inner `/*`, dropping back to depth one — so the real end
    // is the SECOND `*/`, and " still outer " stays commented out rather
    // than spilling into real tokens.
    check(
        "/* outer /* inner */ still outer */",
        expect![[r#"
            SOURCE_FILE@0..35
              COMMENT@0..35 "/* outer /* inner */  ..."
        "#]],
    );
}

#[test]
fn unterminated_block_comment_reports_its_one_opener() {
    // No nesting at all: the one `/*` there is, is also the only one — one
    // obligation, one diagnostic, at exactly those two characters, not the
    // whole (to-EOF) token.
    check_errors(
        "/* unterminated",
        expect![[r#"
            0..2: unterminated block comment: expected a closing `*/`
        "#]],
    );
}

#[test]
fn unterminated_nested_block_comments_report_every_still_open_opener() {
    // Both `/*`s are still open at EOF here — two INDEPENDENT obligations,
    // so two diagnostics, one per opener, outermost first (source order):
    // position 0 ("outer") and position 9 ("inner"). Naming only one
    // (either one) was considered and dropped — see G20 (Discarded, in
    // `docs/design/grammar-and-syntax.md`) — because report-all is
    // what makes a recovery loop see progress: closing either `/*` alone
    // makes exactly one of these two disappear.
    check_errors(
        "/* outer /* inner",
        expect![[r#"
            0..2: unterminated block comment: expected a closing `*/`
            9..11: unterminated block comment: expected a closing `*/`
        "#]],
    );
}

#[test]
fn closing_the_inner_comment_leaves_the_outer_open() {
    // A depth counter that only counted opens (never actually popping on a
    // close) would report the same TWO positions as the fully-open case
    // above. Here the inner DOES close — depth genuinely drops back to
    // one — so there is only ONE remaining obligation, reported once, at
    // the OUTER `/*` (position 0) — not the inner one that just closed,
    // and not a duplicate leftover from before it closed.
    check_errors(
        "/* outer /* inner */",
        expect![[r#"
            0..2: unterminated block comment: expected a closing `*/`
        "#]],
    );
}

#[test]
fn unterminated_block_comment_fix_lands_only_on_the_innermost_opener() {
    // The single-opener case: one obligation, and appending `*/` at EOF is
    // an unambiguous fix for it.
    let single = "/* unterminated";
    let parse = crate::parse(single);
    let err = parse
        .errors()
        .iter()
        .find(|e| e.message.starts_with("unterminated block comment"))
        .expect("expected the unterminated-comment diagnostic");
    let fix = err.fix.as_ref().expect("expected a fix on the sole opener");
    assert_eq!(fix.label, "Insert `*/`");
    assert_eq!(apply_fix(single, fix), "/* unterminated*/");

    // The two-opener case: the OUTER diagnostic gets no fix (there is no
    // principled position inside the file to insert its `*/` — the scan
    // cannot know where the user meant "outer" to end), and the INNER one
    // — the level actually still open when the file runs out — gets the
    // same EOF-append fix as the single-opener case above.
    let nested = "/* outer /* inner";
    let parse = crate::parse(nested);
    let mut unterminated = parse
        .errors()
        .iter()
        .filter(|e| e.message.starts_with("unterminated block comment"));
    let outer = unterminated.next().expect("expected the outer diagnostic");
    assert_eq!(outer.range.start(), crate::TextSize::new(0));
    assert!(
        outer.fix.is_none(),
        "the outer opener has no principled fix"
    );
    let inner = unterminated.next().expect("expected the inner diagnostic");
    assert_eq!(inner.range.start(), crate::TextSize::new(9));
    let fix = inner
        .fix
        .as_ref()
        .expect("expected a fix on the innermost opener");
    assert_eq!(fix.label, "Insert `*/`");
    assert_eq!(apply_fix(nested, fix), "/* outer /* inner*/");
}

#[test]
fn an_open_immediately_after_another_open_is_not_reused_as_a_close() {
    // The classic nesting gotcha: after the first `/*` (positions 0..2),
    // the scan resumes at position 2, so the `/*` starting there (2..4) is
    // a second, real nesting level — not "the same slash" doing double
    // duty. Its matching `*/` (4..6) only closes that inner level, leaving
    // the outer `/*` at position 0 open at EOF. (Matches Rust's own
    // scanner on the identical input.)
    check_errors(
        "/*/**/",
        expect![[r#"
            0..2: unterminated block comment: expected a closing `*/`
        "#]],
    );
    // Its two neighbours, decided by the same "resume after the pair"
    // rule: `/**/` is the empty comment — the scan resumes at 2 and the
    // `*/` there closes the one open level — and `/*/` is the near-miss,
    // where the only `*` is the opener's own, so no `*/` exists at all and
    // the level is still open at EOF.
    check(
        "/**/",
        expect![[r#"
            SOURCE_FILE@0..4
              COMMENT@0..4 "/**/"
        "#]],
    );
    check_errors(
        "/*/",
        expect![[r#"
            0..2: unterminated block comment: expected a closing `*/`
        "#]],
    );
}

#[test]
fn a_string_quote_does_not_start_inside_a_block_comment() {
    // `/* " */` is ONE comment token — the `"` in its body starts no
    // string, so there is neither a second (STRING) token nor an
    // "unterminated string" error trailing off the end of the file.
    check(
        r#"/* " */"#,
        expect![[r#"
            SOURCE_FILE@0..7
              COMMENT@0..7 "/* \" */"
        "#]],
    );
}

#[test]
fn a_block_comment_does_not_start_inside_a_string() {
    // The reverse direction: `"/*"` is a STRING (the lexer dispatches on
    // `"` before it ever asks whether the next two bytes are `/*`), so the
    // `/*` inside it starts no comment. Written in a real item position (a
    // bare string literal isn't a legal top-level item on its own) so the
    // tree stays clean and the point isn't buried under an unrelated
    // "expected an item" error.
    check(
        r#"static x = "/*";"#,
        expect![[r#"
            SOURCE_FILE@0..16
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
                  STRING@11..15 "\"/*\""
                SEMICOLON@15..16 ";"
        "#]],
    );
}

#[test]
fn a_block_comment_between_tokens_is_trivia() {
    check(
        "static x = fn { let a = /* five */ 5; a };",
        expect![[r#"
            SOURCE_FILE@0..42
              STATIC_ITEM@0..42
                ITEM_HEAD@0..6
                  STATIC_KW@0..6 "static"
                WHITESPACE@6..7 " "
                NAME@7..8
                  IDENT@7..8 "x"
                WHITESPACE@8..9 " "
                EQ@9..10 "="
                WHITESPACE@10..11 " "
                FN_LITERAL@11..41
                  FN_KW@11..13 "fn"
                  WHITESPACE@13..14 " "
                  BLOCK_EXPR@14..41
                    L_BRACE@14..15 "{"
                    WHITESPACE@15..16 " "
                    LET_STMT@16..37
                      LET_KW@16..19 "let"
                      WHITESPACE@19..20 " "
                      BIND_PAT@20..21
                        NAME@20..21
                          IDENT@20..21 "a"
                      WHITESPACE@21..22 " "
                      EQ@22..23 "="
                      WHITESPACE@23..24 " "
                      COMMENT@24..34 "/* five */"
                      WHITESPACE@34..35 " "
                      LITERAL@35..36
                        INT_NUMBER@35..36 "5"
                      SEMICOLON@36..37 ";"
                    WHITESPACE@37..38 " "
                    PATH_EXPR@38..39
                      NAME_REF@38..39
                        IDENT@38..39 "a"
                    WHITESPACE@39..40 " "
                    R_BRACE@40..41 "}"
                SEMICOLON@41..42 ";"
        "#]],
    );
}

#[test]
fn a_block_comment_inside_an_expression_is_trivia() {
    check(
        "static x = fn { 1 /* plus */ + 2 };",
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
                    BIN_EXPR@16..32
                      LITERAL@16..17
                        INT_NUMBER@16..17 "1"
                      WHITESPACE@17..18 " "
                      COMMENT@18..28 "/* plus */"
                      WHITESPACE@28..29 " "
                      PLUS@29..30 "+"
                      WHITESPACE@30..31 " "
                      LITERAL@31..32
                        INT_NUMBER@31..32 "2"
                    WHITESPACE@32..33 " "
                    R_BRACE@33..34 "}"
                SEMICOLON@34..35 ";"
        "#]],
    );
}

#[test]
fn every_fixed_token_spelling_lexes_back_to_its_kind() {
    use crate::SyntaxKind::*;
    // Every token kind has a spelling except the ones whose text varies, so
    // a new punctuation kind cannot be left out of `spelling`.
    for raw in 0..=ERROR_TOKEN as u16 {
        let kind = crate::SyntaxKind::from(rowan::SyntaxKind(raw));
        let varies = matches!(
            kind,
            TOMBSTONE
                | EOF
                | WHITESPACE
                | COMMENT
                | IDENT
                | INT_NUMBER
                | STRING
                | CHAR
                | REGION_IDENT
                | ERROR_TOKEN
        );
        let Some(spelling) = kind.spelling() else {
            assert!(varies, "{kind:?} has no `spelling`");
            continue;
        };
        assert!(!varies, "{kind:?} has a varying text but a `spelling`");
        let (tokens, _) = crate::tokenize(spelling);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [kind], "`{spelling}` does not lex as {kind:?}");
    }
}

// ---- keyword-table drift guards ---------------------------------------
//
// One canonical table (`syntax_kind::KEYWORDS`) generates `from_keyword`
// and `is_keyword`; these tests pin that nothing can drift away from it —
// including a `*_KW` kind added to the enum but forgotten in the table,
// which is exactly how `trait`/`requires` ended up unhighlighted.

#[test]
fn every_keyword_in_the_table_lexes_and_answers_is_keyword() {
    for &(text, kind) in crate::KEYWORDS {
        assert_eq!(
            crate::SyntaxKind::from_keyword(text),
            Some(kind),
            "`{text}` does not round-trip through `from_keyword`"
        );
        assert!(kind.is_keyword(), "{kind:?} is not `is_keyword`");
        // The lexer must actually produce the kind for that spelling.
        let (tokens, _) = crate::tokenize(text);
        assert_eq!(
            tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            vec![kind],
            "lexing `{text}` did not yield {kind:?}"
        );
        assert!(
            format!("{kind:?}").ends_with("_KW"),
            "{kind:?} is in the keyword table but is not a `*_KW` kind"
        );
    }
}

#[test]
fn every_kw_kind_is_in_the_canonical_keyword_table() {
    // Walks the whole `SyntaxKind` enum by discriminant: any variant whose
    // name ends in `_KW` must be reachable from the table, so adding a
    // keyword kind without a table entry fails here rather than silently
    // losing its highlighting.
    for raw in 0..=(crate::SyntaxKind::ERROR as u16) {
        let kind = crate::SyntaxKind::from(rowan::SyntaxKind(raw));
        if !format!("{kind:?}").ends_with("_KW") {
            continue;
        }
        assert!(
            crate::KEYWORDS.iter().any(|&(_, k)| k == kind),
            "{kind:?} is missing from the canonical keyword table \
             (`syntax_kind::keywords!`) — add its spelling there"
        );
        assert!(kind.is_keyword(), "{kind:?} is not `is_keyword`");
    }
}

#[test]
fn non_keyword_kinds_are_not_keywords() {
    for kind in [
        crate::SyntaxKind::IDENT,
        crate::SyntaxKind::COMMENT,
        crate::SyntaxKind::INT_NUMBER,
        crate::SyntaxKind::L_BRACE,
        crate::SyntaxKind::TRAIT_ITEM,
        crate::SyntaxKind::ERROR,
    ] {
        assert!(!kind.is_keyword(), "{kind:?} must not be `is_keyword`");
    }
    assert_eq!(crate::SyntaxKind::from_keyword("nonsense"), None);
    assert_eq!(crate::SyntaxKind::from_keyword("Self"), None);
}
