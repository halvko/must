//! Variants and `match` lowering, literal arms included.

use super::*;

#[test]
fn variant_construction_builds_the_enum_value() {
    // Constructing a variant builds the enum value in one aggregate: the
    // tag (named by the variant) plus the payload.
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static c = Shape::Circle(3);
static p = Shape::Point;
"#,
        expect![[r#"
            item Shape:
            item c:
            fn b0() -> Shape {
              _0: Shape  // return
              _1: Shape
              bb0:
                _1 = Shape::Circle(3)
                _0 = _1
                return
            }
            item p:
            fn b0() -> Shape {
              _0: Shape  // return
              _1: Shape
              bb0:
                _1 = Shape::Point
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn mixed_variant_join_builds_each_branch_value_directly() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (a: bool) -> Shape {
    if a { Shape::Circle(1) } else { Shape::Point }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: bool) -> Shape {
              _0: Shape  // return
              _1: bool  // param a
              _2: bool
              _3: Shape
              _4: Shape
              _5: Shape
              bb0:
                _2 = _1
                if _2 -> [then: bb1, else: bb2]
              bb1:
                _4 = Shape::Circle(1)
                _3 = _4
                goto -> bb3
              bb2:
                _5 = Shape::Point
                _3 = _5
                goto -> bb3
              bb3:
                _0 = _3
                return
            }
            fn b1() -> fn(bool) -> Shape {
              _0: fn(bool) -> Shape  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn first_class_constructor_synthesizes_a_body() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let make = Shape::Circle;
    make(3)
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: usize) -> Shape {
              _0: Shape  // return
              _1: usize  // param _
              bb0:
                _0 = Shape::Circle(_1)
                return
            }
            fn b1() -> Shape {
              _0: Shape  // return
              _1: fn(usize) -> Shape  // make
              _2: fn(usize) -> Shape
              _3: Shape
              bb0:
                _1 = fn b0
                _2 = _1
                _3 = call _2(3) -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b2() -> fn() -> Shape {
              _0: fn() -> Shape  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn unknown_variant_traps_with_the_diagnostic() {
    check_mir(
        r#"
type Shape = enum { Point };
static s = Shape::Missing;
"#,
        expect![[r#"
            item Shape:
            item s:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "`Shape` has no variant `Missing`" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn member_generic_args_the_binder_cannot_receive_trap_at_the_path() {
    // Without these arms in `value_traps`, lowering falls through to the
    // unrelated (and doubly wrong) "cannot use a variant" trap: `size`
    // names a member, not a variant, and `Measured`'s declaration has no
    // errors.
    check_mir(
        r#"
type Measured = struct { n: usize } with {
    impl Self { size = fn(m: Self) -> usize { m.n }; }
};
static s = Measured::size::<usize>;
"#,
        expect![[r#"
            item Measured:
            item s:
            fn b0() -> fn(Measured) -> usize {
              _0: fn(Measured) -> usize  // return
              _1: fn(Measured) -> usize
              bb0:
                _1 = trap "`Measured::size` takes no generic arguments" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
    // And the kind still reserved: a member whose own binder declares a
    // CONST parameter refuses the whole list. The report is keyed on the
    // CALL (that is the range the squiggle covers), so the trap REPLACES
    // the call rather than following it — otherwise the impl body runs and
    // reports the reserved `N` instead of the refusal the reader was
    // shown.
    check_mir(
        r#"
trait Counted = requires { step: fn::<const N: usize>(Self) -> usize; } with {
    impl usize { step = fn::<const N: usize>(s: usize) -> usize { N }; }
};
static s = fn(n: usize) -> usize { Counted::step::<3>(n) };
"#,
        expect![[r#"
            item Counted:
            item s:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param n
              _2: usize
              _3: usize
              bb0:
                _2 = _1
                _3 = trap "`Counted::step` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)" -> bb1
              bb1:
                _0 = _3
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn variant_own_generic_args_traps_with_the_correction() {
    check_mir(
        r#"
type Shape = enum::<T> { Circle(T), Point };
static s = Shape::Circle::<usize>;
"#,
        expect![[r#"
            item Shape:
            item s:
            fn b0() -> {error} {
              _0: {error}  // return
              _1: {error}
              bb0:
                _1 = trap "a variant has no generic arguments of its own: they belong to the owner — write `Shape::<...>::Circle`" -> bb1
              bb1:
                _0 = _1
                return
            }
        "#]],
    );
}

#[test]
fn match_on_enum_lowers_to_switch_variant() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: Shape) -> usize {
              _0: usize  // return
              _1: Shape  // param s
              _2: Shape
              _3: usize
              _4: usize  // r
              _5: usize
              bb0:
                _2 = _1
                switch _2 on Shape -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _4 = _2.0
                _5 = _4
                storage_dead _4
                _3 = _5
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
            fn b1() -> fn(Shape) -> usize {
              _0: fn(Shape) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn nonexhaustive_match_traps_in_the_otherwise_arm() {
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0(_1: Shape) -> usize {
              _0: usize  // return
              _1: Shape  // param s
              _2: Shape
              _3: usize
              _4: usize  // r
              _5: usize
              bb0:
                _2 = _1
                switch _2 on Shape -> [0: bb1, otherwise: bb2]
              bb1:
                _4 = _2.0
                _5 = _4
                storage_dead _4
                _3 = _5
                goto -> bb3
              bb2:
                _3 = trap "this `match` does not cover `Shape::Point`" -> bb3
              bb3:
                _0 = _3
                return
            }
            fn b1() -> fn(Shape) -> usize {
              _0: fn(Shape) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn construct_then_match_round_trip() {
    // Construct a variant, then match: the tag the variant aggregate
    // writes is exactly what `SwitchVariant` dispatches on.
    check_mir(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn () -> usize {
    let mut s = Shape::Circle(3);
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        expect![[r#"
            item Shape:
            item f:
            fn b0() -> usize {
              _0: usize  // return
              _1: Shape
              _2: Shape  // s
              _3: Shape
              _4: usize
              _5: usize  // r
              _6: usize
              bb0:
                _1 = Shape::Circle(3)
                _2 = _1
                _3 = _2
                switch _3 on Shape -> [0: bb1, 1: bb2, otherwise: bb3]
              bb1:
                _5 = _3.0
                _6 = _5
                storage_dead _5
                _4 = _6
                goto -> bb4
              bb2:
                _4 = 0
                goto -> bb4
              bb3:
                unreachable
              bb4:
                _0 = _4
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

// ---- literal patterns: dispatch by equality chain ----------------------

#[test]
fn a_character_match_lowers_to_a_chain_of_equality_tests() {
    // No table, no switch: one `Eq` against the literal per arm, each
    // `SwitchBool`ing into its own arm block and falling through to the
    // next test. That is the whole reason `char` needed no new terminator
    // and no new operation for eval or the backend to learn.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              _5: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, '(')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = Eq(_2, ')')
                if _5 -> [then: bb4, else: bb5]
              bb4:
                _3 = 2
                goto -> bb1
              bb5:
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_integer_match_lowers_to_the_same_equality_chain() {
    // Integer patterns reuse the character lowering WHOLE — no switch, no
    // jump table, no new operation. The one difference is where the
    // constant's width comes from: inference resolved the pattern against
    // the scrutinee, so the `Eq` compares a typed `u8` against a typed
    // `u8` even though nothing in the source wrote a width.
    check_mir(
        r#"
static f = fn (b: u8) -> usize {
    match b {
        0 => 1,
        7 => 2,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: u8) -> usize {
              _0: usize  // return
              _1: u8  // param b
              _2: u8
              _3: usize
              _4: bool
              _5: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = Eq(_2, 7)
                if _5 -> [then: bb4, else: bb5]
              bb4:
                _3 = 2
                goto -> bb1
              bb5:
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(u8) -> usize {
              _0: fn(u8) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_character_match_without_a_catch_all_traps_at_the_fall_through() {
    // The chain runs out, and the last `else` edge is the deferred error —
    // carrying the exact message the editor already showed, like every
    // other non-exhaustive lowering.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = trap "this `match` does not cover every possible `char`; add a `_` arm" -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn an_integer_match_without_a_catch_all_traps_the_same_way() {
    // The `_` arm is required at EVERY width — a policy, not an
    // arithmetic — so a `u8` match that lists a value still falls through
    // to the deferred trap, carrying the message the editor showed. The
    // character twin's shape, byte for byte apart from the type.
    check_mir(
        r#"
static f = fn (b: u8) -> usize {
    match b {
        0 => 1,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: u8) -> usize {
              _0: usize  // return
              _1: u8  // param b
              _2: u8
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = trap "this `match` does not cover every possible `u8`; add a `_` arm" -> bb1
            }
            fn b1() -> fn(u8) -> usize {
              _0: fn(u8) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn character_arms_after_a_catch_all_lower_as_orphan_blocks() {
    // Everything after a catch-all is dead, but it still LOWERS — as a
    // block no edge targets — so the CFG stays total, exactly as in the
    // switch and straight-line lowerings. A capture error inside one is
    // still reported: MIR lowering is the pass that finds captures.
    check_mir(
        r#"
static f = fn (c: char) -> usize {
    match c {
        'a' => 1,
        _ => 2,
        'b' => 3,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char) -> usize {
              _0: usize  // return
              _1: char  // param c
              _2: char
              _3: usize
              _4: bool
              bb0:
                _2 = _1
                _4 = Eq(_2, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _3 = 2
                goto -> bb1
              bb4:
                _3 = 3
                goto -> bb1
            }
            fn b1() -> fn(char) -> usize {
              _0: fn(char) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_borrowed_character_match_tests_through_the_deref() {
    // The projection rule reaching literal patterns (M13: the scrutinee's
    // flavour decides, all the way down). The equality test names the
    // POINTEE place, not a detached copy — so the interpreter's aliasing
    // tree sees the access and an invalidated scrutinee is caught at the
    // `match`, before any arm runs. The catch-all's binder still takes the
    // borrow itself: it names the very same place.
    check_mir(
        r#"
static f = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        'a' => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: char.&) -> usize {
              _0: usize  // return
              _1: char.&  // param c
              _2: char.&
              _3: usize
              _4: bool
              _5: char.&  // other
              bb0:
                _2 = _1
                _4 = Eq(_2.*, 'a')
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = _2
                storage_dead _5
                _3 = 0
                goto -> bb1
            }
            fn b1() -> fn(char.&) -> usize {
              _0: fn(char.&) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_borrowed_integer_match_tests_through_the_deref_too() {
    // The lens generalises with the pattern kind: an integer referent
    // dispatches by equality exactly as a character one does, so the test
    // reads THROUGH the borrow here for the same reason, and the binder
    // still takes the borrow itself.
    check_mir(
        r#"
static f = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 1,
        other => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize.&) -> usize {
              _0: usize  // return
              _1: usize.&  // param d
              _2: usize.&
              _3: usize
              _4: bool
              _5: usize.&  // other
              bb0:
                _2 = _1
                _4 = Eq(_2.*, 0)
                if _4 -> [then: bb2, else: bb3]
              bb1:
                _0 = _3
                return
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _5 = _2
                storage_dead _5
                _3 = 0
                goto -> bb1
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
fn a_refused_literal_arm_lowers_no_test() {
    // A literal arm inference REFUSED is dead, the same way a wrong-enum
    // variant arm is: no equality test between mismatched types is
    // planted, and the arm body lowers into an orphan block nothing
    // targets (`bb2`). Both literal kinds answer this identically —
    // `'a'` against a `usize` used to emit a live `Eq(_2, 'a')` in the
    // ENTRY block, ill-typed MIR behind nothing. The match's own value
    // trap is unchanged: the refusal is still what the program dies of.
    check_mir(
        r#"
static f = fn (d: usize) -> usize {
    match d {
        'a' => 1,
        _ => 0,
    }
};
"#,
        expect![[r#"
            item f:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param d
              _2: usize
              _3: usize
              _4: usize
              bb0:
                _2 = _1
                _3 = 0
                goto -> bb1
              bb1:
                _4 = trap "type mismatch: this `match` is on a `usize`, and `char` cannot match one" -> bb3
              bb2:
                _3 = 1
                goto -> bb1
              bb3:
                _0 = _4
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}
