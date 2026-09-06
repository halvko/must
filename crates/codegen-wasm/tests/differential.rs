//! The targeted differential battery: one program per language feature
//! that has to survive compilation, each run under the interpreter and
//! under the compiled module and required to agree.
//!
//! `examples.rs` is the same machine pointed at `examples/`; this file is
//! where the sharp edges live (overflow at every width, division traps,
//! widening, dictionary dispatch, divergence).

mod harness;

use harness::{check, trap_kind};

// --- arithmetic, widths, and the ruled trap-on-overflow ------------------

#[test]
fn integer_arithmetic_of_every_width() {
    check(
        r#"
static main = fn () -> () {
    let a: u8 = 200;
    let b: u8 = 55;
    if a + b == 255 { print("u8 ok\n") } else { print("u8 bad\n") };
    let c: i8 = 0 - 128;
    if c < 0 { print("i8 ok\n") } else { print("i8 bad\n") };
    let d: u16 = 65535;
    if d / 5 == 13107 { print("u16 ok\n") } else { print("u16 bad\n") };
    let e: i16 = 0 - 32768;
    if e + 32767 == 0 - 1 { print("i16 ok\n") } else { print("i16 bad\n") };
    let f: u32 = 4294967295;
    if f - 1 == 4294967294 { print("u32 ok\n") } else { print("u32 bad\n") };
    let g: i32 = 2147483647;
    if g / 2 == 1073741823 { print("i32 ok\n") } else { print("i32 bad\n") };
    let h: u64 = 18446744073709551615;
    if h / 3 == 6148914691236517205 { print("u64 ok\n") } else { print("u64 bad\n") };
    let i: i64 = 0 - 9223372036854775808;
    if i + 1 < 0 { print("i64 ok\n") } else { print("i64 bad\n") };
    let j: usize = 12;
    if j * j == 144 { print("usize ok\n") } else { print("usize bad\n") };
    let k: isize = 0 - 7;
    if k * 3 == 0 - 21 { print("isize ok\n") } else { print("isize bad\n") };
}
"#,
        "main()",
    );
}

#[test]
fn unsigned_comparisons_are_unsigned() {
    // A `u64` above `i64::MAX` must compare as the large number it is —
    // the storage slot is an `i64`, the operation is not.
    check(
        r#"
static main = fn () -> bool {
    let big: u64 = 18446744073709551615;
    let small: u64 = 1;
    big > small
}
"#,
        "main()",
    );
}

#[test]
fn addition_overflow_traps_at_every_width() {
    for (ty, value) in [
        ("u8", "255"),
        ("u16", "65535"),
        ("u32", "4294967295"),
        ("u64", "18446744073709551615"),
        ("usize", "18446744073709551615"),
    ] {
        check(
            &format!(
                r#"
static main = fn () -> {ty} {{
    let a: {ty} = {value};
    a + 1
}}
"#
            ),
            "main()",
        );
    }
}

#[test]
fn subtraction_below_zero_traps() {
    check(
        r#"
static main = fn () -> usize {
    let a: usize = 0;
    a - 1
}
"#,
        "main()",
    );
}

#[test]
fn multiplication_overflow_traps() {
    check(
        r#"
static main = fn () -> u64 {
    let a: u64 = 9223372036854775807;
    a * 3
}
"#,
        "main()",
    );
    check(
        r#"
static main = fn () -> i8 {
    let a: i8 = 100;
    a * 2
}
"#,
        "main()",
    );
}

#[test]
fn signed_overflow_traps_both_directions() {
    check(
        r#"
static main = fn () -> i64 {
    let a: i64 = 9223372036854775807;
    a + 1
}
"#,
        "main()",
    );
    check(
        r#"
static main = fn () -> i64 {
    let a: i64 = 0 - 9223372036854775808;
    a - 1
}
"#,
        "main()",
    );
}

#[test]
fn division_by_zero_traps() {
    check(
        r#"
static main = fn () -> usize {
    let a: usize = 7;
    let b: usize = 0;
    a / b
}
"#,
        "main()",
    );
}

#[test]
fn signed_division_overflow_traps() {
    check(
        r#"
static main = fn () -> i32 {
    let a: i32 = 0 - 2147483648;
    let b: i32 = 0 - 1;
    a / b
}
"#,
        "main()",
    );
}

#[test]
fn negation_traps_on_unsigned_and_on_the_signed_minimum() {
    check(
        r#"
static main = fn () -> usize {
    let a: usize = 3;
    -a
}
"#,
        "main()",
    );
    check(
        r#"
static main = fn () -> i8 {
    let a: i8 = 0 - 128;
    -a
}
"#,
        "main()",
    );
    check(
        r#"
static main = fn () -> i8 {
    let a: i8 = 42;
    -a
}
"#,
        "main()",
    );
}

#[test]
fn the_trap_kinds_are_distinguishable() {
    use codegen_wasm::TrapKind;
    assert_eq!(
        trap_kind(
            "static main = fn () -> usize { let a: usize = 0; a - 1 }",
            "main()"
        ),
        TrapKind::Overflow
    );
    assert_eq!(
        trap_kind(
            "static main = fn () -> usize { let a: usize = 1; let b: usize = 0; a / b }",
            "main()"
        ),
        TrapKind::DivideByZero
    );
    assert_eq!(
        trap_kind(
            r#"static main = fn () -> usize { panic("boom") }"#,
            "main()"
        ),
        TrapKind::Panic
    );
    assert_eq!(
        trap_kind(
            "static main = fn (i: usize) -> usize { let a = [1, 2, 3]; a[i] }",
            "main(9)"
        ),
        TrapKind::IndexOutOfBounds
    );
}

// --- control flow --------------------------------------------------------

#[test]
fn if_loop_break_continue_and_return() {
    check(
        r#"
static is_even = fn (n: usize) -> bool { (n / 2) * 2 == n };
static sum_odds_below = fn (n: usize) -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == n { break acc; };
        i = i + 1;
        if is_even(i - 1) { continue; };
        acc = acc + (i - 1);
    }
};
static find = fn (step: usize, limit: usize) -> usize {
    if step == 0 { return 0; };
    let mut i = step;
    loop {
        if limit < i { break; };
        if is_even(i) { return i; };
        i = i + step;
    };
    0
};
static main = fn () -> usize { sum_odds_below(10) + find(3, 20) }
"#,
        "main()",
    );
}

#[test]
fn divergence_shapes_survive() {
    // A `!`-typed callee: the call has no continuation at all.
    check(
        r#"
static bail = fn (n: usize) -> ! { panic("bail") };
static main = fn () -> usize {
    let x = 1;
    if x == 1 { bail(x) } else { 0 }
}
"#,
        "main()",
    );
    // A diverging body with no `return` at all.
    check(
        r#"
static spin = fn () -> ! { panic("never returns") };
static main = fn () -> usize { spin() }
"#,
        "main()",
    );
}

// --- data ----------------------------------------------------------------

#[test]
fn records_by_value_and_structural_equality() {
    check(
        r#"
type Point = struct { x: usize, y: str };
static main = fn () -> bool {
    let a = Point(struct { x = 1, y = "hi" });
    let b = Point(struct { x = 1, y = "hi" });
    let c = Point(struct { x = 1, y = "ho" });
    if a == b { print("equal\n") } else { print("unequal\n") };
    if a == c { print("wrong\n") } else { print("differs\n") };
    a.x == 1
}
"#,
        "main()",
    );
}

#[test]
fn nested_records_pass_through_calls_by_value() {
    check(
        r#"
type Inner = struct { a: usize, b: usize };
type Outer = struct { inner: Inner, tag: bool };
static bump = fn (o: Outer) -> Outer {
    Outer(struct { inner = Inner(struct { a = o.inner.a + 1, b = o.inner.b }), tag = o.tag })
};
static main = fn () -> usize {
    let o = bump(Outer(struct { inner = Inner(struct { a = 1, b = 2 }), tag = true }));
    o.inner.a + o.inner.b
}
"#,
        "main()",
    );
}

#[test]
fn arrays_literals_repeat_and_indexing() {
    check(
        r#"
static main = fn () -> usize {
    let a: [usize; 3] = [1, 2, 3];
    let b: [usize; 4] = [7; 4];
    let mut c: [usize; 3] = [0; 3];
    let mut i = 0;
    loop {
        if i == 3 { break; };
        c[i] = a[i] * 2;
        i = i + 1;
    };
    let nested: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    c[0] + c[2] + b[3] + nested[1][0]
}
"#,
        "main()",
    );
}

#[test]
fn a_runtime_index_out_of_bounds_traps_like_the_interpreter() {
    check(
        r#"
static main = fn (i: usize) -> usize {
    let a: [usize; 2] = [1, 2];
    a[i]
}
"#,
        "main(5)",
    );
}

#[test]
fn enums_variants_widening_and_match() {
    check(
        r#"
type Light = enum { Red, Yellow, Green };
static advance = const fn (l: Light::Red) -> Light::Green { Light::Green };
static describe = fn (l: Light) -> str {
    match l {
        ::Red => "stop",
        ::Yellow => "caution",
        ::Green => "go",
    }
};
static main = fn () -> () {
    print(describe(Light::Red));
    print("\n");
    print(describe(advance(Light::Red)));
    print("\n");
    let mut l = Light::Red;
    l = Light::Yellow;
    print(describe(l));
    print("\n");
}
"#,
        "main()",
    );
}

#[test]
fn enum_payloads_widen_and_destructure() {
    check(
        r#"
type Shape = enum { Point, Circle(usize), Rect(usize, usize) };
static area = fn (s: Shape) -> usize {
    match s {
        ::Point => 0,
        ::Circle(r) => r * r * 3,
        ::Rect(w, h) => w * h,
    }
};
static main = fn () -> usize {
    area(Shape::Circle(2)) + area(Shape::Rect(3, 4)) + area(Shape::Point)
}
"#,
        "main()",
    );
}

#[test]
fn a_non_exhaustive_match_traps_with_the_editors_message() {
    check(
        r#"
type Shape = enum { Point, Circle(usize) };
static describe = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
static main = fn () -> usize { describe(Shape::Point) }
"#,
        "main()",
    );
}

#[test]
fn a_variant_typed_match_needs_no_tag() {
    check(
        r#"
type Shape = enum { Point, Circle(usize) };
static radius = fn (c: Shape::Circle) -> usize {
    match c {
        ::Circle(r) => r,
    }
};
static main = fn () -> usize { radius(Shape::Circle(9)) }
"#,
        "main()",
    );
}

// --- statics, const evaluation, strings ----------------------------------

#[test]
fn statics_are_const_baked() {
    check(
        r#"
static seconds_per_day: usize = 60 * 60 * 24;
static double: fn(usize) -> usize = const fn (n: usize) -> usize { n + n };
static fortnight = double(seconds_per_day) * 7;
static banner = "computed at compile time\n";
static table = const {
    let mut t = [0; 5];
    let mut i = 0;
    loop {
        if i == 5 { break t; };
        t[i] = i * i;
        i = i + 1;
    }
};
static main = fn () -> usize {
    print(banner);
    fortnight + table[4]
}
"#,
        "main()",
    );
}

#[test]
fn const_blocks_inside_functions_run_at_compile_time() {
    check(
        r#"
static greet = fn () -> str { const { "compiled in" } };
static main = fn () -> () { print(greet()); print("\n"); }
"#,
        "main()",
    );
}

#[test]
fn string_literals_print_and_compare() {
    check(
        r#"
static main = fn () -> bool {
    print("tabs\tand\nnewlines\n");
    print("quote: \" backslash: \\ nul-free\n");
    let a = "same";
    let b = "same";
    let c = "other";
    if a == b { print("eq\n") } else { print("ne\n") };
    if a == c { print("wrong\n") } else { print("differs\n") };
    a != c
}
"#,
        "main()",
    );
}

#[test]
fn character_literals_compare_and_dispatch() {
    // `char` is a scalar on this target — one slot, like `bool` — so
    // literals, `==`/`!=` and character-pattern dispatch all reach the
    // ordinary scalar paths. A multi-byte literal is the same one slot:
    // the value is a codepoint number, never its UTF-8 bytes.
    check(
        r#"
static classify = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        '\n' => 3,
        'æ' => 4,
        _ => 0,
    }
};
static main = fn () -> bool {
    let a = 'x';
    let b = 'x';
    if a == b { print("eq\n") } else { print("ne\n") };
    if a == 'y' { print("wrong\n") } else { print("differs\n") };
    if classify('(') == 1 { print("open\n") } else { print("bad\n") };
    if classify(')') == 2 { print("close\n") } else { print("bad\n") };
    if classify('\n') == 3 { print("newline\n") } else { print("bad\n") };
    if classify('æ') == 4 { print("multibyte\n") } else { print("bad\n") };
    if classify('z') == 0 { print("other\n") } else { print("bad\n") };
    a != 'y'
}
"#,
        "main()",
    );
}

#[test]
fn character_const_arguments_monomorphize() {
    // `char` reaches the const-argument domain through the same machinery
    // `usize`/`str`/`bool` use, which means it also reaches the INSTANCE
    // KEY: two instantiations at different characters must be two
    // functions with each one's value baked in, and two at the same
    // character must collapse back into one.
    check(
        r#"
static pick = const fn::<const C: char>() -> char { C };
static is_open = const fn::<const C: char>(c: char) -> bool { c == C };
static main = fn () -> bool {
    if pick::<'('>() == '(' { print("open\n") } else { print("bad\n") };
    if pick::<'æ'>() == 'æ' { print("multibyte\n") } else { print("bad\n") };
    if pick::<'('>() == pick::<')'>() { print("bad\n") } else { print("distinct\n") };
    if is_open::<'('>('(') { print("hit\n") } else { print("bad\n") };
    if is_open::<'('>(')') { print("bad\n") } else { print("miss\n") };
    pick::<'x'>() == pick::<'x'>()
}
"#,
        "main()",
    );
}

// --- functions, generics, traits ----------------------------------------

#[test]
fn recursion_and_mutual_recursion() {
    check(
        r#"
static fib: fn(usize) -> usize = fn (n: usize) -> usize {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
};
static is_even: fn(usize) -> bool = fn (n: usize) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
};
static is_odd: fn(usize) -> bool = fn (n: usize) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
};
static main = fn () -> usize {
    if is_even(10) { fib(15) } else { 0 }
}
"#,
        "main()",
    );
}

#[test]
fn a_function_value_passed_as_an_argument_becomes_a_direct_call() {
    check(
        r#"
static double: fn(usize) -> usize = fn (n: usize) -> usize { n + n };
static triple: fn(usize) -> usize = fn (n: usize) -> usize { n * 3 };
static apply_twice: fn(fn(usize) -> usize, usize) -> usize =
    fn (f: fn(usize) -> usize, x: usize) -> usize { f(f(x)) };
static main = fn () -> usize { apply_twice(double, 5) + apply_twice(triple, 2) }
"#,
        "main()",
    );
}

#[test]
fn a_function_returning_a_builtin_is_still_a_direct_call() {
    // `hello.must`'s workaround shape: `fn { print }()` yields the
    // builtin, which is then called.
    check(
        r#"
static main = fn () -> () {
    let s = "hello\n";
    fn { print }()(s);
}
"#,
        "main()",
    );
}

#[test]
fn generic_functions_monomorphize_per_type_argument() {
    check(
        r#"
static id = fn::<T>(x: T) -> T { x };
static main = fn () -> usize {
    print(id::<str>("text"));
    print("\n");
    let n = id::<usize>(7);
    let b = id::<bool>(true);
    if b { n } else { 0 }
}
"#,
        "main()",
    );
}

#[test]
fn const_generics_instantiate_and_forward() {
    check(
        r#"
static square = const fn::<const N: usize>() -> usize { N * N };
static step = const fn::<const N: usize>(acc: usize) -> usize { acc * N };
static cube = const fn::<const N: usize>() -> usize { step::<const N>(N * N) };
static main = fn () -> usize { square::<7>() + cube::<3>() + square::<const { 3 + 4 }>() }
"#,
        "main()",
    );
}

#[test]
fn a_generic_type_with_a_const_length_lays_out_per_instantiation() {
    check(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
static main = fn () -> usize {
    let two: Buf::<2> = Buf::<2>(struct { data = [40, 2], len = 2 });
    let three: Buf::<3> = Buf::<3>(struct { data = [1, 2, 3], len = 3 });
    two.data[0] + two.data[1] + three.data[2]
}
"#,
        "main()",
    );
}

#[test]
fn trait_dictionaries_resolve_into_direct_calls() {
    // The milestone shape: a bound-directed dot-call inside a generic
    // body, a qualified short form, and impls in both homes.
    check(
        r#"
trait Write = requires {
    push: fn(s: str, w: Self) -> Self;
};
trait Display = requires {
    fmt: fn::<W: Write>(w: W, x: Self) -> W;
} with {
    impl str {
        fmt = fn::<W: Write>(w: W, x: str) -> W { w.push(x) };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn (s: str, w: Self) -> Self {
            print(s);
            Sink(struct { pushes = w.pushes + 1 })
        };
    }
};
type Tag = struct { name: str } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, t: Self) -> W {
            let w = Display::fmt(w, "<");
            let w = w.push(t.name);
            Display::fmt(w, ">")
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn () -> usize {
    let a = show::<str>("bare");
    print("\n");
    let b = show::<Tag>(Tag(struct { name = "tagged" }));
    print("\n");
    a + b
}
"#,
        "main()",
    );
}

#[test]
fn qualified_and_named_self_call_forms() {
    check(
        r#"
trait Greet = requires {
    hello: fn(x: Self) -> str;
} with {
    impl usize {
        hello = fn (x: usize) -> str { "number" };
    }
};
type Name = struct { n: usize } with {
    impl Self {
        len = fn (x: Self) -> usize { x.n };
    }
    impl Greet {
        hello = fn (x: Self) -> str { "name" };
    }
};
static main = fn () -> usize {
    let n = Name(struct { n = 3 });
    let seven: usize = 7;
    print(Greet::hello(seven));
    print("\n");
    print(Greet::<Self = Name>::hello(n));
    print("\n");
    Name::len(n)
}
"#,
        "main()",
    );
}

// --- places: writing into aggregates ------------------------------------

#[test]
fn field_assignment_writes_through_a_place() {
    check(
        r#"
type Point = struct { x: usize, y: str };
static main = fn () -> usize {
    let mut p = Point(struct { x = 1, y = "start" });
    p.x = 40;
    p.y = "end";
    print(p.y);
    print("\n");
    p.x + 2
}
"#,
        "main()",
    );
}

#[test]
fn nested_field_and_element_assignment() {
    check(
        r#"
type Inner = struct { data: [usize; 3], tag: bool };
type Outer = struct { inner: Inner, count: usize };
static main = fn () -> usize {
    let mut o = Outer(struct {
        inner = Inner(struct { data = [1, 2, 3], tag = false }),
        count = 0,
    });
    o.inner.data[1] = 20;
    o.inner.tag = true;
    o.count = o.count + 1;
    let mut i = 0;
    let mut sum = 0;
    loop {
        if i == 3 { break; };
        sum = sum + o.inner.data[i];
        i = i + 1;
    };
    if o.inner.tag { sum + o.count } else { 0 }
}
"#,
        "main()",
    );
}

#[test]
fn arrays_and_records_compare_structurally() {
    check(
        r#"
type Pair = struct { a: [usize; 2], b: str };
static main = fn () -> bool {
    let x = Pair(struct { a = [1, 2], b = "same" });
    let y = Pair(struct { a = [1, 2], b = "same" });
    let z = Pair(struct { a = [1, 3], b = "same" });
    if x == y { print("eq\n") } else { print("ne\n") };
    if x == z { print("wrong\n") } else { print("differs\n") };
    x != z
}
"#,
        "main()",
    );
}

#[test]
fn a_catch_all_match_arm_takes_the_otherwise_edge() {
    check(
        r#"
type Shape = enum { Point, Circle(usize), Rect(usize, usize) };
static describe = fn (s: Shape) -> str {
    match s {
        ::Circle(r) => "circle",
        other => "something else",
    }
};
static main = fn () -> () {
    print(describe(Shape::Circle(1)));
    print("\n");
    print(describe(Shape::Rect(2, 3)));
    print("\n");
    print(describe(Shape::Point));
    print("\n");
}
"#,
        "main()",
    );
}

#[test]
fn statics_of_aggregate_and_string_types_bake_in() {
    check(
        r#"
static WORDS: [str; 3] = ["alpha", "beta", "gamma"];
type Config = struct { name: str, sizes: [usize; 2], on: bool };
static CONFIG = Config(struct { name = "cfg", sizes = [7, 8], on = true });
static main = fn () -> usize {
    print(WORDS[2]);
    print(" ");
    print(CONFIG.name);
    print("\n");
    if CONFIG.on { CONFIG.sizes[0] + CONFIG.sizes[1] } else { 0 }
}
"#,
        "main()",
    );
}

#[test]
fn unit_and_empty_aggregates_occupy_nothing() {
    check(
        r#"
type Empty = struct { };
static nothing = fn (e: Empty) -> () { };
static main = fn () -> usize {
    let e = Empty(struct { });
    nothing(e);
    if e == Empty(struct { }) { 1 } else { 0 }
}
"#,
        "main()",
    );
}

#[test]
fn nested_loops_break_out_of_the_inner_one_only() {
    check(
        r#"
static main = fn () -> usize {
    let mut total: usize = 0;
    let mut i: usize = 0;
    loop {
        if i == 4 { break; };
        let mut j: usize = 0;
        let inner = loop {
            if j == 3 { break j; };
            j = j + 1;
        };
        total = total + inner;
        i = i + 1;
    };
    total
}
"#,
        "main()",
    );
}

#[test]
fn a_generic_over_a_record_and_an_array_lays_out_per_instance() {
    check(
        r#"
type Point = struct { x: usize, y: usize };
static first = fn::<T>(a: T, b: T) -> T { a };
static main = fn () -> usize {
    let p = first::<Point>(Point(struct { x = 3, y = 4 }), Point(struct { x = 0, y = 0 }));
    let arr = first::<[usize; 2]>([5, 6], [0, 0]);
    let s = first::<str>("kept", "dropped");
    print(s);
    print("\n");
    p.x + p.y + arr[0] + arr[1]
}
"#,
        "main()",
    );
}

#[test]
fn a_const_block_reading_a_const_param_is_evaluated_per_instantiation() {
    check(
        r#"
static scaled = const fn::<const N: usize>(x: usize) -> usize {
    x * const { N * 2 }
};
static main = fn () -> usize { scaled::<3>(2) + scaled::<5>(2) }
"#,
        "main()",
    );
}

#[test]
fn a_runtime_index_into_an_array_of_records_selects_every_slot() {
    check(
        r#"
type Entry = struct { key: str, weight: usize };
static main = fn (i: usize) -> usize {
    let table: [Entry; 3] = [
        Entry(struct { key = "a", weight = 10 }),
        Entry(struct { key = "b", weight = 20 }),
        Entry(struct { key = "c", weight = 30 }),
    ];
    let picked = table[i];
    print(picked.key);
    print("\n");
    picked.weight
}
"#,
        "main(1)",
    );
}

#[test]
fn a_runtime_index_write_into_an_array_of_records() {
    check(
        r#"
type Entry = struct { key: str, weight: usize };
static main = fn (i: usize) -> usize {
    let mut table: [Entry; 3] = [
        Entry(struct { key = "a", weight = 1 }),
        Entry(struct { key = "b", weight = 2 }),
        Entry(struct { key = "c", weight = 3 }),
    ];
    table[i] = Entry(struct { key = "written", weight = 99 });
    print(table[2].key);
    print("\n");
    table[0].weight + table[1].weight + table[2].weight
}
"#,
        "main(2)",
    );
}

// --- two width bugs that a validating engine cannot catch on its own -----
//
// Both classes below compile to a module that validates and runs, so a
// wrong answer is the only sign anything is off. Each has a fixture pinning
// the bug class directly plus neighbours that vary the shape.

#[test]
fn a_tagged_enum_payload_read_is_sized_by_its_binding_not_by_variant_order() {
    // The payload column's type must NOT be taken from the first variant
    // declaring that position: a wider matched variant would then read
    // too few slots and the binding's tail would be zero-filled.
    // `enum { Small(u8), Big(u64, u64), Txt(str) }` matched at `::Txt(s)`
    // pins this — sizing the read from `Small`'s one slot would print
    // nothing at all, since the string's length slot would read as zero.
    check(
        r#"
type Wide = enum { Small(u8), Big(u64, u64), Txt(str) };
static which = fn (w: Wide) -> u64 {
    match w {
        ::Small(x) => 100,
        ::Big(a, b) => b - a,
        ::Txt(s) => { print(s); print("\n"); 0 },
    }
};
static main = fn () -> u64 {
    which(Wide::Txt("a string, printed in full")) + which(Wide::Big(7, 20)) + which(Wide::Small(3))
}
"#,
        "main()",
    );
}

#[test]
fn a_tagged_enum_payload_wider_than_the_first_variants() {
    // The same class with an aggregate payload: sizing `::B(r) => r.x *
    // 1000 + r.y`'s read from `A`'s one-slot `u8` column instead of the
    // matched `B(R2)` binding would drop `r.y` — a 2-slot record read
    // truncated to 1.
    check(
        r#"
type R2 = struct { x: usize, y: usize };
type Mix = enum { A(u8), B(R2) };
static go = fn (m: Mix) -> usize {
    match m {
        ::A(a) => 1,
        ::B(r) => r.x * 1000 + r.y,
    }
};
static main = fn (v: usize) -> usize { go(Mix::B(R2(struct { x = v, y = v + 1 }))) + go(Mix::A(3)) }
"#,
        "main(5)",
    );
}

#[test]
fn a_tagged_enum_payload_narrower_than_the_first_variants() {
    // The benign direction of the same layout question — pinned so a
    // binding narrower than an earlier variant cannot be sized from that
    // wider variant and over-read.
    check(
        r#"
type R2 = struct { x: usize, y: usize };
type Mix6 = enum { W(R2), N(u8) };
static go6 = fn (m: Mix6) -> usize {
    match m { ::W(r) => r.y, ::N(x) => 500 }
};
static main = fn () -> usize { go6(Mix6::N(9)) + go6(Mix6::W(R2(struct { x = 1, y = 77 }))) }
"#,
        "main()",
    );
}

#[test]
fn a_result_shaped_enum_carries_both_arms_through_a_tagged_parameter() {
    // The shape that made this urgent: `Ok(u64) / Err(str)` is ordinary
    // code, and both payloads sit in the same column.
    check(
        r#"
type Outcome = enum { Ok(u64), Err(str) };
static report = fn (o: Outcome) -> u64 {
    match o {
        ::Ok(v) => v,
        ::Err(message) => { print("failed: "); print(message); print("\n"); 0 },
    }
};
static divide = fn (a: u64, b: u64) -> Outcome {
    if b == 0 { Outcome::Err("division by zero") } else { Outcome::Ok(a / b) }
};
static main = fn () -> u64 { report(divide(84, 2)) + report(divide(1, 0)) }
"#,
        "main()",
    );
}

#[test]
fn a_heterogeneous_enum_survives_being_a_static_and_an_entry_value() {
    // Const-baked on one side, returned (and now DECODED by the harness)
    // on the other.
    check(
        r#"
type Wide = enum { Small(u8), Big(u64, u64), Txt(str) };
static baked: Wide = Wide::Txt("baked");
static echo = fn (w: Wide) -> u64 {
    match w {
        ::Small(x) => 1,
        ::Big(a, b) => a + b,
        ::Txt(s) => { print(s); 0 },
    }
};
static main = fn () -> u64 { let w = baked; echo(w) }
"#,
        "main()",
    );
    // The entry VALUE is the tagged enum itself: its tag and payload
    // columns are read back and compared.
    check(
        r#"
type Wide = enum { Small(u8), Big(u64, u64), Txt(str) };
static pick = fn (n: usize) -> Wide {
    if n == 0 { Wide::Small(3) } else if n == 1 { Wide::Big(4, 5) } else { Wide::Txt("chosen") }
};
static main = fn () -> Wide { pick(2) }
"#,
        "main()",
    );
}

#[test]
fn a_fn_literal_passed_to_a_generic_keeps_its_own_return_type() {
    // A `fn` literal operand must NOT be typed as the dummy `fn() -> ()`:
    // unification would then read that as `T = ()`, every `T` local would
    // become zero slots, and `call0(fn() -> usize { 5 })` would produce 0
    // while a `str`-returning literal printed nothing.
    check(
        r#"
static call0 = fn::<T>(f: fn() -> T) -> T { f() };
static call1 = fn::<T>(f: fn(T) -> T, x: T) -> T { f(x) };
static call0b = fn::<T>(f: fn() -> T, d: T) -> T { f() };
static five = fn () -> usize { 5 };
static main = fn () -> usize {
    print(call0(fn () -> str { "a literal's own type\n" }));
    let a = call0(fn () -> usize { 5 });
    let b = call1(fn (x: usize) -> usize { x + 2 }, 40);
    let c = call0b(fn () -> usize { 100 }, 9);
    let d = call0(five);
    a + b + c + d
}
"#,
        "main()",
    );
}

#[test]
fn a_fn_literal_returning_an_aggregate_through_a_generic() {
    // The same seam with a multi-slot `T`, where getting the width wrong
    // would corrupt every field rather than one value.
    check(
        r#"
type Point = struct { x: usize, y: str };
static call0 = fn::<T>(f: fn() -> T) -> T { f() };
static main = fn () -> usize {
    let p = call0(fn () -> Point { Point(struct { x = 7, y = "seven" }) });
    print(p.y);
    print("\n");
    let arr = call0(fn () -> [usize; 3] { [1, 2, 3] });
    p.x + arr[0] + arr[1] + arr[2]
}
"#,
        "main()",
    );
}

// --- adversarial batteries -------------------------------------------------
//
// Each battery below targets one class of correctness risk in the backend
// directly: overflow completeness, type-argument re-derivation, the
// dispatch loop, the trap-reason channel, dictionary-only type arguments,
// zero-width values, and instantiations passed as values.

/// Overflow completeness: every operation at every width, at and around
/// each boundary — including the cases where the 64-bit carrier itself
/// wraps (`u32::MAX * u32::MAX`, `2^32 * 2^32`) and the ones where a
/// naively-written CHECK would trap (`MIN * -1` in both operand orders,
/// `MIN / -1` at every signed width).
#[test]
fn the_overflow_battery_agrees_at_every_boundary() {
    const PROGRAM: &str = "// Overflow-check completeness. One fn per (op, width); boundary values
// supplied from the entry expression. MIN values derived in-body to avoid
// literal-range questions at the entry.
static add_u8 = fn(a: u8, b: u8) -> u8 { a + b };
static sub_u8 = fn(a: u8, b: u8) -> u8 { a - b };
static mul_u8 = fn(a: u8, b: u8) -> u8 { a * b };
static div_u8 = fn(a: u8, b: u8) -> u8 { a / b };
static neg_u8 = fn(a: u8) -> u8 { -a };

static add_i8 = fn(a: i8, b: i8) -> i8 { a + b };
static sub_i8 = fn(a: i8, b: i8) -> i8 { a - b };
static mul_i8 = fn(a: i8, b: i8) -> i8 { a * b };
static div_i8 = fn(a: i8, b: i8) -> i8 { a / b };
static neg_i8 = fn(a: i8) -> i8 { -a };
static i8_min = fn() -> i8 { (0 - 127) - 1 };
static i8_m1 = fn() -> i8 { 0 - 1 };

static add_u16 = fn(a: u16, b: u16) -> u16 { a + b };
static mul_u16 = fn(a: u16, b: u16) -> u16 { a * b };
static add_i16 = fn(a: i16, b: i16) -> i16 { a + b };
static i16_min = fn() -> i16 { (0 - 32767) - 1 };
static neg_i16 = fn(a: i16) -> i16 { -a };
static div_i16 = fn(a: i16, b: i16) -> i16 { a / b };
static mul_i16 = fn(a: i16, b: i16) -> i16 { a * b };

static add_u32 = fn(a: u32, b: u32) -> u32 { a + b };
static sub_u32 = fn(a: u32, b: u32) -> u32 { a - b };
static mul_u32 = fn(a: u32, b: u32) -> u32 { a * b };
static add_i32 = fn(a: i32, b: i32) -> i32 { a + b };
static sub_i32 = fn(a: i32, b: i32) -> i32 { a - b };
static mul_i32 = fn(a: i32, b: i32) -> i32 { a * b };
static div_i32 = fn(a: i32, b: i32) -> i32 { a / b };
static neg_i32 = fn(a: i32) -> i32 { -a };
static i32_min = fn() -> i32 { (0 - 2147483647) - 1 };
static i32_m1 = fn() -> i32 { 0 - 1 };

static add_u64 = fn(a: u64, b: u64) -> u64 { a + b };
static sub_u64 = fn(a: u64, b: u64) -> u64 { a - b };
static mul_u64 = fn(a: u64, b: u64) -> u64 { a * b };
static div_u64 = fn(a: u64, b: u64) -> u64 { a / b };
static neg_u64 = fn(a: u64) -> u64 { -a };

static add_i64 = fn(a: i64, b: i64) -> i64 { a + b };
static sub_i64 = fn(a: i64, b: i64) -> i64 { a - b };
static mul_i64 = fn(a: i64, b: i64) -> i64 { a * b };
static div_i64 = fn(a: i64, b: i64) -> i64 { a / b };
static neg_i64 = fn(a: i64) -> i64 { -a };
static i64_min = fn() -> i64 { (0 - 9223372036854775807) - 1 };
static i64_m1 = fn() -> i64 { 0 - 1 };

static add_us = fn(a: usize, b: usize) -> usize { a + b };
static mul_us = fn(a: usize, b: usize) -> usize { a * b };
static add_is = fn(a: isize, b: isize) -> isize { a + b };
static is_min = fn() -> isize { (0 - 9223372036854775807) - 1 };
static mul_is = fn(a: isize, b: isize) -> isize { a * b };

// signedness of comparisons
static lt_u64 = fn(a: u64, b: u64) -> bool { a < b };
static lt_i64 = fn(a: i64, b: i64) -> bool { a < b };
static ge_u8 = fn(a: u8, b: u8) -> bool { a >= b };
static lt_i8 = fn(a: i8, b: i8) -> bool { a < b };
";
    const CASES: &[&str] = &[
        "add_u8(255, 1)",
        "add_u8(254, 1)",
        "sub_u8(0, 1)",
        "sub_u8(1, 1)",
        "mul_u8(16, 16)",
        "mul_u8(255, 255)",
        "mul_u8(15, 17)",
        "div_u8(7, 0)",
        "div_u8(255, 1)",
        "neg_u8(1)",
        "neg_u8(0)",
        "add_i8(127, 1)",
        "add_i8(126, 1)",
        "sub_i8(i8_min(), 1)",
        "sub_i8(0, 127)",
        "mul_i8(i8_min(), i8_m1())",
        "mul_i8(i8_m1(), i8_min())",
        "div_i8(i8_min(), i8_m1())",
        "div_i8(i8_min(), 1)",
        "neg_i8(i8_min())",
        "neg_i8(127)",
        "add_u16(65535, 1)",
        "mul_u16(256, 256)",
        "mul_u16(255, 257)",
        "add_i16(32767, 1)",
        "neg_i16(i16_min())",
        "div_i16(i16_min(), 0 - 1)",
        "mul_i16(i16_min(), 0 - 1)",
        "add_u32(4294967295, 1)",
        "add_u32(4294967294, 1)",
        "sub_u32(0, 1)",
        "mul_u32(4294967295, 4294967295)",
        "mul_u32(65536, 65536)",
        "mul_u32(65535, 65537)",
        "add_i32(2147483647, 1)",
        "sub_i32(i32_min(), 1)",
        "mul_i32(i32_min(), i32_m1())",
        "mul_i32(46341, 46341)",
        "div_i32(i32_min(), i32_m1())",
        "div_i32(i32_min(), 1)",
        "neg_i32(i32_min())",
        "neg_i32(2147483647)",
        "add_u64(18446744073709551615, 1)",
        "add_u64(18446744073709551614, 1)",
        "sub_u64(0, 1)",
        "sub_u64(18446744073709551615, 18446744073709551615)",
        "mul_u64(4294967296, 4294967296)",
        "mul_u64(18446744073709551615, 2)",
        "mul_u64(18446744073709551615, 1)",
        "mul_u64(0, 18446744073709551615)",
        "mul_u64(2, 9223372036854775807)",
        "mul_u64(2, 9223372036854775808)",
        "div_u64(1, 0)",
        "div_u64(18446744073709551615, 3)",
        "neg_u64(1)",
        "add_i64(9223372036854775806, 1)",
        "add_i64(9223372036854775807, 1)",
        "add_i64(i64_min(), i64_m1())",
        "sub_i64(i64_min(), 1)",
        "sub_i64(0, 9223372036854775807)",
        "mul_i64(i64_min(), i64_m1())",
        "mul_i64(i64_m1(), i64_min())",
        "mul_i64(3037000500, 3037000500)",
        "mul_i64(3037000499, 3037000499)",
        "mul_i64(2147483648, 4294967296)",
        "mul_i64(i64_m1(), 9223372036854775807)",
        "div_i64(i64_min(), i64_m1())",
        "div_i64(i64_min(), 1)",
        "div_i64(i64_min(), 2)",
        "div_i64(7, 0)",
        "neg_i64(i64_min())",
        "neg_i64(9223372036854775807)",
        "add_us(18446744073709551615, 1)",
        "mul_us(4294967296, 4294967296)",
        "add_is(is_min(), 0 - 1)",
        "mul_is(is_min(), 0 - 1)",
        "lt_u64(9223372036854775808, 1)",
        "lt_i64(0 - 9223372036854775808, 1)",
        "ge_u8(255, 1)",
        "lt_i8(0 - 1, 1)",
    ];
    for case in CASES {
        check(PROGRAM, case);
    }
}

/// Type-argument re-derivation under pressure: nested generics,
/// const and type arguments interleaved on one binder, a type argument
/// taught through a chain of generics, same-SHAPED nominal types kept
/// apart, enum-vs-variant unification in both spellings, a type argument
/// reachable only through an array element, and a named function value
/// through a higher-order generic.
#[test]
fn type_argument_rederivation_under_pressure() {
    const PROGRAM: &str = "// Adversarial type-argument re-derivation.
type Pair = struct::<T> { a: T, b: T };
type Option = enum::<T> { Some(T), None };

// nested generics: Pair<Pair<u8>>
static mk = fn::<T>(x: T) -> Pair::<T> { Pair::<T>(struct { a = x, b = x }) };
static nested = fn(v: u8) -> u8 {
    let inner = mk(v);
    let outer = mk(inner);
    outer.b.a
};

// const args and type args interleaved on one binder
static rep = fn::<T, const N: usize>(x: T) -> [T; N] { [x; N] };
static interleaved = fn() -> u16 {
    let xs = rep::<u16, 4>(7);
    xs[3] + xs[0]
};

// generic forwarding through a second generic (T teaches through a chain)
static idg = fn::<T>(x: T) -> T { x };
static chain = fn::<T>(x: T) -> T { idg(x) };
static chained = fn() -> str { chain(\"deep\") };

// same-shaped NOMINAL types through one generic: distinct decls must stay
// distinct (expect duplicate-but-correct instances, never a merge that
// changes behavior).
type Meters = struct { v: usize };
type Feet = struct { v: usize };
static getv = fn::<T>(x: T) -> T { x };
static shapes = fn() -> usize {
    let m = getv(Meters(struct { v = 3 }));
    let f = getv(Feet(struct { v = 4 }));
    let r = getv(struct { v = 5 });
    m.v + f.v + r.v
};

// enum/variant unification: declared Option::<T> vs a concrete variant and
// a concrete enum, T bound through both spellings.
static unwrap_or = fn::<T>(o: Option::<T>, d: T) -> T {
    match o {
        ::Some(x) => x,
        ::None => d,
    }
};
static via_variant = fn() -> usize { unwrap_or(Option::<usize>::Some(41), 1) };
static via_enum = fn(f: bool) -> usize {
    let mut o = Option::<usize>::None;
    if f { o = Option::<usize>::Some(10); };
    unwrap_or(o, 32)
};

// T only reachable through an ARRAY argument's element
static head2 = fn::<T>(xs: [T; 2]) -> T { xs[0] };
static arr_elem = fn() -> str { head2([\"x\", \"y\"]) };

// a generic fn VALUE passed to a higher-order generic and called inside
static apply = fn::<T>(f: fn(T) -> T, x: T) -> T { f(x) };
static double_u8 = fn(x: u8) -> u8 { x + x };
static ho = fn() -> u8 { apply(double_u8, 21) };
";
    for entry in [
        "nested(9)",
        "interleaved()",
        "chained()",
        "shapes()",
        "via_variant()",
        "via_enum(true)",
        "via_enum(false)",
        "arr_elem()",
        "ho()",
    ] {
        check(PROGRAM, entry);
    }
}

/// Dispatch-loop adversaries: a `match` as the loop header driving
/// `continue`/value-`break`/`return`, nested loops whose inner breaks must
/// not leave the outer one, a `return` from three nests deep past two
/// value-carrying breaks, and a loop that traps mid-iteration after
/// printing.
#[test]
fn dispatch_loop_adversaries() {
    const PROGRAM: &str = "// Dispatch-loop adversaries — nested loops, break/continue/return
// interleaved, match in loop headers, value-carrying breaks across nests.
type Cmd = enum { Go, Skip, Stop(usize), Restart };

static next_cmd = fn(i: usize) -> Cmd {
    if i == 7 { Cmd::Stop(i * 10) }
    else if (i / 3) * 3 == i { Cmd::Skip }
    else if i == 5 { Cmd::Restart }
    else { Cmd::Go }
};

// match IS the loop header; continue + value-break + return interleave.
static drive = fn(limit: usize) -> usize {
    let mut i = 0;
    let mut acc = 0;
    let r = loop {
        i = i + 1;
        if i > limit { break 999; };
        match next_cmd(i) {
            ::Skip => { continue; },
            ::Stop(code) => { break code; },
            ::Restart => {
                acc = 0;
                continue;
            },
            ::Go => {},
        };
        acc = acc + i;
        if acc > 100 { return acc; };
    };
    r + acc
};

// nested loops: inner break must not leave the outer; outer carries a value
// computed by inner loops.
static grid = fn(n: usize) -> usize {
    let mut row = 0;
    let mut total = 0;
    loop {
        if row == n { break; };
        let mut col = 0;
        let rowsum = loop {
            if col == n { break col * row; };
            if col == row { col = col + 1; continue; };
            total = total + 1;
            col = col + 1;
        };
        total = total + rowsum;
        row = row + 1;
    };
    total
};

// return from the innermost of three nests, past two value-carrying breaks
static deep_return = fn(x: usize) -> usize {
    let a = loop {
        let b = loop {
            loop {
                if x > 5 { return x * 100; };
                break;
            };
            break x + 1;
        };
        break b + 1;
    };
    a
};

// a loop whose scrutinee computation can trap mid-iteration
static trap_mid = fn(n: usize) -> usize {
    let mut i = 0;
    let mut acc = 0;
    loop {
        if i == n { break acc; };
        print(\"step\\n\");
        acc = acc + 10 / (2 - i);
        i = i + 1;
    }
};
";
    for entry in [
        "drive(6)",
        "drive(20)",
        "grid(3)",
        "grid(0)",
        "deep_return(4)",
        "deep_return(9)",
        "trap_mid(3)",
        "next_cmd(7)",
    ] {
        check(PROGRAM, entry);
    }
}

/// Does the trap-reason channel ever lie? A `u8` add nested inside
/// successful `usize` adds must report the `u8` entry; two checks in one
/// statement must report the one that fired; a runtime panic message
/// under recursion must publish the DEEPEST call's message; per-length
/// bounds entries must stay distinct.
#[test]
fn the_trap_reason_channel_never_lies() {
    const PROGRAM: &str = "// Does the trap-reason channel ever lie?
static tiny = fn(a: u8, b: u8) -> u8 { a + b };
static wide = fn(a: usize, b: usize) -> usize { a + b };

// outer usize adds succeed, inner u8 add traps: the recorded trap must be
// the `+` on `u8` entry, not the usize one.
static layered = fn() -> usize {
    let x = wide(1, 2);
    let y = tiny(250, 10);
    wide(x, 1)
};

// two checks in one statement: first op fine, second traps
static second_of_two = fn(a: usize) -> usize { (a + 1) / (a - a) };

// runtime-computed panic message under recursion: the message set by the
// DEEPEST call is the one that must be published.
static descend = fn(n: usize, m: str) -> usize {
    if n == 0 { panic(m); };
    descend(n - 1, \"inner message\")
};

// literal panic vs runtime panic
static lit = fn() -> usize { panic(\"exact literal\"); };

// bounds message: runtime index (prefix); distinct lengths, distinct entries
static idx3 = fn(xs: [usize; 3], i: usize) -> usize { xs[i] };
static idx2 = fn(xs: [u8; 2], i: usize) -> u8 { xs[i] };
static run_idx = fn(i: usize) -> u8 {
    let a = idx3([1, 2, 3], 0);
    print(\"first index fine\\n\");
    idx2([9, 8], i)
};
";
    for entry in [
        "layered()",
        "second_of_two(3)",
        "descend(2, \"outer message\")",
        "descend(0, \"outer message\")",
        "lit()",
        "run_idx(0)",
        "run_idx(5)",
    ] {
        check(PROGRAM, entry);
    }
}

/// Dictionaries where the type argument is reachable ONLY through them:
/// `T` in no parameter and no return position, `T` known only from
/// a dictionary member's return type, and a dictionary forwarded through
/// a middleman generic.
#[test]
fn dictionary_only_type_arguments() {
    const PROGRAM: &str = "// Dictionaries — Self only reachable through the dict, not the params.
trait Named = requires {
    name: fn() -> str;
};
trait Zero = requires {
    zero: fn() -> Self;
    score: fn(x: Self) -> usize;
};

type Meters = struct { v: usize } with {
    impl Named { name = fn() -> str { \"meters\" }; }
    impl Zero {
        zero = fn() -> Self { Meters(struct { v = 0 }) };
        score = fn(x: Self) -> usize { x.v + 1 };
    }
};
type Feet = struct { v: usize } with {
    impl Named { name = fn() -> str { \"feet\" }; }
    impl Zero {
        zero = fn() -> Self { Feet(struct { v = 10 }) };
        score = fn(x: Self) -> usize { x.v + 2 };
    }
};

// T appears in NO param and NO return: only the dictionary knows it.
static noself = fn::<T: Named>() -> str { Named::<Self = T>::name() };
static both = fn() -> () {
    print(noself::<Meters>());
    print(\"/\");
    print(noself::<Feet>());
};

// T reachable only through the RETURN position of a dict member call:
// let x: T = Zero::<Self = T>::zero() — the local needs T's layout.
static zscore = fn::<T: Zero>() -> usize {
    let x = Zero::<Self = T>::zero();
    Zero::<Self = T>::score(x)
};
static zboth = fn() -> usize { zscore::<Meters>() + zscore::<Feet>() };

// dictionary forwarded through a generic middleman
static via = fn::<T: Named>() -> str { noself::<T>() };
static forwarded = fn() -> str { via::<Feet>() };
";
    for entry in ["both()", "zboth()", "forwarded()"] {
        check(PROGRAM, entry);
    }
}

/// Zero-width values and structural equality.
#[test]
fn zero_width_values_and_structural_equality() {
    const PROGRAM: &str = "type Empty = struct {};
static unitf = fn(e: Empty, n: usize) -> () { print(\"u\\n\") };
static mixed = fn() -> usize {
    let e = Empty(struct {});
    unitf(e, 3);
    7
};
static eqs = fn(a: str, b: str) -> bool { a == b };
static eqrec = fn(n: usize) -> bool {
    struct { s = \"hi\", n = n } == struct { s = \"hi\", n = 4 }
};
static nerec = fn(n: usize) -> bool {
    struct { s = \"hi\", n = n } != struct { s = \"ho\", n = n }
};
static eqarr = fn(x: u8) -> bool { [x, 2] == [1, 2] };
";
    for entry in [
        "mixed()",
        "eqs(\"a\", \"a\")",
        "eqs(\"a\", \"b\")",
        "eqrec(4)",
        "eqrec(5)",
        "nerec(1)",
        "eqarr(1)",
        "eqarr(3)",
    ] {
        check(PROGRAM, entry);
    }
}

/// Instantiations passed as values: `square::<7>` and `square::<9>`
/// handed to a higher-order function must stay two instances.
#[test]
fn instantiations_passed_as_values() {
    check(
        "static square = const fn::<const N: usize>() -> usize { N * N };
static app0 = fn(f: fn() -> usize) -> usize { f() };
static go = fn() -> usize { app0(square::<7>) + app0(square::<9>) };
",
        "go()",
    );
}
