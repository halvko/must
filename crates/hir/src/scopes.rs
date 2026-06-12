//! Lexical scopes and name resolution.
//!
//! [`expr_scopes`] depends only on the item's own [`Body`]; [`resolutions`]
//! additionally reads [`file_scope`] (range-free names of all items), so an
//! edit inside another item's body never reaches it, and adding/removing
//! items only re-resolves names — it never re-runs body lowering.

use base_db::{Db, SourceFile};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;

use crate::body::{Body, BindingId, ExprData, ExprId, Stmt, body};
use crate::item_tree::item_tree;
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
                    Stmt::Let { binding, init } => {
                        // The initializer sees the scope *before* the binding;
                        // shadowing is just a fresh child scope per `let`.
                        compute_expr_scopes(body, scopes, *init, scope);
                        scope = scopes.scopes.alloc(ScopeData {
                            parent: Some(scope),
                            entries: vec![(body.bindings[*binding].name.clone(), *binding)],
                        });
                    }
                    Stmt::Expr(e) => compute_expr_scopes(body, scopes, *e, scope),
                }
            }
            if let Some(tail) = tail {
                compute_expr_scopes(body, scopes, *tail, scope);
            }
        }
        ExprData::FnLiteral { params, body: b, .. } => {
            let scope = scopes.scopes.alloc(ScopeData {
                parent: Some(scope),
                entries: params
                    .iter()
                    .map(|&p| (body.bindings[p].name.clone(), p))
                    .collect(),
            });
            compute_expr_scopes(body, scopes, *b, scope);
        }
        ExprData::Call { callee, args } => {
            compute_expr_scopes(body, scopes, *callee, scope);
            for &arg in args {
                compute_expr_scopes(body, scopes, arg, scope);
            }
        }
        ExprData::Bin { lhs, rhs, .. } => {
            compute_expr_scopes(body, scopes, *lhs, scope);
            compute_expr_scopes(body, scopes, *rhs, scope);
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
        ExprData::Missing | ExprData::Literal(_) | ExprData::NameRef(_) => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolution {
    Local(BindingId),
    Item(ItemLoc),
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
}

impl Builtin {
    pub fn by_name(name: &str) -> Option<Builtin> {
        match name {
            "print" => Some(Builtin::Print),
            "panic" => Some(Builtin::Panic),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Panic => "panic",
        }
    }
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
            Resolution::Item(entry.loc.clone())
        })
    }
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
        let loc = ItemLoc {
            file,
            name: std::sync::Arc::from(data.name.as_str()),
            disambiguator,
        };
        match scope.entries.entry(data.name.clone()) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(ScopeEntry {
                    loc,
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
    scope
}

/// Resolution of every `NameRef` expression in `item`'s body. A `NameRef`
/// absent from the map is unresolved.
#[salsa::tracked(returns(ref))]
pub fn resolutions<'db>(db: &'db dyn Db, item: ItemId<'db>) -> ArenaMap<ExprId, Resolution> {
    let body = body(db, item);
    let scopes = expr_scopes(db, item);
    let file_scope = file_scope(db, item.file(db));
    let mut map = ArenaMap::default();
    for (expr, data) in body.exprs.iter() {
        let ExprData::NameRef(name) = data else {
            continue;
        };
        let local = scopes
            .scope_of(expr)
            .and_then(|scope| scopes.resolve_in_scope(scope, name));
        let resolution = local.map(Resolution::Local).or_else(|| {
            file_scope
                .resolve(name)
                .or_else(|| Builtin::by_name(name).map(Resolution::Builtin))
        });
        if let Some(resolution) = resolution {
            map.insert(expr, resolution);
        }
    }
    map
}
