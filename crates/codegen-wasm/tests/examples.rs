//! The differential harness pointed at `examples/` — the repository's own
//! programs, compiled and run against the interpreter that defines them.
//!
//! Coverage here is deliberately *honest about its edges*: every example
//! is classified, and a file that this backend cannot compile is listed
//! with the refusal it must produce, asserted like any other expectation.
//! Adding an example without classifying it fails
//! [`every_example_is_classified`], so the list can never quietly go stale.

mod harness;

use std::path::{Path, PathBuf};

use harness::{check, refusal};

/// `(file, entry expression)` — every invocation the harness runs under
/// both engines. Entries mirror the `// Run:` lines the examples
/// document, so what is compared is what a reader is told to run.
const SUPPORTED: &[(&str, &str)] = &[
    ("arrays.must", "main()"),
    ("compile_time.must", "fortnight_seconds"),
    ("compile_time.must", "is_big"),
    ("compile_time.must", "greet()"),
    ("display.must", "main()"),
    ("functions.must", "main()"),
    ("functions.must", "fib(20)"),
    ("functions.must", "gcd(1071, 462)"),
    ("functions.must", "is_even(76)"),
    ("functions.must", "is_odd(76)"),
    ("functions.must", "apply_twice(double, 5)"),
    ("generics.must", "id(\"hi\")"),
    ("generics.must", "id::<usize>(4)"),
    ("generics.must", "square_val"),
    ("generics.must", "cube_val"),
    ("generics.must", "braced_val"),
    ("generics.must", "pair_first"),
    ("hello.must", "main()"),
    ("loops.must", "sum_to(10)"),
    ("loops.must", "sum_odds_below(10)"),
    ("loops.must", "sum_to_const"),
    ("loops.must", "find_even_multiple(3, 20)"),
    ("loops.must", "find_even_multiple(5, 4)"),
    ("loops.must", "find_even_multiple(0, 20)"),
    ("records.must", "sum_pair"),
    ("records.must", "moved"),
    ("records.must", "same_point"),
    ("state_machine.must", "cycle_once"),
    ("state_machine.must", "describe(cycle_once)"),
    ("state_machine.must", "run_lights()"),
];

/// `(file, entry expression, what the refusal must name)` — the examples
/// this backend does NOT compile, with the diagnostic it owes the user.
/// The heap and raw pointers are out of scope for this backend; refusing
/// them honestly is the requirement, and this is where that is checked.
const UNSUPPORTED: &[(&str, &str, &str)] = &[
    // Safe borrows: refused BY NAME, not folded into the raw-pointer
    // refusal. A borrow lowers to the same machine word a raw pointer
    // does, so this backend COULD emit something that runs — and would
    // drop the exclusivity contract while doing it. The named refusal is
    // what keeps that from being a silent miscompile.
    (
        "borrows.must",
        "main()",
        "a safe borrow (`.&` / `.&mut`) is not supported by the wasm backend yet",
    ),
    // The borrow-ergonomics fixture is borrow-shaped end to end (its
    // members take `Self.&mut`), so it lands on the same named refusal
    // `borrows.must` does — for the same reason: the backend could emit
    // something that runs and would drop the exclusivity contract doing it.
    (
        "reborrow.must",
        "main()",
        "a safe borrow (`.&` / `.&mut`) is not supported by the wasm backend yet",
    ),
    (
        "reborrow.must",
        "total_count()",
        "a safe borrow (`.&` / `.&mut`) is not supported by the wasm backend yet",
    ),
    // Match-through-a-borrow is borrow code like any other, and it must
    // refuse by the SAME name rather than inventing a match-shaped
    // excuse: a borrowed match lowers to a deref-rooted `switch` and
    // `Rvalue::Borrow` payload bindings, both of which this backend could
    // emit as plain address arithmetic that runs and silently drops
    // exclusivity. The refusal fires twice over — the scrutinee local's
    // type has no layout, and `Rvalue::Borrow` is refused by name — and
    // this pins that neither route was quietly opened.
    (
        "match_projection.must",
        "main()",
        "a safe borrow (`.&` / `.&mut`) is not supported by the wasm backend yet",
    ),
    (
        "match_projection.must",
        "project_in_demo()",
        "a safe borrow (`.&` / `.&mut`) is not supported by the wasm backend yet",
    ),
    (
        "heap.must",
        "main()",
        "the `alloc_array` builtin is not supported by the wasm backend yet",
    ),
    (
        "pointers.must",
        "main()",
        "a raw pointer (heap and pointer primitives are out of scope for this backend)",
    ),
    // Intentionally dirty: `errors.must` exists to show diagnostics, and
    // its erroneous items have no compilable meaning.
    (
        "errors.must",
        "main()",
        "is not supported by the wasm backend yet",
    ),
];

/// `records.must`'s `main` calls a function that loops forever by design
/// (it demonstrates the `!` type). Running it under either engine would
/// hang, so it is covered through its documented `-e` entries instead.
const NOT_RUN: &[(&str, &str)] = &[(
    "records.must",
    "main() diverges by design (`f` ends in `loop {}`)",
)];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/codegen-wasm has two ancestor directories")
        .to_path_buf()
}

fn source(file: &str) -> String {
    let path = workspace_root().join("examples").join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn example_files() -> Vec<String> {
    let dir = workspace_root().join("examples");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("read examples/")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "must"))
        .map(|path| {
            path.file_name()
                .expect("a file has a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn every_example_is_classified() {
    for file in example_files() {
        let classified = SUPPORTED.iter().any(|(name, _)| *name == file)
            || UNSUPPORTED.iter().any(|(name, _, _)| *name == file);
        assert!(
            classified,
            "examples/{file} is neither compiled nor listed as refused — add it to \
             SUPPORTED or UNSUPPORTED in crates/codegen-wasm/tests/examples.rs so \
             this backend's coverage stays honest about its edges"
        );
    }
    assert!(!NOT_RUN.is_empty(), "the exclusion list documents itself");
}

#[test]
fn supported_examples_agree_with_the_interpreter() {
    for (file, entry) in SUPPORTED {
        let text = source(file);
        println!("checking examples/{file} -e '{entry}'");
        check(&text, entry);
    }
}

#[test]
fn unsupported_examples_are_refused_by_name() {
    for (file, entry, expected) in UNSUPPORTED {
        let text = source(file);
        let message = refusal(&text, entry);
        assert!(
            message.contains(expected),
            "examples/{file} refused with the wrong diagnostic\n  \
             expected to contain: {expected}\n  got: {message}"
        );
        // `contains` alone cannot see a refusal that repeats the suffix
        // `Refusal::message` already appends — the message stays a
        // superstring of what the table asks for, so the assertion above
        // passes while the user reads the sentence twice.
        const SUFFIX: &str = "is not supported by the wasm backend yet";
        assert_eq!(
            message.matches(SUFFIX).count(),
            usize::from(message.contains(SUFFIX)),
            "examples/{file}'s refusal repeats its own suffix: {message}"
        );
    }
}
