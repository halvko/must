mod aliasing;
mod arrays;
mod assignment;
mod bytes_and_text;
mod chars_and_strings;
mod const_eval;
mod evaluation_order;
mod generics;
mod heap;
mod host;
mod integers;
mod loops_and_return;
mod members_and_traits;
mod moves_and_storage;
mod multi_file;
mod patterns;
mod raw_pointers;
mod records_and_enums;
mod run;
mod stdin;
mod stepping;
mod traps;

use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

use crate::machine::{Machine, RunMode, StepEvent};
use crate::{EvalErrorKind, Value};

/// Renders `const_value` for every *value* item in the fixture (`type`
/// items declare no value — nothing to render).
fn check_const(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        if hir::item_data(&db, item)
            .as_ref()
            .is_some_and(|data| matches!(data.kind, hir::ItemKind::Type | hir::ItemKind::Trait))
        {
            continue;
        }
        let name = item.name(&db);
        rendered.push_str(&match crate::const_value(&db, item) {
            Ok(value) => format!("{name} = {}\n", value.display()),
            Err(err) => format!("{name} = error[{:?}]: {}\n", err.kind, err.message),
        });
    }
    expect.assert_eq(&rendered);
}

/// Appends `static entrypoint = (<entry>);` to the fixture and runs it with
/// run-mode semantics (`print` is legal at the entry, statics still force as
/// consts). Renders captured output, then the result. No stdin: a
/// `read_line` call sees immediate end-of-input — see
/// [`check_run_with_input`] for the injectable-input twin. `text` may hold
/// several files ([`base_db::fixture_files`]); the entry is appended to the
/// last file.
fn check_run(text: &str, entry: &str, expect: Expect) {
    check_run_impl(text, entry, std::io::empty(), true, expect);
}

/// Runs a program with the CFG checkers' traps (loans, moves) off, so what
/// stops a refused program is the aliasing tree's own detection. The tests
/// of that dynamic half use this; `run` never does.
fn check_run_past_the_cfg_checkers(text: &str, entry: &str, expect: Expect) {
    check_run_impl(text, entry, std::io::empty(), false, expect);
}

/// [`check_run`], but `input` is fed to whatever reads standard input as
/// though it were piped stdin — the mirror of how `check_run` captures
/// `print` output: this is the injection side. The builtin `read_line`
/// consumes it line-by-line (a final line with no trailing `\n` still
/// reads, as the last `Line`, then `End` on the call after); a program
/// that declares its own `read` `extern` item instead consumes it
/// byte-wise, exactly as a real `read(2)` would.
fn check_run_with_input(text: &str, entry: &str, input: &str, expect: Expect) {
    check_run_with_bytes(text, entry, input.as_bytes(), expect);
}

/// [`check_run_with_input`]'s raw-bytes twin, for the one input a Rust
/// `&str` cannot express because it is valid UTF-8 by construction: bytes
/// a bless has to answer for, or an invalid-UTF-8 stream `read_line` must
/// reject by name. This is the one place any eval fixture pipes stdin —
/// [`check_run_with_input`] is defined in terms of it.
fn check_run_with_bytes(text: &str, entry: &str, input: &[u8], expect: Expect) {
    check_run_impl(
        text,
        entry,
        std::io::Cursor::new(input.to_vec()),
        true,
        expect,
    );
}

fn check_run_impl(
    text: &str,
    entry: &str,
    input: impl std::io::BufRead,
    check_traps: bool,
    expect: Expect,
) {
    let mut db = RootDatabase::default();
    let mut files = base_db::fixture_files(text);
    let (_, last) = files.last_mut().expect("a fixture has a file");
    last.push_str(&format!("\nstatic entrypoint = ({entry});\n"));
    let file = *base_db::new_library(&mut db, "", files)
        .last()
        .expect("a fixture has a file");
    let entry_item = *hir::file_item_ids(&db, file)
        .iter()
        .find(|&&it| it.name(&db) == "entrypoint")
        .expect("entrypoint item exists");
    let mut machine = Machine::new(
        &db,
        RunMode {
            out: Vec::new(),
            input,
        },
    );
    machine.check_traps = check_traps;
    let result = machine.eval_root(&hir::item_loc(&db, entry_item));
    let printed = String::from_utf8(machine.mode.out).unwrap();
    // Printed output is shown *escaped*, as one `output:` line. `print`
    // writes exactly its argument and appends nothing, so line breaks are
    // part of the program's output rather than a property of `print` —
    // rendering the raw bytes would run consecutive prints together
    // (`431` for three separate digits) and would hide a newline creeping
    // back into `RunMode`. `{:?}` makes every byte visible.
    let mut rendered = String::new();
    if !printed.is_empty() {
        rendered.push_str(&format!("output: {printed:?}\n"));
    }
    rendered.push_str(&match result {
        Ok(value) => format!("=> {}\n", value.display()),
        Err(err) => {
            // Secondary provenance renders as bare labels here (locations
            // are the driver's business; presence is what these tests pin).
            let notes = err
                .notes
                .iter()
                .map(|note| format!("  note: {}\n", note.message))
                .collect::<String>();
            format!("error[{:?}]: {}\n{notes}", err.kind, err.message)
        }
    });
    expect.assert_eq(&rendered);
}

/// Renders `const_block_values` for every item in the fixture: each
/// `const { … }` block's check-time result, in lowering order (inner blocks
/// before the blocks enclosing them), indexed per item.
fn check_const_blocks(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        for (i, (_, result)) in crate::const_block_values(&db, item).iter().enumerate() {
            rendered.push_str(&match result {
                Ok(value) => format!("{name}#{i} = {}\n", value.display()),
                Err(err) => format!("{name}#{i} = error[{:?}]: {}\n", err.kind, err.message),
            });
        }
    }
    expect.assert_eq(&rendered);
}

/// A `Write` that records writes and flushes in order — the only way to
/// see *when* the output buffer was drained relative to a blocking read.
#[derive(Default)]
struct FlushLog(Vec<String>);

impl std::io::Write for FlushLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .push(format!("write {:?}", String::from_utf8_lossy(buf)));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.push("flush".to_owned());
        Ok(())
    }
}

/// Finds an item by name in the fixture.
fn item<'db>(db: &'db RootDatabase, file: SourceFile, name: &str) -> hir::ItemId<'db> {
    *hir::file_item_ids(db, file)
        .iter()
        .find(|&&it| it.name(db) == name)
        .unwrap_or_else(|| panic!("no item `{name}`"))
}

// ---- the blesses: bytes into `str` --------------------------------------

/// A buffer of `n` bytes filled from a `str` literal's own bytes, so a
/// bless fixture can start from known contents without a host read. The
/// bytes go in one at a time through `add`, which is the only way to write
/// a buffer today.
fn bytes_fixture(bytes: &[u8]) -> String {
    let mut writes = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        writes.push_str(&format!("unsafe {{ add(p, {index}).* = {byte}; }};\n"));
    }
    format!(
        "static with_bytes = fn::<T>(k: fn(u8.&raw mut) -> T) -> T {{\n\
             match alloc_array::<u8>({len}) {{\n\
                 AllocResult::Ok(p) => {{\n\
                     {writes}\
                     let out = k(p);\n\
                     unsafe {{ dealloc_array(p, {len}); }};\n\
                     out\n\
                 }}\n\
                 AllocResult::Err => panic(\"out of memory\"),\n\
             }}\n\
         }};\n",
        len = bytes.len(),
    )
}

// ---- the stdin library, from the example itself ------------------------

/// `examples/stdin_lib.must` verbatim. Reading it off disk (rather than a
/// hand-shrunk copy) means these tests pin the exact behaviour, including
/// the exact panic text, of the file a user actually runs — a re-wording of
/// the example cannot silently drift from what is pinned here. `main`,
/// `print_usize`, `count_bytes` and `digit` go unused under the test-only
/// entry each test appends below; that is fine, `check_run_impl` does not
/// assert on diagnostics.
const STDIN_LIB: &str = include_str!("../../../../examples/stdin_lib.must");

const BROKEN_DECLARATION_IN_ANOTHER_FILE: &str = r#"
//- a.must
type Pair = struct { a: Bogus, b: usize };
//- b.must
use a::Pair;
static main = fn() -> usize {
    print("a;");
    let p = Pair(struct { a = 1, b = 2 });
    print("b;");
    p.b
};
static other = fn() -> usize { 3 };
"#;
