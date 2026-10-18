//! Region obligations through joins and nested literals.

use super::*;

#[test]
fn the_owners_reborrow_program_checks_clean() {
    // The acceptance test for members that borrow `Self`: the owner's own
    // `get_or_default`, unmodified. It exercises all four gaps at once —
    // a member-own region binder, a dot-call on a borrow receiver, `V`
    // inferred through `Self.&mut`, and `::None` in expression position —
    // and it is also the Polonius conditional-return case: `::Some(v) => v`
    // returns a loan derived from `map` out of a match whose other arm
    // borrows `map` again.
    //
    // `examples/reborrow.must` is the same program with a body, and it
    // RUNS: the borrow it hands back really aliases the map's storage.
    check_diagnostics(
        r#"
type Option = enum::<T> {
    Some(T),
    None,
} with {
    impl Self {
        unwrap = fn(s: Self) -> T {
            match s {
                ::Some(t) => t,
                ::None => panic("unwrap was called on a ::None value"),
            }
        }
    }
}

type Map = struct { } with {
    impl Self {
        get = fn::<@b>(key: usize, m: Self.&mut::<@b>) -> Option::<usize.&mut::<@b>> { ::None };
        insert = fn::<@b>(key: usize, val: usize, m: Self.&mut::<@b>) -> () { };
    }
};

static get_or_default = fn::<@a>(
    key: usize, val: usize, map: Map.&mut::<@a>
) -> usize.&mut::<@a> {
    match map.get(key) {
        ::Some(v) => v,
        ::None => {
            map.insert(key, val);
            map.get(key).unwrap()
        },
    }
};
"#,
        expect![""],
    );
}

// ---- region obligations survive wrapping (joins, arrays, records) ------

#[test]
fn a_join_cannot_launder_an_outlives_obligation() {
    // Wrapping a borrow in an `if` must not erase the obligation its use
    // site would otherwise impose: join leaves go through the reborrow
    // relation, never region-blind `unify`, so the control and the joined
    // form agree.
    let joined = "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {\n\
                      let r: usize.&::<@a> = if c { p } else { p };\n\
                      r\n\
                  };";
    let control = "static f = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {\n\
                       let r: usize.&::<@a> = p;\n\
                       r\n\
                   };";
    check_diagnostics(
        joined,
        expect![[r#"
        100..101: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
    "#]],
    );
    check_diagnostics(
        control,
        expect![[r#"
        84..85: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
    "#]],
    );
}

#[test]
fn a_meet_in_the_shorter_position_constrains() {
    // `@b + @c` names the OVERLAP of two universals, and the only way
    // another universal can be known to cover it is to cover a member —
    // the element-set model says the intersection is empty, so
    // propagation alone demands nothing on its own.
    check_diagnostics(
        "static f = fn::<@a, @b, @c>(x: usize.&::<@a>) -> usize.&::<@b + @c> { x };",
        expect![[r#"
            70..71: using this borrow where a longer-lived one is expected needs `@a` to outlive the overlap of `@b + @c`, which this signature does not declare; declaring `@a: @b` or `@a: @c` would be enough
        "#]],
    );
    // Declaring either member is enough — the meet is shorter than both.
    check_diagnostics(
        "static f = fn::<@a: @b, @b, @c>(x: usize.&::<@a>) -> usize.&::<@b + @c> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn an_unknown_region_in_a_signature_says_so() {
    // An unknown region name is reported as such — never as an internal
    // "no error was reported" contradiction — including inside a join.
    check_diagnostics(
        "static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@nope> { x };",
        expect![[r#"
            51..56: no region named `@nope` is in scope; declare it in the binder (`fn::<@nope>`)
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@a>) -> usize.&::<@b + @nope> { x };",
        expect![[r#"
            60..65: no region named `@nope` is in scope; declare it in the binder (`fn::<@nope>`)
        "#]],
    );
}

#[test]
fn blame_lands_on_the_obligation_that_introduced_the_element() {
    // The report must blame the edge that actually introduced the
    // region element, not merely the first edge that has this region as
    // its longer side. Line 3 is the culprit here; line 2 is benign.
    check_diagnostics(
        "static f = fn::<@a, @b>(p: usize.&::<@a>) -> usize.&::<@b> {\n\
             let r: usize.&::<@a> = p;\n\
             r\n\
         };",
        expect![[r#"
            87..88: using this borrow where a longer-lived one is expected needs `@a` to outlive `@b`, which this signature does not declare; add `@a: @b` to the binder
        "#]],
    );
}

#[test]
fn a_nested_fn_literal_may_infer_its_regions() {
    // Three intentional rules compose into what would otherwise be a
    // cliff: a nested literal cannot declare a binder, `@_` was refused in
    // every parameter position, and a signature elides nothing — so no
    // function literal in a body could take a borrow parameter at all. A
    // nested literal's regions genuinely are body-local existentials, so
    // `@_` is the true thing to say about them.
    check_diagnostics(
        "static main = fn::<@a>(p: usize.&::<@a>) -> usize {\n\
             let f = fn (r: usize.&::<@_>) -> usize { r.* };\n\
             f(p)\n\
         };",
        expect![[r#""#]],
    );
    // An ITEM's signature still refuses it — its regions are parameters.
    check_diagnostics(
        "static f = fn (r: usize.&::<@_>) -> usize { r.* };",
        expect![[r#"
            28..30: `@_` cannot be used in a signature — declare the region in the binder (`fn::<@a>`) and name it here
        "#]],
    );
}

#[test]
fn degradation_happens_at_a_join_too() {
    // A join must degrade `&mut` to `&` exactly as the direct form does:
    // an `if` whose branches are a `&` and a `&mut`, consumed by a `&`
    // parameter, is the same reborrow either way.
    check_diagnostics(
        "static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static f = fn::<@a>(p: usize.&::<@a>, m: usize.&mut::<@a>, c: bool) -> usize {\n\
             get(if c { p } else { m })\n\
         };",
        expect![[r#""#]],
    );
}

#[test]
fn no_context_shape_launders_a_region_obligation() {
    let sig = "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {";
    // An `if`.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} if c {{ p }} else {{ p }} }};"),
    );
    // A `match` — the same join machinery through another spelling.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} match c {{ _ => p }} }};"),
    );
    // A `loop`/`break` value.
    check_wrapping_changes_nothing(
        &format!("{sig} p }};"),
        &format!("{sig} loop {{ break p }} }};"),
    );
    // An unannotated ARRAY literal, indexed. The literal's elements are a
    // join too, and an eager fast path must not skip it.
    check_wrapping_changes_nothing(
        "static g = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> { p };",
        "static g = fn::<@a, @b>(p: usize.&::<@b>) -> usize.&::<@a> {\n\
             let arr = [p, p];\n\
             arr[0]\n\
         };",
    );
    // A borrow nested inside a RECORD, joined. The function context
    // is load-bearing: projecting `.x` off a joined value hits a
    // pre-existing deferred-join limitation and would mask this.
    let getx = "static getx = fn::<@x>(s: struct { x: usize.&::<@x> }) -> usize.&::<@x> { s.x };\n";
    check_wrapping_changes_nothing(
        &format!(
            "{getx}static h = fn::<@a, @b>(q: usize.&::<@b>) -> usize.&::<@a> {{\n\
                 getx(struct {{ x = q }})\n\
             }};"
        ),
        &format!(
            "{getx}static h = fn::<@a, @b>(q: usize.&::<@b>, c: bool) -> usize.&::<@a> {{\n\
                 getx(if c {{ struct {{ x = q }} }} else {{ struct {{ x = q }} }})\n\
             }};"
        ),
    );
    // A borrow under a NOMINAL type's generic argument. `Ty::Record` above
    // and `Ty::Named` here are the same obligation one constructor apart,
    // but the region walkers stopped
    // at `Record` — a generic type constructor was a laundry, and a local
    // escaped through `Opt::<usize.&>` with no diagnostic at all. Three
    // walkers had the hole (`relate_type_regions`, `freshen_regions`,
    // `substitute_regions`); one shape pins them.
    let opt = "type Opt = enum::<T> { Some(T), None };\n";
    check_wrapping_changes_nothing(
        &format!(
            "{opt}static k = fn::<@a, @b>(q: usize.&::<@b>) -> Opt::<usize.&::<@a>> {{\n\
                 Opt::<usize.&::<@b>>::Some(q)\n\
             }};"
        ),
        &format!(
            "{opt}static k = fn::<@a, @b>(q: usize.&::<@b>, c: bool) -> Opt::<usize.&::<@a>> {{\n\
                 if c {{ Opt::<usize.&::<@b>>::Some(q) }} \
                 else {{ Opt::<usize.&::<@b>>::Some(q) }}\n\
             }};"
        ),
    );
    // The member-call shape incurs an escape, which `mir::loans` refuses
    // (`a_member_call_cannot_launder_an_escape`).
}
