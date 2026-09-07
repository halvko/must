# Memory and borrows

## Conclusions

- **M01** Typed abstract memory. A pointer is `{ allocation, path }` with `Field`/`Index` path
  elements: element-granular offsets, never bytes. Allocation ids are never reused, so
  provenance exists by construction, and every UB mapping follows: use-after-free is a deref
  of a dead allocation, out-of-bounds an index past the length, a dangling local a frame pop.
  Only address-taken locals reach the allocation table. Copying a value containing a pointer
  copies the bits, so interior pointers survive a whole-value overwrite and do not follow
  copies of the container. Pointers display opaquely, never as a number. This is why byte
  layout could be deferred: v1 never observes it.
- **M02** Validity is checked at deref, not at offset creation; stricter creation-time rules
  can be added later, the reverse cannot. Stepping saturates, so a one-past-the-end pointer is
  fine to hold.
- **M03** The operations, all element-counted: allocate, free, `add(p, usize)`,
  `offset(p, isize)`, `copy` (memmove semantics), `dangling`. `unsafe` is required for a raw
  deref, pointer arithmetic, freeing and copying; taking a raw borrow, comparing pointers,
  `dangling` and allocating are safe.
- **M08** `.&raw` is not a decayed safe borrow, which forces the aliasing model's
  tag-inheritance shape (M10). A raw reborrow `x.*.&raw mut` is safe to create, because every
  consuming operation on a raw pointer is gated: safe code may create a dangling raw pointer
  but cannot use one. This is the language's single region-laundering path; the soundness
  review must name it.
- **M04** First-class `&T`/`&mut T` plus a borrow checker; raw pointers remain the unsafe
  substrate. The system is designed as one piece with outlives first, because a borrow checker
  contains a region engine. Hard constraint: the specialization law (X11); lifetimes reject
  programs and never choose behaviours.
- **M05** Param-named lifetimes are rejected; any revival is sugar. One binder list, three
  kinds: regions ride the generic binder slot as a distinguished, erased kind. Const arguments
  reach instance keys; regions never do.
- **M06** No elision at launch. Every region a signature binds is hand-written until a corpus
  can discriminate the candidate rules.
- **M07** What a reference permits. Safe `.*` is ruled in; safety is decided by pointer
  flavour. `x.*` yields a place; reading it copies when the referent is copyable and is
  rejected otherwise. `&mut` is affine: the value cannot be duplicated, but projection through
  the place holding it repeats. Write permission is therefore judged over the whole place and
  not its outermost step: a `.&` may not be written through, and neither may a `.&mut` held
  behind one, since reading that `.&mut` out is itself a reborrow a shared place cannot grant.
  A `.&raw mut` step ends the walk, because reading a raw pointer out copies it, permission
  and all. Each mention in a position that WANTS a borrow mints a fresh, shorter-lived
  reborrow with the parent suspended for its region; a mention read into a position with no
  borrow-typed expectation (a bare `let`, a read of an affine field) still copies, invisibly
  to both layers, and closing that is a typing change rather than a checker one. Must's `&mut`
  therefore behaves as Rust's, but reborrows are inserted at every use rather than at coercion
  sites, which removes Rust's `let y = x;`-moves wart. Degradation to `&` cannot be subtyping
  (a callee could stash the shared reference for the whole region), so it is also a reborrow.
- **M09** Invariance first. Adding variance later is one query and one argument, and the
  union-find trap is unreachable because region values are sets propagated monotonically over
  a DAG. Three implementation rules, each the only way to throw the work away: relate regions
  with an explicit variance argument from the start rather than equating them; do not collapse
  invariance-equated regions into equivalence classes (keep the outlives edge directed); never
  let a real region reach a type or the unifier. The cost is solver precision: invariance
  emits a 2-cycle at every relation site, so regions merge, loans live longer, and the
  spurious errors that follow are ones users cannot attribute, because the two regions look
  identical in source. The un-retrofittable part is library API shape: a slice's region arity
  and narrowing signature are public from day one, so split read-only from mutable views then.
- **M10** Tree Borrows' structure, strictly: the shape, not Rust's policy. Three commitments:
  a tree of provenance, not a stack; per-node, per-location permission state machines driven
  by access, which buys read-read reordering for the planned optimizer; and a raw borrow
  inherits its parent's node, forced by M08. Posture: more UB than TB wherever Must is unsure,
  relaxed additively. Specifically, no reservation phase at launch and `&mut` starts `Unique`;
  Rust cannot do this because of two-phase borrows, Must can because self-last evaluation made
  them unnecessary, and relaxing later is pure UB removal. Underneath, everything lives by
  access. The borrow checker is what makes safe references appear to live by existence,
  statically. The aliasing model is the dynamic layer and must not know about lifetimes.
- **M11** The aliasing tree is path-granular and reads are path-exact. Every node carries the
  element path it was minted over, and a foreign node reacts only when the two paths overlap
  (one is a prefix of the other), so disjoint fields and elements of one allocation are
  borrowed independently. An access without an exact path passes the root, which over-reports
  rather than under-reports. Reads are exact because stores already resolve their exact place;
  anything coarser would make reading a disjoint field stricter than writing it.
- **M13** Match projects through borrows. Matching a borrow binds payload sub-place borrows;
  matching an owned place copies or moves as before; no new pattern grammar. The scrutinee's
  flavour decides, all the way down. Each payload binder gets a fresh region bounded by the
  scrutinee's (bounded, not shortened, so it can take the parent's whole region, which is what
  makes an `as_ref`-shaped member writable). The tag test — or, for a `char` referent, the
  literal-equality test — is a read through the borrow, so an invalidated scrutinee is caught
  at the match; a match that dispatches on nothing performs no tag test, and so no access.
  The rule moves by design: "a referent this match can dispatch on lifts the lens" is defined
  by which patterns exist, so every future pattern-kind grant also changes the lens for
  borrowed scrutinees of that type.
- **M16** Must-consume is the dual of use-after-move: one walk, one fact per binding, so the
  checker that refuses a double disposal is the one that refuses a leak. No unwinding pays for
  it: a panic traps, so every exit from a scope is written in the source, and a checker that
  visits `return`/`break`/`continue`/fall-off has visited all of them. A language with
  unwinding has an implicit exit edge at every call, which is why such languages need
  destructors. A `break` is not a back edge; it carries its state to the loop's exit join,
  while a `continue` answers the same entry-equals-back-edge invariant a fall-through does.

### Ruled, not built

- **M10** Protectors on reference-typed parameters are in, which pre-rules a piece of the
  lifetime design: "a reference argument is alive for the whole call" is a dynamic rule with UB
  attached. Publish the licence list: what the optimizer may assume, each with its theorem;
  anything not on it is a hope. Neither is built by this stage's aliasing tree.
- **M10** Per-location state within one node is approximated, not built: a node minted over a
  WIDE path (a whole struct, a whole array) is disabled wholesale by any foreign access into
  any part of it, never partially — the path granularity M11 buys stops at the node boundary.
  Strictly more UB than true per-location tracking, on the stated posture; relaxing to genuine
  sub-node partitioning later is pure UB removal.
- **M12** Temporaries live to the end of the innermost enclosing block, always: no shape-based
  extension rules, no liveness derivation; a shorter life is spelled with an explicit block. A
  match scrutinee lives to the end of the match; a loop condition is per iteration. Tail
  expressions are a borrow-check question, not a storage rule, with one diagnostic
  requirement: name where the temporary died, because this is the case where the user sees no
  block. Once destructors exist, both directions change observable behaviour, so an ASAP
  variant lands with them or not at all.
- **M17** Self-referential structs are not v1, deferred on the typing rather than the move
  hook: a field pointing into its own struct needs a region naming the struct's own storage,
  which is not a parameter. Direction: moves may run code, the opposite of Rust, which made
  moves a memcpy and invented pinning to cope. It is safer here because the borrow checker
  guarantees no outstanding borrows at the move. The price is that memcpy-move dies: growth,
  swap, sort and replace all route through the hook. Design `unsafe move` first.
- **M18** Interior mutability in v1 is an immutable handle containing a raw mutable pointer,
  with the pointer flavour governing write-through. Long term, a nominal exclusivity-exempt
  cell type.
- The acceptance test the design must pass: use-after-free after arena teardown (an outlives
  violation); double free by value copy (moves plus region close); dangling handle copies
  after growth (use-after-move); cross-allocator buffer stealing (region inequality, with the
  runtime identity assert as a backstop).

## Discarded

- **Pointer-to-integer casts** — the interpreter is a complete UB detector where Miri is
  explicitly incomplete, and integer addresses would close the moveability and provenance
  doors permanently. **A byte-addressed or flat linear interpreter memory** — address reuse
  makes use-after-free silently read new data. **M01**
- **Second-class references / mutable value semantics** — overridden, not refuted: its case
  was that first-class references are the single largest complexity cliff available, and that
  bill is being paid deliberately. **Raw pointers only** — every collection API would be
  unsafe-flavoured. **Full linearity** — ergonomically heavy at scale; the useful fragment
  returned as must-consume (M16). **M04**
- **Coarse scoping (locality modes, second-class values)** — a two-point lattice can say
  "escapes nothing" but never "outlives arena A but not B"; it tracks escape rather than
  allocator origin; and a container stores its handle, which is first-class use by definition.
  **M04**
- **Runtime-checked safety** — generational references cost 2–10.84% and lose on currency;
  refcounting is hidden allocation traffic plus a de facto runtime; tracing GC fails the
  embeddability checklist (P02). None of them answers allocator-outlives: a GC'd container
  over an arena still dangles when the arena resets, because GC protects the allocation, not
  the allocator relationship. **M04**
- **Param-named implicit lifetimes** — the legality condition is non-local, so adding a region
  to one type breaks unrelated signatures. **M05**
- **Regions as ordinary const parameters** — violates erasure. **Wholesale region elision at
  launch.** **Implied bounds at launch** — sugar, deferred with the rest. **M05 M06**
- **Body-local region names as binders** — a signature region is universal and a body region
  existential, so a body-local name asserts that two regions coincide; it is an annotation,
  not a binder. **M06**
- **Stacked Borrows** — it contradicts M08 (direct-raw-borrow-then-use-the-place is what it
  calls UB); it forbids read-read reordering, so the planned optimizer would be illegal on day
  one; its two known patches paper over the reference-to-raw decay Must designed out; and "SB
  first, TB later" is not a relaxation, because the models are incomparable. **M10**
- **Inventing an aliasing model** — buy proven components. **Published TB verbatim** —
  strict-first is the hedge, and it is about policy, not structure. **Dynamic exclusivity
  without provenance** — buys almost no optimizer licences and cannot describe raw pointers.
  **M10**
- **Contravariance** — no measured genuine uses, and the shape people write is not rescued by
  it. **A targeted "all-static regions may be viewed shorter" rule** — deciding whether
  shortening is safe needs per-parameter shared-versus-exclusive information, which is
  variance under another name. **M09**
- **Lifetimes choosing codegen** — selection becomes inference-dependent, MIR stops being
  erased, borrow check stops being a decl-level query, the two form a mutual fixpoint with no
  termination argument, and the aliasing model becomes hostage to the solver. **M09 M10**
- **An escape lint for raw reborrows** — an accurate one needs static raw-pointer provenance,
  which the model deletes, so any cheap approximation false-positives. **M08**
- **Binder markers in patterns (`t.&`) and binders-as-places** — patterns stay
  construction-shaped; a per-binder mode breaks construction/destruction symmetry. **M13**

## Re-evaluate when

- **A borrowed `str` representation lands** — copying one out of a borrow stops being a copy,
  and the model needs a byte-range path element it deliberately lacks (T17). **M11**
- **The no-reservation fork** — relaxing is pure UB removal; it was ruled ahead of the
  experiment that would justify it. **M10**
- **Covariance is wanted** — the three rules in M09 keep it a one-query change. It revives the
  unused-region-parameter question: a slice's region appears in no field, so variance cannot
  be derived structurally, and a phantom-field marker is written variance. "Variance derived,
  never written" and "no marker" are the same wish, and a slice contradicts both; the question
  is how to spell an annotation that must exist. **M09**
- **Elision rules come back for measurement** — needs an instrument that says which candidate
  rule would have supplied each written region. Without it the corpus reports that all of it
  hurts, which selects no rule. **M06**
- **The move-versus-reborrow rule needs an explicit ruling** — proposed: always reborrow when
  the target region permits it, move only when it requires the source's full region.
  Reject-only under the erasure law. **M07**
- **`unsafe move` is designed** — weigh it against two alternatives: indices (costs nothing,
  moves stay memcpy, fails only where the pointer must be real) and pinning (a discipline
  everywhere). Who the customer is decides it. **M17**
- **Destructors land** — they owe the match-scrutinee-temporary lint (a scrutinee with a
  destructor keeps its loans live through every arm: the deadlocking `match lock()`), drop
  order, glue, partial moves, and the ASAP decision. **M12**
- **Data races** are out of scope for both aliasing models. Whether well-typed safe programs
  can violate the model is tested, not proven (as in Rust); it lands at the soundness review.
  **M10**
