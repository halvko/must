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

/// A raw-pointer deref (read or write) outside any `unsafe { ... }` block —
/// the one operation whose misuse is UB, so the one that needs the marker.
/// The message names the fix so a quick fix can quote it later.
pub const DEREF_REQUIRES_UNSAFE: &str =
    "dereferencing a raw pointer requires an `unsafe { ... }` block";

/// The item-level generic rule (TR06): a generic fn literal's params and
/// return type are its scheme, so all of them must be written. Shown only
/// at the definition (mentions stay silent about it — see
/// `signature_needs_annotation`), but rendered here for the single-render
/// discipline all definition/trap-shared texts follow.
pub const GENERIC_FN_NEEDS_FULL_ANNOTATION: &str =
    "a generic function must annotate all parameters and its return type";

/// A `const { ... }` block where a TYPE's const argument is needed. Type
/// identity lives on the eval-free annotation path (the firewall: const
/// eval never runs while resolving an annotation), so a computed value can
/// never parameterize a type — in annotation position *or* expression
/// position. Shared by inference (expression-position turbofish on a type
/// name) and the annotation mirror in [`crate::file_diagnostics`].
pub const CONST_BLOCK_TYPE_ARG: &str = "a `const { ... }` block cannot parameterize a type; \
     pass the value through a generic function's const parameter instead";

/// The non-block sibling of [`CONST_BLOCK_TYPE_ARG`]: some other computed
/// value (an item reference, a call) in a type's const-argument position.
pub const TYPE_CONST_ARG_NOT_LITERAL: &str =
    "a type's const argument must be a literal or a const parameter name";

/// `_` in a const-argument position: const args are never inferred (TR06).
/// Shared by inference and the annotation mirror.
pub const CONST_ARG_HOLE: &str = "const arguments cannot be inferred";

/// Wrong number of generic arguments for a binder — shared by inference
/// (expression positions) and the annotation mirror in
/// [`crate::file_diagnostics`], so a `Pair::<usize, str>` reads the same
/// wherever it sits.
pub fn generic_arg_count(name: &str, expected: usize, found: usize) -> String {
    let noun = if expected == 1 {
        "generic argument"
    } else {
        "generic arguments"
    };
    format!("`{name}` takes {expected} {noun}, found {found}")
}

/// A turbofish on something that takes no generic arguments — shared like
/// [`generic_arg_count`].
pub fn takes_no_generic_args(name: &str) -> String {
    format!("`{name}` takes no generic arguments")
}

/// A value written where a binder declares a type parameter — shared like
/// [`generic_arg_count`].
pub fn type_param_needs_type(param: &str) -> String {
    format!("`{param}` is a type parameter; write a type")
}

/// A type written where a binder declares a const parameter — shared like
/// [`generic_arg_count`].
pub fn const_param_needs_value(param: &str) -> String {
    format!("`{param}` is a const parameter; write a value (a literal, or `const <expr>`)")
}

/// An array index past the end. Rendered here because it appears in THREE
/// coats that must all say the same thing: the compile-time squiggle (both
/// sides known), the trap MIR plants for it, and the runtime bounds check
/// the interpreter performs when either side is only known dynamically.
pub fn index_out_of_bounds(len: u128, index: u128) -> String {
    format!("index out of bounds: the length is {len} but the index is {index}")
}

/// The const-arg domain exclusion for arrays: the ruled const-arg value
/// domain is builtins + records + variants — array VALUES stay outside it
/// for now. Rejected at the *declaration* (a const param whose declared
/// type mentions an array type) and, as a belt, at every mention — both
/// render this exact text, the array twin of [`FN_CONST_ARG`].
pub const ARRAY_CONST_ARG: &str = "an array value cannot be a const argument (yet)";

/// The const-arg domain exclusion (TR06: concrete data types only): fn
/// values carry a `BodyId` arena index that renumbers under body edits, so
/// admitting them as const arguments would make instance identity (and,
/// later, FFI symbols) churn under unrelated edits. Rejected at the
/// *declaration* (a const param whose declared type mentions a fn type)
/// and, as a belt, at every *mention* that would pass one — both render
/// this exact text.
pub const FN_CONST_ARG: &str = "a function value cannot be a const argument (yet)";
