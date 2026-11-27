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
            103..108: this `match` does not cover `Shape::Pair(_, _)`, `Shape::Point`
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
        r#"
        Shape::Point => panic("unhandled Shape::Point"),"#
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
        r#",
        Shape::Point => panic("unhandled Shape::Point"),"#
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
        r#",
        Shape::Point => panic("unhandled Shape::Point"),"#
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
        r#"
        Shape::Circle(_) => panic("unhandled Shape::Circle(_)"),
        Shape::Point => panic("unhandled Shape::Point"),
    "#
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
        r#"
        Shape::Circle(_) => panic("unhandled Shape::Circle(_)"),
        Shape::Point => panic("unhandled Shape::Point"),
    "#
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
        r#"
        Shape::Point => panic("unhandled Shape::Point"),"#
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
fn match_add_missing_arms_fix_spells_each_missing_pattern() {
    // Unit / one-payload / two-payload variants, in declaration order, each
    // spelled as the message spells it: `_` for every payload.
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
        r#"
        Shape::Point => panic("unhandled Shape::Point"),
        Shape::Circle(_) => panic("unhandled Shape::Circle(_)"),
        Shape::Pair(_, _) => panic("unhandled Shape::Pair(_, _)"),
    "#
    );
}

#[test]
fn match_add_missing_arms_fix_spells_nested_patterns() {
    // A value missing below the top level gets an arm for exactly it.
    let text = r#"
type Inner = enum { A, B };
type Outer = enum { Wrap(Inner), Empty };
static f = fn (o: Outer) {
    match o {
        ::Wrap(::A) => {},
        ::Empty => {},
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
        r#"
        Outer::Wrap(Inner::B) => panic("unhandled Outer::Wrap(Inner::B)"),"#
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
        r#"
        Shape::Point => panic("unhandled Shape::Point"),"#
    );
}

#[test]
fn match_on_a_scalar_offers_a_wildcard_arm() {
    // A scalar's values are not listed one by one: the missing pattern is
    // `_`, and the fix adds that arm.
    let text = r#"
static f = fn (n: usize) -> usize {
    match n { }
};
"#;
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "this `match` does not cover every possible `usize`; add a `_` arm"
    );
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(
        fix.edits[0].insert,
        r#"
        _ => panic("unhandled _"),
    "#
    );
}

#[test]
fn a_broken_part_reads_as_a_wildcard() {
    // `'a'` cannot match a `usize`, so it reads as `_`: `::Circle('a')`
    // covers every circle and the match is exhaustive.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle('a') => 0,
        ::Point => 1,
    }
};
"#,
        expect![[r#"
            112..115: type mismatch: this pattern is on a `usize`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn a_broken_arm_still_reports_what_is_missing() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Square(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle('a') => 0,
    }
};
"#,
        expect![[r#"
            100..105: this `match` does not cover `Shape::Square(_)`, `Shape::Point`
            127..130: type mismatch: this pattern is on a `usize`, and `char` cannot match one
        "#]],
    );
}

#[test]
fn a_broken_arm_leaves_a_later_duplicate_dead() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle('a') => 0,
        ::Point => 1,
        ::Point => 2,
    }
};
"#,
        expect![[r#"
            112..115: type mismatch: this pattern is on a `usize`, and `char` cannot match one
            153..160: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_wrong_slot_count_still_tests_its_variant() {
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(a, b) => 0,
    }
};
"#,
        expect![[r#"
            85..90: this `match` does not cover `Shape::Point`
            103..117: `Circle` has 1 payload, this pattern names 2
        "#]],
    );
}

#[test]
fn a_nested_newtype_pattern_names_the_type_of_an_unknown_value() {
    // Like construction: `Meters(m)` says the payload is a `Meters`, so
    // `m` is its record (the arm's mismatch shows it).
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
type Meters = struct { n: usize };
static f = fn () -> usize {
    let o = Option::None;
    match o {
        ::Some(Meters(m)) => m,
        ::None => 0,
    }
};
"#,
        expect![[r#"
            176..177: type mismatch: expected `usize`, found `struct { n: usize }` (expected `usize` because of this return type at 96..104)
        "#]],
    );
}

#[test]
fn a_newtype_pattern_named_like_a_variant_suggests_the_variant() {
    // `Circle(r)` is a newtype pattern; on a `Shape` the variant is the
    // likely intent, whether or not a type `Circle` exists. Not in a `let`:
    // `::Circle(r)` would be refutable there.
    check_diagnostics(
        r#"
type Shape = enum { Circle(usize), Point };
type Point = struct { x: usize };
static f = fn (s: Shape) -> usize {
    match s {
        Circle(r) => r,
        Point(p) => p.x,
        _ => 0,
    }
};
static g = fn (s: Shape) -> usize {
    let Circle(r) = s;
    r
};
"#,
        expect![[r#"
            137..146: `Circle` does not name a type; write `::Circle(...)` to match the variant
            161..169: type mismatch: expected `Point`, found `Shape`; write `::Point` to match the variant
            247..256: `Circle` does not name a type
        "#]],
    );
}

#[test]
fn a_named_pattern_on_a_number_is_refused() {
    // A number is no named type: the pattern is refused on the spot,
    // whichever kind names the type.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
type Meters = struct { n: usize };
static f = fn () -> usize {
    match 5 {
        Meters(m) => m.n,
        _ => 0,
    }
};
static g = fn () -> usize {
    match 5 {
        Option::Some(x) => x,
        _ => 0,
    }
};
"#,
        expect![[r#"
            129..138: type mismatch: expected `Meters`, found `{number}`
            222..237: a variant pattern needs an enum; only `_` or a binding can match a `{number}`
        "#]],
    );
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
    // One wording for every dead arm, by design: with nested patterns the
    // arms that cover one often do so together, so there is no single arm
    // to name.
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
            119..130: unreachable arm: the arms above match every value it matches
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
            129..140: unreachable arm: the arms above match every value it matches
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
            149..150: unreachable arm: the arms above match every value it matches
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
            59..70: a variant pattern needs an enum; `_`, a binding, or an integer literal can match a `usize`
            158..169: a variant pattern needs an enum; only `_` or a binding can match a `str`
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

// A value two levels down that no arm covers is named in full.
#[test]
fn exhaustiveness_names_a_value_missed_at_depth() {
    check_diagnostics(
        r#"
type S = enum { A, B };
type T = enum { Wrap(S, S), Nil };
static f = fn (t: T) -> usize {
    match t {
        ::Wrap(::A, _) => 1,
        ::Wrap(_, ::A) => 2,
        ::Nil => 3,
    }
};
"#,
        expect![[r#"
            96..101: this `match` does not cover `T::Wrap(S::B, S::B)`
        "#]],
    );
}

// An arm whose every value an earlier arm took, only through their
// sub-patterns together, never runs.
#[test]
fn an_arm_covered_by_earlier_nested_arms_is_unreachable() {
    check_diagnostics(
        r#"
type S = enum { A, B };
type T = enum { Wrap(S, S), Nil };
static f = fn (t: T) -> usize {
    match t {
        ::Wrap(::A, _) => 1,
        ::Wrap(_, ::A) => 2,
        ::Wrap(::A, ::A) => 3,
        _ => 4,
    }
};
"#,
        expect![[r#"
            172..188: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

// Literals inside an array never cover their element type, so the missing
// value is any array.
#[test]
fn literals_inside_an_array_need_a_catch_all() {
    check_diagnostics(
        r#"
static f = fn (c: [char; 2]) -> usize {
    match c {
        ['a', _] => 1,
        [_, 'b'] => 2,
    }
};
"#,
        expect![[r#"
            45..50: this `match` does not cover `[_, _]`
        "#]],
    );
}

// A `let` is a one-arm `match`: a nested pattern that can fail names what
// it misses.
#[test]
fn a_let_with_a_refutable_nested_pattern_names_what_it_misses() {
    check_diagnostics(
        r#"
type S = enum { A, B };
type T = enum { Wrap(S, S), Nil };
static f = fn (t: T) -> usize {
    let ::Wrap(::A, x) = t;
    0
};
"#,
        expect![[r#"
            100..114: this pattern does not cover `T::Wrap(S::B, _)`, `T::Nil`; use `match`
        "#]],
    );
}

// A record pattern is an arm like any other, and one of binders covers
// every value.
#[test]
fn a_record_pattern_in_an_arm_covers_its_type() {
    check_diagnostics(
        r#"
static f = fn (p: struct { x: usize, y: usize }) -> usize {
    match p {
        struct { x, y } => x + y,
    }
};
"#,
        expect![[""]],
    );
}

#[test]
fn array_record_and_newtype_patterns_project_through_a_borrow() {
    check_infer(
        r#"
type N = struct { x: usize };
static f = fn::<@a>(p: [N; 1].&::<@a>) -> usize {
    let [N(struct { x })] = p;
    x.*
};
"#,
        expect![[r#"
            42..121 'fn::<@a>(p: [N; 1...': fn([N; 1].&::<@a>) -> usize
            51..52 'p': [N; 1].&::<@a>
            79..121 '{     let [N(stru...': usize
            101..102 'x': usize.&
            109..110 'p': [N; 1].&::<@a>
            116..117 'x': usize.&
            116..119 'x.*': usize
        "#]],
    );
}

// A variant whose payload has no values (an `enum {}`) holds no value
// either: no arm or `let` has to cover it, and an arm for it never runs.
#[test]
fn a_variant_without_values_needs_no_arm_and_its_arm_never_runs() {
    check_diagnostics(
        r#"
type Void = enum {};
type E = enum { A(usize), B(Void) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(_) => 0 } };
static h = fn (e: E) -> usize { let ::A(n) = e; n };
static k = fn (v: Void) -> usize { match v {} };
"#,
        expect![[r#"
            173..179: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

// A type has no values when every value of it would have to hold a value
// of a type without values, at any depth: in an enum's every variant, a
// record, an array, or itself. A variant whose payload has such a type
// needs no arm, and an arm for it never runs.
#[test]
fn a_payload_enum_whose_variants_all_hold_no_value_needs_no_arm() {
    // `W`'s only variant holds a `Void`.
    check_diagnostics(
        r#"
type Void = enum {};
type W = enum { X(Void) };
type E = enum { A(usize), B(W) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(_) => 0 } };
"#,
        expect![[r#"
            197..203: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_payload_record_with_a_field_without_values_needs_no_arm() {
    // `N`'s field `v` holds no value.
    check_diagnostics(
        r#"
type Void = enum {};
type N = struct { v: Void };
type E = enum { A(usize), B(N) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(_) => 0 } };
"#,
        expect![[r#"
            199..205: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_payload_array_of_a_type_without_values_needs_no_arm() {
    // A `[Void; 1]` has an element, and no element has a value.
    check_diagnostics(
        r#"
type Void = enum {};
type E = enum { A(usize), B([Void; 1]) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(_) => 0 } };
"#,
        expect![[r#"
            178..184: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_payload_type_that_only_holds_itself_needs_no_arm() {
    // Every `Loop` holds another `Loop`, so none is finite.
    check_diagnostics(
        r#"
type Void = enum {};
type Loop = enum { Again(Loop) };
type E = enum { A(usize), B(Loop) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(_) => 0 } };
"#,
        expect![[r#"
            207..213: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_recursive_type_with_a_way_out_needs_its_arms() {
    // `L::Nil` ends a list, so `L::Cons` holds values.
    check_diagnostics(
        r#"
type Void = enum {};
type L = enum { Cons(usize, L), Nil };
static k = fn (l: L) -> usize { match l { ::Nil => 0 } };
"#,
        expect![[r#"
            93..98: this `match` does not cover `L::Cons(_, _)`
        "#]],
    );
}

#[test]
fn a_generic_payload_without_values_at_any_depth_needs_no_arm() {
    // Both payloads bottom out in a `Void`, through `One` twice and
    // through each side of `Either`.
    check_diagnostics(
        r#"
type Void = enum {};
type One = enum::<T> { X(T) };
type Either = enum::<L, R> { Left(L), Right(R) };
type E = enum { A(usize), B(One::<One::<Void>>), C(Either::<Either::<Void, Void>, Void>) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(::X(_)) => 0, ::C(_) => 0 } };
"#,
        expect![[r#"
            309..320: unreachable arm: the arms above match every value it matches
            327..333: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_generic_payload_that_only_holds_itself_needs_no_arm() {
    // Every `K` holds a `K::<Void>`, every `Flip::<A, B>` a `Flip::<B, A>`:
    // neither grows, and neither has a finite value.
    check_diagnostics(
        r#"
type Void = enum {};
type K = enum::<T> { X(T, K::<Void>) };
type Flip = enum::<A, B> { Y(Flip::<B, A>) };
type E = enum { A(usize), B(K::<usize>), C(Flip::<usize, bool>) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(::X(_, _)) => 0, ::C(_) => 0 } };
"#,
        expect![[r#"
            289..303: unreachable arm: the arms above match every value it matches
            310..316: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_generic_payload_that_holds_a_larger_version_of_itself_needs_no_arm() {
    // Every `K` holds a `K::<[Void; 1]>`, which holds a `[Void; 1]`.
    check_diagnostics(
        r#"
type Void = enum {};
type K = enum::<T> { X(T, K::<[Void; 1]>) };
type E = enum { A(usize), B(K::<usize>) };
static f = fn (e: E) -> usize { match e { ::A(n) => n } };
static g = fn (e: E) -> usize { match e { ::A(n) => n, ::B(::X(_, _)) => 0 } };
"#,
        expect![[r#"
            224..238: unreachable arm: the arms above match every value it matches
        "#]],
    );
}

#[test]
fn a_type_that_names_itself_with_larger_arguments_still_needs_its_arms() {
    // Asking whether `Nest::<usize>` has values must end, though it holds
    // a `Nest::<Pair::<usize>>`, which holds a bigger one still; `::Nil`
    // is one.
    check_diagnostics(
        r#"
type Pair = struct::<T> { l: T, r: T };
type Nest = enum::<T> { Nil, Cons(T, Nest::<Pair::<T>>) };
type E = enum { A(usize), B(Nest::<usize>) };
static f = fn (e: E) -> usize {
    match e {
        ::A(n) => n,
    }
};
"#,
        expect![[r#"
            182..187: this `match` does not cover `E::B(_)`
        "#]],
    );
}
