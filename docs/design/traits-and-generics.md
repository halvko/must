# Traits and generics

## Conclusions

- **TR01** Trait and impl syntax, sealed. `trait N = requires { ... };` declares; aliases are a
  committed second constructor. Impls attach as `impl`-keyword elements inside a required
  `with`-chain, at one of three homes: the trait's declaration, the self-type's head, or, via
  a `for`-head covering `Self`, an anchor type in the self-type's arguments. Binder algebra:
  `with::<U>` declares, `with T: Bound` constrains, `with T = usize` pins. Modifier heads
  (`unsafe`, `for <Type>`) distribute over the next element or a brace group. Qualified
  references use named `Self` (`From::<u8, Self = T>::from(x)`), with a short form when `Self`
  is inferable. Clauses immediately precede the item's defining brace. There is no `self`
  token: dot-call is structural, so a member whose last parameter is `Self`-typed is
  dot-callable and the receiver becomes its LAST argument — which is why the written arguments
  evaluate before the receiver binds. Expression-position `Self` is rigid: one meaning per
  body, the owner type at the member's own binders. Implemented so far: the inherent home,
  `impl Self { name = fn(...) -> R { ... }; }` on a `type` declaration. Trait declarations do
  not parse yet, and a member must spell its full signature — that is what lets a dot-call
  resolve without running inference.
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

## Discarded

- **Declaration spellings that lost**: `interface` (a permanent two-vocabulary cost);
  `type N = trait` (kind-dishonest: a trait classifies types, not values); binder-on-the-name
  (breaks the constructor-binder invariant); methods inside the struct body (the brace split
  mirrors the runtime split: fields are data in the value's layout, impls are static fns no
  value carries a pointer to); keywordless elements (context ambiguity, greppability, diff
  anchoring). **TR01**
- **Auto-ref to dodge the receiver wall** — a conversion policy with inference consequences,
  not a spelling: the receiver's type must BE the member's `Self`. **TR01**
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
- **Closures are built** — that owns the access-marker spelling, capture modes and syntax, and
  the capture-free-literal fork: nominal like every closure and coerced to the structural fn
  type (uniform, one seam), or a structural fn value directly (no coercion machinery, but
  adding one capture silently flips the type). **TR08**
- **The stdlib grows higher-order functions** — the cheap moment for effect polymorphism;
  every HOF written at a fixed row meanwhile is migration debt. Foreclosure risk is near zero:
  rigidity transplanted, no new inference domain. **TR08**
- **Derive replacement is designed** — introspection. **TR12**
- **A `send` assertion form** is owed and unruled: the mitigation for the private-field semver
  hazard, which is inherent to derived-from-structure. **TR09**
