//! Moves and initialization: M16's dataflow, one fact per named local,
//! plus one per compiler temporary that holds a linear value.
//! What one expression or pattern does with a linear is
//! [`hir::linear_check`]'s; what has happened to a local on a path is asked
//! here. A move is a whole-local read of a value that cannot be duplicated;
//! a bare borrow is exempt, since every mention of one reborrows (M07).

use base_db::Db;
use hir::body::{BindingKind, ExprData, Stmt};
use hir::infer::InferenceResult;
use hir::{BindingId, ExprId, ItemId, Resolution};
use la_arena::ArenaMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{OnceCell, RefCell};
use syntax::TextRange;

use crate::cfg::{self, BitSet, Findings, Point};
use crate::{
    BlockId, CheckTrap, LocalId, MirBody, Operand, Place, ProjElem, Rvalue, StatementKind,
    TerminatorKind,
};

/// One finding, keyed by HIR ids; ranges attach in the aggregator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MoveDiagnostic {
    /// A linear binding still holds a value where its storage ends — the
    /// leak. Squiggles the EXIT (the block, the arm, the `break`, the
    /// `return`), because that is the path that fails; the binding is the
    /// note, because that is where the obligation started.
    NotConsumed {
        binding: BindingId,
        exit: ExprId,
        /// A move that empties the binding on another path to the exit.
        moved: Option<ExprId>,
    },
    /// A linear value lowering parked in a temporary, abandoned: a
    /// `return`, `break` or `continue` cut its expression short, so it is
    /// held where the frame ends or where the expression runs again.
    /// Squiggles that exit; the expression that made the value is the note.
    ValueNotConsumed { value: ExprId, exit: ExprId },
    /// A binding used after a move took its value — which for a linear is
    /// also disposal twice. `first` is a move that reaches here, only round
    /// a loop's back edge when `earlier_iteration`.
    AlreadyConsumed {
        binding: BindingId,
        expr: ExprId,
        first: ExprId,
        earlier_iteration: bool,
    },
    /// A whole assignment over a linear binding that may still hold a
    /// value: the old value would be lost.
    AssignOverLive { binding: BindingId, target: ExprId },
    /// A deferred binding used before it is assigned: on no path
    /// (`maybe: false`) or not on every path (`maybe: true`).
    ReadUninit {
        binding: BindingId,
        expr: ExprId,
        maybe: bool,
    },
}

impl MoveDiagnostic {
    /// The expression the squiggle lands on.
    pub fn expr(&self) -> ExprId {
        match self {
            MoveDiagnostic::NotConsumed { exit, .. }
            | MoveDiagnostic::ValueNotConsumed { exit, .. } => *exit,
            MoveDiagnostic::AlreadyConsumed { expr, .. }
            | MoveDiagnostic::ReadUninit { expr, .. } => *expr,
            MoveDiagnostic::AssignOverLive { target, .. } => *target,
        }
    }

    /// The binding whose declaration explains the finding, if a binding
    /// holds the value.
    pub fn binding(&self) -> Option<BindingId> {
        match self {
            MoveDiagnostic::NotConsumed { binding, .. }
            | MoveDiagnostic::AlreadyConsumed { binding, .. }
            | MoveDiagnostic::AssignOverLive { binding, .. }
            | MoveDiagnostic::ReadUninit { binding, .. } => Some(*binding),
            MoveDiagnostic::ValueNotConsumed { .. } => None,
        }
    }

    /// The type of the value the finding is about.
    fn ty<'t>(&self, types: &'t InferenceResult) -> Option<&'t hir::Ty> {
        match self {
            MoveDiagnostic::ValueNotConsumed { value, .. } => types.type_of_expr.get(*value),
            _ => types.type_of_binding.get(self.binding()?),
        }
    }

    pub fn range(&self, source_map: &hir::BodySourceMap) -> Option<TextRange> {
        Some(source_map.node_for_expr(self.expr())?.text_range())
    }

    /// The message, naming the binding, and inside a generic body the
    /// binder that is the reason: for a use after a move, whatever the copy
    /// rule refuses; for a lost value, a parameter only when it is what
    /// made the value linear.
    pub fn message(&self, db: &dyn Db, item: ItemId<'_>) -> String {
        let body = hir::body::body(db, item);
        let types = hir::infer::infer(db, item);
        let subject = self.binding().map(|binding| &body.bindings[binding].kind);
        let ty = self.ty(types);
        let param = || ty.and_then(|ty| hir::capability::blaming_param(db, ty));
        match self {
            MoveDiagnostic::NotConsumed { .. } | MoveDiagnostic::ValueNotConsumed { .. } => {
                match param() {
                    Some(param) => hir::diag::not_consumed_param(subject, &param),
                    None => hir::diag::not_consumed(subject),
                }
            }
            MoveDiagnostic::AlreadyConsumed { .. } => ty
                .and_then(|ty| hir::capability::affine_root(db, ty))
                .and_then(|root| hir::diag::already_consumed_dup(subject, &root))
                .unwrap_or_else(|| hir::diag::already_consumed(subject)),
            MoveDiagnostic::AssignOverLive { .. } => match param() {
                Some(param) => hir::diag::assign_over_live_param(subject, &param),
                None => hir::diag::assign_over_live(subject),
            },
            MoveDiagnostic::ReadUninit { maybe, .. } => hir::diag::read_uninit(subject, *maybe),
        }
    }

    /// The companion sites: the binding's declaration (or the expression
    /// that made the value), then the move that explains the finding, when
    /// it has one.
    pub fn related(
        &self,
        db: &dyn Db,
        item: ItemId<'_>,
        source_map: &hir::BodySourceMap,
    ) -> Vec<(TextRange, String)> {
        let body = hir::body::body(db, item);
        let types = hir::infer::infer(db, item);
        let subject: Option<&BindingKind> =
            self.binding().map(|binding| &body.bindings[binding].kind);
        let at = |expr: ExprId| Some(source_map.node_for_expr(expr)?.text_range());
        let mut notes = Vec::new();
        let declared = match self {
            MoveDiagnostic::ReadUninit { .. } => hir::diag::declared_without_value(subject),
            _ => {
                let linear = self
                    .ty(types)
                    .is_some_and(|ty| !hir::capability::has_forget(db, ty));
                hir::diag::born_here(subject, linear)
            }
        };
        let born = match self {
            MoveDiagnostic::ValueNotConsumed { value, .. } => at(*value),
            _ => self
                .binding()
                .and_then(|binding| source_map.node_for_binding(binding))
                .map(|ptr| ptr.text_range()),
        };
        notes.push((born, declared));
        match self {
            MoveDiagnostic::AlreadyConsumed {
                first,
                earlier_iteration,
                ..
            } => {
                let note = if *earlier_iteration {
                    "consumed here, on an earlier iteration of the loop"
                } else {
                    "first consumed here"
                };
                notes.push((at(*first), note.to_owned()));
            }
            MoveDiagnostic::NotConsumed {
                moved: Some(moved), ..
            } => notes.push((at(*moved), "consumed here, on another path".to_owned())),
            _ => {}
        }
        notes
            .into_iter()
            .filter_map(|(range, note)| Some((range?, note)))
            .collect()
    }
}

/// The checker's findings on one item: what it reports, and where the
/// refused operations sit in the lowered MIR.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MoveFindings {
    pub diagnostics: Vec<MoveDiagnostic>,
    /// Every point a refused operation runs at, with the message of the
    /// diagnostic that refuses it: one per point, not one per report.
    pub traps: Vec<CheckTrap>,
}

/// Check every body lowered from one item, each on its own CFG.
#[salsa::tracked(returns(ref))]
pub fn move_findings<'db>(db: &'db dyn Db, item: ItemId<'db>) -> MoveFindings {
    let lowered = crate::mir_lowered(db, item);
    let hir_body = hir::body::body(db, item);
    let check = ItemCheck {
        db,
        hir_body,
        infer: hir::infer::infer(db, item),
        resolutions: hir::scopes::resolutions(db, item),
        assign_targets: OnceCell::new(),
    };
    let message = |diag: &MoveDiagnostic| diag.message(db, item);
    let mut findings = MoveFindings::default();
    for (body_id, body) in lowered.bodies.iter() {
        let mut out = Out {
            findings: Findings::new(&mut findings.diagnostics, &message),
            seen: FxHashSet::default(),
            taken_here: Vec::new(),
        };
        BodyCheck::new(&check, body).run(&mut out);
        findings.traps.extend(out.findings.traps(body_id));
    }
    findings
}

/// The checker's diagnostics on one item.
pub fn move_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> &'db [MoveDiagnostic] {
    &move_findings(db, item).diagnostics
}

/// Every assignment's value, mapped to its target: lowering blames an
/// assignment at its value, and the finding blames the place written.
fn assign_targets(body: &hir::Body) -> FxHashMap<ExprId, ExprId> {
    let mut targets = FxHashMap::default();
    for (_, data) in body.exprs.iter() {
        let ExprData::Block { stmts, .. } = data else {
            continue;
        };
        for stmt in stmts {
            if let Stmt::Assign { target, value } = stmt {
                targets.insert(*value, *target);
            }
        }
    }
    targets
}

/// What the item's bodies share.
struct ItemCheck<'a> {
    db: &'a dyn Db,
    hir_body: &'a hir::Body,
    infer: &'a InferenceResult,
    resolutions: &'a ArenaMap<ExprId, Resolution>,
    /// See [`assign_targets`]; built by the first finding that needs it.
    assign_targets: OnceCell<FxHashMap<ExprId, ExprId>>,
}

impl ItemCheck<'_> {
    /// The place a user's assignment lowered with origin `origin` writes,
    /// or `None` for a compiler write.
    fn assign_target(&self, origin: ExprId) -> Option<ExprId> {
        self.assign_targets
            .get_or_init(|| assign_targets(self.hir_body))
            .get(&origin)
            .copied()
    }

    /// The binding at the root of the place `origin` names, through a
    /// dot-call's receiver when `receivers`.
    fn root(&self, origin: ExprId, receivers: bool) -> Option<(ExprId, BindingId)> {
        let mut expr = origin;
        loop {
            match &self.hir_body.exprs[expr] {
                ExprData::NameRef(_) => {
                    return match self.resolutions.get(expr) {
                        Some(Resolution::Local(binding)) => Some((expr, *binding)),
                        _ => None,
                    };
                }
                ExprData::Borrow { place, .. } | ExprData::AddrOf { place, .. } => expr = *place,
                ExprData::Field { receiver, .. } | ExprData::Deref { receiver } => expr = *receiver,
                ExprData::Index { base, .. } => expr = *base,
                ExprData::ArrayRepeat { element, .. } if receivers => expr = *element,
                ExprData::Call {
                    callee,
                    dot_call: true,
                    ..
                } if receivers => expr = *callee,
                _ => return None,
            }
        }
    }

    /// The mention of `binding` an operation lowered from `origin` makes:
    /// the name at the root of the place it reads, borrows or writes. Falls
    /// back to `origin` where the root is not that name.
    fn mention(&self, origin: ExprId, binding: BindingId) -> ExprId {
        match self.root(origin, false) {
            Some((expr, found)) if found == binding => expr,
            _ => origin,
        }
    }
}

/// The reporting pass's sink.
struct Out<'a> {
    findings: Findings<'a, MoveDiagnostic>,
    seen: FxHashSet<MoveDiagnostic>,
    /// The locals a move took at the current point so far.
    taken_here: Vec<LocalId>,
}

impl Out<'_> {
    fn report(&mut self, point: Point, diag: MoveDiagnostic) {
        self.findings
            .report(point, &mut self.seen, diag.clone(), || Some(diag));
    }
}

/// What a tracked local owes besides holding a value when it is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owed {
    /// Exactly one consumption — a linear value.
    Once,
    /// At most one read out — a value that may not be duplicated.
    AtMostOnce,
    /// Nothing: a copyable value, tracked for its initialization only.
    Nothing,
}

#[derive(Debug, Clone, Copy)]
enum Tracked {
    /// A named local: every use is checked, against what it owes.
    Binding { binding: BindingId, owed: Owed },
    /// A compiler temporary holding a linear value that something reads,
    /// checked for a leak only. Lowering's own reads of it are right by
    /// construction, and a linear nothing reads is
    /// [`hir::linear_check`]'s to refuse.
    Temporary,
}

/// What a local may hold at a point: each flag but `halted` holds on some
/// path to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Holds {
    /// Nothing yet: never assigned, or its storage ended.
    unassigned: bool,
    value: bool,
    /// Nothing, since a move.
    moved: bool,
    /// On every path to here, it holds a trap's value or one computed from
    /// it, and nothing else is set: the code reading it never runs.
    halted: bool,
}

impl Holds {
    const VALUE: Holds = Holds {
        unassigned: false,
        value: true,
        moved: false,
        halted: false,
    };

    const HALTED: Holds = Holds {
        unassigned: false,
        value: false,
        moved: false,
        halted: true,
    };

    fn join(&mut self, other: Holds) -> bool {
        let before = *self;
        self.unassigned |= other.unassigned;
        self.value |= other.value;
        self.moved |= other.moved;
        self.halted &= other.halted;
        *self != before
    }
}

/// Per local, indexed by arena position. An untracked local carries only
/// `halted`.
type State = Vec<Holds>;

/// What a point does to one local, for the walk back to a move.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Effect {
    Takes,
    /// Gives it a value, ends its storage, or halts it.
    Ends,
    Nothing,
}

struct BodyCheck<'a> {
    check: &'a ItemCheck<'a>,
    body: &'a MirBody,
    tracked: Vec<Option<Tracked>>,
    /// Whether lowering planted a trap anywhere in the body: only then can
    /// a local be halted.
    traps: bool,
    /// Built by the first finding that needs them.
    preds: OnceCell<Vec<Vec<BlockId>>>,
    rpo: OnceCell<Vec<Option<usize>>>,
    /// The state [`Self::effect`] steps one point on.
    scratch: RefCell<State>,
}

impl<'a> BodyCheck<'a> {
    fn new(check: &'a ItemCheck<'a>, body: &'a MirBody) -> BodyCheck<'a> {
        let mut read = BitSet::new(body.locals.len());
        cfg::for_each_read(body, &mut |place| {
            read.insert(cfg::local_index(place.local));
        });
        let tracked = body
            .locals
            .iter()
            .map(|(local, data)| {
                let Some(binding) = data.binding else {
                    // The return place is the caller's to consume.
                    return (local != body.return_local()
                        && read.contains(cfg::local_index(local))
                        && !hir::capability::has_forget(check.db, &data.ty))
                    .then_some(Tracked::Temporary);
                };
                if check.hir_body.bindings[binding].is_temporary() {
                    return None;
                }
                let owed = match check.infer.type_of_binding.get(binding) {
                    Some(ty) if !hir::capability::has_forget(check.db, ty) => Owed::Once,
                    Some(hir::Ty::Borrow { .. }) | None => Owed::Nothing,
                    Some(ty) if !hir::capability::is_copyable(check.db, ty) => Owed::AtMostOnce,
                    Some(_) => Owed::Nothing,
                };
                Some(Tracked::Binding { binding, owed })
            })
            .collect();
        BodyCheck {
            check,
            body,
            tracked,
            traps: cfg::has_trap(body),
            preds: OnceCell::new(),
            rpo: OnceCell::new(),
            scratch: RefCell::new(Vec::new()),
        }
    }

    fn tracked(&self, local: LocalId) -> Option<Tracked> {
        self.tracked[cfg::local_index(local)]
    }

    fn entry_state(&self) -> State {
        let mut state: State = vec![Holds::default(); self.body.locals.len()];
        for (local, _) in self.body.locals.iter() {
            if let Some(Tracked::Binding { .. }) = self.tracked(local) {
                state[cfg::local_index(local)].unassigned = true;
            }
        }
        for &param in &self.body.params {
            state[cfg::local_index(param)] = Holds::VALUE;
        }
        state
    }

    /// Forward to a fixpoint, then one more pass over each reachable block
    /// from its entry state, reporting.
    fn run(&self, out: &mut Out) {
        let blocks = self.body.blocks.len();
        let mut entry: Vec<Option<State>> = vec![None; blocks];
        entry[cfg::block_index(self.body.entry)] = Some(self.entry_state());
        let mut work = vec![self.body.entry];
        while let Some(block) = work.pop() {
            let Some(mut state) = entry[cfg::block_index(block)].clone() else {
                continue;
            };
            for point in cfg::points_of(self.body, block) {
                self.step(point, &mut state, None);
            }
            for succ in cfg::successors(self.body, block) {
                let changed = match &mut entry[cfg::block_index(succ)] {
                    Some(old) => {
                        let mut changed = false;
                        for (holds, more) in old.iter_mut().zip(&state) {
                            changed |= holds.join(*more);
                        }
                        changed
                    }
                    slot @ None => {
                        *slot = Some(state.clone());
                        true
                    }
                };
                if changed && !work.contains(&succ) {
                    work.push(succ);
                }
            }
        }
        for (block, _) in self.body.blocks.iter() {
            let Some(mut state) = entry[cfg::block_index(block)].clone() else {
                continue;
            };
            for point in cfg::points_of(self.body, block) {
                out.taken_here.clear();
                self.step(point, &mut state, Some(&mut *out));
            }
        }
    }

    /// One point's effect on `state`, in evaluation order, reporting
    /// refusals to `out` if given.
    fn step(&self, point: Point, state: &mut State, mut out: Option<&mut Out>) {
        let origin = cfg::origin_at(self.body, point);
        let data = &self.body.blocks[point.block];
        match data.statements.get(point.index) {
            Some(stmt) => match &stmt.kind {
                StatementKind::Assign {
                    dest,
                    rvalue: value,
                } => {
                    // Asked before the reads below take a temporary.
                    let halted = self.is_halted(value, state);
                    match value {
                        Rvalue::Borrow { place, .. } | Rvalue::AddrOf { place, .. } => {
                            self.place_use(point, origin, place, false, state, &mut out);
                        }
                        value => cfg::value_operands(value, &mut |op| {
                            self.operand(point, origin, op, state, &mut out)
                        }),
                    }
                    if !dest.projection.is_empty() {
                        // Written into or through: the root must hold its
                        // value.
                        self.place_use(point, origin, dest, true, state, &mut out);
                    } else if halted {
                        self.halt(dest.local, state);
                    } else {
                        self.assign(point, origin, dest.local, state, &mut out);
                    }
                }
                StatementKind::StorageDead { local, .. } => {
                    self.storage_end(point, origin, *local, state, &mut out);
                    if self.tracked(*local).is_some() {
                        state[cfg::local_index(*local)] = Holds {
                            unassigned: true,
                            ..Holds::default()
                        };
                    }
                }
            },
            None => match &data.terminator.kind {
                TerminatorKind::SwitchBool { discr, .. }
                | TerminatorKind::SwitchVariant { discr, .. } => {
                    self.operand(point, origin, discr, state, &mut out);
                }
                TerminatorKind::Call {
                    callee, args, dest, ..
                } => {
                    let halted = std::iter::once(callee)
                        .chain(args)
                        .any(|op| self.reads_halted(op, state));
                    self.operand(point, origin, callee, state, &mut out);
                    for arg in args {
                        self.operand(point, origin, arg, state, &mut out);
                    }
                    match halted {
                        true => self.halt(*dest, state),
                        false => self.assign(point, origin, *dest, state, &mut out),
                    }
                }
                // A trap ends its path at run time, so the binding at the
                // root of the place its origin names is halted, as is its
                // `dest`.
                TerminatorKind::Trap { dest, .. } => {
                    self.halt(*dest, state);
                    if let Some((_, binding)) = self.check.root(origin, true) {
                        for (local, _) in self.body.locals.iter() {
                            if let Some(Tracked::Binding { binding: b, .. }) = self.tracked(local)
                                && b == binding
                            {
                                self.halt(local, state);
                            }
                        }
                    }
                }
                // The frame's storage end: every local still holding a
                // linear leaks here.
                TerminatorKind::Return => {
                    for (local, _) in self.body.locals.iter() {
                        self.storage_end(point, origin, local, state, &mut out);
                    }
                }
                TerminatorKind::Goto { .. }
                | TerminatorKind::ConstTrap { .. }
                | TerminatorKind::Unreachable => {}
            },
        }
    }

    /// Whether `value` is computed from a halted local.
    fn is_halted(&self, value: &Rvalue, state: &State) -> bool {
        let mut halted = false;
        if self.traps {
            cfg::value_operands(value, &mut |op| halted |= self.reads_halted(op, state));
        }
        halted
    }

    fn reads_halted(&self, op: &Operand, state: &State) -> bool {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                self.traps
                    && place.projection.is_empty()
                    && state[cfg::local_index(place.local)].halted
            }
            Operand::Const(_) => false,
        }
    }

    /// Marks `local` halted, clearing what it may hold: the path never
    /// runs on, so nothing is reported of it.
    fn halt(&self, local: LocalId, state: &mut State) {
        state[cfg::local_index(local)] = Holds::HALTED;
    }

    /// Whether `op` takes `local`'s value: a whole-local read of a value
    /// that cannot be duplicated, whichever operand lowering chose.
    fn takes(&self, op: &Operand, local: LocalId) -> bool {
        let (place, moved) = match op {
            Operand::Copy(place) => (place, false),
            Operand::Move(place) => (place, true),
            Operand::Const(_) => return false,
        };
        if place.local != local {
            return false;
        }
        match self.tracked(local) {
            Some(Tracked::Binding { owed, .. }) => {
                place.projection.is_empty()
                    && (moved || matches!(owed, Owed::Once | Owed::AtMostOnce))
            }
            // A temporary's value goes wherever lowering reads it, a field
            // at a time when a pattern takes it apart.
            Some(Tracked::Temporary) => true,
            None => false,
        }
    }

    fn operand(
        &self,
        point: Point,
        origin: ExprId,
        op: &Operand,
        state: &mut State,
        out: &mut Option<&mut Out>,
    ) {
        let place = match op {
            Operand::Copy(place) | Operand::Move(place) => place,
            Operand::Const(_) => return,
        };
        self.place_use(point, origin, place, false, state, out);
        // Taken even where the read was refused, so one mistake is
        // reported once.
        if self.takes(op, place.local) {
            state[cfg::local_index(place.local)] = Holds {
                moved: true,
                ..Holds::default()
            };
            if let Some(out) = out {
                out.taken_here.push(place.local);
            }
        }
    }

    /// A use of `place`: its root must hold a value, and its index operands
    /// read. `written` is a write into or through it, which the finding
    /// blames at the assignment's target.
    fn place_use(
        &self,
        point: Point,
        origin: ExprId,
        place: &Place,
        written: bool,
        state: &mut State,
        out: &mut Option<&mut Out>,
    ) {
        for elem in &place.projection {
            if let ProjElem::Index(op) = elem {
                self.operand(point, origin, op, state, out);
            }
        }
        let Some(Tracked::Binding { binding, .. }) = self.tracked(place.local) else {
            return;
        };
        let Some(out) = out else {
            return;
        };
        let holds = state[cfg::local_index(place.local)];
        if !holds.unassigned && !holds.moved {
            return;
        }
        let at = match written {
            true => self.check.assign_target(origin).unwrap_or(origin),
            false => origin,
        };
        let expr = self.check.mention(at, binding);
        let diag = if holds.unassigned {
            MoveDiagnostic::ReadUninit {
                binding,
                expr,
                maybe: holds.value || holds.moved,
            }
        } else {
            let (first, earlier_iteration) = if out.taken_here.contains(&place.local) {
                (expr, false)
            } else {
                match self.find_move(point, place.local) {
                    Some((at, round)) => (self.mention_at(at, binding), round),
                    None => (expr, false),
                }
            };
            MoveDiagnostic::AlreadyConsumed {
                binding,
                expr,
                first,
                earlier_iteration,
            }
        };
        out.report(point, diag);
    }

    /// A whole-local write. Over a linear that may still hold a value, a
    /// user's assignment loses it, and so does lowering's when it refills a
    /// temporary an earlier iteration left full.
    fn assign(
        &self,
        point: Point,
        origin: ExprId,
        local: LocalId,
        state: &mut State,
        out: &mut Option<&mut Out>,
    ) {
        let holds = &mut state[cfg::local_index(local)];
        let Some(tracked) = self.tracked(local) else {
            holds.halted = false;
            return;
        };
        if let Some(out) = out
            && holds.value
        {
            let diag = match tracked {
                Tracked::Binding {
                    binding,
                    owed: Owed::Once,
                } => self
                    .check
                    .assign_target(origin)
                    .map(|target| MoveDiagnostic::AssignOverLive { binding, target }),
                Tracked::Binding { .. } => None,
                Tracked::Temporary => self.abandoned(point, local, origin),
            };
            if let Some(diag) = diag {
                out.report(point, diag);
            }
        }
        *holds = Holds::VALUE;
    }

    /// The end of `local`'s storage: a linear still holding a value leaks.
    fn storage_end(
        &self,
        point: Point,
        exit: ExprId,
        local: LocalId,
        state: &State,
        out: &mut Option<&mut Out>,
    ) {
        let (Some(tracked), Some(out)) = (self.tracked(local), out) else {
            return;
        };
        if !state[cfg::local_index(local)].value {
            return;
        }
        let diag = match tracked {
            Tracked::Binding {
                binding,
                owed: Owed::Once,
            } => MoveDiagnostic::NotConsumed {
                binding,
                exit,
                moved: self
                    .find_move(point, local)
                    .map(|(at, _)| self.mention_at(at, binding)),
            },
            Tracked::Binding { .. } => return,
            Tracked::Temporary => match self.abandoned(point, local, exit) {
                Some(diag) => diag,
                None => return,
            },
        };
        out.report(point, diag);
    }

    /// The finding for the temporary `local`, holding its value at
    /// `point`: blamed at the exit that cut its expression short, else at
    /// `fallback`, with the write that filled it on a path to `point`.
    fn abandoned(&self, point: Point, local: LocalId, fallback: ExprId) -> Option<MoveDiagnostic> {
        let defs = cfg::defs_of(self.body, local);
        let filled = |at: Point, _| defs.iter().any(|&(def, _)| def == at);
        let def = (self.walk_back(point, local, false, filled))
            .or_else(|| self.walk_back(point, local, true, filled))?;
        Some(MoveDiagnostic::ValueNotConsumed {
            value: cfg::origin_at(self.body, def),
            exit: self.abandoned_at(point, local).unwrap_or(fallback),
        })
    }

    fn mention_at(&self, point: Point, binding: BindingId) -> ExprId {
        self.check
            .mention(cfg::origin_at(self.body, point), binding)
    }

    /// A move of `local` that reaches `to`, walking back from it, and
    /// whether only a walk round a loop's back edge finds one.
    fn find_move(&self, to: Point, local: LocalId) -> Option<(Point, bool)> {
        (self
            .walk_back_to_move(to, local, false)
            .map(|at| (at, false)))
        .or_else(|| self.walk_back_to_move(to, local, true).map(|at| (at, true)))
    }

    fn walk_back_to_move(&self, to: Point, local: LocalId, round_loops: bool) -> Option<Point> {
        self.walk_back(to, local, round_loops, |_, effect| effect == Effect::Takes)
    }

    /// The `break`, `continue` or `return` that carried a value in the
    /// temporary `local` out of the expression meant to consume it, on a
    /// path to the frame's end at `to`.
    fn abandoned_at(&self, to: Point, local: LocalId) -> Option<ExprId> {
        let exits = |point: Point, _| {
            matches!(
                self.check.hir_body.exprs[cfg::origin_at(self.body, point)],
                ExprData::Break { .. } | ExprData::Continue | ExprData::Return { .. }
            )
        };
        let point = (self.walk_back(to, local, false, exits))
            .or_else(|| self.walk_back(to, local, true, exits))?;
        Some(cfg::origin_at(self.body, point))
    }

    /// The first point walking back from `to` that is `found`, without
    /// walking past a point that refills, ends or halts `local`; round a
    /// loop's back edge only when `round_loops`.
    fn walk_back(
        &self,
        to: Point,
        local: LocalId,
        round_loops: bool,
        found: impl Fn(Point, Effect) -> bool,
    ) -> Option<Point> {
        let preds = self.preds.get_or_init(|| predecessors(self.body));
        let rpo = self.rpo.get_or_init(|| cfg::reverse_postorder(self.body));
        let mut seen = BitSet::new(self.body.blocks.len());
        let mut work = vec![to];
        'walk: while let Some(from) = work.pop() {
            for index in (0..from.index).rev() {
                let point = Point {
                    block: from.block,
                    index,
                };
                let effect = self.effect(point, local);
                if found(point, effect) {
                    return Some(point);
                }
                if effect == Effect::Ends {
                    continue 'walk;
                }
            }
            let Some(to_order) = rpo[cfg::block_index(from.block)] else {
                continue;
            };
            for &pred in &preds[cfg::block_index(from.block)] {
                let Some(pred_order) = rpo[cfg::block_index(pred)] else {
                    continue;
                };
                if (round_loops || pred_order < to_order) && seen.insert(cfg::block_index(pred)) {
                    work.push(Point {
                        block: pred,
                        index: self.body.blocks[pred].statements.len() + 1,
                    });
                }
            }
        }
        None
    }

    /// What `point` does to `local`, read off [`Self::step`]: whether it
    /// takes the value, or ends the walk back by refilling, ending or
    /// halting the local.
    fn effect(&self, point: Point, local: LocalId) -> Effect {
        let before = Holds {
            unassigned: true,
            value: true,
            moved: true,
            halted: false,
        };
        let mut state = self.scratch.borrow_mut();
        state.clear();
        state.resize(self.body.locals.len(), Holds::default());
        state[cfg::local_index(local)] = before;
        self.step(point, &mut state, None);
        match state[cfg::local_index(local)] {
            after if after == before => Effect::Nothing,
            Holds {
                moved: true,
                unassigned: false,
                value: false,
                halted: false,
            } => Effect::Takes,
            _ => Effect::Ends,
        }
    }
}

fn predecessors(body: &MirBody) -> Vec<Vec<BlockId>> {
    let mut preds = vec![Vec::new(); body.blocks.len()];
    for (block, _) in body.blocks.iter() {
        for succ in cfg::successors(body, block) {
            preds[cfg::block_index(succ)].push(block);
        }
    }
    preds
}

#[cfg(test)]
mod tests;
