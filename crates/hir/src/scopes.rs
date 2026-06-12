//! Lexical scopes and name resolution.
//!
//! [`expr_scopes`] depends only on the item's own [`Body`]; [`resolutions`]
//! additionally reads [`file_scope`] (range-free names of all items), so an
//! edit inside another item's body never reaches it, and adding/removing
//! items only re-resolves names — it never re-runs body lowering.

use base_db::{Db, SourceFile};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::{FxHashMap, FxHashSet};

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
        ExprData::Missing | ExprData::Literal(_) | ExprData::NameRef(_) => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resolution {
    Local(BindingId),
    Item(ItemLoc),
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

/// Top-level names of a file. Items are visible everywhere, including their
/// own bodies — mutual recursion needs no special casing.
#[salsa::tracked(returns(ref))]
pub fn file_scope(db: &dyn Db, file: SourceFile) -> FxHashMap<String, ItemLoc> {
    let tree = item_tree(db, file);
    let mut scope = FxHashMap::default();
    for (index, data) in tree.items.iter().enumerate() {
        if !data.name.is_empty() {
            // First declaration wins; duplicates will get a diagnostic later.
            scope.entry(data.name.clone()).or_insert(ItemLoc {
                file,
                index: index as u32,
            });
        }
    }
    scope
}

/// Names declared by more than one item in `file`. A reference to one still
/// resolves (first declaration wins, so goto-definition has a target), but
/// it is ambiguous: inference gives such references the type `!`, and the
/// extra declarations get a diagnostic. Range-free, so an edit that doesn't
/// change duplicate-ness backdates.
#[salsa::tracked(returns(ref))]
pub fn duplicated_names(db: &dyn Db, file: SourceFile) -> FxHashSet<String> {
    let tree = item_tree(db, file);
    let mut seen = FxHashSet::default();
    let mut duplicated = FxHashSet::default();
    for data in tree.items.iter() {
        if !data.name.is_empty() && !seen.insert(data.name.as_str()) {
            duplicated.insert(data.name.clone());
        }
    }
    duplicated
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
                .get(name)
                .copied()
                .map(Resolution::Item)
                .or_else(|| Builtin::by_name(name).map(Resolution::Builtin))
        });
        if let Some(resolution) = resolution {
            map.insert(expr, resolution);
        }
    }
    map
}
