//! Regression smoke test over `examples/`: every top-level `.must` file
//! there must `check` clean (`errors.must` excepted — it is intentionally
//! dirty) and must `run` — via the invocation(s) its header comment
//! documents with `// Run: must-lsp run ...`, or the default entry point
//! when none is documented — without hanging, printing exactly what it
//! printed the last time its output was verified by eye.
//!
//! Nothing else runs `examples/`, so [`spawn_with_timeout`]'s per-invocation
//! deadline is what turns a hang into a loud test failure instead of a
//! silently wedged one.
//!
//! Snapshots use `expect_test`, the same crate the unit tests already use
//! (see e.g. `crates/must-lsp/src/check.rs`). To refresh one after a
//! deliberate, eyeballed-correct output change:
//!
//!     UPDATE_EXPECT=1 cargo test --manifest-path Cargo.toml -p must-lsp --test examples

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use expect_test::{Expect, expect};

const BIN: &str = env!("CARGO_BIN_EXE_must-lsp");

/// Generous, but not infinite: every example here runs in well under a
/// second normally, so a few seconds of slack catches only genuine hangs.
const TIMEOUT: Duration = Duration::from_secs(5);

fn workspace_root() -> PathBuf {
    // crates/must-lsp -> crates -> workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/must-lsp has two ancestor directories")
        .to_path_buf()
}

/// Every `.must` file directly inside `examples/`, sorted. Subdirectories
/// are never walked.
fn example_files() -> Vec<String> {
    let dir = workspace_root().join("examples");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("read examples/")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "must"))
        .map(|path| {
            path.file_name()
                .expect("a file has a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The filenames this file has explicit `check`/`run` coverage for below,
/// kept as a literal (not derived from the directory) so that adding a new
/// example to `examples/` without adding matching coverage here fails this
/// test loudly, instead of the new file silently shipping unsmoke-tested.
const COVERED: &[&str] = &[
    "arrays.must",
    "borrows.must",
    "chars.must",
    "compile_time.must",
    "display.must",
    "errors.must",
    "functions.must",
    "generics.must",
    "heap.must",
    "hello.must",
    "loops.must",
    "match_projection.must",
    "option.must",
    "pointers.must",
    "reborrow.must",
    "records.must",
    "state_machine.must",
    "stdin.must",
    "stdin_lib.must",
    "string_lib.must",
];

#[test]
fn every_example_file_has_smoke_coverage() {
    let found = example_files();
    let mut covered: Vec<String> = COVERED.iter().map(|s| s.to_string()).collect();
    covered.sort();
    assert_eq!(
        found, covered,
        "examples/*.must changed on disk — add or remove matching check/run \
         coverage in crates/must-lsp/tests/examples.rs (left: found on disk, \
         right: what this test covers)"
    );
}

/// Spawn `must-lsp <args>` from the workspace root and poll for
/// completion rather than blocking on `wait`, so a wedged child can be
/// killed and reported instead of hanging the test process forever.
/// `label` identifies the invocation in the panic message only.
fn spawn_with_timeout(args: &[&str], label: &str) -> (String, String, i32) {
    spawn_with_timeout_input(args, label, None)
}

/// [`spawn_with_timeout`], but with `input` piped to the child's stdin
/// (written up front, then the handle is dropped/closed so a `read_line`
/// loop sees genuine end-of-input) — the real-process mirror of
/// `runner::tests::check_with_input`'s in-process injection, exercising the
/// actual `must-lsp run program.must < input.txt` contract.
fn spawn_with_timeout_input(
    args: &[&str],
    label: &str,
    input: Option<&str>,
) -> (String, String, i32) {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Callers with no input leave the child's stdin inherited; an
    // injection pipes it, written up front and then dropped/closed so a
    // `read_line` loop on the child's side sees genuine end-of-input.
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().expect("spawn must-lsp");

    if let Some(input) = input {
        use std::io::Write;
        let mut stdin_pipe = child.stdin.take().expect("child has piped stdin");
        stdin_pipe
            .write_all(input.as_bytes())
            .expect("write child stdin");
        drop(stdin_pipe);
    }

    // Drain both pipes on background threads while polling: a wedged
    // child that also floods a pipe would otherwise deadlock this
    // function (it blocks on a full pipe buffer, we'd block on a `wait`
    // that never returns because we never read the buffer that's stalling
    // it). The threads finish on their own once the pipes close, which
    // happens on ordinary exit and also right after `kill`.
    let mut stdout_pipe = child.stdout.take().expect("child has piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("child has piped stderr");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll child status") {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "example {label} did not terminate within {}s — infinite loop?",
                TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let stdout = String::from_utf8(stdout_reader.join().expect("stdout reader thread panicked"))
        .expect("stdout is UTF-8");
    let stderr = String::from_utf8(stderr_reader.join().expect("stderr reader thread panicked"))
        .expect("stderr is UTF-8");
    let code = status.code().expect("exited normally, not signal-killed");
    (stdout, stderr, code)
}

/// `must-lsp check examples/<file>`: assert the combined diagnostics text
/// (stdout: rendered diagnostics; stderr: the summary line) and exit code
/// match. Every example except `errors.must` is expected clean (empty
/// output, exit 0); `errors.must` is intentionally dirty and pins its
/// full diagnostic dump plus the "found N errors and M warnings" line.
fn assert_check(file: &str, want_code: i32, want: Expect) {
    let rel = format!("examples/{file}");
    let label = format!("check {rel}");
    let (stdout, stderr, code) = spawn_with_timeout(&["check", &rel], &label);
    assert_eq!(
        code, want_code,
        "unexpected exit code from `must-lsp {label}`"
    );
    want.assert_eq(&format!("{stdout}{stderr}"));
}

/// One documented (or default) `must-lsp run ...` invocation: assert
/// stdout and exit code match. `args` is the full argument list after the
/// binary name, e.g. `["run", "examples/loops.must", "-e", "sum_to(10)"]`.
fn assert_run(args: &[&str], want_code: i32, want: Expect) {
    let label = args.join(" ");
    let (stdout, _stderr, code) = spawn_with_timeout(args, &label);
    assert_eq!(
        code, want_code,
        "unexpected exit code from `must-lsp {label}`"
    );
    want.assert_eq(&stdout);
}

/// [`assert_run`], with `input` piped to the child's stdin — the real-CLI
/// exercise of `must-lsp run program.must < input.txt`.
fn assert_run_with_input(args: &[&str], input: &str, want_code: i32, want: Expect) {
    let label = args.join(" ");
    let (stdout, _stderr, code) = spawn_with_timeout_input(args, &label, Some(input));
    assert_eq!(
        code, want_code,
        "unexpected exit code from `must-lsp {label}`"
    );
    want.assert_eq(&stdout);
}

// --- check: every example is silent except errors.must ---------------------

#[test]
fn arrays_checks_clean() {
    assert_check("arrays.must", 0, expect![[r#""#]]);
}

#[test]
fn borrows_checks_clean() {
    assert_check("borrows.must", 0, expect![[r#""#]]);
}

#[test]
fn chars_checks_clean() {
    assert_check("chars.must", 0, expect![[r#""#]]);
}

#[test]
fn compile_time_checks_clean() {
    assert_check("compile_time.must", 0, expect![[r#""#]]);
}

#[test]
fn display_checks_clean() {
    assert_check("display.must", 0, expect![[r#""#]]);
}

#[test]
fn errors_checks_dirty_with_the_documented_count() {
    assert_check(
        "errors.must",
        1,
        expect![[r#"
            error: type mismatch: expected `usize`, found `str`
              --> examples/errors.must:17:32
               |
            17 | static bad_annotation: usize = "not a number";
               |                                ^^^^^^^^^^^^^^
               = note: expected `usize` because of this annotation (examples/errors.must:17:24)

            error: cannot call `double` in a const context; marking it `const fn` would allow this
              --> examples/errors.must:27:26
               |
            27 | static const_violation = double(21);
               |                          ^^^^^^
               = help: Mark `double` as `const fn`
               = note: `double` is defined here (examples/errors.must:26:8)
               = note: this item's initializer is a const context (examples/errors.must:27:1)

            error: this `match` does not cover `Shape::Point`
              --> examples/errors.must:35:5
               |
            35 |     match s {
               |     ^^^^^
               = help: Add missing match arms

            error: cannot assign to `x`: it is not declared `mut`
              --> examples/errors.must:45:5
               |
            45 |     x = 2;
               |     ^
               = help: Make `x` mutable
               = note: `x` is declared without `mut` here (examples/errors.must:44:9)

            error: record literal is missing field `y: str`
              --> examples/errors.must:52:53
               |
            52 | static missing_field: struct { x: usize, y: str } = struct { x = 1 };
               |                                                     ^^^^^^^^^^^^^^^^

            error: type mismatch: expected `Shape::Circle`, found `Shape::Point`
              --> examples/errors.must:59:64
               |
            59 | static call_with_wrong_variant = fn () -> usize { wants_circle(Shape::Point) };
               |                                                                ^^^^^^^^^^^^

            warning: `Point` binds the whole value; write `::Point` (or `Shape::Point`) to match the variant
              --> examples/errors.must:69:9
               |
            69 |         Point => "point",
               |         ^^^^^
               = note: `Shape` is defined here (examples/errors.must:29:6)

            error: type mismatch: expected `{number}`, found `T`
              --> examples/errors.must:86:13
               |
            86 |     let n = x + 1;
               |             ^
               = note: `+` requires `{number}` operands (examples/errors.must:86:15)

            error: index out of bounds: the length is 2 but the index is 2
              --> examples/errors.must:98:5
               |
            98 |     a[2]
               |     ^^^^

            error: dereferencing a raw pointer requires an `unsafe { ... }` block
              --> examples/errors.must:108:5
                |
            108 |     p.*
                |     ^^^

            error: cannot take `.&raw mut` of `x`: it is not declared `mut`
              --> examples/errors.must:120:13
                |
            120 |     let p = x.&raw mut;
                |             ^
               = help: Make `x` mutable
               = note: `x` is declared without `mut` here (examples/errors.must:118:9)

            error: type mismatch: expected `Buf::<8>`, found `Buf::<9>`
              --> examples/errors.must:128:36
                |
            128 | static wrong_const_len: Buf::<8> = Buf::<9>(struct { len = 1 });
                |                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               = note: expected `Buf::<8>` because of this annotation (examples/errors.must:128:25)

            error: constant evaluation failed: a pointer cannot leave compile-time evaluation
              --> examples/errors.must:136:33
                |
            136 | static pointer_escaping_const = const {
                |                                 ^^^^^^^

            error: cannot infer the type of this number: it has no defining use — add a type annotation
              --> examples/errors.must:147:13
                |
            147 |     let n = 5;
                |             ^

            error: `300` does not fit in `u8`
              --> examples/errors.must:154:29
                |
            154 | static too_big_for_u8: u8 = 300;
                |                             ^^^

            error: record fields are defined with `=` (`name = value`); `:` annotates a type
              --> examples/errors.must:161:56
                |
            161 | static old_spelling: struct { x: usize } = struct { x: 1 };
                |                                                        ^

            error: unknown trait `Display`
              --> examples/errors.must:168:10
                |
            168 |     impl Display {
                |          ^^^^^^^

            error: a declare-only inherent member is an unimplementable promise; define it: `name = fn(...) -> ... { ... };`
              --> examples/errors.must:179:9
                |
            179 |         len: fn(p: Self) -> usize;
                |         ^^^^^^^^^^^^^^^^^^^^^^^^^^

            error: no field or member `plain_len` on `Opted`
              --> examples/errors.must:189:47
                |
            189 | static dotted_call = fn (o: Opted) -> usize { o.plain_len() };
                |                                               ^^^^^^^^^^^^^
               = note: a module-level `plain_len` is defined here — statics are never dot-callable; call `plain_len(...)` instead (examples/errors.must:188:8)

            error: `scaled` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural)
              --> examples/errors.must:200:56
                |
            200 | static wrong_self_position = fn (s: Scaler) -> usize { s.scaled(2) };
                |                                                        ^^^^^^^^^^^
               = note: `scaled` is defined here (examples/errors.must:197:9)

            error: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
              --> examples/errors.must:209:13
                |
            209 |     let p = &raw mut x;
                |             ^

            error: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
              --> examples/errors.must:220:23
                |
            220 |     const { if true { return 1; }; 0 }
                |                       ^^^^^^^^

            error: `Measured::size` takes no generic arguments
              --> examples/errors.must:239:53
                |
            239 | static member_own_turbofish = fn () -> () { let f = Measured::size::<usize>; };
                |                                                     ^^^^^^^^^^^^^^^^^^^^^^^

            error: `Counted::step` declares a const parameter of its own, and const member arguments are not supported yet (a member's type arguments are written here; its region arguments are always inferred)
              --> examples/errors.must:252:62
                |
            252 | static member_own_const_turbofish = fn (n: usize) -> usize { Counted::step::<3>(n) };
                |                                                              ^^^^^^^^^^^^^^^^^^^^^

            error: `len` is a field of `Sized`, not a member — fields are reached through a value: `value.len`
              --> examples/errors.must:260:55
                |
            260 | static field_through_the_type = fn () -> () { let n = Sized::len; };
                |                                                       ^^^^^^^^^^
               = note: `Sized` is defined here (examples/errors.must:259:6)

            error: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
              --> examples/errors.must:273:45
                |
            273 | static self_hole = fn (n: usize) -> usize { Countable::<Self = _>::count(n) };
                |                                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

            error: using this borrow where a longer-lived one is expected needs `@a` to outlive `@b`, which this signature does not declare; add `@a: @b` to the binder
              --> examples/errors.must:282:80
                |
            282 | static undeclared_outlives = fn::<@a, @b>(x: usize.&::<@a>) -> usize.&::<@b> { x };
                |                                                                                ^

            error: cannot resolve `::Point` without an expected type — write `Enum::Point`
              --> examples/errors.must:292:53
                |
            292 | static sigil_without_a_type = fn () -> () { let s = ::Point; };
                |                                                     ^^^^^^^

            error: `bump` takes `Self.&mut`, and a borrow is never inserted for an owned receiver — write `.&mut.bump(...)`
              --> examples/errors.must:309:5
                |
            309 |     c.bump()
                |     ^^^^^^^^
               = note: `bump` is defined here (examples/errors.must:304:9)

            error: generic arguments use the turbofish: write `Boxed::<...>`
              --> examples/errors.must:320:37
                |
            320 | static bare_angle_generics = fn (b: Boxed<usize>) -> usize { b.value };
                |                                     ^^^^^^^^^^^^
               = help: Insert `::`

            error: cannot call `read_line` in a const context; const evaluation cannot have side effects
              --> examples/errors.must:328:29
                |
            328 | static read_line_in_const = read_line();
                |                             ^^^^^^^^^
               = note: this item's initializer is a const context (examples/errors.must:328:1)

            error: borrows are spelled postfix: `x.&` / `x.&mut`
              --> examples/errors.must:337:13
                |
            337 |     let r = &x;
                |             ^^
               = help: Rewrite as postfix

            error: calling the host import `host_read` requires an `unsafe { ... }` block; nothing on this side of the boundary can check what it does
              --> examples/errors.must:347:58
                |
            347 | static unvouched_import = fn (p: u8.&raw mut) -> isize { host_read(p, 8) };
                |                                                          ^^^^^^^^^^^^^^^

            error: a host import is a DECLARATION, not an initializer: write `extern static retired_import: unsafe fn(...) -> T;`
              --> examples/errors.must:357:25
                |
            357 | static retired_import = extern fn(n: usize) -> usize;
                |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               = help: Rewrite as an `extern static` declaration

            error: an `extern static` has no initializer: the declaration is the whole contract, and an import sets nothing to anything
              --> examples/errors.must:366:65
                |
            366 | extern static import_with_a_value: unsafe fn(n: usize) -> usize = fn (n: usize) -> usize { n };
                |                                                                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               = help: Remove the initializer

            error: an import must be declared `unsafe fn` for now: a safe-to-call import needs the declaration-side `unsafe` marker, and that marker does not exist yet
              --> examples/errors.must:376:28
                |
            376 | extern static safe_import: fn() -> ();
                |                            ^^^^^^^^^^
               = help: Write `unsafe fn`

            error: data imports are not supported yet — an import must have a function type
              --> examples/errors.must:384:30
                |
            384 | extern static a_data_import: usize;
                |                              ^^^^^

            error: an import's type must be written in full: the declaration is the whole contract, and there is no body for `_` to be inferred from
              --> examples/errors.must:391:48
                |
            391 | extern static a_partial_contract: unsafe fn(n: _) -> i64;
                |                                                ^

            error: regions are inferred at calls, never written: drop this argument — a turbofish spells type and const arguments only
              --> examples/errors.must:402:73
                |
            402 | static region_at_a_call = fn::<@b>(p: usize.&::<@b>) -> usize { first::<@b, usize>(p).* };
                |                                                                         ^^

            error: region parameters come first in a binder; move `@a` before `T`
              --> examples/errors.must:410:36
                |
            410 | static misordered_binder = fn::<T, @a>(r: T.&::<@a>) -> T.&::<@a> { r };
                |                                    ^^

            error: regions are inferred at a borrow, never written: drop this argument — a region belongs in a type position, so assert it with an annotation (`let r: _.&::<@a> = x.&;`)
              --> examples/errors.must:419:80
                |
            419 | static region_at_a_borrow = fn::<@c>(p: usize.&::<@c>) -> usize { let q = p.*.&::<@c>; q.* };
                |                                                                                ^^^^^^
               = help: Drop the region argument

            found 40 errors and 1 warning
        "#]],
    );
}

#[test]
fn functions_checks_clean() {
    assert_check("functions.must", 0, expect![[r#""#]]);
}

#[test]
fn generics_checks_clean() {
    assert_check("generics.must", 0, expect![[r#""#]]);
}

#[test]
fn heap_checks_clean() {
    assert_check("heap.must", 0, expect![[r#""#]]);
}

#[test]
fn hello_checks_clean() {
    assert_check("hello.must", 0, expect![[r#""#]]);
}

#[test]
fn loops_checks_clean() {
    assert_check("loops.must", 0, expect![[r#""#]]);
}

#[test]
fn match_projection_checks_clean() {
    assert_check("match_projection.must", 0, expect![[r#""#]]);
}

#[test]
fn option_checks_clean() {
    // `Option` and its inherent members (`is_some`, `unwrap`, `flat_map`,
    // `map`, `as_ref`) — definitions only, no `main`; see `option_runs`
    // below for the four documented `-e` invocations.
    assert_check("option.must", 0, expect![[r#""#]]);
}

#[test]
fn pointers_checks_clean() {
    assert_check("pointers.must", 0, expect![[r#""#]]);
}

#[test]
fn reborrow_checks_clean() {
    assert_check("reborrow.must", 0, expect![[r#""#]]);
}

#[test]
fn records_checks_clean() {
    assert_check("records.must", 0, expect![[r#""#]]);
}

#[test]
fn state_machine_checks_clean() {
    assert_check("state_machine.must", 0, expect![[r#""#]]);
}

#[test]
fn stdin_checks_clean() {
    assert_check("stdin.must", 0, expect![[r#""#]]);
}

#[test]
fn stdin_lib_checks_clean() {
    assert_check("stdin_lib.must", 0, expect![[r#""#]]);
}

#[test]
fn string_lib_checks_clean() {
    assert_check("string_lib.must", 0, expect![[r#""#]]);
}

// --- run: the documented `// Run:` invocation(s), or the default entry -----

#[test]
fn arrays_runs() {
    // No `-e` documented: the default entry (`main()`).
    assert_run(
        &["run", "examples/arrays.must"],
        0,
        expect![[r#"
        table: TABLE[2] + TABLE[4] == 20 — built in a const loop, frozen into a static
        nested: grid[0][1] + grid[1][0] == 5
        index-assign: a[0] = 10 through a mut binding
        generic buffer: Buf::<2> holds a [usize; 2] summing to 42
    "#]],
    );
}

#[test]
fn borrows_runs() {
    // No `-e` documented: the default entry (`main()`). The digits are the
    // values each section produces — see the example's own commentary.
    assert_run(
        &["run", "examples/borrows.must"],
        0,
        expect!["13340804040403"],
    );
}

#[test]
fn compile_time_runs() {
    assert_run(
        &[
            "run",
            "examples/compile_time.must",
            "-e",
            "fortnight_seconds",
        ],
        0,
        expect![[r#"
            1209600
        "#]],
    );
    assert_run(
        &["run", "examples/compile_time.must", "-e", "is_big"],
        0,
        expect![[r#"
            true
        "#]],
    );
    assert_run(
        &["run", "examples/compile_time.must", "-e", "greet()"],
        0,
        expect![[r#"
            "computed at compile time, not when greet() runs"
        "#]],
    );
}

#[test]
fn display_runs() {
    // No `// Run:` line at all: the default entry (`main()`).
    assert_run(
        &["run", "examples/display.must"],
        0,
        expect![[r#"
        show::<usize>(42)   -> 42
        show::<str>(hello)  -> hello
        show::<Point>(1,23) -> 1,23
        7.fmt(sink)         -> 7
        Display::fmt(bye)   -> bye
        show(42): two digits pushed
        show("hello"): one fragment pushed
        show(Point): x, comma, both digits of y
        7.fmt: one digit pushed
        Display::fmt: one fragment pushed
    "#]],
    );
}

#[test]
fn errors_runs() {
    // No `must-lsp run ...` invocation is documented (its header only
    // documents `must-lsp check`), so this is the default entry. The file
    // defines no `main`, so running it (rather than checking it) traps
    // immediately on the synthetic entry expression.
    assert_run(&["run", "examples/errors.must"], 1, expect![[r#""#]]);
}

#[test]
fn functions_runs() {
    assert_run(
        &["run", "examples/functions.must", "-e", "fib(20)"],
        0,
        expect![[r#"
            6765
        "#]],
    );
    assert_run(
        &["run", "examples/functions.must", "-e", "gcd(1071, 462)"],
        0,
        expect![[r#"
            21
        "#]],
    );
    assert_run(
        &["run", "examples/functions.must", "-e", "is_even(76)"],
        0,
        expect![[r#"
            true
        "#]],
    );
    assert_run(
        &["run", "examples/functions.must", "-e", "is_odd(76)"],
        0,
        expect![[r#"
            false
        "#]],
    );
    assert_run(
        &[
            "run",
            "examples/functions.must",
            "-e",
            "apply_twice(double, 5)",
        ],
        0,
        expect![[r#"
            20
        "#]],
    );
    // Documented with no `-e`: the default entry.
    assert_run(
        &["run", "examples/functions.must"],
        0,
        expect![[r#"
        -
        -
        fizz
        -
        buzz
        fizz
        -
        -
        fizz
        buzz
        -
        fizz
        -
        -
        fizzbuzz
        fib: fib(10) == 55
        gcd: gcd(35, 64) == 1 — Euclid via a hand-derived remainder
        mutual recursion: is_even(76) and not is_odd(76)
        higher-order: apply_twice(double, 5) == 20 — a fn value passed and called
    "#]],
    );
}

#[test]
fn generics_runs() {
    assert_run(
        &["run", "examples/generics.must", "-e", r#"id("hi")"#],
        0,
        expect![[r#"
            "hi"
        "#]],
    );
    assert_run(
        &["run", "examples/generics.must", "-e", "id::<usize>(4)"],
        0,
        expect![[r#"
            4
        "#]],
    );
    assert_run(
        &["run", "examples/generics.must", "-e", "square_val"],
        0,
        expect![[r#"
            49
        "#]],
    );
    assert_run(
        &["run", "examples/generics.must", "-e", "cube_val"],
        0,
        expect![[r#"
            27
        "#]],
    );
    assert_run(
        &["run", "examples/generics.must", "-e", "braced_val"],
        0,
        expect![[r#"
            49
        "#]],
    );
    assert_run(
        &["run", "examples/generics.must", "-e", "pair_first"],
        0,
        expect![[r#"
            "left"
        "#]],
    );
}

#[test]
fn heap_runs() {
    assert_run(
        &["run", "examples/heap.must"],
        0,
        expect![[r#"
        roundtrip: wrote 3 elements through add, read them back
        heapvec: pushed 6 squares across a growth, get(5) == 25
        arena: carved 3 of 4 elements
        arena: a further 2 don't fit — Err, by value
    "#]],
    );
}

#[test]
fn hello_runs() {
    assert_run(
        &["run", "examples/hello.must"],
        0,
        expect![[r#"
        hello
        +----------------+
        |  Hello, Must!  |
        +----------------+
    "#]],
    );
}

#[test]
fn loops_runs() {
    assert_run(
        &["run", "examples/loops.must", "-e", "sum_to(10)"],
        0,
        expect![[r#"
            45
        "#]],
    );
    assert_run(
        &["run", "examples/loops.must", "-e", "sum_odds_below(10)"],
        0,
        expect![[r#"
            25
        "#]],
    );
    assert_run(
        &["run", "examples/loops.must", "-e", "sum_to_const"],
        0,
        expect![[r#"
            45
        "#]],
    );
    assert_run(
        &[
            "run",
            "examples/loops.must",
            "-e",
            "find_even_multiple(3, 20)",
        ],
        0,
        expect![[r#"
            6
        "#]],
    );
    assert_run(
        &[
            "run",
            "examples/loops.must",
            "-e",
            "find_even_multiple(5, 4)",
        ],
        0,
        expect![[r#"
            0
        "#]],
    );
    assert_run(
        &[
            "run",
            "examples/loops.must",
            "-e",
            "find_even_multiple(0, 20)",
        ],
        0,
        expect![[r#"
            0
        "#]],
    );
}

#[test]
fn match_projection_runs() {
    // `188` = 101 + 33 + 7 + 5 + 42, and every term is a different half of
    // the ruling:
    //
    //   101 — `bump` wrote through a `.&mut` PAYLOAD binding and the owner
    //         saw it, which is the whole aliasing claim;
    //    33 — 11 + 22, two exclusive payload borrows of one variant, live
    //         at once because they name disjoint slots;
    //     7 — a borrow matched, then the BINDING matched again (transitive
    //         projection, spelled as two matches);
    //     5 — an OWNED match moving a noncopyable (`.&mut`) payload out,
    //         which a borrowed match could never produce;
    //    42 — `project_in`'s result outliving the match that made it.
    assert_run(
        &["run", "examples/match_projection.must"],
        0,
        expect![
            "188
"
        ],
    );
    // The `as_ref` shape on its own: `Opt::<usize>.&` in, `Opt::<usize.&>`
    // out, at the caller's full region.
    assert_run(
        &[
            "run",
            "examples/match_projection.must",
            "-e",
            "project_in_demo()",
        ],
        0,
        expect![
            "42
"
        ],
    );
}

#[test]
fn option_runs() {
    // A borrowing member: `is_some` takes `Self.&::<@local>`.
    assert_run(
        &[
            "run",
            "examples/option.must",
            "-e",
            "{ let o: Option::<usize> = Option::Some(1); o.&.is_some() }",
        ],
        0,
        expect![[r#"
            true
        "#]],
    );
    // A consuming member: `unwrap` takes `Self` by value.
    assert_run(
        &[
            "run",
            "examples/option.must",
            "-e",
            "{ let o: Option::<usize> = Option::Some(1); o.unwrap() }",
        ],
        0,
        expect![[r#"
            1
        "#]],
    );
    // The other side of `unwrap`: panics on `::None` (stderr only — the
    // panic message is not part of what this asserts, `errors_runs` above
    // does the same for its own default-entry trap).
    assert_run(
        &[
            "run",
            "examples/option.must",
            "-e",
            "{ let o: Option::<usize> = Option::None; o.unwrap() }",
        ],
        1,
        expect![[r#""#]],
    );
    // A higher-order member: `flat_map` takes a `fn` value as an argument.
    assert_run(
        &[
            "run",
            "examples/option.must",
            "-e",
            "Option::Some(1).flat_map(fn(x: usize) -> Option::<usize> { Option::Some(x + 1) }).unwrap()",
        ],
        0,
        expect![[r#"
            2
        "#]],
    );
}

#[test]
fn pointers_runs() {
    assert_run(
        &["run", "examples/pointers.must"],
        0,
        expect![[r#"
        static: S.&raw == S.&raw is true — one place, one address
        const: C.&raw == C.&raw is false — each mention is its own copy
        write-through: p.* = 42 updated the local x to 42
        aliasing: writing through the copy q is seen through p
        field pointer: r.a.&raw mut wrote only r.a
    "#]],
    );
}

#[test]
fn reborrow_runs() {
    // `101`: `get_or_default(7, 100, ..)` inserts 100 and hands back a
    // borrow OF THE MAP'S SLOT; `a.* = a.* + 1` writes 101 through it; the
    // second lookup finds key 7 present, ignores the 999 default, and reads
    // back what was written. Any other number means the borrow aliased a
    // copy instead of the map.
    assert_run(&["run", "examples/reborrow.must"], 0, expect!["101\n"]);
    // `60`: 20 impl-directed + 20 + 20 bound-directed through the hidden
    // dictionary — the trait half of a borrow-`Self` member, both call
    // directions, actually executing.
    assert_run(
        &["run", "examples/reborrow.must", "-e", "total_count()"],
        0,
        expect!["60\n"],
    );
}

#[test]
fn records_runs() {
    assert_run(
        &["run", "examples/records.must", "-e", "sum_pair"],
        0,
        expect![[r#"
            0
        "#]],
    );
    assert_run(
        &["run", "examples/records.must", "-e", "moved"],
        0,
        expect![[r#"
            { x = 5, y = 0 }
        "#]],
    );
    assert_run(
        &["run", "examples/records.must", "-e", "same_point"],
        0,
        expect![[r#"
            true
        "#]],
    );
}

#[test]
fn state_machine_runs() {
    assert_run(
        &["run", "examples/state_machine.must", "-e", "cycle_once"],
        0,
        expect![[r#"
            ()
        "#]],
    );
    assert_run(
        &[
            "run",
            "examples/state_machine.must",
            "-e",
            "describe(cycle_once)",
        ],
        0,
        expect![[r#"
            "stop"
        "#]],
    );
    assert_run(
        &["run", "examples/state_machine.must", "-e", "run_lights()"],
        0,
        expect![[r#"
            "go"
        "#]],
    );
}

#[test]
fn stdin_runs() {
    // The documented invocation, piped exactly as `// Run:` shows: three
    // lines in, `expected` matches, one summary line out.
    assert_run_with_input(
        &["run", "examples/stdin.must"],
        "a\nb\nc\n",
        0,
        expect!["read exactly the expected line count\n"],
    );
    // A piped stdin is never a terminal, so `run`'s one-time stdin hint
    // stays out of the way entirely: nothing at all reaches stderr.
    let (_stdout, stderr, code) = spawn_with_timeout_input(
        &["run", "examples/stdin.must"],
        "run examples/stdin.must (stderr)",
        Some("a\nb\nc\n"),
    );
    assert_eq!(code, 0);
    assert_eq!(stderr, "", "a piped run should say nothing on stderr");
    // A different line count still terminates cleanly on genuine
    // end-of-input (no hang waiting for a fourth line) and takes the
    // other branch.
    assert_run_with_input(
        &["run", "examples/stdin.must"],
        "a\nb\n",
        0,
        expect!["read fewer lines than expected\n"],
    );
    // No trailing newline on the last line still reads as a `Line`, not a
    // dropped one — `read_line`'s EOF-without-newline case.
    assert_run_with_input(
        &["run", "examples/stdin.must"],
        "a\nb\nc\nd",
        0,
        expect!["read more lines than expected\n"],
    );
}

#[test]
fn stdin_lib_runs() {
    // The documented invocation. Four lines: `alpha` (CRLF-terminated), a
    // blank one, `beta`, and an unterminated `gamma` — every line-reading
    // case that has ever been got wrong, in eighteen bytes through a
    // sixteen-byte buffer, so the input crosses a refill and a partial line
    // is compacted.
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "alpha\r\n\nbeta\ngamma",
        0,
        expect!["lines: 4, blank: 1, bytes: 14\n"],
    );
    // A trailing newline does NOT produce a phantom final line — the
    // difference between "the input ended" and "the last line was empty".
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "a\nb\n",
        0,
        expect!["lines: 2, blank: 0, bytes: 2\n"],
    );
    // Nothing at all: no lines, no hang waiting for input that cannot come.
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "",
        0,
        expect!["lines: 0, blank: 0, bytes: 0\n"],
    );
    // Seven short lines through a sixteen-byte buffer: several refills,
    // each one compacting a partial line to the front.
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\n",
        0,
        expect!["lines: 7, blank: 0, bytes: 27\n"],
    );
    // A line the buffer cannot hold PANICS — not a truncated line, not a
    // hang, not a silent second line. The honest limit of a fixed buffer,
    // and it exits 1 with nothing on stdout. (The message text is pinned in
    // `eval`'s suite, where it does not ride on an example's line numbers.)
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "abcdefghijklmnopqrstuvwxyz\n",
        1,
        expect![[r#""#]],
    );
    // Multi-byte text survives the bless intact — seven bytes, six
    // characters, and the count is of BYTES. (Bytes that are NOT valid
    // UTF-8 are the other half of that story, and cannot be tested here:
    // this harness pipes a Rust `&str`, which is valid UTF-8 by
    // construction. The refusal is pinned in `eval`'s suite instead.)
    assert_run_with_input(
        &["run", "examples/stdin_lib.must"],
        "smørre\n",
        0,
        expect!["lines: 1, blank: 0, bytes: 7\n"],
    );
}

#[test]
fn string_lib_runs() {
    // The documented invocation, through a sixteen-byte buffer: `alpha` is
    // captured on the first line and survives every refill after it, which
    // is the thing a borrowed view into the reader's buffer cannot do.
    assert_run_with_input(
        &["run", "examples/string_lib.must"],
        "alpha\r\n\nbeta\ngamma",
        0,
        expect![[r#"
            lines: 4, longest: "alpha" (5 bytes)
        "#]],
    );
    // The winner is the THIRD of five lines in a sixteen-byte buffer, so
    // the copy outlives at least two refills of the storage it came from.
    assert_run_with_input(
        &["run", "examples/string_lib.must"],
        "one\ntwo\nthree\nfour\nfive\n",
        0,
        expect![[r#"
            lines: 5, longest: "three" (5 bytes)
        "#]],
    );
    // No input: the empty `String` never allocates, and `drop` knows not to
    // free what was never allocated.
    assert_run_with_input(
        &["run", "examples/string_lib.must"],
        "",
        0,
        expect![[r#"
            lines: 0, longest: "" (0 bytes)
        "#]],
    );
    // Multi-byte text survives the copy intact, and the count is of BYTES
    // — `s.len()` answers what `next_char` indexes with.
    assert_run_with_input(
        &["run", "examples/string_lib.must"],
        "ab\nsm\u{f8}rre\n",
        0,
        expect![[r#"
            lines: 2, longest: "smørre" (7 bytes)
        "#]],
    );
}

#[test]
fn chars_runs() {
    // The documented invocation: two lines in, the running paren balance
    // out, plus the two fixed-string demonstrations that need no input.
    assert_run_with_input(
        &["run", "examples/chars.must"],
        "(a(b)c)\n(()\n",
        0,
        expect![[r#"
            paren balance: 1
            left something open
            "smørre": six characters, seven bytes
            a multi-byte character compares equal to itself
        "#]],
    );
    // Balanced input takes the zero branch...
    assert_run_with_input(
        &["run", "examples/chars.must"],
        "(())\n()\n",
        0,
        expect![[r#"
            paren balance: 0
            balanced
            "smørre": six characters, seven bytes
            a multi-byte character compares equal to itself
        "#]],
    );
    // ...and a line that closes more than it opens drives the total
    // NEGATIVE, which is the whole reason the running count is `isize`.
    assert_run_with_input(
        &["run", "examples/chars.must"],
        "())\n",
        0,
        expect![[r#"
            paren balance: -1
            closed more than it opened
            "smørre": six characters, seven bytes
            a multi-byte character compares equal to itself
        "#]],
    );
    // Empty input: no lines, no parentheses, and the loop still terminates
    // on genuine end-of-input.
    assert_run_with_input(
        &["run", "examples/chars.must"],
        "",
        0,
        expect![[r#"
            paren balance: 0
            balanced
            "smørre": six characters, seven bytes
            a multi-byte character compares equal to itself
        "#]],
    );
}
