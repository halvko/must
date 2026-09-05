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
    "compile_time.must",
    "display.must",
    "errors.must",
    "functions.must",
    "generics.must",
    "heap.must",
    "hello.must",
    "loops.must",
    "pointers.must",
    "records.must",
    "state_machine.must",
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
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn must-lsp");

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

// --- check: every example is silent except errors.must ---------------------

#[test]
fn arrays_checks_clean() {
    assert_check("arrays.must", 0, expect![[r#""#]]);
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
              --> examples/errors.must:80:13
               |
            80 |     let y = x + 1;
               |             ^
               = note: `+` requires `{number}` operands (examples/errors.must:80:15)

            error: index out of bounds: the length is 2 but the index is 2
              --> examples/errors.must:92:5
               |
            92 |     a[2]
               |     ^^^^

            error: dereferencing a raw pointer requires an `unsafe { ... }` block
              --> examples/errors.must:102:5
                |
            102 |     p.*
                |     ^^^

            error: cannot take `.&raw mut` of `x`: it is not declared `mut`
              --> examples/errors.must:114:13
                |
            114 |     let p = x.&raw mut;
                |             ^
               = help: Make `x` mutable
               = note: `x` is declared without `mut` here (examples/errors.must:112:9)

            error: type mismatch: expected `Buf::<8>`, found `Buf::<9>`
              --> examples/errors.must:122:36
                |
            122 | static wrong_const_len: Buf::<8> = Buf::<9>(struct { len = 1 });
                |                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               = note: expected `Buf::<8>` because of this annotation (examples/errors.must:122:25)

            error: constant evaluation failed: a pointer cannot leave compile-time evaluation
              --> examples/errors.must:130:33
                |
            130 | static pointer_escaping_const = const {
                |                                 ^^^^^^^

            error: cannot infer the type of this number: it has no defining use — add a type annotation
              --> examples/errors.must:141:13
                |
            141 |     let n = 5;
                |             ^

            error: `300` does not fit in `u8`
              --> examples/errors.must:148:29
                |
            148 | static too_big_for_u8: u8 = 300;
                |                             ^^^

            error: record fields are defined with `=` (`name = value`); `:` annotates a type
              --> examples/errors.must:155:56
                |
            155 | static old_spelling: struct { x: usize } = struct { x: 1 };
                |                                                        ^

            error: unknown trait `Display`
              --> examples/errors.must:162:10
                |
            162 |     impl Display {
                |          ^^^^^^^

            error: a declare-only inherent member is an unimplementable promise; define it: `name = fn(...) -> ... { ... };`
              --> examples/errors.must:173:9
                |
            173 |         len: fn(p: Self) -> usize;
                |         ^^^^^^^^^^^^^^^^^^^^^^^^^^

            error: no field or member `plain_len` on `Opted`
              --> examples/errors.must:183:47
                |
            183 | static dotted_call = fn (o: Opted) -> usize { o.plain_len() };
                |                                               ^^^^^^^^^^^^^
               = note: a module-level `plain_len` is defined here — statics are never dot-callable; call `plain_len(...)` instead (examples/errors.must:182:8)

            error: `scaled` is not dot-callable: its last parameter is not `Self`-typed (dot-call resolution is structural)
              --> examples/errors.must:194:56
                |
            194 | static wrong_self_position = fn (s: Scaler) -> usize { s.scaled(2) };
                |                                                        ^^^^^^^^^^^
               = note: `scaled` is defined here (examples/errors.must:191:9)

            error: raw borrows are spelled postfix: `x.&raw` / `x.&raw mut`
              --> examples/errors.must:203:13
                |
            203 |     let p = &raw mut x;
                |             ^

            error: `return` inside a `const` block is not supported yet: it would have to leave the enclosing `fn` body, and a `const` block is compiled as a body of its own
              --> examples/errors.must:214:23
                |
            214 |     const { if true { return 1; }; 0 }
                |                       ^^^^^^^^

            error: generic arguments belong to the owner, not the second segment: write `Owner::<...>::name` (a member's own generic arguments are not supported yet)
              --> examples/errors.must:227:69
                |
            227 | static member_own_turbofish = fn () -> () { let f = Measured::size::<usize>; };
                |                                                                     ^^^^^^^

            error: `len` is a field of `Sized`, not a member — fields are reached through a value: `value.len`
              --> examples/errors.must:235:55
                |
            235 | static field_through_the_type = fn () -> () { let n = Sized::len; };
                |                                                       ^^^^^^^^^^
               = note: `Sized` is defined here (examples/errors.must:234:6)

            error: `Self` names the implementer, so it cannot be `_`: write the type (`Trait::<Self = Type>::member`), or use the short form `Trait::member(...)` where an argument determines `Self`
              --> examples/errors.must:248:45
                |
            248 | static self_hole = fn (n: usize) -> usize { Countable::<Self = _>::count(n) };
                |                                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

            found 24 errors and 1 warning
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
fn pointers_checks_clean() {
    assert_check("pointers.must", 0, expect![[r#""#]]);
}

#[test]
fn records_checks_clean() {
    assert_check("records.must", 0, expect![[r#""#]]);
}

#[test]
fn state_machine_checks_clean() {
    assert_check("state_machine.must", 0, expect![[r#""#]]);
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
