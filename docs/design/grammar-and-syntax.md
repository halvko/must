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
- **G08** Deref is postfix: `p.*`, chaining with `.field` and call parens without
  parentheses, which removes the wrong spelling: `p.*.hp = 0` is the only way to write it.
  Address-of is `&raw x` / `&raw mut x`, with the type twins `&raw T` / `&raw mut T`.
- **G12** Record literals construct with `=`: `struct { x = 1 }`. Colon means has-type,
  everywhere. Shorthand `struct { x }` is `struct { x = x }`.
- **G25** A bare pattern name never reinterprets as a variant: it binds fresh with a
  shadowing warning, and `::Circle` is the variant spelling in a pattern. In expression
  position the qualified `Shape::Circle` is the spelling.

## Discarded

- **Prefix deref `*p`** — purely to make users write `(*p).x`. **Auto-deref as the escape
  from those parens** — a conversion policy with inference consequences, not a spelling.
  **G08**
- **`*const T` / `*mut T`** — Rust's pointer-syntax regret: the ergonomic spelling went to the
  type users should reach for last, and `&T`/`&mut T` were later claimed for references.
  **`&x as *const _`-style address-of** — materialises an intermediate reference asserting
  validity and alignment, instant UB for packed fields and uninitialised memory. Must has no
  reference to materialise. **G08**
- **Go-style binders on the name** — items are `name = constructor` and binders belong to the
  constructor. **Positional type-vs-const parsing** — trees would move under declaration
  edits. **G06**
- **`x: 1` record construction** — colon is has-type. **G12**
- **Silent reinterpretation of a bare pattern name as a variant** — footgun. **G25**
- **A null literal** — abstract memory has no address zero to spell. **Pointer ordering** —
  meaningless there. **G08**

## Re-evaluate when

- **Shift operators land** — turbofish needs token-splitting against `>>`. **G06**
- **Parked gaps**, none ruled: `&&`/`||`; comparison chaining (parses, then type-errors, where
  non-associativity would be clearer); loop labels; compound assignment;
  assignment-as-expression; record rest; match-arm record patterns; a line-continuation
  string escape.
