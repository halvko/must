//! Completions outside `match` and the dot.

use super::*;

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
            Shape Struct (struct { r: usize })
            area Function (fn(usize) -> usize)
            main Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
            main Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
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
            unsafe Keyword
            use Keyword
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
fn completions_after_the_vouch_marker_offer_extern() {
    // After `unsafe` at item position, only `extern` is offered.
    check_completions(
        "unsafe $0",
        expect_test::expect![[r#"
            extern Keyword
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
            unsafe Keyword
            use Keyword
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
fn completions_variant_segment_type_position_offers_nothing() {
    // A variant is a value, never a type: nothing to offer after `Shape::`
    // in an annotation.
    check_completions(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape::$0) {};
"#,
        expect_test::expect![[""]],
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
            main Function (fn(str, usize))
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
            get_n Function (fn() -> usize)
            main Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
            main Function (fn())
            other Function (fn(usize) -> str)
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
            main Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
            Point Struct (struct { x: usize })
            main Function (fn(Point, usize))
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
    // A variant candidate under its own enum's expectation: a payload-less
    // one IS a value of the enum (tier 0), a payload-carrying one is a
    // constructor whose call produces one (tier 1). Both outrank every
    // unrankable/mismatching (tier 2) item.
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
    let square = items
        .iter()
        .find(|c| c.label == "Square")
        .expect("Square is offered");
    assert_eq!(square.sort_text, "0_20_Square");

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
fn completions_type_ranking_match_arm_variants_share_one_tier() {
    // Match-arm variant candidates all share one tier (1), with
    // gold-before-covered preserved inside it and `_` staying an unranked
    // keyword.
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
            main Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
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

/// Accepting the whole condition of an `if` with no block yet ends in a
/// space.
#[test]
fn completions_brace_less_if_condition_ends_in_a_space() {
    let fixture_text = r#"
static ready = fn () -> bool { true };
static f = fn (b: bool) {
    if $0
};
"#;
    assert_eq!(
        completion_insert(fixture_text, "b"),
        crate::InsertText::Plain("b ".to_owned())
    );
    assert_eq!(
        completion_insert(fixture_text, "ready"),
        crate::InsertText::Plain("ready() ".to_owned())
    );
    // With the block written, the condition gets no space.
    assert_eq!(
        completion_insert(
            r#"
static f = fn (b: bool) {
    if $0 {}
};
"#,
            "b",
        ),
        crate::InsertText::Plain("b".to_owned()),
    );
}

/// An enum name is not a value, so no expression position offers it.
#[test]
fn completions_offer_no_enum_name_as_a_value() {
    for body in ["match Sha$0", "match Sha$0 {}", "let s = Sha$0;"] {
        let text = r#"
type Shape = enum { Circle(usize), Point };
static f = fn () {
    BODY
};
"#
        .replace("BODY", body);
        let (analysis, _file, pos) = fixture(&text);
        let items = analysis.completions(pos);
        assert!(items.iter().any(|c| c.label == "panic"), "{items:?}");
        assert!(items.iter().all(|c| c.label != "Shape"), "{items:?}");
    }
}

/// An enum's variants are values, offered by their full paths: under the
/// enum's expectation they rank with the enum-typed values, below the
/// locals, and above the fns whose call produces one.
#[test]
fn completions_offer_variants_by_path_below_enum_typed_values() {
    let fixture_text = r#"
type Shape = enum { Circle(usize), Point };
static make = fn () -> Shape { Shape::Point };
static f = fn (s: Shape, n: usize) {
    let t: Shape = $0;
};
"#;
    let (analysis, _file, pos) = fixture(fixture_text);
    let labels: Vec<String> = analysis
        .completions(pos)
        .into_iter()
        .map(|item| item.label)
        .collect();
    let at = |label: &str| labels.iter().position(|l| l == label).unwrap();
    assert!(at("s") < at("Shape::Circle"), "{labels:?}");
    assert!(at("Shape::Circle") < at("Shape::Point"), "{labels:?}");
    assert!(at("Shape::Point") < at("make"), "{labels:?}");
    assert!(at("make") < at("n"), "{labels:?}");
    assert_eq!(
        completion_insert(fixture_text, "Shape::Circle"),
        crate::InsertText::Snippet {
            snippet: "Shape::Circle($1)".to_owned(),
            plain: "Shape::Circle".to_owned(),
        }
    );
    assert_eq!(
        completion_insert(fixture_text, "Shape::Point"),
        crate::InsertText::Plain("Shape::Point".to_owned()),
    );
}

#[test]
fn completions_non_scrutinee_expression_adds_no_space() {
    assert_eq!(
        completion_insert(
            r#"
type Shape = enum { Circle(usize), Point };
static f = fn (x: Shape) {
    let y = x$0;
};
"#,
            "x",
        ),
        crate::InsertText::Plain("x".to_owned()),
    );
}

#[test]
fn completions_keywords_without_a_block_stay_plain() {
    let fixture_text = r#"
static main = fn {
    loop {
        $0
    }
};
"#;
    for word in ["break", "continue", "true", "let"] {
        assert_eq!(
            completion_insert(fixture_text, word),
            crate::InsertText::Plain(word.to_owned()),
        );
    }
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
            main Function (fn(Option::<usize>, fn(usize) -> bool) -> bool)
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            Option::None EnumMember (None)
            Option::Some EnumMember (Some(T))
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
    push: fn::<@r>(char, Self.&mut::<@r>) -> ();
};
trait Show = requires { show: fn(Self) -> str; };
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

/// Completions in a body that borrows a temporary offer the named locals
/// only.
#[test]
fn completions_never_offer_a_materialized_temporary() {
    check_completions(
        r#"
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static mk = fn () -> usize { 7 };
static show = fn (n: usize) -> () { };
static main = fn () -> () {
    let first = get(mk().&);
    let second = get(mk().&);
    show(fi$0);
};
"#,
        expect_test::expect![[r#"
            first Variable (usize)
            second Variable (usize)
            get Function (fn(usize.&::<@a>) -> usize)
            mk Function (fn() -> usize)
            panic Function (fn(str) -> !)
            main Function (fn())
            show Function (fn(usize))
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
fn completions_let_initializer_sees_earlier_lets_but_not_itself() {
    // An earlier `let` is offered in a later `let`'s initializer; the `let`
    // itself is not.
    check_completions(
        r#"
static f = fn (p: usize) {
    let s: usize = 1;
    let t: usize = $0;
};
"#,
        expect_test::expect![[r#"
            p Variable (usize)
            s Variable (usize)
            panic Function (fn(str) -> !)
            f Function (fn(usize))
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
fn completions_unterminated_let_initializer_does_not_see_itself() {
    // A `let` missing its `;` is not offered in its own empty or partial
    // initializer; an earlier `let` is.
    for program in [
        r#"
static f = fn (p: usize) {
    let s: usize = 1;
    let t: usize = $0
};
"#,
        r#"
static f = fn (p: usize) {
    let s: usize = 1;
    let t: usize = s + $0
};
"#,
    ] {
        check_has_completion(program, "s");
        check_no_completion(program, "t");
    }
}

#[test]
fn completions_statement_start_after_unterminated_let_sees_it() {
    // A `let` missing its `;` is offered at the next statement start.
    check_has_completion(
        r#"
static f = fn {
    let s: usize = 1
    $0
};
"#,
        "s",
    );
}

#[test]
fn completions_tail_after_unterminated_let_sees_it() {
    // A `let` missing its `;` is offered in the tail expression after it.
    check_has_completion(
        r#"
static f = fn () -> usize {
    let t: usize = 2
    t + $0
};
"#,
        "t",
    );
}

#[test]
fn completions_arithmetic_operand_ranks_integers_first() {
    // An operand of `+` whose integer type nothing pins ranks integer
    // candidates above the rest.
    check_completions(
        r#"
static f = fn {
    let xf = fn(a: usize) {};
    let xtra: usize = 1;
    let y = 11 + x$0;
};
"#,
        expect_test::expect![[r#"
            xtra Variable (usize)
            panic Function (fn(str) -> !)
            xf Variable (fn(usize))
            f Function (fn())
            AllocResult::Err EnumMember (Err)
            AllocResult::Ok EnumMember (Ok(T.&raw mut))
            NextChar::Char EnumMember (Char(char, usize))
            NextChar::End EnumMember (End)
            ReadLineResult::End EnumMember (End)
            ReadLineResult::Line EnumMember (Line(str))
            Utf8Result::Err EnumMember (Err)
            Utf8Result::Ok EnumMember (Ok(str))
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
