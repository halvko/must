//! Safe borrows and borrowed `match`.

use super::*;

// ---- regions stop at the MIR boundary -----------------------------------

#[test]
fn mir_is_region_erased() {
    // THE erasure proof. Two functions whose signatures differ ONLY in
    // their regions lower to MIR that is textually identical apart from
    // their names — no `@a`, no `@b`, nothing to tell them apart. That is
    // the specialization law made mechanical: nothing below this boundary
    // can branch on a region, because there is no region to branch on.
    check_mir(
        "static one = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static two = fn::<@x, @y>(r: usize.&::<@x + @y>) -> usize { r.* };",
        expect![[r#"
            item one:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param r
              _2: usize.&
              _3: usize
              bb0:
                _2 = _1
                _3 = _2.*
                _0 = _3
                return
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item two:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param r
              _2: usize.&
              _3: usize
              bb0:
                _2 = _1
                _3 = _2.*
                _0 = _3
                return
            }
            fn b1() -> fn(usize.&) -> usize {
              _0: fn(usize.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_safe_borrow_is_its_own_value_expr() {
    // `.&`/`.&mut` and `.&raw`/`.&raw mut` compute the same address and
    // lower through the same place walk, but they are DIFFERENT value expressions —
    // so every exhaustive consumer has to decide what a borrow means for
    // it instead of inheriting the raw answer. The wasm backend's refusal
    // hangs off exactly this.
    check_mir(
        "static f = fn () -> () {\n\
             let mut n: usize = 1;\n\
             let borrowed = n.&mut;\n\
             let r = n.&raw mut;\n\
         };",
        expect![[r#"
            item f:
            fn b0() -> () {
              _0: ()  // return
              _1: usize  // n
              _2: usize.&mut
              _3: usize.&mut  // borrowed
              _4: usize.&raw mut
              _5: usize.&raw mut  // r
              bb0:
                _1 = 1
                _2 = _1.&mut
                _3 = _2
                _4 = _1.&raw mut
                _5 = _4
                _0 = ()
                return
            }
            fn b1() -> fn() {
              _0: fn()  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_reborrow_through_a_borrow_stays_a_borrow() {
    // `m.*.&mut` mints a NEW node (it is `ValueExpr::Borrow`), while
    // `m.*.&raw mut` inherits `m`'s (it is `ValueExpr::AddrOf`). That split
    // is the ruling that `.&raw` is deliberately not a decayed safe
    // borrow, visible in the IR.
    check_mir(
        "static f = fn::<@a>(m: usize.&mut::<@a>) -> () {\n\
             let child = m.*.&mut;\n\
             let r = m.*.&raw mut;\n\
         };",
        expect![[r#"
            item f:
            fn b0(_1: usize.&mut) -> () {
              _0: ()  // return
              _1: usize.&mut  // param m
              _2: usize.&mut
              _3: usize.&mut
              _4: usize.&mut  // child
              _5: usize.&mut
              _6: usize.&raw mut
              _7: usize.&raw mut  // r
              bb0:
                _2 = _1
                _3 = _2.*.&mut
                _4 = _3
                _5 = _1
                _6 = _5.*.&raw mut
                _7 = _6
                _0 = ()
                return
            }
            fn b1() -> fn(usize.&mut) {
              _0: fn(usize.&mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- match projects through borrows -------------------------------------

#[test]
fn a_borrowed_match_reads_the_tag_through_the_deref_and_borrows_its_payloads() {
    // The whole runtime content of the ruling, in one snapshot.
    //
    //   * `switch _2.*` — the tag test is a READ THROUGH THE BORROW, not a
    //     read of a detached copy, so the interpreter's aliasing tree sees
    //     it and an invalidated scrutinee is caught before any arm runs;
    //   * `_4 = _2.*.0.&mut` — the payload binding is a BORROW of the
    //     payload's sub-place, `ValueExpr::Borrow` over `[Deref, Field(0)]`.
    //     Compare the owned lowering (`match_on_enum_lowers_to_switch_variant`),
    //     which is `_4 = _2.0`: a value read out of a copy.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&mut::<@a>) -> () {
    match s {
        ::Some(t) => { t.* = 1; },
        ::None => {},
    }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&mut) -> () {
              _0: ()  // return
              _1: Opt.&mut  // param s
              _2: Opt.&mut
              _3: ()
              _4: usize.&mut  // t
              _5: usize.&mut
              bb0:
                _2 = _1
                switch _2.* on Opt -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = _2.*.0.&mut
                _5 = _4
                _5.* = 1
                storage_dead _4
                _3 = ()
                goto -> bb4
              bb2:
                _3 = ()
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt.&mut) {
              _0: fn(Opt.&mut)  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_shared_borrowed_match_borrows_its_payloads_shared() {
    // The flavor rides through: `&` where the scrutinee was `&`.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&::<@a>) -> usize {
    match s {
        ::Some(t) => t.*,
        ::None => 0,
    }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&) -> usize {
              _0: usize  // return
              _1: Opt.&  // param s
              _2: Opt.&
              _3: usize
              _4: usize.&  // t
              _5: usize.&
              _6: usize
              bb0:
                _2 = _1
                switch _2.* on Opt -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = _2.*.0.&
                _5 = _4
                _6 = _5.*
                storage_dead _4
                _3 = _6
                goto -> bb4
              bb2:
                _3 = 0
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt.&) -> usize {
              _0: fn(Opt.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_whole_value_binder_through_a_borrow_copies_the_pointer() {
    // A bare bind names the same place the scrutinee does, so it aliases
    // the value exactly as it always did — which through a borrow means
    // copying the pointer, not minting a node. It also decides no tag, so
    // there is no switch and nothing is read through the borrow.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn::<@a>(s: Opt.&::<@a>) -> Opt.&::<@a> {
    match s { whole => whole }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt.&) -> Opt.& {
              _0: Opt.&  // return
              _1: Opt.&  // param s
              _2: Opt.&
              _3: Opt.&
              _4: Opt.&  // whole
              _5: Opt.&
              bb0:
                _2 = _1
                goto -> bb1
              bb1:
                _4 = _2
                _5 = _4
                storage_dead _4
                _3 = _5
                goto -> bb2
              bb2:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt.&) -> Opt.& {
              _0: fn(Opt.&) -> Opt.&  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_owned_match_that_dispatches_on_nothing_has_no_switch() {
    // The dead-switch elision is not a borrowed-scrutinee special case: an
    // owned match whose only arm is a catch-all decides nothing on the tag
    // either, so it lowers to a plain `goto`. Here that drops only a read
    // of the pinned temp, which nothing can borrow into.
    check_mir(
        r#"
type Opt = enum { Some(usize), None };
static f = fn(o: Opt) -> usize {
    match o { _ => 7 }
};
"#,
        expect![[r#"
            item Opt:
            item f:
            fn b0(_1: Opt) -> usize {
              _0: usize  // return
              _1: Opt  // param o
              _2: Opt
              _3: usize
              bb0:
                _2 = _1
                goto -> bb1
              bb1:
                _3 = 7
                goto -> bb2
              bb2:
                _0 = _3
                return
            }
            fn b1() -> fn(Opt) -> usize {
              _0: fn(Opt) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}
