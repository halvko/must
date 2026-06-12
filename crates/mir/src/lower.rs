//! HIR → MIR lowering. Total: it never bails and never panics on broken
//! input — erroneous expressions become [`TerminatorKind::Trap`]s that
//! borrow their message from the upstream diagnostic, and lowering carries
//! on so the whole function is always present in the CFG.

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
    let mut ctx = LowerCtx {
        db,
        body,
        infer,
        resolutions: hir::resolutions(db, item),
        bodies: Arena::default(),
        diagnostics: Vec::new(),
        value_traps: FxHashMap::default(),
        call_traps: FxHashMap::default(),
    };
    ctx.seed_traps();
    let root = body.root.map(|root| {
        let ty = ctx.ty(root);
        ctx.lower_fn(&[], root, ty)
    });
    MirLowered {
        bodies: ctx.bodies,
        root,
        diagnostics: ctx.diagnostics,
    }
}

struct LowerCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    infer: &'db InferenceResult,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    bodies: Arena<MirBody>,
    diagnostics: Vec<MirDiagnostic>,
    /// Expressions whose *value* the context can't accept (type mismatches):
    /// lowered normally for the CFG, then trapped before the value flows on.
    value_traps: FxHashMap<ExprId, String>,
    /// Call expressions whose call *operation* is broken (wrong arity,
    /// callee not callable): callee and arguments lower, the call itself
    /// becomes a trap.
    call_traps: FxHashMap<ExprId, String>,
}

impl LowerCtx<'_> {
    fn seed_traps(&mut self) {
        for diag in &self.infer.diagnostics {
            match diag {
                InferenceDiagnostic::TypeMismatch { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message(self.db));
                }
                InferenceDiagnostic::ArgCountMismatch { expr, .. } => {
                    self.call_traps.insert(*expr, diag.message(self.db));
                }
                // Reported on the callee; the unexecutable operation is the
                // call around it.
                InferenceDiagnostic::NotCallable { expr: callee, .. } => {
                    let call = self.body.exprs.iter().find_map(|(id, data)| match data {
                        ExprData::Call { callee: c, .. } if c == callee => Some(id),
                        _ => None,
                    });
                    if let Some(call) = call {
                        self.call_traps.insert(call, diag.message(self.db));
                    }
                }
                // Handled where the name is lowered, which also covers
                // signatures broken by written-but-wrong annotations.
                InferenceDiagnostic::NeedsAnnotation { .. } => {}
            }
        }
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
            // Justified by the "integer literal is too large" diagnostic.
            ExprData::Literal(LiteralData::Int(None)) => {
                self.trap(b, expr, "integer literal is too large".to_owned())
            }
            ExprData::Literal(LiteralData::Str(s)) => Operand::Const(Const::Str(s.clone())),
            ExprData::Literal(LiteralData::Bool(v)) => Operand::Const(Const::Bool(*v)),
            ExprData::NameRef(name) => self.lower_name_ref(b, expr, name),
            ExprData::Call { callee, args } => {
                let callee_op = self.lower_expr(b, *callee);
                let arg_ops: Vec<Operand> =
                    args.iter().map(|&arg| self.lower_expr(b, arg)).collect();
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
            ExprData::FnLiteral {
                params,
                body: fn_body,
                ..
            } => {
                let ret_ty = match self.ty(expr) {
                    Ty::Fn(f) => f.ret.clone(),
                    _ => Ty::Error,
                };
                let body_id = self.lower_fn(params, *fn_body, ret_ty);
                Operand::Const(Const::Fn(body_id))
            }
        }
    }

    fn lower_name_ref(&mut self, b: &mut BodyBuilder, expr: ExprId, name: &str) -> Operand {
        match self.resolutions.get(expr) {
            Some(&Resolution::Local(binding)) => match b.local_for_binding.get(&binding) {
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
            Some(&Resolution::Item(loc)) => {
                if let Some(target) = loc.to_id(self.db) {
                    let sig = hir::signature(self.db, target);
                    if sig.contains_error() {
                        // Justified by the use-site needs-annotation
                        // diagnostic, or by the def-site diagnostics on a
                        // written-but-broken annotation.
                        let message = if hir::ty::signature_needs_annotation(self.db, target) {
                            InferenceDiagnostic::NeedsAnnotation { expr, item: loc }
                                .message(self.db)
                        } else {
                            format!("cannot use `{name}`: its type annotation has errors")
                        };
                        return self.trap(b, expr, message);
                    }
                }
                Operand::Const(Const::Item(loc))
            }
            // Justified by the duplicate-definition diagnostics.
            Some(&Resolution::Ambiguous(_)) => {
                self.trap(b, expr, format!("`{name}` is defined multiple times"))
            }
            Some(&Resolution::Builtin(builtin)) => Operand::Const(Const::Builtin(builtin)),
            // Justified by the unresolved-name diagnostic.
            None => self.trap(b, expr, format!("unresolved name `{name}`")),
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
