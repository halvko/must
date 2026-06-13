//! Interprocedural inference: binding groups.
//!
//! Unannotated items get their signatures from their own bodies. Items that
//! reference each other without annotations must be inferred *together*
//! (mutual recursion has no starting point), so the unannotated items of a
//! file are partitioned into strongly connected components of their
//! reference graph and each group is inferred in one unification context,
//! with a shared signature variable per member. The SCC condensation is a
//! DAG, so groups only ever ask for signatures of *other* groups (or of
//! annotated items — which stay hard firewall edges): no query cycles.
//!
//! Language decision (recorded): once visibility exists, *exported* items
//! will require written contracts regardless — inference is for private
//! items, and today every item counts as private.

use base_db::{Db, SourceFile};
use ena::unify::InPlaceUnificationTable;
use rustc_hash::FxHashMap;

use crate::infer::{InferCtx, resolve_fully};
use crate::item_tree::item_tree;
use crate::ty::{Ty, TyVar, TyVarValue};
use crate::{ItemLoc, file_item_ids, item_loc};

/// One inference group per salsa key.
#[salsa::interned(debug)]
pub struct GroupId<'db> {
    pub file: SourceFile,
    pub index: u32,
}

/// The partition of a file's *unannotated* items into binding groups.
/// Range-free; only membership changes invalidate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceGroups {
    /// Item index → group index; `None` for annotated items.
    pub group_of: Vec<Option<u32>>,
    /// Group index → member item indices (deterministic order).
    pub groups: Vec<Vec<usize>>,
}

#[salsa::tracked(returns(ref))]
pub fn inference_groups(db: &dyn Db, file: SourceFile) -> InferenceGroups {
    let tree = item_tree(db, file);
    let ids = file_item_ids(db, file);
    let unannotated: Vec<bool> = tree.items.iter().map(|it| it.type_ref.is_none()).collect();

    // Item identity → index, the same (name, disambiguator) scheme ids use.
    let mut index_of: FxHashMap<(&str, u32), usize> = FxHashMap::default();
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    for (index, data) in tree.items.iter().enumerate() {
        let disambiguator = seen.entry(data.name.as_str()).or_insert(0);
        index_of.insert((data.name.as_str(), *disambiguator), index);
        *disambiguator += 1;
    }

    // Reference edges between unannotated items.
    let edges: Vec<Vec<usize>> = (0..tree.items.len())
        .map(|index| {
            if !unannotated[index] {
                return Vec::new();
            }
            let mut succ: Vec<usize> = crate::resolutions(db, ids[index])
                .values()
                .filter_map(|resolution| {
                    let crate::Resolution::Item(loc) = resolution else {
                        return None;
                    };
                    let target = *index_of.get(&(&*loc.name, loc.disambiguator))?;
                    unannotated[target].then_some(target)
                })
                .collect();
            succ.sort_unstable();
            succ.dedup();
            succ
        })
        .collect();

    // Tarjan over the unannotated subgraph.
    let mut state = Tarjan {
        edges: &edges,
        include: &unannotated,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next_index: 0,
        groups: Vec::new(),
    };
    for node in 0..edges.len() {
        if unannotated[node] && state.index[node].is_none() {
            state.visit(node);
        }
    }

    let mut group_of = vec![None; edges.len()];
    for (group_index, members) in state.groups.iter().enumerate() {
        for &member in members {
            group_of[member] = Some(group_index as u32);
        }
    }
    InferenceGroups {
        group_of,
        groups: state.groups,
    }
}

struct Tarjan<'a> {
    edges: &'a [Vec<usize>],
    include: &'a [bool],
    index: Vec<Option<u32>>,
    low: Vec<u32>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next_index: u32,
    groups: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, node: usize) {
        self.index[node] = Some(self.next_index);
        self.low[node] = self.next_index;
        self.next_index += 1;
        self.stack.push(node);
        self.on_stack[node] = true;
        for &succ in &self.edges[node] {
            if !self.include[succ] {
                continue;
            }
            if self.index[succ].is_none() {
                self.visit(succ);
                self.low[node] = self.low[node].min(self.low[succ]);
            } else if self.on_stack[succ] {
                self.low[node] = self.low[node].min(self.index[succ].unwrap());
            }
        }
        if Some(self.low[node]) == self.index[node] {
            let mut members = Vec::new();
            while let Some(member) = self.stack.pop() {
                self.on_stack[member] = false;
                members.push(member);
                if member == node {
                    break;
                }
            }
            members.sort_unstable();
            self.groups.push(members);
        }
    }
}

/// Signatures of one group's members (parallel to its member list).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupSignatures {
    pub signatures: Vec<Ty>,
}

#[salsa::tracked(returns(ref))]
pub fn infer_group<'db>(db: &'db dyn Db, group: GroupId<'db>) -> GroupSignatures {
    // TODO: figure out cross file inference - is it worth it? On one hand we don't want to
    // disincentivize splitting code across files, but on the other we want to give a nice real-time
    // experience. Maybe we want to support inference but give warnings where we had to do it cross
    // file such we can add a quick fix to add the type?
    let file = group.file(db);
    let groups = inference_groups(db, file);
    let Some(members) = groups.groups.get(group.index(db) as usize) else {
        return GroupSignatures::default();
    };
    let ids = file_item_ids(db, file);

    let mut table: InPlaceUnificationTable<TyVar> = InPlaceUnificationTable::new();
    // One signature variable per member; in-group references resolve to
    // these instead of calling `signature` (which would cycle).
    let member_locs: Vec<ItemLoc> = members.iter().map(|&i| item_loc(db, ids[i])).collect();
    let in_group: FxHashMap<ItemLoc, Ty> = member_locs
        .iter()
        .map(|loc| (loc.clone(), Ty::Infer(table.new_key(TyVarValue::Unknown))))
        .collect();

    for (&member, loc) in members.iter().zip(&member_locs) {
        let item = ids[member];
        let body = crate::body::body(db, item);
        let Some(root) = body.root else {
            // No initializer: nothing to infer from (parse errors cover it).
            let mut ctx = InferCtx::new(
                db,
                body,
                crate::resolutions(db, item),
                &mut table,
                &in_group,
            );
            ctx.unify_public(&in_group[loc], &Ty::Error);
            continue;
        };
        let mut ctx = InferCtx::new(
            db,
            body,
            crate::resolutions(db, item),
            &mut table,
            &in_group,
        );
        let root_ty = ctx.infer_expr(root, &in_group[loc]);
        ctx.unify_public(&in_group[loc], &root_ty);
        // Per-expression results and diagnostics are the per-item `infer`
        // query's business; only the signatures leave this query.
    }

    GroupSignatures {
        signatures: member_locs
            .iter()
            // Undetermined leftovers become `{error}` — a `TyVar` is only
            // meaningful inside this query's own table, and an
            // undetermined signature is what the needs-annotation
            // diagnostic exists for.
            .map(|loc| erase_infer(&resolve_fully(&mut table, &in_group[loc])))
            .collect(),
    }
}

fn erase_infer(ty: &Ty) -> Ty {
    match ty {
        Ty::Infer(_) => Ty::Error,
        Ty::Fn(f) => Ty::fn_type(
            f.params.iter().map(erase_infer).collect(),
            erase_infer(&f.ret),
        ),
        other => other.clone(),
    }
}
