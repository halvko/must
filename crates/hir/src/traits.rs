//! The trait layer: the per-library impl index (coherence buckets, TR04) and
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

use crate::ItemLoc;
use crate::item_tree::{self, GenericParamData, GenericParamKind, TypeRef};
use crate::library::{LibraryId, library_item_ids, library_of};
use crate::scopes::{Builtin, Resolution, file_scope, type_scope};
use crate::ty::{ParamTy, ReceiverShape, Ty, builtin_type_by_name};

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

/// The per-library impl index: every semantically-live impl element, plus
/// the coherence buckets over the resolved ones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TraitImpls {
    /// Every live impl element, in library order.
    pub(crate) sites: Vec<ImplSite>,
    /// Bucket key → indices into [`Self::sites`], in library order. More than
    /// one entry = a coherence violation (ground-disjointness is trivial
    /// for non-generic traits), diagnosed at the later sites.
    pub(crate) buckets: FxHashMap<(ItemLoc, SelfKey), Vec<usize>>,
    /// Every trait the library declares, in library order.
    pub(crate) traits: Vec<ItemLoc>,
}

impl TraitImpls {
    /// The impl elements in `file`, with their indices into [`Self::sites`].
    pub(crate) fn sites_in(&self, file: SourceFile) -> impl Iterator<Item = (usize, &ImplSite)> {
        self.sites
            .iter()
            .enumerate()
            .filter(move |(_, site)| site.owner.file == file)
    }

    /// The unique impl for `(trait, self)`, if any — the first in library
    /// order when duplicates exist (they carry their own diagnostics).
    pub(crate) fn impl_for(&self, trait_: &ItemLoc, self_key: &SelfKey) -> Option<&ImplSite> {
        let indices = self.buckets.get(&(trait_.clone(), self_key.clone()))?;
        indices.first().map(|&index| &self.sites[index])
    }
}

/// The impl index of `file`'s library.
pub(crate) fn trait_impls(db: &dyn Db, file: SourceFile) -> &TraitImpls {
    library_trait_impls(db, library_of(db, file))
}

/// Build the library's impl index. Resolution of heads: a TYPE-side element's
/// head names a trait ([`file_scope`]); a TRAIT-side element's head names
/// an implementing type — a builtin scalar or a non-generic `type` item
/// ([`type_scope`]). Unresolvable or reserved heads keep their site (so
/// diagnostics can speak about them) but enter no bucket.
#[salsa::tracked(returns(ref))]
pub(crate) fn library_trait_impls<'db>(db: &'db dyn Db, library: LibraryId<'db>) -> TraitImpls {
    let mut index = TraitImpls::default();
    for &item in library_item_ids(db, library) {
        let file = item.file(db);
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
        if !type_side {
            index.traits.push(owner.clone());
        }
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
                    Some(Resolution::TraitItem(loc)) if trait_can_be_implemented(db, &loc) => {
                        Some(loc)
                    }
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

/// Whether the declaration at `loc` declares generic parameters. On a
/// trait that is the RESERVED `requires::<...>` binder: a reserved trait
/// must not go semantically live anywhere downstream.
pub(crate) fn item_is_generic(db: &dyn Db, loc: &ItemLoc) -> bool {
    crate::item_data(db, loc.to_id(db))
        .as_ref()
        .is_some_and(|data| !data.generics.is_empty())
}

/// Whether the builtin type `name` can implement traits: the scalars.
pub fn builtin_can_implement(name: &str) -> bool {
    builtin_type_by_name(name).is_some_and(|ty| SelfKey::for_ty(&ty).is_some())
}

/// Whether the `type` item at `loc` can implement traits: impls for a
/// generic type are reserved.
pub fn type_item_can_implement(db: &dyn Db, loc: &ItemLoc) -> bool {
    !item_is_generic(db, loc)
}

/// Whether the trait at `loc` can be implemented: generic traits are
/// reserved.
pub fn trait_can_be_implemented(db: &dyn Db, loc: &ItemLoc) -> bool {
    !item_is_generic(db, loc)
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
            type_item_can_implement(db, &loc).then_some(SelfKey::Decl(loc))
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
        Some(Resolution::TraitItem(loc)) if trait_can_be_bound(db, &loc) => Some(loc),
        _ => None,
    }
}

/// Whether the trait at `loc` can be named as a bound: generic traits are
/// reserved.
pub fn trait_can_be_bound(db: &dyn Db, loc: &ItemLoc) -> bool {
    !item_is_generic(db, loc)
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
    item_tree::trait_requirements(db, trait_.to_id(db))
        .iter()
        .map(|req| match site_member(db, site, &req.name) {
            ImplMember::Found(loc) => Some(loc),
            ImplMember::Missing { .. } => None,
        })
        .collect()
}

/// What the impl at `site` holds for the requirement `name`.
fn site_member(db: &dyn Db, site: &ImplSite, name: &str) -> ImplMember {
    let qualified = format!("{}::{name}", site.head);
    let members = item_tree::type_members(db, site.owner.to_id(db));
    let Some(member) = members.iter().find(|m| m.name == qualified) else {
        return ImplMember::Missing {
            owner: site.owner.clone(),
        };
    };
    ImplMember::Found(ItemLoc {
        file: site.owner.file,
        name: site.owner.name.clone(),
        disambiguator: site.owner.disambiguator,
        member: Some((
            std::sync::Arc::from(member.name.as_str()),
            member.disambiguator,
        )),
    })
}

/// What an impl holds for one requirement.
#[derive(Debug, Clone)]
pub(crate) enum ImplMember {
    /// The impl's member.
    Found(ItemLoc),
    /// The impl lacks the member; it counts as implementing the trait.
    /// `owner` is the impl's owner item, whose errors include the
    /// missing-member one.
    Missing { owner: ItemLoc },
}

impl ImplMember {
    pub(crate) fn found(&self) -> Option<&ItemLoc> {
        match self {
            ImplMember::Found(loc) => Some(loc),
            ImplMember::Missing { .. } => None,
        }
    }
}

/// What `ty`'s impl of `trait_` holds for the requirement `name`; `None`
/// when `ty` has no impl of `trait_`.
pub(crate) fn impl_member(
    db: &dyn Db,
    file: SourceFile,
    trait_: &ItemLoc,
    ty: &Ty,
    name: &str,
) -> Option<ImplMember> {
    let site = SelfKey::for_ty(ty).and_then(|key| trait_impls(db, file).impl_for(trait_, &key))?;
    Some(site_member(db, site, name))
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

/// One requirement of a trait implemented for a concrete type, with what
/// the impl holds for it.
pub(crate) struct ImplRequirement {
    pub(crate) trait_: ItemLoc,
    pub(crate) name: String,
    pub(crate) member: ImplMember,
}

/// Every requirement of every trait implemented for `self_key` (only those
/// named `name`, when given), in library order: what impl-directed
/// resolution picks a dot-call or a `Type::member` path from. A trait
/// repeating a requirement name lists it once.
pub(crate) fn impl_requirements(
    db: &dyn Db,
    file: SourceFile,
    self_key: &SelfKey,
    name: Option<&str>,
) -> Vec<ImplRequirement> {
    let impls = trait_impls(db, file);
    let mut out = Vec::new();
    for trait_loc in &impls.traits {
        let Some(site) = impls.impl_for(trait_loc, self_key) else {
            continue;
        };
        let requirements = item_tree::trait_requirements(db, trait_loc.to_id(db));
        for (index, req) in requirements.iter().enumerate() {
            if name.is_some_and(|name| req.name != name)
                || requirements[..index]
                    .iter()
                    .any(|earlier| earlier.name == req.name)
            {
                continue;
            }
            out.push(ImplRequirement {
                trait_: trait_loc.clone(),
                name: req.name.clone(),
                member: site_member(db, site, &req.name),
            });
        }
    }
    out
}

/// The traits that can answer a dot-call `recv.name(...)` on a concrete
/// receiver keyed `self_key`: every trait with a requirement named `name`
/// AND an impl for the receiver, with what that impl holds for `name`.
/// Returned in library order (ambiguity — more than one — is the caller's
/// diagnostic).
pub(crate) fn traits_providing_member(
    db: &dyn Db,
    file: SourceFile,
    self_key: &SelfKey,
    name: &str,
) -> Vec<(ItemLoc, ImplMember)> {
    impl_requirements(db, file, self_key, Some(name))
        .into_iter()
        .map(|req| (req.trait_, req.member))
        .collect()
}

/// The inherent member of `decl` named `name` (the first, when duplicates
/// carry their own diagnostic).
pub(crate) fn inherent_member(db: &dyn Db, decl: &ItemLoc, name: &str) -> Option<ItemLoc> {
    if decl.member.is_some() {
        return None;
    }
    item_tree::type_members(db, decl.to_id(db))
        .iter()
        .find(|m| m.name == name)
        .map(|m| ItemLoc {
            file: decl.file,
            name: decl.name.clone(),
            disambiguator: decl.disambiguator,
            member: Some((std::sync::Arc::from(m.name.as_str()), m.disambiguator)),
        })
}

/// One trait that could carry a concrete-receiver dot-call.
pub(crate) struct TraitCallCandidate {
    pub(crate) trait_: ItemLoc,
    /// What the impl holds for the requirement (a missing member is
    /// diagnosed at the impl).
    pub(crate) member: ImplMember,
    /// Whether the impl member's signature is broken (its definition site
    /// carries the diagnostic; the call refuses silently).
    pub(crate) broken: bool,
}

/// Everything a dot-call `recv.name(...)` on a CONCRETE receiver counts: the
/// inherent member, the impl-directed trait candidates (TR01 extended), and
/// a field of the name. Which of them carry the call depends on the
/// receiver's shape, so the methods take it. Resolution acts on it, and
/// completion asks it of every row it might offer.
pub(crate) struct ConcreteDotCandidates {
    pub(crate) inherent: Option<ItemLoc>,
    pub(crate) traits: Vec<TraitCallCandidate>,
    field: Option<Ty>,
}

impl ConcreteDotCandidates {
    /// A broken candidate: the call is an error naming it.
    pub(crate) fn broken(&self, db: &dyn Db) -> Option<&ItemLoc> {
        self.inherent
            .as_ref()
            .filter(|loc| crate::ty::signature(db, loc.to_id(db)).contains_error())
            .or_else(|| {
                self.traits
                    .iter()
                    .find(|candidate| candidate.broken)
                    .and_then(|candidate| candidate.member.found())
            })
    }

    /// The field of the name, never reached through a borrow (that would be
    /// auto-deref).
    pub(crate) fn field(&self, shape: ReceiverShape) -> Option<&Ty> {
        self.field.as_ref().filter(|_| !shape.is_borrow())
    }

    /// The inherent member, when it takes the call from `shape`.
    pub(crate) fn inherent_carrier(&self, db: &dyn Db, shape: ReceiverShape) -> Option<&ItemLoc> {
        self.inherent
            .as_ref()
            .filter(|loc| crate::ty::member_takes(db, loc.to_id(db), shape))
    }

    /// The trait candidates whose impl member takes the call from `shape`.
    pub(crate) fn trait_carriers(
        &self,
        db: &dyn Db,
        shape: ReceiverShape,
    ) -> Vec<&TraitCallCandidate> {
        self.traits
            .iter()
            .filter(|candidate| {
                !candidate.broken
                    && candidate
                        .member
                        .found()
                        .is_some_and(|loc| crate::ty::member_takes(db, loc.to_id(db), shape))
            })
            .collect()
    }

    /// An fn-typed field carries the call beside the members.
    pub(crate) fn fn_field(&self, shape: ReceiverShape) -> bool {
        matches!(self.field(shape), Some(Ty::Fn(_)))
    }

    /// Whether the call from `shape` falls through to a builtin member of
    /// the name: nothing is broken and no user candidate, member or fn-typed
    /// field, takes it (user declarations shadow builtins).
    pub(crate) fn leaves_builtin(&self, db: &dyn Db, shape: ReceiverShape) -> bool {
        self.broken(db).is_none() && self.carriers(db, shape) == 0
    }

    /// How many candidates take the call from `shape`; more than one is
    /// ambiguous.
    pub(crate) fn carriers(&self, db: &dyn Db, shape: ReceiverShape) -> usize {
        usize::from(self.inherent_carrier(db, shape).is_some())
            + self.trait_carriers(db, shape).len()
            + usize::from(self.fn_field(shape))
    }
}

/// The [`ConcreteDotCandidates`] of `recv.name(...)`, where `member_recv` is
/// the receiver members are looked up on (the referent, under a borrow).
/// `None` when the receiver can't carry members (a structural type, or a
/// declaration that is neither a struct nor an enum, whose own diagnostic
/// stands).
pub(crate) fn concrete_dot_candidates(
    db: &dyn Db,
    file: SourceFile,
    member_recv: &Ty,
    name: &str,
) -> Option<ConcreteDotCandidates> {
    let self_key = SelfKey::for_ty(member_recv)?;
    let named = match member_recv {
        Ty::Named(named) => Some(named),
        _ => None,
    };
    let underlying = named.and_then(|named| crate::ty::type_underlying_for(db, named));
    if let Some(named) = named
        && underlying.is_none()
        && crate::ty::enum_variants(db, named.decl.to_id(db)).is_none()
    {
        return None;
    }
    let field = match &underlying {
        Some(Ty::Record(rec)) => rec.field_ty(name).cloned(),
        _ => None,
    };
    let traits = traits_providing_member(db, file, &self_key, name)
        .into_iter()
        .map(|(trait_, member)| TraitCallCandidate {
            broken: member
                .found()
                .is_some_and(|loc| crate::ty::signature(db, loc.to_id(db)).contains_error()),
            trait_,
            member,
        })
        .collect();
    Some(ConcreteDotCandidates {
        inherent: named.and_then(|named| inherent_member(db, &named.decl, name)),
        traits,
        field,
    })
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
/// [`requirement_param_scope`]. Resolves names in `trait_loc`'s file: a
/// requirement's signature is written in the trait declaration, so that is
/// the scope its names resolve in, wherever the call or the impl is —
/// structural now that every caller has the trait's `ItemLoc` in hand.
/// `None` when the requirement isn't fully written (its declaration
/// carries the diagnostic).
pub fn lower_requirement_sig(
    db: &dyn Db,
    trait_loc: &ItemLoc,
    req: &item_tree::TraitRequirement,
    key: &ItemLoc,
    self_ty: Ty,
) -> Option<Ty> {
    let sig = req.sig.as_ref()?;
    let scope = requirement_param_scope(&req.generics, key, self_ty);
    let mut table = ena::unify::InPlaceUnificationTable::new();
    Some(crate::ty::lower_type_ref_in(
        db,
        trait_loc.file,
        sig,
        &mut table,
        &scope,
    ))
}

/// Whether two binders match for impl-vs-requirement purposes: same
/// arity, same kinds position by position, the same RESOLVED bounds in the
/// same written order (the order the dictionaries are passed in) and the
/// same `forget` requirement per type param, and the same OUTLIVES set per
/// region param (order-insensitive; unresolvable bounds compare by absence
/// — their own diagnostics tell that story).
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
    (req_file, req): (SourceFile, &[GenericParamData]),
    (member_file, member): (SourceFile, &[GenericParamData]),
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
        // Each binder's bounds name traits in the file that declares it, and
        // compare in written order: the order of the dictionaries
        // [`bound_slots`] passes, which the caller pushes by the requirement
        // and the member's body reads by its own binder.
        // TODO: halvko/must#163: accept any order by matching slots by trait.
        let resolve = |file: SourceFile, bounds: &[TypeRef]| {
            let mut traits: Vec<ItemLoc> = Vec::new();
            for trait_ in bounds
                .iter()
                .filter_map(|bound| bound_trait(db, file, bound))
            {
                if !traits.contains(&trait_) {
                    traits.push(trait_);
                }
            }
            traits
        };
        resolve(req_file, &r.bounds) == resolve(member_file, &m.bounds)
    })
}

/// One requirement reachable on the dot of a RIGID receiver: a bound's
/// trait, and which of its requirements this is.
///
/// A candidate is a NAME and a declaration, nothing more — whether the
/// receiver can actually take the call is G14's structural question, asked
/// of the requirement's lowered signature by [`crate::ty::self_position_of`]
/// and [`crate::ty::dot_reach`].
#[derive(Debug)]
pub(crate) struct BoundDotCandidate<'db> {
    /// The bound's trait.
    pub(crate) trait_: ItemLoc,
    /// The requirement's index in `trait_requirements` — the same index a
    /// [`crate::infer::BoundMemberCall`] carries, so goto and MIR read it
    /// the way resolution wrote it.
    pub(crate) member_index: u32,
    /// The requirement's name — what the user types after the dot.
    /// Borrowed from [`item_tree::trait_requirements`]'s tracked value
    /// rather than cloned: enumeration runs on the inference hot path for
    /// every bound-directed dot call, once per requirement per bound.
    pub(crate) name: &'db str,
}

/// Every requirement the resolved `bounds` put on a rigid receiver's dot,
/// in bound order then declaration order — ONE enumerator, shared.
///
/// Bound-directed resolution narrows it to the written name
/// ([`narrow_by_name`]); completion runs the SAME narrowing over every name
/// and offers what survives it and the receiver's shape. So a requirement
/// that stops being visible here stops being callable and stops being
/// offered in one edit.
///
/// Two divergences between the offered set and the callable set are
/// deliberate and recorded, both in [`bound_dot_offers`]: a nested body's
/// reservation, and the shape-blind ambiguity refusal.
///
/// A bound naming the same trait twice contributes once (the dedup is on
/// the trait, not the spelling); an unresolvable bound contributes nothing
/// (its declaration carries that diagnostic); and the `forget` CAPABILITY
/// is not in `bounds` at all — [`item_tree::GenericParamData`] keeps it in
/// its own field, so it reaches neither trait resolution nor this list.
pub(crate) fn bound_dot_candidates<'db>(
    db: &'db dyn Db,
    file: SourceFile,
    bounds: &[TypeRef],
) -> Vec<BoundDotCandidate<'db>> {
    let mut traits: Vec<ItemLoc> = Vec::new();
    let mut out: Vec<BoundDotCandidate<'db>> = Vec::new();
    for bound in bounds {
        let Some(trait_) = bound_trait(db, file, bound) else {
            continue;
        };
        if traits.contains(&trait_) {
            continue;
        }
        traits.push(trait_.clone());
        for (index, req) in item_tree::trait_requirements(db, trait_.to_id(db))
            .iter()
            .enumerate()
        {
            out.push(BoundDotCandidate {
                trait_: trait_.clone(),
                member_index: index as u32,
                name: req.name.as_str(),
            });
        }
    }
    out
}

/// The bound-dot candidates visible at a rigid param, from the body that
/// OWNS its binder: nothing unless `param` names `owner`'s own generic
/// parameter (a param of any other binder cannot occur in a body it does
/// not own, and gets nothing rather than a guess). This is the ownership
/// check and bounds lookup shared by inference's bound-directed call
/// resolution ([`crate::infer::InferCtx::bound_dot_candidates`]) and
/// completion's offers query ([`bound_dot_offers`]) — one place decides
/// which binder's bounds a rigid param reads.
pub(crate) fn param_bound_candidates<'db>(
    db: &'db dyn Db,
    owner: Option<&ItemLoc>,
    owner_generics: &[GenericParamData],
    param: &ParamTy,
) -> Vec<BoundDotCandidate<'db>> {
    if !owner.is_some_and(|own| param.item == *own) {
        return Vec::new();
    }
    let Some(data) = owner_generics.get(param.index as usize) else {
        return Vec::new();
    };
    bound_dot_candidates(db, param.item.file, &data.bounds)
}

/// The candidates for ONE written name: at most one per trait — its FIRST
/// requirement of that name — in bound order.
///
/// This is resolution's whole selection rule, and the count is the whole
/// verdict: 0 is "no such member", 1 resolves, and 2+ is the same G13
/// ambiguity as a concrete receiver's — refused PERMANENTLY, with no shape
/// consulted (a bound member call is decided by name before G14 is asked,
/// so even a name with exactly one shape-viable candidate is refused).
/// Completion asks the same question for every name it might offer, which
/// is why an ambiguous name is suppressed rather than offered: an offer
/// that cannot be accepted is a worse answer than no offer.
pub(crate) fn narrow_by_name(
    candidates: &[BoundDotCandidate<'_>],
    name: &str,
) -> Vec<(ItemLoc, u32)> {
    let mut out: Vec<(ItemLoc, u32)> = Vec::new();
    for candidate in candidates {
        if candidate.name == name && !out.iter().any(|(trait_, _)| *trait_ == candidate.trait_) {
            out.push((candidate.trait_.clone(), candidate.member_index));
        }
    }
    out
}

/// One member a receiver's dot can take: its name and signature.
#[derive(Debug)]
pub struct MemberDotOffer {
    /// The member's name — what the user types after the dot.
    pub name: String,
    /// The member's signature. For a rigid receiver's requirement, `Self`
    /// stands for the receiver's param and the requirement's own binder is
    /// as written — the same lowering the trait's hover renders.
    pub sig: Ty,
    /// How the dot reaches the member: as written, or through a written borrow.
    pub reach: crate::ty::DotReach,
    /// The call is refused for want of a `&mut` ([`crate::ty::DotOffer`]).
    pub refused: bool,
}

/// What a receiver of type `receiver_ty` may be offered after the dot,
/// when it is rigid — the bounds of the param it resolves to, narrowed by
/// resolution's own rule ([`narrow_by_name`]) and kept when
/// [`crate::ty::dot_offer`] reaches it, including the borrow-`Self`
/// requirements an owned receiver reaches through a written `.&`/`.&mut`.
///
/// Reads the receiver exactly as `infer_dot_call` reads it: a BORROW
/// reaches the referent's bounds (inside a generic body `T.&mut::<@z>` is
/// the very shape a `Self.&mut`-taking requirement is called on).
/// `exclusive` is whether the receiver can provide a `&mut`; a requirement
/// that needs one it cannot is offered refused ([`crate::ty::dot_offer`]).
/// `owner` is the body being edited — a param of any other binder cannot
/// occur in it, and gets nothing rather than a guess.
///
/// TWO recorded divergences from what a call at that dot would resolve to,
/// each ruled deliberately:
///
/// - A name carried by TWO bounds is offered by NEITHER. Resolution refuses
///   it permanently on the name alone (P15: shape never narrows a
///   bound-directed ambiguity), so no row here could be accepted: not a
///   merged one (it would still error on accept), and not a per-trait one
///   (no qualified spelling fits a dot — the receiver is already written).
///   This suppression asks resolution's rule ([`narrow_by_name`]) rather
///   than restating it, so it stays correct if that rule ever changes.
/// - Inside a NESTED body (a fn literal, a `const` block) the requirements
///   are still offered, though a bound-directed call there is refused as
///   reserved (`NestedBoundUse` — the captured-dictionary wall). That wall
///   is a body-lowering fact, not a fact about the candidate set: teaching
///   it to this item-tree-driven query would mean re-deriving nesting here,
///   a second home for the wall. The refusal explains itself at the call,
///   and the day the wall lifts these offers are already right.
pub fn bound_dot_offers<'db>(
    db: &'db dyn Db,
    owner: crate::ItemId<'db>,
    receiver_ty: &Ty,
    exclusive: bool,
) -> Vec<MemberDotOffer> {
    let member_recv = match receiver_ty {
        Ty::Borrow { referent, .. } => &**referent,
        _ => receiver_ty,
    };
    let Ty::Param(param) = member_recv else {
        return Vec::new();
    };
    let owner_loc = crate::item_loc(db, owner);
    let owner_generics = crate::item_data(db, owner)
        .as_ref()
        .map(|data| data.generics.as_slice())
        .unwrap_or(&[]);
    let candidates = param_bound_candidates(db, Some(&owner_loc), owner_generics, param);
    let self_ty = Ty::Param(param.clone());
    let mut out: Vec<MemberDotOffer> = Vec::new();
    for candidate in &candidates {
        // Resolution's narrowing, asked of this name: anything but a lone
        // winner is unofferable — and the lone winner is the candidate
        // resolution would pick, so a trait's second requirement of the
        // same name never doubles a row either.
        let narrowed = narrow_by_name(&candidates, candidate.name);
        let [(trait_, member_index)] = narrowed.as_slice() else {
            continue;
        };
        if (trait_, member_index) != (&candidate.trait_, &candidate.member_index) {
            continue;
        }
        let requirements = item_tree::trait_requirements(db, trait_.to_id(db));
        let Some(req) = requirements.get(*member_index as usize) else {
            continue;
        };
        // A requirement whose signature isn't fully written is offered by
        // nothing: its trait carries that diagnostic, and there is no shape
        // to judge.
        let Some(sig) = lower_requirement_sig(db, trait_, req, trait_, self_ty.clone()) else {
            continue;
        };
        let Some(offer) = crate::ty::self_position_of(&sig, &self_ty)
            .and_then(|position| crate::ty::dot_offer(receiver_ty, exclusive, position))
        else {
            continue;
        };
        out.push(MemberDotOffer {
            name: candidate.name.to_owned(),
            sig,
            reach: offer.reach,
            refused: offer.refused,
        });
    }
    out
}

/// The builtin members a dot on `receiver_ty` can call: those no user member
/// or field of the same name shadows at that receiver shape.
pub fn builtin_dot_members(db: &dyn Db, file: SourceFile, receiver_ty: &Ty) -> Vec<Builtin> {
    let shape = ReceiverShape::of(receiver_ty);
    Builtin::members_of(receiver_ty)
        .iter()
        .copied()
        .filter(|builtin| {
            concrete_dot_candidates(db, file, receiver_ty, builtin.name())
                .is_none_or(|candidates| candidates.leaves_builtin(db, shape))
        })
        .collect()
}

/// The members a dot on a CONCRETE `receiver_ty` (owned, or the innermost
/// borrow of [`crate::deref_receiver`]) can call: inherent and trait-impl.
/// A row is left out when the call it writes would be refused: some member
/// of that name doesn't lower, or another member (or an fn-typed field)
/// also takes the receiver shape that row's spelling leaves.
pub fn concrete_dot_offers(
    db: &dyn Db,
    file: SourceFile,
    receiver_ty: &Ty,
    exclusive: bool,
) -> Vec<MemberDotOffer> {
    let self_ty = match receiver_ty {
        Ty::Borrow { referent, .. } => &**referent,
        _ => receiver_ty,
    };
    let Some(self_key) = SelfKey::for_ty(self_ty) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    if let Ty::Named(named) = self_ty {
        for id in crate::inherent_member_ids(db, named.decl.to_id(db)) {
            names.push(crate::item_loc(db, id).display_name().to_owned());
        }
    }
    names.extend(
        impl_requirements(db, file, &self_key, None)
            .into_iter()
            .map(|req| req.name),
    );
    let mut seen = rustc_hash::FxHashSet::default();
    names.retain(|name| seen.insert(name.clone()));
    let shape = ReceiverShape::of(receiver_ty);
    let mut out = Vec::new();
    for name in &names {
        let Some(candidates) = concrete_dot_candidates(db, file, self_ty, name) else {
            continue;
        };
        // A broken member makes every call by its name an error.
        if candidates.broken(db).is_some() {
            continue;
        }
        let members = candidates
            .inherent
            .iter()
            .chain(candidates.traits.iter().filter_map(|c| c.member.found()));
        for member in members {
            let id = member.to_id(db);
            let Some(offer) = crate::ty::member_self_position(db, id)
                .and_then(|position| crate::ty::dot_offer(receiver_ty, exclusive, position))
            else {
                continue;
            };
            // The call the accepted row writes, judged as resolution judges
            // it: from the receiver shape that row's spelling leaves.
            let written = match offer.reach {
                crate::ty::DotReach::Direct => shape,
                crate::ty::DotReach::WrittenBorrow { mutable } => ReceiverShape::Borrow { mutable },
            };
            let own = usize::from(crate::ty::member_takes(db, id, written));
            if candidates.carriers(db, written) > own {
                continue;
            }
            out.push(MemberDotOffer {
                name: name.clone(),
                sig: crate::ty::signature(db, id),
                reach: offer.reach,
                refused: offer.refused,
            });
        }
    }
    out
}
