//! `must-lsp dap`: a Debug Adapter Protocol session where the adapter *is*
//! the runtime — Zed spawns this binary, and the program runs in-process on
//! the same MIR interpreter the editor's const eval uses. M9 scope: launch
//! and run (output streamed to the debug console, deferred errors crash with
//! their diagnostic); breakpoints and stepping land with the debugger
//! milestone.

mod transport;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::io::{BufRead, Write};
use std::rc::Rc;

use serde_json::{Value, json};

use crate::ServerResult;
use transport::Incoming;

pub fn run(mut reader: impl BufRead, writer: impl Write) -> ServerResult<()> {
    let mut session = Session {
        out: Rc::new(RefCell::new(writer)),
        seq: Rc::new(Cell::new(0)),
        configured: false,
        pending_launch: None,
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
    /// Breakpoint ids handed out so far. Zed *silently discards* the
    /// verification state of any response breakpoint without an `id`
    /// (`dap_bp.id?` in its session bookkeeping), so every reported
    /// breakpoint gets a unique one.
    next_breakpoint_id: i64,
}

impl<W: Write> Session<W> {
    /// Returns `false` when the session should end (disconnect).
    fn handle(&mut self, request: Incoming) -> ServerResult<bool> {
        match request.command.as_str() {
            "initialize" => {
                self.respond(
                    &request,
                    json!({ "supportsConfigurationDoneRequest": true }),
                )?;
                self.event("initialized", json!({}))?;
            }
            "launch" => {
                if self.configured {
                    self.respond(&request, json!({}))?;
                    let arguments = request.arguments;
                    self.execute(&arguments)?;
                } else {
                    self.pending_launch = Some(request);
                }
            }
            "configurationDone" => {
                self.configured = true;
                self.respond(&request, json!({}))?;
                if let Some(launch) = self.pending_launch.take() {
                    // The launch response is an acknowledgement; the run
                    // itself reports through events.
                    self.respond(&launch, json!({}))?;
                    self.execute(&launch.arguments)?;
                }
            }
            // Accepted but inert until the debugger milestone: reported
            // unverified (greyed + warning in Zed — though only visible
            // while a session is stopped at a frame, which M9 sessions
            // never are).
            "setBreakpoints" => {
                let count = request.arguments["breakpoints"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0);
                let breakpoints: Vec<Value> = (0..count)
                    .map(|_| {
                        self.next_breakpoint_id += 1;
                        json!({
                            "id": self.next_breakpoint_id,
                            "verified": false,
                            "message": "breakpoints are not supported yet",
                        })
                    })
                    .collect();
                self.respond(&request, json!({ "breakpoints": breakpoints }))?;
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

    /// Run the launch configuration to completion, streaming output events.
    fn execute(&mut self, arguments: &Value) -> ServerResult<()> {
        let entry = arguments["entry"].as_str().unwrap_or("main()");
        let exit_code = match arguments["program"].as_str() {
            None => {
                self.output("stderr", "launch configuration needs a `program` path\n")?;
                2
            }
            Some(path) => match std::fs::read_to_string(path) {
                Err(err) => {
                    self.output("stderr", &format!("error: cannot read `{path}`: {err}\n"))?;
                    2
                }
                Ok(text) => {
                    let console = ConsoleWriter {
                        out: Rc::clone(&self.out),
                        seq: Rc::clone(&self.seq),
                        buffer: Vec::new(),
                    };
                    match crate::runner::evaluate(text, path, entry, console) {
                        Ok(Some(value)) => {
                            self.output("console", &format!("{value}\n"))?;
                            0
                        }
                        Ok(None) => 0,
                        Err(rendered) => {
                            self.output("stderr", &format!("{rendered}\n"))?;
                            1
                        }
                    }
                }
            },
        };
        self.event("exited", json!({ "exitCode": exit_code }))?;
        self.event("terminated", json!({}))?;
        Ok(())
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
struct ConsoleWriter<W: Write> {
    out: Rc<RefCell<W>>,
    seq: Rc<Cell<i64>>,
    buffer: Vec<u8>,
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
