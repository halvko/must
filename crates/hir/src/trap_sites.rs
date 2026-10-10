//! Where an error that carries only a source range traps when the code it
//! sits in runs: at the innermost expression or statement containing it, or
//! on the item's value when it lies outside the body. An error in a
//! declaration with no body traps wherever the declaration is named, and an
//! error outside every item traps every item's value.

use base_db::{Db, SourceFile, parse};
use syntax::ast::AstNode as _;
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::body::{Body, BodySourceMap, ExprData, Stmt};
use crate::{
    Diagnostic, ExprId, ItemId, ItemLoc, PatId, Resolution, Severity, body_with_source_map,
    file_item_ids, file_scope, item_loc, item_source, member_item_ids, range_diagnostics,
    resolutions,
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

/// The trap site and message of every [`range_diagnostics`] error that
/// traps in `item`: those in its own syntax, those outside every item, and
/// those of each bodiless declaration it names, at the name. An item with no
/// body has none.
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
    let owned = declaration_errors(db, file);
    let (body, source_map) = body_with_source_map(db, item);
    let Some(root) = body.root else {
        return Vec::new();
    };
    let Some(decl) = item_decl(db, item) else {
        return Vec::new();
    };
    let parse = parse(db, file);
    let tree = parse.syntax_node();
    let mut traps: Vec<(TrapSite, String)> = errors
        .filter_map(|diag| {
            let blame = blame(db, file, diag);
            decl.text_range().contains_range(blame).then(|| {
                let at = diag.range.start();
                let site = site_for_range(&tree, &decl, body, source_map, root, blame, at);
                (site, diag.message.clone())
            })
        })
        .collect();
    traps.extend(
        owned
            .outside
            .iter()
            .map(|message| (TrapSite::Expr(root), message.clone())),
    );
    if !owned.decls.is_empty() {
        for (loc, range) in declaration_mentions(db, item, &decl, source_map) {
            let Some((_, messages)) = owned.decls.iter().find(|(named, _)| *named == loc) else {
                continue;
            };
            let site = site_for_range(&tree, &decl, body, source_map, root, range, range.start());
            traps.extend(messages.iter().map(|message| (site, message.clone())));
        }
    }
    traps
}

/// The [`range_diagnostics`] errors that no body contains, by owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, salsa::Update)]
pub struct DeclarationErrors {
    /// Errors in a top-level declaration with no body (a `type`, a
    /// `trait`, a `static` without a value), outside its members' bodies.
    pub decls: Vec<(ItemLoc, Vec<String>)>,
    /// Errors outside every item.
    pub outside: Vec<String>,
}

#[salsa::tracked(returns(ref))]
pub fn declaration_errors(db: &dyn Db, file: SourceFile) -> DeclarationErrors {
    let mut owned = DeclarationErrors::default();
    let items: Vec<(ItemId<'_>, SyntaxNode)> = file_item_ids(db, file)
        .iter()
        .filter_map(|&item| Some((item, item_source(db, item)?.syntax().clone())))
        .collect();
    let errors = range_diagnostics(db, file)
        .iter()
        .filter(|diag| diag.severity == Severity::Error);
    for diag in errors {
        let blame = blame(db, file, diag);
        let Some(&(item, _)) = items
            .iter()
            .find(|(_, node)| node.text_range().contains_range(blame))
        else {
            owned.outside.push(diag.message.clone());
            continue;
        };
        if crate::body::body(db, item).root.is_some() {
            continue;
        }
        let in_member_body = member_item_ids(db, item).into_iter().any(|member| {
            crate::body::body(db, member).root.is_some()
                && item_decl(db, member).is_some_and(|node| node.text_range().contains_range(blame))
        });
        if in_member_body {
            continue;
        }
        let loc = item_loc(db, item);
        match owned.decls.iter_mut().find(|(decl, _)| *decl == loc) {
            Some((_, messages)) => messages.push(diag.message.clone()),
            None => owned.decls.push((loc, vec![diag.message.clone()])),
        }
    }
    owned
}

/// Whether any item with a body names the declaration at `loc`, so that its
/// errors trap somewhere.
pub fn declaration_is_named(db: &dyn Db, file: SourceFile, loc: &ItemLoc) -> bool {
    crate::all_checkable_items(db, file)
        .into_iter()
        .any(|item| {
            let (body, source_map) = body_with_source_map(db, item);
            body.root.is_some()
                && item_decl(db, item).is_some_and(|decl| {
                    declaration_mentions(db, item, &decl, source_map)
                        .iter()
                        .any(|(named, _)| named == loc)
                })
        })
}

/// Every place `item` names a top-level declaration, with the range of the
/// name: a path in an expression, a type, a bound or an impl head, or the
/// type a pattern names.
fn declaration_mentions(
    db: &dyn Db,
    item: ItemId<'_>,
    decl: &SyntaxNode,
    source_map: &BodySourceMap,
) -> Vec<(ItemLoc, TextRange)> {
    let file = item.file(db);
    let scope = file_scope(db, file);
    let resolved = resolutions(db, item);
    let mut mentions = Vec::new();
    for name_ref in decl
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::NAME_REF)
    {
        let Some(parent) = name_ref.parent() else {
            continue;
        };
        let is_base = parent
            .children()
            .find(|child| child.kind() == SyntaxKind::NAME_REF)
            .is_some_and(|first| first == name_ref);
        if !is_base {
            continue;
        }
        let resolution = match parent.kind() {
            SyntaxKind::PATH_EXPR => {
                let Some(expr) = source_map
                    .expr_for_node(SyntaxNodePtr::new(&name_ref))
                    .or_else(|| source_map.expr_for_node(SyntaxNodePtr::new(&parent)))
                else {
                    continue;
                };
                resolved.get(expr).cloned()
            }
            SyntaxKind::PATH_TYPE | SyntaxKind::VARIANT_PAT | SyntaxKind::NEWTYPE_PAT => {
                let name = name_ref.text().to_string();
                if binder_names(&name_ref).any(|param| param == name) {
                    continue;
                }
                scope.resolve(&name)
            }
            _ => continue,
        };
        let loc = match resolution {
            Some(
                Resolution::Item(loc)
                | Resolution::TypeItem(loc)
                | Resolution::TraitItem(loc)
                | Resolution::Ambiguous(loc),
            ) => loc,
            _ => continue,
        };
        mentions.push((loc, name_ref.text_range()));
    }
    mentions
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

/// The generic parameter names in scope at `node`.
fn binder_names(node: &SyntaxNode) -> impl Iterator<Item = String> {
    node.ancestors()
        .flat_map(|ancestor| ancestor.children())
        .filter(|child| child.kind() == SyntaxKind::GENERIC_PARAM_LIST)
        .flat_map(|list| list.children())
        .filter_map(|param| {
            param
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::IDENT)
                .map(|token| token.text().to_owned())
        })
}

/// The range an error blames: a parse error belongs to the construct the
/// parser was building, which can end before the token it points at.
fn blame(db: &dyn Db, file: SourceFile, diag: &Diagnostic) -> TextRange {
    let parse = parse(db, file);
    parse
        .errors()
        .iter()
        .find(|err| err.range == diag.range && err.message == diag.message)
        .map_or(diag.range, |err| parse.blame(err))
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
