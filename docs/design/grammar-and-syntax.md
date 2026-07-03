# Grammar and syntax

## Conclusions

- **G01** Expression-first, greedy across the brace. Everything in statement position parses
  as a full expression unless it is statement-only (`let`, `let mut`, assignment, empty `;`,
  an item) or ends in `;`. There is no block-like special case: `if c { } - 1` is one
  subtraction. A token sequence has one reading everywhere.
- **G02** `;` terminates, `,` separates (trailing comma tolerated), and a value ending in `}`
  needs no separator.
- **G24** Evaluation order is left-to-right source order everywhere.
- **G12** Record literals construct with `=`: `struct { x = 1 }`. Colon means has-type,
  everywhere. Shorthand `struct { x }` is `struct { x = x }`.

## Discarded

- **`x: 1` record construction** — colon is has-type. **G12**

## Re-evaluate when
