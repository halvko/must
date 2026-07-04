//! The evaluation core. Executes MIR bodies on an explicit, heap-allocated
//! frame stack — one [`Machine::step`] call advances a single statement or
//! terminator, which is what lets a debugger pause between any two of them
//! and walk the stack. Parameterized by a [`Mode`] that decides what impure
//! builtins do; const-ness is tracked by the machine itself (`const_depth`):
//! any item being forced is a const context regardless of the driving mode.

use base_db::Db;
use hir::{Builtin, ExprId, ItemLoc};
use la_arena::ArenaMap;
use mir::{
    AggregateKind, BodyId, Const, LocalData, LocalId, MirBody, MirLowered, Operand, Rvalue,
    StatementKind, TerminatorKind,
};
use rustc_hash::FxHashMap;

use crate::{EvalError, EvalErrorKind, FnValue, GenericArgValue, Instance, Value};

/// What the machine does at its impure edges. [`ConstMode`] refuses;
/// the runner's mode performs the I/O.
pub trait Mode {
    /// `print(text)` outside any const context.
    fn print(&mut self, text: &str) -> Result<(), EvalError>;
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
        })
    }
}

/// Run mode for the CLI and the debug adapter: `print` writes a line.
pub struct RunMode<W: std::io::Write> {
    pub out: W,
}

impl<W: std::io::Write> Mode for RunMode<W> {
    fn print(&mut self, text: &str) -> Result<(), EvalError> {
        writeln!(self.out, "{text}").map_err(|err| EvalError {
            kind: EvalErrorKind::Runtime,
            message: format!("I/O error in `print`: {err}"),
            origin: None,
        })
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
            });
        }
        if self.const_depth >= MAX_CONST_DEPTH {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!("constant evaluation exceeded {MAX_CONST_DEPTH} nested items"),
                origin: root_origin(self.db, &loc),
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
        let result = self.eval_root(&loc);
        self.frames = saved_frames;
        self.const_fuel = saved_fuel;
        self.const_depth -= 1;
        self.forcing.pop();
        self.forced.insert(loc, result.clone());
        result
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
            .and_then(|()| self.run_to_done());
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
            let value = frame.locals.get(id)?.clone();
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
            let StatementKind::Assign { dest, rvalue } = &stmt.kind;
            let value = self.eval_rvalue(&loc, body, rvalue, stmt.origin)?;
            self.write_place(&loc, body, dest, value, stmt.origin)?;
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
                    Value::Builtin(builtin) => {
                        let result = self.builtin_call(builtin, args, &loc, origin)?;
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

    /// Store `value` into `dest` on the topmost frame: the whole local for
    /// an empty projection, or the nested `Value::Record` field the index
    /// path names — mutated in place; values are plain Rust data in
    /// `frame.locals`. Only records are writable through (field assignment
    /// is compile-checked to record chains; variant payloads aren't
    /// reachable as places), so anything else here is an invariant
    /// violation, loud like every other ill-typed value.
    fn write_place(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        dest: &mir::Place,
        value: Value,
        origin: ExprId,
    ) -> Result<(), EvalError> {
        let frame = self.frames.last_mut().expect("frame still live");
        if dest.projection.is_empty() {
            frame.locals.insert(dest.local, value);
            return Ok(());
        }
        let result = match frame.locals.get_mut(dest.local) {
            None => Err(format!(
                "write through uninitialized {}",
                local_name(body, dest.local)
            )),
            Some(slot) => project_mut(slot, &dest.projection).map(|field| *field = value),
        };
        result.map_err(|detail| self.internal_error(detail, Some((loc.clone(), origin))))
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
        }
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
                    return Err(self.ill_typed("`usize` operands", bad, loc, origin));
                };
                let (l, r) = (*l, *r);
                let runtime = |message: String| EvalError {
                    kind: EvalErrorKind::Runtime,
                    message,
                    origin: Some((loc.clone(), origin)),
                };
                Ok(match op {
                    Add => Value::Int(
                        l.checked_add(r)
                            .ok_or_else(|| runtime("attempt to add with overflow".to_owned()))?,
                    ),
                    Sub => {
                        Value::Int(l.checked_sub(r).ok_or_else(|| {
                            runtime("attempt to subtract with overflow".to_owned())
                        })?)
                    }
                    Mul => {
                        Value::Int(l.checked_mul(r).ok_or_else(|| {
                            runtime("attempt to multiply with overflow".to_owned())
                        })?)
                    }
                    Div => Value::Int(
                        l.checked_div(r)
                            .ok_or_else(|| runtime("attempt to divide by zero".to_owned()))?,
                    ),
                    Lt => Value::Bool(l < r),
                    Le => Value::Bool(l <= r),
                    Gt => Value::Bool(l > r),
                    Ge => Value::Bool(l >= r),
                    Eq | Ne => unreachable!(),
                })
            }
            // Equality is structural, on operands inference agreed about.
            Eq => Ok(Value::Bool(l == r)),
            Ne => Ok(Value::Bool(l != r)),
        }
    }

    fn eval_operand(
        &mut self,
        loc: &ItemLoc,
        body: &MirBody,
        op: &Operand,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match op {
            Operand::Copy(local) => self
                .frames
                .last()
                .and_then(|frame| frame.locals.get(*local))
                .cloned()
                .ok_or_else(|| {
                    self.internal_error(
                        format!("read of uninitialized {}", local_name(body, *local)),
                        Some((loc.clone(), origin)),
                    )
                }),
            Operand::Const(c) => Ok(match c {
                Const::Unit => Value::Unit,
                Const::Int(v) => Value::Int(*v),
                Const::Str(s) => Value::Str(s.clone()),
                Const::Bool(b) => Value::Bool(*b),
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

    fn builtin_call(
        &mut self,
        builtin: Builtin,
        args: Vec<Value>,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        let [arg] = args.as_slice() else {
            return Err(self.internal_error(
                format!("builtin `{}` takes 1 argument", builtin.name()),
                Some((loc.clone(), origin)),
            ));
        };
        let Value::Str(text) = arg else {
            return Err(self.ill_typed("a `str` argument", arg, loc, origin));
        };
        match builtin {
            Builtin::Panic => Err(EvalError {
                kind: EvalErrorKind::Panic,
                message: text.clone(),
                origin: Some((loc.clone(), origin)),
            }),
            // Defense in depth: const-check plants a trap at every `print`
            // call it can see in a const context, so this refusal is
            // normally shadowed — it stays as the machine's own guarantee
            // that const evaluation never performs I/O.
            Builtin::Print if self.const_depth > 0 => Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: hir::diag::side_effect_call_in_const(builtin.name()),
                origin: Some((loc.clone(), origin)),
            }),
            Builtin::Print => {
                self.mode.print(text)?;
                Ok(Value::Unit)
            }
        }
    }

    fn spend_fuel(&mut self, loc: &ItemLoc) -> Result<(), EvalError> {
        if self.const_depth == 0 {
            return Ok(());
        }
        self.const_fuel = self.const_fuel.saturating_sub(1);
        if self.const_fuel == 0 {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: "constant evaluation ran out of fuel".to_owned(),
                origin: root_origin(self.db, loc),
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
        }
    }

    fn ill_typed(&self, expected: &str, found: &Value, loc: &ItemLoc, origin: ExprId) -> EvalError {
        self.internal_error(
            format!("expected {expected}, found `{}`", found.display()),
            Some((loc.clone(), origin)),
        )
    }
}

/// Navigate a field-index path to the nested record field it names,
/// mutably. Errors are the *detail* strings of internal errors (the caller
/// attaches the origin): reaching a non-record or an out-of-range index
/// means a value the checker should have refused was written through.
fn project_mut<'v>(slot: &'v mut Value, projection: &[u32]) -> Result<&'v mut Value, String> {
    let mut current = slot;
    for &index in projection {
        let index = index as usize;
        match current {
            Value::Record { fields } => {
                let len = fields.len();
                current = match fields.get_mut(index) {
                    Some((_, field)) => field,
                    None => {
                        return Err(format!(
                            "record field index {index} out of range ({len} elements)"
                        ));
                    }
                };
            }
            other => {
                return Err(format!(
                    "expected a record value to assign into, found `{}`",
                    other.display()
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
        Value::Int(_) => hir::Ty::Int,
        Value::Str(_) => hir::Ty::Str,
        Value::Bool(_) => hir::Ty::Bool,
        Value::Record { fields } => hir::Ty::record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), value_ty(value)))
                .collect(),
        ),
        // Generic args are erased at runtime, so the recovered type is the
        // bare declaration (`Option`, args unknown) — enough for the
        // variables panel; console-eval parameters demote like the other
        // unrecoverable carriers when the static type was an instance.
        Value::Variant { decl, .. } => hir::Ty::Named(hir::NamedTy::plain(decl.clone())),
        Value::Fn(_) | Value::Builtin(_) | Value::Tuple(_) => hir::Ty::Error,
    }
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
