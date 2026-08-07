//! The WebAssembly backend: MIR in, a self-contained `.wasm` module out.
//!
//! **No runtime.** The generated module has no garbage collector, no
//! unwinder, no scheduler and no support library. What it does have is
//! exactly what the three-layer platform model (P01) permits: platform
//! *imports* for effects (`print`), data segments for string literals and
//! const-baked statics, and `unreachable` for traps. The module IS a
//! platform instance: it exports `main`, and the host supplies the
//! effects.
//!
//! **What compilation means here.** Every reachable function is
//! monomorphized (`mono.rs`), laid out into `i64` slots (`layout.rs`) and
//! emitted as one wasm function (`emit.rs`). Trait dictionaries are
//! resolved at compile time and disappear: a bound-directed call becomes a
//! direct `call`. Nothing is dispatched dynamically anywhere.
//!
//! **Refusals, never silence.** Anything the backend cannot compile —
//! raw pointers, the heap builtins, a callee it cannot pin down — is
//! reported as a located "not supported by the wasm backend yet"
//! diagnostic naming the construct. A program either compiles to
//! something that behaves exactly like the interpreter (which the
//! differential harness checks) or is refused.

pub mod emit;
pub mod layout;
pub mod mono;
pub mod wasm;

use base_db::Db;
use hir::ItemLoc;
use rustc_hash::FxHashMap;

pub use mono::Refusal;

use crate::wasm::{ExportKind, Module, ValType};

/// A compiled module plus everything a host needs to interpret what it
/// does.
pub struct Artifact {
    pub wasm: Vec<u8>,
    /// The reason table `trap_code` indexes into.
    pub traps: Vec<TrapInfo>,
    /// How many `i64` results the exported entry point returns.
    pub entry_results: usize,
    /// How many functions monomorphization produced.
    pub instances: usize,
}

/// Why a compiled program stopped. The compiled module writes the index of
/// one of these into its exported `trap_code` global immediately before
/// executing `unreachable`, so a host can always say WHICH trap fired —
/// and compare it with what the interpreter would have reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapInfo {
    pub kind: TrapKind,
    /// The interpreter's message for this trap — in full when the backend
    /// can know it exactly, otherwise the prefix it must start with (an
    /// overflow message names the operand values, which only exist at
    /// runtime).
    pub message: String,
    /// Whether `message` is the whole message or only its beginning.
    pub exact: bool,
    /// What the backend knows about this trap beyond the message it can
    /// promise — the operation and type behind an overflow, say. Never
    /// compared against the interpreter; it exists so a `.wasm` can still
    /// explain itself.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    /// `panic("…")`.
    Panic,
    /// The ruled overflow semantics: every arithmetic operation traps
    /// rather than wrapping.
    Overflow,
    DivideByZero,
    IndexOutOfBounds,
    /// A `mir::TerminatorKind::Trap` — a diagnostic the editor already
    /// shows, executed. Includes a non-exhaustive `match` reaching an
    /// uncovered case.
    Diagnostic,
    /// A block MIR marked unreachable was entered: a compiler bug if it
    /// ever fires.
    Internal,
}

#[derive(Debug, Clone)]
pub enum CompileError {
    /// A construct this backend does not compile yet.
    Unsupported(Refusal),
    /// The entry item has nothing to run.
    NoEntry(String),
    /// A program this backend refuses for a reason that is NOT a missing
    /// feature — nothing here is waiting to be built, so it must not be
    /// reported with [`CompileError::Unsupported`]'s "yet".
    Rejected {
        message: String,
        origin: Option<(ItemLoc, hir::ExprId)>,
    },
}

impl CompileError {
    pub fn message(&self) -> String {
        match self {
            CompileError::Unsupported(refusal) => refusal.message(),
            CompileError::NoEntry(message) => message.clone(),
            CompileError::Rejected { message, .. } => message.clone(),
        }
    }

    pub fn origin(&self) -> Option<(ItemLoc, hir::ExprId)> {
        match self {
            CompileError::Unsupported(refusal) => refusal.origin.clone(),
            CompileError::NoEntry(_) => None,
            CompileError::Rejected { origin, .. } => origin.clone(),
        }
    }
}

/// The module's string data: every literal, interned by content, laid out
/// in one data segment. Two identical literals share bytes; nothing is
/// ever written to this memory at runtime.
#[derive(Debug, Default)]
pub struct Strings {
    data: Vec<u8>,
    offsets: FxHashMap<String, u32>,
}

impl Strings {
    /// Returns `(offset, length)` — the two slots a `str` value occupies.
    pub fn intern(&mut self, text: &str) -> (u32, u32) {
        let len = text.len() as u32;
        if let Some(offset) = self.offsets.get(text) {
            return (*offset, len);
        }
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(text.as_bytes());
        self.offsets.insert(text.to_owned(), offset);
        (offset, len)
    }
}

#[derive(Debug, Default)]
pub struct Traps {
    entries: Vec<TrapInfo>,
}

impl Traps {
    pub fn intern(&mut self, kind: TrapKind, message: String, exact: bool) -> u32 {
        self.intern_detailed(kind, message, exact, String::new())
    }

    pub fn intern_detailed(
        &mut self,
        kind: TrapKind,
        message: String,
        exact: bool,
        detail: String,
    ) -> u32 {
        let entry = TrapInfo {
            kind,
            message,
            exact,
            detail,
        };
        if let Some(index) = self.entries.iter().position(|existing| *existing == entry) {
            return index as u32;
        }
        self.entries.push(entry);
        (self.entries.len() - 1) as u32
    }
}

/// Declare one wasm import per HOST IMPORT the program actually calls, and
/// answer the name -> function-index map the emitter calls through.
///
/// Reachability is monomorphization's answer, not a scan of the file: an
/// import nothing calls costs the module nothing, which is the same
/// dead-code rule every other item follows. The module name is `must` and
/// the field name is the declaring `static`'s own name — the declaration is
/// the whole contract, and there is no override surface to disagree with it.
///
/// CALL THIS AFTER every builtin import and before the first defined
/// function: the module's existing imports are the names it reserves, and
/// imports own the low function-index space.
fn collect_extern_imports(
    mono: &mut mono::Mono<'_>,
    module: &mut Module,
) -> Result<FxHashMap<String, u32>, CompileError> {
    let order = mono.order.clone();
    let mut indices: FxHashMap<String, u32> = FxHashMap::default();
    // Deterministic: instance registration order, then block order within
    // an instance — the same discipline `Mono::register` uses, so a module
    // is byte-identical across runs.
    let mut seen: Vec<(String, Vec<ValType>, Vec<ValType>)> = Vec::new();
    for key in &order {
        let analysis = mono.analyze(key);
        let mut sites: Vec<(usize, &mono::CallTarget)> = analysis
            .calls
            .iter()
            .map(|(block, target)| (block.into_raw().into_u32() as usize, target))
            .collect();
        sites.sort_by_key(|(block, _)| *block);
        for (_, target) in sites {
            if let mono::CallTarget::Extern {
                name,
                params,
                results,
                decl,
            } = target
            {
                // A name the compiler already imports under is RESERVED.
                // Two imports of one `(module, field)` is a module with two
                // answers to the same question, and an engine resolves both
                // — so this is a silent-wrong-answer class, not a missing
                // feature. The reserved set is whatever the module imports
                // ALREADY, so a future sibling of `print` reserves itself by
                // being declared above this function's one call site — which
                // it must be anyway, since imports own the low index space.
                if module.imports_func(IMPORT_MODULE, name) {
                    return Err(CompileError::Rejected {
                        message: format!(
                            "an import may not be named `{name}`: this backend already \
                             imports `{IMPORT_MODULE}.{name}` for the builtin of that name, \
                             and a module cannot import one name twice"
                        ),
                        origin: Some(decl.clone()),
                    });
                }
                if !seen.iter().any(|(known, _, _)| known == name) {
                    seen.push((name.clone(), params.clone(), results.clone()));
                }
            }
        }
    }
    for (name, params, results) in seen {
        let ty = module.func_type(params, results);
        let index = module.import_func(IMPORT_MODULE, &name, ty);
        indices.insert(name, index);
    }
    Ok(indices)
}

/// The name of the exported entry point. P01: the platform owns `main`.
pub const ENTRY_EXPORT: &str = "main";
/// The module effects are imported from.
pub const IMPORT_MODULE: &str = "must";

/// The native stack [`compile`] must be run on.
///
/// [`compile`] walks the call graph recursively — `mono::Mono::register`
/// descends into every callee and `mono::Mono::analyze` nests its own
/// recursion inside that walk — so its depth caps (`MAX_PATH_DEPTH`,
/// `MAX_ANALYSIS_DEPTH` in `mono.rs`) are only honest if the stack they run
/// on is known. They are calibrated to fit this budget with at least a 2x
/// margin (see `MAX_PATH_DEPTH`), and every caller runs [`compile`] on a
/// thread at least this large — this crate's tests do, and so does
/// the CLI's `compile` subcommand. `must-lsp`'s `pool.rs` is the
/// precedent — the same 8 MiB for the same kind of reason, a recursive
/// walk whose cap assumes a stack.
///
/// [`compile`] cannot provide the thread itself: `salsa::Database` is
/// `Send` but not `Sync`, so a `&dyn Db` cannot cross into a scoped
/// thread.
pub const STACK_BUDGET: usize = 8 * 1024 * 1024;

/// Compile the program whose entry point is `entry`'s initializer — the
/// same body the runner evaluates, so `must-lsp run` and `must-lsp
/// compile` execute the same code by construction.
///
/// Run this on a thread of at least [`STACK_BUDGET`] bytes: the walk over
/// the call graph is recursive, and the depth caps that keep it finite are
/// calibrated against that budget.
///
/// Monomorphization (`mono.rs`) is deliberately not a salsa query yet: it
/// runs once, whole-program, right here — the right shape for a batch CLI
/// that compiles one entry point and exits, and the wrong one for an
/// editor. Should this backend ever need to be incremental, the query
/// would key on `codegen_wasm::mono::InstanceKey`, not on `eval::Instance`
/// (whose const-args-only key omits the type-argument widths a backend
/// needs; see that type's doc).
pub fn compile(db: &dyn Db, entry: &ItemLoc) -> Result<Artifact, CompileError> {
    let lowered = mir::mir_lowered(db, entry.to_id(db));
    let Some(root) = lowered.root else {
        return Err(CompileError::NoEntry(format!(
            "`{}` has no value to run",
            entry.display_name()
        )));
    };

    let mut mono = mono::Mono::new(db);
    let key = mono::InstanceKey {
        func: mono::FnRef {
            value: eval::FnValue {
                item: entry.clone(),
                body: root,
                const_args: Vec::new(),
            },
            type_args: Vec::new(),
        },
        arg_statics: Vec::new(),
    };
    mono.register(&key).map_err(CompileError::Unsupported)?;

    let mut module = Module::new();
    let print_ty = module.func_type(vec![ValType::I32, ValType::I32], Vec::new());
    let print = module.import_func(IMPORT_MODULE, "print", print_ty);
    // Host imports, declared BEFORE any defined function: imports own the
    // low function-index space, so the whole set has to be known here. It
    // is — monomorphization has already walked every reachable body and
    // resolved each call site, signature included.
    //
    // `print` stays hand-written above — where `collect_extern_imports`
    // sees it, and so reserves its name — rather than becoming an import
    // declaration in every program: it is a builtin, and `str`'s (offset,
    // length) pair is a platform ABI this backend decided by accident (see
    // `platform-codegen-and-tooling.md`) — not something a user-written
    // signature can spell today.
    let extern_imports = collect_extern_imports(&mut mono, &mut module)?;
    let str_eq_ty = module.func_type(
        vec![ValType::I64, ValType::I64, ValType::I64, ValType::I64],
        vec![ValType::I64],
    );
    let str_eq = module.declare_func(str_eq_ty, "must.str_eq".to_owned());
    let trap_global = module.global(ValType::I32, true, -1);
    // Where a runtime-computed `panic` message lives when one fires.
    let panic_offset = module.global(ValType::I64, true, 0);
    let panic_len = module.global(ValType::I64, true, 0);

    let mut strings = Strings::default();
    let mut traps = Traps::default();
    let mut func_indices = Vec::with_capacity(mono.order.len());

    // Two passes: every function index must exist before any body that
    // calls it is written.
    let order = mono.order.clone();
    {
        let mut emitter = emit::Emitter {
            mono: &mut mono,
            strings: &mut strings,
            traps: &mut traps,
            print,
            str_eq,
            trap_global,
            panic_offset,
            panic_len,
            func_indices: &[],
            extern_imports: &extern_imports,
        };
        let mut signatures = Vec::with_capacity(order.len());
        for key in &order {
            signatures.push(emitter.signature(key).map_err(CompileError::Unsupported)?);
        }
        for (key, (params, results)) in order.iter().zip(signatures) {
            let ty = module.func_type(params, results);
            let name = emitter.mono.mangle(key);
            func_indices.push(module.declare_func(ty, name));
        }
    }
    let entry_results = {
        let mut emitter = emit::Emitter {
            mono: &mut mono,
            strings: &mut strings,
            traps: &mut traps,
            print,
            str_eq,
            trap_global,
            panic_offset,
            panic_len,
            func_indices: &func_indices,
            extern_imports: &extern_imports,
        };
        let mut bodies = Vec::with_capacity(order.len());
        for key in &order {
            bodies.push(emitter.emit(key).map_err(CompileError::Unsupported)?);
        }
        for (index, body) in func_indices.iter().zip(bodies) {
            module.set_body(*index, body);
        }
        emitter
            .signature(&key)
            .map_err(CompileError::Unsupported)?
            .1
            .len()
    };

    module.set_body(str_eq, emit::str_eq_body());

    // One page holds 64KiB of string data; grow only if a program needs
    // more. Nothing else ever touches memory.
    let pages = (strings.data.len() as u32).div_ceil(65_536).max(1);
    module.memory(pages);
    if !strings.data.is_empty() {
        module.data_segment(0, strings.data.clone());
    }
    module.export("memory", ExportKind::Memory, 0);
    module.export("trap_code", ExportKind::Global, trap_global);
    module.export("panic_message_offset", ExportKind::Global, panic_offset);
    module.export("panic_message_len", ExportKind::Global, panic_len);
    module.export(ENTRY_EXPORT, ExportKind::Func, func_indices[0]);
    module.custom_section("must.traps", encode_traps(&traps.entries));

    Ok(Artifact {
        wasm: module.finish(),
        traps: traps.entries,
        entry_results,
        instances: order.len(),
    })
}

/// The trap table, as a custom section: a disassembler (or a host with no
/// access to this crate) can still say which trap fired.
fn encode_traps(entries: &[TrapInfo]) -> Vec<u8> {
    let mut out = Vec::new();
    wasm::uleb(&mut out, entries.len() as u64);
    for entry in entries {
        out.push(match entry.kind {
            TrapKind::Panic => 0,
            TrapKind::Overflow => 1,
            TrapKind::DivideByZero => 2,
            TrapKind::IndexOutOfBounds => 3,
            TrapKind::Diagnostic => 4,
            TrapKind::Internal => 5,
        });
        out.push(u8::from(entry.exact));
        wasm::uleb(&mut out, entry.message.len() as u64);
        out.extend_from_slice(entry.message.as_bytes());
        wasm::uleb(&mut out, entry.detail.len() as u64);
        out.extend_from_slice(entry.detail.as_bytes());
    }
    out
}
