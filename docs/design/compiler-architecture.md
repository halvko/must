# Compiler architecture

## Conclusions

- **X01** Server-first. Every analysis is a demand-driven salsa query over an incremental
  source database. A query's declared inputs are the module boundary, enforced by the
  signature rather than by convention.
- **X02** The syntax tree is lossless and error-tolerant: every byte is in the tree, and a
  malformed construct recovers rather than aborting the parse.
- **X03** Superset-parse, then validate. A recognizable form parses into its real tree
  shape even where it is not legal, and is refused by a later check rather than by the
  parser, so granting it later deletes a diagnostic and moves no grammar. A future-legal
  form is RESERVED ("not supported yet"); a never-legal-but-recognizable one is CORRECTED.
  When the verdict depends on what a name resolves to, stating it is hir's job, not syntax
  validation's — a second segment's own turbofish (`Pair::first::<usize>`) is reserved on
  a member, corrected on a variant, and the split cannot live in the parser (it does not
  know which is which). Ownership in the tree must be structural (its own node), never
  positional. ERROR nodes are for genuinely malformed syntax only.
- **X04** Identity is range-free. `ItemLoc` is (file, name, disambiguator), never an arena
  index, so an edit above an item does not invalidate it. Nothing below the diagnostic boundary
  carries a range.
- **X05** An inferred `{error}` with no diagnostic behind it is an internal error, not a silent
  recovery.
- **X06** MIR is the CFG: one IR lowered from typed HIR, shared by every downstream
  consumer so they share one semantics. Lowering is total: an ill-typed body still lowers,
  with traps that borrow a diagnostic an upstream analysis already reported, so flow
  analyses run past errors. MIR is decl-keyed, erased, and not SSA.
- **X08** One interpreter serves const eval, `run` and the debugger: same MIR, same machine,
  same UB findings.
- **X09** The interpreter is an oracle, not a spec. A detected-UB stop is a property of the
  interpreter, never a guarantee of the language; compiled Must may do anything with the same
  program.
- **X10** Record, don't re-derive. Carry a fact forward rather than reconstructing it from a
  later, lossier IR. The wasm backend lives this daily: MIR carries no type arguments at all,
  so a monomorphizing backend re-derives them by unifying declared parameter types against
  concrete ones at each call site — inference done twice, and incomplete in principle. The
  same discipline decides two shapes directly rather than guessing them: a tagged-enum
  payload read is sized from the read's own destination type, never from variant order, and a
  `fn` literal's type at a call site is its real substituted signature, never a placeholder —
  both are facts the backend already has, carried forward instead of re-derived wrong.
- **X11** The specialization-soundness law, both halves together: selection and codegen are
  lifetime-erased, and every impl is always-applicable modulo lifetimes. Lifetimes reject
  programs, never choose behaviours. This licenses monomorphization at codegen, makes the
  borrow checker's whole output diagnostics, and forbids any rule that derives runtime
  behaviour from region inference.
- **X12** Strict first, relax later, over a reject-only core (ruled, not built). Relaxing is
  additive; tightening breaks code. Sugar may only recover what could have been written
  explicitly, and may never change behaviour.
- **X13** Declaration-site checking. A generic body is checked once against rigid parameters,
  never per instantiation.
- **X14** User-visible field and member order is definition order. Internal name-sorted
  canonicalization is an identity device and must never leak.
- **X15** Naming doctrine. A capability is named for what you can do with a value (`send`,
  `forget`), never for what the value is. An adjective does not extend: there is no adjective
  for "cannot be sent" anyone would guess.
- **X16** Diagnostics have no stable codes. A severity word, free text and a caret are the whole
  contract. Warnings never affect exit status.

## Discarded

- **Per-instantiation body checking** — squiggles appear in a body because a distant caller
  edited an argument; hostile to a server-first compiler and to per-body blame. **X13**
- **Generative instantiation identity** — needs call-site ids in the memo key, and arena
  indices churn under edits. **X04 X13**
- **SSA MIR** — user locals are ordinary mutable places. An SSA layer, if it comes, is a
  separate LIR below MIR. **X06**
- **Lifetime-aware impl selection** — selection becomes inference-dependent, MIR stops being
  erased, borrow check stops being a decl-level query, and selection and region inference
  become a mutual fixpoint with no termination argument. **X11**
- **Interpreter traps as language semantics** — refused in advance, so nobody reads them as
  behaviour codegen must reproduce. **X09**
- **"Stricter now, relax later" as a universal rule** — it holds for programs and inverts for
  optimizations: more UB is the enabling direction for a compiler, so removing UB later
  invalidates optimizations already shipped. **X12**

## Re-evaluate when

- **An LIR is built** — that is where re-derived type arguments get fixed and guaranteed
  optimizations live. Rule the aliasing model first: removing UB later invalidates
  optimizations already shipped. **X06 X10 X12**
- **Layout, tuples or FFI make field order observable** — MIR's order is name-sorted and
  definition order does not exist at that boundary. Fix it then. **X14**
- **Diagnostic codes get a customer** — a way to suppress a lint, or a way to reword without
  breaking anyone matching output. Message text is load-bearing today. **X16**
- **Someone will pay for lifetime-aware selection** — the four costs under Discarded are the
  price. **X11**
