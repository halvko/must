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
- **P06** The wasm backend. A compiled module's entire host dependency is one import: no
  allocator, no GC, no unwinder, no scheduler, no support library, no start function, no
  runtime initialization; statics are const-evaluated at compile time and baked in. The
  encoder is hand-rolled with no runtime dependencies. Monomorphization happens at codegen,
  licensed by X11, and a differential harness makes the law executable: every supported
  example and targeted programs run under the interpreter and a real engine, asserting
  identical output, termination kind, trap reason and decoded value. Dictionaries resolve
  away completely; overflow checks are emitted at every width; records and enums get an
  internal, unspecified layout. Unsupported constructs refuse by name, pointing at the
  source; the backend must never miscompile silently. Monomorphization's own limits are
  refusals too: a body that recurs at an ever-new instantiation of itself is named after a
  fixed number of re-entries counted across the whole cycle (polymorphic recursion), and a
  call chain too deep to walk safely — even one with nothing recursive in it — is refused by
  depth alone. Both are refused by name before the walk can exhaust the stack budget
  `compile` documents (8 MiB), which its callers are contracted to provide.
- Findings from the wasm backend that bind later work. MIR's erasure forces the backend to
  re-derive type arguments by unification, which is inference done twice and incomplete in
  principle (X10); the pre-mono LIR is where the fix belongs. MIR field order is name-sorted
  (X14). Function values being compile-time constants is the only reason dictionaries fully
  resolve; closures will force a table, indirect calls and a real calling convention. `str`
  decided a platform ABI by accident: an offset/length pair plus one generated support
  function, the first exception to "no runtime". Static identity (C01) is not implementable
  in a register-only model.
- **P08** Codegen direction: an SSA-based LIR below MIR, still pre-monomorphization, where
  inlining and optimization happen before the backend, so backends receive less garbage and
  specific optimizations can be guaranteed rather than hoped for. Direction, not commitment;
  compatible with everything sealed. What this layer may assume waits on the aliasing model,
  once ruled — X12.
- **P09** The command surface. `run` exits 0, 1 for a trap/panic/runtime error/UB, or 2
  for a usage or file-IO failure; `check` likewise. `compile` exits 0 once a module is
  written, 1 on a refusal (an unsupported construct, named and located), 2 on a usage or
  file-IO failure, and `check`'s own exit code when the file does not check clean (nothing
  is written in that case). Warnings never affect the exit code. Failure kinds have fixed
  prefix words. The frame limit is 10,000, and the message quotes the number.
- **P11** The debugger runs in-process on the const-eval interpreter (X08): same MIR, same
  machine, same UB findings.
- **P12** `match` completions. Two things keyed off the scrutinee: an
  arm-list template that writes the rest of the statement (every variant,
  payload bindings and arm bodies as tab stops in definition order), and
  scrutinee ranking that re-orders the expression set so enum-typed values
  lead, ordered by definition-scope distance (scope-chain hop count, not a
  hand-written tier list, so closures inherit the rule). Nothing is
  suppressed. The template lives on an explicit invoke, not on a trigger
  character. Snippets are indented absolutely, because the editor's
  snippet path shifts by a tree-sitter result and the extension registers
  no grammar.
- **P03** `print` emits exactly what it is given: `str` only, no newline, no formatting, no
  interpolation. The CLI runner writes to `stdout.lock()` — Rust's own line buffering, no
  per-call flush — and flushes it explicitly before a crash report and before a blocking
  stdin read, so a program's output precedes both the report it led to and the prompt it is
  waiting on; the DAP console forwards each write to the client as its own event, since
  waiting for a newline that may never come would withhold output indefinitely.
- **P04** `read_line()` is a layer-1 platform hook (P01), `print`'s input twin: a nullary
  builtin returning the compiler-provided per-file enum `ReadLineResult = enum { Line(str),
  End }`, minted the way `AllocResult` is, so it is an ordinary nominal enum and a file's own
  declaration of the name shadows it. One call is one line with its terminator stripped
  (`\n`, and a preceding `\r`, so CRLF input reads as LF input); a blank line is `Line("")`;
  a final unterminated line is still a `Line`; `End` is genuine end of input, never "nothing
  available yet". A failed read — input that is not UTF-8 included — crashes the program
  instead; the enum carries no error arm. That line rule is the machine's own, applied to
  whatever bytes a host hands back, so every host obeys it rather than restating it. Refused
  in const contexts by the same judgment as `print`. A host with no stdin hands out `End`
  rather than blocking or inventing input — the debug adapter and the editor's run lens both
  do. The wasm backend refuses it by name (P06).
- **P10** Semantic tokens are served full-file from the server and bound to the parser: one
  keyword table generates the set, the highlighter enumerates no kinds, and a drift guard
  walks the whole syntax-kind enum. The legend grows by appending, so existing indices never
  move.
- **P13** The examples smoke target checks and runs every example against recorded snapshots
  under a timeout, so a hang fails naming the file, and a coverage guard fails if an example
  ships untested.
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
- **`{` as a completion trigger character** — fired the template at the least wanted moment,
  an opening function body chief among them; the template stays on explicit invoke. **P12**

## Re-evaluate when

- **P07** The module is wasm32 while `usize` stays 64-bit. Sound only while the compiled
  subset has no pointers. Rule it when pointers reach wasm: target-parameterized `usize`, or
  fixed 64 with a separate address width.
- **Closures land** — the backend needs a table, indirect calls and a real calling
  convention, and "dictionaries resolve away completely" ends. **P06**
- **An LIR is built** — the wasm backend's re-derived type arguments get fixed there, and
  guaranteed optimizations live there instead of being hoped for. **P08**
- **The `str` platform ABI gets a second customer** — decide it on purpose before anything
  else depends on it. **P06**
- **The editor extension gains a tree-sitter grammar** — the client then
  re-indents multi-line snippet bodies and the template's absolute indentation
  doubles. One function to fix; recorded because nobody would connect the
  trigger to completions. **P12**
- **Completion layers designed, not built**: values that yield an enum one
  step deep (hierarchy and perf unresolved), and importable enums, moot
  until modules exist. Streaming is unavailable; the protocol's only
  mechanism is marking a list incomplete so the client re-queries. **P12**
- **A host-import surface lands** — whether line delimiting, CRLF stripping and telling a
  blank line from end of input belong in the compiler at all, or in Must code over a
  byte-moving import with `read_line` kept as the convenience. **P04**
- **Two open debug-adapter bugs**, not decisions: with loops and unfueled run mode an infinite
  loop hangs the session with no interrupt path; and breakpoint arrivals are deduped by
  frame/line/column, so a breakpoint in a loop body fires once per frame. **P11**
- **A checked build profile** — a codegen-era option with a measured cost:
  generational-reference checking at 2–10.84% overhead; Must's typed abstract memory is that
  idea with an infinite-width generation. **P11**
