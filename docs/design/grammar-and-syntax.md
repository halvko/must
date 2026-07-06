# Grammar and syntax

## Conclusions

- **G01** Expression-first, greedy across the brace. Everything in statement position parses
  as a full expression unless it is statement-only (`let`, `let mut`, assignment, empty `;`,
  an item) or ends in `;`. There is no block-like special case: `if c { } - 1` is one
  subtraction. A token sequence has one reading everywhere.
- **G02** `;` terminates, `,` separates (trailing comma tolerated), and a value ending in `}`
  needs no separator.
- **G24** Evaluation order is left-to-right source order everywhere.
- **G06** Turbofish everywhere. `::<...>` applies const and type arguments in every
  position; `[...]` means `Index` on a value. Each sigil names one operation. Turbofish
  is always spelled, and type-vs-const is routed by form, never by position, so trees
  stay stable under declaration edits.
- **G07** Tuple access is `t::<0>` (ruled, not built): tuple access is const application.
  `t.0` is rejected on the float token-splitting hazard.
- **G08** Deref is postfix: `p.*`, chaining with `.field` and call parens without
  parentheses, which removes the wrong spelling: `p.*.hp = 0` is the only way to write it.
  Address-of is postfix too: `x.&raw` / `x.&raw mut`, with the type twins `T.&raw` /
  `T.&raw mut` — no prefix place adaptor (deref, address-of) exists in the grammar. The
  retired prefix `&raw x` / `&raw mut x` (and the type twins) superset-parse into the same
  nodes with a targeted migration diagnostic, never a silent reinterpretation; prefix
  `&T`/`&mut T` stay the pre-existing reservation for real references. Plain `x.&` / `x.&mut`
  and `T.&` / `T.&mut`, with no `raw`, parse and are reserved for the borrow round —
  validation rejects them.
- **G12** Record literals construct with `=`: `struct { x = 1 }`, spelled out
  `struct { a: usize = 10 }`. Colon means has-type, everywhere, so a field's annotation is a
  real type and fn, pointer and array field types are first class. Shorthand `struct { x }` is
  `struct { x = x }`. The retired `name: value` spelling is a targeted error, never a silent
  reinterpretation.
- **G13** Fields and members are separate namespaces, and the SYNTAX decides which one a name
  reaches: a bare dot always reads the field, and call syntax resolves to a dot-callable
  member — inherent or trait-impl alike. A name reached by BOTH a dot-callable member and a
  same-named fn-typed field is a call-site ambiguity error naming the field escape,
  `(v.name)(...)`, and, when a trait member is the other candidate, the qualified escape
  `Trait::name(...)` — an inherent member's own qualified spelling is itself still reserved,
  so it names no second escape today. There is no collision error at declaration — the getter
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
- **Brackets everywhere** — Go's cost without Go's mitigation: explicit binders make
  expression-position instantiation mandatory, which is exactly where brackets are ambiguous;
  it also hits three other mechanisms and buys about two characters per site. **Brackets in
  type position only** — two spellings for one concept, the wart turbofish-everywhere kills.
  **G06**
- **`x: 1` record construction** — colon is has-type. **G12**
- **A hard error on field/member collisions** — non-local under two impl homes, and it
  outlaws the getter idiom. **Fall-through to a same-named field** — silent action at a
  distance: a new trait impl could otherwise silently reroute an existing field call. **G13**
- **Dot-calling a module-level fn (UFCS-style)** — only members resolve through the dot,
  so a call site can never be silently re-routed to a distant module fn; the diagnostic
  says to call it directly instead. **G13**
- **`with`, `impl`, `for`, `trait` and `requires` as contextual keywords** — the attachment
  and trait-declaration grammars need them at positions where an identifier is also legal, so
  they are full keywords like `raw` and `unsafe`; an identifier with one of those names now
  dies in a parse cascade with no reserved-word hint. **G13**
- **Silent reinterpretation of a bare pattern name as a variant** — footgun. **G25**
- **A null literal** — abstract memory has no address zero to spell. **Pointer ordering** —
  meaningless there. **G08**
- **`t[0]`** — one sigil, one operation; it may return as an ordinary `Index` impl.
  **`t.0`** — float token-splitting hazard. **G07**
- **`<T as Trait>::m`** — bare angles violate turbofish-everywhere, and it spends `as`
  while casts are undecided. **G06**

## Re-evaluate when

- **The field/member call ambiguity proves too noisy in practice** — strict-first chose a
  hard error over silent fall-through precisely because a new trait impl reaching a type from
  its own remote chain could otherwise reroute an existing field call; relaxing back to
  member-wins-plus-a-lint is the named fallback if the error annoys more than it protects.
  **G13**
- **Shift operators land** — turbofish needs token-splitting against `>>`. **Floats
  land** — the defensive float grammar stops being free. **G06 G07**
- **Tuples are built** — postfix turbofish on arbitrary expressions, construction,
  patterns, arity limits and the unit-tuple question. **G07**
- **Someone wants a raw pointer to a fn type** — unspellable postfix (`fn() -> usize.&raw`
  binds to the return type) and there is no type grouping (`(...)` is unit). Inherent to
  the design. **G08**
- **Parked gaps**, none ruled: `&&`/`||`; comparison chaining (parses, then type-errors, where
  non-associativity would be clearer); loop labels; compound assignment;
  assignment-as-expression; record rest; match-arm record patterns; a line-continuation
  string escape.
