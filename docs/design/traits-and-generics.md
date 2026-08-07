# Traits and generics

## Conclusions

- **TR01** Trait and impl syntax, sealed; non-generic traits are live end to end. `trait N =
  requires { ... };` declares a set of named, fully-signatured requirements; aliases are a
  committed second constructor, still reserved. Impls attach as `impl`-keyword elements inside
  a required `with`-chain, at one of three homes: the trait's declaration, the self-type's
  head, or, via a `for`-head covering `Self`, an anchor type in the self-type's arguments —
  the third, anchor home stays reserved. A trait-side impl's `Self` is the head's implementing
  type (a builtin scalar or a non-generic `type` item — builtins have no declaration of their
  own to host the other home); a type-side impl's `Self` is the owner. Binder algebra:
  `with::<U>` declares, `with T: Bound` constrains, `with T = usize` pins. Modifier heads
  (`unsafe`, `for <Type>`) distribute over the next element or a brace group. Qualified
  references use the SHORT form, `Trait::member(args)`, with `Self` inferred from the
  arguments; the named-Self form, `Trait::<Self = Type>::member(args)`, spells `Self` out and
  is the escape when the short form itself is ambiguous (G13). Clauses immediately precede the
  item's defining brace. There is no `self` token: dot-call is structural, so a member whose
  last parameter is `Self`-typed, or a safe borrow of `Self`, is dot-callable and the receiver
  becomes its LAST argument — which is why the written arguments evaluate before the receiver
  binds. Expression-position `Self` is rigid: one meaning per body, the owner type at the
  member's own binders. Reserved for later: trait aliases, generic traits and generic-type
  impls, supertrait clauses, default members, associated types/consts, `unsafe` traits and
  trait members, and marker impls — the house parse-and-reserve pattern throughout.
- **TR02** Dispatch is static only, flattened dictionary-lowered. No `dyn` in v1. Every bounded
  type param contributes one dictionary SLOT per `(param, resolved bound trait)` pair
  (`hir::bound_slots`, canonical order: params in binder order, bounds in written order,
  duplicates collapsed); the body's hidden dictionary parameters are appended strictly AFTER
  its written ones, in that same order — no single record-typed parameter. Bound obligations
  are queued at instantiation edges as inference runs and resolved in `finish()`, right after
  `solve()`, into one dictionary entry per slot: a concrete receiver's impl members become
  `Const::Item` values (a rigid argument's own bound forwards the enclosing root body's hidden
  locals instead), and a concrete receiver outside a generic body skips the dictionary
  entirely for a direct call. Bound checking consumes solved types and never drives
  unification, which keeps type identity applicative. A qualified trait-member call is
  conservatively treated as a value call in const contexts (X12, strict-first).
- **TR03** Impls attach to nominal types only: non-generic `type` declarations and builtin
  scalars (`SelfKey::Decl` and `SelfKey::Builtin`, respectively) — only builtins, which have
  no declaration of their own to host a chain, are reachable through the trait-side home
  alone. Coherence needs an owner; a structural type (a record, array, fn type or raw
  pointer) has none, so `SelfKey::for_ty` returns `None` for one and a bound on it is
  `UnsatisfiedBound` — the escape is one `type` line.
- **TR04** Coherence: decl-pair buckets, keyed `(trait decl, SelfKey)` — a hash lookup off the
  item tree via the shared with-chain element traversal, with no inference and no body
  parsing. Builtin implementer names canonicalize through the type itself (`string` and `str`
  are one key). The bucket law is pairwise ground-disjointness: today, with both traits and
  implementing types non-generic, that is trivially satisfied — a bucket admits exactly one
  impl, and a second one (including a same-pair impl in the OTHER home) is diagnosed at the
  later site, naming both. The bucket structure is what generic traits extend with the
  pairwise ground-disjointness test.
- **TR05** Bounds. `+` composes a param's bound list. Bare trait names are not types
  (`TraitNotValue`) — traits are bounds, never `dyn`, no implicit existentials; naming a type
  where a bound is expected is `UnsatisfiedBound`'s dual, a checked error. A requirement's own
  binder may carry bounds independent of the trait's (`Display`'s `fmt::<W: Write>`), and an
  impl member's binder must match its requirement's, position by position, in the same
  resolved bound set (`binders_match`). Bounds on a `type` declaration's own binder are
  reserved ("not supported yet").
- **TR06** Generics are item-level only: no first-class generic values, no higher-rank types,
  schemes never enter the type. Instantiation identity is applicative: the key is
  `(ItemLoc, canonical args)`; for distinctness, wrap it in a `type` declaration. Bodies check
  once against rigid parameters, with call-site blame through a generic-argument cause.
  Call-site syntax is turbofish, routed by form; type parameters are inferred and `_` is
  allowed; const parameters are never inferred, and `_` in const position is an error. The
  const-parameter domain is any concrete data type. Monomorphization is hybrid: one MIR per
  generic item, with const arguments carried on the instance.
- **TR07** A rigid parameter supports no operations at all; bounds are the only mechanism that
  re-opens them. This decided the allocator interface's shape (A06) and is why
  declaration-site checking is sound.
- **TR08** Closures are nominal: each literal is its own type hiding a capture struct,
  callable through a trait impl, statically dispatched, for zero call overhead. The arrow's
  kind slot is row-shaped, flat until a payload-carrying axis exists. `Fn`/`FnMut`/`FnOnce`
  collapse into one `Fn` trait plus access markers (spelling open). Thread-crossing is
  structurally derived with a definition-site-only unsafe override. Effect handlers are
  fenced, with one crack: asm-jump-compilable effects maybe; runtime continuation capture
  never. Fn items and capture-free fn values keep structural fn types; a blanket "every
  `fn(A) -> B` satisfies `Fn(A) -> B`" is needed when traits land, or the two families split
  the ecosystem.
- **TR09** Send-ness is derived from structure: a value fails to cross iff its type
  transitively contains a fn value with captures at non-portable, a raw mutable pointer to
  shared mutable state, or a field of a type that opted out. Everything else crosses silently.
  Annotation inventory: one for the stdlib author, one per thread-safe allocator, zero for
  application code.
- **TR12** No derive in v1. Copyability stays a builtin judgment (T08); the override of any
  derived judgment is an `unsafe impl`-shaped, member-less, definition-site-only declaration,
  the house pattern for every derived or marker judgment. `clone` for heap-owning types is
  hand-written by ruling: it must read the stored allocator, so it is not derivable even in
  principle. Introspection replaces derive.
- **TR10** Member-own binders. An inherent member's binder is the owner's followed by its own,
  so the owner keeps the low indices and nothing downstream moves; the member's own half is in
  declaration order, kinds interleaved as written. At the use site a member's own type
  arguments are spellable, in both spellings of one instantiation and in the trait forms;
  regions are inferred and are not positions in that list. A member with no binder takes no
  arguments, including an empty `::<>`. There are three spend sites (inherent member,
  trait-impl member, bound-directed requirement), each of which consumes the written list, so
  no already-refused path adds a second diagnostic.

## Discarded

- **Declaration spellings that lost**: `interface` (a permanent two-vocabulary cost);
  `type N = trait` (kind-dishonest: a trait classifies types, not values); binder-on-the-name
  (breaks the constructor-binder invariant); methods inside the struct body (the brace split
  mirrors the runtime split: fields are data in the value's layout, impls are static fns no
  value carries a pointer to); keywordless elements (context ambiguity, greppability, diff
  anchoring). **TR01**
- **Auto-ref to dodge the receiver wall** — the bounded reborrow exception (G14) dissolves the
  wall instead. **TR01**
- **Member-own type binders as redundant sugar** — refuted: true about scope, false about
  need. An owner's binder is fixed per value, a member's own varies per call. Check any "you
  could already say that" objection against "could you say it per call". **Member-own const
  binders** — still reserved: a const argument is part of an instance's identity, and a
  member's arguments are read off the receiver's type, which cannot supply one. **TR10**
- **`dyn` / trait objects in v1** — no customer; brings vtable layout, object safety and
  post-erasure lifetime questions. **Named / first-class impls** — a named impl is a
  dictionary you can pass, which reintroduces incoherence and breaks applicative identity: the
  same bound would mean different code at different sites. **TR02**
- **User impls on structural types** — no owner, so two impls collide with no principled
  winner, and modules would inherit an orphan rule with no orphan test. **TR03**
- **Header-unification overlap checking as the coherence rule** — decl-pair keying is strictly
  stricter, additive to loosen, hash-map cheap, and makes the always-applicable law trivially
  true. **Self-type blankets** — the key needs a type decl and a for-all impl has none. **TR04**
- **First-class generic values / higher-rank types** — binders in an engine that is
  deliberately binder-free; a solver rewrite. **Generic items joining inference groups** — one
  shared signature variable pins them to a monotype. **Unannotated generic literals** — would
  need scheme syntax in annotation position. **TR06**
- **The product encoding of function kinds** — 3 access × 2 const = 6, plus async = 12, plus
  generators = 24, and a payload-carrying axis is a family, not a trait. **Flat marker sets at
  full generality** — they die on generators: a marker set with type-argument'd members and
  set variables is an effect row. **Inferred effect/mode polymorphism** — against the house
  style on every axis. **TR08**
- **Algebraic effect handlers** — incompatible with no-runtime: resumable continuations mean
  heap-allocated continuation state plus an allocator and refcounting underneath. Genuine
  loss: generalized control flow and effect-based dependency injection. **TR08**
- **The `Fn: FnMut: FnOnce` chain** — loses to the collapse and buys neither surface
  familiarity nor per-discipline trait identity. **Receiver-slot and verb-suffix marker
  spellings** — the first puts a `self` token in every higher-order bound; the second
  re-imports the two-subjects-on-`+` problem, and receiver access is a constraint the call
  obeys, not a capability the value grants. **TR08**
- **The modes framework** — dissolved into four pieces: `const` became an empty row,
  thread-crossing became structural derivation plus an override, escape went to lifetimes,
  per-value tracking was ruled never. Each leg lost individually: the machinery-cost argument
  fails because a trait system is needed anyway; per-value granularity fails because a
  thread-safe allocator is a different type, never a constructor flag; and zero-annotation
  ergonomics turned out to be a shared baseline. **TR09**
- **Macro-based derive** — introspection is preferred. **Trait-ising `==` in v1** — a bound in
  half of all generic signatures to buy nothing v1 needs. **TR12**

## Re-evaluate when

- **Trait aliases are scheduled or rejected** — the `Name = constructor` declaration shape
  depends on them: if no second constructor materializes, the shape degenerates and
  keyword-declaration forms re-enter. **TR01**
- **The short form's eager Self resolution proves too strict** — a join-typed argument at a
  Self position cannot determine `Self` today (`Id::id(if b { 1 } else { 2 })` is rejected
  even though both arms agree); error-side and relaxable. **TR02**/**TR05**
- **Generic traits or generic implementing types land** — ground-disjointness stops being
  trivial (today's one-impl-per-bucket law) and needs the real pairwise test; answer the law
  question first: may two bound-sets coexist. **TR04**
- **Closures are built** — that owns the access-marker spelling, capture modes and syntax, and
  the capture-free-literal fork: nominal like every closure and coerced to the structural fn
  type (uniform, one seam), or a structural fn value directly (no coercion machinery, but
  adding one capture silently flips the type). **TR08**
- **The stdlib grows higher-order functions** — the cheap moment for effect polymorphism;
  every HOF written at a fixed row meanwhile is migration debt. Foreclosure risk is near zero:
  rigidity transplanted, no new inference domain. **TR08**
- **Member-own const binders** — trip-wire: instantiation grows a member-side argument list,
  or instance identity moves off the receiver's type. **TR10**
- **Separate item and member turbofish rules** — the member list spells its TYPE parameters
  only while the item list is positional over the whole binder, `@_` included; trip-wire: the
  item turbofish stops spelling regions, or member consts land. **TR10**
- **Derive replacement is designed** — introspection. **TR12**
- **A `send` assertion form** is owed and unruled: the mitigation for the private-field semver
  hazard, which is inherent to derived-from-structure. **TR09**
