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
    BodyId, Const, LocalData, LocalId, MirBody, MirLowered, Operand, Rvalue, StatementKind,
    TerminatorKind,
};
use rustc_hash::FxHashMap;

use crate::{EvalError, EvalErrorKind, FnValue, Value};

/// What the machine does at its impure edges. [`ConstMode`] refuses;
/// the runner's mode performs the I/O.
pub trait Mode {
    /// `print(text)` outside any const context.
    fn print(&mut self, text: &str) -> Result<(), EvalError>;
}

/// The driver behind [`crate::const_value`]: everything is a const context.
pub struct ConstMode;

impl Mode for ConstMode {
    fn print(&mut self, _text: &str) -> Result<(), EvalError> {
        unreachable!("const machines have const_depth > 0 for their whole run")
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
/// against Rust stack overflow (stepping is iterative; only nested const
/// forcing recurses, bounded by the cycle check).
const MAX_FRAMES: usize = 10_000;

/// Statement/terminator budget for const contexts: a salsa query must
/// terminate even on adversarial input. Run-mode code outside initializers
/// is not fueled — a long-running program is the user's business.
const CONST_FUEL: u64 = 1_000_000;

/// One Must call frame.
pub struct Frame {
    pub loc: ItemLoc,
    pub body: BodyId,
    block: mir::BlockId,
    /// Index of the next statement to execute in `block`; past the end
    /// means the terminator is next.
    statement: usize,
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
    /// > 0 while inside a static initializer: the const context marker.
    const_depth: usize,
    const_fuel: u64,
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
            const_depth: 0,
            const_fuel: CONST_FUEL,
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
        self.push_frame(loc.clone(), root, Vec::new(), None)
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

    /// The user-named locals of a frame that currently hold values, with
    /// their declared MIR types, in declaration order (shadowing repeats a
    /// name; later wins).
    pub fn frame_named_locals(&self, index: usize) -> Vec<(String, hir::Ty, Value)> {
        let Some(frame) = self.frames.get(index) else {
            return Vec::new();
        };
        let body = &self.lowered(&frame.loc).bodies[frame.body];
        body.locals
            .iter()
            .filter_map(|(id, data)| {
                let name = data.name.clone()?;
                let value = frame.locals.get(id)?.clone();
                Some((name, data.ty.clone(), value))
            })
            .collect()
    }

    /// Call a function value with already-evaluated arguments, to
    /// completion, on its own stack — the paused frames are untouched. The
    /// debug console's evaluate uses this to run expressions against a
    /// frame's locals.
    pub fn call_value(&mut self, f: FnValue, args: Vec<Value>) -> Result<Value, EvalError> {
        let saved = std::mem::take(&mut self.frames);
        let result = self
            .push_frame(f.item, f.body, args, None)
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
        self.frames.push(Frame {
            loc,
            body: body_id,
            block: body.entry,
            statement: 0,
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
            let frame = self.frames.last_mut().expect("frame still live");
            frame.locals.insert(*dest, value);
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
                        self.push_frame(f.item, f.body, args, Some((*dest, *target)))?;
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
            TerminatorKind::Unreachable => {
                return Err(self.internal_error(
                    "entered an unreachable block".to_owned(),
                    Some((loc, origin)),
                ));
            }
        }
        Ok(StepEvent::Progress)
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
                    Add => Value::Int(l.checked_add(r).ok_or_else(|| {
                        runtime("attempt to add with overflow".to_owned())
                    })?),
                    Sub => Value::Int(l.checked_sub(r).ok_or_else(|| {
                        runtime("attempt to subtract with overflow".to_owned())
                    })?),
                    Mul => Value::Int(l.checked_mul(r).ok_or_else(|| {
                        runtime("attempt to multiply with overflow".to_owned())
                    })?),
                    Div => Value::Int(l.checked_div(r).ok_or_else(|| {
                        runtime("attempt to divide by zero".to_owned())
                    })?),
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
                Const::Fn(body) => Value::Fn(FnValue {
                    item: loc.clone(),
                    body: *body,
                }),
            }),
        }
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
            Builtin::Print if self.const_depth > 0 => Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: "cannot call `print` at compile time".to_owned(),
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

    fn ill_typed(
        &self,
        expected: &str,
        found: &Value,
        loc: &ItemLoc,
        origin: ExprId,
    ) -> EvalError {
        self.internal_error(
            format!("expected {expected}, found `{}`", found.display()),
            Some((loc.clone(), origin)),
        )
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
