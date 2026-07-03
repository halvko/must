//! HIR → MIR lowering. Total: it never bails and never panics on broken
//! input — erroneous expressions become [`TerminatorKind::Trap`]s, and
//! lowering carries on so the whole function is always present in the CFG.
//! Inference-class traps borrow their message from the upstream diagnostic;
//! other classes are hand-written and kept consistent with hir's messages
//! by convention.

use base_db::Db;
use hir::body::{Body, ExprData, LiteralData, Stmt};
use hir::infer::{InferenceDiagnostic, InferenceResult};
use hir::{BindingId, ExprId, ItemId, Resolution, Ty};
use la_arena::{Arena, ArenaMap};
use rustc_hash::FxHashMap;

use crate::{
    BlockData, BlockId, BodyId, Const, LocalData, LocalId, MirBody, MirDiagnostic, MirLowered,
    Operand, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};

pub(crate) fn lower_item(db: &dyn Db, item: ItemId<'_>) -> MirLowered {
    let body = hir::body::body(db, item);
    let infer = hir::infer::infer(db, item);
    let const_diagnostics = hir::const_check::const_check(db, item);
    let mut ctx = LowerCtx {
        db,
        body,
        infer,
        const_diagnostics,
        resolutions: hir::resolutions(db, item),
        bodies: Arena::default(),
        const_blocks: Vec::new(),
        diagnostics: Vec::new(),
        value_traps: FxHashMap::default(),
        call_traps: FxHashMap::default(),
        const_call_traps: FxHashMap::default(),
        assign_traps: FxHashMap::default(),
        initializer_context: true,
    };
    ctx.seed_traps();
    let root = body.root.map(|root| {
        let ty = ctx.ty(root);
        ctx.lower_fn(&[], root, ty)
    });
    MirLowered {
        bodies: ctx.bodies,
        root,
        const_blocks: ctx.const_blocks,
        diagnostics: ctx.diagnostics,
    }
}

struct LowerCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    infer: &'db InferenceResult,
    const_diagnostics: &'db [hir::ConstCheckDiagnostic],
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    bodies: Arena<MirBody>,
    const_blocks: Vec<(ExprId, BodyId)>,
    diagnostics: Vec<MirDiagnostic>,
    /// Expressions whose *value* the context can't accept (type mismatches):
    /// lowered normally for the CFG, then trapped before the value flows on.
    value_traps: FxHashMap<ExprId, String>,
    /// Call expressions whose call *operation* is broken (wrong arity,
    /// callee not callable): callee and arguments lower, the call itself
    /// becomes a trap.
    call_traps: FxHashMap<ExprId, String>,
    /// Call expressions const-check rejected: illegal in a const context,
    /// fine as runtime code. How they trap depends on where they lower —
    /// see the `ExprData::Call` arm.
    const_call_traps: FxHashMap<ExprId, String>,
    /// Assignment *targets* inference rejected (immutable binding, item,
    /// builtin), keyed by the target expression: the write must not happen,
    /// so `lower_assign_target` traps instead of storing. The target is
    /// never lowered as a read, so unlike `value_traps` these only fire
    /// there.
    assign_traps: FxHashMap<ExprId, String>,
    /// True while lowering the item initializer's own const context: the
    /// root body outside any `fn` literal and outside any `const` block.
    /// That is the one const context with a runtime escape (the runner's
    /// synthetic entry), so its const violations get conditional
    /// [`TerminatorKind::ConstTrap`]s instead of unconditional traps.
    initializer_context: bool,
}

impl LowerCtx<'_> {
    fn seed_traps(&mut self) {
        for diag in &self.infer.diagnostics {
            match diag {
                InferenceDiagnostic::TypeMismatch { expr, .. }
                | InferenceDiagnostic::AllBranchesMismatch { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                InferenceDiagnostic::ArgCountMismatch { expr, .. } => {
                    self.call_traps.insert(*expr, diag.message());
                }
                // Reported on the callee; the unexecutable operation is the
                // call around it.
                InferenceDiagnostic::NotCallable { expr: callee, .. } => {
                    if let Some(call) = self.call_for_callee(*callee) {
                        self.call_traps.insert(call, diag.message());
                    }
                }
                InferenceDiagnostic::IfBranchMismatch { else_expr, .. } => {
                    self.value_traps.insert(*else_expr, diag.message());
                }
                // Handled where the name is lowered, which also covers
                // signatures broken by written-but-wrong annotations.
                InferenceDiagnostic::NeedsAnnotation { .. } => {}
                // Reported on the assignment's target; the operation that
                // must not execute is the write — `lower_assign_target`
                // looks these up by target instead of the read path's
                // `value_traps` (the target is never lowered as a read).
                InferenceDiagnostic::AssignToImmutable { target, .. }
                | InferenceDiagnostic::AssignToItem { target, .. }
                | InferenceDiagnostic::AssignToBuiltin { target, .. } => {
                    self.assign_traps.insert(*target, diag.message());
                }
            }
        }
        // Const-check diagnostics are reported on the *callee* (the
        // squiggle sits there), but the operation that must not execute is
        // the call around it — same reconciliation as `NotCallable` above.
        for diag in self.const_diagnostics {
            if let Some(call) = self.call_for_callee(diag.expr()) {
                self.const_call_traps.insert(call, diag.message());
            }
        }
    }

    /// The call expression whose callee is `callee`, if any — const-check
    /// and `NotCallable` diagnostics are reported on the callee, but the
    /// trap belongs on the call: that's the operation that actually fails
    /// to execute.
    fn call_for_callee(&self, callee: ExprId) -> Option<ExprId> {
        self.body.exprs.iter().find_map(|(id, data)| match data {
            ExprData::Call { callee: c, .. } if *c == callee => Some(id),
            _ => None,
        })
    }

    fn ty(&self, expr: ExprId) -> Ty {
        self.infer
            .type_of_expr
            .get(expr)
            .cloned()
            .unwrap_or(Ty::Error)
    }

    fn lower_fn(&mut self, params: &[BindingId], body_expr: ExprId, ret_ty: Ty) -> BodyId {
        let mut b = BodyBuilder::new(ret_ty, body_expr);
        for &param in params {
            let data = &self.body.bindings[param];
            let local = b.locals.alloc(LocalData {
                ty: self
                    .infer
                    .type_of_binding
                    .get(param)
                    .cloned()
                    .unwrap_or(Ty::Error),
                name: (!data.name.is_empty()).then(|| data.name.clone()),
                binding: Some(param),
            });
            b.local_for_binding.insert(param, local);
            b.params.push(local);
        }
        let op = self.lower_expr(&mut b, body_expr);
        let ret = b.ret;
        b.push_assign(ret, Rvalue::Use(op), body_expr);
        b.terminate(TerminatorKind::Return, body_expr);
        self.bodies.alloc(MirBody {
            locals: b.locals,
            blocks: b.blocks,
            params: b.params,
            entry: b.entry,
        })
    }

    fn lower_expr(&mut self, b: &mut BodyBuilder, expr: ExprId) -> Operand {
        let op = self.lower_expr_inner(b, expr);
        // A value the context can't accept: it was evaluated (the CFG keeps
        // everything), now refuse to let it flow onward.
        if let Some(message) = self.value_traps.get(&expr).cloned() {
            return self.trap(b, expr, message);
        }
        op
    }

    fn lower_expr_inner(&mut self, b: &mut BodyBuilder, expr: ExprId) -> Operand {
        let body = self.body;
        match &body.exprs[expr] {
            // Justified by the parse errors of the broken source.
            ExprData::Missing => self.trap(b, expr, "syntax error: missing expression".to_owned()),
            ExprData::Literal(LiteralData::Int(Some(value))) => Operand::Const(Const::Int(*value)),
            // Justified by the equally-worded literal diagnostic.
            ExprData::Literal(LiteralData::Int(None)) => {
                self.trap(b, expr, hir::diag::INT_LITERAL_TOO_LARGE.to_owned())
            }
            ExprData::Literal(LiteralData::Str(s)) => Operand::Const(Const::Str(s.clone())),
            ExprData::Literal(LiteralData::Bool(v)) => Operand::Const(Const::Bool(*v)),
            ExprData::NameRef(name) => self.lower_name_ref(b, expr, name),
            ExprData::Call { callee, args } => {
                let callee_op = self.lower_expr(b, *callee);
                let arg_ops: Vec<Operand> =
                    args.iter().map(|&arg| self.lower_expr(b, arg)).collect();
                // A call const-check rejected. Inside a `const fn` body or
                // a `const` block the code is a const context under every
                // execution, so the call is replaced by an unconditional
                // trap, like the broken calls below. At initializer level
                // the trap is conditional: forcing an item is always a
                // const context (the trap fires there), but the runner's
                // synthetic entry evaluates its initializer as run-mode
                // code at const depth 0 — the one execution where the call
                // may proceed.
                if let Some(message) = self.const_call_traps.get(&expr).cloned() {
                    if self.initializer_context {
                        let target = b.new_block();
                        b.terminate(TerminatorKind::ConstTrap { message, target }, expr);
                        b.current = target;
                    } else {
                        return self.trap(b, expr, message);
                    }
                }
                if let Some(message) = self.call_traps.get(&expr).cloned() {
                    return self.trap(b, expr, message);
                }
                // Divergence comes from the *callee's* signature, not the
                // call expression's type: inference recovers a mismatch with
                // the expected type ("trust the annotation"), so e.g.
                // `static f: ! = g();` types the call as `!` even though `g`
                // returns — emitting `target: None` from that would make a
                // returning call an internal error at runtime. The value
                // trap planted after the call handles the mismatch.
                let diverges = matches!(
                    self.ty(*callee),
                    Ty::Fn(f) if f.ret == Ty::Never
                );
                let dest = b.temp(self.ty(expr));
                let next = b.new_block();
                b.terminate(
                    TerminatorKind::Call {
                        callee: callee_op,
                        args: arg_ops,
                        dest,
                        target: (!diverges).then_some(next),
                    },
                    expr,
                );
                b.current = next;
                Operand::Copy(dest)
            }
            ExprData::Bin { op, lhs, rhs } => {
                let l = self.lower_expr(b, *lhs);
                let r = self.lower_expr(b, *rhs);
                // No operator token: broken source with its own parse error.
                let Some(op) = op else {
                    return self.trap(b, expr, "syntax error: missing operator".to_owned());
                };
                let dest = b.temp(self.ty(expr));
                b.push_assign(dest, Rvalue::BinaryOp(*op, l, r), expr);
                Operand::Copy(dest)
            }
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let discr = self.lower_expr(b, *condition);
                let dest = b.temp(self.ty(expr));
                let then_block = b.new_block();
                let else_block = b.new_block();
                b.terminate(
                    TerminatorKind::SwitchBool {
                        discr,
                        then_block,
                        else_block,
                    },
                    expr,
                );

                b.current = then_block;
                let then_op = self.lower_expr(b, *then_branch);
                b.push_assign(dest, Rvalue::Use(then_op), *then_branch);
                let join = b.new_block();
                b.terminate(TerminatorKind::Goto { target: join }, expr);

                b.current = else_block;
                match else_branch {
                    Some(els) => {
                        let else_op = self.lower_expr(b, *els);
                        b.push_assign(dest, Rvalue::Use(else_op), *els);
                    }
                    // No `else`: the false edge produces `()`.
                    None => b.push_assign(dest, Rvalue::Use(Operand::Const(Const::Unit)), expr),
                }
                b.terminate(TerminatorKind::Goto { target: join }, expr);
                b.current = join;
                Operand::Copy(dest)
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { binding, init } => {
                            let init_op = self.lower_expr(b, *init);
                            let data = &body.bindings[*binding];
                            let local = b.locals.alloc(LocalData {
                                ty: self
                                    .infer
                                    .type_of_binding
                                    .get(*binding)
                                    .cloned()
                                    .unwrap_or(Ty::Error),
                                name: (!data.name.is_empty()).then(|| data.name.clone()),
                                binding: Some(*binding),
                            });
                            b.local_for_binding.insert(*binding, local);
                            b.push_assign(local, Rvalue::Use(init_op), *init);
                        }
                        Stmt::Assign { target, value } => {
                            let value_op = self.lower_expr(b, *value);
                            self.lower_assign_target(b, *target, *value, value_op);
                        }
                        Stmt::Expr(e) => {
                            // Lowered for its effects; the value is dropped.
                            self.lower_expr(b, *e);
                        }
                    }
                }
                match tail {
                    Some(tail) => self.lower_expr(b, *tail),
                    None => Operand::Const(Const::Unit),
                }
            }
            // A compile-time unit of its own: the inner block lowers to a
            // separate zero-parameter body (like a `fn` literal's), and the
            // operand references it — the machine forces that body at
            // compile time and memoizes the value per run. Not initializer
            // context inside: a `const` block is a const context under
            // every execution, so const violations in it trap
            // unconditionally.
            ExprData::ConstBlock { body: inner } => {
                let saved = std::mem::replace(&mut self.initializer_context, false);
                let body_id = self.lower_fn(&[], *inner, self.ty(expr));
                self.initializer_context = saved;
                self.const_blocks.push((expr, body_id));
                Operand::Const(Const::ConstBlock(body_id))
            }
            ExprData::FnLiteral {
                params,
                body: fn_body,
                ..
            } => {
                let ret_ty = match self.ty(expr) {
                    Ty::Fn(f) => f.ret.clone(),
                    _ => Ty::Error,
                };
                // A fn body is never the initializer's own const context: a
                // plain body is runtime code (no const flags outside its
                // `const` blocks), a `const fn` body is a const context
                // under every execution.
                let saved = std::mem::replace(&mut self.initializer_context, false);
                let body_id = self.lower_fn(params, *fn_body, ret_ty);
                self.initializer_context = saved;
                Operand::Const(Const::Fn(body_id))
            }
        }
    }

    fn lower_name_ref(&mut self, b: &mut BodyBuilder, expr: ExprId, name: &str) -> Operand {
        match self.resolutions.get(expr) {
            Some(Resolution::Local(binding)) => match b.local_for_binding.get(binding) {
                Some(&local) => Operand::Copy(local),
                // A local of an enclosing function: a capture. The MIR
                // diagnostic *is* the upstream diagnostic the trap borrows.
                None => {
                    let diag = MirDiagnostic::UnsupportedCapture {
                        expr,
                        name: name.to_owned(),
                    };
                    let message = diag.message();
                    self.diagnostics.push(diag);
                    self.trap(b, expr, message)
                }
            },
            Some(Resolution::Item(loc)) => {
                let target = loc.to_id(self.db);
                let sig = hir::signature(self.db, target);
                if sig.contains_error() {
                    // Justified by the use-site needs-annotation
                    // diagnostic, or by the def-site diagnostics on a
                    // written-but-broken annotation.
                    let message = if hir::ty::signature_needs_annotation(self.db, target) {
                        InferenceDiagnostic::NeedsAnnotation {
                            expr,
                            item: loc.clone(),
                        }
                        .message()
                    } else {
                        format!("cannot use `{name}`: its type annotation has errors")
                    };
                    return self.trap(b, expr, message);
                }
                Operand::Const(Const::Item(loc.clone()))
            }
            // Justified by the duplicate-definition diagnostics.
            Some(Resolution::Ambiguous(_)) => {
                self.trap(b, expr, hir::diag::defined_multiple_times(name))
            }
            Some(Resolution::Builtin(builtin)) => Operand::Const(Const::Builtin(*builtin)),
            // Justified by the unresolved-name diagnostic.
            None => self.trap(b, expr, hir::diag::unresolved_name(name)),
        }
    }

    /// Lower an assignment's LHS: writes `value_op` into the target's local
    /// instead of reading it. The only real write is to a `mut` local that
    /// already has a slot in this body — the same local a `let` allocated,
    /// so the assignment reuses it rather than minting a new one. Every
    /// other case traps instead of silently dropping the RHS's effects,
    /// always with the message of a diagnostic already reported on the
    /// target: inference's assignment enforcement (immutable binding, item,
    /// builtin), the capture diagnostic (same as the read path), name
    /// resolution (unresolved/ambiguous), a parse error (missing target),
    /// or syntax validation (a non-variable target).
    fn lower_assign_target(
        &mut self,
        b: &mut BodyBuilder,
        target: ExprId,
        value: ExprId,
        value_op: Operand,
    ) {
        // A target inference rejected: trap with the squiggle's exact text.
        if let Some(message) = self.assign_traps.get(&target).cloned() {
            self.trap(b, target, message);
            return;
        }
        if let ExprData::NameRef(name) = &self.body.exprs[target] {
            match self.resolutions.get(target) {
                Some(Resolution::Local(binding)) => match b.local_for_binding.get(binding) {
                    Some(&local) => {
                        b.push_assign(local, Rvalue::Use(value_op), value);
                    }
                    // A local of an enclosing function: the same unsupported
                    // capture the read path (`lower_name_ref`) reports.
                    None => {
                        let diag = MirDiagnostic::UnsupportedCapture {
                            expr: target,
                            name: name.clone(),
                        };
                        let message = diag.message();
                        self.diagnostics.push(diag);
                        self.trap(b, target, message);
                    }
                },
                // Item and builtin targets were seeded into `assign_traps`
                // above (inference always reports them), so these arms are
                // unreachable in practice — kept total by re-rendering the
                // same diagnostics' messages rather than inventing text
                // (mirrors `lower_name_ref`'s `NeedsAnnotation` handling).
                Some(Resolution::Item(loc)) => {
                    let constness = hir::item_data(self.db, loc.to_id(self.db))
                        .as_ref()
                        .map(|it| it.constness)
                        .unwrap_or(hir::Constness::Static);
                    let message = InferenceDiagnostic::AssignToItem {
                        target,
                        item: loc.clone(),
                        constness,
                    }
                    .message();
                    self.trap(b, target, message);
                }
                Some(Resolution::Builtin(builtin)) => {
                    let message = InferenceDiagnostic::AssignToBuiltin {
                        target,
                        builtin: *builtin,
                    }
                    .message();
                    self.trap(b, target, message);
                }
                // Justified by the duplicate-definition diagnostics.
                Some(Resolution::Ambiguous(_)) => {
                    self.trap(b, target, hir::diag::defined_multiple_times(name));
                }
                // Justified by the unresolved-name diagnostic.
                None => {
                    self.trap(b, target, hir::diag::unresolved_name(name));
                }
            }
            return;
        }
        match &self.body.exprs[target] {
            // Justified by the parse errors of the broken source (same
            // wording as the read path's `ExprData::Missing` arm).
            ExprData::Missing => {
                self.trap(b, target, "syntax error: missing expression".to_owned());
            }
            // A non-variable target: validation already squiggled it with
            // exactly this text (a shared constant, so the two can't drift).
            _ => {
                self.trap(b, target, syntax::CAN_ONLY_ASSIGN_TO_A_VARIABLE.to_owned());
            }
        }
    }

    /// Emit a trap producing `expr`'s value and continue in a fresh block.
    fn trap(&mut self, b: &mut BodyBuilder, expr: ExprId, message: String) -> Operand {
        let dest = b.temp(self.ty(expr));
        let target = b.new_block();
        b.terminate(
            TerminatorKind::Trap {
                message,
                dest,
                target,
            },
            expr,
        );
        b.current = target;
        Operand::Copy(dest)
    }
}

struct BodyBuilder {
    locals: Arena<LocalData>,
    blocks: Arena<BlockData>,
    params: Vec<LocalId>,
    local_for_binding: FxHashMap<BindingId, LocalId>,
    ret: LocalId,
    entry: BlockId,
    current: BlockId,
    /// Origin for the placeholder terminators of fresh blocks.
    fallback_origin: ExprId,
}

impl BodyBuilder {
    fn new(ret_ty: Ty, origin: ExprId) -> BodyBuilder {
        let mut locals = Arena::default();
        let ret = locals.alloc(LocalData {
            ty: ret_ty,
            name: None,
            binding: None,
        });
        let mut blocks = Arena::default();
        let entry = blocks.alloc(BlockData {
            statements: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Unreachable,
                origin,
            },
        });
        BodyBuilder {
            locals,
            blocks,
            params: Vec::new(),
            local_for_binding: FxHashMap::default(),
            ret,
            entry,
            current: entry,
            fallback_origin: origin,
        }
    }

    fn new_block(&mut self) -> BlockId {
        self.blocks.alloc(BlockData {
            statements: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Unreachable,
                origin: self.fallback_origin,
            },
        })
    }

    fn temp(&mut self, ty: Ty) -> LocalId {
        self.locals.alloc(LocalData {
            ty,
            name: None,
            binding: None,
        })
    }

    fn push_assign(&mut self, dest: LocalId, rvalue: Rvalue, origin: ExprId) {
        self.blocks[self.current].statements.push(Statement {
            kind: StatementKind::Assign { dest, rvalue },
            origin,
        });
    }

    fn terminate(&mut self, kind: TerminatorKind, origin: ExprId) {
        self.blocks[self.current].terminator = Terminator { kind, origin };
    }
}
