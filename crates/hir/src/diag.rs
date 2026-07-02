//! The single rendering site for diagnostic messages that MIR traps borrow.
//!
//! The invariant "every trap carries the message of a reported diagnostic"
//! is only as strong as the guarantee that both sides render the same text —
//! a message written twice will drift. Anything shown both as a squiggle
//! and as a runtime crash must be rendered here (or via
//! [`crate::InferenceDiagnostic::message`], or mir's own diagnostics'
//! `message()`) and nowhere else. [`crate::ConstCheckDiagnostic::message`]
//! renders by delegating to the functions below.

pub fn unresolved_name(name: &str) -> String {
    format!("unresolved name `{name}`")
}

pub fn defined_multiple_times(name: &str) -> String {
    format!("`{name}` is defined multiple times")
}

pub const INT_LITERAL_TOO_LARGE: &str = "integer literal is too large";

pub fn non_const_fn_call(name: &str) -> String {
    format!("cannot call `{name}` in a const context; marking it `const fn` would allow this")
}

pub const NON_CONST_FN_LITERAL_CALL: &str =
    "cannot call this `fn` literal in a const context; marking it `const fn` would allow this";

pub fn side_effect_call_in_const(name: &str) -> String {
    format!("cannot call `{name}` in a const context; const evaluation cannot have side effects")
}

pub const VALUE_CALL_IN_CONST: &str =
    "cannot call a value in a const context; whether it is a `const fn` is not known from its type";
