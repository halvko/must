//! The blesses and `str_bytes`: bytes into and out of `str`.

use super::*;

#[test]
fn the_checked_bless_answers_ok_for_utf8_and_err_for_anything_else() {
    // "Checked" names ONE half: whether the bytes spell a string. That the
    // pointer addresses them at all is the caller's claim in both
    // spellings, which is why both are `unsafe`.
    let fixture = bytes_fixture(&[104, 105, 0xFF]);
    check_run(
        &format!(
            "{fixture}\
             static good = fn(p: u8.&raw mut) -> str {{\n\
                 match unsafe {{ str_from_utf8(p, 2) }} {{\n\
                     Utf8Result::Ok(s) => s,\n\
                     Utf8Result::Err => \"not utf-8\",\n\
                 }}\n\
             }};\n\
             static bad = fn(p: u8.&raw mut) -> str {{\n\
                 match unsafe {{ str_from_utf8(p, 3) }} {{\n\
                     Utf8Result::Ok(s) => s,\n\
                     Utf8Result::Err => \"not utf-8\",\n\
                 }}\n\
             }};\n\
             static f = fn() -> str {{\n\
                 let ok = with_bytes::<str>(good);\n\
                 let refused = with_bytes::<str>(bad);\n\
                 if ok == \"hi\" {{ refused }} else {{ \"unexpected\" }}\n\
             }};"
        ),
        "f()",
        expect![[r#"
            => "not utf-8"
        "#]],
    );
}

#[test]
fn the_claimed_bless_on_bytes_that_are_not_utf8_is_detected_ub() {
    // A `str` whose contents are not a string is a value the language's own
    // invariant says cannot exist. `str_from_utf8_unchecked` is where a
    // program can assert one into being — so that is where the interpreter
    // catches it, naming the offset rather than the byte.
    let fixture = bytes_fixture(&[104, 0xFF]);
    check_run(
        &format!(
            "{fixture}\
             static claim = fn(p: u8.&raw mut) -> str {{\n\
                 unsafe {{ str_from_utf8_unchecked(p, 2) }}\n\
             }};\n\
             static f = fn() -> str {{ with_bytes::<str>(claim) }};"
        ),
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: `str_from_utf8_unchecked` was given bytes that are not valid UTF-8 — the first bad byte is at offset 1
        "#]],
    );
}

#[test]
fn a_bless_past_the_end_of_the_buffer_is_detected_ub() {
    let fixture = bytes_fixture(&[104, 105]);
    check_run(
        &format!(
            "{fixture}\
             static over = fn(p: u8.&raw mut) -> str {{\n\
                 unsafe {{ str_from_utf8_unchecked(p, 5) }}\n\
             }};\n\
             static f = fn() -> str {{ with_bytes::<str>(over) }};"
        ),
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: `str_from_utf8_unchecked` out of bounds — the buffer names 5 element(s) from index 0, but the array has 2
              note: allocated here
        "#]],
    );
}

#[test]
fn a_bless_of_never_written_bytes_is_detected_ub() {
    // A bless is a value read of every byte in the range, so the uninit
    // gate fires exactly as it does for a deref.
    check_run(
        r#"
static f = fn() -> str {
    match alloc_array::<u8>(4) {
        AllocResult::Ok(p) => unsafe { str_from_utf8_unchecked(p, 4) },
        AllocResult::Err => "oom",
    }
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn a_bless_is_foreign_to_a_live_borrow_of_a_byte_it_reads() {
    // A bless reads its range through the aliasing tree, exactly as `copy`
    // reads its source: blessing bytes is not a route around exclusivity,
    // so a live exclusive borrow of one of them is suspended as `let v =
    // a[0];` would suspend it.
    check_run(
        r#"
static f = fn () -> u8 {
    let mut a: [u8; 2] = [104, 105];
    let m = a[0].&mut;
    unsafe { str_from_utf8_unchecked(a[0].&raw, 2); };
    m.* = 5;
    a[0]
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn a_zero_length_bless_judges_no_pointer_and_is_the_empty_string() {
    // `copy`'s rule, and it is what lets a line scanner bless a blank line
    // — a bare "\n", or the buffer's very start — with no special case.
    check_run(
        "static f = fn() -> str { unsafe { str_from_utf8_unchecked(dangling::<u8>(), 0) } };",
        "f()",
        expect![[r#"
            => ""
        "#]],
    );
}

// ---- `str_bytes` and `s.len()`: the bless, read backwards ---------------

#[test]
fn str_bytes_writes_the_text_and_len_counts_the_bytes_it_wrote() {
    // The round trip that `String` is built on: `len` says how much storage
    // to ask for, `str_bytes` fills it, and the claimed bless reads it back
    // — so a value that went out as a `str` comes back as the same `str`
    // through storage the program owns.
    check_run(
        r#"
static f = fn() -> str {
    let text = "smørre";
    let n = text.len();
    match alloc_array::<u8>(n) {
        AllocResult::Ok(p) => {
            unsafe { str_bytes(text, p); };
            let back = unsafe { str_from_utf8_unchecked(p, n) };
            unsafe { dealloc_array(p, n); };
            back
        }
        AllocResult::Err => "oom",
    }
};
"#,
        "f()",
        expect![[r#"
            => "smørre"
        "#]],
    );
}

#[test]
fn len_is_bytes_not_characters_and_is_const_legal() {
    // Bytes, because bytes are what every other `str` operation counts:
    // `next_char` threads a byte index and both blesses take a byte length.
    // Pure, so a `const` context accepts it — `next_char`'s reasoning.
    check_run(
        r#"
static n: usize = const { "smørre".len() };
static f = fn() -> usize { n };
"#,
        "f()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn str_bytes_past_the_end_of_the_destination_is_detected_ub() {
    // The destination claim is the caller's, exactly as the source claim is
    // a bless's — and the interpreter still catches every case its typed
    // memory can see.
    check_run(
        r#"
static f = fn() -> usize {
    match alloc_array::<u8>(2) {
        AllocResult::Ok(p) => {
            unsafe { str_bytes("hello", p); };
            unsafe { dealloc_array(p, 2); };
            0
        }
        AllocResult::Err => 1,
    }
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: `str_bytes` out of bounds — the destination names 5 element(s) from index 0, but the array has 2
              note: allocated here
        "#]],
    );
}

#[test]
fn str_bytes_of_the_empty_string_looks_at_no_pointer_at_all() {
    // The blesses' zero-length rule, from the other side: writing nothing
    // writes nothing, so the empty `String` needs no allocation and no
    // special case beyond the one `alloc_array(0)` forces.
    check_run(
        r#"
static f = fn() -> usize {
    let p = unsafe { dangling::<u8>() };
    unsafe { str_bytes("", p); };
    "".len()
};
"#,
        "f()",
        expect![[r#"
            => 0
        "#]],
    );
}

#[test]
fn a_zero_length_str_bytes_is_legal_through_a_pointer_to_a_scalar() {
    // The write-side twin of `copy_of_zero_elements_is_legal_through_a_
    // pointer_to_a_scalar`: `write_range`'s zero-length shortcut must hold
    // for ANY pointer, including one whose path does not address an array
    // element, not only `dangling()` — which happens to be shaped as
    // element 0 of a registered array and so cannot tell this apart from
    // the ordinary case on its own.
    check_run(
        r#"
static f = fn() -> u8 {
    let mut x: u8 = 9;
    unsafe { str_bytes("", x.&raw mut); };
    x
};
"#,
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn str_bytes_is_foreign_to_a_live_borrow_of_a_byte_it_writes() {
    // The write-side mirror of the bless's read: `str_bytes` writes its
    // range through the aliasing tree, exactly as `copy`'s destination half
    // does, so a live exclusive borrow of one of the bytes it overwrites is
    // invalidated as `a[0] = 7;` would invalidate it. Writing bytes is not
    // a route around exclusivity.
    check_run(
        r#"
static f = fn () -> u8 {
    let mut a: [u8; 4] = [1, 2, 3, 4];
    let m = a[0].&mut;
    unsafe { str_bytes("hi", a[0].&raw mut); };
    m.*
};
"#,
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}
