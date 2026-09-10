//! Unsafe checking: which operations require an `unsafe { ... }` block.
//!
//! The rule is exactly "operations whose misuse is UB": dereferencing a raw
//! pointer — reading `p.*` or writing `p.* = v;` — must sit inside an
//! `unsafe { ... }` block. Taking an address (`x.&raw` / `x.&raw mut`) is
//! safe (creating a pointer is harmless; the hazard is at the deref), and
//! so is pointer comparison.
//!
//! CALLS are the second family, and the question is asked of the callee's
//! TYPE, never the declaration: `unsafe fn(...)` is a type of its own (see
//! [`crate::ty::FnTy`]), and calling a value of it needs the marker — a
//! `fn(...)`-typed host import is vouched for but free to call, exactly
//! like any other safe function. One arm ahead of the type rule names what
//! it is calling when it can — an unsafe builtin — and the type rule's own
//! arm names a directly-called import too, purely so the message can say
//! which; everything else reached through a VALUE is judged by the type
//! alone, which is the only thing that still knows. Taking such a value is
//! free: binding, passing and returning a function run nothing.
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
    /// A call of an `unsafe fn`-typed host import, named directly, outside
    /// any `unsafe { ... }` block. Reached only when the callee's TYPE
    /// already demands the marker — a `fn(...)`-typed import is vouched for
    /// but free to call, so this never fires for one of those.
    ///
    /// The DIRECT call keeps its own variant, beside the nameless
    /// type-driven one below, for exactly one thing: the message can name
    /// the import. Reaching the same import through a binding is the
    /// type's business alone.
    ExternCallOutsideUnsafe { call: ExprId, name: String },
    /// A call THROUGH A VALUE whose type is `unsafe fn(...)`, outside any
    /// `unsafe { ... }` block. The general rule, and the one that closes
    /// the hole none of the three above can see: `let f = read; f(buf, 8)`,
    /// `apply(dealloc_array, p, n)`, a record field holding an import.
    /// Once the function is a value, its TYPE is the only thing that still
    /// knows a marker is owed — so the type is what gets asked.
    ///
    /// Nameless on purpose: the call site genuinely does not know which
    /// function it is about to run, and a message that guessed would be
    /// worse than one that says what is true.
    UnsafeFnValueCallOutsideUnsafe { call: ExprId },
}

impl UnsafeCheckDiagnostic {
    pub fn expr(&self) -> ExprId {
        match self {
            UnsafeCheckDiagnostic::DerefOutsideUnsafe { expr } => *expr,
            UnsafeCheckDiagnostic::BuiltinCallOutsideUnsafe { call, .. }
            | UnsafeCheckDiagnostic::ExternCallOutsideUnsafe { call, .. }
            | UnsafeCheckDiagnostic::UnsafeFnValueCallOutsideUnsafe { call } => *call,
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
            UnsafeCheckDiagnostic::ExternCallOutsideUnsafe { name, .. } => {
                diag::extern_call_requires_unsafe(name)
            }
            UnsafeCheckDiagnostic::UnsafeFnValueCallOutsideUnsafe { .. } => {
                diag::UNSAFE_FN_VALUE_CALL_REQUIRES_UNSAFE.to_owned()
            }
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn unsafe_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<UnsafeCheckDiagnostic> {
    let body = body(db, item);
    let mut ctx = CheckCtx {
        db,
        body,
        resolutions: resolutions(db, item),
        infer: crate::infer::infer(db, item),
        diagnostics: Vec::new(),
    };
    if let Some(root) = body.root {
        ctx.check_expr(root, false);
    }
    ctx.diagnostics
}

struct CheckCtx<'db> {
    /// Consulted for exactly one cross-item question: does this call reach
    /// an `extern static` declaration? ([`crate::is_host_import`].) Asked
    /// only so the message can NAME the import when the type rule already
    /// caught the call — never to decide whether it is owed at all, since a
    /// `fn(...)`-typed import owes nothing.
    db: &'db dyn Db,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    /// Consulted for the three questions name resolution cannot answer: is
    /// this `.*` through a raw pointer or through a safe borrow (safety is
    /// a property of the pointer's FLAVOR, and only inference knows it);
    /// does this dot-call reach a builtin MEMBER (resolved by inference, so
    /// `resolutions` never mentions it); and — the general call rule — is
    /// the thing being called of `unsafe fn` type, which is a fact about a
    /// VALUE and so has no name to resolve at all.
    infer: &'db crate::infer::InferenceResult,
    diagnostics: Vec<UnsafeCheckDiagnostic>,
}

impl CheckCtx<'_> {
    /// Whether `receiver` — the operand of a `.*` — is a RAW pointer.
    ///
    /// A receiver whose type is unknown or broken answers `true`: the
    /// stricter reading, so a program can never lose its `unsafe`
    /// requirement to an inference failure that carries its own
    /// diagnostic. Only a receiver KNOWN to be a safe borrow is exempt.
    fn derefs_a_raw_pointer(&self, receiver: ExprId) -> bool {
        match self.infer.type_of_expr.get(receiver) {
            // A safe borrow: `.*` on it needs no `unsafe` — safety is
            // decided by the pointer's FLAVOR.
            Some(crate::ty::Ty::Borrow { .. }) => false,
            // BROKEN receiver: errors are infectious and SILENT. The
            // default below is deliberately fail-safe ("not known to be a
            // safe borrow, so demand `unsafe`"), which is right for an
            // unresolved type and wrong for an already-diagnosed one —
            // `c.nosuch().*` reported "no field or member" and then
            // accused the user of dereferencing a raw pointer that never
            // existed. Any refusal of the callee in a postfix chain
            // reaches this pair.
            Some(ty) if ty.contains_error() => false,
            _ => true,
        }
    }

    /// Whether `callee` — the thing being called — has a type that demands
    /// the marker: `unsafe fn(...)`.
    ///
    /// An unknown or broken callee answers `false`, the opposite default
    /// from [`Self::derefs_a_raw_pointer`], and the asymmetry is deliberate.
    /// A deref is an unsafe operation until something proves otherwise, so
    /// ignorance must be strict there; a CALL is an ordinary operation and
    /// only a known-unsafe type makes it otherwise, so ignorance here must
    /// be quiet — demanding `unsafe` around every call whose callee failed
    /// to infer would bury the real diagnostic under advice.
    fn calls_an_unsafe_fn_value(&self, callee: ExprId) -> bool {
        matches!(
            self.infer.type_of_expr.get(callee),
            Some(crate::ty::Ty::Fn(f)) if f.unsafe_to_call
        )
    }

    fn check_expr(&mut self, expr: ExprId, in_unsafe: bool) {
        match &self.body.exprs[expr] {
            // A name in VALUE position — `let f = read;`, an argument, a
            // return — is FREE, whatever it names. Binding a function runs
            // nothing; the price is charged where it is called, and the
            // type is what carries the bill there (see the `Call` arm).
            // With nothing left to judge here, a callee needs no special
            // walk either: `check_expr` on it is now a plain recursion.
            ExprData::NameRef(_)
            | ExprData::Missing
            | ExprData::Literal(_)
            // The import DECLARATION itself: naming a host function runs
            // nothing, so it is free exactly as a mention of one is.
            | ExprData::ExternImport
            | ExprData::ElidedVariant { .. } => {}
            // Both lists a path can carry — the owner's turbofish and a
            // second segment's own — hold ordinary const-arg expressions,
            // so both are walked (the reserved one still contains code).
            ExprData::VariantPath {
                args, member_args, ..
            } => {
                for arg in args.iter().flatten().chain(member_args.iter().flatten()) {
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
            ExprData::Call { callee, args, .. } => {
                // The builtins whose `Builtin::requires_unsafe` answers
                // true carry a precondition whose violation is UB, so the
                // CALL needs the marker, exactly like a deref. The callee
                // may be a bare name or a turbofish mention of one.
                let callee_name = match &self.body.exprs[*callee] {
                    ExprData::GenericApp { base, .. } => *base,
                    _ => *callee,
                };
                if !in_unsafe {
                    // A builtin named directly (`copy(p, q, n)`) or reached
                    // through the DOT as a member (`s.len()`): the one
                    // shared lookup, so the marker cannot special-case
                    // members. Both current members are safe, but that is
                    // their property's answer, not something the mechanism
                    // should assume.
                    if let Some(builtin) = self
                        .infer
                        .builtin_of_call(self.resolutions.get(callee_name), expr)
                        && builtin.requires_unsafe()
                    {
                        self.diagnostics
                            .push(UnsafeCheckDiagnostic::BuiltinCallOutsideUnsafe {
                                call: expr,
                                builtin,
                            });
                    } else if self.calls_an_unsafe_fn_value(*callee) {
                        // THE GENERAL RULE: the callee's TYPE says a marker
                        // is owed. Everything the builtin arm above cannot
                        // see arrives here — a bound import, an unsafe
                        // builtin passed as an argument, a record field, a
                        // parameter annotated `unsafe fn(...)`.
                        //
                        // The TYPE decides, never the declaration: an import
                        // may be declared `fn(...)` now (vouched, but safe
                        // to call), and calling one of THOSE costs nothing —
                        // its declarer already vouched, on the declaration,
                        // and reading a clock breaks nothing. Naming the
                        // import is a better message when the call reaches
                        // one directly, so that stays a branch — but it is a
                        // branch INSIDE the type's answer now, not a rule
                        // beside it.
                        let named_import = match self.resolutions.get(callee_name) {
                            Some(Resolution::Item(loc))
                                if crate::is_host_import(self.db, loc.to_id(self.db)) =>
                            {
                                Some(loc.display_name().to_owned())
                            }
                            _ => None,
                        };
                        self.diagnostics.push(match named_import {
                            Some(name) => {
                                UnsafeCheckDiagnostic::ExternCallOutsideUnsafe { call: expr, name }
                            }
                            None => {
                                UnsafeCheckDiagnostic::UnsafeFnValueCallOutsideUnsafe { call: expr }
                            }
                        });
                    }
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
            ExprData::Break { value } | ExprData::Return { value } => {
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
            // Taking an address or a borrow is safe (the hazard is at the
            // deref); the place may still contain a deref of its own, which
            // is judged as one.
            ExprData::AddrOf { place, .. } | ExprData::Borrow { place, .. } => {
                self.check_expr(*place, in_unsafe)
            }
            // THE unsafe operation — but only through a RAW pointer. Safe
            // `.*` on a `T.&`/`T.&mut` needs no marker: the pointer's
            // FLAVOR determines safety, which is already how the language
            // works. That makes this the one place in the pass that needs
            // types; everything else stays structural.
            ExprData::Deref { receiver } => {
                if !in_unsafe && self.derefs_a_raw_pointer(*receiver) {
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
