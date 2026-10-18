//! Completions in `match`: scrutinee, arms and the arm-list template.

use super::*;

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

/// Accepting a whole scrutinee for a `match` with no `{` yet ends in a
/// space, so the next key is the arm list's `{`.
#[test]
fn completions_brace_less_scrutinee_ends_in_a_space() {
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static pick = fn (n: usize) -> Shape { ::Point };
static f = fn (x: Shape) {
    match $0
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "x"),
        crate::InsertText::Plain("x ".to_owned())
    );
    // A fn is called, with the space after the call.
    assert_eq!(
        completion_insert(fixture_text, "pick"),
        crate::InsertText::Snippet {
            snippet: "pick($1) ".to_owned(),
            plain: "pick() ".to_owned(),
        }
    );
    // `match` opens a longer expression, so it gets no space.
    assert_eq!(
        completion_insert(fixture_text, "match"),
        crate::InsertText::Snippet {
            snippet: "match $0".to_owned(),
            plain: "match".to_owned(),
        }
    );
}

/// A fn named in full as the scrutinee completes as a call: the
/// scrutinee's own type is no expectation asking for the fn as a value.
#[test]
fn completions_scrutinee_written_in_full_calls_the_fn() {
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static pick = fn (n: usize) -> Shape { ::Point };
static f = fn () {
    match pick$0
};
"#,
            "pick",
        ),
        crate::InsertText::Snippet {
            snippet: "pick($1) ".to_owned(),
            plain: "pick() ".to_owned(),
        }
    );
}

#[test]
fn completions_brace_less_scrutinee_field_ends_in_a_space() {
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
type P = struct { s: Shape };
static f = fn (p: P) {
    match p.$0
};
"#,
            "s",
        ),
        crate::InsertText::Plain("s ".to_owned())
    );
}

/// A variant inserted by its full path after a brace-less `match` ends in a
/// space, and keeps its ordinary rank: a variant is a pointless scrutinee.
#[test]
fn completions_brace_less_scrutinee_variant_path_ends_in_a_space() {
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn () {
    match $0
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "Shape::Point"),
        crate::InsertText::Plain("Shape::Point ".to_owned())
    );
    assert_eq!(
        completion_insert(fixture_text, "Shape::Circle"),
        crate::InsertText::Snippet {
            snippet: "Shape::Circle($1) ".to_owned(),
            plain: "Shape::Circle ".to_owned(),
        }
    );
    assert_eq!(
        completion_sort_text(fixture_text, "Shape::Point"),
        "2_25_Shape::Point"
    );
}

#[test]
fn completions_brace_less_scrutinee_variant_ends_in_a_space() {
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn () {
    match Shape::$0
};
"#,
            "Point",
        ),
        crate::InsertText::Plain("Point ".to_owned())
    );
}

#[test]
fn completions_scrutinee_with_an_arm_list_adds_no_space() {
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn (x: Shape) {
    match x$0 {
        _ => {},
    }
};
"#,
            "x",
        ),
        crate::InsertText::Plain("x".to_owned()),
    );
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
            f Function (fn(Shape) -> !)
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Shape::Circle EnumMember (Circle(usize))
            Shape::Point EnumMember (Point)
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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

/// A `fn` whose call is a scrutinee, local or item, ranks after every
/// enum-typed value, and a variant path ranks below every local.
#[test]
fn completions_match_scrutinee_ranks_a_producer_above_a_plain_local() {
    const PRODUCERS: &str = r#"
type Shape = enum { Circle(usize), Point };
static make = fn () -> Shape { Shape::Point };
static label = fn () -> str { "x" };
static f = fn (s: Shape) {
    let mk = fn () -> Shape { Shape::Point };
    let noise: str = "x";
    SLOT
};
"#;
    let in_scrutinee = PRODUCERS.replace("SLOT", "match $0");
    let elsewhere = PRODUCERS.replace("SLOT", "let v = $0;");
    assert_eq!(completion_sort_text(&in_scrutinee, "s"), "2_02_s");
    assert_eq!(completion_sort_text(&in_scrutinee, "mk"), "2_09_mk");
    assert_eq!(completion_sort_text(&in_scrutinee, "make"), "2_09_make");
    assert_eq!(completion_sort_text(&in_scrutinee, "noise"), "2_10_noise");
    assert_eq!(
        completion_sort_text(&in_scrutinee, "Shape::Point"),
        "2_25_Shape::Point"
    );
    // A `fn` yielding no enum, and any slot but the scrutinee, keep the
    // ordinary tiers.
    assert_eq!(completion_sort_text(&in_scrutinee, "label"), "2_20_label");
    assert_eq!(completion_sort_text(&elsewhere, "mk"), "2_10_mk");
    assert_eq!(completion_sort_text(&elsewhere, "make"), "2_20_make");
}

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
    // A `str` dispatches on nothing: no lift at all, the ordinary local
    // tier.
    assert_eq!(
        completion_sort_text(SCRUTINEE_LAYERS, "noise"),
        "2_10_noise"
    );
}

#[test]
fn completions_match_scrutinee_lifts_a_character_typed_local() {
    // The scrutinee slot's lift follows `hir::dispatches_on`, so it follows
    // it to `char` too: a `char` local leads the slot the way an enum one
    // does, while a `str` local stays on the flat local tier.
    const CHAR_SCRUTINEE: &str = r#"
static f = fn () {
    let s = "x";
    let c = 'x';
    match $0
};
"#;
    assert_eq!(completion_sort_text(CHAR_SCRUTINEE, "c"), "2_00_c");
    assert_eq!(completion_sort_text(CHAR_SCRUTINEE, "s"), "2_10_s");
}

/// With the arm list written, the `char` local (what a `match` dispatches
/// on) ranks above the `str` one.
#[test]
fn completions_match_scrutinee_ranking_holds_with_braces_written() {
    let (analysis, _file, pos) = fixture(
        r#"
static f = fn () {
    let s = "x";
    let c = 'x';
    match $0 {}
};
"#,
    );
    let labels: Vec<String> = analysis
        .completions(pos)
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert_eq!(labels[..2], ["c", "s"], "{labels:?}");
}

#[test]
fn completions_match_scrutinee_lifts_an_integer_typed_local() {
    // The same rule discharging one more case: integer literal patterns
    // exist, so an integer local is a scrutinee that dispatches and leads
    // the slot. Nothing in ide was taught this — `hir::dispatches_on`
    // widened and the ranking followed.
    const INT_SCRUTINEE: &str = r#"
static f = fn () {
    let s = "x";
    let n: usize = 1;
    match $0
};
"#;
    assert_eq!(completion_sort_text(INT_SCRUTINEE, "n"), "2_00_n");
    assert_eq!(completion_sort_text(INT_SCRUTINEE, "s"), "2_10_s");
}

#[test]
fn completions_match_scrutinee_does_not_suppress_the_normal_set() {
    // The enum-typed values lead; everything else an expression position
    // offers follows in its usual order. Enum names are never offered.
    check_completions(
        SCRUTINEE_LAYERS,
        expect_test::expect![[r#"
            near Variable (Shape)
            outer Variable (Shape)
            param Variable (Shape)
            ambient Constant (Shape)
            noise Variable (str)
            f Function (fn(Shape) -> !)
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Shape::Circle EnumMember (Circle(usize))
            Shape::Point EnumMember (Point)
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
static f = fn::<@a>(s: Shape.&::<@a>, t: str) {
    match $0
};
"#;
    assert_eq!(completion_sort_text(fixture_text, "s"), "2_00_s");
    // The control: a param the lens does not reach keeps the flat tier.
    assert_eq!(completion_sort_text(fixture_text, "t"), "2_10_t");
}
