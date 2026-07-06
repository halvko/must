# Const and compile time

## Conclusions

- **C01** `static` is identity; `const` is copied. A static has one address, promised and
  observable. Mentioning a const item produces a copy, and taking its address takes the address
  of the temporary holding that copy. Only static pointer identity is promised; const-mention
  identity is unspecified (as for string literals), leaving codegen free to merge and inline.
  Statics are evaluated eagerly.
- **C02** Constness is written, not inferred. A fn literal is const only with an explicit
  `const fn` marker, and a call whose value is not provably const is rejected rather than
  speculatively evaluated.
- **C03** Const evaluation runs on a fuel budget, per item and with failures memoized, so
  a runaway item costs its own budget once; run mode is unfueled. Const blocks and const
  arguments inside uncalled functions are forced at check time, so their fuel-outs and
  traps are editor diagnostics rather than latent runtime crashes; dead code can emit
  const-eval errors. A compile-time-known out-of-bounds index becomes a value trap at
  lowering, so it is not reported twice.
- **C04** `unsafe` is legal in const contexts. Under typed abstract memory every would-be UB in
  const eval is a deterministically detected trap, so const evaluation cannot exhibit UB, only
  report it. Pointers cannot escape const evaluation: an allocation id is per machine run, so
  pointers are excluded from the const-argument domain exactly as fn values are. Heap
  operations are refused eagerly under a const context. Eagerness matters: relying on the
  escape rule alone would let allocate-use-free work silently, and lifting that later would be
  a retraction of observed behaviour rather than a grant.
- **C05** Const arguments in type mentions are restricted to literals and const-parameter
  names. The obstacle is not ordering (the query engine is demand-driven) but a cycle: type
  identity → inference → const eval → MIR lowering → inference. Two relaxations, separately
  priced. Closed const blocks (`Pair::<{ 1 + 2 }>`, mentioning no binders) are stratifiable: a
  const-argument query, a self-reference cycle guard, and normalization by evaluation before
  interning. Const blocks mentioning binders (`Pair::<{ N + 1 }>`) require deciding equality of
  symbolic const expressions for type identity, which is undecidable in general; the options
  are fragile syntactic normalization or per-instantiation checking, which abandons X13.
- **C06** Interning and freezing (ruled, not built). Escaping a const context means "became
  the item's memoized value": the memo is the freeze step. Frozen values are unconsumable, so a
  frozen buffer never reaches `dealloc`; a static is not consumable and a const is copied.
  Resurrecting as `.data` is right for the immutable case. Const contexts get a distinguished
  builtin const allocator, refused by const check outside them, so there is no
  comptime/runtime API gap. Target shape: `static TABLE = const { ...build a Vec... };`.
- **C07** Const indexing (ruled, not built). Tuples get builtin const-required indexing as a
  language item, with semantics and diagnostics pinned to the trait desugar so the interim
  cannot drift from what the trait will mean. Out-of-range is an ordinary missing-impl error
  at the mention site: bounds by coherence, not dependent const bounds and not value
  predicates. Resolution is receiver-directed; const-ness never selects the trait.
- Type-producing `-> type` const functions are ruled in, unscheduled. They take only const
  arguments, so type-parametric families are expressible only through generic type
  declarations; the two features are complementary.
- Introspection is the intended replacement for derive: const-evaluated code walking a
  definition-ordered shape (X14). Nothing binds to it in v1.

## Discarded

- **Const-argument inference** — running an instance backwards is inference through a
  conversion; widening never runs backwards either (T04). **C05**
- **Fn values as const arguments** — a fn value carries an arena index that renumbers under
  body edits, so instance identity and FFI symbols would churn. **Pointers as const arguments**
  — allocation ids are per machine run. **Dependent const parameters** — no use case before
  traits, and they entangle const checking with in-flight substitution. **C04**
- **Const heap allocation in v1** — its premise was that const values are closed owned trees
  with no addresses; the pointer ruling (M01) removed that premise. **A lazy const fence** —
  allocate-use-free would work silently, so the eventual revisit would be a retraction. **C04
  C06**
- **Type-producing const fns instead of generic type declarations** — they take only const
  arguments, so `Option<T>`-shaped families are not expressible through them.
- **Per-instantiation const indexing (Zig-shaped)** — abandons checked-once, keys inference by
  instantiation, surfaces errors at sites the body's author never sees, and breaks per-body
  blame. **A const-indexed associated-type family slot** — one `index` body would have to
  return a different type per index, which is the previous option again or compiler magic.
  **C07**
- **Const-ness-directed bracket resolution** — refactoring `x[0]` into `let i = read(); x[i]`
  would silently switch traits and change the result type. It also makes evaluability a
  name-resolution question, against the rule that no impl head's target may require inference
  or const eval. **C07**
- **A homogeneous-tuple special case for runtime `t[i]`** — adding a differently-typed
  component becomes an error at distant index sites. **C07**

## Re-evaluate when

- **Interning is built** — it owns resurrect-as-`.data`, the memo boundary, the frozen-clone
  spelling, and lifting the eager const fence. Until then the fence stands. **C04 C06**
- **Someone wants `Pair::<{ 1 + 2 }>`** — the stratifiable relaxation; takeable on its own.
  **Someone wants `Pair::<{ N + 1 }>`** — only together with a ruling on applicative versus
  generative instance identity (TR06); they are one question. **C05**
- **`size_of` exists** — the zero-size contract strengthening (A05). **C04**
- **The trait retrofit reaches const indexing** — it owes the trait shape,
  bounds-as-coherence, projections carrying trait arguments, and impl-head pins. Tuple
  nominality must be ruled before that step and may stay unruled during the builtin interim.
  Under bounds-by-coherence the arity contract is implicit in the impl set, and missing-impl
  errors are the worst-explained errors in trait systems; a bridge is deliberately deferred.
  **C07**
- **Const-sized arrays want both `arr[i]` and `arr[K]`** — the receiver-directed fence is
  revisitable, not doctrine; homogeneous output moots the type-cliff argument. **C07**
