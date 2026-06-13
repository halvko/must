//! In-process DAP session tests: the loop is single-threaded and never
//! waits on the client mid-request, so a fully pre-buffered message
//! sequence exercises the real session end to end.

use serde_json::{Value, json};

/// Frames `requests` (auto-numbering seq), runs a session, returns every
/// message the adapter sent.
fn run_session(requests: &[(&str, Value)]) -> Vec<Value> {
    let mut input = Vec::new();
    for (i, (command, arguments)) in requests.iter().enumerate() {
        let message = json!({
            "seq": i as i64 + 1,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        let body = serde_json::to_vec(&message).unwrap();
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    super::run(&input[..], &mut output).expect("session failed");
    parse_frames(&output)
}

fn parse_frames(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Some(start) = find(bytes, b"\r\n\r\n") {
        let header = std::str::from_utf8(&bytes[..start]).unwrap();
        let len: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let body_start = start + 4;
        messages.push(serde_json::from_slice(&bytes[body_start..body_start + len]).unwrap());
        bytes = &bytes[body_start + len..];
    }
    messages
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn fixture(name: &str, text: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("must-dap-test-{name}-{}.must", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path
}

fn responses_for<'a>(messages: &'a [Value], command: &str) -> Vec<&'a Value> {
    messages
        .iter()
        .filter(|m| m["type"] == "response" && m["command"] == command)
        .collect()
}

fn events<'a>(messages: &'a [Value], name: &str) -> Vec<&'a Value> {
    messages
        .iter()
        .filter(|m| m["type"] == "event" && m["event"] == name)
        .collect()
}

#[test]
fn launch_session_runs_the_program() {
    let program = fixture("hello", r#"static main = fn { print("hello from dap"); };"#);
    let messages = run_session(&[
        ("initialize", json!({ "adapterID": "must" })),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "entry": "main()" }),
        ),
        ("setExceptionBreakpoints", json!({ "filters": [] })),
        ("configurationDone", json!({})),
        ("disconnect", json!({})),
    ]);

    // Capabilities + the initialized event.
    let init = &responses_for(&messages, "initialize")[0];
    assert_eq!(init["success"], true);
    assert_eq!(init["body"]["supportsConfigurationDoneRequest"], true);
    assert_eq!(events(&messages, "initialized").len(), 1);

    // Every request got exactly one successful response, launch included
    // (answered only after configurationDone).
    for command in [
        "launch",
        "setExceptionBreakpoints",
        "configurationDone",
        "disconnect",
    ] {
        let responses = responses_for(&messages, command);
        assert_eq!(responses.len(), 1, "one response for {command}");
        assert_eq!(responses[0]["success"], true, "{command} succeeded");
    }

    // Program output streamed as an output event; clean exit; terminated.
    let output = events(&messages, "output");
    assert!(
        output
            .iter()
            .any(|e| e["body"]["output"] == "hello from dap\n"
                && e["body"]["category"] == "stdout"),
        "print output reaches the console: {output:?}"
    );
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);
    assert_eq!(events(&messages, "terminated").len(), 1);

    // seq numbering is strictly increasing.
    let seqs: Vec<i64> = messages
        .iter()
        .map(|m| m["seq"].as_i64().unwrap())
        .collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "seqs increase: {seqs:?}"
    );

    let _ = std::fs::remove_file(program);
}

#[test]
fn deferred_errors_stop_at_the_crash_site_then_terminate_on_resume() {
    let program = fixture(
        "broken",
        "static main = fn {\n    print(\"before\");\n    let v: usize = \"s\";\n};\n",
    );
    let messages = run_session(&[
        ("initialize", json!({})),
        ("launch", json!({ "program": program.to_str().unwrap() })),
        ("configurationDone", json!({})),
        // While stopped at the exception: look around.
        ("stackTrace", json!({ "threadId": 1 })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    let output = events(&messages, "output");
    assert!(
        output
            .iter()
            .any(|e| e["body"]["output"] == "before\n" && e["body"]["category"] == "stdout"),
        "code before the trap ran"
    );
    let stderr: String = output
        .iter()
        .filter(|e| e["body"]["category"] == "stderr")
        .map(|e| e["body"]["output"].as_str().unwrap())
        .collect();
    assert!(
        stderr.contains("type mismatch: expected `usize`, found `str`"),
        "crash carries the diagnostic: {stderr}"
    );

    // Stop-on-trap: a stopped(exception) carrying the diagnostic...
    let stopped = events(&messages, "stopped");
    assert_eq!(stopped[0]["body"]["reason"], "exception");
    assert!(
        stopped[0]["body"]["description"]
            .as_str()
            .unwrap()
            .contains("type mismatch"),
    );
    // ...with the crash site on the stack (line 3 = the broken let).
    let stack = &responses_for(&messages, "stackTrace")[0]["body"]["stackFrames"];
    assert_eq!(stack[0]["name"], "main");
    assert_eq!(stack[0]["line"], 3);

    // Resuming a dead program ends it.
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 1);
    assert_eq!(events(&messages, "terminated").len(), 1);

    let _ = std::fs::remove_file(program);
}

#[test]
fn breakpoint_hit_inspect_and_resume() {
    let program = fixture(
        "bp",
        "static double = fn (n: usize) -> usize {\n    let twice = n * 2;\n    twice\n}\nstatic main = fn {\n    print(\"start\");\n    double(21);\n    print(\"end\");\n};\n",
    );
    let messages = run_session(&[
        ("initialize", json!({})),
        ("launch", json!({ "program": program.to_str().unwrap() })),
        (
            "setBreakpoints",
            // Line 2 is `let twice = n * 2;` (executable); line 4 is `}`.
            json!({ "breakpoints": [{ "line": 2 }, { "line": 4 }] }),
        ),
        ("configurationDone", json!({})),
        ("stackTrace", json!({ "threadId": 1 })),
        ("scopes", json!({ "frameId": 3 })),
        ("variables", json!({ "variablesReference": 3 })),
        ("evaluate", json!({ "expression": "n", "frameId": 3 })),
        ("evaluate", json!({ "expression": "n + 1", "frameId": 3 })),
        (
            "evaluate",
            json!({ "expression": "double(n) == 42", "frameId": 3 }),
        ),
        ("evaluate", json!({ "expression": "double(4)" })),
        ("next", json!({ "threadId": 1 })),
        ("variables", json!({ "variablesReference": 3 })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    // Verification: executable line verified, the closing brace not.
    let bps = &responses_for(&messages, "setBreakpoints")[0]["body"]["breakpoints"];
    assert_eq!(bps[0]["verified"], true);
    assert_eq!(bps[0]["line"], 2);
    let bp_id = bps[0]["id"].as_i64().unwrap();

    // The hit: stopped(breakpoint) naming the id.
    let stopped = events(&messages, "stopped");
    assert_eq!(stopped[0]["body"]["reason"], "breakpoint");
    assert_eq!(stopped[0]["body"]["hitBreakpointIds"], json!([bp_id]));

    // Stack: double() on line 2, called from main on line 7.
    let stack = &responses_for(&messages, "stackTrace")[0]["body"]["stackFrames"];
    assert_eq!(stack[0]["name"], "double");
    assert_eq!(stack[0]["line"], 2);
    assert_eq!(stack[1]["name"], "main");
    assert_eq!(stack[1]["line"], 7);

    // Variables before the let executes: just the param.
    let vars = &responses_for(&messages, "variables")[0]["body"]["variables"];
    assert_eq!(vars.as_array().unwrap().len(), 1);
    assert_eq!(vars[0]["name"], "n");
    assert_eq!(vars[0]["value"], "21");

    // Console: a local by name, compound expressions over frame locals
    // (the whole point), and a plain file-scope expression.
    let evals = responses_for(&messages, "evaluate");
    assert_eq!(evals[0]["body"]["result"], "21");
    assert_eq!(evals[1]["body"]["result"], "22");
    assert_eq!(evals[2]["body"]["result"], "true");
    assert_eq!(evals[3]["body"]["result"], "8");

    // After step-over, `twice` exists.
    let vars_after = &responses_for(&messages, "variables")[1]["body"]["variables"];
    let names: Vec<&str> = vars_after
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["n", "twice"]);
    assert_eq!(events(&messages, "stopped")[1]["body"]["reason"], "step");

    // Continue runs to the end: both prints, clean exit.
    let stdout: String = events(&messages, "output")
        .iter()
        .filter(|e| e["body"]["category"] == "stdout")
        .map(|e| e["body"]["output"].as_str().unwrap())
        .collect();
    assert_eq!(stdout, "start\nend\n");
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

#[test]
fn column_breakpoints_distinguish_calls_on_one_line() {
    let program = fixture(
        "cols",
        "static f = fn (n: usize) -> usize { n + 1 }\nstatic main = fn {\n    let x = f(1) + f(2);\n    print(\"done\");\n};\n",
    );
    // Line 3 columns: 13 = `f(1)` (also where the `+` and the let-init
    // anchor), 20 = `f(2)`.
    let messages = run_session(&[
        ("initialize", json!({})),
        ("launch", json!({ "program": program.to_str().unwrap() })),
        ("breakpointLocations", json!({ "source": {}, "line": 3 })),
        (
            "setBreakpoints",
            json!({ "breakpoints": [{ "line": 3, "column": 20 }] }),
        ),
        ("configurationDone", json!({})),
        ("stackTrace", json!({ "threadId": 1 })),
        ("next", json!({ "threadId": 1, "granularity": "statement" })),
        ("stackTrace", json!({ "threadId": 1 })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    // The picker gets both call positions on the line.
    let locations = &responses_for(&messages, "breakpointLocations")[0]["body"]["breakpoints"];
    assert_eq!(locations[0], json!({ "line": 3, "column": 13 }));
    assert_eq!(locations[1], json!({ "line": 3, "column": 20 }));

    // The column breakpoint verifies, with its column echoed.
    let bp = &responses_for(&messages, "setBreakpoints")[0]["body"]["breakpoints"][0];
    assert_eq!(bp["verified"], true);
    assert_eq!(bp["column"], 20);

    // f(1) runs through unbroken; we stop exactly before f(2).
    let stopped = events(&messages, "stopped");
    assert_eq!(stopped[0]["body"]["reason"], "breakpoint");
    let stack = &responses_for(&messages, "stackTrace")[0]["body"]["stackFrames"];
    assert_eq!(stack[0]["name"], "main");
    assert_eq!(stack[0]["line"], 3);
    assert_eq!(stack[0]["column"], 20);

    // A statement-granular step-over stays on the line: after f(2)
    // returns, the addition (anchored at column 13) is next.
    assert_eq!(stopped[1]["body"]["reason"], "step");
    let stack = &responses_for(&messages, "stackTrace")[1]["body"]["stackFrames"];
    assert_eq!(stack[0]["line"], 3);
    assert_eq!(stack[0]["column"], 13);

    // And the rest runs out clean.
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

#[test]
fn stop_on_entry_stops_at_the_first_user_line() {
    let program = fixture("entry", "static main = fn {\n    print(\"hi\");\n};\n");
    let messages = run_session(&[
        ("initialize", json!({})),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "stopOnEntry": true }),
        ),
        ("configurationDone", json!({})),
        ("stackTrace", json!({ "threadId": 1 })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    let stopped = events(&messages, "stopped");
    assert_eq!(stopped[0]["body"]["reason"], "entry");
    let stack = &responses_for(&messages, "stackTrace")[0]["body"]["stackFrames"];
    assert_eq!(stack[0]["name"], "main");
    assert_eq!(stack[0]["line"], 2);
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

const COUNTDOWN: &str =
    "static down = fn (n: usize) -> usize {\n    if n == 0 { 0 } else { down(n - 1) }\n}\n";

#[test]
fn conditional_breakpoints_stop_only_when_true() {
    let program = fixture("cond", COUNTDOWN);
    let messages = run_session(&[
        ("initialize", json!({})),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "entry": "down(5)" }),
        ),
        (
            "setBreakpoints",
            json!({ "breakpoints": [{ "line": 2, "condition": "n == 2" }] }),
        ),
        ("configurationDone", json!({})),
        ("evaluate", json!({ "expression": "n" })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    // Arrivals at n = 5, 4, 3 pass silently; the stop is at n == 2.
    let stopped = events(&messages, "stopped");
    assert_eq!(stopped.len(), 1, "exactly one stop: {stopped:?}");
    assert_eq!(stopped[0]["body"]["reason"], "breakpoint");
    assert_eq!(
        responses_for(&messages, "evaluate")[0]["body"]["result"],
        "2"
    );
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

#[test]
fn hit_conditions_skip_arrivals() {
    let program = fixture("hits", COUNTDOWN);
    let messages = run_session(&[
        ("initialize", json!({})),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "entry": "down(5)" }),
        ),
        (
            "setBreakpoints",
            // Stop on the third arrival only: n = 5, 4, then 3.
            json!({ "breakpoints": [{ "line": 2, "hitCondition": "3" }] }),
        ),
        ("configurationDone", json!({})),
        ("evaluate", json!({ "expression": "n" })),
        ("continue", json!({ "threadId": 1 })),
        ("disconnect", json!({})),
    ]);

    let stopped = events(&messages, "stopped");
    assert_eq!(stopped.len(), 1, "exactly one stop: {stopped:?}");
    assert_eq!(
        responses_for(&messages, "evaluate")[0]["body"]["result"],
        "3"
    );
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

#[test]
fn log_points_emit_without_stopping() {
    let program = fixture("log", COUNTDOWN);
    let messages = run_session(&[
        ("initialize", json!({})),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "entry": "down(2)" }),
        ),
        (
            "setBreakpoints",
            json!({ "breakpoints": [{ "line": 2, "logMessage": "n is {n}, doubled {n * 2}" }] }),
        ),
        ("configurationDone", json!({})),
        ("disconnect", json!({})),
    ]);

    assert_eq!(
        events(&messages, "stopped").len(),
        0,
        "log points don't stop"
    );
    let stdout: String = events(&messages, "output")
        .iter()
        .filter(|e| e["body"]["category"] == "stdout")
        .map(|e| e["body"]["output"].as_str().unwrap())
        .collect();
    assert!(
        stdout.contains("n is 2, doubled 4\n"),
        "interpolated: {stdout}"
    );
    assert!(
        stdout.contains("n is 0, doubled 0\n"),
        "interpolated: {stdout}"
    );
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 0);

    let _ = std::fs::remove_file(program);
}

#[test]
fn invalid_hit_conditions_unverify_the_breakpoint() {
    let program = fixture("badhits", COUNTDOWN);
    let messages = run_session(&[
        ("initialize", json!({})),
        (
            "launch",
            json!({ "program": program.to_str().unwrap(), "entry": "down(1)" }),
        ),
        (
            "setBreakpoints",
            json!({ "breakpoints": [{ "line": 2, "hitCondition": "sometimes" }] }),
        ),
        ("configurationDone", json!({})),
        ("disconnect", json!({})),
    ]);
    let bp = &responses_for(&messages, "setBreakpoints")[0]["body"]["breakpoints"][0];
    assert_eq!(bp["verified"], false);
    assert!(
        bp["message"]
            .as_str()
            .unwrap()
            .contains("invalid hit condition")
    );

    let _ = std::fs::remove_file(program);
}
