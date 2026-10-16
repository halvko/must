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
//! as much as the callee constrains the caller). Each body in a group is
//! checked once, in a table of its own, with its own copy of each member's
//! signature; the copies are merged callees first (see [`infer_group`]).
//! The component condensation is a DAG, so groups only ever ask for
//! signatures of *other* groups (or of fully-typed items, whose signatures
//! are read off their annotations): no query cycles.
//!
//! A fully typed, non-generic item is never a member, but its body is a
//! constraint source for the group of the unannotated items it references:
//! it runs after the members and fills only what their bodies left open
//! (T11). Everything one such body references joins one group.
//!
//! Groups span a library, so splitting code across a library's files needs
//! no extra annotations. An edit in one file can re-infer a group that
//! reaches into another; how well salsa early-cutoff contains that depends
//! on how large groups grow.
//!
//! Language decision (recorded): items exported at the *library* boundary
//! will require written contracts regardless — inference is for internal
//! items. Today every item counts as internal.

use base_db::{Db, SourceFile};
use ena::unify::{InPlaceUnificationTable, UnifyKey};
use rustc_hash::FxHashMap;

use crate::constraint::{Constraints, resolve_finished, resolve_fully};
use crate::infer::{InferCtx, InferenceDiagnostic, InferenceResult};
use crate::library::{LibraryId, library_item_ids, library_of};
use crate::ty::{
    ParamScope, Safety, Ty, TyVar, TyVarValue, erase_infer, export_ty, lower_type_ref_in,
};
use crate::{ItemLoc, item_loc};

/// One inference group per salsa key.
#[salsa::interned(debug)]
pub struct GroupId<'db> {
    pub library: LibraryId<'db>,
    pub index: u32,
}

/// The partition of a library's group-inferrable items (no fully-typed
/// annotation) into binding groups, indexed like [`library_item_ids`], with
/// the fully typed items that feed each. Range-free; only membership
/// changes invalidate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InferenceGroups {
    /// Group index → member item indices (deterministic order).
    pub groups: Vec<Vec<usize>>,
    /// Group index → the fully typed items (top-level items and members)
    /// whose bodies constrain the group's members without being members,
    /// in [`crate::all_checkable_items`] order. Each feeds at most one
    /// group.
    pub sources: Vec<Vec<ItemLoc>>,
    /// Each group member → its group index and its position in the group.
    pub slot: FxHashMap<ItemLoc, (u32, usize)>,
    /// Each fully typed source → the group it feeds and its position among
    /// the group's bodies, after the members.
    pub source_slot: FxHashMap<ItemLoc, (u32, usize)>,
}

#[salsa::tracked(returns(ref))]
pub fn inference_groups<'db>(db: &'db dyn Db, library: LibraryId<'db>) -> InferenceGroups {
    let items = crate::scopes::library_items(db, library);
    let group_inferrable: Vec<bool> = items
        .iter()
        .map(|(_, it)| {
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

    // Item identity → index.
    let index_of: FxHashMap<&ItemLoc, usize> = items
        .iter()
        .enumerate()
        .map(|(index, (loc, _))| (loc, index))
        .collect();

    // The group-inferrable items each item's body references (sorted,
    // deduplicated). Only the library's top-level items are indexed, so a
    // member is never one.
    let inferrable_refs = |item: crate::ItemId<'_>| -> Vec<usize> {
        let mut refs: Vec<usize> = crate::resolutions(db, item)
            .values()
            .filter_map(|resolution| {
                let crate::Resolution::Item(loc) = resolution else {
                    return None;
                };
                let target = *index_of.get(loc)?;
                group_inferrable[target].then_some(target)
            })
            .collect();
        refs.sort_unstable();
        refs.dedup();
        refs
    };

    // Reference edges between group-inferrable items.
    let edges: Vec<Vec<usize>> = (0..items.len())
        .map(|index| {
            if group_inferrable[index] {
                inferrable_refs(items[index].0.to_id(db))
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
    let source_refs: Vec<(ItemLoc, Vec<usize>)> = library
        .files(db)
        .into_iter()
        .flat_map(|file| crate::all_checkable_items(db, file))
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

    let mut slot = FxHashMap::default();
    for (group_index, members) in state.groups.iter().enumerate() {
        for (position, &member) in members.iter().enumerate() {
            slot.insert(items[member].0.clone(), (group_index as u32, position));
        }
    }
    let mut sources: Vec<Vec<ItemLoc>> = vec![Vec::new(); state.groups.len()];
    let mut source_slot = FxHashMap::default();
    for (source, refs) in source_refs {
        if let Some(&(group, _)) = slot.get(&items[refs[0]].0) {
            let feeds = &mut sources[group as usize];
            let position = state.groups[group as usize].len() + feeds.len();
            source_slot.insert(source.clone(), (group, position));
            feeds.push(source);
        }
    }
    InferenceGroups {
        groups: state.groups,
        sources,
        slot,
        source_slot,
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

/// The groups of `library` bigger than [`base_db::Settings`] allows, each
/// with its expression count over the bodies it covers (members and the
/// fully typed bodies feeding it). Range-free, so it backdates across
/// edits that change no oversized group's size.
#[salsa::tracked(returns(ref))]
pub fn oversized_groups<'db>(db: &'db dyn Db, library: LibraryId<'db>) -> Vec<(u32, u64)> {
    let Some(threshold) = base_db::settings(db).big_inference_group(db) else {
        return Vec::new();
    };
    let groups = inference_groups(db, library);
    let ids = library_item_ids(db, library);
    groups
        .groups
        .iter()
        .zip(&groups.sources)
        .enumerate()
        .filter_map(|(group, (members, sources))| {
            let size: u64 = members
                .iter()
                .map(|&member| ids[member])
                .chain(sources.iter().map(|source| source.to_id(db)))
                .map(|item| crate::body::body(db, item).exprs.len() as u64)
                .sum();
            (size > u64::from(threshold)).then_some((group as u32, size))
        })
        .collect()
}

/// The warning for groups bigger than [`base_db::Settings`] allows: every
/// body the group covers (members and the fully typed bodies feeding it)
/// gets one warning, at its first use of an unannotated member, with each
/// member it uses as related information. Annotating those members takes
/// them out of the group.
pub fn big_group_diagnostics(db: &dyn Db, file: SourceFile) -> Vec<crate::Diagnostic> {
    use syntax::ast::AstNode as _;
    let library = crate::library::library_of(db, file);
    let oversized = oversized_groups(db, library);
    if oversized.is_empty() {
        return Vec::new();
    }
    let groups = inference_groups(db, library);
    let items = crate::scopes::library_items(db, library);
    let mut diagnostics = Vec::new();
    for &(group, size) in oversized {
        let members = &groups.groups[group as usize];
        let sources = &groups.sources[group as usize];
        let count = members.len() + sources.len();
        let member_locs: rustc_hash::FxHashSet<&ItemLoc> =
            members.iter().map(|&m| &items[m].0).collect();
        // Each file warns for its own bodies.
        let bodies = members
            .iter()
            .map(|&m| &items[m].0)
            .chain(sources)
            .filter(|loc| loc.file == file);
        for body_loc in bodies {
            let item = body_loc.to_id(db);
            let own = body_loc;
            let (_, source_map) = crate::body::body_with_source_map(db, item);
            let mut first_use = None;
            let mut used: Vec<&ItemLoc> = Vec::new();
            let mut seen: rustc_hash::FxHashSet<&ItemLoc> = Default::default();
            for (expr, resolution) in crate::resolutions(db, item).iter() {
                let crate::Resolution::Item(target) = resolution else {
                    continue;
                };
                // Recursion is the item's own business, not a call into
                // the group.
                if target == own || !member_locs.contains(target) || seen.contains(target) {
                    continue;
                }
                let Some(ptr) = source_map.node_for_expr(expr) else {
                    continue;
                };
                first_use.get_or_insert(ptr.text_range());
                seen.insert(target);
                used.push(target);
            }
            let (Some(range), Some(first)) = (first_use, used.first()) else {
                continue;
            };
            let name = first.display_name();
            let what = match used.len() {
                1 => format!("`{name}` is missing a complete type signature, and it is"),
                2 => format!(
                    "`{name}` and 1 other item used here are missing complete type signatures, \
                     and they are"
                ),
                n => format!(
                    "`{name}` and {} other items used here are missing complete type \
                     signatures, and they are",
                    n - 1
                ),
            };
            diagnostics.push(crate::Diagnostic {
                range,
                severity: crate::Severity::Warning,
                message: format!(
                    "{what} inferred in a group of {count} items (with {size} expressions); \
                     inference this large may slow down your editor"
                ),
                fix: None,
                related: used
                    .iter()
                    .filter_map(|target| {
                        let name = crate::item_tree::item_source(db, target.to_id(db))?.name()?;
                        Some(crate::RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        })
                    })
                    .collect(),
            });
        }
    }
    diagnostics
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

/// What one group's inference produces: its members' signatures, and the
/// result of every body it checked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupSignatures {
    /// Parallel to the group's member list.
    pub signatures: Vec<Ty>,
    /// Parallel to the member list: whether uses of the member say "add a
    /// type annotation". Its signature has an `{error}` the group could not
    /// decide, and its own body isn't broken where it produces a value
    /// (that body already carries the error, so its uses say nothing).
    pub needs_annotation: Vec<bool>,
    /// Parallel to the member list: what the bodies whose uses left the
    /// member `{error}` disagree on.
    pub disagreements: Vec<Option<Disagreement>>,
    /// Each body's inference result: the members in member order, then the
    /// fully typed bodies feeding the group in [`InferenceGroups::sources`]
    /// order.
    pub results: Vec<std::sync::Arc<InferenceResult>>,
}

/// Why a body's use of a member lost (T11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outvoted {
    /// The member's type as this use makes it, rendered.
    pub makes: String,
    /// The bodies whose uses decided the member's type, in body order: the
    /// member alone when its own body did.
    pub deciders: Vec<ItemLoc>,
}

/// What the uses that leave a member `{error}` disagree on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disagreement {
    /// The member's uses tie: each body voting, in body order, with the
    /// type it makes the member, rendered.
    Member(Vec<(ItemLoc, String)>),
    /// The conflict shows only across members linked by a shared variable:
    /// all of them, and every body using one, in body order.
    Linked {
        members: Vec<ItemLoc>,
        callers: Vec<ItemLoc>,
    },
}

impl Disagreement {
    /// The message at a use of the member `name`.
    pub fn message(&self, name: &str) -> String {
        let quoted = |locs: &[ItemLoc]| {
            let names: Vec<String> = locs.iter().map(|loc| format!("`{}`", loc.name)).collect();
            names.join(", ")
        };
        match self {
            Disagreement::Member(callers) => {
                let makes: Vec<String> = callers
                    .iter()
                    .map(|(caller, ty)| format!("`{}` makes it `{ty}`", caller.name))
                    .collect();
                format!(
                    "callers disagree on the type of `{name}`: {}; \
                     add a type annotation to its definition",
                    makes.join(", ")
                )
            }
            // Members' bodies contradicting each other, with no caller
            // outside them.
            Disagreement::Linked { members, callers }
                if callers.iter().all(|caller| members.contains(caller)) =>
            {
                format!(
                    "{} disagree on each other's types; \
                     add type annotations to their definitions",
                    quoted(members),
                )
            }
            Disagreement::Linked { members, callers } => format!(
                "callers {} disagree on the types of {}, which are linked; \
                 add type annotations to their definitions",
                quoted(callers),
                quoted(members),
            ),
        }
    }

    /// The members and callers the message names, each once.
    pub fn named(&self) -> Vec<&ItemLoc> {
        let mut named: Vec<&ItemLoc> = Vec::new();
        let all: Vec<&ItemLoc> = match self {
            Disagreement::Member(callers) => callers.iter().map(|(caller, _)| caller).collect(),
            Disagreement::Linked { members, callers } => members.iter().chain(callers).collect(),
        };
        for loc in all {
            if !named.contains(&loc) {
                named.push(loc);
            }
        }
        named
    }
}

/// The result of `item`'s body when an inference group checked it: a
/// member, or a fully typed body feeding a group. `None` for every other
/// item.
pub fn group_result<'db>(
    db: &'db dyn Db,
    item: crate::ItemId<'db>,
) -> Option<std::sync::Arc<InferenceResult>> {
    let (group, position) = body_group(db, item)?;
    infer_group(db, GroupId::new(db, library_of(db, item.file(db)), group))
        .results
        .get(position as usize)
        .cloned()
}

/// The group that checks `item`'s body, and the body's position in its
/// [`GroupSignatures::results`]. Its own query, so a change to the library's
/// groups re-runs only the bodies whose group or position it changes.
/// Decided from the item alone where it can be, so an item that can't be
/// in a group never reads its library's groups.
#[salsa::tracked]
pub fn body_group<'db>(db: &'db dyn Db, item: crate::ItemId<'db>) -> Option<(u32, u32)> {
    let data = crate::item_data(db, item).as_ref()?;
    let fully_typed = data.type_ref.as_ref().is_some_and(|tr| tr.is_fully_typed());
    let candidate = if fully_typed {
        is_constraint_source(db, item)
    } else {
        item.member(db).is_none()
            && matches!(data.kind, crate::item_tree::ItemKind::Value(_))
            && data.generics.is_empty()
    };
    if !candidate {
        return None;
    }
    let groups = inference_groups(db, library_of(db, item.file(db)));
    let loc = item_loc(db, item);
    let slots = if fully_typed {
        &groups.source_slot
    } else {
        &groups.slot
    };
    let &(group, position) = slots.get(&loc)?;
    Some((group, position as u32))
}

/// Inference for one binding group: checks every body the group covers
/// once, in its own table, and returns the members' signatures and every
/// body's result. Callees are checked before their callers, and the uses
/// fill what their bodies left open together, then the projections waiting
/// for a receiver are retried ([`Solver::settle`]); T11 in
/// `docs/design/types-and-data.md`.
#[salsa::tracked(returns(ref))]
pub fn infer_group<'db>(db: &'db dyn Db, group: GroupId<'db>) -> GroupSignatures {
    let groups = inference_groups(db, group.library(db));
    let Some(members) = groups.groups.get(group.index(db) as usize) else {
        return GroupSignatures::default();
    };
    let ids = library_item_ids(db, group.library(db));
    let member_locs: Vec<ItemLoc> = members.iter().map(|&i| item_loc(db, ids[i])).collect();
    let member_of: FxHashMap<ItemLoc, usize> = member_locs
        .iter()
        .enumerate()
        .map(|(i, loc)| (loc.clone(), i))
        .collect();
    let member_items: Vec<crate::ItemId<'db>> = members.iter().map(|&i| ids[i]).collect();
    let member_count = members.len();
    // Per member, whether it is broken before its body is even checked:
    // its body has no value, or its written annotation doesn't lower.
    let wrong: Vec<Wrong> = member_items
        .iter()
        .map(|&item| wrong_as_a_whole(db, item))
        .collect();
    // In [`GroupSignatures::results`] order: the members first, so member
    // `m`'s body is `bodies[m]`, then the fully typed bodies.
    let bodies: Vec<GroupBody<'db>> = member_items
        .iter()
        .enumerate()
        .map(|(m, &item)| (item, Some(m)))
        .chain(
            groups.sources[group.index(db) as usize]
                .iter()
                .map(|source| (source.to_id(db), None)),
        )
        .map(|(item, own)| {
            let mut mentioned: Vec<usize> = crate::resolutions(db, item)
                .values()
                .filter_map(|resolution| match resolution {
                    crate::Resolution::Item(loc) => member_of.get(loc).copied(),
                    _ => None,
                })
                .chain(own)
                .collect();
            mentioned.sort_unstable();
            mentioned.dedup();
            GroupBody {
                item,
                own,
                mentioned,
            }
        })
        .collect();

    let mut solver = Solver {
        table: InPlaceUnificationTable::new(),
        constraints: Constraints::new(true),
        decided: vec![Ty::Error; member_count],
        own: vec![Ty::Error; member_count],
        met_error: vec![Vec::new(); member_count],
        fills: vec![Vec::new(); bodies.len()],
        body_locs: bodies.iter().map(|body| item_loc(db, body.item)).collect(),
        disagreements: vec![None; member_count],
        outvoted: vec![FxHashMap::default(); bodies.len()],
        links: vec![FxHashMap::default(); bodies.len()],
    };
    let mut ctxs: Vec<Option<InferCtx<'db>>> = (0..bodies.len()).map(|_| None).collect();

    // The members, a cycle of calls at a time, callees first.
    let calls: Vec<Vec<usize>> = (0..member_count)
        .map(|m| {
            let mentioned = &bodies[m].mentioned;
            mentioned.iter().copied().filter(|&x| x != m).collect()
        })
        .collect();
    for cycle in call_cycles(&calls) {
        // Per body: each calling body has a vote ([`unify_uses`]).
        let mut internal: Vec<Vec<(usize, Ty)>> = vec![Vec::new(); bodies.len()];
        for &m in &cycle {
            let mut ctx = solver.check(db, &member_items, &member_locs, &bodies[m], &cycle);
            internal[m] = solver.export(&mut ctx, &member_locs, &bodies[m], &cycle, m);
            ctxs[m] = Some(ctx);
        }
        for &m in &cycle {
            solver.decided[m] = match wrong[m] {
                Wrong::No => solver.own[m].clone(),
                _ => Ty::Error,
            };
        }
        solver.unify_uses(&internal);
    }
    // The members' uses fill what their callees left open, together.
    let member_fills: Vec<Vec<(usize, Ty)>> = bodies
        .iter()
        .zip(&mut solver.fills)
        .map(|(body, fills)| match body.own {
            Some(_) => std::mem::take(fills),
            None => Vec::new(),
        })
        .collect();
    solver.unify_uses(&member_fills);
    solver.settle(&mut ctxs, &member_locs, &bodies);

    let mut own_error = vec![false; member_count];
    for m in 0..member_count {
        // A member whose own body went wrong where it produces a value
        // carries its own error; its uses say nothing.
        let mut own_produced = rustc_hash::FxHashSet::default();
        solver.own[m].produced_vars(&mut own_produced);
        own_error[m] = wrong[m] == Wrong::OwnError
            || solver.own[m] == Ty::Error
            || solver.met_error[m]
                .iter()
                .any(|var| own_produced.contains(var));
        // A variable its body met an error on that is open in what the
        // member produces becomes `{error}`, so no fully typed body decides
        // what the body got wrong. Parameters stay open for uses.
        let mut produced = rustc_hash::FxHashSet::default();
        resolve_fully(&mut solver.table, &solver.decided[m]).produced_vars(&mut produced);
        for &var in &solver.met_error[m] {
            if let Ty::Infer(root) = resolve_fully(&mut solver.table, &Ty::Infer(var))
                && produced.contains(&root)
            {
                solver.table.union_value(root, TyVarValue::Known(Ty::Error));
            }
        }
    }

    // The fully typed bodies, against what the members decided; then their
    // fills, together.
    for (b, body) in bodies.iter().enumerate() {
        if body.own.is_none() {
            let mut ctx = solver.check(db, &member_items, &member_locs, body, &[]);
            solver.export(&mut ctx, &member_locs, body, &[], b);
            ctxs[b] = Some(ctx);
        }
    }
    let source_fills = std::mem::take(&mut solver.fills);
    solver.unify_uses(&source_fills);
    solver.settle(&mut ctxs, &member_locs, &bodies);

    // The decided signatures, as the group publishes them: what no body
    // determined is `{error}`, which the needs-annotation diagnostic is for.
    let signatures: Vec<Ty> = (0..member_count)
        .map(|m| erase_infer(&resolve_fully(&mut solver.table, &solver.decided[m])))
        .collect();
    let needs_annotation: Vec<bool> = signatures
        .iter()
        .zip(&own_error)
        .map(|(sig, &own_error)| !own_error && sig.contains_error())
        .collect();

    let mut results = Vec::with_capacity(bodies.len());
    for (b, (body, ctx)) in bodies.iter().zip(ctxs).enumerate() {
        let mut ctx = ctx.expect("every body of the group is checked");
        // A member's own body sees what its uses filled in, open parts
        // staying open, so its literals report only what nobody determined;
        // another member's copy gets the published signature, exactly as
        // a body outside the group would.
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        for &m in &body.mentioned {
            let exported = if body.own == Some(m) {
                solver.instance(m, &mut vars, &mut |kind| ctx.new_var(kind))
            } else if solver.outvoted[b].contains_key(&m) {
                // The use's own error: what follows from it is silent.
                Ty::Error
            } else {
                signatures[m].clone()
            };
            let copy = ctx.copies()[&member_locs[m]].clone();
            ctx.adopt_exported(&copy, &exported);
        }
        ctx.retry_deferred_projections();
        ctx.report_undetermined_projections();
        ctx.poison_left_unbound();
        let outvoted = &solver.outvoted[b];
        ctx.report_undetermined_members(|expr, loc| {
            let m = member_of[loc];
            if let Some(outvoted) = outvoted.get(&m) {
                return Some(InferenceDiagnostic::OutvotedUse {
                    expr,
                    item: loc.clone(),
                    ty: signatures[m].clone(),
                    outvoted: outvoted.clone(),
                });
            }
            needs_annotation[m].then(|| InferenceDiagnostic::NeedsAnnotation {
                expr,
                item: loc.clone(),
                // A generic body never joins a group.
                generic_caller: false,
                disagreement: solver.disagreements[m].clone(),
            })
        });
        results.push(std::sync::Arc::new(ctx.finish()));
    }

    GroupSignatures {
        signatures,
        needs_annotation,
        disagreements: solver.disagreements,
        results,
    }
}

/// What member `item` writes of its type, lowered into `table` with
/// everything else fresh: its annotation and its fn literal's parameters
/// and return.
fn written(db: &dyn Db, item: crate::ItemId<'_>, table: &mut InPlaceUnificationTable<TyVar>) -> Ty {
    use crate::body::ExprData;
    let file = item.file(db);
    let mut lower = |type_ref: Option<&crate::item_tree::TypeRef>| match type_ref {
        Some(type_ref) => lower_type_ref_in(db, file, type_ref, table, &ParamScope::default()),
        None => Ty::Infer(table.new_key(TyVarValue::Unknown)),
    };
    let annotation = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
        .map(|type_ref| lower(Some(type_ref)));
    let body = crate::body::body(db, item);
    let literal = match body.root.map(|root| &body.exprs[root]) {
        Some(ExprData::FnLiteral {
            params, ret_type, ..
        }) => {
            let params = params
                .iter()
                .map(|param| crate::infer::literal_param_ty(db, file, body, param, &mut lower))
                .collect();
            Some(Ty::fn_type_with(false, params, lower(ret_type.as_ref())))
        }
        _ => None,
    };
    match (annotation, literal) {
        // Both are written (`: _ = fn (n: usize) ...`): each fills the
        // other's holes. A conflict between them is the member's own
        // body's error, so the annotation alone stands then.
        (Some(annotation), Some(literal)) => {
            let mut constraints = Constraints::default();
            let snapshot = constraints.snapshot(table);
            if constraints.unify_exported(table, &annotation, &literal) {
                constraints.commit(table, snapshot);
            } else {
                constraints.rollback_to(table, snapshot);
            }
            annotation
        }
        (Some(written), None) | (None, Some(written)) => written,
        (None, None) => lower(None),
    }
}

/// Why member `item` is wrong as a whole before its body is checked, if it
/// is.
fn wrong_as_a_whole(db: &dyn Db, item: crate::ItemId<'_>) -> Wrong {
    if crate::body::body(db, item).root.is_none() {
        return Wrong::OwnError;
    }
    let broken = crate::item_data(db, item)
        .as_ref()
        .and_then(|it| it.type_ref.as_ref())
        .is_some_and(|type_ref| {
            lower_type_ref_in(
                db,
                item.file(db),
                type_ref,
                &mut InPlaceUnificationTable::new(),
                &ParamScope::default(),
            )
            .contains_error()
        });
    if broken { Wrong::Contract } else { Wrong::No }
}

/// See [`wrong_as_a_whole`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wrong {
    No,
    /// Its body has no value.
    OwnError,
    /// Its written annotation is broken.
    Contract,
}

/// A body the group checks: a member's, or a fully typed body feeding it.
struct GroupBody<'db> {
    item: crate::ItemId<'db>,
    /// The member whose body this is; `None` for a fully typed body.
    own: Option<usize>,
    /// The members it mentions, sorted, `own` included.
    mentioned: Vec<usize>,
}

/// The members' signatures as the group decides them, in a table of
/// their own (no body's variables are in it).
struct Solver {
    table: InPlaceUnificationTable<TyVar>,
    /// Errors abstain: a body's own mistake decides nothing about a member.
    constraints: Constraints,
    /// Per member, its decided signature.
    decided: Vec<Ty>,
    /// Per member, its own body's copy of its signature.
    own: Vec<Ty>,
    /// Per member, the variables its body met an error on.
    met_error: Vec<Vec<TyVar>>,
    /// Per body, its uses of members decided before it was checked: what
    /// they fill in, kept apart until all of them fill together.
    fills: Vec<Vec<(usize, Ty)>>,
    /// Per body, its item.
    body_locs: Vec<ItemLoc>,
    /// Per member, what the uses that left it `{error}` disagree on.
    disagreements: Vec<Option<Disagreement>>,
    /// Per body, its uses of members that lost, by member.
    outvoted: Vec<FxHashMap<usize, Outvoted>>,
    /// Per body, the solver variable each of its exported variables became,
    /// so what the body learns later lands on the same variables.
    links: Vec<FxHashMap<TyVar, TyVar>>,
}

impl Solver {
    /// An instance of member `m`'s decided signature in another table: each
    /// variable becomes the one `vars` has for it, else a new one `mint`
    /// makes of its kind, so the links between them survive.
    fn instance(
        &mut self,
        m: usize,
        vars: &mut FxHashMap<TyVar, Ty>,
        mint: &mut impl FnMut(TyVarValue) -> Ty,
    ) -> Ty {
        let decided = resolve_fully(&mut self.table, &self.decided[m]);
        export_ty(&decided, &mut |var| {
            let kind = self.table.probe_value(var);
            vars.entry(var).or_insert_with(|| mint(kind)).clone()
        })
    }

    /// Check `body`, with a copy of the signature of each member it
    /// mentions: fresh for itself, what a member of its `cycle` writes,
    /// else an instance of what the member's body decided.
    fn check<'db>(
        &mut self,
        db: &'db dyn Db,
        member_items: &[crate::ItemId<'db>],
        member_locs: &[ItemLoc],
        body: &GroupBody<'db>,
        cycle: &[usize],
    ) -> InferCtx<'db> {
        let mut table = InPlaceUnificationTable::new();
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        let copies = body
            .mentioned
            .iter()
            .map(|&m| {
                let copy = if body.own == Some(m) {
                    Ty::Infer(table.new_key(TyVarValue::Unknown))
                } else if cycle.contains(&m) {
                    // What its annotation writes is final here too; only
                    // its holes are the cycle's to decide.
                    written(db, member_items[m], &mut table)
                } else {
                    self.instance(m, &mut vars, &mut |kind| Ty::Infer(table.new_key(kind)))
                };
                (member_locs[m].clone(), copy)
            })
            .collect();
        let mut ctx = crate::infer::check_item_body(db, body.item, table, copies, true);
        // The body's own joins are settled by the body alone.
        ctx.solve();
        ctx
    }

    /// Move `body`'s copies into the solver's table (`b` is its index in
    /// the group's bodies), one variable per variable of the body's table so
    /// the links between them survive: its own copy into [`Self::own`], its
    /// uses of members outside its `cycle` into [`Self::fills`]. Returns its
    /// uses of the cycle's members.
    fn export(
        &mut self,
        ctx: &mut InferCtx<'_>,
        member_locs: &[ItemLoc],
        body: &GroupBody<'_>,
        cycle: &[usize],
        b: usize,
    ) -> Vec<(usize, Ty)> {
        let mut internal = Vec::new();
        for (m, moved) in self.moved_copies(ctx, member_locs, &body.mentioned, b) {
            if body.own == Some(m) {
                self.own[m] = if ctx.root_has_error() {
                    Ty::Error
                } else {
                    moved
                };
            } else if cycle.contains(&m) {
                internal.push((m, moved));
            } else {
                self.fills[b].push((m, moved));
            }
        }
        self.record_errors(ctx, body, b);
        internal
    }

    /// Body `b`'s copy of each member it mentions, moved into the solver's
    /// table through [`Self::links`].
    fn moved_copies(
        &mut self,
        ctx: &mut InferCtx<'_>,
        member_locs: &[ItemLoc],
        mentioned: &[usize],
        b: usize,
    ) -> Vec<(usize, Ty)> {
        let links = &mut self.links[b];
        let table = &mut self.table;
        mentioned
            .iter()
            .map(|&m| {
                let copy = ctx.copies()[&member_locs[m]].clone();
                let resolved = ctx.resolve(&copy);
                let moved = export_ty(&resolved, &mut |var| {
                    Ty::Infer(
                        *links
                            .entry(var)
                            .or_insert_with(|| table.new_key(ctx.var_kind(var))),
                    )
                });
                (m, moved)
            })
            .collect()
    }

    /// The variables a member's `body` (body `b`) met an error on, in the
    /// solver's table.
    fn record_errors(&mut self, ctx: &mut InferCtx<'_>, body: &GroupBody<'_>, b: usize) {
        let Some(own) = body.own else {
            return;
        };
        let errors = ctx.left_unbound().to_vec();
        self.met_error[own] = errors
            .into_iter()
            .filter_map(|var| match ctx.resolve(&Ty::Infer(var)) {
                Ty::Infer(root) => self.links[b].get(&root).copied(),
                _ => None,
            })
            .collect();
    }

    /// Retry the field accesses and indexes still waiting for their
    /// receiver, against what the group has decided so far, until none
    /// moves: each waiting body sees the decided signatures of the members
    /// it mentions, and what its projections then determine joins the
    /// decided signatures like any use. One body's projection can
    /// determine another's receiver, whatever order they come in.
    fn settle(
        &mut self,
        ctxs: &mut [Option<InferCtx<'_>>],
        member_locs: &[ItemLoc],
        bodies: &[GroupBody<'_>],
    ) {
        loop {
            let mut progressed = false;
            for b in 0..ctxs.len() {
                let Some(ctx) = ctxs[b].as_mut() else {
                    continue;
                };
                if !ctx.has_deferred_projections() {
                    continue;
                }
                let body = &bodies[b];
                self.import(ctx, member_locs, &body.mentioned, b);
                if !ctx.retry_deferred_projections() {
                    continue;
                }
                progressed = true;
                let moved = self.moved_copies(ctx, member_locs, &body.mentioned, b);
                if let Some(own) = body.own {
                    self.own[own] = if ctx.root_has_error() {
                        Ty::Error
                    } else {
                        moved
                            .iter()
                            .find(|(m, _)| *m == own)
                            .map_or(Ty::Error, |(_, ty)| ty.clone())
                    };
                }
                self.record_errors(ctx, body, b);
                let mut uses = vec![Vec::new(); ctxs.len()];
                uses[b] = moved;
                self.unify_uses(&uses);
            }
            if !progressed {
                return;
            }
        }
    }

    /// Give body `b`'s copies what the group decided for them, through
    /// [`Self::links`]: a solver variable the body has a variable for
    /// becomes that one (two of them that the solver joined are joined).
    fn import(
        &mut self,
        ctx: &mut InferCtx<'_>,
        member_locs: &[ItemLoc],
        mentioned: &[usize],
        b: usize,
    ) {
        let mut back: FxHashMap<TyVar, Ty> = FxHashMap::default();
        let links: Vec<(TyVar, TyVar)> = self.links[b].iter().map(|(&k, &v)| (k, v)).collect();
        for (body_var, solver_var) in links {
            // A decided safety reaches the body through the copies below.
            if matches!(self.table.probe_value(solver_var), TyVarValue::Safety(_)) {
                continue;
            }
            if let Ty::Infer(root) = resolve_fully(&mut self.table, &Ty::Infer(solver_var)) {
                match back.get(&root) {
                    Some(other) => {
                        let other = other.clone();
                        ctx.adopt_exported(&Ty::Infer(body_var), &other);
                    }
                    None => {
                        back.insert(root, Ty::Infer(body_var));
                    }
                }
            }
        }
        for &m in mentioned {
            let decided = resolve_fully(&mut self.table, &self.decided[m]);
            let links = &mut self.links[b];
            let table = &mut self.table;
            let imported = export_ty(&decided, &mut |var| {
                back.entry(var)
                    .or_insert_with(|| {
                        let fresh = ctx.new_var(table.probe_value(var));
                        if let Ty::Infer(body_var) = fresh {
                            links.insert(body_var, var);
                        }
                        fresh
                    })
                    .clone()
            });
            let copy = ctx.copies()[&member_locs[m]].clone();
            ctx.adopt_exported(&copy, &imported);
        }
    }

    fn unify_uses(&mut self, uses: &[Vec<(usize, Ty)>]) {
        let verdicts = unify_uses(
            &mut self.table,
            &mut self.constraints,
            &mut self.decided,
            &self.body_locs,
            uses,
        );
        for (m, disagreement) in verdicts.undecided {
            self.disagreements[m].get_or_insert(disagreement);
        }
        for (b, m, outvoted) in verdicts.outvoted {
            self.outvoted[b].entry(m).or_insert(outvoted);
        }
    }
}

/// The cycles of `calls` (its strongly connected components), each sorted,
/// callees before their callers.
fn call_cycles(calls: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut state = Tarjan {
        edges: calls,
        include: &vec![true; calls.len()],
        index: vec![None; calls.len()],
        low: vec![0; calls.len()],
        on_stack: vec![false; calls.len()],
        stack: Vec::new(),
        next_index: 0,
        groups: Vec::new(),
    };
    for node in 0..calls.len() {
        if state.index[node].is_none() {
            state.visit(node);
        }
    }
    // Tarjan finishes a component only after everything it reaches.
    state.groups
}

/// What [`unify_uses`] concluded about the uses it could not unify.
#[derive(Default)]
struct Verdicts {
    /// Members left undecided (`{error}`), with what their uses disagree on.
    undecided: Vec<(usize, Disagreement)>,
    /// Body, member and why: uses that lost to the member's own body or to
    /// the other uses.
    outvoted: Vec<(usize, usize, Outvoted)>,
}

/// Unify each body's `uses` with the members' `decided` signatures (T11).
/// Members linked by a shared variable are decided as one unit, whatever
/// order they come in. `bodies` names the bodies `uses` belongs to.
fn unify_uses(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &mut [Ty],
    bodies: &[ItemLoc],
    uses: &[Vec<(usize, Ty)>],
) -> Verdicts {
    let mut used: Vec<usize> = uses.iter().flatten().map(|&(m, _)| m).collect();
    used.sort_unstable();
    used.dedup();
    // Link the used members through the variables their signatures and
    // uses share.
    let mut unit: FxHashMap<usize, usize> = used.iter().map(|&m| (m, m)).collect();
    fn root(unit: &mut FxHashMap<usize, usize>, i: usize) -> usize {
        let parent = unit[&i];
        if parent == i {
            return i;
        }
        let r = root(unit, parent);
        unit.insert(i, r);
        r
    }
    let mut owner: FxHashMap<TyVar, usize> = FxHashMap::default();
    for &m in &used {
        let mut vars = rustc_hash::FxHashSet::default();
        resolve_fully(table, &decided[m]).open_vars(&mut vars);
        for (_, ty) in uses.iter().flatten().filter(|(of, _)| *of == m) {
            resolve_fully(table, ty).open_vars(&mut vars);
        }
        for var in vars {
            let other = *owner.entry(var).or_insert(m);
            let (a, b) = (root(&mut unit, m), root(&mut unit, other));
            unit.insert(a, b);
        }
    }
    let mut units: Vec<Vec<usize>> = Vec::new();
    let mut unit_at: FxHashMap<usize, usize> = FxHashMap::default();
    for &m in &used {
        let head = root(&mut unit, m);
        let at = *unit_at.entry(head).or_insert_with(|| {
            units.push(Vec::new());
            units.len() - 1
        });
        units[at].push(m);
    }
    let mut verdicts = Verdicts::default();
    for members in units {
        let live: Vec<(usize, Vec<(usize, &Ty)>)> = uses
            .iter()
            .enumerate()
            .filter_map(|(b, body_uses)| {
                let in_unit: Vec<(usize, &Ty)> = body_uses
                    .iter()
                    .filter(|(m, _)| members.contains(m))
                    .map(|(m, ty)| (*m, ty))
                    .collect();
                (!in_unit.is_empty()).then_some((b, in_unit))
            })
            .collect();
        let mut unit = Unit {
            table: &mut *table,
            constraints: &mut *constraints,
            decided: &mut *decided,
            bodies,
            members,
            live,
            verdicts: &mut verdicts,
        };
        unit.decide();
    }
    verdicts
}

/// One unit of [`unify_uses`]: members linked by shared variables, and
/// the uses of them still in play, per body.
struct Unit<'a, 't> {
    table: &'a mut InPlaceUnificationTable<TyVar>,
    constraints: &'a mut Constraints,
    decided: &'a mut [Ty],
    bodies: &'a [ItemLoc],
    members: Vec<usize>,
    live: Vec<(usize, Vec<(usize, &'t Ty)>)>,
    verdicts: &'a mut Verdicts,
}

impl Unit<'_, '_> {
    /// A use that contradicts what its member's own body decided loses to
    /// it. Then each shared variable (member places that one body's uses
    /// make the same type) is decided by a plurality vote: each body votes
    /// once for the concrete type its uses give it, and a body that leaves
    /// it open abstains. The other voters are outvoted; a tie leaves every
    /// member the variable is in undecided. A use the winners still
    /// contradict is outvoted by them, and when the rest still conflict
    /// the unit's remaining members are undecided.
    fn decide(&mut self) {
        if self.agree_all() {
            return;
        }
        // What is already decided of each member: in a cycle, what its
        // own body decided.
        for (b, body_uses) in self.live.clone() {
            let mut lost: Vec<usize> = body_uses
                .iter()
                .filter(|&&(m, ty)| !self.probe(&[(m, ty)]))
                .map(|&(m, _)| m)
                .collect();
            if lost.is_empty() && !self.probe(&body_uses) {
                lost = body_uses.iter().map(|&(m, _)| m).collect();
            }
            for m in lost {
                let deciders = vec![self.bodies[m].clone()];
                self.outvote(b, m, deciders);
            }
        }

        // The variables the unit's members leave open, in member order.
        let mut vars: Vec<TyVar> = Vec::new();
        for &m in &self.members {
            let mut open = rustc_hash::FxHashSet::default();
            resolve_fully(self.table, &self.decided[m]).open_vars(&mut open);
            let mut open: Vec<TyVar> = open.into_iter().filter(|v| !vars.contains(v)).collect();
            open.sort_by_key(|v| v.index());
            vars.extend(open);
        }
        // What each body's uses alone make each variable.
        let views: Vec<(usize, Vec<Ty>)> = self
            .live
            .clone()
            .into_iter()
            .map(|(b, body_uses)| {
                let snapshot = self.constraints.snapshot(self.table);
                agree_uses(self.table, self.constraints, self.decided, &body_uses);
                let view = vars
                    .iter()
                    .map(|&v| resolve_fully(self.table, &Ty::Infer(v)))
                    .collect();
                self.constraints.rollback_to(self.table, snapshot);
                (b, view)
            })
            .collect();
        // The shared variables: the places some body's uses make one.
        let mut class: Vec<usize> = (0..vars.len()).collect();
        fn class_of(class: &mut [usize], i: usize) -> usize {
            if class[i] == i {
                return i;
            }
            let r = class_of(class, class[i]);
            class[i] = r;
            r
        }
        for (_, view) in &views {
            for i in 0..vars.len() {
                for j in i + 1..vars.len() {
                    if matches!(view[i], Ty::Infer(_)) && view[i] == view[j] {
                        let (a, b) = (class_of(&mut class, i), class_of(&mut class, j));
                        class[b] = a;
                    }
                }
            }
        }
        let mut winners: Vec<usize> = Vec::new();
        for head in 0..vars.len() {
            let places: Vec<usize> = (0..vars.len())
                .filter(|&i| class_of(&mut class, i) == head)
                .collect();
            if places.is_empty() {
                continue;
            }
            winners.extend(self.vote(&vars, &views, &places));
        }

        if self.agree_all() {
            return;
        }
        // Uses the winners still contradict.
        winners.sort_unstable();
        winners.dedup();
        for (b, body_uses) in self.live.clone() {
            if !self.probe(&body_uses) {
                for (m, _) in body_uses {
                    let mut deciders: Vec<ItemLoc> = winners
                        .iter()
                        .filter(|&&w| w != b)
                        .map(|&w| self.bodies[w].clone())
                        .collect();
                    if deciders.is_empty() {
                        deciders.push(self.bodies[m].clone());
                    }
                    self.outvote(b, m, deciders);
                }
            }
        }
        if self.agree_all() {
            return;
        }
        let members: Vec<usize> = self
            .members
            .iter()
            .copied()
            .filter(|m| {
                self.live
                    .iter()
                    .any(|(_, uses)| uses.iter().any(|(of, _)| of == m))
            })
            .collect();
        let linked = self.linked(&members);
        self.undecide(&members, linked);
    }

    /// Vote on the shared variable at `places` (indices into `vars`).
    /// Returns the bodies whose vote won.
    fn vote(&mut self, vars: &[TyVar], views: &[(usize, Vec<Ty>)], places: &[usize]) -> Vec<usize> {
        // Each concrete type a body's uses give the variable, with the
        // bodies voting for it.
        let mut tally: Vec<(Ty, Vec<usize>)> = Vec::new();
        for &(l, ref view) in views {
            for &i in places {
                let ty = &view[i];
                if ty.contains_infer() || ty.contains_error() {
                    continue;
                }
                // A member's own open safety votes as the safe it defaults
                // to: every body leaves it open unless it says otherwise.
                let key = match ty.erase_regions() {
                    Ty::Fn(f) if matches!(f.safety, Safety::Infer(_)) => {
                        f.with_safety(Safety::Safe)
                    }
                    key => key,
                };
                match tally.iter_mut().find(|(voted, _)| *voted == key) {
                    Some((_, voters)) if voters.contains(&l) => {}
                    Some((_, voters)) => voters.push(l),
                    None => tally.push((key, vec![l])),
                }
            }
        }
        let Some(most) = tally.iter().map(|(_, voters)| voters.len()).max() else {
            return Vec::new();
        };
        let leaders: Vec<usize> = (0..tally.len())
            .filter(|&t| tally[t].1.len() == most)
            .collect();
        // The members the variable is in.
        let at: Vec<usize> = self
            .members
            .iter()
            .copied()
            .filter(|&m| {
                let mut open = rustc_hash::FxHashSet::default();
                resolve_fully(self.table, &self.decided[m]).open_vars(&mut open);
                places.iter().any(|&i| open.contains(&vars[i]))
            })
            .collect();
        if let [winner] = leaders[..] {
            let snapshot = self.constraints.snapshot(self.table);
            let (ty, voters) = (tally[winner].0.clone(), tally[winner].1.clone());
            let fits = places.iter().all(|&i| {
                self.constraints
                    .unify_exported(self.table, &Ty::Infer(vars[i]), &ty)
            });
            if fits {
                self.constraints.commit(self.table, snapshot);
                let deciders: Vec<ItemLoc> =
                    voters.iter().map(|&b| self.bodies[b].clone()).collect();
                let losers: Vec<usize> = tally
                    .iter()
                    .enumerate()
                    .filter(|&(t, _)| t != winner)
                    .flat_map(|(_, (_, voters))| voters.iter().copied())
                    .collect();
                for b in losers {
                    for &m in &at {
                        self.outvote(b, m, deciders.clone());
                    }
                }
                return voters;
            }
            self.constraints.rollback_to(self.table, snapshot);
        }
        // A tie, each side as honest as the other; or a winner the
        // variable's places can't all take.
        let disagreement = if let [m] = at[..] {
            let voters: Vec<usize> = {
                let mut voters: Vec<usize> =
                    tally.iter().flat_map(|(_, v)| v.iter().copied()).collect();
                voters.sort_unstable();
                voters.dedup();
                voters
            };
            let makes = voters
                .into_iter()
                .filter_map(|b| {
                    let (_, body_uses) = self.live.iter().find(|(of, _)| *of == b)?;
                    let &(_, ty) = body_uses.iter().find(|(of, _)| *of == m)?;
                    Some((
                        self.bodies[b].clone(),
                        resolve_finished(self.table, ty).display(),
                    ))
                })
                .collect();
            Disagreement::Member(makes)
        } else {
            self.linked(&at)
        };
        self.undecide(&at, disagreement);
        Vec::new()
    }

    /// The disagreement among `members`: them, and every body using one.
    fn linked(&self, members: &[usize]) -> Disagreement {
        Disagreement::Linked {
            members: members.iter().map(|&m| self.bodies[m].clone()).collect(),
            callers: self
                .live
                .iter()
                .filter(|(_, uses)| uses.iter().any(|(m, _)| members.contains(m)))
                .map(|&(b, _)| self.bodies[b].clone())
                .collect(),
        }
    }

    /// Leave `members` undecided (`{error}`), their uses out of play.
    fn undecide(&mut self, members: &[usize], disagreement: Disagreement) {
        for &m in members {
            self.verdicts.undecided.push((m, disagreement.clone()));
            self.decided[m] = Ty::Error;
        }
        for (_, uses) in &mut self.live {
            uses.retain(|(m, _)| !members.contains(m));
        }
        self.live.retain(|(_, uses)| !uses.is_empty());
    }

    /// Take body `b`'s use of member `m` out of play: `deciders` decided
    /// the member otherwise.
    fn outvote(&mut self, b: usize, m: usize, deciders: Vec<ItemLoc>) {
        let Some(l) = self.live.iter().position(|&(of, _)| of == b) else {
            return;
        };
        let Some(at) = self.live[l].1.iter().position(|&(of, _)| of == m) else {
            return;
        };
        let (_, ty) = self.live[l].1.remove(at);
        let makes = resolve_finished(self.table, ty).display();
        self.verdicts
            .outvoted
            .push((b, m, Outvoted { makes, deciders }));
        if self.live[l].1.is_empty() {
            self.live.remove(l);
        }
    }

    /// Whether `uses` agree with the decided signatures, leaving the table
    /// as it was.
    fn probe(&mut self, uses: &[(usize, &Ty)]) -> bool {
        let snapshot = self.constraints.snapshot(self.table);
        let agrees = agree_uses(self.table, self.constraints, self.decided, uses);
        self.constraints.rollback_to(self.table, snapshot);
        agrees
    }

    /// Unify every use still in play, keeping the result only if they all
    /// agree.
    fn agree_all(&mut self) -> bool {
        let snapshot = self.constraints.snapshot(self.table);
        let all: Vec<(usize, &Ty)> = self
            .live
            .iter()
            .flat_map(|(_, uses)| uses.iter().copied())
            .collect();
        if agree_uses(self.table, self.constraints, self.decided, &all) {
            self.constraints.commit(self.table, snapshot);
            true
        } else {
            self.constraints.rollback_to(self.table, snapshot);
            false
        }
    }
}

/// Unify each of `uses` with its member's decided signature.
fn agree_uses(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &[Ty],
    uses: &[(usize, &Ty)],
) -> bool {
    // Exact, safety included: a body's copy is the member's type, not a
    // value meeting a position, so its safety is the member's (#168).
    uses.iter()
        .all(|&(m, ty)| constraints.unify_exported(table, &decided[m], ty))
}
