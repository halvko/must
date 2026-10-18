//! Recorded expectations, and expected signatures flowing into fn literals.

use super::*;

#[test]
fn expectation_recorded_for_annotated_let_initializer() {
    check_expectations(
        "static main = fn { let x: usize = 5; };",
        expect![[r#"
            14..38 'fn { let x: usize...': fn()
            17..38 '{ let x: usize = ...': ()
            34..35 '5': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_call_argument() {
    check_expectations(
        r#"static main = fn { print("hi"); };"#,
        expect![[r#"
            14..33 'fn { print("hi"); }': fn()
            17..33 '{ print("hi"); }': ()
            19..24 'print': fn(str)
            19..30 'print("hi")': ()
            25..29 '"hi"': str
        "#]],
    );
}

#[test]
fn expectation_recorded_for_record_literal_field() {
    check_expectations(
        r#"
type Foo = struct { x: usize };
static main = fn { Foo(struct { x = 1 }); };
"#,
        expect![[r#"
            47..76 'fn { Foo(struct {...': fn()
            50..76 '{ Foo(struct { x ...': ()
            52..73 'Foo(struct { x = ...': Foo
            56..72 'struct { x = 1 }': struct { x: usize }
            69..70 '1': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_if_branches_under_annotation() {
    check_expectations(
        "static f: fn(bool) -> usize = fn (b: bool) -> usize { if b { 1 } else { 2 } };",
        expect![[r#"
            30..77 'fn (b: bool) -> u...': fn(bool) -> usize
            52..77 '{ if b { 1 } else...': usize
            54..75 'if b { 1 } else {...': usize
            57..58 'b': bool
            59..64 '{ 1 }': usize
            61..62 '1': usize
            70..75 '{ 2 }': usize
            72..73 '2': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_match_arm_tails() {
    check_expectations(
        r#"
type Shape = enum { Circle, Square };
static f: fn(Shape) -> usize = fn (s: Shape) -> usize {
    match s { ::Circle => 1, ::Square => 2 }
};
"#,
        expect![[r#"
            70..141 'fn (s: Shape) -> ...': fn(Shape) -> usize
            93..141 '{     match s { :...': usize
            99..139 'match s { ::Circl...': usize
            121..122 '1': usize
            136..137 '2': usize
        "#]],
    );
}

#[test]
fn no_expectation_recorded_for_method_receiver() {
    check_expectations(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static f: fn(A) -> usize = fn (a: A) -> usize { a.get() };
"#,
        expect![[r#"
            133..163 'fn (a: A) -> usiz...': fn(A) -> usize
            152..163 '{ a.get() }': usize
            154..159 'a.get': fn(A) -> usize
            154..161 'a.get()': usize
        "#]],
    );
}

#[test]
fn expectation_recorded_for_break_value() {
    check_expectations(
        "static f: fn() -> usize = fn () -> usize { loop { break 1; } };",
        expect![[r#"
            26..62 'fn () -> usize { ...': fn() -> usize
            41..62 '{ loop { break 1;...': usize
            43..60 'loop { break 1; }': usize
            56..57 '1': usize
        "#]],
    );
}

#[test]
fn expectation_for_bare_let_initializer_is_its_own_type() {
    // Pinned behavior, not a design statement: a bare `let`'s initializer
    // is checked against a fresh variable, which the check then unifies
    // with the initializer's own type — so the recorded expectation
    // resolves concrete (to `usize` here) and is KEPT, even though nothing
    // outside the expression demanded it.
    check_expectations(
        "static main = fn { let x = 5; };",
        expect![[r#"
            14..31 'fn { let x = 5; }': fn()
            17..31 '{ let x = 5; }': ()
        "#]],
    );
}

#[test]
fn expectation_dropped_when_fresh_variable_stays_unbound() {
    // `panic(..)` is `!`, which *adopts* a still-free expectation instead
    // of unifying with it (see `InferCtx::check`'s early `Never` path) —
    // the bare `let`'s fresh variable stays unbound, so the initializer
    // call gets NO kept expectation. Its argument still does (`str`, the
    // builtin's parameter type).
    check_expectations(
        r#"static main = fn { let x = panic("msg"); };"#,
        expect![[r#"
            14..42 'fn { let x = pani...': fn()
            17..42 '{ let x = panic("...': ()
            27..32 'panic': fn(str) -> !
            33..38 '"msg"': str
        "#]],
    );
}

/// Companion to the two firewall tests above (same event-log style):
/// expectation recording must not leak extra invalidation. A body edit
/// that changes no types — and so no *kept* expectations — re-executes
/// only the edited item's own `infer`, and produces a value-EQUAL
/// `InferenceResult` (expectations included: they are range-free and hold
/// no canonicalized variable indices, those entries are dropped), so
/// everything downstream of the query backdates exactly as before
/// expectation recording existed.
#[test]
fn firewall_expectation_recording_backdates_unchanged_types() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: fn() -> usize = fn () -> usize { let x: usize = 1; x };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";
    // Only the literal changes: every type, and every kept expectation
    // (`x`'s annotated initializer among them), is identical.
    let text_v2 = "static a: fn() -> usize = fn () -> usize { let x: usize = 2; x };\n\
                   static b: fn() -> usize = fn () -> usize { a() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    let before: Vec<std::sync::Arc<crate::InferenceResult>> = crate::file_item_ids(&db, file)
        .iter()
        .map(|&item| crate::infer::infer(&db, item).clone())
        .collect();
    assert!(
        before
            .iter()
            .any(|result| result.expectation_of_expr.iter().count() > 0),
        "the fixture records expectations at all"
    );

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    let after: Vec<std::sync::Arc<crate::InferenceResult>> = crate::file_item_ids(&db, file)
        .iter()
        .map(|&item| crate::infer::infer(&db, item).clone())
        .collect();
    assert_eq!(
        before, after,
        "a types-preserving edit must leave the results value-equal (backdating)"
    );
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

// ---- an fn literal takes its signature from its position ---------------
//
// An unannotated parameter or return type on a `fn` literal comes from the
// EXPECTATION at the position the literal sits in, before the body is
// checked. Recorded with its scope, because it has a sharp edge: a join
// leaf has no expectation to give, so a literal in witness position keeps
// its own fresh variables and mismatches as a whole.

#[test]
fn an_annotation_flows_its_signature_into_the_literal() {
    // The bug this rule fixes, in its smallest form and with nothing
    // generic in sight: the tail is a VARIANT, the sanctioned variant→enum
    // conversion happens at a CHECK, and the check is against the
    // literal's RETURN type — so the expected `Option::<usize>` has to be
    // in hand while the tail is checked. Before the rule this was
    // "expected `fn(usize) -> Option::<usize>`, found `fn(usize) ->
    // Option::<usize>::Some`", with the squiggle on the whole literal
    // rather than on anything the reader could act on.
    //
    // Both annotation positions, because they are different axioms: an
    // ITEM's own annotation, and a `let` binding's.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static item_annotation: fn(usize) -> Option::<usize> = fn(t) { Option::Some(t) };
static let_annotation = fn(n: usize) -> Option::<usize> {
    let f: fn(usize) -> Option::<usize> = fn(t) { Option::Some(t) };
    f(n)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_field_an_element_and_a_return_all_carry_the_expectation() {
    // The register in `types-and-data.md` claims to be exhaustive, so the
    // four positions that are neither an annotation nor a call argument
    // are pinned here — for BOTH rules that turn on it: the `::` sigil
    // (which needs an expected ENUM) and the fn-literal rule (which needs
    // an expected SIGNATURE).
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
type Box = struct { v: Option::<usize>, fs: [fn(usize) -> Option::<usize>; 1] };
static field_and_element = fn(n: usize) -> Box {
    Box(struct { v = ::Some(n), fs = [fn(t) { ::Some(t) }] })
};
static returned_sigil = fn(n: usize) -> Option::<usize> { return ::Some(n); };
static returned_literal = fn() -> fn(usize) -> Option::<usize> {
    return fn(t) { Option::Some(t) };
};
static assigned_rhs = fn(n: usize) -> Box {
    let mut b = Box(struct { v = ::None, fs = [fn(t) { ::Some(t) }] });
    b.v = ::Some(n);
    b.fs = [fn(t) { Option::Some(t) }];
    b
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn the_registers_other_enum_capable_positions_carry_it_too() {
    // The rest of the register's rows that can hold an ENUM, pinned for
    // the `::` sigil the same way: the right operand of `==`/`!=` takes
    // the left operand's type, and the three pass-through positions — a
    // block's tail, a `const` block's body and an `unsafe` block's body —
    // hand the enclosing expectation straight down. (A `Newtype`
    // constructor's argument is an expectation position too, but its
    // underlying type is always a `struct`/`enum` literal, so no sigil
    // can land there.)
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static eq_operand = fn(o: Option::<usize>, n: usize) -> bool { o == ::Some(n) };
static block_tail = fn(n: usize) -> Option::<usize> { { ::Some(n) } };
static const_block = fn() -> Option::<usize> { const { ::Some(1) } };
static unsafe_block = fn(n: usize) -> Option::<usize> { unsafe { ::Some(n) } };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_argument_position_flows_its_signature_into_the_literal() {
    // The same rule at the position that matters most in practice — a
    // literal handed to a higher-order function — in isolation from
    // members and from `let`.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static apply = fn::<T, U>(g: fn(T) -> U, t: T) -> U { g(t) };
static main = fn(n: usize) -> Option::<usize> {
    apply::<usize, Option::<usize>>(fn(t) { Option::Some(t) }, n)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_expected_signature_names_the_literals_parameters() {
    // The PARAMETER half of the same rule: an unannotated parameter takes
    // its type from the position too, so the body may use it for real
    // (`t.n` needs `t` to be a `Counter`, and nothing in the literal says
    // so). The literal's own annotation still wins where there is one.
    check_infer(
        r#"
type Counter = struct { n: usize };
static apply = fn(g: fn(Counter) -> usize, c: Counter) -> usize { g(c) };
static main = fn(c: Counter) -> usize { apply(fn(t) { t.n }, c) };
"#,
        expect![[r#"
            52..109 'fn(g: fn(Counter)...': fn(fn(Counter) -> usize, Counter) -> usize
            55..56 'g': fn(Counter) -> usize
            80..81 'c': Counter
            101..109 '{ g(c) }': usize
            103..104 'g': fn(Counter) -> usize
            103..107 'g(c)': usize
            105..106 'c': Counter
            125..176 'fn(c: Counter) ->...': fn(Counter) -> usize
            128..129 'c': Counter
            149..176 '{ apply(fn(t) { t...': usize
            151..156 'apply': fn(fn(Counter) -> usize, Counter) -> usize
            151..174 'apply(fn(t) { t.n...': usize
            157..170 'fn(t) { t.n }': fn(Counter) -> usize
            160..161 't': Counter
            163..170 '{ t.n }': usize
            165..166 't': Counter
            165..168 't.n': usize
            172..173 'c': Counter
        "#]],
    );
}

#[test]
fn an_expected_signature_reaches_a_destructuring_parameter() {
    // The other parameter-lowering path: a pattern parameter with no
    // annotation of its own and no `Newtype` head to name its type. It
    // asks the position, and the destructure then checks against what came
    // back.
    check_diagnostics(
        r#"
static apply = fn(
    g: fn(struct { x: usize, y: usize }) -> usize,
    p: struct { x: usize, y: usize },
) -> usize { g(p) };
static main = fn(p: struct { x: usize, y: usize }) -> usize {
    apply(fn(struct { x, y }) { x + y }, p)
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_inherited_return_type_is_an_axiom_position_for_the_sigil() {
    // A consequence worth pinning rather than discovering: the `::`
    // variant sigil resolves wherever an expectation reaches, so giving a
    // literal's body one opened a NEW axiom position for it. Recorded in
    // `types-and-data.md`'s expectation register.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static f: fn(usize) -> Option::<usize> = fn(t) { ::Some(t) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_expectation_of_the_wrong_arity_is_not_inherited() {
    // The guard: the expectation is only a signature for this literal when
    // it AGREES on parameter count. A literal that takes two parameters at
    // a one-parameter position must keep its own fresh variables and
    // mismatch as a whole — inheriting positionally from a list of the
    // wrong length would put the wrong type on the wrong name and blame
    // the body for it.
    check_diagnostics(
        r#"
static apply = fn(g: fn(usize) -> usize, n: usize) -> usize { g(n) };
static main = fn(n: usize) -> usize { apply(fn(a, b) { n }, n) };
"#,
        expect![[r#"
            115..129: type mismatch: expected `fn(usize) -> usize`, found `fn(_, _) -> usize`
        "#]],
    );
}

#[test]
fn an_inherited_return_type_points_at_the_position_it_came_from() {
    // A tail mismatch inside the literal names the POSITION as its
    // reason: the return type was not written on the literal, so the slot
    // that supplied it is the only thing there is to point at — and the
    // reader did write that. (The second complaint is the join-leaf limit
    // below, not this rule.)
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static f: fn(bool) -> Option::<usize> = fn(b) { if b { 1 } else { ::None } };
"#,
        expect![[r#"
            92..118: every branch produces `{number}`, but `Option::<usize>` is needed (expected `Option::<usize>` because of this annotation at 54..81)
            110..116: cannot resolve `::None` without an expected type — write `Enum::None`
        "#]],
    );
}

#[test]
fn a_literal_that_mismatches_its_slot_does_not_also_blame_its_tail() {
    // A signature that disagrees is the whole story. The unwritten return
    // is still a bare variable when the mismatch is reported, so `check`'s
    // unresolved-number poisoning cannot reach the number the BODY mints
    // afterwards; it runs again once the body is in. Without that, a
    // literal that already mismatched as a whole also collects a
    // "no defining use" complaint about a tail nothing was going to keep.
    check_diagnostics(
        r#"
static apply = fn(g: fn(usize) -> usize, n: usize) -> usize { g(n) };
static arity = fn(n: usize) -> usize { apply(fn(a, b) { 1 }, n) };
static param: fn(usize) -> usize = fn(t: str) { 1 };
"#,
        expect![[r#"
            116..130: type mismatch: expected `fn(usize) -> usize`, found `fn(_, _) -> {error}`
            173..189: type mismatch: expected `fn(usize) -> usize`, found `fn(str) -> {error}` (expected `fn(usize) -> usize` because of this annotation at 152..170)
        "#]],
    );
}

#[test]
fn an_unknown_type_in_the_slot_is_inherited_silently() {
    // Why no `Ty::Error` filtering is written at the literal: `unify`
    // binds `{error}` to the literal's parameter like any other type and
    // errors are infectious and silent, so the body's use of it is
    // accepted and the unknown type is reported once, where it is written.
    check_diagnostics(
        r#"
static apply = fn(g: fn(Bogus) -> usize, n: usize) -> usize { n };
static main = fn(n: usize) -> usize { apply(fn(t) { t.n }, n) };
"#,
        expect![[r#"
            25..30: unknown type `Bogus`
        "#]],
    );
}

#[test]
fn known_gap_the_expectation_does_not_reach_a_fn_literal_in_a_join_leaf() {
    // A fn literal in a join leaf gets no expectation, so its elided
    // `::Some` has no enum to resolve against and is refused. See
    // "Expectations reach join leaves" in `types-and-data.md`.
    check_diagnostics(
        r#"
type Option = enum::<T> { Some(T), None };
static pick = fn(b: bool) -> fn(usize) -> Option::<usize> {
    if b { fn(t) { ::Some(t) } } else { fn(t) { ::Some(t) } }
};
"#,
        expect![[r#"
            123..129: cannot resolve `::Some` without an expected type — write `Enum::Some`
            152..158: cannot resolve `::Some` without an expected type — write `Enum::Some`
        "#]],
    );
}
