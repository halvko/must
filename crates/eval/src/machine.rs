//! The evaluation core. Executes MIR bodies on an explicit, heap-allocated
//! frame stack — one [`Machine::step`] call advances a single statement or
//! terminator, which is what lets a debugger pause between any two of them
//! and walk the stack. Parameterized by a [`Mode`] that decides what impure
//! builtins do; const-ness is tracked by the machine itself (`const_depth`):
//! any item being forced is a const context regardless of the driving mode.

use base_db::Db;
use hir::{Builtin, ConstLegality, ExprId, IntKind, ItemLoc, Ty};
use la_arena::ArenaMap;
use mir::{
    AggregateKind, BodyId, Const, LocalData, LocalId, MirBody, MirLowered, Operand, ProjElem,
    Rvalue, StatementKind, TerminatorKind,
};
use rustc_hash::FxHashMap;

use crate::{
    AllocId, BorrowTag, EvalError, EvalErrorKind, EvalNote, FnValue, GenericArgValue, Instance,
    PathElem, Provenance, Value,
};

/// What the machine does at its impure edges. [`ConstMode`] refuses;
/// the runner's mode performs the I/O.
pub trait Mode {
    /// `print(text)` outside any const context. Implementations write
    /// `text` verbatim — adding a separator here would make `print` a
    /// line-writer, which it deliberately is not.
    fn print(&mut self, text: &str) -> Result<(), EvalError>;

    /// `read_line()` outside any const context — `print`'s input twin.
    /// Implementations move bytes only: they hand back the next line as
    /// the source spells it, terminator INCLUDED, and `None` at genuine
    /// end-of-input. What counts as a line for the language — terminator
    /// stripped, a blank line still a line, an unterminated final line
    /// still a line (P04) — is `Machine::builtin_call`'s business, so
    /// every host obeys one rule instead of re-deriving it.
    fn read_line(&mut self) -> Result<Option<String>, EvalError>;

    /// The host primitive behind `extern fn read(buf, len) -> isize` — the
    /// machine-shaped byte read, and the layer `read_line` would be built on
    /// if it were library code.
    ///
    /// Appends AT MOST `len` bytes to `out` and answers what POSIX `read`
    /// answers: the number of bytes appended, `0` at end of input, or a
    /// NEGATIVE `-errno`. Note what is deliberately not promised — a short
    /// read is not end of input, and the caller has no way to tell the two
    /// apart except by the count being zero. That is the boundary's shape,
    /// not the interpreter's simplification.
    ///
    /// An `Err` here is an *interpreter* failure (there was no errno to
    /// report), not the program's; the program's errors ride the return
    /// value, where library code can see them.
    fn read(&mut self, out: &mut Vec<u8>, len: usize) -> Result<i64, EvalError>;
}

/// The driver behind [`crate::const_value`]: everything is a const context.
pub struct ConstMode;

impl Mode for ConstMode {
    /// Unreachable in practice: [`Machine::for_const`] starts at
    /// `const_depth = 1`, so the const fence below answers first. It is the
    /// last line of the same defense, so it says the same sentence.
    fn print(&mut self, _text: &str) -> Result<(), EvalError> {
        Err(EvalError {
            kind: EvalErrorKind::NotConst,
            message: hir::diag::side_effect_call_in_const("print"),
            origin: None,
            notes: Vec::new(),
        })
    }

    /// Unreachable in practice, like `print`'s arm above, and the same
    /// sentence for the same reason.
    fn read_line(&mut self) -> Result<Option<String>, EvalError> {
        Err(EvalError {
            kind: EvalErrorKind::NotConst,
            message: hir::diag::side_effect_call_in_const("read_line"),
            origin: None,
            notes: Vec::new(),
        })
    }

    /// Unreachable in practice, like the arms above: [`Machine::for_const`]
    /// starts at `const_depth = 1`, so `extern_call` refuses before any
    /// host is asked. It is the last line of the same defense, so it says
    /// the same sentence the squiggle did.
    fn read(&mut self, _out: &mut Vec<u8>, _len: usize) -> Result<i64, EvalError> {
        Err(EvalError {
            kind: EvalErrorKind::NotConst,
            message: hir::diag::extern_call_in_const("read"),
            origin: None,
            notes: Vec::new(),
        })
    }
}

/// Run mode for the CLI and the debug adapter: `print` writes its argument
/// and nothing else — no newline is appended (P03). `print` is a write,
/// not a line; a program that wants a line break emits one itself with the
/// `\n` escape. `read_line` (P04) reads `input` — real, locked,
/// line-buffered stdin for the CLI; an injectable in-memory reader for
/// tests; [`std::io::Empty`] for the hosts that have no stdin to offer (the
/// debug adapter and the editor's run lens, each of which documents why).
pub struct RunMode<W: std::io::Write, R: std::io::BufRead> {
    pub out: W,
    pub input: R,
}

impl<W: std::io::Write> RunMode<W, std::io::Empty> {
    /// A host with no stdin to offer — the debug adapter and the editor's
    /// run lens. `read_line` reports `End` on the very first call rather
    /// than blocking on input that can never arrive (P04); the same shape
    /// a test that injects nothing wants.
    pub fn without_stdin(out: W) -> Self {
        RunMode {
            out,
            input: std::io::empty(),
        }
    }
}

impl<W: std::io::Write, R: std::io::BufRead> Mode for RunMode<W, R> {
    fn print(&mut self, text: &str) -> Result<(), EvalError> {
        write!(self.out, "{text}").map_err(|err| EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!("I/O error in `print`: {err}"),
            origin: None,
            notes: Vec::new(),
        })
    }

    fn read_line(&mut self) -> Result<Option<String>, EvalError> {
        // `out` is line-buffered and never flushed per call (P03), so an
        // unterminated prompt would still be sitting in the buffer while
        // this blocks. Flush first, or `print("name? "); read_line()` waits
        // at a terminal that shows nothing.
        self.out.flush().map_err(|err| EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!("I/O error flushing output before `read_line`: {err}"),
            origin: None,
            notes: Vec::new(),
        })?;
        let mut line = String::new();
        // A failed read — input that is not UTF-8 included — crashes the
        // program (P04): `ReadLineResult` has no error arm to carry it.
        let read = self.input.read_line(&mut line).map_err(|err| EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!("I/O error in `read_line`: {err}"),
            origin: None,
            notes: Vec::new(),
        })?;
        // `BufRead::read_line` reports 0 bytes read exactly at genuine EOF
        // (never for a blank line, which is 1 byte: the newline itself).
        Ok(if read == 0 { None } else { Some(line) })
    }

    fn read(&mut self, out: &mut Vec<u8>, len: usize) -> Result<i64, EvalError> {
        if len == 0 {
            // A zero-length request reads nothing and means nothing: `0`
            // here is NOT end of input, which is why the boundary's EOF
            // rule is stated for a nonzero-len request only.
            return Ok(0);
        }
        // `Read::read` is allowed to return fewer bytes than asked for, and
        // that IS the host contract — no loop here to paper over it, because
        // library code above must be written to expect short reads anyway.
        let start = out.len();
        out.resize(start + len, 0);
        let result = std::io::Read::read(&mut self.input, &mut out[start..]);
        match result {
            Ok(n) => {
                out.truncate(start + n);
                Ok(i64::try_from(n).unwrap_or(i64::MAX))
            }
            Err(err) => {
                out.truncate(start);
                // The error channel is the RETURN VALUE, not a trap — that
                // is the whole point of a machine-shaped boundary, and it is
                // what makes the lifting wrapper's `Err` arm reachable. Only
                // an error with no errno to report has nowhere to go.
                match err.raw_os_error() {
                    Some(errno) if errno > 0 => Ok(-i64::from(errno)),
                    _ => Err(EvalError {
                        kind: EvalErrorKind::Runtime,
                        message: format!("I/O error in the host `read`: {err}"),
                        origin: None,
                        notes: Vec::new(),
                    }),
                }
            }
        }
    }
}

/// Frames live on the heap, so this guards against runaway recursion, not
/// against Rust stack overflow (stepping is iterative; nested const forcing
/// does recurse on the Rust stack, bounded by [`MAX_CONST_DEPTH`]).
const MAX_FRAMES: usize = 10_000;

/// How deeply items may force each other's initializers. Each level is a
/// Rust-stack recursion (`force_item` → `eval_root` → `step` → …), so this
/// is a real stack bound, kept at the old frame limit's 128. The `forcing`
/// cycle check bounds same-item cycles only; this bounds distinct chains.
const MAX_CONST_DEPTH: usize = 128;

/// Statement/terminator budget for const contexts: a salsa query must
/// terminate even on adversarial input. Run-mode code outside initializers
/// is not fueled — a long-running program is the user's business.
const CONST_FUEL: u64 = 1_000_000;

/// The tracked-uninit read message (ruled A04): one text for every gate —
/// value reads that would carry poison out of memory, and path steps into
/// never-written structure.
const UNINIT_READ: &str = "read of uninitialized memory — this element was never written";

/// One Must call frame.
pub struct Frame {
    pub loc: ItemLoc,
    pub body: BodyId,
    /// Unique per frame *instance* within a machine — two calls of the same
    /// function are distinguishable (the debugger keys breakpoint arrivals
    /// on this).
    pub serial: u64,
    block: mir::BlockId,
    /// Index of the next statement to execute in `block`; past the end
    /// means the terminator is next.
    statement: usize,
    /// The instance this frame executes under: the evaluated const args of
    /// the fn value that was called (dense const-param order — see
    /// [`FnValue::const_args`]), threaded into compile-time bodies forced
    /// from here (`const` blocks, const arguments) so `ConstParam`
    /// operands resolve anywhere inside the generic body. Empty outside
    /// generic code.
    const_args: Vec<Value>,
    locals: ArenaMap<LocalId, Value>,
    /// The two-tier promotion map: locals whose address was taken, and the
    /// allocation each one moved into. Populated lazily at the first
    /// `.&raw` of a local (MIR statically marks the candidates — see
    /// `LocalData::addressable`); once promoted, every read/write of the
    /// local goes through the allocation, so writes through pointers and
    /// direct uses see one place. Empty in pointer-free bodies — the
    /// `is_empty` fast path keeps those executing exactly as before. The
    /// frame's allocations die (become detectably dangling) when the
    /// frame returns: `x.&raw` is the address of that frame slot, for that
    /// frame's lifetime.
    promoted: FxHashMap<LocalId, AllocId>,
    /// Caller linkage: the local the return value lands in, and the block
    /// the caller resumes at (`None` = the callee's type promised to
    /// diverge). `None` overall marks the bottom frame of an execution.
    return_to: Option<(LocalId, Option<mir::BlockId>)>,
}

/// What one [`Machine::step`] did.
pub enum StepEvent {
    Progress,
    /// The bottom frame returned: the execution's result.
    Done(Value),
}

/// One abstract-memory allocation: a TYPED value with liveness and
/// writability — Miri's skeleton without Miri's bytes. Pointers address
/// allocations by [`AllocId`] plus an element-granular path; there are no
/// byte offsets and no integer addresses anywhere, which is what makes the
/// evaluator a UB detector by construction (a dead allocation is
/// recognizably dead forever — ids are never reused).
struct Allocation {
    value: Value,
    /// `false` once the owning frame returned (a local) or the allocation
    /// was freed (`dealloc_array` on heap): any deref then is detected UB.
    live: bool,
    /// `false` for statics: any write through a pointer into one is
    /// detected UB (unreachable from well-typed code — statics only hand
    /// out shared `.&raw` — but the memory model enforces it regardless).
    writable: bool,
    kind: AllocKind,
    /// The allocation's birth site — recorded for heap allocations (the
    /// `alloc_array` call), `None` for the rest. The blame currency of the
    /// heap-UB diagnostics: double free / use-after-free / dealloc
    /// mismatches attach an "allocated here" note pointing at it.
    origin: Option<(ItemLoc, ExprId)>,
}

impl Allocation {
    /// The "allocated here" note heap-UB errors carry — empty when no
    /// birth site was recorded (non-heap allocations).
    fn allocated_here(&self) -> Vec<EvalNote> {
        self.origin
            .as_ref()
            .map(|origin| {
                vec![EvalNote {
                    message: "allocated here".to_owned(),
                    origin: Some(origin.clone()),
                }]
            })
            .unwrap_or_default()
    }
}

/// What an allocation backs — decides the UB message's wording.
#[derive(Clone, Copy)]
enum AllocKind {
    /// An address-taken local (a frame slot promoted into memory).
    Local,
    /// A `static` item's one place.
    Static,
    /// An `alloc_array` heap allocation: dies at `dealloc_array` (the
    /// frame-pop-kills-locals mechanism applied to heap — dangling
    /// detection needed zero new logic).
    Heap,
    /// The reserved never-live allocation behind the `dangling` builtin,
    /// minted at most once per machine. Never `live`, so every deref is
    /// detected UB, with its own wording.
    Dangling,
}

pub struct Machine<'db, M> {
    db: &'db dyn Db,
    pub mode: M,
    frames: Vec<Frame>,
    /// Items currently being forced (cycle detection), innermost last.
    forcing: Vec<ItemLoc>,
    /// Memoized const values — failures too, or a failing item would be
    /// re-evaluated at every use site.
    forced: FxHashMap<ItemLoc, Result<Value, EvalError>>,
    /// Compile-time bodies (`const { … }` blocks and turbofish const
    /// arguments) currently being forced (cycle detection), innermost last
    /// — the block-level twin of `forcing`. Keyed per *instance*: the same
    /// block forced under two instantiations is two evaluations, not a
    /// cycle.
    forcing_blocks: Vec<(Instance, BodyId)>,
    /// Per-run memo for compile-time bodies, keyed by owning instance
    /// (item + the const-param values the body may read — TR06's applicative
    /// identity) and lowered body: a const block inside a hot function
    /// evaluates once per machine run *per instance*. Failures memoize
    /// too, like `forced`.
    forced_blocks: FxHashMap<(Instance, BodyId), Result<Value, EvalError>>,
    /// How many const-block bodies were actually executed (memo misses) —
    /// observable instrumentation for the memoization guarantee.
    const_block_evaluations: u64,
    /// > 0 while inside a static initializer: the const context marker.
    const_depth: usize,
    const_fuel: u64,
    next_frame_serial: u64,
    /// The typed abstract memory: every address-taken local and every
    /// `.&raw`-mentioned static lives here; nothing else ever does
    /// (pay-for-what-you-use, applied to the interpreter itself).
    memory: FxHashMap<AllocId, Allocation>,
    /// `static` items' one place each, minted read-only on the first
    /// `S.&raw` — so `S.&raw == S.&raw` holds across mentions
    /// (static=identity, observable). Plain mentions of `S` keep cloning
    /// the `forced` memo, unchanged.
    static_allocs: FxHashMap<ItemLoc, AllocId>,
    /// Every aliasing-tree node minted this run, indexed by [`BorrowTag`].
    /// One flat table rather than a tree per allocation: nodes are never
    /// removed (a disabled node stays, exactly as a freed [`AllocId`]
    /// stays), so an index is a permanent identity and the parent chain
    /// IS the tree.
    borrow_nodes: Vec<BorrowNode>,
    /// The root node of each allocation a safe borrow has ever covered.
    /// Created on demand — a program with no safe borrows never allocates
    /// one, so raw-only programs pay nothing and behave exactly as before.
    borrow_roots: FxHashMap<AllocId, BorrowTag>,
    /// Which nodes of each allocation an access still has to CONSIDER,
    /// split by what can still happen to them.
    ///
    /// Three things keep the scan small, and each is a property of the
    /// state machine rather than a heuristic. An access only ever affects
    /// nodes of its OWN allocation, so the map is per-allocation. A READ
    /// can only demote `Unique` to `Frozen`, so it never has to look at a
    /// node that is already settled. And a DISABLED node is dropped
    /// entirely: disabling is permanent and transitive, so it can neither
    /// change state again nor shield a descendant.
    ///
    /// Nodes stay in [`Self::borrow_nodes`] forever regardless — a tag
    /// must keep naming its node, exactly as a freed `AllocId` keeps
    /// naming its allocation. Only the SCAN lists shrink.
    ///
    /// Without this a loop that borrows per iteration is quadratic in both
    /// flavors: every access rescans every borrow the program ever made.
    borrow_nodes_of: FxHashMap<AllocId, AllocNodes>,
    /// The reserved never-live allocation behind `dangling`, minted once
    /// per machine on the first call — every `dangling()` result compares
    /// equal, and every deref of one is detected UB.
    dangling_alloc: Option<AllocId>,
    next_alloc: u64,
}

impl<'db> Machine<'db, ConstMode> {
    pub fn for_const(db: &'db dyn Db) -> Machine<'db, ConstMode> {
        let mut machine = Machine::new(db, ConstMode);
        // The whole machine lives inside one `const { … }`.
        machine.const_depth = 1;
        machine
    }
}

impl<'db, M: Mode> Machine<'db, M> {
    pub fn new(db: &'db dyn Db, mode: M) -> Machine<'db, M> {
        Machine {
            db,
            mode,
            frames: Vec::new(),
            forcing: Vec::new(),
            forced: FxHashMap::default(),
            forcing_blocks: Vec::new(),
            forced_blocks: FxHashMap::default(),
            const_block_evaluations: 0,
            const_depth: 0,
            const_fuel: CONST_FUEL,
            next_frame_serial: 0,
            memory: FxHashMap::default(),
            static_allocs: FxHashMap::default(),
            borrow_nodes: Vec::new(),
            borrow_roots: FxHashMap::default(),
            borrow_nodes_of: FxHashMap::default(),
            dangling_alloc: None,
            next_alloc: 0,
        }
    }

    /// The (memoized) const value of a top-level item. Always a const
    /// context, whichever mode drives the machine.
    pub fn force_item(&mut self, loc: ItemLoc) -> Result<Value, EvalError> {
        if let Some(result) = self.forced.get(&loc) {
            return result.clone();
        }
        if self.forcing.contains(&loc) {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!("cycle detected while evaluating `{}`", loc.display_name()),
                origin: root_origin(self.db, &loc),
                notes: Vec::new(),
            });
        }
        if self.const_depth >= MAX_CONST_DEPTH {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!("constant evaluation exceeded {MAX_CONST_DEPTH} nested items"),
                origin: root_origin(self.db, &loc),
                notes: Vec::new(),
            });
        }
        self.forcing.push(loc.clone());
        self.const_depth += 1;
        // Forcing runs on its own (swapped-in) frame stack — a paused
        // debugger never sees compile-time frames — and gets its own fuel
        // budget: `const_value(B)` must give the same answer whether B is
        // queried directly or forced from inside another item's evaluation.
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_fuel = std::mem::replace(&mut self.const_fuel, CONST_FUEL);
        let result = self
            .eval_root(&loc)
            .and_then(|value| self.check_const_escape(value, root_origin(self.db, &loc)));
        self.frames = saved_frames;
        self.const_fuel = saved_fuel;
        self.const_depth -= 1;
        self.forcing.pop();
        self.forced.insert(loc, result.clone());
        result
    }

    /// The v1 const-escape rule: **pointers do not escape const
    /// evaluation.** A memoized compile-time result carrying a
    /// `Value::Ptr` is refused — an [`AllocId`] is per-machine-run, so the
    /// memo would be meaningless to any later run (and the pointee — a
    /// promoted local of an already-popped frame — is dangling anyway).
    /// Pointers may live and die *inside* a const computation freely;
    /// const-built pointer-carrying values (a const `Vec`) wait for an
    /// interning design.
    fn check_const_escape(
        &self,
        value: Value,
        origin: Option<(ItemLoc, ExprId)>,
    ) -> Result<Value, EvalError> {
        if value.contains_ptr() {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: "a pointer cannot leave compile-time evaluation".to_owned(),
                origin,
                notes: Vec::new(),
            });
        }
        Ok(value)
    }

    /// The (per-run memoized) value of a compile-time body — a `const
    /// { … }` block or a turbofish const argument; `body` is one of `loc`'s
    /// lowered bodies. `const_env` is the enclosing instance's const-param
    /// values (empty outside generic code): the body may read `ConstParam`
    /// operands, so both the executing frame and the memo key carry it —
    /// the same block under two instantiations is two values. Like
    /// [`Self::force_item`], forcing is always a const context, whichever
    /// mode drives the machine.
    pub fn force_const_block(
        &mut self,
        loc: &ItemLoc,
        body: BodyId,
        const_env: Vec<Value>,
    ) -> Result<Value, EvalError> {
        let instance = Instance {
            item: loc.clone(),
            args: const_env
                .iter()
                .cloned()
                .map(GenericArgValue::Const)
                .collect(),
        };
        let key = (instance, body);
        if let Some(result) = self.forced_blocks.get(&key) {
            return result.clone();
        }
        if self.forcing_blocks.contains(&key) {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!(
                    "cycle detected while evaluating a `const` block in `{}`",
                    loc.display_name()
                ),
                origin: self.const_block_origin(loc, body),
                notes: Vec::new(),
            });
        }
        self.forcing_blocks.push(key.clone());
        self.const_depth += 1;
        self.const_block_evaluations += 1;
        // Same isolation as forcing an item: own (swapped-in) frame stack,
        // own fuel budget — the block's value must not depend on who forced
        // it first.
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_fuel = std::mem::replace(&mut self.const_fuel, CONST_FUEL);
        let result = self
            .push_frame(loc.clone(), body, Vec::new(), const_env, None)
            .and_then(|()| self.run_to_done())
            .and_then(|value| self.check_const_escape(value, self.const_block_origin(loc, body)));
        self.frames = saved_frames;
        self.const_fuel = saved_fuel;
        self.const_depth -= 1;
        self.forcing_blocks.pop();
        self.forced_blocks.insert(key, result.clone());
        result
    }

    /// How many `const { … }` block bodies this machine actually executed
    /// (memo misses): the observable face of per-run memoization.
    pub fn const_block_evaluations(&self) -> u64 {
        self.const_block_evaluations
    }

    /// The `const` block (or const argument) expression `body` was lowered
    /// from — the origin for errors about the block as a whole (cycles).
    fn const_block_origin(&self, loc: &ItemLoc, body: BodyId) -> Option<(ItemLoc, ExprId)> {
        let lowered = self.lowered(loc);
        let expr = lowered
            .const_blocks
            .iter()
            .chain(&lowered.const_args)
            .find_map(|&(expr, b)| (b == body).then_some(expr))?;
        Some((loc.clone(), expr))
    }

    /// Execute an item's root body to completion — for [`Self::force_item`],
    /// and for the runner's entry item (at `const_depth` 0, where `print`
    /// is legal).
    pub fn eval_root(&mut self, loc: &ItemLoc) -> Result<Value, EvalError> {
        self.start(loc)?;
        // Run-to-completion callers don't inspect crash state.
        self.run_to_done()
    }

    /// Push the bottom frame of an execution without running it — the
    /// debugger's entry, paired with [`Self::step`].
    pub fn start(&mut self, loc: &ItemLoc) -> Result<(), EvalError> {
        let Some(root) = self.lowered(loc).root else {
            // Broken source; the parse errors carry the diagnostic.
            return Err(EvalError {
                kind: EvalErrorKind::Trap,
                message: format!("`{}` has no value", loc.display_name()),
                origin: None,
                notes: Vec::new(),
            });
        };
        // An item root executes outside any generic binder (a generic
        // item's root just constructs its fn value), so no const env.
        self.push_frame(loc.clone(), root, Vec::new(), Vec::new(), None)
    }

    /// The live call stack, bottom first. On an `Err` from [`Self::step`]
    /// the frames stay put, so a debugger can inspect the crash site.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Provenance of what `frames()[index]` executes next (its statement or
    /// terminator) — the debugger's "current line" for that frame.
    pub fn frame_origin(&self, index: usize) -> Option<(ItemLoc, ExprId)> {
        let frame = self.frames.get(index)?;
        let body = &self.lowered(&frame.loc).bodies[frame.body];
        let block = &body.blocks[frame.block];
        let origin = match block.statements.get(frame.statement) {
            Some(statement) => statement.origin,
            None => block.terminator.origin,
        };
        Some((frame.loc.clone(), origin))
    }

    /// What `frames()[index]` executes next, at machine granularity: the
    /// block and the statement index (past the end = the terminator) its
    /// next [`Self::step`] advances. Distinct MIR statements can share a
    /// source position; a debugger tells them — and whether execution
    /// moved — apart by this, not by positions.
    pub fn frame_step_point(&self, index: usize) -> Option<(mir::BlockId, usize)> {
        let frame = self.frames.get(index)?;
        Some((frame.block, frame.statement))
    }

    /// The user-named locals of a frame that currently hold values, with
    /// their declared MIR types, in declaration order (shadowing repeats a
    /// name; later wins). A frame executing under a generic instance
    /// additionally lists the binder's const params first (`N = 3` in
    /// `rep::<3>`'s frame) — they read like locals in the source, so the
    /// debugger shows them like locals; locals declared later shadow them,
    /// consistent with the name resolution order. Their type is
    /// reconstructed from the value (the declared `TypeRef` would need a
    /// lowering context this debugger surface doesn't warrant).
    pub fn frame_named_locals(&self, index: usize) -> Vec<(String, hir::Ty, Value)> {
        let Some(frame) = self.frames.get(index) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if !frame.const_args.is_empty() {
            let generics = hir::item_data(self.db, frame.loc.to_id(self.db))
                .as_ref()
                .map(|data| data.generics.clone())
                .unwrap_or_default();
            let mut values = frame.const_args.iter();
            for param in &generics {
                let hir::item_tree::GenericParamKind::Const(_) = param.kind else {
                    continue;
                };
                let Some(value) = values.next() else {
                    break;
                };
                if !param.name.is_empty() {
                    out.push((param.name.clone(), value_ty(value), value.clone()));
                }
            }
        }
        let body = &self.lowered(&frame.loc).bodies[frame.body];
        out.extend(body.locals.iter().filter_map(|(id, data)| {
            let name = data.name.clone()?;
            // A promoted (address-taken) local's current value lives in
            // its allocation, not the frame map.
            let value = match frame.promoted.get(&id) {
                Some(alloc) => self.memory.get(alloc)?.value.clone(),
                None => frame.locals.get(id)?.clone(),
            };
            Some((name, data.ty.clone(), value))
        }));
        out
    }

    /// Call a function value with already-evaluated arguments, to
    /// completion, on its own stack — the paused frames are untouched. The
    /// debug console's evaluate uses this to run expressions against a
    /// frame's locals.
    pub fn call_value(&mut self, f: FnValue, args: Vec<Value>) -> Result<Value, EvalError> {
        let saved = std::mem::take(&mut self.frames);
        let result = self
            .push_frame(f.item, f.body, args, f.const_args, None)
            .and_then(|()| self.run_to_done());
        self.frames = saved;
        result
    }

    fn run_to_done(&mut self) -> Result<Value, EvalError> {
        loop {
            match self.step() {
                Ok(StepEvent::Progress) => {}
                Ok(StepEvent::Done(value)) => return Ok(value),
                Err(err) => {
                    self.frames.clear();
                    return Err(err);
                }
            }
        }
    }

    fn lowered(&self, loc: &ItemLoc) -> &'db MirLowered {
        mir::mir_lowered(self.db, loc.to_id(self.db))
    }

    fn push_frame(
        &mut self,
        loc: ItemLoc,
        body_id: BodyId,
        args: Vec<Value>,
        const_args: Vec<Value>,
        return_to: Option<(LocalId, Option<mir::BlockId>)>,
    ) -> Result<(), EvalError> {
        if self.frames.len() >= MAX_FRAMES {
            // In a const context this is a const error; in run-mode code
            // it's an ordinary stack overflow.
            return Err(EvalError {
                kind: if self.const_depth > 0 {
                    EvalErrorKind::NotConst
                } else {
                    EvalErrorKind::Runtime
                },
                message: format!("stack overflow: recursion exceeded {MAX_FRAMES} frames"),
                origin: root_origin(self.db, &loc),
                notes: Vec::new(),
            });
        }
        let body = &self.lowered(&loc).bodies[body_id];
        if args.len() != body.params.len() {
            return Err(self.internal_error(
                format!(
                    "arity mismatch reached execution: {} argument(s) for {} parameter(s)",
                    args.len(),
                    body.params.len()
                ),
                None,
            ));
        }
        let mut locals: ArenaMap<LocalId, Value> = ArenaMap::default();
        for (&param, arg) in body.params.iter().zip(args) {
            locals.insert(param, arg);
        }
        self.next_frame_serial += 1;
        self.frames.push(Frame {
            loc,
            body: body_id,
            serial: self.next_frame_serial,
            block: body.entry,
            statement: 0,
            const_args,
            locals,
            promoted: FxHashMap::default(),
            return_to,
        });
        Ok(())
    }

    /// Execute exactly one statement or terminator of the topmost frame.
    /// On `Err`, the frame stack is left intact for inspection.
    pub fn step(&mut self) -> Result<StepEvent, EvalError> {
        let Some(frame) = self.frames.last() else {
            return Err(self.internal_error("step with no live frames".to_owned(), None));
        };
        let loc = frame.loc.clone();
        let (body_id, block_id, statement) = (frame.body, frame.block, frame.statement);
        self.spend_fuel(&loc)?;
        let body = &self.lowered(&loc).bodies[body_id];
        let block = &body.blocks[block_id];

        if let Some(stmt) = block.statements.get(statement) {
            match &stmt.kind {
                StatementKind::Assign { dest, rvalue } => {
                    let value = self.eval_rvalue(&loc, body, rvalue, stmt.origin)?;
                    // Element indices in the destination evaluate before
                    // the write (bounds are then checked at the write
                    // itself — an out-of-range element write is an
                    // ordinary trap, never silent corruption).
                    let projection =
                        self.resolve_projection(&loc, body, &dest.projection, stmt.origin)?;
                    // A destination whose projection derefs is a store
                    // through a raw pointer: resolved against abstract
                    // memory, with liveness and writability checked right
                    // there — the misuse cases are detected UB, not
                    // silent corruption.
                    if projection
                        .iter()
                        .any(|elem| matches!(elem, ResolvedProj::Deref))
                    {
                        self.write_through(
                            &loc,
                            body,
                            dest.local,
                            &projection,
                            value,
                            stmt.origin,
                        )?;
                    } else {
                        self.write_place(&loc, body, dest.local, &projection, value, stmt.origin)?;
                    }
                }
            }
            let frame = self.frames.last_mut().expect("frame still live");
            frame.statement += 1;
            return Ok(StepEvent::Progress);
        }

        let origin = block.terminator.origin;
        match &block.terminator.kind {
            TerminatorKind::Goto { target } => {
                self.jump(*target);
            }
            TerminatorKind::SwitchBool {
                discr,
                then_block,
                else_block,
            } => {
                let discr = self.eval_operand(&loc, body, discr, origin)?;
                let target = match discr {
                    Value::Bool(true) => *then_block,
                    Value::Bool(false) => *else_block,
                    other => {
                        return Err(self.ill_typed("a `bool` condition", &other, &loc, origin));
                    }
                };
                self.jump(target);
            }
            // Tagged dispatch: read the widening-injected tag, jump to the
            // arm for that variant index (or `otherwise`). Only ever
            // executed on enum-typed values — a variant-typed scrutinee's
            // match compiled to no switch at all.
            TerminatorKind::SwitchVariant {
                discr,
                decl,
                arms,
                otherwise,
            } => {
                let value = self.eval_operand(&loc, body, discr, origin)?;
                let Value::Variant {
                    decl: value_decl,
                    index,
                    ..
                } = &value
                else {
                    return Err(self.ill_typed("a tagged enum value", &value, &loc, origin));
                };
                if value_decl != decl {
                    return Err(self.ill_typed(
                        &format!("a `{}` value", decl.display_name()),
                        &value,
                        &loc,
                        origin,
                    ));
                }
                let target = arms
                    .iter()
                    .find(|(arm_index, _)| arm_index == index)
                    .map(|&(_, target)| target)
                    .unwrap_or(*otherwise);
                self.jump(target);
            }
            TerminatorKind::Call {
                callee,
                args,
                dest,
                target,
            } => {
                let callee = self.eval_operand(&loc, body, callee, origin)?;
                let args = args
                    .iter()
                    .map(|arg| self.eval_operand(&loc, body, arg, origin))
                    .collect::<Result<Vec<_>, _>>()?;
                match callee {
                    Value::Fn(f) => {
                        self.push_frame(
                            f.item,
                            f.body,
                            args,
                            f.const_args,
                            Some((*dest, *target)),
                        )?;
                    }
                    Value::Builtin(_) | Value::ExternFn { .. } => {
                        let result = match callee {
                            Value::Builtin(builtin) => {
                                self.builtin_call(builtin, args, &loc, origin)?
                            }
                            Value::ExternFn { decl, sig } => {
                                self.extern_call(decl.display_name(), &sig, args, &loc, origin)?
                            }
                            _ => unreachable!("matched just above"),
                        };
                        match target {
                            Some(target) => {
                                let frame = self.frames.last_mut().expect("frame still live");
                                frame.locals.insert(*dest, result);
                                let target = *target;
                                self.jump(target);
                            }
                            None => {
                                return Err(self.internal_error(
                                    "a diverging call returned".to_owned(),
                                    Some((loc, origin)),
                                ));
                            }
                        }
                    }
                    other => {
                        return Err(self.ill_typed("a callable value", &other, &loc, origin));
                    }
                }
            }
            TerminatorKind::Return => {
                let frame = self.frames.last().expect("frame still live");
                let value = frame
                    .locals
                    .get(body.return_local())
                    .cloned()
                    .unwrap_or(Value::Unit);
                let finished = self.frames.pop().expect("frame still live");
                // The frame's address-taken locals die with it: their
                // allocations stay in memory (ids are never reused) but go
                // dead — a later deref through a surviving pointer is
                // *detected* UB, deterministically.
                for alloc in finished.promoted.values() {
                    if let Some(allocation) = self.memory.get_mut(alloc) {
                        allocation.live = false;
                    }
                }
                match finished.return_to {
                    None => return Ok(StepEvent::Done(value)),
                    Some((dest, Some(target))) => {
                        let caller = self.frames.last_mut().ok_or_else(|| {
                            // Can't happen: linked frames always have callers.
                            EvalError {
                                kind: EvalErrorKind::Runtime,
                                message: "internal error: a linked frame had no caller — \
                                          this is a bug in the Must language server"
                                    .to_owned(),
                                origin: None,
                                notes: Vec::new(),
                            }
                        })?;
                        caller.locals.insert(dest, value);
                        caller.block = target;
                        caller.statement = 0;
                    }
                    Some((_, None)) => {
                        return Err(self.internal_error(
                            "a diverging call returned".to_owned(),
                            Some((loc, origin)),
                        ));
                    }
                }
            }
            // Deferred error: the editor already shows this exact message
            // as a diagnostic; execution reached it.
            TerminatorKind::Trap { message, .. } => {
                return Err(EvalError {
                    kind: EvalErrorKind::Trap,
                    message: message.clone(),
                    origin: Some((loc, origin)),
                    notes: Vec::new(),
                });
            }
            // A const-check violation at initializer level. Forcing an
            // item is always a const context, so this fires there with the
            // editor's exact message; the one fall-through is the runner's
            // synthetic entry, whose initializer runs as run-mode code at
            // `const_depth` 0. The guard is a mode check, not a program
            // point: falling through advances into the guarded block
            // within the same step, so stepping never pauses on it.
            TerminatorKind::ConstTrap { message, target } => {
                if self.const_depth > 0 {
                    return Err(EvalError {
                        kind: EvalErrorKind::Trap,
                        message: message.clone(),
                        origin: Some((loc, origin)),
                        notes: Vec::new(),
                    });
                }
                self.jump(*target);
                return self.step();
            }
            TerminatorKind::Unreachable => {
                return Err(self.internal_error(
                    "entered an unreachable block".to_owned(),
                    Some((loc, origin)),
                ));
            }
        }
        Ok(StepEvent::Progress)
    }

    /// Evaluate a place projection's element-index operands, producing the
    /// value-walkable form the place readers/writers consume. Field and
    /// deref steps pass through; index steps evaluate to their `usize`
    /// value (bounds are checked later, where the place is used and the
    /// array's length is known).
    fn resolve_projection(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        projection: &[ProjElem],
        origin: ExprId,
    ) -> Result<Vec<ResolvedProj>, EvalError> {
        projection
            .iter()
            .map(|elem| match elem {
                ProjElem::Field(index) => Ok(ResolvedProj::Field(*index)),
                ProjElem::Index(op) => {
                    let value = self.eval_operand(loc, body, op, origin)?;
                    // The checker pins index positions to `usize`; any other
                    // kind here means an already-diagnosed program running in
                    // deferred-error mode — trap as ill-typed rather than
                    // silently widening a mistyped index.
                    let index = match &value {
                        Value::Int(iv) => iv.usize_payload(),
                        _ => None,
                    };
                    let Some(index) = index else {
                        return Err(self.ill_typed("a `usize` index", &value, loc, origin));
                    };
                    Ok(ResolvedProj::Index(index))
                }
                ProjElem::Deref => Ok(ResolvedProj::Deref),
            })
            .collect()
    }

    /// Read a projected place's current value: the root local
    /// (promoted-aware), then field/element steps — and across a
    /// [`ResolvedProj::Deref`], the pointee (liveness checked first: a
    /// dead allocation is detected UB; then the pointer's own stored path
    /// is validated — an address minted past the end of an array is
    /// detected UB at this, its first use). The place's own element steps
    /// are the source program's `[i]`, so THEIR bounds failures are
    /// ordinary runtime traps, exactly like `a[i]` reads.
    fn read_place(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        local: LocalId,
        projection: &[ResolvedProj],
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let frame = self.frames.last();
        // The promoted tier first (empty in pointer-free bodies): an
        // address-taken local's current value lives in its allocation.
        let root = frame.and_then(|frame| {
            if !frame.promoted.is_empty()
                && let Some(alloc) = frame.promoted.get(&local)
            {
                return self.memory.get(alloc).map(|allocation| &allocation.value);
            }
            frame.locals.get(local)
        });
        let Some(mut current) = root else {
            return Err(self.internal_error(
                format!("read of uninitialized {}", local_name(body, local)),
                Some((loc.clone(), origin)),
            ));
        };
        // `(tag, allocation, path)` per deref stepped through — checked
        // against the aliasing tree once the walk's borrow of memory is
        // released. `path` is the ELEMENT PATH the access actually
        // touches, so two nodes covering disjoint fields/elements of the
        // same allocation never interact — see `paths_overlap`. A
        // field/index step AFTER a deref extends the most recent entry's
        // path (see `after_deref` below): the pointer's own stored path
        // names where it points, but `p.*.y` touches only the `y` field
        // of that pointee, not the whole thing.
        let mut reads: Vec<(Provenance, AllocId, Vec<PathElem>)> = Vec::new();
        // Whether the walk has stepped through a deref yet — gates the
        // field/index path-extension above so it never fires for the
        // deref-free special case just below, whose entry already carries
        // the full projection.
        let mut after_deref = false;
        // A place with NO deref names the local's own storage. If that
        // storage is covered by borrows, reading it is a read through the
        // allocation's ROOT — foreign to every borrow below it whose path
        // overlaps this one, so it freezes exclusive children exactly as
        // a sibling borrow would.
        //
        // Without this the root node was inert for the local's own name,
        // and `let r = n.&; n = 99; r.*` — the most ordinary exclusivity
        // bug there is — ran clean.
        if !projection.iter().any(|e| matches!(e, ResolvedProj::Deref))
            && let Some(frame) = self.frames.last()
            && let Some(&alloc) = frame.promoted.get(&local)
            && let Some(&root) = self.borrow_roots.get(&alloc)
        {
            reads.push((
                Provenance(Some(root)),
                alloc,
                resolved_proj_path(projection),
            ));
        }
        for elem in projection {
            // Projecting into tracked-uninit: the structure was never
            // written — detected UB (same judgement as `project_path`).
            if matches!(current, Value::Uninit) {
                return Err(self.uninit_read(loc, origin));
            }
            current = match elem {
                ResolvedProj::Field(index) => {
                    if after_deref && let Some(pending) = reads.last_mut() {
                        pending.2.push(PathElem::Field(*index));
                    }
                    match field_step(current, *index) {
                        Ok(next) => next,
                        Err(error) => return Err(self.ptr_path_error(error, loc, origin)),
                    }
                }
                ResolvedProj::Index(index) => {
                    if after_deref && let Some(pending) = reads.last_mut() {
                        pending
                            .2
                            .push(PathElem::Index(u64::try_from(*index).unwrap_or(u64::MAX)));
                    }
                    match current {
                        Value::Array(values) => {
                            if *index >= values.len() as u128 {
                                return Err(EvalError {
                                    kind: EvalErrorKind::Runtime,
                                    message: hir::diag::index_out_of_bounds(
                                        values.len() as u128,
                                        *index,
                                    ),
                                    origin: Some((loc.clone(), origin)),
                                    notes: Vec::new(),
                                });
                            }
                            &values[*index as usize]
                        }
                        other => {
                            return Err(self.ill_typed("an array value", other, loc, origin));
                        }
                    }
                }
                ResolvedProj::Deref => {
                    let Value::Ptr { alloc, path, tag } = current else {
                        return Err(self.ill_typed("a raw pointer", current, loc, origin));
                    };
                    // The aliasing check rides the READ, not the address:
                    // safe pointers live by EXISTENCE, and this is where
                    // existence becomes a use. Deferred past the loop
                    // because the walk holds a borrow of memory and the
                    // check mutates the tree; the order is unobservable
                    // (both outcomes stop execution). Steps AFTER this
                    // one extend this entry's path (see `after_deref`
                    // above) — `p.*.y` only touches `y`.
                    reads.push((*tag, *alloc, path.clone()));
                    after_deref = true;
                    let allocation = self.allocation_for_deref(*alloc, loc, origin)?;
                    self.follow_ptr_path(&allocation.value, path, loc, origin)?
                }
            };
        }
        // The tracked-uninit read gate: a value-read may not carry poison
        // into pure value land — a fresh heap element (or an aggregate
        // containing one) reads as detected UB until it is written. This
        // is THE gate that keeps `Value::Uninit` unobservable: `==`,
        // `print`, and every other consumer only ever see values that
        // passed through here.
        if current.contains_uninit() {
            return Err(self.uninit_read(loc, origin));
        }
        let value = current.clone();
        for (tag, alloc, path) in reads {
            self.aliasing_access(tag, alloc, &path, Access::Read, loc, origin)?;
        }
        Ok(value)
    }

    /// Resolve a place to an abstract-memory location `(allocation,
    /// element path)` without touching the final slot — the shared engine
    /// behind `.&raw` minting and pointer-routed stores. A place with no
    /// deref addresses the root local's own storage: the local is
    /// promoted into memory (first address-taking only). A deref instead
    /// READS the pointer the leading steps name and continues inside its
    /// allocation, path extended — nothing promoted, nothing
    /// materialized: the result carries the ORIGINAL allocation's
    /// identity. Mid-chain derefs read memory, so their liveness (and
    /// stored-path) UB checks apply on the way.
    fn resolve_place_alloc(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        local: LocalId,
        projection: &[ResolvedProj],
        mode: PathMode,
        origin: ExprId,
    ) -> Result<(AllocId, Vec<PathElem>, Provenance), EvalError> {
        // The node the resulting address speaks through. A place with no
        // deref addresses the root local's own storage: the untracked
        // marker, resolved against that local's ROOT (if a safe borrow
        // has ever minted one) the first time it is actually used — see
        // `Machine::aliasing_access`. A deref inherits the pointer's own
        // node, which is exactly the `.&raw`-is-not-a-decayed-borrow rule
        // as a mechanism: a raw pointer minted from a borrow keeps the
        // borrow's node instead of getting one of its own, so the two die
        // together.
        let mut provenance = Provenance::default();
        let split = projection
            .iter()
            .position(|elem| matches!(elem, ResolvedProj::Deref));
        let (mut alloc, mut path, rest) = match split {
            None => {
                let alloc = self.promote_local(local, body, loc, origin)?;
                (alloc, Vec::new(), projection)
            }
            Some(split) => {
                let ptr = self.read_place(loc, body, local, &projection[..split], origin)?;
                let Value::Ptr { alloc, path, tag } = ptr else {
                    return Err(self.ill_typed("a raw pointer", &ptr, loc, origin));
                };
                provenance = tag;
                (alloc, path, &projection[split + 1..])
            }
        };
        for elem in rest {
            match elem {
                ResolvedProj::Field(index) => path.push(PathElem::Field(*index)),
                ResolvedProj::Index(index) => {
                    if mode == PathMode::Store {
                        // The program's own checked `[i]` step: bounds
                        // judged against the current element (liveness
                        // and the pointer's stored path are checked on
                        // the way — both UB judgements).
                        let allocation = self.allocation_for_deref(alloc, loc, origin)?;
                        let current =
                            self.follow_ptr_path(&allocation.value, &path, loc, origin)?;
                        if matches!(current, Value::Uninit) {
                            return Err(self.uninit_read(loc, origin));
                        }
                        let Value::Array(values) = current else {
                            return Err(self.ill_typed("an array value", current, loc, origin));
                        };
                        if *index >= values.len() as u128 {
                            return Err(EvalError {
                                kind: EvalErrorKind::Runtime,
                                message: hir::diag::index_out_of_bounds(
                                    values.len() as u128,
                                    *index,
                                ),
                                origin: Some((loc.clone(), origin)),
                                notes: Vec::new(),
                            });
                        }
                    }
                    let index = u64::try_from(*index).map_err(|_| {
                        self.internal_error(
                            format!("array index {index} overflows a pointer path"),
                            Some((loc.clone(), origin)),
                        )
                    })?;
                    path.push(PathElem::Index(index));
                }
                ResolvedProj::Deref => {
                    let allocation = self.allocation_for_deref(alloc, loc, origin)?;
                    let current = self.follow_ptr_path(&allocation.value, &path, loc, origin)?;
                    if matches!(current, Value::Uninit) {
                        return Err(self.uninit_read(loc, origin));
                    }
                    let Value::Ptr {
                        alloc: next_alloc,
                        path: next_path,
                        tag: next_tag,
                    } = current
                    else {
                        return Err(self.ill_typed("a raw pointer", current, loc, origin));
                    };
                    alloc = *next_alloc;
                    path = next_path.clone();
                    provenance = *next_tag;
                }
            }
        }
        Ok((alloc, path, provenance))
    }

    /// Store through a pointer-routed place (a projection containing a
    /// deref): resolve to `(allocation, path)`, then the store-time
    /// checks — liveness (a dead allocation is detected UB), writability
    /// (a static's allocation is read-only — a write into one is detected
    /// UB), and the pointer's stored path (an address minted out of
    /// bounds: detected UB). The store mutates the pointee IN PLACE, so
    /// interior pointers into the overwritten value survive it — exactly
    /// real-memory behavior, same as whole-local overwrites of promoted
    /// locals.
    fn write_through(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        local: LocalId,
        projection: &[ResolvedProj],
        value: Value,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let (alloc, path, provenance) =
            self.resolve_place_alloc(loc, body, local, projection, PathMode::Store, origin)?;
        self.allocation_for_deref(alloc, loc, origin)?;
        // The write half of the aliasing check. A write through a borrow
        // invalidates every borrow it is foreign to, and is itself
        // undefined behavior if this borrow was already invalidated —
        // exclusivity, enforced dynamically until the loan checker lands.
        self.aliasing_access(provenance, alloc, &path, Access::Write, loc, origin)?;
        let allocation = self.writable_allocation(alloc, loc, origin)?;
        match project_path_mut(&mut allocation.value, &path) {
            Ok(slot) => {
                *slot = value;
                Ok(())
            }
            Err(error) => Err(self.ptr_path_error(error, loc, origin)),
        }
    }

    /// The writability judgement, and the only place it is spelled: a
    /// `static`'s allocation is read-only, so a write through a pointer
    /// into one is detected UB — whether the write is a single store
    /// ([`Machine::write_through`]) or a whole range (judged by
    /// [`Machine::judge_write_range`] on `write_range`'s behalf). Answers
    /// the allocation itself: `write_through` passes and goes straight on
    /// to mutating it; the range judgement passes and discards the handle,
    /// since a range write re-navigates to its own element slot rather
    /// than reusing this one. LIVENESS is the caller's to have judged
    /// first (`allocation_for_deref`, directly or through
    /// `checked_range`); this gate only asks whether the memory accepts
    /// writes at all.
    fn writable_allocation(
        &mut self,
        alloc: AllocId,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<&mut Allocation, EvalError> {
        let allocation = self
            .memory
            .get_mut(&alloc)
            .expect("liveness judged before the write");
        // No test pins this message, and that is not an oversight: the
        // verdict is UNREACHABLE from well-typed source today — statics
        // hand out shared `.&raw` only (there is no `static mut`), and no
        // pointer escapes const evaluation. It is the memory model's
        // belt-and-braces until `static mut` (or an escape) makes it
        // reachable; whoever lands one should pin it here.
        if !allocation.writable {
            return Err(EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: "write through a pointer into read-only memory (a `static`)".to_owned(),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            });
        }
        Ok(allocation)
    }

    /// Navigate a pointer's stored path inside a (live) allocation's
    /// value, classifying failures: out-of-range element steps are
    /// detected UB (the address was minted past the end of an array —
    /// address-taking never bounds-checks, the deref is where validity is
    /// judged), shape mismatches are internal errors.
    fn follow_ptr_path<'v>(
        &self,
        value: &'v Value,
        path: &[PathElem],
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<&'v Value, EvalError> {
        project_path(value, path).map_err(|error| self.ptr_path_error(error, loc, origin))
    }

    /// The UB/internal split for failures along a pointer's STORED path —
    /// deterministic, like every detected-UB message.
    fn ptr_path_error(&self, error: ProjectError, loc: &ItemLoc, origin: ExprId) -> EvalError {
        match error {
            ProjectError::OutOfBounds { len, index } => EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: format!(
                    "out-of-bounds pointer — it points to element {index} of an array \
                     with {len} elements"
                ),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            },
            ProjectError::Uninit => self.uninit_read(loc, origin),
            ProjectError::Shape(detail) => self.internal_error(detail, Some((loc.clone(), origin))),
        }
    }

    /// The tracked-uninit gate's error — detected UB, one message
    /// everywhere ([`UNINIT_READ`]).
    fn uninit_read(&self, loc: &ItemLoc, origin: ExprId) -> EvalError {
        EvalError {
            kind: EvalErrorKind::UndefinedBehavior,
            message: UNINIT_READ.to_owned(),
            origin: Some((loc.clone(), origin)),
            notes: Vec::new(),
        }
    }

    /// Store `value` into the place on the topmost frame: the whole local
    /// for an empty projection, or the nested `Value::Record` field /
    /// `Value::Array` element the resolved path names — mutated in place;
    /// values are plain Rust data in `frame.locals`. Only records and
    /// arrays are writable through (assignment targets are compile-checked
    /// to field/element chains; variant payloads aren't reachable as
    /// places), so any other shape here is an invariant violation, loud
    /// like every other ill-typed value — while an out-of-bounds element
    /// write is an ordinary runtime trap (the same message as an
    /// out-of-bounds read), never UB and never corruption.
    fn write_place(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        local: LocalId,
        projection: &[ResolvedProj],
        value: Value,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let project_error = |this: &Self, error: ProjectError| match error {
            ProjectError::OutOfBounds { len, index } => EvalError {
                kind: EvalErrorKind::Runtime,
                message: hir::diag::index_out_of_bounds(len, index),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            },
            ProjectError::Uninit => this.uninit_read(loc, origin),
            ProjectError::Shape(detail) => this.internal_error(detail, Some((loc.clone(), origin))),
        };
        // A promoted (address-taken) local lives in memory, not in the
        // frame map: the write lands in its allocation, so pointers into
        // it observe it — including interior pointers surviving a
        // whole-value overwrite, exactly real-memory behavior. The
        // `is_none` fast path keeps pointer-free bodies on the plain
        // frame-map route.
        let promoted = self
            .frames
            .last()
            .expect("frame still live")
            .promoted
            .get(&local)
            .copied();
        if let Some(alloc) = promoted {
            // Writing a local BY ITS OWN NAME is a write through the
            // allocation's root, foreign to every borrow below it whose
            // path overlaps the field/element actually written — which
            // disables them, so a later use of one is caught. Without this
            // the root node was inert for the local's own name and
            // `let m = n.&mut; n = 99; m.* = 5;` ran clean. The path is
            // the projection itself (never a deref here — that route is
            // `write_through`), so `p.y = 5` does not disturb a borrow of
            // the disjoint field `p.x`.
            if let Some(&root) = self.borrow_roots.get(&alloc) {
                self.aliasing_access(
                    Provenance(Some(root)),
                    alloc,
                    &resolved_proj_path(projection),
                    Access::Write,
                    loc,
                    origin,
                )?;
            }
            let slot = match self.memory.get_mut(&alloc) {
                Some(allocation) => &mut allocation.value,
                None => {
                    return Err(self.internal_error(
                        "a promoted local's allocation is missing".to_owned(),
                        Some((loc.clone(), origin)),
                    ));
                }
            };
            if projection.is_empty() {
                *slot = value;
                return Ok(());
            }
            return match project_mut(slot, projection) {
                Ok(field) => {
                    *field = value;
                    Ok(())
                }
                Err(error) => Err(project_error(self, error)),
            };
        }
        let frame = self.frames.last_mut().expect("frame still live");
        if projection.is_empty() {
            frame.locals.insert(local, value);
            return Ok(());
        }
        match frame.locals.get_mut(local) {
            None => Err(self.internal_error(
                format!("write through uninitialized {}", local_name(body, local)),
                Some((loc.clone(), origin)),
            )),
            Some(slot) => match project_mut(slot, projection) {
                Ok(field) => {
                    *field = value;
                    Ok(())
                }
                Err(error) => Err(project_error(self, error)),
            },
        }
    }

    fn jump(&mut self, target: mir::BlockId) {
        let frame = self.frames.last_mut().expect("frame still live");
        frame.block = target;
        frame.statement = 0;
    }

    fn eval_rvalue(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        rvalue: &Rvalue,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match rvalue {
            Rvalue::Use(op) => self.eval_operand(loc, body, op, origin),
            Rvalue::BinaryOp(op, l, r) => {
                let l = self.eval_operand(loc, body, l, origin)?;
                let r = self.eval_operand(loc, body, r, origin)?;
                self.eval_bin_op(*op, l, r, loc, origin)
            }
            Rvalue::UnaryNeg(op) => {
                let v = self.eval_operand(loc, body, op, origin)?;
                self.eval_unary_neg(v, loc, origin)
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Record(names),
                ops,
            } => {
                let mut fields = Vec::with_capacity(ops.len());
                for (name, op) in names.iter().zip(ops) {
                    let value = self.eval_operand(loc, body, op, origin)?;
                    fields.push((name.clone(), value));
                }
                Ok(Value::Record { fields })
            }
            Rvalue::Aggregate {
                kind: AggregateKind::VariantPayload,
                ops,
            } => {
                let values = ops
                    .iter()
                    .map(|op| self.eval_operand(loc, body, op, origin))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Tuple(values))
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ops,
            } => {
                let values = ops
                    .iter()
                    .map(|op| self.eval_operand(loc, body, op, origin))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Array(values))
            }
            // `[e; N]`: N copies of the element. Const contexts pay fuel
            // per element — a compile-time `[0; 10_000_000_000]` must
            // exhaust the budget, not the server's memory (run-mode code
            // is the user's own process, unfueled as always).
            Rvalue::Repeat { elem, count } => {
                let elem = self.eval_operand(loc, body, elem, origin)?;
                let count = self.eval_operand(loc, body, count, origin)?;
                // The checker pins repeat counts to `usize`; a wrong kind is
                // deferred-error mode — trap ill-typed, don't launder.
                let Some(count) = (match &count {
                    Value::Int(iv) => iv.usize_payload(),
                    _ => None,
                }) else {
                    return Err(self.ill_typed("a `usize` repeat count", &count, loc, origin));
                };
                if self.const_depth > 0 {
                    self.spend_fuel_n(count.min(u64::MAX as u128) as u64, loc)?;
                }
                let count = usize::try_from(count).map_err(|_| EvalError {
                    kind: EvalErrorKind::Runtime,
                    message: format!("array length {count} is too large"),
                    origin: Some((loc.clone(), origin)),
                    notes: Vec::new(),
                })?;
                Ok(Value::Array(vec![elem; count]))
            }
            // `a[i]`: the runtime bounds check — an ordinary trap (the
            // rejected-op story), deterministic, and worded exactly like
            // the compile-time squiggle when both sides are known there.
            Rvalue::Index { base, index } => {
                let base = self.eval_operand(loc, body, base, origin)?;
                let index = self.eval_operand(loc, body, index, origin)?;
                let Value::Array(mut values) = base else {
                    return Err(self.ill_typed("an array value", &base, loc, origin));
                };
                // The checker pins index positions to `usize`; a wrong kind
                // is deferred-error mode — trap ill-typed, don't launder.
                let Some(index) = (match &index {
                    Value::Int(iv) => iv.usize_payload(),
                    _ => None,
                }) else {
                    return Err(self.ill_typed("a `usize` index", &index, loc, origin));
                };
                if index >= values.len() as u128 {
                    return Err(EvalError {
                        kind: EvalErrorKind::Runtime,
                        message: hir::diag::index_out_of_bounds(values.len() as u128, index),
                        origin: Some((loc.clone(), origin)),
                        notes: Vec::new(),
                    });
                }
                Ok(values.swap_remove(index as usize))
            }
            // The widening conversion: the one place a tag comes into
            // existence — the tag-free payload carrier becomes a tagged
            // enum value.
            Rvalue::WidenToEnum {
                op,
                decl,
                index,
                variant,
            } => {
                let value = self.eval_operand(loc, body, op, origin)?;
                let Value::Tuple(payload) = value else {
                    return Err(self.ill_typed("a variant payload", &value, loc, origin));
                };
                Ok(Value::Variant {
                    decl: decl.clone(),
                    index: *index,
                    name: variant.clone(),
                    payload,
                })
            }
            // Constructing a generic instance's fn value: the item's own
            // value (its root is the fn literal — a plain lazy forcing)
            // with the *evaluated* const arguments attached. Each operand
            // is a compile-time body (`Const::ConstBlock`); forcing it
            // inherits this frame's const env, which is what lets a const
            // param forward as a const argument (`rep::<const N>` inside
            // another generic body).
            Rvalue::Instantiate { item, const_args } => {
                let args = const_args
                    .iter()
                    .map(|op| self.eval_operand(loc, body, op, origin))
                    .collect::<Result<Vec<_>, _>>()?;
                let value = self.force_item(item.clone())?;
                let Value::Fn(f) = value else {
                    return Err(self.ill_typed("a function value", &value, loc, origin));
                };
                Ok(Value::Fn(FnValue {
                    const_args: args,
                    ..f
                }))
            }
            Rvalue::Field { base, index } => {
                let base = self.eval_operand(loc, body, base, origin)?;
                let index = *index as usize;
                let out_of_range = |this: &Self, what: &str, len: usize| {
                    this.internal_error(
                        format!("{what} index {index} out of range ({len} elements)"),
                        Some((loc.clone(), origin)),
                    )
                };
                match base {
                    Value::Record { mut fields } => {
                        if index < fields.len() {
                            Ok(fields.swap_remove(index).1)
                        } else {
                            Err(out_of_range(self, "record field", fields.len()))
                        }
                    }
                    // Positional payload extraction, both carriers: the
                    // tag-free variant-typed value and the tagged enum
                    // value (a match arm reads payloads out of whichever
                    // its scrutinee is).
                    Value::Tuple(mut values) => {
                        if index < values.len() {
                            Ok(values.swap_remove(index))
                        } else {
                            Err(out_of_range(self, "payload", values.len()))
                        }
                    }
                    Value::Variant { mut payload, .. } => {
                        if index < payload.len() {
                            Ok(payload.swap_remove(index))
                        } else {
                            Err(out_of_range(self, "payload", payload.len()))
                        }
                    }
                    other => Err(self.ill_typed("a record or payload value", &other, loc, origin)),
                }
            }
            // `place.&raw [mut]`: resolve the place to `(allocation,
            // path)` and mint the pointer — an (AllocId, path) pair, never
            // a number. A plain local root is promoted into abstract
            // memory (first address-taking only; after that the allocation
            // IS the local); a deref-rooted place (`p.*.x.&raw mut`)
            // instead reads the pointer and extends its path — the
            // ORIGINAL allocation's identity, no intermediate
            // materialization, no promotion. Element steps are NOT
            // bounds-checked here (validity is judged at the deref): an
            // out-of-range address mints silently, and every later deref
            // of it is detected UB.
            Rvalue::AddrOf { place, .. } => {
                let projection = self.resolve_projection(loc, body, &place.projection, origin)?;
                let (alloc, path, tag) = self.resolve_place_alloc(
                    loc,
                    body,
                    place.local,
                    &projection,
                    PathMode::Mint,
                    origin,
                )?;
                // The raw pointer INHERITS the node it was minted through
                // rather than getting one of its own — the mechanism
                // behind the ruling that `.&raw` is deliberately not a
                // decayed safe borrow. `x.*.&raw mut` therefore points at
                // the referent under `x`'s node: the two interleave
                // freely and die together.
                Ok(Value::Ptr { alloc, path, tag })
            }
            // `place.&` / `place.&mut` — a SAFE borrow. Same address as
            // its raw sibling would produce, plus the one thing that
            // separates them: a fresh node in the allocation's aliasing
            // tree, whose creation is itself an access through the parent.
            Rvalue::Borrow { mutable, place } => {
                let projection = self.resolve_projection(loc, body, &place.projection, origin)?;
                let (alloc, path, parent) = self.resolve_place_alloc(
                    loc,
                    body,
                    place.local,
                    &projection,
                    PathMode::Mint,
                    origin,
                )?;
                let tag = self.mint_borrow(parent, alloc, &path, *mutable, loc, origin)?;
                Ok(Value::Ptr { alloc, path, tag })
            }
            // `S[.field | [index]]....&raw`: the static's ONE allocation,
            // minted read-only on first mention — so two `S.&raw` are the
            // same address (static=identity, observable). Plain `S`
            // mentions keep cloning the memo, unchanged. Element steps
            // append unchecked, like every address-taking.
            Rvalue::AddrOfStatic { item, projection } => {
                let alloc = match self.static_allocs.get(item) {
                    Some(&alloc) => alloc,
                    None => {
                        let value = self.force_item(item.clone())?;
                        let alloc = self.fresh_alloc(Allocation {
                            value,
                            live: true,
                            writable: false,
                            kind: AllocKind::Static,
                            origin: None,
                        });
                        self.static_allocs.insert(item.clone(), alloc);
                        alloc
                    }
                };
                let resolved = self.resolve_projection(loc, body, projection, origin)?;
                let mut path = Vec::with_capacity(resolved.len());
                for elem in &resolved {
                    match elem {
                        ResolvedProj::Field(index) => path.push(PathElem::Field(*index)),
                        ResolvedProj::Index(index) => {
                            let index = u64::try_from(*index).map_err(|_| {
                                self.internal_error(
                                    format!("array index {index} overflows a pointer path"),
                                    Some((loc.clone(), origin)),
                                )
                            })?;
                            path.push(PathElem::Index(index));
                        }
                        // Deref-rooted chains lower through a pointer temp
                        // (`Rvalue::AddrOf`), never through the static
                        // form.
                        ResolvedProj::Deref => {
                            return Err(self.internal_error(
                                "a deref projection reached `.&raw` of a static".to_owned(),
                                Some((loc.clone(), origin)),
                            ));
                        }
                    }
                }
                // A static's allocation is read-only and shared for the
                // whole run, so there is no exclusivity to track: the
                // untracked root is the honest node for it.
                Ok(Value::Ptr {
                    alloc,
                    path,
                    tag: Provenance::default(),
                })
            }
        }
    }

    /// The allocation backing `local`, promoting it into abstract memory on
    /// the first address-taking: the current value moves out of the frame's
    /// plain map into a live, writable allocation; every later read/write
    /// of the local goes through it (see `write_place`/`eval_operand`).
    fn promote_local(
        &mut self,
        local: LocalId,
        body: &MirBody,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<AllocId, EvalError> {
        let frame = self.frames.last_mut().expect("frame still live");
        if let Some(&alloc) = frame.promoted.get(&local) {
            return Ok(alloc);
        }
        let Some(value) = frame.locals.remove(local) else {
            // `.&raw` of a local no `let` initialized: unreachable from real
            // programs (a `let` always initializes); loud like other reads
            // of uninitialized slots.
            return Err(self.internal_error(
                format!("address of uninitialized {}", local_name(body, local)),
                Some((loc.clone(), origin)),
            ));
        };
        let alloc = self.fresh_alloc(Allocation {
            value,
            live: true,
            writable: true,
            kind: AllocKind::Local,
            origin: None,
        });
        let frame = self.frames.last_mut().expect("frame still live");
        frame.promoted.insert(local, alloc);
        Ok(alloc)
    }

    fn fresh_alloc(&mut self, allocation: Allocation) -> AllocId {
        let alloc = AllocId(self.next_alloc);
        self.next_alloc += 1;
        self.memory.insert(alloc, allocation);
        alloc
    }

    /// The liveness gate every deref (read or write) passes through. A
    /// missing or dead allocation is *detected undefined behavior*: the
    /// interpreter stops with a message — a quality of the interpreter,
    /// not a guarantee of the language (compiled Must may do anything with
    /// the same program).
    fn allocation_for_deref(
        &self,
        alloc: AllocId,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<&Allocation, EvalError> {
        let ub = |message: &str| EvalError {
            kind: EvalErrorKind::UndefinedBehavior,
            message: message.to_owned(),
            origin: Some((loc.clone(), origin)),
            notes: Vec::new(),
        };
        let Some(allocation) = self.memory.get(&alloc) else {
            // A pointer from a different machine run (the const-escape
            // rule and the debugger's parameter filter should make this
            // unreachable): still deterministic, still UB.
            return Err(ub(
                "dangling pointer — it does not point into this execution's memory",
            ));
        };
        if !allocation.live {
            return Err(match allocation.kind {
                AllocKind::Local => ub(
                    "dangling pointer — the local it pointed to no longer exists \
                     (its frame has returned)",
                ),
                // A static's allocation is never popped, so a pointer into
                // one can never dangle this way; the arm only keeps the
                // match total.
                AllocKind::Static => ub("use after free — this allocation was already freed"),
                // A freed heap allocation: the classic use-after-free,
                // with the birth site attached.
                AllocKind::Heap => {
                    let mut err = ub("use after free — this allocation was already freed");
                    err.notes = allocation.allocated_here();
                    err
                }
                AllocKind::Dangling => ub("dangling pointer — this pointer was never valid"),
            });
        }
        Ok(allocation)
    }

    fn eval_bin_op(
        &mut self,
        op: hir::body::BinOp,
        l: Value,
        r: Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        use hir::body::BinOp::*;
        match op {
            Add | Sub | Mul | Div | Lt | Le | Gt | Ge => {
                let (Value::Int(l), Value::Int(r)) = (&l, &r) else {
                    let bad = if matches!(l, Value::Int(_)) { &r } else { &l };
                    return Err(self.ill_typed("integer operands", bad, loc, origin));
                };
                let (l, r) = (*l, *r);
                // Same-type by checking; the left operand's kind is the
                // operation's type. A mismatched kind is deferred-error mode
                // (an already-diagnosed program) — trap ill-typed rather
                // than compute a nonsense pairing.
                if l.kind() != r.kind() {
                    return Err(self.ill_typed("integer operands", &Value::Int(r), loc, origin));
                }
                let kind = l.kind();
                let runtime = |message: String| EvalError {
                    kind: EvalErrorKind::Runtime,
                    message,
                    origin: Some((loc.clone(), origin)),
                    notes: Vec::new(),
                };
                // Overflow = trap (runtime-error class, div-by-zero
                // precedent): arithmetic runs through the checked [`Number`]
                // surface, so an out-of-range result is `None` — the message
                // names the operation and the type. Signed edge cases
                // (`i::MIN / -1`) fall out of the same check.
                let arith =
                    |symbol: &str, result: Option<hir::IntValue>| -> Result<Value, EvalError> {
                        match result {
                            Some(iv) => Ok(Value::Int(iv)),
                            None => Err(runtime(format!(
                                "arithmetic overflow: `{} {symbol} {}` does not fit in `{}`",
                                l.to_i128(),
                                r.to_i128(),
                                kind.name()
                            ))),
                        }
                    };
                match op {
                    Add => arith("+", l.checked_add(r)),
                    Sub => arith("-", l.checked_sub(r)),
                    Mul => arith("*", l.checked_mul(r)),
                    Div => {
                        if r.to_i128() == 0 {
                            return Err(runtime("attempt to divide by zero".to_owned()));
                        }
                        arith("/", l.checked_div(r))
                    }
                    Lt => Ok(Value::Bool(l.to_i128() < r.to_i128())),
                    Le => Ok(Value::Bool(l.to_i128() <= r.to_i128())),
                    Gt => Ok(Value::Bool(l.to_i128() > r.to_i128())),
                    Ge => Ok(Value::Bool(l.to_i128() >= r.to_i128())),
                    Eq | Ne => unreachable!(),
                }
            }
            // Equality is structural, on operands inference agreed about —
            // with arithmetic's same-kind check: a mixed-kind integer
            // pairing (deferred-error mode) traps ill-typed, never silently
            // unequal.
            Eq | Ne => {
                if let (Value::Int(l), Value::Int(r)) = (&l, &r) {
                    if l.kind() != r.kind() {
                        return Err(self.ill_typed(
                            "integer operands",
                            &Value::Int(*r),
                            loc,
                            origin,
                        ));
                    }
                }
                Ok(Value::Bool(if matches!(op, Eq) { l == r } else { l != r }))
            }
        }
    }

    /// `-x`: negate in the wide carrier, then range-check against the
    /// operand's own type — on unsigned types every non-zero operand
    /// traps (the overflow rule), and the signed minimum traps too.
    fn eval_unary_neg(
        &mut self,
        v: Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Int(iv) = &v else {
            return Err(self.ill_typed("an integer operand", &v, loc, origin));
        };
        match iv.checked_neg() {
            Some(result) => Ok(Value::Int(result)),
            None => {
                // A negative operand renders parenthesized (`-(-128)`),
                // never as a confusing `--128`.
                let operand = if iv.to_i128() < 0 {
                    format!("({})", iv.to_i128())
                } else {
                    iv.to_i128().to_string()
                };
                Err(EvalError {
                    kind: EvalErrorKind::Runtime,
                    message: format!(
                        "arithmetic overflow: `-{operand}` does not fit in `{}`",
                        iv.kind().name()
                    ),
                    origin: Some((loc.clone(), origin)),
                    notes: Vec::new(),
                })
            }
        }
    }

    /// The aliasing half of a move: the local's storage no longer holds
    /// what any borrow of it was taken of, so every borrow into its
    /// allocation is invalidated — the identical root-level write access
    /// `write_place` performs when a local is written by its own name.
    ///
    /// Only ADDRESS-TAKEN locals have an allocation, and only they can have
    /// been borrowed, so a body that never borrows pays one map lookup.
    fn invalidate_moved_local(
        &mut self,
        local: LocalId,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let Some(frame) = self.frames.last() else {
            return Ok(());
        };
        if frame.promoted.is_empty() {
            return Ok(());
        }
        let Some(&alloc) = frame.promoted.get(&local) else {
            return Ok(());
        };
        let Some(&root) = self.borrow_roots.get(&alloc) else {
            return Ok(());
        };
        self.aliasing_access(
            Provenance(Some(root)),
            alloc,
            &[],
            Access::Write,
            loc,
            origin,
        )
    }

    fn eval_operand(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        op: &Operand,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match op {
            Operand::Copy(place) => {
                // A projected place (a deref read `p.*`, or a nested
                // field/element): index operands evaluate first, then the
                // walk — deref steps carry the UB checks.
                if !place.projection.is_empty() {
                    let projection =
                        self.resolve_projection(loc, body, &place.projection, origin)?;
                    return self.read_place(loc, body, place.local, &projection, origin);
                }
                let frame = self.frames.last();
                // The promoted tier first (empty in pointer-free bodies —
                // one branch, no behavior change): an address-taken
                // local's current value lives in its allocation.
                if let Some(frame) = frame
                    && !frame.promoted.is_empty()
                    && let Some(&alloc) = frame.promoted.get(&place.local)
                    && let Some(allocation) = self.memory.get(&alloc)
                {
                    // The tracked-uninit read gate (see `read_place`): a
                    // promoted local's storage is real memory, so `copy`
                    // can have planted poison in it.
                    if allocation.value.contains_uninit() {
                        return Err(self.uninit_read(loc, origin));
                    }
                    let value = allocation.value.clone();
                    // Reading a local BY ITS OWN NAME is a read through the
                    // allocation's root — foreign to every borrow below it,
                    // so an exclusive child freezes. The projected route
                    // (`read_place`) does the same; this is the bare-local
                    // fast path, and leaving it out made the check depend
                    // on whether a projection happened to be written.
                    if let Some(&root) = self.borrow_roots.get(&alloc) {
                        self.aliasing_access(
                            Provenance(Some(root)),
                            alloc,
                            &[],
                            Access::Read,
                            loc,
                            origin,
                        )?;
                    }
                    return Ok(value);
                }
                frame
                    .and_then(|frame| frame.locals.get(place.local))
                    .cloned()
                    .ok_or_else(|| {
                        self.internal_error(
                            format!("read of uninitialized {}", local_name(body, place.local)),
                            Some((loc.clone(), origin)),
                        )
                    })
            }
            // A MOVE reads exactly what a copy reads, and then says so to
            // the aliasing model: the place no longer owns the value, so
            // every borrow taken OF that place is stale from here on. That
            // is the same event a write is (`s = mk(2);` already disabled
            // them), which is why it reuses the same access — the only
            // difference is that this one destroys by leaving rather than
            // by overwriting.
            Operand::Move(place) => {
                let value = self.eval_operand(loc, body, &Operand::Copy(place.clone()), origin)?;
                // Only a whole-local move ends the local's ownership. A
                // projected move (never produced today) would end only the
                // sub-place's, which needs the path rather than the root.
                if place.projection.is_empty() {
                    self.invalidate_moved_local(place.local, loc, origin)?;
                }
                Ok(value)
            }
            Operand::Const(c) => Ok(match c {
                Const::Unit => Value::Unit,
                Const::Int(v) => Value::Int(*v),
                Const::Str(s) => Value::Str(s.clone()),
                Const::Bool(b) => Value::Bool(*b),
                Const::Char(c) => Value::Char(*c),
                Const::Builtin(b) => Value::Builtin(*b),
                Const::Item(item) => self.force_item(item.clone())?,
                // A fn literal constructed inside a generic frame inherits
                // the frame's const-param values: its body may read them
                // (the binder scopes the whole item), so const params
                // behave like auto-captured constants. Empty everywhere
                // else — zero cost for non-generic code.
                Const::Fn(body) => Value::Fn(FnValue {
                    item: loc.clone(),
                    body: *body,
                    const_args: self.current_const_args(),
                }),
                // A host import carries only its declaration — there is no
                // body, and nothing to capture.
                Const::ExternFn { decl, sig } => Value::ExternFn {
                    decl: decl.clone(),
                    sig: sig.clone(),
                },
                Const::ConstBlock(body) => {
                    let env = self.current_const_args();
                    self.force_const_block(loc, *body, env)?
                }
                // TR06's substitution model: the operand resolves against
                // the executing frame's instance. No frame values means a
                // *standalone* check-time forcing inside a generic item —
                // not knowable pre-instantiation, so the distinguished
                // kind lets the diagnostics layer skip it.
                Const::ConstParam(index) => self
                    .frames
                    .last()
                    .and_then(|frame| frame.const_args.get(*index as usize))
                    .cloned()
                    .ok_or(EvalError {
                        kind: EvalErrorKind::Uninstantiated,
                        message: "the value of a const parameter is not known \
                                  before instantiation"
                            .to_owned(),
                        origin: Some((loc.clone(), origin)),
                        notes: Vec::new(),
                    })?,
            }),
        }
    }

    /// The executing frame's const-param values — the environment
    /// compile-time bodies and nested fn values constructed from it
    /// inherit. Empty with no live frame (item roots).
    fn current_const_args(&self) -> Vec<Value> {
        self.frames
            .last()
            .map(|frame| frame.const_args.clone())
            .unwrap_or_default()
    }

    /// Call a HOST IMPORT — `static name = extern fn(...) -> T;`.
    ///
    /// The interpreter is one particular host, and this is the whole set of
    /// primitives it provides. Dispatch is BY NAME, because the name is the
    /// contract (there is no symbol-override surface), and a name this host
    /// does not implement is refused BY NAME at the call — the P01 layer-1
    /// property stated at runtime: a host that does not provide a hook has
    /// denied the capability, and the honest answer is to say which one.
    ///
    /// The declared SIGNATURE is checked here too, at the call rather than at
    /// the declaration, and deliberately: a compiler that validated import
    /// signatures would have to know every host, which is exactly the
    /// coupling `extern` exists to avoid.
    fn extern_call(
        &mut self,
        name: &str,
        sig: &hir::FnTy,
        args: Vec<Value>,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        // Defense in depth, the same split `print` and `read_line` have:
        // const-check plants the trap it can see, this is the machine's own
        // guarantee that compile-time evaluation never reaches a host.
        if self.const_depth > 0 {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: hir::diag::extern_call_in_const(name),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            });
        }
        match name {
            "read" => self.host_read(sig, args, loc, origin),
            _ => Err(EvalError {
                kind: EvalErrorKind::Runtime,
                message: format!(
                    "no host implementation for the import `{name}` — \
                     the interpreter provides `read` and nothing else"
                ),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            }),
        }
    }

    /// `extern fn read(buf: u8.&raw mut, len: usize) -> isize` — the one host
    /// primitive the interpreter provides, and the only place stdin enters
    /// the language below `read_line`.
    ///
    /// Bytes land in the caller's buffer as `u8` values, one per element:
    /// the interpreter's memory is typed, so "filling a buffer" is writing
    /// elements, not moving bytes. The return value is the machine-shaped
    /// count — nonnegative, `0` at end of input, negative `-errno` — and
    /// lifting it into something a Must program can match on is the FIRST
    /// WRAPPER's job, in Must, above this line.
    fn host_read(
        &mut self,
        sig: &hir::FnTy,
        args: Vec<Value>,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        const SHAPE: &str = "fn(buf: u8.&raw mut, len: usize) -> isize";
        // THE DECLARATION IS WHAT IS JUDGED, in full, before anything is
        // read or written.
        //
        // Judging argument VALUES instead would be unsound here and the
        // reason is worth stating: a fresh `alloc_array::<bool>(n)` and a
        // fresh `alloc_array::<u8>(n)` hold the same uninitialized elements,
        // so no inspection of the buffer can tell a byte array from an array
        // of something else. Only the declared pointee can, and getting it
        // wrong would fill a `bool` array with numbers that are not `bool`s
        // — a value the type system says cannot exist, discovered much later
        // as an internal error blaming the compiler.
        let [buf_ty, len_ty] = sig.params.as_slice() else {
            return Err(self.host_signature_error("read", SHAPE, loc, origin));
        };
        // A MUTABLE raw pointer to `u8`: `read` writes bytes, so a shared
        // pointer is as wrong as a non-byte one, and a non-pointer buffer is
        // refused here rather than reaching the value check below (which
        // would blame the compiler for what the declaration said).
        let byte_buffer = matches!(
            buf_ty,
            Ty::RawPtr { mutable: true, pointee } if matches!(**pointee, Ty::Int(IntKind::U8))
        );
        if !byte_buffer || !matches!(len_ty, Ty::Int(IntKind::Usize)) {
            return Err(self.host_signature_error("read", SHAPE, loc, origin));
        }
        // The count comes back as a SIGNED MACHINE WORD, and both Must
        // spellings of one — `isize` (POSIX's own `ssize_t`) and `i64` — are
        // accepted, with the answer built in whichever the declaration asked
        // for. The declaration decides the Must-side type; the host only
        // promises the machine shape. That is what lets a library feed the
        // count straight to `offset(p, i: isize)`, which matters because the
        // language has no integer conversions to bridge the gap with.
        let count_kind = match &sig.ret {
            Ty::Int(kind @ (IntKind::Isize | IntKind::I64)) => *kind,
            _ => return Err(self.host_signature_error("read", SHAPE, loc, origin)),
        };
        // Everything below is deferred-error mode: the signature is right,
        // so a value of the wrong shape reaching here is an invariant
        // violation, not a user's mistake.
        let [buf, len] = args.as_slice() else {
            return Err(self.internal_error(
                "the host `read` was called with the wrong number of arguments".to_owned(),
                Some((loc.clone(), origin)),
            ));
        };
        let Some(len) = (match len {
            Value::Int(hir::IntValue::Usize(v)) => Some(u128::from(*v)),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` length", len, loc, origin));
        };
        let len = usize::try_from(len).unwrap_or(usize::MAX);
        // Judge the destination range as a WRITE, for the FULL requested
        // length, before reading: a read that consumed input and then
        // trapped would have eaten bytes nobody can get back. This is the
        // complete write judgement — bounds, aliasing, and writability
        // ([`Machine::judge_write_range`]) — not bounds alone: a live safe
        // borrow of one of these bytes must be foreign to this write
        // exactly as it would be foreign to `p.*[i] = v`. A zero-length
        // request judges only that `buf` is a pointer at all (`copy`'s
        // rule), which is what lets a caller ask for "however much room
        // is left" without a special case when the answer is none.
        self.judge_write_range(buf, u128::from(len as u64), "read", "buffer", loc, origin)?;
        let mut bytes = Vec::new();
        let count = self.mode.read(&mut bytes, len)?;
        if bytes.is_empty() {
            return Ok(count_value(count_kind, count));
        }
        // What was actually read is what is written, and a short read
        // fills a PREFIX of the range judged above — so `write_range` here
        // re-judges the narrower range it lands on, which the wider
        // judgement above has already answered (bounds, aliasing, AND
        // writability); the repeat is idempotent, since a second write
        // through a still-valid tag finds nothing newly foreign to it.
        let elements = bytes
            .into_iter()
            .map(|byte| Value::Int(hir::IntValue::U8(byte)))
            .collect();
        self.write_range(buf, elements, "read", "buffer", loc, origin)?;
        Ok(count_value(count_kind, count))
    }

    /// A host primitive reached with arguments its contract does not accept.
    /// Named as a mismatch between the DECLARATION and the host rather than
    /// as an internal error, because that is what it is: the declaration is
    /// the user's claim about a host they cannot see, and this host disagrees.
    fn host_signature_error(
        &self,
        name: &str,
        expected: &str,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> EvalError {
        EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!(
                "the host import `{name}` was declared with a signature this host \
                 does not provide; it provides `{expected}`"
            ),
            origin: Some((loc.clone(), origin)),
            notes: Vec::new(),
        }
    }

    fn builtin_call(
        &mut self,
        builtin: Builtin,
        args: Vec<Value>,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        // Arity is checker-guaranteed (`ArgCountMismatch` traps before the
        // call); a mismatch reaching execution is an invariant violation.
        let expect_args = |this: &Self, n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(this.internal_error(
                    format!(
                        "builtin `{}` takes {n} argument(s), got {}",
                        builtin.name(),
                        args.len()
                    ),
                    Some((loc.clone(), origin)),
                ))
            }
        };
        // The const fence, in one place: [`hir::ConstLegality`] carries WHY
        // a builtin is refused, so this guard renders the same reason the
        // checker already showed as a squiggle — defense in depth, since
        // const-check plants a trap at every refused call it can see; this
        // is the machine's own guarantee. `panic` stays `Legal` (the one
        // side effect const contexts allow), so it reaches the dispatch
        // below untouched.
        if self.const_depth > 0 {
            match builtin.const_legality() {
                ConstLegality::Legal => {}
                ConstLegality::HostEffect => {
                    return Err(EvalError {
                        kind: EvalErrorKind::NotConst,
                        message: hir::diag::side_effect_call_in_const(builtin.name()),
                        origin: Some((loc.clone(), origin)),
                        notes: Vec::new(),
                    });
                }
                ConstLegality::HeapFence => {
                    return Err(EvalError {
                        kind: EvalErrorKind::NotConst,
                        message: hir::diag::heap_call_in_const(builtin.name()),
                        origin: Some((loc.clone(), origin)),
                        notes: Vec::new(),
                    });
                }
            }
        }
        match builtin {
            Builtin::Panic => {
                expect_args(self, 1)?;
                let Value::Str(text) = &args[0] else {
                    return Err(self.ill_typed("a `str` argument", &args[0], loc, origin));
                };
                Err(EvalError {
                    kind: EvalErrorKind::Panic,
                    message: text.clone(),
                    origin: Some((loc.clone(), origin)),
                    notes: Vec::new(),
                })
            }
            Builtin::Print => {
                expect_args(self, 1)?;
                let Value::Str(text) = &args[0] else {
                    return Err(self.ill_typed("a `str` argument", &args[0], loc, origin));
                };
                self.mode.print(text)?;
                Ok(Value::Unit)
            }
            Builtin::ReadLine => {
                expect_args(self, 0)?;
                Ok(match self.mode.read_line()? {
                    // What a line IS, decided here rather than once per
                    // host (P04): the terminator is STRIPPED — `\n`, and a
                    // preceding `\r` with it, so CRLF input reads the same
                    // as LF input instead of leaking a stray `\r` onto the
                    // end of every line. A blank line is `Line("")`, and a
                    // final unterminated line is a `Line` like any other.
                    Some(mut text) => {
                        if text.ends_with('\n') {
                            text.pop();
                            if text.ends_with('\r') {
                                text.pop();
                            }
                        }
                        builtin_variant(
                            loc.file,
                            hir::READ_LINE_RESULT_NAME,
                            "Line",
                            vec![Value::Str(text)],
                        )
                    }
                    None => {
                        builtin_variant(loc.file, hir::READ_LINE_RESULT_NAME, "End", Vec::new())
                    }
                })
            }
            Builtin::AllocArray => {
                expect_args(self, 1)?;
                self.builtin_alloc_array(&args[0], loc, origin)
            }
            Builtin::DeallocArray => {
                expect_args(self, 2)?;
                self.builtin_dealloc_array(&args[0], &args[1], loc, origin)
            }
            Builtin::Add => {
                expect_args(self, 2)?;
                self.builtin_add(&args[0], &args[1], loc, origin)
            }
            Builtin::Offset => {
                expect_args(self, 2)?;
                self.builtin_offset(&args[0], &args[1], loc, origin)
            }
            Builtin::Copy => {
                expect_args(self, 3)?;
                self.builtin_copy(&args[0], &args[1], &args[2], loc, origin)
            }
            Builtin::Dangling => {
                expect_args(self, 0)?;
                Ok(self.builtin_dangling())
            }
            // No const fence: decoding a `str` is pure (see
            // `Builtin::NextChar`), so it runs identically at compile time
            // and at run time — which is the whole reason it needs no
            // special case here.
            Builtin::NextChar => {
                expect_args(self, 2)?;
                // Dot-callable shape: the receiver is the LAST argument
                // (TR01), so the `str` is `args[1]` and the index is
                // `args[0]` — the order they were written in, too.
                self.builtin_next_char(&args[1], &args[0], loc, origin)
            }
            // No const fence either, for the same reason: deciding whether
            // bytes are UTF-8 observes nothing outside the arguments. Both
            // blesses read a range of `u8` elements through a raw pointer;
            // they differ only in what they do with bytes that are not.
            Builtin::StrFromUtf8 | Builtin::StrFromUtf8Unchecked => {
                expect_args(self, 2)?;
                self.builtin_bless(builtin, &args[0], &args[1], loc, origin)
            }
            // `s.len()` — pure, like `next_char`, and for the same reason
            // needs no const-context case of its own.
            Builtin::StrLen => {
                expect_args(self, 1)?;
                let Value::Str(text) = &args[0] else {
                    return Err(self.ill_typed("a `str` argument", &args[0], loc, origin));
                };
                Ok(Value::Int(hir::IntValue::Usize(text.len() as u64)))
            }
            Builtin::StrBytes => {
                expect_args(self, 2)?;
                self.builtin_str_bytes(&args[0], &args[1], loc, origin)
            }
        }
    }

    /// `str_bytes(s, dst)` — the bless read backwards: the bytes of `s`,
    /// written into caller storage as `u8` elements.
    ///
    /// The whole of the safety story is the destination, and it is the
    /// caller's claim: that `dst` addresses `s.len()` writable `u8`
    /// elements. The interpreter still catches every case its typed memory
    /// can see — a freed allocation, a range that runs off the end,
    /// read-only memory, and a live safe borrow of one of the bytes — by
    /// writing through [`Machine::write_range`], the same home `copy`'s
    /// destination half and the host `read` builtin write through.
    ///
    /// A zero-length string still requires `dst` to be a pointer (the
    /// shape [`Machine::checked_range`] always checks) but judges no
    /// bounds, aliasing, or writability for it — matching the blesses'
    /// rule from the other side: the empty string is written by writing
    /// nothing.
    fn builtin_str_bytes(
        &mut self,
        text: &Value,
        dst: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Str(text) = text else {
            return Err(self.ill_typed("a `str` argument", text, loc, origin));
        };
        let bytes: Vec<Value> = text
            .as_bytes()
            .iter()
            .map(|byte| Value::Int(hir::IntValue::U8(*byte)))
            .collect();
        self.write_range(dst, bytes, "str_bytes", "destination", loc, origin)?;
        Ok(Value::Unit)
    }

    /// `s.next_char(i)`: the scalar value starting at byte index `i`, plus
    /// the index of the next boundary — or `End` at or past the end.
    ///
    /// Interpreter strings ARE Rust strings, so UTF-8 validity is given and
    /// the decode cannot fail on its own terms. The one thing that CAN go
    /// wrong is the caller's index landing mid-codepoint, and that is a
    /// PANIC, not an `End` and not a silent slide to the next boundary: an
    /// index that is not a boundary means the program lost track of where
    /// it was, and quietly rounding it would turn a bug into wrong output.
    fn builtin_next_char(
        &mut self,
        text: &Value,
        index: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Str(text) = text else {
            return Err(self.ill_typed("a `str` argument", text, loc, origin));
        };
        let Value::Int(index) = index else {
            return Err(self.ill_typed("a `usize` argument", index, loc, origin));
        };
        let index = usize::try_from(index.to_i128().max(0)).unwrap_or(usize::MAX);
        let end = builtin_variant(loc.file, hir::NEXT_CHAR_NAME, "End", Vec::new());
        if index >= text.len() {
            return Ok(end);
        }
        if !text.is_char_boundary(index) {
            return Err(EvalError {
                kind: EvalErrorKind::Panic,
                message: format!(
                    "next_char: byte index {index} is not a char boundary; \
                     it is inside a multi-byte character"
                ),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            });
        }
        let Some(c) = text[index..].chars().next() else {
            // Unreachable: `index < len` and `index` is a boundary.
            return Ok(end);
        };
        // The next boundary. A `usize` is 64-bit here and the index came
        // from a `str` that fits in memory, so the only way this fails is
        // an internal invariant break, not a program's doing.
        let Some(next) = i128::try_from(index + c.len_utf8())
            .ok()
            .and_then(|v| hir::IntValue::new(hir::IntKind::Usize, v))
        else {
            return Err(self.internal_error(
                "next_char: the next boundary does not fit in `usize`".to_owned(),
                Some((loc.clone(), origin)),
            ));
        };
        Ok(builtin_variant(
            loc.file,
            hir::NEXT_CHAR_NAME,
            "Char",
            vec![Value::Char(c), Value::Int(next)],
        ))
    }

    /// `alloc_array::<T>(n)`: one fresh heap allocation of `n`
    /// tracked-uninit elements — MIR carries no type argument (erasure
    /// doctrine: the machine allocates `n` uninit ELEMENTS whatever `T`
    /// is; only the checker ever needed `T`). Returns the result-shaped
    /// `AllocResult` value: always `Ok(head pointer)` here — the
    /// interpreter cannot meaningfully OOM, the `Err` arm exists for
    /// signature stability (codegen-era fallible alloc).
    ///
    /// `n == 0` is a defined TRAP (A05): allocators are not
    /// required to handle zero-size requests — restrictive now, loosenable
    /// later, and never UB. A consequence worth writing down: no zero-size
    /// allocation can ever exist, so `dealloc_array` never legally sees
    /// `n == 0`.
    fn builtin_alloc_array(
        &mut self,
        n: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        // The checker pins this count to `usize`; any other kind is an
        // already-diagnosed program in deferred-error mode — trap ill-typed
        // rather than launder a mistyped count.
        let Some(n) = (match n {
            Value::Int(iv) => iv.usize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` element count", n, loc, origin));
        };
        if n == 0 {
            return Err(EvalError {
                kind: EvalErrorKind::Runtime,
                message: "cannot allocate zero elements: zero-size allocation support \
                          is reserved"
                    .to_owned(),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            });
        }
        let n = usize::try_from(n).map_err(|_| EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!("allocation of {n} elements is too large"),
            origin: Some((loc.clone(), origin)),
            notes: Vec::new(),
        })?;
        let alloc = self.fresh_alloc(Allocation {
            value: Value::Array(vec![Value::Uninit; n]),
            live: true,
            writable: true,
            kind: AllocKind::Heap,
            // The birth site: the blame the heap-UB notes point back at.
            origin: Some((loc.clone(), origin)),
        });
        // The head pointer: element 0 of the allocation.
        Ok(builtin_variant(
            loc.file,
            hir::ALLOC_RESULT_NAME,
            "Ok",
            vec![Value::Ptr {
                alloc,
                path: vec![PathElem::Index(0)],
                tag: Provenance::default(),
            }],
        ))
    }

    /// `dealloc_array::<T>(p, n)`: exact-match free (ruled A03/A04). Every
    /// contract violation is detected UB with its own message — and, where
    /// an allocation exists to blame, an "allocated here" note. Wrong-TYPE
    /// dealloc is undetectable under erasure (no runtime `T` exists) and
    /// accepted as such.
    fn builtin_dealloc_array(
        &mut self,
        p: &Value,
        n: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Ptr {
            alloc,
            path,
            tag: _,
        } = p
        else {
            return Err(self.ill_typed("a raw pointer", p, loc, origin));
        };
        // The checker pins this count to `usize`; a wrong kind is
        // deferred-error mode — trap ill-typed, don't launder.
        let Some(count) = (match n {
            Value::Int(iv) => iv.usize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` element count", n, loc, origin));
        };
        let ub = |message: String, notes: Vec<EvalNote>| EvalError {
            kind: EvalErrorKind::UndefinedBehavior,
            message,
            origin: Some((loc.clone(), origin)),
            notes,
        };
        let Some(allocation) = self.memory.get(alloc) else {
            return Err(ub(
                "dangling pointer — it does not point into this execution's memory".to_owned(),
                Vec::new(),
            ));
        };
        // Kind first: freeing a local or a static is a category error
        // whether or not the allocation is still live.
        if !matches!(allocation.kind, AllocKind::Heap) {
            return Err(ub(
                "`dealloc_array` of a pointer that does not point to a heap allocation".to_owned(),
                Vec::new(),
            ));
        }
        if !allocation.live {
            return Err(ub(
                "double free — this allocation was already freed".to_owned(),
                allocation.allocated_here(),
            ));
        }
        // Head check: the pointer `alloc_array` returned addresses element
        // 0; anything else (an `add` result, an interior element) does
        // not name the allocation.
        if path.as_slice() != [PathElem::Index(0)] {
            return Err(ub(
                "`dealloc_array` of a pointer that is not the head of its allocation \
                 — it points inside it"
                    .to_owned(),
                allocation.allocated_here(),
            ));
        }
        let len = match &allocation.value {
            Value::Array(values) => values.len() as u128,
            other => {
                let other = other.display();
                return Err(self.internal_error(
                    format!("a heap allocation held `{other}`, not an array"),
                    Some((loc.clone(), origin)),
                ));
            }
        };
        if count != len {
            return Err(ub(
                format!(
                    "`dealloc_array` with the wrong element count — this allocation \
                     has {len} element(s), but {count} were passed"
                ),
                allocation.allocated_here(),
            ));
        }
        // The frame-pop-kills-locals mechanism, applied to heap: the id is
        // never reused, so every surviving pointer into this allocation is
        // recognizably dangling forever (that is the whole double-free /
        // use-after-free detection story).
        let allocation = self.memory.get_mut(alloc).expect("checked just above");
        allocation.live = false;
        Ok(Value::Unit)
    }

    /// `add(p, i)`: pointer to element (head-index + i) of the same
    /// allocation. Minting is UNCHECKED per the shipped rule — no bounds
    /// judgement here; an out-of-range result is detected UB at its first
    /// deref, exactly like `a[i].&raw mut` past the end. The one shape the
    /// abstract machine cannot represent — advancing a pointer that does
    /// not address an array element (a lone local, a record field) — is
    /// refused as detected UB at the call (with `i == 0` as the harmless
    /// identity, matching `ptr.add(0)`).
    fn builtin_add(
        &mut self,
        p: &Value,
        i: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Ptr { alloc, path, tag } = p else {
            return Err(self.ill_typed("a raw pointer", p, loc, origin));
        };
        // The checker pins `add`'s index to `usize`; a wrong kind is
        // deferred-error mode — trap ill-typed, don't launder.
        let Some(i) = (match i {
            Value::Int(iv) => iv.usize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` count", i, loc, origin));
        };
        if i == 0 {
            return Ok(Value::Ptr {
                alloc: *alloc,
                path: path.clone(),
                tag: *tag,
            });
        }
        let mut path = path.clone();
        match path.last_mut() {
            Some(PathElem::Index(index)) => {
                // Saturating on purpose: an `add` past `u64::MAX` cannot
                // name a real element of any allocation, so the saturated
                // address is out of bounds at every deref — the ordinary
                // detected-UB story, no extra failure mode.
                *index = u64::try_from(i)
                    .ok()
                    .and_then(|i| index.checked_add(i))
                    .unwrap_or(u64::MAX);
                Ok(Value::Ptr {
                    alloc: *alloc,
                    path,
                    tag: *tag,
                })
            }
            _ => Err(EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: "`add` of a pointer that does not address an array element".to_owned(),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            }),
        }
    }

    /// `offset(p, i)`: the signed sibling of [`Self::builtin_add`] —
    /// element arithmetic both directions. Minting stays unchecked
    /// *upward* (validity past the end is a deref-time judgement, exactly
    /// like `add`), but a result index below the allocation's start
    /// (index < 0) is detected UB at the call: the abstract machine has no
    /// representation for an address before element 0 — the same
    /// precondition class as `add`'s non-array-element rule.
    fn builtin_offset(
        &mut self,
        p: &Value,
        i: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let Value::Ptr { alloc, path, tag } = p else {
            return Err(self.ill_typed("a raw pointer", p, loc, origin));
        };
        // The checker pins `offset`'s argument to `isize`; a wrong kind is
        // deferred-error mode — trap ill-typed, don't launder.
        let Some(i) = (match i {
            Value::Int(iv) => iv.isize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("an `isize` count", i, loc, origin));
        };
        if i == 0 {
            return Ok(Value::Ptr {
                alloc: *alloc,
                path: path.clone(),
                tag: *tag,
            });
        }
        let mut path = path.clone();
        match path.last_mut() {
            Some(PathElem::Index(index)) => {
                let result = i128::from(*index) + i;
                if result < 0 {
                    return Err(EvalError {
                        kind: EvalErrorKind::UndefinedBehavior,
                        message: "`offset` result points below the start of the allocation"
                            .to_owned(),
                        origin: Some((loc.clone(), origin)),
                        notes: Vec::new(),
                    });
                }
                // Saturating upward, like `add`: a past-`u64::MAX` address
                // is out of bounds at every deref.
                *index = u64::try_from(result).unwrap_or(u64::MAX);
                Ok(Value::Ptr {
                    alloc: *alloc,
                    path,
                    tag: *tag,
                })
            }
            _ => Err(EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: "`offset` of a pointer that does not address an array element".to_owned(),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            }),
        }
    }

    /// `copy(src, dst, n)`: element-count bulk copy, memmove semantics —
    /// the source range is read out in full before the destination is
    /// written, so overlap (any overlap, same allocation included) is
    /// DEFINED, not UB. Deliberately bypasses the tracked-uninit read gate:
    /// `copy` transports poison silently (a copy of a partially-written
    /// buffer must not lie); only reading an element AS A VALUE traps.
    /// Liveness, writability, range validity, and aliasing are checked
    /// exactly like derefs — violations are detected UB. The source is
    /// judged and read in full ([`Machine::read_range`]) before the
    /// destination is judged and written ([`Machine::write_range`]); a
    /// source that is merely foreign to a live borrow does not fault here —
    /// it SUSPENDS that borrow, same as any other read through the tag —
    /// but a program whose source read faults outright (a read through a
    /// borrow whose tag is no longer valid) AND whose destination is out
    /// of bounds reports the source fault first, memmove's own statement
    /// of "read, then write".
    fn builtin_copy(
        &mut self,
        src: &Value,
        dst: &Value,
        n: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        // The checker pins this count to `usize`; a wrong kind is
        // deferred-error mode — trap ill-typed, don't launder.
        let Some(n) = (match n {
            Value::Int(iv) => iv.usize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` element count", n, loc, origin));
        };
        // A zero-length copy is valid through ANY pointers — dangling
        // included (Rust's rule, and what lets a growing container copy
        // its 0 elements out of the never-allocated `dangling()` buffer
        // without a special case). `read_range`/`write_range` still check
        // that `src`/`dst` are pointers at all ([`Machine::checked_range`]'s
        // shape check runs even for `n == 0`), but judge no bounds,
        // aliasing, or writability for a range with nothing in it.
        let elements = self.read_range(src, n, "copy", "source", loc, origin)?;
        self.write_range(dst, elements, "copy", "destination", loc, origin)?;
        Ok(Value::Unit)
    }

    /// `str_from_utf8(p, len)` and `str_from_utf8_unchecked(p, len)` — the
    /// two blesses, and the whole of Must's bytes-to-text story.
    ///
    /// A `str` view over bytes is a VALIDITY CLAIM, and the claim has two
    /// halves that are deliberately separated. That `p` addresses `len`
    /// readable bytes is the CALLER's, unchecked in both spellings, which
    /// is why both are `unsafe`; the interpreter still catches the cases
    /// its typed memory can see (out of range, freed, never written, or
    /// foreign to a live safe borrow of one of the bytes). Whether those
    /// bytes SPELL a string is answered by `str_from_utf8` and merely
    /// asserted by `str_from_utf8_unchecked` — and a false assertion is
    /// undefined behavior, DETECTED here, because a `str` whose contents are
    /// not a string is a value the language's own invariant says cannot
    /// exist. Producing one silently is exactly what this machine is for.
    fn builtin_bless(
        &mut self,
        builtin: Builtin,
        p: &Value,
        len: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let name = builtin.name();
        // The checker pins this length to `usize`; a wrong kind is
        // deferred-error mode — trap ill-typed, don't launder.
        let Some(len) = (match len {
            Value::Int(iv) => iv.usize_payload(),
            _ => None,
        }) else {
            return Err(self.ill_typed("a `usize` byte count", len, loc, origin));
        };
        // A zero-length bless still requires `p` to be a pointer (the
        // shape [`Machine::checked_range`] always checks) but judges no
        // bounds, aliasing, or written-ness for it — `copy`'s rule, and it
        // is what lets a line scanner bless an empty line (a bare "\n", or
        // a buffer's very start) with no special case. The empty string is
        // valid UTF-8, so both spellings answer the same thing.
        let elements = self.read_range(p, len, name, "buffer", loc, origin)?;
        let mut bytes = Vec::with_capacity(elements.len());
        for element in &elements {
            match element {
                // Strictly `u8`: the checker pins the pointee, so any
                // other width reaching here is deferred-error mode — trap
                // ill-typed, don't launder a wider integer into a byte.
                Value::Int(hir::IntValue::U8(byte)) => bytes.push(*byte),
                // Reading never-written memory AS A VALUE is UB, and a
                // bless is a value read of every byte in the range.
                Value::Uninit => return Err(self.uninit_read(loc, origin)),
                other => return Err(self.ill_typed("a `u8` element", other, loc, origin)),
            }
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(self.blessed(builtin, text, loc)),
            Err(err) => match builtin {
                Builtin::StrFromUtf8 => Ok(builtin_variant(
                    loc.file,
                    hir::UTF8_RESULT_NAME,
                    "Err",
                    Vec::new(),
                )),
                _ => Err(EvalError {
                    kind: EvalErrorKind::UndefinedBehavior,
                    message: format!(
                        "`str_from_utf8_unchecked` was given bytes that are not valid \
                         UTF-8 — the first bad byte is at offset {}",
                        err.utf8_error().valid_up_to()
                    ),
                    origin: Some((loc.clone(), origin)),
                    notes: Vec::new(),
                }),
            },
        }
    }

    /// The successful answer of a bless, in whichever shape the spelling
    /// asks for: the checked one wraps it in `Utf8Result::Ok`, the claimed
    /// one hands back the `str` itself.
    fn blessed(&self, builtin: Builtin, text: String, loc: &ItemLoc) -> Value {
        match builtin {
            Builtin::StrFromUtf8 => builtin_variant(
                loc.file,
                hir::UTF8_RESULT_NAME,
                "Ok",
                vec![Value::Str(text)],
            ),
            _ => Value::Str(text),
        }
    }

    /// The BOUNDS half of the shared range judgement, common to a read and
    /// a write alike: `p` must be a pointer, and — unless `n == 0` — a
    /// LIVE one addressing an array element, with `n` elements starting
    /// there sitting inside the array (deref-like on both ends, the same
    /// UB-detection spirit as A03/A04). A zero-length range checks only
    /// that `p` is a pointer at all and answers empty without walking the
    /// allocation — dangling included, `copy`'s rule — which is what lets
    /// a zero-length read or write through ANY pointer, live or not,
    /// answer nothing rather than a special case. Neither aliasing nor
    /// writability is this function's to judge: see [`Machine::read_range`]
    /// and [`Machine::judge_write_range`].
    fn checked_range(
        &self,
        p: &Value,
        n: u128,
        what: &str,
        role: &str,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<&[Value], EvalError> {
        let Value::Ptr {
            alloc,
            path,
            tag: _,
        } = p
        else {
            return Err(self.ill_typed("a raw pointer", p, loc, origin));
        };
        if n == 0 {
            return Ok(&[]);
        }
        let allocation = self.allocation_for_deref(*alloc, loc, origin)?;
        let Some(PathElem::Index(head)) = path.last() else {
            return Err(EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: format!("`{what}` {role} pointer does not address an array element"),
                origin: Some((loc.clone(), origin)),
                notes: Vec::new(),
            });
        };
        let parent = &path[..path.len() - 1];
        let value = self.follow_ptr_path(&allocation.value, parent, loc, origin)?;
        let Value::Array(values) = value else {
            return Err(self.ill_typed("an array value", value, loc, origin));
        };
        let len = values.len() as u128;
        let head = *head as u128;
        if head.checked_add(n).is_none_or(|end| end > len) {
            return Err(EvalError {
                kind: EvalErrorKind::UndefinedBehavior,
                message: format!(
                    "`{what}` out of bounds — the {role} names {n} element(s) from \
                     index {head}, but the array has {len}"
                ),
                origin: Some((loc.clone(), origin)),
                notes: allocation.allocated_here(),
            });
        }
        Ok(&values[head as usize..(head + n) as usize])
    }

    /// The READ half: bounds ([`Machine::checked_range`]) plus the
    /// aliasing tree — a live safe borrow of one of the `n` elements must
    /// be foreign to this read, same as an ordinary `p.*[i]` would be
    /// ([`Machine::aliasing_access_range`]). Returns the elements read
    /// (clones — `copy`'s read-before-write is what makes its overlap
    /// defined). A zero-length range judges neither bounds nor aliasing,
    /// per [`Machine::checked_range`].
    fn read_range(
        &mut self,
        p: &Value,
        n: u128,
        what: &str,
        role: &str,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Vec<Value>, EvalError> {
        let elements = self.checked_range(p, n, what, role, loc, origin)?.to_vec();
        if n == 0 {
            return Ok(elements);
        }
        let Value::Ptr { alloc, path, tag } = p else {
            unreachable!("checked_range verified the pointer shape");
        };
        self.aliasing_access_range(*tag, *alloc, path, n, Access::Read, loc, origin)?;
        Ok(elements)
    }

    /// The WRITE judgement, without the store: bounds
    /// ([`Machine::checked_range`]), then the aliasing tree — a live safe
    /// borrow of one of the `n` elements must be foreign to this write,
    /// same as an ordinary `p.*[i] = v` would be
    /// ([`Machine::aliasing_access_range`]) — then writability
    /// ([`Machine::writable_allocation`]). This is the half a primitive
    /// that must judge a write BEFORE consuming its input needs (the host
    /// `read` builtin, on the full requested length, before it draws from
    /// stdin); [`Machine::write_range`] is this plus the store. A
    /// zero-length range judges only that `dst` is a pointer, per
    /// [`Machine::checked_range`].
    fn judge_write_range(
        &mut self,
        dst: &Value,
        n: u128,
        what: &str,
        role: &str,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        self.checked_range(dst, n, what, role, loc, origin)?;
        if n == 0 {
            return Ok(());
        }
        let Value::Ptr { alloc, path, tag } = dst else {
            unreachable!("checked_range verified the pointer shape");
        };
        self.aliasing_access_range(*tag, *alloc, path, n, Access::Write, loc, origin)?;
        self.writable_allocation(*alloc, loc, origin)?;
        Ok(())
    }

    /// The WRITE twin of [`Machine::read_range`], complete:
    /// [`Machine::judge_write_range`] (bounds, aliasing, writability), then
    /// the elements land IN PLACE, one per array element. `what` and
    /// `role` name the primitive and its pointer in the range diagnostics,
    /// spelled exactly as [`Machine::read_range`] spells them. A
    /// zero-length write judges only that `dst` is a pointer, per
    /// [`Machine::checked_range`], and stores nothing — the same shortcut
    /// [`Machine::read_range`] and [`Machine::judge_write_range`] take, and
    /// this home must take it too: `dst` is not guaranteed to address an
    /// array element when there is nothing to store.
    fn write_range(
        &mut self,
        dst: &Value,
        elements: Vec<Value>,
        what: &str,
        role: &str,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        self.judge_write_range(dst, elements.len() as u128, what, role, loc, origin)?;
        if elements.is_empty() {
            return Ok(());
        }
        let Value::Ptr {
            alloc,
            path,
            tag: _,
        } = dst
        else {
            unreachable!("judge_write_range verified the pointer shape");
        };
        let Some(PathElem::Index(head)) = path.last() else {
            unreachable!("judge_write_range verified the element shape");
        };
        let head = *head as usize;
        let parent = &path[..path.len() - 1];
        let allocation = self
            .memory
            .get_mut(alloc)
            .expect("judge_write_range judged liveness");
        let slot = match project_path_mut(&mut allocation.value, parent) {
            Ok(slot) => slot,
            Err(error) => return Err(self.ptr_path_error(error, loc, origin)),
        };
        let Value::Array(values) = slot else {
            unreachable!("judge_write_range verified the array shape");
        };
        for (offset, element) in elements.into_iter().enumerate() {
            values[head + offset] = element;
        }
        Ok(())
    }

    /// `dangling::<T>()`: the reserved never-live pointer (there is no
    /// null) — minted once per machine, so every `dangling()` compares
    /// equal, and `allocation_for_deref` reports every deref as "never
    /// valid". Shaped like a heap head pointer (element 0 of an
    /// empty never-live array) so `add` arithmetic on it mints
    /// (unchecked, as always) instead of erroring.
    fn builtin_dangling(&mut self) -> Value {
        let alloc = match self.dangling_alloc {
            Some(alloc) => alloc,
            None => {
                let alloc = self.fresh_alloc(Allocation {
                    value: Value::Array(Vec::new()),
                    live: false,
                    writable: false,
                    kind: AllocKind::Dangling,
                    origin: None,
                });
                self.dangling_alloc = Some(alloc);
                alloc
            }
        };
        Value::Ptr {
            alloc,
            path: vec![PathElem::Index(0)],
            tag: Provenance::default(),
        }
    }

    fn spend_fuel(&mut self, loc: &ItemLoc) -> Result<(), EvalError> {
        self.spend_fuel_n(1, loc)
    }

    /// Charge `amount` units of const fuel — bulk work (an array repeat's
    /// elements) pays proportionally, so one statement can't dodge the
    /// budget by doing its looping inside the machine.
    fn spend_fuel_n(&mut self, amount: u64, loc: &ItemLoc) -> Result<(), EvalError> {
        if self.const_depth == 0 {
            return Ok(());
        }
        self.const_fuel = self.const_fuel.saturating_sub(amount);
        if self.const_fuel == 0 {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: "constant evaluation ran out of fuel".to_owned(),
                origin: root_origin(self.db, loc),
                notes: Vec::new(),
            });
        }
        Ok(())
    }

    /// Invariant violations: values that should have been trapped before
    /// execution could touch them. Loud, like the diagnostics tripwire.
    fn internal_error(&self, detail: String, origin: Option<(ItemLoc, ExprId)>) -> EvalError {
        EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!(
                "internal error: {detail} — this is a bug in the Must language server"
            ),
            origin,
            notes: Vec::new(),
        }
    }

    fn ill_typed(&self, expected: &str, found: &Value, loc: &ItemLoc, origin: ExprId) -> EvalError {
        self.internal_error(
            format!("expected {expected}, found `{}`", found.display()),
            Some((loc.clone(), origin)),
        )
    }
}

/// A tagged value of one of the compiler-provided enums — the interpreter's
/// ONE way to build one (`hir::synthetic_decls` is the declaration).
///
/// The tag comes off the ROW: a variant's index is its position in the
/// table, the same order `type_decl` hands the rest of the compiler, so a
/// table edit that reorders variants moves the runtime tag with it. Spelling
/// `index: 1, name: "End"` at the call instead would be a second copy of the
/// order — and `TerminatorKind::SwitchVariant` dispatches on the INDEX
/// alone, so the two disagreeing would misroute values with nothing to
/// notice it.
///
/// Panics if `enum_name` names no row or `variant` no variant of it: both
/// are typos in this file, not states a program can reach.
fn builtin_variant(
    file: base_db::SourceFile,
    enum_name: &str,
    variant: &str,
    payload: Vec<Value>,
) -> Value {
    let row = hir::synthetic_decl_named(enum_name)
        .unwrap_or_else(|| panic!("`{enum_name}` is not a compiler-provided enum"));
    let (index, payload_types) = row
        .variant(variant)
        .unwrap_or_else(|| panic!("`{enum_name}` has no variant `{variant}`"));
    debug_assert_eq!(
        payload.len(),
        payload_types.len(),
        "`{enum_name}::{variant}` was built with the wrong number of payload slots"
    );
    Value::Variant {
        decl: hir::synthetic_decl_loc(file, enum_name),
        index,
        name: variant.to_owned(),
        payload,
    }
}

/// Navigate a pointer's element-granular path to the value it names.
/// Out-of-bounds element steps are structured (a pointer minted past the
/// end of an array — address-taking never bounds-checks — is detected UB
/// at its first deref); a shape mismatch means a pointer the type system
/// should have refused was materialized — an internal error. The caller
/// classifies (see `Machine::ptr_path_error`).
fn project_path<'v>(slot: &'v Value, path: &[PathElem]) -> Result<&'v Value, ProjectError> {
    let mut current = slot;
    for elem in path {
        current = match (elem, current) {
            // Stepping INTO uninitialized memory: its structure was never
            // written — detected UB, not a shape violation.
            (_, Value::Uninit) => return Err(ProjectError::Uninit),
            (PathElem::Field(index), value) => field_step(value, *index)?,
            (PathElem::Index(index), Value::Array(values)) => {
                let len = values.len() as u128;
                let index = *index as u128;
                match values.get(index as usize) {
                    Some(element) => element,
                    None => return Err(ProjectError::OutOfBounds { len, index }),
                }
            }
            (PathElem::Index(_), other) => {
                return Err(ProjectError::Shape(format!(
                    "expected an array value to project into, found `{}`",
                    other.display()
                )));
            }
        };
    }
    Ok(current)
}

/// [`project_path`], mutably — the store side.
fn project_path_mut<'v>(
    slot: &'v mut Value,
    path: &[PathElem],
) -> Result<&'v mut Value, ProjectError> {
    let mut current = slot;
    for elem in path {
        current = match (elem, current) {
            // Stepping INTO uninitialized memory — see `project_path`.
            // (A path ENDING at an uninit slot succeeds: the write
            // replaces the poison — that is how elements get initialized.)
            (_, Value::Uninit) => return Err(ProjectError::Uninit),
            (PathElem::Field(index), value) => field_step_mut(value, *index)?,
            (PathElem::Index(index), Value::Array(values)) => {
                let len = values.len() as u128;
                let index = *index as u128;
                if index >= len {
                    return Err(ProjectError::OutOfBounds { len, index });
                }
                &mut values[index as usize]
            }
            (PathElem::Index(_), other) => {
                return Err(ProjectError::Shape(format!(
                    "expected an array value to project into, found `{}`",
                    other.display()
                )));
            }
        };
    }
    Ok(current)
}

/// One FIELD step of a path walk: a record field by canonical index, or a
/// variant/tuple payload element by position.
///
/// The payload arms are what makes a match-through-a-borrow binding
/// reachable — the binder's pointer path steps into the matched value, and
/// every read and write through it walks that step back down. An enum-typed
/// value is a [`Value::Variant`]; an unwidened variant-TYPED one is a
/// tag-free [`Value::Tuple`], so both spell the same step.
///
/// Overrunning either is an internal error, not UB: arity is a
/// compile-time fact (the checker reports `PatArity`, and lowering plants a
/// placeholder), so a path that overruns one means lowering and the value
/// disagree. Shared by [`project_path`], [`project_path_mut`] and
/// `Machine::read_place`, which is what keeps the read and write sides
/// agreeing about what a field step means.
fn field_step(value: &Value, index: u32) -> Result<&Value, ProjectError> {
    match value {
        Value::Record { fields } => fields
            .get(index as usize)
            .map(|(_, field)| field)
            .ok_or_else(|| field_out_of_range("record field", index, fields.len())),
        Value::Variant { payload, .. } | Value::Tuple(payload) => payload
            .get(index as usize)
            .ok_or_else(|| field_out_of_range("variant payload", index, payload.len())),
        other => Err(ProjectError::Shape(format!(
            "expected a record, variant or tuple value to project into, found `{}`",
            other.display()
        ))),
    }
}

/// [`field_step`], mutably — the store half. Writing through a `.&mut`
/// payload binding mutates the matched value IN PLACE, which is the whole
/// point of projecting rather than copying.
fn field_step_mut(value: &mut Value, index: u32) -> Result<&mut Value, ProjectError> {
    match value {
        Value::Record { fields } => {
            let len = fields.len();
            fields
                .get_mut(index as usize)
                .map(|(_, field)| field)
                .ok_or_else(|| field_out_of_range("record field", index, len))
        }
        Value::Variant { payload, .. } | Value::Tuple(payload) => {
            let len = payload.len();
            payload
                .get_mut(index as usize)
                .ok_or_else(|| field_out_of_range("variant payload", index, len))
        }
        other => Err(ProjectError::Shape(format!(
            "expected a record, variant or tuple value to project into, found `{}`",
            other.display()
        ))),
    }
}

/// The overrun message both halves render, so the read and write sides
/// cannot drift apart on the wording.
fn field_out_of_range(what: &str, index: u32, len: usize) -> ProjectError {
    ProjectError::Shape(format!(
        "{what} index {index} out of range ({len} elements)"
    ))
}

/// How `Machine::resolve_place_alloc` treats the place's own element
/// steps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PathMode {
    /// `.&raw` address-taking: validity is a deref-time judgement (never a
    /// minting-time one, so stricter checks can be added later without
    /// breaking programs), so element steps append UNCHECKED. An
    /// out-of-range address mints silently; every later deref of it is
    /// detected UB.
    Mint,
    /// A store destination: the source-level `[i]` steps are the
    /// program's own checked indexing, so they bounds-check during
    /// resolution — an ordinary runtime trap, exactly like `a[i] = v;`
    /// and symmetric with the `p.*[i]` read — while the pointer's stored
    /// path stays a deref-time UB judgement.
    Store,
}

/// One resolved step of a place projection: index operands already
/// evaluated, ready to walk a value.
enum ResolvedProj {
    /// A record field, by canonical sorted index.
    Field(u32),
    /// An array element. Kept as the raw `u128` the index evaluated to so
    /// the bounds check compares honestly (a `usize` conversion would have
    /// to invent a verdict for huge indices).
    Index(u128),
    /// Follow the raw pointer at the current position: the rest of the
    /// place continues inside the pointee's allocation.
    Deref,
}

/// Why a place projection failed. Out-of-bounds element steps are
/// classified by the caller (an ordinary runtime trap for the program's
/// own `[i]` steps, detected UB for a pointer's stored path); a step that
/// lands on tracked-uninit is detected UB (projecting into a value that
/// was never written is reading its structure); everything else means a
/// value the checker should have refused was projected through — an
/// internal error.
enum ProjectError {
    OutOfBounds {
        len: u128,
        index: u128,
    },
    /// A NON-final step landed on [`Value::Uninit`]: the walk would read
    /// structure that was never written (a final slot of `Uninit` is
    /// fine — reads gate on it afterwards, writes replace it).
    Uninit,
    Shape(String),
}

/// Navigate a resolved projection to the nested record field or array
/// element it names, mutably — the write side of places.
fn project_mut<'v>(
    slot: &'v mut Value,
    projection: &[ResolvedProj],
) -> Result<&'v mut Value, ProjectError> {
    let mut current = slot;
    for elem in projection {
        match (elem, current) {
            // Stepping INTO uninitialized memory — see `project_path`.
            (_, Value::Uninit) => return Err(ProjectError::Uninit),
            (ResolvedProj::Field(index), Value::Record { fields }) => {
                let index = *index as usize;
                let len = fields.len();
                current = match fields.get_mut(index) {
                    Some((_, field)) => field,
                    None => {
                        return Err(ProjectError::Shape(format!(
                            "record field index {index} out of range ({len} elements)"
                        )));
                    }
                };
            }
            (ResolvedProj::Index(index), Value::Array(values)) => {
                let len = values.len() as u128;
                if *index >= len {
                    return Err(ProjectError::OutOfBounds { len, index: *index });
                }
                current = &mut values[*index as usize];
            }
            (ResolvedProj::Field(_), other) => {
                return Err(ProjectError::Shape(format!(
                    "expected a record value to assign into, found `{}`",
                    other.display()
                )));
            }
            (ResolvedProj::Index(_), other) => {
                return Err(ProjectError::Shape(format!(
                    "expected an array value to assign into, found `{}`",
                    other.display()
                )));
            }
            // Deref-containing destinations route through
            // `Machine::write_through`, never here.
            (ResolvedProj::Deref, _) => {
                return Err(ProjectError::Shape(
                    "a deref projection reached a frame-local write".to_owned(),
                ));
            }
        }
    }
    Ok(current)
}

/// Best-effort type of a runtime value, for displaying const params in the
/// debugger (their declared `TypeRef` isn't lowered here). Scalars and
/// records reconstruct exactly; a tagged variant value is its enum; the
/// carriers whose static type isn't recoverable from the value alone
/// (tag-free payloads, fn values) fall back to `{error}`, which only
/// demotes them from console-eval parameters — the variables panel still
/// shows name and value.
fn value_ty(value: &Value) -> hir::Ty {
    match value {
        Value::Unit => hir::Ty::Unit,
        Value::Int(iv) => hir::Ty::Int(iv.kind()),
        Value::Str(_) => hir::Ty::Str,
        Value::Bool(_) => hir::Ty::Bool,
        Value::Char(_) => hir::Ty::Char,
        Value::Record { fields } => hir::Ty::record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), value_ty(value)))
                .collect(),
        ),
        // The element type comes from the first element; an empty array's
        // is unrecoverable from the value alone — `{error}`, which only
        // demotes it from console-eval parameters.
        Value::Array(values) => match values.first() {
            Some(first) => {
                let elem = value_ty(first);
                if elem.contains_error() {
                    hir::Ty::Error
                } else {
                    hir::Ty::array(elem, hir::ConstArgValue::Int(values.len() as u128))
                }
            }
            None => hir::Ty::Error,
        },
        // Generic args are erased at runtime, so the recovered type is the
        // bare declaration (`Option`, args unknown) — enough for the
        // variables panel; console-eval parameters demote like the other
        // unrecoverable carriers when the static type was an instance.
        Value::Variant { decl, .. } => hir::Ty::Named(hir::NamedTy::plain(decl.clone())),
        // A pointer's pointee type isn't recoverable from the value alone
        // (and a pointer must not cross into a fresh console-eval machine
        // anyway — its allocation lives here): `{error}` demotes it from
        // console-eval parameters, the variables panel still shows it.
        Value::Fn(_)
        | Value::ExternFn { .. }
        | Value::Builtin(_)
        | Value::Tuple(_)
        | Value::Ptr { .. } => hir::Ty::Error,
        // Machine-internal poison: no surface type exists for it.
        Value::Uninit => hir::Ty::Error,
    }
}

/// The host `read`'s machine-shaped count, built in whichever Must spelling
/// of a signed machine word the declaration asked for.
fn count_value(kind: IntKind, count: i64) -> Value {
    Value::Int(match kind {
        IntKind::I64 => hir::IntValue::I64(count),
        _ => hir::IntValue::Isize(count),
    })
}

fn local_name(body: &MirBody, local: LocalId) -> String {
    match &body.locals[local] {
        LocalData {
            name: Some(name), ..
        } => format!("local `{name}`"),
        _ => "temporary".to_owned(),
    }
}

/// A best-effort origin for errors about an item as a whole (cycles, fuel):
/// its root expression.
fn root_origin(db: &dyn Db, loc: &ItemLoc) -> Option<(ItemLoc, ExprId)> {
    let root = hir::body::body(db, loc.to_id(db)).root?;
    Some((loc.clone(), root))
}

// ---- the aliasing tree (Tree Borrows structure, no-Reserved launch) -----
//
// Static exclusivity — which borrows may be live at once — is not built
// yet (no loan liveness). Until it exists the INTERPRETER answers the
// same question dynamically, which is the house pattern: the unsafe
// substrate is checked at runtime while the static story is built
// (`alloc_array`'s UB detection got exactly this treatment).
//
// The structure is Tree Borrows': every safe borrow mints a NODE that is a
// child of the node its parent pointer speaks through, and an access
// through one node changes the state of every node it is foreign to. The
// launch configuration is the ruled one: there is NO `Reserved` phase —
// `&mut` starts `Unique`. Rust cannot do that (two-phase borrows depend on
// it); Must can, because self-last evaluation made two-phase borrows
// unnecessary. Relaxing to `Reserved` later is pure UB removal, so nothing
// written against this can break.
//
// Scope: a raw pointer minted through a deref inherits the node its
// parent pointer speaks through, per the ruling that `.&raw` is not a
// decayed safe borrow. A raw pointer minted straight off a bare local's
// name (`n.&raw mut`) carries the untracked marker (`Provenance(None)`)
// until it is resolved against an access, at which point it inherits
// that local's ROOT if one already exists — the root IS the local's own
// storage, so the two must be the same node. Only an allocation no safe
// borrow has EVER covered has no root to inherit, and a raw access to it
// is then a true no-op.
//
// Per-location state is APPROXIMATED, not per-allocation: every node also
// carries the element path (field/index chain) it was minted over, and
// two nodes interact only when one path is a prefix of the other (they
// could name overlapping memory). `p.x.&mut` and `p.y.&mut` therefore
// coexist — disjoint fields of the same struct never alias — while
// `p.&mut` (path `[]`, the whole value) still dominates both, and every
// access through a place — read or write, deref-routed or not — carries
// the path of the chain it names, so neither `p.y = 5;` nor `let v =
// p.y;` can disturb a borrow of `p.x`. That is a joint property with
// `mir::lower`: a chain only has a path here because it arrives as ONE
// projected place (`lower_place_read`) instead of a copy of its root.
// The approximation: a node minted over a WIDE path is disabled
// wholesale by any foreign access into any part of it, never partially —
// true sub-node partitioning is not built (M10, Ruled-not-built). See
// `paths_overlap`.

/// The scan lists of one allocation — see [`Machine::borrow_nodes_of`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AllocNodes {
    /// Nodes still in `Unique`: the only ones a READ can affect.
    unique: Vec<BorrowTag>,
    /// Nodes in `Frozen`: a read cannot touch them, a write disables them.
    settled: Vec<BorrowTag>,
}

/// One node of an allocation's aliasing tree.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BorrowNode {
    alloc: AllocId,
    /// `None` for an allocation's root.
    parent: Option<BorrowTag>,
    /// The element path this node covers — empty for the allocation's
    /// root (the whole value) and for a borrow of the whole local. Two
    /// nodes are only ever foreign to each other when their paths
    /// overlap (see `paths_overlap`), which is what lets disjoint
    /// fields/elements of one allocation be borrowed independently.
    path: Vec<PathElem>,
    state: NodeState,
    /// Where this borrow was created — `None` for a root, which no
    /// expression writes.
    born: Option<(ItemLoc, ExprId)>,
    /// Where it was invalidated, once it has been. Carried so the report
    /// can name the OTHER site: a bare "this borrow is no longer valid"
    /// at the use tells you nothing about which access killed it, and the
    /// use is routinely inside a callee while both interesting sites are
    /// in the caller.
    invalidated: Option<(ItemLoc, ExprId)>,
}

/// Whether an access covering `a` and a node covering `b` could name
/// overlapping memory — true exactly when one path is a prefix of the
/// other (including the equal case). Two field/index steps that diverge
/// anywhere along the shorter path are provably disjoint locations.
fn paths_overlap(a: &[PathElem], b: &[PathElem]) -> bool {
    a.iter().zip(b).all(|(x, y)| x == y)
}

/// Convert a deref-free resolved projection (the program's own field/
/// index steps into a local's own storage) into the element path the
/// aliasing tree keys on. Index operands are the program's own checked
/// `[i]`, already bounds-verified by the caller, so the `u64` conversion
/// is infallible in practice; `u64::MAX` is a safe, non-panicking
/// fallback since this path is used only for aliasing-tree comparisons,
/// never to address memory.
fn resolved_proj_path(projection: &[ResolvedProj]) -> Vec<PathElem> {
    projection
        .iter()
        .map(|elem| match elem {
            ResolvedProj::Field(index) => PathElem::Field(*index),
            ResolvedProj::Index(index) => {
                PathElem::Index(u64::try_from(*index).unwrap_or(u64::MAX))
            }
            ResolvedProj::Deref => {
                unreachable!("resolved_proj_path is only called on a deref-free projection")
            }
        })
        .collect()
}

/// What a node currently permits. No `Reserved`: the ruled launch starts
/// `&mut` at `Unique`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeState {
    /// Exclusive: reads and writes.
    Unique,
    /// Shared: reads only.
    Frozen,
    /// Invalidated by a conflicting access through another node. Any use
    /// is undefined behavior.
    Disabled,
}

/// Which kind of access is happening — the only axis the transitions need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Write,
}

impl<M> Machine<'_, M> {
    /// The root node of `alloc`, created on first use.
    fn borrow_root(&mut self, alloc: AllocId) -> BorrowTag {
        if let Some(&tag) = self.borrow_roots.get(&alloc) {
            return tag;
        }
        let tag = BorrowTag(self.borrow_nodes.len() as u32);
        self.borrow_nodes.push(BorrowNode {
            alloc,
            parent: None,
            path: Vec::new(),
            state: NodeState::Unique,
            born: None,
            invalidated: None,
        });
        self.borrow_roots.insert(alloc, tag);
        self.borrow_nodes_of
            .entry(alloc)
            .or_default()
            .unique
            .push(tag);
        tag
    }

    /// Whether `node` is `ancestor` or below it.
    fn is_descendant_of(&self, node: BorrowTag, ancestor: BorrowTag) -> bool {
        let mut current = Some(node);
        while let Some(tag) = current {
            if tag == ancestor {
                return true;
            }
            current = self.borrow_nodes[tag.0 as usize].parent;
        }
        false
    }

    /// Perform an access through `tag`, covering element path `path` —
    /// the whole dynamic check.
    ///
    /// Two things happen, in order. The accessing node must still permit
    /// the access (a disabled node is undefined behavior, which is the
    /// use-after-parent-invalidated case: disabling is transitive). Then
    /// every node the access is FOREIGN to — a strict descendant of the
    /// accessor, or an unrelated cousin, WHOSE PATH OVERLAPS `path` —
    /// reacts: a foreign write disables it, a foreign read freezes it.
    /// Ancestors are untouched: an access through a child is not foreign
    /// to the parent it was reborrowed from, which is precisely what
    /// makes reborrow-at-every-use work. A node whose path names a
    /// disjoint field/element is untouched for the same reason a cousin
    /// allocation would be: it cannot be the same memory.
    fn aliasing_access(
        &mut self,
        tag: Provenance,
        alloc: AllocId,
        path: &[PathElem],
        access: Access,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let tag = match tag {
            Provenance(Some(tag)) => tag,
            // A raw pointer minted directly from a bare local's own name
            // (never through a safe borrow) inherits that local's ROOT —
            // once one exists. `n.&raw mut` and `n`'s own name are the
            // SAME storage, so they must be the same node: if a safe
            // borrow has already covered `n` (creating its root), a raw
            // write here is exactly as foreign to that borrow as a plain
            // `n = ...` would be. Only when no safe borrow has EVER
            // covered this allocation is there no root to inherit, and
            // the access is a true no-op — see
            // `a_raw_only_program_never_touches_the_aliasing_tree`.
            Provenance(None) => match self.borrow_roots.get(&alloc) {
                Some(&root) => root,
                None => return Ok(()),
            },
        };
        let node = &self.borrow_nodes[tag.0 as usize];
        // The two sites a reader actually needs: where this borrow came
        // from, and what killed it. Both are usually in the caller while
        // the failing use is inside a callee.
        let mut notes = Vec::new();
        if let Some(born) = node.born.clone() {
            notes.push(EvalNote {
                message: "this borrow was created here".to_owned(),
                origin: Some(born),
            });
        }
        if let Some(invalidated) = node.invalidated.clone() {
            // What happened there is exactly what the node's state says:
            // a foreign READ only suspends an exclusive borrow (`Frozen`),
            // a foreign write or reborrow kills it outright. The headline
            // below draws the same distinction, and the two must not
            // disagree about the same source location.
            notes.push(EvalNote {
                message: if node.state == NodeState::Frozen {
                    "suspended here — the place was read while this exclusive borrow was live"
                } else {
                    "invalidated here — the value was borrowed again, \
                     written through another borrow, or moved away"
                }
                .to_owned(),
                origin: Some(invalidated),
            });
        }
        let ub = |message: &str| EvalError {
            kind: EvalErrorKind::UndefinedBehavior,
            message: message.to_owned(),
            origin: Some((loc.clone(), origin)),
            notes: notes.clone(),
        };
        match self.borrow_nodes[tag.0 as usize].state {
            NodeState::Disabled => {
                let verb = match access {
                    Access::Read => "read",
                    Access::Write => "write",
                };
                return Err(ub(&format!(
                    "{verb} through a borrow that is no longer valid: the value was \
                     borrowed again, written through another borrow, or moved away, \
                     while this borrow was still live"
                )));
            }
            NodeState::Frozen if access == Access::Write => {
                // A node reaches `Frozen` two ways, and they are different
                // stories. A `.&mut` was frozen by a foreign READ
                // (something else looked at the place while this borrow
                // was live), and calling that "a shared borrow" is simply
                // false of the value being written through. A `.&` was
                // BORN frozen — a flavor error, which the checker refuses
                // statically before it can run (no write may travel
                // through a shared step), so that arm is the backstop for
                // a path the static rule does not see.
                return Err(ub(
                    if self.borrow_nodes[tag.0 as usize].invalidated.is_some() {
                        "write through a borrow that was suspended by a read of the same place \
                     while this borrow was live"
                    } else {
                        "write through a shared borrow — only `.&mut` may write through a borrow"
                    },
                ));
            }
            NodeState::Unique | NodeState::Frozen => {}
        }
        // A read scans only the exclusive nodes: it can do nothing to a
        // node that is already `Frozen`, and nothing at all to a
        // `Disabled` one. A write has to see both.
        let scan: Vec<BorrowTag> = match self.borrow_nodes_of.get(&alloc) {
            None => Vec::new(),
            Some(list) => match access {
                Access::Read => list.unique.clone(),
                Access::Write => list.unique.iter().chain(&list.settled).copied().collect(),
            },
        };
        let mut touched = false;
        for other in scan {
            if other == tag {
                continue;
            }
            // An ancestor of the accessor is not foreign to it.
            if self.is_descendant_of(tag, other) {
                continue;
            }
            // A node whose path is disjoint from this access's path
            // cannot name the same memory — two borrows into different
            // fields/elements of one allocation never interact.
            if !paths_overlap(path, &self.borrow_nodes[other.0 as usize].path) {
                continue;
            }
            touched = true;
            let victim = &mut self.borrow_nodes[other.0 as usize];
            match access {
                Access::Write => {
                    if victim.state != NodeState::Disabled {
                        victim.invalidated = Some((loc.clone(), origin));
                    }
                    victim.state = NodeState::Disabled;
                }
                Access::Read => {
                    if victim.state == NodeState::Unique {
                        victim.state = NodeState::Frozen;
                        victim.invalidated = Some((loc.clone(), origin));
                    }
                }
            }
        }
        // Re-file whatever changed state: demoted nodes move out of the
        // exclusive list (a read will never need them again) and disabled
        // ones leave both. Both moves are forced by the state machine, so
        // neither can hide a violation — a node that is still able to
        // react is still scanned by the access that could make it react.
        // Skipped entirely when the scan changed nothing — which is the
        // common case and the one that decides the cost: an access that
        // finds only ancestors (every access in a loop that borrows the
        // same place) must not pay for the whole history to be re-filed.
        if touched && let Some(list) = self.borrow_nodes_of.get_mut(&alloc) {
            let nodes = &self.borrow_nodes;
            let mut demoted: Vec<BorrowTag> = Vec::new();
            list.unique
                .retain(|node| match nodes[node.0 as usize].state {
                    NodeState::Unique => true,
                    NodeState::Frozen => {
                        demoted.push(*node);
                        false
                    }
                    NodeState::Disabled => false,
                });
            list.settled
                .retain(|node| nodes[node.0 as usize].state != NodeState::Disabled);
            list.settled.extend(demoted);
        }
        Ok(())
    }

    /// `aliasing_access` for the `n` array elements a range judges,
    /// starting at `path` (whose last step is the head element's `Index`)
    /// — one call per element, since a node's path is an exact
    /// field/index chain and cannot name a whole range at once. Reading or
    /// writing a range is not a route around the tree: it must be exactly
    /// as foreign to a live safe borrow of one of its elements as an
    /// ordinary `p.*[i]`/`p.*[i] = v` would be. The two homes that call
    /// this are [`Machine::read_range`]'s and [`Machine::judge_write_range`]'s
    /// own aliasing step, so every range primitive built on them — present
    /// or future — is covered by construction, not by a list of names.
    fn aliasing_access_range(
        &mut self,
        tag: Provenance,
        alloc: AllocId,
        path: &[PathElem],
        n: u128,
        access: Access,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let Some((PathElem::Index(head), prefix)) = path.split_last() else {
            // `checked_range` already rejected a pointer whose path does
            // not end in an `Index` — unreachable in practice, but a
            // no-op rather than a panic if this is ever called before
            // that check runs.
            return Ok(());
        };
        let mut elem_path = prefix.to_vec();
        elem_path.push(PathElem::Index(*head));
        for offset in 0..n {
            let index = *head + u64::try_from(offset).unwrap_or(u64::MAX);
            *elem_path.last_mut().expect("just pushed") = PathElem::Index(index);
            self.aliasing_access(tag, alloc, &elem_path, access, loc, origin)?;
        }
        Ok(())
    }

    /// Mint the node a `.&`/`.&mut` creates.
    ///
    /// Creating a borrow is itself an access through the PARENT — a write
    /// for `.&mut`, a read for `.&` — which is what disables a sibling
    /// exclusive borrow and what freezes one on a shared reborrow. The new
    /// node then starts `Unique` (exclusive) or `Frozen` (shared): no
    /// `Reserved` phase, per the ruling.
    fn mint_borrow(
        &mut self,
        parent: Provenance,
        alloc: AllocId,
        path: &[PathElem],
        mutable: bool,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Provenance, EvalError> {
        let parent = match parent {
            Provenance(Some(tag)) => tag,
            Provenance(None) => self.borrow_root(alloc),
        };
        self.aliasing_access(
            Provenance(Some(parent)),
            alloc,
            path,
            if mutable { Access::Write } else { Access::Read },
            loc,
            origin,
        )?;
        let tag = BorrowTag(self.borrow_nodes.len() as u32);
        self.borrow_nodes.push(BorrowNode {
            alloc,
            parent: Some(parent),
            path: path.to_vec(),
            state: if mutable {
                NodeState::Unique
            } else {
                NodeState::Frozen
            },
            born: Some((loc.clone(), origin)),
            invalidated: None,
        });
        let list = self.borrow_nodes_of.entry(alloc).or_default();
        if mutable {
            list.unique.push(tag);
        } else {
            list.settled.push(tag);
        }
        Ok(Provenance(Some(tag)))
    }
}
