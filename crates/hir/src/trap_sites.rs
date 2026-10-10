//! Where an error that carries only a source range traps when the code it
//! sits in runs: at the innermost expression or statement containing it, or
//! on the item's value when it lies outside the body.

use base_db::{Db, SourceFile, parse};
use rustc_hash::FxHashMap;
use syntax::ast::AstNode as _;
use syntax::{SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::body::{Body, BodySourceMap, ExprData, Stmt};
use crate::{
    ExprId, ItemId, ItemLoc, PatId, Severity, body_with_source_map, checkable_item_at, item_loc,
    item_source, range_diagnostics,
};

/// A point in an item's body where control traps with an error's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapSite {
    /// Control reaching the expression traps before any of it runs.
    Expr(ExprId),
    /// Control reaching statement `index` of the block traps; an `index`
    /// equal to the statement count is the tail.
    Stmt { block: ExprId, index: usize },
}

/// The trap site and message of every parse error and [`range_diagnostics`]
/// error that lies in `item`'s own syntax. An item with no body (a `type` or `trait`) has
/// none: its errors trap nowhere.
#[salsa::tracked(returns(ref))]
pub fn range_traps<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<(TrapSite, String)> {
    let file = item.file(db);
    let loc = item_loc(db, item);
    let Some((_, errors)) = body_errors(db, file)
        .iter()
        .find(|(owner, _)| *owner == loc)
    else {
        return Vec::new();
    };
    let Some(sites) = Sites::new(db, item) else {
        return Vec::new();
    };
    errors
        .iter()
        .map(|error| (sites.site(error.blame, error.at), error.message.clone()))
        .collect()
}

/// An error inside a body: the range it is blamed on, its own position, and
/// its message.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct BodyError {
    pub blame: TextRange,
    pub at: TextSize,
    pub message: String,
}

/// Every parse error and [`range_diagnostics`] error inside a body, by the
/// item or member whose body it is.
#[salsa::tracked(returns(ref))]
pub fn body_errors(db: &dyn Db, file: SourceFile) -> Vec<(ItemLoc, Vec<BodyError>)> {
    let parse = parse(db, file);
    // A parse error belongs to the construct the parser was building,
    // which can end before the token it points at.
    let parse_errors = (parse.errors().iter().enumerate())
        .map(|(index, err)| (parse.blame(index), err.range, &err.message));
    let range_errors = (range_diagnostics(db, file).iter())
        .filter(|diag| diag.severity == Severity::Error)
        .map(|diag| (diag.range, diag.range, &diag.message));
    let tree = parse.syntax_node();
    let mut owned = Vec::new();
    for (blame, range, message) in parse_errors.chain(range_errors) {
        let element = tree.covering_element(blame);
        let node = element
            .clone()
            .into_node()
            .or_else(|| element.into_token()?.parent());
        let Some(owner) = node.and_then(|node| checkable_item_at(db, file, &node)) else {
            continue;
        };
        if crate::body::body(db, owner).root.is_none() {
            continue;
        }
        let error = BodyError {
            blame,
            at: range.start(),
            message: message.clone(),
        };
        push_owned(&mut owned, item_loc(db, owner), error);
    }
    owned
}

fn push_owned<T>(owned: &mut Vec<(ItemLoc, Vec<T>)>, loc: ItemLoc, error: T) {
    match owned.iter_mut().find(|(owner, _)| *owner == loc) {
        Some((_, errors)) => errors.push(error),
        None => owned.push((loc, vec![error])),
    }
}

/// The trap site of each finding reported on one of `item`'s patterns: the
/// innermost construct containing the pattern.
pub fn trap_sites_for_pats(
    db: &dyn Db,
    item: ItemId<'_>,
    findings: Vec<(PatId, String)>,
) -> Vec<(TrapSite, String)> {
    if findings.is_empty() {
        return Vec::new();
    }
    let Some(sites) = Sites::new(db, item) else {
        return Vec::new();
    };
    findings
        .into_iter()
        .filter_map(|(pat, message)| {
            let range = sites.source_map.node_for_pat(pat)?.text_range();
            Some((sites.site(range, range.start()), message))
        })
        .collect()
}

/// The syntax that declares `item`: the item, or the member.
fn item_decl(db: &dyn Db, item: ItemId<'_>) -> Option<SyntaxNode> {
    if item.member(db).is_some() {
        crate::item_tree::member_source(db, item).map(|member| member.syntax().clone())
    } else {
        item_source(db, item).map(|it| it.syntax().clone())
    }
}

/// What finding the trap sites in one item's body needs.
struct Sites<'db> {
    tree: SyntaxNode,
    decl: SyntaxNode,
    body: &'db Body,
    source_map: &'db BodySourceMap,
    root: ExprId,
    /// Each head's construction: the call a callee belongs to, the path or
    /// turbofish a base belongs to.
    heads: FxHashMap<ExprId, ExprId>,
}

impl<'db> Sites<'db> {
    /// `None` for an item with no body.
    fn new(db: &'db dyn Db, item: ItemId<'db>) -> Option<Self> {
        let (body, source_map) = body_with_source_map(db, item);
        let root = body.root?;
        let decl = item_decl(db, item)?;
        let tree = parse(db, item.file(db)).syntax_node();
        let heads = body
            .exprs
            .iter()
            .filter_map(|(outer, data)| match data {
                ExprData::Call { callee: head, .. }
                | ExprData::VariantPath { base: head, .. }
                | ExprData::GenericApp { base: head, .. } => Some((*head, outer)),
                _ => None,
            })
            .collect();
        Some(Sites {
            tree,
            decl,
            body,
            source_map,
            root,
            heads,
        })
    }

    /// The site for an error blamed on `range`: the innermost expression
    /// containing it, or the statement of the innermost block that `at`,
    /// the error's own position, falls in.
    fn site(&self, range: TextRange, at: TextSize) -> TrapSite {
        let element = self.tree.covering_element(range);
        let Some(covering) = element
            .clone()
            .into_node()
            .or_else(|| element.into_token()?.parent())
        else {
            return TrapSite::Expr(self.root);
        };
        let start = if self.decl.text_range().contains_range(covering.text_range()) {
            covering
        } else {
            self.decl.clone()
        };
        for node in start.ancestors() {
            if let Some(expr) = self.source_map.expr_for_node(SyntaxNodePtr::new(&node)) {
                if let ExprData::Block { stmts, .. } = &self.body.exprs[expr] {
                    let index = stmts
                        .iter()
                        .position(|stmt| {
                            stmt_end(self.source_map, stmt).is_some_and(|end| end > at)
                        })
                        .unwrap_or(stmts.len());
                    return TrapSite::Stmt { block: expr, index };
                }
                return TrapSite::Expr(self.headed(expr));
            }
            if node == self.decl {
                break;
            }
        }
        TrapSite::Expr(self.root)
    }

    /// The outermost expression `expr` is the head of. A construction
    /// lowers its head as part of itself, so the head's trap belongs to it.
    fn headed(&self, mut expr: ExprId) -> ExprId {
        while let Some(&outer) = self.heads.get(&expr) {
            expr = outer;
        }
        expr
    }
}

/// Where a statement's last lowered part ends.
fn stmt_end(source_map: &BodySourceMap, stmt: &Stmt) -> Option<TextSize> {
    let ptr = match stmt {
        Stmt::Let { pat, init, .. } => source_map
            .node_for_expr(*init)
            .or_else(|| source_map.node_for_pat(*pat)),
        Stmt::Assign { value, .. } => source_map.node_for_expr(*value),
        Stmt::Expr(expr) => source_map.node_for_expr(*expr),
    };
    ptr.map(|ptr| ptr.text_range().end())
}
