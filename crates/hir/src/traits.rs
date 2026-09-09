//! The trait layer: the per-file impl index (coherence buckets, TR04) and
//! bound resolution — the non-generic-trait core.
//!
//! Everything here is range-free and item-tree-driven (the impl index
//! closes over item trees with zero body parsing). Buckets are keyed by
//! `(trait decl, self type)` — with self types either declarations
//! ([`SelfKey::Decl`]) or the builtin scalar types ([`SelfKey::Builtin`],
//! which have no `ItemLoc` but are legal implementers: `impl usize` in a
//! trait's chain). Ground-disjointness is trivial while traits and
//! implementing types are non-generic — a bucket admits exactly one impl —
//! but the bucket structure is what generic traits extend with the
//! pairwise ground-disjointness test.

use base_db::{Db, SourceFile};
use rustc_hash::FxHashMap;

use crate::item_tree::{self, GenericParamData, GenericParamKind, TypeRef};
use crate::scopes::{Resolution, file_scope, type_scope};
use crate::ty::{Ty, builtin_type_by_name};
use crate::{ItemId, ItemLoc};

/// The self-type half of a coherence bucket key: a `type` declaration, or
/// one of the nameable builtin types (which are legal implementers but
/// have no declaration). Builtin names are canonicalized through the type
/// itself (`string` and `str` are the same key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum SelfKey {
    Decl(ItemLoc),
    /// The canonical builtin type name (`usize`, `str`, `bool`, ...).
    Builtin(String),
}

impl SelfKey {
    /// The bucket key of a receiver type, when it can implement traits:
    /// nominal declarations and the builtin scalars. Structural types
    /// (records, arrays, fn types, raw pointers) take no user impls
    /// (TR03: coherence needs an owner).
    pub(crate) fn for_ty(ty: &Ty) -> Option<SelfKey> {
        match ty {
            Ty::Named(named) => Some(SelfKey::Decl(named.decl.clone())),
            Ty::Int(kind) => Some(SelfKey::Builtin(kind.name().to_owned())),
            Ty::Str => Some(SelfKey::Builtin("str".to_owned())),
            Ty::Bool => Some(SelfKey::Builtin("bool".to_owned())),
            Ty::Char => Some(SelfKey::Builtin("char".to_owned())),
            _ => None,
        }
    }

    /// The type a key names — the `Self` of impls in its bucket.
    pub(crate) fn to_ty(&self) -> Ty {
        match self {
            SelfKey::Decl(loc) => Ty::Named(crate::ty::NamedTy::plain(loc.clone())),
            SelfKey::Builtin(name) => builtin_type_by_name(name).unwrap_or(Ty::Error),
        }
    }

    pub(crate) fn display(&self) -> String {
        match self {
            SelfKey::Decl(loc) => loc.display_name().to_owned(),
            SelfKey::Builtin(name) => name.clone(),
        }
    }
}

/// One impl site: an `impl` element living in some item's `with`-chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ImplSite {
    /// The item whose chain hosts the element (the type for type-side
    /// impls, the trait for trait-side ones).
    pub(crate) owner: ItemLoc,
    /// The element's written head name — the member-name qualifier
    /// (member items are named `head::member`).
    pub(crate) head: String,
    /// The trait being implemented, when the head/owner resolves to one.
    pub(crate) trait_: Option<ItemLoc>,
    /// The implementing type, when it resolves.
    pub(crate) self_key: Option<SelfKey>,
    /// Which home the element sits in.
    pub(crate) type_side: bool,
    /// The element's index among the owner's live trait-impl elements, in
    /// source order — the site→syntax bridge (see [`impl_site_source`]).
    pub(crate) ordinal: u32,
}

/// The per-file impl index: every semantically-live impl element, plus the
/// coherence buckets over the resolved ones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TraitImpls {
    /// Every live impl element, in file order (diagnostics iterate this).
    pub(crate) sites: Vec<ImplSite>,
    /// Bucket key → indices into [`Self::sites`], in file order. More than
    /// one entry = a coherence violation (ground-disjointness is trivial
    /// for non-generic traits), diagnosed at the later sites.
    pub(crate) buckets: FxHashMap<(ItemLoc, SelfKey), Vec<usize>>,
}

impl TraitImpls {
    /// The unique impl for `(trait, self)`, if any — the first in file
    /// order when duplicates exist (they carry their own diagnostics).
    pub(crate) fn impl_for(&self, trait_: &ItemLoc, self_key: &SelfKey) -> Option<&ImplSite> {
        let indices = self.buckets.get(&(trait_.clone(), self_key.clone()))?;
        indices.first().map(|&index| &self.sites[index])
    }
}

/// Build the file's impl index. Resolution of heads: a TYPE-side element's
/// head names a trait ([`file_scope`]); a TRAIT-side element's head names
/// an implementing type — a builtin scalar or a non-generic `type` item
/// ([`type_scope`]). Unresolvable or reserved heads keep their site (so
/// diagnostics can speak about them) but enter no bucket.
#[salsa::tracked(returns(ref))]
pub(crate) fn trait_impls(db: &dyn Db, file: SourceFile) -> TraitImpls {
    let mut index = TraitImpls::default();
    for &item in crate::file_item_ids(db, file) {
        let Some(data) = crate::item_data(db, item).as_ref() else {
            continue;
        };
        let type_side = match data.kind {
            item_tree::ItemKind::Type => true,
            item_tree::ItemKind::Trait => false,
            // `item` came from `file_item_ids` (file-scope ids only) —
            // `Member` is structurally unreachable here.
            item_tree::ItemKind::Value(_) | item_tree::ItemKind::Member => continue,
        };
        let owner = crate::item_loc(db, item);
        // The impl heads are exactly the distinct qualifiers of the item's
        // trait-impl members — plus impls with EMPTY bodies, which mint no
        // members; the shared with-chain traversal enumerates them (still
        // item-tree shaped: heads are readable while bodies stay unparsed).
        let Some(decl) = item_tree::item_source(db, item) else {
            continue;
        };
        for (ordinal, (_, head)) in trait_impl_elements(&decl).into_iter().enumerate() {
            let (trait_, self_key) = if type_side {
                // A RESERVED generic trait never resolves as an
                // implemented trait — its impls stay out of the buckets
                // (diagnosed at the head).
                let trait_ = match file_scope(db, file).resolve(&head) {
                    Some(Resolution::TraitItem(loc)) if !trait_is_generic(db, &loc) => Some(loc),
                    _ => None,
                };
                (trait_, Some(SelfKey::Decl(owner.clone())))
            } else {
                let self_key = resolve_self_head(db, file, &head);
                (Some(owner.clone()), self_key)
            };
            let site_index = index.sites.len();
            index.sites.push(ImplSite {
                owner: owner.clone(),
                head,
                trait_: trait_.clone(),
                self_key: self_key.clone(),
                type_side,
                ordinal: ordinal as u32,
            });
            if let (Some(trait_), Some(self_key)) = (trait_, self_key) {
                index
                    .buckets
                    .entry((trait_, self_key))
                    .or_default()
                    .push(site_index);
            }
        }
    }
    index
}

/// The live trait-impl elements of one declaration's chain, in source
/// order — `(element, head name)` per element, through the SHARED
/// with-chain traversal ([`item_tree::semantic_impl_elements`]), so the
/// index, its syntax lookups and member minting can never disagree about
/// which elements exist or their order.
fn trait_impl_elements(decl: &syntax::ast::Item) -> Vec<(syntax::ast::ImplElement, String)> {
    item_tree::semantic_impl_elements(decl)
        .into_iter()
        .filter_map(|(element, home)| match home {
            item_tree::MemberHome::TraitImpl { head } => Some((element, head)),
            item_tree::MemberHome::Inherent => None,
        })
        .collect()
}

/// Whether a trait declaration carries the RESERVED `requires::<...>`
/// binder (reserved for generic traits): a reserved trait must not go
/// semantically live anywhere downstream.
pub(crate) fn trait_is_generic(db: &dyn Db, loc: &ItemLoc) -> bool {
    crate::item_data(db, loc.to_id(db))
        .as_ref()
        .is_some_and(|data| !data.generics.is_empty())
}

/// Resolve a trait-side impl head to an implementing type: a builtin
/// scalar, or a non-generic `type` item. `None` for anything else (the
/// diagnostics pass carries the story: unknown, generic, a value...).
pub(crate) fn resolve_self_head(db: &dyn Db, file: SourceFile, head: &str) -> Option<SelfKey> {
    if let Some(ty) = builtin_type_by_name(head) {
        return SelfKey::for_ty(&ty);
    }
    match type_scope(db, file).resolve(head) {
        Some(Resolution::TypeItem(loc)) => {
            let generic = crate::item_data(db, loc.to_id(db))
                .as_ref()
                .is_some_and(|data| !data.generics.is_empty());
            (!generic).then_some(SelfKey::Decl(loc))
        }
        _ => None,
    }
}

/// The trait a bound names, when it resolves: bounds are bare trait names
/// (validation rejects other shapes). A RESERVED generic trait
/// never resolves — the reservation must not go semantically live (the
/// use site carries the diagnostic).
pub(crate) fn bound_trait(db: &dyn Db, file: SourceFile, bound: &TypeRef) -> Option<ItemLoc> {
    let TypeRef::Path(name) = bound else {
        return None;
    };
    match file_scope(db, file).resolve(name) {
        Some(Resolution::TraitItem(loc)) if !trait_is_generic(db, &loc) => Some(loc),
        _ => None,
    }
}

/// The canonical dictionary-slot enumeration of a generic binder: one slot
/// per `(type param, resolved bound trait)` pair, params in binder order,
/// bounds in written order, duplicates collapsed. Call sites push
/// dictionary operands in exactly this order and callee bodies declare
/// their hidden dictionary parameters in exactly this order — the shared
/// contract between `infer` and MIR lowering.
pub fn bound_slots(db: &dyn Db, file: SourceFile, generics: &[GenericParamData]) -> Vec<BoundSlot> {
    let mut slots: Vec<BoundSlot> = Vec::new();
    for (index, param) in generics.iter().enumerate() {
        if !matches!(param.kind, GenericParamKind::Type) {
            continue;
        }
        for bound in &param.bounds {
            let Some(trait_) = bound_trait(db, file, bound) else {
                continue;
            };
            if slots
                .iter()
                .any(|slot| slot.param_index == index as u32 && slot.trait_ == trait_)
            {
                continue;
            }
            slots.push(BoundSlot {
                param_index: index as u32,
                trait_,
            });
        }
    }
    slots
}

/// One dictionary slot of a bounded generic binder — see [`bound_slots`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoundSlot {
    /// The bounded type param's binder index.
    pub param_index: u32,
    pub trait_: ItemLoc,
}

/// The member fns one dictionary carries: the impl's member items in
/// TRAIT-REQUIREMENT order — the runtime "dictionary = the impl's member
/// set" of members, spread member-by-member (each slot expands to one
/// hidden parameter per requirement). `None` when the impl doesn't cover
/// a requirement (broken impls carry their own diagnostics; callers trap).
pub(crate) fn impl_dict_members(
    db: &dyn Db,
    trait_: &ItemLoc,
    site: &ImplSite,
) -> Option<Vec<ItemLoc>> {
    let requirements = item_tree::trait_requirements(db, trait_.to_id(db));
    let members = item_tree::type_members(db, site.owner.to_id(db));
    requirements
        .iter()
        .map(|req| {
            let qualified = format!("{}::{}", site.head, req.name);
            members
                .iter()
                .find(|m| m.name == qualified)
                .map(|m| ItemLoc {
                    file: site.owner.file,
                    name: site.owner.name.clone(),
                    disambiguator: site.owner.disambiguator,
                    member: Some((std::sync::Arc::from(m.name.as_str()), m.disambiguator)),
                })
        })
        .collect()
}

/// The number of hidden dictionary parameters a binder's bounds add to a
/// body — the MIR-side count (one per slot per requirement of the slot's
/// trait).
pub fn dict_param_count(db: &dyn Db, file: SourceFile, generics: &[GenericParamData]) -> usize {
    bound_slots(db, file, generics)
        .iter()
        .map(|slot| item_tree::trait_requirements(db, slot.trait_.to_id(db)).len())
        .sum()
}

/// All traits declared in `file`, in file order.
pub(crate) fn trait_items<'db>(db: &'db dyn Db, file: SourceFile) -> Vec<ItemId<'db>> {
    crate::file_item_ids(db, file)
        .iter()
        .copied()
        .filter(|&item| {
            crate::item_data(db, item)
                .as_ref()
                .is_some_and(|data| matches!(data.kind, item_tree::ItemKind::Trait))
        })
        .collect()
}

/// The traits that can answer a dot-call `recv.name(...)` on a concrete
/// receiver keyed `self_key`: every trait with a requirement named `name`
/// AND an impl for the receiver. Returned in file order (ambiguity — more
/// than one — is the caller's diagnostic).
pub(crate) fn traits_providing_member(
    db: &dyn Db,
    file: SourceFile,
    self_key: &SelfKey,
    name: &str,
) -> Vec<(ItemLoc, u32)> {
    let impls = trait_impls(db, file);
    let mut out = Vec::new();
    for trait_item in trait_items(db, file) {
        let requirements = item_tree::trait_requirements(db, trait_item);
        let Some(index) = requirements.iter().position(|req| req.name == name) else {
            continue;
        };
        let trait_loc = crate::item_loc(db, trait_item);
        if impls.impl_for(&trait_loc, self_key).is_some() {
            out.push((trait_loc, index as u32));
        }
    }
    out
}

/// The syntax of an impl site — ranges enter here and only here (the
/// firewall discipline of [`item_tree::item_source`]). Goes through the
/// SAME traversal that built the index, so the ordinal can never drift.
pub(crate) fn impl_site_source(db: &dyn Db, site: &ImplSite) -> Option<syntax::ast::ImplElement> {
    let decl = item_tree::item_source(db, site.owner.to_id(db))?;
    trait_impl_elements(&decl)
        .into_iter()
        .nth(site.ordinal as usize)
        .map(|(element, _)| element)
}

/// The [`crate::ty::ParamScope`] a requirement's signature lowers under:
/// `Self` bound to `self_ty`, and the requirement's own binder params
/// keyed by `key` (the trait item for the requirement's own scheme; the
/// IMPL MEMBER for matching an impl member's signature index-by-index —
/// alpha-equivalence by binder position).
pub(crate) fn requirement_param_scope(
    generics: &[GenericParamData],
    key: &ItemLoc,
    self_ty: Ty,
) -> crate::ty::ParamScope {
    let mut scope = crate::ty::ParamScope::default();
    scope.types.insert("Self".to_owned(), self_ty);
    for (index, param) in generics.iter().enumerate() {
        if param.name.is_empty() {
            continue;
        }
        match param.kind {
            GenericParamKind::Type => {
                scope.types.insert(
                    param.name.clone(),
                    Ty::Param(crate::ty::ParamTy {
                        item: key.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    }),
                );
            }
            // Regions are neither values nor types: they enter the scope's
            // own region map, consulted only from region-argument
            // positions.
            GenericParamKind::Region => {
                scope.regions.insert(
                    param.name.clone(),
                    crate::ty::Region::Param {
                        item: key.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    },
                );
            }
            GenericParamKind::Const(_) => {
                scope.consts.insert(
                    param.name.clone(),
                    crate::ty::ConstArgValue::Param {
                        item: key.clone(),
                        index: index as u32,
                        name: std::sync::Arc::from(param.name.as_str()),
                    },
                );
            }
        }
    }
    scope
}

/// Lower a requirement's signature with `Self` bound to `self_ty` and the
/// requirement's binder params keyed by `key` — see
/// [`requirement_param_scope`]. `None` when the requirement isn't fully
/// written (its declaration carries the diagnostic).
pub fn lower_requirement_sig(
    db: &dyn Db,
    file: SourceFile,
    req: &item_tree::TraitRequirement,
    key: &ItemLoc,
    self_ty: Ty,
) -> Option<Ty> {
    let sig = req.sig.as_ref()?;
    let scope = requirement_param_scope(&req.generics, key, self_ty);
    let mut table = ena::unify::InPlaceUnificationTable::new();
    Some(crate::ty::lower_type_ref_in(
        db, file, sig, &mut table, &scope,
    ))
}

/// Whether two binders match for impl-vs-requirement purposes: same
/// arity, same kinds position by position, the same RESOLVED bound set and
/// the same `forget` requirement per type param, and the same OUTLIVES set
/// per region param (the sets order-insensitive; unresolvable bounds
/// compare by absence — their own diagnostics tell that story).
///
/// A REGION param matches a region param. Without that arm a requirement
/// could not carry a region binder at all — and a requirement whose member
/// borrows `Self` **must** carry one, because a signature elides nothing —
/// so this single guard was what made the trait half of borrow-`Self`
/// members unreachable, no matter how exactly the impl copied the
/// requirement.
///
/// Region outlives bounds compare POSITIONALLY, because they name sibling
/// params rather than resolvable items: `@b: @a` means "the param at index
/// 0", so the requirement's spelling and the impl's need not agree, exactly
/// as type-param names need not. Exact-set comparison (not subset) is the
/// strict-first choice already made for type bounds; an impl declaring
/// FEWER outlives bounds than its requirement is in fact sound (it promises
/// more), so loosening in that direction later is purely additive.
pub(crate) fn binders_match(
    db: &dyn Db,
    file: SourceFile,
    req: &[GenericParamData],
    member: &[GenericParamData],
) -> bool {
    if req.len() != member.len() {
        return false;
    }
    // Each binder's own name → index map, for the positional comparison.
    fn index_of(params: &[GenericParamData]) -> rustc_hash::FxHashMap<&str, u32> {
        params
            .iter()
            .enumerate()
            .map(|(index, param)| (param.name.as_str(), index as u32))
            .collect()
    }
    let (req_index, member_index) = (index_of(req), index_of(member));
    req.iter().zip(member).all(|(r, m)| {
        let kinds_match = matches!(
            (&r.kind, &m.kind),
            (GenericParamKind::Type, GenericParamKind::Type)
                | (GenericParamKind::Region, GenericParamKind::Region)
                | (GenericParamKind::Const(_), GenericParamKind::Const(_))
        );
        if !kinds_match {
            return false;
        }
        if matches!(r.kind, GenericParamKind::Region) {
            let positions = |names: &[String], map: &rustc_hash::FxHashMap<&str, u32>| {
                // A name resolving to nothing is dropped, mirroring an
                // unresolvable trait bound: the declaration carries that
                // diagnostic and matching must not invent a second one.
                let mut out: Vec<u32> = names
                    .iter()
                    .filter_map(|n| map.get(n.as_str()).copied())
                    .collect();
                out.sort_unstable();
                out.dedup();
                out
            };
            return positions(&r.outlives, &req_index) == positions(&m.outlives, &member_index);
        }
        // The CAPABILITY bound is part of the contract too, and it is not
        // among the trait bounds (it resolves to no dictionary): an impl
        // asking `T: forget` where the requirement does not demands more of
        // every caller than the requirement promised.
        //
        // The OTHER direction — an impl OMITTING a bound its requirement
        // writes — is refused too, and the honest reason is not that the
        // impl is stricter (it is the reverse: a body that asks less is
        // usable everywhere the requirement is). It is that binder matching
        // here is an EQUALITY, deliberately: there is no variance story for
        // binders, and accepting one direction would mean defending it
        // against the neighbours that compare the same way — trait bounds
        // as an exact set, `outlives` positionally — and keeping it right
        // the day a dictionary starts carrying capability information.
        // Strict-first: loosening is additive, and this is the record that
        // the symmetry was chosen rather than fallen into.
        if r.forget != m.forget {
            return false;
        }
        let resolve = |bounds: &[TypeRef]| {
            let mut traits: Vec<ItemLoc> = bounds
                .iter()
                .filter_map(|bound| bound_trait(db, file, bound))
                .collect();
            traits.sort_by(|a, b| {
                (a.name.as_ref(), a.disambiguator).cmp(&(b.name.as_ref(), b.disambiguator))
            });
            traits.dedup();
            traits
        };
        resolve(&r.bounds) == resolve(&m.bounds)
    })
}

/// The traits among `bounds` (resolved) declaring a requirement named
/// `name` — bound-directed member resolution's candidate set.
pub(crate) fn bound_traits_providing_member(
    db: &dyn Db,
    file: SourceFile,
    bounds: &[TypeRef],
    name: &str,
) -> Vec<(ItemLoc, u32)> {
    let mut out: Vec<(ItemLoc, u32)> = Vec::new();
    for bound in bounds {
        let Some(trait_) = bound_trait(db, file, bound) else {
            continue;
        };
        if out.iter().any(|(t, _)| *t == trait_) {
            continue;
        }
        let requirements = item_tree::trait_requirements(db, trait_.to_id(db));
        if let Some(index) = requirements.iter().position(|req| req.name == name) {
            out.push((trait_, index as u32));
        }
    }
    out
}
