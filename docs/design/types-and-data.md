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
  mutability changes a type rather than only permissions.
- **T23** The compiler-provided enums (`AllocResult`) are the prelude Must cannot write
  yet: ordinary declarations minted per file, user-shadowable and never duplicate-flagged.
  They go away when modules land.
- **T07** Mutability. `let mut` declares a mutable binding; assignment is a statement; local
  mutation inside a const context is fine; `mut` parameters are local copies; an assignment
  the checker rejects traps rather than proceeding. Field assignment is legal exactly when the
  root binding is `mut`, and a deref is a new root whose legality is the pointer's mutability.
- **T08** Copyability is a builtin structural judgment, never a user trait; there is no
  `Copy` trait. Scalars, records of copyable fields and variants are copyable. Heap-backed
  types are ruled noncopyable: assignment and passing move, duplication is explicit.
  Noncopyable-plus-moves does not foreclose implicit copying later (copy-on-write is an
  optimization of copy); implicit copying would foreclose the move guarantees.
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
- **T15** `str` is a primitive; `Vec`, `String` and `Slice` are library types, and slices
  are not primitive. Interpolation is a library feature.

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
- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**

## Re-evaluate when

- **Dynamic strings** force the `let s2 = s;` cost question `str` currently dodges. Staging
  rule: keep literal `str` rodata-able and let dynamic strings arrive with an explicit
  allocating conversion. **T15**
- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
- **Modules land** — the join plurality vote, `let mut` widening and per-file inference are
  observable rules that were never ruled. Per-file solving is invisible in a single-file world
  and will shape or break programs once modules exist. **T04 T11 T12**
- **Integer conversions** land when a customer names itself. **Variadic generics** stay
  parked; the first customer is interpolation. **T01 T15**
