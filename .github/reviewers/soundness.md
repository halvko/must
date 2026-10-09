---
model: opus
---

Judge whether the change is correct, above all whether it keeps the
language sound.

- Ownership, moves, borrows and regions: can a program the checker now
  accepts read moved-out or uninitialized storage, outlive a loan, alias a
  `&mut`, or leak a value that is not `forget`? Can it reject a program it
  should accept? The rules are in `docs/design/memory-and-borrows.md` and
  `docs/main.typ`.
- HIR, inference and MIR: does the change agree with what the other
  passes assume (types fully resolved before they are compared, storage
  live where it is read, places pinned where they are borrowed)?
- Edge cases the diff does not test: empty and nested blocks, early
  `return`/`break`, `match` arms that diverge, generics, error-recovery
  trees with missing nodes.
- `unsafe` and `extern` items: is every new unsafety visible in the
  function type, as the design requires?

Give each finding a concrete program or input that shows the failure.
Leave naming, comments and wording to the other reviewers.
