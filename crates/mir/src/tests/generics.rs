//! Generic instances at the call site.

use super::*;

// ---- generics: real instances, no staging traps ----
//
// These replace the earlier `generic_mention_lowers_to_a_staging_trap`:
// the same fixture now lowers to a real instantiation — the staging
// refusal (and `TerminatorKind::StagingTrap` itself) is gone.

#[test]
fn generic_call_site_constructs_the_instance_and_calls_it() {
    // The generic body reads its const param as a real `Const::ConstParam`
    // operand (dense const-only index); the call site in `v` lowers the
    // const argument `3` to a compile-time body of its own (the same
    // machinery as a `const` block) and constructs the instance's fn value
    // with `instantiate`, then calls it like any ordinary fn value — no
    // traps anywhere.
    check_mir(
        "static rep = const fn::<const N: usize>() -> usize { N };\nstatic v: usize = rep::<3>();",
        expect![[r#"
            item rep:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = const param 0
                return
            }
            fn b1() -> fn() -> usize {
              _0: fn() -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item v:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = 3
                return
            }
            fn b1() -> usize {
              _0: usize  // return
              _1: fn() -> usize
              _2: usize
              bb0:
                _1 = instantiate rep(const b0)
                _2 = call _1() -> bb1
              bb1:
                _0 = _2
                return
            }
        "#]],
    );
}

#[test]
fn type_only_generic_mention_lowers_to_the_plain_item_value() {
    // Type args need nothing at runtime (TR06): with no const params there
    // is no `instantiate` — the mention lowers to the item's own value,
    // exactly like a non-generic mention.
    //
    // The `move` on the parameter read is the other half of T22 landing
    // here: an UNBOUNDED `T` has no `forget`, so a read of one is a move
    // like any other non-copyable value's. Nothing about lowering changed
    // — `has_forget` simply answers differently now.
    check_mir(
        "static id = fn::<T>(x: T) -> T { x };\nstatic g = fn () -> usize { id::<usize>(4) };",
        expect![[r#"
            item id:
            fn b0(_1: T) -> T {
              _0: T  // return
              _1: T  // param x
              _2: T
              bb0:
                _2 = move _1
                _0 = _2
                return
            }
            fn b1() -> fn(T) -> T {
              _0: fn(T) -> T  // return
              bb0:
                _0 = fn b0
                return
            }
            item g:
            fn b0() -> usize {
              _0: usize  // return
              _1: usize
              bb0:
                _1 = call item id(4) -> bb1
              bb1:
                _0 = _1
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

#[test]
fn forwarded_const_param_lowers_to_a_const_param_body() {
    // The composition case: inside `rep2`, the const argument `const N` is
    // itself a compile-time body reading the enclosing binder's param —
    // forced per instance when the mention executes, the value passing
    // straight through to `rep`'s instance.
    check_mir(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };",
        expect![[r#"
            item rep:
            fn b0(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param x
              _2: usize
              _3: usize
              bb0:
                _2 = _1
                _3 = Mul(_2, const param 0)
                _0 = _3
                return
            }
            fn b1() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
            item rep2:
            fn b0() -> usize {
              _0: usize  // return
              bb0:
                _0 = const param 0
                return
            }
            fn b1(_1: usize) -> usize {
              _0: usize  // return
              _1: usize  // param x
              _2: fn(usize) -> usize
              _3: usize
              _4: usize
              bb0:
                _2 = instantiate rep(const b0)
                _3 = _1
                _4 = call _2(_3) -> bb1
              bb1:
                _0 = _4
                return
            }
            fn b2() -> fn(usize) -> usize {
              _0: fn(usize) -> usize  // return
              bb0:
                _0 = fn b1
                return
            }
        "#]],
    );
}

#[test]
fn generic_enum_match_switches_on_the_declaration() {
    check_mir(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn (o: Option::<usize>) -> usize {\n\
             match o { ::Some(x) => x, ::None => 0, }\n\
         };",
        expect![[r#"
            item Option:
            item main:
            fn b0(_1: Option::<usize>) -> usize {
              _0: usize  // return
              _1: Option::<usize>  // param o
              _2: Option::<usize>
              _3: usize
              _4: usize  // x
              _5: usize
              bb0:
                _2 = _1
                switch _2 on Option -> [0: bb1, 1: bb2, otherwise: bb3]
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
            fn b1() -> fn(Option::<usize>) -> usize {
              _0: fn(Option::<usize>) -> usize  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}

#[test]
fn a_generic_variant_value_is_a_tagged_aggregate() {
    check_mir(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::<usize>::None };",
        expect![[r#"
            item Option:
            item main:
            fn b0() -> Option::<usize> {
              _0: Option::<usize>  // return
              _1: Option::<usize>
              bb0:
                _1 = Option::None
                _0 = _1
                return
            }
            fn b1() -> fn() -> Option::<usize> {
              _0: fn() -> Option::<usize>  // return
              bb0:
                _0 = fn b0
                return
            }
        "#]],
    );
}
