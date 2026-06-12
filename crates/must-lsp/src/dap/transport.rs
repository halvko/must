//! DAP wire format: the same `Content-Length` framing as LSP, but DAP's own
//! envelope (`seq`/`type`/`request_seq` — not JSON-RPC).

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::ServerResult;

/// An incoming protocol message, loosely typed: M9 needs requests only
/// (clients send no events, and reverse-request responses are ignorable).
#[derive(Debug)]
pub(crate) struct Incoming {
    pub(crate) seq: i64,
    pub(crate) kind: String,
    pub(crate) command: String,
    pub(crate) arguments: Value,
}

/// Read one framed message; `None` on a cleanly closed stream.
pub(crate) fn read_message(reader: &mut impl BufRead) -> ServerResult<Option<Incoming>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse()?);
        }
    }
    let content_length = content_length.ok_or("missing Content-Length header")?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let value: Value = serde_json::from_slice(&body)?;
    Ok(Some(Incoming {
        seq: value["seq"].as_i64().unwrap_or(0),
        kind: value["type"].as_str().unwrap_or_default().to_owned(),
        command: value["command"].as_str().unwrap_or_default().to_owned(),
        arguments: value.get("arguments").cloned().unwrap_or(Value::Null),
    }))
}

pub(crate) fn write_message(writer: &mut impl Write, message: &Value) -> ServerResult<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn response(seq: i64, request: &Incoming, body: Value) -> Value {
    json!({
        "seq": seq,
        "type": "response",
        "request_seq": request.seq,
        "command": request.command,
        "success": true,
        "body": body,
    })
}

pub(crate) fn error_response(seq: i64, request: &Incoming, message: &str) -> Value {
    json!({
        "seq": seq,
        "type": "response",
        "request_seq": request.seq,
        "command": request.command,
        "success": false,
        "message": message,
    })
}

pub(crate) fn event(seq: i64, name: &str, body: Value) -> Value {
    json!({
        "seq": seq,
        "type": "event",
        "event": name,
        "body": body,
    })
}
