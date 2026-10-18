//! Diagnostics as the editor shows them, with their fixes.

use super::*;

#[test]
fn a_nullary_unsafe_fn_gets_no_run_lens() {
    // ▶ on a nullary `extern` item has exactly one possible outcome — the
    // unsafe-block refusal — so the button is not offered. Its safe
    // neighbour still is, which is what keeps this a filter rather than a
    // retreat.
    let mut host = AnalysisHost::new();
    let file = host.create_file(
        "test.must".to_owned(),
        "unsafe extern static tick: unsafe fn() -> ();\n\
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
fn an_unused_binding_is_renamed_with_a_leading_underscore() {
    let (label, fixed) = apply_only_fix("static f = fn (a: usize, mut b: usize) -> usize { a };");
    assert_eq!(label, "Rename to `_b`");
    assert_eq!(
        fixed,
        "static f = fn (a: usize, mut _b: usize) -> usize { a };"
    );
}

/// In `{ b }` one name both selects the field and binds it, so the
/// rename keeps the field: `{ b as _b }`.
#[test]
fn an_unused_shorthand_field_binding_is_renamed_with_as() {
    let (label, fixed) = apply_only_fix(
        "static f = fn (r: struct { a: usize, b: usize }) -> usize { let struct { a, mut b } = r; a };",
    );
    assert_eq!(label, "Rename to `_b`");
    assert_eq!(
        fixed,
        "static f = fn (r: struct { a: usize, b: usize }) -> usize { let struct { a, mut b as _b } = r; a };"
    );
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

/// A member's loan findings are reported with the file's diagnostics.
#[test]
fn diagnostics_include_members_loan_findings() {
    let src = r#"
type Cell = struct { n: usize } with {
    impl Self {
        leak = fn::<@a>() -> usize.&::<@a> { let n: usize = 5; n.& };
    }
};
"#;
    let (analysis, file, _pos) = fixture(&format!("{src}$0"));
    let diagnostics = analysis.diagnostics(file);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == crate::Severity::Error)
        .map(|d| (&src[d.range], d.message.as_str()))
        .collect();
    assert_eq!(
        errors,
        [(
            "n.&",
            "borrowed value does not live long enough: this borrows a local, but the borrow \
             has to last for `@a`, which outlives the body"
        )]
    );
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
                   static f = fn(_x: Box::<Res>) -> usize { 1 };\n";
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
        errors[0].starts_with("`_x` is not consumed on this path"),
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
fn record_expression_evaluates_cleanly() {
    // Records are typed structurally and have a MIR/eval story
    // (aggregates and field projections) — the "not yet" diagnostic is
    // gone, and a record initializer is exactly as clean as any other.
    let (analysis, file, _pos) = fixture("static p = struct { x = { let n: usize = 1; n } };$0");
    let diagnostics = analysis.diagnostics(file);
    assert_eq!(diagnostics, Vec::new(), "diagnostics: {diagnostics:?}");
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
    // The holder's use is its own read, in `a.*`.
    assert_eq!(&src[errors[0].related[1].range], "a");
}
