# Traits and generics

## Conclusions

- **TR06** Generics are item-level only: no first-class generic values, no higher-rank types,
  schemes never enter the type. Instantiation identity is applicative: the key is
  `(ItemLoc, canonical args)`; for distinctness, wrap it in a `type` declaration. Bodies check
  once against rigid parameters, with call-site blame through a generic-argument cause.
  Call-site syntax is turbofish, routed by form; type parameters are inferred and `_` is
  allowed; const parameters are never inferred, and `_` in const position is an error. The
  const-parameter domain is any concrete data type. Monomorphization is hybrid: one MIR per
  generic item, with const arguments carried on the instance.
- **TR07** A rigid parameter supports no operations at all; bounds are the only mechanism
  that re-opens them, and that is why declaration-site checking is sound.

## Discarded

- **First-class generic values / higher-rank types** — binders in an engine that is
  deliberately binder-free; a solver rewrite. **Generic items joining inference groups** — one
  shared signature variable pins them to a monotype. **Unannotated generic literals** — would
  need scheme syntax in annotation position. **TR06**

## Re-evaluate when
