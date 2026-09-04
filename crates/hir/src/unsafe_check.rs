//! Unsafe checking: which operations require an `unsafe { ... }` block.
//!
//! The rule is exactly "operations whose misuse is UB": dereferencing a raw
//! pointer — reading `p.*` or writing `p.* = v;` — must sit inside an
//! `unsafe { ... }` block. Taking an address (`&raw x` / `&raw mut x`) is
//! safe (creating a pointer is harmless; the hazard is at the deref), and
//! so is pointer comparison.
//!
//! The region is lexical *within a function*: an `unsafe` block covers
//! everything written inside it, `const { ... }` blocks included (they are
//! separate compile-time bodies, but the checker judges source regions, and
//! unsafe operations are legal in const evaluation — every would-be UB
//! there is a deterministic detected trap). A `fn` literal's body RESETS
//! the region: it is a separate function that runs under any caller, so it
//! must declare its own unsafety — the same boundary reasoning as
//! [`crate::const_check`]'s.
//!
//! Enforcement is the house pattern end-to-end: findings here become editor
//! diagnostics, and MIR lowering plants a trap with the identical message
//! (squiggle text == trap text, single-render preserved).

use base_db::Db;
use la_arena::ArenaMap;

use crate::body::{Body, ExprData, ExprId, Stmt, body};
use crate::scopes::{Builtin, Resolution, resolutions};
use crate::{ItemId, diag};

/// Range-free (keyed by HIR ids); ranges are attached by the diagnostics
/// layer through the body source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsafeCheckDiagnostic {
    /// A raw-pointer deref (read or write) outside any `unsafe { ... }`
    /// block. The squiggle (and MIR's trap) lands on the deref expression.
    DerefOutsideUnsafe { expr: ExprId },
    /// A call of an unsafe builtin (`dealloc_array`, `copy` — see
    /// [`Builtin::requires_unsafe`]) outside any `unsafe { ... }` block.
    /// The squiggle (and MIR's trap) lands on the call expression: the
    /// call is the operation that must not run.
    BuiltinCallOutsideUnsafe { call: ExprId, builtin: Builtin },
}

impl UnsafeCheckDiagnostic {
    pub fn expr(&self) -> ExprId {
        match self {
            UnsafeCheckDiagnostic::DerefOutsideUnsafe { expr } => *expr,
            UnsafeCheckDiagnostic::BuiltinCallOutsideUnsafe { call, .. } => *call,
        }
    }

    /// Rendered through [`crate::diag`] so MIR traps carry exactly the text
    /// the squiggle showed.
    pub fn message(&self) -> String {
        match self {
            UnsafeCheckDiagnostic::DerefOutsideUnsafe { .. } => {
                diag::DEREF_REQUIRES_UNSAFE.to_owned()
            }
            UnsafeCheckDiagnostic::BuiltinCallOutsideUnsafe { builtin, .. } => {
                diag::builtin_call_requires_unsafe(builtin.name())
            }
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn unsafe_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<UnsafeCheckDiagnostic> {
    let body = body(db, item);
    let mut ctx = CheckCtx {
        body,
        resolutions: resolutions(db, item),
        diagnostics: Vec::new(),
    };
    if let Some(root) = body.root {
        ctx.check_expr(root, false);
    }
    ctx.diagnostics
}

struct CheckCtx<'db> {
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    diagnostics: Vec<UnsafeCheckDiagnostic>,
}

impl CheckCtx<'_> {
    fn check_expr(&mut self, expr: ExprId, in_unsafe: bool) {
        match &self.body.exprs[expr] {
            ExprData::Missing | ExprData::Literal(_) | ExprData::NameRef(_) => {}
            ExprData::VariantPath { args, .. } => {
                for arg in args.iter().flatten() {
                    if let crate::body::GenericArgData::Const(value) = arg {
                        self.check_expr(*value, in_unsafe);
                    }
                }
            }
            ExprData::GenericApp { args, .. } => {
                for arg in args {
                    if let crate::body::GenericArgData::Const(value) = arg {
                        self.check_expr(*value, in_unsafe);
                    }
                }
            }
            ExprData::Call { callee, args } => {
                // The unsafe builtins: freeing invalidates every pointer
                // into the allocation, and `copy` writes through a raw
                // pointer — misuse of either is UB, so the CALL needs the
                // marker, exactly like a deref. The callee may be a bare
                // name or a turbofish mention of one.
                let callee_name = match &self.body.exprs[*callee] {
                    ExprData::GenericApp { base, .. } => *base,
                    _ => *callee,
                };
                if !in_unsafe
                    && let Some(Resolution::Builtin(builtin)) = self.resolutions.get(callee_name)
                    && builtin.requires_unsafe()
                {
                    self.diagnostics
                        .push(UnsafeCheckDiagnostic::BuiltinCallOutsideUnsafe {
                            call: expr,
                            builtin: *builtin,
                        });
                }
                self.check_expr(*callee, in_unsafe);
                for &arg in args {
                    self.check_expr(arg, in_unsafe);
                }
            }
            ExprData::Bin { lhs, rhs, .. } => {
                self.check_expr(*lhs, in_unsafe);
                self.check_expr(*rhs, in_unsafe);
            }
            ExprData::Neg { operand } => self.check_expr(*operand, in_unsafe),
            ExprData::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(*condition, in_unsafe);
                self.check_expr(*then_branch, in_unsafe);
                if let Some(else_branch) = else_branch {
                    self.check_expr(*else_branch, in_unsafe);
                }
            }
            ExprData::Loop { body } => self.check_expr(*body, in_unsafe),
            ExprData::Break { value } => {
                if let Some(value) = value {
                    self.check_expr(*value, in_unsafe);
                }
            }
            ExprData::Continue => {}
            ExprData::Match { scrutinee, arms } => {
                self.check_expr(*scrutinee, in_unsafe);
                for arm in arms {
                    self.check_expr(arm.body, in_unsafe);
                }
            }
            ExprData::Block { stmts, tail } => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let { init, .. } => self.check_expr(*init, in_unsafe),
                        // An assignment target that is (or projects
                        // through) a deref is a deref *write*: the target's
                        // own traversal below flags it — one rule for reads
                        // and writes alike.
                        Stmt::Assign { target, value } => {
                            self.check_expr(*target, in_unsafe);
                            self.check_expr(*value, in_unsafe);
                        }
                        Stmt::Expr(e) => self.check_expr(*e, in_unsafe),
                    }
                }
                if let Some(tail) = tail {
                    self.check_expr(*tail, in_unsafe);
                }
            }
            ExprData::RecordLit { fields } => {
                for field in fields {
                    self.check_expr(field.value, in_unsafe);
                }
            }
            ExprData::Field { receiver, .. } => self.check_expr(*receiver, in_unsafe),
            ExprData::ArrayLit { elements } => {
                for &element in elements {
                    self.check_expr(element, in_unsafe);
                }
            }
            ExprData::ArrayRepeat { element, count } => {
                self.check_expr(*element, in_unsafe);
                self.check_expr(*count, in_unsafe);
            }
            ExprData::Index { base, index } => {
                self.check_expr(*base, in_unsafe);
                self.check_expr(*index, in_unsafe);
            }
            // Taking `&raw` is safe (the hazard is at the deref); the place
            // may still contain a deref of its own, which is judged as one.
            ExprData::AddrOf { place, .. } => self.check_expr(*place, in_unsafe),
            // THE unsafe operation: deref, read or write.
            ExprData::Deref { receiver } => {
                if !in_unsafe {
                    self.diagnostics
                        .push(UnsafeCheckDiagnostic::DerefOutsideUnsafe { expr });
                }
                self.check_expr(*receiver, in_unsafe);
            }
            // The region itself.
            ExprData::Unsafe { body } => self.check_expr(*body, true),
            // A `const` block is lexically inside the region (unsafe is
            // legal in const contexts — const-eval UB is a deterministic
            // detected trap).
            ExprData::ConstBlock { body } => self.check_expr(*body, in_unsafe),
            // A separate function: it runs under any caller, so it declares
            // its own unsafety — the region resets.
            ExprData::FnLiteral { body, .. } => self.check_expr(*body, false),
        }
    }
}
