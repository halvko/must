#!/usr/bin/env node
// wasm-run.mjs — run a `must-lsp compile`d WebAssembly module from a plain
// Node process. No dependencies beyond Node itself and ./must-wasm.js.
//
// Usage:
//   node tools/wasm-run.mjs <module.wasm> [--json]
//
// What it does:
//   - wires the builtin import, `must.print(ptr: i32, len: i32)`, to
//     stdout (the bytes live in the module's own exported `memory`) — a
//     module that declares `extern static` imports of its own needs a host
//     that knows them, which this is not;
//   - calls the module's one exported function, `main`
//     (`codegen_wasm::ENTRY_EXPORT` — see crates/codegen-wasm/src/lib.rs;
//     the backend exports exactly one function, chosen at compile time
//     with `must-lsp compile -e <expression>`, default `main()`);
//   - on a clean return, prints the result: nothing for a unit (zero-slot)
//     result, otherwise the raw `i64` slot(s) the ABI returns — there is no
//     type information in a `.wasm` file alone, so this is a "RAW"
//     rendering, not a typed `Display`;
//   - on a trap, decodes *why* by reading the module's `must.traps` custom
//     section and its `trap_code`/`panic_message_*` globals — see
//     `./must-wasm.js` (the decoder shared with tools/playground.html) and
//     `encode_traps` in crates/codegen-wasm/src/lib.rs for the full ABI
//     story.
//
// `--json` prints one JSON object to stdout instead of streaming text and
// exits the same way; nothing else goes to stdout in that mode, so it's
// safe to pipe.
//
// This driver is a trimmed, dependency-free cousin of the differential
// harness (crates/codegen-wasm/tests/harness/mod.rs, wasmi-based): same
// trap-table decoding, same multi-value result handling, no interpreter
// side-by-side run.

import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { runModule } = require("./must-wasm.js");

const USAGE = "usage: node wasm-run.mjs <module.wasm> [--json]\n";

// Thrown to unwind out of `main()` with a process exit code already
// decided, without calling `process.exit()` directly: a hard exit can cut
// off a buffered stdout write that hasn't flushed yet (real on some
// platforms when stdout is a pipe), where setting `process.exitCode` and
// letting the event loop drain naturally cannot.
class CliExit extends Error {
  constructor(code) {
    super(`exit ${code}`);
    this.code = code;
  }
}

function fail(message, code) {
  process.stderr.write(`error: ${message}\n`);
  throw new CliExit(code);
}

function parseArgs(argv) {
  let json = false;
  const positional = [];
  for (const arg of argv) {
    if (arg === "--json") json = true;
    else if (arg === "-h" || arg === "--help") {
      process.stdout.write(USAGE);
      throw new CliExit(0);
    } else if (arg.startsWith("-")) {
      process.stderr.write(USAGE);
      fail(`unknown option \`${arg}\``, 2);
    } else {
      positional.push(arg);
    }
  }
  if (positional.length !== 1) {
    process.stderr.write(USAGE);
    throw new CliExit(2);
  }
  const [wasmPath] = positional;
  return { wasmPath, json };
}

async function main() {
  const { wasmPath, json } = parseArgs(process.argv.slice(2));

  let bytes;
  try {
    bytes = await readFile(wasmPath);
  } catch (err) {
    fail(`cannot read \`${wasmPath}\`: ${err.message}`, 2);
  }

  let capturedStdout = "";
  let outcome;
  try {
    outcome = await runModule(bytes, (text) => {
      capturedStdout += text;
      if (!json) process.stdout.write(text);
    });
  } catch (err) {
    fail(`cannot run \`${wasmPath}\`: ${err.message}`, 2);
  }

  if (!outcome.ok) {
    const t = outcome.trap;
    if (json) {
      process.stdout.write(
        JSON.stringify({
          ok: false,
          stdout: capturedStdout,
          trap: { kind: t.kind, message: t.message, exact: t.exact, detail: t.detail },
        }) + "\n",
      );
    } else {
      process.stderr.write(
        `trap [${t.kind}${t.exact ? ", exact" : ""}]: ${t.message}\n`,
      );
      if (t.detail) process.stderr.write(`  detail: ${t.detail}\n`);
    }
    process.exitCode = 1;
    return;
  }

  // A clean return. `result` are the raw `i64` ABI slots rendered as
  // strings — there is no type in a `.wasm` file to render them against,
  // so an empty result (unit) prints nothing, matching `must-lsp run`'s
  // own silence on `()`, and any slots print as plain numbers rather than
  // a typed `Display`.
  if (json) {
    process.stdout.write(
      JSON.stringify({ ok: true, stdout: capturedStdout, result: outcome.result }) +
        "\n",
    );
  } else if (outcome.result.length === 1) {
    process.stdout.write(`${outcome.result[0]}\n`);
  } else if (outcome.result.length > 1) {
    process.stdout.write(`[${outcome.result.join(", ")}]\n`);
  }
  process.exitCode = 0;
}

main().catch((err) => {
  if (err instanceof CliExit) {
    process.exitCode = err.code;
  } else {
    process.stderr.write(`error: ${err.stack ?? String(err)}\n`);
    process.exitCode = 2;
  }
});
