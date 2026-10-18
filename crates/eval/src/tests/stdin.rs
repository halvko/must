//! Standard input: `read_line` and the stdin library.

use super::*;

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
    check_run_past_the_cfg_checkers(
        r#"
type R = struct { line: str, n: usize } with {
    impl Self {
        next = fn::<@b>(r: Self.&mut::<@b>) -> str.&::<@b> {
            r.*.n = r.*.n + 1;
            r.*.line = if r.*.n == 1 { "one" } else { "two" };
            r.*.line.&
        };
    }
};
static f = fn() -> str {
    let mut r = R(struct { line = "", n = 0 });
    let m = r.&mut;
    let a = m.next();
    let b = m.next();
    a.*
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
fn a_borrowed_view_read_before_the_reader_moves_on_is_fine() {
    // The other half of the same claim, and the half the ruling leans on:
    // a view read before the owner moves on is an ordinary read, so the
    // discipline rejects only the program that holds one too long.
    check_run(
        r#"
type R = struct { line: str, n: usize } with {
    impl Self {
        next = fn::<@b>(r: Self.&mut::<@b>) -> str.&::<@b> {
            r.*.n = r.*.n + 1;
            r.*.line = if r.*.n == 1 { "one" } else { "two" };
            r.*.line.&
        };
    }
};
static f = fn() -> str {
    let mut r = R(struct { line = "", n = 0 });
    let m = r.&mut;
    let first = m.next().*;
    let second = m.next().*;
    if first == "one" { second } else { "unexpected" }
};
"#,
        "f()",
        expect![[r#"
            => "two"
        "#]],
    );
}

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
    // call that may refill and overwrite the bytes it came from — kills it.
    // The loan checker refuses it, so the run traps at that call with the
    // checker's message, before the stale line is ever read.
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
            error[Trap]: using `m` mutably here invalidates a borrow of it that is still live: the borrow is used after this point, and reading through it then would read through an invalidated borrow
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
