//! Must-consume checking: the dual of use-after-move.
//!
//! A value whose type lacks the `forget` capability (see
//! [`crate::capability`]) must be consumed EXACTLY ONCE on every path.
//! Use-after-move asks "was this value already given away?"; this asks the
//! same question from the other side — "was it ever?" — and both answers
//! fall out of one walk over the body with one fact per binding: live, or
//! consumed.
//!
//! Why this is cheap here and expensive elsewhere: **there is no unwinding
//! in Must.** A panic traps the program; nothing runs on the way out. So
//! the paths out of a scope are the ones written in the source — falling off
//! the end, `return`, `break`, `continue` — and a checker that visits those
//! has visited all of them. A language with unwinding has an implicit exit
//! edge at every call, which is precisely why such languages reach for
//! destructors instead: there is no source position to write the cleanup at.
//!
//! What COUNTS as consuming is deliberately small, and each entry is a
//! place where the value visibly leaves:
//!
//! - reading the binding as a value — passing it to a call, returning it,
//!   binding it to another name, putting it in a record or an array;
//! - matching an owned scrutinee (the payload bindings inherit the
//!   obligation, arm by arm);
//! - destructuring it (`let String(struct { ptr, cap, .. }) = s;`) — the
//!   bottom of every disposal chain, and the reason no compiler magic is
//!   needed for `drop`: taking a linear apart hands its obligation to the
//!   parts, and a `String`'s parts are a pointer and two integers, which
//!   anyone may forget.
//!
//! Everything else — borrowing it, reading one of its non-linear fields —
//! leaves the obligation where it was.

use base_db::Db;
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::body::{Body, ExprData, ExprId, PatData, PatId, Stmt, body};
use crate::capability::{Affine, has_forget};
use crate::infer::{InferenceResult, infer};
use crate::scopes::{Resolution, resolutions};
use crate::ty::Ty;
use crate::{BindingId, ItemId, diag};

/// Range-free (keyed by HIR ids); the diagnostics layer attaches ranges
/// through the body source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearDiagnostic {
    /// A live linear binding at a scope's exit — the leak. Squiggles the
    /// EXIT (the block, the `return`, the `break`), because that is the
    /// path that fails; the binding is the related note, because that is
    /// where the obligation started.
    NotConsumed { binding: BindingId, exit: ExprId },
    /// A linear binding read after it was already consumed — use-after-move,
    /// which for a linear type is also double-disposal.
    AlreadyConsumed {
        binding: BindingId,
        expr: ExprId,
        first: ExprId,
    },
    /// A linear value produced by a statement whose value is discarded.
    Discarded { expr: ExprId },
    /// Assignment over a linear binding that still holds a live value.
    AssignOverLive { binding: BindingId, target: ExprId },
    /// A join whose branches disagree: consumed on one path, live on
    /// another. Reported at the join, per the statement-boundary rule.
    JoinDisagrees { binding: BindingId, join: ExprId },
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
    /// A match-arm `_` on an OWNED linear scrutinee. The match consumed the
    /// value and the arm bound nothing, so it has nowhere to go — the
    /// match-arm twin of [`Self::RestSkipsLinear`], and of the `let _ =`
    /// case (which is caught as an unnamed binding). `param` is that twin's
    /// too, for the SCRUTINEE's type.
    WildcardSkipsLinear { pat: PatId, param: Option<String> },
    /// A binding's liveness differs between the start and the end of a
    /// loop body — the next iteration would see a different world than the
    /// one the body was checked against.
    LoopChangesLinear { binding: BindingId, at: ExprId },
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
    /// record field, an array element, a `.&mut` referent. The twin of
    /// [`Self::AssignOverLive`] where there is no binding to name and no
    /// liveness to track: the place holds such a value BY TYPE, so the
    /// write always loses one. A raw-pointer deref target is deliberately
    /// exempt; that is `unsafe`'s hatch, the same one raw storage is.
    AssignOverPlace { target: ExprId },
}

impl LinearDiagnostic {
    /// The expression the squiggle lands on.
    pub fn expr(&self) -> Option<ExprId> {
        match self {
            LinearDiagnostic::NotConsumed { exit, .. } => Some(*exit),
            LinearDiagnostic::AlreadyConsumed { expr, .. }
            | LinearDiagnostic::Discarded { expr }
            | LinearDiagnostic::CopiedOut { expr }
            | LinearDiagnostic::Repeated { expr }
            | LinearDiagnostic::ItemHoldsLinear { expr }
            | LinearDiagnostic::ConstBlockHoldsLinear { expr } => Some(*expr),
            LinearDiagnostic::AssignOverLive { target, .. }
            | LinearDiagnostic::AssignOverPlace { target } => Some(*target),
            LinearDiagnostic::JoinDisagrees { join, .. } => Some(*join),
            LinearDiagnostic::WildcardSkipsLinear { .. } => None,
            LinearDiagnostic::LoopChangesLinear { at, .. } => Some(*at),
            LinearDiagnostic::RestSkipsLinear { .. } => None,
        }
    }

    /// The pattern the squiggle lands on, for the one variant that has no
    /// expression to point at.
    pub fn pat(&self) -> Option<PatId> {
        match self {
            LinearDiagnostic::RestSkipsLinear { pat, .. }
            | LinearDiagnostic::WildcardSkipsLinear { pat, .. } => Some(*pat),
            _ => None,
        }
    }

    /// The binding whose birth explains this diagnostic, for the related
    /// note ("`s` is born here and must be consumed").
    pub fn binding(&self) -> Option<BindingId> {
        match self {
            LinearDiagnostic::NotConsumed { binding, .. }
            | LinearDiagnostic::AlreadyConsumed { binding, .. }
            | LinearDiagnostic::AssignOverLive { binding, .. }
            | LinearDiagnostic::JoinDisagrees { binding, .. }
            | LinearDiagnostic::LoopChangesLinear { binding, .. } => Some(*binding),
            _ => None,
        }
    }

    /// Whether this finding is about DUPLICATION (a value read twice, or
    /// copied out of a place) rather than about a value that was lost.
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
            LinearDiagnostic::AlreadyConsumed { .. }
                | LinearDiagnostic::CopiedOut { .. }
                | LinearDiagnostic::Repeated { .. }
                | LinearDiagnostic::LoopChangesLinear { .. }
        )
    }

    /// The expression of an EARLIER event this diagnostic refers back to
    /// (the first consumption of a double-consume).
    pub fn related_expr(&self) -> Option<ExprId> {
        match self {
            LinearDiagnostic::AlreadyConsumed { first, .. } => Some(*first),
            _ => None,
        }
    }

    /// The message, given the binding's name and — when the value's type
    /// has a root the message can name — what that root is.
    ///
    /// EVERY message that explains why a value could not be lost or read
    /// again takes the hint, because inside a generic body all of them are
    /// about the binder: nothing was declared `only move`, so a reader
    /// sent looking for a declaration finds none.
    ///
    /// Which hint, and therefore which advice, is decided by
    /// [`Self::about_duplication`]. The LEAK arms may offer `T: forget`,
    /// because they fire only where an unbounded parameter is the reason
    /// the value had to be consumed, and a parameter is the only root they
    /// can have that a declaration does not explain. The DUPLICATION arms
    /// may not offer the bound: a bounded parameter reaches them too, and
    /// for one of them the bound has already waived consumption — so they
    /// say what is true of every root, which is that nothing grants
    /// copying, and name borrowing. Their third root has no declaration
    /// and no binder either: a value HOLDING an exclusive borrow, refused
    /// a second read because two of them would name one place.
    ///
    /// Two arms ignore the `param` they are handed and use one they
    /// carried from the walk instead. The type they answer for is neither
    /// a binding's nor an expression's — a `..` blames the FIELD it
    /// skipped, a match-arm `_` blames the SCRUTINEE it swallowed — so
    /// only the walk that found them knows which type to blame. The one
    /// arm that takes no hint at all is an ITEM's own value, which is
    /// never inside a generic body's scope.
    pub fn message(&self, name: &str, root: Option<&Affine>) -> String {
        let param = match root {
            Some(Affine::Param(param)) => Some(param.as_str()),
            _ => None,
        };
        match self {
            LinearDiagnostic::NotConsumed { .. } => match param {
                Some(param) => diag::not_consumed_param(name, param),
                None => diag::not_consumed(name),
            },
            LinearDiagnostic::AlreadyConsumed { .. } => root
                .and_then(|root| diag::already_consumed_dup(name, root))
                .unwrap_or_else(|| diag::already_consumed(name)),
            LinearDiagnostic::Discarded { .. } => match param {
                Some(param) => diag::discarded_param(param),
                None => diag::DISCARDED_LINEAR.to_owned(),
            },
            LinearDiagnostic::AssignOverLive { .. } => match param {
                Some(param) => diag::assign_over_live_param(name, param),
                None => diag::assign_over_live(name),
            },
            LinearDiagnostic::JoinDisagrees { .. } => match param {
                Some(param) => diag::join_disagrees_param(name, param),
                None => diag::join_disagrees(name),
            },
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
            LinearDiagnostic::LoopChangesLinear { .. } => root
                .and_then(|root| diag::loop_changes_dup(name, root))
                .unwrap_or_else(|| diag::loop_changes_linear(name)),
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
        state: FxHashMap::default(),
        owed: FxHashMap::default(),
        scopes: Vec::new(),
        loops: Vec::new(),
        error_exprs: inference
            .diagnostics
            .iter()
            .filter(|diag| diag.severity() == crate::Severity::Error)
            .map(|diag| diag.expr())
            .collect(),
        poisoned: rustc_hash::FxHashSet::default(),
        body_floor: 0,
    };
    if let Some(root) = body.root {
        // The item's own initializer is a scope of its own — a const
        // context, and the same rule applies in one (a linear born at
        // compile time must be consumed at compile time). Its VALUE,
        // though, is the item's value, and a `static` is never destroyed.
        ctx.scopes.push(Vec::new());
        let flow = ctx.read_value(root);
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
        ctx.pop_scope(root, flow);
    }
    // The promise `crate::capability` makes: a broken program does not grow
    // must-consume errors on top of the error it already has. Only the
    // "it never happened" family is dropped — the "you did something
    // wrong" family (a double consume, a copy-out) is about something
    // written, and stays.
    let poisoned = std::mem::take(&mut ctx.poisoned);
    ctx.diagnostics.retain(|diag| match diag {
        LinearDiagnostic::NotConsumed { binding, .. }
        | LinearDiagnostic::JoinDisagrees { binding, .. }
        | LinearDiagnostic::LoopChangesLinear { binding, .. } => !poisoned.contains(binding),
        _ => true,
    });
    ctx.diagnostics
}

/// Whether control reaches the end of an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Falls,
    /// `return`, `break`, `continue`, a call that never returns. The
    /// obligations of every scope this jumped out of were checked AT the
    /// jump, so the scopes it unwinds report nothing.
    Diverges,
}

/// What a tracked binding owes. The walk carries two obligations, and they
/// differ by exactly one diagnostic family.
///
/// A LINEAR value owes a consumption on every path: losing it is a leak,
/// and duplicating it is two obligations naming one resource. A value that
/// is merely UNDUPLICABLE owes only the second half — it may be lost, and
/// the walk still refuses a second read of it.
///
/// The second obligation is the use-after-move check the copy rule needs,
/// so it asks the copy rule's own question (`capability::is_copyable`)
/// rather than a narrower one of its own. Two things reach it: a rigid
/// type PARAMETER, bounded or not — `T: forget` waives the consumption and
/// grants no copying (TR11) — and a value HOLDING an exclusive borrow,
/// which is the same statement about the same instantiation, since a
/// bounded `T` may be `usize.&mut`. A body that read one binding of either
/// twice would hand out two exclusive borrows of one place.
///
/// A BARE borrow is the exception, by name rather than by walk, and the
/// reason is M07's. Where a position WANTS a borrow, every mention mints a
/// fresh reborrow, so there are no two loans to refuse; where none does —
/// a bare `let`, a read of an affine field — the read still copies,
/// invisibly to both layers, and M07 rules that closing THAT is a typing
/// change rather than a checker one. So this walk leaves the bare borrow
/// where the ruling put it, and refuses only what HOLDS one, which no
/// reborrow reaches.
///
/// A memo of a fact derivable from the binding's type, kept beside
/// [`CheckCtx::state`] rather than re-derived at each of the half-dozen
/// sites that ask: written once when the binding is tracked, never
/// branch-dependent, and never cloned across the arms of a join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owed {
    /// Exactly once — a linear value.
    Once,
    /// At most once — a value that may not be duplicated, whatever its
    /// disposal story is.
    AtMostOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Live,
    Consumed { at: ExprId },
}

impl Status {
    fn is_live(self) -> bool {
        matches!(self, Status::Live)
    }
}

type State = FxHashMap<BindingId, Status>;

struct LoopFrame {
    /// `scopes.len()` at the loop's entry — `break`/`continue` check every
    /// scope above this.
    floor: usize,
    /// The liveness of every tracked binding at the loop's entry. The body
    /// must leave them exactly as it found them, or the second iteration
    /// runs against a world the first was not checked in.
    entry: FxHashMap<BindingId, bool>,
    /// The state at each `break` — the loop's exit is their join.
    breaks: Vec<State>,
}

struct CheckCtx<'db> {
    db: &'db dyn Db,
    body: &'db Body,
    resolutions: &'db ArenaMap<ExprId, Resolution>,
    infer: &'db InferenceResult,
    diagnostics: Vec<LinearDiagnostic>,
    state: State,
    /// What each tracked binding owes — see [`Owed`].
    owed: FxHashMap<BindingId, Owed>,
    /// One frame per open lexical scope, holding the tracked bindings it
    /// introduced. Popping a frame is where "not consumed" is found.
    scopes: Vec<Vec<BindingId>>,
    loops: Vec<LoopFrame>,
    /// Every expression inference reported an ERROR at. Two uses, both
    /// about not saying the same thing twice or the wrong thing once: a
    /// copy-out inference already refused is not repeated, and a
    /// consumption site inference could not resolve POISONS the value it
    /// would have consumed.
    error_exprs: rustc_hash::FxHashSet<ExprId>,
    /// Bindings whose consumption sites are broken. `s.` half-typed, or
    /// `s.some_unresolved_member()`, is a consumption the user is in the
    /// middle of writing — reporting a leak beside the real error would
    /// accuse them of the opposite of what they are doing.
    poisoned: rustc_hash::FxHashSet<BindingId>,
    /// `scopes.len()` at the current fn body's entry — `return` checks
    /// every scope above this, and no further (an enclosing body's locals
    /// are not this body's to consume; captures do not exist).
    body_floor: usize,
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

    /// What a value of `ty` owes, or `None` if it owes nothing and needs no
    /// tracking. See [`Owed`]: the walk tracks two kinds of value and the
    /// difference between them is exactly one diagnostic family.
    fn owed_of(&self, ty: &Ty) -> Option<Owed> {
        if self.is_linear(ty) {
            Some(Owed::Once)
        } else if matches!(ty, Ty::Borrow { .. }) {
            None
        } else if !crate::capability::is_copyable(self.db, ty) {
            Some(Owed::AtMostOnce)
        } else {
            None
        }
    }

    /// Whether a value of this expression's type may be DUPLICATED by a
    /// plain read — the question a copy-out and an array-repeat ask, and
    /// the one place the two obligations behave alike.
    fn is_unduplicable_expr(&self, expr: ExprId) -> bool {
        self.infer
            .type_of_expr
            .get(expr)
            .and_then(|ty| self.owed_of(ty))
            .is_some()
    }

    /// Whether this binding owes a consumption at the end of its scope, as
    /// opposed to merely owing not to be duplicated.
    fn owes_once(&self, binding: BindingId) -> bool {
        self.owed.get(&binding) == Some(&Owed::Once)
    }

    /// Whether control can leave this expression at all. `!` coerces to
    /// anything and [`InferenceResult::type_of_expr`] records the type the
    /// POSITION accepted, so a diverging expression checked against a real
    /// expectation — a body's tail against the return type, an annotated
    /// `let` — is filed under that expectation and not under `!`. A
    /// callee's signature is never coerced, so a call asks that instead.
    fn diverges(&self, expr: ExprId) -> bool {
        if matches!(self.infer.type_of_expr.get(expr), Some(Ty::Never)) {
            return true;
        }
        match &self.body.exprs[expr] {
            ExprData::Call { callee, .. } => matches!(
                self.infer.type_of_expr.get(*callee),
                Some(Ty::Fn(signature)) if matches!(signature.ret, Ty::Never)
            ),
            _ => false,
        }
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

    // ---- scopes and bindings -------------------------------------------

    fn track_pat(&mut self, pat: PatId) {
        for (_, binding) in self.body.pat_bindings(pat) {
            let owed = self
                .infer
                .type_of_binding
                .get(binding)
                .and_then(|ty| self.owed_of(ty));
            if let Some(owed) = owed {
                self.state.insert(binding, Status::Live);
                self.owed.insert(binding, owed);
                if let Some(frame) = self.scopes.last_mut() {
                    frame.push(binding);
                }
            }
        }
        self.check_rest_patterns(pat);
    }

    /// A `..` that would skip a linear field. The rest marker means "don't
    /// bind the others", which for a forgettable field is exactly right and
    /// for a linear one is a leak with no name to report it under — so it is
    /// refused at the pattern instead.
    fn check_rest_patterns(&mut self, pat: PatId) {
        match &self.body.pats[pat] {
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
                self.check_rest_patterns(inner);
            }
            _ => {}
        }
    }

    fn pop_scope(&mut self, exit: ExprId, flow: Flow) {
        let frame = self.scopes.pop().unwrap_or_default();
        for binding in frame {
            if flow == Flow::Falls
                && self.owes_once(binding)
                && self.state.get(&binding).is_some_and(|s| s.is_live())
            {
                self.diagnostics
                    .push(LinearDiagnostic::NotConsumed { binding, exit });
            }
            self.state.remove(&binding);
        }
    }

    /// Every live binding in the scopes above `floor` — the check a jump
    /// out of them performs. Nothing is mutated: the path ends here.
    fn report_live_above(&mut self, floor: usize, exit: ExprId) {
        let live: Vec<BindingId> = self.scopes[floor.min(self.scopes.len())..]
            .iter()
            .flatten()
            .copied()
            .filter(|b| self.owes_once(*b) && self.state.get(b).is_some_and(|s| s.is_live()))
            .collect();
        for binding in live {
            self.diagnostics
                .push(LinearDiagnostic::NotConsumed { binding, exit });
        }
    }

    /// Whether inference gave up on this expression — it reported an error
    /// AT it, or typed it `{error}`. The silent case matters as much as the
    /// loud one: a dot-call on a member whose own definition is broken is
    /// deliberately not re-diagnosed at the call (the definition carries
    /// it), and that silence is exactly the shape a poisoned consumption
    /// site has.
    fn is_broken(&self, expr: ExprId) -> bool {
        self.error_exprs.contains(&expr)
            || matches!(self.infer.type_of_expr.get(expr), Some(Ty::Error))
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

    /// Mark the binding at the root of a place chain as one whose
    /// consumption sites are broken.
    fn poison_root(&mut self, expr: ExprId) {
        let mut root = expr;
        loop {
            match &self.body.exprs[root] {
                ExprData::Field { receiver, .. } | ExprData::Deref { receiver } => {
                    root = *receiver;
                }
                ExprData::Index { base, .. } => root = *base,
                _ => break,
            }
        }
        if let Some(Resolution::Local(binding)) = self.resolutions.get(root).cloned() {
            self.poisoned.insert(binding);
        }
    }

    fn consume(&mut self, binding: BindingId, at: ExprId) {
        match self.state.get(&binding).copied() {
            Some(Status::Live) => {
                self.state.insert(binding, Status::Consumed { at });
            }
            Some(Status::Consumed { at: first }) => {
                self.diagnostics.push(LinearDiagnostic::AlreadyConsumed {
                    binding,
                    expr: at,
                    first,
                });
            }
            None => {}
        }
    }

    // ---- the walk -------------------------------------------------------

    /// `expr`'s value is produced and used by whoever asked. This is where
    /// consumption happens.
    fn read_value(&mut self, expr: ExprId) -> Flow {
        let flow = self.read_value_inner(expr);
        if flow == Flow::Falls && self.diverges(expr) {
            return Flow::Diverges;
        }
        flow
    }

    fn read_value_inner(&mut self, expr: ExprId) -> Flow {
        match &self.body.exprs[expr] {
            ExprData::Missing
            | ExprData::Literal(_)
            // A signature and nothing else: no value is produced from a
            // linear one and none is consumed.
            | ExprData::ExternImport
            | ExprData::ElidedVariant { .. }
            | ExprData::VariantPath { .. }
            | ExprData::GenericApp { .. } => Flow::Falls,
            ExprData::NameRef(_) => {
                if let Some(Resolution::Local(binding)) = self.resolutions.get(expr).cloned() {
                    self.consume(binding, expr);
                }
                Flow::Falls
            }
            ExprData::Field { receiver, name, .. } => {
                let (receiver, broken_name) = (*receiver, name.is_empty());
                // `s.` — a consumption the user is in the middle of typing.
                // The parse error covers it; a leak reported beside it
                // would say the opposite of what is happening.
                if broken_name {
                    self.poison_root(receiver);
                }
                let flow = self.walk_place(receiver);
                if flow == Flow::Diverges {
                    return flow;
                }
                // Reading a linear FIELD out of a place would copy it, and
                // the place keeps its own copy: two obligations, one
                // resource. Taking the whole value apart is the way in.
                if self.is_unduplicable_expr(expr) {
                    self.report_copied_out(expr);
                }
                Flow::Falls
            }
            ExprData::Index { base, index } => {
                let (base, index) = (*base, *index);
                if self.walk_place(base) == Flow::Diverges {
                    return Flow::Diverges;
                }
                if self.read_value(index) == Flow::Diverges {
                    return Flow::Diverges;
                }
                if self.is_unduplicable_expr(expr) {
                    self.report_copied_out(expr);
                }
                Flow::Falls
            }
            ExprData::Call {
                callee,
                args,
                dot_call,
            } => {
                let (callee, args, dot_call) = (*callee, args.clone(), *dot_call);
                // A dot-call that resolved to a MEMBER passes its receiver
                // as an argument — last, for a user member and a builtin
                // one alike (TR01) — so the receiver is a value read, not a
                // projection. Where in the argument list it lands decides
                // nothing here; that it is READ decides everything.
                //
                // No auto-borrow exists, so the receiver's own type is
                // exactly what is passed: `s.drop()` hands over the
                // `String`, `m.next_line()` hands over the borrow `m`
                // already is. The consumption question needs no signature.
                let member = dot_call && self.is_member_call(expr);
                // A dot-call inference could not resolve is a consumption
                // that did not happen because the CALL did not: the
                // receiver's obligation is news to break only after the
                // real error is fixed.
                if dot_call
                    && !member
                    && (self.is_broken(expr) || self.is_broken(callee))
                    && let ExprData::Field { receiver, .. } = &self.body.exprs[callee]
                {
                    let receiver = *receiver;
                    self.poison_root(receiver);
                }
                let receiver = if member {
                    match &self.body.exprs[callee] {
                        ExprData::Field { receiver, .. } => Some(*receiver),
                        _ => None,
                    }
                } else {
                    None
                };
                match receiver {
                    Some(receiver) => {
                        for arg in args {
                            if self.read_value(arg) == Flow::Diverges {
                                return Flow::Diverges;
                            }
                        }
                        if self.read_value(receiver) == Flow::Diverges {
                            return Flow::Diverges;
                        }
                    }
                    None => {
                        if self.read_value(callee) == Flow::Diverges {
                            return Flow::Diverges;
                        }
                        for arg in args {
                            if self.read_value(arg) == Flow::Diverges {
                                return Flow::Diverges;
                            }
                        }
                    }
                }
                Flow::Falls
            }
            ExprData::Bin { lhs, rhs, .. } => {
                let (lhs, rhs) = (*lhs, *rhs);
                if self.read_value(lhs) == Flow::Diverges {
                    return Flow::Diverges;
                }
                self.read_value(rhs)
            }
            ExprData::Neg { operand } => {
                let operand = *operand;
                self.read_value(operand)
            }
            ExprData::AddrOf { place, .. } | ExprData::Borrow { place, .. } => {
                let place = *place;
                self.walk_place(place)
            }
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                // A copy out of a borrow of a linear is already inference's
                // "cannot move out of a borrow" (linear implies
                // non-copyable); nothing to add here.
                self.read_value(receiver)
            }
            ExprData::RecordLit { fields } => {
                let values: Vec<ExprId> = fields.iter().map(|f| f.value).collect();
                for value in values {
                    if self.read_value(value) == Flow::Diverges {
                        return Flow::Diverges;
                    }
                }
                Flow::Falls
            }
            ExprData::ArrayLit { elements } => {
                let elements = elements.clone();
                for element in elements {
                    if self.read_value(element) == Flow::Diverges {
                        return Flow::Diverges;
                    }
                }
                Flow::Falls
            }
            ExprData::ArrayRepeat { element, count } => {
                let (element, count) = (*element, *count);
                if self.read_value(element) == Flow::Diverges {
                    return Flow::Diverges;
                }
                if self.is_unduplicable_expr(element) {
                    self.diagnostics
                        .push(LinearDiagnostic::Repeated { expr: element });
                }
                self.read_value(count)
            }
            ExprData::ConstBlock { body } => {
                let body = *body;
                let flow = self.read_value(body);
                // The item-level rule one nesting level down: a const
                // block's value is a CONSTANT, copied into every
                // evaluation, so a linear one is as many obligations as
                // there are evaluations and no path discharges it once.
                // The question is the block's TYPE, not a path through it
                // — the same question its item-level twin asks above — so
                // a diverging body is no excuse: `!` is forgettable, and a
                // position that pinned the block to a linear type anyway
                // is the mistake being reported.
                if self.is_linear_expr(expr) {
                    self.diagnostics
                        .push(LinearDiagnostic::ConstBlockHoldsLinear { expr });
                }
                flow
            }
            ExprData::Unsafe { body } => {
                let body = *body;
                self.read_value(body)
            }
            ExprData::Block { .. } => self.check_block(expr),
            ExprData::If { .. } => self.check_if(expr),
            ExprData::Match { .. } => self.check_match(expr),
            ExprData::Loop { .. } => self.check_loop(expr),
            ExprData::Break { value } => {
                let value = *value;
                if let Some(value) = value
                    && self.read_value(value) == Flow::Diverges
                {
                    return Flow::Diverges;
                }
                let floor = self.loops.last().map(|frame| frame.floor);
                if let Some(floor) = floor {
                    self.report_live_above(floor, expr);
                    // A break is NOT a back edge: it leaves the loop, so the
                    // invariant it would have to restore is nobody's. Its
                    // state travels to the loop's exit join instead, which
                    // is why taking ownership of something and breaking out
                    // with it is the ordinary search-loop shape and checks
                    // clean.
                    let snapshot = self.state.clone();
                    if let Some(frame) = self.loops.last_mut() {
                        frame.breaks.push(snapshot);
                    }
                }
                Flow::Diverges
            }
            ExprData::Continue => {
                let floor = self.loops.last().map(|frame| frame.floor);
                if let Some(floor) = floor {
                    self.report_live_above(floor, expr);
                    self.check_loop_invariant(expr);
                }
                Flow::Diverges
            }
            ExprData::Return { value } => {
                let value = *value;
                if let Some(value) = value
                    && self.read_value(value) == Flow::Diverges
                {
                    return Flow::Diverges;
                }
                let floor = self.body_floor;
                self.report_live_above(floor, expr);
                Flow::Diverges
            }
            ExprData::FnLiteral { params, body, .. } => {
                let (params, fn_body) = (params.clone(), *body);
                // A nested body is checked in isolation: captures do not
                // exist (MIR refuses them), so no obligation crosses the
                // boundary in either direction.
                let saved_state = std::mem::take(&mut self.state);
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_loops = std::mem::take(&mut self.loops);
                let saved_floor = self.body_floor;
                self.body_floor = 0;
                self.scopes.push(Vec::new());
                for param in &params {
                    self.track_pat(param.pat);
                }
                let flow = self.read_value(fn_body);
                self.pop_scope(fn_body, flow);
                self.state = saved_state;
                self.scopes = saved_scopes;
                self.loops = saved_loops;
                self.body_floor = saved_floor;
                Flow::Falls
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
    /// borrow, the target of an assignment, the receiver of a field access.
    /// Nothing here consumes.
    fn walk_place(&mut self, expr: ExprId) -> Flow {
        match &self.body.exprs[expr] {
            ExprData::NameRef(_) => Flow::Falls,
            ExprData::Field { receiver, .. } => {
                let receiver = *receiver;
                self.walk_place(receiver)
            }
            ExprData::Index { base, index } => {
                let (base, index) = (*base, *index);
                if self.walk_place(base) == Flow::Diverges {
                    return Flow::Diverges;
                }
                self.read_value(index)
            }
            ExprData::Deref { receiver } => {
                let receiver = *receiver;
                self.read_value(receiver)
            }
            // Not a place at all: a temporary. Its value is produced, and
            // then only projected from — so if it is linear it is lost
            // right here, with nothing left holding it.
            _ => {
                let flow = self.read_value(expr);
                if flow == Flow::Falls && self.is_linear_expr(expr) {
                    self.diagnostics.push(LinearDiagnostic::Discarded { expr });
                }
                flow
            }
        }
    }

    fn check_block(&mut self, expr: ExprId) -> Flow {
        let ExprData::Block { stmts, tail } = &self.body.exprs[expr] else {
            return Flow::Falls;
        };
        let (stmts, tail) = (stmts.clone(), *tail);
        self.scopes.push(Vec::new());
        let mut flow = Flow::Falls;
        for stmt in stmts {
            if flow == Flow::Diverges {
                break;
            }
            flow = self.check_stmt(&stmt);
        }
        if flow == Flow::Falls
            && let Some(tail) = tail
        {
            flow = self.read_value(tail);
        }
        self.pop_scope(expr, flow);
        flow
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Flow {
        match stmt {
            Stmt::Let { pat, init, .. } => {
                let (pat, init) = (*pat, *init);
                let flow = self.read_value(init);
                if flow == Flow::Diverges {
                    return flow;
                }
                self.track_pat(pat);
                Flow::Falls
            }
            Stmt::Assign { target, value } => {
                let (target, value) = (*target, *value);
                let flow = self.read_value(value);
                if flow == Flow::Diverges {
                    return flow;
                }
                if self.walk_place(target) == Flow::Diverges {
                    return Flow::Diverges;
                }
                if let Some(Resolution::Local(binding)) = self.resolutions.get(target).cloned() {
                    // Writing over a live linear loses it: the old value is
                    // gone and nothing was done about it. The new value
                    // starts a fresh obligation.
                    if self.owes_once(binding)
                        && self.state.get(&binding).is_some_and(|s| s.is_live())
                    {
                        self.diagnostics
                            .push(LinearDiagnostic::AssignOverLive { binding, target });
                    }
                    if self.state.contains_key(&binding) {
                        self.state.insert(binding, Status::Live);
                    }
                } else if self.is_linear_expr(target) && !self.writes_through_raw_pointer(target) {
                    // A field, an element, a `.&mut` referent: there is no
                    // binding whose liveness could say otherwise, and the
                    // place holds such a value by TYPE, so the write always
                    // loses one.
                    self.diagnostics
                        .push(LinearDiagnostic::AssignOverPlace { target });
                }
                Flow::Falls
            }
            Stmt::Expr(expr) => {
                let expr = *expr;
                let flow = self.read_value(expr);
                if flow == Flow::Falls && self.is_linear_expr(expr) {
                    self.diagnostics.push(LinearDiagnostic::Discarded { expr });
                }
                flow
            }
        }
    }

    fn check_if(&mut self, expr: ExprId) -> Flow {
        let ExprData::If {
            condition,
            then_branch,
            else_branch,
        } = &self.body.exprs[expr]
        else {
            return Flow::Falls;
        };
        let (condition, then_branch, else_branch) = (*condition, *then_branch, *else_branch);
        if self.read_value(condition) == Flow::Diverges {
            return Flow::Diverges;
        }
        let entry = self.state.clone();
        let then_flow = self.read_value(then_branch);
        let then_state = std::mem::replace(&mut self.state, entry.clone());
        let (else_flow, else_state) = match else_branch {
            Some(branch) => {
                let flow = self.read_value(branch);
                (flow, std::mem::replace(&mut self.state, entry.clone()))
            }
            // No `else`: the missing branch falls through having done
            // nothing, which is exactly the state at the join.
            None => (Flow::Falls, entry.clone()),
        };
        self.join(expr, vec![(then_flow, then_state), (else_flow, else_state)])
    }

    fn check_match(&mut self, expr: ExprId) -> Flow {
        let ExprData::Match { scrutinee, arms } = &self.body.exprs[expr] else {
            return Flow::Falls;
        };
        let (scrutinee, arms) = (*scrutinee, arms.clone());
        // One read decides everything: an OWNED linear scrutinee is
        // consumed by the match (its payloads take over, arm by arm), a
        // borrowed one is not (a borrow is forgettable, so reading it
        // consumes nothing). The scrutinee decides — the same rule
        // match-projection already runs on.
        if self.read_value(scrutinee) == Flow::Diverges {
            return Flow::Diverges;
        }
        // A match on an OWNED linear consumed it, so every arm has to name
        // it. `_` names nothing — the match-arm twin of a `..` that skips a
        // linear field, and it has to be refused for the same reason: the
        // value was consumed by the match and the arm gave it nowhere to
        // go. (Through a BORROW nothing was consumed, so nothing is owed.)
        let owned_linear = self.is_linear_expr(scrutinee);
        let entry = self.state.clone();
        let mut branches = Vec::new();
        for arm in arms {
            self.state = entry.clone();
            self.scopes.push(Vec::new());
            if owned_linear && matches!(self.body.pats[arm.pat], PatData::Wildcard) {
                self.diagnostics
                    .push(LinearDiagnostic::WildcardSkipsLinear {
                        pat: arm.pat,
                        param: self
                            .infer
                            .type_of_expr
                            .get(scrutinee)
                            .and_then(|ty| crate::capability::blaming_param(self.db, ty)),
                    });
            }
            self.track_pat(arm.pat);
            let flow = self.read_value(arm.body);
            self.pop_scope(arm.body, flow);
            branches.push((flow, std::mem::replace(&mut self.state, entry.clone())));
        }
        if branches.is_empty() {
            return Flow::Falls;
        }
        self.join(expr, branches)
    }

    fn check_loop(&mut self, expr: ExprId) -> Flow {
        let ExprData::Loop { body } = &self.body.exprs[expr] else {
            return Flow::Falls;
        };
        let body = *body;
        let entry = self
            .state
            .iter()
            .map(|(binding, status)| (*binding, status.is_live()))
            .collect();
        self.loops.push(LoopFrame {
            floor: self.scopes.len(),
            entry,
            breaks: Vec::new(),
        });
        let flow = self.read_value(body);
        // The back edge: falling off the body's end runs the body again.
        if flow == Flow::Falls {
            self.check_loop_invariant(body);
        }
        let frame = self.loops.pop().expect("loop frame");
        if frame.breaks.is_empty() {
            // No way out: the loop diverges, and the state after it is
            // unreachable.
            return Flow::Diverges;
        }
        let branches = frame
            .breaks
            .into_iter()
            .map(|state| (Flow::Falls, state))
            .collect();
        self.join(expr, branches)
    }

    /// The loop's one invariant: every binding the loop could see at entry
    /// must be in the same state when the body runs again. Consuming an
    /// outer linear inside a loop is the case this catches — the second
    /// iteration would consume it again — and assigning a fresh value back
    /// is the way to satisfy it.
    fn check_loop_invariant(&mut self, at: ExprId) {
        let Some(frame) = self.loops.last() else {
            return;
        };
        let changed: Vec<BindingId> = frame
            .entry
            .iter()
            .filter(|(binding, was_live)| {
                self.state
                    .get(binding)
                    .is_some_and(|status| status.is_live() != **was_live)
            })
            .map(|(binding, _)| *binding)
            .collect();
        for binding in changed {
            self.diagnostics
                .push(LinearDiagnostic::LoopChangesLinear { binding, at });
        }
    }

    /// Resolve a set of branch outcomes into one state. Only branches that
    /// FALL contribute — a branch that returned or broke has already
    /// answered for its obligations at the jump. Where the survivors
    /// disagree, the join is the error (joins resolve at statement
    /// boundaries, so this is the boundary that owns it).
    fn join(&mut self, at: ExprId, branches: Vec<(Flow, State)>) -> Flow {
        let surviving: Vec<State> = branches
            .into_iter()
            .filter(|(flow, _)| *flow == Flow::Falls)
            .map(|(_, state)| state)
            .collect();
        let Some(first) = surviving.first() else {
            return Flow::Diverges;
        };
        let mut merged = first.clone();
        for other in &surviving[1..] {
            let mut disagreed = Vec::new();
            for (binding, status) in &mut merged {
                let Some(theirs) = other.get(binding) else {
                    continue;
                };
                if theirs.is_live() != status.is_live() {
                    disagreed.push(*binding);
                    // Settle on "consumed" so the scope exit does not pile
                    // a second complaint on top of this one.
                    *status = match theirs {
                        Status::Consumed { at } => Status::Consumed { at: *at },
                        Status::Live => Status::Consumed { at },
                    };
                }
            }
            // The merge above settles a disagreement on "consumed" for
            // BOTH kinds — a later read of it must be refused either way —
            // but only a value that owes a consumption has been wronged by
            // the disagreement itself.
            for binding in disagreed {
                if self.owes_once(binding) {
                    self.diagnostics
                        .push(LinearDiagnostic::JoinDisagrees { binding, join: at });
                }
            }
        }
        self.state = merged;
        Flow::Falls
    }
}
