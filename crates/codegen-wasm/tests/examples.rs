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
    // `option.must`'s three owned-receiver `Run:` lines: a tagged-enum
    // payload out through a consuming member (both the `::Some` and the
    // `::None`-traps-on-`panic` path), and a `fn`-value argument threaded
    // through a generic member. Its borrowing member (`is_some`) is with
    // the borrow cluster below.
    (
        "option.must",
        "{ let o: Option::<usize> = Option::Some(1); o.unwrap() }",
    ),
    (
        "option.must",
        "{ let o: Option::<usize> = Option::None; o.unwrap() }",
    ),
    (
        "option.must",
        "Option::Some(1).flat_map(fn(x: usize) -> Option::<usize> { Option::Some(x + 1) }).unwrap()",
    ),
    // The borrow cluster: borrows of locals, fields and temporaries,
    // reborrows, `Self.&mut` members, and matching through a borrow —
    // all of it frame slots and address arithmetic.
    ("borrows.must", "main()"),
    ("reborrow.must", "main()"),
    ("reborrow.must", "total_count()"),
    ("match_projection.must", "main()"),
    ("match_projection.must", "project_in_demo()"),
    (
        "option.must",
        "{ let o: Option::<usize> = Option::Some(1); o.&.is_some() }",
    ),
    ("records.must", "sum_pair"),
    ("records.must", "moved"),
    ("records.must", "same_point"),
];

/// `(file, entry expression, what the refusal must name)` — the
/// invocations this backend does NOT compile, with the diagnostic each
/// owes the user.
/// The heap and raw pointers are out of scope for this backend; refusing
/// them honestly is the requirement, and this is where that is checked.
const UNSUPPORTED: &[(&str, &str, &str)] = &[
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
    // `read_line` has no wasm import yet — `mono::resolve_call`'s
    // exhaustive `Builtin` match lists it among the refused, so it is
    // named by its own name, exactly like `alloc_array` above.
    (
        "stdin.must",
        "main()",
        "the `read_line` builtin is not supported by the wasm backend yet",
    ),
    // `chars.must` reads stdin too, so `read_line` is what its `main`
    // reaches first. Its OTHER refusal — `next_char`, which this backend
    // has no string decoder for — is pinned on its own program in
    // `differential.rs`'s `next_char_is_refused_by_name`, so neither
    // builtin can quietly start compiling behind the other.
    (
        "chars.must",
        "main()",
        "the `read_line` builtin is not supported by the wasm backend yet",
    ),
    // The stdin LIBRARY needs a heap for its buffer, so it stops at
    // `alloc_array` — before either of the two things this backend cannot
    // do for it. Those are pinned on their own programs elsewhere, so
    // neither can quietly start compiling behind this one: the `extern`
    // item `read` (its buffer parameter is a raw pointer) in `structure.rs`'s
    // `an_extern_item_whose_signature_has_no_wasm_shape_is_refused_by_name`, and
    // the two blesses in `differential.rs`'s `the_blesses_are_refused_by_name`.
    (
        "stdin_lib.must",
        "main()",
        "the `alloc_array` builtin is not supported by the wasm backend yet",
    ),
    // Its heap allocations (`heap.must`, `stdin_lib.must`) are not what
    // fires here: `register` reports the first refused call site in a
    // function's own body before descending into a callee, and `s.len()` is
    // one in `main`. The refusal is about a builtin, not linearity (see
    // `structure.rs`'s `a_linear_type_changes_nothing_about_the_emitted_module`).
    (
        "string_lib.must",
        "main()",
        "the `len` builtin is not supported by the wasm backend yet",
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
    let files = example_files();
    for file in &files {
        let classified = SUPPORTED.iter().any(|(name, _)| *name == file)
            || UNSUPPORTED.iter().any(|(name, _, _)| *name == file);
        assert!(
            classified,
            "examples/{file} is neither compiled nor listed as refused — add it to \
             SUPPORTED or UNSUPPORTED in crates/codegen-wasm/tests/examples.rs so \
             this backend's coverage stays honest about its edges"
        );
    }
    // An exclusion has to name a real file that still has entries run
    // instead, or it documents nothing.
    for (file, why) in NOT_RUN {
        assert!(
            files.iter().any(|name| name == file),
            "NOT_RUN names examples/{file}, which does not exist ({why})"
        );
        assert!(
            SUPPORTED.iter().any(|(name, _)| name == file),
            "NOT_RUN excludes an entry of examples/{file}, but none of its \
             entries are in SUPPORTED ({why})"
        );
    }
}

#[test]
fn supported_examples_agree_with_the_interpreter() {
    // One `check` per entry, not `check_many`: each entry has to type
    // the file exactly as `must run -e` does.
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
