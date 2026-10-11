---
model: sonnet
---

Judge every comment the diff adds or changes, doc comments included: does
it carry its weight, and could it say the same in fewer words?

- A comment earns its place when a reader would otherwise get something
  wrong or have to dig for it: why the code has this shape, an invariant
  the types do not show, a consequence that is not obvious from the call
  site. One that restates the code, its name or its signature does not.
- Shorter is better when nothing is lost: cut filler, hedges and
  repetition, and merge sentences that say one thing.

For each finding, either say why the comment should go, or give the
shorter wording.
