//! HIR → MIR lowering. Total: it never bails and never panics on broken
//! input — erroneous expressions become [`TerminatorKind::Trap`]s, and
//! lowering carries on so the whole function is always present in the CFG.
//! Inference-class traps borrow their message from the upstream diagnostic;
//! other classes are hand-written and kept consistent with hir's messages
//! by convention.

use base_db::Db;
use hir::body::{Body, ExprData, LiteralData, MatchArm, PatData, Stmt};
use hir::infer::{InferenceDiagnostic, InferenceResult};
use hir::{BindingId, ExprId, ItemId, ItemLoc, PatId, Resolution, Ty};
use la_arena::{Arena, ArenaMap};
use rustc_hash::FxHashMap;

use crate::{
    AggregateKind, BlockData, BlockId, BodyId, Const, LocalData, LocalId, MirBody, MirDiagnostic,
    MirLowered, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
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
        nonexhaustive_traps: FxHashMap::default(),
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
    /// Non-exhaustive `match` expressions, keyed by the match: the value
    /// only fails to exist when the *uncovered* case actually shows up, so
    /// this is not a value trap — it becomes the switch's otherwise-arm
    /// trap (or the whole lowering, when no arm can run), firing with
    /// exactly the squiggle's message.
    nonexhaustive_traps: FxHashMap<ExprId, String>,
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
                // A literal that doesn't have the record type it must have,
                // or a field value with nowhere to go: the value must not
                // flow on. `RecordLitExtraField` squiggles the field name,
                // but the diagnostic's `expr` is the field's value — a value
                // trap there is the direct reconciliation.
                InferenceDiagnostic::RecordLitMissingFields { expr, .. }
                | InferenceDiagnostic::RecordLitExtraField { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                // Both are reported at (or inside) the field-access
                // expression, which is exactly the value that cannot be
                // produced — the reconciliation is direct.
                InferenceDiagnostic::NoSuchField { expr, .. }
                | InferenceDiagnostic::FieldOnUnknownType { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                // A bare type name read as a value: the name itself is the
                // value that cannot be produced.
                InferenceDiagnostic::TypeNotValue { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                // A construction call with the wrong arity: like
                // `ArgCountMismatch`, the call operation itself is broken.
                InferenceDiagnostic::TypeCtorArgCount { expr, .. } => {
                    self.call_traps.insert(*expr, diag.message());
                }
                // A `::` path that names no variant (or no enum at all):
                // the path itself is the value that cannot be produced.
                InferenceDiagnostic::NoSuchVariant { expr, .. }
                | InferenceDiagnostic::NoVariantsOnStruct { expr, .. }
                | InferenceDiagnostic::VariantPathOnValue { expr, .. } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                // `Shape(...)` on an enum: the call operation is broken
                // (an enum constructs through its variants).
                InferenceDiagnostic::EnumCtorIsVariant { expr, .. } => {
                    self.call_traps.insert(*expr, diag.message());
                }
                // Deferred like any other error, but only on the executions
                // that reach the uncovered case: the otherwise arm traps.
                InferenceDiagnostic::NonExhaustiveMatch { expr, .. }
                | InferenceDiagnostic::MatchWithoutCatchAll { expr, .. } => {
                    self.nonexhaustive_traps.insert(*expr, diag.message());
                }
                // Warnings: an unreachable arm is dead (lowers as an
                // unreachable block), and a bind shadowing a variant name is
                // well-typed — neither traps.
                InferenceDiagnostic::UnreachableArm { .. }
                | InferenceDiagnostic::BindShadowsVariant { .. } => {}
                // A broken pattern means the match operation as a whole
                // can't be judged: the arms still lower (the CFG keeps
                // everything), then the match's value is refused.
                InferenceDiagnostic::NonEnumScrutineeVariantPat { match_expr, .. }
                | InferenceDiagnostic::PatNoSuchVariant { match_expr, .. }
                | InferenceDiagnostic::PatWrongEnum { match_expr, .. }
                | InferenceDiagnostic::PatArity { match_expr, .. }
                | InferenceDiagnostic::PatPathError { match_expr, .. }
                | InferenceDiagnostic::VariantPatUnknownScrutinee { match_expr, .. } => {
                    self.value_traps.insert(*match_expr, diag.message());
                }
                // A `break`/`continue` with no loop to go to: there is no
                // edge to emit, so the expression itself is the operation
                // that cannot execute — a value trap right there.
                InferenceDiagnostic::BreakOutsideLoop { expr }
                | InferenceDiagnostic::ContinueOutsideLoop { expr } => {
                    self.value_traps.insert(*expr, diag.message());
                }
                // A `let`/parameter destructuring pattern that names an
                // unknown field, misses required fields, unwraps the wrong
                // named type, or can't be checked at all (no type known):
                // compile-time only, like `NeedsAnnotation` above — the
                // affected bindings are already `{error}`-typed (infectious
                // and silent), and a parameter pattern has no single
                // per-call expression to trap (every call runs the same
                // broken destructure, so there is no "the executions that
                // reach it" distinction the way a non-exhaustive `match`
                // has).
                InferenceDiagnostic::PatUnknownField { .. }
                | InferenceDiagnostic::PatMissingFields { .. }
                | InferenceDiagnostic::PatBindingNeedsAnnotation { .. }
                | InferenceDiagnostic::PatNotRecord { .. }
                | InferenceDiagnostic::PatUnknownType { .. }
                | InferenceDiagnostic::PatNamedTypeMismatch { .. } => {}
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

    /// The positional index of `name` in `receiver`'s record type — the
    /// name→index resolution both field reads and field-assign places use.
    /// A named receiver projects through its declared shape: erased at
    /// runtime, the value is the underlying record and the index comes from
    /// the declaration's sorted field order (the same canonical order
    /// `Ty::Record` uses). `None` when the receiver isn't a (known) record
    /// or the field doesn't exist — always already diagnosed upstream.
    fn field_index(&self, receiver: ExprId, name: &str) -> Option<u32> {
        let receiver_record = match self.ty(receiver) {
            Ty::Record(rec) => Some(Ty::Record(rec)),
            Ty::Named(loc) => hir::type_underlying(self.db, loc.to_id(self.db)),
            _ => None,
        };
        match receiver_record {
            Some(Ty::Record(rec)) => rec
                .fields
                .iter()
                .position(|(n, _)| n == name)
                .map(|index| index as u32),
            _ => None,
        }
    }

    fn lower_fn(&mut self, params: &[hir::body::Param], body_expr: ExprId, ret_ty: Ty) -> BodyId {
        let mut b = BodyBuilder::new(ret_ty, body_expr);
        for param in params {
            let local = self.alloc_pat_slot_local(&mut b, param.pat, body_expr);
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
        // A widening edge: inference accepted this variant-typed value
        // where its enum was needed — the conversion (tag injection)
        // happens here and only here.
        if let Some(variant) = self.infer.widened.get(expr).cloned() {
            let dest = b.temp(Ty::Named(variant.decl.clone()));
            b.push_assign(
                dest,
                Rvalue::WidenToEnum {
                    op,
                    decl: variant.decl,
                    index: variant.index,
                    variant: variant.name.to_string(),
                },
                expr,
            );
            return Operand::Copy(dest);
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
                // A construction call `Foo(arg)`. Erasure decision: nominal
                // types exist only in the static type system — a `Foo` *is*
                // its underlying record at runtime (`Value::Record`, no
                // tag), matching the language's types-don't-exist-at-runtime
                // stance. So the constructor lowers to nothing at all: the
                // argument's operand simply flows through. Equality between
                // two `Foo`s is therefore structural under the hood, and the
                // type system alone guarantees a `Foo` never meets a bare
                // record in a comparison. The callee is not lowered — a type
                // name has no value (reading one traps, see
                // `lower_name_ref`); as a construction head it is legal and
                // erased.
                if let Some(Resolution::TypeItem(_)) = self.resolutions.get(*callee) {
                    let mut arg_ops: Vec<Operand> =
                        args.iter().map(|&arg| self.lower_expr(b, arg)).collect();
                    // Wrong arity, or an enum type constructed directly
                    // (`EnumCtorIsVariant`): inference seeded a call trap;
                    // the arguments were still evaluated for their
                    // effects, like any broken call.
                    if let Some(message) = self.call_traps.get(&expr).cloned() {
                        return self.trap(b, expr, message);
                    }
                    return arg_ops.pop().unwrap_or(Operand::Const(Const::Unit));
                }
                // A directly-called variant constructor with payloads
                // (`Shape::Circle(3)`): no function value, no call — the
                // arguments assemble straight into the tag-free payload
                // aggregate. The callee path itself is not lowered (its
                // first-class lowering would synthesize a constructor
                // body nothing here needs).
                if let ExprData::VariantPath { .. } = &body.exprs[*callee]
                    && self.infer.variant_of_expr.get(*callee).is_some()
                    && matches!(self.ty(*callee), Ty::Fn(_))
                {
                    let arg_ops: Vec<Operand> =
                        args.iter().map(|&arg| self.lower_expr(b, arg)).collect();
                    // Wrong arity: plain `ArgCountMismatch`, seeded as a
                    // call trap like any broken call.
                    if let Some(message) = self.call_traps.get(&expr).cloned() {
                        return self.trap(b, expr, message);
                    }
                    let dest = b.temp(self.ty(expr));
                    b.push_assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::VariantPayload,
                            ops: arg_ops,
                        },
                        expr,
                    );
                    return Operand::Copy(dest);
                }
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
                        Stmt::Let { pat, init, .. } => {
                            let init_op = self.lower_expr(b, *init);
                            match &self.body.pats[*pat] {
                                // The common case, unchanged: one local,
                                // one assignment.
                                PatData::Bind(binding) => {
                                    let local = self.alloc_binding_local(b, *binding);
                                    b.push_assign(local, Rvalue::Use(init_op), *init);
                                }
                                PatData::Wildcard | PatData::Missing => {
                                    // Evaluated for effects only; nothing to
                                    // bind (a hole lowers as `Bind` above,
                                    // never reaches here in practice).
                                }
                                // A destructuring pattern: stash the
                                // initializer's value in a synthetic local,
                                // then destructure out of it — the field
                                // reads need an addressable operand to
                                // project from, exactly like a record
                                // literal's fields need one to assemble.
                                PatData::Record { .. } | PatData::Newtype { .. } => {
                                    let local = b.temp(self.ty(*init));
                                    b.push_assign(local, Rvalue::Use(init_op), *init);
                                    self.bind_binding_pattern(
                                        b,
                                        *pat,
                                        &Operand::Copy(local),
                                        *init,
                                    );
                                }
                                PatData::Variant { .. } => {
                                    // Never produced by `binding_pattern`'s
                                    // grammar; defensive.
                                }
                            }
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
            // A record literal lowers to an `Aggregate`. Field initializers
            // are lowered in *source* order (so calls, mutation, etc. happen
            // in the order the program writes them), collected by name, then
            // reassembled into the operand list in the type's canonical
            // (sorted) order — the same order `Ty::Record` uses, which is
            // what `Rvalue::Field`'s index refers to. A field the literal
            // doesn't actually supply (missing-field diagnostic, already a
            // pending value trap) or one the type doesn't want (extra-field
            // diagnostic, likewise) gets a harmless placeholder: the whole
            // expression is about to be trapped by the `lower_expr` wrapper,
            // so the aggregate itself is never observed.
            ExprData::RecordLit { fields } => {
                let mut by_name: Vec<(String, Operand)> = Vec::new();
                for (name, field_expr) in fields {
                    let op = self.lower_expr(b, *field_expr);
                    if !by_name.iter().any(|(n, _)| n == name) {
                        by_name.push((name.clone(), op));
                    }
                }
                let field_names: Vec<String> = match self.ty(expr) {
                    Ty::Record(rec) => rec.fields.iter().map(|(name, _)| name.clone()).collect(),
                    // The literal didn't conclude a record type (a plain
                    // type mismatch elsewhere already traps this value) —
                    // any deterministic order keeps lowering total.
                    _ => {
                        let mut names: Vec<String> =
                            by_name.iter().map(|(name, _)| name.clone()).collect();
                        names.sort();
                        names
                    }
                };
                let ops = field_names
                    .iter()
                    .map(|name| {
                        by_name
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, op)| op.clone())
                            .unwrap_or(Operand::Const(Const::Unit))
                    })
                    .collect();
                let dest = b.temp(self.ty(expr));
                b.push_assign(
                    dest,
                    Rvalue::Aggregate {
                        kind: AggregateKind::Record(field_names),
                        ops,
                    },
                    expr,
                );
                Operand::Copy(dest)
            }
            // `receiver.field` lowers to a positional projection: the
            // receiver's record type is already resolved by inference, so
            // the field name becomes an index into its sorted field list.
            ExprData::Field { receiver, name } => {
                let base = self.lower_expr(b, *receiver);
                // No field name at all (`a.`): the parse error covers it,
                // same invented-but-generic wording as a missing operand.
                if name.is_empty() {
                    return self.trap(b, expr, "syntax error: missing field name".to_owned());
                }
                match self.field_index(*receiver, name) {
                    Some(index) => {
                        let dest = b.temp(self.ty(expr));
                        b.push_assign(dest, Rvalue::Field { base, index }, expr);
                        Operand::Copy(dest)
                    }
                    // No such field, or not a (known) record at all:
                    // `NoSuchField`/`FieldOnUnknownType` already seeded a
                    // value trap, or the receiver diverges or was itself
                    // already trapped — either way this operand is never
                    // observed.
                    None => Operand::Const(Const::Unit),
                }
            }
            // `Shape::Circle` as a value (not a direct call — those are
            // intercepted in the `Call` arm). Payload-less: the variant
            // value itself, an empty tag-free payload. With payloads: a
            // first-class constructor function, synthesized as a tiny MIR
            // body (params → payload aggregate) so calling it through a
            // variable runs like any function value.
            ExprData::VariantPath { base, .. } => {
                match self.infer.variant_of_expr.get(expr).cloned() {
                    Some(variant) => {
                        let payload_tys = hir::enum_variants(self.db, variant.decl.to_id(self.db))
                            .as_ref()
                            .and_then(|variants| variants.get(variant.index as usize))
                            .map(|(_, payload)| payload.clone())
                            .unwrap_or_default();
                        if payload_tys.is_empty() {
                            let dest = b.temp(self.ty(expr));
                            b.push_assign(
                                dest,
                                Rvalue::Aggregate {
                                    kind: AggregateKind::VariantPayload,
                                    ops: Vec::new(),
                                },
                                expr,
                            );
                            Operand::Copy(dest)
                        } else {
                            let body_id = self.synth_ctor_body(expr, &variant, &payload_tys);
                            Operand::Const(Const::Fn(body_id))
                        }
                    }
                    // No resolved variant. The diagnosed cases
                    // (`NoSuchVariant`, `NoVariantsOnStruct`,
                    // `VariantPathOnValue`) are pending value traps — the
                    // `lower_expr` wrapper replaces this placeholder. The
                    // silent cases trap here with their upstream
                    // diagnostic's message, mirroring `lower_name_ref`.
                    None => {
                        if self.value_traps.contains_key(&expr) {
                            return Operand::Const(Const::Unit);
                        }
                        let name = match &body.exprs[*base] {
                            ExprData::NameRef(name) => name.clone(),
                            _ => String::new(),
                        };
                        match self.resolutions.get(*base) {
                            // Justified by the duplicate-definition
                            // diagnostics on the base name.
                            Some(Resolution::Ambiguous(_)) => {
                                self.trap(b, expr, hir::diag::defined_multiple_times(&name))
                            }
                            // A broken `type` declaration: its own
                            // diagnostics sit at the declaration site
                            // (same reconciliation as a broken annotation
                            // in `lower_name_ref`).
                            Some(_) => self.trap(
                                b,
                                expr,
                                format!(
                                    "cannot use a variant of `{name}`: \
                                     its declaration has errors"
                                ),
                            ),
                            // Justified by the unresolved-name diagnostic.
                            None => self.trap(b, expr, hir::diag::unresolved_name(&name)),
                        }
                    }
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
                // A fn body is never the initializer's own const context: a
                // plain body is runtime code (no const flags outside its
                // `const` blocks), a `const fn` body is a const context
                // under every execution.
                let saved = std::mem::replace(&mut self.initializer_context, false);
                let body_id = self.lower_fn(params, *fn_body, ret_ty);
                self.initializer_context = saved;
                Operand::Const(Const::Fn(body_id))
            }
            ExprData::Match { scrutinee, arms } => self.lower_match(b, expr, *scrutinee, arms),
            // The first cyclic CFGs: a header block the body re-enters (the
            // back edge), an exit block the `break` edges target. The
            // loop's value lives in a dedicated result local — every break
            // stores into it (unit for a bare `break;`) before jumping to
            // the exit. A loop with no breaks never targets the exit: like
            // the continuation after a diverging call, it stays a
            // predecessor-less block so the CFG remains total.
            ExprData::Loop { body: loop_body } => {
                let dest = b.temp(self.ty(expr));
                let header = b.new_block();
                let exit = b.new_block();
                b.terminate(TerminatorKind::Goto { target: header }, expr);
                b.current = header;
                b.loop_frames.push(LoopFrame {
                    header,
                    exit,
                    result: dest,
                });
                // The body's value is discarded: running off its end is the
                // back edge to the header.
                self.lower_expr(b, *loop_body);
                b.terminate(TerminatorKind::Goto { target: header }, expr);
                b.loop_frames.pop();
                b.current = exit;
                Operand::Copy(dest)
            }
            ExprData::Break { value } => {
                let op = match value {
                    Some(value) => self.lower_expr(b, *value),
                    // A bare `break;` carries `()` as the loop's value.
                    None => Operand::Const(Const::Unit),
                };
                match b.loop_frames.last().copied() {
                    Some(frame) => {
                        b.push_assign(frame.result, Rvalue::Use(op), expr);
                        b.terminate(TerminatorKind::Goto { target: frame.exit }, expr);
                        // Code after a break is unreachable; keep lowering
                        // it into a predecessor-less block (CFG stays
                        // total), like after a diverging call.
                        b.current = b.new_block();
                        Operand::Const(Const::Unit)
                    }
                    // Outside any loop: the outside-a-loop diagnostic
                    // seeded a value trap on this expression — the
                    // `lower_expr` wrapper plants it; this placeholder is
                    // never observed.
                    None => Operand::Const(Const::Unit),
                }
            }
            ExprData::Continue => match b.loop_frames.last().copied() {
                Some(frame) => {
                    b.terminate(
                        TerminatorKind::Goto {
                            target: frame.header,
                        },
                        expr,
                    );
                    b.current = b.new_block();
                    Operand::Const(Const::Unit)
                }
                // Same story as an outside-a-loop break: the pending value
                // trap is the whole story.
                None => Operand::Const(Const::Unit),
            },
        }
    }

    /// Lower a `match`. Two shapes, decided by the scrutinee's type:
    /// enum-typed (tagged) dispatches through [`TerminatorKind::SwitchVariant`];
    /// variant-typed destructures directly — it can only be its one
    /// variant, so there is NO switch and no tag read (the state-machine
    /// payoff). Anything else can only be matched by a catch-all arm,
    /// which lowers straight-line too. Arms an execution can never reach
    /// (after a catch-all, duplicate variants, other variants of a
    /// variant-typed scrutinee) still lower — as blocks no edge targets —
    /// so the CFG stays total.
    fn lower_match(
        &mut self,
        b: &mut BodyBuilder,
        expr: ExprId,
        scrutinee: ExprId,
        arms: &[MatchArm],
    ) -> Operand {
        let scrut_op = self.lower_expr(b, scrutinee);
        // Pin the scrutinee in a temp: the switch reads it and every
        // payload extraction re-reads it.
        let scrut_local = b.temp(self.ty(scrutinee));
        b.push_assign(scrut_local, Rvalue::Use(scrut_op), scrutinee);
        let scrut = Operand::Copy(scrut_local);

        match self.ty(scrutinee) {
            Ty::Named(decl) if hir::enum_variants(self.db, decl.to_id(self.db)).is_some() => {
                self.lower_match_switch(b, expr, &scrut, decl, arms)
            }
            Ty::Variant(variant) => {
                let covering =
                    arms.iter()
                        .position(|arm| match self.arm_kind(arm.pat, Some(&variant.decl)) {
                            ArmKind::CatchAll => true,
                            ArmKind::Variant(index) => index == variant.index,
                            ArmKind::Dead => false,
                        });
                let fallback = InferenceDiagnostic::NonExhaustiveMatch {
                    expr,
                    uncovered: vec![format!("{}::{}", variant.decl.display_name(), variant.name)],
                }
                .message();
                self.lower_match_straight(b, expr, &scrut, arms, covering, fallback)
            }
            scrut_ty => {
                let covering = arms
                    .iter()
                    .position(|arm| matches!(self.arm_kind(arm.pat, None), ArmKind::CatchAll));
                let fallback = InferenceDiagnostic::MatchWithoutCatchAll {
                    expr,
                    scrutinee: scrut_ty,
                }
                .message();
                self.lower_match_straight(b, expr, &scrut, arms, covering, fallback)
            }
        }
    }

    /// The tagged dispatch: one switch arm per first-covering variant
    /// pattern, a catch-all arm as `otherwise` — or, when there is none
    /// and coverage has holes, a trap carrying the non-exhaustiveness
    /// diagnostic's exact message.
    fn lower_match_switch(
        &mut self,
        b: &mut BodyBuilder,
        expr: ExprId,
        scrut: &Operand,
        decl: ItemLoc,
        arms: &[MatchArm],
    ) -> Operand {
        let dest = b.temp(self.ty(expr));
        let arm_blocks: Vec<BlockId> = arms.iter().map(|_| b.new_block()).collect();
        let mut switch_arms: Vec<(u32, BlockId)> = Vec::new();
        let mut otherwise = None;
        for (arm, &block) in arms.iter().zip(&arm_blocks) {
            if otherwise.is_some() {
                break; // everything after a catch-all is dead
            }
            match self.arm_kind(arm.pat, Some(&decl)) {
                ArmKind::Variant(index) => {
                    if !switch_arms.iter().any(|&(i, _)| i == index) {
                        switch_arms.push((index, block));
                    }
                }
                ArmKind::CatchAll => otherwise = Some(block),
                ArmKind::Dead => {}
            }
        }
        let (otherwise_block, needs_trap) = match otherwise {
            Some(block) => (block, false),
            None => (b.new_block(), true),
        };
        b.terminate(
            TerminatorKind::SwitchVariant {
                discr: scrut.clone(),
                decl: decl.clone(),
                arms: switch_arms.clone(),
                otherwise: otherwise_block,
            },
            expr,
        );
        let join = b.new_block();
        for (arm, &block) in arms.iter().zip(&arm_blocks) {
            b.current = block;
            self.bind_match_pattern(b, arm.pat, scrut, arm.body);
            let op = self.lower_expr(b, arm.body);
            b.push_assign(dest, Rvalue::Use(op), arm.body);
            b.terminate(TerminatorKind::Goto { target: join }, expr);
        }
        if needs_trap {
            b.current = otherwise_block;
            // Normally seeded from the diagnostic; the fallback re-renders
            // the same message from the same set-cover, so the text can't
            // drift even if the seeding ever does.
            let message = self.nonexhaustive_traps.get(&expr).cloned().or_else(|| {
                let uncovered: Vec<String> = hir::enum_variants(self.db, decl.to_id(self.db))
                    .as_ref()
                    .map(|variants| {
                        variants
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| !switch_arms.iter().any(|&(j, _)| j as usize == i))
                            .map(|(_, (name, _))| format!("{}::{name}", decl.display_name()))
                            .collect()
                    })
                    .unwrap_or_default();
                (!uncovered.is_empty())
                    .then(|| InferenceDiagnostic::NonExhaustiveMatch { expr, uncovered }.message())
            });
            match message {
                Some(message) => b.terminate(
                    TerminatorKind::Trap {
                        message,
                        dest,
                        target: join,
                    },
                    expr,
                ),
                // Every variant has an arm and only valid tags exist: the
                // otherwise edge can never be taken.
                None => b.terminate(TerminatorKind::Unreachable, expr),
            }
        }
        b.current = join;
        Operand::Copy(dest)
    }

    /// The no-dispatch lowering: run the first covering arm straight-line
    /// (variant-typed scrutinees destructure the tag-free value directly;
    /// other scrutinees just bind), or trap when nothing covers. The
    /// remaining arms lower as dead blocks.
    fn lower_match_straight(
        &mut self,
        b: &mut BodyBuilder,
        expr: ExprId,
        scrut: &Operand,
        arms: &[MatchArm],
        covering: Option<usize>,
        fallback: String,
    ) -> Operand {
        let dest = b.temp(self.ty(expr));
        let join = b.new_block();
        match covering {
            Some(covering) => {
                let arm = &arms[covering];
                self.bind_match_pattern(b, arm.pat, scrut, arm.body);
                let op = self.lower_expr(b, arm.body);
                b.push_assign(dest, Rvalue::Use(op), arm.body);
                b.terminate(TerminatorKind::Goto { target: join }, expr);
            }
            None if self.value_traps.contains_key(&expr) => {
                // A broken-pattern match: the pending value trap (planted
                // by the `lower_expr` wrapper) is the whole story.
                b.push_assign(dest, Rvalue::Use(Operand::Const(Const::Unit)), expr);
                b.terminate(TerminatorKind::Goto { target: join }, expr);
            }
            None => {
                // Same fallback contract as the switch's trap: normally
                // seeded, re-rendered identically if seeding ever drifts.
                let message = self
                    .nonexhaustive_traps
                    .get(&expr)
                    .cloned()
                    .unwrap_or(fallback);
                b.terminate(
                    TerminatorKind::Trap {
                        message,
                        dest,
                        target: join,
                    },
                    expr,
                );
            }
        }
        for (index, arm) in arms.iter().enumerate() {
            if Some(index) == covering {
                continue;
            }
            let block = b.new_block();
            b.current = block;
            self.bind_match_pattern(b, arm.pat, scrut, arm.body);
            let op = self.lower_expr(b, arm.body);
            b.push_assign(dest, Rvalue::Use(op), arm.body);
            b.terminate(TerminatorKind::Goto { target: join }, expr);
        }
        b.current = join;
        Operand::Copy(dest)
    }

    /// How an arm participates in dispatch: a variant arm (keyed by index,
    /// only when its variant belongs to `scrut_enum`), a catch-all, or
    /// dead (broken pattern, wrong enum — already diagnosed).
    fn arm_kind(&self, pat: PatId, scrut_enum: Option<&ItemLoc>) -> ArmKind {
        match &self.body.pats[pat] {
            PatData::Missing => ArmKind::Dead,
            PatData::Wildcard => ArmKind::CatchAll,
            // A bare bind always binds the whole scrutinee — never a
            // variant (see `check_match_pat`), so always a catch-all.
            PatData::Bind(_) => ArmKind::CatchAll,
            PatData::Variant { .. } => match self.infer.variant_of_pat.get(pat) {
                Some(vt) if scrut_enum.is_none_or(|d| *d == vt.decl) => ArmKind::Variant(vt.index),
                _ => ArmKind::Dead,
            },
            // `let`/parameter-only shapes; `match_pattern`'s grammar never
            // produces them. Defensive fallback.
            PatData::Record { .. } | PatData::Newtype { .. } => ArmKind::Dead,
        }
    }

    /// Emit the arm's pattern bindings from the pinned scrutinee operand.
    /// A variant pattern reads its payloads out of the (tagged or
    /// variant-typed) value by field index; a bare bind aliases the whole
    /// value.
    fn bind_match_pattern(
        &mut self,
        b: &mut BodyBuilder,
        pat: PatId,
        scrut: &Operand,
        origin: ExprId,
    ) {
        match &self.body.pats[pat].clone() {
            PatData::Missing | PatData::Wildcard => {}
            PatData::Bind(binding) => {
                // A bare bind always binds the whole scrutinee (never
                // narrowed to a variant), so it just aliases the value.
                let local = self.alloc_binding_local(b, *binding);
                b.push_assign(local, Rvalue::Use(scrut.clone()), origin);
            }
            PatData::Variant { bindings, .. } => {
                let payloads = self
                    .infer
                    .variant_of_pat
                    .get(pat)
                    .and_then(|vt| {
                        hir::enum_variants(self.db, vt.decl.to_id(self.db))
                            .as_ref()?
                            .get(vt.index as usize)
                            .map(|(_, payload)| payload.len())
                    })
                    .unwrap_or(0);
                for (index, &binding) in bindings.iter().enumerate() {
                    let local = self.alloc_binding_local(b, binding);
                    if index < payloads {
                        b.push_assign(
                            local,
                            Rvalue::Field {
                                base: scrut.clone(),
                                index: index as u32,
                            },
                            origin,
                        );
                    } else {
                        // Arity error (already trapped): a placeholder
                        // keeps the local initialized and lowering total.
                        b.push_assign(local, Rvalue::Use(Operand::Const(Const::Unit)), origin);
                    }
                }
            }
            // `let`/parameter-only shapes; `match_pattern`'s grammar never
            // produces them. Defensive fallback.
            PatData::Record { .. } | PatData::Newtype { .. } => {}
        }
    }

    /// Allocate (and register) the MIR local for a user binding — `let`s
    /// and pattern bindings share the shape.
    fn alloc_binding_local(&mut self, b: &mut BodyBuilder, binding: BindingId) -> LocalId {
        let data = &self.body.bindings[binding];
        let local = b.locals.alloc(LocalData {
            ty: self
                .infer
                .type_of_binding
                .get(binding)
                .cloned()
                .unwrap_or(Ty::Error),
            name: (!data.name.is_empty()).then(|| data.name.clone()),
            binding: Some(binding),
        });
        b.local_for_binding.insert(binding, local);
        local
    }

    /// Allocate the MIR local(s) for one `let`/parameter pattern's value —
    /// a parameter's counterpart of a `let`'s [`Stmt::Let`] lowering (see
    /// `ExprData::Block`'s arm above). A bare name is exactly the ordinary
    /// single local, no extra indirection; a `Record`/`Newtype` pattern
    /// additionally allocates one synthetic "whole value" local (this is
    /// the parameter's own slot — what callers' arguments line up against)
    /// and destructures every binding it introduces out of it.
    fn alloc_pat_slot_local(&mut self, b: &mut BodyBuilder, pat: PatId, origin: ExprId) -> LocalId {
        match &self.body.pats[pat] {
            PatData::Bind(binding) => self.alloc_binding_local(b, *binding),
            PatData::Record { .. } | PatData::Newtype { .. } => {
                let ty = self
                    .infer
                    .type_of_pat
                    .get(pat)
                    .cloned()
                    .unwrap_or(Ty::Error);
                let local = b.locals.alloc(LocalData {
                    ty,
                    name: None,
                    binding: None,
                });
                self.bind_binding_pattern(b, pat, &Operand::Copy(local), origin);
                local
            }
            // A hole lowers as `Bind` (see `LowerCtx::lower_binding_pattern`
            // in `hir`), so `Wildcard` never reaches here in practice; kept
            // total regardless — still needs a slot so callers' argument
            // lists line up positionally.
            PatData::Wildcard | PatData::Missing => {
                let ty = self
                    .infer
                    .type_of_pat
                    .get(pat)
                    .cloned()
                    .unwrap_or(Ty::Error);
                b.locals.alloc(LocalData {
                    ty,
                    name: None,
                    binding: None,
                })
            }
            PatData::Variant { .. } => {
                // Never produced by `binding_pattern`'s grammar; defensive.
                b.locals.alloc(LocalData {
                    ty: Ty::Error,
                    name: None,
                    binding: None,
                })
            }
        }
    }

    /// Destructure `value` into every binding `pat` introduces — the
    /// `let`/parameter counterpart of [`Self::bind_match_pattern`], minus
    /// the enum-tag concerns (a `let`/parameter pattern is never matched
    /// against a tagged value; there is no `Cover` to compute, every
    /// binding here always runs). Field reads reuse the exact name→index
    /// lookup `ExprData::Field` uses (the receiver's type is resolved at
    /// lowering, so the field name is already gone by the time it reaches
    /// MIR); a `Newtype` unwrap is a pure retype at runtime (see
    /// [`hir::body::PatData::Newtype`]'s doc comment) — no MIR operation,
    /// just a recursive call with the same operand.
    fn bind_binding_pattern(
        &mut self,
        b: &mut BodyBuilder,
        pat: PatId,
        value: &Operand,
        origin: ExprId,
    ) {
        match &self.body.pats[pat].clone() {
            PatData::Missing | PatData::Wildcard => {}
            PatData::Bind(binding) => {
                let local = self.alloc_binding_local(b, *binding);
                b.push_assign(local, Rvalue::Use(value.clone()), origin);
            }
            PatData::Record { fields, .. } => {
                let rec = match self.infer.type_of_pat.get(pat) {
                    Some(Ty::Record(rec)) => Some(rec.clone()),
                    Some(Ty::Named(loc)) => match hir::type_underlying(self.db, loc.to_id(self.db))
                    {
                        Some(Ty::Record(rec)) => Some(rec),
                        _ => None,
                    },
                    _ => None,
                };
                for f in fields {
                    let local = self.alloc_binding_local(b, f.binding);
                    let index = rec
                        .as_ref()
                        .and_then(|rec| rec.fields.iter().position(|(n, _)| *n == f.field));
                    match index {
                        Some(index) => b.push_assign(
                            local,
                            Rvalue::Field {
                                base: value.clone(),
                                index: index as u32,
                            },
                            origin,
                        ),
                        // Unknown field (already diagnosed): a placeholder
                        // keeps the local initialized and lowering total.
                        None => {
                            b.push_assign(local, Rvalue::Use(Operand::Const(Const::Unit)), origin)
                        }
                    }
                }
            }
            PatData::Newtype { inner, .. } => {
                self.bind_binding_pattern(b, *inner, value, origin);
            }
            PatData::Variant { .. } => {
                // Never produced by `binding_pattern`'s grammar; defensive.
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
            // A type has no value to read. Construction heads never get
            // here (the `Call` arm intercepts them); every other read is
            // justified by the type-not-a-value diagnostic, whose message
            // this trap re-renders (kept total even if the seeding ever
            // drifts).
            Some(Resolution::TypeItem(_)) => {
                let message = InferenceDiagnostic::TypeNotValue {
                    expr,
                    name: name.to_owned(),
                }
                .message();
                self.trap(b, expr, message)
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

    /// Lower an assignment's LHS: writes `value_op` into the target's place
    /// instead of reading it. The only real writes are to a `mut` local
    /// that already has a slot in this body — the same local a `let`
    /// allocated, so the assignment reuses it rather than minting a new
    /// one — and through a chain of field projections rooted at one (see
    /// [`Self::lower_field_assign_target`]). Every other case traps instead
    /// of silently dropping the RHS's effects, always with the message of a
    /// diagnostic already reported on the target: inference's assignment
    /// enforcement (immutable binding, item, builtin), the capture
    /// diagnostic (same as the read path), name resolution
    /// (unresolved/ambiguous), a parse error (missing target), or syntax
    /// validation (a non-place target).
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
        if let ExprData::Field { .. } = &self.body.exprs[target] {
            self.lower_field_assign_target(b, target, value, value_op);
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
                        .and_then(|it| it.kind.constness())
                        .unwrap_or(hir::Constness::Static);
                    let message = InferenceDiagnostic::AssignToItem {
                        target,
                        item: loc.clone(),
                        constness,
                    }
                    .message();
                    self.trap(b, target, message);
                }
                // Assigning to a type name: the target read already carries
                // the type-not-a-value diagnostic; trap with its message.
                Some(Resolution::TypeItem(_)) => {
                    let message = InferenceDiagnostic::TypeNotValue {
                        expr: target,
                        name: name.clone(),
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
            // Any other non-place target: validation already squiggled it
            // with exactly this text (a shared constant, so the two can't
            // drift).
            _ => {
                self.trap(b, target, syntax::CAN_ONLY_ASSIGN_TO_A_VARIABLE.to_owned());
            }
        }
    }

    /// Lower a field-chain assignment target (`p.x = e;`, `p.a.b = e;`):
    /// the write goes through a [`Place`] projection — the root binding's
    /// local plus the chain's field indices (resolved through the receiver
    /// types exactly like [`Self::field_index`] read projections). The
    /// target is never lowered as a read, so the traps inference/validation
    /// reported on it are reconciled here, mirroring the plain-name path:
    /// the root's assignment enforcement first (immutable root — the
    /// headline diagnostic — item, builtin), then any broken link in the
    /// chain (unknown field, non-record receiver, a root that isn't even a
    /// value), then the root's own resolution failures.
    fn lower_field_assign_target(
        &mut self,
        b: &mut BodyBuilder,
        target: ExprId,
        value: ExprId,
        value_op: Operand,
    ) {
        // The chain's field-access expressions, outermost first; `root` is
        // the non-field expression at its base.
        let mut chain = Vec::new();
        let mut root = target;
        while let ExprData::Field { receiver, .. } = &self.body.exprs[root] {
            chain.push(root);
            root = *receiver;
        }
        // Inference rejected the root as an assignment target (immutable
        // binding, item, builtin): trap with the squiggle's exact text.
        if let Some(message) = self.assign_traps.get(&root).cloned() {
            self.trap(b, root, message);
            return;
        }
        // A broken link, root-outward: the root read itself (a type name,
        // …), then each projection (unknown field, non-record receiver,
        // undetermined type) — all pending value traps seeded from the
        // target's read-typing.
        for &expr in std::iter::once(&root).chain(chain.iter().rev()) {
            if let Some(message) = self.value_traps.get(&expr).cloned() {
                self.trap(b, expr, message);
                return;
            }
        }
        let ExprData::NameRef(name) = &self.body.exprs[root] else {
            match &self.body.exprs[root] {
                // Justified by the parse errors of the broken source.
                ExprData::Missing => {
                    self.trap(b, root, "syntax error: missing expression".to_owned());
                }
                // A chain rooted in a non-variable: validation already
                // squiggled the whole target with exactly this text.
                _ => {
                    self.trap(b, target, syntax::CAN_ONLY_ASSIGN_TO_A_VARIABLE.to_owned());
                }
            }
            return;
        };
        match self.resolutions.get(root) {
            Some(Resolution::Local(binding)) => match b.local_for_binding.get(binding) {
                Some(&local) => {
                    // Innermost projection first: `p.a.b` writes through
                    // `a`'s index in `p`, then `b`'s index in `p.a`.
                    let mut projection = Vec::with_capacity(chain.len());
                    for &field_expr in chain.iter().rev() {
                        let ExprData::Field { receiver, name } = &self.body.exprs[field_expr]
                        else {
                            unreachable!("chain holds only field expressions");
                        };
                        match self.field_index(*receiver, name) {
                            Some(index) => projection.push(index),
                            // No index and no diagnosed trap above: the
                            // receiver's type is `{error}` (infectious and
                            // silent), so the value the root would hold is
                            // already trapped upstream — skip the write,
                            // like the read path's never-observed
                            // placeholder, and keep lowering total.
                            None => return,
                        }
                    }
                    b.push_assign(Place { local, projection }, Rvalue::Use(value_op), value);
                }
                // A local of an enclosing function: the same unsupported
                // capture the read path (`lower_name_ref`) reports.
                None => {
                    let diag = MirDiagnostic::UnsupportedCapture {
                        expr: root,
                        name: name.clone(),
                    };
                    let message = diag.message();
                    self.diagnostics.push(diag);
                    self.trap(b, root, message);
                }
            },
            // Item and builtin roots were seeded into `assign_traps` above
            // (inference always reports them — fields don't make a
            // non-place root assignable), so these arms are unreachable in
            // practice; kept total by re-rendering the same diagnostics'
            // messages, like `lower_assign_target`'s twins.
            Some(Resolution::Item(loc)) => {
                let constness = hir::item_data(self.db, loc.to_id(self.db))
                    .as_ref()
                    .and_then(|it| it.kind.constness())
                    .unwrap_or(hir::Constness::Static);
                let message = InferenceDiagnostic::AssignToItem {
                    target: root,
                    item: loc.clone(),
                    constness,
                }
                .message();
                self.trap(b, root, message);
            }
            // A type name's field: the root read already carries the
            // type-not-a-value diagnostic (a pending value trap, handled
            // above); re-render for totality.
            Some(Resolution::TypeItem(_)) => {
                let message = InferenceDiagnostic::TypeNotValue {
                    expr: root,
                    name: name.clone(),
                }
                .message();
                self.trap(b, root, message);
            }
            Some(Resolution::Builtin(builtin)) => {
                let message = InferenceDiagnostic::AssignToBuiltin {
                    target: root,
                    builtin: *builtin,
                }
                .message();
                self.trap(b, root, message);
            }
            // Justified by the duplicate-definition diagnostics.
            Some(Resolution::Ambiguous(_)) => {
                self.trap(b, root, hir::diag::defined_multiple_times(name));
            }
            // Justified by the unresolved-name diagnostic.
            None => {
                self.trap(b, root, hir::diag::unresolved_name(name));
            }
        }
    }

    /// Synthesize the body of a first-class variant constructor
    /// (`let f = Shape::Circle; f(3)`): one block that assembles the
    /// parameters into the tag-free payload aggregate and returns it. The
    /// same shape a user-written `fn (p0, ...) { Shape::Circle(p0, ...) }`
    /// would lower to, minus the interception.
    fn synth_ctor_body(
        &mut self,
        expr: ExprId,
        variant: &hir::VariantTy,
        payload_tys: &[Ty],
    ) -> BodyId {
        let ret_ty = Ty::Variant(variant.clone());
        let mut b = BodyBuilder::new(ret_ty, expr);
        for ty in payload_tys {
            let local = b.locals.alloc(LocalData {
                ty: ty.clone(),
                name: None,
                binding: None,
            });
            b.params.push(local);
        }
        let ops = b.params.iter().map(|&p| Operand::Copy(p)).collect();
        let ret = b.ret;
        b.push_assign(
            ret,
            Rvalue::Aggregate {
                kind: AggregateKind::VariantPayload,
                ops,
            },
            expr,
        );
        b.terminate(TerminatorKind::Return, expr);
        self.bodies.alloc(MirBody {
            locals: b.locals,
            blocks: b.blocks,
            params: b.params,
            entry: b.entry,
        })
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

/// How one match arm participates in dispatch.
enum ArmKind {
    /// Keyed by variant index in the scrutinee's enum.
    Variant(u32),
    /// A `_` or plain-binding arm: matches anything.
    CatchAll,
    /// Never dispatched to (broken pattern, wrong enum — already
    /// diagnosed); lowers as an unreachable block.
    Dead,
}

/// One live `loop` during lowering: where `continue` goes (the header),
/// where `break` goes (the exit), and the local carrying the loop's value.
#[derive(Clone, Copy)]
struct LoopFrame {
    header: BlockId,
    exit: BlockId,
    result: LocalId,
}

struct BodyBuilder {
    locals: Arena<LocalData>,
    blocks: Arena<BlockData>,
    params: Vec<LocalId>,
    local_for_binding: FxHashMap<BindingId, LocalId>,
    ret: LocalId,
    entry: BlockId,
    current: BlockId,
    /// The loops currently being lowered, innermost last. Lives on the
    /// *builder* — one per MIR body — so `fn` literals and `const` blocks
    /// (which lower to bodies of their own) reset it structurally: a
    /// `break` in them can never target a block of the enclosing body.
    loop_frames: Vec<LoopFrame>,
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
            loop_frames: Vec::new(),
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

    fn push_assign(&mut self, dest: impl Into<Place>, rvalue: Rvalue, origin: ExprId) {
        self.blocks[self.current].statements.push(Statement {
            kind: StatementKind::Assign {
                dest: dest.into(),
                rvalue,
            },
            origin,
        });
    }

    fn terminate(&mut self, kind: TerminatorKind, origin: ExprId) {
        self.blocks[self.current].terminator = Terminator { kind, origin };
    }
}
