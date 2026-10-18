//! Safe borrows: regions, permissions, reborrows and outlives.

use super::*;

// ---- safe borrows and regions -------------------------------------------

#[test]
fn borrow_types_carry_their_region() {
    check_infer(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#"
            13..56 'fn::<@a>(r: usize...': fn(usize.&::<@a>) -> usize
            22..23 'r': usize.&::<@a>
            49..56 '{ r.* }': usize
            51..52 'r': usize.&::<@a>
            51..54 'r.*': usize
        "#]],
    );
}

#[test]
fn borrow_without_a_region_is_reported_not_guessed() {
    // Elision is DEFERRED, not absent by oversight — the whole point of
    // launching without it is that the omission is visible.
    check_diagnostics(
        "static f = fn (r: usize.&) -> usize { r.* };",
        expect![[r#"
            24..25: a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet
        "#]],
    );
}

#[test]
fn wildcard_region_is_a_body_answer_not_a_signature_one() {
    // `@_` says "there is a region here, infer it". A body can answer
    // that; a signature cannot, because its regions are parameters, and a
    // parameter needs the binder name that its outlives clauses and its
    // other mentions refer to.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { let s: usize.&::<@_> = r; s.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn (r: usize.&::<@_>) -> usize { r.* };",
        expect![[r#"
            28..30: `@_` cannot be used in a signature — declare the region in the binder (`fn::<@a>`) and name it here
        "#]],
    );
}

#[test]
fn safe_deref_needs_no_unsafe_but_raw_still_does() {
    // Safety is decided by the pointer's FLAVOR — the one place
    // `unsafe_check` consults types.
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn (r: usize.&raw) -> usize { r.* };",
        expect![[r#"
            41..44: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_non_copyable_referent_cannot_be_read_out_of_a_borrow() {
    // M07's copy rule, and the reason safe `.*` needed a ruling: `r.*` as
    // a VALUE duplicates the referent, which `usize.&mut` may not be.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>) \
         -> () { let s = r.*; };",
        expect![[r#"
            92..95: cannot move out of a borrow: `struct { q: usize.&mut::<@a>, x: usize }` cannot be copied
        "#]],
    );
}

#[test]
fn a_deref_in_place_position_projects_instead_of_copying() {
    // The same borrow, PROJECTED: a field read, a write through it, a
    // reborrow of the inner borrow, and a new borrow of a field. None of
    // them copies the affine referent, so the copy rule does not apply —
    // this is the ordinary context-struct shape.
    let ty = "struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>";
    for body in [
        "r.*.x",
        "{ r.*.x = 9; 0 }",
        "r.*.q.*",
        "{ let m = r.*.x.&mut; 0 }",
    ] {
        check_diagnostics(
            &format!("static f = fn::<@a>(r: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
}

#[test]
fn a_borrow_projection_is_judged_at_its_last_step() {
    // Projecting THROUGH a deref defers the copy question; it does not
    // retire it. What the chain finally materializes is copied like any
    // other read, so an exclusive borrow reached through a shared one is
    // refused — two live `.&mut` to one place is exactly what M07 exists
    // to prevent, and the whole-referent read is no longer the only route
    // to it.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> () { let c = r.*.q; };",
        expect![[r#"
            89..94: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(r: [usize.&mut::<@a>; 2].&::<@a>) -> () { let c = r.*[0]; };",
        expect![[r#"
            70..76: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
    // And the steps that are not the last one keep projecting: a copyable
    // field is a copy of THAT, a further step stays a place, and a step
    // handed to a borrow-expecting parameter is a reborrow.
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> usize { r.*.x };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&::<@a>) \
         -> usize { r.*.q.* };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static set_to = fn::<@c>(m: usize.&mut::<@c>, v: usize) -> () { m.* = v; }; \
         static f = fn::<@a>(r: struct { x: usize, q: usize.&mut::<@a> }.&mut::<@a>) \
         -> () { set_to(r.*.q, 5); };",
        expect![[r#""#]],
    );
}

#[test]
fn a_borrow_behind_a_borrow_projects_and_reborrows() {
    // `T.&mut.&mut` is a type the language accepts, so every use of it
    // has to be reachable. The inner `.*` is the outer one's RECEIVER —
    // a place, not a value — and `bb.*` in an argument position is a
    // reborrow (M07), which suspends the parent instead of copying it.
    let ty = "usize.&mut::<@a>.&mut::<@b>";
    for body in ["bb.*.*", "{ bb.*.* = 5; 0 }", "{ let c = bb.*.*.&mut; 0 }"] {
        check_diagnostics(
            &format!("static f = fn::<@a, @b>(bb: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
    check_diagnostics(
        "static set_to = fn::<@c>(r: usize.&mut::<@c>, v: usize) -> () { r.* = v; }; \
         static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> usize \
         { set_to(bb.*, 5); 0 };",
        expect![[r#""#]],
    );
    // The one read that really is a copy still cannot happen: no
    // borrow-typed context caught it, so nothing reborrowed.
    check_diagnostics(
        "static f = fn::<@a, @b>(bb: usize.&mut::<@a>.&mut::<@b>) -> () { let c = bb.*; };",
        expect![[r#"
            73..77: cannot move out of a borrow: `usize.&mut::<@a>` cannot be copied
        "#]],
    );
}

#[test]
fn a_dot_call_receiver_is_a_value_position() {
    // `recv.name(args)` hands the receiver to the MEMBER by value (G13's
    // structural selection), so a `.*` there is a copy like any other —
    // the one field receiver that is not a place.
    check_diagnostics(
        "type Cell = struct::<T> { v: T } with \
         { impl Self { take = fn (c: Self) -> T { let Cell(struct { v }) = c; v }; } }; \
         static f = fn::<@a>(r: Cell::<usize.&mut::<@a>>.&mut::<@a>) -> () \
         { let m = r.*.take(); };",
        expect![[r#"
            193..196: cannot move out of a borrow: `Cell::<usize.&mut::<@a>>` cannot be copied
        "#]],
    );
    // Which is why the question waits for resolution: the SAME syntax on
    // an fn-typed field reads only the field, leaving the receiver a
    // place — as the parenthesized call of that field plainly does.
    let ty = "struct { q: usize.&mut::<@a>, f: fn(usize) -> usize }.&mut::<@a>";
    for body in ["r.*.f(3)", "(r.*.f)(3)"] {
        check_diagnostics(
            &format!("static f = fn::<@a>(r: {ty}) -> usize {{ {body} }};"),
            expect![[r#""#]],
        );
    }
}

#[test]
fn exclusive_borrow_needs_a_mut_root() {
    // Same transitive-root judgement as an assignment or a `.&raw mut`,
    // with the same "declared without `mut` here" note on the declaration.
    check_diagnostics(
        "static f = fn () -> () { let n: usize = 1; let m = n.&mut; };",
        expect![[r#"
            51..52: cannot borrow `n` as `.&mut`: it is not declared `mut` (`n` is declared without `mut` here at 29..30)
        "#]],
    );
    // Through a projection the two names differ: the place is what was
    // borrowed, the root is what the fix goes on, and the message says
    // both rather than blaming the field for the declaration's omission.
    check_diagnostics(
        "static f = fn () -> () { let p: struct { x: usize } = struct { x = 1 }; \
         let m = p.x.&mut; };",
        expect![[r#"
            80..81: cannot borrow `p.x` as `.&mut`: `p` is not declared `mut` (`p` is declared without `mut` here at 29..30)
        "#]],
    );
}

#[test]
fn an_exclusive_borrow_of_an_item_points_at_its_definition() {
    // Neither item is an exclusively-borrowable place, each for its own
    // reason, and both say where the name is defined — the courtesy
    // `AssignToItem` and `AddrOfMutItem` already extend.
    check_diagnostics(
        "static s: usize = 7;\nconst g: usize = 8;\n\
         static f = fn () -> () { let a = s.&mut; let b = g.&mut; };",
        expect![[r#"
            74..75: cannot borrow a `static` as `.&mut`: `static mut` is not supported yet (`s` is defined here at 7..8)
            90..91: cannot borrow a `const` as `.&mut`: a `const` is copied at every mention, so there is no one place to borrow (`g` is defined here at 27..28)
        "#]],
    );
}

#[test]
fn borrow_mut_immutable_offers_the_make_mut_fix() {
    // Same machinery as `addr_of_mut_immutable_offers_the_make_mut_fix`:
    // `.&mut` of a non-`mut` root anchors the insert-`mut` fix at the
    // binding's declaration, not the borrow.
    let text = "static f = fn () -> () { let n: usize = 1; let m = n.&mut; };";
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let diagnostics = crate::file_diagnostics(&db, file);
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let fix = diagnostics[0].fix.as_ref().expect("diagnostic has a fix");
    assert_eq!(fix.label, "Make `n` mutable");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].insert, "mut ");
    // Immediately before the binding's name `n` in `let n` (offset 29):
    // applying it yields `let mut n = 1;`.
    assert_eq!(u32::from(fix.edits[0].range.start()), 29);
}

#[test]
fn shared_parent_cannot_launder_into_an_exclusive_child() {
    check_diagnostics(
        "static f = fn::<@a>(r: usize.&::<@a>) -> () { let m = r.*.&mut; };",
        expect![[r#"
            54..62: cannot borrow `.&mut` through `usize.&::<@a>`: an exclusive borrow needs a `.&mut` parent
        "#]],
    );
}

#[test]
fn a_shared_step_anywhere_in_a_place_refuses_a_write() {
    // The headline rule: a shared borrow never grants a write. It is
    // judged over the WHOLE chain, not the outermost step — a `.&mut`
    // held behind a `.&` is read out by REBORROWING it (M07's affinity),
    // which the place holding it must be allowed to grant.
    let direct = "usize.&::<@a>";
    let nested = "usize.&mut::<@a>.&::<@b>";
    for (ty, body, expected) in [
        (
            direct,
            "{ b.* = 5; 0 }",
            expect![[r#"
                55..58: cannot assign through `usize.&::<@a>`: writing needs a `.&mut` borrow
            "#]],
        ),
        (
            direct,
            "{ let p = b.*.&raw mut; 0 }",
            expect![[r#"
                63..75: cannot take `.&raw mut` through `usize.&::<@a>`: minting a mutating address needs a `.&mut` borrow
            "#]],
        ),
        (
            nested,
            "{ b.*.* = 5; 0 }",
            expect![[r#"
                66..71: cannot assign through `usize.&mut::<@a>.&::<@b>`: writing needs a `.&mut` borrow
            "#]],
        ),
        (
            nested,
            "{ let c = b.*.*.&mut; 0 }",
            expect![[r#"
                74..84: cannot borrow `.&mut` through `usize.&mut::<@a>.&::<@b>`: an exclusive borrow needs a `.&mut` parent
            "#]],
        ),
        (
            nested,
            "{ let p = b.*.*.&raw mut; 0 }",
            expect![[r#"
                74..88: cannot take `.&raw mut` through `usize.&mut::<@a>.&::<@b>`: minting a mutating address needs a `.&mut` borrow
            "#]],
        ),
        (
            "usize.&::<@a>.&mut::<@b>",
            "{ b.*.* = 5; 0 }",
            expect![[r#"
                66..71: cannot assign through `usize.&::<@a>`: writing needs a `.&mut` borrow
            "#]],
        ),
        // An element step is as transparent as a field: a shared borrow
        // of an array cannot launder a write into one of its elements.
        (
            "[usize.&mut::<@a>; 2].&::<@b>",
            "{ b.*[0].* = 5; 0 }",
            expect![[r#"
                71..79: cannot assign through `[usize.&mut::<@a>; 2].&::<@b>`: writing needs a `.&mut` borrow
            "#]],
        ),
    ] {
        check_diagnostics(
            &format!("static f = fn::<@a, @b>(b: {ty}) -> usize {{ {body} }};"),
            expected,
        );
    }
    // The implicit mutable reborrow is the same mint with the `.&mut`
    // left unwritten, so it answers to the same rule.
    check_diagnostics(
        "static set_to = fn::<@c>(r: usize.&mut::<@c>, v: usize) -> () { r.* = v; }; \
         static f = fn::<@a, @b>(b: usize.&mut::<@a>.&::<@b>) -> usize \
         { set_to(b.*, 5); 0 };",
        expect![[r#"
            147..150: cannot borrow `.&mut` through `usize.&mut::<@a>.&::<@b>`: an exclusive borrow needs a `.&mut` parent
        "#]],
    );
}

#[test]
fn a_raw_mut_step_grants_the_permission_it_carries() {
    // The raw world's own laundering, and the reason the walk stops at a
    // `.&raw mut`: reading a raw pointer out of a place COPIES it, and a
    // copy carries the whole permission. `unsafe` is what gates it — a
    // `.&raw` above makes no difference.
    check_diagnostics(
        "static f = fn (pp: usize.&raw mut.&raw) -> () { unsafe { pp.*.* = 5; } };",
        expect![[r#""#]],
    );
    // A `.&mut` in the same position does NOT stop the walk: reading it
    // out is a reborrow, not a copy.
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw) -> () { unsafe { pb.*.* = 5; } };",
        expect![[r#"
            64..70: cannot assign through `usize.&mut::<@a>.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
    // Minting through the same pair, so the advice the diagnostic gives
    // is the advice that works.
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw) -> () \
         { unsafe { let c = pb.*.*.&mut; }; };",
        expect![[r#"
            72..83: cannot borrow `.&mut` through `usize.&mut::<@a>.&raw`: an exclusive borrow needs a `.&raw mut` parent
        "#]],
    );
    check_diagnostics(
        "static f = fn::<@a>(pb: usize.&mut::<@a>.&raw mut) -> () \
         { unsafe { let c = pb.*.*.&mut; }; };",
        expect![[r#""#]],
    );
    // And a non-`mut` binding holding a `.&mut` still writes: the root's
    // own mutability is a different rule, about reassigning the binding.
    check_diagnostics(
        "static f = fn () -> usize { let mut n: usize = 1; let b = n.&mut; b.* = 5; n };",
        expect![[r#""#]],
    );
}

#[test]
fn a_shared_borrow_cannot_be_minted_through_a_raw_pointer() {
    // A raw pointer carries no region — unlike the shared-parent case
    // (`shared_parent_cannot_launder_into_an_exclusive_child`, whose
    // parent HAS a region and is refused only for `.&mut`), no safe
    // borrow at all can be minted through one.
    check_diagnostics(
        "static f = fn (r: usize.&raw) -> () { let m = unsafe { r.*.& }; };",
        expect![[r#"
            55..60: cannot mint a safe borrow through `usize.&raw`: a raw pointer carries no region for the new borrow to be bounded by
        "#]],
    );
}

#[test]
fn an_exclusive_borrow_cannot_be_minted_through_a_raw_pointer() {
    check_diagnostics(
        "static f = fn (r: usize.&raw mut) -> () { let m = unsafe { r.*.&mut }; };",
        expect![[r#"
            59..67: cannot mint a safe borrow through `usize.&raw mut`: a raw pointer carries no region for the new borrow to be bounded by
        "#]],
    );
}

/// An unknown region name is reported where region names are WRITTEN — a
/// TYPE position. The expression-side spelling this used to check
/// (`n.&::<@q>`) is refused before any name is resolved, so the annotation
/// is the only place left that can ask the question at all.
#[test]
fn a_region_name_must_be_declared() {
    check_diagnostics(
        "static f = fn () -> () { let n: usize = 1; let r: usize.&::<@q> = n.&; };",
        expect![[r#"
            60..62: no region named `@q` is in scope; declare it in the binder (`fn::<@q>`)
        "#]],
    );
}

#[test]
fn regions_on_type_declarations_are_reserved() {
    check_diagnostics(
        "type Slice = struct::<@a, T> { n: usize };",
        expect![[r#"
            22..24: region parameters on type declarations are not supported yet
        "#]],
    );
}

#[test]
fn a_misordered_type_declaration_binder_is_told_twice() {
    // The regions-come-first rule is checked on `GenericParamList`, so it
    // reaches a TYPE declaration's binder too, where the region is also
    // still reserved: two errors on one token — move it, and it is not
    // supported yet. Pinned so the reservation's lifting sees the pair.
    check_diagnostics(
        "type Slice = struct::<T, @a> { n: usize };",
        expect![[r#"
            25..27: region parameters come first in a binder; move `@a` before `T`
            25..27: region parameters on type declarations are not supported yet
        "#]],
    );
}

#[test]
fn undeclared_outlives_is_rejected_and_the_clause_fixes_it() {
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };",
        expect![[r#"
            61..62: using this borrow where a longer-lived one is expected needs `@a` to outlive `@b`, which this signature does not declare; add `@a: @b` to the binder
        "#]],
    );
    // With the clause written, the same body is accepted.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn outlives_is_transitive_through_declared_bounds() {
    check_diagnostics(
        "static f = fn::<@a: @b, @b: @c, @c>(x: usize.&::<@a>) -> usize.&::<@c> { x };",
        expect![[r#""#]],
    );
}

#[test]
fn a_bare_mention_infers_its_regions_freely() {
    // No turbofish at all: every region of the callee is a fresh
    // existential of this call, solved from the arguments passed. (A
    // written one is refused — see
    // `a_written_region_argument_is_refused_at_every_mention`.)
    check_diagnostics(
        "static f = fn::<@a>(x: usize.&::<@a>) -> usize.&::<@a> { x };\n\
         static g = fn::<@p>(x: usize.&::<@p>) -> usize.&::<@p> { f(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn a_callees_declared_bound_still_binds_an_elided_call() {
    // `f`'s `@a: @b` travels with the instantiation as an obligation
    // between the two fresh existentials this call mints
    // (`push_region_binder_bounds`), so a caller whose regions cannot
    // satisfy it is refused. With no written region to stand between the
    // caller's binder and the callee's, the blame lands on the argument
    // whose reborrow carries the element across — never on the bound
    // itself, which is why `CalleeBound` has no message of its own to pin.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };\n\
         static g = fn::<@p, @q>(x: usize.&::<@p>) -> usize.&::<@q> { f(x) };",
        expect![[r#"
            133..134: using this borrow where a longer-lived one is expected needs `@p` to outlive `@q`, which this signature does not declare; add `@p: @q` to the binder
        "#]],
    );
    // The same call with the bound declared, and a shape whose fresh
    // existentials can satisfy the bound on their own, are both clean.
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };\n\
         static g = fn::<@p: @q, @q>(x: usize.&::<@p>) -> usize.&::<@q> { f(x) };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static f = fn::<@a: @b, @b>(x: usize.&::<@a>, y: usize.&::<@b>) -> usize { 1 };\n\
         static g = fn::<@p, @q>(x: usize.&::<@p>, y: usize.&::<@q>) -> usize { f(x, y) };",
        expect![[r#""#]],
    );
}

#[test]
fn an_explicit_shared_reborrow_needs_the_parents_region_to_outlive_it() {
    // `x.*.&` is a WRITTEN reborrow, not a use-site coercion — it must
    // incur the same outlives edge the implicit path does. Without it
    // `@b`'s region launders into `@a` with no bound declared.
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@b>) -> usize.&::<@a> { x.*.& };",
        expect![[r#"
            61..66: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
    // Declaring the bound makes it clean.
    check_diagnostics(
        "static f = fn::<@a, @b: @a>(x: usize.&::<@b>) -> usize.&::<@a> { x.*.& };",
        expect![[r#""#]],
    );
}

#[test]
fn an_explicit_mut_reborrow_needs_the_parents_region_to_outlive_it() {
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&mut::<@b>) -> usize.&::<@a> { x.*.&mut };",
        expect![[r#"
            64..72: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn an_explicit_reborrow_with_a_named_region_still_needs_the_bound() {
    // Pinning the target region with an annotation (`_.&::<@a>`) is the
    // same event as leaving it to inference — the region comes from the
    // annotation rather than the return, but the outlives edge it incurs
    // is identical.
    check_diagnostics(
        "static f = fn::<@a, @b>(x: usize.&::<@b>) -> usize.&::<@a> { let r: _.&::<@a> = x.*.&; r };",
        expect![[r#"
            80..85: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn reborrow_at_every_use_lets_a_mut_borrow_be_used_repeatedly() {
    // The affinity correction made real: `m` is passed twice and then read.
    // A language that MOVED on the first use would reject this.
    check_diagnostics(
        "static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = 1; };\n\
         static f = fn::<@a>(m: usize.&mut::<@a>) -> usize { bump(m); bump(m); m.* };",
        expect![[r#""#]],
    );
}

#[test]
fn exclusive_degrades_to_shared_but_never_the_reverse() {
    check_diagnostics(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static f = fn::<@a>(m: usize.&mut::<@a>) -> usize { get(m) };",
        expect![[r#""#]],
    );
    check_diagnostics(
        "static set = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = 1; };\n\
         static f = fn::<@a>(r: usize.&::<@a>) -> () { set(r) };",
        expect![[r#"
            113..114: type mismatch: expected `usize.&mut`, found `usize.&::<@a>`
        "#]],
    );
}

#[test]
fn a_region_join_is_outlived_by_every_member() {
    // `@a + @b` reads as conjunction: an obligation against the join is an
    // obligation against each member, so this needs BOTH clauses.
    check_diagnostics(
        "static f = fn::<@a, @b, @c>(x: usize.&::<@a + @b>) -> usize.&::<@c> { x };",
        expect![[r#"
            70..71: using this borrow where a longer-lived one is expected needs `@a` to outlive `@c`, which this signature does not declare; add `@a: @c` to the binder
            70..71: using this borrow where a longer-lived one is expected needs `@b` to outlive `@c`, which this signature does not declare; add `@b: @c` to the binder
        "#]],
    );
}

#[test]
fn a_join_of_borrows_reborrows_into_its_context() {
    // A join is a use site like any other, so each branch REBORROWS into
    // the join's result — one directed edge per branch, never a mutual
    // relation between the branches.
    //
    // The comment here used to say the opposite: that relating the
    // branches invariantly was "the whole cost of the invariance-first
    // posture". That diagnosis was wrong and it hid a soundness hole for
    // an entire arc. Invariance governs the REFERENT of a borrow, not the
    // region of a borrow being formed — under reborrow-at-every-use each
    // branch mints its own shorter loan, and the join's region is their
    // MEET. Because the framing said the rejection was expected, nobody
    // asked why a join emitted no edge to its context at all.
    //
    // So: two independent universals join cleanly...
    check_diagnostics(
        "static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static pick = fn::<@a, @b>(p: usize.&::<@a>, q: usize.&::<@b>, c: bool) -> usize {\n\
             get(if c { p } else { q })\n\
         };",
        expect![[r#""#]],
    );
    // ...and the obligation a branch owes its CONTEXT still lands.
    check_diagnostics(
        "static f = fn::<@a, @b>(p: usize.&::<@b>, c: bool) -> usize.&::<@a> {\n\
             if c { p } else { p }\n\
         };",
        expect![[r#"
            77..78: using this borrow where a longer-lived one is expected needs `@b` to outlive `@a`, which this signature does not declare; add `@b: @a` to the binder
        "#]],
    );
}

#[test]
fn a_region_is_not_part_of_a_types_identity() {
    // Two `usize.&mut` at DIFFERENT regions join without a TYPE mismatch:
    // unification decides type identity, and a region is not part of one.
    // What the difference costs is an outlives obligation, which the
    // declared clauses here discharge — so the user sees nothing, rather
    // than a message about types disagreeing.
    check_diagnostics(
        "static f = fn::<@a: @b, @b: @a>(x: usize.&mut::<@a>, y: usize.&mut::<@b>)\n\
                        -> usize.&mut::<@a> { if true { x } else { y } };",
        expect![[r#""#]],
    );
}
