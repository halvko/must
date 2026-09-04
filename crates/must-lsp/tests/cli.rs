//! CLI tests that need a real process: exit codes, and the interleaving of
//! stdout with stderr. The in-process `runner::tests` harness collects both
//! into one buffer in call order, so it cannot see ordering — only two
//! independently buffered streams sharing a terminal can.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_must-lsp");

/// Write `text` to a uniquely named temporary `.must` file.
fn fixture(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("must-cli-{}-{name}.must", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(text.as_bytes()).expect("write fixture");
    path
}

/// Run `must-lsp` with stdout and stderr pointed at the *same* file, the
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
        .expect("spawn must-lsp");
    let text = std::fs::read_to_string(&out_path).expect("read output");
    let _ = std::fs::remove_file(&out_path);
    (text, status.code().expect("exited normally"))
}

#[test]
fn help_exits_zero_and_usage_errors_exit_two() {
    let (help, code) = run_merged(&["--help"]);
    assert_eq!(code, 0, "--help exits 0: {help}");
    assert!(help.contains("must-lsp run <file.must>"), "{help}");

    let (msg, code) = run_merged(&["frobnicate"]);
    assert_eq!(code, 2, "an unknown subcommand exits 2: {msg}");
    assert!(msg.contains("try `must-lsp --help`"), "{msg}");

    let (msg, code) = run_merged(&["--serve-harder"]);
    assert_eq!(code, 2, "an unknown flag exits 2: {msg}");
    assert!(msg.contains("try `must-lsp --help`"), "{msg}");
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
