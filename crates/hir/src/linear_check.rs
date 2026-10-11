//! Must-consume checking, the half that needs no path: what one expression
//! or one pattern does with a value whose type lacks the `forget`
//! capability — copied out of a place, repeated, dropped, written over, or
//! skipped by a pattern (M16; whether a path consumed it is `mir::moves`'s).

use base_db::Db;
use la_arena::ArenaMap;
use rustc_hash::FxHashSet;

use crate::body::{Body, ExprData, ExprId, PatData, PatId, Stmt, body};
use crate::capability::{Affine, has_forget};
use crate::infer::{InferenceResult, infer};
use crate::scopes::{Resolution, resolutions};
use crate::ty::Ty;
use crate::{ItemId, diag};

/// Range-free (keyed by HIR ids); the diagnostics layer attaches ranges
/// through the body source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearDiagnostic {
    /// A linear value produced by a statement whose value is discarded.
    Discarded { expr: ExprId },
    /// A linear value borrowed as a temporary: it has no name, so nothing
    /// could consume it.
    BorrowedTemporary { expr: ExprId },
    /// A linear read out of a place that is not a whole owned binding — a
    /// field of a value, an element of an array. The read would COPY, and a
    /// copy of a linear is two obligations naming one resource.
    CopiedOut { expr: ExprId },
    /// `[s; 3]` where `s` is linear: the repeat form duplicates.
    Repeated { expr: ExprId },
    /// A `..` in a record pattern that would silently skip a linear field.
    ///
    /// `param` is the binder to blame when the field is linear only
    /// because a rigid parameter is — carried here rather than looked up
    /// at render time, because the type it answers for is the FIELD's and
    /// only this walk knows which field that was.
    RestSkipsLinear {
        pat: PatId,
        field: String,
        param: Option<String>,
    },
    /// A `_` pattern on an OWNED linear value: a match arm's (the match
    /// consumed the scrutinee), a `let`'s, or a parameter's, at any depth
    /// of a destructuring pattern. The value has nowhere to go. `param` is
    /// the binder to blame when the skipped value's type is linear only
    /// because a rigid parameter is.
    WildcardSkipsLinear { pat: PatId, param: Option<String> },
    /// An item's own value is linear. A `static` is never destroyed, so
    /// holding a linear in one is a leak with no exit path to blame.
    ItemHoldsLinear { expr: ExprId },
    /// A `const { ... }` block whose own value is linear. A const block is
    /// a CONSTANT — computed once and copied into every evaluation — so it
    /// is [`Self::ItemHoldsLinear`]'s reasoning one nesting level down: as
    /// many obligations as there are evaluations, and no path that could
    /// discharge them exactly once.
    ConstBlockHoldsLinear { expr: ExprId },
    /// Assignment over a linear place that is not a whole binding — a
    /// record field, an array element, a `.&mut` referent. The place holds
    /// such a value BY TYPE, so the write always loses one. A raw-pointer
    /// deref target is exempt: that is `unsafe`'s hatch.
    AssignOverPlace { target: ExprId },
}

impl LinearDiagnostic {
    /// The expression the squiggle lands on.
    pub fn expr(&self) -> Option<ExprId> {
        match self {
            LinearDiagnostic::Discarded { expr }
            | LinearDiagnostic::BorrowedTemporary { expr }
            | LinearDiagnostic::CopiedOut { expr }
            | LinearDiagnostic::Repeated { expr }
            | LinearDiagnostic::ItemHoldsLinear { expr }
            | LinearDiagnostic::ConstBlockHoldsLinear { expr } => Some(*expr),
            LinearDiagnostic::AssignOverPlace { target } => Some(*target),
            LinearDiagnostic::RestSkipsLinear { .. }
            | LinearDiagnostic::WildcardSkipsLinear { .. } => None,
        }
    }

    /// The pattern the squiggle lands on, for the variants that have no
    /// expression to point at.
    pub fn pat(&self) -> Option<PatId> {
        match self {
            LinearDiagnostic::RestSkipsLinear { pat, .. }
            | LinearDiagnostic::WildcardSkipsLinear { pat, .. } => Some(*pat),
            _ => None,
        }
    }

    /// Whether this finding is about DUPLICATION (a value copied out of a
    /// place, or repeated) rather than about a value that was lost.
    ///
    /// The two families ask different questions of the type, so they take
    /// different ROOTS. Duplication is refused for anything the copy rule
    /// refuses — a rigid parameter bounded or not, an exclusive borrow held
    /// inside the value — while losing a value is only the parameter's
    /// fault when the parameter is what made the value linear. Getting this
    /// backwards would offer `T: forget` as the fix for a program the bound
    /// does not fix.
    pub fn about_duplication(&self) -> bool {
        matches!(
            self,
            LinearDiagnostic::CopiedOut { .. } | LinearDiagnostic::Repeated { .. }
        )
    }

    /// The message as reported for a finding of `item`'s check: the
    /// blamed root read off its types.
    pub fn render(&self, db: &dyn Db, item: ItemId<'_>) -> String {
        let types = infer(db, item);
        let ty = self.expr().and_then(|expr| types.type_of_expr.get(expr));
        // Duplication and loss ask different questions of the type, so
        // they blame different roots (see [`Self::about_duplication`]).
        let root = ty.and_then(|ty| {
            if self.about_duplication() {
                crate::capability::affine_root(db, ty)
            } else {
                crate::capability::blaming_param(db, ty).map(Affine::Param)
            }
        });
        self.message(root.as_ref())
    }

    /// The message, given — when the value's type has a root the message
    /// can name — what that root is.
    ///
    /// Inside a generic body every message that explains why a value could
    /// not be lost or copied is about the binder: nothing was declared
    /// `only move`, so a reader sent looking for a declaration finds none.
    ///
    /// Which hint, and therefore which advice, is decided by
    /// [`Self::about_duplication`]. The LEAK arms may offer `T: forget`,
    /// because they fire only where an unbounded parameter is the reason
    /// the value had to be consumed. The DUPLICATION arms may not offer the
    /// bound: a bounded parameter reaches them too, and for one of them the
    /// bound has already waived consumption — so they say what is true of
    /// every root, which is that nothing grants copying.
    ///
    /// Two arms ignore the `param` they are handed and use one they
    /// carried from the walk instead. The type they answer for is not an
    /// expression's — a `..` blames the FIELD it skipped, a match-arm `_`
    /// blames the SCRUTINEE it swallowed — so only the walk that found them
    /// knows which type to blame. The one arm that takes no hint at all is
    /// an ITEM's own value, which is never inside a generic body's scope.
    pub fn message(&self, root: Option<&Affine>) -> String {
        let param = match root {
            Some(Affine::Param(param)) => Some(param.as_str()),
            _ => None,
        };
        match self {
            LinearDiagnostic::Discarded { .. } => match param {
                Some(param) => diag::discarded_param(param),
                None => diag::DISCARDED_LINEAR.to_owned(),
            },
            // No `param` split: the advice is the same sentence whether the
            // type is linear or a parameter that might be, because the
            // problem is the borrow's operand and not the bound.
            LinearDiagnostic::BorrowedTemporary { .. } => {
                diag::BORROWED_LINEAR_TEMPORARY.to_owned()
            }
            LinearDiagnostic::CopiedOut { .. } => root
                .and_then(diag::copied_out_dup)
                .unwrap_or_else(|| diag::COPIED_OUT_LINEAR.to_owned()),
            LinearDiagnostic::Repeated { .. } => root
                .and_then(diag::repeated_dup)
                .unwrap_or_else(|| diag::REPEATED_LINEAR.to_owned()),
            LinearDiagnostic::RestSkipsLinear { field, param, .. } => match param {
                Some(param) => diag::rest_skips_param(field, param),
                None => diag::rest_skips_linear(field),
            },
            LinearDiagnostic::WildcardSkipsLinear { param, .. } => match param {
                Some(param) => diag::wildcard_skips_param(param),
                None => diag::WILDCARD_SKIPS_LINEAR.to_owned(),
            },
            LinearDiagnostic::ItemHoldsLinear { .. } => diag::ITEM_HOLDS_LINEAR.to_owned(),
            // Reached through the ANNOTATION rather than through a
            // runtime binding, which a const context indeed cannot read:
            // `let x: T = const { panic("x") }` types the block from its
            // position while a diverging body supplies the value. Leak
            // family like its item-level twin, and the one whose hint
            // cannot offer consumption — see [`diag::const_block_holds_param`].
            LinearDiagnostic::ConstBlockHoldsLinear { .. } => match param {
                Some(param) => diag::const_block_holds_param(param),
                None => diag::CONST_BLOCK_HOLDS_LINEAR.to_owned(),
            },
            LinearDiagnostic::AssignOverPlace { .. } => match param {
                Some(param) => diag::assign_over_place_param(param),
                None => diag::ASSIGN_OVER_PLACE.to_owned(),
            },
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn linear_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<LinearDiagnostic> {
    let body = body(db, item);
    let inference = infer(db, item);
    let mut ctx = CheckCtx {
        db,
        body,
        resolutions: resolutions(db, item),
        infer: inference,
        diagnostics: Vec::new(),
        error_exprs: inference
            .diagnostics
            .iter()
            .filter(|diag| diag.severity() == crate::Severity::Error)
            .map(|diag| diag.expr())
            .collect(),
    };
    if let Some(root) = body.root {
        ctx.read_value(root);
        // The item's VALUE is the item's value, and a `static` is never
        // destroyed.
        if ctx.is_linear_expr(root) {
            // `static held = const { make(1) };` is one mistake, and the
            // const block's own complaint is this one a nesting level down
            // — said on the very same range. The item-level sentence names
            // why a `static` in particular can never discharge the
            // obligation, so it is the one that survives.
            ctx.diagnostics.retain(|diag| {
                !matches!(diag, LinearDiagnostic::ConstBlockHoldsLinear { expr } if *expr == root)
            });
            ctx.diagnostics
                .push(LinearDiagnostic::ItemHoldsLinear { expr: root });
        }
    }
    ctx.diagnostics
}

struct CheckCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    infer: &'db InferenceResult,
    diagnostics: Vec<LinearDiagnostic>,
    /// Every expression inference reported an ERROR at: a copy-out
    /// inference already refused is not repeated.
    error_exprs: FxHashSet<ExprId>,
}

impl CheckCtx<'_> {
    // ---- the type question ---------------------------------------------

    fn is_linear(&self, ty: &Ty) -> bool {
        !has_forget(self.db, ty)
    }

    fn is_linear_expr(&self, expr: ExprId) -> bool {
        self.infer
            .type_of_expr
            .get(expr)
            .is_some_and(|ty| self.is_linear(ty))
    }

    /// Whether a value of this expression's type may NOT be duplicated by a
    /// plain read, the question a copy-out and an array-repeat ask. A bare
    /// borrow is exempt (M07); a value HOLDING an exclusive borrow is not.
    fn is_unduplicable_expr(&self, expr: ExprId) -> bool {
        self.infer.type_of_expr.get(expr).is_some_and(|ty| {
            self.is_linear(ty)
                || (!matches!(ty, Ty::Borrow { .. })
                    && !crate::capability::is_copyable(self.db, ty))
        })
    }

    /// Whether a dot-call resolved to a MEMBER (inherent, trait-impl,
    /// builtin or bound-directed) rather than falling back to a call of a
    /// field's fn value. Every map inference records one in counts: the
    /// question is only "does the receiver travel as an argument".
    fn is_member_call(&self, expr: ExprId) -> bool {
        self.infer.member_of_expr.get(expr).is_some()
            || self.infer.builtin_member_of_expr.get(expr).is_some()
            || self.infer.bound_member_of_expr.get(expr).is_some()
    }

    /// A `..` that would skip a linear field, or a `_` that would skip a
    /// linear value. Both mean "don't bind this", which for a forgettable
    /// value is exactly right and for a linear one is a leak with no name to
    /// report it under — so it is refused at the pattern instead.
    fn check_skipping_patterns(&mut self, pat: PatId) {
        match &self.body.pats[pat] {
            PatData::Wildcard => {
                if let Some(ty) = self.infer.type_of_pat.get(pat).cloned() {
                    self.check_wildcard(pat, &ty);
                }
            }
            PatData::Record { fields, rest: true } => {
                let Some(ty) = self.infer.type_of_pat.get(pat) else {
                    return;
                };
                let record = match ty {
                    Ty::Record(record) => Some(record.clone()),
                    Ty::Named(named) => match crate::ty::type_underlying_for(self.db, named) {
                        Some(Ty::Record(record)) => Some(record),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(record) = record else { return };
                for (name, field_ty) in &record.fields {
                    if fields.iter().any(|f| &f.field == name) {
                        continue;
                    }
                    if self.is_linear(field_ty) {
                        self.diagnostics.push(LinearDiagnostic::RestSkipsLinear {
                            pat,
                            field: name.clone(),
                            param: crate::capability::blaming_param(self.db, field_ty),
                        });
                    }
                }
            }
            PatData::Newtype { inner, .. } => {
                let inner = *inner;
                self.check_skipping_patterns(inner);
            }
            PatData::Array { elements } => {
                for element in elements.clone() {
                    self.check_skipping_patterns(element);
                }
            }
            _ => {}
        }
    }

    /// A `_` that would skip a linear value of type `ty`.
    fn check_wildcard(&mut self, pat: PatId, ty: &Ty) {
        if self.is_linear(ty) {
            self.diagnostics
                .push(LinearDiagnostic::WildcardSkipsLinear {
                    pat,
                    param: crate::capability::blaming_param(self.db, ty),
                });
        }
    }

    /// A copy-out, unless inference already refused the same read at the
    /// same place. `r.*[0]` of a linear element is BOTH "cannot move out of
    /// a borrow" and "cannot copy a value that must be consumed"; they are
    /// one mistake, and the borrow-side message is the one that names the
    /// place.
    fn report_copied_out(&mut self, expr: ExprId) {
        if self.error_exprs.contains(&expr) {
            return;
        }
        self.diagnostics.push(LinearDiagnostic::CopiedOut { expr });
    }

    // ---- the walk -------------------------------------------------------

    /// `expr`'s value is produced and used by whoever asked.
    fn read_value(&mut self, expr: ExprId) {
        match &self.body.exprs[expr] {
            ExprData::NameRef(_)
            | ExprData::Missing
            | ExprData::Literal(_)
            | ExprData::Continue
            // A signature and nothing else: no value is produced from a
            // linear one.
            | ExprData::ExternItem
            | ExprData::ElidedVariant { .. }
            | ExprData::VariantPath { .. }
            | ExprData::GenericApp { .. } => {}
            ExprData::Field { receiver, .. } => {
                let receiver = *receiver;
                self.walk_place(receiver);
                // Reading a linear FIELD out of a place would copy it, and
                // the place keeps its own copy: two obligations, one
                // resource. Taking the whole value apart is the way in.
                if self.is_unduplicable_expr(expr) {
                    self.report_copied_out(expr);
                }
            }
            ExprData::Index { base, index } => {
                let (base, index) = (*base, *index);
                self.walk_place(base);
                self.read_value(index);
                if self.is_unduplicable_expr(expr) {
                    self.report_copied_out(expr);
                }
            }
            ExprData::Call {
                callee,
                args,
                dot_call,
            } => {
                let (callee, args, dot_call) = (*callee, args.clone(), *dot_call);
                // A dot-call that resolved to a MEMBER passes its receiver
                // as an argument (TR01), so the receiver is a value read,
                // not a projection. No auto-borrow exists, so the
                // receiver's own type is exactly what is passed.
                let member = dot_call && self.is_member_call(expr);
                let receiver = match &self.body.exprs[callee] {
                    ExprData::Field { receiver, .. } if member => Some(*receiver),
                    _ => None,
                };
                self.read_value(receiver.unwrap_or(callee));
                for arg in args {
                    self.read_value(arg);
                }
            }
            ExprData::Bin { lhs, rhs, .. } => {
                let (lhs, rhs) = (*lhs, *rhs);
                self.read_value(lhs);
                self.read_value(rhs);
            }
            ExprData::Neg { operand } => {
                let operand = *operand;
                self.read_value(operand);
            }
            ExprData::AddrOf { place, .. } | ExprData::Borrow { place, .. } => {
                let place = *place;
                self.walk_place(place);
            }
            // A copy out of a borrow of a linear is already inference's
            // "cannot move out of a borrow" (linear implies non-copyable);
            // nothing to add here.
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                self.read_value(receiver);
            }
            ExprData::RecordLit { fields } => {
                let values: Vec<ExprId> = fields.iter().map(|f| f.value).collect();
                for value in values {
                    self.read_value(value);
                }
            }
            ExprData::ArrayLit { elements } => {
                for element in elements.clone() {
                    self.read_value(element);
                }
            }
            ExprData::ArrayRepeat { element, count } => {
                let (element, count) = (*element, *count);
                self.read_value(element);
                if self.is_unduplicable_expr(element) {
                    self.diagnostics
                        .push(LinearDiagnostic::Repeated { expr: element });
                }
                self.read_value(count);
            }
            ExprData::ConstBlock { body } => {
                let body = *body;
                self.read_value(body);
                // The item-level rule one nesting level down: a const
                // block's value is a CONSTANT, copied into every
                // evaluation, so a linear one is as many obligations as
                // there are evaluations and no path discharges it once.
                // The question is the block's TYPE, not a path through it
                // — so a diverging body is no excuse: `!` is forgettable,
                // and a position that pinned the block to a linear type
                // anyway is the mistake being reported.
                if self.is_linear_expr(expr) {
                    self.diagnostics
                        .push(LinearDiagnostic::ConstBlockHoldsLinear { expr });
                }
            }
            ExprData::Unsafe { body } | ExprData::Loop { body } => {
                let body = *body;
                self.read_value(body);
            }
            ExprData::Block { stmts, tail } => {
                let (stmts, tail) = (stmts.clone(), *tail);
                for stmt in &stmts {
                    self.check_stmt(stmt);
                }
                if let Some(tail) = tail {
                    self.read_value(tail);
                }
            }
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let (condition, then_branch, else_branch) =
                    (*condition, *then_branch, *else_branch);
                self.read_value(condition);
                self.read_value(then_branch);
                if let Some(branch) = else_branch {
                    self.read_value(branch);
                }
            }
            ExprData::Match { scrutinee, arms } => {
                let (scrutinee, arms) = (*scrutinee, arms.clone());
                self.read_value(scrutinee);
                for arm in arms {
                    // An arm's whole `_` binds nothing and has the
                    // scrutinee's type: an owned linear one is refused, a
                    // borrowed one owes nothing.
                    if matches!(self.body.pats[arm.pat], PatData::Wildcard) {
                        if let Some(ty) = self.infer.type_of_expr.get(scrutinee).cloned() {
                            self.check_wildcard(arm.pat, &ty);
                        }
                    } else {
                        self.check_skipping_patterns(arm.pat);
                    }
                    self.read_value(arm.body);
                }
            }
            ExprData::Break { value } | ExprData::Return { value } => {
                if let Some(value) = *value {
                    self.read_value(value);
                }
            }
            ExprData::FnLiteral { params, body, .. } => {
                let (params, fn_body) = (params.clone(), *body);
                for param in &params {
                    self.check_skipping_patterns(param.pat);
                }
                self.read_value(fn_body);
            }
        }
    }

    /// Whether a place chain passes through a RAW pointer deref. Writing
    /// through one is `unsafe`, and what it does to a value that must be
    /// consumed is the caller's word — the same hatch raw storage already
    /// is. A `.&mut` deref is no hatch: it is safe code, and the value it
    /// writes over is one somebody still owes.
    fn writes_through_raw_pointer(&self, expr: ExprId) -> bool {
        match &self.body.exprs[expr] {
            ExprData::Field { receiver, .. } => self.writes_through_raw_pointer(*receiver),
            ExprData::Index { base, .. } => self.writes_through_raw_pointer(*base),
            ExprData::Deref { receiver } => {
                matches!(
                    self.infer.type_of_expr.get(*receiver),
                    Some(Ty::RawPtr { .. })
                ) || self.writes_through_raw_pointer(*receiver)
            }
            _ => false,
        }
    }

    /// `expr` names a PLACE whose root is not read out: the operand of a
    /// borrow, the target of a projected assignment, the receiver of a
    /// field access.
    fn walk_place(&mut self, expr: ExprId) {
        match &self.body.exprs[expr] {
            ExprData::NameRef(_) => {}
            ExprData::Field { receiver, .. } => {
                let receiver = *receiver;
                self.walk_place(receiver);
            }
            ExprData::Index { base, index } => {
                let (base, index) = (*base, *index);
                self.walk_place(base);
                self.read_value(index);
            }
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                self.read_value(receiver);
            }
            // Not a place: a value that is only projected from, so a linear
            // one is lost here. A linear temporary (M12) is refused outright:
            // it cannot be named, so nothing could ever consume it.
            _ => {
                self.read_value(expr);
                if self.is_linear_expr(expr) {
                    self.diagnostics
                        .push(if self.body.temp_local(expr).is_some() {
                            LinearDiagnostic::BorrowedTemporary { expr }
                        } else {
                            LinearDiagnostic::Discarded { expr }
                        });
                }
            }
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { pat, init, .. } => {
                let (pat, init) = (*pat, *init);
                if let Some(init) = init {
                    self.read_value(init);
                    self.check_skipping_patterns(pat);
                }
            }
            Stmt::Assign { target, value } => {
                let (target, value) = (*target, *value);
                self.read_value(value);
                // A whole binding as the target is `mir::moves`' question:
                // whether it still holds a value depends on the path.
                if matches!(self.resolutions.get(target), Some(Resolution::Local(_))) {
                    return;
                }
                self.walk_place(target);
                if self.is_linear_expr(target) && !self.writes_through_raw_pointer(target) {
                    // A field, an element, a `.&mut` referent: there is no
                    // binding whose path could say otherwise, and the place
                    // holds such a value by TYPE, so the write always loses
                    // one.
                    self.diagnostics
                        .push(LinearDiagnostic::AssignOverPlace { target });
                }
            }
            Stmt::Expr(expr) => {
                let expr = *expr;
                self.read_value(expr);
                if self.is_linear_expr(expr) {
                    self.diagnostics.push(LinearDiagnostic::Discarded { expr });
                }
            }
        }
    }
}
