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

use crate::ItemLoc;
use crate::body::{BindingId, ExprId};
use crate::infer::InferenceDiagnostic;
use crate::ty::{ConstArgValue, GenericArg, NamedTy, Ty, TyVar, TyVarValue, VariantTy};

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
    /// Axiom: a turbofish argument instantiated a generic item's type
    /// parameter to this type at this mention (`id::<usize>` pinned `T`).
    /// Recorded on the instantiation's fresh variable; when the variable
    /// later mismatches, the renderer points at the turbofish argument —
    /// "because `T` was instantiated to `usize` by this argument". (A type
    /// param pinned by a *value* argument instead records no `GenericArg`;
    /// such mismatches render the ordinary [`Cause::CallSite`] hints.)
    GenericArg {
        /// The turbofish mention expression (`ExprData::GenericApp`).
        mention: ExprId,
        /// The argument's position in the turbofish list (= the param's
        /// binder index; positions align once arity checked out).
        index: u32,
    },
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
    /// The leaf sub-expression that produced the type — not the whole branch
    /// block, and never a nested `if`/`else` (traversal flattens a nest into
    /// one witness set): squiggles and "this branch has type …" hints point
    /// here, at the code someone would actually change.
    pub blame: ExprId,
    pub ty: Ty,
}

/// A deferred agreement constraint: every witness must have the type of
/// `result`. Emitted for an `if`/`else` in *statement position* — where its
/// value meets a non-join consumer (a `let`, a call argument, a field
/// initializer, an item root, a function-body tail, a statement discard).
/// An `if` in *witness position* (a branch tail of an enclosing `if`) never
/// forms a join of its own: its leaves are contributed to the enclosing
/// join during traversal, so a whole nest of `if`s is ONE join here and
/// blame speaks about the leaves. Future joining constructs (match arms,
/// loop-break values) contribute witnesses through the same mechanism.
#[derive(Debug)]
pub(crate) struct Join {
    /// The joining construct as a whole (the outermost `if` of its nest).
    /// Blamed when the witnesses agree with each other but contradict an
    /// axiom.
    pub expr: ExprId,
    /// Function-literal nesting depth of the scope this join sits in. Inner
    /// functions solve before their enclosing scope, so a function's type is
    /// settled internally before any outer join consumes it — a function is
    /// a unit that must be internally consistent on its own, and from
    /// outside it is one witness.
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
    /// Join witnesses the solver accepted by *widening* (variant → enum
    /// conversion) rather than by unification: `(leaf expression, the
    /// variant it was)`. Drained into `InferenceResult::widened` by
    /// `InferCtx::solve`, so MIR plants the conversion exactly at these
    /// edges.
    widenings: Vec<(ExprId, VariantTy)>,
}

impl Constraints {
    pub(crate) fn push_join(&mut self, join: Join) {
        self.joins.push(join);
    }

    pub(crate) fn take_widenings(&mut self) -> Vec<(ExprId, VariantTy)> {
        std::mem::take(&mut self.widenings)
    }

    /// The [`Cause::GenericArg`]s recorded on `ty`'s variable (if it is
    /// one): why an instantiated type parameter has the type it has.
    /// Consulted by `InferCtx::check` on a direct mismatch — unlike the
    /// join solver, direct checks don't otherwise read the cause store, and
    /// without this the "instantiated by this argument" hint would only
    /// ever show on join mismatches.
    pub(crate) fn generic_arg_causes(
        &self,
        table: &mut InPlaceUnificationTable<TyVar>,
        ty: &Ty,
    ) -> Vec<Cause> {
        let Ty::Infer(var) = ty else {
            return Vec::new();
        };
        self.causes
            .get(&table.find(*var))
            .map(|causes| {
                causes
                    .iter()
                    .copied()
                    .filter(|cause| matches!(cause, Cause::GenericArg { .. }))
                    .collect()
            })
            .unwrap_or_default()
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
                // A NUMBER-CLASS variable resolves only to an integer
                // scalar type (a defining use); anything else is an
                // ordinary mismatch. `Ty::Error` stays infectious and
                // silent, exactly as for plain variables.
                if matches!(table.probe_value(var), TyVarValue::UnknownNumber)
                    && !matches!(ty, Ty::Int(_) | Ty::Error)
                {
                    return false;
                }
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
            | (Ty::Str, Ty::Str)
            | (Ty::Bool, Ty::Bool) => true,
            // Same-kind only: `u8` never unifies with `u32` — no implicit
            // mixing, an ordinary type mismatch (no conversions in v1).
            (Ty::Int(k1), Ty::Int(k2)) => k1 == k2,
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
            // Exact and equational: same mutability, pointwise pointee. No
            // variance (nothing to be variant over without subtyping), and
            // `&raw mut T` vs `&raw T` is FALSE — a future relaxation would
            // be a shallow `widens_to` conversion, never unification.
            (
                Ty::RawPtr {
                    mutable: m1,
                    pointee: p1,
                },
                Ty::RawPtr {
                    mutable: m2,
                    pointee: p2,
                },
            ) => m1 == m2 && self.unify(table, &p1, &p2, cause),
            // Structural and exact: element pointwise, length by plain
            // equality with `Error` infectious on either side — the same
            // judgement generic const args get in `unify_args`. `[T; 8]`
            // vs `[T; 9]` is simply FALSE: no variance, no conversion.
            (Ty::Array { elem: e1, len: l1 }, Ty::Array { elem: e2, len: l2 }) => {
                (matches!(l1, ConstArgValue::Error)
                    || matches!(l2, ConstArgValue::Error)
                    || l1 == l2)
                    && self.unify(table, &e1, &e2, cause)
            }
            // Nominal: same declaration AND pointwise-unifying args, or
            // nothing — the applicative identity, with NO variance of any
            // kind (Must has no subtyping; every argument position is
            // invariant). A `Named` never unifies with its own underlying
            // record either (no implicit nominal↔structural coercion) —
            // that case falls through to the catch-all `false` below.
            (Ty::Named(a), Ty::Named(b)) => {
                a.decl == b.decl && self.unify_args(table, &a.args, &b.args, cause)
            }
            // Rigid: a type parameter unifies only with itself (same item,
            // same binder index). `Param` vs anything concrete is FALSE —
            // that opacity is what makes a generic body check once, before
            // any instantiation (TR06).
            (Ty::Param(a), Ty::Param(b)) => a == b,
            // A variant type unifies only with itself — same declaration,
            // same variant, pointwise-unifying enum args. Variant vs. its
            // enum is deliberately FALSE here: unification is equational,
            // and variant → enum is a runtime conversion (`widens_to`),
            // applied only at check sites — never inferred backwards
            // through a unification variable.
            (Ty::Variant(a), Ty::Variant(b)) => {
                a.decl == b.decl
                    && a.index == b.index
                    && self.unify_args(table, &a.args, &b.args, cause)
            }
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

    /// Pointwise generic-argument unification (both lists come from
    /// mentions of the SAME declaration, so lengths and kinds line up by
    /// construction; a defensive length check keeps this total anyway).
    /// Const args are values, not types: they unify by plain equality —
    /// rigid `Param`s equal only themselves — with `Error` infectious and
    /// silent on either side.
    fn unify_args(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: &[GenericArg],
        b: &[GenericArg],
        cause: Option<Cause>,
    ) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| match (x, y) {
                (GenericArg::Ty(x), GenericArg::Ty(y)) => self.unify(table, x, y, cause),
                (GenericArg::Const(x), GenericArg::Const(y)) => {
                    matches!(x, ConstArgValue::Error) || matches!(y, ConstArgValue::Error) || x == y
                }
                _ => false,
            })
    }

    /// The variant → enum widening judgement, argument-preserving: `actual`
    /// is a variant of exactly the enum `expected` names AND the enum args
    /// unify pointwise (`Option::<usize>::Some` widens to `Option::<usize>`,
    /// never to `Option::<str>`). Returns the variant (for the conversion
    /// record) on success. The two check sites (`InferCtx::check`, the join
    /// solver below) go through here so the args can bind still-free
    /// variables on either side.
    pub(crate) fn widen_to_enum(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        actual: &Ty,
        expected: &Ty,
    ) -> Option<VariantTy> {
        let (Ty::Variant(variant), Ty::Named(named)) = (actual, expected) else {
            return None;
        };
        if variant.decl != named.decl {
            return None;
        }
        let variant = variant.clone();
        self.unify_args(table, &variant.args, &named.args, None)
            .then_some(variant)
    }

    /// Solve all deferred joins, emitting blame-attributed diagnostics.
    ///
    /// Inner functions solve before their enclosing scope: a function must
    /// be internally consistent on its own, so its joins settle among
    /// themselves before any outer join consumes the function's type — an
    /// internal inconsistency is never resolved by how the function is
    /// used. Within one depth joins solve in traversal order, so a
    /// statement-position join's verdict (a `let`-bound `if`, say) is
    /// settled before any later join consumes the binding. Joins are
    /// independent of each other beyond that: nesting was already flattened
    /// into single joins during traversal.
    pub(crate) fn solve(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
    ) -> Vec<InferenceDiagnostic> {
        let joins = std::mem::take(&mut self.joins);
        let mut order: Vec<usize> = (0..joins.len()).collect();
        // Stable sort: traversal (push) order within one depth.
        order.sort_by_key(|&i| std::cmp::Reverse(joins[i].depth));
        let mut diagnostics = Vec::new();
        for i in order {
            self.solve_join(&joins[i], table, &mut diagnostics);
        }
        diagnostics
    }

    fn solve_join(
        &mut self,
        join: &Join,
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
            // No axiom constrained the result: the witnesses — the leaf
            // branches of the whole (possibly nested) construct — vote, so
            // a plurality of leaves wins regardless of the nesting shape
            // they arrived in. Voting is *family-aware*: a Variant or
            // Named-enum leaf votes for its enum declaration (the family
            // root), any other leaf for its own type. The winning family
            // then resolves to its least upper bound — every leaf the same
            // variant keeps that variant (precision survives, zero
            // conversions); mixed variants of one enum widen to the enum
            // (each variant leaf gets its conversion in the loop below).
            Ty::Infer(_) => {
                let leaves: Vec<(ExprId, Ty)> = join
                    .witnesses
                    .iter()
                    .map(|witness| (witness.blame, resolve_fully(table, &witness.ty)))
                    .collect();
                let mut tally: Vec<(Family, usize)> = Vec::new();
                for (_, ty) in &leaves {
                    // A leaf with ANY undetermined part doesn't vote: two
                    // `[{number}; 2]`s must end up unified (the tie-them-
                    // together path below), never counted as two distinct
                    // families.
                    if ty.contains_infer() || matches!(ty, Ty::Error) {
                        continue;
                    }
                    let family = family_of(ty);
                    match tally.iter_mut().find(|(f, _)| *f == family) {
                        Some((_, n)) => *n += 1,
                        None => tally.push((family, 1)),
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
                let family = tally.iter().find(|&&(_, n)| n == max).unwrap().0.clone();
                let members: Vec<&Ty> = leaves
                    .iter()
                    .filter(|(_, ty)| family_of(ty) == family)
                    .map(|(_, ty)| ty)
                    .collect();
                // Least upper bound within the family: identical leaves
                // keep their exact type; anything mixed is only possible in
                // an enum family, whose LUB is the enum itself.
                let winner = if members.iter().all(|&ty| ty == members[0]) {
                    members[0].clone()
                } else {
                    let Family::Enum(loc) = &family else {
                        unreachable!("only enum families hold more than one type")
                    };
                    // The enum with the first member's generic args: the
                    // remaining members' args unify against it in the
                    // witness pass below (an arg disagreement is an
                    // ordinary culprit).
                    let args = members
                        .iter()
                        .find_map(|ty| match ty {
                            Ty::Variant(variant) => Some(variant.args.clone()),
                            Ty::Named(named) => Some(named.args.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    Ty::Named(NamedTy {
                        decl: loc.clone(),
                        args,
                    })
                };
                // The winning leaves are the causes: "this branch has type
                // …" hints, wherever in the nesting those leaves sit.
                let voters = leaves
                    .iter()
                    .filter(|(_, ty)| family_of(ty) == family)
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
        // variables adopt the type; witnesses whose variant type widens to
        // an expected enum pass with a conversion at their edge; the rest
        // are culprits.
        let mut siblings: Vec<Cause> = Vec::new();
        let mut widened = 0usize;
        let mut culprits: Vec<(&Witness, Ty)> = Vec::new();
        for witness in &join.witnesses {
            match resolve_shallow(table, &witness.ty) {
                Ty::Error => {}
                // Still free (a cross-scope or in-group variable): adopt
                // the expected type. A NUMBER variable is a real leaf,
                // not a silent adopter: agreeing with an integer winner
                // makes it a sibling ("this branch has type …" hints stay
                // honest — the leaf now IS that type), and refusing a
                // non-integer winner is an ordinary culprit (`{number}`
                // against the winner), with the variable poisoned so the
                // literal doesn't also report a no-defining-use error.
                Ty::Infer(var) => {
                    let is_number = matches!(table.probe_value(var), TyVarValue::UnknownNumber);
                    if self.unify(table, &witness.ty, &expected, None) {
                        if is_number {
                            siblings.push(Cause::Branch(witness.blame));
                        }
                    } else {
                        // The culprit's honest type, resolved BEFORE the var
                        // is poisoned to `{error}`: a number var reads
                        // `{number}`, any other still-free variable reads `_`.
                        let actual = resolve_finished(table, &witness.ty);
                        table.union_value(var, TyVarValue::Known(Ty::Error));
                        culprits.push((witness, actual));
                    }
                }
                actual => {
                    if self.unify(table, &witness.ty, &expected, None) {
                        // Hint on the tail sub-expression that produced the
                        // type, not the whole branch.
                        siblings.push(Cause::Branch(witness.blame));
                    } else if let Some(variant) = self.widen_to_enum(table, &actual, &expected) {
                        // Not unified (the leaf keeps its precise variant
                        // type) — the conversion op lands on this edge. Not
                        // a sibling either: a "this branch has type
                        // `Shape`" hint on a `Shape::Circle` leaf would
                        // lie.
                        self.widenings.push((witness.blame, variant));
                        widened += 1;
                    } else {
                        culprits.push((witness, actual));
                    }
                }
            }
        }

        let reasons = compose_reasons(&expected_causes, &siblings);

        if !culprits.is_empty() {
            // Unanimous means *every* witness is a culprit of the same
            // type. A vote can't end up here unanimously (its winner comes
            // from a witness), so a unanimous conflict always has an axiom
            // behind `expected` — though not necessarily a recorded cause
            // yet.
            let unanimous = siblings.is_empty()
                && widened == 0
                && culprits.iter().all(|(_, ty)| *ty == culprits[0].1);
            if unanimous {
                // The leaves agree with each other and only contradict the
                // context: one diagnostic on the whole (flattened)
                // construct, not a squiggle per leaf.
                let actual = resolve_finished(table, &culprits[0].1);
                for (witness, _) in &culprits {
                    poison_unresolved_number(table, &witness.ty);
                }
                diagnostics.push(InferenceDiagnostic::AllBranchesMismatch {
                    expr: join.expr,
                    expected: expected.clone(),
                    actual,
                    reasons: compose_reasons(&expected_causes, &[]),
                });
            } else {
                for (witness, actual) in culprits {
                    // Render nested unpinned numbers as `{number}` first,
                    // then poison them: this mismatch is their whole story.
                    let actual = resolve_finished(table, &actual);
                    poison_unresolved_number(table, &witness.ty);
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

/// Which "family" a join leaf votes for. Variant-typed and enum-typed
/// leaves of one enum are votes for the *same* outcome family (their least
/// upper bound is decided after the vote); every other type is a family of
/// its own. Keyed by declaration, not by asking the database whether a
/// `Named` is an enum: a struct's `Named` gets a declaration key too, but
/// no `Variant` can share it (variants are only minted from enum
/// declarations), so its family LUB degenerates to plain equality.
#[derive(Clone, PartialEq)]
enum Family {
    Enum(ItemLoc),
    Shape(Ty),
}

fn family_of(ty: &Ty) -> Family {
    // Keyed by declaration alone (not args): mixed-arg leaves of one enum
    // are one family whose vote resolves to the first member's args — a
    // real arg disagreement then surfaces as an ordinary witness mismatch
    // rather than a family tie.
    match ty {
        Ty::Variant(variant) => Family::Enum(variant.decl.clone()),
        Ty::Named(named) => Family::Enum(named.decl.clone()),
        other => Family::Shape(other.clone()),
    }
}

/// The tie diagnostic: the first leaf against the first one that concretely
/// disagrees with it (for a plain `if` that is then vs. else).
fn push_tie_mismatch(leaves: &[(ExprId, Ty)], diagnostics: &mut Vec<InferenceDiagnostic>) {
    let concrete = |ty: &Ty| !ty.contains_infer() && !matches!(ty, Ty::Error);
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
/// Only when the witnesses themselves decided (a vote) are the agreeing
/// siblings the explanation. Deduplicated: a witness can be both a direct
/// sibling and a voting leaf.
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
            TyVarValue::Unknown | TyVarValue::UnknownNumber => break,
        }
    }
    ty
}

/// Whether `ty` resolves (shallowly) to a still-unbound NUMBER-CLASS
/// variable — the "this is an unpinned integer literal" predicate.
pub(crate) fn is_unresolved_number(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> bool {
    matches!(
        resolve_shallow(table, ty),
        Ty::Infer(var) if matches!(table.probe_value(var), TyVarValue::UnknownNumber)
    )
}

/// Bind every still-unbound NUMBER-CLASS variable anywhere inside `ty` to
/// `{error}` — used after a mismatch was already reported against it, so
/// the literal(s) that minted them don't pile no-defining-use diagnostics
/// on top (errors are infectious and silent).
pub(crate) fn poison_unresolved_number(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) {
    match resolve_shallow(table, ty) {
        Ty::Infer(var) => {
            if matches!(table.probe_value(var), TyVarValue::UnknownNumber) {
                table.union_value(var, TyVarValue::Known(Ty::Error));
            }
        }
        Ty::Fn(f) => {
            for param in &f.params {
                poison_unresolved_number(table, param);
            }
            poison_unresolved_number(table, &f.ret);
        }
        Ty::RawPtr { pointee, .. } => poison_unresolved_number(table, &pointee),
        Ty::Array { elem, .. } => poison_unresolved_number(table, &elem),
        Ty::Record(rec) => {
            for (_, field) in &rec.fields {
                poison_unresolved_number(table, field);
            }
        }
        Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
            for arg in &args {
                if let GenericArg::Ty(ty) = arg {
                    poison_unresolved_number(table, ty);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn resolve_fully(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(var) => match table.probe_value(*var) {
            TyVarValue::Known(known) => resolve_fully(table, &known),
            // Canonicalize so equal results stay equal across runs. The
            // number flavor stays in the table here — `resolve_finished`
            // is the one place it becomes a rendered `{number}`.
            TyVarValue::Unknown | TyVarValue::UnknownNumber => Ty::Infer(table.find(*var)),
        },
        Ty::Fn(f) => {
            let params = f.params.iter().map(|p| resolve_fully(table, p)).collect();
            let ret = resolve_fully(table, &f.ret);
            Ty::fn_type(params, ret)
        }
        Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(*mutable, resolve_fully(table, pointee)),
        Ty::Array { elem, len } => Ty::array(resolve_fully(table, elem), len.clone()),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), resolve_fully(table, ty)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: resolve_args_fully(table, &named.args),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: resolve_args_fully(table, &variant.args),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

/// [`resolve_fully`], additionally rendering still-unbound NUMBER-CLASS
/// variables into the table-free [`Ty::UnresolvedNumber`] sentinel — the
/// resolution `infer`'s finish pass applies to everything a consumer will
/// *display* (hover, diagnostics, inlays): an unpinned literal reads
/// `{number}`, never `_`. Purely a rendering step (it reads the table's
/// number-class flavor and rewrites it into the sentinel); it mutates
/// nothing.
pub(crate) fn resolve_finished(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    let resolved = resolve_fully(table, ty);
    render_unresolved_numbers(table, &resolved)
}

fn render_unresolved_numbers(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(var) => match table.probe_value(*var) {
            TyVarValue::UnknownNumber => Ty::UnresolvedNumber,
            _ => ty.clone(),
        },
        Ty::Fn(f) => Ty::fn_type(
            f.params
                .iter()
                .map(|p| render_unresolved_numbers(table, p))
                .collect(),
            render_unresolved_numbers(table, &f.ret),
        ),
        Ty::RawPtr { mutable, pointee } => {
            Ty::raw_ptr(*mutable, render_unresolved_numbers(table, pointee))
        }
        Ty::Array { elem, len } => Ty::array(render_unresolved_numbers(table, elem), len.clone()),
        Ty::Record(rec) => Ty::record(
            rec.fields
                .iter()
                .map(|(name, ty)| (name.clone(), render_unresolved_numbers(table, ty)))
                .collect(),
        ),
        Ty::Named(named) => Ty::Named(NamedTy {
            decl: named.decl.clone(),
            args: named
                .args
                .iter()
                .map(|arg| match arg {
                    GenericArg::Ty(ty) => GenericArg::Ty(render_unresolved_numbers(table, ty)),
                    GenericArg::Const(value) => GenericArg::Const(value.clone()),
                })
                .collect(),
        }),
        Ty::Variant(variant) => Ty::Variant(VariantTy {
            args: variant
                .args
                .iter()
                .map(|arg| match arg {
                    GenericArg::Ty(ty) => GenericArg::Ty(render_unresolved_numbers(table, ty)),
                    GenericArg::Const(value) => GenericArg::Const(value.clone()),
                })
                .collect(),
            ..variant.clone()
        }),
        other => other.clone(),
    }
}

/// [`resolve_fully`] over a generic-argument list — also used by `infer`'s
/// finish pass for the `VariantTy`s persisted outside any `Ty` (`widened`,
/// `variant_of_expr`, `variant_of_pat`).
pub(crate) fn resolve_args_fully(
    table: &mut InPlaceUnificationTable<TyVar>,
    args: &[GenericArg],
) -> Vec<GenericArg> {
    args.iter()
        .map(|arg| match arg {
            GenericArg::Ty(ty) => GenericArg::Ty(resolve_fully(table, ty)),
            GenericArg::Const(value) => GenericArg::Const(value.clone()),
        })
        .collect()
}

fn occurs(table: &mut InPlaceUnificationTable<TyVar>, var: TyVar, ty: &Ty) -> bool {
    match ty {
        Ty::Infer(other) => {
            if table.unioned(var, *other) {
                return true;
            }
            match table.probe_value(*other) {
                TyVarValue::Known(known) => occurs(table, var, &known),
                TyVarValue::Unknown | TyVarValue::UnknownNumber => false,
            }
        }
        Ty::Fn(f) => f.params.iter().any(|p| occurs(table, var, p)) || occurs(table, var, &f.ret),
        Ty::RawPtr { pointee, .. } => occurs(table, var, pointee),
        Ty::Array { elem, .. } => occurs(table, var, elem),
        Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| occurs(table, var, ty)),
        Ty::Named(NamedTy { args, .. }) | Ty::Variant(VariantTy { args, .. }) => {
            args.iter().any(|arg| match arg {
                GenericArg::Ty(ty) => occurs(table, var, ty),
                GenericArg::Const(_) => false,
            })
        }
        _ => false,
    }
}
