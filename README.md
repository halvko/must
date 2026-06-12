# Must

A new programming language whose compiler is a *server first*: query-based,
incremental, and error-resilient (rust-analyzer's architecture, not a batch
compiler with an LSP bolted on). Design notes live in `docs/main.typ`.

## Hacking on it

You need stable Rust and [Zed](https://zed.dev).

```sh
cargo build            # builds target/debug/must-lsp
cargo test --workspace # parser snapshots, hir/inference, in-process LSP tests
```

Then install the editor extension once:

1. In Zed, run `zed: install dev extension` from the command palette and pick
   `editors/zed/` from this repo.
2. Open this repo in Zed and edit `examples/hello.must` — diagnostics, hover,
   goto-definition, and quick fixes come from your local build.
3. For syntax highlighting, enable LSP semantic tokens in your Zed
   `settings.json` (Must has no tree-sitter grammar; the server is the only
   coloring source):

   ```json
   "languages": { "Must": { "semantic_tokens": "full" } }
   ```

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

## Running programs

The same binary runs Must code (the LSP's analysis, MIR, and interpreter —
no separate toolchain):

```sh
must-lsp run examples/hello.must               # evaluates main()
must-lsp run examples/hello.must -e 'fib(20)'  # any expression in file scope
```

Programs run even when they don't typecheck: execution proceeds until it
reaches something broken, then crashes with the same message the editor
shows as a diagnostic, plus a source location. Top-level `static`s are
evaluated at compile time (`static x = 4 + 5` is an implicit `const { … }`),
so `print` inside an initializer is an error while `print` in code you run
is fine.

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
  syntax/    lexer + resilient parser + lossless rowan CST + typed AST
  base-db/   salsa database, source inputs, parse query
  hir/       item tree, body lowering, name resolution, type inference
  mir/       control-flow-graph IR, lowered totally (errors become traps)
  eval/      the MIR interpreter: const eval (salsa query) + the runner
  ide/       editor-agnostic analysis API (diagnostics, hover, goto-def)
  must-lsp/  the LSP binary: transport + main loop, plus the `run` command
editors/zed/ Zed extension (separate workspace; compiled to wasm by Zed)
```

Dependency rule: `syntax` knows nothing of salsa; `ide` knows nothing of
lsp-types; `must-lsp` is pure protocol plumbing.
