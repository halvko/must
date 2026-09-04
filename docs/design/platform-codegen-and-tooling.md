# Platform, codegen and tooling

## Conclusions

- **P01** Three layers, and the platform owns `main`. Layer 0 is bare: no platform, no
  effects; purity is checkable by a symbol scan. Layer 1 is platform hooks: effects are
  platform imports, so a host that does not provide a hook has statically denied the
  capability. Layer 2 is batteries: day-to-day users write what they write today and never
  learn the word "platform"; embedders replace layer 2, not the language. Const check's "no
  effects in const contexts" is the same judgment at a different boundary.
- **P02** The embeddability checklist, an acceptance test for any memory, FFI or platform
  ruling: no process-global mutable state, many instances per process; the host supplies the
  allocator at instantiation through one small uniform interface; deterministic teardown;
  errors cross a registered boundary and never unwind host frames; no raw pointers cross the
  boundary; no ambient authority. Items 1 and 3 rule out tracing GC and pervasive refcounting;
  item 4 is why traps map to a panic hook.
- **P09** The command surface. `run` exits 0, 1 for a trap/panic/runtime error/UB, or 2
  for a usage or file-IO failure; `check` likewise. Warnings never affect the exit code.
  Failure kinds have fixed prefix words. The frame limit is 10,000, and the message quotes
  the number.
- **P11** The debugger runs in-process on the const-eval interpreter (X08): same MIR, same
  machine, same UB findings.
- **P03** `print` emits exactly what it is given: `str` only, no newline, no formatting, no
  interpolation. The CLI runner writes to `stdout.lock()` — Rust's own line buffering, no
  per-call flush — and flushes it explicitly only before a crash report, so a program's
  output still precedes the report it led to; the DAP console forwards each write to the
  client as its own event, since waiting for a newline that may never come would withhold
  output indefinitely.
- **P10** Semantic tokens are served full-file from the server and bound to the parser: one
  keyword table generates the set, the highlighter enumerates no kinds, and a drift guard
  walks the whole syntax-kind enum. The legend grows by appending, so existing indices never
  move.
- **P14** Roadmap: AoC puzzles, then a small embedded OS, then self-hosting. The embedded stage
  makes bare-asm entrypoints, layer-0 purity, linker placement and volatile access scheduled
  requirements.

## Discarded

- **Tracing GC and pervasive refcounting** — fail the embeddability checklist before pause
  times or throughput come up, and independently fail no-runtime. **A runtime of any kind** —
  this is what killed algebraic effect handlers, ambient-context calling conventions, and
  unwinding. **P01 P02**
- **Unwinding through host frames** — errors cross a registered boundary; traps map to a panic
  hook. **P02**
- **A hidden context parameter on every call** — non-allocating functions pay an ABI slot.
  **P01**
- **A byte-addressed interpreter memory** — commits to layout now and removes the structured
  value representation that debugger rendering, value display and structural equality depend
  on. Nothing in the typed model has to be undone for a codegen-era byte model. **A flat
  linear memory** — address reuse makes use-after-free silently read new data, and the
  interpreter stops being a UB detector. **Handle/path fakes with no allocation table** —
  cannot represent heap allocations. **P11**

## Re-evaluate when

- **Two open debug-adapter bugs**, not decisions: with loops and unfueled run mode an infinite
  loop hangs the session with no interrupt path; and breakpoint arrivals are deduped by
  frame/line/column, so a breakpoint in a loop body fires once per frame. **P11**
- **A checked build profile** — a codegen-era option with a measured cost:
  generational-reference checking at 2–10.84% overhead; Must's typed abstract memory is that
  idea with an infinite-width generation. **P11**
