# Compiler architecture

## Conclusions

- **X01** Server-first. Every analysis is a demand-driven salsa query over an incremental
  source database. A query's declared inputs are the module boundary, enforced by the
  signature rather than by convention.
- **X02** The syntax tree is lossless and error-tolerant: every byte is in the tree, and a
  malformed construct recovers rather than aborting the parse.
- **X03** Superset-parse, then validate. A recognizable form parses into its real tree
  shape even where it is not legal, and is refused by a later check rather than by the
  parser, so granting it later deletes a diagnostic and moves no grammar. ERROR nodes are
  for genuinely malformed syntax only.
- **X05** An inferred `{error}` with no diagnostic behind it is an internal error, not a silent
  recovery.
- **X06** MIR is the CFG: one IR lowered from typed HIR, shared by every downstream
  consumer so they share one semantics. Lowering is total: an ill-typed body still lowers,
  with traps that borrow a diagnostic an upstream analysis already reported, so flow
  analyses run past errors. MIR is decl-keyed, erased, and not SSA.
- **X16** Diagnostics have no stable codes. A severity word, free text and a caret are the
  whole contract.

## Discarded

- **SSA MIR** — user locals are ordinary mutable places. An SSA layer, if it comes, is a
  separate LIR below MIR. **X06**

## Re-evaluate when

- **Diagnostic codes get a customer** — a way to suppress a lint, or a way to reword without
  breaking anyone matching output. Message text is load-bearing today. **X16**
