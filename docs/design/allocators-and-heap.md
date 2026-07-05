# Allocators and the heap

## Conclusions

- **A01** The root allocator is a platform import: immutable, program-level, resolved at final
  link, supplied by the platform or embedder, per embedding instance rather than per process.
  No user setter, no fallback; a target without a root fails to link (a symbol scan over
  weak-symbol resolution). The core language never allocates implicitly. This is not a global:
  a global is a user-settable ambient singleton; this is an explicit, immutable, per-instance
  binding written by the embedder instead of the application.
- **A02** Every allocation site names its allocator; nothing is elided. A container stores its
  allocator handle, so disposal takes no allocator argument and freeing with the wrong
  allocator is structurally impossible per value. Mixing instances is an explicit clone;
  same-instance is enforced by a runtime same-allocator assert plus interpreter UB detection.
  Composition is a language rule: libraries never instantiate their dependencies; requirements
  float up and dedupe at one composition root. Applicative instantiation identity (TR06) is
  what makes that dedup sound.
- **A03** The allocator interface. `alloc(state, n)` returns `n` uninitialized elements, never
  bytes, and is result-shaped because allocation is fallible. `dealloc(state, p, n)` is an
  exact-match free: the allocator needs the size back, and Must will not smuggle it through a
  header, because a header is byte layout. There is no `grow`/`realloc`: promising in-place
  reallocation constrains every allocator for one optimization, so growth is composed by the
  container. Allocating is safe (a leak is not UB); deallocating requires `unsafe`.
- **A04** `dealloc` has a four-way UB contract, all detected by the interpreter: a non-head
  pointer, a count that differs from the alloc-time count, a local or static allocation, and a
  double free. Freeing reuses the frame-pop mechanism, so the last case also extends
  dangling-pointer detection. Wrong-type dealloc is not detectable under erasure; v1 accepts
  that. Fresh elements carry a poison no surface program can name, and reading one is
  detected UB. There is no surface uninitialized-wrapper type.
- **A05** `alloc_array(0)` traps. A refused request is a trap, and that is defined behaviour.
  The cost: every growable container spells "no allocation" as zero capacity plus a dangling
  pointer.
- **A06** Concrete containers, no vtable. A heap vector and a typed arena are the v1 library.
  Rootless allocators exist: a raw borrow of a mutable local array plus pointer stepping is a
  stack-buffer arena that references no root symbol. A stateless allocator still takes a state
  pointer; `dangling` supplies it. Accepted debt: M×N duplication once there is a second
  container or a third allocator.
- **A07** Type-level allocator brands are rejected: a brand parameter is viral through every
  signature that stays precise. Static instance-distinctness is deferred to regions, where it
  should fall out of region identity rather than be a second mechanism. The runtime
  same-allocator assert stays even then: region equality proves outlives, the comparison
  proves identity, and two handles can share a region without being the same allocator.
- **A08** No `defer`. Early-return paths leak; a leak is not UB and never will be. A leak
  report can be added later at no retrofit cost, because every allocation already records a
  birth origin. Such a report is interpreter quality, not language semantics.
- Two things v1 cannot express. The two-word erased `{ ptr, vtable }` allocator: it erases an
  opaque state pointer (a pointer-to-pointer cast, refused until layout exists) and byte-count
  operations (no `size_of`), so a Must vtable is necessarily unerased. The untyped bump arena:
  a byte blob with typed objects carved out of it is pointer reinterpretation. Consequence:
  carved sub-buffers are paths into one backing allocation, so an intra-arena overrun into a
  neighbour is silent corruption, detected only at the backing allocation's edge.

## Discarded

- **A program-wide link-time global allocator** — no per-subsystem override and no
  multi-instance embedding. Rejected as a model; its weak-symbol mechanism is kept and is what
  makes layer-0 purity a symbol scan. **A01**
- **The allocator as a type parameter** — two allocators give incompatible container types and
  every touching signature goes generic. A codegen-time strategy const parameter for
  specialized embedded containers stays possible, never as the foundation. **As a const
  parameter** — a useful allocator is runtime-stateful; const arguments are compile-time
  values. **A02**
- **Threading the allocator through every call** — the mitigation people reach for is storing
  it at init, one struct at a time, which is what a handle-carrying container automates.
  **A02**
- **An implicit ambient context** — dynamic scoping. A hidden pointer costs an ABI register,
  forcing a second "contextless" calling convention so non-allocating functions pay; and
  ambient state that must be initialized before any function runs is a runtime. **A01 A02**
- **Prior art, each surveyed**: implicits and `given` (resolution is still a compiler search
  over ambient scope); modular implicits (signature-directed search is the hard part, and it
  has not shipped in twelve years); effect handlers as the supply mechanism (the row names the
  dependency kind, never the provider); functor towers (the failure moves to composition);
  tooling-level module systems (right semantics, wrong layer); implicit region inference
  (annotated programs were large and hard to read; its authors added a GC); lexical regions
  (the default region was a global GC'd heap, and LIFO arenas do not fit event loops); a
  default polymorphic memory resource (a default-constructed container silently using a
  program-wide mutable default whose behaviour changed between standard versions, plus sticky
  propagation). The lesson: fully implicit is as bad as fully explicit; every survivor uses
  explicit introduction and implicit propagation. **A01 A02**
- **Functor-style static instantiation of an allocator** — instantiation arguments must be
  static and an arena is a runtime scoped value, so it cannot express a per-frame arena. It
  chooses an ambient default rather than scoping an override; it may return as the module
  story. **A module instance as a runtime value** — a record field holding a generic fn is a
  first-class generic value (TR06). **A02**
- **Dynamic `with alloc { ... }` scopes** — its one virtue is retargeting a library's internals
  without its cooperation. It loses because: inside a callee the source stops answering the
  question; values born inside the block outlive it silently; hoisting a call out of the block
  changes its allocation behaviour with no diff on any line of the callee; and the ABI is a
  hidden parameter on every call or thread-local state. Standing rule: any hybrid that
  reintroduces caller-frame resolution is this option again. **A02**
- **Lexical context slots** — stashing is only statically checkable, and the slot protects the
  slot rather than the handle. If it returns, keep its resolution rule: resolution is decided
  by the signature of the fn the text sits in, never by the caller's frame. **A02**
- **One generic container body over an unerased vtable record** — lost to concrete containers.
  Its migration argument may still be how the trait retrofit reads: the trait's method set is
  the vtable's field set verbatim. **A generic container over an opaque allocator parameter** —
  a rigid parameter supports no operations (TR07), and constraining it to a projectable shape
  is the vtable again. **A06**
- **Zero or default fill on allocation** — no default exists, and record defaults are not
  derivable. **Caller-supplied fill** — a growing container has no value for capacity beyond
  length, and pays a fill loop for memory the next write overwrites. **A04**
- **Trap on OOM** — allocation is fallible and result-shaped; changing this later would be
  signature churn across every allocator. **A03**
- **Spelling the root as a new item kind or as an `extern static`** — invents a per-file
  declaration discipline before the module system that will own it; the second also commits
  the FFI surface's most visible keyword early. **A01**
- **An interpreter-native untyped arena primitive** — typed arenas cover the need. **A
  non-overlapping copy primitive** — growth, the only customer, never overlaps; layout era at
  the earliest. **A06**

## Re-evaluate when

- **`size_of` exists** — that unlocks the zero-size strengthening (an allocator may treat a
  zero-size request as UB once the caller can discharge the obligation; strengthen additively,
  never flip the safe primitive), wrong-type dealloc detection, untyped arenas and byte views.
  **A04 A05**
- **A may-grow operation gets a customer** — add it as "may return the same pointer or a fresh
  one", so it never becomes an in-place promise. **A03**
- **Modules land** — builtins become declared imports: resolution moves from the builtin table
  to an import declaration and nothing below sees it. Per-file visibility of the root ("which
  files may allocate") becomes an import-privilege question. **A01**
- **The leak report or `defer` is built** — `defer` needs grammar, MIR insertion on every
  scope-exit edge (traps excluded), and a ruling on its interaction with joins resolving at
  statement boundaries (T10). **A08**
- **The M×N duplication hurts** — the trait retrofit replaces it with one generic body over an
  allocator bound. **A06**
- **Codegen exists** — revisit the OOM posture. Checked OOM is right, but there is no
  error-union ergonomics to make every push a match, and the interpreter cannot run out of
  memory. Builtin names are free to rename before release. **A03**
