//! CLI tests that need a real subprocess: exit codes are the process's own
//! contract, not something the in-process `runner::tests` harness (which
//! only ever calls into library code) can observe.

use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_must-lsp");

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
