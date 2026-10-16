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
- **T04** A variant is a value, never a type. A variant constructor — `Shape::Point`,
  `Option::<usize>::Some(11)`, the elided `::Some(x)` — has its enum's type, and a
  payload-carrying one is the constructor function `fn(payload...) -> Enum`; every enum value
  carries its tag from construction. `Enum::Variant` in type position is an error that names
  the enum to write instead, and a `let` pattern naming one variant of a multi-variant enum is
  refutable. A variant's precision is flow narrowing's job (halvko/must#125).
- **T23** The compiler-provided enums (`AllocResult`, `ReadLineResult`, `NextChar`,
  `Utf8Result`) are the prelude Must cannot write yet: ordinary declarations in the one prelude
  file, minted from one table, user-shadowable and never duplicate-flagged; the table's order is the
  variant index. It goes away when modules land.
- **T07** Mutability. `let mut` declares a mutable binding, with or without an initializer
  (`let mut x;` starts uninitialized, M21); its type is the annotation's or the assignments';
  assignment is a statement; local
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
  instantiations, so they never join a group. A fully-typed non-generic item (or member of a
  non-generic type, or of a non-generic trait's impl) is still a constraint source: its body
  feeds the group of the unannotated items it references, and fills only holes the members
  left open; its own signature stays as written. Everything one such body references joins one
  group. A group checks each of its bodies once, in a table of its own, against its own copy of
  the signature of each member it mentions; that check is the body's result, diagnostics
  included. Information flows from callee to caller: members that call each other are decided
  together by their own bodies (what each writes of its type is final in every body), and a caller is checked after its callees, against what their
  bodies decided, so a use that contradicts a determined part is reported at the use. A part a
  callee left open is filled by the members' uses, all of them together once every member is
  decided. The fully typed bodies are checked only then, against what the members' uses decided,
  as a caller is against its callee's body: a use of theirs that contradicts it is a type
  mismatch in that body, and they fill, voting among themselves the same way, only what the
  members' uses left open. A stage's decision is final for the later ones, so one unannotated
  caller outranks any number of annotated ones. In a cycle, a use that contradicts what the
  member's own body decided loses to it. Where a stage's uses don't all agree, each member is
  decided by a plurality vote among its uses, ruled for callers and a join's branches (T12) alike
  so that one rule settles every disagreement among one stage's uses: a count of shapes does not
  prove which caller is wrong, it picks where to report. A use votes for the member's whole
  signature as that use alone makes it, weighing as many as its body's mentions of the member; a
  use that leaves part of the signature open abstains, except that a literal's unsuffixed number
  votes as any number type. One use, one vote matches T12's one branch, one vote; a use is a
  mention of the member, whatever that mention constrains. A body with many uses can outvote
  several callers with one each; the count only moves where the errors are reported, and the
  messages name the bodies, not the count. Votes count shapes; the member takes each winning vote
  as a join takes a branch (T12), so one `unsafe fn` among the winners makes the member's place
  `unsafe fn`, whatever the count. A winner whose own use contradicts the result (calling an
  `unsafe fn` as a safe fn) loses that use to the other winners. Every other use is then unified
  with the result; one that contradicts it loses, reported at the use and naming the winners that
  decided. Callers that agree on part of a signature still tie on all of it (finer diagnostics:
  halvko/must#233). A tie leaves the member `{error}`, and each use names the bodies that tie with
  the type each makes it; a use that lost to what was decided before lost to nothing, so it is one
  of them. So is a use whose winners' result the group leaves `{error}` in any part: the message
  names it and the winners, without types. Winners that a place can't take are a real conflict and
  tie the same way: below the top a safety is invariant (T13), so winners differing there don't fit,
  and neither does a result that contradicts every winner. So do uses that leave the member open and
  contradict each other. Every member votes against what was decided before any vote. Members whose
  types a body ties together (it passes a value from one to another, or one member's body uses
  another) are one group. When the callers of different members of a group conflict through that
  link, every member of the group ties, each use naming the winners of the whole group (or, when
  only the members' own bodies conflict, the members); otherwise the group's results commit. A
  member tied in only through a type everyone agrees on ties with the rest (halvko/must#234). A
  member whose own callers tie leaves the members linked to it open: nothing fills the part they
  share with it (halvko/must#233). Bodies have no order, so a tie is never broken by one, and the
  vote counts exact shapes (but for a literal's number), not T12's families (whose arguments come
  from the first branch): the result never depends on the order of the items.
  No body's own error decides a type: a variable that meets an erroneous type stays unbound
  while the group decides, and becomes `{error}` in its own body only after. A body's mentions
  of a member all vote with its one copy, though, so a mention that errors in its own body still
  counts for the type the body gives the member; the program has an error there already, and the
  count only moves where the others are reported. A variable a member's body met an error on that is
  still open in what the member produces (its value, or what its functions return) after the
  members' uses becomes `{error}`, so no fully typed body decides what the body got wrong;
  every signature sharing the variable gets `{error}` too. A variable only in a function's
  parameters, at any depth, stays open for uses. A member whose own body went wrong where it
  produces a value carries the error itself: its uses report nothing. A member's own body
  finally sees what its uses filled in, a defining use for its literals. The firewall covers
  the item's signature, not its body: editing a fully typed body re-infers the groups it feeds,
  and can change its callees' signatures. A generic body stays a hard firewall: its rigid
  params must not fill a monotype. A group spans its library's files (M05).
- **T12** An unconstrained join resolves by a family-aware plurality vote over concrete branch
  types: unresolved branches abstain, all-free ties tie together, a family tie recovers with
  the first witness. Ruled together with T11's vote among callers.
- **T13** All generic positions are invariant; no subtyping anywhere.
- **T14** An unannotated fn body that diverges concludes `!` — the body's own conclusion, the
  same way `let x = panic(..)` makes `x: !`. A written slot the literal is checked against
  wins: `let f: fn() -> usize = fn { panic(..) }` is `fn() -> usize`, the body's `!` coercing
  at the tail. `Never` coerces to anything on the actual side, but an item's own so-concluded
  signature is as fixed as any other (T13: `fn() -> T` is invariant, so `!` in return position
  never widens to something else on a later use's say-so).
- **T05** The expectation register, exhaustive because two rules depend on it (the `::` sigil,
  G25, and the safe→`unsafe` fn conversion, which happens at a check):

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
  | a scrutinee, a `let` variant pattern's initializer, a callee, an expression statement, an unannotated `let` | none |
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
  the finished type afterwards, because the tail is checked against the return type: with no
  expected return an elided `::Some(t)` in the tail has no enum to resolve against.
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
  over another element type would be a layout claim. Both are ordinary first-class `unsafe fn`
  values over the raw pointer type (M23), and both are pure and so const-legal. The error
  carries no payload, because a payload cannot be taken away later.
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
  variable. While a function's safety is unknown it is a variable, solved like the rest of the
  type: unifying two fn types makes their safeties equal, every check site relates them by safe
  `<=` unsafe (a free position gets a safety of its own), a call outside `unsafe { }` makes its
  callee safe once the body's joins have solved, and one nothing forces to unsafe is safe. So an
  `unsafe fn` position never decides that the value going into it is unsafe, and the order of a
  body's uses doesn't matter (#168). A relation still open when its body is solved becomes an
  equality, since a later body in the same group sees the variables but not the direction
  between them. It is shallow: no variance, so a formed
  `struct { go: fn() }` does not convert, while a record LITERAL does, field by field,
  because a literal's fields are checked one at a time against the annotation. It has no
  runtime consequence: the conversion moves no bits, it declines a permission. A join takes
  the LUB: a safe branch converts into it, so a mixed-safety join is an `unsafe fn`. One
  population gets the type without writing it: every builtin that requires the
  marker and has a first-class fn type (`dealloc_array`, `str_bytes`). An `extern` item writes it
  when its signature calls for one — the declaration is the whole contract, and the type says
  exactly what is true of it, `unsafe` included only where the call really costs something
  (G22).
- **T20** The `only move` ceiling. A capability names something you can do with a value; a
  declaration states its ceiling, the most that can be done on the ladders the clause names.
  One ladder exists, disposal, with rungs `access` < `move` < `forget`; `forget` is the top
  and the default, so a forgettable type writes nothing. An `only` clause belongs to a `type`
  declaration: it is superset-parsed and refused, in one sentence, on `static`, `const` and
  `trait` items — `extern` items included, which have no `= rhs` for it to trail — as on every
  generic parameter (G21). A type capped at `move` is linear: every path consumes a value of
  it exactly once. There is no destructor, no drop glue and no unwinding; the checker is the
  whole mechanism, and codegen never learns the type is linear (identical output bytes with
  and without the clause). An `only` clause ceils only the ladders it names, so when a `send`
  capability lands every existing `only move` type keeps the send-axis default.
- **T21** Containment infects; indirection does not. A record, enum or array holding a linear
  is linear. A borrow, a raw pointer or a
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
- **Variant types** (removed 2026-10-13) — each variant was a type of its own, tag-free at
  runtime so code that stayed on one variant never materialized the enum's tag, with a
  runtime widening conversion to the enum at check sites, join edges, an unannotated
  `let mut` or `mut` pattern binder (G27), and every receiver-like `Self` position. Its
  precision was paid for everywhere a value met an API on the enum: a constructed
  `Option::<usize>::Some(11)` was not an `Option::<usize>`, so `o.&.is_some()` on it did not
  check, and each place that had to see the enum through the variant grew its own widening
  rule. Flow narrowing (halvko/must#125) is the planned replacement: the value keeps its
  enum type, and a match or test narrows what is known about it. **T04**
- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**
- **`char` as a raw codepoint** — UTF-8 cannot encode a surrogate, so admitting them makes
  every encoder fallible for values no text contains. **`char` as an integer alias** — an
  alias hands back arithmetic and ordering, which is how a non-character gets built. **T16**
- **A borrowed `str` representation, today** — needs a byte-range path element the aliasing
  model deliberately lacks (M11), and every `str` consumer would learn a second shape. **T17**
- **Value-position pricing for `extern` items** — a mention outside `unsafe` was an error, on
  the reasoning that a bound `extern` item cannot be named at the call. The reasoning survives; the
  mechanism priced the wrong event (taking a function runs nothing), named the wrong site, and
  could not reach a function handed to a body that never mentions it. **A distinguished
  "extern type"** — one type constructor serves `extern` items, unsafe builtins and user code, which
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
- **Libraries depend on each other** — inference stops at the library boundary, which imports
  across libraries make visible. **T11**
- **Members' uses decide before fully typed bodies'** — the fully typed bodies are checked
  against what the members' uses decided, so one unannotated caller's use, which may go through
  its own holes, outranks any number of annotated ones, and those report type mismatches.
  Tallying both stages in one vote, or reversing their order (halvko/must#194), is unruled.
  **T11**
- **Linked members tie as one group** — a correct member can become `{error}` because it is
  linked to another member's conflicting callers; tying only the members that conflict is
  halvko/must#234. **T11**
- **Integer conversions** land when a customer names itself; `char ↔ u32` arrives fallible
  in the integer-to-char direction. **Variadic generics** stay parked; the first customer is
  interpolation. **T01 T15 T16**
