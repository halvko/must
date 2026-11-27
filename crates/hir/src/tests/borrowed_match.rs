//! `match` through a borrow: payload projection.

use super::*;

// ---- match projects through borrows -------------------------------------
//
// The ruling in one sentence: matching a BORROW binds payload sub-place
// borrows; matching an owned place copies or moves, exactly as before.
// There is no new pattern grammar, and no per-binder mode — the
// scrutinee's flavor decides.
//
// The tests split three ways, and the split is the argument:
//
//   * what a binder is TYPED as, through each flavor and through nesting;
//   * what REGION obligations the projection incurs (the soundness half —
//     a payload borrow may never outlive its scrutinee's loan);
//   * that the OWNED path is untouched, diagnostics included.

const OPT: &str = "type Opt = enum::<T> { Some(T), None };\n";

#[test]
fn a_shared_borrowed_match_binds_shared_payload_borrows() {
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {\n\
             match s { ::Some(t) => t.*, ::None => 0 }\n\
         };",
        expect![[r#"
            51..139 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&::<@a>) -> usize
            60..61 's': Opt::<usize>.&::<@a>
            94..139 '{ match s { ::Som...': usize
            96..137 'match s { ::Some(...': usize
            102..103 's': Opt::<usize>.&::<@a>
            113..114 't': usize.&
            119..120 't': usize.&
            119..122 't.*': usize
            134..135 '0': usize
        "#]],
    );
}

#[test]
fn an_exclusive_borrowed_match_binds_exclusive_payload_borrows() {
    // The flavor is inherited, not chosen: `.&mut` in, `.&mut` out. That
    // is what makes the binding writable, and it is also why there is no
    // binder marker — there would be nothing for one to say.
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&mut::<@a>) -> () {\n\
             match s { ::Some(t) => { t.* = 5; }, ::None => {} }\n\
         };",
        expect![[r#"
            51..149 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&mut::<@a>)
            60..61 's': Opt::<usize>.&mut::<@a>
            94..149 '{ match s { ::Som...': ()
            96..147 'match s { ::Some(...': ()
            102..103 's': Opt::<usize>.&mut::<@a>
            113..114 't': usize.&mut
            119..131 '{ t.* = 5; }': ()
            121..122 't': usize.&mut
            121..124 't.*': usize
            127..128 '5': usize
            143..145 '{}': ()
        "#]],
    );
}

#[test]
fn a_borrowed_match_binds_every_payload_of_the_variant() {
    // Two payloads, two independent borrows of two disjoint sub-places.
    check_infer(
        "type Pair = enum { Both(usize, str), Neither };\n\
         static f = fn::<@a>(p: Pair.&mut::<@a>) -> usize {\n\
             match p { ::Both(x, y) => x.*, ::Neither => 0 }\n\
         };",
        expect![[r#"
            59..148 'fn::<@a>(p: Pair....': fn(Pair.&mut::<@a>) -> usize
            68..69 'p': Pair.&mut::<@a>
            97..148 '{ match p { ::Bot...': usize
            99..146 'match p { ::Both(...': usize
            105..106 'p': Pair.&mut::<@a>
            116..117 'x': usize.&mut
            119..120 'y': str.&mut
            125..126 'x': usize.&mut
            125..128 'x.*': usize
            143..144 '0': usize
        "#]],
    );
}

#[test]
fn a_whole_value_binder_on_a_borrowed_scrutinee_stays_the_borrow() {
    // A bare bind names the very same place the scrutinee does, so there
    // is nothing to project and nothing to shorten: it gets the borrow
    // back, at the scrutinee's own region. (This is also exactly what it
    // got before the projection existed, when a borrowed scrutinee was
    // classified as "some other type".)
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> Opt::<usize>.&::<@a> {\n\
             match s { whole => whole }\n\
         };",
        expect![[r#"
            51..139 'fn::<@a>(s: Opt::...': fn(Opt::<usize>.&::<@a>) -> Opt::<usize>.&::<@a>
            60..61 's': Opt::<usize>.&::<@a>
            109..139 '{ match s { whole...': Opt::<usize>.&::<@a>
            111..137 'match s { whole =...': Opt::<usize>.&::<@a>
            117..118 's': Opt::<usize>.&::<@a>
            121..126 'whole': Opt::<usize>.&::<@a>
            130..135 'whole': Opt::<usize>.&::<@a>
        "#]],
    );
}

#[test]
fn projection_is_transitive_through_nested_matches() {
    // "All the way down" — spelled as two matches, because the binding IS
    // a borrow and so matching IT projects again. (Nested PATTERNS are not
    // grammar yet, so `::Some(::Pair(a, b))` has nothing to parse into;
    // this is the shape that expresses the same rule today.)
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(o: Opt::<Opt::<usize>>.&::<@a>) -> usize {{\n\
                 match o {{\n\
                     ::Some(inner) => match inner {{ ::Some(n) => n.*, ::None => 0 }},\n\
                     ::None => 0,\n\
                 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_owners_project_in_checks_clean() {
    // The shape the projection exists for: `Opt::<T>.&::<@a>` in,
    // `Opt::<T.&::<@a>>` out — an `as_ref`-shaped function, in the
    // surface language.
    //
    // What it proves is the REGION half. The payload binder's region is a
    // fresh variable with one upper bound, `@a`, so it is *allowed* to be
    // shorter and *able* to be exactly `@a`. Had the projection pinned the
    // binder to something shorter, this signature would be unwritable; had
    // it emitted no edge at all, the two rejections below would not fire.
    //
    // FLAGGED — the arm bodies name the enum instead of writing the elided
    // `::Some(t)`. That is not about borrows: a match arm's body is a JOIN
    // LEAF, which by design has no expected type to read the sigil against
    // (`an_elided_variant_expression_is_refused_in_a_join_position`), and
    // the owned twin below fails identically. Lifting it is the second
    // expectation channel that test names, and it is a separate ruling.
    check_diagnostics(
        &format!(
            "{OPT}static project_in = fn::<@a, T>(s: Opt::<T>.&::<@a>) -> Opt::<T.&::<@a>> {{\n\
                 match s {{\n\
                     ::Some(t) => Opt::<T.&::<@a>>::Some(t),\n\
                     ::None => Opt::<T.&::<@a>>::None,\n\
                 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn the_elided_sigil_in_an_arm_body_fails_the_same_way_owned() {
    // The control for the flag above: the join-leaf limitation is not
    // something the projection introduced. Borrowed and owned spellings of
    // the same program produce the same two rejections, in the same
    // places.
    let borrowed = format!(
        "{OPT}static f = fn::<@a, T>(s: Opt::<T>.&::<@a>) -> Opt::<T.&::<@a>> {{\n\
             match s {{ ::Some(t) => ::Some(t), ::None => ::None }}\n\
         }};"
    );
    let owned = format!(
        "{OPT}static f = fn::<T>(s: Opt::<T>) -> Opt::<T> {{\n\
             match s {{ ::Some(t) => ::Some(t), ::None => ::None }}\n\
         }};"
    );
    let db = RootDatabase::default();
    let messages = |text: &str| {
        let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
        crate::file_diagnostics(&db, file)
            .into_iter()
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    let borrowed = messages(&borrowed);
    assert_eq!(
        borrowed,
        vec![
            "cannot resolve `::Some` without an expected type — write `Enum::Some`".to_owned(),
            "cannot resolve `::None` without an expected type — write `Enum::None`".to_owned(),
        ]
    );
    assert_eq!(borrowed, messages(&owned));
}

// ---- the region half: a payload borrow can never outlive its scrutinee --

#[test]
fn a_payload_borrow_cannot_outlive_the_scrutinees_loan() {
    // The projection emits ONE directed edge, `@scrutinee: @payload`, and
    // this is it failing. Without the edge the body would hand the caller
    // a borrow good for `@a` derived from one only good for `@b` — the
    // exact unsoundness the edge exists to rule out.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
        expect![[r#"
            114..115: matching this borrow to bind its payloads needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn a_declared_bound_makes_the_projection_legal() {
    // The same program with the guarantee the message asked for. This is
    // the other half of the edge being real: it is checkable, so declaring
    // the relation satisfies it.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b: @a>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn a_borrowed_match_cannot_launder_a_region_obligation() {
    // No laundering through a projection. Returning a `@b` borrow where
    // `@a` is promised is refused without `@b: @a`, and reaching `@a`
    // through a borrowed match's binder must incur exactly the obligation
    // reaching it directly does. A projection that forgot its edge, or
    // that adopted instead of relating, would let the wrapped spelling
    // pass where the direct one is refused — a hole in the region graph
    // that no error message would name.
    check_wrapping_changes_nothing(
        &format!("{OPT}static f = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {{ p }};"),
        &format!(
            "{OPT}static f = fn::<@a, @b>(s: Opt::<usize>.&::<@b>) -> usize.&::<@a> {{\n\
                 match s {{ ::Some(t) => t, ::None => panic(\"none\") }}\n\
             }};"
        ),
    );
}

// ---- the owned path, unchanged -----------------------------------------

#[test]
fn an_owned_match_still_moves_a_noncopyable_payload_out() {
    // The byte-identity claim, at its sharpest point. `usize.&mut` is the
    // one affine type there is, so this program is only possible if the
    // owned path still MOVES: a projection would have handed back
    // `usize.&mut.&mut`, and reading through a borrow of it would be
    // "cannot move out of a borrow". Clean means the owned path did not
    // move.
    check_infer(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn::<@b>(o: Opt::<usize.&mut::<@b>>) -> usize {\n\
             match o { ::Some(t) => t.*, ::None => 0 }\n\
         };",
        expect![[r#"
            51..142 'fn::<@b>(o: Opt::...': fn(Opt::<usize.&mut::<@b>>) -> usize
            60..61 'o': Opt::<usize.&mut::<@b>>
            97..142 '{ match o { ::Som...': usize
            99..140 'match o { ::Some(...': usize
            105..106 'o': Opt::<usize.&mut::<@b>>
            116..117 't': usize.&mut::<@b>
            122..123 't': usize.&mut::<@b>
            122..125 't.*': usize
            137..138 '0': usize
        "#]],
    );
}

#[test]
fn a_borrowed_match_of_a_noncopyable_payload_refuses_the_read() {
    // And the contrast that proves the two paths are different: the SAME
    // enum behind a borrow binds `usize.&mut.&`, and copying the payload
    // OUT of that would duplicate the exclusive permission. Refused by the
    // ordinary safe-`.*` rule — no new diagnostic was needed.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(o: Opt::<usize.&mut::<@b>>.&::<@a>) -> usize {{\n\
                 match o {{ ::Some(t) => {{ let c = t.*; 0 }}, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            144..147: cannot move out of a borrow: `usize.&mut::<@b>` cannot be copied
        "#]],
    );
    // Reading THROUGH the binding is not that: `t.*.*` projects, and only
    // the `usize` at the end is copied, so it is legal exactly as
    // `b.*.*` on any other doubly-borrowed place is.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a, @b>(o: Opt::<usize.&mut::<@b>>.&::<@a>) -> usize {{\n\
                 match o {{ ::Some(t) => t.*.*, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#""#]],
    );
}

#[test]
fn a_borrow_where_the_owned_value_is_wanted_names_both_ways_out() {
    // The hint is gated on shape alone — found is a borrow of exactly the
    // type wanted — so it is language-wide: a plain typed `let` gets it,
    // with no `match` anywhere. Both routes out are named, because which
    // one is right depends on the referent: `.*` for a copyable one, the
    // owned value for anything that has to move.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let x: usize = r; x };",
        expect![[r#"
            64..65: type mismatch: expected `usize`, found `usize.&::<@a>`; a borrow is not the value — write `.*` to read through it (a copy, so the referent must be copyable), or use the owned value instead of a borrow of it (expected `usize` because of this annotation at 56..61)
        "#]],
    );
    // And its loudest customer: the message someone lands on the first
    // time they match a borrow, where the binder is a borrow and the
    // position wants the value.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {{\n\
                 match s {{ ::Some(t) => t, ::None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            119..120: type mismatch: expected `usize`, found `usize.&`; a borrow is not the value — write `.*` to read through it (a copy, so the referent must be copyable), or use the owned value instead of a borrow of it (expected `usize` because of this return type at 85..93)
        "#]],
    );
}

#[test]
fn a_borrow_where_a_copyable_value_is_wanted_is_read_through() {
    let fixed = apply_fix(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let x: usize = r; x };",
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("let x: usize = r.*;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // A binding made by matching through a borrow.
    let fixed = apply_fix(
        &[
            OPT,
            r#"
static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {
    match s { ::Some(t) => t, ::None => 0 }
};
"#,
        ]
        .concat(),
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("::Some(t) => t.*,"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // `.*` goes after the whole operand.
    let fixed = apply_fix(
        "static f = fn::<@a>(p: struct { b: usize.&::<@a> }) -> usize { let x: usize = p.b; x };",
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("let x: usize = p.b.*;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_borrow_in_a_shorthand_field_is_read_through_by_spelling_the_value_out() {
    // `struct { b, }` has no expression to put `.*` after: the value is the
    // name itself, so the fix writes it out as `b = b.*`.
    let fixed = apply_fix(
        "static f = fn::<@a>(b: usize.&::<@a>) -> struct { b: usize } { struct { b, } };",
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("struct { b = b.*, }"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_borrow_broken_out_of_a_loop_is_read_through_at_the_break() {
    let fixed = apply_fix(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let x: usize = loop { break r; }; x };",
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("break r.*;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_borrow_written_where_the_value_is_wanted_is_removed() {
    // The owned value is right there, so it wins over a copy through the
    // borrow, and needs no copyable referent.
    let fixed = apply_fix(
        "static f = fn (n: usize) -> usize { let x: usize = n.&; x };",
        "Remove the borrow",
    );
    assert!(fixed.contains("let x: usize = n;"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_borrow_of_a_value_that_cannot_be_copied_is_still_read_through() {
    // The fix makes the edit the message names; the copy it then asks for
    // is the next error's to explain.
    let fixed = apply_fix(
        "static f = fn::<@a, T>(r: T.&::<@a>) -> T { r };",
        "Read through the borrow with `.*`",
    );
    assert!(fixed.contains("{ r.* }"), "fixed: {fixed}");
    check_diagnostics(
        &fixed,
        expect![[r#"
        44..47: cannot move out of a borrow: `T` cannot be copied
    "#]],
    );
    let fixed = apply_fix(
        "static f = fn::<@a, T>(r: T.&::<@a>) -> T { r.*.& };",
        "Remove the borrow",
    );
    assert!(fixed.contains("{ r.* }"), "fixed: {fixed}");
}

#[test]
fn a_dot_through_a_borrow_offers_the_deref() {
    // A dot-call reaching a value `Self` through a borrow.
    let fixed = apply_fix(
        r#"
type Counter = struct { n: usize } with {
    impl Self { get = fn (c: Self) -> usize { c.n }; }
};
static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.get() };
"#,
        "Insert `.*` before `.get`",
    );
    assert!(fixed.contains("{ c.*.get() }"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // A builtin member, which takes its receiver by value too.
    let fixed = apply_fix(
        "static f = fn::<@a>(r: str.&::<@a>) -> () { let n = r.next_char(0); };",
        "Insert `.*` before `.next_char`",
    );
    assert!(fixed.contains("r.*.next_char(0)"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // A field read.
    let fixed = apply_fix(
        r#"
type Counter = struct { n: usize };
static f = fn::<@a>(c: Counter.&::<@a>) -> usize { c.n };
"#,
        "Insert `.*` before `.n`",
    );
    assert!(fixed.contains("{ c.*.n }"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
}

#[test]
fn a_dot_through_a_borrow_of_a_non_copyable_referent_offers_the_deref() {
    // A field read copies only the field...
    let fixed = apply_fix(
        r#"
type W = struct::<T> { t: T, n: usize };
static f = fn::<@a, T>(w: W::<T>.&::<@a>) -> usize { w.n };
"#,
        "Insert `.*` before `.n`",
    );
    assert!(fixed.contains("{ w.*.n }"), "fixed: {fixed}");
    check_diagnostics(&fixed, expect![[r#""#]]);
    // ...and a field that cannot be copied still gets the edit the message
    // names, which then reports the move.
    let fixed = apply_fix(
        r#"
type W = struct::<T> { t: T };
static f = fn::<@a, T>(w: W::<T>.&::<@a>) -> T { w.t };
"#,
        "Insert `.*` before `.t`",
    );
    assert!(fixed.contains("{ w.*.t }"), "fixed: {fixed}");
    check_diagnostics(
        &fixed,
        expect![[r#"
        81..86: cannot move out of a borrow: `T` cannot be copied
    "#]],
    );
}

#[test]
fn a_variant_pattern_on_a_borrowed_struct_names_the_struct() {
    // A pattern that looks inside a borrowed value meets what is behind
    // the borrow, so the message names the referent; a match of only `_`
    // never looks inside at all.
    check_diagnostics(
        "type P = struct { x: usize };\n\
         static f = fn::<@a>(p: P.&::<@a>, s: str.&::<@a>) -> usize {\n\
             match p { ::Some(t) => 1, _ => 0 } + match s { _ => 2 }\n\
         };\n\
         static g = fn::<@a>(s: str.&::<@a>) -> usize { match s { } };",
        expect![[r#"
            101..110: a variant pattern needs an enum; only `_` or a binding can match a `P`
            197..202: this `match` does not cover every possible `str`; add a `_` arm
        "#]],
    );
}

#[test]
fn a_bind_shadowing_a_variant_warns_through_a_borrow_too() {
    // FLAGGED as a deliberate behavior change: before the projection a
    // borrowed scrutinee classified as "some other type", so this warning
    // — a bare binder named like a variant of the scrutinee's enum, which
    // is almost always a stale rename — could not fire through one. It is
    // the same footgun either way, so it now does.
    check_diagnostics(
        &format!(
            "{OPT}static f = fn::<@a>(s: Opt::<usize>.&::<@a>) -> usize {{\n\
                 match s {{ None => 0 }}\n\
             }};"
        ),
        expect![[r#"
            106..110: `None` binds the whole value; write `::None` (or `Opt::None`) to match the variant (`Opt` is defined here at 5..8)
        "#]],
    );
}
