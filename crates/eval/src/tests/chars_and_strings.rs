//! Characters, strings, escapes and `next_char`.

use super::*;

// --- char -------------------------------------------------------------

#[test]
fn character_literals_are_values_with_equality() {
    // Escapes are cooked here exactly as a string's are (one table, one
    // answer), and `==`/`!=` fall out of the machine's structural operand
    // equality — a `char` needs no comparison code of its own.
    check_const(
        r#"
static a = 'x';
static newline = '\n';
static quote = '\'';
static nul = '\0';
static multibyte = 'æ';
static same = 'x' == 'x';
static different = 'x' == 'y';
static unequal = 'x' != 'y';
"#,
        expect![[r#"
            a = 'x'
            newline = '\n'
            quote = '\''
            nul = '\0'
            multibyte = 'æ'
            same = true
            different = false
            unequal = true
        "#]],
    );
}

#[test]
fn a_character_match_dispatches_on_the_value() {
    // Dispatch is a chain of equality TESTS (there is no table to index
    // into a million-wide space), so the first matching arm wins and the
    // `_` arm catches everything else.
    check_run(
        r#"
static classify = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        'æ' => 3,
        _ => 0,
    }
};
static main = fn () -> usize {
    classify('(') + classify(')') * 10 + classify('æ') * 100 + classify('z') * 1000
};
"#,
        "main()",
        expect![[r#"
            => 321
        "#]],
    );
}

#[test]
fn an_integer_match_dispatches_on_the_value() {
    // The same equality chain, over the other scalar — including through
    // a borrow, where the test is a read of the pointee place rather than
    // of a detached copy. The widths come from the scrutinees, and two
    // different ones (`u8`, `usize`) run the same written literals.
    check_run(
        r#"
static classify = fn (b: u8) -> usize {
    match b {
        0 => 1,
        7 => 2,
        255 => 3,
        _ => 0,
    }
};
static borrowed = fn::<@a>(d: usize.&::<@a>) -> usize {
    match d {
        0 => 5,
        _ => 9,
    }
};
static main = fn () -> usize {
    let zero = 0;
    let three = 3;
    classify(0) + classify(7) * 10 + classify(255) * 100 + classify(9) * 1000
        + borrowed(zero.&) * 10000
        + borrowed(three.&) * 100000
};
"#,
        "main()",
        expect![[r#"
            => 950321
        "#]],
    );
}

#[test]
fn next_char_walks_a_string_and_ends_at_its_end() {
    // The index-threading walk: each step gives the scalar value AT `i`
    // and the index of the NEXT boundary, and `i == len` answers `End`.
    check_run(
        r#"
static main = fn () -> str {
    let mut i = 0;
    loop {
        match "hey".next_char(i) {
            ::Char(c, next) => { i = next; },
            ::End => break "done",
        }
    }
};
"#,
        "main()",
        expect![[r#"
            => "done"
        "#]],
    );
}

#[test]
fn next_char_counts_characters_not_bytes() {
    // "smørre" is six characters and seven bytes: the walk visits six
    // times, and the last `next` is the byte length.
    check_run(
        r#"
static main = fn () -> usize {
    let mut n = 0;
    let mut i = 0;
    loop {
        match "smørre".next_char(i) {
            ::Char(c, next) => { n = n + 1; i = next; },
            ::End => break n,
        }
    }
};
static bytes = fn () -> usize {
    let mut i = 0;
    loop {
        match "smørre".next_char(i) {
            ::Char(c, next) => { i = next; },
            ::End => break i,
        }
    }
};
"#,
        "struct { chars = main(), bytes = bytes() }",
        expect![[r#"
            => { bytes = 7, chars = 6 }
        "#]],
    );
}

#[test]
fn next_char_past_the_end_is_end() {
    // At OR PAST the length: an index beyond the string is `End`, not a
    // trap — the same "the input is over" reading `read_line`'s `End` has.
    check_run(
        r#"
static at = fn () -> usize {
    match "ab".next_char(2) { ::Char(c, next) => 1, ::End => 0 }
};
static past = fn () -> usize {
    match "ab".next_char(99) { ::Char(c, next) => 1, ::End => 0 }
};
"#,
        "at() + past()",
        expect![[r#"
            => 0
        "#]],
    );
}

#[test]
fn next_char_mid_codepoint_panics() {
    // An index INSIDE a multi-byte character is a program that lost track
    // of its own index. That is a panic, not an `End` and not a silent
    // slide to the next boundary — rounding it would turn a bug into
    // wrong output. "ø" occupies bytes 1 and 2 of "sø".
    check_run(
        r#"
static main = fn () -> usize {
    match "sø".next_char(2) { ::Char(c, next) => next, ::End => 0 }
};
"#,
        "main()",
        expect![[r#"
            error[Panic]: next_char: byte index 2 is not a char boundary; it is inside a multi-byte character
        "#]],
    );
}

#[test]
fn next_char_runs_at_compile_time() {
    // Pure, so const-legal: the same decode runs in a const context and
    // the answer freezes into the static.
    check_const(
        r#"
static first_boundary = const {
    match "æb".next_char(0) {
        ::Char(c, next) => next,
        ::End => 0,
    }
};
"#,
        expect![[r#"
            first_boundary = 2
        "#]],
    );
}

#[test]
fn a_builtin_dot_calls_arguments_evaluate_before_its_receiver() {
    // A builtin member is not an exception to TR01: like every other
    // dot-call it means `next_char(at(0), text("ab"))`, so the written
    // argument runs before the receiver expression. Both sides print, so
    // the order is observable rather than asserted.
    check_run(
        r#"
static text = fn (s: str) -> str { print("recv "); s };
static at = fn (i: usize) -> usize { print("arg "); i };
static main = fn () -> usize {
    match text("ab").next_char(at(0)) {
        ::Char(c, next) => next,
        ::End => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            output: "arg recv "
            => 1
        "#]],
    );
}

#[test]
fn a_borrowed_character_match_dispatches_through_the_borrow() {
    // The projection rule reaching literal patterns, run for real: the
    // equality test reads the POINTEE, so a `char.&` scrutinee dispatches
    // exactly as the owned value would.
    check_run(
        r#"
static classify = fn::<@a>(c: char.&::<@a>) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
static f = fn() -> usize {
    let a = '(';
    let b = ')';
    let z = 'z';
    classify(a.&) + classify(b.&) * 10 + classify(z.&) * 100
};
"#,
        "f()",
        expect![[r#"
            => 21
        "#]],
    );
}

#[test]
fn a_borrowed_character_match_reads_through_the_borrow() {
    // And the read is a real one THROUGH the borrow, not a detached copy:
    // a scrutinee invalidated before the `match` is caught at the `match`
    // itself — the same event the tag test is for an enum.
    check_run_past_the_cfg_checkers(
        r#"
static f = fn() -> usize {
    let mut c = 'a';
    let m = c.&mut;
    c = 'b';
    match m { 'a' => 1, _ => 0 }
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

#[test]
fn a_character_is_a_const_argument() {
    // `char` joins the annotation-representable const domain alongside
    // `usize`/`str`/`bool` — it fell out of the same machinery rather than
    // being carved in, so it is pinned rather than assumed.
    check_const(
        r#"
static pick = const fn::<const C: char>() -> char { C };
static open = pick::<'('>();
static newline = pick::<'\n'>();
static multibyte = pick::<'æ'>();
static same_instance = pick::<'x'>() == pick::<'x'>();
"#,
        expect![[r#"
            pick = fn
            open = '('
            newline = '\n'
            multibyte = 'æ'
            same_instance = true
        "#]],
    );
}

#[test]
fn escapes_are_cooked_in_const_contexts() {
    // A static initializer is a const context: the same one decoding runs,
    // so the frozen constant already holds the real bytes.
    check_const(
        r#"
static newline = "a\nb";
static tab = "a\tb";
static nul = "a\0b";
static quote = "a\"b";
static backslash = "a\\b";
"#,
        expect![[r#"
            newline = "a\nb"
            tab = "a\tb"
            nul = "a\0b"
            quote = "a\"b"
            backslash = "a\\b"
        "#]],
    );
}

#[test]
fn escapes_work_inside_a_const_block() {
    // `const { ... }` re-enters compile time from runtime code; the value
    // it freezes is the cooked string, not the raw token text.
    check_run(
        r#"static main = fn { print(const { "x\ty\n" }); };"#,
        "main()",
        expect![[r#"
            output: "x\ty\n"
            => ()
        "#]],
    );
}

#[test]
fn escapes_inside_a_multiline_string() {
    // Strings stay multiline: a literal newline is still itself, and an
    // escape in the same literal still decodes.
    check_run(
        "static main = fn { print(\"one\\ttwo\nthree\\n\"); };",
        "main()",
        expect![[r#"
            output: "one\ttwo\nthree\n"
            => ()
        "#]],
    );
}
