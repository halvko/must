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
  ide/       editor-agnostic analysis API (diagnostics, hover, goto-def)
  must-lsp/  the LSP binary: transport + main loop only
editors/zed/ Zed extension (separate workspace; compiled to wasm by Zed)
```

Dependency rule: `syntax` knows nothing of salsa; `ide` knows nothing of
lsp-types; `must-lsp` is pure protocol plumbing.
