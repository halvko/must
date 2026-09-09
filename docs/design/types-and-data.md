# Types and data

## Conclusions

- **T01** Integers are `i`/`u` × 8/16/32/64 plus `usize`/`isize`. Literal typing is inferred:
  an unresolved literal renders `{number}` and is never defaulted, and one with no defining
  use is a diagnostic asking for an annotation. No suffixes, no implicit conversions, no
  mixed-width arithmetic.
- **T02** Overflow traps everywhere, runtime included. One semantics, no debug/release split,
  so a program's meaning never depends on its build profile. Wrapping is spelled explicitly.
- **T09** `==`/`!=` are builtin in v1 and permitted on any type; the checker only requires the
  operands to agree. Structurally defined for records (over canonically sorted fields),
  arrays, tuples, variants, raw pointers and function values.
- **T03** `type Foo =` always mints. No transparent aliases; two identical spellings are
  two types. Records are exact structural types; a structural record has no declaration,
  so it has no owner, and the newtype line is what gives it one. Nominal and structural
  never coerce.
- **T04** Variant widening is a runtime conversion over a tag-free variant representation: a
  variant-typed value carries no tag, and widening adds one. A plain `let` keeps the precise
  variant; an unannotated `let mut` widens to the enum at binding time, the one place
  mutability changes a type rather than only permissions. Trait dispatch adds a second
  widening site: every RECEIVER-LIKE `Self` position — a dot-call's receiver, and each
  argument at a literal-`Self` position of a qualified short-form call — is inferred freely
  first, widens to its enum if it came back a variant, and only THEN is checked against the
  determined `Self`, so `Self` itself can never bind to a tag-free variant type (a variant has
  no impls of its own to dispatch to). A NESTED `Self` position (inside a receiver-like
  argument's own type, never the position itself) does not widen; a variant that reaches
  there lands on the sound `NoTraitImpl` rather than silently picking its enum's impl.
- **T23** The compiler-provided enums (`AllocResult`, `ReadLineResult`, `NextChar`,
  `Utf8Result`) are the prelude Must cannot write yet: ordinary declarations minted per file
  from one table, user-shadowable and never duplicate-flagged; the table's order is the
  variant index. It goes away when modules land.
- **T07** Mutability. `let mut` declares a mutable binding; assignment is a statement; local
  mutation inside a const context is fine; `mut` parameters are local copies; an assignment
  the checker rejects traps rather than proceeding. Field assignment is legal exactly when the
  root binding is `mut`, and a deref is a new root whose legality is the pointer's mutability.
- **T08** Copyability is a builtin structural judgment, never a user trait; there is no
  `Copy` trait. Scalars, records of copyable fields and variants are copyable. Heap-backed
  and linear types are ruled noncopyable: assignment and passing move, duplication is
  explicit. So is every rigid type parameter, bound or not — a capability bound grants
  discard, never duplication (TR11). Noncopyable-plus-moves does not foreclose implicit
  copying later (copy-on-write is an optimization of copy); implicit copying would foreclose
  the move guarantees.
- **T10** Joins resolve at statement boundaries, function return included; nested joins
  flatten to one, and blame treats the nest as one statement.
- **T11** Inference groups use bidirected edges, not pure SCCs, because higher-order functions
  need their call sites in the same group to constrain type variables. Generic and fully-typed
  items are firewall items, checked once against their own contract and never against
  instantiations, so they never join a group. There is no cross-file inference.
- **T12** An unconstrained join resolves by a family-aware plurality vote over concrete branch
  types: unresolved branches abstain, all-free ties tie together, a family tie recovers with
  the first witness.
- **T13** All generic positions are invariant; no subtyping anywhere.
- **T14** An unannotated fn body that diverges concludes `!` — the body's own conclusion, the
  same way `let x = panic(..)` makes `x: !`. A written slot the literal is checked against
  wins: `let f: fn() -> usize = fn { panic(..) }` is `fn() -> usize`, the body's `!` coercing
  at the tail. `Never` coerces to anything on the actual side, but an item's own so-concluded
  signature is as fixed as any other (T13: `fn() -> T` is invariant, so `!` in return position
  never widens to something else on a later use's say-so).
- **T05** The expectation register, exhaustive because two rules depend on it (the `::` sigil,
  G25, and the variant→enum conversion, which happens at a check):

  | position | expectation |
  |---|---|
  | an annotation — `let`, parameter, item | the written type |
  | a written return type | the written type |
  | a `return` expression | the enclosing literal's return type |
  | a call argument | the callee's parameter type |
  | an argument at a trait member's `Self` position | none — checked once `Self` is known |
  | a record-literal field | the field's declared type |
  | an array element | the expected element type |
  | an assignment's RHS | the assigned place's type |
  | a `Newtype` constructor's argument | the declared underlying type |
  | a const generic argument | the declared const parameter's type |
  | the right operand of `==`/`!=` | the left operand's type |
  | an arithmetic or comparison operand | a shared number variable |
  | an `if` condition | `bool` |
  | the then-branch of an `if` with no `else` | `()` |
  | an array-repeat count, an index | `usize` |
  | a `let` destructured by a `Newtype` pattern | the pattern's type |
  | an unannotated `fn` literal's parameters and return | the position's, taken apart |
  | a block's tail, a `const` or `unsafe` block's body | the enclosing expectation, unchanged |
  | a join leaf (an `if`/`match` branch, a `break` value) | none; see the Re-evaluate entry |
  | a scrutinee, a callee, an expression statement, an unannotated `let` | none |
  | a receiver — of a field, an index, a deref, a borrow or `&raw` | none |

  A position missing from this table is a bug in the table.
- **T06** An unannotated `fn` literal takes its signature from its position. The literal's
  signature — fresh variables wherever it wrote nothing — is checked against the position's
  expectation before its parameters are destructured and before its body is inferred, so an
  unwritten parameter or return type is the slot's. The literal's own annotations still win,
  and a `Newtype` parameter pattern still names its own type first: both are lowered first,
  and only a still-free variable can be bound. How much is inherited is `unify`'s answer, not
  a rule of its own — arity first, then the parameters in order, the return last, stopping at
  the first disagreement — so a literal of the wrong arity inherits nothing, and one whose
  written part disagrees inherits only what was bound before that part, which is also what
  the whole-literal mismatch names. It has to happen before the body rather than by unifying
  the finished type afterwards, because the tail is checked against the return type and the
  variant→enum conversion happens at a check: with no expected return the tail types as the
  tag-free variant and the whole literal mismatches, naming a type the reader never wrote.
  A mismatch in the body names the position as its reason — nothing was written on the
  literal, so the slot that supplied the type is what there is to point at.
- **T15** `str` is a primitive; `Vec`, `String` and `Slice` are library types, and slices
  are not primitive. Interpolation is a library feature. Proof: a `String` written in Must,
  where the representation, allocation, copy, read-back and free are all ordinary code.
- **T16** `char` is a Unicode scalar value: its own primitive, never an integer alias,
  holding a codepoint that is not a surrogate. A character literal's type is definite, which
  lets a `'x'` pattern blame itself rather than re-type the scrutinee. `char` has equality
  and nothing else: ordering and arithmetic are how a program builds a value that is not a
  character. `str.next_char(i)` answers a scalar plus the next boundary's byte index, and an
  index in the middle of a codepoint panics rather than sliding, because the program has lost
  track of its own index.
- **T17** Blessing bytes into `str` is a validity claim with two halves, kept separate. That
  the pointer addresses that many readable bytes is the caller's claim, unchecked in both
  spellings, which is why both are `unsafe`; "checked" names the other half. Whether the
  bytes spell a string is answered by one spelling and asserted by the other, and a false
  assertion is detected UB naming the offset. The pointee is pinned to `u8`, because a bless
  over another element type would be a layout claim. Both are flavour-polymorphic and so not
  first-class values, and both are pure and so const-legal. The error carries no payload,
  because a payload cannot be taken away later.
- **T18** Two `str` primitives read the bless backwards. `s.len()` is the byte length, because
  bytes are what every other `str` operation counts and a character count under the shorter
  name would be a trap. `str_bytes(s, dst)` writes those bytes into storage the caller owns;
  `unsafe`, with the marker about the destination as the bless's is about the source. Both
  retire toward the str-view fork.
- **T19** Unsafety lives in the function type. `unsafe fn(usize) -> usize` is distinct from
  `fn(usize) -> usize`; calling a value of it needs an `unsafe` block, while taking, passing,
  returning and forgetting one are free. The type is the only thing that travels with a value
  through a `let`, an argument, a return or a field, so the call site always knows. Coercion
  is one-way, safe to unsafe, applied at check sites and never inferred backwards through a
  variable, the same discipline as variant→enum. It is shallow: no variance, so a formed
  `struct { go: fn() }` does not convert, while a record LITERAL does, field by field,
  because a literal's fields are checked one at a time against the annotation. It has no
  runtime consequence: the conversion moves no bits, it declines a permission. A join takes
  the LUB wherever it has a check site or a consuming call to convert at; an unannotated,
  unconsumed mixed-safety join has neither, so it is a branch disagreement asking for an
  annotation. One population gets the type without writing it: every builtin that requires the
  marker and has a first-class fn type (`dealloc_array`, `str_bytes`). An import writes it —
  the declaration is the whole contract, and `unsafe` is part of what it says (G22).
- **T20** The `only move` ceiling. A capability names something you can do with a value; a
  declaration states its ceiling, the most that can be done on the ladders the clause names.
  One ladder exists, disposal, with rungs `access` < `move` < `forget`; `forget` is the top
  and the default, so a forgettable type writes nothing. An `only` clause belongs to a `type`
  declaration: it is superset-parsed and refused, in one sentence, on `static`, `const` and
  `trait` items — imports included, which have no `= rhs` for it to trail — as on every
  generic parameter (G21). A type capped at `move` is linear: every path consumes a value of
  it exactly once. There is no destructor, no drop glue and no unwinding; the checker is the
  whole mechanism, and codegen never learns the type is linear (identical output bytes with
  and without the clause). An `only` clause ceils only the ladders it names, so when a `send`
  capability lands every existing `only move` type keeps the send-axis default.
- **T21** Containment infects; indirection does not. A record, enum or array holding a linear
  is linear, and widening a variant never changes the answer. A borrow, a raw pointer or a
  `fn` type mentioning one keeps `forget`, because the obligation stayed with the owner, which
  is what lets the allocator take a linear element type at no cost. That is a hatch: raw
  storage will hold a linear nothing tracks. It is the same hatch as writing any value through
  a raw pointer, it lives behind `unsafe`, and the alternative (refusing linear element types
  at the allocator) would make an owned collection of linears unwritable. A linear ends by
  being taken apart: destructuring hands the container's obligation to its parts, so `drop()`
  is an ordinary consuming method with zero codegen. The hole is closed at the pattern: `..`
  may not skip a linear field, `let _ =` may not swallow one, and neither may a match-arm `_`.
  A `static` cannot hold one, because a static is never destroyed, and neither can a
  `const { ... }`, whose value is copied into every evaluation; the refusal does not depend on
  whether the surrounding path completes — a const block is judged like an item initializer.
- **T22** A data-side type parameter says nothing about capabilities: no default, no opt-out,
  no opt-in. `Option::<String>` is linear because `String` is, and the enum never had to be
  told. The answer for an instance is COMPOSITIONAL rather than a walk over the instantiated
  type: each declaration is summarized once — whether its own components cost it the capability
  whatever its arguments are, and which of its parameters reach a value position of it — and
  `D::<args>` has the capability when the declaration does and every argument in a reaching
  position has it. A rigid parameter answers from its bound, which is where the recursion
  bottoms out. The summary is a least fixpoint over the declaration graph, which is finite, so
  a declaration that names itself with LARGER arguments is answered rather than bounded:
  `type Nest = enum::<T> { Cons(T, Nest::<Pair::<T>>), Nil }` is forgettable at `usize` and
  linear at a linear payload, and no nested mention is ever expanded to say so. A parameter
  that reaches no value position constrains nothing — every value position of a
  `type P = struct::<T> { v: P::<W::<T>> }` is another `P`, so a `P` holds no `T` and no
  argument makes one linear. Nothing here needs a depth limit or a termination guess, and
  nothing inhabited is refused for want of one.

## Discarded

- **Wrap on overflow**, and **a debug/release split** — the latter makes a program's meaning
  depend on its build profile. **T02**
- **Literal defaulting to a fallback integer type** — a literal's type is always something
  someone wrote or inference proved. **Literal suffixes** — not now. **T01**
- **Implicit copies with copy-on-write or refcounting** — refcount traffic plus hidden
  allocator and dealloc calls on write-after-share: a de facto runtime woven through generated
  code. **Implicit eager deep copies** — `let s2 = s;` on a megabyte string becomes a hidden
  allocation. **A `Copy` trait** — copyability stays a judgment. **T08**
- **Transparent type aliases** — two identical spellings are two types, always.
  **Nominal-to-structural coercion** — a named type and its identical record shape stay
  distinct. **Subtyping in generic positions.** **T03 T13**
- **A subtractive capability clause (`without forget`)** — said the same thing, and the
  checker did not change; the reading did. `without` names an absence against a default the
  reader must supply from memory, and there is no natural way to write "and it may not even be
  moved" as a further subtraction that reads as a lower rung. `only` names a position on a
  ladder, so it stays true as rungs are added below. **T20**
- **`linear struct { ... }`** — names a property, not a capability, and does not extend. **A
  marker element in the `with`-chain** — that brace holds impl elements; reusing it makes
  "what is in a `with` block" two questions. **A negative bound in the `type` item's `: Type`
  slot** — validation already rejects that slot. **An attribute** — there are none, and adding
  a surface for one fact is how a language grows two ways to say everything. **T20**
- **A total-ceiling reading of `only`** ("`move` and nothing else, ever") — the arrival of a
  new capability would silently remove it from every declaration already written, the
  retrofit the ladder exists to avoid. **T20**
- **A `forget` default bound on every type parameter with opt-out** — its argument was timing:
  such a bound cannot be retrofitted against an ecosystem. That only proves a needed bound
  must be installed early, and this one was not needed: containment already makes a container
  of linears linear whether or not any bound is checked. Its cost was every container written
  twice, once per payload kind, differing by a clause that changed no behaviour. **T22**
- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**
- **`char` as a raw codepoint** — UTF-8 cannot encode a surrogate, so admitting them makes
  every encoder fallible for values no text contains. **`char` as an integer alias** — an
  alias hands back arithmetic and ordering, which is how a non-character gets built. **T16**
- **A borrowed `str` representation, today** — needs a byte-range path element the aliasing
  model deliberately lacks (M11), and every `str` consumer would learn a second shape. **T17**
- **Value-position pricing for host imports** — a mention outside `unsafe` was an error, on
  the reasoning that a bound import cannot be named at the call. The reasoning survives; the
  mechanism priced the wrong event (taking a function runs nothing), named the wrong site, and
  could not reach a function handed to a body that never mentions it. **A distinguished
  "import type"** — one type constructor serves imports, unsafe builtins and user code, which
  makes the call gate a single rule. **T19**

## Re-evaluate when

- **The str-view fork: `String`, slices, or any borrowed `str`.** A bless materializes today,
  so the region discipline lives in the signature while the bytes are copied, and a view
  copied out of a borrow survives because the copy is real. The moment a `str` can be a view,
  that copy is unsound. Options: materialize (current; needs no memory-model change); a
  borrowed representation (a byte-range path element, an overlap rule, every `str` consumer
  learning a second shape, and a collision with the discarded byte-addressed interpreter
  memory); a region-carrying `str::<@a>` (not spellable, since regions on type declarations
  do not exist). The second is the likely answer. **T17 T18**
- **A capability grants duplication** — closed today; granting it later is the reversible
  relaxation. **T08 T20**
- **In-place replacement of a linear behind `.&mut`.** A linear reached only through an
  exclusive borrow can be neither moved out (a copy out of a borrow) nor written over (that
  would lose it), so `take`/`swap`/`replace` — an `Option::take`, a collection's slot — has no
  safe spelling; today it is the raw hatch or restructuring to pass ownership. **T21**
- **Linear types meet concurrency** — a linear inside a shared handle, sent across a thread,
  or stranded in a deadlocked one are combinations no API can build today. That API's ruling
  decides whether "exactly once on every path" survives threads. **T20**
- **Expectations reach join leaves.** An `if`/`match` branch or a `break` value in witness
  position is inferred against a fresh variable, so outer pressure cannot leak into a
  branch's honest type and the construct's expectation meets the join's result only after
  every leaf has been visited. Both rules that read the register stop there: the `::` sigil
  (G25) does not resolve in a leaf, and a `fn` literal in one takes nothing from the position.
  Lifting it means a second expectation channel threaded through the join solver, which fixes
  both at once and is purely additive. **T05 T12**
- **Fn-literal arguments are checked in argument order.** A literal handed to a GENERIC callee
  ahead of the argument that would pin the callee's variables sees free parameter types, so
  `apply(fn(t) { t.n }, c)` is refused when `apply` is generic in its parameter — while the
  same call is fine against a monomorphic `apply`, and fine against the generic one when a
  turbofish (`apply::<Counter, usize>(..)`) or an earlier argument has already pinned it.
  Deferring literal arguments until the others have been checked is the fix. **T06**
- **A conversion is wanted in depth** — variance. Fn-safety is the strongest covariance
  candidate, since it is the one conversion that moves no bits and only invariance refuses it.
  Judge it with the borrow subsystem's variance question (M09). **T13 T19**
- **`const fn` types** — constness and unsafety are a product, not two states of one flag: a
  second bool on the fn type, never an enum state alongside `unsafe`. The signpost is already
  in the diagnostics, which refuse a call through a value because whether it is a `const fn`
  is not known from its type. **T19**
- **Dynamic strings** force the `let s2 = s;` cost question `str` currently dodges. Staging
  rule: keep literal `str` rodata-able and let dynamic strings arrive with an explicit
  allocating conversion. **T15**
- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
- **Modules land** — the join plurality vote, `let mut` widening and per-file inference are
  observable rules that were never ruled. Per-file solving is invisible in a single-file world
  and will shape or break programs once modules exist. **T04 T11 T12**
- **Integer conversions** land when a customer names itself; `char ↔ u32` arrives fallible
  in the integer-to-char direction. **Variadic generics** stay parked; the first customer is
  interpolation. **T01 T15 T16**
