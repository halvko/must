//! The single rendering site for diagnostic messages that MIR traps borrow.
//!
//! The invariant "every trap carries the message of a reported diagnostic"
//! is only as strong as the guarantee that both sides render the same text —
//! a message written twice will drift. Anything shown both as a squiggle
//! and as a runtime crash must be rendered here (or via
//! [`crate::InferenceDiagnostic::message`], or mir's own diagnostics'
//! `message()`) and nowhere else. [`crate::ConstCheckDiagnostic::message`]
//! renders by delegating to the functions below.

use crate::capability::Affine;

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

/// A call of a host import outside any `unsafe { ... }` block. The same rule
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

/// A call THROUGH A VALUE whose type is `unsafe fn(...)`, outside any
/// `unsafe { ... }` block — a bound host import, an unsafe builtin passed
/// as an argument, a record field holding either, a parameter declared
/// `unsafe fn(...)`.
///
/// It names no function because the call site knows none: what it knows is
/// the callee's TYPE, and the type is exactly the thing that says a marker
/// is owed — so that is what the message says. Taking such a value is free;
/// running it is not.
pub const UNSAFE_FN_VALUE_CALL_REQUIRES_UNSAFE: &str =
    "calling a value of `unsafe fn` type requires an `unsafe { ... }` block";

/// A host-import call in a const context — [`side_effect_call_in_const`]'s
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

/// A REGION argument written in a mention's turbofish (`f::<@_, usize>`,
/// `o.get::<@a>`). Regions are elided at every call site: a callee's
/// regions become fresh existentials of the call, solved from the
/// arguments actually passed, so there is nothing at the mention for a
/// written one to pin. One sentence for every list a mention can carry —
/// item, member, a trait's own — with or without a binder behind it, with
/// one exception: a TYPE's own list (`Pair::<@a>(...)`). A type declaration
/// binds no region yet, so a region there is a wrong-kind argument for a
/// type slot (`GenericArgKindMismatch`), not an elision, and the list stays
/// positional over the whole binder. Its twin at a borrow EXPRESSION
/// (`x.&::<@a>`) is `syntax::validation`'s `region_arg_at_borrow`; the two
/// are phrased to read as one sentence.
pub const REGION_ARG_AT_MENTION: &str = "regions are inferred at calls, never written: drop this argument — \
     a turbofish spells type and const arguments only";

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

/// `Owner::member::<Self = Type>` — `Self` named in a MEMBER's own list.
/// The owner is perfectly free to be a trait, so the not-a-trait sentence
/// would deny what the reader can see; what is wrong is the POSITION.
pub const NAMED_ARG_OWNERS_SELF: &str = "a member's own generic arguments are positional: \
     `Self` is the owner's, one segment to the left \
     (`Trait::<Self = Type>::member`)";

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

/// A safe borrow type written with no region (`T.&`). SIGNATURE elision is
/// DEFERRED, not absent by accident: every region a signature binds is
/// hand-written until a corpus says which elision rule earns its keep, so
/// the omission is reported rather than guessed at. The message names the
/// spelling so the fix is copyable, and says "in a signature" because a
/// call site elides every region ([`REGION_ARG_AT_MENTION`]).
pub const BORROW_NEEDS_REGION: &str =
    "a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet";

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

/// A borrow TYPE's turbofish carrying the wrong number of arguments. A
/// borrow type takes exactly one thing: how long it is good for. An
/// expression-position borrow has no argument slot at all (refused whole
/// by `syntax::validation`), so there is nothing there to miscount.
pub fn borrow_region_arity(found: usize) -> String {
    format!("a safe borrow takes exactly one region argument (`T.&::<@a>`), found {found}")
}

/// A borrow's turbofish carrying something that is not a region.
pub const BORROW_REGION_KIND: &str =
    "a safe borrow's argument is a region (`@a`, or `@_` to infer one) — not a type or a value";

// ---- capabilities and must-consume checking (linear types) --------------

/// How a must-consume diagnostic NAMES the thing it is about. A hole
/// binding (`let _ = ...`) has no name to quote, and quoting the empty
/// string reads as a compiler bug — so it gets a phrase instead, and one
/// that says where to look.
pub fn linear_subject(name: &str) -> String {
    if name.is_empty() {
        "the value bound by `_`".to_owned()
    } else {
        format!("`{name}`")
    }
}

/// The related note on the binding a must-consume finding names: where the
/// value came from, and what it owes. A LINEAR value owes a consumption;
/// one tracked only against DUPLICATION owes nothing but its own
/// singleness, and saying "must be consumed" of it would be false.
pub fn born_here(name: &str, linear: bool) -> String {
    if linear {
        format!("{} is born here and must be consumed", linear_subject(name))
    } else {
        format!(
            "{} is born here, and there is only one of it",
            linear_subject(name)
        )
    }
}

/// Why a LEAK refusal fired inside a generic body: the binder promised
/// nothing about the value, so the body is checked against every type a
/// caller might supply.
fn leak_reason(param: &str) -> String {
    format!("`{param}` may be a type that must be consumed")
}

/// The way out of a leak that is the same in every position: narrow the
/// callers with the bound.
fn forget_advice(param: &str) -> String {
    format!("write `{param}: forget` to require one that can be discarded")
}

/// The shared tail of the LEAK refusals whose position also offers the
/// ORDINARY way out — consume the value where it stands. Three positions
/// do not (a `const` block's value, a skipped field, a swallowed
/// scrutinee), and each says what its own way out is before naming the
/// bound.
fn leak_hint(param: &str) -> String {
    format!(
        "{}; consume it, or {}",
        leak_reason(param),
        forget_advice(param)
    )
}

/// What a DUPLICATION refusal is about, said as a noun phrase, and why a
/// second read of it would be a second value. `None` for a LINEAR root:
/// its declaration says what it is, and the plain sentences point at it
/// without a clause of their own.
///
/// Neither of the other two offers the bound, and that is load-bearing
/// rather than cautious: `forget` answers what may be LOST, a second read
/// asks what may be DUPLICATED, and no bound in the language grants the
/// second — `T.&mut` is forgettable and refuses copying, which is both why
/// a bounded parameter is tracked exactly as an unbounded one is (TR11)
/// and why the borrow it may be instantiated with is tracked too.
fn duplication_of(root: &Affine) -> Option<(String, String)> {
    match root {
        Affine::Linear => None,
        Affine::Param(param) => Some((
            format!("a value of `{param}`"),
            format!("a value of `{param}` may not be duplicated, and no bound grants copying"),
        )),
        Affine::MutBorrow => Some((
            "a value holding an exclusive borrow".to_owned(),
            "an exclusive borrow may not be duplicated: the two would name one place".to_owned(),
        )),
    }
}

/// The leak: a value of a type without `forget` reached the end of its
/// scope alive. The message names the ONE thing that discharges the
/// obligation in general terms, because which method does it is the
/// library's business, not the compiler's.
pub fn not_consumed(name: &str) -> String {
    format!(
        "{} is not consumed on this path; its type has no `forget` capability, \
         so every path must consume it",
        linear_subject(name)
    )
}

/// The same leak inside a GENERIC body, where the value's type is a rigid
/// parameter with no `forget` bound. A different sentence, because the fix
/// is a different one: nothing was declared linear here — the body is
/// checked against every type the caller might supply.
pub fn not_consumed_param(name: &str, param: &str) -> String {
    format!(
        "{} is not consumed on this path, and {}",
        linear_subject(name),
        leak_hint(param)
    )
}

/// Use-after-consume — which for a linear type is also disposal twice.
/// Named after what the user did, with the earlier site as a related note.
pub fn already_consumed(name: &str) -> String {
    format!("{} was already consumed", linear_subject(name))
}

/// Use-after-consume where the reason is a root with no declaration to
/// point at — a binder, or an exclusive borrow held inside the value.
/// Names it, because "already consumed" alone sends the reader looking for
/// a declaration that says so and there is none. Offers BORROWING, never
/// the bound — see [`duplication_of`].
pub fn already_consumed_dup(name: &str, root: &Affine) -> Option<String> {
    let (_, hint) = duplication_of(root)?;
    Some(format!(
        "{} was already consumed: {hint} — borrow it for the second use",
        linear_subject(name)
    ))
}

/// A linear value produced and dropped on the floor by a statement. There
/// is no binding to name, so the message names the type's obligation
/// instead.
pub const DISCARDED_LINEAR: &str = "this value must be consumed; its type has no `forget` \
     capability, so it cannot be discarded";

/// [`DISCARDED_LINEAR`] where the discarded value's type is a rigid
/// parameter — the statement-position twin of [`not_consumed_param`].
pub fn discarded_param(param: &str) -> String {
    format!("this value is discarded here, and {}", leak_hint(param))
}

/// Writing over a live linear: the old value is gone, and nothing was done
/// about it. The same leak as [`not_consumed`] at a different moment.
pub fn assign_over_live(name: &str) -> String {
    format!(
        "{} still holds a value that must be consumed; assigning here would lose it",
        linear_subject(name)
    )
}

/// [`assign_over_live`] where the binder is the reason the old value had
/// to go somewhere. A LEAK-family message, so it may offer the bound: it
/// fires only for a parameter nobody constrained.
pub fn assign_over_live_param(name: &str, param: &str) -> String {
    format!(
        "{} still holds a value that must be consumed, and {}",
        linear_subject(name),
        leak_hint(param)
    )
}

/// The join case, stated as the disagreement it is. Joins resolve at
/// statement boundaries, so the squiggle sits on the whole `if`/`match`
/// rather than on one arm — neither arm is wrong on its own.
pub fn join_disagrees(name: &str) -> String {
    format!(
        "{} is consumed on some paths through this expression and not on others",
        linear_subject(name)
    )
}

/// [`join_disagrees`] where the binder is the reason the paths had to
/// agree. Leak family, so the bound is one way out.
pub fn join_disagrees_param(name: &str, param: &str) -> String {
    format!(
        "{} is consumed on some paths through this expression and not on others, and {}",
        linear_subject(name),
        leak_hint(param)
    )
}

/// Copying a linear out of a place. Says what to do instead, because the
/// answer is not obvious and is the same every time: take the whole value
/// apart.
pub const COPIED_OUT_LINEAR: &str = "cannot copy a value that must be consumed out of a place; \
     take the whole value apart instead (`let Name(struct { .. }) = value;`)";

/// [`COPIED_OUT_LINEAR`] where the copy is refused for a root with no
/// declaration behind it. A generic body cannot take an opaque `T` apart —
/// it has no pattern for one — so the advice changes with the reason.
pub fn copied_out_dup(root: &Affine) -> Option<String> {
    let (subject, hint) = duplication_of(root)?;
    Some(format!(
        "cannot copy {subject} out of a place: {hint} — borrow the place, \
         or move the whole value"
    ))
}

/// `[s; 3]` where `s` must be consumed: the repeat form would make three
/// obligations out of one value.
pub const REPEATED_LINEAR: &str = "cannot repeat a value that must be consumed: the copies would each have to be consumed, \
     and there is only one value";

/// [`REPEATED_LINEAR`] where the repeat is refused for a root with no
/// declaration behind it. DUPLICATION family, so it says nothing about
/// consuming: such a value may not need consuming at all, and the repeat
/// is refused anyway because nothing grants copying.
pub fn repeated_dup(root: &Affine) -> Option<String> {
    let (subject, hint) = duplication_of(root)?;
    Some(format!(
        "cannot repeat {subject}: {hint} — there is only one value"
    ))
}

/// A `..` skipping a field that must be consumed. `..` means "don't bind
/// the rest", which for such a field means "lose it".
pub fn rest_skips_linear(field: &str) -> String {
    format!(
        "`..` would skip `{field}`, which must be consumed; name it in the pattern so it has \
         somewhere to go"
    )
}

/// [`rest_skips_linear`] inside a generic body, where the SKIPPED FIELD's
/// type is a rigid parameter. Leak family — the field goes nowhere — so it
/// offers the bound, and it names the binder for the reason every other
/// twin does: no declaration in sight said anything about this type.
pub fn rest_skips_param(field: &str, param: &str) -> String {
    format!(
        "`..` would skip `{field}`, and {}: name it in the pattern so it has somewhere to \
         go, or {}",
        leak_reason(param),
        forget_advice(param)
    )
}

/// The loop invariant. Phrased as the next iteration's problem, because
/// that is what makes it one — the body read on its own is fine.
pub fn loop_changes_linear(name: &str) -> String {
    format!(
        "{} is left in a different state than the loop found it in; \
         the next iteration would run against a world this body was not checked in",
        linear_subject(name)
    )
}

/// [`loop_changes_linear`] where the value is merely unduplicable.
/// Duplication family: what the next iteration would do is READ a value
/// this one already moved, which nothing makes legal.
pub fn loop_changes_dup(name: &str, root: &Affine) -> Option<String> {
    let (_, hint) = duplication_of(root)?;
    Some(format!(
        "{} is left in a different state than the loop found it in: {hint} — \
         the next iteration would read one this one already moved",
        linear_subject(name)
    ))
}

/// An item whose own value must be consumed. A `static` is never destroyed,
/// so there is no path to put the consumption on.
pub const ITEM_HOLDS_LINEAR: &str = "an item's value must have the `forget` capability: a `static` is never destroyed, \
     so nothing could ever consume this";

/// A `const { ... }` whose own value must be consumed. Same shape as
/// [`ITEM_HOLDS_LINEAR`], one nesting level down, and the reason is the
/// one that makes a const block a constant rather than a block.
pub const CONST_BLOCK_HOLDS_LINEAR: &str = "a `const` block's value must have the `forget` capability: \
     it is computed once and copied into every evaluation, so no single path could consume it";

/// [`CONST_BLOCK_HOLDS_LINEAR`] inside a generic body, where the block's
/// value is a rigid parameter — reached through the ANNOTATION, since
/// `let x: T = const { ... }` types the block from its position while a
/// diverging body supplies the value.
///
/// Leak family, and the one leak position where [`leak_hint`]'s advice
/// cannot be taken: the value is computed once and copied into every
/// evaluation, so there is no path to consume it on — which is what its
/// non-generic twin says in so many words. It therefore composes its own
/// tail, and the bound is the whole of the way out.
pub fn const_block_holds_param(param: &str) -> String {
    format!(
        "a `const` block's value must have the `forget` capability, and {} — no path here \
         could consume it, so {}",
        leak_reason(param),
        forget_advice(param)
    )
}

/// Writing over a place that holds a value which must be consumed, where
/// no binding names it — a field, an element, a `.&mut` referent. The twin
/// of [`assign_over_live`], with "this place" standing in for the name.
pub const ASSIGN_OVER_PLACE: &str = "this place still holds a value that must be consumed; \
     assigning here would lose it";

/// [`ASSIGN_OVER_PLACE`] where the place's type is a rigid parameter. Leak
/// family, and the binder is the only thing there is to name: a bounded
/// parameter is forgettable, so this never fires for one.
pub fn assign_over_place_param(param: &str) -> String {
    format!(
        "this place still holds a value that must be consumed, and {}",
        leak_hint(param)
    )
}

/// Instantiating a parameter that WROTE `T: forget` with a type that has
/// none. Names the bound as the promise it is, and says what the promise
/// was for — the body may drop a `T`, and this one cannot be dropped.
pub fn forget_bound_unsatisfied(param: &str, ty: &str, reason: &str) -> String {
    format!(
        "`{ty}` cannot be a `{param}`: {reason}, and `{param}: forget` \
         asks for a type whose values may be dropped on the floor"
    )
}

/// A match-arm `_` on a value that must be consumed. The match already
/// consumed the scrutinee, and `_` bound nothing to consume it with — the
/// same sentence [`rest_skips_linear`] says about a record field, and the
/// same one the unnamed `let _ =` binding gets through
/// [`linear_subject`].
pub const WILDCARD_SKIPS_LINEAR: &str = "`_` matches the value without binding it, and it must be consumed; \
     give it a name so it has somewhere to go";

/// [`WILDCARD_SKIPS_LINEAR`] inside a generic body, where the SCRUTINEE's
/// type is a rigid parameter. Leak family, like the `..` it is the arm-shaped
/// twin of.
pub fn wildcard_skips_param(param: &str) -> String {
    format!(
        "`_` matches the value without binding it, and {}: give it a name so it has \
         somewhere to go, or {}",
        leak_reason(param),
        forget_advice(param)
    )
}

/// A block-tailed expression continued by an infix or postfix token that
/// sits on a LATER LINE than the `}` it continues.
///
/// Nothing here is wrong. A statement whose expression ends in `}` closes
/// itself, and the expression grammar stays greedy across that brace, so
/// `if c { } - 1` is one subtraction — the language never guesses which
/// reading was meant. But when the continuation moved to its own line, the
/// text says "new statement" and the grammar says "same expression", and
/// only the author knows which. Hence a warning that names BOTH ways out:
/// the separator that splits and the affirmation that keeps one expression.
///
/// The message names the separator in every case; whether it is also
/// OFFERED as a quick fix is a narrower question, and
/// [`crate::SplitPoint::could_begin_one`] answers it — where the split
/// reading is not a program, the affirm route is the whole answer and the
/// sentence still says the true thing about what the separator would do.
///
/// `split` supplies that separator because it is not always `;`: between
/// match arms it is `,`, and a `;` there is a parse error.
pub(crate) fn block_tail_continued(op: &str, split: crate::SplitPoint) -> String {
    let (separator, started) = (split.separator(), split.started());
    format!(
        "this `{op}` continues the expression that ends with the `}}` above, \
         rather than starting a new {started}; write `{separator}` after that \
         `}}` to split them, or move the `{op}` up onto the same line (or \
         parenthesize the whole expression) if one expression is what you meant"
    )
}
