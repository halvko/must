# Types and data

## Conclusions

- **T09** `==`/`!=` are builtin in v1 and permitted on any type; the checker only requires
  the operands to agree. Structurally defined for every value shape; records compare over
  canonically sorted fields.
- **T03** `type Foo =` always mints. No transparent aliases; two identical spellings are
  two types. Records are exact structural types; a structural record has no declaration,
  so it has no owner, and the newtype line is what gives it one. Nominal and structural
  never coerce.
- **T07** Mutability. `let mut` declares a mutable binding; assignment is a statement;
  local mutation inside a const context is fine; `mut` parameters are local copies; an
  assignment the checker rejects traps rather than proceeding.
- **T11** Inference groups use bidirected edges, not pure SCCs, because higher-order
  functions need their call sites in the same group to constrain type variables.
  Fully-typed items are firewall items, checked once against their own contract and never
  joining a group; `_` marks where inference is still asked for. There is no cross-file
  inference.
- **T12** An unconstrained join resolves by a plurality vote over concrete branch types:
  unresolved branches abstain, all-free ties tie together, a tie recovers with the first
  witness.

## Discarded

- **Transparent type aliases** — two identical spellings are two types, always.
  **Nominal-to-structural coercion** — a named type and its identical record shape stay
  distinct. **T03**
- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**

## Re-evaluate when

- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
