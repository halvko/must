//! What the CFG passes ([`crate::loans`], [`crate::moves`],
//! [`crate::unused`]) share: points in a body, its edges, dense sets, the
//! places a statement names, local liveness, and where refusals are
//! reported.

use hir::ExprId;
use rustc_hash::FxHashSet;

use crate::{
    BlockId, BodyId, CheckTrap, LocalId, MirBody, Operand, Place, ProjElem, StatementKind,
    TerminatorKind, ValueExpr,
};

/// A location in one body: before statement `index` of `block`, or before
/// the terminator when `index` is the statement count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Point {
    pub(crate) block: BlockId,
    pub(crate) index: usize,
}

pub(crate) fn local_index(local: LocalId) -> usize {
    u32::from(local.into_raw()) as usize
}

pub(crate) fn block_index(block: BlockId) -> usize {
    u32::from(block.into_raw()) as usize
}

pub(crate) fn successors(body: &MirBody, block: BlockId) -> Vec<BlockId> {
    match &body.blocks[block].terminator.kind {
        TerminatorKind::Goto { target } => vec![*target],
        TerminatorKind::SwitchBool {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        TerminatorKind::SwitchVariant {
            arms, otherwise, ..
        } => arms
            .iter()
            .map(|(_, target)| *target)
            .chain(std::iter::once(*otherwise))
            .collect(),
        TerminatorKind::Call { target, .. } => target.iter().copied().collect(),
        TerminatorKind::Trap { target, .. } | TerminatorKind::ConstTrap { target, .. } => {
            vec![*target]
        }
        TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
    }
}

/// Every point of a block: its statements, then its terminator.
pub(crate) fn points_of(body: &MirBody, block: BlockId) -> impl Iterator<Item = Point> + '_ {
    (0..=body.blocks[block].statements.len()).map(move |index| Point { block, index })
}

/// The HIR expression the statement or terminator at `point` was lowered
/// from.
pub(crate) fn origin_at(body: &MirBody, point: Point) -> ExprId {
    let data = &body.blocks[point.block];
    match data.statements.get(point.index) {
        Some(stmt) => stmt.origin,
        None => data.terminator.origin,
    }
}

/// A dense set of locals (or loans), indexed by arena position.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub(crate) fn new(len: usize) -> BitSet {
        BitSet {
            words: vec![0; len.div_ceil(64)],
        }
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    /// Whether the bit was newly set.
    pub(crate) fn insert(&mut self, index: usize) -> bool {
        let was = self.contains(index);
        self.words[index / 64] |= 1 << (index % 64);
        !was
    }

    pub(crate) fn remove(&mut self, index: usize) {
        self.words[index / 64] &= !(1 << (index % 64));
    }

    pub(crate) fn union_with(&mut self, other: &BitSet) {
        for (word, more) in self.words.iter_mut().zip(&other.words) {
            *word |= more;
        }
    }

    pub(crate) fn intersect_with(&mut self, other: &BitSet) {
        for (word, mask) in self.words.iter_mut().zip(&other.words) {
            *word &= mask;
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(w, &word)| {
            (0..64)
                .filter(move |bit| word & (1 << bit) != 0)
                .map(move |bit| w * 64 + bit)
        })
    }
}

/// A place and the places its index operands name.
pub(crate) fn place_places(place: &Place, f: &mut impl FnMut(&Place)) {
    f(place);
    for elem in &place.projection {
        if let ProjElem::Index(op) = elem {
            operand_places(op, f);
        }
    }
}

pub(crate) fn operand_places(op: &Operand, f: &mut impl FnMut(&Place)) {
    match op {
        Operand::Copy(place) | Operand::Move(place) => place_places(place, f),
        Operand::Const(_) => {}
    }
}

/// The operands a value is computed from, in evaluation order. A borrow's
/// or a raw borrow's place is not an operand.
pub(crate) fn value_operands(value: &ValueExpr, f: &mut impl FnMut(&Operand)) {
    match value {
        ValueExpr::Use(op) | ValueExpr::UnaryNeg(op) => f(op),
        ValueExpr::BinaryOp(_, a, b)
        | ValueExpr::Index { base: a, index: b }
        | ValueExpr::Repeat { elem: a, count: b } => {
            f(a);
            f(b);
        }
        ValueExpr::Aggregate { ops, .. }
        | ValueExpr::Instantiate {
            const_args: ops, ..
        } => ops.iter().for_each(f),
        ValueExpr::Field { base, .. } => f(base),
        ValueExpr::AddrOfStatic { projection, .. } => {
            for elem in projection {
                if let ProjElem::Index(op) = elem {
                    f(op);
                }
            }
        }
        ValueExpr::Borrow { .. } | ValueExpr::AddrOf { .. } | ValueExpr::Closure { .. } => {}
    }
}

/// The places a value reads or borrows.
pub(crate) fn value_places(value: &ValueExpr, f: &mut impl FnMut(&Place)) {
    match value {
        ValueExpr::AddrOf { place, .. } | ValueExpr::Borrow { place, .. } => place_places(place, f),
        // Binding a closure to its places reads none of them; the
        // places are named, as a borrow's is.
        ValueExpr::Closure { captures, .. } => {
            for capture in captures {
                place_places(&capture.place, f);
            }
        }
        value => value_operands(value, &mut |op| operand_places(op, f)),
    }
}

/// Every place the body reads or borrows, index operands included: the
/// places in each assignment's value expression and the index operands of
/// its place expression, each switch's discriminant, and each call's
/// callee and arguments.
pub(crate) fn for_each_read(body: &MirBody, f: &mut impl FnMut(&Place)) {
    for (_, data) in body.blocks.iter() {
        for stmt in &data.statements {
            let StatementKind::Assign { dest, value } = &stmt.kind else {
                continue;
            };
            value_places(value, f);
            for elem in &dest.projection {
                if let ProjElem::Index(op) = elem {
                    operand_places(op, f);
                }
            }
        }
        match &data.terminator.kind {
            TerminatorKind::SwitchBool { discr, .. }
            | TerminatorKind::SwitchVariant { discr, .. } => operand_places(discr, f),
            TerminatorKind::Call { callee, args, .. } => {
                operand_places(callee, f);
                for arg in args {
                    operand_places(arg, f);
                }
            }
            TerminatorKind::Return
            | TerminatorKind::Goto { .. }
            | TerminatorKind::Trap { .. }
            | TerminatorKind::ConstTrap { .. }
            | TerminatorKind::Unreachable => {}
        }
    }
}

/// Every point that defines the whole of `local`, with its origin.
pub(crate) fn defs_of(body: &MirBody, local: LocalId) -> Vec<(Point, ExprId)> {
    let mut defs = Vec::new();
    for (block, data) in body.blocks.iter() {
        for (index, stmt) in data.statements.iter().enumerate() {
            let StatementKind::Assign { dest, .. } = &stmt.kind else {
                continue;
            };
            if dest.local == local && dest.projection.is_empty() {
                defs.push((Point { block, index }, stmt.origin));
            }
        }
        let dest = match &data.terminator.kind {
            TerminatorKind::Call { dest, .. } | TerminatorKind::Trap { dest, .. } => Some(*dest),
            _ => None,
        };
        if dest == Some(local) {
            defs.push((
                Point {
                    block,
                    index: data.statements.len(),
                },
                data.terminator.origin,
            ));
        }
    }
    defs
}

/// The locals a point reads. A projected destination reads its root (the
/// write goes into or through it); a whole-local destination is a
/// definition, not a use.
pub(crate) fn uses_at(body: &MirBody, point: Point) -> Vec<LocalId> {
    let data = &body.blocks[point.block];
    let mut uses = Vec::new();
    let mut push = |place: &Place| uses.push(place.local);
    match data.statements.get(point.index) {
        Some(stmt) => match &stmt.kind {
            StatementKind::Assign { dest, value } => {
                value_places(value, &mut push);
                if !dest.projection.is_empty() {
                    place_places(dest, &mut push);
                }
            }
            // Neither a use nor a def: counting it as a use would keep
            // every holder live up to its own storage end.
            StatementKind::StorageDead { .. } => {}
        },
        None => match &data.terminator.kind {
            TerminatorKind::SwitchBool { discr, .. }
            | TerminatorKind::SwitchVariant { discr, .. } => operand_places(discr, &mut push),
            TerminatorKind::Call { callee, args, .. } => {
                operand_places(callee, &mut push);
                for arg in args {
                    operand_places(arg, &mut push);
                }
            }
            TerminatorKind::Return => uses.push(body.return_local()),
            TerminatorKind::Goto { .. }
            | TerminatorKind::Trap { .. }
            | TerminatorKind::ConstTrap { .. }
            | TerminatorKind::Unreachable => {}
        },
    }
    uses
}

/// The local a point writes whole, if any.
pub(crate) fn def_at(body: &MirBody, point: Point) -> Option<LocalId> {
    let data = &body.blocks[point.block];
    match data.statements.get(point.index) {
        Some(stmt) => match &stmt.kind {
            StatementKind::Assign { dest, .. } => dest.projection.is_empty().then_some(dest.local),
            StatementKind::StorageDead { .. } => None,
        },
        None => match &data.terminator.kind {
            TerminatorKind::Call { dest, .. } | TerminatorKind::Trap { dest, .. } => Some(*dest),
            _ => None,
        },
    }
}

/// Backward local liveness to a fixpoint: per block, per point
/// (statements then terminator), the locals live before it — used there,
/// or later on some path.
pub(crate) fn liveness(body: &MirBody) -> Vec<Vec<BitSet>> {
    liveness_of(body, |point| uses_at(body, point))
}

/// [`liveness`] with the locals each point reads given by `uses`.
pub(crate) fn liveness_of(
    body: &MirBody,
    uses: impl Fn(Point) -> Vec<LocalId>,
) -> Vec<Vec<BitSet>> {
    let locals = body.locals.len();
    let blocks = body.blocks.len();
    let mut live_in: Vec<BitSet> = vec![BitSet::new(locals); blocks];
    let transfer = |block: BlockId, live_out: BitSet| -> Vec<BitSet> {
        let mut before: Vec<BitSet> = Vec::new();
        let mut live = live_out;
        for point in points_of(body, block).collect::<Vec<_>>().into_iter().rev() {
            if let Some(def) = def_at(body, point) {
                live.remove(local_index(def));
            }
            for local in uses(point) {
                live.insert(local_index(local));
            }
            before.push(live.clone());
        }
        before.reverse();
        before
    };
    let live_out = |live_in: &[BitSet], block: BlockId| {
        let mut out = BitSet::new(locals);
        for succ in successors(body, block) {
            out.union_with(&live_in[block_index(succ)]);
        }
        out
    };
    let mut changed = true;
    while changed {
        changed = false;
        for (block, _) in body.blocks.iter().collect::<Vec<_>>().into_iter().rev() {
            let before = transfer(block, live_out(&live_in, block));
            let entry = before[0].clone();
            if entry != live_in[block_index(block)] {
                live_in[block_index(block)] = entry;
                changed = true;
            }
        }
    }
    body.blocks
        .iter()
        .map(|(block, _)| transfer(block, live_out(&live_in, block)))
        .collect()
}

/// Whether lowering planted a `Trap` for an error anywhere in the body.
pub(crate) fn has_trap(body: &MirBody) -> bool {
    body.blocks
        .iter()
        .any(|(_, data)| matches!(data.terminator.kind, TerminatorKind::Trap { .. }))
}

/// Each reachable block's position in reverse postorder: an edge to a
/// block no later than its source is a loop's back edge.
pub(crate) fn reverse_postorder(body: &MirBody) -> Vec<Option<usize>> {
    let blocks = body.blocks.len();
    let mut post = Vec::with_capacity(blocks);
    let mut seen = BitSet::new(blocks);
    seen.insert(block_index(body.entry));
    let mut stack = vec![(body.entry, successors(body, body.entry), 0)];
    while let Some((block, succs, next)) = stack.last_mut() {
        if let Some(&succ) = succs.get(*next) {
            *next += 1;
            if seen.insert(block_index(succ)) {
                stack.push((succ, successors(body, succ), 0));
            }
        } else {
            post.push(*block);
            stack.pop();
        }
    }
    let mut order = vec![None; blocks];
    for (position, block) in post.iter().rev().enumerate() {
        order[block_index(*block)] = Some(position);
    }
    order
}

/// Where a checker reports one body's refusals: each diagnostic once per
/// key, and one trap per point, with the message of the first refusal
/// found there.
pub(crate) struct Findings<'a, D> {
    diagnostics: &'a mut Vec<D>,
    message: &'a dyn Fn(&D) -> String,
    traps: Vec<(Point, String)>,
    trapped: FxHashSet<Point>,
}

impl<'a, D> Findings<'a, D> {
    pub(crate) fn new(
        diagnostics: &'a mut Vec<D>,
        message: &'a dyn Fn(&D) -> String,
    ) -> Findings<'a, D> {
        Findings {
            diagnostics,
            message,
            traps: Vec::new(),
            trapped: FxHashSet::default(),
        }
    }

    /// Report the refusal `build` describes, trapped at `point` and keyed
    /// by `key` in `seen`. `build` runs only when the trap or the report
    /// is new; `None` means there is nothing to refuse.
    pub(crate) fn report<K: Eq + std::hash::Hash>(
        &mut self,
        point: Point,
        seen: &mut FxHashSet<K>,
        key: K,
        build: impl FnOnce() -> Option<D>,
    ) {
        if self.trapped.contains(&point) && seen.contains(&key) {
            return;
        }
        let Some(diag) = build() else {
            return;
        };
        if self.trapped.insert(point) {
            self.traps.push((point, (self.message)(&diag)));
        }
        if seen.insert(key) {
            self.diagnostics.push(diag);
        }
    }

    pub(crate) fn traps(self, body: BodyId) -> impl Iterator<Item = CheckTrap> {
        self.traps
            .into_iter()
            .map(move |(point, message)| CheckTrap {
                body,
                block: point.block,
                index: point.index,
                message,
            })
    }
}

/// The closures a body binds (#175), and which locals may hold each.
///
/// The closures a body binds are its own, so this is a question about the
/// body alone: a closure's value flows by assignment, through a call
/// (whatever is passed in may come back out, or be stored through a
/// borrow passed beside it) and by borrowing. Every local whose type may
/// hold a closure is unified with what it is computed from, and each class
/// holds the closures bound into it — flow-insensitive and coarse on
/// purpose: a class only ever widens what a call is judged to touch.
pub(crate) struct ClosureFlow<'b> {
    /// Each bound closure: its [`ValueExpr::Closure`]'s origin and places.
    closures: Vec<(ExprId, &'b [crate::Capture])>,
    /// Per local: indexes into `closures`.
    held: Vec<Vec<usize>>,
}

impl<'b> ClosureFlow<'b> {
    pub(crate) fn of(body: &'b MirBody) -> ClosureFlow<'b> {
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        let count = body.locals.len();
        let mut parent: Vec<usize> = (0..count).collect();
        let holds = |local: LocalId| body.locals[local].ty.may_hold_closure();
        let union = |parent: &mut Vec<usize>, a: LocalId, b: LocalId| {
            if holds(a) && holds(b) {
                let a = find(parent, local_index(a));
                let b = find(parent, local_index(b));
                parent[a] = b;
            }
        };
        let mut closures = Vec::new();
        let mut bound_into = Vec::new();
        for (_, data) in body.blocks.iter() {
            for stmt in &data.statements {
                let StatementKind::Assign { dest, value } = &stmt.kind else {
                    continue;
                };
                if let ValueExpr::Closure { captures, .. } = value {
                    bound_into.push((dest.local, closures.len()));
                    closures.push((stmt.origin, captures.as_slice()));
                    continue;
                }
                value_places(value, &mut |place| {
                    union(&mut parent, dest.local, place.local)
                });
            }
            if let TerminatorKind::Call {
                callee, args, dest, ..
            } = &data.terminator.kind
            {
                for op in std::iter::once(callee).chain(args) {
                    operand_places(op, &mut |place| union(&mut parent, *dest, place.local));
                }
            }
        }
        let mut by_root: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (local, closure) in bound_into {
            let root = find(&mut parent, local_index(local));
            by_root[root].push(closure);
        }
        let held = (0..count)
            .map(|index| by_root[find(&mut parent, index)].clone())
            .collect();
        ClosureFlow { closures, held }
    }

    /// The closures an operand may hold.
    pub(crate) fn held_by(&self, op: &Operand) -> &[usize] {
        match op {
            Operand::Copy(place) | Operand::Move(place) => &self.held[local_index(place.local)],
            Operand::Const(_) => &[],
        }
    }

    pub(crate) fn captures(&self, closure: usize) -> &'b [crate::Capture] {
        self.closures[closure].1
    }

    pub(crate) fn origin(&self, closure: usize) -> ExprId {
        self.closures[closure].0
    }
}
