//! Where an error that carries only a source range traps when the code it
//! sits in runs: at the innermost expression or statement containing it, or
//! on the item's value when it lies outside the body.

use base_db::{Db, parse};
use syntax::ast::AstNode as _;
use syntax::{SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::body::{Body, BodySourceMap, ExprData, Stmt};
use crate::{
    ExprId, ItemId, PatId, Severity, body_with_source_map, item_source, range_diagnostics,
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

/// The trap site and message of every [`range_diagnostics`] error that lies
/// in `item`'s own syntax. An item with no body (a `type` or `trait`) has
/// none: its errors trap nowhere.
#[salsa::tracked(returns(ref))]
pub fn range_traps<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<(TrapSite, String)> {
    let file = item.file(db);
    let mut errors = range_diagnostics(db, file)
        .iter()
        .filter(|diag| diag.severity == Severity::Error)
        .peekable();
    if errors.peek().is_none() {
        return Vec::new();
    }
    let (body, source_map) = body_with_source_map(db, item);
    let Some(root) = body.root else {
        return Vec::new();
    };
    let Some(decl) = item_decl(db, item) else {
        return Vec::new();
    };
    let parse = parse(db, file);
    let tree = parse.syntax_node();
    errors
        .filter_map(|diag| {
            // A parse error belongs to the construct the parser was
            // building, which can end before the token it points at.
            let blame = parse
                .errors()
                .iter()
                .find(|err| err.range == diag.range && err.message == diag.message)
                .map_or(diag.range, |err| parse.blame(err));
            decl.text_range().contains_range(blame).then(|| {
                let at = diag.range.start();
                let site = site_for_range(&tree, &decl, body, source_map, root, blame, at);
                (site, diag.message.clone())
            })
        })
        .collect()
}

/// The trap site for a finding reported on one of `item`'s patterns: the
/// innermost construct containing the pattern.
pub fn trap_site_for_pat(db: &dyn Db, item: ItemId<'_>, pat: PatId) -> Option<TrapSite> {
    let (body, source_map) = body_with_source_map(db, item);
    let root = body.root?;
    let range = source_map.node_for_pat(pat)?.text_range();
    let decl = item_decl(db, item)?;
    let tree = parse(db, item.file(db)).syntax_node();
    Some(site_for_range(
        &tree,
        &decl,
        body,
        source_map,
        root,
        range,
        range.start(),
    ))
}

/// The syntax that declares `item`: the item, or the member.
fn item_decl(db: &dyn Db, item: ItemId<'_>) -> Option<SyntaxNode> {
    if item.member(db).is_some() {
        crate::item_tree::member_source(db, item).map(|member| member.syntax().clone())
    } else {
        item_source(db, item).map(|it| it.syntax().clone())
    }
}

/// The site for an error blamed on `range`: the innermost expression
/// containing it, or the statement of the innermost block that `at`, the
/// error's own position, falls in.
fn site_for_range(
    tree: &SyntaxNode,
    decl: &SyntaxNode,
    body: &Body,
    source_map: &BodySourceMap,
    root: ExprId,
    range: TextRange,
    at: TextSize,
) -> TrapSite {
    let element = tree.covering_element(range);
    let Some(covering) = element
        .clone()
        .into_node()
        .or_else(|| element.into_token()?.parent())
    else {
        return TrapSite::Expr(root);
    };
    let start = if decl.text_range().contains_range(covering.text_range()) {
        covering
    } else {
        decl.clone()
    };
    for node in start.ancestors() {
        if let Some(expr) = source_map.expr_for_node(SyntaxNodePtr::new(&node)) {
            if let ExprData::Block { stmts, .. } = &body.exprs[expr] {
                let index = stmts
                    .iter()
                    .position(|stmt| stmt_end(source_map, stmt).is_some_and(|end| end > at))
                    .unwrap_or(stmts.len());
                return TrapSite::Stmt { block: expr, index };
            }
            return TrapSite::Expr(headed(body, expr));
        }
        if &node == decl {
            break;
        }
    }
    TrapSite::Expr(root)
}

/// The outermost expression `expr` is the head of: the call it is the
/// callee of, or the path or turbofish it is the base of. A construction
/// lowers its head as part of itself, so the head's trap belongs to it.
fn headed(body: &Body, mut expr: ExprId) -> ExprId {
    while let Some((outer, _)) = body.exprs.iter().find(|(_, data)| match data {
        ExprData::Call { callee: head, .. }
        | ExprData::VariantPath { base: head, .. }
        | ExprData::GenericApp { base: head, .. } => *head == expr,
        _ => false,
    }) {
        expr = outer;
    }
    expr
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
