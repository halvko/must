//! Lexical scopes and name resolution.
//!
//! [`expr_scopes`] depends only on the item's own [`Body`]; [`resolutions`]
//! additionally reads [`file_scope`] (range-free names of all items), so an
//! edit inside another item's body never reaches it, and adding/removing
//! items only re-resolves names — it never re-runs body lowering.

use base_db::{Db, SourceFile};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;

use crate::body::{BindingId, Body, ExprData, ExprId, Stmt, body};
use crate::item_tree::{ItemKind, item_tree};
use crate::{ItemId, ItemLoc};

pub type ScopeId = Idx<ScopeData>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExprScopes {
    scopes: Arena<ScopeData>,
    scope_of: ArenaMap<ExprId, ScopeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeData {
    parent: Option<ScopeId>,
    entries: Vec<(String, BindingId)>,
}

impl ExprScopes {
    pub fn scope_of(&self, expr: ExprId) -> Option<ScopeId> {
        self.scope_of.get(expr).copied()
    }

    pub fn resolve_in_scope(&self, mut scope: ScopeId, name: &str) -> Option<BindingId> {
        loop {
            let data = &self.scopes[scope];
            // Latest entry wins, so shadowing within one scope works too.
            if let Some((_, binding)) = data.entries.iter().rev().find(|(n, _)| n == name) {
                return Some(*binding);
            }
            scope = data.parent?;
        }
    }

    /// All bindings visible from `scope`, shadowing respected (used by
    /// completions later; cheap to provide now).
    pub fn visible_bindings(&self, scope: ScopeId) -> Vec<(String, BindingId)> {
        let mut seen = FxHashMap::default();
        let mut current = Some(scope);
        while let Some(scope) = current {
            let data = &self.scopes[scope];
            for (name, binding) in data.entries.iter().rev() {
                seen.entry(name.clone()).or_insert(*binding);
            }
            current = data.parent;
        }
        seen.into_iter().collect()
    }
}

#[salsa::tracked(returns(ref))]
pub fn expr_scopes<'db>(db: &'db dyn Db, item: ItemId<'db>) -> ExprScopes {
    let body = body(db, item);
    let mut scopes = ExprScopes::default();
    let root_scope = scopes.scopes.alloc(ScopeData::default());
    if let Some(root) = body.root {
        compute_expr_scopes(body, &mut scopes, root, root_scope);
    }
    scopes
}

fn compute_expr_scopes(body: &Body, scopes: &mut ExprScopes, expr: ExprId, scope: ScopeId) {
    scopes.scope_of.insert(expr, scope);
    match &body.exprs[expr] {
        ExprData::Block { stmts, tail } => {
            let mut scope = scope;
            for stmt in stmts {
                match stmt {
                    Stmt::Let { pat, init, .. } => {
                        // The initializer sees the scope *before* the
                        // pattern's bindings; shadowing is just a fresh
                        // child scope per `let`, entered with every binding
                        // the pattern introduces (one for a bare name, any
                        // number for a destructuring pattern).
                        compute_expr_scopes(body, scopes, *init, scope);
                        scope = scopes.scopes.alloc(ScopeData {
                            parent: Some(scope),
                            entries: body.pat_bindings(*pat),
                        });
                    }
                    Stmt::Assign { target, value } => {
                        compute_expr_scopes(body, scopes, *target, scope);
                        compute_expr_scopes(body, scopes, *value, scope);
                    }
                    Stmt::Expr(e) => compute_expr_scopes(body, scopes, *e, scope),
                }
            }
            if let Some(tail) = tail {
                compute_expr_scopes(body, scopes, *tail, scope);
            }
        }
        // Transparent: no scope of its own, just the parent's.
        ExprData::ConstBlock { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::FnLiteral {
            params, body: b, ..
        } => {
            let scope = scopes.scopes.alloc(ScopeData {
                parent: Some(scope),
                entries: params
                    .iter()
                    .flat_map(|p| body.pat_bindings(p.pat))
                    .collect(),
            });
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::Call { callee, args, .. } => {
            compute_expr_scopes(body, scopes, *callee, scope);
            for &arg in args {
                compute_expr_scopes(body, scopes, arg, scope);
            }
        }
        ExprData::Bin { lhs, rhs, .. } => {
            compute_expr_scopes(body, scopes, *lhs, scope);
            compute_expr_scopes(body, scopes, *rhs, scope);
        }
        ExprData::Neg { operand } => {
            compute_expr_scopes(body, scopes, *operand, scope);
        }
        ExprData::If {
            condition,
            then_branch,
            else_branch,
        } => {
            compute_expr_scopes(body, scopes, *condition, scope);
            compute_expr_scopes(body, scopes, *then_branch, scope);
            if let Some(else_branch) = else_branch {
                compute_expr_scopes(body, scopes, *else_branch, scope);
            }
        }
        ExprData::RecordLit { fields } => {
            for field in fields {
                compute_expr_scopes(body, scopes, field.value, scope);
            }
        }
        // The field name is a projection, not a scoped reference; only the
        // receiver is an expression.
        ExprData::Field { receiver, .. } => {
            compute_expr_scopes(body, scopes, *receiver, scope);
        }
        ExprData::ArrayLit { elements } => {
            for &element in elements {
                compute_expr_scopes(body, scopes, element, scope);
            }
        }
        ExprData::ArrayRepeat { element, count } => {
            compute_expr_scopes(body, scopes, *element, scope);
            compute_expr_scopes(body, scopes, *count, scope);
        }
        ExprData::Index { base, index } => {
            compute_expr_scopes(body, scopes, *base, scope);
            compute_expr_scopes(body, scopes, *index, scope);
        }
        ExprData::AddrOf { place, .. } => {
            compute_expr_scopes(body, scopes, *place, scope);
        }
        ExprData::Deref { receiver } => {
            compute_expr_scopes(body, scopes, *receiver, scope);
        }
        // Transparent, like a `const` block: a pure checker region.
        ExprData::Unsafe { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        // The variant name is resolved against the enum during inference,
        // not lexically; only the base is a scoped reference — plus any
        // turbofish const-arg values, which are ordinary scoped
        // expressions (same as a `GenericApp`'s).
        ExprData::VariantPath { base, args, .. } => {
            compute_expr_scopes(body, scopes, *base, scope);
            for arg in args.iter().flatten() {
                if let crate::body::GenericArgData::Const(value) = arg {
                    compute_expr_scopes(body, scopes, *value, scope);
                }
            }
        }
        // The base is a scoped reference like a variant path's; type args
        // are not expressions, const-arg *values* are ordinary scoped
        // expressions.
        ExprData::GenericApp { base, args } => {
            compute_expr_scopes(body, scopes, *base, scope);
            for arg in args {
                if let crate::body::GenericArgData::Const(value) = arg {
                    compute_expr_scopes(body, scopes, *value, scope);
                }
            }
        }
        ExprData::Match { scrutinee, arms } => {
            compute_expr_scopes(body, scopes, *scrutinee, scope);
            for arm in arms {
                // Each arm's body sees its own pattern's bindings — a fresh
                // child scope per arm, like a fn literal's params. A bare
                // binding always binds the whole scrutinee (never
                // reinterpreted as a variant), so its entry is just an
                // ordinary local.
                let arm_scope = scopes.scopes.alloc(ScopeData {
                    parent: Some(scope),
                    entries: body.pat_bindings(arm.pat),
                });
                compute_expr_scopes(body, scopes, arm.body, arm_scope);
            }
        }
        // A loop introduces no bindings of its own; its body is a block,
        // which scopes itself.
        ExprData::Loop { body: b } => {
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::Break { value } => {
            if let Some(value) = value {
                compute_expr_scopes(body, scopes, *value, scope);
            }
        }
        ExprData::Missing | ExprData::Literal(_) | ExprData::NameRef(_) | ExprData::Continue => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolution {
    Local(BindingId),
    /// A `const` parameter of the enclosing item's generic binder
    /// (`fn::<const N: usize>` — `N` reads as a value of its declared type
    /// inside the body). Deliberately NOT a [`Resolution::Local`]: const
    /// params have no [`BindingId`] (they are binder facts from the item
    /// tree, not body bindings), so none of the locals machinery — value
    /// slots, `mut`, scopes arena — applies to them; carrying the binder
    /// index instead keeps every consumer honest about that. Locals shadow
    /// const params; const params shadow items and builtins (the binder
    /// sits lexically between the body and the file).
    ConstParam(u32),
    Item(ItemLoc),
    /// A `type` item. Kept apart from [`Resolution::Item`] so every consumer
    /// is forced to decide what a *type* means for it: a value use is an
    /// error, a call is a construction, a type position is a name hit.
    TypeItem(ItemLoc),
    /// A `trait` item. Its own variant for the same reason as
    /// [`Resolution::TypeItem`]: a trait is neither a value nor a type —
    /// the legal positions are bounds, impl heads and the base of a
    /// qualified member call (`Display::fmt(...)`).
    TraitItem(ItemLoc),
    /// The name is defined by more than one item. Resolves to the first
    /// definition so navigation has a target, but no use can be given a
    /// meaning: inference types these as `{error}`, and the extra
    /// definitions carry the diagnostic.
    Ambiguous(ItemLoc),
    Builtin(Builtin),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Print,
    Panic,
    /// `alloc_array::<T>(n)` — one fresh heap allocation of `n`
    /// uninitialized elements. SAFE (allocating cannot UB); returns the
    /// result-shaped [`ALLOC_RESULT_NAME`] enum (`Ok(T.&raw mut)` /
    /// `Err`) — the interpreter never produces `Err` (it cannot
    /// meaningfully OOM), but the signature is stable for the codegen era.
    /// Refused in const contexts (heap values wait for an interning
    /// design).
    AllocArray,
    /// `dealloc_array::<T>(p, n)` — exact-match free. UNSAFE. `p` must be
    /// the head of a live heap allocation and `n` its alloc-time count;
    /// anything else is detected UB. Refused in const contexts.
    DeallocArray,
    /// `add(p, i)` — pointer to element `i` past `p`, within the same
    /// allocation. UNSAFE: it carries a real precondition — advancing a
    /// pointer that does not address an array element with `i > 0` is
    /// detected UB at the call. Flavor-preserving (`.&raw mut` in →
    /// `.&raw mut` out) — a checker special case, not expressible as one
    /// `fn` type. Takes a `usize`; [`Builtin::Offset`] is the signed
    /// sibling, mirroring Rust's `add`/`offset` split.
    Add,
    /// `offset(p, i)` — the signed sibling of [`Builtin::Add`]: element
    /// arithmetic in both directions, `i: isize`. UNSAFE like `add`, with
    /// one extra precondition: a result index below the allocation's start
    /// (index < 0) is detected UB at the call (the abstract machine cannot
    /// represent it — the same class as `add`'s non-array-element rule).
    /// Everything else mirrors `add`, flavor preservation included.
    Offset,
    /// `copy(src, dst, n)` — element-count bulk copy, memmove semantics
    /// (overlap is DEFINED). UNSAFE (writes through a raw pointer).
    /// Copying an uninitialized element propagates the marker silently —
    /// only reading one *as a value* traps.
    Copy,
    /// `dangling::<T>()` — a `T.&raw mut` that was never valid (one
    /// reserved never-live allocation per machine). SAFE; any deref is
    /// detected UB.
    Dangling,
}

impl Builtin {
    pub fn by_name(name: &str) -> Option<Builtin> {
        match name {
            "print" => Some(Builtin::Print),
            "panic" => Some(Builtin::Panic),
            "alloc_array" => Some(Builtin::AllocArray),
            "dealloc_array" => Some(Builtin::DeallocArray),
            "add" => Some(Builtin::Add),
            "offset" => Some(Builtin::Offset),
            "copy" => Some(Builtin::Copy),
            "dangling" => Some(Builtin::Dangling),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Panic => "panic",
            Builtin::AllocArray => "alloc_array",
            Builtin::DeallocArray => "dealloc_array",
            Builtin::Add => "add",
            Builtin::Offset => "offset",
            Builtin::Copy => "copy",
            Builtin::Dangling => "dangling",
        }
    }

    /// Whether calling this builtin requires an enclosing
    /// `unsafe { ... }` block — exactly the operations that carry a
    /// precondition whose violation is UB even without a visible deref:
    /// freeing invalidates every pointer into the allocation, `copy`
    /// writes through a raw pointer, and `add` on a pointer that does not
    /// address an array element (with `i > 0`) is detected UB at the call.
    pub fn requires_unsafe(self) -> bool {
        matches!(
            self,
            Builtin::DeallocArray | Builtin::Copy | Builtin::Add | Builtin::Offset
        )
    }
}

/// The disambiguator reserved for compiler-provided declarations —
/// [`crate::file_item_ids`] numbers real items 0.., so no source item can
/// ever collide with it, and an [`ItemLoc`] carrying it round-trips through
/// interning like any other (its item-tree queries answer the builtin
/// shape; its *source* queries answer the empty case).
pub const BUILTIN_DISAMBIGUATOR: u32 = u32::MAX;

/// The result-shaped return of `alloc_array` (and the library convention
/// for fallible allocators built over it): a compiler-provided generic enum
/// `AllocResult::<T> = enum { Ok(T.&raw mut), Err }`. Provided per FILE —
/// resolution is per-file today, so each file sees "its" declaration; the
/// identity scheme is the ordinary `ItemLoc` one with the reserved
/// disambiguator, which keeps every downstream consumer (patterns, match
/// lowering, widening, the runtime tag) on the completely ordinary
/// nominal-enum machinery.
pub const ALLOC_RESULT_NAME: &str = "AllocResult";

/// The [`ItemLoc`] of `file`'s compiler-provided [`ALLOC_RESULT_NAME`] enum.
pub fn alloc_result_loc(file: SourceFile) -> ItemLoc {
    ItemLoc::top_level(
        file,
        std::sync::Arc::from(ALLOC_RESULT_NAME),
        BUILTIN_DISAMBIGUATOR,
    )
}

/// Top-level names of a file, *including* what's wrong with them: the scope
/// decides that the first declaration wins, so it is also the analysis that
/// knows about the losers. Diagnostics travel with the analysis that
/// discovers them; the aggregator only attaches ranges. Range-free, so body
/// edits backdate it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileScope {
    entries: FxHashMap<String, ScopeEntry>,
    /// One entry per extra declaration of an already-declared name.
    pub duplicates: Vec<Duplicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopeEntry {
    loc: ItemLoc,
    kind: ItemKind,
    ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub first: ItemLoc,
    pub second: ItemLoc,
}

impl FileScope {
    pub fn resolve(&self, name: &str) -> Option<Resolution> {
        let entry = self.entries.get(name)?;
        Some(if entry.ambiguous {
            Resolution::Ambiguous(entry.loc.clone())
        } else {
            match entry.kind {
                ItemKind::Value(_) | ItemKind::Member => Resolution::Item(entry.loc.clone()),
                ItemKind::Type => Resolution::TypeItem(entry.loc.clone()),
                ItemKind::Trait => Resolution::TraitItem(entry.loc.clone()),
            }
        })
    }

    /// Every unambiguous name this file declares, each with its resolution —
    /// completions' candidate source (used instead of [`Self::resolve`] when
    /// the caller wants *all* names, not one). An ambiguous name is skipped:
    /// no single resolution names a symbol worth offering, and its
    /// definitions already carry the diagnostic.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Resolution)> {
        self.entries.iter().filter_map(|(name, entry)| {
            if entry.ambiguous {
                return None;
            }
            let resolution = match entry.kind {
                ItemKind::Value(_) | ItemKind::Member => Resolution::Item(entry.loc.clone()),
                ItemKind::Type => Resolution::TypeItem(entry.loc.clone()),
                ItemKind::Trait => Resolution::TraitItem(entry.loc.clone()),
            };
            Some((name.as_str(), resolution))
        })
    }
}

/// The type-item names of a file — the slice of [`FileScope`] that type
/// *annotation* lowering depends on. Its own query so that adding or
/// removing a **value** item leaves this value unchanged and inference of
/// annotated items backdates behind it (the item-insertion firewall test
/// pins this); depending on the full [`file_scope`] from `lower_type_ref`
/// would re-run every annotated item's inference on any item insertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeScope {
    entries: FxHashMap<String, TypeScopeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeScopeEntry {
    loc: ItemLoc,
    /// Whether the name is declared more than once file-wide (by items of
    /// *any* kind): an ambiguous name names no type, and a value item
    /// stealing a type's name must flip this value (so dependents re-run).
    ambiguous: bool,
}

impl TypeScope {
    pub fn resolve(&self, name: &str) -> Option<Resolution> {
        let entry = self.entries.get(name)?;
        Some(if entry.ambiguous {
            Resolution::Ambiguous(entry.loc.clone())
        } else {
            Resolution::TypeItem(entry.loc.clone())
        })
    }

    /// Every unambiguous type name this file declares — completions' type-
    /// position candidate source. Same ambiguity handling as
    /// [`FileScope::iter`].
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ItemLoc)> {
        self.entries
            .iter()
            .filter_map(|(name, entry)| (!entry.ambiguous).then_some((name.as_str(), &entry.loc)))
    }
}

#[salsa::tracked(returns(ref))]
pub fn type_scope(db: &dyn Db, file: SourceFile) -> TypeScope {
    let tree = item_tree(db, file);
    let mut counts: FxHashMap<&str, u32> = FxHashMap::default();
    for data in tree.items.iter() {
        *counts.entry(data.name.as_str()).or_insert(0) += 1;
    }
    // Disambiguators count occurrences the same way `file_item_ids` does,
    // so an `ItemLoc` here and the interned `ItemId` agree on identity.
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    let mut scope = TypeScope::default();
    for data in tree.items.iter() {
        let disambiguator = {
            let counter = seen.entry(data.name.as_str()).or_insert(0);
            let current = *counter;
            *counter += 1;
            current
        };
        if data.name.is_empty() || !matches!(data.kind, ItemKind::Type) {
            continue;
        }
        let loc = ItemLoc::top_level(
            file,
            std::sync::Arc::from(data.name.as_str()),
            disambiguator,
        );
        scope
            .entries
            .entry(data.name.clone())
            .or_insert(TypeScopeEntry {
                loc,
                ambiguous: counts[data.name.as_str()] > 1,
            });
    }
    // The compiler-provided declarations, visible everywhere like the
    // builtin value names — inserted only when the file doesn't declare the
    // name itself (user declarations shadow builtins, the `print`
    // precedent), and never counted as a duplicate.
    if !counts.contains_key(ALLOC_RESULT_NAME) {
        scope.entries.insert(
            ALLOC_RESULT_NAME.to_owned(),
            TypeScopeEntry {
                loc: alloc_result_loc(file),
                ambiguous: false,
            },
        );
    }
    scope
}

/// Items are visible everywhere, including their own bodies — mutual
/// recursion needs no special casing.
#[salsa::tracked(returns(ref))]
pub fn file_scope(db: &dyn Db, file: SourceFile) -> FileScope {
    let tree = item_tree(db, file);
    let mut scope = FileScope::default();
    // Disambiguators count occurrences the same way `file_item_ids` does,
    // so an `ItemLoc` here and the interned `ItemId` agree on identity.
    let mut seen: FxHashMap<&str, u32> = FxHashMap::default();
    for data in tree.items.iter() {
        let disambiguator = {
            let counter = seen.entry(data.name.as_str()).or_insert(0);
            let current = *counter;
            *counter += 1;
            current
        };
        if data.name.is_empty() {
            continue;
        }
        let loc = ItemLoc::top_level(
            file,
            std::sync::Arc::from(data.name.as_str()),
            disambiguator,
        );
        match scope.entries.entry(data.name.clone()) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(ScopeEntry {
                    loc,
                    kind: data.kind,
                    ambiguous: false,
                });
            }
            std::collections::hash_map::Entry::Occupied(mut first) => {
                first.get_mut().ambiguous = true;
                scope.duplicates.push(Duplicate {
                    first: first.get().loc.clone(),
                    second: loc,
                });
            }
        }
    }
    // The compiler-provided declarations — shadowable, never a duplicate
    // (see `type_scope`).
    if !scope.entries.contains_key(ALLOC_RESULT_NAME) {
        scope.entries.insert(
            ALLOC_RESULT_NAME.to_owned(),
            ScopeEntry {
                loc: alloc_result_loc(file),
                kind: ItemKind::Type,
                ambiguous: false,
            },
        );
    }
    scope
}

/// Resolution of every `NameRef` expression in `item`'s body. A `NameRef`
/// absent from the map is unresolved.
#[salsa::tracked(returns(ref))]
pub fn resolutions<'db>(db: &'db dyn Db, item: ItemId<'db>) -> ArenaMap<ExprId, Resolution> {
    let body = body(db, item);
    let scopes = expr_scopes(db, item);
    let file_scope = file_scope(db, item.file(db));
    // The item's own generic binder: const params are value names for the
    // whole body (the body IS the binder's literal for a generic item), a
    // scope layer between the locals and the file. A name duplicated
    // within ONE binder is diagnosed at its second occurrence (see
    // `file_diagnostics`'s duplicate-generic-parameter pass), so this
    // fallback lookup only ever has to pick one, total-lowering-style;
    // last declaration wins the same way local shadowing does.
    let generics = &crate::item_data(db, item)
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    let const_param = |name: &str| {
        generics
            .iter()
            .enumerate()
            .rev()
            .find(|(_, param)| {
                matches!(param.kind, crate::item_tree::GenericParamKind::Const(_))
                    && param.name == name
                    && !param.name.is_empty()
            })
            .map(|(index, _)| Resolution::ConstParam(index as u32))
    };
    // Inside a MEMBER's body, `Self` in expression position names the
    // owning type (so `Self(struct { ... })` constructs it) — a scope
    // layer between the locals and the file, like the binder's const
    // params.
    let self_owner = crate::member_owner(db, item).map(|owner| crate::item_loc(db, owner));
    let mut map = ArenaMap::default();
    for (expr, data) in body.exprs.iter() {
        let ExprData::NameRef(name) = data else {
            continue;
        };
        let local = scopes
            .scope_of(expr)
            .and_then(|scope| scopes.resolve_in_scope(scope, name));
        let resolution = local.map(Resolution::Local).or_else(|| {
            const_param(name).or_else(|| {
                if name == "Self"
                    && let Some(owner) = &self_owner
                {
                    return Some(Resolution::TypeItem(owner.clone()));
                }
                file_scope
                    .resolve(name)
                    .or_else(|| Builtin::by_name(name).map(Resolution::Builtin))
            })
        });
        if let Some(resolution) = resolution {
            map.insert(expr, resolution);
        }
    }
    map
}
