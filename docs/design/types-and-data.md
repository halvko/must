# Types and data

## Conclusions

- **T09** `==`/`!=` are builtin in v1 and permitted on any type; the checker only requires
  the operands to agree. Structurally defined for every value shape; records compare over
  canonically sorted fields.
- **T03** `type Foo =` always mints. No transparent aliases; two identical spellings are
  two types. Records are exact structural types; a structural record has no declaration,
  so it has no owner, and the newtype line is what gives it one. Nominal and structural
  never coerce.
- **T04** Variant widening is a runtime conversion over a tag-free variant representation: a
  variant-typed value carries no tag, and widening adds one. A plain `let` keeps the precise
  variant; an unannotated `let mut` widens to the enum at binding time, the one place
  mutability changes a type rather than only permissions.
- **T07** Mutability. `let mut` declares a mutable binding; assignment is a statement;
  local mutation inside a const context is fine; `mut` parameters are local copies; an
  assignment the checker rejects traps rather than proceeding. Field assignment is legal
  exactly when the root binding is `mut`; there is no per-field `mut`.
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

## Discarded

- **Transparent type aliases** — two identical spellings are two types, always.
  **Nominal-to-structural coercion** — a named type and its identical record shape stay
  distinct. **Subtyping in generic positions.** **T03 T13**
- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**

## Re-evaluate when

- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
- **Modules land** — the join plurality vote, `let mut` widening and per-file inference are
  observable rules that were never ruled. Per-file solving is invisible in a single-file world
  and will shape or break programs once modules exist. **T04 T11 T12**
