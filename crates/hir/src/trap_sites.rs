//! Where an error that carries only a source range traps when the code it
//! sits in runs: at the innermost expression or statement containing it, or
//! on the item's value when it lies outside the body. An error in a
//! declaration with no body traps wherever the declaration is named, and an
//! error outside every item traps every item's value.

use base_db::{Db, SourceFile, parse};
use rustc_hash::FxHashMap;
use syntax::ast::AstNode as _;
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::body::{Body, BodySourceMap, ExprData, Stmt};
use crate::{
    ExprId, ItemId, ItemLoc, PatId, Resolution, Severity, body_with_source_map, checkable_item_at,
    file_scope, item_loc, item_source, range_diagnostics, resolutions,
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
/// error that traps in `item`: those in its own syntax, those outside every item, and
/// those of each bodiless declaration it names, at the name. An item with no
/// body has none.
#[salsa::tracked(returns(ref))]
pub fn range_traps<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<(TrapSite, String)> {
    let file = item.file(db);
    let owners = error_owners(db, file);
    let loc = item_loc(db, item);
    let own = owners.bodies.iter().find(|(owner, _)| *owner == loc);
    if own.is_none() && owners.decls.is_empty() && owners.outside.is_empty() {
        return Vec::new();
    }
    let Some(sites) = Sites::new(db, item) else {
        return Vec::new();
    };
    let mut traps: Vec<(TrapSite, String)> = own
        .into_iter()
        .flat_map(|(_, errors)| errors)
        .map(|error| (sites.site(error.blame, error.at), error.message.clone()))
        .collect();
    traps.extend(
        owners
            .outside
            .iter()
            .map(|message| (TrapSite::Expr(sites.root), message.clone())),
    );
    if !owners.decls.is_empty() {
        for (loc, range) in declaration_mentions(db, item, &sites.decl, sites.source_map) {
            let Some((_, messages)) = owners.decls.iter().find(|(named, _)| *named == loc) else {
                continue;
            };
            let site = sites.site(range, range.start());
            traps.extend(messages.iter().map(|message| (site, message.clone())));
        }
    }
    traps
}

/// Every parse error and [`range_diagnostics`] error of a file, by the code
/// that owns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, salsa::Update)]
pub struct ErrorOwners {
    /// Errors inside an item or member that has a body, by that item.
    pub bodies: Vec<(ItemLoc, Vec<BodyError>)>,
    /// Errors in a top-level declaration with no body (a `type`, a
    /// `trait`, a `static` without a value), outside its members' bodies.
    pub decls: Vec<(ItemLoc, Vec<String>)>,
    /// Errors outside every item.
    pub outside: Vec<String>,
}

/// An error inside a body: the range it is blamed on, its own position, and
/// its message.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct BodyError {
    pub blame: TextRange,
    pub at: TextSize,
    pub message: String,
}

#[salsa::tracked(returns(ref))]
pub fn error_owners(db: &dyn Db, file: SourceFile) -> ErrorOwners {
    let parse = parse(db, file);
    // A parse error belongs to the construct the parser was building,
    // which can end before the token it points at.
    let parse_errors = (parse.errors().iter().enumerate())
        .map(|(index, err)| (parse.blame(index), err.range, &err.message));
    let range_errors = (range_diagnostics(db, file).iter())
        .filter(|diag| diag.severity == Severity::Error)
        .map(|diag| (diag.range, diag.range, &diag.message));
    let tree = parse.syntax_node();
    let mut owners = ErrorOwners::default();
    for (blame, range, message) in parse_errors.chain(range_errors) {
        let element = tree.covering_element(blame);
        let node = element
            .clone()
            .into_node()
            .or_else(|| element.into_token()?.parent());
        let Some(owner) = node.and_then(|node| checkable_item_at(db, file, &node)) else {
            owners.outside.push(message.clone());
            continue;
        };
        // An error in code with a body traps in that body. Code without a
        // body never runs, so its error traps wherever its top-level
        // declaration is named: a bodiless member's error belongs to the
        // type or trait it is declared in.
        if crate::body::body(db, owner).root.is_some() {
            let error = BodyError {
                blame,
                at: range.start(),
                message: message.clone(),
            };
            push_owned(&mut owners.bodies, item_loc(db, owner), error);
        } else {
            let decl = ItemLoc::top_level(
                file,
                owner.name(db).as_str().into(),
                owner.disambiguator(db),
            );
            push_owned(&mut owners.decls, decl, message.clone());
        }
    }
    owners
}

fn push_owned<T>(owned: &mut Vec<(ItemLoc, Vec<T>)>, loc: ItemLoc, error: T) {
    match owned.iter_mut().find(|(owner, _)| *owner == loc) {
        Some((_, errors)) => errors.push(error),
        None => owned.push((loc, vec![error])),
    }
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
                let name = syntax::ast::NameRef::cast(name_ref.clone())
                    .map_or_else(String::new, |it| it.text());
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
    // A member call reaches its owner and the trait it implements without
    // spelling either.
    let inference = crate::infer::infer(db, item);
    let members = inference
        .member_of_expr
        .iter()
        .chain(inference.qualified_member_of_expr.iter());
    for (call, member) in members {
        let Some(range) = source_map.node_for_expr(call).map(|ptr| ptr.text_range()) else {
            continue;
        };
        let owner = ItemLoc::top_level(member.file, member.name.clone(), member.disambiguator);
        // A trait-impl member is named `head::member`, `head` being the
        // impl's trait or implementer.
        let head = member
            .member
            .as_ref()
            .and_then(|(name, _)| name.split_once("::"))
            .map(|(head, _)| head);
        if let Some(Resolution::TraitItem(trait_)) = head.and_then(|head| scope.resolve(head)) {
            mentions.push((trait_, range));
        }
        mentions.push((owner, range));
    }
    for (call, bound) in inference.bound_member_of_expr.iter() {
        if let Some(ptr) = source_map.node_for_expr(call) {
            mentions.push((bound.trait_.clone(), ptr.text_range()));
        }
    }
    mentions
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
