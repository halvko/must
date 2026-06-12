//! `must-lsp dap`: a Debug Adapter Protocol session where the adapter *is*
//! the runtime — Zed spawns this binary, and the program runs in-process on
//! the same MIR interpreter the editor's const eval uses. Supports launch,
//! breakpoints, stepping, stack/variables inspection, console evaluation,
//! and stop-on-trap: a deferred error pauses at the crash site with the
//! editor's diagnostic instead of just dying.

mod debuggee;
mod transport;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::io::{BufRead, Write};
use std::rc::Rc;

use serde_json::{Value, json};

use crate::ServerResult;
use debuggee::{BreakpointSpec, Debuggee, Outcome, ResumeMode};
use transport::Incoming;

pub fn run(mut reader: impl BufRead, writer: impl Write) -> ServerResult<()> {
    let mut session = Session {
        out: Rc::new(RefCell::new(writer)),
        seq: Rc::new(Cell::new(0)),
        configured: false,
        pending_launch: None,
        debuggee: None,
        requested_breakpoints: Vec::new(),
        next_breakpoint_id: 0,
    };
    while let Some(message) = transport::read_message(&mut reader)? {
        if message.kind != "request" {
            continue;
        }
        if !session.handle(message)? {
            break;
        }
    }
    Ok(())
}

struct Session<W: Write> {
    out: Rc<RefCell<W>>,
    /// Outgoing sequence counter (the adapter's own numbering namespace).
    seq: Rc<Cell<i64>>,
    configured: bool,
    /// `launch` conventionally arrives before the breakpoint configuration
    /// is finished; the debuggee must not start until `configurationDone`.
    pending_launch: Option<Incoming>,
    debuggee: Option<Debuggee<ConsoleWriter<W>>>,
    /// Latest client breakpoint state; transferred to the debuggee when
    /// it exists.
    requested_breakpoints: Vec<BreakpointSpec>,
    /// Zed *silently discards* the verification state of any response
    /// breakpoint without an `id`, so every reported breakpoint gets one.
    next_breakpoint_id: i64,
}

impl<W: Write> Session<W> {
    /// Returns `false` when the session should end (disconnect).
    fn handle(&mut self, request: Incoming) -> ServerResult<bool> {
        match request.command.as_str() {
            "initialize" => {
                self.respond(
                    &request,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        // Multiple calls on one line are distinct stops:
                        // inline (column) breakpoints and statement steps.
                        "supportsBreakpointLocationsRequest": true,
                        "supportsSteppingGranularity": true,
                        "supportsConditionalBreakpoints": true,
                        "supportsHitConditionalBreakpoints": true,
                        "supportsLogPoints": true,
                    }),
                )?;
                self.event("initialized", json!({}))?;
            }
            "launch" => match self.prepare_debuggee(&request.arguments) {
                Err(message) => {
                    self.output("stderr", &format!("{message}\n"))?;
                    self.respond_err(&request, &message)?;
                }
                Ok(()) => {
                    if self.configured {
                        self.respond(&request, json!({}))?;
                        self.begin(&request.arguments)?;
                    } else {
                        self.pending_launch = Some(request);
                    }
                }
            },
            "configurationDone" => {
                self.configured = true;
                self.respond(&request, json!({}))?;
                if let Some(launch) = self.pending_launch.take() {
                    // The launch response is an acknowledgement; the run
                    // itself reports through events.
                    self.respond(&launch, json!({}))?;
                    self.begin(&launch.arguments)?;
                }
            }
            "setBreakpoints" => {
                let requested: Vec<Value> = request.arguments["breakpoints"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut accepted = Vec::new();
                let breakpoints: Vec<Value> = requested
                    .into_iter()
                    .filter_map(|raw| {
                        let line = raw["line"].as_u64()? as u32;
                        let column = raw["column"].as_u64().map(|c| c as u32);
                        self.next_breakpoint_id += 1;
                        let id = self.next_breakpoint_id;
                        let hit_condition_text =
                            raw["hitCondition"].as_str().filter(|t| !t.trim().is_empty());
                        let hit_condition = hit_condition_text.map(debuggee::HitCondition::parse);
                        let mut verified = self
                            .debuggee
                            .as_ref()
                            .is_some_and(|d| d.can_break_at(line, column));
                        let mut message = (!verified)
                            .then(|| "no executable code at this position".to_owned());
                        if matches!(hit_condition, Some(None)) {
                            verified = false;
                            message = Some(
                                "invalid hit condition (use a number, ==N, !=N, >N, >=N, <N, <=N, or %N)"
                                    .to_owned(),
                            );
                        }
                        accepted.push(BreakpointSpec {
                            line,
                            column,
                            id,
                            condition: raw["condition"]
                                .as_str()
                                .filter(|t| !t.trim().is_empty())
                                .map(str::to_owned),
                            hit_condition: hit_condition.flatten(),
                            log_message: raw["logMessage"]
                                .as_str()
                                .filter(|t| !t.trim().is_empty())
                                .map(str::to_owned),
                            hits: 0,
                            visited: Default::default(),
                        });
                        let mut bp = json!({ "id": id, "verified": verified, "line": line });
                        if let Some(column) = column {
                            bp["column"] = json!(column);
                        }
                        if let Some(message) = message {
                            bp["message"] = json!(message);
                        }
                        Some(bp)
                    })
                    .collect();
                self.requested_breakpoints = accepted.clone();
                if let Some(debuggee) = self.debuggee.as_mut() {
                    debuggee.breakpoints = accepted;
                }
                self.respond(&request, json!({ "breakpoints": breakpoints }))?;
            }
            "breakpointLocations" => {
                let Some(debuggee) = self.debuggee.as_ref() else {
                    self.respond(&request, json!({ "breakpoints": [] }))?;
                    return Ok(true);
                };
                let line = request.arguments["line"].as_u64().unwrap_or(0) as u32;
                let end_line = request.arguments["endLine"]
                    .as_u64()
                    .map(|l| l as u32)
                    .unwrap_or(line);
                let locations: Vec<Value> = debuggee
                    .breakpoint_locations(line, end_line)
                    .into_iter()
                    .map(|(line, column)| json!({ "line": line, "column": column }))
                    .collect();
                self.respond(&request, json!({ "breakpoints": locations }))?;
            }
            "setExceptionBreakpoints" => {
                self.respond(&request, json!({ "breakpoints": [] }))?;
            }
            "threads" => {
                self.respond(
                    &request,
                    json!({ "threads": [{ "id": 1, "name": "main" }] }),
                )?;
            }
            "continue" | "next" | "stepIn" | "stepOut" => {
                if self.debuggee.is_none() {
                    self.respond_err(&request, "no program is running")?;
                    return Ok(true);
                }
                let body = if request.command == "continue" {
                    json!({ "allThreadsContinued": true })
                } else {
                    json!({})
                };
                self.respond(&request, body)?;
                if self.debuggee.as_ref().is_some_and(|d| d.crashed.is_some()) {
                    // Resuming a crashed program: it's over.
                    self.event("exited", json!({ "exitCode": 1 }))?;
                    self.event("terminated", json!({}))?;
                    self.debuggee = None;
                    return Ok(true);
                }
                let mode = match request.command.as_str() {
                    "next" => ResumeMode::StepOver,
                    "stepIn" => ResumeMode::StepIn,
                    "stepOut" => ResumeMode::StepOut,
                    _ => ResumeMode::Continue,
                };
                // MIR statements are our "instructions".
                let statement = matches!(
                    request.arguments["granularity"].as_str(),
                    Some("statement" | "instruction")
                );
                let outcome = self
                    .debuggee
                    .as_mut()
                    .expect("checked above")
                    .resume(mode, statement);
                self.report(outcome)?;
            }
            "stackTrace" => {
                let Some(debuggee) = self.debuggee.as_ref() else {
                    self.respond_err(&request, "no program is running")?;
                    return Ok(true);
                };
                let path = debuggee.path().to_owned();
                let file_name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let frames: Vec<Value> = debuggee
                    .stack_frames()
                    .iter()
                    .map(|frame| {
                        json!({
                            "id": frame.id,
                            "name": frame.name,
                            "line": frame.line,
                            "column": frame.column,
                            "source": { "name": file_name, "path": path },
                        })
                    })
                    .collect();
                let total = frames.len();
                self.respond(
                    &request,
                    json!({ "stackFrames": frames, "totalFrames": total }),
                )?;
            }
            "scopes" => {
                let frame_id = request.arguments["frameId"].as_i64().unwrap_or(0);
                self.respond(
                    &request,
                    json!({ "scopes": [{
                        "name": "Locals",
                        "variablesReference": frame_id,
                        "expensive": false,
                    }] }),
                )?;
            }
            "variables" => {
                let reference = request.arguments["variablesReference"]
                    .as_i64()
                    .unwrap_or(0);
                let variables: Vec<Value> = self
                    .debuggee
                    .as_ref()
                    .map(|d| d.locals(reference))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, value)| {
                        json!({
                            "name": name,
                            "value": value.display(),
                            "variablesReference": 0,
                        })
                    })
                    .collect();
                self.respond(&request, json!({ "variables": variables }))?;
            }
            "evaluate" => {
                let Some(debuggee) = self.debuggee.as_mut() else {
                    self.respond_err(&request, "no program is running")?;
                    return Ok(true);
                };
                let expression = request.arguments["expression"].as_str().unwrap_or("");
                let frame_id = request.arguments["frameId"].as_i64();
                match debuggee.evaluate(expression, frame_id) {
                    Ok(result) => self.respond(
                        &request,
                        json!({ "result": result, "variablesReference": 0 }),
                    )?,
                    Err(message) => self.respond_err(&request, &message)?,
                }
            }
            "disconnect" => {
                self.respond(&request, json!({}))?;
                return Ok(false);
            }
            other => {
                let message = format!("unsupported request: {other}");
                self.respond_err(&request, &message)?;
            }
        }
        Ok(true)
    }

    fn prepare_debuggee(&mut self, arguments: &Value) -> Result<(), String> {
        let path = arguments["program"]
            .as_str()
            .ok_or("launch configuration needs a `program` path")?;
        let entry = arguments["entry"].as_str().unwrap_or("main()");
        let mut debuggee = Debuggee::new(path, entry, self.console())?;
        debuggee.breakpoints = self.requested_breakpoints.clone();
        self.debuggee = Some(debuggee);
        Ok(())
    }

    /// Start the prepared debuggee running.
    fn begin(&mut self, arguments: &Value) -> ServerResult<()> {
        let no_debug = arguments["noDebug"].as_bool().unwrap_or(false);
        let stop_on_entry = arguments["stopOnEntry"].as_bool().unwrap_or(false);
        let Some(debuggee) = self.debuggee.as_mut() else {
            return Ok(());
        };
        if no_debug {
            debuggee.breakpoints.clear();
        }
        let mode = if stop_on_entry && !no_debug {
            ResumeMode::Entry
        } else {
            ResumeMode::Continue
        };
        let outcome = debuggee.resume(mode, false);
        self.report(outcome)
    }

    fn report(&mut self, outcome: Outcome) -> ServerResult<()> {
        match outcome {
            Outcome::Stopped {
                reason,
                hit_breakpoint,
            } => {
                let mut body = json!({
                    "reason": reason,
                    "threadId": 1,
                    "allThreadsStopped": true,
                });
                if let Some(id) = hit_breakpoint {
                    body["hitBreakpointIds"] = json!([id]);
                }
                self.event("stopped", body)?;
            }
            Outcome::Done(value) => {
                if !matches!(value, eval::Value::Unit) {
                    self.output("console", &format!("{}\n", value.display()))?;
                }
                self.event("exited", json!({ "exitCode": 0 }))?;
                self.event("terminated", json!({}))?;
                self.debuggee = None;
            }
            // Stop-on-trap: the program is dead, but its frames stay
            // inspectable until the user resumes.
            Outcome::Crashed(err) => {
                let rendered = self
                    .debuggee
                    .as_ref()
                    .map(|d| d.render_error(&err))
                    .unwrap_or_else(|| err.message.clone());
                self.output("stderr", &format!("{rendered}\n"))?;
                self.event(
                    "stopped",
                    json!({
                        "reason": "exception",
                        "threadId": 1,
                        "allThreadsStopped": true,
                        "description": err.message,
                        "text": err.message,
                    }),
                )?;
                if let Some(debuggee) = self.debuggee.as_mut() {
                    debuggee.crashed = Some(err);
                }
            }
        }
        Ok(())
    }

    fn console(&self) -> ConsoleWriter<W> {
        ConsoleWriter {
            out: Rc::clone(&self.out),
            seq: Rc::clone(&self.seq),
            buffer: Vec::new(),
        }
    }

    fn next_seq(&self) -> i64 {
        self.seq.set(self.seq.get() + 1);
        self.seq.get()
    }

    fn send(&self, message: &Value) -> ServerResult<()> {
        transport::write_message(&mut *self.out.borrow_mut(), message)
    }

    fn respond(&self, request: &Incoming, body: Value) -> ServerResult<()> {
        self.send(&transport::response(self.next_seq(), request, body))
    }

    fn respond_err(&self, request: &Incoming, message: &str) -> ServerResult<()> {
        self.send(&transport::error_response(self.next_seq(), request, message))
    }

    fn event(&self, name: &str, body: Value) -> ServerResult<()> {
        self.send(&transport::event(self.next_seq(), name, body))
    }

    fn output(&self, category: &str, text: &str) -> ServerResult<()> {
        self.event("output", json!({ "category": category, "output": text }))
    }
}

/// An `io::Write` that turns each completed line of program output into a
/// DAP `output` event — `print` streams to the debug console as it runs.
pub(crate) struct ConsoleWriter<W: Write> {
    out: Rc<RefCell<W>>,
    seq: Rc<Cell<i64>>,
    buffer: Vec<u8>,
}

impl<W: Write> Clone for ConsoleWriter<W> {
    fn clone(&self) -> Self {
        ConsoleWriter {
            out: Rc::clone(&self.out),
            seq: Rc::clone(&self.seq),
            buffer: Vec::new(),
        }
    }
}

impl<W: Write> ConsoleWriter<W> {
    fn emit(&mut self, text: &str) {
        self.seq.set(self.seq.get() + 1);
        let message = transport::event(
            self.seq.get(),
            "output",
            json!({ "category": "stdout", "output": text }),
        );
        let _ = transport::write_message(&mut *self.out.borrow_mut(), &message);
    }
}

impl<W: Write> Write for ConsoleWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        while let Some(newline) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            self.emit(&String::from_utf8_lossy(&line));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<W: Write> Drop for ConsoleWriter<W> {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
            self.emit(&rest);
        }
    }
}
