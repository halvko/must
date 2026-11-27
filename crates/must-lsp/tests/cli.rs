//! CLI tests that need a real process: exit codes, and the interleaving of
//! stdout with stderr. The in-process `runner::tests` harness collects both
//! into one buffer in call order, so it cannot see ordering — only two
//! independently buffered streams sharing a terminal can.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_must");

/// Write `text` to a uniquely named temporary `.must` file.
fn fixture(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("must-cli-{}-{name}.must", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(text.as_bytes()).expect("write fixture");
    path
}

/// Run `must` with stdout and stderr pointed at the *same* file, the
/// way a terminal joins them. Returns (merged output, exit code).
fn run_merged(args: &[&str]) -> (String, i32) {
    // Tests run in parallel, so the sink must be unique per call — not per
    // test and not per argument shape.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out_path =
        std::env::temp_dir().join(format!("must-cli-{}-{serial}.out", std::process::id()));
    let file = std::fs::File::create(&out_path).expect("create output file");
    let err = file.try_clone().expect("clone handle");
    let status = Command::new(BIN)
        .args(args)
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(err))
        .status()
        .expect("spawn must");
    let text = std::fs::read_to_string(&out_path).expect("read output");
    let _ = std::fs::remove_file(&out_path);
    (text, status.code().expect("exited normally"))
}

#[test]
fn help_exits_zero() {
    let (help, code) = run_merged(&["--help"]);
    assert_eq!(code, 0, "--help exits 0: {help}");
    assert!(help.contains("must run <file.must>"), "{help}");
}

#[test]
fn a_bare_invocation_is_a_usage_error_not_a_server() {
    let output = Command::new(BIN)
        .stdin(Stdio::null())
        .output()
        .expect("spawn must");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(output.stdout.is_empty(), "nothing on stdout: {output:?}");
    assert!(stderr.starts_with("error: no subcommand given"), "{stderr}");
    assert!(stderr.contains("try `must --help`"), "{stderr}");
}

/// Frame one JSON-RPC message the way LSP's base protocol does.
fn lsp_frame(json: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{json}", json.len())
}

#[test]
fn lsp_serves_and_exits_cleanly() {
    let mut child = Command::new(BIN)
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn must lsp");
    let session = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ]
    .map(lsp_frame)
    .concat();
    // Dropping stdin after the write closes it, as an editor does on exit.
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(session.as_bytes())
        .expect("write the session");
    let output = child.wait_with_output().expect("wait for must lsp");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.contains(r#""capabilities""#),
        "initialize is answered with the server's capabilities:\n{stdout}"
    );
}

#[test]
fn program_output_precedes_the_crash_report() {
    // The regression this pins: `print` appends no newline, so a program
    // that crashes mid-line leaves its output in stdout's line buffer.
    // stderr is unbuffered, so without an explicit flush the error report
    // reaches the terminal FIRST and the output that led to it appears
    // after — bytes all present, order inverted.
    let path = fixture(
        "partial",
        r#"static main = fn { print("partial"); let v: usize = "s"; };"#,
    );
    let (merged, code) = run_merged(&["run", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 1, "a trap exits 1: {merged}");
    assert!(
        merged.starts_with("partial"),
        "program output must precede the crash report, got:\n{merged}"
    );
    assert!(
        merged.contains("type mismatch: expected `usize`, found `str`"),
        "the trap is still reported:\n{merged}"
    );
}

#[test]
fn a_clean_run_writes_only_what_the_program_wrote() {
    // No trailing newline anywhere: `print` adds nothing, and the final
    // partial line still reaches the terminal at exit.
    let path = fixture("exact", r#"static main = fn { print("a"); print("b"); };"#);
    let (merged, code) = run_merged(&["run", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 0);
    assert_eq!(merged, "ab");
}

/// A fresh directory holding `files` (`(path, text)`, paths relative to it).
fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("must-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture directory");
        std::fs::write(&path, text).expect("write fixture");
    }
    dir
}

#[test]
fn run_loads_the_library_of_the_file_it_is_given() {
    // The Advent of Code shape: each day imports a shared module, and the
    // day being run sits in a subdirectory of the library.
    let dir = project(
        "library",
        &[
            ("must.toml", ""),
            (
                "util.must",
                r#"
static twice = fn (n: usize) -> usize { n * 2 };
"#,
            ),
            (
                "days/day1.must",
                r#"
use util::twice;
static main = fn () -> usize { twice(21) };
"#,
            ),
        ],
    );
    let day1 = dir.join("days/day1.must");
    let (merged, code) = run_merged(&["run", day1.to_str().unwrap()]);
    assert_eq!((merged.as_str(), code), ("42\n", 0));
    // `check` loads the same library (`compile` too: see tests/compile.rs).
    let (merged, code) = run_merged(&["check", day1.to_str().unwrap()]);
    assert_eq!((merged.as_str(), code), ("", 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_reports_every_error_of_the_library() {
    let dir = project(
        "check-library",
        &[
            ("must.toml", ""),
            (
                "days/day1.must",
                r#"
static main = fn () -> () { let n: usize = "s"; };
"#,
            ),
            (
                "days/day2.must",
                r#"
static main = fn () -> () { let m: usize = "s"; };
"#,
            ),
            (
                "days/day3.must",
                r#"
static main = fn () -> usize { 3 };
"#,
            ),
        ],
    );
    // Named day2 first: its error comes first, then the rest of the library.
    let day2 = dir.join("days/day2.must");
    let (merged, code) = run_merged(&["check", day2.to_str().unwrap()]);
    assert_eq!(code, 1, "{merged}");
    let at_day2 = merged.find("day2.must:2:44").expect(&merged);
    let at_day1 = merged.find("day1.must:2:44").expect(&merged);
    assert!(at_day2 < at_day1, "{merged}");
    // `run` judges only what it runs.
    let day3 = dir.join("days/day3.must");
    let (merged, code) = run_merged(&["run", day3.to_str().unwrap()]);
    assert_eq!((merged.as_str(), code), ("3\n", 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_trap_in_another_file_of_the_library_is_located_there() {
    let dir = project(
        "elsewhere",
        &[
            ("must.toml", ""),
            (
                "util.must",
                r#"
static f = fn () -> usize {
    let v: usize = "s";
    v
};
"#,
            ),
            (
                "main.must",
                r#"
use util::f;
static main = fn { f(); };
"#,
            ),
        ],
    );
    let main = dir.join("main.must");
    let (merged, code) = run_merged(&["run", main.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 1, "{merged}");
    assert!(merged.contains("util.must:3:20"), "{merged}");
}

#[test]
fn a_nested_manifest_starts_another_library() {
    // `inner/` is a library of its own: the outer library has no module
    // `inner::lib`, and the inner one is not part of it.
    let dir = project(
        "nested",
        &[
            ("must.toml", ""),
            ("inner/must.toml", ""),
            (
                "inner/lib.must",
                r#"
static one = fn () -> usize { 1 };
"#,
            ),
            (
                "main.must",
                r#"
use inner::lib::one;
static main = fn () -> usize { one() };
"#,
            ),
        ],
    );
    let main = dir.join("main.must");
    let (merged, code) = run_merged(&["check", main.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 1, "{merged}");
    assert!(
        merged.contains("error: no module `inner::lib` in this library to import from"),
        "{merged}"
    );
}

#[test]
fn a_standalone_file_cannot_import() {
    // The `use` is an error at `check` and a trap at `run`, where the
    // imported name is used; a same-named local item is no way around it.
    let dir = project(
        "standalone",
        &[
            (
                "util.must",
                r#"
static one = fn () -> usize { 1 };
"#,
            ),
            (
                "main.must",
                r#"
use util::one;
static one = fn () -> usize { 2 };
static main = fn () -> usize { one() };
"#,
            ),
        ],
    );
    let main = dir.join("main.must");
    let message = "this file isn't in a project (no `must.toml` above it), \
                   so it cannot import from `util`";
    let (checked, check_code) = run_merged(&["check", main.to_str().unwrap()]);
    let (ran, run_code) = run_merged(&["run", main.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(check_code, 1, "{checked}");
    assert!(checked.contains(&format!("error: {message}")), "{checked}");
    assert!(
        checked.contains("= help: Create `must.toml` here, making this directory a project"),
        "{checked}"
    );
    assert_eq!(run_code, 1, "{ran}");
    assert!(ran.contains(message), "{ran}");
}

#[test]
fn a_file_the_walk_does_not_reach_is_refused() {
    // Under a `must.toml` but in a hidden directory: not a module of the
    // library, and not standalone either.
    let dir = project(
        "hidden",
        &[
            ("must.toml", ""),
            (
                ".scratch/main.must",
                r#"
static main = fn {};
"#,
            ),
        ],
    );
    let main = dir.join(".scratch/main.must");
    let (merged, code) = run_merged(&["run", main.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 2, "{merged}");
    assert!(
        merged.contains("is not a module of the library at"),
        "{merged}"
    );
}

#[test]
fn a_manifest_with_keys_is_refused() {
    let dir = project(
        "manifest",
        &[
            (
                "must.toml",
                r#"
name = "aoc"
"#,
            ),
            (
                "main.must",
                r#"
static main = fn {};
"#,
            ),
        ],
    );
    let main = dir.join("main.must");
    let (merged, code) = run_merged(&["run", main.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 2, "{merged}");
    assert!(
        merged.contains("must.toml: unknown key `name`: `must.toml` has no keys"),
        "{merged}"
    );
}
