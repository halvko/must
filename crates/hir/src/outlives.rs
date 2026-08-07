//! The outlives module: region inference and the universal-region check.
//!
//! This is the FIRST shipped module of the borrow checker, and it is
//! deliberately the whole of one stage rather than a slice of several. Its
//! entire output is a list of diagnostics — nothing it computes is consumed
//! by lowering, layout, selection or identity, because the
//! specialization-soundness law says regions may reject programs and never
//! select behaviors. That makes the pass boundary unusually clean: delete
//! this module and the language still compiles and runs exactly the same
//! programs, minus the rejections. (X07, X18)
//!
//! # What it does
//!
//! Inference has already RECORDED, per body, every outlives obligation the
//! body incurs ([`crate::constraint::RegionConstraint`]) — reborrows,
//! `&mut`→`&` degradations, invariant relations, and the callee bounds
//! instantiated at each call site. This module does not re-derive any of
//! that from a region-erased later IR; it consumes the record (X10:
//! record, don't re-derive).
//!
//! Given those edges it computes each region's VALUE and checks two things:
//!
//! 1. **The universal-region check.** A signature's regions are UNIVERSAL —
//!    parameters, opaque to the body, chosen by the caller. If the body
//!    forces `@a` to cover `@b` and the signature never said `@a: @b`, the
//!    body is asking the caller for a guarantee it never made. Rejected,
//!    blamed at the obligation's origin.
//! 2. **The escape check.** A borrow of a body-local mints a region. If
//!    that region has to reach a universal's end, the borrow outlives the
//!    storage it points at — the local is gone by then.
//!
//! # What it does NOT do, and why that is not a gap
//!
//! It does not build a point set (which locations each region covers) and
//! it does not read the MIR CFG. Under NLL a region's value is
//! `points ∪ free regions`, propagated by subset — and subset propagation
//! can never turn a point element into a free-region element. So the
//! free-region half of every region's value, which is *exactly* what both
//! checks above ask about, is computed here precisely, not approximately.
//!
//! Points earn their keep for the OTHER half of borrow checking — how long
//! each loan is live, and therefore which pairs of loans conflict. Static
//! loan liveness is not built yet; until it lands, the interpreter detects
//! exclusivity violations dynamically.
//!
//! The CFG is not consulted here, because for the two checks this module
//! owns it cannot change an answer. The seam is preserved rather than
//! skipped: static loan liveness, when it lands, adds point elements to
//! each region's value and a liveness-seeding pass over MIR, and nothing
//! here changes shape.

use base_db::Db;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ItemId;
use crate::body::ExprData;
use crate::constraint::{RegionConstraint, RegionConstraintReason};
use crate::item_tree::GenericParamKind;
use crate::scopes::Resolution;
use crate::ty::{Region, RegionVar};

/// One finding. Carries the expression to blame and everything the message
/// needs, so rendering (ranges, related locations) stays in the diagnostics
/// pass and the analysis stays free of spans — the split that keeps a
/// borrow checker from fusing with its own error reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlivesDiagnostic {
    /// A signature's regions were forced into a relation the signature
    /// does not declare.
    UndeclaredOutlives {
        expr: crate::body::ExprId,
        /// The region that would have to be the longer one.
        longer: String,
        /// The region it would have to cover.
        shorter: String,
        reason: RegionConstraintReason,
    },
    /// A borrow of a local escaped the body.
    BorrowEscapes {
        expr: crate::body::ExprId,
        /// The region the borrow was forced to reach.
        region: String,
    },
}

impl OutlivesDiagnostic {
    pub fn expr(&self) -> crate::body::ExprId {
        match self {
            OutlivesDiagnostic::UndeclaredOutlives { expr, .. }
            | OutlivesDiagnostic::BorrowEscapes { expr, .. } => *expr,
        }
    }

    pub fn message(&self) -> String {
        match self {
            OutlivesDiagnostic::UndeclaredOutlives {
                longer,
                shorter,
                reason,
                ..
            } => {
                let because = match reason {
                    // Named as the operation the user performed, not as
                    // the machinery it went through: under
                    // reborrow-at-every-use a plain mention of a borrow IS
                    // a borrow site, and a message about "reborrowing"
                    // would point at code containing no visible borrow.
                    RegionConstraintReason::Reborrow => {
                        "using this borrow where a longer-lived one is expected"
                    }
                    RegionConstraintReason::Degradation => {
                        "using this `.&mut` where a longer-lived `.&` is expected"
                    }
                    RegionConstraintReason::Invariance => {
                        "requiring these two borrows to be the same type"
                    }
                    // Exhaustiveness only: under elision both ends of a
                    // `CalleeBound` edge are the call's own fresh
                    // existentials, so no rigid region ever violates it
                    // here — the blame lands on the reborrow or return
                    // that carried the element across (see the variant's
                    // doc in `constraint.rs`).
                    RegionConstraintReason::CalleeBound => "calling this function",
                    // The user wrote a `match`, not a borrow — name that,
                    // for the same reason `Reborrow` names the use rather
                    // than the machinery.
                    RegionConstraintReason::Projection => {
                        "matching this borrow to bind its payloads"
                    }
                };
                // A MEET on the shorter side needs only ONE member
                // covered — it is the overlap of its members, so covering
                // any of them covers it. Saying `add @a: @b + @c` would
                // ask for more than the rule does.
                if shorter.contains(" + ") {
                    let options = shorter
                        .split(" + ")
                        .map(|member| format!("`{longer}: {member}`"))
                        .collect::<Vec<_>>()
                        .join(" or ");
                    return format!(
                        "{because} needs `{longer}` to outlive the overlap of \
                         `{shorter}`, which this signature does not declare; \
                         declaring {options} would be enough"
                    );
                }
                format!(
                    "{because} needs `{longer}` to outlive `{shorter}`, \
                     which this signature does not declare; add `{longer}: {shorter}` \
                     to the binder"
                )
            }
            OutlivesDiagnostic::BorrowEscapes { region, .. } => format!(
                "borrowed value does not live long enough: this borrows a local, \
                 but the borrow has to last for `{region}`, which outlives the body"
            ),
        }
    }
}

/// One element a region's value may contain. This stage has exactly one
/// kind: the END of a universal region — the point in the caller past
/// which that region's guarantee no longer holds. Static loan liveness,
/// when it lands, adds `Point(BlockId, usize)` here and seeds it from MIR
/// liveness; every consumer below is written as a set operation so that
/// addition changes no logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RegionElement {
    /// `end(@a)`, identified by the universal's binder index.
    End(u32),
}

/// The set of elements a region covers. Grows monotonically; never shrinks.
type RegionValue = FxHashSet<RegionElement>;

/// One edge into a region's value, carrying the obligation it came from —
/// so that when an element turns out to be illegal, the message can point
/// at the expression that PUT IT THERE.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    kind: EdgeKind,
    origin: crate::body::ExprId,
    reason: RegionConstraintReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EdgeKind {
    /// `Values(sup) ⊇ Values(sub)` — the ordinary outlives edge.
    From(Node),
    /// `Values(sup) ⊇ ⋂ Values(parts)` — a join in the SHORTER position,
    /// which names the overlap of its members rather than each of them.
    Meet(Vec<Node>),
}

/// A node in the region graph: either one of the signature's universals or
/// one of the body's existential variables. Numbered so a single dense
/// table holds both — universals first, so a node index below
/// `universal_count` IS a universal's binder index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Node(u32);

/// The region graph of one body, plus the solver.
struct Solver {
    universal_count: u32,
    /// `universal_count + region_count` nodes.
    values: Vec<RegionValue>,
    /// `sup -> sources`: each edge says what `Values(sup)` must contain.
    edges: Vec<Vec<Edge>>,
    /// Display name per universal, sigil included.
    universal_names: Vec<String>,
    /// The declared `@a: @b` relation, transitively closed — what the
    /// signature promises, and therefore what the check measures against.
    declared: Vec<FxHashSet<u32>>,
    /// Per node, which obligation first put each element into its value.
    ///
    /// This is what makes blame land on the culprit. Reporting the first
    /// edge that merely HAS this region as its longer side points at
    /// whichever statement happened to come first — routinely an innocent
    /// one — and attributes the culprit's reason to it too, so a reborrow
    /// gets described as an invariance requirement.
    introduced: Vec<FxHashMap<RegionElement, (crate::body::ExprId, RegionConstraintReason)>>,
    /// `sup ⊇ meet(members)` obligations, checked separately — see
    /// [`Solver::add`]'s join arm for why propagation alone cannot carry
    /// them.
    meet_checks: Vec<(Node, Vec<Node>, crate::body::ExprId, RegionConstraintReason)>,
}

impl Solver {
    fn new(
        universal_names: Vec<String>,
        declared_bounds: Vec<Vec<u32>>,
        region_count: u32,
    ) -> Self {
        let universal_count = universal_names.len() as u32;
        let total = (universal_count + region_count) as usize;
        let mut values = vec![RegionValue::default(); total];
        // A universal covers its own end, and the ends of everything it
        // was DECLARED to outlive. That seeding is what makes the check
        // below a comparison against the signature rather than against
        // nothing.
        let declared = transitive_closure(&declared_bounds);
        for index in 0..universal_count {
            values[index as usize].insert(RegionElement::End(index));
            for &bound in &declared[index as usize] {
                values[index as usize].insert(RegionElement::End(bound));
            }
        }
        Solver {
            universal_count,
            values,
            edges: vec![Vec::new(); total],
            introduced: vec![FxHashMap::default(); total],
            meet_checks: Vec::new(),
            universal_names,
            declared,
        }
    }

    /// The graph node a region denotes, or `None` for a region that is not
    /// a single solvable node (an error, or the erased region).
    fn node(&self, region: &Region) -> Option<Node> {
        match region {
            Region::Param { index, .. } if *index < self.universal_count => Some(Node(*index)),
            Region::Var(RegionVar(var)) => Some(Node(self.universal_count + var)),
            _ => None,
        }
    }

    /// Record `sup ⊇ sub`. A JOIN on either side decomposes: `@a + @b` is
    /// outlived by both members, so an obligation against the join is an
    /// obligation against each — which is what makes `+` read as
    /// conjunction rather than as a fresh region needing its own solving.
    fn add(
        &mut self,
        sup: &Region,
        sub: &Region,
        origin: crate::body::ExprId,
        reason: RegionConstraintReason,
    ) {
        match (sup, sub) {
            // A join on the LONGER side decomposes. `@a + @b` names the
            // region both outlive, so covering something means BOTH cover
            // it — which is the join reading as conjunction, applied to
            // the obligation rather than to the spelling.
            (Region::Join(parts), sub) => {
                for part in parts {
                    self.add(part, sub, origin, reason);
                }
            }
            // A join on the SHORTER side is a MEET, and decomposing it
            // there would be wrong in the expensive direction: it would
            // demand `sup` cover each member separately when covering
            // their overlap is all that was asked. One edge carrying the
            // intersection says exactly what was meant.
            (sup, Region::Join(parts)) => {
                let Some(sup_node) = self.node(sup) else {
                    return;
                };
                let nodes: Vec<Node> = parts.iter().filter_map(|part| self.node(part)).collect();
                if nodes.len() != parts.len() {
                    return;
                }
                // Two obligations, because a meet is two things at once.
                //
                // PROPAGATION: `Values(sup) ⊇ ⋂ Values(parts)`, the honest
                // set reading, which grows `sup` by whatever the members
                // genuinely share.
                //
                // And a CHECK the set model cannot express. For two
                // unrelated universals the intersection of the element
                // sets is EMPTY — `{End(@b)} ∩ {End(@c)} = ∅` — so
                // propagation alone demands nothing, while the region
                // `@b + @c` is emphatically not empty: the caller picks
                // both independently and the meet is whatever they overlap
                // in. The only way a universal can be known to cover that
                // overlap is to cover one of its members, which the
                // signature has to have said. So: at least one member must
                // be declared-outlived.
                //
                // Stricter than the set model can express, on a seam the
                // module already owns: the principled fix is a
                // `RegionElement` that can represent a meet, which is a
                // region-value change and belongs with the point elements
                // static loan liveness adds.
                self.meet_checks
                    .push((sup_node, nodes.clone(), origin, reason));
                self.edges[sup_node.0 as usize].push(Edge {
                    kind: EdgeKind::Meet(nodes),
                    origin,
                    reason,
                });
            }
            (sup, sub) => {
                if let (Some(sup), Some(sub)) = (self.node(sup), self.node(sub)) {
                    self.edges[sup.0 as usize].push(Edge {
                        kind: EdgeKind::From(sub),
                        origin,
                        reason,
                    });
                }
            }
        }
    }

    /// Propagate to a fixpoint: `Values(sup) ∪= Values(sub)` along every
    /// edge until nothing grows.
    ///
    /// A worklist rather than SCC condensation, deliberately. Invariance
    /// emits both directions at every relation site, so the graph is dense
    /// with 2-cycles — and a 2-cycle under set-union propagation simply
    /// converges, at the same answer condensation would give. Keeping the
    /// edges DIRECTED (never merging the two regions into an equivalence
    /// class) is the load-bearing part: the day a relation becomes
    /// one-directional, only which edge is emitted changes. Condensation is
    /// a speed optimization to make when a body is large enough to want
    /// one.
    fn solve(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;
            for sup in 0..self.edges.len() {
                for index in 0..self.edges[sup].len() {
                    let (origin, reason) =
                        (self.edges[sup][index].origin, self.edges[sup][index].reason);
                    let source: RegionValue = match &self.edges[sup][index].kind {
                        EdgeKind::From(sub) => self.values[sub.0 as usize].clone(),
                        EdgeKind::Meet(nodes) => {
                            let mut iter = nodes.iter();
                            match iter.next() {
                                None => RegionValue::default(),
                                Some(first) => {
                                    iter.fold(self.values[first.0 as usize].clone(), |acc, node| {
                                        acc.intersection(&self.values[node.0 as usize])
                                            .copied()
                                            .collect()
                                    })
                                }
                            }
                        }
                    };
                    let additions: Vec<RegionElement> = source
                        .iter()
                        .filter(|elem| !self.values[sup].contains(*elem))
                        .copied()
                        .collect();
                    if additions.is_empty() {
                        continue;
                    }
                    for elem in &additions {
                        // First writer wins: the obligation that actually
                        // put this element here is the one to blame.
                        self.introduced[sup]
                            .entry(*elem)
                            .or_insert((origin, reason));
                    }
                    self.values[sup].extend(additions);
                    changed = true;
                }
            }
        }
    }

    /// Whether the signature declares `longer: shorter` (transitively, and
    /// reflexively).
    fn declares(&self, longer: u32, shorter: u32) -> bool {
        longer == shorter || self.declared[longer as usize].contains(&shorter)
    }

    /// The lowest-numbered universal whose end this region has to cover, if
    /// any — the escape check's whole question.
    ///
    /// A JOIN is decomposed rather than skipped. `@b + @c` is not a node
    /// (it is the overlap of two), but a borrow that has to survive that
    /// overlap has to survive into the caller just the same, so a member
    /// reaching a universal's end is enough. Skipping it instead was a hole
    /// with no ceremony at all: a function returning `usize.&::<@b + @c>`
    /// could hand back a borrow of its own local and check clean.
    fn reaches_universal(&self, region: &Region) -> Option<u32> {
        match region {
            Region::Join(parts) => parts
                .iter()
                .filter_map(|part| self.reaches_universal(part))
                .min(),
            region => {
                let node = self.node(region)?;
                self.values[node.0 as usize]
                    .iter()
                    .map(|RegionElement::End(index)| *index)
                    .min()
            }
        }
    }
}

/// Close a declared-bounds relation under transitivity — `@a: @b` and
/// `@b: @c` means the caller has already guaranteed `@a: @c`, so the body
/// may use it without saying so again.
fn transitive_closure(bounds: &[Vec<u32>]) -> Vec<FxHashSet<u32>> {
    let mut closed: Vec<FxHashSet<u32>> = bounds
        .iter()
        .map(|list| list.iter().copied().collect())
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for index in 0..closed.len() {
            let reachable: Vec<u32> = closed[index].iter().copied().collect();
            for step in reachable {
                let further: Vec<u32> = closed
                    .get(step as usize)
                    .map(|set| set.iter().copied().collect())
                    .unwrap_or_default();
                for target in further {
                    if closed[index].insert(target) {
                        changed = true;
                    }
                }
            }
        }
    }
    closed
}

/// Borrow-check one body's regions. A pure per-body query: it reads this
/// item's inference result, its binder and its body, and nothing from any
/// other body. That isolation is enforced by the query's signature rather
/// than by convention — no cross-body facts can enter because there is
/// nowhere for them to come from.
#[salsa::tracked(returns(ref))]
pub fn outlives_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<OutlivesDiagnostic> {
    let generics = crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    // Universals keep their BINDER index, so a `Region::Param`'s index is
    // its node number directly. Non-region params leave a hole, which
    // nothing ever names.
    let mut universal_names: Vec<String> = Vec::with_capacity(generics.len());
    let mut declared_bounds: Vec<Vec<u32>> = Vec::with_capacity(generics.len());
    let by_name: FxHashMap<&str, u32> = generics
        .iter()
        .enumerate()
        .filter(|(_, param)| matches!(param.kind, GenericParamKind::Region))
        .map(|(index, param)| (param.name.as_str(), index as u32))
        .collect();
    for param in &generics {
        universal_names.push(param.name.clone());
        declared_bounds.push(
            param
                .outlives
                .iter()
                .filter_map(|name| by_name.get(name.as_str()).copied())
                .collect(),
        );
    }

    let infer = crate::infer::infer(db, item);
    let mut solver = Solver::new(universal_names, declared_bounds, infer.region_count);
    for RegionConstraint {
        sup,
        sub,
        origin,
        reason,
    } in &infer.region_constraints
    {
        solver.add(sup, sub, *origin, *reason);
    }
    solver.solve();

    let mut diagnostics = Vec::new();

    // 1. The universal-region check. Read off the constraints rather than
    //    off the solved values, so blame lands on the obligation the user's
    //    code created instead of on a solver fact with no source.
    for universal in 0..solver.universal_count {
        let mut offending: Vec<RegionElement> =
            solver.values[universal as usize].iter().copied().collect();
        offending.sort_unstable();
        for element in offending {
            let RegionElement::End(shorter) = element;
            if solver.declares(universal, shorter) {
                continue;
            }
            // Blamed at the obligation that INTRODUCED this element, with
            // that obligation's reason — not at whichever edge happens to
            // mention this region first, which is routinely an innocent
            // statement and routinely carries the wrong reason with it.
            let Some(&(origin, reason)) = solver.introduced[universal as usize].get(&element)
            else {
                continue;
            };
            diagnostics.push(OutlivesDiagnostic::UndeclaredOutlives {
                expr: origin,
                longer: solver.universal_names[universal as usize].clone(),
                shorter: solver.universal_names[shorter as usize].clone(),
                reason,
            });
        }
    }

    // 1b. The MEET obligations, which propagation cannot carry (see
    //     `Solver::add`). A universal covering `@b + @c` must be declared
    //     to outlive at least one member — that is the only way it can be
    //     KNOWN to cover their overlap, since the caller picks all three
    //     independently.
    for (sup_node, members, origin, reason) in solver.meet_checks.clone() {
        if sup_node.0 >= solver.universal_count {
            continue;
        }
        if members.iter().any(|member| {
            member.0 < solver.universal_count && solver.declares(sup_node.0, member.0)
        }) {
            continue;
        }
        let longer = solver.universal_names[sup_node.0 as usize].clone();
        let shorter = members
            .iter()
            .filter(|member| member.0 < solver.universal_count)
            .map(|member| solver.universal_names[member.0 as usize].clone())
            .collect::<Vec<_>>()
            .join(" + ");
        if shorter.is_empty() {
            continue;
        }
        let already = diagnostics.iter().any(|diag| {
            matches!(
                diag,
                OutlivesDiagnostic::UndeclaredOutlives { longer: l, shorter: sh, .. }
                    if *l == longer && *sh == shorter
            )
        });
        if already {
            continue;
        }
        diagnostics.push(OutlivesDiagnostic::UndeclaredOutlives {
            expr: origin,
            longer,
            shorter,
            reason,
        });
    }

    // 2. The escape check: a borrow whose place roots in a body-local, with
    //    a region forced to reach some universal's end. The local's storage
    //    is gone by then — this is "borrowed value does not live long
    //    enough", caught at the borrow rather than at the return, because
    //    the borrow is the operation that cannot be honored.
    let body = crate::body::body(db, item);
    let resolutions = crate::scopes::resolutions(db, item);
    for (expr, data) in body.exprs.iter() {
        let ExprData::Borrow { place, .. } = data else {
            continue;
        };
        if !roots_in_local(body, resolutions, *place) {
            continue;
        }
        let Some(crate::ty::Ty::Borrow { region, .. }) = infer.type_of_expr.get(expr) else {
            continue;
        };
        // A borrow taken AT a universal region by explicit annotation is
        // the user asking for exactly this; the reach is still an error,
        // and naming the region they wrote is the clearest message.
        let Some(first) = solver.reaches_universal(region) else {
            continue;
        };
        diagnostics.push(OutlivesDiagnostic::BorrowEscapes {
            expr,
            region: solver.universal_names[first as usize].clone(),
        });
    }

    diagnostics
}

/// Whether a borrow's place bottoms out in a local BINDING of this body —
/// the storage that dies when the body returns.
///
/// A deref-rooted place does not count: `r.*.field.&mut` borrows through
/// `r`, so the storage it names belongs to whoever `r` points at, and the
/// reborrow edge already relates the two regions. A `static` does not count
/// either — its allocation outlives every body.
fn roots_in_local(
    body: &crate::body::Body,
    resolutions: &la_arena::ArenaMap<crate::body::ExprId, Resolution>,
    place: crate::body::ExprId,
) -> bool {
    let mut root = place;
    loop {
        match &body.exprs[root] {
            ExprData::Field { receiver, .. } => root = *receiver,
            ExprData::Index { base, .. } => root = *base,
            _ => break,
        }
    }
    matches!(resolutions.get(root), Some(Resolution::Local(_)))
}
