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

## Discarded

## Re-evaluate when
