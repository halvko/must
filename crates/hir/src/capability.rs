//! Capabilities: what you can DO with a value of a type.
//!
//! A declaration states its CEILING — the most that can be done with one of
//! its values — and the ceiling is a rung of a ladder (T20). Two rungs
//! exist so far: `move` (pass it around, and that is all) and `forget` (let
//! it go out of scope with nothing done about it), which is the top and
//! therefore the default. A declaration that writes `type S = struct { … }
//! only move;` sits one rung down, and a container inherits the lower
//! ceiling from its parts.
//!
//! A type capped at `move` is a linear type: a value of it must be consumed
//! exactly once on every path. That is the whole of the disposal story —
//! there is no destructor, no drop glue and no unwinding, so the checker
//! (see [`crate::linear_check`]) is the only thing that runs, and it runs at
//! check time only. Codegen never learns that a type is linear.
//!
//! Two rules and their reasons:
//!
//! - **Containment infects.** A record whose field lacks `forget` lacks it;
//!   an enum whose payload lacks it lacks it. It could not be otherwise: if
//!   the container could be dropped on the floor, so could the part, and the
//!   part is the thing with the obligation.
//! - **Indirection does not.** A borrow, a raw pointer, or a `fn` value
//!   mentioning a linear type has `forget`. A borrow is not the value; a raw
//!   pointer's story is `unsafe` and always was; a `fn` type is a signature,
//!   not an inhabitant. Losing any of them loses no obligation, because the
//!   obligation stayed with the owner.
//!
//! And the generic side, which is where the capability stops being a fact
//! about a type and becomes a fact about a BODY. A data-side parameter says
//! nothing at all (T22): `Option::<String>` is linear because `String` is,
//! and the enum did not have to be told. A parameter of a generic BODY says
//! something only when the body needs it (TR11): no bound means the body is
//! checked as if the parameter were linear — move it, return it, pass it on
//! and all is well; discard it and the body is refused — and `T: forget` is
//! the positive bound a body writes when it genuinely drops a `T` on the
//! floor. Requirements are written where requirements are written; nothing
//! is assumed and nothing is subtracted.
//!
//! How an instance is judged is [`decl_flows`]: each DECLARATION is
//! summarized once, independently of any argument, and a mention is then
//! answered from its summary and its arguments without expanding anything.
//! That is what makes the question total on a declaration that names
//! itself, and it is why nothing here bounds how deep or how large a type
//! the walk will read.
//!
//! One more question is answered here, because it is answered by the same
//! walk over the same components: whether a value may be DUPLICATED. It is
//! not the disposal question read twice — a `T.&mut` may be forgotten and
//! may not be copied — so it has its own leaves ([`Affine`]) and its own
//! answer, and the two rules above hold for it unchanged: what a value
//! contains, it is; what it merely points at, it is not.

use std::collections::HashMap;

use base_db::Db;

use crate::ty::{GenericArg, Ty};
use crate::{ItemId, ItemLoc};

/// Whether values of `ty` may be forgotten — let go with nothing done about
/// them. `false` makes the type LINEAR: [`crate::linear_check`] then demands
/// exactly one consumption on every path.
///
/// Total and silent on broken types ([`Ty::Error`], unresolved variables,
/// declarations that failed to lower): an unknown type answers `true`, so a
/// broken program never grows must-consume errors on top of the error it
/// already has.
pub fn has_forget(db: &dyn Db, ty: &Ty) -> bool {
    find_component(db, ty, None, &mut Search::default(), &no_forget_leaf).is_none()
}

/// What a DISPOSAL walk looks for: something that costs a value of this
/// type the `forget` capability. Two things do, and containment carries
/// either up to whatever holds it.
fn no_forget_leaf(db: &dyn Db, ty: &Ty) -> Option<()> {
    match ty {
        // A declaration that capped itself.
        Ty::Named(named) if decl_only_move(db, &named.decl) => Some(()),
        // A variant-typed value is its payload tuple; the enum's own
        // ceiling covers it too, since widening one to the other must not
        // change what has to happen to it.
        Ty::Variant(variant) if decl_only_move(db, &variant.decl) => Some(()),
        // A rigid parameter is whatever its binder ASKED FOR. `T: forget`
        // makes every caller supply a forgettable type, so the body may
        // forget it; no bound assumes nothing, so the body is checked as
        // if the parameter were linear.
        Ty::Param(param) if !param_has_forget_bound(db, &param.item, param.index) => Some(()),
        _ => None,
    }
}

/// Whether a value of `ty` may be DUPLICATED by a plain read — `x.*`, a
/// second read of a binding, `[x; 3]`.
///
/// The other capability question, and deliberately not a reading of the
/// first: `forget` answers what may be LOST, this answers what may be
/// COPIED, and the language supplies its own witness that one does not
/// decide the other. `T.&mut` HAS `forget` — losing a borrow loses no
/// obligation — and may not be copied anyway, because the whole
/// exclusivity story rests on there being one of it.
pub fn is_copyable(db: &dyn Db, ty: &Ty) -> bool {
    affine_root(db, ty).is_none()
}

/// What makes a value of `ty` unduplicable, or `None` when a plain read of
/// one is an ordinary copy. The root the DUPLICATION refusals name, and
/// [`is_copyable`]'s own answer — one walk, so the predicate and the
/// message can never disagree about why.
pub fn affine_root(db: &dyn Db, ty: &Ty) -> Option<Affine> {
    find_component(db, ty, None, &mut Search::default(), &affine_leaf)
}

/// Why a value may not be duplicated. Three roots, three different
/// sentences, and the reader needs to know which: the first has a
/// declaration to look at, the second has only a binder, and the third has
/// neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Affine {
    /// A LINEAR type. Duplicating it duplicates an OBLIGATION, and the two
    /// copies name one resource — which is how a checked-linear `String`
    /// would come to be freed twice.
    Linear,
    /// A rigid type PARAMETER, bounded or not (TR11). Not "linearity read
    /// at a binder": `T: forget` waives the CONSUMPTION and grants no
    /// copying, since instantiating such a `T` with `usize.&mut` would
    /// make a second read of one binding a second exclusive borrow of one
    /// place. Refusing every parameter is the strict, reversible
    /// direction — a future `T: copy` can only ever accept more programs.
    Param(String),
    /// An exclusive BORROW held inside the value. Duplicating it
    /// duplicates a PERMISSION; two handles on one loan is the thing
    /// exclusivity exists to prevent.
    MutBorrow,
}

fn affine_leaf(db: &dyn Db, ty: &Ty) -> Option<Affine> {
    match ty {
        Ty::Param(param) => Some(Affine::Param(param.name.to_string())),
        Ty::Borrow { mutable: true, .. } => Some(Affine::MutBorrow),
        // Everything a value must be consumed FOR, it must also not be
        // duplicated for. The leaves are the disposal walk's own.
        _ => no_forget_leaf(db, ty).map(|()| Affine::Linear),
    }
}

/// A one-line explanation of why `ty` lacks `forget`, for the diagnostic
/// that refuses it — `None` when it has the capability. Names what CAPPED
/// itself below the capability and, when the cap came through containment,
/// the path that carried it (`through field \`buf\``).
pub fn no_forget_reason(db: &dyn Db, ty: &Ty) -> Option<String> {
    let mut path = Vec::new();
    let root = blame(db, ty, None, &mut Search::default(), &mut path)?;
    if path.is_empty() {
        return Some(root.sentence);
    }
    path.reverse();
    Some(format!("{}, reached {}", root.sentence, path.join(", ")))
}

/// The rigid PARAMETER at the root of `ty`'s obligation, when a binder is
/// what put it there — directly (`t: T`) or through containment
/// (`b: Box::<T>`, linear only because its payload might be). `None` when
/// the type may be forgotten, or when a DECLARATION is the root instead:
/// that case has a declaration to point at and this one does not, which is
/// the whole reason a diagnostic wants to tell them apart.
///
/// It is [`blame`]'s OWN root, not a second search for one, and that is
/// load-bearing rather than tidy. A type can be linear for two reasons at
/// once — `struct::<T> { a: Lin, b: T }` — and a second walk with its own
/// preference order would name `T` while the message named `Lin`, so the
/// diagnostic would offer `T: forget` for a program that stays linear
/// after taking the advice. One walk, one root, one fix.
pub fn blaming_param(db: &dyn Db, ty: &Ty) -> Option<String> {
    let mut path = Vec::new();
    blame(db, ty, None, &mut Search::default(), &mut path)?.param
}

/// Find the first component of `ty` — `ty` itself, or something a value of
/// it CONTAINS — that `leaf` has a verdict about.
///
/// The one traversal behind every capability question. Containment infects
/// and indirection does not, so it descends through records, arrays,
/// nominal mentions and variants and stops at everything else; `leaf` says
/// which of the types it meets is an answer. An ALL-question ("may this be
/// forgotten?") asks for the first component that says no and reads a
/// `None` as yes; an ANY-question ("what makes this unduplicable?") reads
/// the finding itself.
///
/// `owner` is the declaration whose own components are being walked, when
/// one is — see [`find_in_mention`].
fn find_component<T>(
    db: &dyn Db,
    ty: &Ty,
    owner: Option<&ItemLoc>,
    search: &mut Search,
    leaf: &impl Fn(&dyn Db, &Ty) -> Option<T>,
) -> Option<T> {
    // A parameter of the declaration whose components these are is not a
    // component of anything: what it stands for is decided at the MENTION,
    // by the argument in its place, and [`decl_flows`] is what says whether
    // this position is one that argument reaches.
    if let Ty::Param(param) = ty {
        if owner == Some(&param.item) {
            return None;
        }
    }
    if let Some(found) = leaf(db, ty) {
        return Some(found);
    }
    match ty {
        Ty::Record(record) => record
            .fields
            .iter()
            .find_map(|(_, field)| find_component(db, field, owner, search, leaf)),
        Ty::Array { elem, .. } => find_component(db, elem, owner, search, leaf),
        Ty::Named(named) => {
            find_in_mention(db, &named.decl, None, &named.args, owner, search, leaf)
        }
        Ty::Variant(variant) => find_in_mention(
            db,
            &variant.decl,
            Some(variant.index),
            &variant.args,
            owner,
            search,
            leaf,
        ),
        // Indirection does not infect: what a value points at, it does
        // not hold. A parameter reaching here is either bounded or another
        // declaration's, and `leaf` has said what it says about it.
        Ty::RawPtr { .. } | Ty::Borrow { .. } | Ty::Fn(_) | Ty::Param(_) => None,
        // Nothing inside to look at. Named so that a type form added later
        // has to say which side of containment it is on.
        Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => None,
    }
}

/// Judge one MENTION from its declaration's summary: the declaration's own
/// components first, then the arguments the summary says reach a value
/// position of it.
///
/// Nothing is substituted and no nested mention is expanded, which is the
/// whole termination argument. The declaration side is walked at most once
/// per query ([`Search`]) and there are finitely many declarations; the
/// argument side descends into a strictly smaller piece of finite syntax.
/// So `type Nest = enum::<T> { Cons(T, Nest::<Pair::<T>>), Nil }` is
/// ANSWERED — `Nest::<usize>` may be forgotten, `Nest::<String>` may not —
/// where any walk over the instantiated type would have to bound itself
/// and guess.
fn find_in_mention<T>(
    db: &dyn Db,
    decl: &ItemLoc,
    variant: Option<u32>,
    args: &[GenericArg],
    owner: Option<&ItemLoc>,
    search: &mut Search,
    leaf: &impl Fn(&dyn Db, &Ty) -> Option<T>,
) -> Option<T> {
    if search.enter(decl, variant) {
        let found = decl_components(db, decl, variant)
            .iter()
            .find_map(|(_, component)| find_component(db, component, Some(decl), search, leaf));
        if found.is_some() {
            return found;
        }
    }
    for &index in flows(db, decl, variant) {
        let Some(GenericArg::Ty(arg)) = args.get(index as usize) else {
            continue;
        };
        if let Some(found) = find_component(db, arg, owner, search, leaf) {
            return Some(found);
        }
    }
    None
}

/// One query's state: the declarations whose own components it has already
/// searched.
///
/// The visited set is never unwound, because what it remembers is a
/// property of a DECLARATION and not of a route: a declaration's own
/// components hold what they hold wherever the declaration is mentioned
/// from, its parameters are excluded from them by construction, and any
/// finding stops the whole query — so a declaration that answered nothing
/// answers nothing anywhere, and re-walking it is work with no answer in
/// it.
///
/// What it saves is sharing INSIDE the declaration graph, not nesting in
/// the arguments: a mention nested in its own argument (`Q::<Q::<…>>`)
/// already costs one walk per level, since each level's summary is read
/// once and its argument descended once. The shape that needs the set is a
/// declaration reached through two components of one declaration —
/// `type D1 = struct::<T> { a: D0::<T>, b: D0::<T> }`, `D2` over `D1`,
/// and so on — where a walk with no memory of `D0` would visit it
/// 2^depth times.
#[derive(Default)]
struct Search {
    seen: Vec<(ItemLoc, Option<u32>)>,
}

impl Search {
    /// Claim a declaration's own components for this query — `false` when
    /// they have already been searched.
    fn enter(&mut self, decl: &ItemLoc, variant: Option<u32>) -> bool {
        // The enum's own components ARE every variant's payloads, so a
        // whole-declaration walk subsumes each variant's; the reverse does
        // not hold, and the key keeps them apart.
        let claimed = |of: &Option<u32>| *of == variant || (variant.is_some() && of.is_none());
        if self
            .seen
            .iter()
            .any(|(seen, of)| seen == decl && claimed(of))
        {
            return false;
        }
        self.seen.push((decl.clone(), variant));
        true
    }
}

/// The parameters of `decl` whose arguments a mention of this shape — the
/// declaration, or one of its variants — holds. A declaration that failed
/// to lower flows nothing, which is what it would have computed.
fn flows<'db>(db: &'db dyn Db, decl: &ItemLoc, variant: Option<u32>) -> &'db [u32] {
    decl_flows(db, decl.to_id(db)).of(variant)
}

/// One declaration's parametric summary: which of its parameters reach a
/// VALUE position of it, so that a mention holds the argument in that
/// place. Summarized once, independently of any argument.
///
/// This is the finite half of the construction. The parametric question is
/// answered on the declaration graph, which is finite; an instance is then
/// judged compositionally from the summary and its own arguments, which
/// are finite syntax. Neither half ever expands a nested mention, so the
/// answer terminates with no bound on depth or size, and no inhabited type
/// is refused for want of such a bound.
#[derive(Debug, Default, PartialEq, Eq)]
struct DeclFlows {
    /// For the declaration itself: a struct's fields, or every variant's
    /// payloads.
    all: Vec<u32>,
    /// For each variant on its own. A variant-typed value is its own
    /// payload tuple and nothing else: `Opt::<Res>::N` holds no `Res`.
    variants: Vec<Vec<u32>>,
}

impl DeclFlows {
    fn of(&self, variant: Option<u32>) -> &[u32] {
        match variant {
            None => &self.all,
            Some(index) => self.variants.get(index as usize).map_or(&[], Vec::as_slice),
        }
    }
}

/// [`DeclFlows`] for one declaration, memoized by salsa: it changes only
/// when a declaration it can reach changes, and a query then costs a
/// lookup per mention.
///
/// The LEAST fixpoint over every declaration reachable from this one, and
/// it has to be least: no parameter flows until some component says it
/// does. `type P = struct::<T> { v: P::<W::<T>> }` is the case that decides
/// it — assume `P`'s parameter flows and it flows forever, taking every
/// argument any `P` was ever given with it; start from nothing and the
/// truth comes out, which is that every value position of a `P` is another
/// `P` and no `T` is ever held. The iteration is monotone (a component can
/// only add positions) and bounded by the parameter count, so it ends.
#[salsa::tracked(returns(ref))]
fn decl_flows<'db>(db: &'db dyn Db, item: ItemId<'db>) -> DeclFlows {
    let root = crate::item_loc(db, item);
    let decls = reachable_decls(db, &root);
    let mut known: HashMap<ItemLoc, DeclFlows> = HashMap::new();
    loop {
        let mut changed = false;
        // Leaves first: a flow enters a declaration through the ones it
        // mentions, so a chain converges in a round or two this way round
        // and in one round per link the other.
        for decl in decls.iter().rev() {
            let flows = DeclFlows {
                all: value_params(&known, decl, &decl_components(db, decl, None)),
                variants: (0..variant_count(db, decl))
                    .map(|index| {
                        value_params(&known, decl, &decl_components(db, decl, Some(index)))
                    })
                    .collect(),
            };
            if known.get(decl) != Some(&flows) {
                known.insert(decl.clone(), flows);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    known.remove(&root).unwrap_or_default()
}

/// What the fixpoint knows so far about a mention of this shape.
fn known_flows<'a>(
    known: &'a HashMap<ItemLoc, DeclFlows>,
    decl: &ItemLoc,
    variant: Option<u32>,
) -> &'a [u32] {
    known.get(decl).map_or(&[], |flows| flows.of(variant))
}

/// Which of `decl`'s parameters reach a VALUE position of `components`.
///
/// A value position is the component itself, a field of a record it holds,
/// an array's element — and the argument of a nested mention, at exactly
/// the positions that mention's OWN summary says reach a value position of
/// it. A pointer, a borrow and a `fn` type are leaves: what a value points
/// at, it does not hold (T21), and linearity travels by ownership only.
fn value_params(
    known: &HashMap<ItemLoc, DeclFlows>,
    decl: &ItemLoc,
    components: &[(Step, Ty)],
) -> Vec<u32> {
    let mut found = Vec::new();
    for (_, component) in components {
        params_in_value_position(known, decl, component, &mut found);
    }
    found.sort_unstable();
    found.dedup();
    found
}

fn params_in_value_position(
    known: &HashMap<ItemLoc, DeclFlows>,
    decl: &ItemLoc,
    ty: &Ty,
    found: &mut Vec<u32>,
) {
    match ty {
        Ty::Param(param) if param.item == *decl => found.push(param.index),
        Ty::Record(record) => record
            .fields
            .iter()
            .for_each(|(_, field)| params_in_value_position(known, decl, field, found)),
        Ty::Array { elem, .. } => params_in_value_position(known, decl, elem, found),
        Ty::Named(named) => {
            for &index in known_flows(known, &named.decl, None) {
                if let Some(GenericArg::Ty(arg)) = named.args.get(index as usize) {
                    params_in_value_position(known, decl, arg, found);
                }
            }
        }
        Ty::Variant(variant) => {
            for &index in known_flows(known, &variant.decl, Some(variant.index)) {
                if let Some(GenericArg::Ty(arg)) = variant.args.get(index as usize) {
                    params_in_value_position(known, decl, arg, found);
                }
            }
        }
        // Leaves: a pointee is not held, and a parameter of any other
        // declaration cannot occur in this one's components. Named, not
        // wildcarded, because a type form left out here would be a hole a
        // linear value could fall through unjudged.
        Ty::RawPtr { .. } | Ty::Borrow { .. } | Ty::Fn(_) | Ty::Param(_) => {}
        Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => {}
    }
}

/// Every declaration `root` can reach by containment: itself, and
/// transitively the ones its value positions name. The set the fixpoint
/// runs over, and the reason it is a fixpoint over something finite.
///
/// The edges are [`params_in_value_position`]'s own, widened to every
/// argument of a mention because which arguments flow is what the fixpoint
/// is computing — so every summary it reads is one it computed. Nothing
/// past a pointer, a borrow or a `fn` type is followed: the fixpoint never
/// reads a summary from there, so a declaration reachable only through a
/// pointee could not change the answer, and summarizing it anyway would
/// only make this memo depend on declarations whose edits cannot alter it.
fn reachable_decls(db: &dyn Db, root: &ItemLoc) -> Vec<ItemLoc> {
    let components = |decl: &ItemLoc| {
        decl_components(db, decl, None)
            .into_iter()
            .map(|(_, ty)| ty)
    };
    let mut found = vec![root.clone()];
    let mut pending: Vec<Ty> = components(root).collect();
    while let Some(ty) = pending.pop() {
        mentions(&ty, &mut |decl| {
            if found.contains(decl) {
                return;
            }
            found.push(decl.clone());
            pending.extend(components(decl));
        });
    }
    found
}

/// Every nominal declaration the type TERM `ty` names in a value position,
/// at any depth; nothing on the far side of an indirection.
fn mentions(ty: &Ty, found: &mut impl FnMut(&ItemLoc)) {
    let args = match ty {
        Ty::Named(named) => {
            found(&named.decl);
            named.args.as_slice()
        }
        Ty::Variant(variant) => {
            found(&variant.decl);
            variant.args.as_slice()
        }
        Ty::Record(record) => {
            record
                .fields
                .iter()
                .for_each(|(_, field)| mentions(field, found));
            &[]
        }
        Ty::Array { elem, .. } => {
            mentions(elem, found);
            &[]
        }
        Ty::RawPtr { .. } | Ty::Borrow { .. } | Ty::Fn(_) | Ty::Param(_) => &[],
        Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => &[],
    };
    for arg in args {
        if let GenericArg::Ty(arg) = arg {
            mentions(arg, found);
        }
    }
}

/// How a message names the step from a declaration down to one of its own
/// components. Carried alongside the component rather than phrased on the
/// spot, because only [`blame`] ever renders one.
enum Step {
    Field(String),
    Payload(String),
}

impl Step {
    fn phrase(&self) -> String {
        match self {
            Step::Field(name) => format!("through field `{name}`"),
            Step::Payload(name) => format!("through the `::{name}` payload"),
        }
    }
}

/// A declaration's OWN components, with its parameters left RIGID: a
/// struct's fields, one variant's payloads, or every variant's payloads for
/// the enum itself. Nothing is substituted — the arguments are the other
/// half of the answer, and keeping them out of this half is what makes it a
/// property of the declaration.
fn decl_components(db: &dyn Db, decl: &ItemLoc, variant: Option<u32>) -> Vec<(Step, Ty)> {
    let item = decl.to_id(db);
    if let Some(Ty::Record(record)) = crate::ty::type_underlying(db, item) {
        return record
            .fields
            .iter()
            .map(|(name, ty)| (Step::Field(name.clone()), ty.clone()))
            .collect();
    }
    let Some(variants) = crate::ty::enum_variants(db, item).as_ref() else {
        return Vec::new();
    };
    let payloads = |(name, payload): &(String, Vec<Ty>)| {
        payload
            .iter()
            .map(|ty| (Step::Payload(name.clone()), ty.clone()))
            .collect::<Vec<_>>()
    };
    match variant {
        Some(index) => variants
            .get(index as usize)
            .map(payloads)
            .unwrap_or_default(),
        None => variants.iter().flat_map(payloads).collect(),
    }
}

/// How many variants `decl` declares — none when it is a struct or broken.
fn variant_count(db: &dyn Db, decl: &ItemLoc) -> u32 {
    crate::ty::enum_variants(db, decl.to_id(db))
        .as_ref()
        .map_or(0, |variants| variants.len() as u32)
}

/// Whether the `type` declaration at `decl` wrote `only move`.
pub fn decl_only_move(db: &dyn Db, decl: &ItemLoc) -> bool {
    crate::item_data(db, decl.to_id(db))
        .as_ref()
        .is_some_and(|data| data.only_move)
}

/// Whether the generic parameter at `(item, index)` wrote the `forget`
/// bound. A missing or non-type parameter answers `false` — nothing is
/// assumed of a parameter, and a broken binder promises nothing either.
pub fn param_has_forget_bound(db: &dyn Db, item: &ItemLoc, index: u32) -> bool {
    crate::item_data(db, item.to_id(db))
        .as_ref()
        .and_then(|data| data.generics.get(index as usize))
        .is_some_and(|param| param.forget)
}

/// What the walk found to blame: the phrased root clause, and — when the
/// root is a rigid PARAMETER rather than a declaration — its name.
struct Blame {
    /// The root clause, already phrased: the two kinds of root are
    /// different sentences (a DECLARATION capped itself; a rigid PARAMETER
    /// simply never asked for anything), so the caller is not left to guess
    /// a verb for a bare name.
    sentence: String,
    /// `Some` exactly when the root is a parameter — the one case where a
    /// diagnostic can offer `T: forget` and be right about it.
    param: Option<String>,
}

impl Blame {
    fn decl(decl: &ItemLoc) -> Blame {
        Blame {
            sentence: format!("`{}` is declared `only move`", decl.display_name()),
            param: None,
        }
    }

    fn param(name: &str) -> Blame {
        Blame {
            sentence: format!(
                "`{name}` has no `forget` bound, so it may be a type that must be consumed"
            ),
            param: Some(name.to_owned()),
        }
    }
}

/// The named-in-the-message half of [`no_forget_reason`]: the clause that
/// caps the capability, said as a sentence, plus the containment path back
/// to `ty`. A declaration IS declared `only move`; a type parameter is not
/// declared anything — it simply never asked for the capability — so the
/// two roots read differently, and only the second has a fix to offer.
///
/// [`find_component`]'s walk with a path built on the way out, which is
/// the one thing that walk cannot do for it: the step names it reports
/// (`through field \`buf\``) are the container's, and a component list has
/// already thrown them away. It shares the [`Search`] and the summaries all
/// the same, so the answer can never disagree with [`has_forget`]'s.
fn blame(
    db: &dyn Db,
    ty: &Ty,
    owner: Option<&ItemLoc>,
    search: &mut Search,
    path: &mut Vec<String>,
) -> Option<Blame> {
    if let Ty::Param(param) = ty {
        if owner == Some(&param.item) {
            return None;
        }
    }
    match ty {
        Ty::Named(named) => {
            if decl_only_move(db, &named.decl) {
                return Some(Blame::decl(&named.decl));
            }
            blame_mention(db, &named.decl, None, &named.args, owner, search, path)
        }
        Ty::Variant(variant) => {
            if decl_only_move(db, &variant.decl) {
                return Some(Blame::decl(&variant.decl));
            }
            blame_mention(
                db,
                &variant.decl,
                Some(variant.index),
                &variant.args,
                owner,
                search,
                path,
            )
        }
        Ty::Record(record) => record.fields.iter().find_map(|(name, field)| {
            let root = blame(db, field, owner, search, path)?;
            path.push(format!("through field `{name}`"));
            Some(root)
        }),
        Ty::Array { elem, .. } => {
            let root = blame(db, elem, owner, search, path)?;
            path.push("through the array's element type".to_owned());
            Some(root)
        }
        Ty::Param(param) if !param_has_forget_bound(db, &param.item, param.index) => {
            Some(Blame::param(&param.name))
        }
        Ty::RawPtr { .. } | Ty::Borrow { .. } | Ty::Fn(_) | Ty::Param(_) => None,
        Ty::Infer(_)
        | Ty::UnresolvedNumber
        | Ty::Unit
        | Ty::Never
        | Ty::Int(_)
        | Ty::Str
        | Ty::Bool
        | Ty::Char
        | Ty::Error => None,
    }
}

fn blame_mention(
    db: &dyn Db,
    decl: &ItemLoc,
    variant: Option<u32>,
    args: &[GenericArg],
    owner: Option<&ItemLoc>,
    search: &mut Search,
    path: &mut Vec<String>,
) -> Option<Blame> {
    if search.enter(decl, variant) {
        let found = decl_components(db, decl, variant)
            .iter()
            .find_map(|(step, component)| {
                let root = blame(db, component, Some(decl), search, path)?;
                path.push(step.phrase());
                Some(root)
            });
        if found.is_some() {
            return found;
        }
    }
    for &index in flows(db, decl, variant) {
        let Some(GenericArg::Ty(arg)) = args.get(index as usize) else {
            continue;
        };
        if let Some(root) = blame(db, arg, owner, search, path) {
            path.push(argument_step(db, decl, index));
            return Some(root);
        }
    }
    None
}

/// The step a message reports for an ARGUMENT that cost a mention its
/// capability: the parameter it was given for, named. The route from the
/// declaration down to that parameter's value positions is not walked for
/// it — the mention holds the argument because the parameter reaches a
/// value position, and the parameter is what the reader has to see to know
/// which argument to change.
fn argument_step(db: &dyn Db, decl: &ItemLoc, index: u32) -> String {
    let name = crate::item_data(db, decl.to_id(db))
        .as_ref()
        .and_then(|data| data.generics.get(index as usize))
        .map(|param| param.name.clone())
        .unwrap_or_default();
    if name.is_empty() {
        return format!("through an argument of `{}`", decl.display_name());
    }
    format!("through `{}`'s `{name}`", decl.display_name())
}
