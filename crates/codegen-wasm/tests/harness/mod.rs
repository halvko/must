// Each test binary uses a different slice of this harness.
#![allow(dead_code)]

//! The differential harness: run a program under the INTERPRETER and
//! under the COMPILED WASM, and insist they agree.
//!
//! This is not a unit test of the backend's opinion of itself: the
//! interpreter is the specification (it is what `must-lsp run`, the
//! debugger and const eval all execute), so agreement here is the only
//! evidence that compiling a Must program without a runtime preserves its
//! meaning.
//!
//! What is compared, for every program:
//!
//! * **stdout, byte for byte** — `print` is wired to a capture buffer on
//!   both sides.
//! * **termination kind** — a clean finish on one side must be a clean
//!   finish on the other, and a trap must be a trap.
//! * **the trap reason** — the compiled module records which trap fired in
//!   its exported `trap_code` global, and the backend's trap table says
//!   what the interpreter must have said (exactly, or as a prefix where
//!   the message names runtime values).
//! * **the result value** — the entry expression's value, decoded out of
//!   the module's multi-value return (integers, `bool`, `str`, `()`).
//!
//! **Known gaps, stated rather than closed.** An engine-level trap (a call
//! stack exhausted by real native recursion) is not comparable to the
//! interpreter's own frame-count limit, so deep recursion near either
//! limit is untested by design — the two sides can legitimately give up
//! at different depths. And the interpreter is both the specification
//! *and* the source of the backend's compile-time values (`static`s,
//! `const` blocks, const arguments all run through the same const
//! machine, X08): a bug in const evaluation is invisible to this harness
//! by construction, since it would produce the same wrong value on both
//! sides.

use base_db::{RootDatabase, SourceFile};
use codegen_wasm::{Artifact, CompileError, TrapKind};
use eval::{EvalErrorKind, Machine, RunMode, Value};

/// How an execution ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Ran to completion with this rendered value (`None` for `()`).
    Value(Option<String>),
    /// Stopped. The payload is the interpreter's message (or, on the wasm
    /// side, the trap table's entry).
    Trap(String),
}

pub struct Run {
    pub stdout: String,
    pub outcome: Outcome,
    /// Which trap entry fired, on the compiled side.
    pub trap: Option<codegen_wasm::TrapInfo>,
}

/// The entry item the runner injects — mirrored here so the harness runs
/// exactly what `must-lsp run` runs.
const ENTRY: &str = "__must_entry";

pub fn prepare(db: &RootDatabase, source: &str, entry: &str) -> hir::ItemLoc {
    let text = format!("{source}\nstatic {ENTRY} = ({entry});\n");
    let file = SourceFile::new(db, "test.must".to_owned(), text);
    let item = hir::file_item_ids(db, file)
        .last()
        .copied()
        .filter(|item| item.name(db) == ENTRY)
        .unwrap_or_else(|| panic!("the injected entry item must be the last item"));
    hir::item_loc(db, item)
}

/// Run `f` on a thread with [`codegen_wasm::STACK_BUDGET`] of stack.
///
/// Every helper here that compiles goes through this or its `on_stack`
/// sibling (only the two depth tests take the sibling, at half the budget,
/// to enforce the margin), so the suite runs the backend under the contract
/// [`codegen_wasm::STACK_BUDGET`] documents rather than on whatever the test
/// harness happens to hand a test thread — a size the backend's depth caps
/// were never calibrated against.
pub fn on_budget<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    on_stack(codegen_wasm::STACK_BUDGET, f)
}

/// Run `f` on a thread of exactly `stack` bytes.
///
/// The depth tests pass `STACK_BUDGET / 2`, the margin `MAX_PATH_DEPTH`'s
/// doc claims: the deepest walk the caps allow has to come back with a
/// refusal there, not abort. The database is built inside `f`:
/// `salsa::Database` is `Send` but not `Sync`, so it can be moved onto
/// the thread but not borrowed across.
pub fn on_stack<T: Send>(stack: usize, f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn_scoped(scope, f)
            .expect("spawning a compile thread")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

/// Run under the interpreter — the specification side.
pub fn interpret(db: &RootDatabase, entry: &hir::ItemLoc) -> Run {
    let mut out = Vec::new();
    // Nothing supported here calls `read_line`: the wasm backend refuses it
    // by name, so no differentially compared program reaches that codepath.
    let result = Machine::new(db, RunMode::without_stdin(&mut out)).eval_root(entry);
    let stdout = String::from_utf8(out).expect("print output is UTF-8");
    let outcome = match result {
        Ok(Value::Unit) => Outcome::Value(None),
        Ok(value) => Outcome::Value(Some(value.display())),
        Err(err) => {
            assert!(
                !matches!(err.kind, EvalErrorKind::Uninstantiated),
                "the interpreter hit an internal case: {}",
                err.message
            );
            Outcome::Trap(err.message)
        }
    };
    Run {
        stdout,
        outcome,
        trap: None,
    }
}

/// Execute a compiled artifact on a real engine (`wasmi` — chosen for
/// being a lightweight, dependency-light interpreter that VALIDATES what
/// it loads, so a malformed module fails loudly here).
pub fn execute(db: &RootDatabase, artifact: &Artifact, entry_ty: &Value) -> Run {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &artifact.wasm[..])
        .unwrap_or_else(|err| panic!("the emitted module must validate: {err}"));
    let mut store = wasmi::Store::new(&engine, Vec::<u8>::new());
    let mut linker = wasmi::Linker::new(&engine);
    linker
        .func_wrap(
            codegen_wasm::IMPORT_MODULE,
            "print",
            |mut caller: wasmi::Caller<'_, Vec<u8>>, offset: i32, len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(wasmi::Extern::into_memory)
                    .expect("the module exports its memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, offset as usize, &mut buf)
                    .expect("`print` reads inside the module's memory");
                caller.data_mut().extend_from_slice(&buf);
            },
        )
        .expect("define the `print` import");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    let func = instance
        .get_func(&store, codegen_wasm::ENTRY_EXPORT)
        .expect("the entry point is exported");
    let mut results = vec![wasmi::Val::I64(0); artifact.entry_results];
    let call = func.call(&mut store, &[], &mut results);
    let stdout = String::from_utf8(store.data().clone()).expect("print output is UTF-8");
    match call {
        Ok(()) => {
            let slots: Vec<i64> = results
                .iter()
                .map(|value| value.i64().expect("every result slot is an i64"))
                .collect();
            let memory = instance
                .get_memory(&store, "memory")
                .expect("the module exports its memory");
            Run {
                trap: None,
                stdout,
                outcome: Outcome::Value(if matches!(entry_ty, Value::Unit) {
                    None
                } else {
                    decode(db, entry_ty, &slots, |offset, len| {
                        let mut buf = vec![0u8; len];
                        memory.read(&store, offset, &mut buf).expect("string bytes");
                        String::from_utf8(buf).expect("string bytes are UTF-8")
                    })
                }),
            }
        }
        Err(_) => {
            let code = instance
                .get_global(&store, "trap_code")
                .expect("the module exports `trap_code`")
                .get(&store)
                .i32()
                .expect("`trap_code` is an i32");
            if code < 0 {
                // The ENGINE trapped, not the program: a call-stack
                // exhaustion, say. The interpreter's frame limit and an
                // engine's stack limit are different numbers, so these are
                // not comparable — say so instead of pretending.
                return Run {
                    stdout,
                    outcome: Outcome::Trap(
                        "<the engine trapped without a reason recorded by the module>".to_owned(),
                    ),
                    trap: None,
                };
            }
            let info = artifact.traps[code as usize].clone();
            // A panic whose message is only known at runtime publishes it
            // through the module's globals.
            let message = if info.kind == TrapKind::Panic && info.message.is_empty() {
                let read = |name: &str| {
                    instance
                        .get_global(&store, name)
                        .expect("the module exports its panic-message globals")
                        .get(&store)
                        .i64()
                        .expect("an i64 global")
                };
                let (offset, len) = (read("panic_message_offset"), read("panic_message_len"));
                let memory = instance
                    .get_memory(&store, "memory")
                    .expect("the module exports its memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&store, offset as usize, &mut buf)
                    .expect("the panic message is inside memory");
                String::from_utf8(buf).expect("the panic message is UTF-8")
            } else {
                info.message.clone()
            };
            Run {
                stdout,
                outcome: Outcome::Trap(message),
                trap: Some(info),
            }
        }
    }
}

/// Render the compiled entry's result slots the way the interpreter
/// renders its value.
///
/// The interpreter's value is the *reference*: it says what shape to read
/// the slots back as (and, for integers, whether the slot was zero- or
/// sign-extended). Reading the compiled result back through the shape and
/// comparing the RENDERING is what makes a wrong field order or a
/// misplaced slot fail this harness rather than pass unnoticed.
fn decode(
    db: &RootDatabase,
    reference: &Value,
    slots: &[i64],
    read_str: impl Fn(usize, usize) -> String,
) -> Option<String> {
    let mut rest = slots;
    let rendered = decode_into(db, reference, &mut rest, &read_str)?;
    assert!(
        rest.is_empty(),
        "the compiled entry returned {} slot(s) more than its value needs",
        rest.len()
    );
    Some(rendered)
}

fn decode_into(
    db: &RootDatabase,
    reference: &Value,
    slots: &mut &[i64],
    read_str: &impl Fn(usize, usize) -> String,
) -> Option<String> {
    let mut take = |count: usize| {
        let (head, tail) = slots.split_at(count);
        *slots = tail;
        head.to_vec()
    };
    let list = |parts: Vec<String>| parts.join(", ");
    match reference {
        // The interpreter prints nothing for `()`; it occupies no slots.
        Value::Unit => Some("()".to_owned()),
        Value::Int(int) => {
            let raw = take(1)[0];
            Some(if int.kind().is_signed() {
                i128::from(raw).to_string()
            } else {
                u128::from(raw as u64).to_string()
            })
        }
        Value::Bool(_) => Some((take(1)[0] != 0).to_string()),
        Value::Str(_) => {
            let pair = take(2);
            let text = read_str(pair[0] as usize, pair[1] as usize);
            Some(format!("{text:?}"))
        }
        Value::Record { fields } => {
            if fields.is_empty() {
                return Some("{}".to_owned());
            }
            let mut parts = Vec::with_capacity(fields.len());
            for (name, value) in fields {
                parts.push(format!(
                    "{name} = {}",
                    decode_into(db, value, slots, read_str)?
                ));
            }
            Some(format!("{{ {} }}", list(parts)))
        }
        Value::Array(values) => {
            let mut parts = Vec::with_capacity(values.len());
            for value in values {
                parts.push(decode_into(db, value, slots, read_str)?);
            }
            Some(format!("[{}]", list(parts)))
        }
        // A variant-typed (tag-free) value: its payload, in order.
        Value::Tuple(values) => {
            let mut parts = Vec::with_capacity(values.len());
            for value in values {
                parts.push(decode_into(db, value, slots, read_str)?);
            }
            Some(format!("({})", list(parts)))
        }
        // A TAGGED value: tag in slot 0, payload element `j` at its
        // column offset. Reading it back needs the backend's column
        // layout — which is the point: a payload read that takes the
        // wrong number of slots shows up as a wrong rendering here, not
        // only through whatever the program happened to print. Decoding
        // the result independently, rather than only comparing stdout, is
        // what makes a payload-width bug visible at all.
        Value::Variant {
            decl,
            index,
            name,
            payload,
        } => {
            // A generic enum's arguments are not carried on the value, so
            // its layout cannot be reconstructed from here.
            let generic = hir::item_data(db, decl.to_id(db))
                .as_ref()
                .is_none_or(|data| !data.generics.is_empty());
            if generic {
                return None;
            }
            let ty = hir::Ty::Named(hir::NamedTy::plain(decl.clone()));
            let layout = codegen_wasm::layout::enum_layout(db, &ty).ok()?;
            let whole = take(layout.slots as usize);
            assert_eq!(
                whole[0],
                i64::from(*index),
                "the compiled value's tag names variant {index}"
            );
            let mut parts = Vec::with_capacity(payload.len());
            for (position, value) in payload.iter().enumerate() {
                let offset = *layout.columns.get(position)? as usize;
                let mut column = &whole[offset..];
                parts.push(decode_into(db, value, &mut column, read_str)?);
            }
            let head = format!("{}::{name}", decl.display_name());
            Some(if parts.is_empty() {
                head
            } else {
                format!("{head}({})", list(parts))
            })
        }
        _ => None,
    }
}

/// The whole comparison, for one program and one entry expression.
pub fn check(source: &str, entry: &str) {
    on_budget(|| check_on_budget(source, entry))
}

fn check_on_budget(source: &str, entry: &str) {
    let db = RootDatabase::default();
    let loc = prepare(&db, source, entry);
    let interpreted = interpret(&db, &loc);
    let artifact = match codegen_wasm::compile(&db, &loc) {
        Ok(artifact) => artifact,
        Err(err) => panic!("compilation refused: {}", err.message()),
    };
    let reference = match &interpreted.outcome {
        Outcome::Value(_) => reference_value(&db, &loc),
        Outcome::Trap(_) => Value::Unit,
    };
    let compiled = execute(&db, &artifact, &reference);

    assert_eq!(
        interpreted.stdout, compiled.stdout,
        "stdout differs\n  interpreter: {:?}\n  wasm:        {:?}",
        interpreted.stdout, compiled.stdout
    );
    match (&interpreted.outcome, &compiled.outcome) {
        (Outcome::Value(a), Outcome::Value(b)) => {
            // Only a GENERIC enum's value is exempt from comparison (its
            // arguments are not on the value, so its layout cannot be
            // reconstructed). Everything else must decode, so this
            // coverage cannot quietly thin out.
            if a.is_some() && !generic_enum(&db, &reference) {
                assert!(
                    b.is_some(),
                    "the compiled entry's value could not be read back \
                     (interpreter said {a:?})"
                );
            }
            if a.is_some() && b.is_some() {
                assert_eq!(a, b, "the entry expression's value differs");
            }
        }
        (Outcome::Trap(message), Outcome::Trap(expected)) => {
            assert!(
                message == expected || message.starts_with(expected.as_str()),
                "both trapped, but for different reasons\n  interpreter: {message}\n  \
                 wasm:        {expected}"
            );
            // A prefix match alone is weak: every overflow starts with
            // "arithmetic overflow: ", so a trap fired by the WRONG
            // operation still matched. The trap entry's `detail` says
            // which operation and which width the backend thinks it was —
            // check that against what the interpreter said.
            if let Some(info) = &compiled.trap {
                assert_detail(info, message);
            }
        }
        (a, b) => panic!("termination differs\n  interpreter: {a:?}\n  wasm:        {b:?}"),
    }
}

/// Cross-check a trap's `detail` against the interpreter's message.
///
/// The backend cannot know the operand VALUES an overflow message names,
/// but it does know the operation and the type — and those must agree, or
/// the compiled program trapped somewhere else than the interpreter did.
fn assert_detail(info: &codegen_wasm::TrapInfo, message: &str) {
    if info.kind != TrapKind::Overflow {
        return;
    }
    // detail: "`+` on `u8`"; message: "arithmetic overflow: `255 + 1` does
    // not fit in `u8`".
    let quoted: Vec<&str> = info.detail.split('`').collect();
    let (symbol, width) = match quoted.as_slice() {
        [_, symbol, _, width, _] => (*symbol, *width),
        _ => panic!("an overflow trap must carry an `<op>` on `<type>` detail: {info:?}"),
    };
    assert!(
        message.ends_with(&format!("does not fit in `{width}`")),
        "the compiled trap blames `{width}`, the interpreter said:\n  {message}"
    );
    let operands = message
        .split('`')
        .nth(1)
        .expect("the interpreter's overflow message quotes the operation");
    let names_operation = if symbol == "-" && !operands.contains(" - ") {
        // Negation renders as `-3`, not as `a - b`.
        operands.starts_with('-')
    } else {
        operands.contains(&format!(" {symbol} "))
    };
    assert!(
        names_operation,
        "the compiled trap blames `{symbol}`, the interpreter said:\n  {message}"
    );
}

/// Whether a value is a GENERIC enum's — the one shape the decoder cannot
/// reconstruct a layout for (the arguments do not travel on the value).
fn generic_enum(db: &RootDatabase, value: &Value) -> bool {
    let Value::Variant { decl, .. } = value else {
        return false;
    };
    hir::item_data(db, decl.to_id(db))
        .as_ref()
        .is_none_or(|data| !data.generics.is_empty())
}

/// Re-run the interpreter just to learn the entry value's SHAPE (which
/// tells the decoder how to read the result slots back).
fn reference_value(db: &RootDatabase, entry: &hir::ItemLoc) -> Value {
    let mut sink = Vec::new();
    Machine::new(db, RunMode::without_stdin(&mut sink))
        .eval_root(entry)
        .unwrap_or(Value::Unit)
}

/// Compile-only: the refusal a program must produce.
pub fn refusal(source: &str, entry: &str) -> String {
    refusal_on_stack(codegen_wasm::STACK_BUDGET, source, entry)
}

/// [`refusal`], on a thread of exactly `stack` bytes — for the tests that
/// hold the depth caps to the stack margin their docs claim.
pub fn refusal_on_stack(stack: usize, source: &str, entry: &str) -> String {
    on_stack(stack, || {
        let db = RootDatabase::default();
        let loc = prepare(&db, source, entry);
        match codegen_wasm::compile(&db, &loc) {
            Ok(_) => panic!("expected the wasm backend to refuse this program"),
            Err(CompileError::Unsupported(refusal)) => refusal.message(),
            Err(err) => err.message(),
        }
    })
}

/// The trap kind a program's compiled form records, for tests that pin
/// which classification fired.
pub fn trap_kind(source: &str, entry: &str) -> TrapKind {
    on_budget(|| {
        let db = RootDatabase::default();
        let loc = prepare(&db, source, entry);
        let artifact = codegen_wasm::compile(&db, &loc).expect("compiles");
        let run = execute(&db, &artifact, &Value::Unit);
        let Outcome::Trap(message) = run.outcome else {
            panic!("expected a trap, got {:?}", run.outcome);
        };
        let _ = message;
        run.trap.expect("a compiled trap names its entry").kind
    })
}
