//! `unsafe fn` types: coercion, calls and safety through joins.

use super::*;

#[test]
fn an_unsafe_fn_annotation_is_a_type_of_its_own() {
    check_infer(
        "static run = fn(g: unsafe fn(usize) -> usize, x: usize) -> usize { unsafe { g(x) } };",
        expect![[r#"
            13..84 'fn(g: unsafe fn(u...': fn(unsafe fn(usize) -> usize, usize) -> usize
            16..17 'g': unsafe fn(usize) -> usize
            46..47 'x': usize
            65..84 '{ unsafe { g(x) } }': usize
            67..82 'unsafe { g(x) }': usize
            74..82 '{ g(x) }': usize
            76..77 'g': unsafe fn(usize) -> usize
            76..80 'g(x)': usize
            78..79 'x': usize
        "#]],
    );
}

#[test]
fn a_safe_fn_coerces_to_an_unsafe_fn() {
    // The one directed conversion: a function that needs no vouching is
    // welcome in a position willing to vouch. Nothing is converted at run
    // time — the position simply declines to use a permission it has.
    check_unsafe_fn(
        "static f = fn() -> () { let g: unsafe fn(u8.&raw mut, usize) -> isize = safe_read; };",
        expect![[r#""#]],
    );
}

#[test]
fn an_unsafe_fn_does_not_coerce_to_a_safe_fn() {
    // The other direction is the unsound one, and the two rendered types
    // differ by the one token that is the whole story.
    check_unsafe_fn(
        "static f = fn() -> () { let g: fn(u8.&raw mut, usize) -> isize = read; };",
        expect![[r#"
            200..204: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize` (expected `fn(u8.&raw mut, usize) -> isize` because of this annotation at 166..197)
        "#]],
    );
}

#[test]
fn calling_a_let_bound_unsafe_fn_needs_the_marker() {
    check_unsafe_fn(
        "static f = fn(p: u8.&raw mut) -> isize { let g = read; g(p, 1) };\n\
         static ok = fn(p: u8.&raw mut) -> isize { let g = read; unsafe { g(p, 1) } };",
        expect![[r#"
            190..197: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn calling_an_unsafe_fn_parameter_needs_the_marker() {
    // The parameter never saw a declaration: its TYPE is the whole reason
    // the marker is owed, which is the point of moving unsafety here.
    check_unsafe_fn(
        "static apply = fn(g: unsafe fn(u8.&raw mut, usize) -> isize, p: u8.&raw mut) -> isize { g(p, 1) };\n\
         static ok = fn(g: unsafe fn(u8.&raw mut, usize) -> isize, p: u8.&raw mut) -> isize { unsafe { g(p, 1) } };",
        expect![[r#"
            223..230: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn calling_an_unsafe_fn_returned_from_a_function_needs_the_marker() {
    check_unsafe_fn(
        "static pick = fn() -> unsafe fn(u8.&raw mut, usize) -> isize { read };\n\
         static f = fn(p: u8.&raw mut) -> isize { pick()(p, 1) };",
        expect![[r#"
            247..259: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn calling_an_unsafe_fn_out_of_a_record_field_needs_the_marker() {
    check_unsafe_fn(
        "static f = fn(p: u8.&raw mut) -> isize { \
             let h = struct { go = read }; h.go(p, 1) \
         };",
        expect![[r#"
            206..216: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn passing_an_unsafe_fn_to_a_higher_order_function_carries_the_price() {
    // THE HOLE the declaration-side rule could not see: `apply(read, ...)`
    // hands the `extern` item to a body that never mentions it. The body's own
    // parameter type is now what demands the marker — so the price arrives
    // with the value instead of being left behind at the mention.
    check_unsafe_fn(
        "static apply = fn(g: unsafe fn(u8.&raw mut, usize) -> isize, p: u8.&raw mut) -> isize { \
             unsafe { g(p, 1) } \
         };\n\
         static f = fn(p: u8.&raw mut) -> isize { apply(read, p) };",
        expect![[r#""#]],
    );
}

#[test]
fn a_fn_literal_takes_its_parameter_types_from_an_unsafe_fn_position() {
    // The literal's expectation is a `Ty::Fn` whatever its safety, so the
    // unwritten parameter and return types are still inherited from the
    // position — and the finished (safe) literal then coerces into it.
    check_unsafe_fn(
        "static f = fn() -> () { let g: unsafe fn(usize) -> usize = fn(x) { x + 1 }; };",
        expect![[r#""#]],
    );
}

#[test]
fn the_conversion_is_shallow_but_a_literal_is_checked_per_part() {
    // Shallow means the conversion never reaches INSIDE a formed value or
    // through a fn's own return type — both refuse, and both name the whole
    // type. A record LITERAL is the case worth stating separately, because
    // it looks like an exception and is not one: its fields are checked one
    // by one against the annotation, so each field converts on its own and
    // no record ever converted at all.
    check_unsafe_fn(
        "static built = fn() -> () { \
             let r: struct { go: unsafe fn(u8.&raw mut, usize) -> isize } = \
                 struct { go = safe_read }; \
         };\n\
         static formed = fn(v: struct { go: fn(u8.&raw mut, usize) -> isize }) -> () { \
             let r: struct { go: unsafe fn(u8.&raw mut, usize) -> isize } = v; \
         };\n\
         static returned = fn(g: fn() -> fn(usize) -> usize) -> () { \
             let h: fn() -> unsafe fn(usize) -> usize = g; \
         };",
        expect![[r#"
            397..398: type mismatch: expected `struct { go: unsafe fn(u8.&raw mut, usize) -> isize }`, found `struct { go: fn(u8.&raw mut, usize) -> isize }` (expected `struct { go: unsafe fn(u8.&raw mut, usize) -> isize }` because of this annotation at 341..394)
            506..507: type mismatch: expected `fn() -> unsafe fn(usize) -> usize`, found `fn() -> fn(usize) -> usize` (expected `fn() -> unsafe fn(usize) -> usize` because of this annotation at 470..503)
        "#]],
    );
}

#[test]
fn a_safe_fn_parameter_refuses_an_unsafe_fn() {
    // The other half of the hole, and the reason the coercion is one-way:
    // a body that promised to call its argument WITHOUT a marker may not be
    // handed something that owes one. The refusal lands at the argument.
    check_unsafe_fn(
        "static apply = fn(g: fn(u8.&raw mut, usize) -> isize, p: u8.&raw mut) -> isize { \
             g(p, 1) \
         };\n\
         static f = fn(p: u8.&raw mut) -> isize { apply(read, p) };",
        expect![[r#"
            274..278: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize`
        "#]],
    );
}

#[test]
fn a_safe_fn_argument_coerces_at_an_unsafe_fn_parameter() {
    // The coercion at an ARGUMENT, not just an annotation: `apply` promises
    // to vouch, and a function that needs no vouching still fits.
    check_unsafe_fn(
        "static apply = fn(g: unsafe fn(u8.&raw mut, usize) -> isize, p: u8.&raw mut) -> isize { \
             unsafe { g(p, 1) } \
         };\n\
         static f = fn(p: u8.&raw mut) -> isize { apply(safe_read, p) };",
        expect![[r#""#]],
    );
}

#[test]
fn a_generic_instantiated_at_an_unsafe_fn_keeps_the_price() {
    // Through a BINDER: `T` is instantiated at `unsafe fn(...)`, the value
    // makes a round trip through a generic body that knows nothing about
    // it, and the call on the other side still owes the marker.
    check_unsafe_fn(
        "static id = fn::<T>(t: T) -> T { t };\n\
         static f = fn(p: u8.&raw mut) -> isize { id(read)(p, 1) };\n\
         static ok = fn(p: u8.&raw mut) -> isize { unsafe { id(read)(p, 1) } };",
        expect![[r#"
            214..228: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn an_unsafe_fn_is_writable_as_an_explicit_generic_argument() {
    // The type is spellable everywhere a type is: a turbofish argument
    // reaches the same `type_core`, so `T` can be pinned at an `unsafe fn`
    // by hand and the instantiated signature keeps the marker.
    check_unsafe_fn(
        "static id = fn::<T>(t: T) -> T { t };\n\
         static f = fn(p: u8.&raw mut) -> isize { \
             unsafe { id::<unsafe fn(u8.&raw mut, usize) -> isize>(read)(p, 1) } \
         };",
        expect![[r#""#]],
    );
}

#[test]
fn an_unsafe_builtin_taken_as_a_value_carries_its_price() {
    // The documented pre-existing hole, closed by the same rule: taking
    // `dealloc_array` as a value used to launder the marker away entirely.
    // Its TYPE is an `unsafe fn` now, so the call answers for it.
    check_diagnostics(
        "static f = fn(p: usize.&raw mut) -> () { let d = dealloc_array::<usize>; d(p, 1) };\n\
         static ok = fn(p: usize.&raw mut) -> () { \
             let d = dealloc_array::<usize>; unsafe { d(p, 1) } \
         };",
        expect![[r#"
            73..80: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_safe_builtin_taken_as_a_value_keeps_its_safe_type() {
    // The flag comes from `Builtin::requires_unsafe` and nowhere else, so
    // a builtin with nothing to vouch for stays exactly as first-class and
    // exactly as callable as it was.
    check_infer(
        "static f = fn() -> () { let p = print; p(\"hi\") };",
        expect![[r#"
            11..48 'fn() -> () { let ...': fn()
            22..48 '{ let p = print; ...': ()
            28..29 'p': fn(str)
            32..37 'print': fn(str)
            39..40 'p': fn(str)
            39..46 'p("hi")': ()
            41..45 '"hi"': str
        "#]],
    );
}

#[test]
fn an_item_level_unsafe_fn_annotation_lowers_the_same() {
    // A different lowering path from a `let`'s (`lower_decl_ty`, outside
    // every binder), so it is checked separately: the marker must survive
    // there too, and the coercion must still apply.
    check_unsafe_fn(
        "static vouching: unsafe fn(u8.&raw mut, usize) -> isize = safe_read;\n\
         static f = fn(p: u8.&raw mut) -> isize { vouching(p, 1) };",
        expect![[r#"
            245..259: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn an_unsafe_fn_value_is_forgettable_like_any_signature() {
    // Indirection does not infect: an `unsafe fn` is a signature, not an
    // inhabitant, so it has `forget` like every other fn type and the
    // capability rules never look at the flag — bare, and inside a record
    // that is itself dropped on the floor.
    check_unsafe_fn(
        "static f = fn() -> () { \
             let g: unsafe fn(u8.&raw mut, usize) -> isize = read; \
             let held = struct { go = g }; \
         };",
        expect![[r#""#]],
    );
}

#[test]
fn a_join_of_unsafe_fns_is_an_unsafe_fn_and_its_call_is_gated() {
    // THE MINT-FROM-UNKNOWN. The call runs before the join solves, so it is
    // the call that has to commit `g`'s variable to a function shape — and
    // a call has no business naming a safety. Guessing safe here refused a
    // value every branch produced, blaming `fn(...)`, a shape nothing in
    // the source asked for. The safety stays open, and the join decides it.
    check_unsafe_fn(
        r#"static gated = fn(c: bool, p: u8.&raw mut, n: usize) -> isize {
    let g = if c { read } else { read }; g(p, n)
};
static ok = fn(c: bool, p: u8.&raw mut, n: usize) -> isize {
    let g = if c { read } else { read }; unsafe { g(p, n) }
};"#,
        expect![[r#"
            240..247: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_join_of_safe_functions_stays_safe_when_called() {
    // The other half of the same mint: nothing here produces an obligation,
    // so nothing may invent one. (Guessing UNSAFE instead of SAFE would
    // have made this the bug, in the direction that demands markers nobody
    // owes.)
    check_unsafe_fn(
        r#"static other = fn(buf: u8.&raw mut, len: usize) -> isize { 1 };
static f = fn(c: bool, p: u8.&raw mut, n: usize) -> isize {
    let g = if c { safe_read } else { other }; g(p, n)
};"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_match_join_and_a_loop_break_join_decide_safety_the_same_way() {
    // The sink is shared machinery: `match` arms and `break` values decide
    // the call's open safety by the same path.
    check_unsafe_fn(
        r#"static by_match = fn(k: char, p: u8.&raw mut, len: usize) -> isize {
    let g = match k { 'a' => read, _ => read }; g(p, len)
};
static by_break = fn(c: bool, p: u8.&raw mut, len: usize) -> isize {
    let g = loop { if c { break read; }; break read; }; g(p, len)
};"#,
        expect![[r#"
            252..261: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
            390..399: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn an_unsafe_fn_position_does_not_make_its_value_unsafe_in_either_order() {
    // Passing a value where an `unsafe fn` is expected says it MAY be unsafe
    // to call, not that it is: a safe one fits too. Nothing here decides
    // `f`'s safety, so it is a safe `fn()` and the call needs no block,
    // whichever comes first.
    check_unsafe_fn(
        r#"static before = fn(f) -> () { let g: unsafe fn() = f; f(); };
static after = fn(f) -> () { f(); let g: unsafe fn() = f; };"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_safe_alias_in_a_call_cycle_stays_callable_after_an_unsafe_fn_use() {
    // #168. `q` is an unannotated alias inside the cycle `p → q → r → p`,
    // so `p` meets it before its type is known. Meeting the `unsafe fn()`
    // parameter must leave its safety open for the group to decide (safe,
    // from `r`), or `q()` reads as a call to an `unsafe fn`.
    check_unsafe_fn(
        r#"static vouch = fn(f: unsafe fn()) -> () { };
static r = fn() -> () { p(); };
static q = r;
static p = fn() -> () { vouch(q); q(); };
static r2 = fn() -> () { p2(); };
static q2 = r2;
static p2 = fn() -> () { let f: unsafe fn() = q2; q2(); };"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_hole_parameter_called_outside_unsafe_refuses_an_unsafe_argument() {
    // `p` calls `f` outside `unsafe { }`, so `p`'s own body decides `f` is
    // safe; a caller checked later must not reopen that, or `p` would call
    // `read` with no block. In `vouched` the call is inside one, so the
    // caller is free to pass an `unsafe fn`.
    check_unsafe_fn(
        r#"static p = fn(f, b: u8.&raw mut) -> () { let n: usize = 1; let x: isize = f(b, n); };
static q = fn(b: u8.&raw mut) -> () { p(read, b); };
static vouched = fn(f, b: u8.&raw mut) -> () {
    let n: usize = 1;
    let x: isize = unsafe { f(b, n) };
};
static r = fn(b: u8.&raw mut) -> () { vouched(read, b); };"#,
        expect![[r#"
            261..265: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize`
        "#]],
    );
}

#[test]
fn a_caller_checked_first_cannot_make_a_called_parameter_unsafe() {
    // The same as above with the caller first in item order: callees are
    // checked first, so `p`'s own verdict that `f` is called as safe wins and
    // `read` is refused.
    check_unsafe_fn(
        r#"
static q = fn(b) -> () { p(read, b); };
static p = fn(f, b: u8.&raw mut) -> () { let n: usize = 1; let x: isize = f(b, n); };"#,
        expect![[r#"
            163..167: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize`
        "#]],
    );
}

#[test]
fn a_returned_alias_of_a_parameter_keeps_its_safety_across_bodies() {
    // `g` and the result take `f`'s safety through edges that end with
    // `p`'s body, so they become equations there: when `q` passes `read`,
    // the function `p` returns is unsafe too, and calling it needs a block.
    check_unsafe_fn(
        r#"static p = fn(f, b: u8.&raw mut) {
    let n: usize = 1;
    let x: isize = unsafe { f(b, n) };
    let g = f;
    g
};
static q = fn(b: u8.&raw mut) -> () {
    let n: usize = 1;
    let k = p(read, b);
    let y: isize = k(b, n);
};"#,
        expect![[r#"
            358..365: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_hole_parameter_only_passed_on_takes_an_unsafe_argument() {
    // Nothing in `stash` calls `f`, so its safety stays open for its
    // callers to fill.
    check_unsafe_fn(
        r#"static stash = fn(f) -> () { let g: unsafe fn(u8.&raw mut, usize) -> isize = f; };
static use_it = fn() -> () { stash(read) };"#,
        expect![[r#""#]],
    );
}

#[test]
fn unsafety_does_not_flow_back_into_a_value_through_a_binding() {
    // `m` gets `f`'s shape with a safety of its own, so assigning `read` to
    // it makes `m` unsafe and leaves `f` safe: the call on `f` needs no
    // block in either order.
    check_unsafe_fn(
        r#"static before = fn(f, b: u8.&raw mut) -> () {
    let n: usize = 1;
    let x: isize = f(b, n);
    let mut m = f;
    m = read;
};
static after = fn(f, b: u8.&raw mut) -> () {
    let n: usize = 1;
    let mut m = read;
    m = f;
    let x: isize = f(b, n);
};"#,
        expect![[r#""#]],
    );
}

#[test]
fn an_open_safety_decided_unsafe_is_refused_by_a_safe_parameter() {
    // The other direction: `g` is open until the join makes it unsafe, and
    // then a safe `fn` parameter refuses it.
    check_unsafe_fn(
        r#"static takes_safe = fn(g: fn(u8.&raw mut, usize) -> isize) -> () { };
static f = fn(c: bool) -> () {
    let g = if c { read } else { safe_read };
    takes_safe(g)
};"#,
        expect![[r#"
            255..259: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize` (this call requires `fn(u8.&raw mut, usize) -> isize` at 286..296) (this argument needs to be `fn(u8.&raw mut, usize) -> isize` at 297..298)
        "#]],
    );
}

#[test]
fn a_join_mixing_a_safe_fn_and_an_unsafe_fn_settles_on_the_unsafe_one() {
    // The least upper bound under the one-way conversion: safe converts up,
    // nothing converts back, so a join with one unsafe leaf can only be an
    // `unsafe fn` — and the safe leaf converts INTO the join, at the join.
    // The call is gated, which is the honest answer: one of the two things
    // `g` may be needs vouching.
    check_unsafe_fn(
        "static gated = fn(c: bool, p: u8.&raw mut, n: usize) -> isize { \
             let g = if c { read } else { safe_read }; g(p, n) \
         };\n\
         static ok = fn(c: bool, p: u8.&raw mut, n: usize) -> isize { \
             let g = if c { read } else { safe_read }; unsafe { g(p, n) } \
         };",
        expect![[r#"
            241..248: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_chain_of_joins_decides_the_calls_open_safety_as_unsafe() {
    // `g`'s witnesses are `h`, whose join has not solved either when `g` is
    // called; the safety the call left open is decided once both joins
    // have.
    check_unsafe_fn(
        r#"static gated = fn(c: bool, d: bool, p: u8.&raw mut, n: usize) -> isize {
    let h = if d { read } else { read };
    let g = if c { h } else { h }; g(p, n)
};
static mixed = fn(c: bool, d: bool, p: u8.&raw mut, n: usize) -> isize {
    let h = if d { read } else { read };
    let g = if c { h } else { safe_read }; g(p, n)
};
static ok = fn(c: bool, d: bool, p: u8.&raw mut, n: usize) -> isize {
    let h = if d { read } else { read };
    let g = if c { h } else { h }; unsafe { g(p, n) }
};"#,
        expect![[r#"
            284..291: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
            452..459: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_converted_fn_takes_the_type_of_the_position_it_landed_in() {
    // The conversion happened HERE: past a converted edge the recorded type
    // is the EXPECTED one, so hover on the argument reads `unsafe fn(...)`
    // — the type the value has where it landed. Same decision as
    // variant -> enum, and it is what makes the coercion invisible
    // downstream.
    check_infer(
        "static takes = fn(g: unsafe fn(usize) -> usize) -> () { };\n\
         static twice = fn(x: usize) -> usize { x + x };\n\
         static call = fn() -> () { takes(twice) };",
        expect![[r#"
            15..57 'fn(g: unsafe fn(u...': fn(unsafe fn(usize) -> usize)
            18..19 'g': unsafe fn(usize) -> usize
            54..57 '{ }': ()
            74..105 'fn(x: usize) -> u...': fn(usize) -> usize
            77..78 'x': usize
            96..105 '{ x + x }': usize
            98..99 'x': usize
            98..103 'x + x': usize
            102..103 'x': usize
            121..148 'fn() -> () { take...': fn()
            132..148 '{ takes(twice) }': ()
            134..139 'takes': fn(unsafe fn(usize) -> usize)
            134..146 'takes(twice)': ()
            140..145 'twice': unsafe fn(usize) -> usize
        "#]],
    );
}

#[test]
fn an_unconsumed_mixed_safety_join_is_an_unsafe_fn() {
    // A safe branch's safety is its own variable once it flows into the
    // join, so it does not vote against the unsafe one: the join is the
    // least type both convert to, `unsafe fn`, with or without a check site
    // or a call to convert at. Calling it then needs the block.
    check_unsafe_fn(
        r#"static f = fn(c: bool) -> () { let g = if c { read } else { safe_read }; };
static called = fn(c: bool, p: u8.&raw mut) -> isize {
    let g = if c { read } else { safe_read };
    g(p, 1)
};"#,
        expect![[r#"
            316..323: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn a_join_converts_a_safe_branch_at_an_unsafe_fn_annotation() {
    // THE SECOND CHECK SITE. A join is a check site, so every conversion a
    // direct check performs it must perform too — otherwise wrapping a
    // value in an `if` changes what the language accepts. Three positions,
    // because each reaches the witness loop with a different axiom behind
    // the expectation: an annotation, an argument, a return type.
    check_unsafe_fn(
        r#"static takes = fn(g: unsafe fn(u8.&raw mut, usize) -> isize) -> () { };
static annotated = fn(c: bool) -> () {
    let g: unsafe fn(u8.&raw mut, usize) -> isize = if c { read } else { safe_read };
};
static argument = fn(c: bool) -> () { takes(if c { read } else { safe_read }) };
static returned = fn(c: bool) -> unsafe fn(u8.&raw mut, usize) -> isize {
    if c { read } else { safe_read }
};"#,
        expect![[r#""#]],
    );
}

#[test]
fn a_join_still_refuses_an_unsafe_fn_where_a_safe_fn_is_demanded() {
    // The conversion stayed one-way inside the join too: the branch that
    // owes a marker is the culprit, named on its own, and the safe branch
    // is not dragged in with it.
    check_unsafe_fn(
        "static f = fn(c: bool) -> () { \
             let g: fn(u8.&raw mut, usize) -> isize = if c { read } else { safe_read }; \
         };",
        expect![[r#"
            214..218: type mismatch: expected `fn(u8.&raw mut, usize) -> isize`, found `unsafe fn(u8.&raw mut, usize) -> isize` (expected `fn(u8.&raw mut, usize) -> isize` because of this annotation at 173..204)
        "#]],
    );
}
