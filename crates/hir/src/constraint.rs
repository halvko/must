//! Deferred constraints, provenance, and blame attribution.
//!
//! Inference distinguishes two strengths of type information:
//!
//! - **Axioms**: types the user wrote or usage demands — annotations, a
//!   call's parameter types. Axioms are trusted, so they unify eagerly
//!   during traversal, and when a conflict involves one, the other side
//!   carries the blame.
//! - **Conclusions**: types inference derives from expressions — literals,
//!   branch results. Conclusions are evidence, not truth. Where several
//!   conclusions must agree on one type (a *join*: `if`/`else` branches
//!   today; `match` arms, collection elements later), unification is
//!   deferred so axioms that arrive later in the traversal (an annotation
//!   above, a call below) can pick the winner before the conclusions are
//!   played against each other.
//!
//! Whenever unification binds a type variable to a concrete type, the
//! [`Cause`] is recorded. Blame attribution reads these records back instead
//! of threading "why was this expected" state through the traversal: when a
//! deferred join finds a disagreeing witness, the causes on the join's
//! result variable say why the winning type won, and the sibling witnesses
//! that match it say who voted for it.

use ena::unify::InPlaceUnificationTable;
use rustc_hash::FxHashMap;

use crate::body::{BindingId, ExprId};
use crate::infer::InferenceDiagnostic;
use crate::ty::{Ty, TyVar, TyVarValue};

/// Why a type was required or concluded. Attached to
/// [`InferenceDiagnostic::TypeMismatch`] to render "because of this" hints;
/// each variant has a dedicated arm in the diagnostic renderer so future
/// causes can't accidentally re-use existing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Axiom: a `let` binding's type annotation established the type.
    Binding(BindingId),
    /// Axiom: the enclosing item's written type annotation.
    ItemAnnotation,
    /// Axiom: a function literal's written return type. Carries the
    /// function literal expression; the renderer digs out the return-type
    /// node.
    ReturnAnnotation(ExprId),
    /// Axiom: a call requires the type for one of its arguments.
    CallSite {
        /// The call expression (hinted at its callee).
        call: ExprId,
        /// The argument the requirement applies to.
        arg: ExprId,
    },
    /// Axiom: a type-constructor call (`Foo(...)`) requires its argument to
    /// have the declared underlying record type. Carries the call
    /// expression; the renderer digs the mismatching field's *declaration*
    /// out of the `type` item and points there.
    Constructor(ExprId),
    /// Axiom: an arithmetic/comparison operator requires its operand type.
    /// Carries the binary expression; the renderer points at the operator
    /// token.
    Operator(ExprId),
    /// Axiom: an `if` requires its condition to be `bool`. Carries the `if`
    /// expression; the renderer points at the `if` keyword.
    Condition(ExprId),
    /// Axiom: an `if` without an `else` produces `()` when the condition is
    /// false, so the then-branch must too. Carries the `if` expression.
    MissingElse(ExprId),
    /// Conclusion: this sibling branch of a join produced the type.
    Branch(ExprId),
    /// Conclusion: in an equality, the first operand's type is what the
    /// other side must match.
    Operand(ExprId),
}

impl Cause {
    /// Axioms (annotations, usage requirements) are ground truth: they are
    /// never blamed, and when one decided a type it is the *whole* cause —
    /// conclusions that happen to agree are coincidence, not evidence.
    pub fn is_axiom(self) -> bool {
        !matches!(self, Cause::Branch(_) | Cause::Operand(_))
    }
}

/// One expression flowing into a [`Join`].
#[derive(Debug)]
pub(crate) struct Witness {
    /// The tail sub-expression that produced the type (not the whole branch
    /// block): squiggles and "this branch has type …" hints point here, at
    /// the code someone would actually change.
    pub blame: ExprId,
    pub ty: Ty,
}

/// A deferred agreement constraint: every witness must have the type of
/// `result`. Emitted for `if`/`else`; any future joining construct is the
/// same shape.
#[derive(Debug)]
pub(crate) struct Join {
    /// The joining construct as a whole (the `if` expression). Blamed when
    /// the witnesses agree with each other but contradict an axiom.
    pub expr: ExprId,
    /// The function literal this join sits in (`None` for joins directly in
    /// the item's initializer). A function is a unit: it must be internally
    /// consistent on its own, so flattened voting and delegation stop at
    /// this boundary — from outside, the whole function is one witness.
    pub scope: Option<ExprId>,
    /// Function-literal nesting depth of `scope`. Inner functions solve
    /// before their enclosing scope, so a function's type is settled
    /// internally before any outer join consumes it.
    pub depth: usize,
    /// A fresh variable standing for the join's type. Axioms reaching it
    /// during traversal decide the expected type before the witnesses are
    /// judged against each other.
    pub result: Ty,
    pub witnesses: Vec<Witness>,
}

/// Unification plus the provenance and deferred-join state accumulated over
/// one inference pass. Owned by `InferCtx`; [`Constraints::solve`] runs after
/// traversal, in both the per-item query and group inference (group mode
/// discards the diagnostics — the per-item query re-derives them — but the
/// unifications are what make join-typed signatures come out concrete).
#[derive(Default)]
pub(crate) struct Constraints {
    joins: Vec<Join>,
    /// Canonical root var → the causes that decided its concrete type.
    causes: FxHashMap<TyVar, Vec<Cause>>,
}

impl Constraints {
    pub(crate) fn push_join(&mut self, join: Join) {
        self.joins.push(join);
    }

    /// The one unification entry point. Binding a variable to a concrete
    /// type records `cause` (first cause wins) for later blame attribution.
    pub(crate) fn unify(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: &Ty,
        b: &Ty,
        cause: Option<Cause>,
    ) -> bool {
        let a = resolve_shallow(table, a);
        let b = resolve_shallow(table, b);
        match (a, b) {
            (Ty::Infer(v1), Ty::Infer(v2)) => {
                if table.unioned(v1, v2) {
                    return true;
                }
                // The union picks one root; carry recorded causes over to it.
                let carried: Vec<Cause> = self
                    .causes
                    .remove(&table.find(v1))
                    .into_iter()
                    .chain(self.causes.remove(&table.find(v2)))
                    .flatten()
                    .collect();
                table.union(v1, v2);
                if !carried.is_empty() {
                    self.causes.insert(table.find(v1), carried);
                }
                true
            }
            (Ty::Infer(var), ty) | (ty, Ty::Infer(var)) => {
                if occurs(table, var, &ty) {
                    return false;
                }
                table.union_value(var, TyVarValue::Known(ty));
                if let Some(cause) = cause {
                    self.causes
                        .entry(table.find(var))
                        .or_insert_with(|| vec![cause]);
                }
                true
            }
            // Errors are infectious and silent.
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Unit, Ty::Unit)
            | (Ty::Never, Ty::Never)
            | (Ty::Int, Ty::Int)
            | (Ty::Str, Ty::Str)
            | (Ty::Bool, Ty::Bool) => true,
            (Ty::Fn(f1), Ty::Fn(f2)) => {
                f1.params.len() == f2.params.len() && {
                    let params_ok = f1
                        .params
                        .iter()
                        .zip(&f2.params)
                        .all(|(p1, p2)| self.unify(table, p1, p2, cause));
                    params_ok && self.unify(table, &f1.ret, &f2.ret, cause)
                }
            }
            // Nominal: same declaration or nothing. A `Named` never unifies
            // with its own underlying record either (no implicit
            // nominal↔structural coercion) — that case falls through to the
            // catch-all `false` below.
            (Ty::Named(a), Ty::Named(b)) => a == b,
            // Structural, exact field-set equality: same names (both sides
            // are canonically sorted, so zipping compares the sets), then
            // the field types unify pairwise. No subtyping.
            (Ty::Record(r1), Ty::Record(r2)) => {
                r1.fields.len() == r2.fields.len()
                    && r1
                        .fields
                        .iter()
                        .zip(&r2.fields)
                        .all(|((n1, t1), (n2, t2))| n1 == n2 && self.unify(table, t1, t2, cause))
            }
            _ => false,
        }
    }

    /// Solve all deferred joins, emitting blame-attributed diagnostics.
    ///
    /// Inner functions solve before their enclosing scope: a function must
    /// be internally consistent on its own, so its joins settle among
    /// themselves before any outer join consumes the function's type — an
    /// internal inconsistency is never resolved by how the function is
    /// used. Within one scope, joins are processed outermost-first (reverse
    /// push order: a nested `if` is pushed while its parent's branch is
    /// being inferred); when an outer join decides its type and a
    /// disagreeing witness is itself an unsolved join, the expectation —
    /// and the causes explaining it — is delegated inward, so blame lands
    /// on the innermost disagreeing expression.
    pub(crate) fn solve(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
    ) -> Vec<InferenceDiagnostic> {
        let joins = std::mem::take(&mut self.joins);
        let mut order: Vec<usize> = (0..joins.len()).collect();
        order.sort_by_key(|&i| (std::cmp::Reverse(joins[i].depth), std::cmp::Reverse(i)));
        let mut solved = vec![false; joins.len()];
        let mut diagnostics = Vec::new();
        for i in order {
            solved[i] = true;
            self.solve_join(&joins[i], &joins, &solved, table, &mut diagnostics);
        }
        diagnostics
    }

    fn solve_join(
        &mut self,
        join: &Join,
        joins: &[Join],
        solved: &[bool],
        table: &mut InPlaceUnificationTable<TyVar>,
        diagnostics: &mut Vec<InferenceDiagnostic>,
    ) {
        // The type every witness must have, and the causes explaining why.
        let (expected, expected_causes) = match resolve_fully(table, &join.result) {
            // Errors are infectious and silent.
            Ty::Error => {
                for witness in &join.witnesses {
                    self.unify(table, &witness.ty, &Ty::Error, None);
                }
                return;
            }
            // No axiom constrained the result: the branches vote. The vote
            // is flattened — every leaf branch of nested joins in the same
            // function counts individually, so a plurality of leaves wins
            // regardless of the nesting shape they arrive in.
            Ty::Infer(_) => {
                let mut leaves = Vec::new();
                flatten_leaves(join, joins, solved, table, &mut leaves);
                let mut tally: Vec<(Ty, usize)> = Vec::new();
                for (_, ty) in &leaves {
                    if matches!(ty, Ty::Infer(_) | Ty::Error) {
                        continue;
                    }
                    match tally.iter_mut().find(|(t, _)| t == ty) {
                        Some((_, n)) => *n += 1,
                        None => tally.push((ty.clone(), 1)),
                    }
                }
                let Some(max) = tally.iter().map(|&(_, n)| n).max() else {
                    // Every leaf is still free: tie them together and let
                    // downstream axioms (or a caller) decide.
                    for witness in &join.witnesses {
                        self.unify(table, &join.result, &witness.ty, None);
                    }
                    return;
                };
                if tally.iter().filter(|&&(_, n)| n == max).count() > 1 {
                    // A tie between honest conclusions: neither side is
                    // wrong, so report the disagreement itself and recover
                    // with the first witness so downstream code still checks.
                    push_tie_mismatch(&leaves, diagnostics);
                    self.unify(table, &join.result, &join.witnesses[0].ty, None);
                    return;
                }
                let winner = tally.iter().find(|&&(_, n)| n == max).unwrap().0.clone();
                // The winning leaves are the causes: "this branch has type
                // …" hints, wherever in the nesting those leaves sit.
                let voters = leaves
                    .iter()
                    .filter(|(_, ty)| *ty == winner)
                    .map(|&(blame, _)| Cause::Branch(blame))
                    .collect();
                (winner, voters)
            }
            concrete => {
                let causes = match &join.result {
                    Ty::Infer(var) => self
                        .causes
                        .get(&table.find(*var))
                        .cloned()
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                (concrete, causes)
            }
        };

        // Witnesses that agree become "this branch has type …" hints; free
        // variables adopt the type; unsolved nested joins get the
        // expectation delegated inward; the rest are culprits.
        let mut siblings: Vec<Cause> = Vec::new();
        let mut delegated: Vec<TyVar> = Vec::new();
        let mut culprits: Vec<(&Witness, Ty)> = Vec::new();
        for witness in &join.witnesses {
            match resolve_shallow(table, &witness.ty) {
                Ty::Error => {}
                Ty::Infer(var) => {
                    if unsolved_nested(join, joins, solved, table, var).is_some() {
                        delegated.push(table.find(var));
                    }
                    self.unify(table, &witness.ty, &expected, None);
                }
                actual => {
                    if self.unify(table, &witness.ty, &expected, None) {
                        // Hint on the tail sub-expression that produced the
                        // type, not the whole branch.
                        siblings.push(Cause::Branch(witness.blame));
                    } else {
                        culprits.push((witness, actual));
                    }
                }
            }
        }

        let reasons = compose_reasons(&expected_causes, &siblings);
        for &root in &delegated {
            self.causes.entry(root).or_insert_with(|| reasons.clone());
        }

        if !culprits.is_empty() {
            // Unanimous means *every* witness is a culprit of the same type
            // — a delegated nested join counts as a dissenter, since blame
            // continues inside it. A vote can't end up here unanimously (its
            // winner comes from a witness), so a unanimous conflict always
            // has an axiom behind `expected` — though not necessarily a
            // recorded cause yet.
            let unanimous = siblings.is_empty()
                && delegated.is_empty()
                && culprits.iter().all(|(_, ty)| *ty == culprits[0].1);
            if unanimous {
                // The branches agree with each other and only contradict the
                // context: one diagnostic on the whole construct, not a
                // squiggle per branch.
                diagnostics.push(InferenceDiagnostic::AllBranchesMismatch {
                    expr: join.expr,
                    expected: expected.clone(),
                    actual: culprits[0].1.clone(),
                    reasons: compose_reasons(&expected_causes, &[]),
                });
            } else {
                for (witness, actual) in culprits {
                    diagnostics.push(InferenceDiagnostic::TypeMismatch {
                        expr: witness.blame,
                        expected: expected.clone(),
                        actual,
                        reasons: reasons.clone(),
                    });
                }
            }
        }
        self.unify(table, &join.result, &expected, None);
    }
}

/// The witnesses of `join` with unsolved same-scope nested joins expanded
/// into their own witnesses, recursively: the individual leaf branches that
/// vote. Types come out fully resolved. Function boundaries are opaque —
/// a nested join in another scope was already solved (or is deliberately
/// left free), so it resolves like any other witness.
fn flatten_leaves(
    join: &Join,
    joins: &[Join],
    solved: &[bool],
    table: &mut InPlaceUnificationTable<TyVar>,
    leaves: &mut Vec<(ExprId, Ty)>,
) {
    for witness in &join.witnesses {
        let ty = resolve_fully(table, &witness.ty);
        if let Ty::Infer(var) = ty
            && let Some(nested) = unsolved_nested(join, joins, solved, table, var)
        {
            flatten_leaves(nested, joins, solved, table, leaves);
            continue;
        }
        leaves.push((witness.blame, ty));
    }
}

/// The unsolved join in the same scope as `join` whose result is `var`, if
/// any: a witness of that type is a nested join to flatten into the vote or
/// delegate the expectation to, not a leaf to judge.
fn unsolved_nested<'j>(
    join: &Join,
    joins: &'j [Join],
    solved: &[bool],
    table: &mut InPlaceUnificationTable<TyVar>,
    var: TyVar,
) -> Option<&'j Join> {
    let root = table.find(var);
    joins.iter().enumerate().find_map(|(i, other)| {
        if solved[i] || other.scope != join.scope {
            return None;
        }
        match &other.result {
            Ty::Infer(v) if table.find(*v) == root => Some(other),
            _ => None,
        }
    })
}

/// The tie diagnostic: the first leaf against the first one that concretely
/// disagrees with it (for a plain `if` that is then vs. else).
fn push_tie_mismatch(leaves: &[(ExprId, Ty)], diagnostics: &mut Vec<InferenceDiagnostic>) {
    let concrete = |ty: &Ty| !matches!(ty, Ty::Infer(_) | Ty::Error);
    let Some((first, first_ty)) = leaves.iter().find(|(_, ty)| concrete(ty)) else {
        return;
    };
    let conflicting = leaves.iter().find(|(_, ty)| concrete(ty) && ty != first_ty);
    if let Some((other, other_ty)) = conflicting {
        diagnostics.push(InferenceDiagnostic::IfBranchMismatch {
            else_expr: *other,
            then_expr: *first,
            then_ty: first_ty.clone(),
            else_ty: other_ty.clone(),
        });
    }
}

/// Assemble the hint list for a join mismatch. When an axiom decided the
/// expected type it is the whole story: siblings that agree with it are
/// coincidence, not causes — they would be rejected too if they disagreed.
/// Only when the witnesses themselves decided (a vote, or a delegating
/// outer vote) are the agreeing siblings the explanation. Deduplicated:
/// a witness can be both a direct sibling and a voting leaf.
fn compose_reasons(expected_causes: &[Cause], siblings: &[Cause]) -> Vec<Cause> {
    if expected_causes.iter().any(|cause| cause.is_axiom()) {
        return expected_causes.to_vec();
    }
    let mut reasons: Vec<Cause> = Vec::new();
    for &cause in siblings.iter().chain(expected_causes) {
        if !reasons.contains(&cause) {
            reasons.push(cause);
        }
    }
    reasons
}

pub(crate) fn resolve_shallow(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    let mut ty = ty.clone();
    while let Ty::Infer(var) = ty {
        match table.probe_value(var) {
            TyVarValue::Known(known) => ty = known,
            TyVarValue::Unknown => break,
        }
    }
    ty
}

pub(crate) fn resolve_fully(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(var) => match table.probe_value(*var) {
            TyVarValue::Known(known) => resolve_fully(table, &known),
            // Canonicalize so equal results stay equal across runs.
            TyVarValue::Unknown => Ty::Infer(table.find(*var)),
        },
        Ty::Fn(f) => {
            let params = f.params.iter().map(|p| resolve_fully(table, p)).collect();
            let ret = resolve_fully(table, &f.ret);
            Ty::fn_type(params, ret)
        }
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), resolve_fully(table, ty)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn occurs(table: &mut InPlaceUnificationTable<TyVar>, var: TyVar, ty: &Ty) -> bool {
    match ty {
        Ty::Infer(other) => {
            if table.unioned(var, *other) {
                return true;
            }
            match table.probe_value(*other) {
                TyVarValue::Known(known) => occurs(table, var, &known),
                TyVarValue::Unknown => false,
            }
        }
        Ty::Fn(f) => f.params.iter().any(|p| occurs(table, var, p)) || occurs(table, var, &f.ret),
        Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| occurs(table, var, ty)),
        _ => false,
    }
}
