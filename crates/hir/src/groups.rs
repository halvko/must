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
use ena::unify::InPlaceUnificationTable;
use rustc_hash::FxHashMap;

use crate::constraint::{Constraints, resolve_fully};
use crate::infer::{InferCtx, InferenceResult};
use crate::library::{LibraryId, library_item_ids, library_of};
use crate::ty::{ParamScope, Ty, TyVar, TyVarValue, erase_infer, export_ty, lower_type_ref_in};
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
    /// Each body's inference result: the members in member order, then the
    /// fully typed bodies feeding the group in [`InferenceGroups::sources`]
    /// order.
    pub results: Vec<std::sync::Arc<InferenceResult>>,
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
/// fill what their bodies left open together (T11 in
/// `docs/design/types-and-data.md`).
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
    let bodies: Vec<crate::ItemId<'db>> = members
        .iter()
        .map(|&i| ids[i])
        .chain(
            groups.sources[group.index(db) as usize]
                .iter()
                .map(|source| source.to_id(db)),
        )
        .collect();
    let member_count = members.len();
    let wrong: Vec<Wrong> = (0..member_count)
        .map(|m| wrong_as_a_whole(db, bodies[m]))
        .collect();
    // The members each body mentions; a member always has its own copy.
    let mentioned: Vec<Vec<usize>> = bodies
        .iter()
        .enumerate()
        .map(|(b, &item)| {
            let mut mentioned: Vec<usize> = crate::resolutions(db, item)
                .values()
                .filter_map(|resolution| match resolution {
                    crate::Resolution::Item(loc) => member_of.get(loc).copied(),
                    _ => None,
                })
                .chain((b < member_count).then_some(b))
                .collect();
            mentioned.sort_unstable();
            mentioned.dedup();
            mentioned
        })
        .collect();

    let mut solver = Solver {
        table: InPlaceUnificationTable::new(),
        constraints: Constraints::new(true),
        decided: vec![Ty::Error; member_count],
        own: vec![Ty::Error; member_count],
        met_error: vec![Vec::new(); member_count],
        fills: vec![Vec::new(); bodies.len()],
    };
    let mut ctxs: Vec<Option<InferCtx<'db>>> = (0..bodies.len()).map(|_| None).collect();

    // The members, a cycle of calls at a time, callees first.
    let calls: Vec<Vec<usize>> = (0..member_count)
        .map(|m| mentioned[m].iter().copied().filter(|&x| x != m).collect())
        .collect();
    for cycle in call_cycles(&calls) {
        let mut internal: Vec<Vec<(usize, Ty)>> = vec![Vec::new(); bodies.len()];
        for &b in &cycle {
            let mut ctx = solver.check(db, &bodies, &member_locs, &mentioned[b], &cycle, b);
            internal[b] = solver.export(&mut ctx, &member_locs, &mentioned[b], &cycle, b);
            ctxs[b] = Some(ctx);
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
    let member_fills: Vec<Vec<(usize, Ty)>> = (0..bodies.len())
        .map(|b| {
            if b < member_count {
                std::mem::take(&mut solver.fills[b])
            } else {
                Vec::new()
            }
        })
        .collect();
    solver.unify_uses(&member_fills);

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
    for b in member_count..bodies.len() {
        let mut ctx = solver.check(db, &bodies, &member_locs, &mentioned[b], &[], b);
        solver.export(&mut ctx, &member_locs, &mentioned[b], &[], b);
        ctxs[b] = Some(ctx);
    }
    let source_fills = std::mem::take(&mut solver.fills);
    solver.unify_uses(&source_fills);

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
    for (b, ctx) in ctxs.into_iter().enumerate() {
        let mut ctx = ctx.expect("every body of the group is checked");
        // A member's own body sees what its uses filled in, open parts
        // staying open, so its literals report only what nobody determined;
        // another member's copy gets the published signature, exactly as
        // a body outside the group would.
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        for &m in &mentioned[b] {
            let exported = if m == b {
                solver.instance(m, &mut vars, &mut |kind| ctx.new_var(kind))
            } else {
                signatures[m].clone()
            };
            let copy = ctx.copies()[&member_locs[m]].clone();
            ctx.adopt_exported(&copy, &exported);
        }
        ctx.poison_left_unbound();
        ctx.report_undetermined_members(|loc| needs_annotation[member_of[loc]]);
        results.push(std::sync::Arc::new(ctx.finish()));
    }

    GroupSignatures {
        signatures,
        needs_annotation,
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

    /// Check body `b`, with a copy of the signature of each member it
    /// mentions: fresh for itself, what a member of its `cycle` writes,
    /// else an instance of what the member's body decided.
    fn check<'db>(
        &mut self,
        db: &'db dyn Db,
        bodies: &[crate::ItemId<'db>],
        member_locs: &[ItemLoc],
        mentioned: &[usize],
        cycle: &[usize],
        b: usize,
    ) -> InferCtx<'db> {
        let mut table = InPlaceUnificationTable::new();
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        let copies = mentioned
            .iter()
            .map(|&m| {
                let copy = if m == b {
                    Ty::Infer(table.new_key(TyVarValue::Unknown))
                } else if cycle.contains(&m) {
                    // What its annotation writes is final here too; only
                    // its holes are the cycle's to decide.
                    written(db, bodies[m], &mut table)
                } else {
                    self.instance(m, &mut vars, &mut |kind| Ty::Infer(table.new_key(kind)))
                };
                (member_locs[m].clone(), copy)
            })
            .collect();
        let mut ctx = crate::infer::check_item_body(db, bodies[b], table, copies, true);
        // The body's own joins are settled by the body alone.
        ctx.solve();
        ctx
    }

    /// Move body `b`'s copies into the solver's table, one variable per
    /// variable of the body's table so the links between them survive: its
    /// own copy into [`Self::own`], its uses of members outside its
    /// `cycle` into [`Self::fills`]. Returns its uses of the cycle's
    /// members.
    fn export(
        &mut self,
        ctx: &mut InferCtx<'_>,
        member_locs: &[ItemLoc],
        mentioned: &[usize],
        cycle: &[usize],
        b: usize,
    ) -> Vec<(usize, Ty)> {
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        let mut internal = Vec::new();
        for &m in mentioned {
            let copy = ctx.copies()[&member_locs[m]].clone();
            let resolved = ctx.resolve(&copy);
            let moved = export_ty(&resolved, &mut |var| {
                vars.entry(var)
                    .or_insert_with(|| Ty::Infer(self.table.new_key(ctx.var_kind(var))))
                    .clone()
            });
            if m == b {
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
        if b < self.own.len() {
            let errors = ctx.left_unbound().to_vec();
            self.met_error[b] = errors
                .into_iter()
                .filter_map(|var| match ctx.resolve(&Ty::Infer(var)) {
                    Ty::Infer(root) => match vars.get(&root) {
                        Some(Ty::Infer(moved)) => Some(*moved),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
        }
        internal
    }

    fn unify_uses(&mut self, uses: &[Vec<(usize, Ty)>]) {
        unify_uses(
            &mut self.table,
            &mut self.constraints,
            &mut self.decided,
            uses,
        );
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

/// Unify each body's `uses` with the members' `decided` signatures,
/// together. Members linked by a shared variable succeed or fail as one,
/// whatever order they come in. When a unit fails, the members whose own
/// uses disagree are left undecided (`{error}`) and the rest are tried
/// again; when no member's uses disagree alone, the whole unit is.
fn unify_uses(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &mut [Ty],
    uses: &[Vec<(usize, Ty)>],
) {
    // Each used member's uses; members nobody uses here are untouched.
    let mut uses_of: FxHashMap<usize, Vec<&Ty>> = FxHashMap::default();
    for (m, ty) in uses.iter().flatten() {
        uses_of.entry(*m).or_default().push(ty);
    }
    let mut used: Vec<usize> = uses_of.keys().copied().collect();
    used.sort_unstable();
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
        for ty in &uses_of[&m] {
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
    for mut members in units {
        while !members.is_empty() {
            let snapshot = constraints.snapshot(table);
            if agree(table, constraints, decided, &uses_of, &members) {
                constraints.commit(table, snapshot);
                break;
            }
            constraints.rollback_to(table, snapshot);
            let disagreeing: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&m| !probe_agree(table, constraints, decided, &uses_of, &[m]))
                .collect();
            let disagreeing = if disagreeing.is_empty() {
                // The conflict is only through shared variables: blame the
                // members whose removal lets the rest agree, or all of them
                // if none does.
                let needed = in_conflict(table, constraints, decided, &uses_of, &members);
                if needed.is_empty() {
                    members.clone()
                } else {
                    needed
                }
            } else {
                disagreeing
            };
            for &m in &disagreeing {
                decided[m] = Ty::Error;
            }
            members.retain(|m| !disagreeing.contains(m));
        }
    }
}

/// The `members` whose removal lets the rest agree.
fn in_conflict(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &[Ty],
    uses_of: &FxHashMap<usize, Vec<&Ty>>,
    members: &[usize],
) -> Vec<usize> {
    members
        .iter()
        .copied()
        .filter(|&m| {
            let rest: Vec<usize> = members.iter().copied().filter(|&o| o != m).collect();
            probe_agree(table, constraints, decided, uses_of, &rest)
        })
        .collect()
}

/// Whether the uses of `members` agree with their decided signatures,
/// leaving the table as it was.
fn probe_agree(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &[Ty],
    uses_of: &FxHashMap<usize, Vec<&Ty>>,
    members: &[usize],
) -> bool {
    let snapshot = constraints.snapshot(table);
    let agrees = agree(table, constraints, decided, uses_of, members);
    constraints.rollback_to(table, snapshot);
    agrees
}

/// Unify the uses of each of `members` with its decided signature.
fn agree(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &[Ty],
    uses_of: &FxHashMap<usize, Vec<&Ty>>,
    members: &[usize],
) -> bool {
    members.iter().all(|&m| {
        uses_of[&m]
            .iter()
            .all(|ty| unify_use(table, constraints, &decided[m], ty))
    })
}

/// Unify one use with a member's decided signature. Exact, safety
/// included: a body's copy is the member's type, not a value meeting a
/// position, so where the body passed it as an `unsafe fn` its own safety
/// stayed open for this to decide (#168).
fn unify_use(
    table: &mut InPlaceUnificationTable<TyVar>,
    constraints: &mut Constraints,
    decided: &Ty,
    use_ty: &Ty,
) -> bool {
    constraints.unify_exported(table, decided, use_ty)
}
