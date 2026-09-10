use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

use crate::machine::{Machine, RunMode, StepEvent};
use crate::{EvalErrorKind, Value};

/// Renders `const_value` for every *value* item in the fixture (`type`
/// items declare no value — nothing to render).
fn check_const(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        if hir::item_data(&db, item)
            .as_ref()
            .is_some_and(|data| matches!(data.kind, hir::ItemKind::Type | hir::ItemKind::Trait))
        {
            continue;
        }
        let name = item.name(&db);
        rendered.push_str(&match crate::const_value(&db, item) {
            Ok(value) => format!("{name} = {}\n", value.display()),
            Err(err) => format!("{name} = error[{:?}]: {}\n", err.kind, err.message),
        });
    }
    expect.assert_eq(&rendered);
}

/// Appends `static entrypoint = (<entry>);` to the fixture and runs it with
/// run-mode semantics (`print` is legal at the entry, statics still force as
/// consts). Renders captured output, then the result. No stdin: a
/// `read_line` call sees immediate end-of-input — see
/// [`check_run_with_input`] for the injectable-input twin.
fn check_run(text: &str, entry: &str, expect: Expect) {
    check_run_impl(text, entry, std::io::empty(), expect);
}

/// [`check_run`], but `input` is fed to whatever reads standard input as
/// though it were piped stdin — the mirror of how `check_run` captures
/// `print` output: this is the injection side. The builtin `read_line`
/// consumes it line-by-line (a final line with no trailing `\n` still
/// reads, as the last `Line`, then `End` on the call after); a program
/// that declares its own `read` host import instead consumes it
/// byte-wise, exactly as a real `read(2)` would.
fn check_run_with_input(text: &str, entry: &str, input: &str, expect: Expect) {
    check_run_with_bytes(text, entry, input.as_bytes(), expect);
}

/// [`check_run_with_input`]'s raw-bytes twin, for the one input a Rust
/// `&str` cannot express because it is valid UTF-8 by construction: bytes
/// a bless has to answer for, or an invalid-UTF-8 stream `read_line` must
/// reject by name. This is the one place any eval fixture pipes stdin —
/// [`check_run_with_input`] is defined in terms of it.
fn check_run_with_bytes(text: &str, entry: &str, input: &[u8], expect: Expect) {
    check_run_impl(text, entry, std::io::Cursor::new(input.to_vec()), expect);
}

fn check_run_impl(text: &str, entry: &str, input: impl std::io::BufRead, expect: Expect) {
    let db = RootDatabase::default();
    let full = format!("{text}\nstatic entrypoint = ({entry});\n");
    let file = SourceFile::new(&db, "test.must".to_owned(), full);
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(
        &db,
        RunMode {
            out: Vec::new(),
            input,
        },
    );
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    let printed = String::from_utf8(machine.mode.out).unwrap();
    // Printed output is shown *escaped*, as one `output:` line. `print`
    // writes exactly its argument and appends nothing, so line breaks are
    // part of the program's output rather than a property of `print` —
    // rendering the raw bytes would run consecutive prints together
    // (`431` for three separate digits) and would hide a newline creeping
    // back into `RunMode`. `{:?}` makes every byte visible.
    let mut rendered = String::new();
    if !printed.is_empty() {
        rendered.push_str(&format!("output: {printed:?}\n"));
    }
    rendered.push_str(&match result {
        Ok(value) => format!("=> {}\n", value.display()),
        Err(err) => {
            // Secondary provenance renders as bare labels here (locations
            // are the driver's business; presence is what these tests pin).
            let notes = err
                .notes
                .iter()
                .map(|note| format!("  note: {}\n", note.message))
                .collect::<String>();
            format!("error[{:?}]: {}\n{notes}", err.kind, err.message)
        }
    });
    expect.assert_eq(&rendered);
}

/// Renders `const_block_values` for every item in the fixture: each
/// `const { … }` block's check-time result, in lowering order (inner blocks
/// before the blocks enclosing them), indexed per item.
fn check_const_blocks(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        for (i, (_, result)) in crate::const_block_values(&db, item).iter().enumerate() {
            rendered.push_str(&match result {
                Ok(value) => format!("{name}#{i} = {}\n", value.display()),
                Err(err) => format!("{name}#{i} = error[{:?}]: {}\n", err.kind, err.message),
            });
        }
    }
    expect.assert_eq(&rendered);
}

#[test]
fn arithmetic_and_literals_const_evaluate() {
    check_const(
        r#"
static example: usize = 4 + 5;
static greeting = "hi";
static truth = example < 20;
"#,
        expect![[r#"
            example = 9
            greeting = "hi"
            truth = true
        "#]],
    );
}

#[test]
fn if_and_calls_const_evaluate() {
    // `double` carries the `const fn` marker: only const fns are callable
    // in an initializer, and this test is about the call *working*.
    check_const(
        r#"
static double = const fn (n: usize) -> usize { n * 2 }
static pick: usize = if true { double(21) } else { 0 };
static chained = pick + 1;
"#,
        expect![[r#"
            double = fn
            pick = 42
            chained = 43
        "#]],
    );
}

#[test]
fn fn_items_are_fn_values() {
    check_const(
        "static f = fn { print(\"hi\"); };",
        expect![[r#"
        f = fn
    "#]],
    );
}

#[test]
fn print_is_refused_at_compile_time() {
    // The const-check trap fires with the editor's exact message; the
    // machine's own dynamic refusal (`NotConst`) stays behind it as
    // defense in depth.
    check_const(
        r#"static x = print("hi");"#,
        expect![[r#"
            x = error[Trap]: cannot call `print` in a const context; const evaluation cannot have side effects
        "#]],
    );
}

#[test]
fn panic_in_const_is_an_error() {
    check_const(
        r#"static x: usize = panic("boom");"#,
        expect![[r#"
        x = error[Panic]: boom
    "#]],
    );
}

#[test]
fn division_by_zero_and_overflow_are_runtime_errors() {
    check_const(
        r#"
static div: usize = 1 / 0;
static sub: usize = 0 - 1;
"#,
        expect![[r#"
            div = error[Runtime]: attempt to divide by zero
            sub = error[Runtime]: arithmetic overflow: `0 - 1` does not fit in `usize`
        "#]],
    );
}

#[test]
fn const_cycles_are_detected() {
    check_const(
        r#"
static a: usize = b;
static b: usize = a;
"#,
        expect![[r#"
            a = error[NotConst]: cycle detected while evaluating `a`
            b = error[NotConst]: cycle detected while evaluating `b`
        "#]],
    );
}

#[test]
fn runaway_recursion_hits_the_frame_limit() {
    // `rec` is `const fn` so the calls pass const-check — the subject here
    // is the frame limit, which needs the recursion to actually run.
    check_const(
        r#"
static rec: fn() -> usize = const fn { rec() };
static r: usize = rec();
"#,
        expect![[r#"
            rec = fn
            r = error[NotConst]: stack overflow: recursion exceeded 10000 frames
        "#]],
    );
}

#[test]
fn never_annotated_call_traps_with_the_mismatch_not_an_internal_error() {
    // `g` is `const fn` so the call passes const-check — the subject here
    // is the `!` mismatch trap, which needs the call to be otherwise fine.
    check_const(
        r#"
static g = const fn () -> usize { 1 }
static f: ! = g();
"#,
        expect![[r#"
            g = fn
            f = error[Trap]: type mismatch: expected `!`, found `usize`
        "#]],
    );
}

#[test]
fn run_mode_recursion_overflow_is_a_runtime_error() {
    check_run(
        r#"
static rec = fn (n: usize) -> usize { rec(n + 1) }
"#,
        "rec(0)",
        expect![[r#"
            error[Runtime]: stack overflow: recursion exceeded 10000 frames
        "#]],
    );
}

#[test]
fn reaching_a_trap_reports_the_borrowed_diagnostic() {
    check_const(
        "static x = missing;",
        expect![[r#"
        x = error[Trap]: unresolved name `missing`
    "#]],
    );
}

#[test]
fn run_hello() {
    check_run(
        r#"static main = fn { print("hello"); };"#,
        "main()",
        expect![[r#"
            output: "hello"
            => ()
        "#]],
    );
}

#[test]
fn print_appends_no_newline() {
    // P03, pinned at the only place it is observable: two `print`s land
    // on one line, because `print` writes its argument and nothing else.
    check_run(
        r#"static main = fn { print("a"); print("b"); };"#,
        "main()",
        expect![[r#"
            output: "ab"
            => ()
        "#]],
    );
}

#[test]
fn a_program_writes_its_own_line_breaks() {
    // With no auto-newline, `\n` is the only way to end a line — which is
    // exactly why the escape set exists.
    check_run(
        r#"static main = fn { print("a\n"); print("b\n"); };"#,
        "main()",
        expect![[r#"
            output: "a\nb\n"
            => ()
        "#]],
    );
}

#[test]
fn every_escape_reaches_the_value() {
    // Each escape, cooked once during HIR lowering, observed as the
    // string's runtime bytes, all six in one literal. `\0` is a NUL, not
    // the character `0`.
    check_const(
        r#"static all = "\n\t\r\\\"\0";"#,
        expect![[r#"
            all = "\n\t\r\\\"\0"
        "#]],
    );
}

#[test]
fn read_line_is_refused_at_compile_time() {
    // `read_line`'s exact `print` treatment: the const-check trap fires
    // with the editor's message; the machine's own dynamic refusal
    // (`NotConst`) stays behind it as defense in depth.
    check_const(
        r#"static x = read_line();"#,
        expect![[r#"
            x = error[Trap]: cannot call `read_line` in a const context; const evaluation cannot have side effects
        "#]],
    );
}

#[test]
fn read_line_reads_the_documented_idiom_to_end() {
    // The exact loop `read_line`'s doc comment shows, run for real: two
    // injected lines come back as `Line`, then genuine end-of-input hits
    // `::End` and the loop breaks — no third `Line` invented, no hang.
    check_run_with_input(
        r#"
static main = fn {
    loop {
        match read_line() {
            ::Line(s) => { print(s); print("|"); },
            ::End => break,
        };
    };
};
"#,
        "main()",
        "a\nb\n",
        expect![[r#"
            output: "a|b|"
            => ()
        "#]],
    );
}

#[test]
fn read_line_strips_the_trailing_newline_lf_and_crlf() {
    // Both line-ending styles read identically — no stray `\r` leaks onto
    // the end of a CRLF-terminated line.
    check_run_with_input(
        r#"
static main = fn {
    loop {
        match read_line() {
            ::Line(s) => { print(s); print("|"); },
            ::End => break,
        };
    };
};
"#,
        "main()",
        "unix\nwindows\r\n",
        expect![[r#"
            output: "unix|windows|"
            => ()
        "#]],
    );
}

#[test]
fn read_line_blank_line_is_line_empty_not_end() {
    // A blank line is real input — `Line("")` — never confused with
    // end-of-input. The middle line here is empty; the count after the
    // loop proves all three (including the blank one) were read as lines.
    check_run_with_input(
        r#"
static main = fn () -> usize {
    let mut n = 0;
    loop {
        match read_line() {
            ::Line(s) => { n = n + 1; },
            ::End => break n,
        }
    }
};
"#,
        "main()",
        "a\n\nb\n",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn read_line_reads_a_final_line_with_no_trailing_newline() {
    // A stream that ends mid-line (no final `\n`, as a pipe closing
    // mid-write would look) still yields that text as a `Line` — the
    // BYTES read is what decides `Line` vs `End`, not the presence of a
    // terminator.
    check_run_with_input(
        r#"
static main = fn () -> usize {
    let mut n = 0;
    loop {
        match read_line() {
            ::Line(s) => { n = n + 1; },
            ::End => break n,
        }
    }
};
"#,
        "main()",
        "a\nb",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn read_line_with_no_input_is_immediate_end() {
    // `check_run` (no injected input) is `read_line`'s empty-stdin case:
    // the very first call reports `End`.
    check_run(
        r#"
static main = fn {
    match read_line() {
        ::Line(s) => print("unexpected line\n"),
        ::End => print("end\n"),
    };
};
"#,
        "main()",
        expect![[r#"
            output: "end\n"
            => ()
        "#]],
    );
}

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
        "static classify = fn::<@a>(c: char.&::<@a>) -> usize {\n\
             match c {\n\
                 '(' => 1,\n\
                 ')' => 2,\n\
                 _ => 0,\n\
             }\n\
         };\n\
         static f = fn() -> usize {\n\
             let a = '(';\n\
             let b = ')';\n\
             let z = 'z';\n\
             classify(a.&) + classify(b.&) * 10 + classify(z.&) * 100\n\
         };",
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
    check_run(
        "static f = fn() -> usize {\n\
             let mut c = 'a';\n\
             let m = c.&mut;\n\
             c = 'b';\n\
             match m { 'a' => 1, _ => 0 }\n\
         };",
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
fn read_line_crashes_on_input_it_cannot_decode() {
    // `ReadLineResult` has no error arm (P04), so a failed read is a
    // runtime crash — never an `End` (which would look like clean
    // end-of-input) and never a lossily patched `Line`.
    check_run_with_bytes(
        r#"
static main = fn {
    match read_line() {
        ::Line(s) => print("line\n"),
        ::End => print("end\n"),
    };
};
"#,
        "main()",
        b"\xff\xfe\n",
        expect![[r#"
            error[Runtime]: I/O error in `read_line`: stream did not contain valid UTF-8
        "#]],
    );
}

/// A `Write` that records writes and flushes in order — the only way to
/// see *when* the output buffer was drained relative to a blocking read.
#[derive(Default)]
struct FlushLog(Vec<String>);

impl std::io::Write for FlushLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .push(format!("write {:?}", String::from_utf8_lossy(buf)));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.push("flush".to_owned());
        Ok(())
    }
}

#[test]
fn an_unterminated_prompt_is_flushed_before_read_line_reads() {
    // The CLI's `out` is line-buffered with no per-call flush (P03), so a
    // prompt with no newline would still be sitting in the buffer while
    // `read_line` blocked — the terminal would show nothing and the user
    // would face a bare cursor. `RunMode::read_line` flushes first.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        r#"
static main = fn {
    print("name? ");
    match read_line() {
        ::Line(s) => print(s),
        ::End => print("!"),
    };
};
static entrypoint = (main());
"#
        .to_owned(),
    );
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(
        &db,
        RunMode {
            out: FlushLog::default(),
            input: std::io::Cursor::new("ada\n".to_owned()),
        },
    );
    machine
        .eval_root(&hir::item_loc(&db, entry_item))
        .expect("the program runs clean");
    assert_eq!(
        machine.mode.out.0,
        vec![
            "write \"name? \"".to_owned(),
            "flush".to_owned(),
            "write \"ada\"".to_owned(),
        ]
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

#[test]
fn run_recursive_fib() {
    check_run(
        r#"
static fib = fn (n: usize) -> usize {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
"#,
        "fib(10)",
        expect![[r#"
            => 55
        "#]],
    );
}

#[test]
fn statics_are_const_contexts_even_in_run_mode() {
    // `print` is fine at the entry, but the static's initializer is an
    // implicit `const { … }` whichever driver evaluates it: forcing `x`
    // fires the const-check trap with the editor's message.
    check_run(
        r#"static x: () = print("side effect");"#,
        "x",
        expect![[r#"
            error[Trap]: cannot call `print` in a const context; const evaluation cannot have side effects
        "#]],
    );
}

#[test]
fn deferred_type_errors_crash_at_the_trap_not_before() {
    // Code before the broken line runs; the crash carries the same message
    // the editor shows as a diagnostic.
    check_run(
        r#"
static main = fn {
    print("before");
    let v: usize = "s";
    print("after");
};
"#,
        "main()",
        expect![[r#"
            output: "before"
            error[Trap]: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn only_the_evaluated_culprit_branch_traps() {
    // Both string branches carry type-error traps, but the `0` path is
    // fine: with n != 0 neither wrong branch is reached, so the partial
    // program runs to completion.
    let program = r#"
static constrainer = fn (s: str, u: usize) {}

static f = fn (n: usize) -> () {
    let x = if n == 0 { "" } else { if n == 0 { "" } else { 0 } };
    constrainer("", x);
}
"#;
    check_run(
        program,
        "f(1)",
        expect![[r#"
            => ()
        "#]],
    );
    // With n == 0 the outer wrong branch is evaluated and traps with
    // exactly the diagnostic the editor shows.
    check_run(
        program,
        "f(0)",
        expect![[r#"
            error[Trap]: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn let_hole_pattern_runs_initializer_for_its_side_effects() {
    // The value is discarded, but `print` still runs.
    check_run(
        r#"
static main = fn {
    let _ = print("side effect");
}
"#,
        "main()",
        expect![[r#"
            output: "side effect"
            => ()
        "#]],
    );
}

#[test]
fn run_if_else_chain() {
    check_run(
        r#"
static classify = fn (n: usize) -> str {
    if n == 0 { "zero" } else if n < 10 { "small" } else { "big" }
}
"#,
        r#"classify(5)"#,
        expect![[r#"
            => "small"
        "#]],
    );
}

/// Finds an item by name in the fixture.
fn item<'db>(db: &'db RootDatabase, file: SourceFile, name: &str) -> hir::ItemId<'db> {
    *hir::file_item_ids(db, file)
        .iter()
        .find(|&&it| it.name(db) == name)
        .unwrap_or_else(|| panic!("no item `{name}`"))
}

/// Pins the single-step surface: `start` pushes one frame, `step` advances
/// one statement or terminator, `frame_origin`/`frame_named_locals` stay
/// inspectable between steps (past the last statement they report the
/// terminator's origin), and a step that const-forces an item swaps in and
/// out without leaving a trace in the frame list.
#[test]
fn single_step_surface_walks_frames_across_calls_and_const_forcing() {
    let db = RootDatabase::default();
    let text = r#"
static helper = fn (n: usize) -> usize { n + 1 }
static main = fn (n: usize) -> usize {
    let doubled = helper(n) * 2;
    doubled
}
static entrypoint = (main(20));
"#;
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let (entry, main, helper) = (
        item(&db, file, "entrypoint"),
        item(&db, file, "main"),
        item(&db, file, "helper"),
    );
    let main_body = hir::body::body(&db, main);
    let helper_body = hir::body::body(&db, helper);
    let entry_body = hir::body::body(&db, entry);
    // Origins named through the HIR: main's `helper(n)` call, helper's
    // `n + 1` tail, and the fn-literal blocks (the `return` terminators'
    // origin).
    let hir::body::ExprData::FnLiteral {
        body: main_fn_block,
        ..
    } = &main_body.exprs[main_body.root.unwrap()]
    else {
        panic!("main is a fn literal");
    };
    let hir::body::ExprData::Block { stmts, .. } = &main_body.exprs[*main_fn_block] else {
        panic!("a fn literal's body is a block");
    };
    let hir::body::Stmt::Let { init, .. } = &stmts[0] else {
        panic!("main starts with a let");
    };
    let hir::body::ExprData::Bin { lhs: main_call, .. } = &main_body.exprs[*init] else {
        panic!("the let's initializer is a product");
    };
    let hir::body::ExprData::FnLiteral {
        body: helper_fn_block,
        ..
    } = &helper_body.exprs[helper_body.root.unwrap()]
    else {
        panic!("helper is a fn literal");
    };
    let hir::body::ExprData::Block {
        tail: Some(helper_add),
        ..
    } = &helper_body.exprs[*helper_fn_block]
    else {
        panic!("helper's body is a block with a tail");
    };

    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    machine.start(&hir::item_loc(&db, entry)).unwrap();
    assert_eq!(machine.frames().len(), 1);

    // The entry's call terminator: `main`'s initializer const-forces inside
    // the step; only the run-mode frame appears.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 2);
    assert_eq!(
        machine.frame_named_locals(1),
        vec![(
            "n".to_owned(),
            hir::Ty::Int(hir::IntKind::Usize),
            Value::Int(hir::IntValue::Usize(20)),
        )]
    );
    assert_eq!(
        machine.frame_origin(1),
        Some((hir::item_loc(&db, main), *main_call))
    );

    // Main's call terminator: `helper`'s initializer const-forces inside the
    // step, and helper's run frame is pushed on top.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 3);
    assert_eq!(
        machine.frame_origin(2),
        Some((hir::item_loc(&db, helper), *helper_add))
    );

    // Helper's two statements, then the past-the-end origin: the `return`
    // terminator's, which the next step executes (popping the frame).
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(
        machine.frame_origin(2),
        Some((hir::item_loc(&db, helper), *helper_fn_block))
    );
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(machine.frames().len(), 2);

    // Main's product and let: `doubled` shows up once it holds a value.
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert!(matches!(machine.step(), Ok(StepEvent::Progress)));
    assert_eq!(
        machine.frame_named_locals(1),
        vec![
            (
                "n".to_owned(),
                hir::Ty::Int(hir::IntKind::Usize),
                Value::Int(hir::IntValue::Usize(20)),
            ),
            (
                "doubled".to_owned(),
                hir::Ty::Int(hir::IntKind::Usize),
                Value::Int(hir::IntValue::Usize(42)),
            ),
        ]
    );
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // ret = doubled
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // main returns
    assert_eq!(machine.frames().len(), 1);

    // The entry body has only compiler temps: no named locals.
    assert!(machine.frame_named_locals(0).is_empty());
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // ret = call
    assert_eq!(
        machine.frame_origin(0),
        Some((hir::item_loc(&db, entry), entry_body.root.unwrap()))
    );
    match machine.step() {
        Ok(StepEvent::Done(value)) => assert_eq!(value, Value::Int(hir::IntValue::Usize(42))),
        Ok(StepEvent::Progress) | Err(_) => panic!("expected Done(42)"),
    }
    assert!(machine.frames().is_empty());
}

/// A step that errors leaves the frames exactly as they were, so a debugger
/// can inspect the crash site; the top frame's origin is the panicking call.
#[test]
fn step_errors_leave_frames_intact_for_inspection() {
    let db = RootDatabase::default();
    let text = r#"
static main = fn {
    print("before");
    panic("boom");
}
static entrypoint = (main());
"#;
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let (entry, main) = (item(&db, file, "entrypoint"), item(&db, file, "main"));
    let main_body = hir::body::body(&db, main);
    let hir::body::ExprData::FnLiteral {
        body: main_fn_block,
        ..
    } = &main_body.exprs[main_body.root.unwrap()]
    else {
        panic!("main is a fn literal");
    };
    let hir::body::ExprData::Block { stmts, .. } = &main_body.exprs[*main_fn_block] else {
        panic!("a fn literal's body is a block");
    };
    let hir::body::Stmt::Expr(panic_call) = &stmts[1] else {
        panic!("the second statement is the panic");
    };

    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    machine.start(&hir::item_loc(&db, entry)).unwrap();
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // the entry's call
    assert!(matches!(machine.step(), Ok(StepEvent::Progress))); // print("before")

    let err = match machine.step() {
        Err(err) => err,
        Ok(_) => panic!("expected the panic to stop the machine"),
    };
    assert_eq!(err.kind, EvalErrorKind::Panic);
    assert_eq!(err.message, "boom");
    assert_eq!(machine.frames().len(), 2);
    assert_eq!(
        machine.frame_origin(1),
        Some((hir::item_loc(&db, main), *panic_call))
    );
    assert_eq!(
        machine.frame_origin(0).map(|(loc, _)| loc),
        Some(hir::item_loc(&db, entry))
    );
}

/// A chain of *distinct* items forcing each other is not a cycle: it is the
/// forcing-depth cap that stops it (each level recurses on the Rust stack).
/// Runs on a dedicated thread sized like the server's pool workers (8 MiB,
/// see `must-lsp`'s `pool.rs`): the test asserts that the *cap* fires, and
/// the per-level frames (origin tracking, variant values) need more
/// headroom than libtest's default thread gives.
#[test]
fn distinct_item_chains_hit_the_forcing_depth_cap() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let db = RootDatabase::default();
            // Annotated so hir's cross-item inference (which gives up earlier) does
            // not trap first; the chain below is pure const forcing.
            let mut text = String::from("static s0: usize = 1;\n");
            for i in 1..130 {
                text.push_str(&format!("static s{i}: usize = s{};\n", i - 1));
            }
            let file = SourceFile::new(&db, "test.must".to_owned(), text);
            let top = item(&db, file, "s129");
            let Err(err) = crate::const_value(&db, top) else {
                panic!("a 130-item chain must not const-evaluate");
            };
            assert_eq!(err.kind, EvalErrorKind::NotConst);
            assert_eq!(
                err.message.as_str(),
                "constant evaluation exceeded 128 nested items"
            );
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn static_initializer_calling_a_plain_fn_traps_at_const_eval() {
    // The initializer is a const context and `double` is a plain fn: const
    // evaluation crashes at the trap with exactly the message the editor
    // shows as a squiggle.
    check_const(
        r#"
static double = fn (n: usize) -> usize { n * 2 }
static x = double(2);
"#,
        expect![[r#"
            double = fn
            x = error[Trap]: cannot call `double` in a const context; marking it `const fn` would allow this
        "#]],
    );
}

#[test]
fn run_mode_reaching_an_illegal_call_in_a_const_fn_body_traps() {
    // A `const fn` body is a const context under every execution: run-mode
    // code calling `apply` runs up to the illegal value call inside it,
    // then crashes with the editor's message.
    check_run(
        r#"
static apply = const fn (f: fn() -> usize) -> usize { f() };
static main = fn {
    print("before");
    apply(fn () -> usize { 1 });
};
"#,
        "main()",
        expect![[r#"
            output: "before"
            error[Trap]: cannot call a value in a const context; whether it is a `const fn` is not known from its type
        "#]],
    );
}

#[test]
fn illegal_call_in_an_unevaluated_branch_does_not_crash_const_eval() {
    // The dead branch's call is squiggled (tested in hir) and trapped, but
    // only the evaluated culprit branch traps: with the condition true the
    // initializer const-evaluates to completion.
    check_const(
        r#"
static f = fn () -> usize { 1 }
static x: usize = if true { 5 } else { f() };
"#,
        expect![[r#"
            f = fn
            x = 5
        "#]],
    );
}

#[test]
fn const_block_evaluates_to_its_inner_value() {
    // In run mode the forced compile-time value flows in where the block
    // sits, indistinguishable from evaluating it inline.
    check_run(
        r#"
static main = fn {
    let y: usize = const { 2 + 3 };
    y
}
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn const_blocks_memoize_within_a_machine_run() {
    // `f` runs twice, but its `const { … }` body executes once per machine
    // run: the second hit is served from the per-run memo.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        r#"
static f = fn () -> usize { const { 2 + 3 } }
static entrypoint = (f() + f());
"#
        .to_owned(),
    );
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    assert_eq!(result, Ok(crate::Value::Int(hir::IntValue::Usize(10))));
    assert_eq!(machine.const_block_evaluations(), 1);
}

#[test]
fn const_block_in_a_never_called_fn_fails_at_check_time() {
    // Nothing calls the function, but the `const` block inside it is
    // compile-time code: the panic surfaces from `const_block_values`.
    check_const_blocks(
        r#"static f = fn { const { panic("boom") }; };"#,
        expect![[r#"
            f#0 = error[Panic]: boom
        "#]],
    );
}

#[test]
fn nested_const_blocks_evaluate_inside_out() {
    check_const_blocks(
        r#"static f = fn () -> usize { const { const { 2 } + 3 } };"#,
        expect![[r#"
            f#0 = 2
            f#1 = 5
        "#]],
    );
}

#[test]
fn const_block_forces_items_and_detects_cycles() {
    // `ok`'s block forces the `base` static like any other use; `cyc`'s
    // block forces `cyc` itself, whose initializer re-enters the very block
    // being forced — the cycle is detected, not an infinite regress.
    check_const_blocks(
        r#"
static base: usize = 2 + 3;
static ok = fn () -> usize { const { base } };
static cyc: usize = const { cyc };
"#,
        expect![[r#"
            ok#0 = 5
            cyc#0 = error[NotConst]: cycle detected while evaluating a `const` block in `cyc`
        "#]],
    );
}

#[test]
fn mutation_of_a_let_binding_is_visible_to_later_reads() {
    check_run(
        "",
        "(fn { let mut x: usize = 1; x = x + 2; x })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn mutating_a_mut_param_is_visible_to_later_reads() {
    check_run(
        "static f = fn (mut n: usize) -> usize { n = n + 1; n };",
        "f(41)",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn assigning_to_an_immutable_binding_traps_at_runtime() {
    // The executed path runs into the assignment: it crashes with exactly
    // the message the editor shows as a squiggle.
    check_run(
        "",
        "(fn { let x: usize = 1; x = 2; x })()",
        expect![[r#"
            error[Trap]: cannot assign to `x`: it is not declared `mut`
        "#]],
    );
}

#[test]
fn illegal_assignment_in_an_unevaluated_branch_does_not_crash() {
    // The dead branch's assignment is squiggled (tested in hir) and
    // trapped, but only the evaluated path crashes: with the condition
    // true the function returns normally.
    check_run(
        "",
        "(fn () -> usize { let x = 1; if true { x } else { x = 2; x } })()",
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn field_assignment_mutates_the_record() {
    check_run(
        "",
        "(fn { let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; p.x = 10; p.x + p.y })()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn nested_field_assignment_writes_through_both_levels() {
    // The projection navigates `outer.inner` then `inner.b`; the sibling
    // field and the outer record's other field are untouched.
    check_run(
        "",
        "(fn {\n            let mut p: struct { inner: struct { a: usize, b: usize }, c: usize } =\n                struct { inner = struct { a = 1, b = 2 }, c = 3 };\n            p.inner.b = 20;\n            p.inner.a + p.inner.b + p.c\n        })()",
        expect![[r#"
            => 24
        "#]],
    );
}

#[test]
fn field_assignment_through_a_named_type() {
    // Erasure: a `Point` is its underlying record at runtime; the write
    // projects through the declaration's field order.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> usize {
    let mut p = Point(struct { x = 1, y = 2 });
    p.x = 40;
    p.x + p.y
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn field_assignment_in_a_const_fn_works_at_compile_time() {
    // The field-projection twin of local mutation in a const context:
    // forcing `x` runs the write at check time.
    check_const(
        r#"
static bump = const fn (mut p: struct { n: usize }) -> usize {
    p.n = p.n + 1;
    p.n
};
static x = bump(struct { n = 41 });
"#,
        expect![[r#"
            bump = fn
            x = 42
        "#]],
    );
}

#[test]
fn field_assignment_to_an_immutable_root_traps_at_runtime() {
    // Squiggle-equals-crash, field edition: the message blames the root
    // binding, exactly as the editor shows it.
    check_run(
        "",
        "(fn { let p: struct { x: usize } = struct { x = 1 }; p.x = 2; p.x })()",
        expect![[r#"
            error[Trap]: cannot assign to `p.x`: `p` is not declared `mut`
        "#]],
    );
}

#[test]
fn illegal_field_assign_in_an_unevaluated_branch_does_not_crash() {
    // Culprit-branch idiom: the dead branch's unknown-field write is
    // squiggled (tested in hir) and trapped, but only the evaluated path
    // decides the run.
    check_run(
        "",
        "(fn () -> usize {\n            let mut p = struct { x = 1 };\n            if true { p.x } else { p.bogus = 2; p.x }\n        })()",
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn mutation_inside_a_const_fn_body_works_at_compile_time() {
    // `double` mutates a local of its own body; forcing `x` through it at
    // check time exercises mutation in a const context end to end.
    check_const(
        r#"
static double = const fn (n: usize) -> usize {
    let mut r = n;
    r = r + r;
    r
};
static x = double(21);
"#,
        expect![[r#"
            double = fn
            x = 42
        "#]],
    );
}

#[test]
fn record_construction_and_field_access() {
    check_run(
        "",
        "(fn { let p: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; p.x + p.y })()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn nested_records_construct_and_project() {
    check_run(
        "",
        r#"(fn { let a: struct { b: struct { c: usize } } = struct { b = struct { c = 5 } }; a.b.c })()"#,
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn records_const_evaluate() {
    check_const(
        r#"
static p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };
static sum = p.x + p.y;
"#,
        expect![[r#"
            p = { x = 1, y = 2 }
            sum = 3
        "#]],
    );
}

#[test]
fn shorthand_fields_evaluate() {
    check_run(
        "",
        r#"(fn { let x: usize = 5; let y: usize = 6; let p = struct { x, y }; p.x + p.y })()"#,
        expect![[r#"
            => 11
        "#]],
    );
}

#[test]
fn record_equality_is_structural_both_ways() {
    check_run(
        "",
        r#"(fn { let one: usize = 1; struct { x = one } == struct { x = one } })()"#,
        expect![[r#"
            => true
        "#]],
    );
    check_run(
        "",
        r#"(fn { let one: usize = 1; let two: usize = 2; struct { x = one } == struct { x = two } })()"#,
        expect![[r#"
            => false
        "#]],
    );
    check_run(
        "",
        r#"(fn { let one: usize = 1; let two: usize = 2; struct { x = one } != struct { x = two } })()"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn field_access_on_a_nonexistent_field_still_traps_at_runtime() {
    // Records have a real MIR/eval story now, but a field inference
    // rejected is exactly as trapped as before records had an eval story
    // (the earlier message, unchanged).
    check_run(
        "",
        r#"(fn { let p: struct { x: usize } = struct { x = 1 }; p.y })()"#,
        expect![[r#"
            error[Trap]: no field `y` on `struct { x: usize }`
        "#]],
    );
}

#[test]
fn named_type_construction_erases_to_its_record() {
    // Full erasure: `Foo(v)` is `v` at runtime — the const value of a
    // Foo-typed item is a plain record value. Forcing the `type` item
    // itself (this helper forces every item) yields a Trap-kind error,
    // the kind `ide` never surfaces as a diagnostic: a type has no value,
    // and reads of `Foo` already trap with their own type-not-a-value
    // message.
    check_const(
        r#"
type Foo = struct { x: usize, y: str };
static p = Foo(struct { x = 1, y = "s" });
static x = p.x;
"#,
        expect![[r#"
            p = { x = 1, y = "s" }
            x = 1
        "#]],
    );
}

#[test]
fn named_type_equality_is_structural_under_the_hood() {
    // The type system keeps `Foo` and bare records apart; between two
    // `Foo`s, equality is the underlying records' structural equality.
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x = 1 }) == Foo(struct { x = 1 }))"#,
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn named_type_inequality_observes_field_values() {
    check_run(
        "type Foo = struct { x: usize };",
        r#"(Foo(struct { x = 1 }) == Foo(struct { x = 2 }))"#,
        expect![[r#"
            => false
        "#]],
    );
}

#[test]
fn variant_values_are_tag_free_payloads() {
    // Round-trip through a fn demanding the variant: the value stays the
    // bare payload tuple (equal to a freshly constructed one — no hidden
    // tag could sneak in), and the run's result renders with no enum, no
    // variant, no tag in sight.
    check_run(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static through = fn (c: Shape::Circle) -> Shape::Circle { c };
static main = fn -> Shape::Pair {
    if through(Shape::Circle(3)) == Shape::Circle(3) {
        print("round-tripped intact");
    };
    Shape::Pair(1, "a")
};
"#,
        "main()",
        expect![[r#"
            output: "round-tripped intact"
            => (1, "a")
        "#]],
    );
}

#[test]
fn variant_and_widened_values_const_evaluate() {
    // Tag-free carriers for every arity, and the tag appearing exactly at
    // the widening edge (the `Shape` annotations).
    check_const(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static circle = Shape::Circle(3);
static pair = Shape::Pair(1, "a");
static point = Shape::Point;
static widened_circle: Shape = Shape::Circle(3);
static widened_point: Shape = Shape::Point;
"#,
        expect![[r#"
            circle = (3)
            pair = (1, "a")
            point = ()
            widened_circle = Shape::Circle(3)
            widened_point = Shape::Point
        "#]],
    );
}

#[test]
fn widened_values_compare_by_tag_and_payload() {
    check_const(
        r#"
type Shape = enum { Circle(usize), Point };
static a: Shape = Shape::Point;
static b: Shape = Shape::Point;
static c: Shape = Shape::Circle(1);
static same = a == b;
static different = a == c;
"#,
        expect![[r#"
            a = Shape::Point
            b = Shape::Point
            c = Shape::Circle(1)
            same = true
            different = false
        "#]],
    );
}

#[test]
fn first_class_constructor_runs() {
    // Called at runtime: a first-class constructor is an ordinary fn
    // *value*, so const contexts reject calling it (the conservative
    // value-call rule, same as any fn value).
    check_run(
        r#"
type Shape = enum { Circle(usize) };
static make: fn(usize) -> Shape::Circle = Shape::Circle;
static main = fn -> bool { make(3) == Shape::Circle(3) };
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn const_context_construction_and_widening() {
    // Constructors are const-legal; the widening conversion is too.
    check_const_blocks(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn {
    let a = const { Shape::Circle(2) };
    let b: Shape = const { Shape::Point };
};
"#,
        expect![[r#"
            f#0 = (2)
            f#1 = Shape::Point
        "#]],
    );
}

#[test]
fn match_dispatches_on_each_variant() {
    check_run(
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static describe = fn (s: Shape) -> str {
    match s {
        ::Circle(r) => "circle",
        ::Pair(n, text) => text,
        ::Point => "point",
    }
};
static main = fn {
    print(describe(Shape::Circle(3)));
    print(describe(Shape::Pair(1, "pair")));
    print(describe(Shape::Point));
};
"#,
        "main()",
        expect![[r#"
            output: "circlepairpoint"
            => ()
        "#]],
    );
}

#[test]
fn match_extracts_payloads_positionally() {
    check_run(
        r#"
type Shape = enum { Pair(usize, usize) };
static sum = fn (s: Shape) -> usize {
    match s {
        ::Pair(a, b) => a + b,
    }
};
"#,
        "sum(Shape::Pair(30, 12))",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn nonexhaustive_match_traps_with_the_diagnostic_message() {
    // Reaching the uncovered variant crashes with exactly the text the
    // squiggle shows; the covered variant still runs fine.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
    }
};
static main = fn {
    print("covered arm runs");
    f(Shape::Circle(1));
    f(Shape::Point);
};
"#,
        "main()",
        expect![[r#"
            output: "covered arm runs"
            error[Trap]: this `match` does not cover `Shape::Point`
        "#]],
    );
}

#[test]
fn match_on_widened_value_round_trips() {
    // Construct tag-free, widen at the `let mut`, dispatch on the injected
    // tag — the full round trip.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn () -> usize {
    let mut s = Shape::Point;
    s = Shape::Circle(42);
    match s {
        ::Circle(r) => r,
        ::Point => 0,
    }
};
"#,
        "f()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn match_in_const_context_evaluates() {
    check_const(
        r#"
type Shape = enum { Circle(usize), Point };
static pick = const fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r,
        ::Point => 7,
    }
};
static a = pick(Shape::Circle(3));
static b = pick(Shape::Point);
static c = const { match Shape::Circle(9) { ::Circle(r) => r, ::Point => 0 } };
"#,
        expect![[r#"
            pick = fn
            a = 3
            b = 7
            c = 9
        "#]],
    );
}

#[test]
fn variant_typed_match_runs_the_state_machine_end_to_end() {
    // Construct `Running(5)`, pass it to a function taking the *variant*
    // type, match inside (no dispatch — see the MIR snapshot), extract the
    // payload.
    check_run(
        r#"
type State = enum { Idle, Running(usize) };
static tick = fn (s: State::Running) -> usize {
    match s {
        ::Running(n) => n + 1,
        ::Idle => 0,
    }
};
"#,
        "tick(State::Running(5))",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn match_binding_arm_receives_the_whole_scrutinee() {
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> Shape {
    match s {
        whole => whole,
    }
};
"#,
        "f(Shape::Circle(8))",
        expect![[r#"
            => Shape::Circle(8)
        "#]],
    );
}

#[test]
fn bind_arm_named_like_payload_variant_binds_the_whole_value() {
    // G25's motivating scenario, end to end: `Circle` (bare, no `::`) is a
    // binding named like a payload-carrying variant. It used to require a
    // `PatArity` error under reinterpretation; now it just binds the whole
    // tagged value — no error, no payload extraction — and the value
    // round-trips as the original variant instance.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
static f = fn (s: Shape) -> Shape {
    match s {
        Circle => Circle,
    }
};
"#,
        "f(Shape::Circle(8))",
        expect![[r#"
            => Shape::Circle(8)
        "#]],
    );
}

#[test]
fn accumulator_loop_runs() {
    // The mutability motivation: a mutable accumulator stepped by a loop.
    check_run(
        r#"
static sum = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
"#,
        "sum()",
        expect![[r#"
            => 45
        "#]],
    );
}

#[test]
fn accumulator_loop_const_evaluates() {
    // The same accumulator inside a `const fn`, forced at compile time.
    check_const(
        r#"
static sum = const fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
static answer = sum();
"#,
        expect![[r#"
            sum = fn
            answer = 45
        "#]],
    );
}

#[test]
fn bare_break_carries_unit() {
    check_run(
        r#"
static f = fn () {
    let mut i: usize = 0;
    loop {
        if i == 3 { break; };
        i = i + 1;
    }
};
"#,
        "f()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn continue_skips_work() {
    // Sum 0..10 skipping the even numbers: 1 + 3 + 5 + 7 + 9 = 25.
    check_run(
        r#"
static is_even = fn (n: usize) -> bool { (n / 2) * 2 == n };
static sum_odd = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        i = i + 1;
        if is_even(i - 1) { continue; };
        acc = acc + (i - 1);
    }
};
"#,
        "sum_odd()",
        expect![[r#"
            => 25
        "#]],
    );
}

#[test]
fn nested_loops_inner_break_stays_inner() {
    // 3 * 4: the inner loop finishes per outer iteration — its break never
    // exits the outer loop.
    check_run(
        r#"
static grid = fn () -> usize {
    let mut total: usize = 0;
    let mut row: usize = 0;
    loop {
        if row == 3 { break total; };
        let mut col: usize = 0;
        total = total + loop {
            if col == 4 { break col; };
            col = col + 1;
        };
        row = row + 1;
    }
};
"#,
        "grid()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn break_in_nested_if_exits_the_loop() {
    check_run(
        r#"
static first_over = fn (limit: usize) -> usize {
    let mut n = 0;
    loop {
        if limit < n {
            if true { break n * 10; };
        };
        n = n + 1;
    }
};
"#,
        "first_over(2)",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn infinite_loop_in_an_initializer_runs_out_of_fuel() {
    // Loops are const-legal; the machine's existing fuel budget is what
    // bounds a runaway compile-time loop.
    check_const(
        "static spin = loop { };",
        expect![[r#"
            spin = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

#[test]
fn infinite_loop_in_a_const_block_runs_out_of_fuel() {
    check_const_blocks(
        "static f = fn { const { loop { } } };",
        expect![[r#"
            f#0 = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

// ---- `return` end to end ----------------------------------------------

#[test]
fn return_skips_the_statements_after_it() {
    // The observable proof that the early exit really happens: "after"
    // never prints.
    check_run(
        r#"
static f = fn (c: bool) -> usize {
    print("before ");
    if c { return 1; };
    print("after ");
    2
};
"#,
        "f(true)",
        expect![[r#"
            output: "before "
            => 1
        "#]],
    );
}

#[test]
fn without_the_early_exit_the_rest_still_runs() {
    // The same function down the other edge — the contrast that makes the
    // test above mean something.
    check_run(
        r#"
static f = fn (c: bool) -> usize {
    print("before ");
    if c { return 1; };
    print("after ");
    2
};
"#,
        "f(false)",
        expect![[r#"
            output: "before after "
            => 2
        "#]],
    );
}

#[test]
fn return_leaves_a_loop_and_the_function_together() {
    // A `break` would only leave the loop and fall into `999`; `return`
    // leaves both.
    check_run(
        r#"
static first_over = fn (limit: usize) -> usize {
    let mut n = 0;
    loop {
        if limit < n { return n * 10; };
        n = n + 1;
    };
    999
};
"#,
        "first_over(2)",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn bare_return_evaluates_to_unit() {
    check_run(
        r#"
static f = fn (c: bool) -> () {
    if c { return; };
    print("not taken");
};
"#,
        "f(true)",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn return_in_a_nested_fn_literal_only_exits_that_literal() {
    // `inner` returns early; the OUTER function carries on and prints
    // after the call — the semantics people get wrong, run for real.
    check_run(
        r#"
static f = fn () -> usize {
    let inner = fn (n: usize) -> usize {
        if n == 0 { return 100; };
        n
    };
    let a = inner(0);
    print("outer still running ");
    a + 1
};
"#,
        "f()",
        expect![[r#"
            output: "outer still running "
            => 101
        "#]],
    );
}

#[test]
fn return_in_a_const_fn_evaluates_at_compile_time() {
    check_const(
        r#"
static clamped = const fn (n: usize) -> usize {
    if n > 10 { return 10; };
    n
};
static x: usize = clamped(42);
static y: usize = clamped(3);
"#,
        expect![[r#"
            clamped = fn
            x = 10
            y = 3
        "#]],
    );
}

#[test]
fn return_in_a_const_block_traps_at_the_return() {
    // The reserve's runtime half: the block is still lowered and still
    // evaluated — deferred errors, like every other refused construct —
    // and the execution that REACHES the `return` traps there with the
    // checker's own message. The old behavior (this block evaluating to
    // `42`) must not survive anywhere.
    check_const_blocks(
        "static f = fn { let x: usize = const { if true { return 42; }; 0 }; };",
        expect![[r#"
            f#0 = error[Trap]: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
        "#]],
    );
}

#[test]
fn return_in_a_fn_literal_inside_a_const_block_still_runs() {
    // The other side of the boundary: the literal is its own body, so its
    // `return` is an ordinary early exit and the block evaluates fine.
    check_const_blocks(
        "static f = fn { let x: usize = const { let inner = fn () -> usize { return 7; }; 9 }; };",
        expect![[r#"
            f#0 = 9
        "#]],
    );
}

// ---- record destructuring end to end ----

#[test]
fn let_record_destructure_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x, y }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; x + y })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn let_record_destructure_rename_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x as a, y as b }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; a + b })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn let_record_destructure_rest_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { x, .. }: struct { x: usize, y: usize, z: usize } = struct { x = 1, y = 2, z = 3 }; x })()"#,
        expect![[r#"
            => 1
        "#]],
    );
}

#[test]
fn param_record_destructure_evaluates() {
    check_run(
        "static add = fn (struct { x, y }: struct { x: usize, y: usize }) -> usize { x + y };",
        "add(struct { x = 4, y = 5 })",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn newtype_destructure_evaluates() {
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static add = fn (Point(struct { x, y })) -> usize { x + y };
"#,
        "add(Point(struct { x = 4, y = 5 }))",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn record_destructure_in_const_context_evaluates() {
    check_const(
        r#"
static p: struct { x: usize, y: usize } = struct { x = 3, y = 4 };
static sum = const fn () -> usize {
    let struct { x, y } = p;
    x + y
}();
"#,
        expect![[r#"
            p = { x = 3, y = 4 }
            sum = 7
        "#]],
    );
}

#[test]
fn per_binding_mut_record_destructure_evaluates() {
    check_run(
        "",
        r#"(fn { let struct { mut x, y }: struct { x: usize, y: usize } = struct { x = 1, y = 2 }; x = x + y; x })()"#,
        expect![[r#"
            => 3
        "#]],
    );
}

// ---- generics: instances actually run ----

#[test]
fn generic_const_fn_instantiates_and_const_evaluates() {
    // The arc's accumulator test: a `const fn` with a const param,
    // instantiated in an initializer, EVALUATES at check time — the value
    // of `N` rides the instance (`FnValue.const_args`), resolved by the
    // frame executing the one shared MIR body.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<3>(14);",
        expect![[r#"
            rep = fn
            y = 42
        "#]],
    );
}

#[test]
fn generic_plain_fn_runs_in_run_mode() {
    // A PLAIN generic fn called as runtime code: the entry initializer is
    // the runner's one const-context escape, so the call proceeds at
    // const depth 0 like any non-const call.
    check_run(
        "static rep = fn::<const N: usize>(x: usize) -> usize { x * N };",
        "rep::<3>(14)",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn type_param_generic_runs_end_to_end() {
    // Type args need nothing at runtime (TR06): the explicit and the
    // inferred mention run the same item value.
    check_run(
        "static id = fn::<T>(x: T) -> T { x };",
        "id::<usize>(4)",
        expect![[r#"
            => 4
        "#]],
    );
    check_run(
        "static id = fn::<T>(x: T) -> T { x };",
        r#"id("s")"#,
        expect![[r#"
            => "s"
        "#]],
    );
}

#[test]
fn mixed_binder_instantiates() {
    // `fn::<T, const N: usize>`: the type param claims no runtime slot —
    // `N` is dense const index 0 even though its binder index is 1.
    check_run(
        "static tag = fn::<T, const N: usize>(x: T) -> usize { N };",
        r#"tag::<str, 7>("s")"#,
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn const_param_driven_recursion_terminates() {
    // Recursion in a generic fn: each level re-instantiates the scheme
    // (`count::<const N>` forwards the frame's own value), the runtime
    // argument does the counting — well within the fuel budget.
    check_const(
        "static count = const fn::<const N: usize>(x: usize) -> usize { if x < N { count::<const N>(x + 1) } else { x } };\nstatic y = count::<3>(0);",
        expect![[r#"
            count = fn
            y = 3
        "#]],
    );
}

#[test]
fn const_arg_may_reference_a_const_item() {
    // TR06's ruled spelling: a non-literal const argument is written with
    // the `const` prefix — `rep::<const LEN>` reads the const item.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nconst LEN: usize = 3;\nstatic y = rep::<const LEN>(14);",
        expect![[r#"
            rep = fn
            LEN = 3
            y = 42
        "#]],
    );
}

#[test]
fn generic_calling_generic_forwards_the_const_param() {
    // The key composition case: `rep2`'s const argument `const N` is a
    // compile-time body reading the ENCLOSING frame's const param —
    // forced per instance at the mention, passing the value through to
    // `rep`'s instance.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic rep2 = const fn::<const N: usize>(x: usize) -> usize { rep::<const N>(x) };\nstatic y = rep2::<3>(14);",
        expect![[r#"
            rep = fn
            rep2 = fn
            y = 42
        "#]],
    );
}

#[test]
fn braced_const_arg_evaluates_to_a_value() {
    // Behavior floor: a `const { ... }` const argument is a compile-time
    // body that reaches evaluation and produces the right `Value` — here the
    // block computes `N = 40 + 2`, and the instance runs `x * N`.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<const { 40 + 2 }>(2);",
        expect![[r#"
            rep = fn
            y = 84
        "#]],
    );
}

#[test]
fn const_block_in_a_generic_body_is_per_instance() {
    // The same `const { … }` body under two instantiations is two values:
    // the machine's compile-time memo is keyed by `Instance` (item + const
    // args), not by body alone.
    check_const(
        "static f = const fn::<const N: usize>() -> usize { const { N + 1 } };\nstatic a = f::<1>();\nstatic b = f::<2>();",
        expect![[r#"
            f = fn
            a = 2
            b = 3
        "#]],
    );
}

#[test]
fn const_block_reading_a_const_param_is_uninstantiated_at_check_time() {
    // Forced standalone (no instance), a compile-time body inside a
    // generic item reports the distinguished `Uninstantiated` kind — the
    // diagnostics layer skips it (the value simply isn't knowable
    // pre-instantiation, TR06); instantiated executions never produce it.
    check_const_blocks(
        "static f = const fn::<const N: usize>() -> usize { const { N } };",
        expect![[r#"
            f#0 = error[Uninstantiated]: the value of a const parameter is not known before instantiation
        "#]],
    );
}

#[test]
fn const_arg_panic_fails_the_instantiating_item() {
    // A panicking const argument fails the mention's evaluation — the
    // check-time surface (`const_arg_values`, exercised in the ide loop)
    // and the item's own forcing report the same origin.
    check_const(
        "static rep = const fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic y = rep::<const { panic(\"nope\") }>(14);",
        expect![[r#"
            rep = fn
            y = error[Panic]: nope
        "#]],
    );
}

#[test]
fn nested_fn_literal_inherits_the_const_env() {
    // A plain fn literal nested in a generic body reads the binder's
    // const param: the fn value captures the frame's const args at
    // construction, so the value survives being returned and called from
    // non-generic code.
    check_run(
        "static make = fn::<const N: usize>() -> fn() -> usize { fn () -> usize { N } };",
        "make::<9>()()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn generic_frame_shows_const_params_as_named_locals() {
    // The debugger surface: a frame executing a generic instance lists
    // the binder's const params ahead of its locals.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static rep = fn::<const N: usize>(x: usize) -> usize { x * N };\nstatic entry = (rep::<3>(14));".to_owned(),
    );
    let entry = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entry")
        .expect("entry item");
    let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
    machine
        .start(&hir::item_loc(&db, entry))
        .expect("entry starts");
    while machine.frames().len() < 2 {
        match machine.step().expect("no crash before the call") {
            crate::StepEvent::Progress => {}
            crate::StepEvent::Done(_) => panic!("finished without entering `rep`"),
        }
    }
    let locals = machine.frame_named_locals(1);
    assert_eq!(locals[0].0, "N");
    assert_eq!(locals[0].2, crate::Value::Int(hir::IntValue::Usize(3)));
    assert!(
        locals.iter().any(|(name, _, _)| name == "x"),
        "the ordinary param is still listed: {locals:?}"
    );
}

// ---- generic type declarations ----

#[test]
fn generic_record_constructs_and_projects() {
    check_run(
        "type Pair = struct::<T> { a: T, b: T };\n\\\n         static main = fn () -> usize { let p = Pair::<usize>(struct { a = 1, b = 2 }); p.a + p.b };",
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn generic_enum_matches_and_widens() {
    check_run(
        "type Option = enum::<T> { Some(T), None };\n\
         static unwrap_or = fn (o: Option::<usize>, d: usize) -> usize {\n\
             match o { ::Some(x) => x, ::None => d, }\n\
         };\n\
         static main = fn () -> usize {\n\
             let mut o = Option::<usize>::Some(3);\n\
             let first = unwrap_or(o, 0);\n\
             o = Option::<usize>::None;\n\
             first + unwrap_or(o, 10)\n\
         };",
        "main()",
        expect![[r#"
            => 13
        "#]],
    );
}

#[test]
fn generic_variant_value_renders() {
    check_run(
        "type Option = enum::<T> { Some(T), None };\n\
         static main = fn () -> Option::<usize> { Option::Some(3) };",
        "main()",
        expect![[r#"
            => Option::Some(3)
        "#]],
    );
}

#[test]
fn const_param_type_constructs_and_evaluates() {
    check_run(
        "type Buf = struct::<const N: usize> { len: usize };\n\\\n         static main = fn () -> usize { let b: Buf::<8> = Buf::<8>(struct { len = 3 }); b.len };",
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn write_through_raw_mut_is_visible_through_the_local() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    unsafe { p.* = 42; }
    x
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn pointer_to_a_field_reads_and_writes_that_element() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw mut;
    unsafe { pa.* = 10; }
    r.a + r.b
};
"#,
        "main()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn interior_pointer_survives_whole_value_overwrite() {
    // Overwriting the whole record writes INTO the allocation, so an
    // interior pointer minted before the overwrite sees the new field —
    // exactly real-memory behavior.
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw mut;
    r = struct { a = 3, b = 4 };
    unsafe { pa.* }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn pointee_field_reads_chain() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    unsafe { p.*.a + p.*.b }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn pointer_copies_alias_the_same_place() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    let q = p;
    unsafe { q.* = 9; p.* }
};
"#,
        "main()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn two_addr_of_the_same_static_are_the_same_address() {
    check_run(
        r#"
static s: usize = 7;
static main = fn() -> bool {
    let a = s.&raw;
    let b = s.&raw;
    a == b
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn deref_of_a_static_pointer_reads_the_static() {
    check_run(
        r#"
static s: struct { a: usize, b: usize } = struct { a = 40, b = 2 };
static main = fn() -> usize {
    let pa = s.a.&raw;
    let pb = s.b.&raw;
    unsafe { pa.* + pb.* }
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn addr_of_a_const_takes_the_address_of_each_use_copy() {
    // const=copied: the interpreter happens to give each `c.&raw` mention
    // its own temporary, so two of them compare unequal. Const-mention
    // identity is deliberately unspecified — this pins today's interpreter
    // behaviour, not a language promise.
    check_run(
        r#"
const c: usize = 7;
static main = fn() -> bool { c.&raw == c.&raw };
"#,
        "main()",
        expect![[r#"
            => false
        "#]],
    );
}

#[test]
fn two_addr_of_the_same_local_are_equal() {
    check_run(
        r#"
static main = fn() -> bool {
    let mut x: usize = 1;
    x.&raw mut == x.&raw mut
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn dangling_deref_after_frame_return_is_detected_ub() {
    check_run(
        r#"
static make = fn() -> usize.&raw mut {
    let mut x = 5;
    x.&raw mut
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn dangling_deref_traps_deterministically() {
    // Same program, two fresh machines: identical trap kind and message.
    let text = r#"
static make = fn() -> usize.&raw mut { let mut x = 5; x.&raw mut };
static main = fn() -> usize { let p = make(); unsafe { p.* } };
static entrypoint = (main());
"#;
    let render = || {
        let db = RootDatabase::default();
        let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
        let entry_item = *hir::file_item_ids(&db, file)
            .iter()
            .find(|&&it| it.name(&db) == "entrypoint")
            .expect("entrypoint item exists");
        let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
        match machine.eval_root(&hir::item_loc(&db, entry_item)) {
            Ok(value) => format!("=> {}", value.display()),
            Err(err) => format!("error[{:?}]: {}", err.kind, err.message),
        }
    };
    let first = render();
    let second = render();
    assert_eq!(first, second);
    assert!(
        first.starts_with("error[UndefinedBehavior]"),
        "expected detected UB, got: {first}"
    );
}

#[test]
fn pointers_work_inside_const_evaluation() {
    // Unsafe (and pointers) are legal in const contexts: the whole
    // computation runs at compile time, the pointer dies inside it.
    check_const(
        r#"
static v = {
    let mut x: usize = 1;
    let p = x.&raw mut;
    unsafe { p.* = 41; }
    x + 1
};
"#,
        expect![[r#"
            v = 42
        "#]],
    );
}

#[test]
fn a_pointer_cannot_leave_const_evaluation() {
    check_const(
        r#"
static p = {
    let mut x: usize = 1;
    x.&raw mut
};
"#,
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn a_pointer_inside_a_record_cannot_leave_const_evaluation_either() {
    check_const(
        r#"
static p = {
    let mut x: usize = 1;
    struct { ptr = x.&raw mut }
};
"#,
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn a_pointer_cannot_leave_a_const_block() {
    check_const_blocks(
        r#"
static main = fn() -> usize {
    const { let mut y: usize = 1; let p = y.&raw mut; unsafe { p.* } }
};
static bad = fn() {
    const { let mut y: usize = 1; y.&raw mut };
};
"#,
        expect![[r#"
            main#0 = 1
            bad#0 = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn deref_display_is_opaque_never_a_number() {
    check_run(
        r#"
static main = fn() -> usize.&raw {
    let mut x = 1;
    x.&raw
};
"#,
        "main()",
        expect![[r#"
            => &raw <opaque>
        "#]],
    );
}

// ---- raw pointers, through-pointer places ----

#[test]
fn through_pointer_field_write_is_visible_afterward() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    unsafe { p.*.a = 40; }
    r.a + r.b
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn through_pointer_field_chain_writes_the_nested_field() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r = struct { inner = struct { v = 1 }, other = 2 };
    let p = r.&raw mut;
    unsafe { p.*.inner.v = 40; }
    r.inner.v + r.other
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn through_pointer_element_reads_and_writes() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2, 3];
    let p = a.&raw mut;
    unsafe { p.*[1] = 9; }
    unsafe { a[0] + p.*[1] }
};
"#,
        "main()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn through_pointer_element_write_out_of_bounds_is_an_ordinary_trap() {
    // The `[i]` step is the program's own checked indexing — an ordinary
    // runtime trap with the same message as `a[i]`, symmetric with the
    // `p.*[i]` read. (Pointers MINTED out of bounds are the UB case; a
    // compile-time-known OOB index would be the planted squiggle trap
    // instead — same message either way.)
    check_run(
        r#"
static main = fn() {
    let mut a: [usize; 2] = [1, 2];
    let p = a.&raw mut;
    let i: usize = 5;
    unsafe { p.*[i] = 0; }
};
"#,
        "main()",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 5
        "#]],
    );
}

#[test]
fn chained_deref_writes_through_a_pointer_to_a_pointer() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut x = 1;
    let mut p = x.&raw mut;
    let pp = p.&raw mut;
    unsafe { pp.*.* = 7; }
    x
};
"#,
        "main()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn deref_in_the_middle_of_a_write_chain_composes() {
    // `p.*.q.*.v = 5;` — the OUTERMOST deref governs the store; the inner
    // one is an ordinary read that fetches the interior pointer.
    check_run(
        r#"
static main = fn() -> usize {
    let mut inner = struct { v = 1 };
    let mut outer = struct { q = inner.&raw mut };
    let p = outer.&raw mut;
    unsafe { p.*.q.*.v = 5; }
    inner.v
};
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn through_pointer_mixed_chain_with_elements_and_fields() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { buf: [struct { v: usize }; 2] } = struct { buf = [struct { v = 1 }, struct { v = 2 }] };
    let p = r.&raw mut;
    unsafe { p.*.buf[1].v = 9; }
    unsafe { p.*.buf[0].v + p.*.buf[1].v }
};
"#,
        "main()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn addr_of_array_element_writes_through_to_the_array() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2, 3];
    let p = a[1].&raw mut;
    unsafe { p.* = 20; }
    a[0] + a[1] + a[2]
};
"#,
        "main()",
        expect![[r#"
            => 24
        "#]],
    );
}

#[test]
fn addr_of_out_of_bounds_element_mints_silently_and_derefs_as_ub() {
    // Address-taking never bounds-checks (validity is a deref-time
    // judgement): the out-of-range address mints fine, the deref is
    // detected UB.
    check_run(
        r#"
static main = fn() -> usize {
    let mut a = [1, 2];
    let i = 5;
    let p = a[i].&raw mut;
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn out_of_bounds_pointer_traps_deterministically() {
    // Same program, two fresh machines: identical trap kind and message —
    // the determinism pin for the new UB wording.
    let text = r#"
static main = fn() -> usize {
    let mut a = [1, 2];
    let i = 5;
    let p = a[i].&raw mut;
    unsafe { p.* = 9; a[0] }
};
static entrypoint = (main());
"#;
    let render = || {
        let db = RootDatabase::default();
        let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
        let entry_item = *hir::file_item_ids(&db, file)
            .iter()
            .find(|&&it| it.name(&db) == "entrypoint")
            .expect("entrypoint item exists");
        let mut machine = Machine::new(&db, RunMode::without_stdin(Vec::new()));
        match machine.eval_root(&hir::item_loc(&db, entry_item)) {
            Ok(value) => format!("=> {}", value.display()),
            Err(err) => format!("error[{:?}]: {}", err.kind, err.message),
        }
    };
    let first = render();
    let second = render();
    assert_eq!(first, second);
    assert!(
        first.starts_with("error[UndefinedBehavior]: out-of-bounds pointer"),
        "expected detected UB, got: {first}"
    );
}

#[test]
fn addr_of_through_a_deref_is_double_indirection_free() {
    // `p.*.a.&raw mut` carries the ORIGINAL allocation's identity with an
    // extended path — no intermediate materialization, so it is the same
    // address `r.a.&raw mut` mints.
    check_run(
        r#"
static main = fn() -> bool {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    unsafe { p.*.a.&raw mut == r.a.&raw mut }
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn addr_of_through_a_deref_writes_the_original_place() {
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    let q = unsafe { p.*.a.&raw mut };
    unsafe { q.* = 40; }
    r.a + r.b
};
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn dangling_interior_element_pointer_is_detected_ub() {
    // Interior pointers die with their frame's allocation — the liveness
    // story covers extended paths with no extra machinery.
    check_run(
        r#"
static make = fn() -> usize.&raw mut {
    let mut a = [1, 2];
    a[0].&raw mut
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn dangling_pointer_from_addr_of_through_deref_is_detected_ub() {
    check_run(
        r#"
static make = fn() -> usize.&raw mut {
    let mut r: struct { a: usize } = struct { a = 1 };
    let p = r.&raw mut;
    unsafe { p.*.a.&raw mut }
};
static main = fn() -> usize {
    let p = make();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn interior_pointer_survives_through_pointer_whole_value_overwrite() {
    // Direct overwrites (`r = ...`) pinned this first; the same
    // invariant holds when the overwrite itself goes through a pointer:
    // `p.* = ...` writes INTO the allocation, so the interior pointer
    // sees the new field.
    check_run(
        r#"
static main = fn() -> usize {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let pa = r.a.&raw mut;
    let p = r.&raw mut;
    unsafe { p.* = struct { a = 3, b = 4 }; }
    unsafe { pa.* }
};
"#,
        "main()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn through_pointer_write_into_a_static_is_rejected_at_the_flavor() {
    // A static only hands out shared `.&raw`, so the write is refused
    // statically (the squiggle's message, re-fired as a trap) — extended
    // paths do not open a route around the read-only allocation.
    check_run(
        r#"
static s: struct { a: usize } = struct { a = 1 };
static main = fn() {
    let p = s.&raw;
    unsafe { p.*.a = 2; }
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: cannot assign through `struct { a: usize }.&raw`: writing needs a `.&raw mut` pointer
        "#]],
    );
}

#[test]
fn through_pointer_field_write_outside_unsafe_traps() {
    check_run(
        r#"
static main = fn() {
    let mut r: struct { a: usize } = struct { a = 1 };
    let p = r.&raw mut;
    p.*.a = 2;
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn mid_chain_deref_outside_unsafe_traps() {
    // The INNER deref of `pp.*.* = 7;` is an ordinary read — it needs
    // `unsafe` like every deref site, even when the outer one is the
    // store.
    check_run(
        r#"
static main = fn() {
    let mut x: usize = 1;
    let mut p = x.&raw mut;
    let pp = p.&raw mut;
    pp.*.* = 7;
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: dereferencing a raw pointer requires an `unsafe { ... }` block
        "#]],
    );
}

#[test]
fn through_pointer_writes_work_inside_const_evaluation() {
    check_const(
        r#"
static v = {
    let mut r: struct { a: usize, b: usize } = struct { a = 1, b = 2 };
    let p = r.&raw mut;
    unsafe { p.*.a = 40; }
    r.a + r.b
};
"#,
        expect![[r#"
            v = 42
        "#]],
    );
}

// ---- fixed-size arrays ----

#[test]
fn arrays_build_read_and_write() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [1, 2, 3];
    a[0] = 10;
    let m: [[usize; 2]; 2] = [[1, 2], [3, 4]];
    a[0] + a[2] + m[1][0]
};
"#,
        "main()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn arrays_const_evaluate_and_freeze_into_statics() {
    check_const(
        r#"
static table = const {
    let mut t: [usize; 4] = [0; 4];
    t[0] = 1;
    t[1] = 2;
    t[3] = t[0] + t[1];
    t
};
static row: struct { name: str, cells: [usize; 3] } = struct { name = "row", cells = [1, 2, 3] };
static grid: [struct { x: usize }; 2] = [struct { x = 1 }, struct { x = 2 }];
"#,
        expect![[r#"
            table = [1, 2, 0, 3]
            row = { cells = [1, 2, 3], name = "row" }
            grid = [{ x = 1 }, { x = 2 }]
        "#]],
    );
}

#[test]
fn compile_time_out_of_bounds_traps_with_the_squiggle_text() {
    // The trap MIR planted for the compile-time-known OOB — the exact text
    // of the editor squiggle (single render), reached at runtime.
    check_run(
        r#"
static main = fn () -> usize {
    let a = [1, 2];
    a[2]
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn runtime_out_of_bounds_read_traps_deterministically() {
    check_run(
        r#"
static get = fn (a: [usize; 2], i: usize) -> usize { a[i] };
static main = fn () -> usize { get([1, 2], 5) };
"#,
        "main()",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 5
        "#]],
    );
}

#[test]
fn runtime_out_of_bounds_write_traps() {
    check_run(
        r#"
static set = fn (i: usize) -> usize {
    let mut a = [1, 2];
    a[i] = 9;
    a[0]
};
"#,
        "set(2)",
        expect![[r#"
            error[Runtime]: index out of bounds: the length is 2 but the index is 2
        "#]],
    );
}

#[test]
fn array_repeat_with_const_param_length() {
    check_run(
        r#"
static rep = const fn::<const N: usize>(v: usize) -> [usize; N] { [v; N] };
static main = fn () -> usize {
    let a = rep::<3>(7);
    a[0] + a[1] + a[2]
};
"#,
        "main()",
        expect![[r#"
            => 21
        "#]],
    );
}

#[test]
fn generic_buffer_type_with_const_length_runs() {
    check_run(
        r#"
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
static first = fn (b: Buf::<2>) -> usize { b.data[0] + b.len };
static main = fn () -> usize { first(Buf::<2>(struct { data = [40, 1], len = 2 })) };
"#,
        "main()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn mixed_variant_array_elements_dispatch_through_match() {
    check_run(
        r#"
type Shape = enum { Point, Circle(usize) };
static area = fn (s: Shape) -> usize {
    match s {
        ::Circle(r) => r * r,
        ::Point => 0,
    }
};
static main = fn () -> usize {
    let shapes = [Shape::Circle(3), Shape::Point];
    area(shapes[0]) + area(shapes[1])
};
"#,
        "main()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn arrays_of_records_mutate_in_place() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut pts: [struct { x: usize, y: usize }; 2] = [struct { x = 1, y = 2 }, struct { x = 3, y = 4 }];
    pts[1].x = 30;
    pts[1].x + pts[0].y
};
"#,
        "main()",
        expect![[r#"
            => 32
        "#]],
    );
}

#[test]
fn huge_const_array_repeat_runs_out_of_fuel() {
    check_const(
        "static big: [usize; 4_000_000_000] = [0; 4_000_000_000];",
        expect![[r#"
            big = error[NotConst]: constant evaluation ran out of fuel
        "#]],
    );
}

#[test]
fn empty_array_and_equality() {
    check_run(
        r#"
static main = fn () -> bool {
    let a: [usize; 0] = [];
    let b: [usize; 0] = [];
    let c: [usize; 2] = [1, 2];
    let d: [usize; 2] = [1, 2];
    a == b == (c == d)
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

// ---- the heap builtins --------------------------------------------------

#[test]
fn heap_alloc_write_read_roundtrip() {
    // The result-shaped alloc: run mode never produces `Err` (the
    // interpreter cannot meaningfully OOM), so the `Ok` arm is the one
    // that runs — but the match is mandatory (the call's TYPE is the
    // result enum).
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(3) {
        AllocResult::Ok(p) => {
            unsafe {
                p.* = 10;
                let p1 = add(p, 1);
                p1.* = 20;
                let p2 = add(p, 2);
                p2.* = 30;
            };
            let sum = unsafe { p.* + add(p, 1).* + add(p, 2).* };
            unsafe { dealloc_array(p, 3); };
            sum
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 60
        "#]],
    );
}

#[test]
fn heap_alloc_result_is_a_tagged_enum_value() {
    // What the builtin actually returns: a tagged `AllocResult` value —
    // displayable, matchable, ordinary.
    check_run(
        r#"
static main = fn () -> AllocResult::<usize> { alloc_array::<usize>(1) };
"#,
        "main()",
        expect![[r#"
            => AllocResult::Ok(&raw <opaque>)
        "#]],
    );
}

#[test]
fn heap_uninit_read_is_detected_ub() {
    // Fresh elements are tracked-uninit (A04): reading one before its
    // first write is detected UB, not a zero.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { p.* },
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn heap_uninit_read_through_aggregate_copy_is_detected_ub() {
    // The read gate covers AGGREGATE copies too: `copy` plants poison in
    // a local array's element (silently — that is `copy`'s license), and
    // then reading the WHOLE array as a value trips the same UB.
    check_run(
        r#"
static main = fn () -> bool {
    let mut a = [1, 2];
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => {
            unsafe { copy(p, a[0].&raw mut, 2) };
            let b = a;
            b == b
        }
        AllocResult::Err => false,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn heap_write_initializes_and_partial_writes_track_per_element() {
    // Per-ELEMENT tracking: writing element 0 makes element 0 readable;
    // element 1 stays poison until its own write.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => {
            unsafe { p.* = 5; };
            let first = unsafe { p.* };
            unsafe { dealloc_array(p, 2); };
            first
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn heap_double_free_is_detected_ub_with_allocation_origin() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe {
            dealloc_array(p, 2);
            dealloc_array(p, 2);
        },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: double free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_use_after_free_is_detected_ub_with_allocation_origin() {
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(1) {
        AllocResult::Ok(p) => {
            unsafe { p.* = 3; };
            unsafe { dealloc_array(p, 1); };
            unsafe { p.* }
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_count_mismatch_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { dealloc_array(p, 3) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` with the wrong element count — this allocation has 2 element(s), but 3 were passed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_of_interior_pointer_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { dealloc_array(add(p, 1), 2) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` of a pointer that is not the head of its allocation — it points inside it
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_dealloc_of_local_is_detected_ub() {
    // A promoted local is memory, but not HEAP memory: freeing it is a
    // category error whatever its liveness.
    check_run(
        r#"
static main = fn () -> () {
    let mut x: usize = 4;
    unsafe { dealloc_array(x.&raw mut, 1) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `dealloc_array` of a pointer that does not point to a heap allocation
        "#]],
    );
}

#[test]
fn heap_zero_element_alloc_is_a_defined_trap() {
    // A05: allocators are NOT required to handle zero-size
    // requests — `alloc_array(0)` is a defined trap (never UB),
    // restrictive now and loosenable later. Corollary: no zero-size
    // allocation can exist, so `dealloc_array` never legally sees `n == 0`.
    check_run(
        r#"
static main = fn () -> () {
    match alloc_array::<usize>(0) {
        AllocResult::Ok(p) => unsafe { dealloc_array(p, 0) },
        AllocResult::Err => (),
    }
};
"#,
        "main()",
        expect![[r#"
            error[Runtime]: cannot allocate zero elements: zero-size allocation support is reserved
        "#]],
    );
}

#[test]
fn dangling_deref_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> usize {
    let p = dangling::<usize>();
    unsafe { p.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — this pointer was never valid
        "#]],
    );
}

#[test]
fn dangling_pointers_compare_equal() {
    // One reserved allocation per machine: every `dangling()` is the same
    // address.
    check_run(
        r#"
static main = fn () -> bool {
    dangling::<usize>() == dangling::<usize>()
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn add_stays_within_allocation_and_oob_deref_is_detected_ub() {
    // Minting past the end is silent (validity is a deref-time
    // judgement, same as `a[i].&raw mut`); the deref is where it traps.
    check_run(
        r#"
static main = fn () -> usize {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => unsafe { add(p, 5).* },
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn add_works_on_local_array_element_pointers() {
    // `add` is not heap-only: any pointer addressing an array element
    // supports it — the stack-buffer story.
    check_run(
        r#"
static main = fn () -> usize {
    let mut a = [10, 20, 30];
    let p = a[0].&raw;
    unsafe { add(p, 2).* }
};
"#,
        "main()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn add_zero_is_identity_on_any_pointer() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut x = 7;
    let p = x.&raw mut;
    unsafe { add(p, 0).* }
};
"#,
        "main()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn add_of_non_element_pointer_is_detected_ub() {
    // The one shape the abstract machine cannot represent: advancing a
    // pointer that doesn't address an array element (a lone local).
    check_run(
        r#"
static main = fn () -> usize {
    let mut x = 7;
    let p = x.&raw mut;
    unsafe { add(p, 1).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `add` of a pointer that does not address an array element
        "#]],
    );
}

#[test]
fn copy_moves_elements_between_allocations() {
    check_run(
        r#"
static main = fn () -> usize {
    let a = [1, 2, 3];
    match alloc_array::<usize>(3) {
        AllocResult::Ok(p) => {
            unsafe { copy(a[0].&raw, p, 3) };
            let sum = unsafe { p.* + add(p, 1).* + add(p, 2).* };
            unsafe { dealloc_array(p, 3) };
            sum
        }
        AllocResult::Err => 0,
    }
};
"#,
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn copy_overlap_is_defined_memmove_semantics() {
    // Overlapping forward copy: the source range is read out in full
    // before the destination is written — `[1,2,3,4,5]` shifted right by
    // one is `[1,1,2,3,4]`, never the memcpy smear `[1,1,1,1,1]`.
    check_run(
        r#"
static main = fn () -> bool {
    let mut a: [usize; 5] = [1, 2, 3, 4, 5];
    let p = a[0].&raw mut;
    unsafe { copy(p, add(p, 1), 4) };
    a == [1, 1, 2, 3, 4]
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn copy_propagates_uninit_silently() {
    // `copy` is the ONE mover licensed to transport poison: copying a
    // partially initialized buffer works; only a later value-read of the
    // still-uninit element traps.
    check_run(
        r#"
static main = fn () -> usize {
    let p = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let q = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = 9; };
    unsafe { copy(p, q, 2) };
    print("copy of a half-written buffer did not trap");
    let ok = unsafe { q.* };
    print("the initialized element arrived");
    unsafe { add(q, 1).* }
};
"#,
        "main()",
        expect![[r#"
            output: "copy of a half-written buffer did not trapthe initialized element arrived"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

#[test]
fn copy_out_of_bounds_source_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a: [usize; 2] = [1, 2];
    let mut b: [usize; 3] = [0, 0, 0];
    unsafe { copy(a[0].&raw, b[0].&raw mut, 3) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `copy` out of bounds — the source names 3 element(s) from index 0, but the array has 2
        "#]],
    );
}

#[test]
fn copy_out_of_bounds_destination_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a: [usize; 3] = [1, 2, 3];
    let mut b: [usize; 2] = [0, 0];
    unsafe { copy(a[0].&raw, b[0].&raw mut, 3) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `copy` out of bounds — the destination names 3 element(s) from index 0, but the array has 2
        "#]],
    );
}

#[test]
fn copy_of_zero_elements_is_legal_through_any_pointer() {
    // Rust's rule, adopted: zero-length copies are valid through any
    // pointer, `dangling()` included — what lets a growing container copy
    // its 0 elements out of the never-allocated buffer.
    check_run(
        r#"
static main = fn () -> () {
    let mut a = [1];
    unsafe { copy(dangling::<usize>(), a[0].&raw mut, 0) };
    unsafe { copy(a[0].&raw, dangling::<usize>(), 0) };
};
"#,
        "main()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn copy_of_zero_elements_is_legal_through_a_pointer_to_a_scalar() {
    // The zero-length shortcut must hold for a pointer whose path does not
    // address an array element at all, not only for `dangling()` (which
    // happens to be shaped as element 0 of a registered, never-live
    // array) and array-element pointers: `write_range`'s effect must not
    // run when there is nothing to store, or this panics the interpreter
    // instead of answering the copy's rule.
    check_run(
        r#"
static main = fn () -> () {
    let mut a: usize = 1;
    let mut b: usize = 2;
    unsafe { copy(a.&raw, b.&raw mut, 0) };
    unsafe { copy(dangling::<usize>(), a.&raw mut, 0) };
};
"#,
        "main()",
        expect![[r#"
            => ()
        "#]],
    );
}

#[test]
fn copy_into_freed_allocation_is_detected_ub() {
    check_run(
        r#"
static main = fn () -> () {
    let a = [1, 2];
    let p = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { dealloc_array(p, 2) };
    unsafe { copy(a[0].&raw, p, 2) };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_alloc_is_refused_in_const_contexts() {
    // The eager const fence (C04): const-check plants the editor's exact
    // message; the machine's own `NotConst` refusal stands behind it as
    // defense in depth.
    check_const(
        r#"
static x = match alloc_array::<usize>(1) {
    AllocResult::Ok(p) => 1,
    AllocResult::Err => 2,
};
"#,
        expect![[r#"
            x = error[Trap]: cannot allocate during compile-time evaluation: const-built heap values wait for an interning design
        "#]],
    );
}

#[test]
fn heap_dealloc_is_refused_in_const_contexts() {
    check_const(
        r#"
static x = const fn () -> () { unsafe { dealloc_array(dangling::<usize>(), 1) } };
static y: usize = { x(); 1 };
"#,
        expect![[r#"
            x = fn
            y = error[Trap]: cannot deallocate during compile-time evaluation: const-built heap values wait for an interning design
        "#]],
    );
}

#[test]
fn add_copy_and_dangling_are_const_legal_on_locals() {
    // NOT fenced (they allocate nothing): pointer arithmetic and copies
    // over const-local storage work at compile time — the pointer-escape
    // rule stays the backstop for anything trying to leave.
    check_const(
        r#"
static x: usize = const {
    let mut a = [1, 2, 3];
    let p = a[0].&raw mut;
    unsafe { copy(p, add(p, 1), 2); };
    let d = dangling::<usize>();
    unsafe { add(p, 1).* }
};
"#,
        expect![[r#"
            x = 1
        "#]],
    );
}

#[test]
fn heap_pointer_cannot_escape_const_evaluation() {
    // The shipped escape rule, untouched, as the backstop: even if a heap
    // pointer were built during const eval (here: `dangling`, which is
    // not fenced), it may not reach the memoized result.
    check_const(
        "static p = dangling::<usize>();",
        expect![[r#"
            p = error[NotConst]: a pointer cannot leave compile-time evaluation
        "#]],
    );
}

#[test]
fn heapvec_push_growth_get_and_deinit_roundtrip() {
    // The proof program's core, as a machine test: growth (alloc + copy +
    // dealloc old) preserves elements across reallocation, checked get
    // reads through `add`, and one deinit frees the one live buffer.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_new = fn::<T>() -> HeapVec::<T> {
    HeapVec::<T>(struct { ptr = dangling::<T>(), len = 0, cap = 0 })
};
static heapvec_push = fn::<T>(mut v: HeapVec::<T>, x: T) -> HeapVec::<T> {
    if v.len == v.cap {
        let new_cap = if v.cap == 0 { 4 } else { v.cap * 2 };
        let fresh = match alloc_array::<T>(new_cap) {
            AllocResult::Ok(p) => p,
            AllocResult::Err => panic("heapvec_push: out of memory"),
        };
        unsafe {
            copy(v.ptr, fresh, v.len);
            if v.cap != 0 {
                dealloc_array(v.ptr, v.cap);
            };
        };
        v.ptr = fresh;
        v.cap = new_cap;
    };
    let slot = unsafe { add(v.ptr, v.len) };
    unsafe { slot.* = x; };
    v.len = v.len + 1;
    v
};
static heapvec_get = fn::<T>(v: HeapVec::<T>, i: usize) -> T {
    if i < v.len {
        unsafe { add(v.ptr, i).* }
    } else {
        panic("index out of bounds")
    }
};
static heapvec_deinit = fn::<T>(v: HeapVec::<T>) -> () {
    if v.cap != 0 {
        unsafe { dealloc_array(v.ptr, v.cap); };
    };
};
static main = fn () -> usize {
    let mut v = heapvec_new::<usize>();
    let mut i = 0;
    loop {
        if i == 6 { break; };
        v = heapvec_push(v, i * i);
        i = i + 1;
    };
    let picked = heapvec_get(v, 5) + heapvec_get(v, 1);
    heapvec_deinit(v);
    picked
};
"#,
        "main()",
        expect![[r#"
            => 26
        "#]],
    );
}

#[test]
fn heapvec_get_out_of_bounds_panics_like_the_future_index_desugar() {
    // `heapvec_get` is written as EXACTLY the future `a[i]` desugar
    // (check + panic + add + deref) — past `len` it panics, an
    // ordinary trap, never UB.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_get = fn::<T>(v: HeapVec::<T>, i: usize) -> T {
    if i < v.len {
        unsafe { add(v.ptr, i).* }
    } else {
        panic("index out of bounds")
    }
};
static main = fn () -> usize {
    let p = match alloc_array::<usize>(4) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = 1; };
    let v = HeapVec::<usize>(struct { ptr = p, len = 1, cap = 4 });
    heapvec_get(v, 3)
};
"#,
        "main()",
        expect![[r#"
            error[Panic]: index out of bounds
        "#]],
    );
}

#[test]
fn heapvec_double_deinit_is_detected_double_free() {
    // The v1 handle posture, honestly: copying the handle copies nothing
    // but the pointer, so a second deinit is a double free — detected,
    // deterministically, with the allocation's birth site.
    check_run(
        r#"
type HeapVec = struct::<T> { ptr: T.&raw mut, len: usize, cap: usize };
static heapvec_deinit = fn::<T>(v: HeapVec::<T>) -> () {
    if v.cap != 0 {
        unsafe { dealloc_array(v.ptr, v.cap); };
    };
};
static main = fn () -> () {
    let p = match alloc_array::<usize>(4) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let v = HeapVec::<usize>(struct { ptr = p, len = 0, cap = 4 });
    let w = v;
    heapvec_deinit(v);
    heapvec_deinit(w);
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: double free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn arena_carves_and_exhaustion_produces_err() {
    // The typed arena: ONE backing allocation carved by a bump cursor,
    // and a REAL `Err` producer for the result-shaped interface —
    // exhaustion is a value, not a trap.
    check_run(
        r#"
type ArenaState = struct::<T> { base: T.&raw mut, cap: usize, cursor: usize };
type Arena = struct::<T> { state: ArenaState::<T>.&raw mut };
static arena_new = fn::<T>(cap: usize) -> Arena::<T> {
    let state = match alloc_array::<ArenaState::<T>>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("arena_new: out of memory"),
    };
    let base = match alloc_array::<T>(cap) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("arena_new: out of memory"),
    };
    unsafe { state.* = ArenaState::<T>(struct { base = base, cap = cap, cursor = 0 }); };
    Arena::<T>(struct { state = state })
};
static arena_alloc = fn::<T>(a: Arena::<T>, n: usize) -> AllocResult::<T> {
    let cap = unsafe { a.state.*.cap };
    let cursor = unsafe { a.state.*.cursor };
    if cap - cursor < n {
        AllocResult::<T>::Err
    } else {
        unsafe {
            let p = add(a.state.*.base, cursor);
            a.state.*.cursor = cursor + n;
            AllocResult::<T>::Ok(p)
        }
    }
};
static arena_deinit = fn::<T>(a: Arena::<T>) -> () {
    unsafe {
        dealloc_array(a.state.*.base, a.state.*.cap);
        dealloc_array(a.state, 1);
    };
};
static main = fn () -> () {
    let a = arena_new::<usize>(4);
    match arena_alloc(a, 3) {
        AllocResult::Ok(p) => {
            let last = unsafe { add(p, 2) };
            unsafe { last.* = 7; };
            print("carved 3 of 4");
        }
        AllocResult::Err => print("unexpected exhaustion"),
    };
    match arena_alloc::<usize>(a, 2) {
        AllocResult::Ok(p) => print("unexpected fit"),
        AllocResult::Err => print("exhausted: Err, by value"),
    };
    match arena_alloc(a, 1) {
        AllocResult::Ok(p) => print("the last element still fits"),
        AllocResult::Err => print("unexpected exhaustion"),
    };
    arena_deinit(a);
};
"#,
        "main()",
        expect![[r#"
            output: "carved 3 of 4exhausted: Err, by valuethe last element still fits"
            => ()
        "#]],
    );
}

#[test]
fn arena_use_after_deinit_is_detected_ub() {
    // Carved pointers are paths into the ONE backing allocation, so they
    // all die together at `arena_deinit` — with the backing's birth site
    // in the note.
    check_run(
        r#"
static main = fn () -> usize {
    let base = match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    let carved = unsafe { add(base, 1) };
    unsafe { dealloc_array(base, 2) };
    unsafe { carved.* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: use after free — this allocation was already freed
              note: allocated here
        "#]],
    );
}

#[test]
fn heap_allocations_of_records_project_like_any_memory() {
    // Heap elements are typed values: records allocated behind a pointer
    // support field writes through paths, exactly like promoted locals.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> usize {
    let p = match alloc_array::<Point>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.* = Point(struct { x = 1, y = 2 }); };
    unsafe { p.*.y = 40; };
    let got = unsafe { p.*.x + p.*.y };
    unsafe { dealloc_array(p, 1) };
    got
};
"#,
        "main()",
        expect![[r#"
            => 41
        "#]],
    );
}

#[test]
fn heap_field_write_through_uninit_element_is_detected_ub() {
    // Projecting INTO a never-written element is UB even as a write
    // target: the element's structure doesn't exist yet — write the whole
    // element first.
    check_run(
        r#"
type Point = struct { x: usize, y: usize };
static main = fn () -> () {
    let p = match alloc_array::<Point>(1) {
        AllocResult::Ok(p) => p,
        AllocResult::Err => panic("oom"),
    };
    unsafe { p.*.x = 1; };
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: read of uninitialized memory — this element was never written
        "#]],
    );
}

// --- Integer types: typed arithmetic, overflow traps, unary minus, offset ---

#[test]
fn typed_arithmetic_overflow_traps_name_the_operation_and_type() {
    // Const contexts get the eager error (div-by-zero precedent); the
    // message names the operation and the type.
    check_const(
        r#"
static add8: u8 = 250 + 10;
static sub8: u8 = 3 - 5;
static mul8: u8 = 16 * 16;
static ok8: u8 = 250 + 5;
"#,
        expect![[r#"
            add8 = error[Runtime]: arithmetic overflow: `250 + 10` does not fit in `u8`
            sub8 = error[Runtime]: arithmetic overflow: `3 - 5` does not fit in `u8`
            mul8 = error[Runtime]: arithmetic overflow: `16 * 16` does not fit in `u8`
            ok8 = 255
        "#]],
    );
}

#[test]
fn signed_overflow_edges_trap() {
    check_const(
        r#"
static div_min: i8 = -128 / -1;
static neg_min: i8 = const { let m: i8 = -128; -m };
static ok: i8 = -128 / 1;
"#,
        expect![[r#"
            div_min = error[Runtime]: arithmetic overflow: `-128 / -1` does not fit in `i8`
            neg_min = error[Runtime]: arithmetic overflow: `-(-128)` does not fit in `i8`
            ok = -128
        "#]],
    );
}

#[test]
fn arithmetic_overflow_traps_at_runtime_too() {
    check_run(
        r#"
static bump = fn (n: u8) -> u8 { n + 10 };
"#,
        "bump(250)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `250 + 10` does not fit in `u8`
        "#]],
    );
}

#[test]
fn unary_minus_evaluates_on_signed_and_traps_on_unsigned_nonzero() {
    check_run(
        r#"
static negate = fn (n: i32) -> i32 { -n };
"#,
        "negate(41) + 83",
        expect![[r#"
            => 42
        "#]],
    );
    check_run(
        r#"
static negate = fn (n: u8) -> u8 { -n };
"#,
        "negate(0)",
        expect![[r#"
            => 0
        "#]],
    );
    check_run(
        r#"
static negate = fn (n: u8) -> u8 { -n };
"#,
        "negate(1)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `-1` does not fit in `u8`
        "#]],
    );
}

#[test]
fn i64_min_negation_traps() {
    check_run(
        r#"
static negate = fn (n: i64) -> i64 { -n };
static min: i64 = -9223372036854775808;
"#,
        "negate(min)",
        expect![[r#"
            error[Runtime]: arithmetic overflow: `-(-9223372036854775808)` does not fit in `i64`
        "#]],
    );
}

#[test]
fn each_width_wraps_its_own_range() {
    // The same value overflows a narrow type and fits a wide one.
    check_const(
        r#"
static narrow: i16 = 300 * 300;
static wide: i32 = 300 * 300;
"#,
        expect![[r#"
            narrow = error[Runtime]: arithmetic overflow: `300 * 300` does not fit in `i16`
            wide = 90000
        "#]],
    );
}

#[test]
fn offset_moves_both_directions() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [10, 20, 30];
    let p = a[1].&raw mut;
    unsafe {
        let forward = offset(p, 1);
        let back = offset(p, -1);
        forward.* + back.*
    }
};
"#,
        "main()",
        expect![[r#"
            => 40
        "#]],
    );
}

#[test]
fn offset_below_the_start_is_detected_ub_at_the_call() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 3] = [10, 20, 30];
    let p = a[1].&raw mut;
    unsafe { offset(p, -2).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: `offset` result points below the start of the allocation
        "#]],
    );
}

#[test]
fn offset_past_the_end_mints_silently_and_derefs_as_ub_like_add() {
    check_run(
        r#"
static main = fn () -> usize {
    let mut a: [usize; 2] = [1, 2];
    let p = a[0].&raw mut;
    unsafe { offset(p, 5).* }
};
"#,
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: out-of-bounds pointer — it points to element 5 of an array with 2 elements
        "#]],
    );
}

#[test]
fn offset_by_zero_is_the_identity() {
    check_run(
        r#"
static main = fn () -> bool {
    let mut x: u8 = 7;
    let p = x.&raw mut;
    unsafe { offset(p, 0) == p }
};
"#,
        "main()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn typed_values_freeze_into_statics_per_width() {
    check_const(
        r#"
static a: u8 = 200;
static b: i8 = -100;
static c: isize = -5;
"#,
        expect![[r#"
            a = 200
            b = -100
            c = -5
        "#]],
    );
}

#[test]
fn wrong_kind_builtin_count_is_intercepted_at_the_argument() {
    // Defense-in-depth: the count/index builtins now require the exact
    // checker-pinned kind (`usize` for the alloc/copy/`add` counts, `isize`
    // for `offset`), trapping a wrong kind ill-typed rather than laundering
    // it. That trap is not reachable from source, though: the checker pins
    // each of these argument positions to its kind, so a wrong-kind
    // argument (here a `u8` where `alloc_array` wants `usize`) is
    // value-trapped AT THE ARGUMENT — with the ordinary type-mismatch
    // message — before the builtin ever runs. This test pins that
    // interception (the same reason the sibling `ProjElem::Index`
    // tightening ships without a builtin-arm test): a mistyped builtin
    // count traps cleanly, never launders and never panics the machine.
    check_run(
        r#"
static main = fn () -> () {
    let n: u8 = 3;
    match alloc_array::<usize>(n) {
        AllocResult::Ok(p) => { unsafe { dealloc_array(p, 3); }; }
        AllocResult::Err => {}
    };
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `usize`, found `u8`
        "#]],
    );
}

#[test]
fn mixed_kind_integer_equality_is_intercepted_at_the_operand() {
    // The same no-laundering discipline as the arithmetic ops: the checker
    // requires `==`/`!=` operands to agree, so a mixed-kind pairing (a `u8`
    // against a `usize`) is an ordinary mismatch on the culprit operand, and
    // the machine replays it as a value trap before the operands ever meet.
    // The operator's own same-kind check (the machine arms for `eval_bin_op`
    // equality) stays behind it as defense in depth, unreachable from source
    // — the same reason the sibling builtin-count tightening ships with an
    // interception test and no builtin-arm test.
    check_run(
        r#"
static main = fn () -> bool {
    let x: u8 = 1;
    let y: usize = 1;
    x == y
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `u8`, found `usize`
        "#]],
    );
    check_run(
        r#"
static main = fn () -> bool {
    let x: u8 = 1;
    let y: usize = 1;
    x != y
};
"#,
        "main()",
        expect![[r#"
            error[Trap]: type mismatch: expected `u8`, found `usize`
        "#]],
    );
}

// ---- inherent members and dot-calls -------------------------------------

#[test]
fn dot_call_runs_as_a_direct_call() {
    check_run(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Counter(struct { n = c.n + by }) };
    }
};
"#,
        "Counter(struct { n = 3 }).bump(4).get()",
        expect![[r#"
            => 7
        "#]],
    );
}

// THE TR01 evaluation-order pin: `recv.name(a)` desugars to `name(a, recv)`,
// and arguments evaluate left to right — so the ARGUMENT runs BEFORE the
// receiver expression binds its value. `bump` mutates the cell through a
// raw pointer while the receiver expression reads through the same
// pointer: argument-first order sees the bumped value (99), receiver-first
// would have seen 1. This order is WHY self is the last parameter.
#[test]
fn dot_call_arguments_evaluate_before_the_receiver_binds() {
    check_run(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        plus = fn(extra: usize, c: Self) -> usize { c.v + extra };
    }
};
static bump = fn(p: Cell.&raw mut) -> usize {
    unsafe { p.* = Cell(struct { v = 99 }); };
    0
};
static main = fn() -> usize {
    let mut c = Cell(struct { v = 1 });
    let p = c.&raw mut;
    unsafe { p.* }.plus(bump(p))
};
"#,
        "main()",
        expect![[r#"
            => 99
        "#]],
    );
}

#[test]
fn generic_member_dispatches_at_the_receiver_args() {
    check_run(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { b.v };
        put = fn(x: T, b: Self) -> Self { Box2::<T>(struct { v = x }) };
    }
};
"#,
        r#"Box2(struct { v = "hi" }).put("ho").get()"#,
        expect![[r#"
            => "ho"
        "#]],
    );
}

// A const-generic owner: the member reads the binder's `N`, supplied by
// the RECEIVER's type (the owner's arguments, which a dot-call never
// spells) — and one member forwards it to another through a dot-call on
// `Self`.
#[test]
fn const_generic_member_reads_the_receivers_const_arg() {
    check_run(
        r#"
type Buf = struct::<const N: usize> { used: usize } with {
    impl Self {
        cap = fn(b: Self) -> usize { N };
        free = fn(b: Self) -> usize { b.cap() - b.used };
    }
};
"#,
        "Buf::<8>(struct { used = 3 }).free()",
        expect![[r#"
            => 5
        "#]],
    );
}

#[test]
fn variant_typed_receiver_widens_into_the_member() {
    check_run(
        r#"
type Light = enum { Red, Green } with {
    impl Self {
        flip = fn(l: Self) -> Light {
            match l {
                ::Red => Light::Green,
                ::Green => Light::Red,
            }
        };
    }
};
"#,
        "Light::Red.flip()",
        expect![[r#"
            => Light::Green
        "#]],
    );
}

// A `const fn` member is const-callable; a plain member is rejected in a
// const context with the ordinary const-check story.
#[test]
fn const_fn_members_run_at_compile_time() {
    check_const(
        r#"
type Sq = struct { n: usize } with {
    impl Self {
        area = const fn(s: Self) -> usize { s.n * s.n };
    }
};
static a = const { Sq(struct { n = 5 }).area() };
"#,
        expect![[r#"
            a = 25
        "#]],
    );
}

#[test]
fn plain_member_call_rejected_in_const_context() {
    check_const(
        r#"
type Sq = struct { n: usize } with {
    impl Self {
        area = fn(s: Self) -> usize { s.n * s.n };
    }
};
static a = const { Sq(struct { n = 5 }).area() };
"#,
        expect![[r#"
            a = error[Trap]: cannot call `area` in a const context; marking it `const fn` would allow this
        "#]],
    );
}

// SEPARATE NAMESPACES (G13): call syntax runs the MEMBER, bare
// access reads the FIELD — the getter idiom end to end at runtime.
#[test]
fn member_shadowing_a_field_dispatches_by_syntax() {
    check_run(
        r#"
type Vecish = struct { len: usize } with {
    impl Self {
        len = fn(v: Self) -> usize { v.len + 10 };
    }
};
"#,
        "Vecish(struct { len = 3 }).len() + Vecish(struct { len = 3 }).len",
        expect![[r#"
            => 16
        "#]],
    );
}

// ---- trait declarations: dictionary dispatch ----------------------------

#[test]
fn trait_dispatch_through_generic_fn() {
    // One bounded generic fn, three implementers: trait-side impls (str,
    // usize) and a type-side impl (Point) behave identically — the
    // dictionary resolves per call site at the instantiation edge.
    check_run(
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
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("num") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self {
            print(s);
            Sink(struct { pushes = w.pushes + 1 })
        };
    }
};
type Point = struct { x: usize, y: usize } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, p: Self) -> W {
            let w = Display::fmt(w, p.x);
            let w = w.push(",");
            Display::fmt(w, p.y)
        };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn() -> usize {
    let a = show::<str>("hi");
    let b = show::<usize>(7);
    let c = show::<Point>(Point(struct { x = 1, y = 2 }));
    a + b + c
};
"#,
        "main()",
        expect![[r#"
            output: "hinumnum,num"
            => 5
        "#]],
    );
}

#[test]
fn dictionary_forwarding_through_recursion() {
    // `fmt_usize` recurses (forwarding its own dictionary) and is called
    // from an impl member (forwarding the member's binder dictionary).
    check_run(
        r#"
trait Write = requires { push: fn(s: str, w: Self) -> Self; };
trait Display = requires { fmt: fn::<W: Write>(w: W, x: Self) -> W; } with {
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { fmt_usize(w, x) };
    }
};
static digit = fn(d: usize) -> str {
    if d == 0 { "0" } else if d == 1 { "1" } else if d == 2 { "2" }
    else if d == 3 { "3" } else if d == 4 { "4" } else { "5+" }
};
static fmt_usize = fn::<W: Write>(w: W, n: usize) -> W {
    if n < 10 {
        w.push(digit(n))
    } else {
        let w = fmt_usize(w, n / 10);
        w.push(digit(n - (n / 10) * 10))
    }
};
type Out = struct { c: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self { print(s); Out(struct { c = w.c + 1 }) };
    }
};
static main = fn() -> usize {
    let n: usize = 431;
    let o = n.fmt(Out(struct { c = 0 }));
    o.c
};
"#,
        "main()",
        expect![[r#"
            output: "431"
            => 3
        "#]],
    );
}

#[test]
fn qualified_short_form_dispatches() {
    check_run(
        r#"
trait D = requires { m: fn(x: Self) -> str; } with {
    impl usize { m = fn(x: usize) -> str { "int" }; }
    impl str { m = fn(x: str) -> str { x }; }
};
static main = fn() -> () {
    let n: usize = 3;
    print(D::m(n));
    print(D::m("qq"));
    print(n.m());
};
"#,
        "main()",
        expect![[r#"
            output: "intqqint"
            => ()
        "#]],
    );
}

#[test]
fn collision_escapes_each_run_to_their_own_member() {
    // G13: every escape the ambiguity error names must actually reach the
    // thing it names — inherent, trait impl, fn-typed field.
    check_run(
        r#"
trait D = requires { m: fn(x: Self) -> str; };
type P = struct { m: fn() -> str } with {
    impl Self { m = fn(x: Self) -> str { "inherent" }; }
    impl D { m = fn(x: Self) -> str { "trait" }; }
};
static main = fn() -> () {
    let p = P(struct { m = fn() -> str { "field" } });
    print(P::m(p));
    print(D::m(p));
    print(D::<Self = P>::m(p));
    print((p.m)());
};
"#,
        "main()",
        expect![[r#"
            output: "inherenttraittraitfield"
            => ()
        "#]],
    );
}

#[test]
fn collision_call_traps_with_the_ambiguity_message() {
    // Deferred-error mode: the refused call carries the squiggle's text.
    check_run(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { v: usize } with {
    impl Self { m = fn(x: Self) -> usize { 1 }; }
    impl D { m = fn(x: Self) -> usize { 2 }; }
};
static main = fn() -> usize { P(struct { v = 0 }).m() };
"#,
        "main()",
        expect![[r#"
            error[Trap]: `m` is ambiguous on `P`: it could be the inherent member (`P::m(value)`) or `D`'s member (`D::m(value)`) — spell the one you mean
        "#]],
    );
}

#[test]
fn qualified_inherent_member_runs_as_a_plain_fn_value() {
    // `Type::member` is an ordinary fn value: callable directly, bindable,
    // and instantiated at the TYPE's arguments when the owner is generic.
    check_run(
        r#"
type Pair = struct::<T> { a: T, b: T } with {
    impl Self { first = fn(p: Self) -> T { p.a }; }
};
type P = struct { v: usize } with {
    impl Self { len = fn(p: Self) -> usize { p.v }; }
};
static main = fn() -> usize {
    let p = P(struct { v = 7 });
    let f = P::len;
    let q = Pair::<usize>(struct { a = 5, b = 6 });
    f(p) + P::len(p) + Pair::<usize>::first(q)
};
"#,
        "main()",
        expect![[r#"
            => 19
        "#]],
    );
}

#[test]
fn a_member_own_type_param_runs_at_every_spelling() {
    // The member-own TYPE binder end to end: inferred at the call,
    // spelled on the dot, and spelled on the qualified path — the same
    // instantiation, three ways, all of them running.
    check_run(
        r#"
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        unwrap = const fn(s: Self) -> T {
            match s { ::Some(t) => t, ::None => panic("none") }
        }
        flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self) -> Option::<U> {
            match s {
                ::Some(v) => { f(v) }
                ::None => { Option::None }
            }
        }
    }
};
static twice = fn(n: usize) -> Option::<usize> { Option::Some(n + n) };
static label = fn(n: usize) -> Option::<str> { Option::Some("hit") };
static main = fn() -> usize {
    let inferred = Option::Some(3).flat_map(twice).unwrap();
    let dotted = Option::Some(4).flat_map::<usize>(twice).unwrap();
    let pathed = Option::unwrap(Option::flat_map::<usize>(twice, Option::Some(5)));
    // A SECOND instantiation of the same member, at a different `U`: the
    // member's own binder varies per call, which is the whole point.
    print(Option::Some(1).flat_map(label).unwrap());
    inferred + dotted + pathed
};
"#,
        "main()",
        expect![[r#"
            output: "hit"
            => 24
        "#]],
    );
}

#[test]
fn named_self_call_and_value_dispatch() {
    // The full named-Self form states `Self` where nothing infers it, and
    // its member VALUE is one impl's fn.
    check_run(
        r#"
trait D = requires { n: fn(k: usize) -> Self; m: fn(x: Self) -> str; } with {
    impl usize { n = fn(k: usize) -> usize { k + 1 }; m = fn(x: usize) -> str { "int" }; }
    impl str { n = fn(k: usize) -> str { "s" }; m = fn(x: str) -> str { x }; }
};
static generic = fn::<T: D>(x: T) -> str { D::<Self = T>::m(x) };
static main = fn() -> () {
    print(D::<Self = str>::n(1));
    let f = D::<Self = usize>::m;
    print(f(D::<Self = usize>::n(1)));
    print(generic("rigid"));
};
"#,
        "main()",
        expect![[r#"
            output: "sintrigid"
            => ()
        "#]],
    );
}

#[test]
fn unsatisfied_bound_traps_at_runtime() {
    // Deferred-error mode: the broken call traps with the squiggle's text
    // when it actually runs.
    check_run(
        r#"
trait D = requires { m: fn(x: Self) -> usize; };
type P = struct { a: usize };
static f = fn::<T: D>(x: T) -> usize { x.m() };
static main = fn() -> usize { f::<P>(P(struct { a = 1 })) };
"#,
        "main()",
        expect![[r#"
            error[Trap]: the bound `T: D` is not satisfied here: `P` does not implement `D`
        "#]],
    );
}

#[test]
fn trait_impls_on_enums_and_variant_receivers() {
    // A variant-typed receiver reaches its ENUM's trait impl through the
    // sanctioned widening — dot-form and qualified short form alike.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point };
trait D = requires { m: fn(x: Self) -> usize; } with {
    impl Shape {
        m = fn(x: Self) -> usize {
            match x { ::Circle(r) => r, ::Point => 0 }
        };
    }
};
static main = fn() -> () {
    let c = Shape::Circle(3);
    if c.m() == 3 { print("dot ok") } else { print("bad") };
    if D::m(Shape::Point) == 0 { print("qualified ok") } else { print("bad") };
};
"#,
        "main()",
        expect![[r#"
            output: "dot okqualified ok"
            => ()
        "#]],
    );
}

#[test]
fn qualified_variant_self_widens_and_dispatches() {
    // A variant-typed argument at a `Self` position widens to its enum,
    // so the impl's returned tag is real and the match dispatches
    // honestly.
    check_run(
        r#"
type Shape = enum { Circle(usize), Point } with {
    impl Id { id = fn(x: Self) -> Self { Shape::Point }; }
};
trait Id = requires { id: fn(x: Self) -> Self; };
static main = fn() -> () {
    let c = Id::id(Shape::Circle(3));
    match c {
        ::Circle(n) => { let _ = n; print("circle") },
        ::Point => print("point — correct"),
    };
};
"#,
        "main()",
        expect![[r#"
            output: "point — correct"
            => ()
        "#]],
    );
}

#[test]
fn nested_bound_use_traps_with_the_reservation() {
    // A check-time reservation and its deferred trap carry the same
    // text at runtime (never an internal error).
    check_run(
        r#"
trait Size = requires { size: fn(x: Self) -> usize; } with {
    impl usize { size = fn(x: usize) -> usize { x }; }
};
static outer = fn::<T: Size>(x: T) -> usize {
    let f = fn(y: T) -> usize { y.size() };
    f(x)
};
static main = fn() -> usize { outer::<usize>(4) };
"#,
        "main()",
        expect![[r#"
            error[Trap]: code nested inside a bounded fn (a nested fn literal or a `const` block) cannot use the enclosing bounds yet (it would have to capture the dictionary)
        "#]],
    );
}

// ---- safe borrows: the dynamic aliasing check ---------------------------
//
// Static exclusivity is the loan checker's (`mir::loans`); the interpreter
// answers the same question dynamically, as depth. Every SAFE program here
// that traps is one the checker refuses too, and the trap is what stops it
// when it is run regardless; the ones that mint a raw pointer check clean
// (minting is no access, M08) and trap only here — that is the depth the
// fence does not reach. The launch configuration is the ruled one: Tree
// Borrows' structure with NO `Reserved` phase, so `&mut` starts `Unique`.

#[test]
fn a_borrow_reads_and_writes_the_place_it_borrows() {
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize { let mut n: usize = 1; set(n.&mut, 9); n };",
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn two_exclusive_borrows_of_one_place_is_detected_ub() {
    // The exclusive-write violation. Minting `b` is a write through the
    // parent, which disables the sibling `a`; writing through `a`
    // afterwards is the use of an invalidated borrow.
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let a = n.&mut;\n\
             let b = n.&mut;\n\
             set(b, 2);\n\
             set(a, 3);\n\
             n\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn using_a_child_after_writing_through_its_parent_is_detected_ub() {
    // Use-after-parent-invalidated. `child` is a reborrow of `a`; writing
    // through `a` is foreign to `child`, so `child` is disabled — and
    // disabling is transitive, which is what makes one check cover a whole
    // subtree.
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn::<@a>(m: usize.&mut::<@a>) -> () {\n\
             let child = m.*.&mut;\n\
             set(m, 5);\n\
             set(child, 9);\n\
         };\n\
         static g = fn () -> usize { let mut n: usize = 1; f(n.&mut); n };",
        "g()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn many_shared_borrows_of_one_place_are_fine() {
    // The shared regime: any number of readers, no invalidation. A shared
    // creation is a READ through the parent, which freezes rather than
    // disables.
    check_run(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static f = fn () -> usize {\n\
             let n: usize = 4;\n\
             let a = n.&;\n\
             let b = n.&;\n\
             get(a) + get(b)\n\
         };",
        "f()",
        expect![[r#"
            => 8
        "#]],
    );
}

#[test]
fn a_nested_literal_frame_escape_is_caught_dynamically() {
    // A borrow returned at `@_` from a nested literal never reaches a
    // universal of the ENCLOSING item, so the outlives module's escape
    // check has nothing to reject (hir's
    // `a_nested_fn_literals_own_frame_escape_is_not_the_escape_checks_finding`);
    // the loan checker refuses it, because the loan is live where the
    // literal's body returns. Run regardless, the interpreter catches it
    // too: the borrowed local's storage really is gone once the nested
    // literal's own frame returns.
    check_run(
        "static main = fn () -> usize {\n\
             let f = fn () -> usize.&::<@_> { let mut n = 7; n.& };\n\
             f().*\n\
         };",
        "main()",
        expect![[r#"
            error[UndefinedBehavior]: dangling pointer — the local it pointed to no longer exists (its frame has returned)
        "#]],
    );
}

#[test]
fn a_raw_only_program_never_touches_the_aliasing_tree() {
    // Scope, pinned: a raw-only allocation (never covered by a safe
    // borrow) has no root to inherit, so its raw pointers resolve to
    // nothing and never touch the tree. So the aliasing model costs
    // raw-pointer-only programs exactly nothing, and cannot have changed
    // one — which is what makes this an addition rather than a
    // semantics change.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let p = n.&raw mut;\n\
             let q = n.&raw mut;\n\
             unsafe { p.* = 2; q.* = 3; p.* }\n\
         };",
        "f()",
        expect![[r#"
            => 3
        "#]],
    );
}

#[test]
fn disjoint_field_borrows_of_one_struct_coexist() {
    // `p.x.&mut` and `p.y.&mut` are minted straight off the local's own
    // name (no deref): each node's path is exactly its own field, and
    // `paths_overlap` says disjoint fields never interact — so writing
    // through one, then the other, is not exclusivity violation, unlike
    // two borrows of the SAME field (see
    // `two_exclusive_borrows_of_one_place_is_detected_ub`).
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let bx = p.x.&mut;\n\
             let by = p.y.&mut;\n\
             set(bx, 10);\n\
             set(by, 20);\n\
             p.x + p.y\n\
         };",
        "f()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn writing_a_disjoint_sibling_field_does_not_disturb_a_borrow() {
    // The direct-write-by-name route (`write_place`'s promoted-local
    // branch) must carry the WRITE's own path too: `p.y = 5;` writes
    // through the root with path `[y]`, foreign to a borrow of `[x]` only
    // because the paths are disjoint.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let bx = p.x.&mut;\n\
             p.y = 50;\n\
             bx.* = 9;\n\
             p.x + p.y\n\
         };",
        "f()",
        expect![[r#"
            => 59
        "#]],
    );
}

#[test]
fn a_sibling_field_access_through_a_nested_pointer_runs_clean() {
    // The static fence accepts this shape (mir's loan tests: the copy of
    // `bb.*` into the temp `bb.*.*` is spelled through is no access);
    // the tree agrees, `bb.*.*.f` and `bb.*.*.g` being disjoint paths
    // through the same root.
    check_run(
        "type P = struct { f: usize, g: usize };\n\
         static f = fn::<@a, @b>(bb: P.&mut::<@a>.&mut::<@b>) -> usize {\n\
             let x = bb.*.*.g.&mut;\n\
             let y = bb.*.*.f;\n\
             bb.*.*.f = 3;\n\
             x.* = x.* + y;\n\
             x.* + bb.*.*.f\n\
         };\n\
         static main = fn () -> usize {\n\
             let mut p: P = P(struct { f = 1, g = 2 });\n\
             let mut q = p.&mut;\n\
             f(q.&mut)\n\
         };",
        "main()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn two_exclusive_borrows_of_the_same_field_is_still_detected_ub() {
    // Regression for the per-location fix: SAME field, not disjoint ones
    // — `paths_overlap` says `[x]`/`[x]` overlap, so this must still be
    // the ordinary exclusivity violation.
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let a = p.x.&mut;\n\
             let b = p.x.&mut;\n\
             set(b, 2);\n\
             set(a, 3);\n\
             p.x\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_whole_struct_borrow_still_dominates_every_field() {
    // Regression: a borrow of path `[]` (the whole struct) must still be
    // foreign to (and disable, and be disabled by) a borrow of any one
    // field — `paths_overlap` treats the empty path as a prefix of
    // everything.
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let whole = p.&mut;\n\
             let field = p.x.&mut;\n\
             set(field, 9);\n\
             whole.* = struct { x = 0, y = 0 };\n\
             p.x\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn disjoint_array_element_borrows_coexist() {
    check_run(
        "static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn () -> usize {\n\
             let mut a: [usize; 2] = [1, 2];\n\
             let b0 = a[0].&mut;\n\
             let b1 = a[1].&mut;\n\
             set(b0, 10);\n\
             set(b1, 20);\n\
             a[0] + a[1]\n\
         };",
        "f()",
        expect![[r#"
            => 30
        "#]],
    );
}

#[test]
fn reading_one_field_through_a_wide_borrow_does_not_kill_a_sibling_field_child() {
    // The read-side per-location fix: `b` is a WIDE reborrow (path `[]`,
    // the whole struct); reading `b.*.x` must be checked with path `[x]`
    // — the field the read actually touches, extended past the deref —
    // not `b`'s own `[]` path, which would make every read through `b`
    // foreign to every child of `b`, however disjoint. Rust accepts the
    // equivalent (`let c = &mut b.y; let v = b.x; *c = 5;`).
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let b = p.&mut;\n\
             let c = b.*.y.&mut;\n\
             let v = b.*.x;\n\
             c.* = 5;\n\
             v + p.y\n\
         };",
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_one_element_through_a_wide_borrow_does_not_kill_a_sibling_element_child() {
    check_run(
        "static f = fn () -> usize {\n\
             let mut a: [usize; 2] = [1, 2];\n\
             let b = a.&mut;\n\
             let c = b.*[1].&mut;\n\
             let v = b.*[0];\n\
             c.* = 9;\n\
             v + a[1]\n\
         };",
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn reading_a_nested_field_through_a_wide_borrow_does_not_kill_a_sibling_child() {
    // Two derefs deep: `by` is a reborrow of `p.y` (path `[y]`), `cw` a
    // child of `by` over `w` (path `[y, w]`). Reading `by.*.u` must be
    // checked with path `[y, u]` — disjoint from `[y, w]` — not `by`'s
    // own `[y]`.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { y: struct { u: usize, w: usize } } =\n\
                 struct { y = struct { u = 1, w = 2 } };\n\
             let by = p.y.&mut;\n\
             let cw = by.*.w.&mut;\n\
             let v = by.*.u;\n\
             cw.* = 9;\n\
             v + p.y.w\n\
         };",
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn reading_a_sibling_field_by_name_does_not_kill_a_field_child() {
    // The plainest shape in the language: a two-field struct, one field
    // borrowed, the OTHER read by name. `p.y` reads through the
    // allocation's root, so the path it carries has to be `[y]` — which
    // means the read must lower as one projected place
    // (`lower_place_read`), not as a copy of the whole `p` followed by a
    // field extraction. Rust accepts the equivalent.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let bx = p.x.&mut;\n\
             let v = p.y;\n\
             bx.* = 9;\n\
             p.x + p.y + v\n\
         };",
        "f()",
        expect![[r#"
            => 13
        "#]],
    );
}

#[test]
fn reading_a_sibling_element_by_name_does_not_kill_an_element_child() {
    check_run(
        "static f = fn () -> usize {\n\
             let mut a: [usize; 3] = [1, 2, 3];\n\
             let b0 = a[0].&mut;\n\
             let v = a[1];\n\
             b0.* = 9;\n\
             a[0] + v\n\
         };",
        "f()",
        expect![[r#"
            => 11
        "#]],
    );
}

#[test]
fn reading_a_nested_field_by_name_does_not_kill_a_sibling_child() {
    // Two steps deep, no deref anywhere: `p.x.y` must carry `[x, y]`,
    // disjoint from the borrow's `[x, z]`.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: struct { y: usize, z: usize }, w: usize } =\n\
                 struct { x = struct { y = 1, z = 2 }, w = 3 };\n\
             let c = p.x.z.&mut;\n\
             let v = p.x.y;\n\
             c.* = 5;\n\
             v + p.x.z\n\
         };",
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_a_disjoint_top_level_field_does_not_kill_a_deeper_child() {
    // The read is SHALLOWER than the borrow: `p.z` carries `[z]`, the
    // borrow `[a, q]`. Neither is a prefix of the other.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { a: struct { q: usize, r: usize }, z: usize } =\n\
                 struct { a = struct { q = 1, r = 2 }, z = 3 };\n\
             let c = p.a.q.&mut;\n\
             let v = p.z;\n\
             c.* = 9;\n\
             v + p.a.q\n\
         };",
        "f()",
        expect![[r#"
            => 12
        "#]],
    );
}

#[test]
fn reading_a_field_of_an_element_by_name_does_not_kill_its_sibling() {
    // An index step then a field step: `a[0].x` carries `[0, x]`.
    check_run(
        "static f = fn () -> usize {\n\
             let mut a: [struct { x: usize, y: usize }; 2] =\n\
                 [struct { x = 1, y = 2 }, struct { x = 3, y = 4 }];\n\
             let c = a[0].y.&mut;\n\
             let v = a[0].x;\n\
             c.* = 5;\n\
             v + a[0].y\n\
         };",
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_two_steps_past_a_deref_narrows_to_the_field_it_touches() {
    // The deref-rooted twin of the tests above: the chain past `b.*` is
    // TWO steps long, so the path is `[x, y]` — a chain does not stop
    // narrowing one step after the deref.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: struct { y: usize, z: usize }, w: usize } =\n\
                 struct { x = struct { y = 1, z = 2 }, w = 3 };\n\
             let b = p.&mut;\n\
             let c = b.*.x.z.&mut;\n\
             let v = b.*.x.y;\n\
             c.* = 5;\n\
             v + p.x.z\n\
         };",
        "f()",
        expect![[r#"
            => 6
        "#]],
    );
}

#[test]
fn reading_the_same_field_by_name_still_suspends_an_exclusive_borrow() {
    // The other side of the narrowing: `p.x` read by name carries `[x]`,
    // which OVERLAPS the borrow's `[x]` — a shared use of a place with a
    // live exclusive borrow of it, and writing through that borrow
    // afterwards is the ordinary violation.
    check_run(
        "static f = fn () -> usize {\n\
             let mut p: struct { x: usize, y: usize } = struct { x = 1, y = 2 };\n\
             let bx = p.x.&mut;\n\
             let v = p.x;\n\
             bx.* = 9;\n\
             p.x + v\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn a_borrow_behind_a_borrow_reads_and_writes_the_root() {
    // Two borrow steps: the inner `bb.*` is an ordinary pointer LOAD, and
    // the loaded value carries the tag `b.&mut` minted, so the write
    // through it lands on `n`'s own allocation as a CHILD of that node
    // rather than as a stranger to it.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let mut b = n.&mut;\n\
             let bb = b.&mut;\n\
             bb.*.* = 5;\n\
             bb.*.* + n\n\
         };",
        "f()",
        expect![[r#"
            => 10
        "#]],
    );
}

#[test]
fn a_raw_write_past_a_live_safe_borrow_is_detected_ub() {
    // `n.&raw mut` is minted from a bare local's name AFTER a safe borrow
    // already covers `n` — it inherits `n`'s existing root (not a fresh
    // inert node), so the raw write is exactly as foreign to `r` as
    // `n = 9;` would be.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let r = n.&mut;\n\
             let p = n.&raw mut;\n\
             unsafe { p.* = 9; }\n\
             r.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_raw_pointer_minted_before_the_safe_borrow_still_shares_its_root() {
    // The raw pointer is minted FIRST, while `Provenance(None)` still has
    // no root to resolve against; the safe borrow minted after it is what
    // creates the root. The raw write must still resolve lazily, at the
    // moment of the access, against whatever root exists then.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let p = n.&raw mut;\n\
             let r = n.&mut;\n\
             unsafe { p.* = 9; }\n\
             r.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_raw_reborrow_through_a_live_mut_borrow_stays_clean() {
    // A raw reborrow ROUTED THROUGH the safe borrow (`r.*.&raw mut`, not
    // straight off `n`'s name) inherits `r`'s own node rather than the
    // root, per the `.&raw`-is-not-a-decayed-borrow rule — so it does not
    // disturb `r` at all.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let r = n.&mut;\n\
             let p = unsafe { r.*.&raw mut };\n\
             unsafe { p.* = 9; }\n\
             r.*\n\
         };",
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

#[test]
fn copy_through_the_destination_is_foreign_to_a_live_borrow_of_an_element() {
    // `copy` is memmove, not a route around the aliasing tree: a live
    // safe borrow of an element the copy overwrites must be invalidated
    // exactly as `a[0] = 7;` would invalidate it.
    check_run(
        "static f = fn () -> usize {\n\
             let mut src: [usize; 1] = [7];\n\
             let mut a: [usize; 2] = [1, 2];\n\
             let m = a[0].&mut;\n\
             unsafe { copy(src[0].&raw, a[0].&raw mut, 1); }\n\
             m.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn copy_through_the_source_is_foreign_to_a_live_borrow_of_an_element() {
    // The other half of the same claim: the SOURCE range is read, element
    // by element, so a live exclusive borrow of an element the copy reads
    // is suspended exactly as `let v = a[0];` would suspend it.
    check_run(
        "static f = fn () -> usize {\n\
             let mut a: [usize; 2] = [1, 2];\n\
             let mut dst: [usize; 1] = [0];\n\
             let m = a[0].&mut;\n\
             unsafe { copy(a[0].&raw, dst[0].&raw mut, 1); }\n\
             m.* = 5;\n\
             a[0]\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn copy_reports_a_dead_source_tag_before_a_destination_bounds_fault() {
    // The observable half of `builtin_copy`'s "source before destination"
    // order: a source whose OWN TAG is no longer valid — not merely
    // foreign to a still-live borrow, which would only suspend it — faults
    // at the read itself, so it is reported even though the destination
    // (one element, asked to receive two) is also out of bounds.
    check_run(
        "static f = fn () -> u8 {\n\
             let mut a: [u8; 2] = [1, 2];\n\
             let mut b: [u8; 1] = [0];\n\
             let m = a.&mut;\n\
             let q = m.*[0].&raw;\n\
             a[0] = 7;\n\
             unsafe { copy(q, b[0].&raw mut, 2); }\n\
             b[0]\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_borrow_tag_is_not_part_of_pointer_equality() {
    // Two pointers to the same place must compare EQUAL however they were
    // derived — one through a `.&mut` (so it carries that borrow's
    // aliasing node) and one straight off the local (so it carries the
    // untracked root). A tag is aliasing bookkeeping, and letting it reach
    // `==` would turn bookkeeping into an observable program result,
    // which is exactly what an aliasing model must never do.
    check_run(
        "static f = fn () -> bool {\n\
             let mut n: usize = 1;\n\
             let borrowed = n.&mut;\n\
             let via_borrow = borrowed.*.&raw mut;\n\
             let direct = n.&raw mut;\n\
             via_borrow == direct\n\
         };",
        "f()",
        expect![[r#"
            => true
        "#]],
    );
}

#[test]
fn a_direct_write_to_a_borrowed_local_invalidates_the_borrow() {
    // A write to a local BY ITS OWN NAME must reach the aliasing tree
    // exactly as a write through a deref-rooted place does — the most
    // ordinary exclusivity violation in the language, not a special case.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let r = n.&;\n\
             n = 99;\n\
             r.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_direct_read_of_a_borrowed_local_freezes_an_exclusive_borrow() {
    // A foreign read through the root demotes an exclusive child to
    // shared, so a later write through it is caught. And the message
    // must name the STATE that rejected the write (`Frozen`, from a
    // foreign read), not the borrow's own flavor — `m` here IS a
    // `.&mut`, so "write through a shared borrow" would be false of it.
    check_run(
        "static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let m = n.&mut;\n\
             let copy: usize = n;\n\
             m.* = 5;\n\
             copy\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn an_implicit_degradation_mints_a_real_child_node() {
    // M07 says degradation is not spelled `v.*.&` "because the explicit
    // form produces exactly the same child node" — a claim about the IR
    // that an implicit reborrow must actually make true: it needs its own
    // MIR operation, so `s` and `m` mint distinct tags with a real
    // parent/child relation to violate.
    check_run(
        "static get = fn::<@x>(r: usize.&::<@x>) -> usize { r.* };\n\
         static f = fn () -> usize {\n\
             let mut n: usize = 1;\n\
             let m = n.&mut;\n\
             let s: usize.&::<@_> = m;\n\
             m.* = 7;\n\
             get(s)\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn dynamic_ub_names_the_borrow_site_and_the_invalidating_site() {
    // The report must name both interesting sites even when the trap
    // fires inside a callee that has no borrow sites of its own — both
    // the borrow site and the invalidating site live in the caller here.
    check_run(
        "static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };\n\
         static f = fn () -> () {\n\
             let mut n: usize = 1;\n\
             let a = n.&mut;\n\
             let b = n.&mut;\n\
             bump(b);\n\
             bump(a);\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_read_only_borrow_loop_does_not_accumulate_scan_cost() {
    // A read can only demote `Unique`, so it never has to look at a node
    // that is already settled — a read-only borrow loop must not rescan
    // every borrow the program has ever made. Pinned behaviorally: the
    // loop must still produce the right answer with several thousand
    // live shared borrows outstanding.
    check_run(
        "static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };\n\
         static f = fn () -> usize {\n\
             let n: usize = 3;\n\
             let mut acc: usize = 0;\n\
             let mut i: usize = 0;\n\
             loop {\n\
                 acc = acc + get(n.&);\n\
                 i = i + 1;\n\
                 if i == 2000 { break } else { }\n\
             };\n\
             acc\n\
         };",
        "f()",
        expect![[r#"
            => 6000
        "#]],
    );
}

// ---- match projects through borrows: aliasing and the tag read ----------
//
// The static story is in `hir`; what runs here is the two things only the
// interpreter can show. A payload binding really points INTO the matched
// value (so a write through it is visible to its owner), and the tag test
// really is an access through the scrutinee's node (so an invalidated
// scrutinee is caught at the `match`, not at the first arm that uses a
// binding).

#[test]
fn a_mut_payload_binding_aliases_the_matched_value() {
    // THE aliasing proof. `bump` never sees the option by value; its write
    // lands in `main`'s storage, and the owner reads it back. Any answer
    // other than 101 would mean the binding was a copy.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static bump = fn::<@a>(o: Opt::<usize>.&mut::<@a>) -> () {\n\
             match o { ::Some(t) => { t.* = t.* + 100; }, ::None => {} }\n\
         };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             bump(o.&mut);\n\
             match o { ::Some(n) => n, ::None => 0 }\n\
         };",
        "f()",
        expect![[r#"
            => 101
        "#]],
    );
}

#[test]
fn two_mut_payload_bindings_of_one_variant_are_independent() {
    // Two exclusive borrows of two payload SLOTS, live at once. This is
    // only sound — and only accepted — because the aliasing tree is
    // path-granular: minting `y` is a write at the second slot, which the
    // borrow of the first cannot see.
    check_run(
        "type Pair = enum { Both(usize, usize), Neither };\n\
         static go = fn::<@a>(p: Pair.&mut::<@a>) -> () {\n\
             match p { ::Both(x, y) => { x.* = 7; y.* = 9; }, ::Neither => {} }\n\
         };\n\
         static f = fn() -> usize {\n\
             let mut p: Pair = ::Both(1, 2);\n\
             go(p.&mut);\n\
             match p { ::Both(a, b) => a + b, ::Neither => 0 }\n\
         };",
        "f()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn a_containing_borrow_still_conflicts_with_a_payload_borrow() {
    // Paths OVERLAP when one contains the other, so a borrow of the whole
    // option and a borrow of its payload are not disjoint: minting the
    // second is a write the first can see. (`[]` — the allocation root —
    // contains everything, which is what keeps a write by the local's own
    // name reaching every borrow into it.)
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let inner = match o.&mut { ::Some(t) => t, ::None => panic(\"none\") };\n\
             o = Opt::<usize>::Some(5);\n\
             set(inner, 9);\n\
             0\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_tag_read_of_a_shared_borrowed_match_goes_through_the_tree() {
    // The tag test is an access, and this is it firing. `r` was
    // invalidated by a direct write to the local; the `match` itself is
    // the undefined behavior, before any arm body runs. Lowering the tag
    // read as a copy of a detached value — which is what the owned path
    // does — would have let this run clean.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let r = o.&;\n\
             o = Opt::<usize>::None;\n\
             match r { ::Some(_) => 1, ::None => 0 }\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_tag_read_of_an_exclusive_borrowed_match_is_a_shared_freeze() {
    // The `.&mut` flavor, ISOLATED: the second `match` dispatches on the
    // tag but binds nothing (`::None` carries no payload, `_` binds the
    // pointer), so the only access it performs is the tag read.
    //
    // That read goes through `m`, and `inner` — a payload borrow minted
    // under `m` — is foreign to it: a foreign read FREEZES an exclusive
    // node rather than disabling it, so writing through `inner` afterwards
    // is the suspended-by-a-read case, which gets its own message. Reading
    // the tag off a detached copy would have done none of this.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let m = o.&mut;\n\
             let inner = match m { ::Some(t) => t, ::None => panic(\"none\") };\n\
             let tag = match m { ::None => 0, _ => 7 };\n\
             set(inner, 9);\n\
             tag\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn a_borrowed_match_that_dispatches_on_nothing_reads_nothing() {
    // The control for the test above, and the other half of the rule: a
    // `match` whose arms make no tag decision does not read the tag, so it
    // is not an access at all. Same program, with `_` back in place of the
    // two variant arms — `inner` survives it and the write lands.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static set = fn::<@a>(m: usize.&mut::<@a>, v: usize) -> () { m.* = v; };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let m = o.&mut;\n\
             let inner = match m { ::Some(t) => t, ::None => panic(\"none\") };\n\
             let tag = match m { _ => 7 };\n\
             set(inner, 9);\n\
             tag + match o { ::Some(n) => n, ::None => 0 }\n\
         };",
        "f()",
        expect![[r#"
            => 16
        "#]],
    );
}

#[test]
fn a_variant_typed_borrowed_match_writes_through_its_payload() {
    // Tag-free at runtime — no switch, no tag read — but the payload
    // binding is a borrow of the slot just the same.
    check_run(
        "type State = enum { Run(usize), Stop };\n\
         static tick = fn::<@a>(s: State::Run.&mut::<@a>) -> () {\n\
             match s { ::Run(n) => { n.* = n.* + 1; } }\n\
         };\n\
         static f = fn() -> usize {\n\
             let mut s: State::Run = ::Run(41);\n\
             tick(s.&mut);\n\
             match s { ::Run(n) => n }\n\
         };",
        "f()",
        expect![[r#"
            => 42
        "#]],
    );
}

#[test]
fn projecting_twice_reaches_the_inner_payload() {
    // Transitivity, running: the binding is a borrow, so matching IT
    // projects again, and the write lands two levels down in the owner.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static bump = fn::<@a>(o: Opt::<Opt::<usize>>.&mut::<@a>) -> () {\n\
             match o {\n\
                 ::Some(inner) => match inner {\n\
                     ::Some(n) => { n.* = n.* + 1; },\n\
                     ::None => {},\n\
                 },\n\
                 ::None => {},\n\
             }\n\
         };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<Opt::<usize>> =\n\
                 Opt::<Opt::<usize>>::Some(Opt::<usize>::Some(6));\n\
             bump(o.&mut);\n\
             match o {\n\
                 ::Some(inner) => match inner { ::Some(n) => n, ::None => 0 },\n\
                 ::None => 0,\n\
             }\n\
         };",
        "f()",
        expect![[r#"
            => 7
        "#]],
    );
}

#[test]
fn an_owned_match_still_copies_its_payloads_out() {
    // The owned path, running unchanged: the binding is the VALUE, so
    // writing to a mutable copy of it cannot touch the scrutinee.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static f = fn() -> usize {\n\
             let o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let copied = match o { ::Some(t) => t, ::None => 0 };\n\
             let again = match o { ::Some(t) => t, ::None => 0 };\n\
             copied + again\n\
         };",
        "f()",
        expect![[r#"
            => 2
        "#]],
    );
}

// ---- containment, from the borrows a projection makes ------------------
//
// Reads are path-exact (a disjoint field read leaves a sibling borrow
// alone), and the other half of that rule is that CONTAINMENT in either
// direction is still a conflict. These pin it over the shapes only a
// borrowed match produces — a payload borrow under a whole-value read —
// plus the two containment cases the sibling-read tests above do not
// reach.

#[test]
fn reading_the_whole_value_still_freezes_a_payload_borrow() {
    // CONTAINMENT, the read covering the borrow: `m.*` reads all of the
    // option, which includes the payload slot `t` points at. The borrow is
    // frozen by it, so the later write through `t` is the
    // suspended-by-a-read case. Path exactness must not have turned this
    // off — a shorter path covers more, and `[]` covers everything.
    check_run(
        "type Opt = enum::<T> { Some(T), None };\n\
         static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };\n\
         static f = fn() -> usize {\n\
             let mut o: Opt::<usize> = Opt::<usize>::Some(1);\n\
             let m = o.&mut;\n\
             let t = match m { ::Some(t) => t, ::None => panic(\"none\") };\n\
             let whole = m.*;\n\
             set(t, 9);\n\
             match whole { ::Some(n) => n, ::None => 0 }\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn reading_through_a_prefix_still_freezes_the_borrow_below_it() {
    // CONTAINMENT one level in: the borrow is of `w.*.inner.b`, the read is
    // of `w.*.inner` — a strict PREFIX of the borrow's path, so it contains
    // it and the freeze fires. Overlap is prefix-either-way, not equality.
    check_run(
        "type Inner = struct { a: usize, b: usize };\n\
         type W = struct { inner: Inner, other: usize };\n\
         static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };\n\
         static f = fn::<@a>(w: W.&mut::<@a>) -> usize {\n\
             let bb = w.*.inner.b.&mut;\n\
             let seen = w.*.inner;\n\
             set(bb, 3);\n\
             seen.a\n\
         };\n\
         static g = fn() -> usize {\n\
             let mut w: W = W(struct { inner = Inner(struct { a = 1, b = 2 }), other = 0 });\n\
             f(w.&mut)\n\
         };",
        "g()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn reading_a_local_by_its_bare_name_still_reaches_every_borrow_into_it() {
    // The root read, unchanged: a place with no element steps at all is the
    // empty path, which contains everything. `let r = n.&; n = 99; r.*` —
    // the ordinary exclusivity bug — must still be caught, and a bare-name
    // read of a borrowed STRUCT must still reach a borrow of one field.
    check_run(
        "type W = struct { a: usize, b: usize };\n\
         static set = fn::<@r>(m: usize.&mut::<@r>, v: usize) -> () { m.* = v; };\n\
         static f = fn() -> usize {\n\
             let mut w: W = W(struct { a = 1, b = 2 });\n\
             let bb = w.b.&mut;\n\
             let whole = w;\n\
             set(bb, 3);\n\
             whole.a\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: write through a borrow that was suspended by a read of the same place while this borrow was live
              note: this borrow was created here
              note: suspended here — the place was read while this exclusive borrow was live
        "#]],
    );
}

#[test]
fn a_raw_pointer_minted_from_a_borrow_is_path_filtered_too() {
    // M08's raw-pointer consequence, under path-granular nodes: a
    // `.&raw mut` minted through a borrow inherits the borrow's NODE
    // (`.&raw` is not a decayed safe borrow), and its PATH comes along
    // with it — so a raw write at a disjoint slot does not kill a sibling
    // borrow. Correct and consistent: the raw pointer speaks through the
    // same node, at its own location.
    check_run(
        "type W = struct { a: usize, b: usize };\n\
         static f = fn::<@a>(w: W.&mut::<@a>) -> usize {\n\
             let bb = w.*.b.&mut;\n\
             let ra = w.*.a.&raw mut;\n\
             unsafe { ra.* = 7; };\n\
             bb.* = 3;\n\
             w.*.a + w.*.b\n\
         };\n\
         static g = fn() -> usize {\n\
             let mut w: W = W(struct { a = 1, b = 2 });\n\
             f(w.&mut)\n\
         };",
        "g()",
        expect![[r#"
            => 10
        "#]],
    );
}

// ---- the `read` import — the host byte read -----------------------------

#[test]
fn the_host_read_fills_a_byte_buffer_and_answers_the_count() {
    // The whole boundary in one program: a caller-owned buffer, a
    // machine-shaped count, and no text policy anywhere — the bytes come
    // back as bytes.
    check_run_with_input(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> i64 {\n\
             match alloc_array::<u8>(8) {\n\
                 AllocResult::Ok(p) => {\n\
                     let n = unsafe { read(p, 8) };\n\
                     let first = unsafe { p.* };\n\
                     unsafe { dealloc_array(p, 8); };\n\
                     if first == 104 { n } else { 0 - n }\n\
                 }\n\
                 AllocResult::Err => 0,\n\
             }\n\
         };",
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
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> i64 {\n\
             match alloc_array::<u8>(4) {\n\
                 AllocResult::Ok(p) => {\n\
                     let n = unsafe { read(p, 4) };\n\
                     unsafe { dealloc_array(p, 4); };\n\
                     n\n\
                 }\n\
                 AllocResult::Err => 0 - 1,\n\
             }\n\
         };",
        "f()",
        expect![[r#"
            => 0
        "#]],
    );
}

#[test]
fn an_import_this_host_does_not_provide_is_refused_by_name() {
    // The P01 layer-1 property, stated at run time: a host that does not
    // provide a hook has denied the capability. Saying WHICH one is the
    // whole difference between a refusal and a crash.
    check_run(
        "extern static launch_missiles: unsafe fn(n: i64) -> i64;\n\
         static f = fn() -> i64 { unsafe { launch_missiles(1) } };",
        "f()",
        expect![[r#"
            error[Runtime]: no host implementation for the import `launch_missiles` — the interpreter provides `read` and nothing else
        "#]],
    );
}

#[test]
fn the_host_read_refuses_a_buffer_shorter_than_the_request() {
    // Judged BEFORE the read runs: a read that consumed input and then
    // trapped would have eaten bytes nobody can get back.
    check_run_with_input(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> i64 {\n\
             match alloc_array::<u8>(2) {\n\
                 AllocResult::Ok(p) => unsafe { read(p, 8) },\n\
                 AllocResult::Err => 0,\n\
             }\n\
         };",
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
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> i64 { unsafe { read(dangling::<u8>(), 0) } };",
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
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> u8 {\n\
             let mut a: [u8; 4] = [1, 2, 3, 4];\n\
             let p = a[0].&raw mut;\n\
             let m = a[3].&mut;\n\
             unsafe { read(p, 4); };\n\
             m.*\n\
         };",
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
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn() -> u8 {\n\
             let mut a: [u8; 4] = [1, 2, 3, 4];\n\
             let p = a[0].&raw mut;\n\
             unsafe { read(p, 4); };\n\
             unsafe { p.* }\n\
         };",
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
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
         static f = fn () -> u8 {\n\
             let mut a: [u8; 4] = [1, 2, 3, 4];\n\
             let m = a[0].&mut;\n\
             unsafe { read(a[0].&raw mut, 2); };\n\
             m.*\n\
         };",
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
fn a_host_import_declared_with_the_wrong_signature_is_refused() {
    let program = |buf: &str, arg: &str, len: &str, ret: &str| {
        format!(
            "extern static read: unsafe fn(buf: {buf}, len: usize) -> {ret};\n\
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
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A SHARED pointer: `read` writes, so read-only is as wrong as non-byte.
    check_run(
        &program("u8.&raw", "p.*.&raw", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // Not a pointer at all. These used to blame the compiler ("internal
    // error: expected a raw pointer") for what the declaration said.
    check_run(
        &program("i64", "1", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    check_run(
        &program("str", "\"x\"", "8", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A zero-length request touches no buffer, and is judged all the same:
    // the signature is wrong before any argument is looked at.
    check_run(
        &program("i64", "1", "0", "isize"),
        "f()",
        expect![[r#"
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
        "#]],
    );
    // A return type that is not a signed machine word.
    check_run(
        &program("u8.&raw mut", "p", "8", "bool"),
        "f()",
        expect![[r#"
            error[Runtime]: the host import `read` was declared with a signature this host does not provide; it provides `unsafe fn(buf: u8.&raw mut, len: usize) -> isize`
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
                "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> {spelling};\n\
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
fn calling_a_host_import_through_a_binding_traps_with_the_squiggle_text() {
    // The check-time diagnostic and the trap are the same sentence — the
    // house rule — so a program that reaches the host through a binding
    // cannot execute it unvouched. Note WHERE the trap is: taking the value
    // ran fine, and the CALL is what refused, because the call is the
    // operation and `g`'s type is what still knows a marker is owed.
    check_run_with_input(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;\n\
         static f = fn() -> isize {\n\
             let g = read;\n\
             match alloc_array::<u8>(8) { ::Ok(p) => g(p, 8), ::Err => 0 }\n\
         };",
        "f()",
        "hi",
        expect![[r#"
            error[Trap]: calling a value of `unsafe fn` type requires an `unsafe { ... }` block
        "#]],
    );
    // Vouched at the CALL, it runs — first-class-ness is priced, not
    // removed, and the binding itself never needed a marker.
    check_run_with_input(
        "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;\n\
         static f = fn() -> isize {\n\
             let g = read;\n\
             match alloc_array::<u8>(8) { ::Ok(p) => unsafe { g(p, 8) }, ::Err => 0 }\n\
         };",
        "f()",
        "hi",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn an_extern_static_with_an_initializer_is_not_an_import() {
    // The syntax error stands on its own; what must NOT happen is the item
    // becoming an import anyway and refusing by name at run time, which
    // blames a boundary the program never crossed. The written value is
    // what runs.
    check_run(
        "extern static bad: unsafe fn(n: i64) -> i64 = fn(n: i64) -> i64 { n + 1 };\n\
         static f = fn() -> i64 { unsafe { bad(1) } };",
        "f()",
        expect![[r#"
            => 2
        "#]],
    );
}

#[test]
fn a_data_import_is_refused_where_it_is_mentioned() {
    // A data import is reserved (G22), so there is no value for the
    // interpreter to produce and no host to ask for one. The mention is
    // what refuses, naming the item and what is wrong with it — the
    // alternative is a `usize` no host function can be, carried into
    // arithmetic that then blames the compiler for a program's mistake.
    check_run(
        "extern static x: usize;\n\
         static f = fn() -> usize { x + 1 };",
        "f()",
        expect![[r#"
            error[Trap]: cannot use `x`: its type annotation has errors
        "#]],
    );
}

#[test]
fn a_misplaced_extern_fn_is_not_an_import() {
    // The other half of a well-formed declaration: no name of its own, so
    // no import. Refusing here under the enclosing item's name would blame
    // a boundary the program never declared — this is the same "missing
    // expression" a bodyless plain `fn` gives.
    check_run(
        "static f = fn() -> i64 { let g = extern fn(n: i64) -> i64; unsafe { g(1) } };",
        "f()",
        expect![[r#"
            error[Trap]: syntax error: missing expression
        "#]],
    );
}

// ---- the blesses: bytes into `str` --------------------------------------

/// A buffer of `n` bytes filled from a `str` literal's own bytes, so a
/// bless fixture can start from known contents without a host read. The
/// bytes go in one at a time through `add`, which is the only way to write
/// a buffer today.
fn bytes_fixture(bytes: &[u8]) -> String {
    let mut writes = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        writes.push_str(&format!("unsafe {{ add(p, {index}).* = {byte}; }};\n"));
    }
    format!(
        "static with_bytes = fn::<T>(k: fn(u8.&raw mut) -> T) -> T {{\n\
             match alloc_array::<u8>({len}) {{\n\
                 AllocResult::Ok(p) => {{\n\
                     {writes}\
                     let out = k(p);\n\
                     unsafe {{ dealloc_array(p, {len}); }};\n\
                     out\n\
                 }}\n\
                 AllocResult::Err => panic(\"out of memory\"),\n\
             }}\n\
         }};\n",
        len = bytes.len(),
    )
}

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
        "static f = fn() -> str {\n\
             match alloc_array::<u8>(4) {\n\
                 AllocResult::Ok(p) => unsafe { str_from_utf8_unchecked(p, 4) },\n\
                 AllocResult::Err => \"oom\",\n\
             }\n\
         };",
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
        "static f = fn () -> u8 {\n\
             let mut a: [u8; 2] = [104, 105];\n\
             let m = a[0].&mut;\n\
             unsafe { str_from_utf8_unchecked(a[0].&raw, 2); };\n\
             m.* = 5;\n\
             a[0]\n\
         };",
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

// ---- the load-bearing property: a view dies when the reader moves on ----

#[test]
fn a_borrowed_view_is_invalidated_when_the_reader_is_used_again() {
    // Why this rides the blesses: the ruling that `str` stays a value and
    // gets no borrowed representation rests on a view copied out of a
    // borrow being real while a view HELD across the owner's next use is
    // not, and this is that second half. A reader handing out views into
    // its own buffer returns a borrow of its own `line` slot, so its next
    // `&mut` use — refilling, advancing, anything — invalidates the view,
    // and reading it afterwards is detected UB naming both sites.
    //
    // Nothing was added to the aliasing model for it: this is
    // reborrow-at-every-use doing its job.
    check_run(
        "type R = struct { line: str, n: usize } with {\n\
             impl Self {\n\
                 next = fn::<@b>(r: Self.&mut::<@b>) -> str.&::<@b> {\n\
                     r.*.n = r.*.n + 1;\n\
                     r.*.line = if r.*.n == 1 { \"one\" } else { \"two\" };\n\
                     r.*.line.&\n\
                 };\n\
             }\n\
         };\n\
         static f = fn() -> str {\n\
             let mut r = R(struct { line = \"\", n = 0 });\n\
             let m = r.&mut;\n\
             let a = m.next();\n\
             let b = m.next();\n\
             a.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_borrowed_view_read_before_the_reader_moves_on_is_fine() {
    // The other half of the same claim, and the half the ruling leans on:
    // a view read before the owner moves on is an ordinary read, so the
    // discipline rejects only the program that holds one too long.
    check_run(
        "type R = struct { line: str, n: usize } with {\n\
             impl Self {\n\
                 next = fn::<@b>(r: Self.&mut::<@b>) -> str.&::<@b> {\n\
                     r.*.n = r.*.n + 1;\n\
                     r.*.line = if r.*.n == 1 { \"one\" } else { \"two\" };\n\
                     r.*.line.&\n\
                 };\n\
             }\n\
         };\n\
         static f = fn() -> str {\n\
             let mut r = R(struct { line = \"\", n = 0 });\n\
             let m = r.&mut;\n\
             let first = m.next().*;\n\
             let second = m.next().*;\n\
             if first == \"one\" { second } else { \"unexpected\" }\n\
         };",
        "f()",
        expect![[r#"
            => "two"
        "#]],
    );
}

// ---- the stdin library, from the example itself ------------------------

/// `examples/stdin_lib.must` verbatim. Reading it off disk (rather than a
/// hand-shrunk copy) means these tests pin the exact behaviour, including
/// the exact panic text, of the file a user actually runs — a re-wording of
/// the example cannot silently drift from what is pinned here. `main`,
/// `print_usize`, `count_bytes` and `digit` go unused under the test-only
/// entry each test appends below; that is fine, `check_run_impl` does not
/// assert on diagnostics.
const STDIN_LIB: &str = include_str!("../../../examples/stdin_lib.must");

#[test]
fn the_stdin_library_joins_lines_across_refills_and_strips_crlf() {
    // Eighteen bytes through an EIGHT-byte buffer, so every line but the
    // first is assembled across a refill and compacted to the front. The
    // joined answer is the whole proof: CRLF stripped to nothing, a blank
    // line kept as a real (empty) line, and the unterminated `gamma` read
    // rather than dropped.
    check_run_with_input(
        &format!(
            "{STDIN_LIB}\n\
             static f = fn() -> str {{\n\
                 let mut r = reader_new(8);\n\
                 let m = r.&mut;\n\
                 let mut out = \"\";\n\
                 let done = loop {{\n\
                     match m.next_line() {{\n\
                         ::Some(line) => {{ out = join(out, line.*); }},\n\
                         ::None => break out,\n\
                     }}\n\
                 }};\n\
                 r.drop();\n\
                 done\n\
             }};\n\
             static join = fn(a: str, b: str) -> str {{\n\
                 if a == \"\" {{ tag(b) }} else {{ pair(a, tag(b)) }}\n\
             }};\n\
             static tag = fn(s: str) -> str {{\n\
                 if s == \"alpha\" {{ \"A\" }}\n\
                 else if s == \"\" {{ \"_\" }}\n\
                 else if s == \"beta\" {{ \"B\" }}\n\
                 else if s == \"gamma\" {{ \"G\" }}\n\
                 else {{ \"?\" }}\n\
             }};\n\
             static pair = fn(a: str, b: str) -> str {{\n\
                 if a == \"A\" {{ if b == \"_\" {{ \"A_\" }} else {{ \"?\" }} }}\n\
                 else if a == \"A_\" {{ if b == \"B\" {{ \"A_B\" }} else {{ \"?\" }} }}\n\
                 else if a == \"A_B\" {{ if b == \"G\" {{ \"A_BG\" }} else {{ \"?\" }} }}\n\
                 else {{ \"?\" }}\n\
             }};"
        ),
        "f()",
        "alpha\r\n\nbeta\ngamma",
        expect![[r#"
            => "A_BG"
        "#]],
    );
}

#[test]
fn the_stdin_library_invalidates_a_line_when_the_reader_moves_on() {
    // THE property this example exists to prove, on the library itself
    // rather than on a fixture that resembles it: a line handed out by
    // `next_line` is a borrow OF THE READER, so the next `next_line` — the
    // call that may refill and overwrite the bytes it came from — kills it,
    // and reading it afterwards is detected UB naming both sites.
    check_run_with_input(
        &format!(
            "{STDIN_LIB}\n\
             static f = fn() -> str {{\n\
                 let mut r = reader_new(8);\n\
                 let m = r.&mut;\n\
                 let first = match m.next_line() {{\n\
                     ::Some(l) => l,\n\
                     ::None => panic(\"no input\"),\n\
                 }};\n\
                 let second = m.next_line();\n\
                 let out = first.*;\n\
                 r.drop();\n\
                 out\n\
             }};"
        ),
        "f()",
        "one\ntwo\n",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_stdin_library_reports_bytes_that_are_not_text() {
    // The CHECKED bless earning its name at the top of the stack: the
    // library answers rather than asserts, so invalid input is a message
    // and not a `str` that isn't one.
    check_run_with_bytes(
        &format!(
            "{STDIN_LIB}\n\
             static f = fn() -> str {{\n\
                 let mut r = reader_new(8);\n\
                 let m = r.&mut;\n\
                 let out = match m.next_line() {{ ::Some(l) => l.*, ::None => \"none\" }};\n\
                 r.drop();\n\
                 out\n\
             }};"
        ),
        "f()",
        b"ok\xff\n",
        expect![[r#"
            error[Panic]: stdin_lib: standard input is not valid UTF-8
        "#]],
    );
}

#[test]
fn the_stdin_library_refuses_a_line_longer_than_its_buffer() {
    // A fixed buffer has an honest limit, and the library says so by name
    // instead of truncating a line or looping forever asking for room it
    // does not have. The limit is `cap - 1`, not `cap`: a line needs room
    // for one byte more than itself — the terminator, or the read that
    // discovers there isn't one — so a 4-byte reader stops at 3.
    check_run_with_input(
        &format!(
            "{STDIN_LIB}\n\
             static f = fn() -> str {{\n\
                 let mut r = reader_new(4);\n\
                 let m = r.&mut;\n\
                 let out = match m.next_line() {{ ::Some(l) => l.*, ::None => \"none\" }};\n\
                 r.drop();\n\
                 out\n\
             }};"
        ),
        "f()",
        "abcdefgh\n",
        expect![[r#"
            error[Panic]: stdin_lib: no line boundary in a full buffer
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
        "static f = fn() -> str {\n\
             let text = \"smørre\";\n\
             let n = text.len();\n\
             match alloc_array::<u8>(n) {\n\
                 AllocResult::Ok(p) => {\n\
                     unsafe { str_bytes(text, p); };\n\
                     let back = unsafe { str_from_utf8_unchecked(p, n) };\n\
                     unsafe { dealloc_array(p, n); };\n\
                     back\n\
                 }\n\
                 AllocResult::Err => \"oom\",\n\
             }\n\
         };",
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
        "static n: usize = const { \"smørre\".len() };\n\
         static f = fn() -> usize { n };",
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
        "static f = fn() -> usize {\n\
             match alloc_array::<u8>(2) {\n\
                 AllocResult::Ok(p) => {\n\
                     unsafe { str_bytes(\"hello\", p); };\n\
                     unsafe { dealloc_array(p, 2); };\n\
                     0\n\
                 }\n\
                 AllocResult::Err => 1,\n\
             }\n\
         };",
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
        "static f = fn() -> usize {\n\
             let p = unsafe { dangling::<u8>() };\n\
             unsafe { str_bytes(\"\", p); };\n\
             \"\".len()\n\
         };",
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
        "static f = fn() -> u8 {\n\
             let mut x: u8 = 9;\n\
             unsafe { str_bytes(\"\", x.&raw mut); };\n\
             x\n\
         };",
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
        "static f = fn () -> u8 {\n\
             let mut a: [u8; 4] = [1, 2, 3, 4];\n\
             let m = a[0].&mut;\n\
             unsafe { str_bytes(\"hi\", a[0].&raw mut); };\n\
             m.*\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

// ---- moving is an invalidation event ------------------------------------

#[test]
fn moving_a_value_invalidates_borrows_of_it_exactly_as_writing_does() {
    // The aliasing model's event is "the storage no longer holds what the
    // borrow was taken of", and a MOVE is that event as surely as a write
    // is. Without this, a borrow of a value that was then consumed read
    // stale bytes with nothing to say about it — while its write-shaped
    // twin (below) was caught, which is what made the gap a bug rather
    // than a limit.
    check_run(
        "type Lin = struct { id: usize } only move with {\n\
             impl Self {\n\
                 eat = fn(s: Self) -> () { let Lin(struct { id }) = s; };\n\
             }\n\
         };\n\
         static f = fn() -> usize {\n\
             let s = Lin(struct { id = 1 });\n\
             let b = s.&;\n\
             s.eat();\n\
             b.*.id\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn the_write_shaped_twin_of_a_move_invalidation_says_the_same_thing() {
    check_run(
        "type Lin = struct { id: usize } only move with {\n\
             impl Self {\n\
                 eat = fn(s: Self) -> () { let Lin(struct { id }) = s; };\n\
             }\n\
         };\n\
         static f = fn() -> usize {\n\
             let mut s = Lin(struct { id = 1 });\n\
             let b = s.&;\n\
             s = Lin(struct { id = 2 });\n\
             let n = b.*.id;\n\
             s.eat();\n\
             n\n\
         };",
        "f()",
        expect![[r#"
            error[UndefinedBehavior]: read through a borrow that is no longer valid: the value was borrowed again, written through another borrow, or moved away, while this borrow was still live
              note: this borrow was created here
              note: invalidated here — the value was borrowed again, written through another borrow, or moved away
        "#]],
    );
}

#[test]
fn a_move_with_no_borrow_outstanding_costs_nothing_and_still_runs() {
    // The invalidation is one map lookup on an address-taken local, so a
    // body that never borrows pays for the distinction with nothing.
    check_run(
        "type Lin = struct { id: usize } only move with {\n\
             impl Self {\n\
                 into_id = fn(s: Self) -> usize { let Lin(struct { id }) = s; id };\n\
             }\n\
         };\n\
         static f = fn() -> usize {\n\
             let s = Lin(struct { id = 7 });\n\
             let t = s;\n\
             t.into_id()\n\
         };",
        "f()",
        expect![[r#"
            => 7
        "#]],
    );
}

// ---- materialized temporaries (M12) --------------------------------------

/// A write through a `.&mut` of a temporary is visible to the code holding
/// the borrow.
#[test]
fn a_temporary_borrowed_exclusively_is_written_through() {
    check_run(
        r#"
type Printer = struct { count: usize } with {
    impl Self {
        emit = fn::<@a>(s: str, p: Self.&mut::<@a>) -> () {
            print(s);
            p.*.count = p.*.count + 1;
        };
    }
};
type Inner = struct { label: str } with {
    impl Self {
        fmt = fn::<@a>(p: Printer.&mut::<@a>, i: Self) -> usize {
            p.emit(i.label);
            p.emit(i.label);
            p.*.count
        };
    }
};
static f = fn() -> usize {
    let inner = Inner(struct { label = "x" });
    inner.fmt(Printer(struct { count = 0 }).&mut)
};
"#,
        "f()",
        expect![[r#"
            output: "xx"
            => 2
        "#]],
    );
}

/// A branch's temporary is created only when that branch runs, and a
/// loop's is created afresh every iteration.
#[test]
fn a_temporary_in_a_branch_or_a_loop_is_created_per_path() {
    check_run(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        bump = fn::<@a>(by: usize, c: Self.&mut::<@a>) -> usize {
            c.*.v = c.*.v + by;
            c.*.v
        };
    }
};
static pick = fn(c: bool) -> usize {
    if c { Cell(struct { v = 10 }).&mut.bump(1) }
    else { Cell(struct { v = 20 }).&mut.bump(2) }
};
static f = fn() -> usize {
    let mut total = 0;
    let mut i = 0;
    loop {
        if i == 3 { break; };
        total = total + Cell(struct { v = i }).&mut.bump(1);
        i = i + 1;
    };
    total + pick(true) + pick(false)
};
"#,
        "f()",
        expect![[r#"
            => 39
        "#]],
    );
}

/// A borrow of a temporary's FIELD is written through and read back like
/// a borrow of the whole value.
#[test]
fn a_borrow_of_a_temporarys_field_names_the_temporarys_storage() {
    check_run(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> Pair { Pair(struct { a = 3, b = 4 }) };
static set = fn::<@a>(v: usize, r: usize.&mut::<@a>) -> usize {
    r.* = v;
    r.*
};
static f = fn() -> usize { set(9, mk().a.&mut) };
"#,
        "f()",
        expect![[r#"
            => 9
        "#]],
    );
}

/// A temporary is evaluated where it is written: `a()` runs before
/// `mk()`, so the program prints `am`.
#[test]
fn materializing_a_temporary_does_not_reorder_the_operands_around_it() {
    check_run(
        r#"
static a = fn() -> usize { print("a"); 1 };
static mk = fn() -> usize { print("m"); 2 };
static add = fn::<@x>(l: usize, r: usize.&::<@x>) -> usize { l + r.* };
static f = fn() -> usize { add(a(), mk().&) };
"#,
        "f()",
        expect![[r#"
            output: "am"
            => 3
        "#]],
    );
}

/// A temporary in a CONST context is an ordinary local there too: the
/// compile-time evaluator allocates it, borrows it and reads through the
/// borrow, in a `static`'s initializer and in a `const` block alike.
#[test]
fn a_temporary_can_be_borrowed_at_compile_time() {
    check_const(
        r#"
static get = const fn::<@a>(r: usize.&::<@a>) -> usize { r.* };
static mk = const fn() -> usize { 7 };
static folded: usize = get(mk().&);
static in_const_block = const { get(mk().&) };
"#,
        expect![[r#"
            get = fn
            mk = fn
            folded = 7
            in_const_block = 7
        "#]],
    );
}

/// `.&raw` of a temporary runs against the same storage `.&` would get:
/// `mkp().a.&raw mut` writes into the temporary's own field, not a copy
/// of it, and the read through `p` sees the materialized value.
#[test]
fn a_raw_pointer_to_a_temporary_reads_and_writes_its_storage() {
    check_run(
        r#"
type Pair = struct { a: usize, b: usize };
static mk = fn() -> usize { 7 };
static mkp = fn() -> Pair { Pair(struct { a = 1, b = 2 }) };
static main = fn() -> usize {
    let p = mk().&raw;
    let q = mkp().a.&raw mut;
    unsafe {
        q.* = 9;
        p.* + q.*
    }
};
"#,
        "main()",
        expect![[r#"
            => 16
        "#]],
    );
}
