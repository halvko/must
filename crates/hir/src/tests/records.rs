//! Record literals, record types and field access.

use super::*;

// --- Records: structural typing ---

#[test]
fn record_literal_infers_structurally() {
    check_infer(
        r#"static f = fn { let p = struct { x = 1, y = "s" }; };"#,
        expect![[r#"
            11..52 'fn { let p = stru...': fn()
            14..52 '{ let p = struct ...': ()
            20..21 'p': struct { x: {number}, y: str }
            24..49 'struct { x = 1, y...': struct { x: {number}, y: str }
            37..38 '1': {number}
            44..47 '"s"': str
        "#]],
    );
}

#[test]
fn record_type_canonicalizes_field_order() {
    // The annotation writes the fields in one order, the literal in the
    // other: field order is irrelevant to the type, so both are the same
    // record and nothing is reported.
    check_diagnostics(
        r#"static f = fn { let p: struct { y: str, x: usize } = struct { x = 1, y = "s" }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_field_mismatch_blames_the_field_and_cites_the_annotation() {
    // Bidirectional: the annotation's field types flow into the field
    // initializers, so the squiggle lands on `"s"` (not the whole literal)
    // and cites the annotation as the cause.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x = "s" }; };"#,
        expect![[r#"
            58..61: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 23..42)
        "#]],
    );
}

#[test]
fn record_literal_missing_field() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str } = struct { x = 1 }; };"#,
        expect![[r#"
            53..69: record literal is missing field `y: str`
        "#]],
    );
}

#[test]
fn record_literal_missing_several_fields() {
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize, y: str, z: bool } = struct { y = "s" }; };"#,
        expect![[r#"
            62..80: record literal is missing fields `x: usize`, `z: bool`
        "#]],
    );
}

#[test]
fn record_literal_extra_field() {
    // Exact field-set equality: the extra field is an error (squiggle on
    // its name), never silently dropped.
    check_diagnostics(
        r#"static f = fn { let p: struct { x: usize } = struct { x = 1, z = 2 }; };"#,
        expect![[r#"
            61..62: no field `z` in expected type `struct { x: usize }`
            65..66: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

#[test]
fn record_literal_against_non_record_expectation() {
    check_diagnostics(
        r#"static f = fn { let n: usize = struct { x = 1 }; };"#,
        expect![[r#"
            31..47: type mismatch: expected `usize`, found `struct { x: {number} }` (expected `usize` because of this annotation at 23..28)
        "#]],
    );
}

#[test]
fn field_access_infers_the_field_type() {
    check_infer(
        r#"static f = fn { let p = struct { x = 1 }; let y = p.x; };"#,
        expect![[r#"
            11..56 'fn { let p = stru...': fn()
            14..56 '{ let p = struct ...': ()
            20..21 'p': struct { x: {number} }
            24..40 'struct { x = 1 }': struct { x: {number} }
            37..38 '1': {number}
            46..47 'y': {number}
            50..51 'p': struct { x: {number} }
            50..53 'p.x': {number}
        "#]],
    );
}

#[test]
fn field_access_unknown_field() {
    check_diagnostics(
        r#"static f = fn { let p = struct { x = 1 }; p.z; };"#,
        expect![[r#"
            37..38: cannot infer the type of this number: it has no defining use — add a type annotation
            44..45: no field `z` on `struct { x: {number} }`
        "#]],
    );
}

#[test]
fn field_access_on_non_record() {
    check_diagnostics(
        r#"static f = fn { let n = 1; n.x; };"#,
        expect![[r#"
            24..25: cannot infer the type of this number: it has no defining use — add a type annotation
            27..28: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn chained_field_access_through_nested_records() {
    check_infer(
        r#"static f = fn { let a = struct { b = struct { c = "deep" } }; a.b.c };"#,
        expect![[r#"
            11..69 'fn { let a = stru...': fn() -> str
            14..69 '{ let a = struct ...': str
            20..21 'a': struct { b: struct { c: str } }
            24..60 'struct { b = stru...': struct { b: struct { c: str } }
            37..58 'struct { c = "dee...': struct { c: str }
            50..56 '"deep"': str
            62..63 'a': struct { b: struct { c: str } }
            62..65 'a.b': struct { c: str }
            62..67 'a.b.c': str
        "#]],
    );
}

#[test]
fn field_access_on_unannotated_param_requires_an_annotation() {
    // Exact structural equality can't run backwards from a field name, so
    // an undetermined receiver is reported (on the receiver — the
    // annotation goes there), not silently left dangling.
    check_diagnostics(
        r#"static f = fn (p) { p.x };"#,
        expect![[r#"
            20..21: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

/// A field access on an undetermined receiver waits for it:
/// `translate`'s caller makes `p` a `Point`, and `p.x + dx` then makes `dx`
/// a `usize`.
#[test]
fn field_access_waits_for_a_caller_to_determine_the_receiver() {
    let text = r#"
type Point = struct { x: usize, y: usize };
static origin = Point(struct { x = 0, y = 0 });
static translate = const fn (p, dx) {
    Point(struct { x = p.x + dx, y = p.y })
};
static moved = translate(origin, 5);
"#;
    check_signatures(
        text,
        expect![[r#"
        origin: Point
        translate: fn(Point, usize) -> Point
        moved: Point
    "#]],
    );
    check_diagnostics(text, expect![[r#""#]]);
    check_infer(
        text,
        expect![[r#"
        61..66 'Point': fn(struct { x: usize, y: usize }) -> Point
        61..91 'Point(struct { x ...': Point
        67..90 'struct { x = 0, y...': struct { x: usize, y: usize }
        80..81 '0': usize
        87..88 '0': usize
        112..176 'const fn (p, dx) ...': fn(Point, usize) -> Point
        122..123 'p': Point
        125..127 'dx': usize
        129..176 '{     Point(struc...': Point
        135..140 'Point': fn(struct { x: usize, y: usize }) -> Point
        135..174 'Point(struct { x ...': Point
        141..173 'struct { x = p.x ...': struct { x: usize, y: usize }
        154..155 'p': Point
        154..157 'p.x': usize
        154..162 'p.x + dx': usize
        160..162 'dx': usize
        168..169 'p': Point
        168..171 'p.y': usize
        193..202 'translate': fn(Point, usize) -> Point
        193..213 'translate(origin, 5)': Point
        203..209 'origin': Point
        211..212 '5': usize
    "#]],
    );
}

/// The same for an index, and through a chain of projections: each one
/// waits for the one before it.
#[test]
fn index_and_chained_fields_wait_for_the_receiver() {
    let text = r#"
static first = fn (a) { a[0] };
static inner = fn (r) { r.a.b };
static main = fn () -> usize {
    first([1, 2]) + inner(struct { a = struct { b = 3 } })
};
"#;
    check_signatures(
        text,
        expect![[r#"
        first: fn([usize; 2]) -> usize
        inner: fn(struct { a: struct { b: usize } }) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(text, expect![[r#""#]]);
}

/// A receiver determined later in the same body: the call after the field
/// access types `p`.
#[test]
fn field_access_waits_for_a_later_use_in_the_same_body() {
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize };
static take = fn (_p: Point) -> () { };
static f = fn (p) -> usize { let x = p.x; take(p); x };
"#,
        expect![[r#""#]],
    );
}

/// A deferred `let mut q;` read before it is assigned: the access is typed
/// from the assignment, so type checking reports nothing; the read before
/// the assignment is MIR's initialization check
/// (`ide::tests::a_deferred_field_access_before_assignment_reports_the_read`).
#[test]
fn field_access_on_a_later_assigned_local_waits() {
    let text = r#"
type Point = struct { x: usize, y: usize };
static f = fn (c: bool) -> usize {
    let mut q;
    if c { return q.x; };
    q = Point(struct { x = 1, y = 2 });
    q.y
};
"#;
    check_diagnostics(text, expect![[r#""#]]);
    check_infer(
        text,
        expect![[r#"
            56..170 'fn (c: bool) -> u...': fn(bool) -> usize
            60..61 'c': bool
            78..170 '{     let mut q; ...': usize
            92..93 'q': Point
            99..119 'if c { return q.x; }': ()
            102..103 'c': bool
            104..119 '{ return q.x; }': ()
            106..116 'return q.x': !
            113..114 'q': Point
            113..116 'q.x': usize
            125..126 'q': Point
            129..134 'Point': fn(struct { x: usize, y: usize }) -> Point
            129..159 'Point(struct { x ...': Point
            135..158 'struct { x = 1, y...': struct { x: usize, y: usize }
            148..149 '1': usize
            155..156 '2': usize
            165..166 'q': Point
            165..168 'q.y': usize
        "#]],
    );
}

/// Projections across members resolve whatever order the members are
/// written in: each caller determines its callee's receiver in turn.
#[test]
fn projections_across_members_resolve_in_any_order() {
    let callee_first = r#"
type Inner = struct { x: usize };
type Mid = struct { i: Inner };
type Outer = struct { m: Mid };
static a = fn (p) { p.x };
static b = fn (q) { a(q.i) };
static c = fn (r) { b(r.m) };
static o = Outer(struct { m = Mid(struct { i = Inner(struct { x = 1 }) }) });
static main = fn () -> usize { c(o) };
"#;
    let expected = expect![[r#"
        a: fn(Inner) -> usize
        b: fn(Mid) -> usize
        c: fn(Outer) -> usize
        o: Outer
        main: fn() -> usize
    "#]];
    check_signatures(callee_first, expected);
    check_diagnostics(callee_first, expect![[r#""#]]);
    let caller_first = r#"
type Inner = struct { x: usize };
type Mid = struct { i: Inner };
type Outer = struct { m: Mid };
static main = fn () -> usize { c(o) };
static o = Outer(struct { m = Mid(struct { i = Inner(struct { x = 1 }) }) });
static c = fn (r) { b(r.m) };
static b = fn (q) { a(q.i) };
static a = fn (p) { p.x };
"#;
    check_signatures(
        caller_first,
        expect![[r#"
        main: fn() -> usize
        o: Outer
        c: fn(Outer) -> usize
        b: fn(Mid) -> usize
        a: fn(Inner) -> usize
    "#]],
    );
    check_diagnostics(caller_first, expect![[r#""#]]);
}

/// Calling a field whose receiver is undetermined commits it to a safe
/// `fn`, which a caller then confirms.
#[test]
fn calling_a_field_of_an_undetermined_receiver() {
    let text = r#"
static inc = fn (n: usize) -> usize { n + 1 };
static call = fn (h) { (h.f)(1) };
static main = fn () -> usize { call(struct { f = inc }) };
"#;
    check_signatures(
        text,
        expect![[r#"
        inc: fn(usize) -> usize
        call: fn(struct { f: fn(usize) -> usize }) -> usize
        main: fn() -> usize
    "#]],
    );
    check_diagnostics(text, expect![[r#""#]]);
}

/// A caller whose field is not the `fn` the call commits it to: a mismatch.
#[test]
fn calling_a_field_of_an_undetermined_receiver_checks_the_field() {
    check_diagnostics(
        r#"
static call = fn (h) -> usize { (h.f)(1) };
static main = fn () -> usize { call(struct { f = true }) };
"#,
        expect![[r#"
            34..37: type mismatch: expected `fn({number}) -> usize`, found `bool`
        "#]],
    );
}

/// What a determined receiver turns out to lack is reported at the access
/// once the receiver is known. `z`'s use says nothing: like any member
/// whose body meets an error in what it produces, `z` carries its own
/// error.
#[test]
fn a_deferred_field_access_reports_a_missing_field() {
    let text = r#"
type Point = struct { x: usize, y: usize };
static origin = Point(struct { x = 0, y = 0 });
static z = fn (p) { p.z };
static main = fn () { z(origin); };
"#;
    check_signatures(
        text,
        expect![[r#"
        origin: Point
        z: fn(Point) -> {error}
        main: fn()
    "#]],
    );
    check_diagnostics(
        text,
        expect![[r#"
        115..116: no field `z` on `Point` (the fields of `Point` are declared here at 14..43)
    "#]],
    );
}

/// A deferred field access whose value doesn't fit its context reports the
/// mismatch at the access once the receiver is known.
#[test]
fn a_deferred_field_access_reports_a_mismatch() {
    check_diagnostics(
        r#"
type Point = struct { x: usize, y: usize };
static origin = Point(struct { x = 0, y = 0 });
static wrong = fn (p) -> bool { p.x };
static main = fn () { wrong(origin); };
"#,
        expect![[r#"
            125..128: type mismatch: expected `bool`, found `usize` (expected `bool` because of this return type at 115..122)
        "#]],
    );
}

#[test]
fn shorthand_field_resolves_and_type_checks() {
    check_diagnostics(
        r#"static f = fn { let x = 1; let p: struct { x: usize } = struct { x, }; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn shorthand_field_with_wrong_type_blames_the_shorthand() {
    // The shorthand's value is a normal reference to `s`, so the squiggle
    // lands on the shorthand name itself.
    check_diagnostics(
        r#"static f = fn { let s = "a"; let p: struct { s: usize } = struct { s, }; };"#,
        expect![[r#"
            67..68: type mismatch: expected `usize`, found `str` (expected `usize` because of this annotation at 36..55)
        "#]],
    );
}

#[test]
fn unresolved_shorthand_field_reports_unresolved_name() {
    check_diagnostics(
        r#"static f = fn { let p = struct { x, }; };"#,
        expect![[r#"
            33..34: unresolved name `x`
        "#]],
    );
}

#[test]
fn if_branches_with_mismatched_records_report_branch_mismatch() {
    // Join regression: records go through the existing join machinery
    // unchanged, so two branches disagreeing on a record type produce the
    // ordinary branch-mismatch diagnostic, verbatim.
    check_diagnostics(
        r#"static f = fn (c: bool) { if c { struct { x = 1 } } else { struct { x = "s" } } };"#,
        expect![[r#"
            33..49: type mismatch: expected `struct { x: str }`, found `struct { x: {number} }` (this branch has type `struct { x: str }` at 59..77)
        "#]],
    );
}

#[test]
fn record_literal_in_initializer_is_const_clean() {
    // Record construction is not a call: const-checking has nothing to say
    // about a pure record literal in an item initializer.
    check_diagnostics(
        r#"static p = struct { x = { let n: usize = 1; n }, y = "s" };"#,
        expect![[r#""#]],
    );
}

#[test]
fn record_signature_flows_across_items() {
    check_diagnostics(
        r#"
static origin: struct { x: usize, y: usize } = struct { x = 0, y = 0 };
static f = fn { let x = origin.x; };
"#,
        expect![[r#""#]],
    );
}

/// `is_fully_typed` digs into record fields: a record annotation with a
/// hole field is only a partial contract, so the item routes through group
/// inference and — the hole staying unfilled — the use site asks for an
/// annotation instead of the annotation acting as a firewall. A hole-free
/// record annotation is a full contract: its item is a firewall edge, and
/// editing its body never re-runs its caller's group inference.
#[test]
fn hole_field_record_annotation_is_not_a_firewall_contract() {
    // `panic` diverges, so nothing ever fills `y`: the group signature has
    // an undetermined field, and the use site requests an annotation
    // rather than silently publishing `y: _` as if it were a contract.
    check_diagnostics(
        r#"
static s: struct { x: usize, y: _ } = panic("boom");
static main = fn { s; };
"#,
        expect![[r#"
            73..74: not enough information to infer the type of `s`; add a type annotation to its definition (defined here at 8..9)
        "#]],
    );

    // Positive: `a`'s annotation names every field, so it is a full
    // contract — `a` stays out of `b`'s binding group.
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static a: struct { x: usize } = struct { x = 1 };\n\
                   static b = fn { a.x; };\n";
    // Only `a`'s body changes; the annotation (the whole contract) is
    // identical, so dependents must backdate.
    let text_v2 = "static a: struct { x: usize } = struct { x = 2 };\n\
                   static b = fn { a.x; };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let executed_infers = |log: &Mutex<Vec<String>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.contains("infer"))
            .count()
    };
    assert!(
        executed_infers(&log) >= 2,
        "both items inferred initially; executed: {:?}",
        log.lock().unwrap()
    );

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    // `a`'s annotation is a full contract, so `b` never joins its group:
    // editing `a`'s body re-infers `a` alone and never runs `infer_group`.
    assert!(
        !log.iter().any(|entry| entry.contains("infer_group")),
        "a hole-free record annotation must be a firewall edge: {log:#?}"
    );
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited item may re-infer; executed: {log:#?}"
    );
}

// ---- the record-literal equals-defines respell --------------------------

#[test]
fn record_field_ascription_checks_the_value() {
    // The spelled-out member production `name: Type = value`: the colon
    // half is a real annotation — it pins the value's type (a defining use
    // for the literal) and must itself agree with the field's expected
    // type.
    check_infer(
        r#"
static a = struct { n: u8 = 10 };
"#,
        expect![[r#"
            12..33 'struct { n: u8 = ...': struct { n: u8 }
            29..31 '10': u8
        "#]],
    );
}

#[test]
fn record_field_ascription_mismatch_is_reported() {
    check_diagnostics(
        r#"
static a = struct { n: u8 = "no" };
"#,
        expect![[r#"
            29..33: type mismatch: expected `u8`, found `str`
        "#]],
    );
}

#[test]
fn record_field_ascription_must_agree_with_the_expected_field() {
    check_diagnostics(
        r#"
type P = struct { n: usize };
static a = P(struct { n: u8 = 10 });
"#,
        expect![[r#"
            61..63: type mismatch: expected `usize`, found `u8` (expected `usize` because of this field declaration at 19..27)
        "#]],
    );
}

#[test]
fn construction_field_without_a_value_is_rejected() {
    check_diagnostics(
        r#"
static a: struct { n: usize } = struct { n: usize };
"#,
        expect![[r#"
            42..50: this field has a type but no value; write `name: Type = value` (or `name = value`)
        "#]],
    );
}

#[test]
fn type_decl_field_with_a_value_is_rejected() {
    check_diagnostics(
        r#"
type P = struct { n: usize = 5 };
"#,
        expect![[r#"
            28..31: a `type` declaration's field declares a type, not a value
        "#]],
    );
}

#[test]
fn old_colon_construction_spelling_gets_the_targeted_error() {
    check_diagnostics(
        r#"
static a: struct { n: usize } = struct { n: 1 };
"#,
        expect![[r#"
            43..44: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn old_colon_construction_spelling_with_a_composite_value_is_checked_as_a_value() {
    // `p.x - 1` is type-shaped up to its `.`, but it is the value: one
    // error per field, the literal checks against `P`, and the body after
    // it still checks (the `true` is the only other error).
    check_diagnostics(
        r#"
type P = struct { x: usize, y: usize };
static f = fn (p: P) -> usize {
    let q = P(struct { x: p.x - 1, y: p.y });
    q.x + true
};
"#,
        expect![[r#"
            97..98: record fields are defined with `=` (`name = value`); `:` annotates a type
            109..110: record fields are defined with `=` (`name = value`); `:` annotates a type
            129..133: type mismatch: expected `usize`, found `bool` (`+` requires `usize` operands at 127..128)
        "#]],
    );
}

#[test]
fn a_lone_name_after_a_fields_colon_reads_as_what_it_names() {
    // `other` is a value, so `x: other` is the retired spelling; `P` and
    // `usize` are types, so those fields have no value. The literal still
    // checks against `P` (the `true` is the only other error).
    check_diagnostics(
        r#"
type P = struct { x: usize, y: usize };
static f = fn (other: usize) -> usize {
    let q = P(struct { x: other, y: P });
    let r = P(struct { x: usize, y = 1 });
    q.x + true
};
"#,
        expect![[r#"
            105..106: record fields are defined with `=` (`name = value`); `:` annotates a type
            114..118: this field has a type but no value; write `name: Type = value` (or `name = value`)
            146..154: this field has a type but no value; write `name: Type = value` (or `name = value`)
            176..180: type mismatch: expected `usize`, found `bool` (`+` requires `usize` operands at 174..175)
        "#]],
    );
}

#[test]
fn an_undecided_field_operand_reads_as_what_its_name_names() {
    // A generic path, a borrow and an array are spelled alike as types and
    // values; the name at their head decides. `Pair` and `usize` are types,
    // so those fields have no value; `id` and `n` are values, so those are
    // the retired spelling.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T };
static id = fn::<T>(t: T) -> T { t };
static f = fn (n: usize) {
    let p = struct { a: Pair::<usize>, b: id::<usize>, c: n.&, d: [usize; 3], e: [n; 3] };
};
"#,
        expect![[r#"
            121..137: this field has a type but no value; write `name: Type = value` (or `name = value`)
            140..141: record fields are defined with `=` (`name = value`); `:` annotates a type
            156..157: record fields are defined with `=` (`name = value`); `:` annotates a type
            163..176: this field has a type but no value; write `name: Type = value` (or `name = value`)
            179..180: record fields are defined with `=` (`name = value`); `:` annotates a type
        "#]],
    );
}

#[test]
fn a_type_parameter_after_a_fields_colon_is_a_type() {
    // A type parameter is a type, so `x`, `y` and `z` have no value.
    check_diagnostics(
        r#"
static f = fn::<T>(n: usize, t: T) -> usize {
    let p = struct { x: T, y: [T; 3], z: T.&::<@_> };
    n
};
"#,
        expect![[r#"
            68..72: this field has a type but no value; write `name: Type = value` (or `name = value`)
            74..83: this field has a type but no value; write `name: Type = value` (or `name = value`)
            85..97: this field has a type but no value; write `name: Type = value` (or `name = value`)
        "#]],
    );
}

#[test]
fn a_record_operand_of_one_field_is_settled_by_its_name() {
    // A one-field record operand is settled by its field's name: `q`'s `n` is
    // a value (retired `:`), `r`'s `usize` a type. With more fields no name
    // decides: `s` is a record type, while `t` and `u` have a field (`1`,
    // `[1]`) only a value can be, so they are values and every `:` is retired.
    // `v`'s names are all values, but no single name decides a record of two
    // fields, so it reads as a record type, as `s` does.
    check_diagnostics(
        r#"
static f = fn (n: usize) -> usize {
    let p = struct { q: struct { a: n }, r: struct { a: usize } };
    let o = struct { s: struct { a: usize, b: n }, t: struct { a: n, b: 1 }, u: struct { a: n, b: [1] }, v: struct { a: n, b: n } };
    n
};
"#,
        expect![[r#"
            59..60: record fields are defined with `=` (`name = value`); `:` annotates a type
            78..100: this field has a type but no value; write `name: Type = value` (or `name = value`)
            125..153: this field has a type but no value; write `name: Type = value` (or `name = value`)
            150..151: unknown type `n`
            156..157: record fields are defined with `=` (`name = value`); `:` annotates a type
            168..169: record fields are defined with `=` (`name = value`); `:` annotates a type
            174..175: record fields are defined with `=` (`name = value`); `:` annotates a type
            176..177: cannot infer the type of this number: it has no defining use — add a type annotation
            182..183: record fields are defined with `=` (`name = value`); `:` annotates a type
            194..195: record fields are defined with `=` (`name = value`); `:` annotates a type
            200..201: record fields are defined with `=` (`name = value`); `:` annotates a type
            203..204: cannot infer the type of this number: it has no defining use — add a type annotation
            209..233: this field has a type but no value; write `name: Type = value` (or `name = value`)
            224..225: unknown type `n`
            230..231: unknown type `n`
        "#]],
    );
}

#[test]
fn a_value_less_field_whose_parse_recovered_still_gets_its_error() {
    // A missing `,` and an unclosed turbofish each leave `n` and `p` an
    // annotation with no value. TODO: halvko/must#242: the open generic list
    // swallows `n: 1`, so its retired `:` goes unreported and `q` and `n`
    // are checked as type arguments.
    check_diagnostics(
        r#"
type Box = struct::<T> { v: T };
static a = struct { n: usize m = 1 };
static b = fn (q: usize) { struct { p: Box::<usize = q, n: 1 }; };
"#,
        expect![[r#"
            54..62: this field has a type but no value; write `name: Type = value` (or `name = value`)
            63..64: expected `,` or `}`
            67..68: cannot infer the type of this number: it has no defining use — add a type annotation
            108..129: this field has a type but no value; write `name: Type = value` (or `name = value`)
            125..126: unknown type `q`
            128..129: unknown type `n`
            129..130: expected `,` or `>`
        "#]],
    );
}

#[test]
fn a_lone_unknown_or_trait_name_after_a_fields_colon() {
    // An unknown name gets only its own error. A trait is not a value, so
    // the field has no value. A `type` declaration's lone names are types.
    check_diagnostics(
        r#"
trait Show = requires { show: fn(Self) -> usize; };
type Q = struct { x: usize, y: usize };
static f = fn -> Q { Q(struct { x: missing, y: Show }) };
"#,
        expect![[r#"
            128..135: unresolved name `missing`
            137..144: this field has a type but no value; write `name: Type = value` (or `name = value`)
        "#]],
    );
}

#[test]
fn a_lone_value_name_after_a_fields_colon_offers_the_equals_fix() {
    let text = "static f = fn (n: usize) { struct { n: n }; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(u32::from(diagnostics[0].range.start()), 37);
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Replace `:` with `=`");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, " =");
    assert_eq!(fix.edits[0].range, diagnostics[0].range);
}
