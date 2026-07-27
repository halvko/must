// must-wasm.js — the shared engine behind tools/wasm-run.mjs (a Node CLI)
// and tools/playground.html (a browser page): compile and run a module
// `must-lsp compile` produced, decoding its trap table when it doesn't
// return cleanly. No dependencies beyond the WebAssembly and TextDecoder
// globals every JS host already has.
//
// Loaded two different ways so the decoding logic lives in exactly one
// place: `require("./must-wasm.js")` from Node (wasm-run.mjs and this
// crate's smoke tests), and a plain `<script src="must-wasm.js">` before
// playground.html's own inline script — a classic script works over
// `file://` (unlike an ES module import, which browsers block there), and
// its top-level `const`/`function` bindings are visible to the page's own
// `<script>` that follows it, the same way two classic `<script>` tags in
// one document already share a scope.

// ---- `must.traps` custom section decoding -------------------------------
// Mirrors `encode_traps` in crates/codegen-wasm/src/lib.rs: a ULEB128
// count, then per entry a kind byte, an `exact` byte, a length-prefixed
// message and a length-prefixed detail. The section exists precisely so a
// host with no access to that crate — a disassembler, or either tool here
// — can still say which trap fired.

const TRAP_KINDS = [
  "Panic",
  "Overflow",
  "DivideByZero",
  "IndexOutOfBounds",
  "Diagnostic",
  "Internal",
];

function uleb(buf, pos) {
  let result = 0n;
  let shift = 0n;
  let p = pos;
  for (;;) {
    const byte = BigInt(buf[p++]);
    result |= (byte & 0x7fn) << shift;
    if ((byte & 0x80n) === 0n) break;
    shift += 7n;
  }
  return [Number(result), p];
}

function parseTraps(mod) {
  const sections = WebAssembly.Module.customSections(mod, "must.traps");
  if (sections.length === 0) return [];
  const buf = new Uint8Array(sections[0]);
  const dec = new TextDecoder();
  let [count, pos] = uleb(buf, 0);
  const traps = [];
  for (let i = 0; i < count; i++) {
    const kind = TRAP_KINDS[buf[pos++]] ?? "?";
    const exact = buf[pos++] !== 0;
    let messageLen;
    [messageLen, pos] = uleb(buf, pos);
    const message = dec.decode(buf.slice(pos, pos + messageLen));
    pos += messageLen;
    let detailLen;
    [detailLen, pos] = uleb(buf, pos);
    const detail = dec.decode(buf.slice(pos, pos + detailLen));
    pos += detailLen;
    traps.push({ kind, exact, message, detail });
  }
  return traps;
}

// ---- the engine: compile, instantiate, run, decode ----------------------
//
// The backend exports exactly one function, `main`
// (`codegen_wasm::ENTRY_EXPORT`) — the entry expression is chosen at
// compile time with `must-lsp compile -e <expression>`, default `main()` —
// so there is no entry name to pass in here.
//
// `onPrint(text)` fires once per `must.print` call, in order. Returns a
// promise resolving to either
//   { ok: true, result: string[] }               — raw i64 slots, or
//   { ok: false, trap: { kind, message, exact, detail } }
// and rejects if the bytes aren't a valid module, if instantiation fails,
// or if the module exports no `main` function.

async function runModule(bytes, onPrint) {
  const mod = await WebAssembly.compile(bytes);
  const traps = parseTraps(mod);
  let instance;
  const imports = {
    must: {
      print(offset, len) {
        const memory = instance.exports.memory;
        const text = new TextDecoder().decode(
          new Uint8Array(memory.buffer, offset, len),
        );
        onPrint(text);
      },
    },
  };
  instance = await WebAssembly.instantiate(mod, imports);

  const entry = instance.exports.main;
  if (typeof entry !== "function") {
    throw new Error("the module exports no `main` function");
  }

  try {
    const raw = entry();
    const slots = raw === undefined ? [] : Array.isArray(raw) ? raw : [raw];
    return { ok: true, result: slots.map((s) => s.toString()) };
  } catch (err) {
    const codeGlobal = instance.exports.trap_code;
    const code = codeGlobal ? Number(codeGlobal.value) : -1;
    let trap;
    if (code < 0 || !traps[code]) {
      // No trap_code recorded, or it points nowhere: an engine-level trap
      // (realistically call-stack exhaustion — a gap the differential
      // harness states rather than closes, since the two sides can give up
      // at different depths; see crates/codegen-wasm/tests/harness/mod.rs)
      // rather than one the generated code planted deliberately.
      trap = {
        kind: "Engine",
        exact: false,
        message: "<engine trap, no trap_code recorded>",
        detail: err && err.message ? err.message : String(err),
      };
    } else {
      trap = { ...traps[code], index: code };
      if (trap.kind === "Panic" && trap.message === "") {
        const offsetGlobal = instance.exports.panic_message_offset;
        const lenGlobal = instance.exports.panic_message_len;
        if (offsetGlobal && lenGlobal) {
          const offset = Number(offsetGlobal.value);
          const len = Number(lenGlobal.value);
          trap.runtimeMessage = new TextDecoder().decode(
            new Uint8Array(instance.exports.memory.buffer, offset, len),
          );
        }
      }
    }
    return {
      ok: false,
      trap: {
        kind: trap.kind,
        message: trap.runtimeMessage ?? trap.message,
        exact: trap.exact,
        detail: trap.detail ?? "",
      },
    };
  }
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { TRAP_KINDS, uleb, parseTraps, runModule };
}
