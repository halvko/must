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
    /// By member position: why the member is `{error}`, when its uses
    /// disagree.
    pub disagreements: FxHashMap<usize, Disagreement>,
    /// Each body's inference result: the members in member order, then the
    /// fully typed bodies feeding the group in [`InferenceGroups::sources`]
    /// order.
    pub results: Vec<std::sync::Arc<InferenceResult>>,
}

/// A use of a member that lost to its own body or to the other uses (T11).
/// `B` names a body: an [`ItemLoc`] once published, before that an index
/// into the group's bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outvoted<B = ItemLoc> {
    /// The member's type as this use gives it; a solver type until
    /// `locate` resolves it.
    pub use_ty: Ty,
    /// The bodies whose uses decided the member's type, in body order: the
    /// member alone when its own body did, none when no vote did.
    pub deciders: Vec<B>,
}

impl<B> Outvoted<B> {
    fn locate<C>(
        &self,
        locate: impl FnMut(&B) -> C,
        resolve: impl FnOnce(&Ty) -> Ty,
    ) -> Outvoted<C> {
        Outvoted {
            use_ty: resolve(&self.use_ty),
            deciders: self.deciders.iter().map(locate).collect(),
        }
    }
}

/// Why a member is `{error}` (T11). `B` names a body, as in [`Outvoted`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disagreement<B = ItemLoc> {
    /// The member's uses tie.
    Tie {
        /// Each body whose use ties, in body order, with the type it gives
        /// the member.
        callers: Vec<(B, Ty)>,
        /// What contradicts them through a variable the member shares with
        /// other members: by member and body, the type the body gives that
        /// member.
        linked: Vec<(B, B, Ty)>,
        /// Whether every body named is one of the members: their own
        /// bodies contradict each other.
        mutual: bool,
    },
    /// A use lost to a decision the group leaves `{error}`, so it lost to
    /// nothing: it and the winners disagree, without a type to name.
    Lost {
        /// The bodies, in body order.
        bodies: Vec<B>,
        /// Whether every body named is one of the members.
        mutual: bool,
    },
}

impl<B> Disagreement<B> {
    fn locate<C>(
        &self,
        mut locate: impl FnMut(&B) -> C,
        mut resolve: impl FnMut(&Ty) -> Ty,
    ) -> Disagreement<C> {
        match self {
            Disagreement::Tie {
                callers,
                linked,
                mutual,
            } => Disagreement::Tie {
                callers: callers
                    .iter()
                    .map(|(caller, ty)| (locate(caller), resolve(ty)))
                    .collect(),
                linked: linked
                    .iter()
                    .map(|(member, caller, ty)| (locate(member), locate(caller), resolve(ty)))
                    .collect(),
                mutual: *mutual,
            },
            Disagreement::Lost { bodies, mutual } => Disagreement::Lost {
                bodies: bodies.iter().map(locate).collect(),
                mutual: *mutual,
            },
        }
    }
}

impl Disagreement {
    /// The message at a use of the member `name`.
    pub fn message(&self, name: &str) -> String {
        use std::fmt::Write as _;
        let mutual = |others: &str| {
            format!(
                "`{name}` and {others} disagree on each other's types; \
                 add type annotations to their definitions"
            )
        };
        let (callers, linked) = match self {
            Disagreement::Lost {
                bodies,
                mutual: true,
            } => return mutual(&quoted_names(bodies)),
            Disagreement::Lost { bodies, .. } => {
                return format!(
                    "{} disagree on the type of `{name}`; \
                     add a type annotation to its definition",
                    quoted_names(bodies)
                );
            }
            Disagreement::Tie {
                callers, linked, ..
            } => (callers, linked),
        };
        let mut members: Vec<ItemLoc> = Vec::new();
        for (member, ..) in linked {
            if !members.contains(member) {
                members.push(member.clone());
            }
        }
        if matches!(self, Disagreement::Tie { mutual: true, .. }) {
            return mutual(&quoted_names(&members));
        }
        let mut message = format!("callers disagree on the type of `{name}`");
        if !members.is_empty() {
            let _ = write!(
                message,
                ", which shares a type with {}",
                quoted_names(&members)
            );
        }
        let mut comma = ": ";
        for (caller, ty) in callers {
            let it = if members.is_empty() {
                "it".to_owned()
            } else {
                format!("`{name}`")
            };
            let _ = write!(
                message,
                "{comma}`{}` makes {it} `{}`",
                caller.name,
                ty.display()
            );
            comma = ", ";
        }
        for (member, caller, ty) in linked {
            let _ = write!(
                message,
                "{comma}`{}` makes `{}` `{}`",
                caller.name,
                member.name,
                ty.display()
            );
            comma = ", ";
        }
        message.push_str("; add a type annotation to its definition");
        message
    }

    /// The bodies and members the message names, each once.
    pub fn named(&self) -> Vec<&ItemLoc> {
        let all: Vec<&ItemLoc> = match self {
            Disagreement::Tie {
                callers, linked, ..
            } => {
                let linked = linked
                    .iter()
                    .flat_map(|(member, caller, _)| [member, caller]);
                callers
                    .iter()
                    .map(|(caller, _)| caller)
                    .chain(linked)
                    .collect()
            }
            Disagreement::Lost { bodies, .. } => bodies.iter().collect(),
        };
        let mut named: Vec<&ItemLoc> = Vec::new();
        for loc in all {
            if !named.contains(&loc) {
                named.push(loc);
            }
        }
        named
    }
}

/// `items` for a message: "`a`", "`a` and `b`", "`a`, `b` and `c`".
pub fn quoted_names(items: &[ItemLoc]) -> String {
    let names: Vec<String> = items
        .iter()
        .map(|item| format!("`{}`", item.name))
        .collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.concat(),
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
    let (bodies, mentions): (Vec<GroupBody<'db>>, Vec<FxHashMap<usize, usize>>) = member_items
        .iter()
        .enumerate()
        .map(|(m, &item)| (item, Some(m)))
        .chain(
            groups.sources[group.index(db) as usize]
                .iter()
                .map(|source| (source.to_id(db), None)),
        )
        .map(|(item, own)| {
            let mut mentions: FxHashMap<usize, usize> = FxHashMap::default();
            for resolution in crate::resolutions(db, item).values() {
                if let crate::Resolution::Item(loc) = resolution
                    && let Some(&m) = member_of.get(loc)
                {
                    *mentions.entry(m).or_default() += 1;
                }
            }
            let mut mentioned: Vec<usize> = mentions.keys().copied().chain(own).collect();
            mentioned.sort_unstable();
            mentioned.dedup();
            let body = GroupBody {
                item,
                own,
                mentioned,
            };
            (body, mentions)
        })
        .unzip();

    let mut solver = Solver {
        table: InPlaceUnificationTable::new(),
        constraints: Constraints::new(true),
        decided: vec![Ty::Error; member_count],
        own: vec![Ty::Error; member_count],
        met_error: vec![Vec::new(); member_count],
        fills: vec![Vec::new(); bodies.len()],
        disagreements: FxHashMap::default(),
        lost: Vec::new(),
        mentions,
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
        let mut internal: Vec<(usize, Vec<(usize, Ty)>)> = Vec::new();
        for &m in &cycle {
            let mut ctx = solver.check(db, &member_items, &member_locs, &bodies[m], &cycle);
            let uses = solver.export(&mut ctx, &member_locs, &bodies[m], &cycle, m);
            if !uses.is_empty() {
                internal.push((m, uses));
            }
            ctxs[m] = Some(ctx);
        }
        for &m in &cycle {
            solver.decided[m] = match wrong[m] {
                Wrong::No => solver.own[m].clone(),
                _ => Ty::Error,
            };
        }
        solver.unify_uses(&internal, true);
    }
    // The members' uses fill what their callees left open, together.
    let member_fills = solver.take_fills();
    solver.unify_uses(&member_fills, false);

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
    let source_fills = solver.take_fills();
    solver.unify_uses(&source_fills, false);

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

    // A use that lost, outvoted or in its member's disagreement, gets
    // `{error}`.
    let lost = std::mem::take(&mut solver.lost);
    let lost_uses: rustc_hash::FxHashSet<_> = lost.iter().map(|&(b, m, _)| (b, m)).collect();
    let outvoted = solver.sort_lost(lost, &needs_annotation);

    // The bodies verdicts name, located only when there are any, and the
    // types they give as the group finished them.
    let locate = |&b: &usize| item_loc(db, bodies[b].item);
    let disagreements: FxHashMap<usize, Disagreement> = solver
        .disagreements
        .iter()
        .map(|(&m, disagreement)| {
            let resolve = |ty: &Ty| resolve_finished(&mut solver.table, ty);
            (m, disagreement.locate(locate, resolve))
        })
        .collect();
    let outvoted: FxHashMap<(usize, usize), Outvoted> = outvoted
        .iter()
        .map(|(&key, outvoted)| {
            let resolve = |ty: &Ty| resolve_finished(&mut solver.table, ty);
            (key, outvoted.locate(locate, resolve))
        })
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
            } else if lost_uses.contains(&(b, m)) {
                // The use's own error: what follows from it is silent.
                Ty::Error
            } else {
                signatures[m].clone()
            };
            let copy = ctx.copies()[&member_locs[m]].clone();
            // Every copy has its member's shape, and the group decided it
            // with the uses agreeing (or `{error}`), so this can't fail; if
            // it did, nothing would report it.
            let adopted = ctx.adopt_exported(&copy, &exported);
            debug_assert!(adopted, "a group body's copy disagrees with its decision");
        }
        ctx.poison_left_unbound();
        ctx.report_undetermined_members(|expr, loc| {
            let m = member_of[loc];
            if let Some(outvoted) = outvoted.get(&(b, m)) {
                return Some(InferenceDiagnostic::OutvotedUse {
                    expr,
                    item: loc.clone(),
                    ty: signatures[m].clone(),
                    outvoted: outvoted.clone(),
                });
            }
            let needs = needs_annotation[m].then(|| disagreements.get(&m).cloned());
            // A generic body never joins a group.
            InferenceDiagnostic::needs_annotation(expr, loc, || false, needs)
        });
        results.push(std::sync::Arc::new(ctx.finish()));
    }

    GroupSignatures {
        signatures,
        needs_annotation,
        disagreements,
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
    /// By member: what the uses that left it `{error}` disagree on.
    disagreements: FxHashMap<usize, Disagreement<usize>>,
    /// Each use that lost, by body and member, kept until the group is
    /// decided (see [`Self::sort_lost`]).
    lost: Vec<Lost>,
    /// Per body, how many times it mentions each member: its uses, each a
    /// vote.
    mentions: Vec<FxHashMap<usize, usize>>,
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
        let mut vars: FxHashMap<TyVar, Ty> = FxHashMap::default();
        let mut internal = Vec::new();
        for &m in &body.mentioned {
            let copy = ctx.copies()[&member_locs[m]].clone();
            let resolved = ctx.resolve(&copy);
            let moved = export_ty(&resolved, &mut |var| {
                vars.entry(var)
                    .or_insert_with(|| Ty::Infer(self.table.new_key(ctx.var_kind(var))))
                    .clone()
            });
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
        if let Some(own) = body.own {
            let errors = ctx.left_unbound().to_vec();
            self.met_error[own] = errors
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

    /// The fills of each body that has any, by body.
    fn take_fills(&mut self) -> Vec<(usize, Vec<(usize, Ty)>)> {
        self.fills
            .iter_mut()
            .enumerate()
            .filter(|(_, fills)| !fills.is_empty())
            .map(|(b, fills)| (b, std::mem::take(fills)))
            .collect()
    }

    /// Unify each body's uses with the members' `decided` signatures
    /// (T11). Where they don't all agree, every member votes against the
    /// state before any vote, so the order of the items decides nothing;
    /// members whose results share a variable commit together, or all tie.
    /// `uses` pairs each body with its uses, one per member it
    /// uses. `own_decides`: what is decided of a member is its own body's.
    fn unify_uses(&mut self, uses: &[(usize, Vec<(usize, Ty)>)], own_decides: bool) {
        let all = || {
            uses.iter()
                .flat_map(|(b, uses)| uses.iter().map(|(m, ty)| (*b, *m, ty)))
        };
        if self.agree(all().map(|(_, m, ty)| (m, ty))) {
            return;
        }
        let mut used: Vec<usize> = all().map(|(_, m, _)| m).collect();
        used.sort_unstable();
        used.dedup();
        let mut plans = Vec::new();
        for m in used {
            let uses: Vec<(usize, &Ty)> = all()
                .filter(|&(_, of, _)| of == m)
                .map(|(b, _, ty)| (b, ty))
                .collect();
            let (plan, lost) = self.vote(m, &uses, own_decides);
            plans.extend(plan);
            self.lost.extend(lost);
        }
        // Plans that share a variable, or are linked by a safety edge, stand
        // or fall together: each group commits if it fits, and every member
        // in a group that doesn't ties.
        // TODO: halvko/must#234: tie only the members that conflict.
        let snapshot = self.constraints.snapshot(&mut self.table);
        for &(sub, sup) in self.constraints.safety_edges() {
            self.table.union(sub, sup);
        }
        let mut keys: Vec<Option<TyVar>> = Vec::with_capacity(plans.len());
        for plan in &plans {
            let mut vars = rustc_hash::FxHashSet::default();
            let uses = plan.winners.iter().map(|&(_, ty, _)| ty);
            let agreeing = plan.agreeing.iter().map(|&(_, ty)| ty);
            for ty in uses.chain(agreeing).chain([&self.decided[plan.m]]) {
                resolve_fully(&mut self.table, ty).table_vars(&mut vars);
            }
            let mut vars = vars.into_iter();
            let key = vars.next();
            if let Some(key) = key {
                vars.for_each(|var| self.table.union(key, var));
            }
            keys.push(key);
        }
        let keys: Vec<Option<TyVar>> = keys
            .into_iter()
            .map(|key| key.map(|var| self.table.find(var)))
            .collect();
        self.constraints.rollback_to(&mut self.table, snapshot);
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut by_key: FxHashMap<TyVar, usize> = FxHashMap::default();
        for (i, key) in keys.into_iter().enumerate() {
            let g = match key {
                Some(key) => *by_key.entry(key).or_insert(groups.len()),
                None => groups.len(),
            };
            if g == groups.len() {
                groups.push(Vec::new());
            }
            groups[g].push(i);
        }
        for at in groups {
            if self.apply_all(&plans, &at) {
                continue;
            }
            for &i in &at {
                let linked = without(&at, &[i])
                    .into_iter()
                    .flat_map(|j| {
                        let other = &plans[j];
                        let winners = other.winners.iter();
                        winners.map(|&(b, ty, _)| (other.m, b, ty.clone()))
                    })
                    .collect();
                self.undecide(plans[i].m, plans[i].named().collect(), linked);
            }
        }
    }

    /// Member `m`'s vote among its uses, by body, against what is decided
    /// (T11). A use against what is already decided loses to it; the others
    /// vote with the signature each makes alone, the plurality winning. A
    /// tie leaves the member `{error}`; otherwise returns what the vote
    /// would commit, leaving the table as it was. Returns the uses that
    /// lost too, tie or not.
    fn vote<'t>(
        &mut self,
        m: usize,
        uses: &[(usize, &'t Ty)],
        own_decides: bool,
    ) -> (Option<Plan<'t>>, Vec<Lost>) {
        // What is decided of the member leaves these open.
        let mut holes = rustc_hash::FxHashSet::default();
        resolve_fully(&mut self.table, &self.decided[m]).open_vars(&mut holes);
        let mut holes: Vec<TyVar> = holes.into_iter().collect();
        holes.sort_by_key(|hole| hole.index());

        // Each signature a use makes that is fully typed, but for the
        // numbers its literals leave, is a vote; one that leaves the member
        // otherwise open abstains.
        let mut votes: Vec<Vote<'t>> = Vec::new();
        let mut abstaining: Vec<(usize, &Ty)> = Vec::new();
        let mut lost = Vec::new();
        for &(b, ty) in uses {
            let snapshot = self.constraints.snapshot(&mut self.table);
            let agrees = self
                .constraints
                .unify_exported(&mut self.table, &self.decided[m], ty);
            let view = resolve_fully(&mut self.table, &self.decided[m]);
            let fills: Vec<Ty> = holes
                .iter()
                .map(|&hole| resolve_fully(&mut self.table, &Ty::Infer(hole)))
                .collect();
            // Read while the use's values are in the table: a variable of
            // the view can be one the rollback resets.
            let mut open = rustc_hash::FxHashSet::default();
            view.open_vars(&mut open);
            let numbers: Vec<TyVar> = open.into_iter().collect();
            let all_numbers = numbers
                .iter()
                .all(|&var| matches!(self.table.probe_value(var), TyVarValue::UnknownNumber));
            self.constraints.rollback_to(&mut self.table, snapshot);
            if !agrees {
                let deciders = if own_decides { vec![m] } else { Vec::new() };
                let use_ty = ty.clone();
                lost.push((b, m, Outvoted { use_ty, deciders }));
            } else if view.contains_error() || !all_numbers {
                abstaining.push((b, ty));
            } else {
                // TODO: halvko/must#233: a vote is the whole signature, so
                // callers agreeing on part of it still tie on all of it.
                votes.push(Vote {
                    b,
                    ty,
                    key: view.erase_regions_and_safety(),
                    numbers,
                    fills,
                    weight: self.mentions[b][&m],
                });
            }
        }

        // A vote counts for each it matches, a literal's number matching
        // any number type; one that matches a vote without numbers counts
        // only for those.
        let matches: Vec<Vec<bool>> = (0..votes.len())
            .map(|i| {
                (0..votes.len())
                    .map(|j| self.matches(&votes[i], &votes[j]))
                    .collect()
            })
            .collect();
        let candidates: Vec<usize> = (0..votes.len())
            .filter(|&i| {
                votes[i].numbers.is_empty()
                    || (0..votes.len()).all(|j| !votes[j].numbers.is_empty() || !matches[i][j])
            })
            .collect();
        let weights: Vec<usize> = candidates
            .iter()
            .map(|&c| {
                (0..votes.len())
                    .filter(|&v| matches[c][v])
                    .map(|v| votes[v].weight)
                    .sum()
            })
            .collect();
        let most = weights.iter().copied().max();
        let leaders: Vec<usize> = candidates
            .iter()
            .zip(&weights)
            .filter(|&(_, &weight)| Some(weight) == most)
            .map(|(&c, _)| c)
            .collect();
        if leaders.is_empty() {
            // No vote: the uses leave the member open, and agree or tie.
            let snapshot = self.constraints.snapshot(&mut self.table);
            let agrees = self.agree(abstaining.iter().map(|&(_, ty)| (m, ty)));
            self.constraints.rollback_to(&mut self.table, snapshot);
            if !agrees {
                let tied = abstaining.iter().map(|&(b, ty)| (b, ty.clone())).collect();
                self.undecide(m, tied, Vec::new());
                return (None, lost);
            }
            let plan = Plan {
                m,
                holes,
                winners: Vec::new(),
                agreeing: abstaining,
            };
            return (Some(plan), lost);
        }
        let won = |v: usize| leaders.iter().any(|&l| matches[l][v]);
        let tied = |votes: &[Vote<'t>]| -> Vec<(usize, Ty)> {
            (0..votes.len())
                .filter(|&v| won(v))
                .map(|v| (votes[v].b, votes[v].ty.clone()))
                .collect()
        };
        if leaders
            .iter()
            .any(|&l| leaders.iter().any(|&k| !matches[l][k]))
        {
            self.undecide(m, tied(&votes), Vec::new());
            return (None, lost);
        }

        // Below the top a safety is invariant (T13): winners that differ
        // there tie.
        let snapshot = self.constraints.snapshot(&mut self.table);
        let joined = (0..votes.len())
            .filter(|&v| won(v))
            .all(|v| self.join(&holes, &votes[v].fills));
        let mut plan = Plan {
            m,
            holes,
            winners: Vec::new(),
            agreeing: Vec::new(),
        };
        let mut rest = abstaining;
        for (v, vote) in votes.iter_mut().enumerate() {
            if won(v) && joined && self.agree(std::iter::once((m, vote.ty))) {
                plan.winners
                    .push((vote.b, vote.ty, std::mem::take(&mut vote.fills)));
            } else {
                rest.push((vote.b, vote.ty));
            }
        }
        if plan.winners.is_empty() {
            // The join contradicts every winner's use, or doesn't fit them.
            self.constraints.rollback_to(&mut self.table, snapshot);
            self.undecide(m, tied(&votes), Vec::new());
            return (None, lost);
        }
        // Every other use folds in, or loses to the winners.
        rest.sort_by_key(|&(b, _)| b);
        let deciders: Vec<usize> = plan.winners.iter().map(|&(b, ..)| b).collect();
        for (b, ty) in rest {
            if self.agree(std::iter::once((m, ty))) {
                plan.agreeing.push((b, ty));
            } else {
                let (use_ty, deciders) = (ty.clone(), deciders.clone());
                lost.push((b, m, Outvoted { use_ty, deciders }));
            }
        }
        self.constraints.rollback_to(&mut self.table, snapshot);
        (Some(plan), lost)
    }

    /// Fill `holes` with what one winner fills them with, as a join takes
    /// a branch: a safe and an unsafe fn make `unsafe fn` (T12).
    fn join(&mut self, holes: &[TyVar], fills: &[Ty]) -> bool {
        holes.iter().zip(fills).all(|(&hole, fill)| {
            self.constraints
                .relate_exported(&mut self.table, fill, &Ty::Infer(hole))
        })
    }

    /// Whether the `plans` at `at` fit together; if they do, they stay
    /// applied, else the table is left as it was.
    fn apply_all(&mut self, plans: &[Plan<'_>], at: &[usize]) -> bool {
        let snapshot = self.constraints.snapshot(&mut self.table);
        let fits = at.iter().all(|&i| self.apply(&plans[i]));
        if fits {
            self.constraints.commit(&mut self.table, snapshot);
        } else {
            self.constraints.rollback_to(&mut self.table, snapshot);
        }
        fits
    }

    /// Whether `a` and `b` vote for one type, a literal's number matching
    /// any number type.
    fn matches(&mut self, a: &Vote<'_>, b: &Vote<'_>) -> bool {
        if a.numbers.is_empty() && b.numbers.is_empty() {
            return a.key == b.key;
        }
        let snapshot = self.constraints.snapshot(&mut self.table);
        // The rollback after each use reset its literals' numbers.
        for &var in a.numbers.iter().chain(&b.numbers) {
            self.table.union_value(var, TyVarValue::UnknownNumber);
        }
        let matches = self
            .constraints
            .unify_exported(&mut self.table, &a.key, &b.key);
        self.constraints.rollback_to(&mut self.table, snapshot);
        matches
    }

    /// Commit `plan`'s winners and the uses that agree with them.
    fn apply(&mut self, plan: &Plan<'_>) -> bool {
        let joined = plan
            .winners
            .iter()
            .all(|(_, _, fills)| self.join(&plan.holes, fills));
        let uses = plan.winners.iter().map(|&(b, ty, _)| (b, ty));
        joined
            && self.agree(
                uses.chain(plan.agreeing.iter().copied())
                    .map(|(_, ty)| (plan.m, ty)),
            )
    }

    /// Unify each of `uses` with its member's decided signature, keeping
    /// the result only if they all agree.
    fn agree<'t>(&mut self, mut uses: impl Iterator<Item = (usize, &'t Ty)>) -> bool {
        let snapshot = self.constraints.snapshot(&mut self.table);
        // Exact, safety included: a body's copy is the member's type, not a
        // value meeting a position, so its safety is the member's (#168).
        let agrees = uses.all(|(m, ty)| {
            self.constraints
                .unify_exported(&mut self.table, &self.decided[m], ty)
        });
        if agrees {
            self.constraints.commit(&mut self.table, snapshot);
        } else {
            self.constraints.rollback_to(&mut self.table, snapshot);
        }
        agrees
    }

    /// Leave member `m` undecided (`{error}`): `callers` give it the types
    /// that tie, and `linked` gives, by member and body, what contradicts
    /// them through a shared variable.
    fn undecide(&mut self, m: usize, callers: Vec<(usize, Ty)>, linked: Vec<(usize, usize, Ty)>) {
        self.decided[m] = Ty::Error;
        self.disagreements.entry(m).or_insert(Disagreement::Tie {
            callers,
            linked,
            mutual: false,
        });
    }

    /// Sort each use that `lost`, once the group is decided (T11): one on a
    /// member the group leaves undecided (`{error}`) lost to nothing, so it
    /// joins that member's disagreement; every other one is outvoted, by
    /// body and member. `needs_annotation` is by member, as published.
    fn sort_lost(
        &mut self,
        mut lost: Vec<Lost>,
        needs_annotation: &[bool],
    ) -> FxHashMap<(usize, usize), Outvoted<usize>> {
        let members = needs_annotation.len();
        let mut outvoted = FxHashMap::default();
        lost.sort_unstable_by_key(|&(b, m, _)| (b, m));
        for (b, m, loss) in lost {
            if let Some(Disagreement::Tie { callers, .. }) = self.disagreements.get_mut(&m) {
                callers.push((b, loss.use_ty));
                callers.sort_by_key(|&(b, _)| b);
                continue;
            }
            if !needs_annotation[m] {
                outvoted.insert((b, m), loss);
                continue;
            }
            // What was decided before any vote is the member's own body's.
            let mut named = match &loss.deciders[..] {
                [] => vec![m],
                deciders => deciders.to_vec(),
            };
            named.push(b);
            // A member's body is its own index among the bodies.
            let mutual = named.iter().all(|&n| n < members);
            for &n in named.iter().chain([&m]) {
                let tied = matches!(self.disagreements.get(&n), Some(Disagreement::Tie { .. }));
                if n != m && (!mutual || !needs_annotation[n] || tied) {
                    continue;
                }
                let entry = self.disagreements.entry(n).or_insert(Disagreement::Lost {
                    bodies: Vec::new(),
                    mutual: true,
                });
                // A tied `m` was skipped above, and a tied `n` here.
                let Disagreement::Lost {
                    bodies,
                    mutual: all,
                } = entry
                else {
                    unreachable!("a tied member takes no lost bodies")
                };
                bodies.extend(named.iter().filter(|&&other| other != n));
                bodies.sort_unstable();
                bodies.dedup();
                *all &= mutual;
            }
        }
        // The members' own bodies contradict each other when every body a
        // tie names is one of its members.
        for (&m, disagreement) in &mut self.disagreements {
            if let Disagreement::Tie {
                callers,
                linked,
                mutual,
            } = disagreement
            {
                let member = |b: usize| b == m || linked.iter().any(|&(of, ..)| of == b);
                *mutual = !linked.is_empty()
                    && callers.iter().all(|&(b, _)| member(b))
                    && linked.iter().all(|&(_, b, _)| member(b));
            }
        }
        outvoted
    }
}

/// A body's use of a member that lost, by body and member.
type Lost = (usize, usize, Outvoted<usize>);

/// A use's vote in [`Solver::vote`].
struct Vote<'t> {
    b: usize,
    ty: &'t Ty,
    /// The member's signature as the use alone makes it, regions and
    /// safety erased.
    key: Ty,
    /// The literals' numbers `key` leaves open.
    numbers: Vec<TyVar>,
    /// What the use fills each hole of the member with.
    fills: Vec<Ty>,
    /// The body's mentions of the member.
    weight: usize,
}

/// What a member's vote would commit (see [`Solver::vote`]).
struct Plan<'t> {
    m: usize,
    /// What is decided of the member leaves these open.
    holes: Vec<TyVar>,
    /// The winning uses, with what each fills `holes` with; empty when no
    /// use voted and the abstaining uses agree.
    winners: Vec<(usize, &'t Ty, Vec<Ty>)>,
    /// Every other use that agrees with the winners.
    agreeing: Vec<(usize, &'t Ty)>,
}

impl Plan<'_> {
    /// The uses a tie names: the winners, or the abstaining uses when none
    /// voted.
    fn named(&self) -> impl Iterator<Item = (usize, Ty)> + '_ {
        let winners = self.winners.iter().map(|&(b, ty, _)| (b, ty.clone()));
        let abstaining = self.winners.is_empty().then_some(&self.agreeing);
        let abstaining = abstaining
            .into_iter()
            .flatten()
            .map(|&(b, ty)| (b, ty.clone()));
        winners.chain(abstaining)
    }
}

/// `of` without the entries in `drop`.
fn without(of: &[usize], drop: &[usize]) -> Vec<usize> {
    of.iter().copied().filter(|i| !drop.contains(i)).collect()
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
