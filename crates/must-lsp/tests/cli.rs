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
