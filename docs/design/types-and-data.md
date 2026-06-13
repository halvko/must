# Types and data

## Conclusions

- **T09** `==`/`!=` are builtin and permitted on any type; the checker only requires the
  operands to agree.
- **T11** Inference is interprocedural: items that constrain each other form a binding
  group solved in one unification context, so a signature is never peeked at before its
  body is checked; an undetermined or contradictory member erases to `{error}`. A
  signature with no holes is a firewall: it is checked against what it declares, and `_`
  marks where inference is still asked for. There is no cross-file inference.

## Discarded

- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**

## Re-evaluate when

- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
