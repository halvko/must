//! Generic functions: binders, turbofish and const params.

use super::*;

// ---- generics: item tree, schemes, rigid bodies, instantiation ----

#[test]
fn item_tree_records_generic_binder() {
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = fn::<T, const N: usize>(x: T) -> T { x };".to_owned(),
    );
    let item = crate::file_item_ids(&db, file)[0];
    let data = crate::item_data(&db, item).as_ref().expect("item data");
    let rendered: Vec<String> = data
        .generics
        .iter()
        .map(|param| match &param.kind {
            crate::item_tree::GenericParamKind::Region => format!("region {}", param.name),
            crate::item_tree::GenericParamKind::Type => format!("type {}", param.name),
            crate::item_tree::GenericParamKind::Const(ty) => {
                format!("const {}: {ty:?}", param.name)
            }
        })
        .collect();
    assert_eq!(
        rendered,
        vec!["type T".to_owned(), "const N: Path(\"usize\")".to_owned()],
        "binder kinds and order are recorded"
    );
    assert!(
        data.type_ref.is_some(),
        "a fully annotated binder synthesizes the scheme's TypeRef"
    );
}

/// Same style as the body-edit firewall test above: an edit inside a
/// generic item's body must not re-run its callers' inference — the scheme
/// (from the binder's mandatory annotations) is unchanged, so `item_data`
/// backdates and `signature` never re-fires downstream.
#[test]
fn firewall_generic_body_edit_does_not_reinfer_callers() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "static id = fn::<T>(x: T) -> T { x };\n\
                   static g: fn() -> usize = fn () -> usize { id::<usize>(4) };\n";
    let text_v2 = "static id = fn::<T>(x: T) -> T { let y = x; y };\n\
                   static g: fn() -> usize = fn () -> usize { id::<usize>(4) };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for &item in crate::file_item_ids(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter().filter(|entry| entry.contains("infer")).count(),
        1,
        "only the edited generic item may re-infer; executed: {log:#?}"
    );
}

#[test]
fn generic_fn_fully_annotated_is_clean() {
    check_diagnostics("static id = fn::<T>(x: T) -> T { x };", expect![[r#""#]]);
}

#[test]
fn generic_fn_missing_param_annotation_errors() {
    check_diagnostics(
        "static id = fn::<T>(x) -> T { x };",
        expect![[r#"
            14..19: a generic function must annotate all parameters and its return type
        "#]],
    );
}

#[test]
fn generic_fn_missing_return_type_errors() {
    check_diagnostics(
        "static id = fn::<T>(x: T) { x };",
        expect![[r#"
            14..19: a generic function must annotate all parameters and its return type
        "#]],
    );
}

#[test]
fn rigid_param_passes_stores_returns_and_compares() {
    // Everything an opaque value supports, with NO bound written: pass
    // through a `let` (with a `T` annotation resolving to the rigid
    // param), store it in a record field, take it back out by
    // destructuring, return it through a join, and compare two of them
    // with `==`.
    //
    // Every one of those moves a value exactly once, which is why none of
    // them needs a bound. The shape this replaces read one `x` four times
    // and leaned on the default bound to allow it — which a bound does not
    // do: it waives the obligation to CONSUME, never the ban on
    // DUPLICATING (see `the_forget_bound_is_not_permission_to_copy`).
    check_diagnostics(
        "static f = fn::<T>(x: T, c: bool) -> T { let y: T = x; let r = struct { v = y }; let struct { v } = r; if c { v } else { v } };\nstatic eq = fn::<T>(a: T, b: T) -> bool { a == b };",
        expect![[r#""#]],
    );
}

#[test]
fn rigid_param_field_access_call_and_arithmetic_error_ordinarily() {
    // One refusal per body, and none of the three needs a bound: each
    // reads its `x` exactly once. (The one-body version this replaces read
    // it four times, which no capability bound licenses — copying is a
    // question the language has not answered.)
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { let a = x.field; x };\nstatic g = fn::<T>(x: T) -> usize { x() };\nstatic h = fn::<T>(x: T, y: T) -> T { let n = x + 1; y };",
        expect![[r#"
            42..47: no field `field` on `T`
            90..91: expression of type `T` is not callable
            143..144: type mismatch: expected `{number}`, found `T` (`+` requires `{number}` operands at 145..146)
        "#]],
    );
}

#[test]
fn const_param_reads_as_a_value_of_its_declared_type() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N + 1 };",
        expect![[r#""#]],
    );
}

#[test]
fn const_param_misused_errors_ordinarily() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N() };",
        expect![[r#"
            45..46: expression of type `usize` is not callable
        "#]],
    );
}

#[test]
fn const_param_cannot_be_assigned() {
    check_diagnostics(
        "static f = fn::<const N: usize>() -> usize { N = 3; N };",
        expect![[r#"
            45..46: cannot assign to `N`: it is a const parameter
        "#]],
    );
}

#[test]
fn dependent_const_param_type_is_rejected() {
    // TR06: no dependent params — a const param's type cannot name a type
    // param of the same binder.
    check_diagnostics(
        "static f = fn::<T, const N: T>(x: T) -> T { x };",
        expect![[r#"
            28..29: a const parameter's type cannot mention a type parameter
        "#]],
    );
}

#[test]
fn const_param_hole_type_is_rejected() {
    check_diagnostics(
        "static f = fn::<const N: _>() -> usize { N };",
        expect![[r#"
            25..26: a const parameter's type must be a fully written type; a declaration has nothing to infer `_` from
        "#]],
    );
}

#[test]
fn variant_path_on_type_param_errors() {
    check_diagnostics(
        "static f = fn::<T: forget>(x: T) -> usize { let y: T::Bad = 1; 1 };",
        expect![[r#"
            51..57: `T` has no variants (it is a type parameter)
        "#]],
    );
}

#[test]
fn nested_generic_binder_is_rejected() {
    // TR06: generics are item-level; a binder on a nested literal parses but
    // is rejected (generic closures wait for the capture story). The
    // binder's own error also covers the nested `T` mentions, which lower
    // to a silent `{error}`.
    check_diagnostics(
        "static f = fn () -> usize { let id = fn::<T>(x: T) -> T { x }; 1 };",
        expect![[r#"
            39..44: generic function literals are only supported as item initializers
        "#]],
    );
}

#[test]
fn generic_item_never_joins_an_inference_group() {
    // TR06, load-bearing: a group's shared signature variable is a monotype,
    // so membership would pin the scheme to one instantiation.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static id = fn::<T>(x: T) -> T { x };\nstatic caller = fn () { print(id(\"hi\")) };"
            .to_owned(),
    );
    let library = crate::library::library_of(&db, file);
    let groups = crate::groups::inference_groups(&db, library);
    let items = crate::scopes::library_items(&db, library);
    assert_eq!(
        groups.slot.get(&items[0].0),
        None,
        "the generic item joins no group"
    );
    assert!(
        groups.slot.contains_key(&items[1].0),
        "the unannotated caller still gets a group of its own"
    );
    // The other direction: the caller sees only the scheme, and the call
    // type-checks against a fresh instantiation (T = str) — no diagnostics.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic caller = fn () { print(id(\"hi\")) };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_instantiates_the_scheme() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(4) };",
        expect![[r#""#]],
    );
}

#[test]
fn type_params_infer_from_value_arguments() {
    // A generic item with ONLY type params may be mentioned bare: ordinary
    // unification fills the params from the value arguments.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> str { id(\"hi\") };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_pins_the_param_against_the_value_argument() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(\"hi\") };",
        expect![[r#"
            78..82: type mismatch: expected `usize`, found `str` (because `T` was instantiated to `usize` by this argument at 71..76)
        "#]],
    );
}

#[test]
fn a_region_bearing_binders_blame_names_the_binder_index() {
    // `Cause::GenericArg` records the BINDER index (`param`) beside the
    // WRITTEN position (`index`), and this is the shape where the two
    // differ: `U` is binder slot 1 but written slot 0, because `@a` is
    // never written. The blame must name `U` — not `@a` — and point at the
    // written `usize`.
    check_diagnostics(
        "static f = fn::<@a, U>(x: U, p: usize.&::<@a>) -> U { x };\n\
         static g = fn::<@b>(q: usize.&::<@b>) -> usize { f::<usize>(\"hi\", q) };",
        expect![[r#"
            119..123: type mismatch: expected `usize`, found `str` (because `U` was instantiated to `usize` by this argument at 112..117)
        "#]],
    );
}

#[test]
fn turbofish_arity_mismatch() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize, usize>(4) };",
        expect![[r#"
            66..84: `id` takes 1 generic argument, found 2 (declared here at 7..9)
        "#]],
    );
}

#[test]
fn turbofish_hole_leaves_a_type_param_to_inference() {
    check_diagnostics(
        "static pick = fn::<A, B: forget>(a: A, b: B) -> A { a };\nstatic g = fn () -> usize { pick::<_, str>(4, \"x\") };",
        expect![[r#""#]],
    );
}

#[test]
fn const_arg_hole_is_an_error() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, _>(4) };",
        expect![[r#"
            83..98: const arguments cannot be inferred
        "#]],
    );
}

#[test]
fn turbofish_on_a_non_generic_item_errors() {
    check_diagnostics(
        "static f = fn (x: usize) -> usize { x };\nstatic g = fn () -> usize { f::<usize>(1) };",
        expect![[r#"
            69..79: `f` takes no generic arguments
        "#]],
    );
}

#[test]
fn bare_mention_with_const_params_requires_a_turbofish() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep(4) };",
        expect![[r#"
            83..86: const arguments must be written explicitly; write `rep::<...>` (declared here at 7..10)
        "#]],
    );
}

#[test]
fn const_arg_type_checks_against_the_declared_type() {
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, \"x\">(4) };",
        expect![[r#"
            96..99: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn generic_arg_kind_mismatches() {
    // A value where a type param is declared...
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<42>(4) };",
        expect![[r#"
            66..74: `T` is a type parameter; write a type
            71..73: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
    // ...and a type where a const param is declared.
    check_diagnostics(
        "static rep = fn::<T, const N: usize>(x: T) -> T { x };\nstatic g = fn () -> usize { rep::<usize, str>(4) };",
        expect![[r#"
            83..100: `N` is a const parameter; write a value (a literal, or `const <expr>`)
        "#]],
    );
}

#[test]
fn unresolved_type_param_at_a_mention_is_reported() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () { id; };",
        expect![[r#"
            57..59: cannot infer the type parameter `T` of `id`; write `id::<...>` to specify it (defined here at 7..9)
        "#]],
    );
}

#[test]
fn turbofish_in_type_position_takes_no_generic_arguments() {
    check_diagnostics(
        "type Foo = struct { x: usize };\nstatic f: Foo::<usize> = Foo(struct { x = 1 });",
        expect![[r#"
            42..54: `Foo` takes no generic arguments (declared here at 5..8)
        "#]],
    );
}

#[test]
fn bare_angle_generic_mention_gets_only_the_syntax_correction() {
    // `Boxed<usize>` parses into the same `GENERIC_ARG_LIST` `Boxed::<usize>`
    // does (G06/X03 — see `syntax::validation`), so hir judges the arguments
    // it can see: the arity matches, and the ONE diagnostic is the syntax
    // correction. The bare-mention arity mirror, which would fire on a
    // `Boxed` with no list at all, never sees this.
    check_diagnostics(
        "type Boxed = struct::<T> { value: T };\n\
         static f = fn (b: Boxed::<usize>) -> usize { b.value };\n\
         static g = fn (b: Boxed<usize>) -> usize { b.value };",
        expect![[r#"
            113..125: generic arguments use the turbofish: write `Boxed::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_generic_call_gets_only_the_syntax_correction() {
    // Expression position, the case the heuristic exists for: the call's
    // arguments are really in the tree, so inference reads them and the
    // ONE diagnostic is the syntax correction.
    check_diagnostics(
        "static f = fn::<T>(x: T) -> T { x };\n\
         static u = fn () -> usize { f<usize>(3) };",
        expect![[r#"
            65..73: generic arguments use the turbofish: write `f::<...>`
        "#]],
    );
}

#[test]
fn bare_angle_ordering_chain_is_read_as_a_call() {
    // The heuristic's residual measured where it is paid: `a < b > (d)` is
    // token-for-token `f::<T>(d)`, is read as the call, and the misread
    // costs the whole expression rather than one message. No well-typed
    // program is spelled that way — `a < b` is a `bool` and `>` wants
    // numbers — so nothing that compiles is traded for it. A comma makes
    // the shape a working program again, and then the heuristic declines
    // it (see `syntax`'s `bare_angle_top_level_comma_keeps_the_comparison_reading`).
    check_diagnostics(
        "static x = fn (a: usize, b: usize, d: usize) -> bool { a < b > (d) };",
        expect![[r#"
            55..62: generic arguments use the turbofish: write `a::<...>`
            55..62: `a` takes no generic arguments
            55..62: expression of type `usize` is not callable
            59..60: unknown type `b`
        "#]],
    );
}

#[test]
fn bare_angle_on_a_non_generic_type_also_reports_the_arity() {
    // The honest other half: a non-generic target gets the correction AND
    // the judgement the correctly-spelled `Boxed::<usize>` gets, because the
    // arguments are really there in the tree. That is why the worked example
    // in `examples/errors.must` uses a GENERIC target.
    check_diagnostics(
        "type Boxed = struct { value: usize };\n\
         static f = fn (b: Boxed<usize>) -> usize { b.value };",
        expect![[r#"
            56..68: generic arguments use the turbofish: write `Boxed::<...>`
            56..68: `Boxed` takes no generic arguments (declared here at 5..10)
        "#]],
    );
}

#[test]
fn generic_const_fn_body_is_a_const_context() {
    check_diagnostics(
        "static f = const fn::<const N: usize>() -> usize { g() };\nstatic g = fn () -> usize { 1 };",
        expect![[r#"
            51..52: cannot call `g` in a const context; marking it `const fn` would allow this (`g` is defined here at 65..66) (this `const fn` is always a const context at 11..16)
        "#]],
    );
}

#[test]
fn instantiated_generic_const_fn_call_is_const_legal() {
    // The const-check diagnostic must NOT fire: the rule keys off the
    // `const fn` marker exactly as for a plain mention. (The
    // initializer also genuinely EVALUATES — pinned on the eval side.)
    check_diagnostics(
        "static cid = const fn::<T>(x: T) -> T { x };\nstatic v: usize = cid::<usize>(4);",
        expect![[r#""#]],
    );
}

#[test]
fn instantiated_generic_plain_fn_call_in_a_const_context_is_rejected() {
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic v: usize = id::<usize>(4);",
        expect![[r#"
            56..67: cannot call `id` in a const context; marking it `const fn` would allow this (`id` is defined here at 7..9) (this item's initializer is a const context at 38..44)
        "#]],
    );
}

/// Type-tier interplay: an expectation against a rigid-param annotation is
/// a REAL expectation — `Ty::Param` is not an inference variable, so the
/// finish pass must keep it (`contains_infer` is false for params).
#[test]
fn expectation_recorded_against_rigid_param_annotation() {
    check_expectations(
        "static id = fn::<T>(x: T) -> T { let y: T = x; y };",
        expect![[r#"
            12..50 'fn::<T>(x: T) -> ...': fn(T) -> T
            31..50 '{ let y: T = x; y }': T
            44..45 'x': T
            47..48 'y': T
        "#]],
    );
}

#[test]
fn generic_item_may_recurse_through_a_fresh_instantiation() {
    // The mention in its own body instantiates the scheme like any other
    // (no group membership, no cycle: the scheme never consults the body).
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { id(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn turbofish_may_name_the_enclosing_binder_param() {
    // Inside a generic body, a turbofish type argument may be the body's
    // own rigid param: `id::<U>` pins the callee's `T` to rigid `U`.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic wrap = fn::<U>(x: U) -> U { id::<U>(x) };",
        expect![[r#""#]],
    );
}

#[test]
fn divergence_widens_into_a_rigid_param_return() {
    // `!` widens to everything, a rigid param included (divergence produces
    // no value to convert); no VALUE type widens to or from a param.
    check_diagnostics(
        "static f = fn::<T: forget>(x: T) -> T { panic(\"unimplemented\") };",
        expect![[r#""#]],
    );
}

#[test]
fn a_binder_names_each_type_param_once() {
    // A rigid `Ty::Param` is positional, so a repeated name leaves the
    // first parameter unnameable; the second occurrence is the error.
    check_diagnostics(
        "static id = fn::<T, T>(x: T) -> T { x };",
        expect![[r#"
            20..21: duplicate generic parameter `T` (first declared here at 17..18)
        "#]],
    );
}

#[test]
fn a_binder_names_each_const_param_once() {
    // Same rule for const params, whose mentions resolve to the LAST
    // declaration of the name.
    check_diagnostics(
        "static sq = fn::<const N: usize, const N: usize>() -> usize { N };",
        expect![[r#"
            39..40: duplicate generic parameter `N` (first declared here at 23..24)
        "#]],
    );
}

#[test]
fn type_and_const_params_share_the_binders_namespace() {
    check_diagnostics(
        "static f = fn::<T: forget, const T: usize>(x: T) -> usize { T };",
        expect![[r#"
            33..34: duplicate generic parameter `T` (first declared here at 16..17)
        "#]],
    );
}

#[test]
fn a_type_declarations_binder_names_each_param_once() {
    check_diagnostics(
        "type Pair = struct::<T, T> { a: T, b: T };",
        expect![[r#"
            24..25: duplicate generic parameter `T` (first declared here at 21..22)
        "#]],
    );
}

#[test]
fn a_binder_names_each_region_once() {
    // Same rule for region params; their `@` sigil keeps them from
    // colliding with type and const parameter names.
    check_diagnostics(
        "static get = fn::<@a, @a>(r: usize.&::<@a>) -> usize { r.* };",
        expect![[r#"
            22..24: duplicate generic parameter `@a` (first declared here at 18..20)
        "#]],
    );
}

#[test]
fn the_wildcard_region_is_not_a_binder_name() {
    // `@_` asks for a region to be inferred; a binder is where regions are
    // declared, so it names nothing there — and a second one is the same
    // mistake again, not a duplicate of the first.
    check_diagnostics(
        "static f = fn::<@_>(x: usize) -> usize { x };",
        expect![[r#"
            16..18: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
        "#]],
    );
    check_diagnostics(
        "static g = fn::<@_, @_>(x: usize) -> usize { x };",
        expect![[r#"
            16..18: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
            20..22: `@_` is not a region name; a binder declares regions by name (`fn::<@a>`)
        "#]],
    );
}

#[test]
fn a_region_in_a_type_slot_reads_the_same_in_both_positions() {
    // A type declaration binds no region, so `@a` in its own list is a
    // wrong-kind argument. Annotation and expression are one mistake: one
    // sentence, and the same range — the written argument.
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T };
static q: Pair::<@a> = Pair::<str>(struct { a = "l", b = "r" });
"#,
        expect![[r#"
            58..60: `T` is not a region parameter; a region argument (`@a`) does not belong here
        "#]],
    );
    check_diagnostics(
        r#"
type Pair = struct::<T> { a: T, b: T };
static q = Pair::<@a>(struct { a = "l", b = "r" });
"#,
        expect![[r#"
            59..61: `T` is not a region parameter; a region argument (`@a`) does not belong here
        "#]],
    );
}

#[test]
fn two_items_may_each_bind_the_same_param_name() {
    // The rule is per BINDER, not per file: separate binders are separate
    // namespaces.
    check_diagnostics(
        "static id = fn::<T>(x: T) -> T { x };\nstatic other = fn::<T>(x: T) -> T { x };",
        expect![[r#""#]],
    );
}

// ---- generics: the const-arg domain excludes fn values ----

#[test]
fn fn_typed_const_param_is_rejected_at_the_declaration() {
    // TR06 (concrete data types only): fn values carry edit-unstable
    // `BodyId` identity, so they are outside the const-arg domain —
    // rejected where the domain is declared.
    check_diagnostics(
        "static f = fn::<const N: fn() -> usize>() -> usize { 1 };",
        expect![[r#"
            25..38: a function value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn fn_typed_const_param_smuggled_in_a_record_is_rejected_too() {
    check_diagnostics(
        "static f = fn::<const N: { g: fn() -> usize }>() -> usize { 1 };",
        expect![[r#"
            25..26: expected a type
            27..28: expected `,` or `>`
            30..43: only a trait name can be a bound
            44..45: expected `,` or `>`
        "#]],
    );
}

#[test]
fn fn_typed_const_arg_is_rejected_at_the_mention() {
    // The belt: a mention that would pass an fn value repeats the
    // declaration's exact text at the mention — and the argument is never
    // recorded, so no fn value can reach instance identity.
    check_diagnostics(
        "static f = fn::<const N: fn() -> usize>() -> usize { 1 };\nstatic g = fn () -> usize { let h = fn () -> usize { 2 }; f::<const h>() };",
        expect![[r#"
            25..38: a function value cannot be a const argument (yet)
            116..128: a function value cannot be a const argument (yet)
        "#]],
    );
}

#[test]
fn const_param_forwards_as_a_const_arg() {
    // The composition case: inside a generic body, the binder's own
    // const param is a legal const ARGUMENT — `rep::<const N>` records
    // and type-checks like any const-arg expression.
    check_diagnostics(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };",
        expect![[r#""#]],
    );
}
