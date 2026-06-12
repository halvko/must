//! The single rendering site for diagnostic messages that MIR traps borrow.
//!
//! The invariant "every trap carries the message of a reported diagnostic"
//! is only as strong as the guarantee that both sides render the same text —
//! a message written twice will drift. Anything shown both as a squiggle
//! and as a runtime crash must be rendered here (or via
//! [`crate::InferenceDiagnostic::message`], or mir's own diagnostics'
//! `message()`) and nowhere else.

pub fn unresolved_name(name: &str) -> String {
    format!("unresolved name `{name}`")
}

pub fn defined_multiple_times(name: &str) -> String {
    format!("`{name}` is defined multiple times")
}

pub const INT_LITERAL_TOO_LARGE: &str = "integer literal is too large";
