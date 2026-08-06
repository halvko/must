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

/// A call of an unsafe builtin (`dealloc_array`, `copy`) outside any
/// `unsafe { ... }` block — the same rule as [`DEREF_REQUIRES_UNSAFE`]
/// (operations whose misuse is UB need the marker), same fix-naming shape.
pub fn builtin_call_requires_unsafe(name: &str) -> String {
    format!("calling `{name}` requires an `unsafe {{ ... }}` block")
}

/// A call of an `extern fn` outside any `unsafe { ... }` block. The same rule
/// as [`DEREF_REQUIRES_UNSAFE`], and the reason is the boundary itself: what
/// an import does is written in a language this compiler never sees, so
/// nothing on this side can establish that calling it is sound. The caller
/// vouches, which is exactly what the marker means.
pub fn extern_call_requires_unsafe(name: &str) -> String {
    format!(
        "calling the host import `{name}` requires an `unsafe {{ ... }}` block; \
         nothing on this side of the boundary can check what it does"
    )
}

/// An `extern fn` mentioned as a VALUE — bound, passed, returned — outside
/// any `unsafe { ... }` block. The marker moves to where the value is TAKEN
/// because that is the last place a reader can see which import is in play:
/// once it is a value, the call site says only that something is being
/// called. Nothing here forbids first-class imports; it prices them.
pub fn extern_value_requires_unsafe(name: &str) -> String {
    format!(
        "taking the host import `{name}` as a value requires an `unsafe {{ ... }}` block; \
         a value can be called from anywhere, so vouching happens where it is taken"
    )
}

/// An `extern fn` call in a const context — [`side_effect_call_in_const`]'s
/// judgment at a different boundary, stated in its own words because "side
/// effect" is the wrong noun for "there is nobody there".
pub fn extern_call_in_const(name: &str) -> String {
    format!(
        "cannot call the host import `{name}` in a const context; \
         there is no host at compile time"
    )
}

/// The eager const fence (C04): `alloc_array` / `dealloc_array` refuse in
/// const contexts until interning (C06) delivers — refusing eagerly keeps
/// the later relaxation a grant instead of a retraction. Rendered per verb
/// so the message names the construct.
pub fn heap_call_in_const(builtin_name: &str) -> String {
    let verb = if builtin_name == "dealloc_array" {
        "deallocate"
    } else {
        "allocate"
    };
    format!(
        "cannot {verb} during compile-time evaluation: \
         const-built heap values wait for an interning design"
    )
}

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

/// A named generic argument outside a trait's argument list (TR01 gives v1
/// exactly one nameable argument, a trait's `Self`) — shared between
/// inference and the annotation-position pass.
pub fn named_arg_not_a_trait(name: &str) -> String {
    if name == "Self" {
        return "only a trait has a `Self` argument to name".to_owned();
    }
    named_arg_not_self(name)
}

/// A named generic argument whose name isn't `Self` — shared like
/// [`named_arg_not_a_trait`].
pub fn named_arg_not_self(name: &str) -> String {
    format!(
        "`{name}` cannot be supplied by name: `Self` is the only nameable generic \
         argument (`Trait::<Self = Type>::member`)"
    )
}

/// `Trait::<Self = _>::member` — the `Self` argument written as a HOLE.
/// `Self` names the implementer, which is the whole point of the named
/// form: the member it denotes is impl-specific, so a hole there declines
/// to answer the only question the spelling asks. Refused structurally at
/// lowering in BOTH positions — in call position a hole would be
/// inferable, but it adds nothing the short form `Trait::member(...)`
/// does not already say, and one rule beats two.
pub const NAMED_ARG_SELF_HOLE: &str = "`Self` names the implementer, so it cannot be `_`: \
     write the type (`Trait::<Self = Type>::member`), or use the short form \
     `Trait::member(...)` where an argument determines `Self`";

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

// ---- regions and safe borrows --------------------------------------------

/// A safe borrow type written with no region (`T.&`). Elision is DEFERRED,
/// not absent by accident: every region is hand-written until a corpus says
/// which elision rule earns its keep, so the omission is reported rather
/// than guessed at. The message names the spelling so the fix is copyable.
pub const BORROW_NEEDS_REGION: &str =
    "a safe borrow must name its region (`T.&::<@a>`); regions are never elided yet";

/// `@_` written in a SIGNATURE. The wildcard says "there is a region here,
/// infer it", which a body can answer and a signature cannot: a signature's
/// regions are parameters, so they need names in the binder.
pub const WILDCARD_REGION_IN_SIGNATURE: &str = "`@_` cannot be used in a signature — declare the region in the binder \
     (`fn::<@a>`) and name it here";

/// `@_` written in a BINDER (`fn::<@_>`). The wildcard is the elision
/// sigil, not a name: it asks for a region to be inferred, and a binder is
/// where regions are declared, so there is nothing for it to denote — and
/// a second `@_` in the same list would collide with the first as if two
/// parameters had been named the same. Refused rather than accepted as a
/// name nobody can mention.
pub const WILDCARD_REGION_IN_BINDER: &str =
    "`@_` is not a region name; a binder declares regions by name (`fn::<@a>`)";

/// A region name that no enclosing binder declares.
pub fn unknown_region(name: &str) -> String {
    format!("no region named `{name}` is in scope; declare it in the binder (`fn::<{name}>`)")
}

/// A region argument where the binder declares a type or const parameter.
pub fn unexpected_region_arg(param: &str) -> String {
    format!("`{param}` is not a region parameter; a region argument (`@a`) does not belong here")
}

/// Region parameters on a TYPE declaration (`struct::<@a, T>`). Reserved,
/// not rejected: making a declaration carry a region needs variance and
/// well-formedness rulings this arc does not own, and granting it later
/// deletes this diagnostic without changing the grammar.
pub const REGION_ON_TYPE_DECL: &str =
    "region parameters on type declarations are not supported yet";

/// Reading `x.*` where the referent is not copyable — the rule safe `.*`
/// makes load-bearing, since heap-backed types are noncopyable (T08). Named
/// after the operation the user attempted, not the rule it broke.
pub const MOVE_OUT_OF_BORROW: &str = "cannot move out of a borrow";

/// A borrow's turbofish carrying the wrong number of arguments. A borrow
/// takes exactly one thing: how long it is good for.
pub fn borrow_region_arity(found: usize) -> String {
    format!("a safe borrow takes exactly one region argument (`T.&::<@a>`), found {found}")
}

/// A borrow's turbofish carrying something that is not a region.
pub const BORROW_REGION_KIND: &str =
    "a safe borrow's argument is a region (`@a`, or `@_` to infer one) — not a type or a value";
