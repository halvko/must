//! The evaluation core. Executes MIR bodies; parameterized by a [`Mode`]
//! that decides what impure builtins do. Const-ness is tracked by the
//! machine itself (`const_depth`): any item being forced is a const context
//! regardless of the driving mode.

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

/// Run mode for the CLI (and later the debugger): `print` writes a line.
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

/// Keep well under Rust's own stack — each Must frame costs several
/// recursive Rust frames, and test threads get only 2 MiB (512 overflowed
/// there). Enough for toy programs; the debugger milestone moves frames to
/// the heap (DAP's `stackTrace` wants them explicit) and can raise this.
const MAX_FRAMES: usize = 128;

/// Statement/block-transition budget for const contexts: a salsa query must
/// terminate even on adversarial input. Run-mode code outside initializers
/// is not fueled — a long-running program is the user's business.
const CONST_FUEL: u64 = 1_000_000;

pub struct Machine<'db, M> {
    db: &'db dyn Db,
    pub mode: M,
    /// Items currently being forced (cycle detection), innermost last.
    forcing: Vec<ItemLoc>,
    forced: FxHashMap<ItemLoc, Value>,
    /// > 0 while inside a static initializer: the const context marker.
    const_depth: usize,
    const_fuel: u64,
    frames: usize,
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
            forcing: Vec::new(),
            forced: FxHashMap::default(),
            const_depth: 0,
            const_fuel: CONST_FUEL,
            frames: 0,
        }
    }

    /// The (memoized) const value of a top-level item. Always a const
    /// context, whichever mode drives the machine.
    pub fn force_item(&mut self, loc: ItemLoc) -> Result<Value, EvalError> {
        if let Some(value) = self.forced.get(&loc) {
            return Ok(value.clone());
        }
        if self.forcing.contains(&loc) {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!(
                    "cycle detected while evaluating `{}`",
                    item_name(self.db, loc)
                ),
                origin: root_origin(self.db, loc),
            });
        }
        self.forcing.push(loc);
        self.const_depth += 1;
        let result = self.eval_root(loc);
        self.const_depth -= 1;
        self.forcing.pop();
        if let Ok(value) = &result {
            self.forced.insert(loc, value.clone());
        }
        result
    }

    /// Execute an item's root body — for [`Self::force_item`], and for the
    /// runner's entry item (at `const_depth` 0, where `print` is legal).
    pub fn eval_root(&mut self, loc: ItemLoc) -> Result<Value, EvalError> {
        let lowered = self.lowered(loc)?;
        let Some(root) = lowered.root else {
            // Broken source; the parse errors carry the diagnostic.
            return Err(EvalError {
                kind: EvalErrorKind::Trap,
                message: format!("`{}` has no value", item_name(self.db, loc)),
                origin: None,
            });
        };
        self.eval_body(loc, root, Vec::new())
    }

    fn lowered(&self, loc: ItemLoc) -> Result<&'db MirLowered, EvalError> {
        let item = loc.to_id(self.db).ok_or_else(|| self.internal_error(
            format!("dangling item reference #{}", loc.index),
            None,
        ))?;
        Ok(mir::mir_lowered(self.db, item))
    }

    fn eval_body(
        &mut self,
        loc: ItemLoc,
        body_id: BodyId,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        self.frames += 1;
        let result = self.eval_body_inner(loc, body_id, args);
        self.frames -= 1;
        result
    }

    fn eval_body_inner(
        &mut self,
        loc: ItemLoc,
        body_id: BodyId,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        if self.frames > MAX_FRAMES {
            return Err(EvalError {
                kind: EvalErrorKind::NotConst,
                message: format!("recursion exceeded {MAX_FRAMES} frames"),
                origin: root_origin(self.db, loc),
            });
        }
        let body = &self.lowered(loc)?.bodies[body_id];
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

        let mut block = body.entry;
        loop {
            self.spend_fuel(loc)?;
            let data = &body.blocks[block];
            for stmt in &data.statements {
                self.spend_fuel(loc)?;
                let StatementKind::Assign { dest, rvalue } = &stmt.kind;
                let value = self.eval_rvalue(loc, body, &locals, rvalue, stmt.origin)?;
                locals.insert(*dest, value);
            }
            let origin = data.terminator.origin;
            match &data.terminator.kind {
                TerminatorKind::Goto { target } => block = *target,
                TerminatorKind::SwitchBool {
                    discr,
                    then_block,
                    else_block,
                } => {
                    let discr = self.eval_operand(loc, body, &locals, discr, origin)?;
                    block = match discr {
                        Value::Bool(true) => *then_block,
                        Value::Bool(false) => *else_block,
                        other => {
                            return Err(self.ill_typed("a `bool` condition", &other, loc, origin));
                        }
                    };
                }
                TerminatorKind::Call {
                    callee,
                    args,
                    dest,
                    target,
                } => {
                    let callee = self.eval_operand(loc, body, &locals, callee, origin)?;
                    let args = args
                        .iter()
                        .map(|arg| self.eval_operand(loc, body, &locals, arg, origin))
                        .collect::<Result<Vec<_>, _>>()?;
                    let result = self.call(callee, args, loc, origin)?;
                    match target {
                        Some(target) => {
                            locals.insert(*dest, result);
                            block = *target;
                        }
                        None => {
                            return Err(self.internal_error(
                                "a diverging call returned".to_owned(),
                                Some((loc, origin)),
                            ));
                        }
                    }
                }
                TerminatorKind::Return => {
                    return Ok(locals
                        .get(body.return_local())
                        .cloned()
                        .unwrap_or(Value::Unit));
                }
                // Deferred error: the editor already shows this exact
                // message as a diagnostic; execution reached it.
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
        }
    }

    fn eval_rvalue(
        &mut self,
        loc: ItemLoc,
        body: &MirBody,
        locals: &ArenaMap<LocalId, Value>,
        rvalue: &Rvalue,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match rvalue {
            Rvalue::Use(op) => self.eval_operand(loc, body, locals, op, origin),
            Rvalue::BinaryOp(op, l, r) => {
                let l = self.eval_operand(loc, body, locals, l, origin)?;
                let r = self.eval_operand(loc, body, locals, r, origin)?;
                self.eval_bin_op(*op, l, r, loc, origin)
            }
        }
    }

    fn eval_bin_op(
        &mut self,
        op: hir::body::BinOp,
        l: Value,
        r: Value,
        loc: ItemLoc,
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
                    origin: Some((loc, origin)),
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
        loc: ItemLoc,
        body: &MirBody,
        locals: &ArenaMap<LocalId, Value>,
        op: &Operand,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match op {
            Operand::Copy(local) => locals.get(*local).cloned().ok_or_else(|| {
                self.internal_error(
                    format!("read of uninitialized {}", local_name(body, *local)),
                    Some((loc, origin)),
                )
            }),
            Operand::Const(c) => Ok(match c {
                Const::Unit => Value::Unit,
                Const::Int(v) => Value::Int(*v),
                Const::Str(s) => Value::Str(s.clone()),
                Const::Bool(b) => Value::Bool(*b),
                Const::Builtin(b) => Value::Builtin(*b),
                Const::Item(item) => self.force_item(*item)?,
                Const::Fn(body) => Value::Fn(FnValue {
                    item: loc,
                    body: *body,
                }),
            }),
        }
    }

    fn call(
        &mut self,
        callee: Value,
        args: Vec<Value>,
        loc: ItemLoc,
        origin: ExprId,
    ) -> Result<Value, EvalError> {
        match callee {
            Value::Fn(f) => self.eval_body(f.item, f.body, args),
            Value::Builtin(builtin) => {
                let [arg] = args.as_slice() else {
                    return Err(self.internal_error(
                        format!("builtin `{}` takes 1 argument", builtin.name()),
                        Some((loc, origin)),
                    ));
                };
                let Value::Str(text) = arg else {
                    return Err(self.ill_typed("a `str` argument", arg, loc, origin));
                };
                match builtin {
                    Builtin::Panic => Err(EvalError {
                        kind: EvalErrorKind::Panic,
                        message: text.clone(),
                        origin: Some((loc, origin)),
                    }),
                    Builtin::Print if self.const_depth > 0 => Err(EvalError {
                        kind: EvalErrorKind::NotConst,
                        message: "cannot call `print` at compile time".to_owned(),
                        origin: Some((loc, origin)),
                    }),
                    Builtin::Print => {
                        self.mode.print(text)?;
                        Ok(Value::Unit)
                    }
                }
            }
            other => Err(self.ill_typed("a callable value", &other, loc, origin)),
        }
    }

    fn spend_fuel(&mut self, loc: ItemLoc) -> Result<(), EvalError> {
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
        loc: ItemLoc,
        origin: ExprId,
    ) -> EvalError {
        self.internal_error(
            format!("expected {expected}, found `{}`", found.display()),
            Some((loc, origin)),
        )
    }
}

fn item_name(db: &dyn Db, loc: ItemLoc) -> String {
    hir::item_tree::item_tree(db, loc.file)
        .items
        .get(loc.index as usize)
        .map(|it| it.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("#{}", loc.index))
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
fn root_origin(db: &dyn Db, loc: ItemLoc) -> Option<(ItemLoc, ExprId)> {
    let root = loc.to_id(db).and_then(|item| hir::body::body(db, item).root)?;
    Some((loc, root))
}
