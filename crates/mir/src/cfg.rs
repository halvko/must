//! What the CFG checkers ([`crate::loans`], [`crate::moves`]) share: points
//! in a body, its edges, dense sets, the places a statement names, and
//! where refusals are reported.

use hir::ExprId;
use rustc_hash::FxHashSet;

use crate::{
    BlockId, BodyId, CheckTrap, LocalId, MirBody, Operand, Place, ProjElem, Rvalue, StatementKind,
    TerminatorKind,
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
pub(crate) fn value_operands(value: &Rvalue, f: &mut impl FnMut(&Operand)) {
    match value {
        Rvalue::Use(op) | Rvalue::UnaryNeg(op) => f(op),
        Rvalue::BinaryOp(_, a, b)
        | Rvalue::Index { base: a, index: b }
        | Rvalue::Repeat { elem: a, count: b } => {
            f(a);
            f(b);
        }
        Rvalue::Aggregate { ops, .. }
        | Rvalue::Instantiate {
            const_args: ops, ..
        } => ops.iter().for_each(f),
        Rvalue::Field { base, .. } => f(base),
        Rvalue::AddrOfStatic { projection, .. } => {
            for elem in projection {
                if let ProjElem::Index(op) = elem {
                    f(op);
                }
            }
        }
        Rvalue::Borrow { .. } | Rvalue::AddrOf { .. } => {}
    }
}

/// The places a value reads or borrows.
pub(crate) fn value_places(value: &Rvalue, f: &mut impl FnMut(&Place)) {
    match value {
        Rvalue::AddrOf { place, .. } | Rvalue::Borrow { place, .. } => place_places(place, f),
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
            let StatementKind::Assign { dest, rvalue } = &stmt.kind else {
                continue;
            };
            value_places(rvalue, f);
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
