# Grammar and syntax

## Conclusions

- **G01** Expression-first, greedy across the brace. Everything in statement position
  parses as a full expression unless it is statement-only (`let`, empty `;`, an item) or
  ends in `;`. There is no block-like special case: `if c { } - 1` is one subtraction. A
  token sequence has one reading everywhere.
- **G02** `;` terminates, `,` separates (trailing comma tolerated), and a value ending in `}`
  needs no separator.
- **G24** Evaluation order is left-to-right source order everywhere.

## Discarded

## Re-evaluate when
