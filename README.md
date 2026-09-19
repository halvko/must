# Must

A new programming language whose compiler is a *server first*: query-based,
incremental, and error-resilient (rust-analyzer's architecture, not a batch
compiler with an LSP bolted on). Design notes live in `docs/main.typ`.

## Hacking on it

You need stable Rust, a C compiler (the workspace builds the tree-sitter
parser in `editors/tree-sitter-must` to test it) and [Zed](https://zed.dev).

```sh
cargo build            # builds target/debug/must-lsp
cargo test --workspace # parser snapshots, hir/inference, in-process LSP tests
```

Then install the editor extension once:

1. In Zed, run `zed: install dev extension` from the command palette and pick
   `editors/zed/` from this repo.
2. Open this repo in Zed and edit `examples/hello.must` — diagnostics, hover,
   goto-definition, quick fixes, and completions (type-directed ranking,
   member/match-arm/record-field candidates, an arm-list template for an
   arm-less `match`, and snippets for parameterful calls, `type` RHS shells,
   payload variants, and record fields when your client supports them) come
   from your local build.
3. Syntax highlighting comes from the tree-sitter grammar the extension
   registers. For the server's semantic tokens on top of it, and for the ▶
   run buttons, enable both in your Zed `settings.json` (code lenses are off
   by default in Zed):

   ```json
   "languages": { "Must": { "semantic_tokens": "combined" } },
   "code_lens": "on"
   ```

   `"combined"` layers the two: tree-sitter colors what only it classifies
   (regions, field names, punctuation) and the server refines names through
   resolution and inference (a callee is a function, a parameter is a
   parameter). `"full"` shows the server's tokens alone and leaves those
   plain; `"off"` shows tree-sitter alone.

The extension finds the server by looking for `must-lsp` on PATH first, then
falling back to `<worktree>/target/debug/must-lsp` — so opening this repo
Just Works after `cargo build`. After rebuilding the server, restart it with
`editor: restart language server` (no extension reinstall needed).

To edit `.must` files *outside* this repo, put the server on PATH:

```sh
cargo install --path crates/must-lsp
```

(Remember to re-run that after pulling changes — PATH wins over the worktree
fallback.)

### Helix

Helix has no extension system; `editors/helix/languages.toml` is the whole
integration. Append it to `~/.config/helix/languages.toml` (with `must-lsp`
on PATH, or edit both `command`s to an absolute path to
`target/debug/must-lsp`) and check `hx --health must`. Diagnostics, hover,
goto-definition, completions, code actions, and the debugger
(`:debug-start`, same templates as `.zed/debug.json`) all work. What
doesn't: the ▶ run lenses, since Helix has no code lenses. Run files from
the shell instead.

Highlighting comes from the tree-sitter grammar (Helix has no LSP semantic
tokens) and needs **Helix 25.07 or later**: the queries rely on a later
pattern winning over an earlier one on the same node, and older releases
resolve that the other way round. Build the grammar and link the queries:

```sh
hx --grammar fetch && hx --grammar build
mkdir -p ~/.config/helix/runtime/queries
ln -s "$PWD/editors/tree-sitter-must/queries" ~/.config/helix/runtime/queries/must
```

### The tree-sitter grammar

`editors/tree-sitter-must/` holds `grammar.js`, the generated parser in
`src/` (committed: both editors compile that C), and the canonical
`queries/highlights.scm`, written in Helix's scope names. Zed's copy,
`editors/zed/languages/must/highlights.scm`, is generated from it.
`crates/tree-sitter-agreement` runs under `cargo test` and fails when the
grammar cannot parse `examples/`, when a token is highlighted differently by
the query and by the server, when a keyword or a query pattern is missing
from the test corpus, when `grammar.js` was edited without regenerating, or
when Zed's copy has drifted.

After editing `grammar.js` (node and npm needed, only here):

```sh
cd editors/tree-sitter-must
npm install          # once: a local tree-sitter CLI
npm run generate     # rewrites src/, runs test/corpus, records grammar.js's hash
```

The hash is recorded only when `test/corpus/` passes; after an intended
change of tree shape, `npx tree-sitter test --update` and review the diff.

`src/scanner.c` is hand-written (nested block comments), not generated.
After editing `queries/highlights.scm`, regenerate Zed's copy with
`UPDATE_EXPECT=1 cargo test -p tree-sitter-agreement`.

Both editors build the grammar from a pinned commit (`rev` in
`editors/zed/extension.toml` and `editors/helix/languages.toml`) but read
the queries from the checkout. A commit cannot name itself, so a grammar
change lands in two steps: push the change, then bump both `rev`s to it and
rebuild (`hx --grammar fetch && hx --grammar build`; reinstall the Zed dev
extension). In between, a query may name a node the pinned parser lacks;
the symptom is an uncolored buffer and a query error in `hx --log` or the
Zed log, and `cargo test` cannot see it. When working on the grammar, build
from the checkout instead: in Helix
`source = { path = "/abs/path/editors/tree-sitter-must" }`, in Zed
`repository = "file:///abs/path/to/this/repo"` with `rev` a local commit.
Either way the editor rebuilds only when asked: `hx --grammar build`, or
reinstall the dev extension, after every `npm run generate`.

## Running programs

With code lenses enabled, every zero-parameter function gets a `▶ run`
lens in the editor: it evaluates the *current buffer* (not the saved
file) on the in-process interpreter and reports output and result in a
message. Functions with parameters need arguments, so they run through
the debugger's entry expression (F4) or the CLI instead.

The same binary runs Must code (the LSP's analysis, MIR, and interpreter —
no separate toolchain):

```sh
must-lsp run examples/hello.must                    # evaluates main()
must-lsp run examples/functions.must -e 'fib(20)'   # any expression in file scope
```

It also *compiles* Must code, to a self-contained WebAssembly module with
no runtime of any kind — no allocator, no collector, no unwinder, no
support library:

```sh
must-lsp compile examples/display.must -o display.wasm
node tools/wasm-run.mjs display.wasm
```

`tools/playground.html` is the same thing with a UI: open it directly in a
browser (`file://` works, no server) and drag the `.wasm` file onto it.

The module imports exactly one thing, the platform effect `must.print`,
and exports `main` plus its memory and its reporting globals (`trap_code`
saying which trap fired, and the panic message's offset/length). What it
does not compile yet — raw pointers, the heap builtins, safe borrows,
`read_line` and the `str` builtins — it refuses by name, with a source
location, rather than miscompiling. The backend lives in
`crates/codegen-wasm`; its differential test harness runs every
supported example under both the interpreter and a real engine and
requires byte-identical behavior. Design notes:
`docs/design/platform-codegen-and-tooling.md`.

`examples/` has a short tour beyond `hello.must` — records and named types
(`records.must`), the tag-free variant-parameter state-machine pattern
(`state_machine.must`), loops and mutability (`loops.must`), functions,
recursion, and higher-order calls (`functions.must`), compile-time
evaluation (`compile_time.must`), generics (`generics.must`), fixed-size
arrays (`arrays.must`), raw pointers (`pointers.must`), the heap built on
top of them (`heap.must`), safe borrows (`borrows.must`), members that
borrow `Self` (`reborrow.must`), matching through a borrow
(`match_projection.must`), reading standard input (`stdin.must`) and the
same thing written as a library over a declared host import
(`stdin_lib.must`), an owned, heap-backed string built the same way, over
the `forget` capability (`string_lib.must`), walking text character by
character (`chars.must`), and a generic `Option` with the member shapes
around it (`option.must`) — plus `errors.must`, an intentionally broken file
pairing each diagnostic with the exact message `must-lsp check` prints for
it.

Programs run even when they don't typecheck: execution proceeds until it
reaches something broken, then crashes with the same message the editor
shows as a diagnostic, plus a source location. Top-level `static`s are
evaluated at compile time (`static x = 4 + 5` is an implicit `const { … }`),
so `print` inside an initializer is an error, while `print` in the `-e`
expression (or code it calls) is fine. Captures aren't supported by the
runner yet: a program that captures traps with that diagnostic.

### Debugging in Zed

The extension registers a debug adapter (the same binary in `dap` mode —
the adapter *is* the interpreter, nothing to attach to). After
(re)installing the dev extension, hit F4 (`debugger: start`) and pick a
scenario from `.zed/debug.json`: `program` is the file, `entry` is the
expression to evaluate (default `main()`), `"stopOnEntry": true` pauses at
the first line.

Breakpoints, stepping (over/in/out), the call stack, and a Locals panel
all work; `print` output streams to the debug console, where you can also
evaluate — a bare name reads a local from the selected frame, anything
else runs as an expression against that frame's locals (it's a repl:
calls included). When a broken program reaches its error, it *stops
there* like a breakpoint, stack and locals inspectable, with the same
message the editor shows as a diagnostic; resuming ends the run.

To step *within* a line (multiple calls on one line are distinct stops),
set Zed's global granularity:

```json
"debugger": { "stepping_granularity": "statement" }
```

(The adapter also supports column breakpoints and `breakpointLocations`
per the DAP spec, but Zed's UI is line-only today — those light up in
clients with an inline-breakpoint picker.)

### Debugging the server

Set `MUST_LSP_LOG` (a `tracing` env-filter, e.g. `must_lsp=debug`) in the
shell you launch Zed from; logs go to stderr, visible via `zed --foreground`
or Zed's "open language server logs". Most issues are easier to pin down with
the in-process protocol tests in `crates/must-lsp/tests/server.rs`.

## Crate layout

```
crates/
  syntax/       lexer + resilient parser + lossless rowan CST + typed AST
  base-db/      salsa database, source inputs, parse query
  hir/          item tree, body lowering, name resolution, type inference
  mir/          control-flow-graph IR, lowered totally (errors become traps)
  eval/         the MIR interpreter: const eval (salsa query)
  ide/          editor-agnostic analysis API (diagnostics, hover, goto-def)
  codegen-wasm/ the WebAssembly backend: monomorphization + code emission
  must-lsp/     the LSP binary: transport + main loop, plus `run`, `compile` and the runner
  tree-sitter-agreement/ test-only: the tree-sitter grammar checked against syntax + ide
editors/zed/ Zed extension (separate workspace; compiled to wasm by Zed)
editors/helix/ Helix configuration (languages.toml)
editors/tree-sitter-must/ tree-sitter grammar + highlight queries, for both editors
tools/       run a compiled `.wasm` module: wasm-run.mjs (Node CLI), playground.html (browser)
```

Dependency rule: `syntax` knows nothing of salsa; `ide` knows nothing of
lsp-types; `must-lsp` is pure protocol plumbing.
