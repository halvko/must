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
    check_const_impl(text, None, expect);
}

fn check_const_impl(text: &str, fuel: Option<u64>, expect: Expect) {
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
        let result = match fuel {
            None => crate::const_value(&db, item).clone(),
            Some(fuel) => {
                crate::const_value_on(&db, Machine::for_const(&db).with_const_fuel(fuel), item)
            }
        };
        rendered.push_str(&match result {
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
    let rendered = run_entry(text, entry, input, check_traps, |db, machine, result| {
        render_run(db, machine.mode.out, result, false)
    });
    expect.assert_eq(&rendered);
}

/// Appends `static entrypoint = (<entry>);` to the fixture's last file,
/// runs it in run mode, and hands the finished machine and the result to
/// `inspect`.
fn run_entry<I: std::io::BufRead, R>(
    text: &str,
    entry: &str,
    input: I,
    check_traps: bool,
    inspect: impl for<'db> FnOnce(
        &'db RootDatabase,
        Machine<'db, RunMode<Vec<u8>, I>>,
        Result<Value, crate::EvalError>,
    ) -> R,
) -> R {
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
    inspect(&db, machine, result)
}

fn render_run(
    db: &RootDatabase,
    out: Vec<u8>,
    result: Result<Value, crate::EvalError>,
    note_sites: bool,
) -> String {
    let printed = String::from_utf8(out).unwrap();
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
            // Secondary provenance renders as bare labels unless the test
            // asks for its sites (locations are otherwise the driver's
            // business; presence is what most tests pin).
            let mut notes = String::new();
            for note in &err.notes {
                notes.push_str(&format!("  note: {}\n", note.message));
                if note_sites {
                    notes.push_str(&note_site(db, note));
                }
            }
            format!("error[{:?}]: {}\n{notes}", err.kind, err.message)
        }
    });
    rendered
}

/// `    at line N: `<expr>` in `<line>`` for a note's origin.
fn note_site(db: &RootDatabase, note: &crate::EvalNote) -> String {
    let (loc, expr) = note.origin.clone().expect("the note has a site");
    let item = loc.to_id(db);
    let text = item.file(db).text(db);
    let (_, source_map) = hir::body_with_source_map(db, item);
    let range = source_map
        .node_for_expr(expr)
        .expect("the site was written")
        .text_range();
    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
    let line = text[..start].lines().count();
    let line_text = text.lines().nth(line - 1).unwrap().trim();
    format!(
        "    at line {line}: `{}` in `{line_text}`\n",
        &text[start..end]
    )
}

/// Renders `const_block_values` for every item in the fixture: each
/// `const { … }` block's check-time result, in lowering order (inner blocks
/// before the blocks enclosing them), indexed per item.
fn check_const_blocks(text: &str, expect: Expect) {
    check_const_blocks_impl(text, None, expect);
}

fn check_const_blocks_impl(text: &str, fuel: Option<u64>, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        let results = match fuel {
            None => crate::const_block_values(&db, item).clone(),
            Some(fuel) => crate::const_block_values_on(
                &db,
                Machine::for_const(&db).with_const_fuel(fuel),
                item,
            ),
        };
        for (i, (_, result)) in results.iter().enumerate() {
            rendered.push_str(&match result {
                Ok(value) => format!("{name}#{i} = {}\n", value.display()),
                Err(err) => format!("{name}#{i} = error[{:?}]: {}\n", err.kind, err.message),
            });
        }
    }
    expect.assert_eq(&rendered);
}

/// Finds an item by name in the fixture.
fn item<'db>(db: &'db RootDatabase, file: SourceFile, name: &str) -> hir::ItemId<'db> {
    *hir::file_item_ids(db, file)
        .iter()
        .find(|&&it| it.name(db) == name)
        .unwrap_or_else(|| panic!("no item `{name}`"))
}
