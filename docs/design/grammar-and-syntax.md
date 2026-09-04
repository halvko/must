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
- **G12** Record literals construct with `=`: `struct { x = 1 }`, spelled out
  `struct { a: usize = 10 }`. Colon means has-type, everywhere, so a field's annotation is a
  real type and fn, pointer and array field types are first class. Shorthand `struct { x }` is
  `struct { x = x }`. The retired `name: value` spelling is a targeted error, never a silent
  reinterpretation.
- **G13** Fields and members are separate namespaces, and the SYNTAX decides which one a name
  reaches: call syntax resolves to a dot-callable member first and otherwise to the field,
  a bare dot always reads the field, and `(b.len)()` is the escape that calls a fn-typed field
  even when a member shares its name. There is no collision error at declaration — the getter
  idiom is legal.
- **G14** No auto-deref, ever, and no auto-ref. Resolution never reaches through a deref, so
  an outer name disappearing can never silently re-resolve; a pointer to a type with members
  does not dot-call them, because the receiver's type must BE the member's `Self`.
- **G25** A bare pattern name never reinterprets as a variant: it binds fresh with a
  shadowing warning, and `::Circle` is the variant spelling in a pattern. In expression
  position the qualified `Shape::Circle` is the spelling.

## Discarded

- **Prefix deref `*p`** — purely to make users write `(*p).x`. **Auto-deref as the escape
  from those parens** — a conversion policy with inference consequences, not a spelling.
  **G08 G14**
- **`*const T` / `*mut T`** — Rust's pointer-syntax regret: the ergonomic spelling went to the
  type users should reach for last, and `&T`/`&mut T` were later claimed for references.
  **`&x as *const _`-style address-of** — materialises an intermediate reference asserting
  validity and alignment, instant UB for packed fields and uninitialised memory. Must has no
  reference to materialise. **G08**
- **Go-style binders on the name** — items are `name = constructor` and binders belong to the
  constructor. **Positional type-vs-const parsing** — trees would move under declaration
  edits. **G06**
- **`x: 1` record construction** — colon is has-type. **G12**
- **A hard error on field/member collisions** — non-local under two impl homes, and it
  outlaws the getter idiom. **G13**
- **Dot-calling a module-level fn (UFCS-style)** — only members resolve through the dot,
  so a call site can never be silently re-routed to a distant module fn; the diagnostic
  says to call it directly instead. **G13**
- **`with` and `impl` as contextual keywords** — the attachment grammar needs them at
  positions where an identifier is also legal, so they are full keywords like `raw` and
  `unsafe`; an identifier with one of those names now dies in a parse cascade with no
  reserved-word hint. **G13**
- **Silent reinterpretation of a bare pattern name as a variant** — footgun. **G25**
- **A null literal** — abstract memory has no address zero to spell. **Pointer ordering** —
  meaningless there. **G08**

## Re-evaluate when

- **A member shadows a fn-typed FIELD of the same name** — then `r.f`, `r.f(1)` and `(r.f)(1)`
  are three different well-typed meanings, and reordering a member's parameters can flip
  `r.f(1)` between them silently. A shadow lint is the queued mitigation; the getter idiom
  (a member over a plain data field) is the common case and stays clean. **G13**
- **Shift operators land** — turbofish needs token-splitting against `>>`. **G06**
- **Parked gaps**, none ruled: `&&`/`||`; comparison chaining (parses, then type-errors, where
  non-associativity would be clearer); loop labels; compound assignment;
  assignment-as-expression; record rest; match-arm record patterns; a line-continuation
  string escape.
