//! `must compile file.must -o file.wasm`: the WebAssembly backend at
//! the command line.
//!
//! The compiled module is a *platform instance* in P01's sense — it
//! imports its effects (`must.print`) and exports `main`, and it carries
//! no runtime whatsoever. What it runs is the same MIR `must run`
//! interprets, so the two agree by construction (and by the differential
//! harness in `crates/codegen-wasm/tests`).
//!
//! Two refusals are possible, and both are loud:
//!
//! * the FILE's library (every module of it, not only those the program
//!   reaches) does not check clean — a backend has no business compiling
//!   code the checker rejects, so the diagnostics are printed and nothing
//!   is written, exiting with `check`'s own exit code. This is narrower
//!   than it sounds: it covers the library's items, not the `-e` entry
//!   expression injected on top of them. That expression runs the way
//!   `must run` runs it — as run-mode code, not a checked const
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

use crate::runner::{LoadError, PrepareError, Program};

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
    let program = match crate::runner::load(path, entry) {
        Ok(program) => program,
        Err(LoadError::Source(message)) => {
            eprintln!("{message}");
            return 2;
        }
        // A file-syntax failure is the same class `check` reports on the
        // same file (exit 1); a bad entry expression is a usage error (the
        // file itself may check clean).
        Err(LoadError::Prepare(err @ PrepareError::FileSyntax(_))) => {
            eprintln!("{err}");
            return 1;
        }
        Err(LoadError::Prepare(err @ PrepareError::Entry(_))) => {
            eprintln!("{err}");
            return 2;
        }
    };
    let Program {
        db,
        sources,
        prepared,
    } = &program;

    // Every error in the library refuses the compile, the named file's
    // first. Diagnostics inside the synthetic `-e` wrapper are dropped (see
    // the module doc), so an ordinary call cannot raise a const-context error.
    let analysis = ide::Analysis::new(db.clone());
    let mut errors = 0;
    for file in crate::check::library_files(db, prepared.file) {
        let diagnostics: Vec<_> = (analysis.diagnostics(file).into_iter())
            .filter(|d| !prepared.in_entry(file, d.range.start()))
            .collect();
        let rendered = crate::check::render(&analysis, sources, file, &diagnostics);
        print!("{}", rendered.text);
        errors += rendered.errors;
    }
    if errors > 0 {
        eprintln!("error: the library of `{path}` has errors; nothing was compiled");
        return 1;
    }

    let artifact = match codegen_wasm::compile(db, &prepared.entry) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("error: {}", err.message());
            if let Some(origin) = err.origin() {
                let file = origin.0.file;
                let position = crate::runner::source_position(
                    db,
                    prepared.file,
                    prepared.original_len,
                    &origin,
                );
                match (position, sources.display(file).zip(sources.text(file))) {
                    (None, _) => eprintln!("  --> the entry expression"),
                    // In a file no tool loaded (the prelude): nowhere to point.
                    (Some(_), None) => {}
                    (Some((line, column)), Some((path, text))) => {
                        eprintln!("  --> {path}:{line}:{column}");
                        if let Some(source_line) = text.lines().nth(line as usize - 1) {
                            eprintln!("   |");
                            eprintln!("{line:>3}| {}", source_line.trim_end());
                        }
                    }
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
