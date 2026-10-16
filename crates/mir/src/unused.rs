//! Unused bindings and values never read, from local liveness over the
//! lowered CFG. Liveness is whole-local: a write into `s.f` counts as read
//! when any part of `s` is read afterwards.

use base_db::Db;
use hir::body::{ExprData, PatData, Stmt};
use hir::infer::InferenceDiagnostic;
use hir::{BindingId, ExprId, ItemId, Resolution};
use la_arena::ArenaMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::OnceCell;
use syntax::TextRange;

use crate::cfg::{self, BitSet, Point};
use crate::{LocalId, MirBody, ProjElem, StatementKind};

#[cfg(test)]
mod tests;

/// One warning, keyed by HIR ids; ranges attach in the aggregator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnusedDiagnostic {
    /// Nothing reads the binding. Squiggles its name.
    Unused { binding: BindingId },
    /// A value written to the binding is overwritten or abandoned on every
    /// path before a read. Squiggles `value`, the expression written, when
    /// it is the binding's alone; else, inside a destructuring, the name.
    NeverRead {
        binding: BindingId,
        value: Option<ExprId>,
    },
    /// The argument a parameter receives is overwritten on every path
    /// before a read. Squiggles the parameter's name.
    ParamNeverRead { binding: BindingId },
}

impl UnusedDiagnostic {
    pub fn binding(&self) -> BindingId {
        match self {
            UnusedDiagnostic::Unused { binding }
            | UnusedDiagnostic::NeverRead { binding, .. }
            | UnusedDiagnostic::ParamNeverRead { binding } => *binding,
        }
    }

    pub fn range(&self, source_map: &hir::BodySourceMap) -> Option<TextRange> {
        let ptr = match self {
            UnusedDiagnostic::NeverRead {
                value: Some(value), ..
            } => source_map.node_for_expr(*value)?,
            _ => source_map.node_for_binding(self.binding())?,
        };
        Some(ptr.text_range())
    }

    pub fn message(&self, body: &hir::Body) -> String {
        let name = body.bindings[self.binding()]
            .written_name()
            .unwrap_or_default();
        match self {
            UnusedDiagnostic::Unused { .. } => format!("unused binding `{name}`"),
            UnusedDiagnostic::NeverRead { .. } => {
                format!("value written to `{name}` is never read")
            }
            UnusedDiagnostic::ParamNeverRead { .. } => {
                format!("the argument passed as `{name}` is never read")
            }
        }
    }
}

/// The warnings on every body lowered from one item, each on its own CFG.
/// An item with a lowering trap gets none: the broken expression's reads
/// vanish into the trap.
#[salsa::tracked(returns(ref))]
pub fn unused_check<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<UnusedDiagnostic> {
    let lowered = crate::mir_lowered(db, item);
    let mut out = Vec::new();
    if lowered.bodies.iter().any(|(_, body)| cfg::has_trap(body)) {
        return out;
    }
    let check = ItemCheck {
        db,
        item,
        hir_body: hir::body::body(db, item),
        owners: OnceCell::new(),
    };
    for (_, body) in lowered.bodies.iter() {
        check.body(body, &mut out);
    }
    out
}

struct ItemCheck<'db> {
    db: &'db dyn Db,
    item: ItemId<'db>,
    hir_body: &'db hir::Body,
    /// Each value a statement writes to one binding alone, with that
    /// binding; built by the first warning that needs it.
    owners: OnceCell<FxHashMap<ExprId, BindingId>>,
}

impl ItemCheck<'_> {
    fn body(&self, body: &MirBody, out: &mut Vec<UnusedDiagnostic>) {
        // Per local, the binding it is judged as. A linear local's loss is
        // the leak error's; a raw pointer can read an addressable one.
        let tracked: Vec<Option<BindingId>> = body
            .locals
            .iter()
            .map(|(_, data)| {
                let binding = data.binding?;
                let name = self.hir_body.bindings[binding].written_name()?;
                let skip = name.starts_with('_')
                    || data.addressable
                    || matches!(data.ty, hir::Ty::Error)
                    || !hir::capability::has_forget(self.db, &data.ty);
                (!skip).then_some(binding)
            })
            .collect();
        if tracked.iter().all(Option::is_none) {
            return;
        }
        let reads = |point| reads_at(body, point);
        // Reads in unreachable code count: the binding is used there.
        let mut used = BitSet::new(body.locals.len());
        for (block, _) in body.blocks.iter() {
            for point in cfg::points_of(body, block) {
                for local in reads(point) {
                    used.insert(cfg::local_index(local));
                }
            }
        }
        let live = cfg::liveness_of(body, reads);
        let reachable = cfg::reverse_postorder(body);
        let mut never_read: Vec<Vec<Point>> = vec![Vec::new(); body.locals.len()];
        for (block, _) in body.blocks.iter() {
            if reachable[cfg::block_index(block)].is_none() {
                continue;
            }
            for point in cfg::points_of(body, block) {
                let Some(local) = cfg::def_at(body, point).or_else(|| in_place_write(body, point))
                else {
                    continue;
                };
                if tracked[cfg::local_index(local)].is_some()
                    && !live_after(body, &live, point, local)
                {
                    never_read[cfg::local_index(local)].push(point);
                }
            }
        }
        let entry_live = &live[cfg::block_index(body.entry)][0];
        let mut seen = FxHashSet::default();
        for (local, _) in body.locals.iter() {
            let index = cfg::local_index(local);
            let Some(binding) = tracked[index] else {
                continue;
            };
            if !used.contains(index) {
                if !self.shadows_variant(binding) {
                    out.push(UnusedDiagnostic::Unused { binding });
                }
                continue;
            }
            if body.params.contains(&local) && !entry_live.contains(index) {
                out.push(UnusedDiagnostic::ParamNeverRead { binding });
            }
            for &point in &never_read[index] {
                let origin = cfg::origin_at(body, point);
                if seen.insert((binding, origin)) {
                    let value = self.writes_alone(binding, origin).then_some(origin);
                    out.push(UnusedDiagnostic::NeverRead { binding, value });
                }
            }
        }
    }

    /// Whether `binding` is a match arm's bare name that already warns it
    /// shadows a variant: that warning's fix removes the binding.
    fn shadows_variant(&self, binding: BindingId) -> bool {
        hir::infer::infer(self.db, self.item)
            .diagnostics
            .iter()
            .any(|diag| match diag {
                InferenceDiagnostic::BindShadowsVariant { pat, .. } => {
                    self.hir_body.pats[*pat] == PatData::Bind(binding)
                }
                _ => false,
            })
    }

    /// Whether `value` is written to `binding` alone: a `let` binding just
    /// that name, or an assignment to it or into it.
    fn writes_alone(&self, binding: BindingId, value: ExprId) -> bool {
        self.owners
            .get_or_init(|| self.collect_owners())
            .get(&value)
            == Some(&binding)
    }

    /// Every value a statement writes to one binding alone, with that binding.
    fn collect_owners(&self) -> FxHashMap<ExprId, BindingId> {
        let resolutions = hir::scopes::resolutions(self.db, self.item);
        let mut owned = FxHashMap::default();
        for (_, data) in self.hir_body.exprs.iter() {
            let ExprData::Block { stmts, .. } = data else {
                continue;
            };
            for stmt in stmts {
                let (value, binding) = match stmt {
                    Stmt::Let {
                        pat,
                        init: Some(init),
                        ..
                    } => match &self.hir_body.pats[*pat] {
                        PatData::Bind(binding) => (*init, Some(*binding)),
                        _ => continue,
                    },
                    Stmt::Assign { target, value } => {
                        (*value, root_binding(self.hir_body, resolutions, *target))
                    }
                    _ => continue,
                };
                if let Some(binding) = binding {
                    owned.insert(value, binding);
                }
            }
        }
        owned
    }
}

/// The binding a place expression's root names: `x` in `x`, `x.f`, `x[i]`.
fn root_binding(
    body: &hir::Body,
    resolutions: &ArenaMap<ExprId, Resolution>,
    mut expr: ExprId,
) -> Option<BindingId> {
    loop {
        match &body.exprs[expr] {
            ExprData::Field { receiver, .. } => expr = *receiver,
            ExprData::Index { base, .. } => expr = *base,
            ExprData::NameRef(_) => {
                return match resolutions.get(expr)? {
                    Resolution::Local(binding) => Some(*binding),
                    _ => None,
                };
            }
            _ => return None,
        }
    }
}

/// The locals a point reads, where a write into a local (`s.f = v`) does
/// not read it.
fn reads_at(body: &MirBody, point: Point) -> Vec<LocalId> {
    let mut reads = cfg::uses_at(body, point);
    if let Some(written) = in_place_write(body, point) {
        reads.retain(|&local| local != written);
    }
    reads
}

/// The local `s` of a write into it, `s.f = v` or `s[i] = v`, when nothing
/// else in the statement reads `s`. A write through a deref is not one.
fn in_place_write(body: &MirBody, point: Point) -> Option<LocalId> {
    let stmt = body.blocks[point.block].statements.get(point.index)?;
    let StatementKind::Assign { dest, value } = &stmt.kind else {
        return None;
    };
    if dest.projection.is_empty()
        || dest
            .projection
            .iter()
            .any(|elem| matches!(elem, ProjElem::Deref))
    {
        return None;
    }
    let mut read = false;
    cfg::value_places(value, &mut |place| read |= place.local == dest.local);
    for elem in &dest.projection {
        if let ProjElem::Index(op) = elem {
            cfg::operand_places(op, &mut |place| read |= place.local == dest.local);
        }
    }
    (!read).then_some(dest.local)
}

/// Whether `local` is live right after `point`: before the next statement,
/// or at the entry of some successor after a terminator.
fn live_after(body: &MirBody, live: &[Vec<BitSet>], point: Point, local: LocalId) -> bool {
    let index = cfg::local_index(local);
    let block = cfg::block_index(point.block);
    if point.index < body.blocks[point.block].statements.len() {
        return live[block][point.index + 1].contains(index);
    }
    cfg::successors(body, point.block)
        .into_iter()
        .any(|succ| live[cfg::block_index(succ)][0].contains(index))
}
