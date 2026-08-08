use crate::{AnalysisHost, FilePosition};
use base_db::SourceFile;
use syntax::TextSize;

const BROKEN: &str = "static = true;";

/// Splits a fixture on `$0` (the cursor). Panics if absent.
fn fixture(text: &str) -> (crate::Analysis, SourceFile, FilePosition) {
    let cursor = text.find("$0").expect("fixture needs a $0 cursor marker");
    let text = text.replace("$0", "");
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), text);
    let analysis = host.snapshot();
    let pos = FilePosition {
        file,
        offset: TextSize::new(cursor as u32),
    };
    (analysis, file, pos)
}

/// Asserts goto-definition from `$0` lands focused on `expected`, located by
/// `occurrence` (0-based) of that exact text in the fixture (after `$0` is
/// stripped).
fn check_goto(fixture_text: &str, expected: &str, occurrence: usize) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let nav = analysis
        .goto_definition(pos)
        .expect("goto_definition returned None");
    let text = fixture_text.replace("$0", "");
    let expected_start = text
        .match_indices(expected)
        .nth(occurrence)
        .map(|(i, _)| i)
        .expect("expected text not found in fixture");
    assert_eq!(
        u32::from(nav.focus_range.start()),
        expected_start as u32,
        "focus range {:?} (text: {:?})",
        nav.focus_range,
        &text[nav.focus_range.start().into()..nav.focus_range.end().into()],
    );
}

fn check_no_goto(fixture_text: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert_eq!(analysis.goto_definition(pos), None);
}

#[test]
fn goto_local_picks_nearest_shadowing_binding() {
    check_goto(
        r#"
static f = fn {
    let x = 1;
    let x = 2;
    print(x$0);
}
"#,
        "x",
        1,
    );
}

#[test]
fn goto_param() {
    check_goto("static f = fn (count: usize) { count$0 + 1 }", "count", 0);
}

#[test]
fn goto_static_across_mutual_recursion() {
    check_goto(
        r#"
const a = fn { b$0() };
static b = fn { a() };
"#,
        // Occurrence 0 of `b` is the reference itself; 1 is the definition.
        "b",
        1,
    );
}

#[test]
fn goto_static_from_its_own_body() {
    check_goto("static rec = fn { rec$0() };", "rec", 0);
}

#[test]
fn goto_builtin_is_none() {
    check_no_goto(r#"static f = fn { print$0("hi") };"#);
}

#[test]
fn goto_assignment_lhs() {
    // The LHS of an assignment lowers as a normal expression, so goto-def
    // works on it exactly like any other read of the binding.
    check_goto(
        r#"
static f = fn {
    let mut x = 1;
    x$0 = 5;
};
"#,
        "x",
        0,
    );
}

fn check_hover(fixture_text: &str, expected_markup: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let hover = analysis.hover(pos).expect("hover returned None");
    assert_eq!(hover.markup, expected_markup);
}

fn check_no_hover(fixture_text: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert_eq!(analysis.hover(pos), None);
}

#[test]
fn goto_hole_pattern_is_none() {
    // `_` isn't a name, so there's nothing to jump to — and nothing panics.
    check_no_goto("static f = fn { let _ = 1; _$0; };");
}

#[test]
fn hover_hole_pattern_is_none() {
    check_no_hover("static f = fn { let _$0 = 1; };");
}

#[test]
fn hover_local_use() {
    check_hover(
        r#"static main = fn { let s = "hello"; print(s$0); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_binding_definition() {
    check_hover(
        r#"static main = fn { let s$0 = "hello"; print(s); };"#,
        "```must\ns: str\n```",
    );
}

#[test]
fn hover_mut_binding_definition_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s$0 = "hello"; s = "bye"; };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_binding_use_shows_mut() {
    check_hover(
        r#"static main = fn { let mut s = "hello"; print(s$0); };"#,
        "```must\nmut s: str\n```",
    );
}

#[test]
fn hover_mut_param_shows_mut() {
    check_hover(
        "static f = fn (mut n$0: usize) { n = n + 1; };",
        "```must\nmut n: usize\n```",
    );
}

#[test]
fn hover_item_name_shows_inferred_fn_type() {
    check_hover(
        "static ma$0in = fn { print(\"hi\"); };",
        "```must\nmain: fn()\n```",
    );
}

#[test]
fn a_nullary_unsafe_fn_gets_no_run_lens() {
    // ▶ on a nullary host import has exactly one possible outcome — the
    // unsafe-block refusal — so the button is not offered. Its safe
    // neighbour still is, which is what keeps this a filter rather than a
    // retreat.
    let mut host = AnalysisHost::new();
    let file = host.create_file(
        "test.must".to_owned(),
        "extern static tick: unsafe fn() -> ();\n\
         static main = fn() -> () { };"
            .to_owned(),
    );
    let names: Vec<String> = host
        .snapshot()
        .run_lenses(file)
        .into_iter()
        .map(|lens| lens.name)
        .collect();
    assert_eq!(names, vec!["main".to_owned()]);
}

#[test]
fn hover_shows_the_unsafe_marker_on_a_fn_type() {
    // The one-token difference is the whole of what a reader needs to know
    // about a value here, so hover shows it exactly where it is written.
    check_hover(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;\n\
         static main = fn { let g$0 = read; };",
        "```must\ng: unsafe fn(u8.&raw mut, usize) -> isize\n```",
    );
}

#[test]
fn hover_on_an_import_declaration_shows_its_type() {
    // The declaration has no value expression at all, so hover must answer
    // from the DECLARATION — which is the whole contract anyway.
    check_hover(
        "extern static read$0: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;",
        "```must\nread: unsafe fn(u8.&raw mut, usize) -> isize\n```",
    );
}

#[test]
fn hover_static_use_shows_signature() {
    check_hover(
        r#"
static f = fn (n: usize) -> usize { n }
static main = fn { f$0(1); };
"#,
        "```must\nf: fn(usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_definition_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static high$0er_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    higher_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn hover_higher_order_call_shows_fully_inferred_type() {
    check_hover(
        r#"
static double = fn(n) {
    n + n
}

static higher_order = fn(f, a) -> usize {
    f(a)
}

static main = fn() {
    high$0er_order(double, 10);
}
"#,
        "```must\nhigher_order: fn(fn(usize) -> usize, usize) -> usize\n```",
    );
}

#[test]
fn non_block_fn_body_diagnostic_carries_wrap_fix() {
    let (analysis, file, _pos) = fixture("static f = fn true$0;");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Wrap in `{ }`");
    // Insert "{ " before `true` (offset 14) and " }" after it (offset 18).
    assert_eq!(fix.edits.len(), 2);
    assert_eq!(u32::from(fix.edits[0].edit.range.start()), 14);
    assert_eq!(fix.edits[0].edit.insert, "{ ");
    assert_eq!(u32::from(fix.edits[1].edit.range.start()), 18);
    assert_eq!(fix.edits[1].edit.insert, " }");
}

#[test]
fn missing_semicolon_diagnostic_carries_insert_fix() {
    let (analysis, file, _pos) = fixture(
        r#"
static name = fn {
    let f = fn { "" }$0
    f()
}
"#,
    );
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "expected `;`");
    // Anchored on the closing `}` of `fn { 42 }` (a visible, one-token
    // range a cursor can sit on); the fix inserts right after it.
    assert_eq!(diagnostics[0].range.len(), syntax::TextSize::new(1));
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Insert `;`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].edit.insert, ";");
    assert!(fix.edits[0].edit.range.is_empty());
    assert_eq!(fix.edits[0].edit.range.start(), diagnostics[0].range.end());
}

/// Renders each highlight as `start..end text tag[.mods]`, one per line.
fn check_highlights(text: &str, expect: expect_test::Expect) {
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), text.to_owned());
    let analysis = host.snapshot();
    let mut rendered = String::new();
    for hl in analysis.highlight(file) {
        let (start, end) = (u32::from(hl.range.start()), u32::from(hl.range.end()));
        rendered.push_str(&format!(
            "{start}..{end} {:?} {:?}",
            &text[start as usize..end as usize],
            hl.tag
        ));
        for (flag, name) in [
            (crate::HlMods::DECLARATION, "declaration"),
            (crate::HlMods::STATIC, "static"),
            (crate::HlMods::DEFAULT_LIBRARY, "defaultLibrary"),
            (crate::HlMods::MUTABLE, "mutable"),
        ] {
            if hl.mods.contains(flag) {
                rendered.push('.');
                rendered.push_str(name);
            }
        }
        rendered.push('\n');
    }
    expect.assert_eq(&rendered);
}

#[test]
fn highlights_classify_names_semantically() {
    check_highlights(
        r#"// the entry point
static greeting: str = "hi";
static main = fn (count: usize) {
    let next = count - 1;
    print(greeting);
    main(next);
};"#,
        expect_test::expect![[r#"
            0..18 "// the entry point" Comment
            19..25 "static" Keyword
            26..34 "greeting" Variable.declaration.static
            36..39 "str" Type.defaultLibrary
            40..41 "=" Operator
            42..46 "\"hi\"" String
            48..54 "static" Keyword
            55..59 "main" Function.declaration.static
            60..61 "=" Operator
            62..64 "fn" Keyword
            66..71 "count" Parameter.declaration
            73..78 "usize" Type.defaultLibrary
            86..89 "let" Keyword
            90..94 "next" Variable.declaration
            95..96 "=" Operator
            97..102 "count" Parameter
            103..104 "-" Operator
            105..106 "1" Number
            112..117 "print" Function.defaultLibrary
            118..126 "greeting" Variable.static
            133..137 "main" Function.static
            138..142 "next" Variable
        "#]],
    );
}

#[test]
fn highlights_block_comments_as_comments() {
    // `/* ... */` shares `COMMENT`'s one lexical tag with `//` (see
    // `lexer::scan_block_comment`), so it needs no highlighter code of its
    // own — pinned here so that stays true. Nested, to also confirm the
    // highlighter sees ONE token spanning the whole thing rather than
    // splitting at the first (inner) `*/`.
    check_highlights(
        "/* a /* nested */ comment */\nstatic x = 1;",
        expect_test::expect![[r#"
            0..28 "/* a /* nested */ comment */" Comment
            29..35 "static" Keyword
            36..37 "x" Variable.declaration.static
            38..39 "=" Operator
            40..41 "1" Number
        "#]],
    );
}

#[test]
fn highlights_split_multiline_block_comments_per_line() {
    // `push_line_split` already existed for multi-line STRING tokens (LSP
    // clients aren't required to handle a highlight range that spans
    // lines) — `/* */` is the second token kind to go through that same
    // function, since `//` never contained a newline of its own to split
    // on. Two ranges, not one, and the `\n` between them is in neither.
    check_highlights(
        "static s = 1; /* one\ntwo */",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "s" Variable.declaration.static
            9..10 "=" Operator
            11..12 "1" Number
            14..20 "/* one" Comment
            21..27 "two */" Comment
        "#]],
    );
}

#[test]
fn highlights_hole_pattern_unstyled() {
    // `_` isn't an IDENT token, so it's left unclassified (no panic, no
    // bogus Variable highlight).
    check_highlights(
        "static f = fn { let _ = 1; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            22..23 "=" Operator
            24..25 "1" Number
        "#]],
    );
}

#[test]
fn highlights_const_fn_and_const_block_keywords() {
    check_highlights(
        "static f = const fn { const { 1 } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..16 "const" Keyword
            17..19 "fn" Keyword
            22..27 "const" Keyword
            30..31 "1" Number
        "#]],
    );
}

#[test]
fn highlights_struct_keyword() {
    check_highlights(
        "static p = struct { x = 1 };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "p" Variable.declaration.static
            9..10 "=" Operator
            11..17 "struct" Keyword
            22..23 "=" Operator
            24..25 "1" Number
        "#]],
    );
}

#[test]
fn highlights_mut_keyword() {
    check_highlights(
        "static f = fn { let mut x = 1; x = 2; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..19 "let" Keyword
            20..23 "mut" Keyword
            24..25 "x" Variable.declaration.mutable
            26..27 "=" Operator
            28..29 "1" Number
            31..32 "x" Variable.mutable
            33..34 "=" Operator
            35..36 "2" Number
        "#]],
    );
}

#[test]
fn highlights_mutable_param_declaration_and_uses() {
    // The `mutable` modifier follows the binding, not just its declaration:
    // both the `mut` parameter's name and its later reads/assignment carry
    // it, while an ordinary immutable `let` next to it does not.
    check_highlights(
        "static f = fn (mut n: usize) { let y = n; n = y; };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..18 "mut" Keyword
            19..20 "n" Parameter.declaration.mutable
            22..27 "usize" Type.defaultLibrary
            31..34 "let" Keyword
            35..36 "y" Variable.declaration
            37..38 "=" Operator
            39..40 "n" Parameter.mutable
            42..43 "n" Parameter.mutable
            44..45 "=" Operator
            46..47 "y" Variable
        "#]],
    );
}

#[test]
fn highlights_a_fn_types_parameter_names() {
    // A fn TYPE's parameter names bind nothing — they are the signature's
    // spelling — but they READ as parameters, which is what a declaration a
    // reader must be able to read needs from an editor. The name sits
    // directly under `PARAM` here (no pattern wraps it), the one place a
    // parameter is spelled that way.
    check_highlights(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;",
        expect_test::expect![[r#"
            0..6 "extern" Keyword
            7..13 "static" Keyword
            14..18 "read" Function.declaration.static
            20..26 "unsafe" Keyword
            27..29 "fn" Keyword
            30..33 "buf" Parameter.declaration
            35..37 "u8" Type.defaultLibrary
            38..39 "&" Operator
            39..42 "raw" Keyword
            43..46 "mut" Keyword
            48..51 "len" Parameter.declaration
            53..58 "usize" Type.defaultLibrary
            60..62 "->" Operator
            63..68 "isize" Type.defaultLibrary
        "#]],
    );
}

#[test]
fn highlights_split_multiline_strings_per_line() {
    check_highlights(
        "static s = \"one\ntwo\";",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "s" Variable.declaration.static
            9..10 "=" Operator
            11..15 "\"one" String
            16..20 "two\"" String
        "#]],
    );
}

#[test]
fn highlights_character_literals_as_strings() {
    // A character literal is string-like and colored like one: the palette
    // has no character class, and every theme already paints quoted text
    // the same way whichever quote it is. A literal PATTERN gets the same
    // tag as an expression — the token kind decides, and there is only one
    // token kind for both positions.
    check_highlights(
        "static c = 'x';\nstatic f = fn (c: char) -> usize { match c { '\\n' => 1, _ => 0 } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "c" Variable.declaration.static
            9..10 "=" Operator
            11..14 "'x'" String
            16..22 "static" Keyword
            23..24 "f" Function.declaration.static
            25..26 "=" Operator
            27..29 "fn" Keyword
            31..32 "c" Parameter.declaration
            34..38 "char" Type.defaultLibrary
            40..42 "->" Operator
            43..48 "usize" Type.defaultLibrary
            51..56 "match" Keyword
            57..58 "c" Parameter
            61..65 "'\\n'" String
            66..68 "=>" Operator
            69..70 "1" Number
            74..76 "=>" Operator
            77..78 "0" Number
        "#]],
    );
}

#[test]
fn diagnostics_include_mir_findings() {
    // Captures are invisible to name resolution and inference — only MIR
    // lowering notices the binding lives in an enclosing function.
    let (analysis, file, _pos) =
        fixture("static f = fn () -> usize { let a = 1; let g = fn () -> usize { a$0 }; g() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "`a` is a local of an enclosing function; captures are not supported yet"
    );
    // On the `a` inside the nested fn literal.
    assert_eq!(u32::from(diagnostics[0].range.start()), 64);
}

#[test]
fn diagnostics_include_const_check_findings() {
    // Item initializers are const contexts; `double` is a plain `fn`, so
    // calling it there is rejected — squiggle on the callee.
    let src = "static double = fn (n: usize) -> usize { n * 2 };\nstatic x: usize = double(2);";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `double` in a const context; marking it `const fn` would allow this"
    );
    // On the callee `double` of the call, not the whole call expression.
    assert_eq!(&src[errors[0].range], "double");
    assert!(
        u32::from(errors[0].range.start()) > 50,
        "the use, not the definition"
    );
    assert_eq!(
        errors[0].related.len(),
        2,
        "related: {:?}",
        errors[0].related
    );
    assert_eq!(errors[0].related[0].message, "`double` is defined here");
    assert_eq!(&src[errors[0].related[0].range], "double");
    // The second hint explains why the call site is a const context at
    // all: it's directly in `x`'s own initializer, so it cites `x`'s
    // leading `static` keyword.
    assert_eq!(
        errors[0].related[1].message,
        "this item's initializer is a const context"
    );
    assert_eq!(&src[errors[0].related[1].range], "static");
}

#[test]
fn assign_to_immutable_squiggles_the_lhs_name_with_related_info() {
    // The squiggle sits on the assignment's LHS use of `x`; the related
    // hint points back at the `let`'s binding name, where `mut` is missing.
    let src = "static f = fn { let x: usize = 1; x = 2; };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot assign to `x`: it is not declared `mut`"
    );
    // The LHS use (offset 34), not the declaration (offset 20).
    assert_eq!(&src[errors[0].range], "x");
    assert_eq!(u32::from(errors[0].range.start()), 34);
    assert_eq!(errors[0].related.len(), 1);
    assert_eq!(
        errors[0].related[0].message,
        "`x` is declared without `mut` here"
    );
    assert_eq!(&src[errors[0].related[0].range], "x");
    assert_eq!(u32::from(errors[0].related[0].range.start()), 20);
}

#[test]
fn field_assign_to_immutable_root_offers_the_make_mut_fix() {
    // Same machinery as the plain-assignment case: the squiggle sits on
    // the ROOT name inside the place, the message carries the field path,
    // and the fix inserts `mut ` at the binding's declaration.
    let src = "static f = fn { let p: struct { x: usize } = struct { x = 1 }; p.x = 2; };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot assign to `p.x`: `p` is not declared `mut`"
    );
    // The root `p` inside the place (offset 63), not the whole `p.x`.
    assert_eq!(&src[errors[0].range], "p");
    assert_eq!(u32::from(errors[0].range.start()), 63);
    let fix = errors[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `p` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].edit.insert, "mut ");
    // Right before the declaration's `p` (offset 20).
    assert_eq!(u32::from(fix.edits[0].edit.range.start()), 20);
}

#[test]
fn const_check_finding_is_not_double_reported_by_const_eval() {
    // Const-evaluating `x` crashes at the trap MIR planted for the
    // const-check violation, but that failure is `Trap` — an
    // already-reported diagnostic execution ran into — so the const-eval
    // layer must add nothing: one squiggle, no "constant evaluation
    // failed" companion.
    let src = "static double = fn (n: usize) -> usize { n * 2 };\nstatic x: usize = double(2);";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `double` in a const context; marking it `const fn` would allow this"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("constant evaluation")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn diagnostics_include_failing_const_blocks_in_uncalled_fns() {
    // Nothing ever calls `f`, but its `const { … }` is compile-time code:
    // the panic is a check-time diagnostic, at the failure inside the block.
    let src = r#"static f = fn { const { panic("boom") }; };"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].message, "constant evaluation panicked: boom");
    assert_eq!(&src[diagnostics[0].range], r#"panic("boom")"#);
}

#[test]
fn const_block_trap_failure_is_not_double_reported() {
    // `print` in a `const` block already has a const-check squiggle;
    // forcing the block runs into the trap MIR planted for that same
    // violation — a `Trap` failure must add nothing.
    let src = r#"static f = fn { const { print("hi") }; };"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        errors[0].message,
        "cannot call `print` in a const context; const evaluation cannot have side effects"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("constant evaluation")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn hover_const_block_shows_its_value() {
    check_hover(
        "static f = fn () -> usize { con$0st { 2 + 3 } };",
        "```must\nconst { … }: usize = 5\n```",
    );
}

#[test]
fn hover_item_shows_const_value() {
    check_hover(
        "static exa$0mple: usize = 4 + 5;",
        "```must\nexample: usize = 9\n```",
    );
}

#[test]
fn hover_item_shows_record_const_value() {
    // Records const-evaluate: `Value::Record` displays like the type
    // does, field by field, in the same canonical (sorted) order.
    check_hover(
        "static po$0int: struct { x: usize, y: usize } = struct { y = 2, x = 1 };",
        "```must\npoint: struct { x: usize, y: usize } = { x = 1, y = 2 }\n```",
    );
}

#[test]
fn hover_use_shows_const_value_of_the_target() {
    check_hover(
        r#"
static greeting = "hi";
static main = fn { print(gree$0ting); };
"#,
        "```must\ngreeting: str = \"hi\"\n```",
    );
}

#[test]
fn diagnostics_include_const_eval_failures() {
    let (analysis, file, _pos) = fixture("static bad: usize = 1 / 0;$0\nstatic also: usize = bad;");
    let diagnostics = analysis.diagnostics(file);
    // Reported once at the origin, not re-reported by the propagating user.
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "constant evaluation failed: attempt to divide by zero"
    );
    // On `1 / 0`.
    assert_eq!(u32::from(diagnostics[0].range.start()), 20);
    assert_eq!(u32::from(diagnostics[0].range.end()), 25);
}

#[test]
fn unused_static_with_panicking_initializer_is_reported_at_check_time() {
    // Statics are eager, not lazy — every item is evaluated at check
    // time regardless of whether anything (transitively) calls or uses it.
    // `x` is never referenced by anything here; its panic must still show up
    // as a check-time diagnostic. This pins behavior that already falls out
    // of `Analysis::diagnostics` forcing `eval::const_value` for every item
    // in the file (not just reachable ones).
    let (analysis, file, _pos) = fixture(r#"static x = panic("boom");$0"#);
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].message, "constant evaluation panicked: boom");
    assert_eq!(diagnostics[0].severity, crate::Severity::Error);
}

#[test]
fn hole_named_item_dead_code_warning_surfaces_through_ide() {
    let (analysis, file, _pos) = fixture("static _: usize = 5;$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "this item binds nothing and its value cannot be used"
    );
    assert_eq!(diagnostics[0].severity, crate::Severity::Warning);
}

#[test]
fn hole_named_item_with_panicking_initializer_reports_both_the_warning_and_the_panic() {
    // A hole-named item's value can never be used (dead-code warning), but
    // since statics are eager it is still evaluated — a panic is the
    // one side effect const contexts allow, so it still surfaces as its own
    // error alongside the warning.
    let (analysis, file, _pos) = fixture(r#"static _ = panic("x");$0"#);
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 2, "diagnostics: {diagnostics:?}");
    let warning = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Warning)
        .expect("expected the dead-code warning");
    assert_eq!(
        warning.message,
        "this item binds nothing and its value cannot be used"
    );
    let error = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error)
        .expect("expected the panic error");
    assert_eq!(error.message, "constant evaluation panicked: x");
}

#[test]
fn diagnostics_include_name_errors() {
    let (analysis, file, _pos) = fixture("static f = fn { missing$0() };");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unresolved name `missing`");
}

// ---------------------------------------------------------------------------
// File lifecycle at this level: a closed document reads as empty, and the
// same handle analyzes again once it has text.
// ---------------------------------------------------------------------------

#[test]
fn a_closed_file_analyzes_again_on_the_same_handle() {
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), BROKEN.to_owned());
    assert_eq!(host.snapshot().diagnostics(file).len(), 1);

    host.close_file(file);
    assert_eq!(host.snapshot().diagnostics(file), vec![]);

    host.set_file_text(file, BROKEN.to_owned());
    assert_eq!(host.snapshot().diagnostics(file).len(), 1);

    host.set_file_text(file, "static a: usize = 1;".to_owned());
    assert_eq!(host.snapshot().diagnostics(file), vec![]);
}

#[test]
fn a_capability_answer_follows_an_edit_to_a_reachable_declaration() {
    // `Box::<Res>` is judged from `Box`'s summary, memoized per
    // declaration. The memo reads other declarations to build itself, so
    // an edit to any of them has to reach it — in both directions, since
    // a stale answer is wrong either way round. Both hops are pinned: the
    // declaration the answer is memoized on (`Box`), and one it only
    // READS (`Res`, two edges from `f`).
    let pointer = "type Res = struct { fd: usize } only move;\n\
                   type Box = struct::<T> { p: T.&raw };\n\
                   static f = fn(x: Box::<Res>) -> usize { 1 };\n";
    let held = pointer.replace("p: T.&raw", "p: T");
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), pointer.to_owned());
    assert_eq!(host.snapshot().diagnostics(file), vec![]);

    host.set_file_text(file, held);
    let errors: Vec<_> = host
        .snapshot()
        .diagnostics(file)
        .into_iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .map(|d| d.message)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(
        errors[0].starts_with("`x` is not consumed on this path"),
        "{errors:#?}"
    );

    // Two hops: `Res` is not mentioned by `Box`'s own text as a linear —
    // it is the argument. Making it forgettable must reach `f` through
    // `Box`'s summary, and putting it back must reach it again.
    let held_forgettable = pointer
        .replace("p: T.&raw", "p: T")
        .replace(" only move", "");
    host.set_file_text(file, held_forgettable);
    assert_eq!(host.snapshot().diagnostics(file), vec![]);

    host.set_file_text(file, pointer.replace("p: T.&raw", "p: T"));
    let errors: Vec<_> = host
        .snapshot()
        .diagnostics(file)
        .into_iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .map(|d| d.message)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");

    host.set_file_text(file, pointer.to_owned());
    assert_eq!(host.snapshot().diagnostics(file), vec![]);
}

#[test]
fn if_branch_mismatch_hint_points_to_other_branch() {
    let src = r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { n } else { "one" };
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("incompatible types"))
        .expect("expected an incompatible-types diagnostic");
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(mismatch.related[0].message, "this branch has type `usize`");
    // The hint points at the value-producing tail expression `1`, not the
    // whole `{ 1 }` block.
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "n");
}

#[test]
fn related_locations_get_companion_hint_diagnostics() {
    // Every related location also becomes its own hint-severity diagnostic
    // that spells out the connection and links back to the error — editors
    // that render related information as bare underlines (Zed) then explain
    // the underline on hover.
    let src = r#"
static f = fn (n: usize) -> () {
    let x = if n == 0 { n } else { "" };
    print(x);
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let error = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error)
        .expect("expected the type mismatch");
    assert_eq!(&src[error.range], "n");
    let hints: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Info)
        .collect();
    assert_eq!(hints.len(), 2);
    assert_eq!(&src[hints[0].range], "print");
    assert_eq!(
        hints[0].message,
        "this call requires `str` — causes the error on line 3: \
         type mismatch: expected `str`, found `usize`"
    );
    assert_eq!(&src[hints[1].range], "x");
    assert_eq!(
        hints[1].message,
        "this argument needs to be `str` — causes the error on line 3: \
         type mismatch: expected `str`, found `usize`"
    );
    for hint in hints {
        assert_eq!(hint.related.len(), 1);
        assert_eq!(hint.related[0].message, "the error reported here");
        assert_eq!(hint.related[0].range, error.range);
    }
}

#[test]
fn let_annotation_mismatch_hints_at_annotation() {
    let src = r#"
static f = fn (n: usize) -> () {
    let x: str = 42;
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("type mismatch"))
        .expect("expected a type mismatch diagnostic");
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(
        mismatch.related[0].message,
        "expected `str` because of this annotation"
    );
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "str");
}

#[test]
fn let_annotation_mismatch_on_agreeing_if_branches_blames_annotation_not_then_branch() {
    // Branches agree (both usize) but the annotation says str. Must produce
    // exactly one every-branch diagnostic on the if-expression pointing at
    // the annotation, not a spurious IfBranchMismatch blaming the then-branch.
    let src = r#"
static f = fn (n: usize) -> () {
    let x: str = if n == 0 { n } else { n + 1 };
    print("done");
}
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.contains("incompatible types")),
        "got a spurious IfBranchMismatch: {diagnostics:?}"
    );
    let mismatch = diagnostics
        .iter()
        .find(|d| d.severity == crate::Severity::Error && d.message.contains("every branch"))
        .expect("expected an every-branch diagnostic");
    assert_eq!(
        mismatch.message,
        "every branch produces `usize`, but `str` is needed"
    );
    assert_eq!(mismatch.related.len(), 1);
    assert_eq!(
        mismatch.related[0].message,
        "expected `str` because of this annotation"
    );
    let hint_text = &src[mismatch.related[0].range];
    assert_eq!(hint_text, "str");
}

#[test]
fn hover_record_typed_binding() {
    // The record type renders canonically (sorted fields) on the binding.
    check_hover(
        r#"static f = fn { let p$0 = struct { y = "s", x = { let n: usize = 1; n } }; };"#,
        "```must\np: struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_field_access_shows_the_field_type() {
    check_hover(
        r#"static f = fn { let p: struct { x: usize } = struct { x = 1 }; let y = p.x$0; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_chained_field_access_intermediate_step() {
    // Hovering `b` in `a.b.c` shows the intermediate record's type.
    check_hover(
        r#"static f = fn { let a = struct { b = struct { c = "deep" } }; a.b$0.c; };"#,
        "```must\nb: struct { c: str }\n```",
    );
}

#[test]
fn record_expression_evaluates_cleanly() {
    // Records are typed structurally and have a MIR/eval story
    // (aggregates and field projections) — the "not yet" diagnostic is
    // gone, and a record initializer is exactly as clean as any other.
    let (analysis, file, _pos) = fixture("static p = struct { x = { let n: usize = 1; n } };$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, Vec::new(), "diagnostics: {diagnostics:?}");
}

// ---- named types ----

#[test]
fn goto_type_item_from_annotation() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_type_item_from_construction_call() {
    check_goto(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x = 1 });
"#,
        "Foo",
        0,
    );
}

#[test]
fn goto_builtin_type_in_annotation_is_none() {
    check_no_goto("static f = fn (n: usize$0) { n };");
}

#[test]
fn hover_type_item_declaration() {
    check_hover(
        "type Foo$0 = struct { y: str, x: usize };",
        "```must\ntype Foo = struct { x: usize, y: str }\n```",
    );
}

#[test]
fn hover_type_name_in_annotation() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn (p: Foo$0) { p.x };
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_type_name_in_construction_call() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static p = Foo$0(struct { x = 1 });
"#,
        "```must\ntype Foo = struct { x: usize }\n```",
    );
}

#[test]
fn hover_named_typed_binding_shows_the_name() {
    check_hover(
        r#"
type Foo = struct { x: usize };
static f = fn { let p$0 = Foo(struct { x = 1 }); p.x };
"#,
        "```must\np: Foo\n```",
    );
}

#[test]
fn highlights_named_types() {
    // The declaration and both reference positions (annotation,
    // construction head) carry the type highlight class; builtins keep
    // the library modifier, user types don't.
    check_highlights(
        r#"type Foo = struct { x: usize };
static f = fn (p: Foo) { Foo(struct { x = p.x }) };"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..8 "Foo" Type.declaration
            9..10 "=" Operator
            11..17 "struct" Keyword
            23..28 "usize" Type.defaultLibrary
            32..38 "static" Keyword
            39..40 "f" Function.declaration.static
            41..42 "=" Operator
            43..45 "fn" Keyword
            47..48 "p" Parameter.declaration
            50..53 "Foo" Type
            57..60 "Foo" Type
            61..67 "struct" Keyword
            72..73 "=" Operator
            74..75 "p" Parameter
        "#]],
    );
}

#[test]
fn type_item_file_evaluates_cleanly() {
    // Type items have no value; the eager check-eval loop must not invent
    // a diagnostic for them.
    let (analysis, file, _pos) =
        fixture("type Foo = struct { x: usize };\nstatic p = Foo(struct { x = 1 });$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, Vec::new(), "diagnostics: {diagnostics:?}");
}

// ---- enums and variants ----

#[test]
fn goto_enum_from_variant_path_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize) };
static s = Sha$0pe::Circle(3);
"#,
        "Shape",
        0,
    );
}

#[test]
fn goto_variant_from_variant_path() {
    // The second segment lands on the variant inside the declaration.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        "Point",
        0,
    );
}

#[test]
fn goto_variant_from_type_annotation() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "Circle",
        0,
    );
}

#[test]
fn hover_variant_value_shows_the_variant_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let c = Shape::Circle(3);
    c$0;
};
"#,
        "```must\nc: Shape::Circle\n```",
    );
}

#[test]
fn hover_widened_binding_shows_the_enum() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let mut s = Shape::Point;
    s$0;
};
"#,
        "```must\nmut s: Shape\n```",
    );
}

#[test]
fn hover_enum_name_shows_the_declaration() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Sha$0pe::Point;
"#,
        "```must\ntype Shape = enum { Circle(usize), Point }\n```",
    );
}

#[test]
fn hover_variant_segment_shows_its_type() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::Poi$0nt;
"#,
        "```must\nPoint: Shape::Point\n```",
    );
}

#[test]
fn hover_variant_in_type_position_shows_the_declaration() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape::Cir$0cle = Shape::Circle(1);
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn goto_variant_from_the_expression_sigil() {
    // `::Point` in expression position resolves to one variant of one
    // enum, so it jumps exactly where the qualified spelling does.
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = ::Poi$0nt;
"#,
        "Point",
        0,
    );
}

#[test]
fn hover_the_expression_sigil_shows_the_variant() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static s: Shape = ::Poi$0nt;
"#,
        "```must\nShape::Point\n```",
    );
}

#[test]
fn highlights_the_expression_sigil_as_an_enum_member() {
    check_highlights(
        r#"type Shape = enum { Circle(usize), Point };
static s: Shape = ::Point;"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "s" Variable.declaration.static
            54..59 "Shape" Type
            60..61 "=" Operator
            64..69 "Point" EnumMember
        "#]],
    );
}

#[test]
fn highlights_enum_declaration_and_variant_paths() {
    check_highlights(
        r#"type Shape = enum { Circle(usize), Point };
static s: Shape::Circle = Shape::Circle(3);"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "s" Variable.declaration.static
            54..59 "Shape" Type
            61..67 "Circle" EnumMember
            68..69 "=" Operator
            70..75 "Shape" Type
            77..83 "Circle" EnumMember
            84..85 "3" Number
        "#]],
    );
}

// ---- match ----

#[test]
fn goto_variant_from_sigil_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 is the declaration inside the enum literal.
        "Circle",
        0,
    );
}

#[test]
fn goto_variant_from_qualified_pattern() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "Circle",
        0,
    );
}

#[test]
fn goto_enum_from_qualified_pattern_base() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape$0::Circle(r) => r,
        _ => 0,
    }
};
"#,
        // Occurrence 0 of `Shape` is the `type` item's name.
        "Shape",
        0,
    );
}

#[test]
fn goto_on_bare_bind_named_like_variant_has_no_target() {
    // Bare `Point` (no `::`) is just a binding — its own declaration — even
    // though it spells a variant's name. Goto on it has nothing to jump to.
    check_no_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
    );
}

#[test]
fn goto_pattern_binding_use_in_arm_body() {
    check_goto(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r$0,
        _ => 0,
    }
};
"#,
        // `r` also occurs inside the two `Circle`s; occurrence 2 is the
        // pattern binding itself.
        "r",
        2,
    );
}

#[test]
fn hover_pattern_binding_shows_payload_type() {
    check_hover(
        r#"
type Shape = enum { Pair(usize, str) };
static f = fn (s: Shape) {
    match s {
        ::Pair(n, text$0) => text,
        _ => "",
    }
};
"#,
        "```must\ntext: str\n```",
    );
}

#[test]
fn hover_variant_name_in_pattern_shows_the_variant() {
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle$0(r) => r,
        _ => 0,
    }
};
"#,
        "```must\nShape::Circle(usize)\n```",
    );
}

#[test]
fn hover_bind_arm_named_like_variant_shows_whole_scrutinee_type() {
    // Bare `Point` (no `::`) is a binding of the whole scrutinee, not a
    // variant match — hover shows the enum type, not the narrowed variant.
    check_hover(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Point$0 => 0,
        _ => 1,
    }
};
"#,
        "```must\nPoint: Shape\n```",
    );
}

#[test]
fn highlights_match_expression() {
    check_highlights(
        // A qualified arm (`Shape::Circle`), an elided sigil arm
        // (`::Point`, whose sole segment must still color as an enum
        // member), a bare bind whose name shadows a variant (`Point` —
        // now a plain `Variable`, never an `EnumMember`), and an ordinary
        // bind (`other`).
        r#"type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        ::Point => 0,
        Point => 1,
        other => 2,
    }
};"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..10 "Shape" Type.declaration
            11..12 "=" Operator
            13..17 "enum" Keyword
            20..26 "Circle" EnumMember.declaration
            27..32 "usize" Type.defaultLibrary
            35..40 "Point" EnumMember.declaration
            44..50 "static" Keyword
            51..52 "f" Function.declaration.static
            53..54 "=" Operator
            55..57 "fn" Keyword
            59..60 "s" Parameter.declaration
            62..67 "Shape" Type
            69..71 "->" Operator
            72..77 "usize" Type.defaultLibrary
            84..89 "match" Keyword
            90..91 "s" Parameter
            102..107 "Shape" Type
            109..115 "Circle" EnumMember
            116..117 "r" Variable.declaration
            119..121 "=>" Operator
            122..123 "r" Variable
            135..140 "Point" EnumMember
            141..143 "=>" Operator
            144..145 "0" Number
            155..160 "Point" Variable.declaration
            161..163 "=>" Operator
            164..165 "1" Number
            175..180 "other" Variable.declaration
            181..183 "=>" Operator
            184..185 "2" Number
        "#]],
    );
}

#[test]
fn highlights_loop_break_continue_keywords() {
    check_highlights(
        "static f = fn { loop { if true { break 1; }; continue; } };",
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            16..20 "loop" Keyword
            23..25 "if" Keyword
            26..30 "true" Keyword
            33..38 "break" Keyword
            39..40 "1" Number
            45..53 "continue" Keyword
        "#]],
    );
}

#[test]
fn hover_on_loop_shows_its_type() {
    check_hover(
        r#"
static f = fn () -> usize {
    lo$0op {
        break 42;
    }
};
"#,
        "```must\nloop { … }: usize\n```",
    );
}

#[test]
fn hover_on_breakless_loop_shows_never() {
    check_hover(
        "static f = fn { lo$0op { } };",
        "```must\nloop { … }: !\n```",
    );
}

#[test]
fn hover_on_break_keyword_is_none() {
    // `break` names nothing; hover stays quiet (and doesn't panic) on it
    // and on its dangling outside-a-loop form.
    let (analysis, _file, pos) = fixture("static f = fn { loop { bre$0ak 1; } };");
    assert_eq!(analysis.hover(pos), None);
    let (analysis, _file, pos) = fixture("static f = fn { bre$0ak; };");
    assert_eq!(analysis.hover(pos), None);
}

// ---- record destructuring — hover and goto-def ----

#[test]
fn hover_record_destructured_binding_definition() {
    check_hover(
        r#"static f = fn { let struct { x$0, y }: struct { x: usize, y: str } = struct { x = 1, y = "s" }; };"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn hover_record_destructured_binding_use() {
    check_hover(
        r#"static f = fn { let struct { x, y } = struct { x = 1, y = "s" }; print(y$0); };"#,
        "```must\ny: str\n```",
    );
}

#[test]
fn hover_record_destructure_rename_shows_the_new_name() {
    check_hover(
        r#"static f = fn { let struct { x as alpha }: struct { x: usize } = struct { x = 1 }; let b = alpha$0; };"#,
        "```must\nalpha: usize\n```",
    );
}

#[test]
fn hover_mut_field_binding_shows_mut() {
    check_hover(
        r#"static f = fn { let struct { mut x$0 }: struct { x: usize } = struct { x = 1 }; x = 2; };"#,
        "```must\nmut x: usize\n```",
    );
}

#[test]
fn hover_param_record_destructured_binding() {
    check_hover(
        "static f = fn (struct { n$0 }: struct { n: usize }) { n };",
        "```must\nn: usize\n```",
    );
}

#[test]
fn hover_newtype_destructured_binding() {
    check_hover(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) { let Point(struct { x$0, y }) = p; x };
"#,
        "```must\nx: usize\n```",
    );
}

#[test]
fn goto_record_destructured_binding_use() {
    check_goto(
        r#"
static f = fn {
    let struct { x, y } = struct { x = 1, y = 2 };
    print(x$0);
}
"#,
        "x",
        // Occurrence 0 is the field/binding name in the pattern (its
        // declaration site — where the use inside `print` should jump to);
        // occurrence 1 is the record literal's field name.
        0,
    );
}

#[test]
fn goto_record_destructure_rename_use_lands_on_the_rename() {
    check_goto(
        r#"
static f = fn {
    let struct { x as alpha } = struct { x = 1 };
    print(alpha$0);
}
"#,
        "alpha",
        0,
    );
}

#[test]
fn goto_param_record_destructured_binding_use() {
    check_goto(
        "static f = fn (struct { num }: struct { num: usize }) { num$0 };",
        "num",
        0,
    );
}

/// Renders each completion as `label Kind (detail)`, one per line, in the
/// exact order `Analysis::completions` returns (already rank-sorted).
fn check_completions(fixture_text: &str, expect: expect_test::Expect) {
    let (analysis, _file, pos) = fixture(fixture_text);
    let mut rendered = String::new();
    for item in analysis.completions(pos) {
        rendered.push_str(&item.label);
        rendered.push(' ');
        rendered.push_str(&format!("{:?}", item.kind));
        if let Some(detail) = &item.detail {
            rendered.push_str(&format!(" ({detail})"));
        }
        rendered.push('\n');
    }
    expect.assert_eq(&rendered);
}

fn check_no_completion(fixture_text: &str, label: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert!(
        !analysis.completions(pos).iter().any(|c| c.label == label),
        "expected no completion labeled {label:?}"
    );
}

fn check_has_completion(fixture_text: &str, label: &str) {
    let (analysis, _file, pos) = fixture(fixture_text);
    assert!(
        analysis.completions(pos).iter().any(|c| c.label == label),
        "expected a completion labeled {label:?}"
    );
}

#[test]
fn completions_expression_position_ranks_locals_items_builtins_keywords() {
    // A call argument: expression position, but not a fresh statement — no
    // `let` among the keywords. The typed prefix `x` is a real
    // expression checked against `print`'s parameter, so the position
    // carries a `str` expectation — `panic` (returns `!`, which widens to
    // anything) is the one candidate whose *call* satisfies it, so its
    // type tier lifts it above every tier-2 candidate; below that the
    // provenance order (locals, items, builtins, keywords) is intact.
    check_completions(
        r#"
type Shape = struct { r: usize };
static area = fn (r: usize) -> usize { r * r };
static main = fn {
    let x: usize = 1;
    print(x$0);
};
"#,
        expect_test::expect![[r#"
            panic Function (fn(str) -> !)
            x Variable (usize)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Shape Struct (struct { r: usize })
            Utf8Result Enum (enum { Ok(str), Err })
            area Function (fn(usize) -> usize)
            main Function (fn())
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_mutable_local_carries_mut_in_detail() {
    // Doubles as the type-tier no-expectation regression pin: an empty
    // prefix in a call-argument hole is not one of `expected_type_at`'s two
    // provable shapes, so no type ranking applies and the provenance
    // ordering is untouched.
    check_completions(
        r#"
static main = fn {
    let mut x: usize = 1;
    print($0);
};
"#,
        expect_test::expect![[r#"
            x Variable (mut usize)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn())
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            panic Function (fn(str) -> !)
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_position_shows_only_types_and_builtins() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static make_point = fn (x: usize) -> $0 { x };
"#,
        expect_test::expect![[r#"
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            Point Struct (struct { x: usize, y: usize })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            bool Keyword
            char Keyword
            i16 Keyword
            i32 Keyword
            i64 Keyword
            i8 Keyword
            isize Keyword
            str Keyword
            string Keyword
            u16 Keyword
            u32 Keyword
            u64 Keyword
            u8 Keyword
            usize Keyword
            fn Keyword
            struct Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_position_excludes_locals_and_value_items() {
    check_no_completion(
        r#"
static helper = fn { 1 };
static main = fn (n: $0) { n };
"#,
        "helper",
    );
}

#[test]
fn completions_statement_start_includes_let() {
    check_has_completion(
        r#"
static main = fn {
    let x = 1;
    $0
};
"#,
        "let",
    );
}

#[test]
fn completions_statement_start_sees_the_preceding_let() {
    // The trailing statement slot has no `ExprId` of its own (nothing was
    // ever lowered there) — this exercises the block-walk reconstruction in
    // `locals_in_block` rather than a plain `scope_of` lookup.
    check_has_completion(
        r#"
static main = fn {
    let x = 1;
    $0
};
"#,
        "x",
    );
}

#[test]
fn completions_expression_position_has_no_let() {
    check_no_completion(
        r#"
static main = fn {
    print($0);
};
"#,
        "let",
    );
}

#[test]
fn completions_break_continue_only_inside_a_loop() {
    check_has_completion(
        r#"
static main = fn {
    loop {
        $0
    }
};
"#,
        "break",
    );
    check_has_completion(
        r#"
static main = fn {
    loop {
        $0
    }
};
"#,
        "continue",
    );
    check_no_completion(
        r#"
static main = fn {
    $0
};
"#,
        "break",
    );
}

#[test]
fn completions_break_does_not_cross_a_nested_fn_literal_boundary() {
    // A closure written inside a loop's body is its own const-check/loop
    // boundary (mirrors `hir::infer`'s `loop_sinks` save/restore) — `break`
    // here couldn't exit the outer loop, so it shouldn't be offered.
    check_no_completion(
        r#"
static main = fn {
    loop {
        let inner = fn { $0 };
    }
};
"#,
        "break",
    );
}

#[test]
fn completions_top_level_offers_exactly_the_item_keywords() {
    // Exactly the grammar's item heads — `completions::tests` guards this
    // set against the parser in both directions.
    check_completions(
        "$0",
        expect_test::expect![[r#"
            const Keyword
            extern Keyword
            static Keyword
            trait Keyword
            type Keyword
        "#]],
    );
}

#[test]
fn completions_after_extern_offer_the_one_keyword_that_may_follow() {
    // `extern` is a marker on a `static`, not an item kind of its own, so
    // there is exactly one thing that can come next.
    check_completions(
        "extern $0",
        expect_test::expect![[r#"
            static Keyword
        "#]],
    );
}

#[test]
fn completions_empty_file_top_level() {
    // Same shape as the general top-level case, exercised on a genuinely
    // empty file (no tokens at all for the speculative parse to hang off).
    check_completions(
        "$0",
        expect_test::expect![[r#"
            const Keyword
            extern Keyword
            static Keyword
            trait Keyword
            type Keyword
        "#]],
    );
}

#[test]
fn completions_typed_prefix_still_classifies_and_edit_covers_the_prefix() {
    let (analysis, _file, pos) = fixture(
        r#"
static main = fn {
    let price = 1;
    pri$0
};
"#,
    );
    let items = analysis.completions(pos);
    let local = items
        .iter()
        .find(|c| c.label == "price")
        .expect("`price` offered even though only `pri` was typed — filtering is client-side");
    // `pri` sits right before the cursor; the edit replaces exactly that,
    // not the identifier that would follow it if there were one.
    let prefix_start = pos.offset - syntax::TextSize::new(3);
    assert_eq!(local.text_edit.range.start(), prefix_start);
    assert_eq!(local.text_edit.range.end(), pos.offset);
}

#[test]
fn completions_cursor_mid_identifier_of_an_existing_name() {
    let (analysis, _file, pos) = fixture(
        r#"
static main = fn {
    let abc = 1;
    print(ab$0c);
};
"#,
    );
    let items = analysis.completions(pos);
    assert!(
        items.iter().any(|c| c.label == "abc"),
        "still classifies as expression position and offers the shadowed-over local"
    );
    let edited = items.iter().find(|c| c.label == "abc").unwrap();
    // Only `ab` (the part before the cursor) is replaced; the `c` that
    // follows is left alone.
    let prefix_start = pos.offset - syntax::TextSize::new(2);
    assert_eq!(edited.text_edit.range.start(), prefix_start);
    assert_eq!(edited.text_edit.range.end(), pos.offset);
}

#[test]
fn completions_inside_a_string_literal_is_empty() {
    let (analysis, _file, pos) = fixture(r#"static main = fn { print("hi $0"); };"#);
    assert_eq!(analysis.completions(pos), Vec::new());
}

#[test]
fn completions_inside_a_comment_is_empty() {
    let (analysis, _file, pos) = fixture("// hi $0\nstatic main = fn { 1 };");
    assert_eq!(analysis.completions(pos), Vec::new());
}

#[test]
fn a_block_comment_offers_no_completions_and_no_hover() {
    // `/* */` shares `COMMENT` with `//` (see `highlights_block_comments_as_comments`),
    // so both already inherit their host feature's comment exclusion with
    // no code of their own — completions via the `SyntaxKind::COMMENT`
    // check in `completions.rs`, hover by construction (it only ever
    // looks for an `IDENT` token, which a comment position never is).
    // Pinned so that stays true.
    let (analysis, _file, pos) = fixture("static main = fn { /* hi $0 */ 1 };");
    assert_eq!(analysis.completions(pos), Vec::new());
    assert_eq!(analysis.hover(pos), None);
}

// ---- field access ----

#[test]
fn completions_dot_field_access_on_record_local() {
    check_completions(
        r#"
static f = fn {
    let p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
    p.$0
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_through_named_type_erasure() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) {
    p.$0
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_chained() {
    check_completions(
        r#"
type Inner = struct { z: usize };
type Outer = struct { inner: Inner };
static f = fn (o: Outer) {
    o.inner.$0
};
"#,
        expect_test::expect![[r#"
            z Field (usize)
        "#]],
    );
}

#[test]
fn completions_dot_field_access_non_record_receiver_is_empty() {
    check_completions(
        r#"
static f = fn (n: usize) {
    n.$0
};
"#,
        expect_test::expect![""],
    );
}

#[test]
fn completions_dot_field_access_unknown_receiver_is_empty() {
    check_completions(
        r#"
static f = fn {
    nope.$0
};
"#,
        expect_test::expect![""],
    );
}

#[test]
fn completions_dot_offers_the_builtin_member_of_str() {
    // `next_char` is reachable only through the dot, so the dot is where
    // it has to be discoverable. Rendered with its real type, `str` last —
    // the dot-callable shape (TR01), the way the call checks.
    check_completions(
        r#"
static f = fn (s: str) {
    s.$0
};
"#,
        expect_test::expect![[r#"
            len Function (fn(str) -> usize)
            next_char Function (fn(usize, str) -> NextChar)
        "#]],
    );
}

#[test]
fn completions_snippet_builtin_member_call_with_params() {
    // A builtin member is fn-shaped like any other dot candidate, so
    // accepting it inserts the call, not the bare name. The receiver is
    // not a written argument, so the index is the only tab stop.
    assert_eq!(
        completion_insert(
            r#"
static f = fn (s: str) {
    s.$0
};
"#,
            "next_char",
        ),
        crate::InsertText::Snippet {
            snippet: "next_char($1)".to_owned(),
            plain: "next_char()".to_owned(),
        }
    );
}

#[test]
fn completions_are_suppressed_inside_a_character_literal() {
    // A half-typed `'` is a CHAR token, so without this the list would pop
    // open on the keystroke after the apostrophe — the same suppression a
    // string and a comment get.
    check_completions(
        r#"
static f = fn () {
    let c = '$0';
};
"#,
        expect_test::expect![""],
    );
}

// ---- `::` second segment (member completions) ----

#[test]
fn completions_variant_segment_expression_position() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    Shape::$0
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_variant_segment_type_position() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape::$0) {};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_variant_segment_after_struct_type_is_empty() {
    check_completions(
        r#"
type Point = struct { x: usize };
static f = fn {
    Point::$0
};
"#,
        expect_test::expect![""],
    );
}

// ---- match-arm pattern position ----

#[test]
fn completions_match_arm_uncovered_variants_ranked_first() {
    // One arm (`::Point`) already written and covered; the bare slot of a
    // fresh arm should rank the two *uncovered* variants above the
    // already-covered one (rank-down, not filtered — an arm matching it
    // again would be unreachable, but it's still shown), with `_` last.
    // The bare slot labels/inserts the sigil form `::Variant` (see
    // `match_arm_items`' doc).
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point, Square(usize) };
static f = fn (s: Shape) {
    match s {
        ::Point => 1,
        $0
    }
};
"#,
        expect_test::expect![[r#"
            ::Circle EnumMember (Circle(usize))
            ::Square EnumMember (Square(usize))
            ::Point EnumMember (Point)
            _ Keyword
        "#]],
    );
}

#[test]
fn completions_match_arm_sigil_context_inserts_bare_variant_name() {
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        ::$0
    }
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_match_arm_qualified_context_inserts_bare_variant_name() {
    // Past an already-typed `Enum::`, same bare spelling as the elided
    // sigil — the qualifier is already on screen.
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

#[test]
fn completions_match_arm_bare_slot_inserts_sigil_variant_and_offers_wildcard() {
    // A bare pattern slot has no `::` at all yet — a bare `Circle` insert
    // would just bind a fresh local named `Circle` (G25: bare names
    // always bind), so this inserts the sigil form `::Circle` (label =
    // insertion; the filter text stays the bare name so a typed `Cir`
    // still matches). `_` is also offered (it's only reachable from a
    // bare slot — there's no way to spell it after a `::`).
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        $0
    }
};
"#;
    check_has_completion(fixture_text, "::Circle");
    check_no_completion(fixture_text, "Circle");
    check_no_completion(fixture_text, "Shape::Circle");
    check_has_completion(fixture_text, "_");

    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    let circle = items.iter().find(|c| c.label == "::Circle").unwrap();
    // `Circle` carries a payload, so the bare slot's sigil insertion is
    // a snippet with a tab-stop for it; a snippet-incapable client falls
    // back to the bare `::Circle` (no parens — the payload-less form).
    assert_eq!(
        circle.text_edit.insert,
        crate::InsertText::Snippet {
            snippet: "::Circle($1)".to_owned(),
            plain: "::Circle".to_owned(),
        }
    );
    assert_eq!(circle.filter_text, "Circle");
}

#[test]
fn completions_match_arm_non_enum_scrutinee() {
    // A bare slot still offers the universal `_`; a `::`-sigil slot has no
    // enum to draw variants from, so it offers nothing at all.
    check_completions(
        r#"
static f = fn (s: usize) {
    match s {
        $0
    }
};
"#,
        expect_test::expect![[r#"
            _ Keyword
        "#]],
    );
    check_completions(
        r#"
static f = fn (s: usize) {
    match s {
        ::$0
    }
};
"#,
        expect_test::expect![""],
    );
}

// ---- record-literal field names ----

#[test]
fn completions_record_literal_missing_fields_excludes_already_written() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    Point(struct { x = 1, $0 })
};
"#,
        expect_test::expect![[r#"
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_matching_local_ranked_first() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let y = 5;
    Point(struct { x = 1, $0 })
};
"#,
        expect_test::expect![[r#"
            y Variable ({number})
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_annotated_let_context() {
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let p: Point = struct { $0 };
};
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_literal_no_expectation_fallback_is_expression_position() {
    // No construction head, no type annotation: the record's shape says
    // nothing here, so this falls back to ordinary expression-position
    // candidates rather than going silent.
    check_has_completion(
        r#"
static f = fn {
    let p = struct { $0 };
};
"#,
        "if",
    );
    check_no_completion(
        r#"
static f = fn {
    let p = struct { $0 };
};
"#,
        "let",
    );
}

// ---- `type X = …` RHS ----

#[test]
fn completions_type_item_rhs_offers_exactly_struct_and_enum() {
    check_completions(
        "type X = $0;",
        expect_test::expect![[r#"
            enum Keyword
            struct Keyword
        "#]],
    );
}

// ---- type-directed ranking ----

#[test]
fn completions_type_ranking_exact_local_above_mismatched_local() {
    // The typed prefix `s` sits in an annotated `let`'s initializer, so
    // the position expects `str`: the `str` local ranks tier 0, `panic`
    // (returns `!`) tier 1 via its return type, everything else tier 2 in
    // the usual provenance order — the mismatched `usize` local included.
    check_completions(
        r#"
static main = fn (s: str, n: usize) {
    let want: str = s$0;
};
"#,
        expect_test::expect![[r#"
            s Variable (str)
            panic Function (fn(str) -> !)
            n Variable (usize)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn(str, usize))
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_matching_return_type_above_mismatched() {
    // `g` resolves to nothing, but the *position* (an annotated `let`'s
    // initializer) still expects `str`: `get_s`'s call would satisfy it
    // (tier 1, like `panic`), `get_n`'s wouldn't (tier 2).
    check_completions(
        r#"
static get_s: fn() -> str = fn () -> str { "s" };
static get_n: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let x: str = g$0;
};
"#,
        expect_test::expect![[r#"
            get_s Function (fn() -> str)
            panic Function (fn(str) -> !)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            get_n Function (fn() -> usize)
            main Function (fn())
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_rank_a_safe_fn_under_an_unsafe_fn_expectation() {
    // The THIRD consumer of the convertibility question, after the two
    // check sites. A safe `fn` is legal in an `unsafe fn` position, so the
    // ranker must say so — `hir::widens_to` is the one home it asks, which
    // is why that edge was added there and not re-derived here.
    check_completions(
        r#"
static helper: fn(usize) -> usize = fn (n: usize) -> usize { n };
static other: fn(usize) -> str = fn (n: usize) -> str { "s" };
static main = fn {
    let f: unsafe fn(usize) -> usize = $0;
};
"#,
        expect_test::expect![[r#"
            helper Function (fn(usize) -> usize)
            panic Function (fn(str) -> !)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn())
            other Function (fn(usize) -> str)
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_fn_typed_expectation_ranks_the_fn_itself_exact() {
    // Under a `fn`-typed expectation the matching function is tier 0 as a
    // *value* (passing it, not calling it); `panic` still reaches tier 1
    // through its `!` return, everything else stays tier 2.
    check_completions(
        r#"
static helper: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let f: fn() -> usize = h$0;
};
"#,
        expect_test::expect![[r#"
            helper Function (fn() -> usize)
            panic Function (fn(str) -> !)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn())
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_fresh_hole_after_annotated_let_eq() {
    // No prefix at all: `let x: Foo = |` lowers the absent initializer as
    // a `Missing` expression checked against the annotation — the one
    // no-prefix shape `expected_type_at` can prove. The `Point`-typed
    // local ranks tier 0 over the `usize` one.
    check_completions(
        r#"
type Point = struct { x: usize };
static main = fn (p: Point, n: usize) {
    let q: Point = $0;
};
"#,
        expect_test::expect![[r#"
            p Variable (Point)
            panic Function (fn(str) -> !)
            n Variable (usize)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            Point Struct (struct { x: usize })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn(Point, usize))
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_type_ranking_variant_under_enum_expectation() {
    // A variant candidate under its own enum's expectation widens to it
    // (tier 1): its sort key outranks every unrankable/mismatching
    // (tier 2) item's.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static main = fn {
    let s: Shape = Shape::C$0;
};
"#,
    );
    let items = analysis.completions(pos);
    let circle = items
        .iter()
        .find(|c| c.label == "Circle")
        .expect("Circle is offered");
    assert_eq!(circle.sort_text, "1_20_Circle");
    assert!(
        circle.sort_text.as_str() < "2_00_",
        "outranks any tier-2 item"
    );

    // Control: under a non-matching expectation the same variant is
    // tier 2 — type ranking is the expectation's doing, not the context's.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static main = fn {
    let n: usize = Shape::C$0;
};
"#,
    );
    let items = analysis.completions(pos);
    let circle = items
        .iter()
        .find(|c| c.label == "Circle")
        .expect("Circle is offered");
    assert_eq!(circle.sort_text, "2_20_Circle");
}

#[test]
fn completions_type_ranking_match_arm_variants_carry_the_widening_tier() {
    // Match-arm variant candidates rank against the scrutinee's enum: all
    // widen (tier 1), with gold-before-covered preserved inside the tier
    // and `_` staying an unranked keyword.
    let (analysis, _file, pos) = fixture(
        r#"
type Shape = enum { Circle(usize), Square };
static f = fn (s: Shape) -> usize {
    match s { ::Square => 2, $0 }
};
"#,
    );
    let items = analysis.completions(pos);
    let sort_text = |label: &str| {
        items
            .iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("{label} is offered"))
            .sort_text
            .clone()
    };
    assert_eq!(sort_text("::Circle"), "1_00_::Circle", "uncovered: gold");
    assert_eq!(sort_text("::Square"), "1_20_::Square", "covered: item");
    assert_eq!(sort_text("_"), "2_40__");
}

#[test]
fn completions_type_ranking_statement_start_without_prefix_keeps_untyped_ordering() {
    // A fresh statement slot with no prefix has no provable expectation:
    // the provenance ordering (locals, items, builtins, keywords) is
    // untouched.
    check_completions(
        r#"
static main = fn {
    let s = "hi";
    $0
};
"#,
        expect_test::expect![[r#"
            s Variable (str)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn())
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            panic Function (fn(str) -> !)
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            let Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

// ---- snippets ----

/// The `InsertText` of the one completion labeled `label` (of any kind).
/// Panics on zero or multiple matches — most snippet fixtures below only ever
/// produce one candidate under that label.
fn completion_insert(fixture_text: &str, label: &str) -> crate::InsertText {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"))
        .text_edit
        .insert
        .clone()
}

/// Same as [`completion_insert`], but disambiguated by kind too — needed
/// where a shorthand-matching local and a field share one label (the
/// record-literal gold case).
fn completion_insert_kind(
    fixture_text: &str,
    label: &str,
    kind: crate::CompletionItemKind,
) -> crate::InsertText {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label && c.kind == kind)
        .unwrap_or_else(|| panic!("no completion labeled {label:?} of kind {kind:?}: {items:?}"))
        .text_edit
        .insert
        .clone()
}

#[test]
fn completions_snippet_fn_call_with_params_inserts_a_snippet() {
    // A parameterful fn in expression position offers `name($1)`, falling
    // back to a bare `name()` for a snippet-incapable client.
    assert_eq!(
        completion_insert(
            r#"
static add = fn (a: usize, b: usize) -> usize { a };
static main = fn {
    $0
};
"#,
            "add",
        ),
        crate::InsertText::Snippet {
            snippet: "add($1)".to_owned(),
            plain: "add()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_zero_arity_fn_call_is_plain() {
    // No parameters, nothing for a tab stop to land on: always the plain
    // `name()`, snippet or not.
    assert_eq!(
        completion_insert(
            r#"
static go = fn { 1 };
static main = fn {
    $0
};
"#,
            "go",
        ),
        crate::InsertText::Plain("go()".to_owned())
    );
}

#[test]
fn completions_snippet_fn_typed_expectation_suppresses_the_call_snippet() {
    // Under a `fn`-typed expectation the position wants the fn *passed*,
    // not called — even though `helper` itself is nominally zero-arity, the
    // insertion is the bare name with no call syntax at all (distinct from
    // the zero-arity case above, which still inserts `()`).
    assert_eq!(
        completion_insert(
            r#"
static helper: fn() -> usize = fn () -> usize { 1 };
static main = fn {
    let f: fn() -> usize = h$0;
};
"#,
            "helper",
        ),
        crate::InsertText::Plain("helper".to_owned())
    );
}

#[test]
fn completions_snippet_builtin_fn_call_with_params() {
    // Builtins go through the same snippet rule as file items — `print`
    // takes one param.
    assert_eq!(
        completion_insert(
            r#"
static main = fn {
    $0
};
"#,
            "print",
        ),
        crate::InsertText::Snippet {
            snippet: "print($1)".to_owned(),
            plain: "print()".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_type_item_rhs_struct_and_enum() {
    let fixture_text = "type X = $0;";
    assert_eq!(
        completion_insert(fixture_text, "struct"),
        crate::InsertText::Snippet {
            snippet: "struct { $1 }".to_owned(),
            plain: "struct { }".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "enum"),
        crate::InsertText::Snippet {
            snippet: "enum { $1 }".to_owned(),
            plain: "enum { }".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_match_arm_payload_variant_qualified_context() {
    // Past an already-typed qualifier, a payload-carrying variant inserts
    // just the bare name plus its own tab-stopped parens; a payload-less
    // one is untouched (still a plain bare name, the payload-less form).
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "Circle"),
        crate::InsertText::Snippet {
            snippet: "Circle($1)".to_owned(),
            plain: "Circle".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "Point"),
        crate::InsertText::Plain("Point".to_owned())
    );
}

#[test]
fn completions_snippet_record_literal_missing_field_inserts_a_snippet() {
    assert_eq!(
        completion_insert(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    Point(struct { x = 1, $0 })
};
"#,
            "y",
        ),
        crate::InsertText::Snippet {
            snippet: "y = $1".to_owned(),
            plain: "y = ".to_owned(),
        }
    );
}

#[test]
fn completions_snippet_record_literal_gold_local_stays_a_plain_bare_name() {
    // The shorthand-matching local (`y` meaning `y: y`) is not the field
    // slot itself — it keeps inserting just its own name, snippet-free.
    assert_eq!(
        completion_insert_kind(
            r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let y = 5;
    Point(struct { x = 1, $0 })
};
"#,
            "y",
            crate::CompletionItemKind::Variable,
        ),
        crate::InsertText::Plain("y".to_owned())
    );
}

// ---- record-pattern field names ----

#[test]
fn completions_record_pattern_let_destructure_field_names() {
    // A `let struct { … }` destructure completes the field names of the
    // initializer's record type, with the field's own type as detail.
    check_completions(
        r#"static f = fn { let struct { $0 }: struct { x: usize, y: str } = struct { x = 1, y = "s" }; };"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (str)
        "#]],
    );
}

#[test]
fn completions_record_pattern_param_destructure_field_names() {
    // A parameter's `struct { … }` pattern completes the field names of its
    // own annotation.
    check_completions(
        "static f = fn (struct { $0 }: struct { m: str, n: usize }) { 1 };",
        expect_test::expect![[r#"
            m Field (str)
            n Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_pattern_nested_in_newtype_pattern() {
    // The genuine "nested record pattern" case in this grammar: a
    // `Name(struct { … })` newtype pattern (match-arm variant payloads only
    // bind names, never nest a `struct { … }`). Completes the newtype's
    // underlying record fields.
    check_completions(
        r#"
type Point = struct { x: usize, y: usize };
static f = fn (p: Point) { let Point(struct { $0 }) = p; 1 };
"#,
        expect_test::expect![[r#"
            x Field (usize)
            y Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_pattern_excludes_already_bound_fields() {
    // A field named earlier in the same pattern is dropped; only the
    // still-unbound fields are offered.
    check_completions(
        r#"static f = fn { let struct { x, $0 }: struct { x: usize, y: usize, z: usize } = struct { x = 1, y = 2, z = 3 }; };"#,
        expect_test::expect![[r#"
            y Field (usize)
            z Field (usize)
        "#]],
    );
}

#[test]
fn completions_record_pattern_partial_prefix_classifies_and_edit_covers_the_prefix() {
    // A half-typed field name still classifies as a record-pattern field
    // slot (filtering is client-side, by `filter_text`); the field slot the
    // cursor sits in does not exclude itself, and the edit replaces exactly
    // the typed prefix.
    let (analysis, _file, pos) =
        fixture(r#"static f = fn { let struct { na$0 } = struct { name = 1, note = 2 }; };"#);
    let items = analysis.completions(pos);
    let field = items
        .iter()
        .find(|c| c.label == "name")
        .expect("`name` offered even though only `na` was typed");
    assert_eq!(field.kind, crate::CompletionItemKind::Field);
    let prefix_start = pos.offset - syntax::TextSize::new(2);
    assert_eq!(field.text_edit.range.start(), prefix_start);
    assert_eq!(field.text_edit.range.end(), pos.offset);
}

#[test]
fn completions_record_pattern_undeterminable_type_offers_no_field_items() {
    // An un-annotated parameter has no determinable record type flowing into
    // the destructure — offer nothing rather than guessing (and a record
    // pattern's field slot has no other generally-valid candidates).
    let (analysis, _file, pos) = fixture("static f = fn (p) { let struct { $0 } = p; 1 };");
    assert_eq!(analysis.completions(pos), Vec::new());
}

#[test]
fn completions_record_pattern_non_record_type_offers_no_field_items() {
    // Destructuring a non-record value is a type error, but completion must
    // not invent field items for it.
    let (analysis, _file, pos) = fixture("static f = fn { let struct { $0 } = 1; };");
    assert!(
        !analysis
            .completions(pos)
            .iter()
            .any(|c| c.kind == crate::CompletionItemKind::Field),
        "no field items for a non-record scrutinee"
    );
}

#[test]
fn completions_record_pattern_rename_slot_offers_nothing() {
    // The `as`-rename target (`x as <cursor>`) is a brand-new binding name,
    // not a field selector — nothing to complete there.
    check_no_completion(
        r#"static f = fn { let struct { x as $0 } = struct { x = 1 }; };"#,
        "x",
    );
}

#[test]
fn completions_variant_item_detail_shows_payload_shape() {
    // Detail assertion for variant items: a `::` segment candidate renders
    // its payload signature the way its declaration writes it.
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static s = Shape::$0;
"#,
        expect_test::expect![[r#"
            Circle EnumMember (Circle(usize))
            Point EnumMember (Point)
        "#]],
    );
}

// ---- The `match` arm-list template ----

/// The sort key of one labelled candidate — read directly rather than
/// eyeballing a rendered order.
fn completion_sort_text(fixture_text: &str, label: &str) -> String {
    let (analysis, _file, pos) = fixture(fixture_text);
    let items = analysis.completions(pos);
    items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no completion labeled {label:?}: {items:?}"))
        .sort_text
        .clone()
}

#[test]
fn completions_match_template_writes_every_variant_as_an_arm() {
    // The whole rest of the statement, house-formatted: arms one level in
    // from the `match` keyword's own line, closing brace back at it. Tab
    // stops run in DOCUMENT order — a variant's payload bindings, then its
    // arm body, then on to the next arm.
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s $0
};
"#,
            "match arms",
        ),
        crate::InsertText::Snippet {
            snippet: "{\n        ::Circle($1) => $2,\n        ::Point => $3,\n    }".to_owned(),
            plain: "{\n        ::Circle => ,\n        ::Point => ,\n    }".to_owned(),
        }
    );
}

#[test]
fn completions_match_template_gives_each_payload_its_own_tab_stop() {
    // `check_match_pat` counts a pattern's bindings against the variant's
    // payloads, so a two-payload variant needs two stops: one `$1` covering
    // both would insert `::Pair($1)` and hand the user an arity error to
    // fix. The numbering keeps running across arms.
    let crate::InsertText::Snippet { snippet, .. } = completion_insert(
        r#"
type Shape = enum { Pair(usize, str), Point };
static f = fn (s: Shape) {
    match s $0
};
"#,
        "match arms",
    ) else {
        panic!("the template is a snippet");
    };
    assert_eq!(
        snippet,
        "{\n        ::Pair($1, $2) => $3,\n        ::Point => $4,\n    }"
    );
}

#[test]
fn completions_match_template_indents_from_the_match_keyword() {
    // Not from the cursor's line and not from a fixed column: the arm list
    // is measured off the line the `match` itself sits on, so a nested one
    // lands where a hand-written arm list would.
    let crate::InsertText::Snippet { snippet, .. } = completion_insert(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    loop {
        match s $0
    }
};
"#,
        "match arms",
    ) else {
        panic!("the template is a snippet");
    };
    assert_eq!(
        snippet,
        "{\n            ::Circle($1) => $2,\n            ::Point => $3,\n        }"
    );
}

#[test]
fn completions_match_template_plain_fallback_carries_no_tab_stops() {
    // A snippet-incapable client must never see a literal `$1`. The parens
    // go with the stops rather than being left empty, the same call
    // `match_arm_items` makes for a single payload variant: `::Circle()`
    // would claim an arity of zero, and there is no name to invent.
    let crate::InsertText::Snippet { plain, .. } = completion_insert(
        r#"
type Shape = enum { Pair(usize, str), Point };
static f = fn (s: Shape) {
    match s $0
};
"#,
        "match arms",
    ) else {
        panic!("the template is a snippet");
    };
    assert!(
        !plain.contains('$'),
        "plain fallback still has tab stops: {plain:?}"
    );
    assert_eq!(plain, "{\n        ::Pair => ,\n        ::Point => ,\n    }");
}

#[test]
fn completions_match_template_ranks_first_but_suppresses_nothing() {
    // Additive, not a replacement: the position also classifies as an
    // ordinary fresh statement (the splice detaches the marker from the
    // arm-less `match`), and those candidates keep their place. The
    // template is gold, so it leads.
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s $0
};
"#,
        expect_test::expect![[r#"
            match arms Snippet (all 2 variants of Shape)
            s Variable (Shape)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Shape Enum (enum { Circle(usize), Point })
            Utf8Result Enum (enum { Ok(str), Err })
            f Function (fn(Shape) -> !)
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            panic Function (fn(str) -> !)
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            let Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_match_template_not_offered_without_an_enum_scrutinee() {
    // No variants to predict: an integer, a struct-typed value and a
    // scrutinee that resolves to nothing at all each offer no template
    // (they keep their ordinary expression candidates, which the caller
    // sees as "nothing changed here").
    for fixture_text in [
        "static f = fn (s: usize) { match s $0 };",
        "type P = struct { x: usize };\nstatic f = fn (s: P) { match s $0 };",
        "static f = fn { match nope $0 };",
    ] {
        check_no_completion(fixture_text, "match arms");
    }
}

#[test]
fn completions_match_template_not_offered_for_a_character_scrutinee() {
    // A `char` DISPATCHES (its literal patterns do), so it is a scrutinee
    // the arm-slot logic takes seriously — but there is no variant list to
    // write out, so there is no template to offer. The arm slot still
    // offers the `_` a `char` match always needs.
    check_no_completion("static f = fn (c: char) { match c $0 };", "match arms");
    check_completions(
        r#"
static f = fn (c: char) {
    match c {
        $0
    }
};
"#,
        expect_test::expect![[r#"
            _ Keyword
        "#]],
    );
}

#[test]
fn completions_match_template_not_offered_while_the_scrutinee_is_typed() {
    // At `match s|` the typed prefix IS the scrutinee's own text — the user
    // is still naming the value, and wrapping an arm list around a
    // half-written name would be wrong. The scrutinee slot answers instead.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) { match s$0 };
"#;
    check_no_completion(fixture_text, "match arms");
    check_has_completion(fixture_text, "s");
}

#[test]
fn completions_match_template_not_offered_once_an_arm_list_exists() {
    // The evidence lives in the REAL tree: splicing the marker in detaches
    // the written arm list from its `match`, so the speculative tree would
    // claim there is none. A template offered here would duplicate arms the
    // user already wrote. Two shapes: no `{` yet (the arm list still parses
    // as a sibling block once spliced), and `{` already there with a real
    // arm inside it — `match_awaiting_arms`'s `arms().next().is_some()`
    // check is what catches the second one; an empty `{}` is the ONLY
    // braced case the template still fires for (see
    // `completions_match_template_offered_inside_empty_braces` below).
    check_no_completion(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) { match s $0{ ::Point => 1 } };
"#,
        "match arms",
    );
    check_no_completion(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) { match s { ::Point => 1, $0 } };
"#,
        "match arms",
    );
}

#[test]
fn completions_match_template_offered_inside_empty_braces() {
    // `match s {|}` — an editor that auto-closes `{` (Zed does, instantly)
    // leaves the cursor exactly here; an explicit invoke at this position
    // must still fire the template (this crate has no notion of "trigger
    // character" at all — that's a `must-lsp` capability concern; see
    // `must_lsp::server_capabilities`'s comment, and P12, on why `{` isn't
    // one spontaneously), but without writing braces of its own: the pair
    // already in the tree is the client's auto-close, and the server has no
    // way to tell it to delete a second one. Compare the no-braces snippet
    // (`completions_match_template_writes_every_variant_as_an_arm`): same
    // arm text, minus the `{` and the `}`.
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {$0}
};
"#,
            "match arms",
        ),
        crate::InsertText::Snippet {
            snippet: "\n        ::Circle($1) => $2,\n        ::Point => $3,\n    ".to_owned(),
            plain: "\n        ::Circle => ,\n        ::Point => ,\n    ".to_owned(),
        }
    );
}

#[test]
fn completions_match_template_offered_inside_empty_braces_across_whitespace() {
    // The same slot, spread over its own lines — the whitespace/newline
    // variant of `match s {|}`. Detection is pure range arithmetic
    // (`l_brace.end() <= edit_range.start()` and
    // `edit_range.end() <= r_brace.start()`), so it doesn't care that real
    // whitespace now sits between the cursor and each brace; only
    // the exact tight-braces case above pins the snippet text byte for
    // byte, since here the pre-existing blank line's own indentation stays
    // in the buffer alongside whatever the template inserts.
    check_has_completion(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        $0
    }
};
"#,
        "match arms",
    );
}

#[test]
fn completions_match_template_not_offered_when_the_brace_belongs_to_the_enclosing_block() {
    // `match s {` left unclosed, with no `}` of its own anywhere before the
    // enclosing block's — the parser's error recovery
    // (`grammar.rs`'s `expect_after_prev(R_BRACE)`) assigns that borrowed
    // `}` to the match rather than reporting one missing, so
    // `match_expr.r_brace_token()` returns non-`None` here too. Accepting
    // the template in this shape would leave the match still unclosed and
    // delete the enclosing block's only closing brace. `match_awaiting_arms`
    // tells the two apart by indentation: the borrowed `}` sits at the
    // enclosing block's shallower indent (0, here) rather than at or past
    // the `match` keyword's own line indent (4).
    check_no_completion(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {$0
};
"#,
        "match arms",
    );
}

#[test]
fn completions_match_template_survives_a_typed_prefix_after_the_scrutinee() {
    // The real tree's parser gives up on the match right after the
    // scrutinee once it meets an unexpected token, so a typed prefix in the
    // arm-list slot (`match s n˽`) looks — from the cursor's own token —
    // like ordinary text outside any `MATCH_EXPR`. Anchoring the ancestor
    // walk at `edit_range.start()` (before the prefix) rather than the
    // cursor keeps it inside the match, so the template survives the first
    // keystroke instead of vanishing.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s n$0
};
"#;
    check_has_completion(fixture_text, "match arms");
}

#[test]
fn completions_match_template_leads_even_under_a_matching_expectation() {
    // The template's type tier is pinned to `0` rather than derived from an
    // expectation it cannot have (a whole statement has no "type" to
    // compare): without the pin, a typed prefix that happens to satisfy a
    // surrounding expectation exactly (tier `0`) would outrank the template
    // (tier `2`, `TYPE_TIER_NONE`) even though the template is the one
    // answer the grammar admits at this position.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape, n: usize) {
    let r: usize = match s n$0
};
"#;
    assert_eq!(
        completion_sort_text(fixture_text, "match arms"),
        "0_00_match arms"
    );
}

#[test]
fn completions_match_arm_pattern_gives_each_payload_its_own_tab_stop() {
    // The pattern slot's own arity fix, matching the template's: a
    // two-payload variant names two bindings. Both the bare (sigil-
    // inserting) slot and a slot already past a `::` carry it.
    let bare = r#"
type Shape = enum { Pair(usize, str), Point };
static f = fn (s: Shape) {
    match s {
        $0
    }
};
"#;
    assert_eq!(
        completion_insert(bare, "::Pair"),
        crate::InsertText::Snippet {
            snippet: "::Pair($1, $2)".to_owned(),
            plain: "::Pair".to_owned(),
        }
    );
    let qualified = r#"
type Shape = enum { Pair(usize, str), Point };
static f = fn (s: Shape) {
    match s {
        Shape::$0
    }
};
"#;
    assert_eq!(
        completion_insert(qualified, "Pair"),
        crate::InsertText::Snippet {
            snippet: "Pair($1, $2)".to_owned(),
            plain: "Pair".to_owned(),
        }
    );
}

// ---- match-scrutinee ranking ----

/// Every layer of the scrutinee ranking in one body: a `let` in the
/// cursor's own scope, one further out, a fn parameter, a file item — and a
/// non-enum local, which stays where every local has always sorted.
const SCRUTINEE_LAYERS: &str = r#"
type Shape = enum { Circle(usize), Point };
static ambient: Shape = Shape::Point;
static f = fn (param: Shape) {
    let outer: Shape = param;
    let noise: usize = 1;
    {
        let near: Shape = outer;
        match $0
    }
};
"#;

#[test]
fn completions_match_scrutinee_ranks_by_definition_scope_distance() {
    // Nearest definition scope first, all the way out: this scope's `let`,
    // then the enclosing scope's, then the enclosing `fn`'s parameter, then
    // the file. The numbers are real chain distances, not a hand-made tier
    // list — `noise` sits one hop nearer than `outer` and so pushes it from
    // 01 to 02, which is exactly what a `let` between them does to the
    // scope chain.
    assert_eq!(completion_sort_text(SCRUTINEE_LAYERS, "near"), "2_00_near");
    assert_eq!(
        completion_sort_text(SCRUTINEE_LAYERS, "outer"),
        "2_02_outer"
    );
    assert_eq!(
        completion_sort_text(SCRUTINEE_LAYERS, "param"),
        "2_03_param"
    );
    assert_eq!(
        completion_sort_text(SCRUTINEE_LAYERS, "ambient"),
        "2_08_ambient"
    );
    // Not enum-typed: no lift at all, the ordinary local tier.
    assert_eq!(
        completion_sort_text(SCRUTINEE_LAYERS, "noise"),
        "2_10_noise"
    );
}

#[test]
fn completions_match_scrutinee_lifts_a_character_typed_local() {
    // The scrutinee slot's lift follows `hir::dispatches_on`, so it follows
    // it to `char` too: a `char` local leads the slot the way an enum one
    // does, while a `usize` local stays on the flat local tier.
    const CHAR_SCRUTINEE: &str = r#"
static f = fn () {
    let n = 0;
    let c = 'x';
    match $0
};
"#;
    assert_eq!(completion_sort_text(CHAR_SCRUTINEE, "c"), "2_00_c");
    assert_eq!(completion_sort_text(CHAR_SCRUTINEE, "n"), "2_10_n");
}

#[test]
fn completions_match_scrutinee_does_not_suppress_the_normal_set() {
    // The enum-typed values lead; everything an expression position would
    // otherwise offer still follows, in its usual order.
    check_completions(
        SCRUTINEE_LAYERS,
        expect_test::expect![[r#"
            near Variable (Shape)
            outer Variable (Shape)
            param Variable (Shape)
            ambient Constant (Shape)
            noise Variable (usize)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            ReadLineResult Enum (enum { Line(str), End })
            Shape Enum (enum { Circle(usize), Point })
            Utf8Result Enum (enum { Ok(str), Err })
            f Function (fn(Shape) -> !)
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            panic Function (fn(str) -> !)
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn completions_match_scrutinee_ranking_survives_a_typed_prefix() {
    // The slot classifies the same whether the marker stands alone or is
    // glued to a prefix, so the ranking is the same too — otherwise the
    // list would reshuffle under the user's first keystroke.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (param: Shape) {
    let near: Shape = param;
    match n$0
};
"#;
    assert_eq!(completion_sort_text(fixture_text, "near"), "2_00_near");
    assert_eq!(completion_sort_text(fixture_text, "param"), "2_01_param");
}

#[test]
fn completions_match_scrutinee_lifts_a_variant_typed_local() {
    // A variant-typed value is a legal scrutinee (it simply has one
    // reachable arm), so it belongs in the leading layer next to the
    // enum-typed ones.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn () {
    let c: Shape::Circle = Shape::Circle(1);
    match $0
};
"#;
    assert_eq!(completion_sort_text(fixture_text, "c"), "2_00_c");
}

#[test]
fn completions_match_scrutinee_ranking_stops_at_the_match() {
    // Only the scrutinee slot re-ranks. The same locals in an ordinary
    // expression position keep the flat local tier — a ranking that leaked
    // would reorder every completion list in the file.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (param: Shape) {
    let near: Shape = param;
    print($0);
};
"#;
    assert_eq!(completion_sort_text(fixture_text, "near"), "2_10_near");
    assert_eq!(completion_sort_text(fixture_text, "param"), "2_10_param");
}

#[test]
fn completions_record_literal_brace_still_offers_fields_not_a_template() {
    // A request landing right after a record literal's own auto-closed
    // brace (however it got here — an explicit invoke, since `{` is not a
    // trigger character; see `must_lsp::server_capabilities`) must not be
    // confused for a match's. Nothing here is a `match`, so
    // `match_awaiting_arms` finds no enclosing `MatchExpr` and declines —
    // the position keeps answering exactly what it always has (the
    // missing-field candidates).
    let fixture_text = r#"
type Point = struct { x: usize, y: usize };
static f = fn {
    let p = Point(struct {$0});
};
"#;
    check_no_completion(fixture_text, "match arms");
    check_has_completion(fixture_text, "x");
    check_has_completion(fixture_text, "y");
}

#[test]
fn completions_block_brace_still_offers_statement_candidates_not_a_template() {
    // A plain block's braces, no `match` anywhere around it: the ordinary
    // statement-start candidates (a visible `let` among them) show up, not
    // a match template.
    let fixture_text = r#"
static f = fn {
    if true {$0}
};
"#;
    check_no_completion(fixture_text, "match arms");
    check_has_completion(fixture_text, "let");
}

#[test]
fn completions_match_template_not_offered_at_other_non_match_brace_positions() {
    // The remaining `{` positions the template slot could in principle be
    // confused with — a `const { … }` block and a `with { … }` chain body —
    // decline the template for the same reason as the record-literal/block
    // cases above: no `MatchExpr` ancestor for `match_awaiting_arms` to find.
    // Only a sanity check that each answers rather than panicking or
    // hanging; the two cases above already pin the "normal candidates
    // still show up" half of the behavior.
    for fixture_text in [
        "static f = fn { let x = const {$0}; };",
        "type Counter = struct { n: usize } with {$0};",
    ] {
        check_no_completion(fixture_text, "match arms");
    }
}

// ---- generics: ide smoke + real evaluation ----
//
// The no-panic smoke tests keep their smoke shape: completions/hover with the
// cursor inside turbofish/binder syntax must not blow up; no assertion on
// the exact results.

#[test]
fn completions_do_not_panic_near_a_turbofish() {
    let (analysis, _file, pos) = fixture(
        "static f = fn (x: usize) -> usize { x };\nstatic g = fn () -> usize { f::<usize$0>(1) };",
    );
    let _ = analysis.completions(pos);
}

#[test]
fn hover_does_not_panic_near_a_turbofish() {
    let (analysis, _file, pos) = fixture(
        "static f = fn (x: usize) -> usize { x };\nstatic g = fn () -> usize { f::<usize$0>(1) };",
    );
    let _ = analysis.hover(pos);
}

#[test]
fn completions_do_not_panic_inside_a_generic_fn_binder() {
    let (analysis, _file, pos) = fixture("static id = fn::<T$0>(x: T) -> T { x };");
    let _ = analysis.completions(pos);
}

#[test]
fn completions_do_not_panic_inside_a_generic_body() {
    let (analysis, _file, pos) = fixture("static id = fn::<T, const N: usize>(x: T) -> T { $0x };");
    let _ = analysis.completions(pos);
}

#[test]
fn a_turbofish_never_completes_a_region() {
    // Regions are elided at every call site, so nothing inside `::<` may
    // offer one. Two independent reasons hold it — the candidate sources
    // are the TYPE scope (no region is in it) and the `@` sigil lexes the
    // completion marker into one `REGION_IDENT`, which the request does not
    // survive. The test pins the OUTCOME, so either one moving is caught.
    let (analysis, _file, pos) = fixture(
        "static get = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };\n\
         static main = fn::<@b>(p: usize.&::<@b>) -> usize { get::<$0>(p) };",
    );
    let offered = analysis.completions(pos);
    assert!(
        !offered.iter().any(|item| item.label.starts_with('@')),
        "a turbofish offered a region: {:?}",
        offered.iter().map(|item| &item.label).collect::<Vec<_>>()
    );
    // And after the sigil there is nothing at all to offer.
    let (analysis, _file, pos) = fixture(
        "static get = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };\n\
         static main = fn::<@b>(p: usize.&::<@b>) -> usize { get::<@$0>(p) };",
    );
    assert!(analysis.completions(pos).is_empty());
}

#[test]
fn hover_does_not_panic_on_a_rigid_param_annotation() {
    // Hover on the `T` of `x: T` inside a generic body.
    let (analysis, _file, pos) = fixture("static id = fn::<T>(x: T$0) -> T { x };");
    let _ = analysis.hover(pos);
}

#[test]
fn hover_shows_the_rigid_param_through_the_bound_value() {
    // Hover on `x` in the body: its type is the rigid param, displayed by
    // its binder name.
    let (analysis, _file, pos) = fixture("static id = fn::<T>(x: T) -> T { x$0 };");
    if let Some(hover) = analysis.hover(pos) {
        assert!(
            hover.markup.contains('T'),
            "hover should mention the param: {:?}",
            hover.markup
        );
    }
}

#[test]
fn generic_call_in_an_initializer_evaluates_at_check_time() {
    // The staging refusal is gone: the same fixture now EVALUATES —
    // zero diagnostics, and hovering the item shows the computed value.
    let (analysis, file, _pos) = fixture(
        "static cid = const fn::<T>(x: T) -> T { x };\nstatic v: usize = cid::<usize>(4);$0",
    );
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, vec![], "expected a clean file");
    check_hover(
        "static cid = const fn::<T>(x: T) -> T { x };\nstatic v$0: usize = cid::<usize>(4);",
        "```must\nv: usize = 4\n```",
    );
}

#[test]
fn generic_const_arg_panic_is_a_single_diagnostic_at_the_arg() {
    // A panicking const argument surfaces once, at the argument's range:
    // `const_arg_values` reports it there, and the initializer's own
    // forcing (which fails at the same origin) dedups against it — no
    // double diagnostics.
    let source = "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\n\
                  static y: usize = rep::<const { panic(\"nope\") }>(14);";
    let (analysis, file, _pos) = fixture(&format!("{source}$0"));
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].message, "constant evaluation panicked: nope");
    assert_eq!(
        &source[usize::from(diagnostics[0].range.start())..usize::from(diagnostics[0].range.end())],
        "panic(\"nope\")",
        "the squiggle sits inside the argument"
    );
}

#[test]
fn goto_definition_on_a_const_param_jumps_to_the_binder() {
    check_goto(
        "static f = fn::<const N: usize>() -> usize { N$0 + 1 };",
        "N",
        0,
    );
}

#[test]
fn hover_shows_generic_type_instance() {
    check_hover(
        "type Pair = struct::<T> { a: T, b: T };\nstatic main = fn { let p$0 = Pair::<usize>(struct { a = 1, b = 2 }); };",
        "```must\np: Pair::<usize>\n```",
    );
}

#[test]
fn hover_shows_generic_variant_instance() {
    check_hover(
        "type Option = enum::<T> { Some(T), None };\nstatic main = fn { let o$0 = Option::<usize>::Some(3); };",
        "```must\no: Option::<usize>::Some\n```",
    );
}

#[test]
fn hover_raw_pointer_binding_shows_the_pointer_type() {
    check_hover(
        "static main = fn { let mut x: usize = 1; let p$0 = x.&raw mut; };",
        "```must\np: usize.&raw mut\n```",
    );
}

#[test]
fn hover_deref_receiver_shows_the_pointer_type() {
    check_hover(
        "static main = fn() -> usize { let mut x = 1; let p = x.&raw mut; unsafe { p$0.* } };",
        "```must\np: usize.&raw mut\n```",
    );
}

// ---- fixed-size arrays ----

#[test]
fn hover_array_local() {
    check_hover(
        r#"static main = fn { let a: [usize; 3] = [1, 2, 3]; let x = a$0[0]; };"#,
        "```must\na: [usize; 3]\n```",
    );
}

#[test]
fn hover_array_binding_definition() {
    check_hover(
        r#"static main = fn { let m$0: [[usize; 2]; 2] = [[1, 2], [3, 4]]; let x = m[0][1]; };"#,
        "```must\nm: [[usize; 2]; 2]\n```",
    );
}

#[test]
fn goto_through_index_chain() {
    check_goto(
        r#"
static f = fn {
    let buf = [1, 2];
    let x = buf$0[0];
}
"#,
        "buf",
        0,
    );
}

#[test]
fn highlights_survive_array_syntax() {
    check_highlights(
        r#"static f = fn (i: usize) { let mut a = [1, 2]; a[i] = a[0]; };"#,
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..16 "i" Parameter.declaration
            18..23 "usize" Type.defaultLibrary
            27..30 "let" Keyword
            31..34 "mut" Keyword
            35..36 "a" Variable.declaration.mutable
            37..38 "=" Operator
            40..41 "1" Number
            43..44 "2" Number
            47..48 "a" Variable.mutable
            49..50 "i" Parameter
            52..53 "=" Operator
            54..55 "a" Variable.mutable
            56..57 "0" Number
        "#]],
    );
}

#[test]
fn hover_unpinned_number_renders_as_number() {
    // A literal no defining use ever pinned hovers as `{number}` — never a
    // silently-defaulted type.
    check_hover(
        "static f = fn { let n$0 = 3; };",
        "```must\nn: {number}\n```",
    );
}

#[test]
fn hover_sized_integer_binding_shows_its_width() {
    check_hover("static f = fn { let n$0: u8 = 3; };", "```must\nn: u8\n```");
}

// ---- inherent members and dot-calls -------------------------------------

const MEMBER_FIXTURE_HEAD: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Self(struct { n = c.n + by }) };
    }
};
"#;

#[test]
fn hover_on_member_call_name_shows_the_member_signature() {
    check_hover(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).get$0() }};"
        ),
        "```must\nget: fn(Counter) -> usize\n```",
    );
}

#[test]
fn hover_on_member_name_declaration_shows_its_signature() {
    check_hover(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get$0 = fn(c: Self) -> usize { c.n };
    }
};
"#,
        "```must\nget: fn(Counter) -> usize\n```",
    );
}

#[test]
fn hover_on_a_member_with_its_own_type_binder_shows_the_binder() {
    // A member's OWN type parameter is part of its signature, and hover
    // renders it rigid — `U`, not a resolved type — because that is what
    // the DECLARATION says. (The owner's `T` reads the same way; the two
    // halves of the binder are indistinguishable here, deliberately.)
    check_hover(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to$0 = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
"#,
        "```must
map_to: fn(fn(T) -> U, Option::<T>) -> U
```",
    );
}

#[test]
fn hover_on_a_member_turbofish_call_shows_the_instantiated_signature() {
    // At the USE site the same member reads instantiated — the owner's
    // argument from the receiver, the member's own from the turbofish.
    check_hover(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U {
            match s { ::Some(t) => f(t), ::None => panic("none") }
        }
    }
};
static width = fn(n: usize) -> bool { n > 2 };
static main = fn(o: Option::<usize>) -> bool { o.map_to$0::<bool>(width) };
"#,
        "```must
map_to: fn(fn(usize) -> bool, Option::<usize>) -> bool
```",
    );
}

#[test]
fn completions_survive_a_member_turbofish() {
    // The dot's new turbofish position, at the three places a user's
    // cursor lands while typing one: inside the argument list, right after
    // it, and inside the call's arguments. None may panic, and the last
    // must still see the locals in scope.
    for fixture_text in [
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U { panic("x") }; }
};
static main = fn(o: Option::<usize>, w: fn(usize) -> bool) -> bool { o.map_to::<b$0>(w) };
"#,
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U { panic("x") }; }
};
static main = fn(o: Option::<usize>, w: fn(usize) -> bool) -> bool { o.map_to::<bool>$0(w) };
"#,
    ] {
        let (analysis, _file, pos) = fixture(fixture_text);
        let _ = analysis.completions(pos);
    }
    check_completions(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self { map_to = fn::<U>(f: fn(T) -> U, s: Self) -> U { panic("x") }; }
};
static main = fn(o: Option::<usize>, wide: fn(usize) -> bool) -> bool {
    o.map_to::<bool>(wid$0)
};
"#,
        expect_test::expect![[r#"
            wide Variable (fn(usize) -> bool)
            panic Function (fn(str) -> !)
            o Variable (Option::<usize>)
            AllocResult Enum (enum { Ok(T.&raw mut), Err })
            NextChar Enum (enum { Char(char, usize), End })
            Option Enum (enum { Some(T), None })
            ReadLineResult Enum (enum { Line(str), End })
            Utf8Result Enum (enum { Ok(str), Err })
            main Function (fn(Option::<usize>, fn(usize) -> bool) -> bool)
            add Function (unsafe fn(T.&raw [mut], usize) -> T.&raw [mut])
            alloc_array Function (fn::<T>(usize) -> AllocResult::<T>)
            copy Function (unsafe fn(T.&raw [mut], T.&raw mut, usize))
            dangling Function (fn::<T>() -> T.&raw mut)
            dealloc_array Function (unsafe fn::<T>(T.&raw mut, usize))
            offset Function (unsafe fn(T.&raw [mut], isize) -> T.&raw [mut])
            print Function (fn(str))
            read_line Function (fn() -> ReadLineResult)
            str_from_utf8 Function (unsafe fn(u8.&raw [mut], usize) -> Utf8Result)
            str_from_utf8_unchecked Function (unsafe fn(u8.&raw [mut], usize) -> str)
            const Keyword
            false Keyword
            fn Keyword
            if Keyword
            loop Keyword
            match Keyword
            struct Keyword
            true Keyword
            unsafe Keyword
        "#]],
    );
}

#[test]
fn a_dot_call_with_a_member_turbofish_still_highlights_as_a_function() {
    // The member name keeps its `function` token with the new node sitting
    // beside it — `MEMBER_GENERIC_ARGS` is a CHILD of the `FIELD_EXPR`, so
    // the call is still the field expression's parent and the resolution
    // lookup is unchanged.
    check_highlights(
        r#"type C = struct { n: usize } with {
    impl Self { pick = fn::<U>(alt: U, c: Self) -> U { alt }; }
};
static main = fn(c: C) -> bool { c.pick::<bool>(true) };
"#,
        expect_test::expect![[r#"
            0..4 "type" Keyword
            5..6 "C" Type.declaration
            7..8 "=" Operator
            9..15 "struct" Keyword
            21..26 "usize" Type.defaultLibrary
            29..33 "with" Keyword
            40..44 "impl" Keyword
            45..49 "Self" Type
            52..56 "pick" Function.declaration
            57..58 "=" Operator
            59..61 "fn" Keyword
            63..64 "<" Operator
            64..65 "U" TypeParameter.declaration
            65..66 ">" Operator
            67..70 "alt" Parameter.declaration
            72..73 "U" TypeParameter
            75..76 "c" Parameter.declaration
            78..82 "Self" Type
            84..86 "->" Operator
            87..88 "U" TypeParameter
            91..94 "alt" Parameter
            103..109 "static" Keyword
            110..114 "main" Function.declaration.static
            115..116 "=" Operator
            117..119 "fn" Keyword
            120..121 "c" Parameter.declaration
            123..124 "C" Type
            126..128 "->" Operator
            129..133 "bool" Type.defaultLibrary
            136..137 "c" Parameter
            138..142 "pick" Function
            144..145 "<" Operator
            145..149 "bool" Type.defaultLibrary
            149..150 ">" Operator
            151..155 "true" Keyword
        "#]],
    );
}

#[test]
fn hover_inside_member_body_resolves_locals() {
    check_hover(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c$0.n };
    }
};
"#,
        "```must\nc: Counter\n```",
    );
}

#[test]
fn goto_definition_on_member_call_name_lands_on_the_member() {
    check_goto(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).get$0() }};"
        ),
        "get",
        0,
    );
}

#[test]
fn goto_definition_on_a_qualified_member_path_lands_on_the_member() {
    // The G13 escape `Type::member` names exactly one member — one click
    // away, like the dot-call's name.
    check_goto(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter::get$0(Counter(struct {{ n = 1 }})) }};"
        ),
        "get",
        0,
    );
}

#[test]
fn dot_completions_offer_members_next_to_fields() {
    check_has_completion(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).$0 }};"
        ),
        "bump",
    );
    check_has_completion(
        &format!(
            "{MEMBER_FIXTURE_HEAD}static main = fn() -> usize {{ Counter(struct {{ n = 1 }}).$0 }};"
        ),
        "n",
    );
}

#[test]
fn dot_completions_on_a_borrow_receiver_follow_the_receiver_shape() {
    // The dot offers what a call would accept: through a `&mut` receiver
    // that is both borrow members and not the value one; through a shared
    // receiver only the shared one. Fields are never offered — projecting
    // through a borrow would be auto-deref.
    const BORROW_FIXTURE: &str = r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        read = fn::<@r>(c: Self.&::<@r>) -> usize { c.*.n };
        bump = fn::<@r>(c: Self.&mut::<@r>) -> () { c.*.n = 1; };
    }
};
"#;
    let exclusive =
        format!("{BORROW_FIXTURE}static main = fn::<@a>(c: Counter.&mut::<@a>) -> () {{ c.$0 }};");
    check_has_completion(&exclusive, "read");
    check_has_completion(&exclusive, "bump");
    check_no_completion(&exclusive, "get");
    check_no_completion(&exclusive, "n");
    let shared =
        format!("{BORROW_FIXTURE}static main = fn::<@a>(c: Counter.&::<@a>) -> () {{ c.$0 }};");
    check_has_completion(&shared, "read");
    check_no_completion(&shared, "bump");
}

#[test]
fn member_bodies_highlight_as_code() {
    check_highlights(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static use_it = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect_test::expect![[r#"
            1..5 "type" Keyword
            6..7 "A" Type.declaration
            8..9 "=" Operator
            10..16 "struct" Keyword
            22..27 "usize" Type.defaultLibrary
            30..34 "with" Keyword
            41..45 "impl" Keyword
            46..50 "Self" Type
            61..64 "get" Function.declaration
            65..66 "=" Operator
            67..69 "fn" Keyword
            70..71 "a" Parameter.declaration
            73..77 "Self" Type
            79..81 "->" Operator
            82..87 "usize" Type.defaultLibrary
            90..91 "a" Parameter
            106..112 "static" Keyword
            113..119 "use_it" Function.declaration.static
            120..121 "=" Operator
            122..124 "fn" Keyword
            127..129 "->" Operator
            130..135 "usize" Type.defaultLibrary
            138..139 "A" Type
            140..146 "struct" Keyword
            151..152 "=" Operator
            153..154 "1" Number
            158..161 "get" Function
        "#]],
    );
}

// ---- trait declarations: hover and goto ---------------------------------

#[test]
fn hover_on_bound_shows_the_trait() {
    check_hover(
        r#"
trait Display = requires { fmt: fn(x: Self) -> str; };
static f = fn::<T: Disp$0lay>(x: T) -> usize { 1 };
"#,
        "```must\ntrait Display = requires { fmt: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_decl_name() {
    check_hover(
        r#"
trait Disp$0lay = requires { fmt: fn(x: Self) -> str; };
"#,
        "```must\ntrait Display = requires { fmt: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_impl_head_shows_the_trait() {
    check_hover(
        r#"
trait Show = requires { show: fn(x: Self) -> str; };
type P = struct { a: usize } with {
    impl Sh$0ow { show = fn(x: Self) -> str { "p" }; }
};
"#,
        "```must\ntrait Show = requires { show: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_member_dot_call_shows_instantiated_signature() {
    check_hover(
        r#"
trait D = requires { m: fn(x: Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { n.m$0() };
"#,
        "```must\nm: fn(usize) -> str\n```",
    );
}

#[test]
fn hover_on_qualified_call_base_shows_the_trait() {
    check_hover(
        r#"
trait D = requires { m: fn(x: Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D$0::m(n) };
"#,
        "```must\ntrait D = requires { m: fn(Self) -> str; }\n```",
    );
}

#[test]
fn hover_on_trait_impl_member_name_shows_signature() {
    check_hover(
        r#"
trait D = requires { m: fn(x: Self) -> str; } with {
    impl usize { m$0 = fn(x: usize) -> str { "n" }; }
};
"#,
        "```must\nm: fn(usize) -> str\n```",
    );
}

#[test]
fn goto_bound_lands_on_the_trait_decl() {
    check_goto(
        r#"
trait Display = requires { fmt: fn(x: Self) -> str; };
static f = fn::<T: Displ$0ay>(x: T) -> usize { 1 };
"#,
        "Display",
        0,
    );
}

#[test]
fn goto_impl_head_lands_on_the_trait_decl() {
    check_goto(
        r#"
trait Show = requires { show: fn(x: Self) -> str; };
type P = struct { a: usize } with {
    impl Sh$0ow { show = fn(x: Self) -> str { "p" }; }
};
"#,
        "Show",
        0,
    );
}

#[test]
fn goto_impl_directed_dot_call_lands_on_the_impl_member() {
    check_goto(
        r#"
trait D = requires { mem: fn(x: Self) -> str; } with {
    impl usize { mem = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { n.mem$0() };
"#,
        // Occurrence 0 is the requirement, 1 is the impl member.
        "mem",
        1,
    );
}

#[test]
fn goto_bound_directed_dot_call_lands_on_the_requirement() {
    check_goto(
        r#"
trait D = requires { m: fn(x: Self) -> str; };
static f = fn::<T: D>(x: T) -> str { x.m$0() };
"#,
        "m",
        0,
    );
}

#[test]
fn goto_qualified_member_lands_on_the_impl_member() {
    check_goto(
        r#"
trait D = requires { mem: fn(x: Self) -> str; } with {
    impl usize { mem = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D::mem$0(n) };
"#,
        "mem",
        1,
    );
}

#[test]
fn goto_qualified_call_base_lands_on_the_trait() {
    check_goto(
        r#"
trait D = requires { m: fn(x: Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "n" }; }
};
static f = fn(n: usize) -> str { D$0::m(n) };
"#,
        "D",
        0,
    );
}

// ---- keyword drift guard ----------------------------------------------

#[test]
fn every_canonical_keyword_classifies_as_a_keyword_highlight() {
    // The architectural guard: keyword-ness reaches the highlighter from
    // the syntax crate's ONE canonical table (`syntax::KEYWORDS`, which
    // also generates `from_keyword`/`is_keyword`), never from a list kept
    // here. A keyword added to the language must therefore highlight with
    // no edit to `syntax_highlighting` — and if anyone reintroduces a
    // hand-written match arm that forgets one, this fails.
    for &(text, kind) in syntax::KEYWORDS {
        assert_eq!(
            syntax::SyntaxKind::from_keyword(text),
            Some(kind),
            "`{text}` does not round-trip through the keyword table"
        );
        assert_eq!(
            crate::syntax_highlighting::lexical_tag(kind),
            Some(crate::HlTag::Keyword),
            "`{text}` ({kind:?}) is a keyword but does not classify as one"
        );
    }
    // Identifiers are classified through hir, not by kind — the keyword
    // branch must not swallow them.
    assert_eq!(
        crate::syntax_highlighting::lexical_tag(syntax::SyntaxKind::IDENT),
        None
    );
}

#[test]
fn return_highlights_as_a_keyword_with_no_ide_change() {
    // The keyword-table architecture, validated on a keyword added AFTER
    // the highlighter was written: `return` reached `syntax_kind.rs`'s
    // `keywords!` table as a one-line entry and NOTHING in this crate was
    // touched to teach the highlighter about it. Rendered here on real
    // code rather than only through the drift guard above, so the claim is
    // legible.
    check_highlights(
        r#"static f = fn (c: bool) -> usize {
    if c { return 1; };
    return 2;
};"#,
        expect_test::expect![[r#"
            0..6 "static" Keyword
            7..8 "f" Function.declaration.static
            9..10 "=" Operator
            11..13 "fn" Keyword
            15..16 "c" Parameter.declaration
            18..22 "bool" Type.defaultLibrary
            24..26 "->" Operator
            27..32 "usize" Type.defaultLibrary
            39..41 "if" Keyword
            42..43 "c" Parameter
            46..52 "return" Keyword
            53..54 "1" Number
            63..69 "return" Keyword
            70..71 "2" Number
        "#]],
    );
}

// ---- trait-aware classification ---------------------------------------

#[test]
fn highlights_trait_declaration_and_requirements() {
    check_highlights(
        r#"
trait Show = requires {
    show: fn(x: Self) -> str;
};
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..11 "Show" Trait.declaration
            12..13 "=" Operator
            14..22 "requires" Keyword
            29..33 "show" Function.declaration
            35..37 "fn" Keyword
            38..39 "x" Parameter.declaration
            41..45 "Self" Type
            47..49 "->" Operator
            50..53 "str" Type.defaultLibrary
        "#]],
    );
}

#[test]
fn highlights_trait_names_at_every_occurrence() {
    // One trait, every position it can be written in: the declaration, an
    // impl head in the trait's OWN chain (the trait-side home), an impl
    // head in a type's chain (the type-side home), a bound on a binder,
    // and the base of a qualified member call.
    check_highlights(
        r#"
trait Show = requires {
    show: fn(x: Self) -> str;
} with {
    impl usize {
        show = fn(x: usize) -> str { "n" };
    }
};
type Tag = struct { t: usize } with {
    impl Show {
        show = fn(x: Self) -> str { Show::show(x.t) };
    }
};
static render = fn::<T: Show>(v: T) -> str { v.show() };
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..11 "Show" Trait.declaration
            12..13 "=" Operator
            14..22 "requires" Keyword
            29..33 "show" Function.declaration
            35..37 "fn" Keyword
            38..39 "x" Parameter.declaration
            41..45 "Self" Type
            47..49 "->" Operator
            50..53 "str" Type.defaultLibrary
            57..61 "with" Keyword
            68..72 "impl" Keyword
            73..78 "usize" Type.defaultLibrary
            89..93 "show" Function.declaration
            94..95 "=" Operator
            96..98 "fn" Keyword
            99..100 "x" Parameter.declaration
            102..107 "usize" Type.defaultLibrary
            109..111 "->" Operator
            112..115 "str" Type.defaultLibrary
            118..121 "\"n\"" String
            134..138 "type" Keyword
            139..142 "Tag" Type.declaration
            143..144 "=" Operator
            145..151 "struct" Keyword
            157..162 "usize" Type.defaultLibrary
            165..169 "with" Keyword
            176..180 "impl" Keyword
            181..185 "Show" Trait
            196..200 "show" Function.declaration
            201..202 "=" Operator
            203..205 "fn" Keyword
            206..207 "x" Parameter.declaration
            209..213 "Self" Type
            215..217 "->" Operator
            218..221 "str" Type.defaultLibrary
            224..228 "Show" Trait
            230..234 "show" Function
            235..236 "x" Parameter
            252..258 "static" Keyword
            259..265 "render" Function.declaration.static
            266..267 "=" Operator
            268..270 "fn" Keyword
            272..273 "<" Operator
            273..274 "T" TypeParameter.declaration
            276..280 "Show" Trait
            280..281 ">" Operator
            282..283 "v" Parameter.declaration
            285..286 "T" TypeParameter
            288..290 "->" Operator
            291..294 "str" Type.defaultLibrary
            297..298 "v" Parameter
            299..303 "show" Function
        "#]],
    );
}

#[test]
fn highlights_generic_binders_and_their_uses() {
    // Type params and const params, at the binder and at every use — in
    // signatures, in bodies, and in a `type` declaration's fields (whose
    // binder sits on the RHS literal, in scope for the whole declaration).
    check_highlights(
        r#"
static id = fn::<T>(x: T) -> T { x };
static rep = const fn::<const N: usize>() -> usize { N + N };
type Pair = struct::<T> { a: T, b: T };
"#,
        expect_test::expect![[r#"
            1..7 "static" Keyword
            8..10 "id" Function.declaration.static
            11..12 "=" Operator
            13..15 "fn" Keyword
            17..18 "<" Operator
            18..19 "T" TypeParameter.declaration
            19..20 ">" Operator
            21..22 "x" Parameter.declaration
            24..25 "T" TypeParameter
            27..29 "->" Operator
            30..31 "T" TypeParameter
            34..35 "x" Parameter
            39..45 "static" Keyword
            46..49 "rep" Function.declaration.static
            50..51 "=" Operator
            52..57 "const" Keyword
            58..60 "fn" Keyword
            62..63 "<" Operator
            63..68 "const" Keyword
            69..70 "N" TypeParameter.declaration
            72..77 "usize" Type.defaultLibrary
            77..78 ">" Operator
            81..83 "->" Operator
            84..89 "usize" Type.defaultLibrary
            92..93 "N" TypeParameter
            94..95 "+" Operator
            96..97 "N" TypeParameter
            101..105 "type" Keyword
            106..110 "Pair" Type.declaration
            111..112 "=" Operator
            113..119 "struct" Keyword
            121..122 "<" Operator
            122..123 "T" TypeParameter.declaration
            123..124 ">" Operator
            130..131 "T" TypeParameter
            136..137 "T" TypeParameter
        "#]],
    );
}

#[test]
fn highlights_type_binder_reaches_its_with_chain_members() {
    // The declaration's binder is in scope inside its `with`-chain, which
    // is a SIBLING of the literal carrying the binder — the one place a
    // naive ancestor walk would lose `T`.
    check_highlights(
        r#"
type Box = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { b.v };
    }
};
"#,
        expect_test::expect![[r#"
            1..5 "type" Keyword
            6..9 "Box" Type.declaration
            10..11 "=" Operator
            12..18 "struct" Keyword
            20..21 "<" Operator
            21..22 "T" TypeParameter.declaration
            22..23 ">" Operator
            29..30 "T" TypeParameter
            33..37 "with" Keyword
            44..48 "impl" Keyword
            49..53 "Self" Type
            64..67 "get" Function.declaration
            68..69 "=" Operator
            70..72 "fn" Keyword
            73..74 "b" Parameter.declaration
            76..80 "Self" Type
            82..84 "->" Operator
            85..86 "T" TypeParameter
            89..90 "b" Parameter
        "#]],
    );
}

#[test]
fn qualified_member_paths_highlight_as_functions() {
    // A qualified member path shares the `Name::name` shape with a variant
    // path but names a FUNCTION — after a TYPE (`P::len`) as well as after
    // a TRAIT, here in the named-`Self` form, whose named argument reads
    // like the type it stands for. (The trait short form `D::m` is pinned
    // by `highlights_trait_names_at_every_occurrence`.)
    check_highlights(
        r#"
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl usize { m = fn(x: usize) -> usize { x }; }
};
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static a = fn(p: P) -> usize { D::<Self = usize>::m(1) + P::len(p) };
"#,
        expect_test::expect![[r#"
            1..6 "trait" Keyword
            7..8 "D" Trait.declaration
            9..10 "=" Operator
            11..19 "requires" Keyword
            22..23 "m" Function.declaration
            25..27 "fn" Keyword
            28..29 "x" Parameter.declaration
            31..35 "Self" Type
            37..39 "->" Operator
            40..45 "usize" Type.defaultLibrary
            49..53 "with" Keyword
            60..64 "impl" Keyword
            65..70 "usize" Type.defaultLibrary
            73..74 "m" Function.declaration
            75..76 "=" Operator
            77..79 "fn" Keyword
            80..81 "x" Parameter.declaration
            83..88 "usize" Type.defaultLibrary
            90..92 "->" Operator
            93..98 "usize" Type.defaultLibrary
            101..102 "x" Parameter
            111..115 "type" Keyword
            116..117 "P" Type.declaration
            118..119 "=" Operator
            120..126 "struct" Keyword
            132..137 "usize" Type.defaultLibrary
            140..144 "with" Keyword
            151..155 "impl" Keyword
            156..160 "Self" Type
            163..166 "len" Function.declaration
            167..168 "=" Operator
            169..171 "fn" Keyword
            172..173 "p" Parameter.declaration
            175..179 "Self" Type
            181..183 "->" Operator
            184..189 "usize" Type.defaultLibrary
            192..193 "p" Parameter
            204..210 "static" Keyword
            211..212 "a" Function.declaration.static
            213..214 "=" Operator
            215..217 "fn" Keyword
            218..219 "p" Parameter.declaration
            221..222 "P" Type
            224..226 "->" Operator
            227..232 "usize" Type.defaultLibrary
            235..236 "D" Trait
            238..239 "<" Operator
            239..243 "Self" Type
            244..245 "=" Operator
            246..251 "usize" Type.defaultLibrary
            251..252 ">" Operator
            254..255 "m" Function
            256..257 "1" Number
            259..260 "+" Operator
            261..262 "P" Type
            264..267 "len" Function
            268..269 "p" Parameter
        "#]],
    );
}

// ---- match projects through borrows: what the editor shows --------------

#[test]
fn hover_on_a_borrowed_matchs_payload_binding_shows_the_borrow() {
    // The binding IS a borrow, and hover says so — including the flavor,
    // which is the thing a reader needs to know before writing `t.*`.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn::<@a>(s: Opt::<usize>.&mut::<@a>) -> () {
    match s { ::Some(t$0) => { t.* = 1; }, ::None => {} }
};
"#,
        "```must\nt: usize.&mut\n```",
    );
}

#[test]
fn hover_on_an_owned_matchs_payload_binding_still_shows_the_value() {
    // The control: the same enum matched by value binds the value.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn(s: Opt::<usize>) -> usize {
    match s { ::Some(t$0) => t, ::None => 0 }
};
"#,
        "```must\nt: usize\n```",
    );
}

#[test]
fn hover_on_a_whole_value_binder_through_a_borrow_shows_the_borrow() {
    // A whole-value binder is not a projection: it names the same place, so
    // it gets the scrutinee's own borrow back, region and all.
    check_hover(
        r#"
type Opt = enum::<T> { Some(T), None };
static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {
    match s { whole$0 => 0 }
};
"#,
        "```must\nwhole: Opt::<usize>.&::<@a>\n```",
    );
}

#[test]
fn hover_shows_a_region_in_the_declaration_and_not_at_the_call() {
    // The elision, as a reader sees it. A DECLARATION's regions are its
    // parameters and are rendered — `@a` is what a caller's borrow has to
    // satisfy. At a MENTION they have been instantiated to this call's own
    // existentials, which have no spelling and print as a bare borrow: the
    // hover never suggests a token the turbofish would refuse.
    check_hover(
        "static ge$0t = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };",
        "```must\nget: fn(T.&::<@a>) -> T\n```",
    );
    check_hover(
        "static get = fn::<@a, T>(x: T.&::<@a>) -> T { x.* };\n\
         static main = fn::<@b>(p: usize.&::<@b>) -> usize { ge$0t(p) };",
        "```must\nget: fn(usize.&) -> usize\n```",
    );
}

#[test]
fn completions_match_arm_sees_through_a_borrowed_scrutinee() {
    // The arm slot has to look through the same lens `infer_match` does,
    // or it would decline exactly the scrutinees the checker accepts.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn::<@a>(s: Shape.&::<@a>) {
    match s {
        $0
    }
};
"#;
    check_has_completion(fixture_text, "::Circle");
    check_has_completion(fixture_text, "::Point");
    check_has_completion(fixture_text, "_");
}

#[test]
fn completions_match_template_fires_on_a_borrowed_scrutinee() {
    // The arm-list template, unchanged in shape: the arms of a borrowed
    // match are the same arms, and only the bindings' types differ — so
    // the very same snippet is right, and its `$1` payload stops now bind
    // borrows.
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn::<@a>(s: Shape.&mut::<@a>) {
    match s $0
};
"#,
            "match arms",
        ),
        crate::InsertText::Snippet {
            snippet: "{\n        ::Circle($1) => $2,\n        ::Point => $3,\n    }".to_owned(),
            plain: "{\n        ::Circle => ,\n        ::Point => ,\n    }".to_owned(),
        }
    );
}

#[test]
fn completions_match_template_not_offered_for_a_borrow_of_a_non_enum() {
    // The lens is lifted only for a referent a `match` dispatches on: a
    // `struct.&` scrutinee is not a dispatching scrutinee, and there are no
    // arms to guess.
    check_no_completion(
        r#"
type P = struct { x: usize };
static f = fn::<@a>(p: P.&::<@a>) {
    match p $0
};
"#,
        "match arms",
    );
}

#[test]
fn completions_match_scrutinee_lifts_a_borrowed_enum_typed_param() {
    // The scrutinee SLOT's ranking follows the same lens: a borrow of an
    // enum is a scrutinee the checker accepts, so it belongs in the leading
    // layer next to the owned ones.
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn::<@a>(s: Shape.&::<@a>, n: usize) {
    match $0
};
"#;
    assert_eq!(completion_sort_text(fixture_text, "s"), "2_00_s");
    // The control: a local the lens does not reach keeps the flat tier.
    assert_eq!(completion_sort_text(fixture_text, "n"), "2_10_n");
}

// ---- a bound is a completion source ------------------------------------

/// The owner's shape: a `Write`-bounded writer, borrowed exclusively.
const BOUND_FIXTURE_HEAD: &str = r#"
trait Write = requires {
    push: fn::<@r>(c: char, w: Self.&mut::<@r>) -> ();
};
"#;

#[test]
fn dot_completions_on_a_rigid_receiver_offer_its_bounds_requirements() {
    // `w.push(x)` has compiled since bound-directed resolution landed, and
    // `w.` offered nothing: the receiver is rigid, so it has no fields and
    // no impl to look in — its BOUNDS are the only thing that re-opens the
    // dot, and now they are read from the same enumeration resolution
    // picks from. Detail renders the requirement at the receiver's own
    // param (`Self` → `W`), its own binder as written — the shape the
    // trait's hover shows.
    check_completions(
        &format!(
            "{BOUND_FIXTURE_HEAD}\
             static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
        ),
        expect_test::expect![[r#"
            push Function (fn(char, W.&mut::<@r>))
        "#]],
    );
}

#[test]
fn dot_completions_snippet_bound_requirement_call_with_params() {
    // A requirement is fn-shaped like a member, so accepting it inserts the
    // call, not the bare name — the same idiom `field_items` uses for an
    // inherent member. The receiver (`Self`) is not a written argument, so
    // it does not count toward the tab stops.
    assert_eq!(
        completion_insert(
            &format!(
                "{BOUND_FIXTURE_HEAD}\
                 static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
            ),
            "push",
        ),
        crate::InsertText::Snippet {
            snippet: "push($1)".to_owned(),
            plain: "push()".to_owned(),
        }
    );
}

#[test]
fn dot_completions_snippet_bound_requirement_call_without_params() {
    // A NULLARY requirement (past its receiver) inserts a plain call, no
    // tab stop — pinned separately because the receiver-only arity is the
    // one a written-arity miscount would get wrong silently (a snippet
    // with an empty placeholder reads the same as this until a tab stop
    // is expected).
    assert_eq!(
        completion_insert(
            r#"
trait Show = requires { show: fn(s: Self) -> str; };
static f = fn::<T: Show>(t: T) -> str { t.$0 };
"#,
            "show",
        ),
        crate::InsertText::Plain("show()".to_owned())
    );
}

#[test]
fn a_bound_requirement_sorts_like_an_inherent_member() {
    // A requirement is rendered through the same [`Provenance::Item`] band
    // as an inherent member — not a band of its own — so a bound-directed
    // offer and a field or member candidate interleave by type match, not
    // by which mechanism produced them.
    assert_eq!(
        completion_sort_text(
            &format!(
                "{BOUND_FIXTURE_HEAD}\
                 static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
            ),
            "push",
        ),
        "2_20_push",
    );
}

#[test]
fn a_bound_receivers_offer_is_exactly_what_resolves() {
    // The pin on the one-home claim: the offered name is the name that
    // compiles, on the very same receiver. If the enumeration and the
    // resolver ever part company, one of these two halves fails.
    let source = format!(
        "{BOUND_FIXTURE_HEAD}\
         static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.push('x') }};"
    );
    let (analysis, file, _) = fixture(&format!("{source}$0"));
    assert_eq!(analysis.diagnostics(file), Vec::new());
    check_has_completion(
        &format!(
            "{BOUND_FIXTURE_HEAD}\
             static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {{ w.$0 }};"
        ),
        "push",
    );
}

#[test]
fn multiple_bounds_union_their_requirements_on_the_dot() {
    // `T: A + B` offers both traits' requirements — the union, not the
    // first bound that answers. (The rendered order is the list's own
    // ranking, which sorts equally-ranked candidates by name; the
    // enumeration itself walks bound order then declaration order, the
    // same order resolution walks when it looks for a name.)
    check_completions(
        r#"
trait Show = requires { show: fn(s: Self) -> str; };
trait Size = requires { size: fn(s: Self) -> usize; len: fn(s: Self) -> usize; };
static f = fn::<T: Show + Size>(t: T) -> str { t.$0 };
"#,
        expect_test::expect![[r#"
            len Function (fn(T) -> usize)
            show Function (fn(T) -> str)
            size Function (fn(T) -> usize)
        "#]],
    );
}

#[test]
fn a_requirement_without_the_dot_callable_shape_is_not_offered() {
    // G14 is structural: the receiver sits in the LAST parameter. A
    // requirement whose `Self` is anywhere else (or nowhere) is callable
    // only by its qualified spelling, so the dot does not offer it — the
    // same test `member_takes_receiver` admits calls by.
    check_completions(
        r#"
trait Mk = requires {
    make: fn(n: usize) -> Self;
    first: fn(s: Self, n: usize) -> usize;
    take: fn(n: usize, s: Self) -> usize;
};
static f = fn::<T: Mk>(t: T) -> usize { t.$0 };
"#,
        expect_test::expect![[r#"
            take Function (fn(usize, T) -> usize)
        "#]],
    );
}

#[test]
fn the_receiver_shape_filters_the_bounds_offer_both_ways() {
    // One table for offers and calls alike. An OWNED receiver is refused a
    // `Self.&mut` requirement (a borrow is never inserted for a local), a
    // SHARED borrow is refused an exclusive one, and an exclusive borrow
    // takes both — `receiver_takes`, read through the completion list.
    let owned = format!("{BOUND_FIXTURE_HEAD}static f = fn::<W: Write>(w: W) -> () {{ w.$0 }};");
    check_no_completion(&owned, "push");
    let shared = format!(
        "{BOUND_FIXTURE_HEAD}\
         static f = fn::<@a, W: Write>(w: W.&::<@a>) -> () {{ w.$0 }};"
    );
    check_no_completion(&shared, "push");
    // A value-`Self` requirement is the mirror: the owned receiver takes
    // it, and the borrowed one does not (that would be auto-deref).
    let by_value = r#"
trait Show = requires { show: fn(s: Self) -> str; };
static f = fn::<S: Show>(s: S) -> str { s.$0 };
"#;
    check_has_completion(by_value, "show");
    check_no_completion(
        r#"
trait Show = requires { show: fn(s: Self) -> str; };
static f = fn::<@a, S: Show>(s: S.&::<@a>) -> str { s.$0 };
"#,
        "show",
    );
}

#[test]
fn the_forget_capability_bound_contributes_nothing_to_the_dot() {
    // `forget` is a CAPABILITY, not a trait: it has no members, resolves
    // to no dictionary, and never enters the bound list at all. The dot of
    // a `T: forget` param is therefore empty — cleanly, with no candidate
    // and no complaint.
    check_completions(
        "static f = fn::<T: forget>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
    // And a capability written NEXT to a trait leaves the trait's
    // requirements exactly as they were.
    check_completions(
        r#"
trait Show = requires { show: fn(s: Self) -> str; };
static f = fn::<T: Show + forget>(t: T) -> str { t.$0 };
"#,
        expect_test::expect![[r#"
            show Function (fn(T) -> str)
        "#]],
    );
}

#[test]
fn an_unbounded_rigid_receiver_offers_nothing() {
    // Nothing is assumed of a parameter that writes no bound: it has no
    // fields, no impl and no requirements, and the dot says so by offering
    // nothing rather than guessing at some type's members.
    check_completions(
        "static f = fn::<T>(t: T) -> T { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn an_unresolvable_bound_offers_nothing() {
    // `Nope` names nothing: the bound's own declaration carries that
    // diagnostic, and the enumeration contributes no candidate for it —
    // no panic, no guess.
    check_completions(
        "static f = fn::<T: Nope>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn a_bound_naming_a_non_trait_offers_nothing() {
    // `usize` resolves to something, but not to a trait: `bound_trait`
    // rejects it the same way a call site's bound resolution does, so it
    // contributes no candidate either.
    check_completions(
        "static f = fn::<T: usize>(t: T) -> () { t.$0 };",
        expect_test::expect![[r#""#]],
    );
}

#[test]
fn a_requirement_without_a_full_signature_is_not_offered() {
    // `n`'s first parameter has no type annotation, so its signature isn't
    // fully written — the trait declaration carries that diagnostic, and
    // `lower_requirement_sig` returns `None` for it. `Self` is deliberately
    // placed in `n`'s SECOND parameter (not dropped) so this isolates the
    // `sig: None` path from `dot_callable`'s shape refusal: if `n` were
    // offered because it merely lacked a `Self` parameter, this fixture
    // would still catch it. The dot offers `m` (fully written) and skips
    // `n` rather than guessing at its shape.
    check_completions(
        r#"
trait D = requires {
    m: fn(x: Self) -> usize;
    n: fn(x, y: Self) -> usize;
};
static f = fn::<T: D>(t: T) -> () { t.$0 };
"#,
        expect_test::expect![[r#"
            m Function (fn(T) -> usize)
        "#]],
    );
}

#[test]
fn completions_do_not_panic_anywhere_in_a_bound_shaped_document() {
    // A request at EVERY offset of a document full of bound shapes —
    // binders, requirement signatures, borrowed rigid receivers, a
    // capability bound — none of which may panic. The sweep is the point:
    // completion runs on half-typed text, and the offsets a user actually
    // stops at are not the ones a hand-written fixture picks. 345 bytes,
    // so 346 requests; the assertion recomputes it rather than trusting
    // the number in this sentence.
    let source = r#"
trait Write = requires {
    push: fn::<@r>(c: char, w: Self.&mut::<@r>) -> ();
};
trait Show = requires { show: fn(s: Self) -> str; };
static fmt_usize = fn::<@a, W: Write>(w: W.&mut::<@a>, n: usize) -> () {
    w.push('x')
};
static both = fn::<@a, T: Show + forget, W: Write>(t: T, w: W.&mut::<@a>) -> str {
    w.push('y');
    t.show()
};
"#;
    let mut host = AnalysisHost::new();
    let file = host.create_file("test.must".to_owned(), source.to_owned());
    let analysis = host.snapshot();
    let mut requests = 0;
    for offset in 0..=source.len() {
        if !source.is_char_boundary(offset) {
            continue;
        }
        let _ = analysis.completions(FilePosition {
            file,
            offset: TextSize::new(offset as u32),
        });
        requests += 1;
    }
    assert_eq!(requests, source.len() + 1);
}

/// Two bounds, both declaring `push`, DIFFERENT shapes — and only one of
/// them fits the exclusive borrow this receiver is.
const AMBIGUOUS_SHAPES_HEAD: &str = r#"
trait A = requires {
    push: fn::<@r>(c: char, w: Self.&mut::<@r>) -> ();
    grow: fn::<@r>(w: Self.&mut::<@r>) -> ();
};
trait B = requires { push: fn(c: char, w: Self) -> (); };
"#;

#[test]
fn a_name_two_bounds_carry_is_offered_by_neither() {
    // A bound member call is decided by NAME before G14 is asked, so this
    // `push` is refused permanently even though exactly one candidate fits
    // the receiver's shape. Offering the shape-viable one would put a row
    // in the list that cannot be accepted — so completion suppresses the
    // name, which is resolution's own verdict (`narrow_by_name` returning
    // two) read straight through.
    //
    // The suppression is per NAME, not per bound set: `grow`, which only
    // `A` declares, is offered as usual.
    let source = format!(
        "{AMBIGUOUS_SHAPES_HEAD}\
         static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () {{ t.$0 }};"
    );
    check_no_completion(&source, "push");
    check_has_completion(&source, "grow");
}

#[test]
fn the_ambiguity_diagnostic_still_names_both_traits() {
    // P15: ambiguity is name-only, so the suppressed completion above is
    // only defensible while the diagnostic itself says which two traits
    // collided and how to spell each — the message, not the list, is what
    // tells the user what to write.
    let (analysis, file, _) = fixture(&format!(
        "{AMBIGUOUS_SHAPES_HEAD}\
         static f = fn::<@a, T: A + B>(t: T.&mut::<@a>) -> () {{ t.push('x') }};$0"
    ));
    let message = analysis
        .diagnostics(file)
        .into_iter()
        .map(|d| d.message)
        .find(|m| m.contains("is ambiguous"))
        .expect("the collision must be diagnosed");
    assert!(
        message.contains("`A`'s member (`A::push(value)`)")
            && message.contains("`B`'s member (`B::push(value)`)"),
        "the hint must name both traits and their spellings: {message}"
    );
}

#[test]
fn two_bounds_of_the_same_shape_do_not_double_a_row() {
    // The same ruling, with the shapes IDENTICAL: resolution refuses just
    // as permanently, and the list must not show two indistinguishable
    // rows for a name that resolves to nothing. Counted, not just
    // `check_no_completion` — one row and two rows are different bugs.
    let (analysis, _file, pos) = fixture(
        r#"
trait A = requires { push: fn(c: char, w: Self) -> (); };
trait B = requires { push: fn(c: char, w: Self) -> (); };
static f = fn::<T: A + B>(t: T) -> () { t.$0 };
"#,
    );
    assert_eq!(
        analysis
            .completions(pos)
            .iter()
            .filter(|item| item.label == "push")
            .count(),
        0,
    );
}

#[test]
fn a_nested_body_is_still_offered_its_enclosing_bounds() {
    // A bound-directed call here is refused — "cannot use the enclosing
    // bounds YET (it would have to capture the dictionary)" — but that
    // wall is a RESERVATION and a body-lowering fact (`in_nested_body`),
    // not a fact about what the bound declares, so the item-tree-driven
    // offers query does not re-derive it. The refusal explains itself at
    // the call, and the day the wall lifts these offers are already
    // correct.
    check_has_completion(
        r#"
trait Show = requires { show: fn(s: Self) -> str; };
static f = fn::<T: Show>(t: T) -> str {
    let g = fn(x: T) -> str { x.$0 };
    g(t)
};
"#,
        "show",
    );
}

#[test]
fn a_tail_read_through_a_borrow_squiggles_the_read_only() {
    // `r.*` in tail position is read into a temp and copied to the
    // return slot; the refusal sits on the read, once, never on the
    // block.
    let src = "static g = fn::<@a, @b>(out: usize.&mut::<@a>.&mut::<@b>, r: usize.&mut::<@a>) -> usize { out.* = r; r.* };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(&src[errors[0].range], "r.*");
    assert!(
        errors[0]
            .message
            .starts_with("reading `r.*` here invalidates an exclusive borrow of it"),
        "{}",
        errors[0].message
    );
}

#[test]
fn a_stale_loan_refusal_carries_its_companions() {
    // The squiggle sits on the invalidating access; the two companions
    // name the borrow's mint and the use that keeps it live, in the
    // interpreter's own vocabulary.
    let src = "static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };\n\
               static f = fn() -> usize { let mut n: usize = 1; let a = n.&mut; let b = n.&mut; bump(b); a.* };";
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(&src[errors[0].range], "n.&mut");
    assert!(
        errors[0]
            .message
            .starts_with("using `n` mutably here invalidates a borrow of it that is still live"),
        "{}",
        errors[0].message
    );
    assert_eq!(
        errors[0].related.len(),
        2,
        "related: {:?}",
        errors[0].related
    );
    assert_eq!(errors[0].related[0].message, "this borrow was created here");
    assert_eq!(&src[errors[0].related[0].range], "n.&mut");
    assert!(errors[0].related[0].range.start() < errors[0].range.start());
    assert_eq!(errors[0].related[1].message, "and it is still used here");
    assert_eq!(&src[errors[0].related[1].range], "a.*");
}
