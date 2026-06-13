//! Interprocedural inference: binding groups.
//!
//! Items whose signatures inference must determine — no annotation at all,
//! or a hole-bearing one (`_` is a partial contract: unconstrained exactly
//! where it says `_`) — get their signatures from their own bodies. Items
//! that reference each other on these terms must be inferred *together*
//! (mutual recursion has no starting point), so they are partitioned into
//! strongly connected components of their reference graph and each group is
//! inferred in one unification context, with a shared signature variable
//! per member. The SCC condensation is a DAG, so groups only ever ask for
//! signatures of *other* groups (or of fully-typed items — which stay hard
//! firewall edges): no query cycles.
//!
//! Language decision (recorded): items exported at the *library* boundary
//! will require written contracts regardless — inference is for internal
//! items. Today every item counts as internal.

use base_db::{Db, SourceFile};
use ena::unify::InPlaceUnificationTable;
use rustc_hash::FxHashMap;

use crate::infer::{InferCtx, resolve_fully};
use crate::item_tree::item_tree;
use crate::ty::{Ty, TyVar, TyVarValue, lower_type_ref};
use crate::{ItemLoc, file_item_ids, item_loc};

/// One inference group per salsa key.
#[salsa::interned(debug)]
pub struct GroupId<'db> {
    pub file: SourceFile,
    pub index: u32,
}

/// The partition of a file's group-inferrable items (no fully-typed
/// annotation) into binding groups. Range-free; only membership changes
/// invalidate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceGroups {
    /// Item index → group index; `None` for fully-typed items.
    pub group_of: Vec<Option<u32>>,
    /// Group index → member item indices (deterministic order).
    pub groups: Vec<Vec<usize>>,
}

#[salsa::tracked(returns(ref))]
pub fn inference_groups(db: &dyn Db, file: SourceFile) -> InferenceGroups {
    let tree = item_tree(db, file);
    let ids = file_item_ids(db, file);
    let group_inferrable: Vec<bool> = tree
        .items
        .iter()
        .map(|it| !it.type_ref.as_ref().is_some_and(crate::ty::is_fully_typed))
        .collect();

    // Item identity → index, the same (name, disambiguator) scheme ids use.
    let mut index_of: FxHashMap<(&str, u32), usize> = FxHashMap::default();
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    for (index, data) in tree.items.iter().enumerate() {
        let disambiguator = seen.entry(data.name.as_str()).or_insert(0);
        index_of.insert((data.name.as_str(), *disambiguator), index);
        *disambiguator += 1;
    }

    // Reference edges between group-inferrable items.
    let edges: Vec<Vec<usize>> = (0..tree.items.len())
        .map(|index| {
            if !group_inferrable[index] {
                return Vec::new();
            }
            let mut succ: Vec<usize> = crate::resolutions(db, ids[index])
                .values()
                .filter_map(|resolution| {
                    let crate::Resolution::Item(loc) = resolution else {
                        return None;
                    };
                    let target = *index_of.get(&(&*loc.name, loc.disambiguator))?;
                    group_inferrable[target].then_some(target)
                })
                .collect();
            succ.sort_unstable();
            succ.dedup();
            succ
        })
        .collect();

    // Tarjan over the group-inferrable subgraph.
    let mut state = Tarjan {
        edges: &edges,
        include: &group_inferrable,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next_index: 0,
        groups: Vec::new(),
    };
    for node in 0..edges.len() {
        if group_inferrable[node] && state.index[node].is_none() {
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
        // Whatever annotation this member has is hole-bearing (that's why
        // it's in a group): lower it here as a partial contract — the
        // written part constrains the body, holes stay unconstrained for
        // the body to fill.
        let expected = crate::item_data(db, item)
            .as_ref()
            .and_then(|it| it.type_ref.as_ref())
            .map(|type_ref| lower_type_ref(type_ref, &mut table))
            .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));
        let mut ctx = InferCtx::new(
            db,
            body,
            crate::resolutions(db, item),
            &mut table,
            &in_group,
        );
        let root_ty = ctx.infer_expr(root, &expected);
        if !ctx.unify_public(&in_group[loc], &root_ty) {
            // The body contradicts what the group already committed this
            // member to (called as a function in one body, bound to a plain
            // value in another): no signature is right, so poison it.
            poison(&mut table, &in_group[loc]);
        }
        // Per-expression results and diagnostics are the per-item `infer`
        // query's business; only the signatures leave this query.
    }

    GroupSignatures {
        signatures: member_locs
            .iter()
            // Undetermined leftovers become `{error}` — a `TyVar` is only
            // meaningful inside this query's own table, and an
            // undetermined signature is what the needs-annotation
            // diagnostic exists for. (Poisoned members already resolve to
            // `Ty::Error` and pass through unchanged.)
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

/// Commit a member's signature variable to `Ty::Error`. `unify_values`
/// lets a committed `Ty::Error` win over the (wrong) type the variable was
/// already committed to, so this undoes even an earlier guess; the poisoned
/// signature resolves to `{error}` and the needs-annotation diagnostic
/// takes over.
fn poison(table: &mut InPlaceUnificationTable<TyVar>, sig: &Ty) {
    if let Ty::Infer(var) = sig {
        table.union_value(*var, TyVarValue::Known(Ty::Error));
    }
}
