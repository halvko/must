# Platform, codegen and tooling

## Conclusions

- **P01** Three layers, and the platform owns `main`. Layer 0 is bare: no platform, no
  effects; purity is checkable by a symbol scan. Layer 1 is platform hooks: effects are
  platform imports, so a host that does not provide a hook has statically denied the
  capability, and a program declares its own hooks with `extern static` (P05). Layer 2 is
  batteries: day-to-day users write what they write today and never learn the word "platform";
  embedders replace layer 2, not the language. Const check's "no effects in const contexts" is
  the same judgment at a different boundary.
- **P02** The embeddability checklist, an acceptance test for any memory, FFI or platform
  ruling: no process-global mutable state, many instances per process; the host supplies the
  allocator at instantiation through one small uniform interface; deterministic teardown;
  errors cross a registered boundary and never unwind host frames; no raw pointers cross the
  boundary; no ambient authority. Items 1 and 3 rule out tracing GC and pervasive refcounting;
  item 4 is why traps map to a panic hook.
- **P05** Host imports: the declaration is the whole contract (G22). The item's name is the
  import's field name, the module is `must` (`print`'s sibling), and the annotation is the one
  machine signature the host must provide. There is no symbol-override surface: config here
  would be a second place for the truth to live. Calling an `unsafe fn`-typed import requires
  `unsafe` wherever the call is, for the same reason a raw deref does — what it does is
  written in a language this compiler never sees; a `fn`-typed import costs nothing at the
  call, vouched for on the declaration instead. Taking one is free either way; a call through
  a binding is gated by the value's type exactly like a direct one, since the price rides the
  type, never the declaration (T19). The compiler validates no import signature, since a
  compiler that did would have to know every host, which is the coupling `extern` exists to
  avoid; each host judges the full declaration at the call and refuses by name — the call
  price included, exactly like a parameter or return type, so a `fn`-typed vouch for a host
  primitive that only makes sense as `unsafe fn` is refused the same way any other mismatched
  signature is. The constant carries the declared signature rather than a host re-deriving it
  from argument values, because no
  argument value can carry a pointee type: a value-inspecting host would fill a boolean array
  with bytes and mint values the type system says cannot exist. Names the compiler already
  imports are reserved, and the reserved set is the module's own import list, so a new builtin
  import reserves itself. Claiming a reserved name is rejected, not "unsupported": two imports
  of one (module, field) is a module an engine resolves twice, a silent-wrong-answer class.
- **P06** The wasm backend. A compiled module's entire host dependency is one builtin import,
  plus the imports the program itself declares (P05): no allocator, no GC, no unwinder, no
  scheduler, no support library, no start function, no runtime initialization; statics are
  const-evaluated at compile time and baked in. The encoder is hand-rolled with no runtime
  dependencies. Monomorphization happens at codegen, licensed by X11, and a differential
  harness makes the law executable: every supported example and targeted programs run under
  the interpreter and a real engine, asserting identical output, termination kind, trap reason
  and decoded value. Dictionaries resolve away completely; overflow checks are emitted at
  every width; records and enums get an internal, unspecified layout. Unsupported constructs
  refuse by name, pointing at the source; the backend must never miscompile silently.
  Monomorphization's own limits are refusals too: a body that recurs at an ever-new
  instantiation of itself is named after a fixed number of re-entries counted across the whole
  cycle (polymorphic recursion), and a call chain too deep to walk safely — even one with
  nothing recursive in it — is refused by depth alone. Both are refused by name before the
  walk can exhaust the stack budget `compile` documents (8 MiB), which its callers are
  contracted to provide.
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
  is written in that case). Warnings never affect the exit code. A diagnostic line carries a
  fixed prefix word per kind — `error:`, `warning:`, `panicked:`, `runtime error:`,
  `undefined behavior:`, `usage:` — each naming something wrong with the program or the
  invocation, and a line that reports nothing wrong carries none, so a prefixed line always
  means something is wrong. Unprefixed accordingly: `run`'s one-time note to stderr on a real
  terminal, `reading from stdin — end input with Ctrl-D`, written before the first stdin read
  blocks. Piped or redirected stdin never sees it. On the `read_line` path the note follows
  that read's own flush (P03), so it cannot overtake a program's own unterminated prompt; the
  `read(buf, len)` primitive flushes nothing first, so there a prompt can still trail the
  note. The frame limit is 10,000, and the message quotes the number.
- **P11** The debugger runs in-process on the const-eval interpreter (X08): same MIR, same
  machine, same UB findings.
- **P12** `match` completions. Two things keyed off the scrutinee: an
  arm-list template that writes the rest of the statement (every variant,
  payload bindings and arm bodies as tab stops in definition order), and
  scrutinee ranking that re-orders the expression set so enum-typed values
  lead, ordered by definition-scope distance (scope-chain hop count, not a
  hand-written tier list, so closures inherit the rule). Nothing is
  suppressed. The template lives on an explicit invoke plus a quick fix on
  the non-exhaustive-match diagnostic, not on a trigger character. Snippets
  are indented absolutely, because the editor's snippet path shifts by a
  tree-sitter result and the extension registers no grammar.
- **P03** `print` emits exactly what it is given: `str` only, no newline, no formatting, no
  interpolation. The CLI runner writes to `stdout.lock()` — Rust's own line buffering, no
  per-call flush — and flushes it explicitly before a crash report and before a blocking
  stdin read, so a program's output precedes both the report it led to and the prompt it is
  waiting on; the DAP console forwards each write to the client as its own event, since
  waiting for a newline that may never come would withhold output indefinitely.
- **P04** stdin is a library. Buffering, line boundaries, CRLF stripping, blank-line versus
  end-of-input, compacting a partial line, and handing a line out without copying are ordinary
  Must code over one byte-moving import (P05) plus the blesses (T17) and the heap builtins —
  see `examples/stdin_lib.must`; the `read_line` builtin is the convenience, not the
  mechanism. Short reads are real and are not end of input; an I/O error rides the return
  value so a lifting wrapper's error arm is reachable; the destination range is judged before
  the read, so a trap never eats input. `read_line()` still returns the compiler-provided
  per-file enum `ReadLineResult = enum { Line(str), End }`, minted the way `AllocResult` is
  and shadowable by a file's own declaration of the name. The line rule — strip the
  terminator (`\n`, and a preceding `\r`, so CRLF reads as LF), a blank line is `Line("")`, a
  final unterminated line is still a `Line`, `End` is genuine end of input — is the machine's
  own, applied to whatever bytes a host hands back, so every host obeys it rather than
  restating it. A failed read crashes the program; the enum carries no error arm. A host with
  no stdin hands out `End` rather than blocking or inventing input — the debug adapter and
  the editor's run lens both do. Refused in const contexts by the same judgment as `print`.
  The wasm backend refuses `read_line` by name (P06).
- **P10** Semantic tokens are served full-file from the server and bound to the parser: one
  keyword table generates the set, the highlighter enumerates no kinds, and a drift guard
  walks the whole syntax-kind enum. The legend grows by appending, so existing indices never
  move.
- **P15** A bound is a completion source. BOUND-directed dot-candidate enumeration has one
  home, and the completion view asks resolution's selection rule rather than restating it, so
  what `w.` offers follows what `w.push(x)` resolves to. A name carried by two bounds is
  decided by name alone: receiver shape never narrows a bound-directed ambiguity, even when
  only one candidate is shape-viable, so the call is a permanent refusal and completion
  suppresses the name rather than offering a merged or shape-picked row. Two recorded
  divergences from what a call there would resolve to: that suppression, and a nested body
  still being offered its enclosing bounds.
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
- **`{` as a completion trigger character** — fired the template at the least wanted moment;
  replaced by the quick fix on the non-exhaustive-match diagnostic. **P12**
- **`print` as an ordinary declared import** — `str`'s (offset, length) pair is a platform ABI
  this backend decided by accident, and no user-written signature can spell it today.
  **P03 P05**
- **A `Mode`-level terminal hook** — eval's `Mode` stays terminal-ignorant across all of its
  constructions; the hint belongs to the CLI's own wrapper under `RunMode::input`. **A `note:`
  prefix on the hint** — `= note:` already means a sub-line of a failure report. **P09**

## Re-evaluate when

- **P07** The module is wasm32 while `usize` stays 64-bit. Sound only while the compiled
  subset has no pointers. Rule it when pointers reach wasm: target-parameterized `usize`, or
  fixed 64 with a separate address width.
- **Closures land** — the backend needs a table, indirect calls and a real calling
  convention, and "dictionaries resolve away completely" ends. **P06**
- **Name-only bound ambiguity bites in practice** — shape narrowing (picking the one bound
  whose signature the receiver can actually take) is the named relaxation, on the condition
  that the refusal keeps naming both traits and their spellings. **P15**
- **An LIR is built** — the wasm backend's re-derived type arguments get fixed there, and
  guaranteed optimizations live there instead of being hoped for. **P08**
- **FFI is designed** — it owns the `extern` surface (data imports are reserved there), symbol
  mangling, export units, and whether Must can claim no-alias equivalents on a native backend.
  **P05**
- **The `str` platform ABI gets a second customer** — decide it on purpose before anything
  else depends on it. **P06**
- **`RunMode::read` flushes before it blocks too** — the stdin hint's ordering guarantee
  holds only on the `read_line` path today, so the note can precede a newline-less prompt
  written before a `read(buf, len)` call. **P09 P03**
- **The editor extension gains a tree-sitter grammar** — the client then
  re-indents multi-line snippet bodies and the template's absolute indentation
  doubles. One function to fix; recorded because nobody would connect the
  trigger to completions. **P12**
- **Completion layers designed, not built**: values that yield an enum one
  step deep (hierarchy and perf unresolved), and importable enums, moot
  until modules exist. Streaming is unavailable; the protocol's only
  mechanism is marking a list incomplete so the client re-queries. **P12**
- **Two open debug-adapter bugs**, not decisions: with loops and unfueled run mode an infinite
  loop hangs the session with no interrupt path; and breakpoint arrivals are deduped by
  frame/line/column, so a breakpoint in a loop body fires once per frame. **P11**
- **A checked build profile** — a codegen-era option with a measured cost:
  generational-reference checking at 2–10.84% overhead; Must's typed abstract memory is that
  idea with an infinite-width generation. **P11**
