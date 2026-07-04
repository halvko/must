# Grammar and syntax

## Conclusions

- **G01** Expression-first, greedy across the brace. Everything in statement position parses
  as a full expression unless it is statement-only (`let`, `let mut`, assignment, empty `;`,
  an item) or ends in `;`. There is no block-like special case: `if c { } - 1` is one
  subtraction. A token sequence has one reading everywhere.
- **G02** `;` terminates, `,` separates (trailing comma tolerated), and a value ending in `}`
  needs no separator.
- **G24** Evaluation order is left-to-right source order everywhere.
- **G06** Turbofish: `::<...>` applies type and const arguments, on a type mention and on
  a value alike. Turbofish is always spelled, and type-vs-const is routed by form, never
  by position, so trees stay stable under declaration edits.
- **G12** Record literals construct with `=`: `struct { x = 1 }`. Colon means has-type,
  everywhere. Shorthand `struct { x }` is `struct { x = x }`.
- **G25** A bare pattern name never reinterprets as a variant: it binds fresh with a
  shadowing warning, and `::Circle` is the variant spelling in a pattern. In expression
  position the qualified `Shape::Circle` is the spelling.

## Discarded

- **Go-style binders on the name** — items are `name = constructor` and binders belong to the
  constructor. **Positional type-vs-const parsing** — trees would move under declaration
  edits. **G06**
- **`x: 1` record construction** — colon is has-type. **G12**
- **Silent reinterpretation of a bare pattern name as a variant** — footgun. **G25**

## Re-evaluate when

- **Shift operators land** — turbofish needs token-splitting against `>>`. **G06**
- **Parked gaps**, none ruled: `&&`/`||`; comparison chaining (parses, then type-errors, where
  non-associativity would be clearer); loop labels; compound assignment;
  assignment-as-expression; record rest; match-arm record patterns; a line-continuation
  string escape.
