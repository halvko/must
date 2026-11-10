//! `match`: exhaustiveness, arms, bindings and the add-missing-arms fix.

use super::*;

// ---- match ----

#[test]
fn match_arms_of_different_variants_join_to_the_enum() {
    // Arms produce different variants of one enum, each of which already
    // has the enum's type.
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    let t = match s {
        ::Circle(r) => Shape::Circle(r + 1),
        ::Point => Shape::Point,
    };
    t
};
"#,
        expect![[r#"
            56..186 'fn (s: Shape) {  ...': fn(Shape) -> Shape
            60..61 's': Shape
            70..186 '{     let t = mat...': Shape
            80..81 't': Shape
            84..177 'match s {        ...': Shape
            90..91 's': Shape
            111..112 'r': usize
            117..130 'Shape::Circle': fn(usize) -> Shape
            117..137 'Shape::Circle(r + 1)': Shape
            131..132 'r': usize
            131..136 'r + 1': usize
            135..136 '1': usize
            158..170 'Shape::Point': Shape
            183..184 't': Shape
        "#]],
    );
}

#[test]
fn match_nested_in_if_flattens_into_one_join() {
    // A match in an if's branch tail contributes its arms' leaves to the
    // enclosing join (J1 through match): the culprit squiggle lands on the
    // one disagreeing *arm tail*, and the sibling hints name leaves from
    // both constructs.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape, c: bool) {
    let x = if c {
        match s {
            ::Circle(r) => r,
            ::Point => "no",
        }
    } else {
        2
    };
    x
};
"#,
        expect![[r#"
            171..175: `if` branches have incompatible types: `usize` vs `str`; add a type annotation to decide between them (this branch has type `usize` at 145..146)
            208..209: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn match_nonexhaustive_missing_one_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            85..90: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn match_nonexhaustive_missing_several_variants() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            103..108: this `match` does not cover `Shape::Pair`, `Shape::Point`
        "#]],
    );
}

#[test]
fn match_nonexhaustive_partial_coverage_offers_add_missing_arms_fix() {
    // Partially-covered match: only the missing arm is added, as a pure
    // insertion right after the last existing one — the buffer's own
    // `\n    }` that follows is left untouched (so the insert carries no
    // trailing newline of its own; adding one would leave a blank line).
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );
    let last_arm_end = text.find("::Circle(r) => r,").unwrap() + "::Circle(r) => r,".len();
    assert_eq!(u32::from(fix.edits[0].range.start()), last_arm_end as u32);
    assert_eq!(
        fix.edits[0].range.start(),
        fix.edits[0].range.end(),
        "a pure insertion"
    );

    // Applying the edit never introduces a new diagnostic: the generated
    // arm's `panic(...)` body joins with the other arm's `usize` because
    // `panic` types as `!`.
    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_last_arm_without_a_trailing_comma_gets_one_inserted() {
    // A comma-less last arm is legal (grammar.rs's `match_arm`: nothing
    // checks for a comma right before the arm list's own `}`) — inserting
    // straight after it would otherwise glue two arms into unparseable
    // text (`r        ::Point`).
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r
    }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        ",\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_last_arm_ending_in_a_block_needs_no_comma_but_tolerates_one() {
    // A `}`-bodied last arm never needs a comma either way (the grammar
    // `eat`s one if present, between arms and before the list's own `}`
    // alike) — prefixing one unconditionally whenever the last token isn't
    // already a comma is therefore always safe, block-bodied or not.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {
        ::Circle(r) => { let _ = r; }
    };
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        ",\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_empty_arm_list_offers_add_all_variants_fix() {
    // Empty arm list: `match s {}` — the checker's own non-exhaustive
    // diagnostic covers it (every variant is "uncovered" when there are no
    // arms at all), so the same fix construction handles it with no
    // special-casing and no separate diagnostic.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("this `match` does not cover")
    );
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );
    let anchor = text.find("match s {").unwrap() + "match s {".len();
    assert_eq!(u32::from(fix.edits[0].range.start()), anchor as u32);
    assert_eq!(u32::from(fix.edits[0].range.end()), anchor as u32);
}

#[test]
fn match_empty_arm_list_with_space_before_brace_still_gets_indented() {
    // `match s { }` (a space, not touching braces) still has no line break
    // between the anchor and `}` — the synthesized trailing newline keys
    // on that, not on the anchor and `}` coinciding exactly.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    match s { };
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_add_missing_arms_fix_leaves_a_trailing_comment_alone() {
    // A comment sitting between the last arm and `}` is never dropped: the
    // insertion lands right after the last arm (trivia attaches to the
    // following real token, not the arm), pushing the comment down rather
    // than overwriting it — the tradeoff a range-replace would have had.
    let text = "type Shape = enum { Circle(usize), Point };\n\
                 static f = fn (s: Shape) -> usize {\n    \
                 match s {\n        \
                 ::Circle(r) => r,\n        \
                 // keep me\n    \
                 }\n\
                 };\n";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );

    let mut patched = text.to_owned();
    patched.insert_str(
        usize::from(fix.edits[0].range.start()),
        &fix.edits[0].insert,
    );
    assert!(
        patched.contains("// keep me"),
        "the comment must survive the edit: {patched}"
    );
    let file = SourceFile::new(&db, "test.must".to_owned(), patched);
    assert_eq!(crate::file_diagnostics(&db, file), vec![]);
}

#[test]
fn match_add_missing_arms_fix_names_binders_by_payload_arity() {
    // Unit / one-payload / two-payload variants, in declaration order:
    // `::Point` gets no parens, `::Circle` gets one binder (`v`), `::Pair`
    // gets one per field (`v1, v2`).
    let text = r#"
type Shape = enum { Point, Circle(usize), Pair(usize, str) };
static f = fn (s: Shape) {
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Circle(v) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Pair(v1, v2) => panic(\"unhandled ::Pair\"),\n    "
    );
}

#[test]
fn match_add_missing_arms_fix_projects_through_a_borrowed_scrutinee() {
    // A borrowed enum scrutinee: match still dispatches on the enum behind
    // the borrow (the projection lens), so the fix fires and computes the
    // same missing-variant set as the unborrowed case.
    let text = "type Shape = enum { Circle(usize), Point };\n\
                 static f = fn::<@a>(s: Shape.&::<@a>) -> () {\n    \
                 match s {\n        ::Circle(r) => { let _ = r; },\n    };\n\
                 };\n";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Add missing match arms");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Point => panic(\"unhandled ::Point\"),"
    );
}

#[test]
fn match_add_missing_arms_fix_avoids_shadowing_a_visible_local() {
    // A `v` already visible at the match's own scope (a `let` right before
    // it) is not shadowed by the generated payload binder — it gets
    // suffixed until free instead.
    let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) {
    let v: usize = 1;
    match s {};
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        "\n        ::Circle(v_) => panic(\"unhandled ::Circle\"),\n\
         \x20\x20\x20\x20\x20\x20\x20\x20::Point => panic(\"unhandled ::Point\"),\n    "
    );
}

#[test]
fn match_without_catch_all_offers_no_fix() {
    // The sibling diagnostic for a non-enum scrutinee with no `_`/binding
    // arm: there is no variant list to enumerate, so no fix is offered
    // (this fix is scoped to `NonExhaustiveMatch` only).
    let text = r#"
static f = fn (n: usize) -> usize {
    match n { }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("this `match` does not cover every possible")
    );
    assert!(diagnostics[0].fix.is_none());
}

#[test]
fn match_wildcard_covers_everything() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        _ => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn match_arm_after_wildcard_is_unreachable() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        _ => 0,
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            119..130: unreachable arm: `Circle` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn match_duplicate_variant_arm_is_unreachable() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Circle(d) => d + d,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            129..140: unreachable arm: `Circle` is already covered by a previous arm
        "#]],
    );
}

#[test]
fn match_binding_arm_binds_scrutinee_type() {
    check_infer(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        whole => 1,
    }
};
"#,
        expect![[r#"
            56..122 'fn (s: Shape) -> ...': fn(Shape) -> usize
            60..61 's': Shape
            79..122 '{     match s {  ...': usize
            85..120 'match s {        ...': usize
            91..92 's': Shape
            103..108 'whole': Shape
            112..113 '1': usize
        "#]],
    );
}

#[test]
fn match_payload_binding_types_come_from_the_declaration() {
    check_infer(
        r#"
type Shape = enum { Pair(usize, str) };
static f = fn (s: Shape) {
    match s {
        ::Pair(n, text) => { print(text); n },
        _ => 0,
    }
};
"#,
        expect![[r#"
            52..152 'fn (s: Shape) {  ...': fn(Shape) -> usize
            56..57 's': Shape
            66..152 '{     match s {  ...': usize
            72..150 'match s {        ...': usize
            78..79 's': Shape
            97..98 'n': usize
            100..104 'text': str
            109..127 '{ print(text); n }': usize
            111..116 'print': fn(str)
            111..122 'print(text)': ()
            117..121 'text': str
            124..125 'n': usize
            142..143 '0': usize
        "#]],
    );
}

#[test]
fn match_pattern_arity_mismatch() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r, extra) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            103..121: `Circle` has 1 payload, this pattern names 2
        "#]],
    );
}

#[test]
fn match_sigil_variant_missing_payloads_needs_them() {
    // `::Circle` names a payload-carrying variant with no payload list: a
    // variant pattern must name (or hole) its payloads, so this is an
    // arity error — the direct successor of the retired bare-`Circle`
    // reinterpretation case.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle => 1,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            103..111: `Circle` has 1 payload, this pattern names 0
        "#]],
    );
}

#[test]
fn match_sigil_and_qualified_patterns_resolve_identically() {
    // Both spellings of both variants — elided `::Variant` and fully
    // qualified `Shape::Variant` — resolve against the scrutinee's enum:
    // fully covered, no diagnostics, and the payload binding types line up.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Circle(r) => r,
        Shape::Point => 0,
    }
};
static g = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn match_bind_arm_named_like_variant_binds_and_warns() {
    // `Point` (bare, no `::`) is a binding, not a variant match — but its
    // name shadows the `Point` variant, almost always a migration mistake.
    // It stays well-typed (a full catch-all binding the whole value), so
    // this is only a warning pointing at the `::Point` spelling.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        Point => 0,
    }
};
"#,
        expect![[r#"
            129..134: `Point` binds the whole value; write `::Point` (or `Shape::Point`) to match the variant (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_bind_arm_named_like_variant_is_still_a_catch_all() {
    // A bind named like a variant is a *full* catch-all (never narrowed),
    // so a trailing `_` arm after it is unreachable — proof the reinterpretation
    // is gone. Two diagnostics: the shadow-name warning and the dead arm.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        Point => 0,
        _ => 1,
    }
};
"#,
        expect![[r#"
            129..134: `Point` binds the whole value; write `::Point` (or `Shape::Point`) to match the variant (`Shape` is defined here at 6..11)
            149..150: unreachable arm: a previous arm already matches anything
        "#]],
    );
}

#[test]
fn match_bind_arm_named_like_payload_variant_is_not_an_error() {
    // G25's motivating scenario: a bare bind named like a
    // *payload-carrying* variant used to require a `PatArity` error under
    // reinterpretation. Now it just binds the whole value — no error, only
    // the shadow-name warning.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Circle => 0,
    }
};
"#,
        expect![[r#"
            103..109: `Circle` binds the whole value; write `::Circle` (or `Shape::Circle`) to match the variant (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_wrong_enum_pattern_is_rejected() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
type State = enum { Idle, Running(usize) };
static f = fn (s: Shape) -> usize {
    match s {
        State::Idle => 0,
        _ => 1,
    }
};
"#,
        expect![[r#"
            147..158: this pattern matches `State::Idle`, but the scrutinee is a `Shape` (`State` is defined here at 50..55)
        "#]],
    );
}

#[test]
fn match_pattern_no_such_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        Shape::Square(x) => x,
        _ => 0,
    }
};
"#,
        expect![[r#"
            103..119: `Shape` has no variant `Square` (`Shape` is defined here at 6..11)
        "#]],
    );
}

#[test]
fn match_non_enum_scrutinee_rejects_variant_patterns() {
    // The "what can match" clause is type-dependent: since integer literal
    // patterns landed, a `usize` takes one, so the old blanket "only `_`
    // or a binding" would be false here. A `str` still gets the blanket.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n {
        ::Circle(r) => r,
        _ => 0,
    }
};
static g = fn (s: str) -> usize {
    match s {
        ::Circle(r) => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            59..70: a variant pattern needs an enum scrutinee; `_`, a binding, or an integer literal can match a `usize`
            158..169: a variant pattern needs an enum scrutinee; only `_` or a binding can match a `str`
        "#]],
    );
}

#[test]
fn match_non_enum_scrutinee_needs_a_catch_all() {
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    match n { }
};
"#,
        expect![[r#"
            41..46: this `match` does not cover every possible `usize`; add a `_` arm
        "#]],
    );
}

#[test]
fn match_in_const_fn_is_clean() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static area_ish = const fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r * r,
        ::Point => 0,
    }
};
static x = area_ish(Shape::Circle(3));
"#,
        expect![[r#""#]],
    );
}
