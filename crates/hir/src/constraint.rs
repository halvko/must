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

use ena::unify::{InPlace, InPlaceUnificationTable, Snapshot};
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::ItemLoc;
use crate::body::{BindingId, ExprId, PatId};
use crate::infer::InferenceDiagnostic;
use crate::ty::{ConstArgValue, GenericArg, NamedTy, Region, Safety, Ty, TyVar, TyVarValue};

/// Why a type was required or concluded. Attached to
/// [`InferenceDiagnostic::TypeMismatch`] to render "because of this" hints;
/// each variant has a dedicated arm in the diagnostic renderer so future
/// causes can't accidentally re-use existing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Axiom: a `let` binding's type annotation established the type.
    Binding(BindingId),
    /// Axiom: the type annotation of a `let` whose pattern is no bare name
    /// (`let _: T = ...`, `let struct { x }: T = ...`).
    LetAnnotation(PatId),
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
        /// The argument's position in the WRITTEN turbofish list — what the
        /// renderer counts to find the node. Not the binder index: a fn
        /// item's regions are elided, so the two diverge the moment a
        /// binder declares one.
        index: u32,
        /// The BINDER index of the parameter the argument instantiated,
        /// RECORDED rather than re-derived. Producers know it exactly; a
        /// renderer counting back from `index` would have to know which
        /// kinds that producer elides, and the two producers elide
        /// differently — a fn item's regions, a type mention's nothing.
        param: u32,
    },
    /// Axiom: a MEMBER turbofish argument instantiated a member's own type
    /// parameter (`o.map::<bool>(f)` pinned `U`). Its own variant rather
    /// than [`Self::GenericArg`] because the two read different NODES —
    /// the owner's list hangs directly off the path, a member's hangs
    /// inside `MEMBER_GENERIC_ARGS` under either a path or a field
    /// expression — and because a member's list is positional over its
    /// TYPE parameters only, so the position is not a binder index.
    MemberGenericArg {
        /// The mention: the qualified path, or the dot-call's callee.
        mention: ExprId,
        /// The argument's position in the MEMBER's own list.
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

/// What [`Constraints::rollback_to`] restores.
pub(crate) struct ConstraintSnapshot {
    table: Snapshot<InPlace<TyVar>>,
    region_edges: usize,
    safety_edges: Vec<(TyVar, TyVar)>,
}

/// How a value met its expected type at a check site
/// ([`Constraints::relate_value`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relation {
    Equal,
    /// Through the safe-to-unsafe conversion.
    Converted,
    Mismatch,
}

/// Unification plus the provenance and deferred-join state accumulated over
/// one inference pass. Owned by `InferCtx`; [`Constraints::solve`] runs after
/// traversal, in both the per-item query and group inference (group mode
/// discards the diagnostics — the per-item query re-derives them — but the
/// unifications are what make join-typed signatures come out concrete).
#[derive(Default)]
pub(crate) struct Constraints {
    joins: Vec<Join>,
    /// Every outlives obligation the body incurs, in emission order.
    ///
    /// Lives HERE rather than in `InferCtx` because the join solver runs
    /// inside this store, after traversal, and a join leaf must produce
    /// exactly the obligations a direct assignment would. Keeping two
    /// emitters was the bug: joins went through `unify`, which is
    /// region-blind by design, so wrapping any borrow in an `if` erased
    /// the obligation its use site would otherwise impose. One emitter,
    /// one answer.
    region_edges: Vec<RegionConstraint>,
    /// Where the checker INSERTED a reborrow, and what flavor came out.
    /// Drained into the inference result so MIR can materialize the
    /// operation: an implicit reborrow that produces no MIR is invisible
    /// to the interpreter's aliasing tree, which means the parent/child
    /// relation Tree Borrows models simply does not exist at runtime.
    reborrows: ArenaMap<ExprId, bool>,
    /// The TARGET region of each inserted reborrow — the child node's own
    /// region, which is what loan liveness measures.
    ///
    /// Recorded rather than read back off the use site's type, because a
    /// use site does not consistently carry it: an ARGUMENT is inferred
    /// against the parameter and records the target, while a member-call
    /// RECEIVER is checked after the fact and keeps the source. Read from
    /// the outside the two are indistinguishable, and reading the source
    /// makes a reborrow of a parameter come out at the parameter's own
    /// universal — a loan with no body-local extent at all.
    reborrow_targets: Vec<(ExprId, Region)>,
    /// How many speculative snapshots are currently open.
    ///
    /// Region obligations are a plain `Vec` with no rollback of their own,
    /// so a `relate` performed speculatively would leave PHANTOM edges
    /// behind when the unification it belongs to is rolled back — an
    /// obligation attributed to a program state that never happened. That
    /// is a worse failure than the laundering this arc closed: laundering
    /// accepts too much and is at least visible as a missing error, while
    /// a phantom edge rejects a correct program for a reason that is not
    /// in the source.
    ///
    /// Nothing does this today (the one snapshot site adopts, and adoption
    /// emits nothing), but speculative unification is the obvious next
    /// consumer — impl selection, coercions. Two cheap prophylactics keep
    /// it from being inherited silently: [`Self::rollback_to`] truncates
    /// the edges, and [`Self::push_outlives`] asserts no snapshot is open.
    /// Whoever builds speculative `relate` gets an assertion failure and
    /// has to design the interaction, rather than a bug.
    snapshot_depth: u32,
    /// The next region-variable index — minted here for the same reason
    /// the edges are recorded here: the join solver needs a fresh region
    /// for a voted borrow result (the MEET of its branches) and cannot
    /// reach back into `InferCtx` to ask for one.
    next_region: u32,
    /// Canonical root var → the causes that decided its concrete type.
    causes: FxHashMap<TyVar, Vec<Cause>>,
    /// `(sub, sup)`: open safety variables related by the safe-to-unsafe
    /// conversion, `sub <= sup` ([`Self::coerce_safety`]). Read when either
    /// side is decided, which can decide the other: an `unsafe` sub makes
    /// the sup `unsafe`, a safe sup makes the sub safe.
    safety_edges: Vec<(TyVar, TyVar)>,
    /// Set for every body checked inside an inference group (T11): an
    /// unbound variable meeting an erroneous type stays unbound, and
    /// [`Self::poison`] poisons nothing, so one body's own error never
    /// becomes `{error}` in another item's signature.
    leave_errors_unbound: bool,
    /// The variables [`Self::leave_errors_unbound`] left unbound because
    /// they met an error.
    left_unbound: Vec<TyVar>,
}

/// One outlives obligation collected from a body: `sup` must outlive `sub`,
/// i.e. `Values(sup) ⊇ Values(sub)`.
///
/// Directed, always — even where invariance emits both directions. Keeping
/// the edge directed (rather than merging the two regions into an
/// equivalence class) is what lets covariance arrive later as a change of
/// which direction is emitted, instead of a rewrite of the solver: SCC
/// condensation collapses a 2-cycle for free at solve time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionConstraint {
    /// The longer-lived region.
    pub sup: Region,
    /// The region it must cover.
    pub sub: Region,
    /// The expression that produced the obligation — blame's anchor.
    pub origin: ExprId,
    pub reason: RegionConstraintReason,
}

/// Why an outlives obligation exists. Carried so a violation can explain
/// itself in the terms the user wrote, rather than as a solver fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionConstraintReason {
    /// A borrow flowed into a position expecting a shorter one — the
    /// implicit REBORROW that every mention of a borrow performs.
    Reborrow,
    /// A `&mut` flowed into a `&` position: degradation, which is a shared
    /// reborrow (the region shrinks, the parent suspends) and never
    /// subtyping.
    Degradation,
    /// Two borrow types had to be the same type — invariance, so both
    /// directions are emitted. Only ever emitted for a borrow nested
    /// INSIDE another type: forming a borrow is never invariant, because
    /// each use mints its own reborrow.
    Invariance,
    /// A callee's declared outlives bound (`fn::<@a, @b: @a>`) travels
    /// with the instantiation at a call site, as an edge between the fresh
    /// existentials that stand for its regions there. The callee is a free
    /// fn, an inherent member or a trait requirement; one reading serves
    /// all three (`push_region_binder_bounds`). Regions are elided at every
    /// call, so no caller region is ever directly on this edge: a violation
    /// surfaces where an argument's reborrow or the returned borrow carries
    /// the element across, blamed as that use — this reason keeps the bound
    /// ENFORCED, not reported.
    CalleeBound,
    /// A `match` on a BORROWED scrutinee bound a payload by borrow — the
    /// projection edge, `@scrutinee: @payload`. Structurally the same
    /// single directed edge a reborrow emits (the payload is a sub-place
    /// of what the scrutinee points at, so it can never be good for
    /// longer), named apart only so the message can point at the match
    /// rather than at a borrow the user never wrote.
    Projection,
}

impl Constraints {
    /// Constraints for one body; `in_group` when it is checked inside an
    /// inference group.
    pub(crate) fn new(in_group: bool) -> Self {
        Constraints {
            leave_errors_unbound: in_group,
            ..Constraints::default()
        }
    }

    /// Commit `var` to `{error}` after its mistake was reported. The one
    /// way any check poisons a variable, so no body checked inside an
    /// inference group ([`Self::new`]) can poison one of the group's.
    pub(crate) fn poison(&mut self, table: &mut InPlaceUnificationTable<TyVar>, var: TyVar) {
        if self.leave_errors_unbound {
            self.left_unbound.push(var);
        } else {
            table.union_value(var, TyVarValue::Known(Ty::Error));
        }
    }

    /// The variables this body left unbound instead of poisoning.
    pub(crate) fn left_unbound(&self) -> &[TyVar] {
        &self.left_unbound
    }

    /// Record `var` as met by an error, to commit to `{error}` once its
    /// group has decided (see [`Self::left_unbound`]).
    pub(crate) fn leave_unbound(&mut self, var: TyVar) {
        self.left_unbound.push(var);
    }

    /// Whether errors are left unbound (see [`Self::new`]).
    pub(crate) fn leaves_errors_unbound(&self) -> bool {
        self.leave_errors_unbound
    }

    /// Whether a variable meeting `ty` stays unbound: errors are left unbound
    /// and `ty` contains one.
    fn is_left_unbound(&self, ty: &Ty) -> bool {
        self.leave_errors_unbound && ty.contains_error()
    }

    /// Bind every unbound NUMBER-CLASS variable anywhere inside `ty`
    /// to `{error}`: used after a mismatch was already reported against
    /// it, so the literal(s) that minted them don't pile no-defining-use
    /// diagnostics on top (errors are infectious and silent).
    pub(crate) fn poison_unresolved_number(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        ty: &Ty,
    ) {
        unresolved_numbers(table, ty, &mut |table, var| {
            if self.leave_errors_unbound {
                self.left_unbound.push(var);
            } else {
                table.union_value(var, TyVarValue::Known(Ty::Error));
            }
        });
    }

    pub(crate) fn push_join(&mut self, join: Join) {
        self.joins.push(join);
    }

    pub(crate) fn take_region_edges(&mut self) -> Vec<RegionConstraint> {
        std::mem::take(&mut self.region_edges)
    }

    pub(crate) fn take_reborrows(&mut self) -> ArenaMap<ExprId, bool> {
        std::mem::take(&mut self.reborrows)
    }

    pub(crate) fn take_reborrow_targets(&mut self) -> Vec<(ExprId, Region)> {
        std::mem::take(&mut self.reborrow_targets)
    }

    /// Open a speculative unification. Every snapshot goes through here so
    /// the region-obligation contract above has somewhere to live; taking
    /// `table.snapshot()` directly would bypass it.
    pub(crate) fn snapshot(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
    ) -> ConstraintSnapshot {
        self.snapshot_depth += 1;
        ConstraintSnapshot {
            table: table.snapshot(),
            region_edges: self.region_edges.len(),
            safety_edges: self.safety_edges.clone(),
        }
    }

    pub(crate) fn commit(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        snapshot: ConstraintSnapshot,
    ) {
        self.snapshot_depth -= 1;
        table.commit(snapshot.table);
    }

    /// Undo a speculative unification — INCLUDING any obligations it
    /// recorded. Truncation is exact because edges are only ever appended.
    pub(crate) fn rollback_to(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        snapshot: ConstraintSnapshot,
    ) {
        self.snapshot_depth -= 1;
        table.rollback_to(snapshot.table);
        self.region_edges.truncate(snapshot.region_edges);
        // Copied whole: deciding a safety spends edges, not only adds them.
        self.safety_edges = snapshot.safety_edges;
    }

    pub(crate) fn region_count(&self) -> u32 {
        self.next_region
    }

    /// A fresh EXISTENTIAL region — one inference variable, numbered per
    /// body. Minted by every borrow expression, every `@_`, every region
    /// parameter of a callee at a call site, and every join of borrows
    /// (whose result region is the MEET of its branches).
    pub(crate) fn fresh_region(&mut self) -> Region {
        let var = crate::ty::RegionVar(self.next_region);
        self.next_region += 1;
        Region::Var(var)
    }

    /// Record `sup: sub` — "`sup` outlives `sub`".
    pub(crate) fn push_outlives(
        &mut self,
        sup: Region,
        sub: Region,
        origin: ExprId,
        reason: RegionConstraintReason,
    ) {
        debug_assert!(
            self.snapshot_depth == 0,
            "an outlives obligation was recorded under a speculative \
             unification — see `Constraints::snapshot_depth`; the rollback \
             would undo the unification and keep the obligation"
        );
        // A broken region carries its own diagnostic; constraining it would
        // only produce a second, derived one.
        if matches!(sup, Region::Error) || matches!(sub, Region::Error) {
            return;
        }
        self.region_edges.push(RegionConstraint {
            sup,
            sub,
            origin,
            reason,
        });
    }

    /// Relate two regions INVARIANTLY — both directions, as two directed
    /// edges. Never an equality merge: see [`RegionConstraint`].
    fn relate_regions_invariant(&mut self, a: &Region, b: &Region, origin: ExprId) {
        self.push_outlives(
            a.clone(),
            b.clone(),
            origin,
            RegionConstraintReason::Invariance,
        );
        self.push_outlives(
            b.clone(),
            a.clone(),
            origin,
            RegionConstraintReason::Invariance,
        );
    }

    /// Every region pairing between two types that are being made equal —
    /// the invariant walk, for borrows nested INSIDE another type. A
    /// top-level borrow never comes here: forming one is a reborrow, which
    /// is one directed edge.
    pub(crate) fn relate_type_regions(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: &Ty,
        b: &Ty,
        origin: ExprId,
    ) {
        let (a, b) = (resolve_shallow(table, a), resolve_shallow(table, b));
        match (&a, &b) {
            (
                Ty::Borrow {
                    region: ra,
                    referent: ta,
                    ..
                },
                Ty::Borrow {
                    region: rb,
                    referent: tb,
                    ..
                },
            ) => {
                self.relate_regions_invariant(ra, rb, origin);
                self.relate_type_regions(table, ta, tb, origin);
            }
            (Ty::RawPtr { pointee: pa, .. }, Ty::RawPtr { pointee: pb, .. }) => {
                self.relate_type_regions(table, pa, pb, origin);
            }
            (Ty::Array { elem: ea, .. }, Ty::Array { elem: eb, .. }) => {
                self.relate_type_regions(table, ea, eb, origin);
            }
            (Ty::Fn(fa), Ty::Fn(fb)) => {
                for (pa, pb) in fa.params.iter().zip(&fb.params) {
                    self.relate_type_regions(table, pa, pb, origin);
                }
                let (ra, rb) = (fa.ret.clone(), fb.ret.clone());
                self.relate_type_regions(table, &ra, &rb, origin);
            }
            (Ty::Record(ra), Ty::Record(rb)) => {
                for ((_, ta), (_, tb)) in ra.fields.iter().zip(&rb.fields) {
                    let (ta, tb) = (ta.clone(), tb.clone());
                    self.relate_type_regions(table, &ta, &tb, origin);
                }
            }
            // A nominal type's generic ARGS are its identity, and a borrow
            // may sit in one (`Option::<V.&mut::<@b>>`). Without this arm a
            // generic type constructor is a laundry: `unify` agrees the two
            // mentions are the same type (it is region-blind by design) and
            // nothing relates the regions inside, so a borrow of a local
            // flows into a caller's region with no diagnostic at all. Same
            // obligation as `Ty::Record`'s, one constructor along.
            //
            // The declared SHAPE stays out of it, correctly: no `type` can
            // name a region today (region params on type declarations are
            // reserved), so a declaration cannot hide one — the args are
            // the whole story. `Ty::contains_borrow` carries the note about
            // the day that changes.
            (Ty::Named(NamedTy { args: aa, .. }), Ty::Named(NamedTy { args: ab, .. })) => {
                for (ga, gb) in aa.iter().zip(ab) {
                    match (ga, gb) {
                        (GenericArg::Ty(ta), GenericArg::Ty(tb)) => {
                            let (ta, tb) = (ta.clone(), tb.clone());
                            self.relate_type_regions(table, &ta, &tb, origin);
                        }
                        // A region ARGUMENT is invariant exactly as a
                        // borrow's own region is — a generic argument is
                        // the type's identity, so it may neither grow nor
                        // shrink. Unreachable until a type declaration may
                        // take a region; exhaustive so the day it can is a
                        // compile error here, not a silent skip.
                        (GenericArg::Region(ra), GenericArg::Region(rb)) => {
                            let (ra, rb) = (ra.clone(), rb.clone());
                            self.relate_regions_invariant(&ra, &rb, origin);
                        }
                        // A const argument carries no region.
                        (GenericArg::Const(_), GenericArg::Const(_)) => {}
                        // Mismatched kinds: `unify` carries that error.
                        (GenericArg::Ty(_) | GenericArg::Region(_) | GenericArg::Const(_), _) => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Every region in `ty` replaced by a fresh variable — the join
    /// result's own regions, which every branch is then related into.
    /// Taking the first branch's regions instead makes the join mean
    /// "whatever branch one said", which is not what a join means.
    fn freshen_regions(&mut self, ty: &Ty) -> Ty {
        match ty {
            Ty::Borrow {
                mutable, referent, ..
            } => {
                let region = self.fresh_region();
                Ty::borrow(*mutable, region, self.freshen_regions(referent))
            }
            Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(*mutable, self.freshen_regions(pointee)),
            Ty::Array { elem, len } => Ty::array(self.freshen_regions(elem), len.clone()),
            Ty::Record(rec) => Ty::record(
                rec.fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), self.freshen_regions(ty)))
                    .collect(),
            ),
            Ty::Fn(f) => {
                let params = f.params.iter().map(|p| self.freshen_regions(p)).collect();
                f.rebuilt(params, self.freshen_regions(&f.ret))
            }
            // A borrow inside a generic argument is as much the join's own
            // region as a top-level one: without this the join's result
            // keeps branch one's regions there, which is precisely what
            // taking the first branch's regions was rejected for.
            Ty::Named(named) => Ty::Named(NamedTy {
                decl: named.decl.clone(),
                args: self.freshen_regions_args(&named.args),
            }),
            other => other.clone(),
        }
    }

    fn freshen_regions_args(&mut self, args: &[GenericArg]) -> Vec<GenericArg> {
        args.iter()
            .map(|arg| match arg {
                GenericArg::Ty(ty) => GenericArg::Ty(self.freshen_regions(ty)),
                // A region ARGUMENT is as much the join's own region as one
                // under a borrow: it was dropped here while the borrow case
                // was handled, which is the same asymmetry that produced
                // the four `Ty::Named` holes. Unreachable until a type
                // declaration may take a region, and correct the day it can.
                GenericArg::Region(_) => GenericArg::Region(self.fresh_region()),
                // A const argument carries no region and never will —
                // regions are erased and must never reach instance keys.
                GenericArg::Const(value) => GenericArg::Const(value.clone()),
            })
            .collect()
    }

    /// The REBORROW relation, the one place a borrow may flow into a
    /// position wanting a different region — and the ONE emitter, shared
    /// by direct checks and by join leaves.
    ///
    /// `actual` is `T.&[mut]::<@src>`, `expected` is `T.&[mut]::<@tgt>`.
    /// The referents are related invariantly (no variance anywhere yet);
    /// the regions get ONE edge, `@src: @tgt` — the parent must outlive the
    /// child it is suspended for. That single asymmetric edge IS
    /// reborrow-at-every-use: every mention of a borrow-typed place mints a
    /// fresh, shorter loan rather than moving the original, and a move is
    /// just the case where the target needs the source's full region. Both
    /// compile to a pointer copy, so preferring the more permissive branch
    /// is free under the erasure law.
    ///
    /// `&mut` into a `&` position is the same event with the mutability
    /// dropped — DEGRADATION, a shared reborrow, not subtyping (a callee
    /// could stash a shared reference for all of the parent's region, so
    /// the region must shrink and the parent must suspend).
    ///
    /// Returns `false` when the flavors cannot reborrow at all (`&` into a
    /// `&mut` position), leaving the ordinary mismatch to report.
    pub(crate) fn try_reborrow(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        actual: &Ty,
        expected: &Ty,
        origin: ExprId,
    ) -> bool {
        let (
            Ty::Borrow {
                mutable: m_src,
                region: r_src,
                referent: t_src,
            },
            Ty::Borrow {
                mutable: m_tgt,
                region: r_tgt,
                referent: t_tgt,
            },
        ) = (
            resolve_shallow(table, actual),
            resolve_shallow(table, expected),
        )
        else {
            return false;
        };
        // Shared can never become exclusive; exclusive may degrade.
        if m_tgt && !m_src {
            return false;
        }
        // Raw is correct here: the very next line relates the referents'
        // regions, and the borrow's own regions get their directed edge
        // below. This IS the edge-emitting wrapper.
        if !self.unify(table, &t_src, &t_tgt, None) {
            return false;
        }
        self.relate_type_regions(table, &t_src, &t_tgt, origin);
        let reason = if m_src && !m_tgt {
            RegionConstraintReason::Degradation
        } else {
            RegionConstraintReason::Reborrow
        };
        self.push_outlives(r_src, r_tgt.clone(), origin, reason);
        // Record the insertion so MIR can MATERIALIZE it. M07 says
        // degradation is not spelled `v.*.&` precisely "because the
        // explicit form produces exactly the same child node" — which is
        // only true if the implicit form produces one at all.
        self.reborrows.insert(origin, m_tgt);
        // And the child node's region, for loan liveness — see
        // `Constraints::reborrow_targets`.
        self.reborrow_targets.push((origin, r_tgt));
        true
    }

    /// The turbofish causes ([`Cause::GenericArg`],
    /// [`Cause::MemberGenericArg`]) recorded on `ty`'s variable (if it is
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
                    .filter(|cause| {
                        matches!(
                            cause,
                            Cause::GenericArg { .. } | Cause::MemberGenericArg { .. }
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The one unification entry point. Binding a variable to a concrete
    /// type records `cause` (first cause wins) for later blame attribution.
    /// ADOPTION: one side is an unbound variable (or an `Error`), so it
    /// takes the other's type WHOLESALE — including its regions. There is
    /// no second set of regions to relate, so no obligation can be lost.
    ///
    /// This is one of the two ways to reach [`Self::unify`], and the
    /// distinction is not stylistic. Region-blind unification of two types
    /// that BOTH have regions silently discards every obligation between
    /// them: `unify` answers "same type", and a region is deliberately not
    /// part of a type's identity. That hole returned three times in this
    /// arc — at direct joins, at array literals, and at aggregate join
    /// leaves — each time because a new call site reached for `unify` and
    /// there was nothing to stop it.
    ///
    /// So `unify` is private and the choice is now named. The debug
    /// assertion below states the safety condition exactly, and runs over
    /// the whole test corpus.
    pub(crate) fn adopt(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: &Ty,
        b: &Ty,
        cause: Option<Cause>,
    ) -> bool {
        debug_assert!(
            adoption_is_region_safe(table, a, b),
            "`adopt` was called on two types that both carry regions, which \
             silently discards every outlives obligation between them — use \
             `relate` (a value meeting a context) or `try_reborrow` (a borrow \
             meeting a borrow) instead"
        );
        self.unify(table, a, b, cause)
    }

    /// Unify two types whose regions are not this body's (an inference
    /// group's merged signatures carry none): no outlives obligation is
    /// recorded between them.
    pub(crate) fn unify_exported(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: &Ty,
        b: &Ty,
    ) -> bool {
        self.unify(table, a, b, None)
    }

    /// RELATION: a value meeting a CONTEXT that already has a type. Unifies
    /// AND relates their regions — the second half is what `adopt` cannot
    /// do and must never be asked to.
    pub(crate) fn relate(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        actual: &Ty,
        expected: &Ty,
        origin: ExprId,
        cause: Option<Cause>,
    ) -> bool {
        if !self.unify(table, actual, expected, cause) {
            return false;
        }
        self.relate_type_regions(table, actual, expected, origin);
        true
    }

    fn unify(
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
                if self.is_left_unbound(&ty) {
                    self.left_unbound.push(var);
                    return true;
                }
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
            | (Ty::Bool, Ty::Bool)
            | (Ty::Char, Ty::Char) => true,
            // Same-kind only: `u8` never unifies with `u32` — no implicit
            // mixing, an ordinary type mismatch (no conversions in v1).
            (Ty::Int(k1), Ty::Int(k2)) => k1 == k2,
            // Exact, like everything else here, and that includes the
            // SAFETY: `fn(usize)` and `unsafe fn(usize)` are two types,
            // and going from the first to the second is a directed
            // conversion applied at check sites ([`Self::relate_value`]),
            // never an equation: inferring it backwards through a variable
            // would let an `unsafe fn` arrive where a safe one was promised.
            (Ty::Fn(f1), Ty::Fn(f2)) => {
                f1.params.len() == f2.params.len()
                    && self.unify_safety(table, f1.safety, f2.safety)
                    && {
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
            // `T.&raw mut` vs `T.&raw` is FALSE — a future relaxation would
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
            // A safe borrow: same mutability, referent pointwise, and the
            // REGIONS ARE NOT COMPARED HERE. Unification decides type
            // IDENTITY; the outlives relation between two related regions
            // is a separate judgement, emitted by `InferCtx` right after a
            // successful unification (`relate_type_regions`) so that the
            // edges carry the expression to blame — which this store,
            // which sees only types, could not supply.
            //
            // Comparing them structurally instead would be wrong twice
            // over: two borrows at different regions would fail to unify
            // even when a reborrow makes them compatible, and the salsa
            // memo for a signature would churn on region numbering that
            // nothing below hir can observe.
            (
                Ty::Borrow {
                    mutable: m1,
                    referent: r1,
                    ..
                },
                Ty::Borrow {
                    mutable: m2,
                    referent: r2,
                    ..
                },
            ) => m1 == m2 && self.unify(table, &r1, &r2, cause),
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

    /// A value meeting its expected type at a check site: [`Self::relate`],
    /// except that function types relate their safety by `actual <=
    /// expected` ([`Self::coerce_safety`]). A free side meeting a function
    /// gets a safety of its own, since an `unsafe fn` position only says the
    /// value MAY be unsafe (#168).
    pub(crate) fn relate_value(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        actual: &Ty,
        expected: &Ty,
        origin: ExprId,
        cause: Option<Cause>,
    ) -> Relation {
        let related = match (
            resolve_shallow(table, actual),
            resolve_shallow(table, expected),
        ) {
            (Ty::Fn(a), Ty::Fn(b)) => {
                let target = b.with_safety(a.safety);
                self.relate(table, actual, &target, origin, cause)
                    .then_some((a.safety, b.safety))
            }
            (free, Ty::Fn(b)) if is_free(table, &free) => {
                let own = Safety::Infer(table.new_key(TyVarValue::Unknown));
                let target = b.with_safety(own);
                self.relate(table, actual, &target, origin, cause)
                    .then_some((own, b.safety))
            }
            // In a group an erroneous value leaves the position unbound
            // ([`Self::unify`]), which a fresh safety would hide.
            (ref value @ Ty::Fn(ref a), free)
                if is_free(table, &free) && !self.is_left_unbound(value) =>
            {
                // The position adopts the value's shape wholesale, so there
                // are no regions to relate; only the safety is its own.
                let own = Safety::Infer(table.new_key(TyVarValue::Unknown));
                self.unify(table, expected, &a.with_safety(own), cause)
                    .then_some((a.safety, own))
            }
            _ => {
                return if self.relate(table, actual, expected, origin, cause) {
                    Relation::Equal
                } else {
                    Relation::Mismatch
                };
            }
        };
        let Some((sub, sup)) = related else {
            return Relation::Mismatch;
        };
        if !self.coerce_safety(table, sub, sup) {
            Relation::Mismatch
        } else if resolve_safety(table, sub) == resolve_safety(table, sup) {
            Relation::Equal
        } else {
            Relation::Converted
        }
    }

    /// Two safeties that must be EQUAL — two fn types unifying.
    fn unify_safety(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        a: Safety,
        b: Safety,
    ) -> bool {
        match (resolve_safety(table, a), resolve_safety(table, b)) {
            (Safety::Infer(x), Safety::Infer(y)) => {
                table.union(x, y);
                true
            }
            (Safety::Infer(var), known) | (known, Safety::Infer(var)) => {
                self.decide_safety(table, var, known)
            }
            (a, b) => a == b,
        }
    }

    /// `actual <= expected` on the two-point lattice `safe <= unsafe`: the
    /// safe-to-unsafe conversion. Decides a variable only when the other
    /// side forces it; two open variables keep an edge, so the conversion
    /// stays directed whichever side is decided later.
    pub(crate) fn coerce_safety(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        actual: Safety,
        expected: Safety,
    ) -> bool {
        match (
            resolve_safety(table, actual),
            resolve_safety(table, expected),
        ) {
            (Safety::Safe, _) | (_, Safety::Unsafe) => true,
            (Safety::Unsafe, Safety::Safe) => false,
            (Safety::Unsafe, Safety::Infer(var)) => self.decide_safety(table, var, Safety::Unsafe),
            (Safety::Infer(var), Safety::Safe) => self.decide_safety(table, var, Safety::Safe),
            (Safety::Infer(sub), Safety::Infer(sup)) => {
                if !table.unioned(sub, sup) {
                    self.safety_edges.push((sub, sup));
                }
                true
            }
        }
    }

    /// Decide an open safety variable to `Safe` or `Unsafe`, and through the
    /// edges whatever that forces. False when an edge contradicts it: some
    /// value flows from an `unsafe fn` into a position decided safe.
    fn decide_safety(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        var: TyVar,
        decided: Safety,
    ) -> bool {
        table.union_value(var, TyVarValue::Safety(decided));
        // Only the edges touching `var` can fire, and each one that fires
        // decides its other end or fails, so it is spent.
        let root = table.find(var);
        let (fired, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.safety_edges)
            .into_iter()
            .partition(|&(sub, sup)| table.find(sub) == root || table.find(sup) == root);
        self.safety_edges = kept;
        fired.into_iter().fold(true, |ok, (sub, sup)| {
            self.coerce_safety(table, Safety::Infer(sub), Safety::Infer(sup)) && ok
        })
    }

    /// Close a body's safety constraints once its joins have solved. Each of
    /// `called` (a call outside `unsafe { }`) still open becomes safe; one already
    /// `unsafe` is `unsafe_check`'s to report. Each edge still open at both ends
    /// becomes an equation, since the edge dies with the body.
    pub(crate) fn settle_safety(
        &mut self,
        table: &mut InPlaceUnificationTable<TyVar>,
        called: Vec<Safety>,
    ) {
        for safety in called {
            if let Safety::Infer(var) = resolve_safety(table, safety) {
                self.decide_safety(table, var, Safety::Safe);
            }
        }
        for (sub, sup) in std::mem::take(&mut self.safety_edges) {
            self.unify_safety(table, Safety::Infer(sub), Safety::Infer(sup));
        }
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
        // The type every witness must have, and what decided it (the
        // causes explaining why are composed only when a witness disagrees).
        let (expected, basis) = match resolve_fully(table, &join.result) {
            // Errors are infectious and silent.
            Ty::Error => {
                for witness in &join.witnesses {
                    self.adopt(table, &witness.ty, &Ty::Error, None);
                }
                return;
            }
            // No axiom constrained the result: the witnesses — the leaf
            // branches of the whole (possibly nested) construct — vote, so
            // a plurality of leaves wins regardless of the nesting shape
            // they arrived in. Voting is *family-aware*: a `Named` leaf
            // votes for its declaration whatever its args, any other leaf
            // for its own type.
            Ty::Infer(_) => {
                let leaves: Vec<(ExprId, Ty)> = join
                    .witnesses
                    .iter()
                    .map(|witness| (witness.blame, resolve_fully(table, &witness.ty)))
                    .collect();
                let mut tally: Vec<(Family, usize)> = Vec::new();
                // A leaf with a poisoned part votes only when no clean leaf
                // does: `{error}` matches any family, but two poisoned leaves
                // of different shapes disagree.
                let clean_vote = leaves.iter().any(|(_, ty)| {
                    !ty.contains_infer() && !ty.has_open_safety() && !ty.contains_error()
                });
                for (_, ty) in &leaves {
                    // A leaf with ANY undetermined part doesn't vote: two
                    // `[{number}; 2]`s must end up unified (the tie-them-
                    // together path below), never counted as two distinct
                    // families.
                    // An open safety is undetermined too: it would fit
                    // either family, so it decides neither.
                    if ty.contains_infer()
                        || ty.has_open_safety()
                        || matches!(ty, Ty::Error)
                        || (clean_vote && ty.contains_error())
                    {
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
                    // downstream axioms (or a caller) decide. A function
                    // leaf keeps its own safety, related to the result's.
                    for witness in &join.witnesses {
                        if matches!(resolve_shallow(table, &witness.ty), Ty::Fn(_)) {
                            self.relate_value(
                                table,
                                &witness.ty,
                                &join.result,
                                witness.blame,
                                None,
                            );
                        } else {
                            self.adopt(table, &join.result, &witness.ty, None);
                        }
                    }
                    return;
                };
                if tally.iter().filter(|&&(_, n)| n == max).count() > 1 {
                    // A tie between honest conclusions: neither side is
                    // wrong, so report the disagreement itself and recover
                    // with the first witness so downstream code still checks.
                    push_tie_mismatch(&leaves, diagnostics);
                    self.adopt(table, &join.result, &join.witnesses[0].ty, None);
                    return;
                }
                let family = tally.iter().find(|&&(_, n)| n == max).unwrap().0.clone();
                let winners: Vec<(ExprId, Ty)> = leaves
                    .into_iter()
                    .filter(|(_, ty)| family_of(ty) == family)
                    .collect();
                // Identical leaves keep their exact type; anything mixed is
                // only possible in a nominal family, whose args disagree.
                // Compared region-erased, for the same reason the family
                // key is: differing regions are not differing types, so
                // they must not send this down the nominal path (which
                // would be unreachable for a borrow family).
                let first = winners[0].1.erase_regions();
                let winner = if winners.iter().all(|(_, ty)| ty.erase_regions() == first) {
                    // A voted BORROW result gets a FRESH region, not the
                    // first branch's. `if c { p } else { q }` produces a
                    // borrow good for as long as BOTH branches are — the
                    // MEET — and every branch reborrows into it below.
                    // Taking the first branch's region instead is what
                    // made two independent universals demand mutual
                    // outlives: an answer that was not only wrong but had
                    // no workaround, since writing the ruled `@a + @b`
                    // could not rescue it either.
                    // Every region in the winner is freshened, not only a
                    // top-level borrow's: an aggregate member carries its
                    // branch's regions just as directly, and privileging
                    // the first branch is the shape that was wrong for
                    // bare borrows too.
                    let winner = winners[0].1.clone();
                    if winner.contains_borrow() {
                        self.freshen_regions(&winner)
                    } else {
                        winner
                    }
                } else {
                    let Family::Named(_) = &family else {
                        unreachable!("only nominal families hold more than one type")
                    };
                    // The first member: the remaining members' args unify
                    // against it in the witness pass below (an arg
                    // disagreement is an ordinary culprit).
                    winners[0].1.clone()
                };
                (winner, Basis::Vote(winners))
            }
            // A resolved class is never re-rooted or given new causes, so
            // its root keys the same causes after the witness loop.
            concrete => match &join.result {
                Ty::Infer(var) => (concrete, Basis::Var(table.find(*var))),
                _ => (concrete, Basis::Fixed),
            },
        };

        // Witnesses that agree become "this branch has type …" hints; free
        // variables adopt the type; the rest are culprits.
        let mut siblings: Vec<Cause> = Vec::new();
        // Leaves that passed through the safe-fn → `unsafe fn` conversion.
        // Counted for exactly one thing: a witness that converted is not a
        // culprit, so the conflict below is not unanimous and must not be
        // reported as one.
        let mut coerced = 0usize;
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
                    if self.adopt(table, &witness.ty, &expected, None) {
                        if is_number {
                            siblings.push(Cause::Branch(witness.blame));
                        }
                    } else {
                        // The culprit's honest type, resolved BEFORE the var
                        // is poisoned to `{error}`: a number var reads
                        // `{number}`, any other still-free variable reads `_`.
                        let actual = resolve_finished(table, &witness.ty);
                        self.poison(table, var);
                        culprits.push((witness, actual));
                    }
                }
                // A borrow leaf meeting a borrow context is a REBORROW,
                // exactly as it is at a direct check site — the one thing
                // this loop used to skip, and the reason wrapping a borrow
                // in an `if` erased every obligation its use site would
                // have imposed. Tried before `unify`, which is
                // region-blind by design; it also lets a `&mut` branch
                // degrade into a `&` context, which a join could not do at
                // all before.
                actual
                    if matches!(actual, Ty::Borrow { .. })
                        && matches!(resolve_shallow(table, &expected), Ty::Borrow { .. }) =>
                {
                    if self.try_reborrow(table, &witness.ty, &expected, witness.blame) {
                        siblings.push(Cause::Branch(witness.blame));
                    } else {
                        culprits.push((witness, actual));
                    }
                }
                actual => {
                    // `relate` rather than `unify`: a leaf that merely
                    // CONTAINS borrows — a record, an array, a fn type —
                    // is agreed by unification, which is region-blind.
                    // Without the relate the whole laundering hole reopens
                    // one level down: a join of two
                    // `struct { x: usize.&::<@…> }` was accepted where the
                    // no-`if` control was rejected, and could return a
                    // borrow of a body-local.
                    //
                    // Invariant, both directions, because a borrow UNDER a
                    // type constructor is not being formed — it is being
                    // required to be the same type.
                    match self.relate_value(table, &witness.ty, &expected, witness.blame, None) {
                        // Hint on the tail sub-expression that produced the
                        // type, not the whole branch.
                        Relation::Equal => siblings.push(Cause::Branch(witness.blame)),
                        // A SAFE fn leaf meeting an `unsafe fn` context: a
                        // join is a check site, and every conversion a
                        // direct check performs it must perform too. Not a
                        // sibling: "this branch has type `unsafe fn(...)`"
                        // would lie about a leaf that is a safe `fn`.
                        Relation::Converted => coerced += 1,
                        Relation::Mismatch => culprits.push((witness, actual)),
                    }
                }
            }
        }

        if !culprits.is_empty() {
            let expected_causes: Vec<Cause> = match basis {
                // The winning leaves are the causes: "this branch has type
                // …" hints, wherever in the nesting those leaves sit.
                Basis::Vote(winners) => winners
                    .iter()
                    .map(|&(blame, _)| Cause::Branch(blame))
                    .collect(),
                Basis::Var(root) => self.causes.get(&root).cloned().unwrap_or_default(),
                Basis::Fixed => Vec::new(),
            };
            // Unanimous means *every* witness is a culprit of the same
            // type. A vote can't end up here unanimously (its winner comes
            // from a witness), so a unanimous conflict always has an axiom
            // behind `expected` — though not necessarily a recorded cause
            // yet.
            let unanimous = siblings.is_empty()
                && coerced == 0
                && culprits.iter().all(|(_, ty)| *ty == culprits[0].1);
            if unanimous {
                // The leaves agree with each other and only contradict the
                // context: one diagnostic on the whole (flattened)
                // construct, not a squiggle per leaf.
                let actual = resolve_finished(table, &culprits[0].1);
                for (witness, _) in &culprits {
                    self.poison_unresolved_number(table, &witness.ty);
                }
                diagnostics.push(InferenceDiagnostic::AllBranchesMismatch {
                    expr: join.expr,
                    expected: expected.clone(),
                    actual,
                    reasons: compose_reasons(&expected_causes, &[]),
                });
            } else {
                let reasons = compose_reasons(&expected_causes, &siblings);
                for (witness, actual) in culprits {
                    // Render nested unpinned numbers as `{number}` first,
                    // then poison them: this mismatch is their whole story.
                    let actual = resolve_finished(table, &actual);
                    self.poison_unresolved_number(table, &witness.ty);
                    diagnostics.push(InferenceDiagnostic::TypeMismatch {
                        expr: witness.blame,
                        expected: expected.clone(),
                        actual,
                        reasons: reasons.clone(),
                    });
                }
            }
        }
        self.adopt(table, &join.result, &expected, None);
    }
}

/// What decided a join's expected type: the winning leaves of a vote, the
/// root of the resolved result variable, or a result that was never a variable.
enum Basis {
    Vote(Vec<(ExprId, Ty)>),
    Var(TyVar),
    Fixed,
}

/// Which "family" a join leaf votes for. `Named` leaves of one declaration
/// are votes for the *same* outcome family whatever their args; every other
/// type is a family of its own.
#[derive(Clone, PartialEq)]
enum Family {
    Named(ItemLoc),
    Shape(Ty),
}

fn family_of(ty: &Ty) -> Family {
    // Keyed by declaration alone (not args): mixed-arg leaves of one
    // declaration are one family whose vote resolves to the first member's
    // args — a real arg disagreement then surfaces as an ordinary witness
    // mismatch rather than a family tie.
    match ty {
        Ty::Named(named) => Family::Named(named.decl.clone()),
        // REGION-ERASED, and that is load-bearing rather than tidy: two
        // borrows of the same referent are the same shape however long
        // each is good for. Keying the family on the region would make a
        // branch join between `@a` and `@b` report a TYPE mismatch — a
        // lifetime question answered in the wrong vocabulary, and the
        // exact confusion the erasure principle exists to prevent. The
        // regions are related invariantly where the witnesses are
        // collected, and their disagreement (if any) is reported by the
        // outlives module as what it is.
        other => Family::Shape(other.erase_regions()),
    }
}

/// An unbound plain type variable (not a number-class one).
fn is_free(table: &mut InPlaceUnificationTable<TyVar>, ty: &Ty) -> bool {
    matches!(ty, Ty::Infer(var) if matches!(table.probe_value(*var), TyVarValue::Unknown))
}

/// A safety with its variable, if decided, replaced by the decision; an
/// open one by its root.
pub(crate) fn resolve_safety(table: &mut InPlaceUnificationTable<TyVar>, safety: Safety) -> Safety {
    match safety {
        Safety::Infer(var) => match table.probe_value(var) {
            TyVarValue::Safety(decided) => decided,
            _ => Safety::Infer(table.find(var)),
        },
        known => known,
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
            TyVarValue::Safety(_) => unreachable!("a type variable holds a safety"),
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

/// Call `visit` on every still-unbound NUMBER-CLASS variable anywhere
/// inside `ty`: the variables [`Constraints::poison_unresolved_number`]
/// poisons, or leaves unbound inside a group.
fn unresolved_numbers(
    table: &mut InPlaceUnificationTable<TyVar>,
    ty: &Ty,
    visit: &mut impl FnMut(&mut InPlaceUnificationTable<TyVar>, TyVar),
) {
    match resolve_shallow(table, ty) {
        Ty::Infer(var) => {
            if matches!(table.probe_value(var), TyVarValue::UnknownNumber) {
                visit(table, var);
            }
        }
        Ty::Fn(f) => {
            for param in &f.params {
                unresolved_numbers(table, param, visit);
            }
            unresolved_numbers(table, &f.ret, visit);
        }
        Ty::RawPtr { pointee, .. } => unresolved_numbers(table, &pointee, visit),
        Ty::Borrow { referent, .. } => unresolved_numbers(table, &referent, visit),
        Ty::Array { elem, .. } => unresolved_numbers(table, &elem, visit),
        Ty::Record(rec) => {
            for (_, field) in &rec.fields {
                unresolved_numbers(table, field, visit);
            }
        }
        Ty::Named(NamedTy { args, .. }) => {
            for arg in &args {
                if let GenericArg::Ty(ty) = arg {
                    unresolved_numbers(table, ty, visit);
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
            TyVarValue::Safety(_) => unreachable!("a type variable holds a safety"),
        },
        Ty::Fn(f) => {
            let params = f.params.iter().map(|p| resolve_fully(table, p)).collect();
            let ret = resolve_fully(table, &f.ret);
            let safety = resolve_safety(table, f.safety);
            Ty::fn_type_safety(safety, params, ret)
        }
        Ty::RawPtr { mutable, pointee } => Ty::raw_ptr(*mutable, resolve_fully(table, pointee)),
        // The REGION is carried through untouched: it is not this table's
        // to resolve — regions are solved by the outlives module, over
        // constraints, not by unification.
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(*mutable, region.clone(), resolve_fully(table, referent)),
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
        // A safety nothing decided is safe: the least solution.
        Ty::Fn(f) => Ty::fn_type_with(
            f.unsafe_to_call(),
            f.params
                .iter()
                .map(|p| render_unresolved_numbers(table, p))
                .collect(),
            render_unresolved_numbers(table, &f.ret),
        ),
        Ty::Borrow {
            mutable,
            region,
            referent,
        } => Ty::borrow(
            *mutable,
            region.clone(),
            render_unresolved_numbers(table, referent),
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
                    GenericArg::Region(_) | GenericArg::Const(_) => arg.clone(),
                })
                .collect(),
        }),
        other => other.clone(),
    }
}

/// [`resolve_fully`] over a generic-argument list — also used by `infer`'s
/// finish pass for the `ResolvedVariant`s persisted outside any `Ty`
/// (`variant_of_expr`, `variant_of_pat`).
pub(crate) fn resolve_args_fully(
    table: &mut InPlaceUnificationTable<TyVar>,
    args: &[GenericArg],
) -> Vec<GenericArg> {
    args.iter()
        .map(|arg| match arg {
            GenericArg::Ty(ty) => GenericArg::Ty(resolve_fully(table, ty)),
            GenericArg::Region(_) | GenericArg::Const(_) => arg.clone(),
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
                TyVarValue::Safety(_) => unreachable!("a type variable holds a safety"),
            }
        }
        Ty::Fn(f) => f.params.iter().any(|p| occurs(table, var, p)) || occurs(table, var, &f.ret),
        Ty::RawPtr { pointee, .. } => occurs(table, var, pointee),
        // A borrow's referent is a type position like any other: without
        // this arm `?0 := ?0.&mut` binds and the type is infinite. Every
        // sibling walker in this file already recurses here.
        Ty::Borrow { referent, .. } => occurs(table, var, referent),
        Ty::Array { elem, .. } => occurs(table, var, elem),
        Ty::Record(rec) => rec.fields.iter().any(|(_, ty)| occurs(table, var, ty)),
        Ty::Named(NamedTy { args, .. }) => args.iter().any(|arg| match arg {
            GenericArg::Ty(ty) => occurs(table, var, ty),
            GenericArg::Region(_) | GenericArg::Const(_) => false,
        }),
        _ => false,
    }
}

/// The exact condition under which region-blind unification loses nothing:
/// one side adopts wholesale (an unbound variable, or the infectious
/// `Error`), or neither side has a region to lose in the first place.
///
/// Stated as a runnable predicate rather than a comment, and asserted on
/// every `adopt` — so the class of bug that returned three times in this
/// arc cannot return silently a fourth. A new seam that reaches for
/// adoption where it owes a relation fails the whole test corpus at that
/// call site, naming the two alternatives.
fn adoption_is_region_safe(table: &mut InPlaceUnificationTable<TyVar>, a: &Ty, b: &Ty) -> bool {
    let (ra, rb) = (resolve_shallow(table, a), resolve_shallow(table, b));
    let adopts = |ty: &Ty| {
        matches!(
            ty,
            Ty::Infer(_) | Ty::UnresolvedNumber | Ty::Error | Ty::Never
        )
    };
    // Identical types have nothing to relate: every pairing would be a
    // region with itself, and `@r ⊇ @r` is vacuous. This is the shape the
    // join solver's closing line has, where the result is re-bound to the
    // very type it was already resolved to. Compared FULLY resolved, as
    // that line resolves it: a shallow resolution leaves inner variables
    // in place, so a borrow whose referent is a bound variable would not
    // equal its own resolution (#26).
    if ra == rb || resolve_fully(table, a) == resolve_fully(table, b) {
        return true;
    }
    adopts(&ra) || adopts(&rb) || !ra.contains_borrow() || !rb.contains_borrow()
}
