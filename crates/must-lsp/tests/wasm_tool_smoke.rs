//! Smoke tests for `tools/must-wasm.js`, the compile/run/trap-decode engine
//! shared by `tools/wasm-run.mjs` (a Node CLI) and `tools/playground.html`
//! (a browser page, which loads it as a plain `<script src="must-wasm.js">`
//! rather than duplicating it): compile a real example, run it through the
//! engine, and check the result against the interpreter.
//!
//! This is deliberately shallow — the ABI itself (traps, multi-value
//! results, widths) is proven by `crates/codegen-wasm/tests`, and the
//! process-level `compile` path by `tests/compile.rs`. What this file
//! proves is that the shared engine both tools actually reach for still
//! works end to end against the current backend, and that the CLI wrapper
//! around it (`wasm-run.mjs`) reports the same thing on the command line.
//!
//! `node` is an external, optional dependency of this workspace — only the
//! playground tooling needs it, never the compiler itself — so if it isn't
//! on PATH, these tests print a note and skip rather than failing the
//! suite.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_must-lsp");

/// A fixture that overflows a `u8`, used to exercise trap decoding.
const OVERFLOW_TRAP_SOURCE: &str =
    "static main = fn () -> u8 {\n    let x: u8 = 255;\n    x + 1\n};\n";

/// A minimal CommonJS driver: `require`s `tools/must-wasm.js` directly
/// (the same file `tools/playground.html` loads via `<script src>`), calls
/// `runModule` the same way both the CLI and the page's Run button do, and
/// prints one JSON line shaped like `wasm-run.mjs --json`'s output so both
/// entry points can be checked with the same assertions.
const ENGINE_DRIVER: &str = r#"
const fs = require("fs");
const { runModule } = require(process.argv[2]);
const bytes = fs.readFileSync(process.argv[3]);
let stdout = "";
runModule(bytes, (text) => { stdout += text; }).then((outcome) => {
  process.stdout.write(JSON.stringify({ ...outcome, stdout }) + "\n");
  process.exitCode = outcome.ok ? 0 : 1;
}).catch((err) => {
  process.stderr.write(String((err && err.stack) || err) + "\n");
  process.exitCode = 2;
});
"#;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn compile(example: &Path, wasm: &Path) {
    let output = Command::new(BIN)
        .args([
            "compile",
            example.to_str().unwrap(),
            "-o",
            wasm.to_str().unwrap(),
        ])
        .output()
        .expect("spawn must-lsp compile");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_overflow_fixture(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let source = dir.join(format!("must-wasm-{tag}-trap-{}.must", std::process::id()));
    let wasm = dir.join(format!("must-wasm-{tag}-trap-{}.wasm", std::process::id()));
    std::fs::write(&source, OVERFLOW_TRAP_SOURCE).expect("write fixture");
    compile(&source, &wasm);
    (source, wasm)
}

#[test]
fn wasm_run_mjs_matches_the_interpreter_on_display_must() {
    if !node_available() {
        println!("note: `node` not found on PATH; skipping the tools/wasm-run.mjs smoke test");
        return;
    }

    let root = workspace_root();
    let example = root.join("examples/display.must");
    let wasm =
        std::env::temp_dir().join(format!("must-wasm-run-smoke-{}.wasm", std::process::id()));
    compile(&example, &wasm);

    let interpreted = Command::new(BIN)
        .args(["run", example.to_str().unwrap()])
        .output()
        .expect("spawn must-lsp run");
    assert!(
        interpreted.status.success(),
        "the interpreter should run display.must cleanly: {}",
        String::from_utf8_lossy(&interpreted.stderr)
    );

    let tool = root.join("tools/wasm-run.mjs");
    let ran = Command::new("node")
        .arg(&tool)
        .arg(&wasm)
        .output()
        .expect("spawn node tools/wasm-run.mjs");
    assert!(
        ran.status.success(),
        "wasm-run.mjs should exit 0 on a clean run: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        ran.stdout, interpreted.stdout,
        "tools/wasm-run.mjs's must.print output must match the interpreter byte-for-byte"
    );

    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn wasm_run_mjs_decodes_a_trap() {
    if !node_available() {
        println!("note: `node` not found on PATH; skipping the tools/wasm-run.mjs trap smoke test");
        return;
    }

    let (source, wasm) = write_overflow_fixture("run");

    let tool = workspace_root().join("tools/wasm-run.mjs");
    let ran = Command::new("node")
        .arg(&tool)
        .arg(&wasm)
        .arg("--json")
        .output()
        .expect("spawn node tools/wasm-run.mjs --json");
    assert_eq!(
        ran.status.code(),
        Some(1),
        "a trapping module must exit 1: stdout={} stderr={}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let json = String::from_utf8(ran.stdout).expect("stdout is UTF-8");
    assert!(
        json.contains("\"kind\":\"Overflow\""),
        "the decoded trap must name its kind:\n{json}"
    );
    assert!(
        json.contains("arithmetic overflow"),
        "the decoded trap must carry the interpreter's message prefix:\n{json}"
    );

    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn must_wasm_js_matches_the_interpreter_on_display_must() {
    if !node_available() {
        println!("note: `node` not found on PATH; skipping the tools/must-wasm.js smoke test");
        return;
    }

    let root = workspace_root();
    let example = root.join("examples/display.must");
    let wasm = std::env::temp_dir().join(format!(
        "must-wasm-engine-smoke-{}.wasm",
        std::process::id()
    ));
    compile(&example, &wasm);

    let interpreted = Command::new(BIN)
        .args(["run", example.to_str().unwrap()])
        .output()
        .expect("spawn must-lsp run");
    assert!(
        interpreted.status.success(),
        "the interpreter should run display.must cleanly: {}",
        String::from_utf8_lossy(&interpreted.stderr)
    );

    let engine = root.join("tools/must-wasm.js");
    let driver = std::env::temp_dir().join(format!(
        "must-wasm-engine-driver-{}.cjs",
        std::process::id()
    ));
    std::fs::write(&driver, ENGINE_DRIVER).expect("write engine driver");

    let ran = Command::new("node")
        .arg(&driver)
        .arg(&engine)
        .arg(&wasm)
        .output()
        .expect("spawn node engine driver");
    assert!(
        ran.status.success(),
        "tools/must-wasm.js should run display.must cleanly: stdout={} stderr={}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let json = String::from_utf8(ran.stdout).expect("stdout is UTF-8");
    assert!(
        json.contains("\"ok\":true"),
        "expected a clean run:\n{json}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("driver prints one JSON line");
    assert_eq!(
        parsed["stdout"].as_str(),
        Some(String::from_utf8_lossy(&interpreted.stdout).as_ref()),
        "must-wasm.js's must.print output must match the interpreter byte-for-byte:\n{json}"
    );

    let _ = std::fs::remove_file(&wasm);
    let _ = std::fs::remove_file(&driver);
}

#[test]
fn must_wasm_js_decodes_a_trap() {
    if !node_available() {
        println!("note: `node` not found on PATH; skipping the tools/must-wasm.js trap smoke test");
        return;
    }

    let root = workspace_root();
    let (source, wasm) = write_overflow_fixture("engine");

    let engine = root.join("tools/must-wasm.js");
    let driver = std::env::temp_dir().join(format!(
        "must-wasm-engine-driver-trap-{}.cjs",
        std::process::id()
    ));
    std::fs::write(&driver, ENGINE_DRIVER).expect("write engine driver");

    let ran = Command::new("node")
        .arg(&driver)
        .arg(&engine)
        .arg(&wasm)
        .output()
        .expect("spawn node engine driver");
    assert_eq!(
        ran.status.code(),
        Some(1),
        "a trapping module must report ok:false: stdout={} stderr={}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let json = String::from_utf8(ran.stdout).expect("stdout is UTF-8");
    assert!(
        json.contains("\"kind\":\"Overflow\""),
        "the decoded trap must name its kind:\n{json}"
    );
    assert!(
        json.contains("arithmetic overflow"),
        "the decoded trap must carry the interpreter's message prefix:\n{json}"
    );

    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
    let _ = std::fs::remove_file(&driver);
}

/// Not a Node test (runs with or without `node` on PATH): guards the wiring
/// between the two files by grepping the page's markup, since the page's
/// own inline script is DOM wiring only and has nothing left worth running
/// under Node once the engine it calls has its own tests above.
#[test]
fn playground_loads_the_shared_engine() {
    let root = workspace_root();
    let html = std::fs::read_to_string(root.join("tools/playground.html"))
        .expect("read tools/playground.html");
    assert!(
        html.contains("<script src=\"must-wasm.js\">"),
        "tools/playground.html must load tools/must-wasm.js rather than duplicating its decoder"
    );
    assert!(
        root.join("tools/must-wasm.js").is_file(),
        "tools/playground.html references tools/must-wasm.js, which must exist"
    );
}
