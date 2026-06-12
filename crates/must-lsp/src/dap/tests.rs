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
    let path = std::env::temp_dir().join(format!("must-dap-test-{name}-{}.must", std::process::id()));
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
    let program = fixture(
        "hello",
        r#"static main = fn { print("hello from dap"); };"#,
    );
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
    let seqs: Vec<i64> = messages.iter().map(|m| m["seq"].as_i64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seqs increase: {seqs:?}");

    let _ = std::fs::remove_file(program);
}

#[test]
fn deferred_errors_crash_with_the_diagnostic() {
    let program = fixture(
        "broken",
        "static main = fn {\n    print(\"before\");\n    let v: usize = \"s\";\n};\n",
    );
    let messages = run_session(&[
        ("initialize", json!({})),
        ("launch", json!({ "program": program.to_str().unwrap() })),
        ("configurationDone", json!({})),
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
    assert_eq!(events(&messages, "exited")[0]["body"]["exitCode"], 1);

    let _ = std::fs::remove_file(program);
}

#[test]
fn breakpoints_are_reported_unverified_for_now() {
    let program = fixture("bp", "static main = fn {};\n");
    let messages = run_session(&[
        ("initialize", json!({})),
        ("launch", json!({ "program": program.to_str().unwrap() })),
        (
            "setBreakpoints",
            json!({ "breakpoints": [{ "line": 1 }, { "line": 2 }] }),
        ),
        ("configurationDone", json!({})),
        ("disconnect", json!({})),
    ]);
    let response = &responses_for(&messages, "setBreakpoints")[0];
    let breakpoints = response["body"]["breakpoints"].as_array().unwrap();
    assert_eq!(breakpoints.len(), 2);
    assert!(breakpoints.iter().all(|b| b["verified"] == false));
    // Each gets a unique id: Zed drops the verification state of any
    // response breakpoint without one.
    let ids: Vec<i64> = breakpoints
        .iter()
        .map(|b| b["id"].as_i64().expect("breakpoint has an id"))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);

    let _ = std::fs::remove_file(program);
}
