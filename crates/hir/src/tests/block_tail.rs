//! The block-tail continuation warning and its split fix.

use super::*;

// ---- the block-tail carve-out ------------------------------------------
//
// A statement whose expression ends in `}` needs no `;`. Both spellings
// produce the same `EXPR_STMT`, which the body lowering never records, so
// the HIR body is identical by construction — pinned below.

#[test]
fn a_self_terminated_block_tail_statement_is_the_semicolon_form_exactly() {
    // ZERO SEMANTIC DELTA. The `;` after a block-tail statement is a
    // spelling, not a boundary.
    //
    // The STRUCTURAL argument, which outlives this fixture: both spellings
    // produce an `EXPR_STMT`, `body.rs`'s block lowering maps every one of
    // them through the single arm `ast::Stmt::ExprStmt(it) =>
    // Stmt::Expr(self.lower_opt_expr(it.expr()))`, and that arm never hands
    // the `EXPR_STMT` node to `alloc_expr` — so the node is not in the
    // source map, does not become an `ExprId`, and cannot be observed by
    // anything downstream. The HIR body is identical BY CONSTRUCTION, and
    // the only thing a test can add is a demonstration that nothing else
    // (inference, joins, diagnostics) sneaks a look at the token.
    //
    // `@` stands in for the two spellings so both texts are the same LENGTH
    // and every range compared below means the same place in both.
    let template =
        "static f = fn (n: usize) -> () { if n == 0 { 1 } else { \"s\" }@ print(\"x\"); }";
    let with_semicolon = template.replace('@', ";");
    let self_terminated = template.replace('@', " ");
    let pairs = infer_pairs(&self_terminated);
    // Structured pairs, NOT the rendered form: the rendering carries a
    // source snippet, and comparing snippets would compare the very
    // characters that differ (see `infer_pairs`).
    assert_eq!(pairs, infer_pairs(&with_semicolon));
    assert_eq!(
        render_diagnostics(&with_semicolon),
        render_diagnostics(&self_terminated)
    );

    // Not vacuous, and the property is NAMED rather than merely present.
    // The block-tail statement's value is DISCARDED: the enclosing block
    // types as `()` — the fn's declared return — and not as whatever the
    // `if` produced. Asserted on both spellings, since "identical" is only
    // worth having if what they agree on is the right thing.
    let block = syntax::TextRange::new(
        u32::try_from(template.find("{ if n").unwrap())
            .unwrap()
            .into(),
        u32::try_from(template.len()).unwrap().into(),
    );
    for (spelling, pairs) in [
        (&self_terminated, &pairs),
        (&with_semicolon, &infer_pairs(&with_semicolon)),
    ] {
        assert!(
            pairs.iter().any(|(r, ty)| *r == block && ty == "()"),
            "the enclosing block should type as `()` in {spelling:?}, got {pairs:?}"
        );
    }
    // And the statement really is a JOIN in statement position — it
    // resolves as its own (an unresolvable tie here), never leaking into
    // the block's tail, identically in both spellings.
    assert!(render_diagnostics(&self_terminated).contains("type mismatch"));
}

// ---- the cross-line block-tail lint -----------------------------------
//
// A statement whose expression ends in `}` closes itself, and the grammar
// stays greedy across that brace. When the continuing token moved to its own
// line, the text says "new statement" and the grammar says "same
// expression" — the one place the two readings diverge visibly, and the one
// place a warning is owed.

#[test]
fn a_block_tail_continued_on_the_next_line_warns() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    if n == 0 { 2 } else { 3 }
    - 1
}
"#,
        expect![[r#"
            72..73: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_block_tail_continued_on_the_same_line_is_silent() {
    // Same line = deliberate. One expression, said plainly.
    check_diagnostics(
        "static f = fn (n: usize) -> usize { if n == 0 { 2 } else { 3 } - 1 }",
        expect![[r#""#]],
    );
}

#[test]
fn the_newline_is_looked_for_in_trivia_not_in_whitespace_shapes() {
    // A block comment now spans lines (comments nest, G20), so "is there a
    // line break between the `}` and the operator" is a question about the
    // TRIVIA, not about a whitespace token's shape. This one crosses a line
    // entirely inside a comment.
    check_diagnostics(
        r#"
static f = fn () -> usize {
    { 3 } /* two
    lines */ - 2
}
"#,
        expect![[r#"
            59..60: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_same_line_comment_between_the_brace_and_the_operator_stays_silent() {
    // The mirror: a comment that does not cross a line does not make one.
    check_diagnostics(
        "static f = fn () -> usize { { 3 } /* here */ - 2 }",
        expect![[r#""#]],
    );
}

#[test]
fn every_continuation_shape_is_seen_not_just_binary_operators() {
    // The lint's set is `expr_bp`'s continuation loop: binary operators,
    // calls, indexing, and the postfix `.` family.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    let a = [1, 2];
    { a }
    [n]
}
"#,
        expect![[r#"
            71..72: this `[` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `[` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_parenthesized_statement_after_a_block_tail_is_swallowed_as_a_call() {
    // The accepted price, named plainly: `(n)` on its own line does not
    // start a statement, it CALLS the block above it. This is exactly the
    // shape the warning exists for, and the `;` it offers is exactly the
    // fix — see `splitting_with_a_semicolon_silences_the_warning` for the
    // same program with the `;` in.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    if n == 0 { print("a") } else { print("b") }
    (n)
}
"#,
        expect![[r#"
            41..85: every branch produces `()`, but `fn(usize) -> usize` is needed
            90..91: this `(` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `(` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_block_tail_continued_inside_a_match_arm_warns() {
    // The identical trap, one level over: the `,` that would have split
    // these is the `;`'s counterpart, and without it this is ONE arm whose
    // body is `{ 1 } - 1`. Nothing else in the program complains — it types
    // fine — so the warning is the only thing standing between the reader
    // and the wrong arm.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        _ => { 1 }
        - 1,
    }
}
"#,
        expect![[r#"
            78..79: this `-` continues the expression that ends with the `}` above, rather than starting a new arm; write `,` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn the_warning_fires_in_expression_statement_position_too() {
    // The sibling of the block-tail cases above: here the continued
    // expression is a STATEMENT (a `;` follows it and the block goes on),
    // not the block's tail. Same ambiguity, same warning.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    if n == 0 { print("a") } else { print("b") }
    - 1;
    n
}
"#,
        expect![[r#"
            90..91: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
            92..93: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn the_postfix_dot_family_continues_a_block_tail_like_any_operator() {
    // `.` reaches the same loop `+` does — field access, `.*`, `.&`,
    // `.&raw` all continue whatever preceded them.
    check_diagnostics(
        r#"
static f = fn (p: usize.&raw) -> usize {
    unsafe { { p }
    .* }
}
"#,
        expect![[r#"
            65..66: this `.` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `.` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_field_access_on_the_next_line_continues_a_block_tail() {
    check_diagnostics(
        r#"
static f = fn () -> usize {
    { struct { a = 1 } }
    .a
}
"#,
        expect![[r#"
            58..59: this `.` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `.` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_prefix_operand_is_not_a_continuation() {
    // `&raw { x }` is a PREFIX address-of: its operand does not start where
    // the node does, so the `ADDR_OF_EXPR` is not itself a continuation —
    // it shares a node kind with the postfix `x.&raw`, and where the operand
    // starts is the only thing telling the two apart. (Both prefix spellings
    // are retired but still PARSE, which is exactly why the guard has to
    // hold on them.)
    //
    // Here the guard merely avoids reporting the same `-` twice, which the
    // ide layer's dedup would have hidden anyway. Its real stake is the
    // sibling below.
    check_diagnostics(
        r#"
static f = fn () -> usize {
    let x = 1;
    &raw { x }
    - 2
}
"#,
        expect![[r#"
            48..49: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
            48..58: type mismatch: expected `{number}`, found `{number}.&raw` (`-` requires `{number}` operands at 63..64)
            63..64: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_prefix_operand_never_reports_a_token_outside_its_own_expression() {
    // THE GUARD'S REAL STAKE. The trivia scan walks forward from the `}` to
    // the next non-trivia token and calls it "the continuation" — which is
    // only true because the node was established to BE a continuation of
    // that operand. Strip that guard and a prefix `&raw { x }` qualifies on
    // the strength of its operand's `}` alone, and the scan then happily
    // picks up whatever comes next in the FILE: here `print`, an IDENT,
    // which is not a continuation token at all and starts an ordinary
    // second statement.
    //
    // A nonsense accusation about a token that continues nothing — not a
    // duplicate, which the ide layer's dedup would have swallowed. The one
    // diagnostic below is the retired prefix spelling, unrelated; the
    // assertion is that the lint says NOTHING here.
    check_diagnostics(
        r#"
static f = fn () -> () {
    let x: usize = 1;
    &raw { x }
    print("a");
}
"#,
        expect![[r#"
            52..53: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
        "#]],
    );
}

#[test]
fn a_statement_starting_with_an_ampersand_after_a_block_tail_is_silent() {
    // The other half of the guard, and of the price: `&` is NOT in the
    // continuation set (there is no binary `&`), so a statement beginning
    // `&raw x` genuinely starts a statement and needs no `;` above it.
    check_diagnostics(
        r#"
static f = fn () -> () {
    let x = 1;
    { print("a") }
    &raw x;
}
"#,
        expect![[r#"
            38..39: cannot infer the type of this number: it has no defining use — add a type annotation
            64..65: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
        "#]],
    );
}

#[test]
fn a_chain_of_continuations_warns_once_per_ambiguity_point() {
    // Two line breaks, two places a reader could believe a statement ended,
    // two warnings. The climb in `at_statement_position` is what lets the
    // INNER one answer the position question at all — its own parent is
    // another continuation, not a statement.
    check_diagnostics(
        r#"
static f = fn () -> usize {
    { 1 }
    - { 2 }
    - 3
}
"#,
        expect![[r#"
            43..44: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
            55..56: this `-` continues the expression that ends with the `}` above, rather than starting a new statement; write `;` after that `}` to split them, or move the `-` up onto the same line (or parenthesize the whole expression) if one expression is what you meant
        "#]],
    );
}

#[test]
fn a_continued_block_tail_in_value_position_is_silent() {
    // A `let`'s initializer is a VALUE: no statement could have started
    // there, so there is no second reading and an operator-led continuation
    // line is ordinary formatting.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    let x = if n == 0 { 2 } else { 3 }
        - 1;
    x
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_continued_non_block_statement_is_silent() {
    // Nothing ends in `}`, so nothing ever looked like two statements.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    g(n)
    - 1
}
static g = fn (n: usize) -> usize { n };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn splitting_with_a_semicolon_silences_the_warning() {
    // Fix one, applied — and the accepted price paid in full: `(n)` is a
    // statement that GENUINELY starts with a continuation-shaped token, so
    // the `;` above it is what makes it one. Clean, both sides.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    if n == 0 { print("a") } else { print("b") };
    (n)
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn parenthesizing_affirms_the_expression_and_silences_the_warning() {
    // Fix two, applied: the expression said out loud. The parens are not a
    // continuation, so the question is never asked.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    (if n == 0 { 2 } else { 3 }
    - 1)
}
"#,
        expect![[r#""#]],
    );
}

#[test]
fn the_warning_carries_the_split_fix_at_the_brace() {
    use crate::Severity;
    let text = "static f = fn (n: usize) -> usize {\n    if n == 0 { 2 } else { 3 }\n    - 1\n}\n";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let [diagnostic] = &diagnostics[..] else {
        panic!("expected exactly one diagnostic, got {diagnostics:?}");
    };
    assert_eq!(diagnostic.severity, Severity::Warning);
    // Reported at the token that continued.
    assert_eq!(&text[diagnostic.range], "-");
    let fix = diagnostic.fix.as_ref().expect("the warning carries a fix");
    assert_eq!(fix.label, "Insert `;` after `}`");
    let [edit] = &fix.edits[..] else {
        panic!("expected one edit");
    };
    assert_eq!(edit.insert, ";");
    assert!(edit.range.is_empty());
    // Inserted right after the `}` that ended the block-tail statement.
    assert_eq!(
        &text[..edit.range.start().into()].chars().last(),
        &Some('}')
    );
    // And applying it is the fix: what it produces IS two statements, so
    // the question is never asked again. (They then say ordinary things
    // about themselves — a discarded number with no defining use, a
    // negative tail under `-> usize`. That is what asking for the split
    // MEANS, and it is why the fix is offered rather than applied.)
    let mut fixed = text.to_owned();
    fixed.insert_str(edit.range.start().into(), &edit.insert);
    assert!(!render_diagnostics(&fixed).contains("continues the expression"));
}

#[test]
fn a_match_arm_warning_offers_no_fix_when_the_line_is_no_arm() {
    // The gate, arm side. The separator between arms is `,`, and the
    // message says so — but `- 1,` is no arm: a negative literal pattern
    // is owed its `=>`, so inserting that `,` would turn a program that
    // compiles into one that does not parse. The affirm route the message
    // names is the whole answer.
    //
    // This is the same standard the `;`-between-arms objection already
    // answered, applied one step further: a fix must not hand back a worse
    // program than it found.
    let text = r#"
static f = fn (n: usize) -> usize {
    match n {
        _ => { 1 }
        - 1,
    }
}
"#;
    let diagnostic = only_diagnostic(text);
    assert!(diagnostic.message.contains("write `,` after that `}`"));
    assert!(
        diagnostic.fix.is_none(),
        "an arm-position warning must offer no split fix, got {:?}",
        diagnostic.fix
    );
}

#[test]
fn a_match_arm_warning_offers_the_split_when_the_line_is_a_negative_pattern_arm() {
    // `-1 =>` on its own line is the next arm, a negative literal pattern,
    // so the `,` after the `}` is the fix, and the split reading compiles
    // cleanly.
    let text = r#"
static f = fn (n: i64) -> usize {
    match n {
        0 => { 1 }
        -1 => 2,
        _ => 0,
    }
}
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.message.contains("continues the expression"))
        .expect("the continuation warning fires");
    assert_eq!(&text[diagnostic.range], "-");
    let fix = diagnostic.fix.as_ref().expect("the warning carries a fix");
    assert_eq!(fix.label, "Insert `,` after `}`");
    let [edit] = &fix.edits[..] else {
        panic!("expected one edit");
    };
    let mut fixed = text.to_owned();
    fixed.insert_str(edit.range.start().into(), &edit.insert);
    expect![[r#""#]].assert_eq(&render_diagnostics(&fixed));
}

#[test]
fn a_statement_warning_offers_no_fix_when_the_operator_cannot_begin_one() {
    // The gate, statement side. `*` continues an expression but cannot
    // START one, so there is no split reading to offer — only `-`, `(` and
    // `[` can, which is exactly the design's named price list. The warning
    // still stands: both readings genuinely exist, and only the button is
    // withheld.
    let text = "static f = fn (n: usize) -> usize {\n    { n }\n    * 2\n}\n";
    let diagnostic = only_diagnostic(text);
    assert!(diagnostic.message.contains("this `*` continues"));
    assert!(
        diagnostic.fix.is_none(),
        "a `*` continuation has no split reading to offer, got {:?}",
        diagnostic.fix
    );
}
