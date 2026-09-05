//! `must-lsp compile file.must -o file.wasm`: the WebAssembly backend at
//! the command line.
//!
//! The compiled module is a *platform instance* in P01's sense — it
//! imports its effects (`must.print`) and exports `main`, and it carries
//! no runtime whatsoever. What it runs is the same MIR `must-lsp run`
//! interprets, so the two agree by construction (and by the differential
//! harness in `crates/codegen-wasm/tests`).
//!
//! Two refusals are possible, and both are loud:
//!
//! * the FILE does not check clean — a backend has no business compiling
//!   code the checker rejects, so its diagnostics are printed and nothing
//!   is written, exiting with `check`'s own exit code. This is narrower
//!   than it sounds: it covers the file's own items, not the `-e` entry
//!   expression injected on top of them. That expression runs the way
//!   `must-lsp run` runs it — as run-mode code, not a checked const
//!   context — because injecting it AS a const context (a `static`
//!   initializer) would reject the ordinary case of calling a non-`const
//!   fn` at all, and `-e` defaults to exactly that: `main()`. A bad entry
//!   expression instead surfaces as a wasm-backend refusal (a call through
//!   a function value that isn't statically known, say) or a trap in the
//!   compiled module;
//! * the program uses a construct this backend does not compile yet, or a
//!   call-graph shape it caps — the refusal names the construct and points
//!   at the source, or names the runaway chain (a capped shape has no
//!   single site). It never miscompiles silently.
//!
//! A file whose own syntax is broken badly enough to swallow the injected
//! entry item (an unterminated string, say) is caught earlier, by
//! `runner::prepare` itself rather than by a diagnostics pass — but it is
//! still the FILE'S fault, so it gets the same exit code as the first
//! bullet, not the usage-error code a malformed `-e` expression gets.

use crate::runner::PrepareError;
use base_db::RootDatabase;

/// Read, compile, write. Returns the process exit code.
///
/// The work runs on a thread of `codegen_wasm::STACK_BUDGET` because
/// `codegen_wasm::compile` walks the call graph recursively and its depth
/// caps are calibrated against exactly that budget: on the process's own
/// main stack (whatever `ulimit -s` happens to be) a call graph the
/// backend would refuse by name could overflow first, and a stack
/// overflow is an abort, not a diagnostic. The backend cannot take the
/// thread itself — `salsa::Database` is not `Sync` — so providing it is
/// the caller's half of the contract; `pool.rs` honours a contract of the
/// same shape for eval's const-forcing cap.
pub fn compile(path: &str, output: &str, entry: &str) -> i32 {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("must-compile".to_owned())
            .stack_size(codegen_wasm::STACK_BUDGET)
            .spawn_scoped(scope, || compile_on_budget(path, output, entry))
            .expect("spawning the compile thread")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

fn compile_on_budget(path: &str, output: &str, entry: &str) -> i32 {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read `{path}`: {err}");
            return 2;
        }
    };

    let db = RootDatabase::default();
    let prepared = match crate::runner::prepare(&db, &text, path, entry) {
        Ok(prepared) => prepared,
        // A file-syntax failure is the same class `check` reports on the
        // same file (exit 1); a bad entry expression is a usage error (the
        // file itself may check clean).
        Err(err @ PrepareError::FileSyntax(_)) => {
            eprintln!("{err}");
            return 1;
        }
        Err(err @ PrepareError::Entry(_)) => {
            eprintln!("{err}");
            return 2;
        }
    };

    // A backend compiles checked programs. `prepared.file` already carries
    // the one parse this needs; diagnostics are filtered to the file's own
    // range so the synthetic `-e` wrapper (see the module doc) cannot
    // manufacture a const-context error out of an ordinary call.
    let diagnostics: Vec<_> = ide::Analysis::new(db.clone())
        .diagnostics(prepared.file)
        .into_iter()
        .filter(|d| usize::from(d.range.start()) < prepared.original_len)
        .collect();
    let rendered = crate::check::render(path, &text, &diagnostics);
    print!("{}", rendered.text);
    if rendered.errors > 0 {
        eprintln!("error: `{path}` has errors; nothing was compiled");
        return 1;
    }

    let artifact = match codegen_wasm::compile(&db, &prepared.entry) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("error: {}", err.message());
            if let Some(origin) = err.origin() {
                match crate::runner::source_position(
                    &db,
                    prepared.file,
                    prepared.original_len,
                    &origin,
                ) {
                    Some((line, column)) => {
                        eprintln!("  --> {path}:{line}:{column}");
                        if let Some(source_line) = text.lines().nth(line as usize - 1) {
                            eprintln!("   |");
                            eprintln!("{line:>3}| {}", source_line.trim_end());
                        }
                    }
                    None => eprintln!("  --> the entry expression"),
                }
            }
            return 1;
        }
    };

    if let Err(err) = std::fs::write(output, &artifact.wasm) {
        eprintln!("error: cannot write `{output}`: {err}");
        return 2;
    }
    0
}
