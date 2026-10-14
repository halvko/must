//! Interprocedural inference: binding groups.
//!
//! Items whose signatures inference must determine — no contract at all
//! (neither a written annotation nor one synthesized from a self-sufficient
//! fn-literal body — see [`crate::item_tree`]), or a hole-bearing one (`_`
//! is a partial contract: unconstrained exactly where it says `_`) — get
//! their signatures from their own bodies. A signature is determined
//! jointly with everything that constrains it — its own body, mutual
//! recursion, and its callers' concrete uses — so items are partitioned
//! into connected components of their reference relation, with every
//! reference edge added in both directions (a caller constrains the callee
//! as much as the callee constrains the caller). Each group is inferred in
//! one unification context, with a shared signature variable per member.
//! The component condensation is a DAG, so groups only ever ask for
//! signatures of *other* groups (or of fully-typed items, whose signatures
//! are read off their annotations): no query cycles.
//!
//! A fully typed, non-generic item is never a member, but its body is a
//! constraint source for the group of the unannotated items it references:
//! it runs after the members and fills only what their bodies left open
//! (T11). Everything one such body references joins one group.
//!
//! Groups are currently scoped to a single file. Cross-file inference
//! within a library is desirable (splitting code across files should not
//! require extra annotations), but the incremental cost is an open
//! question: cross-file groups would let edits in one file trigger
//! re-inference in another, and how well salsa early-cutoff contains that
//! depends on how large those groups grow.
//!
//! Language decision (recorded): items exported at the *library* boundary
//! will require written contracts regardless — inference is for internal
//! items. Today every item counts as internal.

use base_db::{Db, SourceFile};
use ena::unify::InPlaceUnificationTable;
use rustc_hash::FxHashMap;

use crate::constraint::resolve_fully;
use crate::infer::InferCtx;
use crate::item_tree::item_tree;
use crate::ty::{ParamScope, Ty, TyVar, TyVarValue, erase_infer, lower_type_ref_in};
use crate::{ItemLoc, file_item_ids, item_loc};

/// One inference group per salsa key.
#[salsa::interned(debug)]
pub struct GroupId<'db> {
    pub file: SourceFile,
    pub index: u32,
}

/// The partition of a file's group-inferrable items (no fully-typed
/// annotation) into binding groups, with the fully typed items that feed
/// each. Range-free; only membership changes invalidate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceGroups {
    /// Item index → group index; `None` for fully-typed items.
    pub group_of: Vec<Option<u32>>,
    /// Group index → member item indices (deterministic order).
    pub groups: Vec<Vec<usize>>,
    /// Group index → the fully typed items (top-level items and members)
    /// whose bodies constrain the group's members without being members,
    /// in [`crate::all_checkable_items`] order. Each feeds at most one
    /// group.
    pub sources: Vec<Vec<ItemLoc>>,
}

#[salsa::tracked(returns(ref))]
pub fn inference_groups(db: &dyn Db, file: SourceFile) -> InferenceGroups {
    let tree = item_tree(db, file);
    let ids = file_item_ids(db, file);
    let group_inferrable: Vec<bool> = tree
        .items
        .iter()
        .map(|it| {
            // Type items declare no value: they have no signature to infer,
            // so they never join a binding group. Generic items NEVER join
            // one either (TR06, load-bearing): a group's shared signature
            // variable is a monotype, so membership would pin every
            // instantiation of the scheme to one type. The fully-annotated
            // rule makes them firewall items — their scheme is complete
            // from the binder (and when the rule is violated the signature
            // is `{error}` with the definition carrying the diagnostic),
            // so there is nothing group inference could add; the explicit
            // `generics.is_empty()` guard keeps rule-violating generics
            // (whose `type_ref` is `None`) out too.
            matches!(it.kind, crate::item_tree::ItemKind::Value(_))
                && it.generics.is_empty()
                && it.type_ref.as_ref().is_none_or(|tr| !tr.is_fully_typed())
        })
        .collect();

    // Item identity → index, the same (name, disambiguator) scheme ids use.
    let mut index_of: FxHashMap<(&str, u32), usize> = FxHashMap::default();
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    for (index, data) in tree.items.iter().enumerate() {
        let disambiguator = seen.entry(data.name.as_str()).or_insert(0);
        index_of.insert((data.name.as_str(), *disambiguator), index);
        *disambiguator += 1;
    }

    // The group-inferrable items each item's body references (sorted,
    // deduplicated). Only this file's top-level items count: a member or
    // another file's item that shares a name is a different item.
    let inferrable_refs = |item: crate::ItemId<'_>| -> Vec<usize> {
        let mut refs: Vec<usize> = crate::resolutions(db, item)
            .values()
            .filter_map(|resolution| {
                let crate::Resolution::Item(loc) = resolution else {
                    return None;
                };
                if loc.file != file || loc.member.is_some() {
                    return None;
                }
                let target = *index_of.get(&(&*loc.name, loc.disambiguator))?;
                group_inferrable[target].then_some(target)
            })
            .collect();
        refs.sort_unstable();
        refs.dedup();
        refs
    };

    // Reference edges between group-inferrable items.
    let edges: Vec<Vec<usize>> = (0..tree.items.len())
        .map(|index| {
            if group_inferrable[index] {
                inferrable_refs(ids[index])
            } else {
                Vec::new()
            }
        })
        .collect();

    // Add reverse edges so callers and callees form the same group. Without
    // this, a higher-order function like `fn(f, a) { f(a) }` is inferred
    // alone: its parameter types stay unconstrained and are erased to Error.
    // With reverse edges the call site and the callee share a unification
    // context, so the concrete argument types flow back into the callee.
    let mut biedges = edges.clone();
    for (from, succs) in edges.iter().enumerate() {
        for &to in succs {
            biedges[to].push(from);
        }
    }

    // Constraint sources (T11): a fully typed body feeds the group of what
    // it references, and everything one body references joins one group.
    let source_refs: Vec<(ItemLoc, Vec<usize>)> = crate::all_checkable_items(db, file)
        .into_iter()
        .filter(|&item| is_constraint_source(db, item))
        .map(|item| (item_loc(db, item), inferrable_refs(item)))
        .filter(|(_, refs)| !refs.is_empty())
        .collect();
    for (_, refs) in &source_refs {
        for pair in refs.windows(2) {
            biedges[pair[0]].push(pair[1]);
            biedges[pair[1]].push(pair[0]);
        }
    }
    for succs in &mut biedges {
        succs.sort_unstable();
        succs.dedup();
    }

    // Tarjan over the group-inferrable subgraph.
    let mut state = Tarjan {
        edges: &biedges,
        include: &group_inferrable,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next_index: 0,
        groups: Vec::new(),
    };
    for (node, &included) in group_inferrable.iter().enumerate() {
        if included && state.index[node].is_none() {
            state.visit(node);
        }
    }

    let mut group_of = vec![None; edges.len()];
    for (group_index, members) in state.groups.iter().enumerate() {
        for &member in members {
            group_of[member] = Some(group_index as u32);
        }
    }
    let mut sources = vec![Vec::new(); state.groups.len()];
    for (source, refs) in source_refs {
        if let Some(group) = group_of[refs[0]] {
            sources[group as usize].push(source);
        }
    }
    InferenceGroups {
        group_of,
        groups: state.groups,
        sources,
    }
}

/// Whether `item`'s body feeds the groups of the unannotated items it
/// references: a value item or member with a fully typed signature and no
/// generic binder. A generic body (or a member of a generic type) sees its
/// params as rigid [`Ty::Param`]s; letting one fill a group's monotype
/// would leak the param out of its binder, so those stay hard firewalls.
fn is_constraint_source(db: &dyn Db, item: crate::ItemId<'_>) -> bool {
    let Some(data) = crate::item_data(db, item).as_ref() else {
        return false;
    };
    let value = match data.kind {
        crate::item_tree::ItemKind::Value(_) => true,
        // A member's `Self` is concrete when its owner has no binder: a
        // `type`'s own `Self`, or the implementer a trait impl's head names
        // (a builtin or a non-generic `type`, else the silent `Ty::Error`).
        crate::item_tree::ItemKind::Member => crate::member_owner(db, item).is_some_and(|owner| {
            let Some(owner_data) = crate::item_data(db, owner).as_ref() else {
                return false;
            };
            owner_data.generics.is_empty()
                && matches!(
                    owner_data.kind,
                    crate::item_tree::ItemKind::Type | crate::item_tree::ItemKind::Trait
                )
        }),
        _ => false,
    };
    value
        && !crate::is_generic_body(db, item)
        && data
            .type_ref
            .as_ref()
            .is_some_and(|type_ref| type_ref.is_fully_typed())
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

    // Each member's recorded variables, for the walk after the loop.
    let mut recorded = Vec::new();
    // One pass, in this order, no fixpoint: whichever body is processed
    // first constrains the shared variables — its signature, another
    // member's signature via a call, or an annotation hole — and that
    // commitment wins; later members must agree with it (or poison below).
    // So this processing order is load-bearing, not incidental.
    for (&member, loc) in members.iter().zip(&member_locs) {
        let item = ids[member];
        let body = crate::body::body(db, item);
        let Some(root) = body.root else {
            // No initializer: nothing to infer from (parse errors cover it).
            poison(&mut table, &in_group[loc]);
            continue;
        };
        // Whatever annotation this member has is hole-bearing (that's why
        // it's in a group): lower it here as a partial contract — the
        // written part constrains the body, holes stay unconstrained for
        // the body to fill.
        let expected = crate::item_data(db, item)
            .as_ref()
            .and_then(|it| it.type_ref.as_ref())
            .map(|type_ref| {
                lower_type_ref_in(db, file, type_ref, &mut table, &ParamScope::default())
            })
            .unwrap_or_else(|| Ty::Infer(table.new_key(TyVarValue::Unknown)));
        let mut ctx = InferCtx::new(
            db,
            file,
            body,
            crate::resolutions(db, item),
            &mut table,
            &in_group,
            true,
        );
        let root_ty = ctx.infer_expr(root, &expected);
        let unified = ctx.adopt(&in_group[loc], &root_ty);
        // Solve this member's deferred joins *before* the next member runs:
        // a member's signature is decided by its own body (its axioms and
        // conclusions); later members may only fill variables the body left
        // genuinely free, never override its conclusions. Without this, a
        // use like `print(g(..))` in a sibling would set an unannotated
        // `g`'s return type to `str` while its branches say `usize` — and
        // the mismatch would be visible from no single item.
        ctx.solve();
        let left_unbound = ctx.left_unbound().to_vec();
        let mut own_error = false;
        if !left_unbound.is_empty() || root_ty.contains_error() {
            // An error written into the type itself.
            own_error = resolve_fully(&mut table, &root_ty).contains_error();
            if !left_unbound.is_empty() {
                recorded.push((loc, left_unbound));
            }
        } else {
            // Correct code takes this path. An error that reaches one of
            // the type's variables meets the unify guard on the way and is
            // recorded, so with nothing recorded the type is error-free.
            debug_assert!(!resolve_fully(&mut table, &root_ty).contains_error());
        }
        if !unified || own_error {
            // The body contradicts what the group already committed this
            // member to (called as a function in one body, bound to a plain
            // value in another), or an error is written into its type: no
            // signature is right, so poison it.
            // Poisoning wins regardless of the solved joins: a poisoned
            // variable stays poisoned, and poison beats an earlier
            // commitment.
            poison(&mut table, &in_group[loc]);
        }
        // Per-expression results and diagnostics are the per-item `infer`
        // query's business; only the signatures leave this query.
    }

    // A variable a body met an error on that is open in what its member
    // produces (the return of `fn () { missing }`) becomes `{error}`, so no
    // fully typed body decides what the body got wrong. Parameters stay
    // open for uses.
    for (loc, left_unbound) in &recorded {
        let own = resolve_fully(&mut table, &in_group[*loc]);
        poison_produced(&mut table, &own, left_unbound);
    }

    // Fully typed bodies last, so they fill only what the members' own
    // bodies left open (T11). One that disagrees changes nothing here; its
    // own `infer` reports the mismatch. Its own errors (an unknown type in
    // its signature, say) fill nothing either, so they aren't repeated at
    // every use of its callees.
    for source in &groups.sources[group.index(db) as usize] {
        crate::infer::check_item_body(db, source.to_id(db), &mut table, &in_group, true).solve();
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

/// Poison each of `recorded` that is open in what `own` produces.
fn poison_produced(table: &mut InPlaceUnificationTable<TyVar>, own: &Ty, recorded: &[TyVar]) {
    let mut produced = rustc_hash::FxHashSet::default();
    own.produced_vars(&mut produced);
    for &var in recorded {
        let root = resolve_fully(table, &Ty::Infer(var));
        if matches!(root, Ty::Infer(root) if produced.contains(&root)) {
            poison(table, &root);
        }
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
