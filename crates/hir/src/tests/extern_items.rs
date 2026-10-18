//! `extern` items: declared types, safety and const contexts.

use super::*;

// ---- `extern static` — `extern` items ----------------------------------

#[test]
fn an_extern_items_declared_type_carries_its_price() {
    // The `extern` item's type carries its price: `unsafe fn(...)`. Still a
    // first-class function value — annotatable, bindable, passable — but
    // one whose CALLS need the marker wherever they happen, which is what
    // the declaration alone could never say once the value escaped.
    //
    // The declaration writes NO expression — its root is synthesized and
    // owns no syntax node (nothing here is that expression), so the type is
    // read where a reader can see it: at a mention.
    check_infer(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;\n\
         static held = fn() -> () { let f = read; };",
        expect![[r#"
            79..107 'fn() -> () { let ...': fn()
            90..107 '{ let f = read; }': ()
            96..97 'f': unsafe fn(u8.&raw mut, usize) -> i64
            100..104 'read': unsafe fn(u8.&raw mut, usize) -> i64
        "#]],
    );
}

#[test]
fn an_extern_items_unwritten_return_type_is_unit() {
    // G22's rule: there is no body for anything
    // to infer from and no call site may decide it, so the one thing a
    // missing return can mean here is "nothing comes back". (Every other fn
    // type's elided return is still inference's business.)
    check_infer(
        "unsafe extern static tick: unsafe fn(i64);\n\
         static held = fn() -> () { let f = tick; };",
        expect![[r#"
            57..85 'fn() -> () { let ...': fn()
            68..85 '{ let f = tick; }': ()
            74..75 'f': unsafe fn(i64)
            78..82 'tick': unsafe fn(i64)
        "#]],
    );
}

#[test]
fn calling_an_extern_item_outside_unsafe_is_rejected() {
    check_diagnostics(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;\n\
         static f = fn(p: u8.&raw mut) -> i64 { read(p, 1) };",
        expect![[r#"
            104..114: calling the `extern` item `read` requires an `unsafe { ... }` block; nothing on this side of the boundary can check what it does
        "#]],
    );
}

#[test]
fn calling_an_extern_item_in_a_const_context_is_rejected() {
    // Not `NonConstFnCall`: "marking it `const fn` would allow this" is
    // advice that cannot be taken — an `extern` item is a DECLARATION, and there
    // is no const spelling of one to reach for. The honest refusal names
    // the missing host.
    check_diagnostics(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;\n\
         static f = const fn(p: u8.&raw mut) -> i64 { unsafe { read(p, 1) } };",
        expect![[r#"
            119..123: cannot call the `extern` item `read` in a const context; there is no host at compile time (this `const fn` is always a const context at 76..81)
        "#]],
    );
}

#[test]
fn taking_an_extern_item_as_a_value_is_free_and_its_call_is_gated() {
    // The old stopgap priced the MENTION (`let f = unsafe { read };`)
    // because the call site could not name what it was calling. Unsafety in
    // the TYPE removes the need: `f` is an `unsafe fn`, so the call is
    // where the marker is demanded — which is both the honest place and the
    // one a reader of the call can act on. Binding, passing and returning
    // an `extern` item are now ordinary things to do.
    check_diagnostics(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;\n\
         static bound = fn() -> () { let f = read; };\n\
         static loose = fn(p: u8.&raw mut) -> isize { let f = read; f(p, 1) };\n\
         static vouched = fn(p: u8.&raw mut) -> isize { let f = read; unsafe { f(p, 1) } };\n\
         static called = fn(p: u8.&raw mut) -> isize { unsafe { read(p, 1) } };",
        expect![[r#"
            171..178: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn one_signature_two_spellings_is_one_type() {
    // The declaration names its parameters (a contract a reader must be
    // able to read); a fn TYPE elsewhere usually does not. The names stop
    // at the syntax layer, so the two spellings must be the same type —
    // otherwise a signature would depend on whether someone wrote `buf`.
    check_diagnostics(
        "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;\n\
         static held: unsafe fn(u8.&raw mut, usize) -> isize = read;",
        expect![[""]],
    );
}

#[test]
fn an_extern_static_with_an_initializer_is_an_ordinary_item() {
    // The syntax error stands (see `syntax`'s own test); what must not
    // happen is the item becoming `extern` ANYWAY, which would turn a
    // written value into a run-time refusal naming a boundary the program
    // never crossed. The VALUE wins: an `extern` item exists only where the
    // declaration is whole, so the const context refuses this call with the
    // ORDINARY message (advice that can be taken — mark it `const fn`)
    // rather than the `extern` one (advice that cannot).
    check_diagnostics(
        "unsafe extern static bad: unsafe fn(i64) -> i64 = fn(n: i64) -> i64 { n };\n\
         static f = fn() -> i64 { unsafe { bad(1) } };\n\
         static g = const fn() -> i64 { unsafe { bad(1) } };",
        expect![[r#"
            48..73: an `extern static` has no initializer: the declaration is the whole contract, and an `extern` item sets nothing to anything
            161..164: cannot call `bad` in a const context; marking it `const fn` would allow this (`bad` is defined here at 21..24) (this `const fn` is always a const context at 132..137)
        "#]],
    );
}

#[test]
fn an_extern_items_contract_admits_no_holes() {
    // `_` names a type the BODY determines, and an `extern` item has no body: the
    // annotation is the whole contract, so a hole in it is unwritten for
    // good. Refused where it is written (once per hole), and typed as an
    // error here — otherwise the mention would read a free variable and the
    // use site would be told to annotate a definition that already is.
    check_diagnostics(
        "unsafe extern static f: unsafe fn(_) -> i64;\n\
         static g = fn() -> i64 { unsafe { f(1) } };",
        expect![[r#"
            34..35: an `extern` item's type must be written in full: the declaration is the whole contract, and there is no body for `_` to be inferred from
        "#]],
    );
}

#[test]
fn extern_data_publishes_no_type_at_all() {
    // `extern` DATA is reserved (G22), and reserved all the way down. The
    // item's value is a host FUNCTION and nothing else, so a contract that
    // is not a function type is no contract: recorded as an error in the
    // item tree, so the root and every mention agree by construction.
    // Publishing the written `usize` to mentions instead would hand the
    // backend a value it cannot produce — one diagnostic on the file, and a
    // program that runs to an internal error.
    check_infer(
        "unsafe extern static x: usize;\n\
         static g = fn() -> usize { let y = x; y };",
        expect![[r#"
            42..72 'fn() -> usize { l...': fn() -> usize
            56..72 '{ let y = x; y }': {error}
            62..63 'y': {error}
            66..67 'x': {error}
            69..70 'y': {error}
        "#]],
    );
}

#[test]
fn a_use_of_extern_data_is_told_the_truth() {
    // The reservation is refused where it is written, and the use site says
    // what is wrong with the name it is using. "Add a type annotation" would
    // be advice that cannot be taken on an item that IS an annotation.
    check_diagnostics(
        "unsafe extern static x: usize;\n\
         static g = fn() -> usize { x };",
        expect![[r#"
            24..29: `extern` data is not supported yet — an `extern` item must have a function type
        "#]],
    );
}

#[test]
fn a_safe_typed_extern_item_is_called_with_no_marker_at_all() {
    // A `fn`-typed extern item needs no `unsafe` at the call, direct or bound.
    check_diagnostics(
        "unsafe extern static now: fn() -> i64;\n\
         static direct = fn() -> i64 { now() };\n\
         static bound = fn() -> i64 { let f = now; f() };",
        expect![[""]],
    );
}

#[test]
fn a_safe_typed_extern_item_still_has_no_host_at_compile_time() {
    // Even with a safe type, an extern item cannot be called in a const context.
    check_diagnostics(
        "unsafe extern static now: fn() -> i64;\n\
         static f = const fn() -> i64 { now() };",
        expect![[r#"
            70..73: cannot call the `extern` item `now` in a const context; there is no host at compile time (this `const fn` is always a const context at 50..55)
        "#]],
    );
}
