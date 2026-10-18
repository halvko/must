//! Const contexts: what may be called, printed or read at compile time.

use super::*;

#[test]
fn item_tree_classifies_constness() {
    use crate::item_tree::{Constness, item_tree};

    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        r#"
static a = 1;
const b = 5;
const c = fn { 1 };
static d = const fn { 1 };
static e = const { 2 };
"#
        .to_owned(),
    );
    let tree = item_tree(&db, file);
    let constness: Vec<Constness> = tree
        .items
        .iter()
        .filter_map(|item| item.kind.constness())
        .collect();
    assert_eq!(
        constness,
        vec![
            Constness::Static,
            Constness::Const,
            Constness::Const,
            // A `const` starting the *initializer* (`const fn` literal,
            // `const { ... }` block) must not make the item itself const.
            Constness::Static,
            Constness::Static,
        ]
    );
}

/// Const-checking is a separate pass; `const { ... }` is transparent
/// for typing — it gets its own entry in `type_of_expr`, but its type is
/// exactly the inner block's.
#[test]
fn const_block_is_transparent_for_typing() {
    check_infer(
        "static x = const { 5 };",
        expect![[r#"
            11..22 'const { 5 }': {number}
            17..22 '{ 5 }': {number}
            19..20 '5': {number}
        "#]],
    );
}

/// `const fn` is an explicit marker orthogonal to typing: a `const fn`
/// literal infers exactly the same signature a plain `fn` literal would.
#[test]
fn const_fn_literal_infers_like_plain_fn_literal() {
    check_infer(
        "static f = const fn (n: usize) -> usize { n };",
        expect![[r#"
            11..45 'const fn (n: usiz...': fn(usize) -> usize
            21..22 'n': usize
            40..45 '{ n }': usize
            42..43 'n': usize
        "#]],
    );
}

#[test]
fn const_fn_item_callable_from_item_initializer() {
    // An item initializer is a const context (for `static` and `const`
    // alike); an item whose initializer is a `const fn` literal may be
    // called there — the item's own keyword is irrelevant.
    check_diagnostics(
        r#"
static double = const fn (n: usize) -> usize { n * 2 };
const x = double(2);
static y = double(3);
"#,
        expect![[r#""#]],
    );
}

#[test]
fn directly_called_const_fn_literal_is_allowed() {
    check_diagnostics(
        "static x = (const fn (n: usize) -> usize { n })(1);",
        expect![[r#""#]],
    );
}

#[test]
fn panic_is_allowed_in_const_contexts() {
    check_diagnostics(
        r#"static x: usize = if 1 == 2 { panic("impossible") } else { 5 };"#,
        expect![[r#"
            21..22: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn plain_fn_body_exits_the_const_context() {
    // The initializer is a const context, but entering the plain fn
    // literal's body leaves it: that body is runtime code, `print` is fine.
    check_diagnostics(r#"static main = fn { print("hi"); };"#, expect![[r#""#]]);
}

#[test]
fn defining_a_plain_fn_literal_in_a_const_context_is_fine() {
    // Only calls are checked: a plain fn literal may be *defined* in a
    // const context (even inside a `const { ... }`), and its body is
    // runtime code again — the `print` inside is not flagged.
    check_diagnostics(
        r#"static f = const { fn { print("later") } };"#,
        expect![[r#""#]],
    );
}

#[test]
fn calling_a_plain_fn_item_from_an_initializer_is_rejected() {
    check_diagnostics(
        r#"
static double = fn (n: usize) -> usize { n * 2 };
static x = double(2);
"#,
        expect![[r#"
            62..68: cannot call `double` in a const context; marking it `const fn` would allow this (`double` is defined here at 8..14) (this item's initializer is a const context at 51..57)
        "#]],
    );
}

#[test]
fn directly_called_plain_fn_literal_in_const_context_is_rejected() {
    check_diagnostics(
        "static x = (fn (n: usize) -> usize { n })(1);",
        expect![[r#"
            12..40: cannot call this `fn` literal in a const context; marking it `const fn` would allow this (this item's initializer is a const context at 0..6)
        "#]],
    );
}

#[test]
fn print_in_a_const_block_is_rejected() {
    check_diagnostics(
        r#"static x = const { print("hi") };"#,
        expect![[r#"
            19..24: cannot call `print` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 11..16)
        "#]],
    );
}

#[test]
fn read_line_in_a_const_block_is_rejected() {
    // `print`'s input twin gets `print`'s exact treatment: a
    // side-effecting builtin, refused in a const context with the same
    // message shape (the builtin's own name is the only thing that
    // varies).
    check_diagnostics(
        r#"static x = const { read_line() };"#,
        expect![[r#"
            19..28: cannot call `read_line` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 11..16)
        "#]],
    );
}

#[test]
fn const_block_in_a_plain_fn_body_reenters_the_const_context() {
    // Rule 2 exits the const context at the fn body, but `const { ... }`
    // re-enters it — the violation inside is flagged.
    check_diagnostics(
        r#"
static main = fn {
    print("runtime is fine");
    const { print("compile time is not") };
}
"#,
        expect![[r#"
            62..67: cannot call `print` in a const context; const evaluation cannot have side effects (this `const` block is a const context at 54..59)
        "#]],
    );
}

#[test]
fn calling_a_parameter_in_a_const_fn_body_is_rejected() {
    // Const-ness is not part of fn types: a parameter can't be known to be
    // a `const fn`, so calling it is rejected conservatively.
    check_diagnostics(
        "static apply = const fn (f: fn() -> usize) -> usize { f() };",
        expect![[r#"
            54..55: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const fn` is always a const context at 15..20)
        "#]],
    );
}

#[test]
fn calling_a_let_bound_value_in_a_const_block_is_rejected() {
    // Even provably bound to a `const fn`, a let-bound value is rejected:
    // const-ness is not tracked through bindings.
    check_diagnostics(
        r#"
static double = const fn (n: usize) -> usize { n * 2 };
static f = fn {
    let d = double;
    const { d(2) };
}
"#,
        expect![[r#"
            105..106: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this `const` block is a const context at 97..102)
        "#]],
    );
}

#[test]
fn item_whose_root_is_not_a_fn_literal_is_a_value_call() {
    // No peeling: the `const fn` literal sits inside a `const { ... }`
    // wrapper, so the item's root is not a fn literal and calling it is
    // conservatively a value call.
    check_diagnostics(
        r#"
static wrapped = const { const fn (n: usize) -> usize { n } };
static x = wrapped(1);
"#,
        expect![[r#"
            75..82: cannot call a value in a const context; whether it is a `const fn` is not known from its type (this item's initializer is a const context at 64..70)
        "#]],
    );
}

#[test]
fn unresolved_callee_in_const_context_is_not_double_reported() {
    // The unresolved name already carries a diagnostic; const-check stays
    // silent about it.
    check_diagnostics(
        "static x = missing();",
        expect![[r#"
            11..18: unresolved name `missing`
        "#]],
    );
}

#[test]
fn call_nested_in_a_rejected_calls_args_is_still_checked() {
    // No cascading suppression: the outer call is rejected, and the `print`
    // in its argument list is judged on its own merits too.
    check_diagnostics(
        r#"
static f = fn (n: ()) -> usize { 1 };
static x = f(print("hi"));
"#,
        expect![[r#"
            50..51: cannot call `f` in a const context; marking it `const fn` would allow this (`f` is defined here at 8..9) (this item's initializer is a const context at 39..45)
            52..57: cannot call `print` in a const context; const evaluation cannot have side effects (this item's initializer is a const context at 39..45)
        "#]],
    );
}

#[test]
fn non_const_fn_call_offers_a_mark_const_fn_fix() {
    // The callee's initializer is a plain `fn` literal in the same file:
    // the fix inserts `const ` right before it.
    let text = "static f = fn (n: usize) -> usize { n };\nstatic x = f(1);";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Mark `f` as `const fn`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "const ");
    assert!(fix.edits[0].range.is_empty());
    // Right before `f`'s own `fn` keyword (offset 11): applying it yields
    // `static f = const fn (n: usize) -> usize { n };`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 11);
}

#[test]
fn non_const_fn_literal_call_offers_no_fix() {
    // A directly-called plain `fn` literal has no named declaration to
    // edit — nothing to offer.
    check_diagnostics(
        "static f = const { (fn () -> usize { 1 })() };",
        expect![[r#"
            20..40: cannot call this `fn` literal in a const context; marking it `const fn` would allow this (this `const` block is a const context at 11..16)
        "#]],
    );
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = const { (fn () -> usize { 1 })() };".to_owned(),
    );
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].fix.is_none());
}
