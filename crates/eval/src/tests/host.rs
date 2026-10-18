//! `extern` items and the host's `read`.

use super::*;

// ---- the `read` `extern` item — the host byte read ----------------------

#[test]
fn the_host_read_fills_a_byte_buffer_and_answers_the_count() {
    // The whole boundary in one program: a caller-owned buffer, a
    // machine-shaped count, and no text policy anywhere — the bytes come
    // back as bytes.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> i64 {
    match alloc_array::<u8>(8) {
        AllocResult::Ok(p) => {
            let n = unsafe { read(p, 8) };
            let first = unsafe { p.* };
            unsafe { dealloc_array(p, 8); };
            if first == 104 { n } else { 0 - n }
        }
        AllocResult::Err => 0,
    }
};
"#,
        "f()",
        "hi",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn the_host_read_reports_zero_at_end_of_input() {
    check_run(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> i64 {
    match alloc_array::<u8>(4) {
        AllocResult::Ok(p) => {
            let n = unsafe { read(p, 4) };
            unsafe { dealloc_array(p, 4); };
            n
        }
        AllocResult::Err => 0 - 1,
    }
};
"#,
        "f()",
        expect![[r#"
            => 0
        "#]],
    );
}

#[test]
fn an_extern_item_this_host_does_not_provide_is_refused_by_name() {
    // The P01 layer-1 property, stated at run time: a host that does not
    // provide a hook has denied the capability. Saying WHICH one is the
    // whole difference between a refusal and a crash.
    check_run(
        r#"
unsafe extern static launch_missiles: unsafe fn(i64) -> i64;
static f = fn() -> i64 { unsafe { launch_missiles(1) } };
"#,
        "f()",
        expect![[r#"
            error[Runtime]: no host implementation for the `extern` item `launch_missiles` — the interpreter provides `read` and nothing else
        "#]],
    );
}

#[test]
fn an_extern_item_without_its_vouch_traps_with_the_declaration_error() {
    check_run(
        r#"
extern static example: fn() -> ();
static f = fn() -> i64 { print("Hello"); example(); 0 };
"#,
        "f()",
        expect![[r#"
            output: "Hello"
            error[Trap]: declaring an `extern` item is a VOUCH: write `unsafe extern static`
        "#]],
    );
}

#[test]
fn a_misplaced_vouch_traps_with_the_declaration_error() {
    check_run(
        r#"
extern unsafe static example: fn() -> i64;
static f = fn() -> i64 { example() };
"#,
        "f()",
        expect![[r#"
            error[Trap]: the markers come first, `unsafe` before `extern`: write `unsafe extern static`
        "#]],
    );
}

#[test]
fn a_doubled_marker_traps_with_the_declaration_error() {
    check_run(
        r#"
unsafe unsafe extern static example: fn() -> i64;
static f = fn() -> i64 { example() };
"#,
        "f()",
        expect![[r#"
            error[Trap]: `unsafe` is written once, not twice
        "#]],
    );
}

#[test]
fn an_unsafe_non_extern_static_traps_when_read() {
    check_run(
        r#"
unsafe static x: i64 = 1;
static f = fn() -> i64 { x };
"#,
        "f()",
        expect![[r#"
            error[Trap]: only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
        "#]],
    );
}

#[test]
fn a_safe_typed_extern_item_is_called_with_no_marker_and_still_reaches_the_host() {
    // A `fn`-typed extern item is callable without `unsafe` and still reaches the
    // host's name check.
    check_run(
        "unsafe extern static now: fn() -> i64;\n\
         static f = fn() -> i64 { now() };",
        "f()",
        expect![[r#"
            error[Runtime]: no host implementation for the `extern` item `now` — the interpreter provides `read` and nothing else
        "#]],
    );
}

#[test]
fn the_host_read_refuses_a_buffer_shorter_than_the_request() {
    // Judged BEFORE the read runs: a read that consumed input and then
    // trapped would have eaten bytes nobody can get back.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> i64 {
    match alloc_array::<u8>(2) {
        AllocResult::Ok(p) => unsafe { read(p, 8) },
        AllocResult::Err => 0,
    }
};
"#,
        "f()",
        "hello",
        expect![[r#"
            error[UndefinedBehavior]: `read` out of bounds — the buffer names 8 element(s) from index 0, but the array has 2
              note: allocated here
        "#]],
    );
}

#[test]
fn a_zero_length_host_read_touches_nothing() {
    // The `copy` rule: a zero-length request judges no buffer, so a caller
    // can ask for "however much room is left" without a special case when
    // the answer is none. `0` here is NOT end of input.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> i64 { unsafe { read(dangling::<u8>(), 0) } };
"#,
        "f()",
        "hello",
        expect![[r#"
            => 0
        "#]],
    );
}

#[test]
fn the_host_read_invalidates_a_safe_borrow_of_the_bytes_it_writes() {
    // The host's fill is a WRITE, judged against the aliasing tree like
    // `copy`'s destination range: `m` borrows a byte the read overwrites,
    // so reading through `m` afterwards is detected UB, exactly as it
    // would be after `p.*[3] = v`.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> u8 {
    let mut a: [u8; 4] = [1, 2, 3, 4];
    let p = a[0].&raw mut;
    let m = a[3].&mut;
    unsafe { read(p, 4); };
    m.*
};
"#,
        "f()",
        "abcd",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_host_read_is_fine_with_no_live_borrow_of_the_buffer() {
    // The twin: the same write, with nothing borrowing the range.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn() -> u8 {
    let mut a: [u8; 4] = [1, 2, 3, 4];
    let p = a[0].&raw mut;
    unsafe { read(p, 4); };
    unsafe { p.* }
};
"#,
        "f()",
        "abcd",
        expect![[r#"
            => 97
        "#]],
    );
}

#[test]
fn the_host_read_judges_aliasing_before_consuming_input() {
    // The discriminating case for the PRE-consume judgement: with no input
    // arriving, `read`'s post-consume `write_range` is never reached at
    // all (`host_read` returns early on an empty read) — an empty-input
    // run of this program traps only if the judgement on the FULL
    // requested length, before `self.mode.read`, is the one doing the
    // work.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> i64;
static f = fn () -> u8 {
    let mut a: [u8; 4] = [1, 2, 3, 4];
    let m = a[0].&mut;
    unsafe { read(a[0].&raw mut, 2); };
    m.*
};
"#,
        "f()",
        "",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

// ---- the host judges the DECLARATION, in full ---------------------------

/// Every way of declaring `read` that this host does not provide.
///
/// The buffer's POINTEE is the one that matters most, and the reason is that
/// no argument VALUE can carry it: a fresh `alloc_array::<bool>(n)` and a
/// fresh `alloc_array::<u8>(n)` hold the same uninitialized elements. Only
/// the declared type says which array this is, and filling a `bool` array
/// with bytes mints values the type system says cannot exist — discovered
/// much later, as an internal error blaming the compiler.
#[test]
fn an_extern_item_declared_with_the_wrong_signature_is_refused() {
    let program = |buf: &str, arg: &str, len: &str, ret: &str| {
        format!(
            "unsafe extern static read: unsafe fn({buf}, usize) -> {ret};\n\
             static f = fn() -> {ret} {{\n\
                 match alloc_array::<u8>(8) {{\n\
                     ::Ok(p) => {{ unsafe {{ p.* = 0; }}; unsafe {{ read({arg}, {len}) }} }}\n\
                     ::Err => panic(\"oom\"),\n\
                 }}\n\
             }};"
        )
    };
    // A pointer to the wrong element type — the case that used to write
    // bytes into a `bool` array and print `104` as a boolean.
    check_run(
        &program("bool.&raw mut", "dangling::<bool>()", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A SHARED pointer: `read` writes, so read-only is as wrong as non-byte.
    check_run(
        &program("u8.&raw", "p.*.&raw", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // Not a pointer at all. These used to blame the compiler ("internal
    // error: expected a raw pointer") for what the declaration said.
    check_run(
        &program("i64", "1", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    check_run(
        &program("str", "\"x\"", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A zero-length request touches no buffer, and is judged all the same:
    // the signature is wrong before any argument is looked at.
    check_run(
        &program("i64", "1", "0", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A return type that is not a signed machine word.
    check_run(
        &program("u8.&raw mut", "p", "8", "bool"),
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // Declaring `read` as a safe `fn` is a signature error.
    check_run(
        "unsafe extern static read: fn(u8.&raw mut, usize) -> isize;\n\
         static f = fn() -> isize {\n\
             match alloc_array::<u8>(8) {\n\
                 ::Ok(p) => read(p, 8),\n\
                 ::Err => panic(\"oom\"),\n\
             }\n\
         };",
        "f()",
        expect![[r#"
            error[Runtime]: the `extern` item `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
}

#[test]
fn the_host_read_answers_in_whichever_signed_word_the_declaration_asked_for() {
    // `isize` (POSIX's own `ssize_t`) and `i64` are both Must spellings of
    // one signed machine word on this target, so the host accepts either and
    // builds its answer in the one asked for. The declaration decides the
    // Must-side type; the host only promises the machine shape.
    for spelling in ["isize", "i64"] {
        check_run_with_input(
            &format!(
                "unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> {spelling};\n\
                 static f = fn() -> {spelling} {{\n\
                     match alloc_array::<u8>(8) {{\n\
                         ::Ok(p) => unsafe {{ read(p, 8) }},\n\
                         ::Err => panic(\"oom\"),\n\
                     }}\n\
                 }};"
            ),
            "f()",
            "hi",
            expect![[r#"
                => 2
            "#]],
        );
    }
}

#[test]
fn calling_an_extern_item_through_a_binding_traps_with_the_squiggle_text() {
    // The check-time diagnostic and the trap are the same sentence — the
    // house rule — so a program that reaches the host through a binding
    // cannot execute it unvouched. Note WHERE the trap is: taking the value
    // ran fine, and the CALL is what refused, because the call is the
    // operation and `g`'s type is what still knows a marker is owed.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;
static f = fn() -> isize {
    let g = read;
    match alloc_array::<u8>(8) { ::Ok(p) => g(p, 8), ::Err => 0 }
};
"#,
        "f()",
        "hi",
        expect![[r#"
            error[Trap]: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
    // Vouched at the CALL, it runs — first-class-ness is priced, not
    // removed, and the binding itself never needed a marker.
    check_run_with_input(
        r#"
unsafe extern static read: unsafe fn(u8.&raw mut, usize) -> isize;
static f = fn() -> isize {
    let g = read;
    match alloc_array::<u8>(8) { ::Ok(p) => unsafe { g(p, 8) }, ::Err => 0 }
};
"#,
        "f()",
        "hi",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn an_extern_static_with_an_initializer_traps_with_the_declaration_error() {
    check_run(
        r#"
unsafe extern static bad: unsafe fn(i64) -> i64 = fn(n: i64) -> i64 { n + 1 };
static f = fn() -> i64 { unsafe { bad(1) } };
"#,
        "f()",
        expect![[r#"
            error[Trap]: an `extern static` has no initializer: the declaration is the whole contract, and an `extern` item sets nothing to anything
        "#]],
    );
}

#[test]
fn extern_data_is_refused_where_it_is_mentioned() {
    // `extern` data is reserved (G22), so there is no value for the
    // interpreter to produce and no host to ask for one. The mention is
    // what refuses, naming the item and what is wrong with it — the
    // alternative is a `usize` no host function can be, carried into
    // arithmetic that then blames the compiler for a program's mistake.
    check_run(
        r#"
unsafe extern static x: usize;
static f = fn() -> usize { x + 1 };
"#,
        "f()",
        expect![[r#"
            error[Trap]: cannot use `x`: its type annotation has errors
        "#]],
    );
}
