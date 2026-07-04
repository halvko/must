//! Const-checking: which calls are legal in const contexts.
//!
//! Every item initializer is a const context — `static` and `const` items
//! alike compute their value at compile time. Entering the body of a plain
//! `fn` literal *exits* the const context (that body is runtime code); the
//! body of a `const fn` literal is always a const context (it must be
//! const-evaluable wherever it is defined); and a `const { ... }` block
//! re-enters one wherever it appears.
//!
//! Inside a const context the only calls allowed are direct calls of items
//! whose initializer is a `const fn` literal, direct calls of `const fn`
//! literals themselves, and the builtin `panic` — const evaluation permits
//! no side effect except panicking. Everything else is rejected
//! conservatively: const-ness is not part of function types (yet), so for a
//! parameter, a let-bound value, or any other expression it is simply not
//! known — even when a human can see what the value must be. For the same
//! reason no wrappers around an item's root expression are peeled: only a
//! literal `const fn` initializer makes an item const-callable.

use base_db::Db;
use la_arena::ArenaMap;

use crate::body::{Body, ExprData, ExprId, Stmt, body};
use crate::scopes::{Builtin, Resolution, resolutions};
use crate::{ItemId, ItemLoc, diag};

/// Range-free (keyed by HIR ids); ranges are attached by the diagnostics
/// layer through the body source map. Every variant's squiggle lands on the
/// callee expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstCheckDiagnostic {
    /// A call to a named fn item whose literal lacks the `const` marker —
    /// adding the marker is the fix, so the message says so.
    NonConstFnCall { callee: ExprId, item: ItemLoc },
    /// A directly-called plain `fn` literal. Kept apart from
    /// [`ConstCheckDiagnostic::ValueCall`] because here the const-ness *is*
    /// known (the literal visibly lacks the marker) and the fix is local.
    NonConstFnLiteralCall { callee: ExprId },
    /// A call to a side-effecting builtin (`print`). `panic` is the one
    /// side effect const contexts allow, so it never lands here.
    SideEffectCall { callee: ExprId, builtin: Builtin },
    /// A call to a value whose const-ness cannot be known: a parameter, a
    /// let-bound value, an arbitrary expression. Conservative by design —
    /// const-ness is not part of function types, so even a value provably
    /// bound to a `const fn` is rejected.
    ValueCall { callee: ExprId },
}

impl ConstCheckDiagnostic {
    /// The expression the diagnostic is reported on: always the callee.
    pub fn expr(&self) -> ExprId {
        match self {
            ConstCheckDiagnostic::NonConstFnCall { callee, .. }
            | ConstCheckDiagnostic::NonConstFnLiteralCall { callee }
            | ConstCheckDiagnostic::SideEffectCall { callee, .. }
            | ConstCheckDiagnostic::ValueCall { callee } => *callee,
        }
    }

    /// The human-readable message, rendered through [`crate::diag`] so the
    /// MIR traps can carry exactly the text the squiggle showed.
    pub fn message(&self) -> String {
        match self {
            ConstCheckDiagnostic::NonConstFnCall { item, .. } => {
                diag::non_const_fn_call(item.display_name())
            }
            ConstCheckDiagnostic::NonConstFnLiteralCall { .. } => {
                diag::NON_CONST_FN_LITERAL_CALL.to_owned()
            }
            ConstCheckDiagnostic::SideEffectCall { builtin, .. } => {
                diag::side_effect_call_in_const(builtin.name())
            }
            ConstCheckDiagnostic::ValueCall { .. } => diag::VALUE_CALL_IN_CONST.to_owned(),
        }
    }
}

/// Whether `item`'s initializer is a fn literal and, if so, whether it is
/// marked `const fn`. The one cross-item fact const-checking needs, as its
/// own query so an edit inside the target's body reaches other items'
/// [`const_check`] only when this value actually flips.
#[salsa::tracked]
pub fn root_fn_is_const<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<bool> {
    let body = body(db, item);
    match body.exprs[body.root?] {
        ExprData::FnLiteral { is_const, .. } => Some(is_const),
        _ => None,
    }
}

#[salsa::tracked(returns(ref))]
pub fn const_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<ConstCheckDiagnostic> {
    let body = body(db, item);
    let mut ctx = CheckCtx {
        db,
        body,
        resolutions: resolutions(db, item),
        diagnostics: Vec::new(),
    };
    if let Some(root) = body.root {
        ctx.check_expr(root, true);
    }
    ctx.diagnostics
}

struct CheckCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    diagnostics: Vec<ConstCheckDiagnostic>,
}

impl CheckCtx<'_> {
    fn check_expr(&mut self, expr: ExprId, in_const: bool) {
        match &self.body.exprs[expr] {
            ExprData::Missing | ExprData::Literal(_) | ExprData::NameRef(_) => {}
            // A variant path is a name (or a pure constructor value) —
            // nothing to reject; its base is a bare `NameRef`.
            ExprData::VariantPath { .. } => {}
            // A turbofish mention: the base is a bare `NameRef`; const
            // ARGUMENTS are compile-time positions wherever the mention
            // sits (they will be const-evaluated for instance identity), so
            // they check as const contexts unconditionally.
            ExprData::GenericApp { args, .. } => {
                for arg in args {
                    if let crate::body::GenericArgData::Const(value) = arg {
                        self.check_expr(*value, true);
                    }
                }
            }
            ExprData::Call { callee, args } => {
                if in_const {
                    self.check_callee(*callee);
                }
                // Callee and arguments are evaluated by the call, so they
                // sit in the same context as the call itself; a rejected
                // call does not suppress findings inside it.
                self.check_expr(*callee, in_const);
                for &arg in args {
                    self.check_expr(arg, in_const);
                }
            }
            ExprData::Bin { lhs, rhs, .. } => {
                self.check_expr(*lhs, in_const);
                self.check_expr(*rhs, in_const);
            }
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(*condition, in_const);
                self.check_expr(*then_branch, in_const);
                if let Some(else_branch) = else_branch {
                    self.check_expr(*else_branch, in_const);
                }
            }
            // Loops, `break` and `continue` are pure control flow —
            // const-legal (the eval machine's fuel bounds a runaway
            // compile-time loop); body and break values sit in the same
            // context as the loop.
            ExprData::Loop { body } => self.check_expr(*body, in_const),
            ExprData::Break { value } => {
                if let Some(value) = value {
                    self.check_expr(*value, in_const);
                }
            }
            ExprData::Continue => {}
            // `match` is pure control flow (patterns only destructure) —
            // const-legal; scrutinee and arm bodies sit in the same context.
            ExprData::Match { scrutinee, arms } => {
                self.check_expr(*scrutinee, in_const);
                for arm in arms {
                    self.check_expr(arm.body, in_const);
                }
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { init, .. } => self.check_expr(*init, in_const),
                        // Local mutation is allowed in const contexts (no
                        // rule rejects it here); just visit both
                        // sub-expressions the same as any other statement.
                        Stmt::Assign { target, value } => {
                            self.check_expr(*target, in_const);
                            self.check_expr(*value, in_const);
                        }
                        Stmt::Expr(e) => self.check_expr(*e, in_const),
                    }
                }
                if let Some(tail) = tail {
                    self.check_expr(*tail, in_const);
                }
            }
            // Record construction is not a call — nothing to reject; the
            // field initializers sit in the same context as the literal.
            ExprData::RecordLit { fields } => {
                for (_, field) in fields {
                    self.check_expr(*field, in_const);
                }
            }
            // A field access evaluates its receiver; the projection itself
            // has no effect.
            ExprData::Field { receiver, .. } => self.check_expr(*receiver, in_const),
            // Array construction and indexing are pure data operations —
            // const-legal, like record literals and field access; the
            // bounds trap is the ordinary rejected-op story.
            ExprData::ArrayLit { elements } => {
                for &element in elements {
                    self.check_expr(element, in_const);
                }
            }
            ExprData::ArrayRepeat { element, count } => {
                self.check_expr(*element, in_const);
                self.check_expr(*count, in_const);
            }
            ExprData::Index { base, index } => {
                self.check_expr(*base, in_const);
                self.check_expr(*index, in_const);
            }
            // Taking an address and dereferencing are not calls — const
            // contexts allow them (unsafe operations are legal in const
            // eval: every would-be UB there is a deterministic detected
            // trap); only the escape rule in `eval` guards the results.
            ExprData::AddrOf { place, .. } => self.check_expr(*place, in_const),
            ExprData::Deref { receiver } => self.check_expr(*receiver, in_const),
            // `unsafe { ... }` is a checker region, orthogonal to
            // const-ness: the body sits in the same context.
            ExprData::Unsafe { body } => self.check_expr(*body, in_const),
            // `const { ... }` re-enters a const context wherever it appears.
            ExprData::ConstBlock { body } => self.check_expr(*body, true),
            // *Defining* a function in a const context is always fine; only
            // calls are checked. A plain fn body is runtime code (exits the
            // const context), a `const fn` body must be const-evaluable
            // wherever the literal is defined.
            ExprData::FnLiteral { is_const, body, .. } => self.check_expr(*body, *is_const),
        }
    }

    /// `callee` is being called in a const context; reject it unless it is
    /// known to be const-callable.
    fn check_callee(&mut self, callee: ExprId) {
        match &self.body.exprs[callee] {
            // Broken source; the parse error is already reported.
            ExprData::Missing => {}
            // A directly-called fn literal wears its const-ness on its
            // sleeve. (Parens lower transparently, so `(const fn ...)(x)`
            // presents the literal as the callee too.)
            ExprData::FnLiteral { is_const: true, .. } => {}
            ExprData::FnLiteral {
                is_const: false, ..
            } => {
                self.diagnostics
                    .push(ConstCheckDiagnostic::NonConstFnLiteralCall { callee });
            }
            // A directly-called variant constructor (`Shape::Circle(3)`):
            // pure construction, like `Foo(...)` — always const-legal.
            ExprData::VariantPath { .. } => {}
            // A turbofish callee (`f::<usize>(4)`) is judged by what its
            // base names — calling an instantiated generic `const fn` in a
            // const context is legal (evaluation is staged, but the RULE
            // keys off the marker exactly as for a plain mention). The
            // diagnostic still lands on the whole callee expression.
            ExprData::GenericApp { base, .. } => self.check_named_callee(callee, *base),
            ExprData::NameRef(_) => self.check_named_callee(callee, callee),
            // Any other callee expression is a value of unknown const-ness.
            _ => self
                .diagnostics
                .push(ConstCheckDiagnostic::ValueCall { callee }),
        }
    }

    /// The name-resolution part of [`Self::check_callee`]: `name_expr` is
    /// the `NameRef` whose resolution is judged (the callee itself, or a
    /// turbofish callee's base); `callee` is where the diagnostic lands.
    fn check_named_callee(&mut self, callee: ExprId, name_expr: ExprId) {
        match self.resolutions.get(name_expr) {
            Some(Resolution::Item(loc)) => {
                match root_fn_is_const(self.db, loc.to_id(self.db)) {
                    Some(true) => {}
                    Some(false) => self.diagnostics.push(ConstCheckDiagnostic::NonConstFnCall {
                        callee,
                        item: loc.clone(),
                    }),
                    // The item's root isn't literally a fn literal —
                    // maybe a wrapper around one, maybe not a function
                    // at all. No peeling: conservatively a value call.
                    None => self
                        .diagnostics
                        .push(ConstCheckDiagnostic::ValueCall { callee }),
                }
            }
            // A construction call `Foo(...)`: pure construction, not a
            // user function — always legal in a const context (like
            // record literals, which it erases to at runtime).
            Some(Resolution::TypeItem(_)) => {}
            // The one side effect const contexts allow.
            Some(Resolution::Builtin(Builtin::Panic)) => {}
            Some(Resolution::Builtin(builtin @ Builtin::Print)) => {
                self.diagnostics.push(ConstCheckDiagnostic::SideEffectCall {
                    callee,
                    builtin: *builtin,
                });
            }
            Some(Resolution::Local(_)) => self
                .diagnostics
                .push(ConstCheckDiagnostic::ValueCall { callee }),
            // A const param's value is plain data — fn-valued const
            // params are excluded from the domain (TR06: concrete data
            // types only) — so calling one is a value call like calling
            // any local.
            Some(Resolution::ConstParam(_)) => self
                .diagnostics
                .push(ConstCheckDiagnostic::ValueCall { callee }),
            // The duplicate definitions already carry a diagnostic; no
            // verdict about a use of the name is trustworthy.
            Some(Resolution::Ambiguous(_)) => {}
            // Unresolved: already reported by name resolution.
            None => {}
        }
    }
}
