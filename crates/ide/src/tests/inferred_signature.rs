//! The insert-inferred-signature assist and its inlay hints.

use super::*;

#[test]
fn assist_writes_inferred_params_and_return_into_a_fn_literal() {
    check_assist(
        r#"
static g$0 = fn (f, a) { f(a) };
static h = fn (_n: usize) -> bool { true };
static main = fn { g(h, 1); };
"#,
        "Insert inferred signature",
        r#"
static g = fn (f: fn(usize) -> bool, a: usize) -> bool { f(a) };
static h = fn (_n: usize) -> bool { true };
static main = fn { g(h, 1); };
"#,
    );
}

#[test]
fn assist_writes_records_arrays_and_nominal_types() {
    check_assist(
        r#"
type P = struct { n: usize };
static g = fn (p$0, b) { struct { p = p, both = [b, b] } };
static main = fn { g(P(struct { n = 1 }), true); };
"#,
        "Insert inferred signature",
        r#"
type P = struct { n: usize };
static g = fn (p: P, b: bool) -> struct { both: [bool; 2], p: P } { struct { p = p, both = [b, b] } };
static main = fn { g(P(struct { n = 1 }), true); };
"#,
    );
}

#[test]
fn assist_writes_const_generic_arguments() {
    check_assist(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N] };
static g$0 = fn (b) { b };
static main = fn { g(Buf::<2>(struct { data = [1, 2] })); };
"#,
        "Insert inferred signature",
        r#"
type Buf = struct::<const N: usize> { data: [usize; N] };
static g = fn (b: Buf::<2>) -> Buf::<2> { b };
static main = fn { g(Buf::<2>(struct { data = [1, 2] })); };
"#,
    );
}

#[test]
fn assist_gives_a_listless_fn_literal_its_empty_params() {
    check_assist(
        r#"static main$0 = fn { print("hi"); };"#,
        "Insert inferred signature",
        r#"static main = fn () -> () { print("hi"); };"#,
    );
}

#[test]
fn assist_fills_the_return_hole_of_a_listless_fn_literal() {
    check_assist(
        r#"static main$0 = fn -> _ { true };"#,
        "Insert inferred signature",
        r#"static main = fn -> bool { true };"#,
    );
}

#[test]
fn assist_annotates_a_value_that_is_not_a_function() {
    check_assist(
        r#"static $0s = "hi";"#,
        "Insert inferred type",
        r#"static s: str = "hi";"#,
    );
}

/// A written annotation keeps its text: only its holes are filled, and a
/// fn type's missing return type (a hole too) is added.
#[test]
fn assist_fills_only_the_holes_of_an_item_annotation() {
    check_assist(
        r#"
static g$0: fn(_) = fn (x) { x };
static main = fn { let _y: str = g("a"); };
"#,
        "Insert inferred signature",
        r#"
static g: fn(str) -> str = fn (x) { x };
static main = fn { let _y: str = g("a"); };
"#,
    );
}

#[test]
fn assist_fills_holes_inside_type_arguments() {
    check_assist(
        r#"
type Opt = enum::<T> { Some(T), None };
static g$0 = fn (x: Opt::<_>) -> Opt::<_> { x };
static main = fn { let _: Opt::<str> = g(Opt::None); };
"#,
        "Insert inferred signature",
        r#"
type Opt = enum::<T> { Some(T), None };
static g = fn (x: Opt::<str>) -> Opt::<str> { x };
static main = fn { let _: Opt::<str> = g(Opt::None); };
"#,
    );
}

/// What inference could not solve stays a hole, and every solved part is
/// written.
#[test]
fn assist_leaves_unsolved_parts_as_holes() {
    let (label, fixed) = apply_assist(
        r#"
static g$0 = fn (_x: _, y) -> _ { y };
static main = fn { let _y: str = g(1, "a"); };
"#,
    )
    .expect("an assist is offered");
    assert_eq!(label, "Insert inferred signature");
    assert_eq!(
        fixed,
        r#"
static g = fn (_x: _, y: str) -> str { y };
static main = fn { let _y: str = g(1, "a"); };
"#,
    );
}

/// A non-generic signature cannot name a region, so a borrow is a hole
/// while its solved siblings are written.
#[test]
fn assist_leaves_a_borrow_as_a_hole() {
    let (_, fixed) = apply_assist(
        r#"
static g$0 = fn (x, n) { n };
static main = fn { let a: u8 = 1; let _m: usize = g(a.&, 2); };
"#,
    )
    .expect("an assist is offered");
    assert_eq!(
        fixed,
        r#"
static g = fn (x, n: usize) -> usize { n };
static main = fn { let a: u8 = 1; let _m: usize = g(a.&, 2); };
"#,
    );
}

/// `fn(usize) -> bool.&raw` would be a fn returning a pointer, so a `_`
/// pointee that is an fn type stays a hole while the rest is written.
#[test]
fn assist_leaves_an_fn_pointee_hole_under_a_written_pointer() {
    for pointer in [".&raw", ".&"] {
        let before = format!(
            r#"
static h = fn (_n: usize) -> bool {{ true }};
static g$0 = fn (_p: _{pointer}, n) -> _ {{ n }};
static main = fn {{ let f = h; let _m: usize = g(f{pointer}, 2); }};
"#
        );
        let (_, fixed) = apply_assist(&before).expect("an assist is offered");
        assert_eq!(
            fixed,
            format!(
                r#"
static h = fn (_n: usize) -> bool {{ true }};
static g = fn (_p: _{pointer}, n: usize) -> usize {{ n }};
static main = fn {{ let f = h; let _m: usize = g(f{pointer}, 2); }};
"#
            ),
            "under `{pointer}`"
        );
        // Filling adds no diagnostic; `.&` is refused for its unnamed region
        // either way.
        let after = diagnostic_messages(&fixed);
        assert_eq!(diagnostic_messages(&before), after, "under `{pointer}`");
        if pointer == ".&raw" {
            assert_eq!(after, Vec::<String>::new());
        }
    }
}

#[test]
fn assist_leaves_the_return_of_a_written_fn_pointee_under_a_pointer() {
    for pointer in [".&raw", ".&"] {
        let before = format!(
            r#"
static h = fn (_n: usize) -> bool {{ true }};
static g$0 = fn (_p: fn(_){pointer}, n) -> _ {{ n }};
static main = fn {{ let f = h; let _m: usize = g(f{pointer}, 2); }};
"#
        );
        let (_, fixed) = apply_assist(&before).expect("an assist is offered");
        assert_eq!(
            fixed,
            format!(
                r#"
static h = fn (_n: usize) -> bool {{ true }};
static g = fn (_p: fn(usize){pointer}, n: usize) -> usize {{ n }};
static main = fn {{ let f = h; let _m: usize = g(f{pointer}, 2); }};
"#
            ),
            "under `{pointer}`"
        );
        // Filling adds no diagnostic; `.&` is refused for its unnamed region
        // either way.
        let after = diagnostic_messages(&fixed);
        assert_eq!(diagnostic_messages(&before), after, "under `{pointer}`");
        if pointer == ".&raw" {
            assert_eq!(after, Vec::<String>::new());
        }
    }
}

#[test]
fn assist_is_not_offered_when_nothing_was_inferred() {
    // `{number}` never defaults, so `x` has no type to write.
    assert_eq!(
        apply_assist(
            r#"
static g$0 = fn (x) { x };
static main = fn { g(1); };
"#
        ),
        None
    );
}

/// A name the file resolves to another type is no spelling of the inferred
/// one.
#[test]
fn assist_does_not_write_a_shadowed_builtin_name() {
    assert_eq!(
        apply_assist(
            r#"
type str = struct {};
static s$0 = "hi";
"#
        ),
        None
    );
}

#[test]
fn assist_is_not_offered_on_a_fully_annotated_item() {
    assert_eq!(apply_assist("static g$0 = fn (x: str) -> str { x };"), None);
}

#[test]
fn assist_is_offered_on_the_head_only() {
    assert_eq!(
        apply_assist(r#"static main = fn { print($0"hi"); };"#),
        None
    );
}

#[test]
fn needs_annotation_carries_the_insert_signature_fix() {
    let text = r#"
static g = fn (_x: _, y) -> _ { y };
static main = fn { let _y: str = g(1, "a"); };
"#;
    let (analysis, file, _) = fixture(&format!("{text}$0"));
    let diagnostic = analysis
        .diagnostics(file)
        .into_iter()
        .find(|d| {
            d.message
                .starts_with("not enough information to infer the type of `g`")
        })
        .expect("has the needs-annotation diagnostic");
    let fix = diagnostic.fix.expect("diagnostic has a fix");
    assert_eq!(fix.label, "Insert inferred signature");
    assert_eq!(
        apply_fix(text, &fix),
        r#"
static g = fn (_x: _, y: str) -> str { y };
static main = fn { let _y: str = g(1, "a"); };
"#,
    );
}

/// Each inferred part is shown where the action would write it.
#[test]
fn inlay_hints_show_the_inferred_parts_in_place() {
    let text = r#"
static g = fn (f, a) { f(a) };
static h = fn (_n: usize) -> bool { true };
static main = fn { g(h, 1); };
"#;
    let (analysis, file, _) = fixture(&format!("{text}$0"));
    let shown: Vec<(String, usize, bool)> = analysis
        .inlay_hints(file)
        .into_iter()
        .map(|hint| (hint.label, usize::from(hint.offset), hint.padding_left))
        .collect();
    assert_eq!(
        shown,
        [
            (": fn(usize) -> bool".to_owned(), 17, false),
            (": usize".to_owned(), 20, false),
            ("-> bool".to_owned(), 21, true),
            ("() -> ()".to_owned(), text.find("fn {").unwrap() + 2, true),
        ]
    );
}

/// Accepting any hint of a signature writes all of it.
#[test]
fn every_hint_of_a_signature_writes_all_of_it() {
    let text = r#"
static g = fn (f, a) { f(a) };
static h = fn (_n: usize) -> bool { true };
static main = fn { g(h, 1); };
"#;
    let (analysis, file, _) = fixture(&format!("{text}$0"));
    let hints = analysis.inlay_hints(file);
    let of_g: Vec<_> = hints
        .iter()
        .filter(|hint| usize::from(hint.offset) < text.find("static h").unwrap())
        .collect();
    assert_eq!(of_g.len(), 3);
    for hint in of_g {
        let fix = crate::Fix {
            label: hint.fix_label.clone(),
            edits: hint
                .edits
                .iter()
                .map(|edit| crate::FileEdit {
                    file,
                    edit: edit.clone(),
                })
                .collect(),
        };
        assert_eq!(
            apply_fix(text, &fix),
            r#"
static g = fn (f: fn(usize) -> bool, a: usize) -> bool { f(a) };
static h = fn (_n: usize) -> bool { true };
static main = fn { g(h, 1); };
"#,
        );
    }
}
