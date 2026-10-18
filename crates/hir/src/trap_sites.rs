//! Where an error that carries only a source range traps: at the innermost
//! expression or statement containing it, or on the item's value. An error in
//! a bodiless declaration traps wherever it is named, one in a `use` or a
//! clash wherever its file names the name, and one outside every item on
//! every item of its file and wherever another file names one of its items.

use std::cell::OnceCell;

use base_db::{Db, SourceFile, parse};
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;
use syntax::ast::AstNode as _;
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::body::{Body, BodySourceMap, ExprData, Stmt};
use crate::library::{LibraryId, library_of};
use crate::scopes::NameOrigin;
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

/// The trap site and message of every error that traps in `item`: those in
/// its own syntax and outside every item of its file; at the name, those of
/// each bodiless declaration it names and outside every item of that
/// declaration's file; and its file's named errors, at each mention. An item
/// with no body has none.
#[salsa::tracked(returns(ref))]
pub fn range_traps<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Vec<(TrapSite, String)> {
    let file = item.file(db);
    let library_errors = library_errors(db, library_of(db, file));
    let loc = item_loc(db, item);
    let owners = error_owners(db, file);
    let own = owners.bodies.iter().find(|(owner, _)| *owner == loc);
    if own.is_none()
        && library_errors.decls.is_empty()
        && library_errors.outside.is_empty()
        && owners.named.is_empty()
    {
        return Vec::new();
    }
    let Some(sites) = Sites::new(db, item) else {
        return Vec::new();
    };
    let mut traps: Vec<(TrapSite, String)> = Vec::new();
    let mentions = if !library_errors.decls.is_empty()
        || !library_errors.outside.is_empty()
        || !owners.named.is_empty()
    {
        declaration_mentions(db, item, &sites.decl, sites.source_map)
    } else {
        Default::default()
    };
    // Ahead of the item's own errors: a name a broken `use` failed to bring
    // in is also an unresolved name at the mention, and the `use` says why.
    for (name, range, ambiguous) in &mentions.names {
        let site = sites.site(*range, range.start());
        traps.extend(
            (owners.named.iter())
                .filter(|named| named.names.contains(name) && !(named.clash && *ambiguous))
                .map(|named| (site, named.message.clone())),
        );
    }
    traps.extend(
        own.into_iter()
            .flat_map(|(_, errors)| errors)
            .map(|error| (sites.site(error.blame, error.at), error.message.clone())),
    );
    traps.extend(
        (owners.outside.iter()).map(|message| (TrapSite::Expr(sites.root), message.clone())),
    );
    for (loc, range) in &mentions.decls {
        let site = sites.site(*range, range.start());
        if let Some(messages) = library_errors.decls.get(loc) {
            traps.extend(messages.iter().map(|message| (site, message.clone())));
        }
        if loc.file != file
            && let Some(messages) = library_errors.outside.get(&loc.file)
        {
            traps.extend(messages.iter().map(|message| (site, message.clone())));
        }
    }
    // One error can reach a site more than one way (two declarations of
    // one file); it traps once.
    let mut seen = rustc_hash::FxHashSet::default();
    traps.retain(|trap| seen.insert(trap.clone()));
    traps
}

/// The errors of a library that trap in another file than their own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryErrors {
    /// The errors of each bodiless declaration, by the declaration.
    pub decls: FxHashMap<ItemLoc, Vec<String>>,
    /// The errors outside every item, by file.
    pub outside: FxHashMap<SourceFile, Vec<String>>,
}

/// The [`LibraryErrors`] of `library`, merged over its files.
#[salsa::tracked(returns(ref))]
pub fn library_errors<'db>(db: &'db dyn Db, library: LibraryId<'db>) -> LibraryErrors {
    let mut errors = LibraryErrors::default();
    for member in library.files(db) {
        let owners = error_owners(db, member);
        errors.decls.extend(owners.decls.iter().cloned());
        if !owners.outside.is_empty() {
            errors.outside.insert(member, owners.outside.clone());
        }
    }
    errors
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
    /// Errors that trap where the file names a name: one per clash, and
    /// those in a `use`.
    pub named: Vec<NamedError>,
    /// Errors outside every item, and in a `use` the parse left no names.
    pub outside: Vec<String>,
}

/// An error in a `use`, or a clash. It traps wherever the file names one of
/// `names`: what the `use` imports (only the failing name, when the error is
/// in one), or the clashing name.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct NamedError {
    pub names: Vec<String>,
    pub message: String,
    /// A clash. Not applied at an ambiguous expression, which traps by
    /// itself.
    pub clash: bool,
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
    let scope = file_scope(db, file);
    let mut owners = ErrorOwners::default();
    // A clash is about this file's namespace, not any one place the name
    // comes from: its error traps where this file names it, once, and never
    // in a file importing one of the items. Each place reports it, at a
    // `use` or at an item's name, and those reports are skipped below.
    let clash_messages: Vec<(String, &crate::scopes::NameClash)> = (scope.clashes.iter())
        .map(|clash| (crate::diag::defined_multiple_times(&clash.name), clash))
        .collect();
    for (message, clash) in &clash_messages {
        owners.named.push(NamedError {
            names: vec![clash.name.clone()],
            message: message.clone(),
            clash: true,
        });
    }
    // At a `use` (`None`) or at the item `at`.
    let is_clash_report = |message: &str, at: Option<&ItemLoc>| {
        (clash_messages.iter()).any(|(clash_message, clash)| {
            clash_message == message
                && (clash.origins.iter()).any(|origin| match (origin, at) {
                    (NameOrigin::Import(_), None) => true,
                    (NameOrigin::Item(item), Some(at)) => item == at,
                    _ => false,
                })
        })
    };
    for (blame, range, message) in parse_errors.chain(range_errors) {
        let element = tree.covering_element(blame);
        let node = element
            .clone()
            .into_node()
            .or_else(|| element.into_token()?.parent());
        if let Some(use_item) = node
            .as_ref()
            .and_then(|node| node.ancestors().find_map(syntax::ast::UseItem::cast))
        {
            if is_clash_report(message, None) {
                continue;
            }
            let names = use_item
                .names()
                .filter(|name| name.syntax().text_range().contains_range(blame))
                .collect::<Vec<_>>();
            let names = if names.is_empty() {
                use_item.names().collect()
            } else {
                names
            };
            let names: Vec<String> = names.iter().filter_map(imported_as).collect();
            // Nothing can name what a `use` with no names imports; the error
            // traps as one outside every item does.
            if names.is_empty() {
                owners.outside.push(message.clone());
            } else {
                owners.named.push(NamedError {
                    names,
                    message: message.clone(),
                    clash: false,
                });
            }
            continue;
        }
        let Some(owner) = node.and_then(|node| checkable_item_at(db, file, &node)) else {
            owners.outside.push(message.clone());
            continue;
        };
        let loc = item_loc(db, owner);
        if is_clash_report(message, Some(&loc)) {
            continue;
        }
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
            push_owned(&mut owners.bodies, loc, error);
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

/// The name a `use` gives the file for `name`: its rename, or the name.
fn imported_as(name: &syntax::ast::UseName) -> Option<String> {
    match name.rename() {
        Some(rename) => Some(rename.text()),
        None => Some(name.name_ref()?.text()),
    }
}

fn push_owned<T>(owned: &mut Vec<(ItemLoc, Vec<T>)>, loc: ItemLoc, error: T) {
    match owned.iter_mut().find(|(owner, _)| *owner == loc) {
        Some((_, errors)) => errors.push(error),
        None => owned.push((loc, vec![error])),
    }
}

/// Where an item names things, each with the range of the name.
#[derive(Default)]
struct Mentions {
    /// Every top-level declaration the item names: a path in an expression,
    /// a type, a bound or an impl head, the type a pattern names, or what a
    /// member call reaches.
    decls: Vec<(ItemLoc, TextRange)>,
    /// Every name the item reads from its file's top level, resolved or not,
    /// and whether it is an ambiguous expression, which traps by itself.
    names: Vec<(String, TextRange, bool)>,
}

fn declaration_mentions(
    db: &dyn Db,
    item: ItemId<'_>,
    decl: &SyntaxNode,
    source_map: &BodySourceMap,
) -> Mentions {
    let file = item.file(db);
    let scope = file_scope(db, file);
    let resolved = resolutions(db, item);
    let mut mentions = Mentions::default();
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
        let name =
            syntax::ast::NameRef::cast(name_ref.clone()).map_or_else(String::new, |it| it.text());
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
                if binder_names(&name_ref).any(|param| param == name) {
                    continue;
                }
                scope.resolve(&name)
            }
            _ => continue,
        };
        let range = name_ref.text_range();
        // An ambiguous name in an expression lowers to its own trap, so a
        // clash is not added again there.
        let ambiguous = parent.kind() == SyntaxKind::PATH_EXPR
            && matches!(resolution, Some(Resolution::Ambiguous(_)));
        let loc = match resolution {
            Some(
                Resolution::Item(loc)
                | Resolution::TypeItem(loc)
                | Resolution::TraitItem(loc)
                | Resolution::Ambiguous(loc),
            ) => loc,
            None => {
                mentions.names.push((name, range, false));
                continue;
            }
            _ => continue,
        };
        mentions.names.push((name, range, ambiguous));
        mentions.decls.push((loc, range));
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
        // impl's trait or implementer, as the impl's own file reads it.
        let head = member
            .member
            .as_ref()
            .and_then(|(name, _)| name.split_once("::"))
            .map(|(head, _)| head);
        let impl_scope = file_scope(db, member.file);
        if let Some(Resolution::TraitItem(trait_)) = head.and_then(|head| impl_scope.resolve(head))
        {
            mentions.decls.push((trait_, range));
        }
        mentions.decls.push((owner, range));
    }
    for (call, bound) in inference.bound_member_of_expr.iter() {
        if let Some(ptr) = source_map.node_for_expr(call) {
            mentions
                .decls
                .push((bound.trait_.clone(), ptr.text_range()));
        }
    }
    // A use of a member its impl lacks traps with the impl's errors.
    for (call, owner) in inference.incomplete_impl_of_expr.iter() {
        if let Some(ptr) = source_map.node_for_expr(call) {
            mentions.decls.push((owner.clone(), ptr.text_range()));
        }
    }
    // A bound resolved at an instantiation reaches the implementer's impl
    // without spelling it; a broken one traps there with its own errors.
    for (key, entries) in inference.bound_dicts_of_expr.iter() {
        let Some(range) = source_map.node_for_expr(key).map(|ptr| ptr.text_range()) else {
            continue;
        };
        for entry in entries {
            if let crate::infer::DictEntry::BrokenImpl { owner } = entry {
                mentions.decls.push((owner.clone(), range));
            }
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
            let range = sites.pat_node(pat)?.text_range();
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
    /// Each pattern's node, built when first asked.
    pats: OnceCell<ArenaMap<PatId, SyntaxNodePtr>>,
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
            pats: OnceCell::new(),
        })
    }

    fn pat_node(&self, pat: PatId) -> Option<SyntaxNodePtr> {
        self.pats
            .get_or_init(|| self.source_map.pat_nodes())
            .get(pat)
            .copied()
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
                        .position(|stmt| self.stmt_end(stmt).is_some_and(|end| end > at))
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

    /// Where a statement's last lowered part ends.
    fn stmt_end(&self, stmt: &Stmt) -> Option<TextSize> {
        let ptr = match stmt {
            Stmt::Let { pat, init, .. } => init
                .and_then(|init| self.source_map.node_for_expr(init))
                .or_else(|| self.pat_node(*pat)),
            Stmt::Assign { value, .. } => self.source_map.node_for_expr(*value),
            Stmt::Expr(expr) => self.source_map.node_for_expr(*expr),
        };
        ptr.map(|ptr| ptr.text_range().end())
    }
}
