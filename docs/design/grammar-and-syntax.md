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
  nodes with a targeted migration diagnostic, never a silent reinterpretation, and the
  retired prefix safe borrows `&x`/`&mut x`, `&T`/`&mut T` migrate the same way (G26).
  Plain `x.&` / `x.&mut` and the type twins `T.&` / `T.&mut`, with no `raw`, are the SAFE
  borrows, with a region turbofish of their own (`x.&mut::<@a>`) — optional on the borrow
  expression (inferred when omitted) but required on the type form `T.&`/`T.&mut`
  everywhere, since no elision exists yet for a hand-written type.
- **G09** The region sigil is `@`: `@a`, wildcard `@_`, join `@a + @b`, binder slot
  `fn::<@a, T, const N>`, outlives clause `@a: @b`. Chosen because `@` is unclaimed; migrating
  it would be mechanical.
- **G12** Record literals construct with `=`: `struct { x = 1 }`, spelled out
  `struct { a: usize = 10 }`. Colon means has-type, everywhere, so a field's annotation is a
  real type and fn, pointer and array field types are first class. Shorthand `struct { x }` is
  `struct { x = x }`. The retired `name: value` spelling is a targeted error, never a silent
  reinterpretation.
- **G13** Fields and members are separate namespaces, and the SYNTAX decides which one a name
  reaches: a bare dot always reads the field, and call syntax resolves to a dot-callable
  member — inherent or trait-impl alike. A name carried by MORE THAN ONE carrier (an inherent
  member, each trait-impl member, an fn-typed field, in any combination) is one call-site
  ambiguity error naming every candidate's exact spelling, with a related location per
  candidate — no fall-through: a trait impl may live in the trait's own chain, nowhere near
  the type, so silent shadowing either way would be action at a distance. Three escapes exist:
  the field's `(v.name)(...)`; the full named-Self form `Trait::<Self = Type>::member(...)`
  for a trait member (short forms stay available when unambiguous); `Type::member(value)` for
  an inherent member. `Type::m` where `m` names a member a trait provides for `Type` (not
  `Type`'s own) is its own `QualifiedTraitMemberOnType` diagnostic, naming the trait spelling
  it should have used instead. There is no collision error at declaration — the getter idiom
  is legal.
- **G17** String escapes are `\n \t \r \\ \" \0`; anything else after a backslash, and a
  trailing lone backslash, is an error anchored at the escape inside the token. Strings stay
  multiline — a literal newline inside a string is still legal. The lexer and the decoder
  share one table (`syntax::unescape_char`), so they can never disagree about what is an
  escape. `\u{...}` and `\xNN` are answered "not supported yet" rather than "unknown", in
  both literal forms from that one table: the design holds room for them, so calling either
  unknown would send the reader hunting for a spelling that is already spoken for.
- **G16** Full keywords: `raw unsafe with impl for trait requires extern without` (`const`,
  `struct`, `enum` are contextual expression-starters). One `keywords!` table generates the
  set — `from_keyword`, `is_keyword`, and the table itself — so the highlighter (P10) and
  completions classify a keyword by asking, never by enumerating kinds.
- **G15** `return` is an expression of type `Never`, constrained through the same seam tail
  expressions use (an annotated return type blames the operand and cites the annotation; an
  inferred one is pinned by `return e` exactly as by a tail). It targets the nearest enclosing
  `fn` literal; `return` inside a `const { }` block is reserved: it should bail from the outer
  fn body, which needs cross-body machinery (a `fn` literal nested in the block is its own
  body); `return` at an item initializer's own top level is an error. A block with no tail,
  one of whose EXPRESSION statements diverges (any of them, not only the last), is itself `!`
  (not `()`), so `else { return 0; }` and `else { break; }` — with the semicolon everyone
  writes — type-check. A `let` initializer that diverges is not counted yet — `let x =
  return 1;` types the block off the binding, a step toward reachability analysis this rule
  isn't.
- **G14** No auto-deref, ever, and no auto-ref, with one bounded exception. Resolution never
  reaches through a deref, so an outer name disappearing can never silently re-resolve. The
  exception: the compiler may insert a safe borrow of `x.*` where `x` is already a borrow;
  never a borrow of `x` itself; never a raw borrow. The first clause licenses implicit
  reborrow and degradation (M07); the second separates reborrow from auto-ref; the third
  stops a call site minting `x.*.&raw mut` and laundering a region (M08).
- **G25** A bare pattern name never reinterprets as a variant: it binds fresh with a
  shadowing warning, and `::Circle` is the variant spelling. The sigil also works in
  expression position as reject-only sugar: it reads the position's expected type (T05) and
  nothing else, so it resolves wherever an expectation reaches and is refused elsewhere
  with the qualified spelling named. The qualified spelling stays canonical.
- **G26** A retired spelling migrates; it never reinterprets. Prefix `&x`, `&mut x`, `&T`
  and `&mut T` superset-parse into the same node the postfix form produces, with a
  corrective diagnostic and a rewriting fix — withheld where no postfix text means the
  same thing, as for a borrow of a `fn(..) -> T`. The parser does not chase a retired
  spelling across token kinds: in `&'a T` the `&` fires its own migration and the freed
  `'` is an ordinary unterminated character literal. That recovery is diagnostics-layer
  work.
- **G18** A character literal holds one Unicode scalar value; escapes are the string set
  with the quote swapped, so only the delimiter that would end the literal needs one. The
  scan is line-bounded, unlike a string's, so a half-typed quote costs one odd token on its
  own line instead of the rest of the file. Known and accepted: two odd quotes on one line
  pair up.
- **G19** Literal patterns bind nothing; dispatch is a chain of equality tests in source
  order, first match wins. A `char` match always needs a `_` arm, as policy, not arithmetic.
  A repeated literal arm is an unreachable-arm warning keyed on the value, not on its
  rendering. Every literal kind parses into the pattern node; validation names the kinds not
  supported yet.

## Discarded

- **`const { }` bounding `return` the way it bounds `break`** — reachable (a `const` block
  is a separate MIR body) but a different construct wearing the same spelling; the exit
  should leave the outer fn. **G15**
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
  dies in a parse cascade with no reserved-word hint. **G16**
- **Silent reinterpretation of a bare pattern name as a variant** — footgun. **G25**
- **A null literal** — abstract memory has no address zero to spell. **Pointer ordering** —
  meaningless there. **G08**
- **Region sigils that lost**: `'a` (a three-way contest for `'` with char literals and loop
  labels, and it forces the char question first); `` `a `` (hover markup is markdown, so every
  hover would escape it forever); `region a` (form disambiguation forces the keyword to repeat
  in argument position); bare `a` (no form left to spell "infer this region"; identical trees
  for different kinds); `%a`, `^a`, postfix `a'`. **G09**
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
- **Someone wants a pointer or borrow to a fn type** — unspellable postfix
  (`fn() -> usize.&raw`, `fn() -> usize.&` bind to the return type) and there is no type
  grouping (`(...)` is unit). Inherent to the design; for now the retired prefix spelling
  still builds such a type, and its migration reports without offering the rewrite, since
  no postfix text means the same thing. **G08 G26**
- **Parked gaps**, none ruled: `&&`/`||`; comparison chaining (parses, then type-errors, where
  non-associativity would be clearer); loop labels; compound assignment;
  assignment-as-expression; record rest; match-arm record patterns; a line-continuation
  string escape.
