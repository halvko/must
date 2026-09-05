//! `must-lsp compile` at the process level: a real `.wasm` file comes out,
//! a real engine runs it, and the refusals are the ones a user sees.
//!
//! The in-process differential harness (`crates/codegen-wasm/tests`) is
//! where semantics are proven; this file proves the *command* — that the
//! bytes reach disk, that the module a user gets is loadable by an engine
//! that knows nothing about this project, and that the failure paths say
//! what they should and write nothing.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_must-lsp");

fn fixture(name: &str, text: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("must-compile-{}-{name}.must", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(text.as_bytes()).expect("write fixture");
    path
}

fn out_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("must-compile-{}-{name}.wasm", std::process::id()))
}

fn run(args: &[&str]) -> (String, i32) {
    let output = Command::new(BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn must-lsp");
    let mut merged = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    merged.push_str(&String::from_utf8(output.stderr).expect("stderr is UTF-8"));
    (merged, output.status.code().expect("exited normally"))
}

/// Load and run a `.wasm` file the way any host would: wire the one
/// import, call the one export, capture what it printed.
fn execute(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).expect("read the compiled module");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &bytes[..]).expect("the module validates");
    let mut store = wasmi::Store::new(&engine, Vec::<u8>::new());
    let mut linker = wasmi::Linker::new(&engine);
    linker
        .func_wrap(
            "must",
            "print",
            |mut caller: wasmi::Caller<'_, Vec<u8>>, offset: i32, len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(wasmi::Extern::into_memory)
                    .expect("the module exports its memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, offset as usize, &mut buf)
                    .expect("read");
                caller.data_mut().extend_from_slice(&buf);
            },
        )
        .expect("define `print`");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    let main = instance
        .get_func(&store, "main")
        .expect("the module exports `main`");
    let mut results = vec![wasmi::Val::I64(0); main.ty(&store).results().len()];
    main.call(&mut store, &[], &mut results)
        .expect("runs clean");
    String::from_utf8(store.data().clone()).expect("output is UTF-8")
}

#[test]
fn a_compiled_program_runs_on_a_plain_engine() {
    let source = fixture(
        "hello",
        r#"
static greeting = "hello from wasm\n";
static twice = fn (n: usize) -> usize { n + n };
static main = fn () -> () {
    print(greeting);
    if twice(21) == 42 { print("42\n") } else { print("bad\n") };
}
"#,
    );
    let wasm = out_path("hello");
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "compilation should succeed: {message}");
    assert_eq!(execute(&wasm), "hello from wasm\n42\n");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn the_entry_expression_can_be_chosen() {
    let source = fixture(
        "entry",
        r#"
static shout = fn (n: usize) -> () { print("called\n"); };
static main = fn () -> () { print("main\n"); }
"#,
    );
    let wasm = out_path("entry");
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
        "-e",
        "shout(1)",
    ]);
    assert_eq!(code, 0, "{message}");
    assert_eq!(execute(&wasm), "called\n");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn an_unsupported_construct_is_refused_by_name_and_located() {
    let source = fixture(
        "unsupported",
        r#"
static S: usize = 7;
static main = fn () -> () {
    let p = S.&raw;
    unsafe { print("x"); };
}
"#,
    );
    let wasm = out_path("unsupported");
    let _ = std::fs::remove_file(&wasm);
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "a refusal exits 1: {message}");
    assert!(
        message.contains("is not supported by the wasm backend yet"),
        "the refusal must name itself:\n{message}"
    );
    assert!(
        message.contains("raw pointer"),
        "the refusal must name the construct:\n{message}"
    );
    assert!(
        message.contains(".must:4:"),
        "the refusal must point at the source:\n{message}"
    );
    assert!(
        !wasm.exists(),
        "a refused compilation must not write a module"
    );
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_program_with_errors_is_not_compiled() {
    let source = fixture("broken", r#"static main = fn { let v: usize = "s"; };"#);
    let wasm = out_path("broken");
    let _ = std::fs::remove_file(&wasm);
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{message}");
    assert!(
        message.contains("type mismatch: expected `usize`, found `str`"),
        "the checker's diagnostics are shown:\n{message}"
    );
    assert!(
        message.contains("nothing was compiled"),
        "and it says nothing was written:\n{message}"
    );
    assert!(!wasm.exists());
    let _ = std::fs::remove_file(&source);
}

#[test]
fn calling_an_ordinary_function_as_the_entry_is_not_a_const_context_error() {
    // The regression this guards: injecting `-e`'s expression as a
    // `static` initializer makes it a const context, and checking that
    // context by the file's own rules would reject calling any non-`const
    // fn` there at all — which is what the DEFAULT entry, `main()`, always
    // does. The check gate must stay scoped to the file's own items.
    let source = fixture(
        "entry-ordinary-call",
        r#"
static main = fn () -> () { print("ok\n") };
"#,
    );
    let wasm = out_path("entry-ordinary-call");
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert_eq!(
        code, 0,
        "the default entry must not fail to check: {message}"
    );
    assert_eq!(execute(&wasm), "ok\n");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn an_ill_typed_entry_expression_compiles_to_a_module_that_traps() {
    // The entry expression is not check-gated (see the module doc): a
    // wrong-arity call lowers the way any ill-typed body does — totally,
    // with a trap standing in for the diagnostic — so `compile` still
    // writes a module, and that module traps the instant it runs, the way
    // `must-lsp run` would fail on the same expression. Not a silent
    // miscompile: loud, just at a different time than a file-body error.
    let source = fixture(
        "entry-ill-typed",
        r#"
static takes_two = fn (a: usize, b: usize) -> usize { a + b };
static main = fn () -> () {}
"#,
    );
    let wasm = out_path("entry-ill-typed");
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
        "-e",
        "takes_two(1)",
    ]);
    assert_eq!(code, 0, "compiling still succeeds: {message}");
    assert!(wasm.exists());

    let bytes = std::fs::read(&wasm).expect("read the compiled module");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &bytes[..]).expect("the module validates");
    let mut store = wasmi::Store::new(&engine, ());
    let mut linker = wasmi::Linker::new(&engine);
    linker
        .func_wrap(
            "must",
            "print",
            |_: wasmi::Caller<'_, ()>, _: i32, _: i32| {},
        )
        .expect("define `print`");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    let main = instance.get_func(&store, "main").expect("exports `main`");
    let mut results = vec![wasmi::Val::I64(0); main.ty(&store).results().len()];
    let outcome = main.call(&mut store, &[], &mut results);
    assert!(
        outcome.is_err(),
        "the wrong-arity entry must trap, not compute a wrong answer"
    );

    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn missing_arguments_are_usage_errors() {
    let (message, code) = run(&["compile"]);
    assert_eq!(code, 2, "{message}");
    assert!(message.contains("usage: must-lsp compile"), "{message}");

    let source = fixture("noout", "static main = fn () -> () {}");
    let (message, code) = run(&["compile", source.to_str().unwrap()]);
    assert_eq!(code, 2, "an output path is required: {message}");
    assert!(message.contains("usage: must-lsp compile"), "{message}");
    let _ = std::fs::remove_file(&source);

    let (message, code) = run(&["compile", "/nonexistent/nope.must", "-o", "/tmp/x.wasm"]);
    assert_eq!(code, 2, "{message}");
    assert!(message.contains("cannot read"), "{message}");
}

#[test]
fn a_file_syntax_error_that_swallows_the_entry_exits_like_check() {
    // An unterminated string eats everything after it, including the
    // injected entry item — the honest report is the file's own syntax
    // error, and `compile` must fail the same way `check` does on this
    // same file: it is the file's fault, not a bad `-e` usage.
    let source = fixture(
        "unterminated",
        "static s = \"oops;\nstatic main = fn { print(s); };",
    );
    let wasm = out_path("unterminated");
    let _ = std::fs::remove_file(&wasm);
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    let (check_message, check_code) = run(&["check", source.to_str().unwrap()]);
    assert_eq!(
        code, check_code,
        "compile must fail the same way check does on this file:\n{message}\ncheck said:\n{check_message}"
    );
    assert_eq!(code, 1, "{message}");
    assert!(
        message.contains("syntax error that swallows the end of the file"),
        "{message}"
    );
    assert!(!wasm.exists());
    let _ = std::fs::remove_file(&source);
}

#[test]
fn a_malformed_entry_expression_is_a_usage_error() {
    // Unlike a broken file, a broken `-e` expression is the caller's own
    // mistake — the file itself may check clean — so it keeps the usage
    // exit code rather than borrowing `check`'s.
    let source = fixture("valid-file", "static main = fn () -> () {}");
    let wasm = out_path("malformed-entry");
    let _ = std::fs::remove_file(&wasm);
    let (message, code) = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
        "-e",
        "1 +",
    ]);
    assert_eq!(code, 2, "{message}");
    assert!(message.contains("invalid entry expression"), "{message}");
    assert!(!wasm.exists());
    let _ = std::fs::remove_file(&source);
}

#[test]
fn the_examples_flagship_compiles_and_prints_what_the_interpreter_prints() {
    // `display.must` is the traits milestone: dictionaries resolved into
    // direct calls, three implementers of one generic consumer. Compiled
    // and interpreted output must be byte-identical.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let example = root.join("examples/display.must");
    let wasm = out_path("display");
    let (message, code) = run(&[
        "compile",
        example.to_str().unwrap(),
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{message}");
    let (interpreted, code) = run(&["run", example.to_str().unwrap()]);
    assert_eq!(code, 0, "{interpreted}");
    assert_eq!(execute(&wasm), interpreted);
    let _ = std::fs::remove_file(&wasm);
}
