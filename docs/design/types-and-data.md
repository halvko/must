# Types and data

## Conclusions

- **T09** `==`/`!=` are builtin and permitted on any type; the checker only requires the
  operands to agree.

## Discarded

- **Equality on function values as a designed relation** — it fell out of a derive, not a
  decision, and is not to be relied on. **T09**

## Re-evaluate when

- **Traits gate `==`** — everything-is-comparable fell out of reusing unification, and
  narrowing it later is a breaking change. `==` on function values additionally rests on an
  identity nobody trusts. **T09**
