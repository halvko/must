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
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { init, .. } => self.check_expr(*init, in_const),
                        Stmt::Expr(e) => self.check_expr(*e, in_const),
                    }
                }
                if let Some(tail) = tail {
                    self.check_expr(*tail, in_const);
                }
            }
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
            ExprData::NameRef(_) => match self.resolutions.get(callee) {
                Some(Resolution::Item(loc)) => {
                    match root_fn_is_const(self.db, loc.to_id(self.db)) {
                        Some(true) => {}
                        Some(false) => {
                            self.diagnostics.push(ConstCheckDiagnostic::NonConstFnCall {
                                callee,
                                item: loc.clone(),
                            })
                        }
                        // The item's root isn't literally a fn literal —
                        // maybe a wrapper around one, maybe not a function
                        // at all. No peeling: conservatively a value call.
                        None => self
                            .diagnostics
                            .push(ConstCheckDiagnostic::ValueCall { callee }),
                    }
                }
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
                // The duplicate definitions already carry a diagnostic; no
                // verdict about a use of the name is trustworthy.
                Some(Resolution::Ambiguous(_)) => {}
                // Unresolved: already reported by name resolution.
                None => {}
            },
            // Any other callee expression is a value of unknown const-ness.
            _ => self
                .diagnostics
                .push(ConstCheckDiagnostic::ValueCall { callee }),
        }
    }
}
