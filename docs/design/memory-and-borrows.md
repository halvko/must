# Memory and borrows

## Conclusions

- **M01** Typed abstract memory. A pointer is `{ allocation, path }` with `Field`/`Index` path
  elements: element-granular offsets, never bytes. Allocation ids are never reused, so
  provenance exists by construction, and every UB mapping follows: use-after-free is a deref
  of a dead allocation, out-of-bounds an index past the length, a dangling local a frame pop.
  Only address-taken locals reach the allocation table. Copying a value containing a pointer
  copies the bits, so interior pointers survive a whole-value overwrite and do not follow
  copies of the container. Pointers display opaquely, never as a number. This is why byte
  layout could be deferred: v1 never observes it.
- **M02** Validity is checked at deref, not at offset creation; stricter creation-time
  rules can be added later, the reverse cannot. `add` saturates, so a one-past-the-end
  pointer is fine to hold.
- **M03** The operations, all element-counted: allocate, free, `add(p, usize)`, `copy`
  (memmove semantics), `dangling`. `unsafe` is required for a raw deref, pointer
  arithmetic, freeing and copying; taking a raw borrow, comparing pointers, `dangling` and
  allocating are safe.
- **M08** Taking a raw pointer is safe; every consuming operation on one is gated by
  `unsafe`, so safe code may create a dangling raw pointer but cannot use one. A raw
  pointer minted through a deref (`&raw mut p.*.f`) is likewise safe to create.

### Ruled, not built

- **M18** Interior mutability in v1 is an immutable handle containing a raw mutable pointer,
  with the pointer flavour governing write-through. Long term, a nominal exclusivity-exempt
  cell type.
- The acceptance test the design must pass: use-after-free after arena teardown (an outlives
  violation); double free by value copy (moves plus region close); dangling handle copies
  after growth (use-after-move); cross-allocator buffer stealing (region inequality, with the
  runtime identity assert as a backstop).

## Discarded

- **Pointer-to-integer casts** — the interpreter is a complete UB detector where Miri is
  explicitly incomplete, and integer addresses would close the moveability and provenance
  doors permanently. **A byte-addressed or flat linear interpreter memory** — address reuse
  makes use-after-free silently read new data. **M01**

## Re-evaluate when
