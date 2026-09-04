//! Semantic layer: item tree, body lowering, name resolution, type inference.
//!
//! Query layering (the incrementality firewall): everything range-carrying
//! is split from everything range-free, and items see each other only
//! through range-free, value-comparable data. An edit inside one body can
//! therefore only reach other items if a *value* on that path changes.

pub mod body;
pub mod const_check;
pub mod constraint;
pub mod diag;
pub mod groups;
pub mod infer;
pub mod item_tree;
pub mod scopes;
pub mod ty;
pub mod unsafe_check;

#[cfg(test)]
mod tests;

use base_db::{Db, SourceFile, parse};
use syntax::TextRange;
use syntax::ast::{self, AstNode as _};

pub use body::{BindingId, Body, BodySourceMap, ExprId, PatId, body_with_source_map};
pub use const_check::ConstCheckDiagnostic;
pub use constraint::Cause;
pub use infer::{InferenceDiagnostic, InferenceResult};
pub use item_tree::{Constness, ItemKind, ItemTree, TypeDeclData, TypeRef, item_source, type_decl};
pub use scopes::{
    ALLOC_RESULT_NAME, BUILTIN_DISAMBIGUATOR, Builtin, Duplicate, ExprScopes, FileScope,
    Resolution, TypeScope, alloc_result_loc, expr_scopes, file_scope, resolutions, type_scope,
};
pub use ty::{
    ConstArgValue, FnTy, GenericArg, IntKind, IntValue, NamedTy, Ty, VariantTy, enum_variants,
    member_is_dot_callable, member_self_ty, signature, substitute_args, type_underlying,
    type_underlying_for, variant_payloads_for, widens_to,
};
pub use unsafe_check::UnsafeCheckDiagnostic;

/// Stable identity of a top-level item — or of one MEMBER of a type
/// item's attachment chain (`impl Self { ... }`), when [`Self::member`] is
/// set: survives edits to other items, reordering of unrelated code, and
/// any edit inside its own body.
///
/// Used as a *query key*. Inside query *values* use [`ItemLoc`], which is
/// lifetime-free (salsa values holding `'db` ids would need `salsa::Update`).
#[salsa::interned(debug)]
pub struct ItemId<'db> {
    pub file: SourceFile,
    #[returns(ref)]
    pub name: String,
    /// Which occurrence of `name` in the file (0-based); keeps duplicates
    /// and unnamed (broken) items distinct.
    pub disambiguator: u32,
    /// `Some((member name, member disambiguator))` for a member of the
    /// item's `with`-chain (`impl Self { ... }`) — NAME-KEYED, never
    /// positional, so editing a sibling member never changes this
    /// identity. `None` for the item itself.
    pub member: Option<(String, u32)>,
}

/// Lifetime-free reference to an item, carrying the same identity as
/// [`ItemId`] (name + disambiguator, *not* a positional index): inserting an
/// unrelated item above doesn't change any `ItemLoc`, so query values that
/// embed one — resolutions, MIR constants, eval origins — backdate across
/// item reordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemLoc {
    pub file: SourceFile,
    pub name: std::sync::Arc<str>,
    pub disambiguator: u32,
    /// The member half of a member identity (see [`ItemId::member`]);
    /// `None` for top-level items.
    pub member: Option<(std::sync::Arc<str>, u32)>,
}

impl ItemLoc {
    /// A top-level (non-member) location.
    pub fn top_level(file: SourceFile, name: std::sync::Arc<str>, disambiguator: u32) -> ItemLoc {
        ItemLoc {
            file,
            name,
            disambiguator,
            member: None,
        }
    }

    /// One inherent member of `owner`, name-keyed. The only way to build a
    /// member location: the owner half is copied wholesale, so a caller can
    /// never pair one owner's file with another's name.
    pub fn member(owner: &ItemLoc, name: &str, disambiguator: u32) -> ItemLoc {
        ItemLoc {
            file: owner.file,
            name: owner.name.clone(),
            disambiguator: owner.disambiguator,
            member: Some((std::sync::Arc::from(name), disambiguator)),
        }
    }

    /// Always succeeds (interning): an `ItemLoc` held across an edit that
    /// deleted the item yields an id whose queries all answer the empty/
    /// error case — total, never a panic.
    pub fn to_id<'db>(&self, db: &'db dyn Db) -> ItemId<'db> {
        ItemId::new(
            db,
            self.file,
            self.name.to_string(),
            self.disambiguator,
            self.member
                .as_ref()
                .map(|(name, dis)| (name.to_string(), *dis)),
        )
    }

    /// The name for messages — a member's own name for member locations;
    /// unnamed (broken) items render as `?`.
    pub fn display_name(&self) -> &str {
        if let Some((member, _)) = &self.member {
            return member;
        }
        if self.name.is_empty() {
            "?"
        } else {
            &self.name
        }
    }
}

pub fn item_loc(db: &dyn Db, item: ItemId<'_>) -> ItemLoc {
    ItemLoc {
        file: item.file(db),
        name: std::sync::Arc::from(item.name(db).as_str()),
        disambiguator: item.disambiguator(db),
        member: item
            .member(db)
            .map(|(name, dis)| (std::sync::Arc::from(name.as_str()), dis)),
    }
}

/// The owning type item of a member id (`None` for top-level items).
pub fn member_owner<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<ItemId<'db>> {
    item.member(db)?;
    Some(ItemId::new(
        db,
        item.file(db),
        item.name(db).clone(),
        item.disambiguator(db),
        None,
    ))
}

/// The checkable unit owning `node`: the enclosing top-level item — or the
/// enclosing MEMBER id when `node` sits inside an `impl Self` member of a
/// type item's `with`-chain (member bodies are checkable units of their
/// own). The IDE layer's one way from syntax to the id whose
/// body/inference queries know about the node.
pub fn checkable_item_at<'db>(
    db: &'db dyn Db,
    file: SourceFile,
    node: &syntax::SyntaxNode,
) -> Option<ItemId<'db>> {
    let item_node = node.ancestors().find(|n| ast::Item::can_cast(n.kind()))?;
    let root = parse(db, file).syntax_node();
    let index = root
        .children()
        .filter(|n| ast::Item::can_cast(n.kind()))
        .position(|n| n == item_node)?;
    let item = *file_item_ids(db, file).get(index)?;
    if let Some(member_node) = node.ancestors().find_map(ast::Member::cast)
        && let Some(ast::Item::TypeItem(decl)) = item_source(db, item)
        && let Some((name, dis, _)) = item_tree::semantic_member_sources(&decl)
            .into_iter()
            .find(|(_, _, m)| m.syntax() == member_node.syntax())
    {
        return Some(ItemId::new(
            db,
            file,
            item.name(db).clone(),
            item.disambiguator(db),
            Some((name, dis)),
        ));
    }
    Some(item)
}

/// The item's position in its file (= its index in [`item_tree`]). `None`
/// for a stale `ItemId` held across an edit that removed the item.
pub fn item_index(db: &dyn Db, item: ItemId<'_>) -> Option<usize> {
    file_item_ids(db, item.file(db))
        .iter()
        .position(|&it| it == item)
}

/// Whether `item` is the compiler-provided [`ALLOC_RESULT_NAME`] enum —
/// the one declaration that exists without source (see
/// [`scopes::alloc_result_loc`]). Its item-tree-level queries ([`item_data`],
/// [`type_decl`]) answer the builtin shape; source-level queries
/// ([`item_source`], [`item_index`]) answer the empty case, exactly like a
/// stale id.
pub fn is_alloc_result_decl(db: &dyn Db, item: ItemId<'_>) -> bool {
    item.disambiguator(db) == BUILTIN_DISAMBIGUATOR && item.name(db) == ALLOC_RESULT_NAME
}

/// The item-tree entry for `item` (its contract, constness, name).
/// Tracked so that consumers (`signature`, `infer`) depend on this item's
/// *entry* rather than on the whole positional item list — inserting an
/// unrelated item above re-executes only this cheap lookup, and its
/// unchanged value backdates everything downstream.
#[salsa::tracked(returns(ref))]
pub fn item_data<'db>(db: &'db dyn Db, item: ItemId<'db>) -> Option<item_tree::ItemData> {
    // A MEMBER of a type item's `with`-chain: name-level facts synthesized
    // from the range-free [`item_tree::type_members`] — its signature is
    // its fn literal's annotations (like a generic item's scheme), and the
    // OWNER's binder is its binder (the type's params flow into member
    // signatures and bodies).
    if let Some((member_name, member_dis)) = item.member(db) {
        let owner = member_owner(db, item)?;
        let data = item_tree::type_members(db, owner)
            .iter()
            .find(|m| m.name == member_name && m.disambiguator == member_dis)?;
        let generics = item_data(db, owner)
            .as_ref()
            .map(|owner_data| owner_data.generics.clone())
            .unwrap_or_default();
        return Some(item_tree::ItemData {
            name: data.name.clone(),
            kind: item_tree::ItemKind::Member,
            type_ref: data.type_ref.clone(),
            generics,
        });
    }
    if is_alloc_result_decl(db, item) {
        return Some(item_tree::ItemData {
            name: ALLOC_RESULT_NAME.to_owned(),
            kind: item_tree::ItemKind::Type,
            type_ref: None,
            generics: vec![item_tree::GenericParamData {
                name: "T".to_owned(),
                kind: item_tree::GenericParamKind::Type,
            }],
        });
    }
    item_tree::item_tree(db, item.file(db))
        .items
        .get(item_index(db, item)?)
        .cloned()
}

/// The member ids of a type item's `with`-chain, in source order — the
/// checkable-units extension of [`file_item_ids`] (members are inference/
/// MIR/eval units of their own, keyed like items). Empty for non-type
/// items.
pub fn member_item_ids<'db>(db: &'db dyn Db, owner: ItemId<'db>) -> Vec<ItemId<'db>> {
    item_tree::type_members(db, owner)
        .iter()
        .map(|m| {
            ItemId::new(
                db,
                owner.file(db),
                owner.name(db).clone(),
                owner.disambiguator(db),
                Some((m.name.clone(), m.disambiguator)),
            )
        })
        .collect()
}

/// Every unit of `file` that checks like an item: the top-level items plus
/// every type item's inherent members. The diagnostics aggregator (and
/// anything else that wants "all bodies") iterates this; positional
/// consumers (groups, indexes) keep using [`file_item_ids`].
pub fn all_checkable_items<'db>(db: &'db dyn Db, file: SourceFile) -> Vec<ItemId<'db>> {
    let mut items = Vec::new();
    for &item in file_item_ids(db, file) {
        items.push(item);
        items.extend(member_item_ids(db, item));
    }
    items
}

#[salsa::tracked(returns(ref))]
pub fn file_item_ids<'db>(db: &'db dyn Db, file: SourceFile) -> Vec<ItemId<'db>> {
    let tree = item_tree::item_tree(db, file);
    let mut seen: rustc_hash::FxHashMap<&str, u32> = rustc_hash::FxHashMap::default();
    tree.items
        .iter()
        .map(|item| {
            let disambiguator = seen.entry(item.name.as_str()).or_insert(0);
            let id = ItemId::new(db, file, item.name.clone(), *disambiguator, None);
            *disambiguator += 1;
            id
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: Severity,
    pub message: String,
    pub fix: Option<syntax::Fix>,
    /// Other locations that explain this diagnostic (e.g. "first defined
    /// here" on a duplicate definition). Each carries its own file, which is
    /// not necessarily the one diagnosed (see [`RelatedInfo::file`]).
    pub related: Vec<RelatedInfo>,
}

/// Severity of a [`Diagnostic`]. `ide` maps this onto its own richer
/// `Severity` (which additionally has `Info`, synthesized only at the ide
/// layer for related-location companions — hir never produces those).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedInfo {
    /// The file `range` lives in — *not* necessarily the diagnosed file
    /// (e.g. "first defined here" will cross files once imports exist).
    /// Consumers must resolve positions through this file's own line index.
    pub file: SourceFile,
    pub range: TextRange,
    pub message: String,
}

/// All semantic diagnostics for a file. This is the one place that converts
/// range-free facts back into text ranges (via the source maps).
pub fn file_diagnostics(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = parse(db, file)
        .errors()
        .iter()
        .map(|err| Diagnostic {
            range: err.range,
            severity: Severity::Error,
            message: err.message.clone(),
            fix: err.fix.clone(),
            related: Vec::new(),
        })
        .collect();

    let item_name = |loc: &ItemLoc| item_source(db, loc.to_id(db)).and_then(|it| it.name());

    // Duplicate definitions, discovered by `file_scope` (the analysis that
    // decides first-wins also knows about the losers); only the range
    // attachment happens here.
    for dup in &file_scope(db, file).duplicates {
        let Some(second) = item_name(&dup.second) else {
            continue;
        };
        let related = item_name(&dup.first)
            .map(|first| {
                vec![RelatedInfo {
                    file: dup.first.file,
                    range: first.syntax().text_range(),
                    message: "first defined here".to_owned(),
                }]
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: second.syntax().text_range(),
            severity: Severity::Error,
            message: diag::defined_multiple_times(&second.text()),
            fix: None,
            related,
        });
    }

    // Bad type names, in any annotation position: unknown, naming a value
    // item, a `::` path that names no variant, or a generic mention whose
    // turbofish is wrong (arity, kinds, unrepresentable const args).
    // Without this, a typo'd type lowers to a silent `{error}` — this pass
    // is the diagnostic MIRROR of `ty`'s annotation lowering, which stays
    // purely syntactic (const eval never runs there) and silent. Type
    // params of an enclosing generic binder are real type names here (they
    // lower to rigid `Ty::Param`s), EXCEPT inside the binder itself: a
    // const param's declared type naming a type param is a dependent
    // param, deferred (TR06) — it lowers to a silent `{error}` with the
    // diagnostic below.
    for path_type in parse(db, file)
        .syntax_node()
        .descendants()
        .filter_map(ast::PathType::cast)
    {
        let Some(name_ref) = path_type.name_ref() else {
            continue;
        };
        let name = name_ref.text();
        // Inside a `with`-chain, only the one semantically supported context
        // (an `impl Self` member of a plain group) is judged — everything else
        // is parse-and-reserve territory whose single reservation
        // diagnostic already tells the story.
        if in_reserved_with_region(path_type.syntax()) {
            continue;
        }
        // A PathType sitting in a CONST-argument position of an enclosing
        // turbofish (`Buf::<N>`'s `N` parses as a type arg) is judged by
        // the owner's argument checks — inference for expression-position
        // mentions, [`apply_position_diagnostics`] for annotations — never
        // as a type of its own.
        if in_const_arg_position(db, file, &path_type) {
            continue;
        }
        let binder = enclosing_binder_info(path_type.syntax());
        if path_type.generic_arg_list().is_some() || binder.names_const_param(&name) {
            // A turbofish (or a const-param name in plain type position):
            // the generic-mention checks own the whole judgement here — a
            // binder param with args, a non-generic target with args, and
            // every per-argument problem.
            apply_position_diagnostics(
                db,
                file,
                path_type.syntax(),
                &name,
                path_type.generic_arg_list().as_ref(),
                &binder,
                &mut diagnostics,
            );
            continue;
        }
        let message = match type_param_binding(&path_type, &name) {
            // In scope and rigid: a use is fine, but a type param has no
            // variants to name through `::`.
            TypeParamBinding::Bound => path_type
                .variant_name_ref()
                .map(|_| format!("`{name}` has no variants (it is a type parameter)")),
            TypeParamBinding::InOwnBinder => {
                Some("a const parameter's type cannot mention a type parameter".to_owned())
            }
            TypeParamBinding::NotBound => match path_type.variant_name_ref() {
                Some(variant) => variant_position_error(db, file, &name, &variant.text()),
                None => type_position_error(db, file, &name),
            },
        };
        if let Some(message) = message {
            diagnostics.push(Diagnostic {
                range: path_type.syntax().text_range(),
                severity: Severity::Error,
                message,
                fix: None,
                related: Vec::new(),
            });
            continue;
        }
        // A GENERIC type item mentioned bare: annotation lowering is
        // syntactic, so the arity must always be spelled in type position
        // (`Pair::<usize>`, `_` holes allowed where inference can fill
        // them).
        if path_type.variant_name_ref().is_none()
            && !matches!(
                type_param_binding(&path_type, &name),
                TypeParamBinding::Bound | TypeParamBinding::InOwnBinder
            )
            && let Some(Resolution::TypeItem(loc)) = type_scope(db, file).resolve(&name)
        {
            let arity = decl_generics_len(db, &loc);
            if arity > 0 {
                diagnostics.push(Diagnostic {
                    range: path_type.syntax().text_range(),
                    severity: Severity::Error,
                    message: diag::generic_arg_count(&name, arity, 0),
                    fix: None,
                    related: declared_here(db, &loc),
                });
            }
        }
    }

    // Array-type LENGTHS in annotation position: the diagnostic MIRROR of
    // `ty`'s array lowering, which reads the length off the syntax
    // (eval-free, like every const arg) and stays silent about anything it
    // can't represent. Same judgement as a turbofish's const argument
    // against a `usize`-declared param: literals type-check by literal
    // kind, a bare name must be an in-scope `usize` const param, and a
    // `const { ... }` block is outside the annotation domain entirely.
    for array_type in parse(db, file)
        .syntax_node()
        .descendants()
        .filter_map(ast::ArrayType::cast)
    {
        let Some(len) = array_type.len() else {
            // No length at all: the parse error covers it.
            continue;
        };
        if in_reserved_with_region(array_type.syntax()) {
            continue;
        }
        let binder = enclosing_binder_info(array_type.syntax());
        if let Some(message) = array_len_annotation_error(db, file, &len, &binder) {
            diagnostics.push(Diagnostic {
                range: len.syntax().text_range(),
                severity: Severity::Error,
                message,
                fix: None,
                related: Vec::new(),
            });
        }
    }

    // One binder, one name per parameter. Type and const params share the
    // binder's namespace: a rigid `Ty::Param` is positional and a const
    // param's mention resolves to the LAST declaration of the name, so a
    // repeat leaves the earlier parameter unnameable rather than
    // ambiguous. Reported at the second occurrence, pointing at the first.
    // Per LIST, not per item: two binders are two namespaces, whoever owns
    // them.
    for list in parse(db, file)
        .syntax_node()
        .descendants()
        .filter_map(ast::GenericParamList::cast)
    {
        let mut seen: Vec<(String, TextRange)> = Vec::new();
        for param in list.params() {
            let name = match &param {
                ast::GenericParam::TypeParam(it) => it.name(),
                ast::GenericParam::ConstParam(it) => it.name(),
            };
            let Some(name) = name else {
                continue;
            };
            let text = name.text();
            if text.is_empty() {
                continue;
            }
            match seen.iter().find(|(seen, _)| *seen == text) {
                Some(&(_, first)) => diagnostics.push(Diagnostic {
                    range: name.syntax().text_range(),
                    severity: Severity::Error,
                    message: format!("duplicate generic parameter `{text}`"),
                    fix: None,
                    related: vec![RelatedInfo {
                        file,
                        range: first,
                        message: "first declared here".to_owned(),
                    }],
                }),
                None => seen.push((text, name.syntax().text_range())),
            }
        }
    }

    // The item-level generic rule (TR06): a generic fn literal's binder
    // signature IS the item's contract, so every param and the return type
    // must be written — the fact is range-free (`generics` non-empty,
    // synthesized `type_ref` absent), only the range attaches here. Const-
    // param declared types get the enum-payload treatment: real type
    // syntax, but nothing to infer a hole from.
    for &item in file_item_ids(db, file) {
        let Some(data) = item_data(db, item).as_ref() else {
            continue;
        };
        if data.generics.is_empty() {
            continue;
        }
        let fn_literal =
            item_source(db, item)
                .and_then(|it| it.body())
                .and_then(|body| match body {
                    ast::Expr::FnLiteral(fn_lit) => Some(fn_lit),
                    _ => None,
                });
        let Some(fn_literal) = fn_literal else {
            continue;
        };
        if data.type_ref.is_none() {
            let range = fn_literal
                .generic_param_list()
                .map(|list| list.syntax().text_range())
                .unwrap_or_else(|| fn_literal.syntax().text_range());
            diagnostics.push(Diagnostic {
                range,
                severity: Severity::Error,
                message: diag::GENERIC_FN_NEEDS_FULL_ANNOTATION.to_owned(),
                fix: None,
                related: Vec::new(),
            });
        }
        for param in fn_literal
            .generic_param_list()
            .into_iter()
            .flat_map(|list| list.params())
        {
            let ast::GenericParam::ConstParam(const_param) = param else {
                continue;
            };
            let Some(ty) = const_param.ty() else {
                // No declared type at all: the parse error covers it.
                continue;
            };
            let type_ref = TypeRef::from_ast(ty.clone());
            // Fn values are outside the const-arg domain (TR06: concrete
            // data types only): their identity is a `BodyId` arena index,
            // which renumbers under body edits — instance identity built
            // on one would churn.
            // Rejected here at the source (the declaration); inference
            // repeats the same text at any mention that would pass one.
            if type_ref.mentions_fn() {
                diagnostics.push(Diagnostic {
                    range: ty.syntax().text_range(),
                    severity: Severity::Error,
                    message: diag::FN_CONST_ARG.to_owned(),
                    fix: None,
                    related: Vec::new(),
                });
                continue;
            }
            // Array values stay outside the const-arg domain too (the
            // ruled domain is builtins + records + variants) — same
            // declaration-site rejection, same belt at mentions.
            if type_ref.mentions_array() {
                diagnostics.push(Diagnostic {
                    range: ty.syntax().text_range(),
                    severity: Severity::Error,
                    message: diag::ARRAY_CONST_ARG.to_owned(),
                    fix: None,
                    related: Vec::new(),
                });
                continue;
            }
            if type_ref.is_fully_typed() {
                continue;
            }
            diagnostics.push(Diagnostic {
                range: ty.syntax().text_range(),
                severity: Severity::Error,
                message: "a const parameter's type must be a fully written type; \
                          a declaration has nothing to infer `_` from"
                    .to_owned(),
                fix: None,
                related: Vec::new(),
            });
        }
    }

    // `type` declarations: the RHS must be a `struct` or `enum` literal
    // whose field values / variant payloads are types. `type_decl` reads
    // the same shape syntactically; every `TypeRef::Error` (and every
    // erased inference variable) it can produce has a diagnostic from here.
    for &item in file_item_ids(db, file) {
        let Some(ast::Item::TypeItem(decl)) = item_source(db, item) else {
            continue;
        };
        let Some(rhs) = decl.body() else {
            // No RHS at all: the parse errors cover it.
            continue;
        };
        match rhs {
            ast::Expr::RecordExpr(record) => {
                type_decl_field_diagnostics(&record, &mut diagnostics);
            }
            ast::Expr::EnumExpr(en) => {
                enum_decl_payload_diagnostics(&en, &mut diagnostics);
            }
            other => diagnostics.push(Diagnostic {
                range: other.syntax().text_range(),
                severity: Severity::Error,
                message: "only a `struct` or `enum` literal can declare a type".to_owned(),
                fix: None,
                related: Vec::new(),
            }),
        }
    }

    // Inherent members: definition-site rules that live on the range-free
    // member facts, with only the range attached here.
    member_definition_diagnostics(db, file, &mut diagnostics);

    let syntax_root = parse(db, file).syntax_node();
    for item in all_checkable_items(db, file) {
        let (body, source_map) = body_with_source_map(db, item);
        let resolutions = resolutions(db, item);
        for (expr, data) in body.exprs.iter() {
            let message = match data {
                body::ExprData::NameRef(name) if resolutions.get(expr).is_none() => {
                    diag::unresolved_name(name)
                }
                // Without this, an overflowing literal would be a value MIR
                // can only trap on with no diagnostic to borrow.
                body::ExprData::Literal(body::LiteralData::Int(None)) => {
                    diag::INT_LITERAL_TOO_LARGE.to_owned()
                }
                _ => continue,
            };
            let Some(ptr) = source_map.node_for_expr(expr) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                severity: Severity::Error,
                message,
                fix: None,
                related: Vec::new(),
            });
        }

        for diag in &infer::infer(db, item).diagnostics {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            let range = ptr.text_range();
            // Field diagnostics squiggle the field *name*, not the whole
            // expression (which may span a long receiver or initializer):
            // the name is the thing that's wrong.
            let range = match diag {
                InferenceDiagnostic::NoSuchField { .. } => {
                    ast::FieldExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.name_ref())
                        .map(|n| n.syntax().text_range())
                        .unwrap_or(range)
                }
                // Reported on the extra field's value expression; the name
                // sits on the enclosing record-field node. (A shorthand
                // field's value *is* its name, so this is a no-op there.)
                InferenceDiagnostic::RecordLitExtraField { .. } => ptr
                    .to_node(&syntax_root)
                    .ancestors()
                    .find_map(ast::RecordExprField::cast)
                    .and_then(|f| f.name_ref())
                    .map(|n| n.syntax().text_range())
                    .unwrap_or(range),
                // The variant name is the wrong part, not the (correct)
                // enum name in front of it.
                InferenceDiagnostic::NoSuchVariant { .. } => {
                    ast::PathExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.variant_name_ref())
                        .map(|n| n.syntax().text_range())
                        .unwrap_or(range)
                }
                // Reported on the `match` keyword: the construct as a whole
                // is what fails to cover — no single arm is the culprit.
                InferenceDiagnostic::NonExhaustiveMatch { .. }
                | InferenceDiagnostic::MatchWithoutCatchAll { .. } => {
                    ast::MatchExpr::cast(ptr.to_node(&syntax_root))
                        .and_then(|it| it.match_token())
                        .map(|t| t.text_range())
                        .unwrap_or(range)
                }
                _ => range,
            };
            // Pattern diagnostics squiggle the pattern; `diag.expr()` (the
            // enclosing match, where MIR traps) is only the fallback.
            let range = match diag.pat().and_then(|pat| source_map.node_for_pat(pat)) {
                Some(pat_ptr) => pat_ptr.text_range(),
                None => range,
            };
            // Messages render in `InferenceDiagnostic::message` (shared with
            // MIR's traps); only ranges and related locations attach here.
            let mut related = match diag {
                InferenceDiagnostic::NeedsAnnotation { item, .. }
                | InferenceDiagnostic::CannotInferGenericParam { item, .. } => item_name(item)
                    .map(|n| {
                        vec![RelatedInfo {
                            file: item.file,
                            range: n.syntax().text_range(),
                            message: "defined here".to_owned(),
                        }]
                    })
                    .unwrap_or_default(),
                // The binder (arity, param kinds) is one click away.
                InferenceDiagnostic::GenericArgCount { item, .. }
                | InferenceDiagnostic::MissingConstArgs { item, .. } => item_name(item)
                    .map(|n| {
                        vec![RelatedInfo {
                            file: item.file,
                            range: n.syntax().text_range(),
                            message: "declared here".to_owned(),
                        }]
                    })
                    .unwrap_or_default(),
                InferenceDiagnostic::TypeMismatch {
                    reasons, expected, ..
                }
                | InferenceDiagnostic::AllBranchesMismatch {
                    reasons, expected, ..
                } => {
                    // Resolve an expression to its AST node for hints that
                    // point at a sub-element (a return type, an operator
                    // token) rather than the whole expression.
                    let ast_for_expr = |expr: body::ExprId| {
                        Some(source_map.node_for_expr(expr)?.to_node(&syntax_root))
                    };
                    let render_reason = |r: &Cause| -> Vec<RelatedInfo> {
                        match r {
                            Cause::Binding(binding) => source_map
                                .annotation_for_binding(*binding)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "expected `{}` because of this annotation",
                                        expected.display()
                                    ),
                                })
                                .or_else(|| {
                                    // No annotation to point at: the type
                                    // was inferred, so blame the binding's
                                    // name — that's where the decision
                                    // became attached.
                                    source_map
                                        .node_for_binding(*binding)
                                        .map(|ptr| RelatedInfo {
                                            file,
                                            range: ptr.text_range(),
                                            message: format!(
                                                "`{}` was inferred to have type `{}` \
                                             from its initializer",
                                                body.bindings[*binding].name,
                                                expected.display()
                                            ),
                                        })
                                })
                                .into_iter()
                                .collect(),
                            Cause::ItemAnnotation => item_source(db, item)
                                .and_then(|it| it.ty())
                                .map(|ty| RelatedInfo {
                                    file,
                                    range: ty.syntax().text_range(),
                                    message: format!(
                                        "expected `{}` because of this annotation",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::ReturnAnnotation(fn_expr) => ast_for_expr(*fn_expr)
                                .and_then(ast::FnLiteral::cast)
                                .and_then(|f| f.ret_type())
                                .map(|ret| RelatedInfo {
                                    file,
                                    range: ret.syntax().text_range(),
                                    message: format!(
                                        "expected `{}` because of this return type",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Operator(op_expr) => ast_for_expr(*op_expr)
                                .and_then(|node| {
                                    ast::BinExpr::cast(node.clone())
                                        .and_then(|bin| bin.op_token())
                                        .or_else(|| {
                                            ast::NegExpr::cast(node)
                                                .and_then(|neg| neg.minus_token())
                                        })
                                })
                                .map(|op| RelatedInfo {
                                    file,
                                    range: op.text_range(),
                                    message: format!(
                                        "`{}` requires `{}` operands",
                                        op.text(),
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Condition(if_expr) => ast_for_expr(*if_expr)
                                .and_then(ast::IfExpr::cast)
                                .and_then(|it| it.if_token())
                                .map(|token| RelatedInfo {
                                    file,
                                    range: token.text_range(),
                                    message: "this `if` requires a `bool` condition".to_owned(),
                                })
                                .into_iter()
                                .collect(),
                            Cause::MissingElse(if_expr) => ast_for_expr(*if_expr)
                                .and_then(ast::IfExpr::cast)
                                .and_then(|it| it.if_token())
                                .map(|token| RelatedInfo {
                                    file,
                                    range: token.text_range(),
                                    message: "this `if` has no `else`, so its value is `()`"
                                        .to_owned(),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Operand(operand_expr) => source_map
                                .node_for_expr(*operand_expr)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "this operand has type `{}`",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Branch(branch_expr) => source_map
                                .node_for_expr(*branch_expr)
                                .map(|ptr| RelatedInfo {
                                    file,
                                    range: ptr.text_range(),
                                    message: format!(
                                        "this branch has type `{}`",
                                        expected.display()
                                    ),
                                })
                                .into_iter()
                                .collect(),
                            Cause::Constructor(call) => {
                                // Blame the *declaration*: the mismatching
                                // field's declared type when the diagnosed
                                // expression sits in a record-literal field,
                                // the declaration's name otherwise.
                                let callee = match &body.exprs[*call] {
                                    body::ExprData::Call { callee, .. } => *callee,
                                    _ => *call,
                                };
                                let Some(Resolution::TypeItem(loc)) = resolutions.get(callee)
                                else {
                                    return Vec::new();
                                };
                                let Some(ast::Item::TypeItem(decl)) =
                                    item_source(db, loc.to_id(db))
                                else {
                                    return Vec::new();
                                };
                                let field_decl = ptr
                                    .to_node(&syntax_root)
                                    .ancestors()
                                    .find_map(ast::RecordExprField::cast)
                                    .and_then(|f| f.name_ref())
                                    .map(|n| n.text())
                                    .and_then(|name| match decl.body()? {
                                        ast::Expr::RecordExpr(record) => {
                                            record.fields().find(|f| {
                                                f.name_ref().is_some_and(|n| n.text() == name)
                                            })
                                        }
                                        _ => None,
                                    });
                                match field_decl {
                                    Some(field) => vec![RelatedInfo {
                                        file: loc.file,
                                        range: field.syntax().text_range(),
                                        message: format!(
                                            "expected `{}` because of this field declaration",
                                            expected.display()
                                        ),
                                    }],
                                    None => decl
                                        .name()
                                        .map(|n| RelatedInfo {
                                            file: loc.file,
                                            range: n.syntax().text_range(),
                                            message: format!(
                                                "expected `{}` because of `{}`'s declaration",
                                                expected.display(),
                                                loc.display_name()
                                            ),
                                        })
                                        .into_iter()
                                        .collect(),
                                }
                            }
                            Cause::CallSite { call, arg } => {
                                // Self-evident when the mismatch sits inside
                                // the call itself; skip the hints then. (The
                                // generic containment filter below no longer
                                // catches this once the hints point at the
                                // callee and argument instead of the whole
                                // call.)
                                let Some(call_ptr) = source_map.node_for_expr(*call) else {
                                    return Vec::new();
                                };
                                if call_ptr.text_range().contains_range(range) {
                                    return Vec::new();
                                }
                                // Point at the function name and the argument
                                // the requirement travels through, not the
                                // whole call expression.
                                let callee = match &body.exprs[*call] {
                                    body::ExprData::Call { callee, .. } => *callee,
                                    _ => *call,
                                };
                                let callee_hint =
                                    source_map.node_for_expr(callee).map(|ptr| RelatedInfo {
                                        file,
                                        range: ptr.text_range(),
                                        message: format!(
                                            "this call requires `{}`",
                                            expected.display()
                                        ),
                                    });
                                let arg_hint =
                                    source_map.node_for_expr(*arg).map(|ptr| RelatedInfo {
                                        file,
                                        range: ptr.text_range(),
                                        message: format!(
                                            "this argument needs to be `{}`",
                                            expected.display()
                                        ),
                                    });
                                callee_hint.into_iter().chain(arg_hint).collect()
                            }
                            Cause::GenericArg { mention, index } => {
                                // Point at the turbofish argument that
                                // instantiated the param, naming the param
                                // from the mentioned item's binder.
                                let Some(node) = ast_for_expr(*mention) else {
                                    return Vec::new();
                                };
                                let Some(arg) = ast::PathExpr::cast(node)
                                    .and_then(|path| path.generic_arg_list())
                                    .and_then(|list| list.args().nth(*index as usize))
                                else {
                                    return Vec::new();
                                };
                                let param_name = match &body.exprs[*mention] {
                                    body::ExprData::GenericApp { base, .. } => {
                                        match resolutions.get(*base) {
                                            Some(Resolution::Item(loc)) => {
                                                item_data(db, loc.to_id(db)).as_ref().and_then(
                                                    |data| {
                                                        data.generics
                                                            .get(*index as usize)
                                                            .map(|param| param.name.clone())
                                                    },
                                                )
                                            }
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                };
                                let Some(param_name) = param_name else {
                                    return Vec::new();
                                };
                                let range = match &arg {
                                    ast::GenericArg::TypeArg(it) => it.syntax().text_range(),
                                    ast::GenericArg::ConstArg(it) => it.syntax().text_range(),
                                };
                                vec![RelatedInfo {
                                    file,
                                    range,
                                    message: format!(
                                        "because `{param_name}` was instantiated to `{}` \
                                         by this argument",
                                        expected.display()
                                    ),
                                }]
                            }
                        }
                    };
                    reasons.iter().flat_map(render_reason).collect()
                }
                InferenceDiagnostic::IfBranchMismatch {
                    then_expr, then_ty, ..
                } => source_map
                    .node_for_expr(*then_expr)
                    .map(|ptr| {
                        vec![RelatedInfo {
                            file,
                            range: ptr.text_range(),
                            message: format!("this branch has type `{}`", then_ty.display()),
                        }]
                    })
                    .unwrap_or_default(),
                InferenceDiagnostic::ArgCountMismatch { expr, .. } => {
                    // Show where the function is defined, so its parameter
                    // list is one click away.
                    let callee_loc = match &body.exprs[*expr] {
                        body::ExprData::Call { callee, .. } => match resolutions.get(*callee) {
                            Some(Resolution::Item(loc)) => Some(loc),
                            _ => None,
                        },
                        _ => None,
                    };
                    callee_loc
                        .and_then(|loc| {
                            let name = item_name(loc)?;
                            Some(vec![RelatedInfo {
                                file: loc.file,
                                range: name.syntax().text_range(),
                                message: format!("`{}` is defined here", loc.display_name()),
                            }])
                        })
                        .unwrap_or_default()
                }
                InferenceDiagnostic::AssignToImmutable { binding, name, .. }
                | InferenceDiagnostic::AddrOfMutImmutable { binding, name, .. } => {
                    // Where `mut` is missing — also the anchor for the
                    // insert-`mut` quick fix (assignments and `&raw mut`
                    // judge the same transitive root).
                    source_map
                        .node_for_binding(*binding)
                        .map(|ptr| {
                            vec![RelatedInfo {
                                file,
                                range: ptr.text_range(),
                                message: format!("`{name}` is declared without `mut` here"),
                            }]
                        })
                        .unwrap_or_default()
                }
                InferenceDiagnostic::AssignToItem { item: target, .. }
                | InferenceDiagnostic::AddrOfMutItem { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // Same shape as `ArgCountMismatch`: the constructor's
                // declaration is one click away.
                InferenceDiagnostic::TypeCtorArgCount { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // The declaration explains what variants (or fields) do
                // exist — one click away.
                InferenceDiagnostic::NoSuchVariant { item: target, .. }
                | InferenceDiagnostic::NoVariantsOnStruct { item: target, .. }
                | InferenceDiagnostic::EnumCtorIsVariant { item: target, .. }
                | InferenceDiagnostic::PatNoSuchVariant { item: target, .. }
                | InferenceDiagnostic::PatWrongEnum { item: target, .. }
                | InferenceDiagnostic::BindShadowsVariant { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                // The message names the nominal type only; where its shape
                // is declared is the hint (an enum has variants, not
                // fields — say so instead of promising fields).
                // G13's opt-out, made discoverable: a module-level static
                // of the wanted name exists — say how to call it.
                InferenceDiagnostic::NoSuchMember { name, .. } => {
                    match file_scope(db, file).resolve(name) {
                        Some(Resolution::Item(loc)) => item_name(&loc)
                            .map(|n| {
                                vec![RelatedInfo {
                                    file: loc.file,
                                    range: n.syntax().text_range(),
                                    message: format!(
                                        "a module-level `{name}` is defined here — statics are \
                                         never dot-callable; call `{name}(...)` instead"
                                    ),
                                }]
                            })
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    }
                }
                // The member's definition (its last parameter) is what
                // makes it not dot-callable — one click away.
                InferenceDiagnostic::NotDotCallable { member, .. } => {
                    item_tree::member_source(db, member.to_id(db))
                        .and_then(|m| m.name())
                        .map(|n| {
                            vec![RelatedInfo {
                                file: member.file,
                                range: n.syntax().text_range(),
                                message: format!("`{}` is defined here", member.display_name()),
                            }]
                        })
                        .unwrap_or_default()
                }
                InferenceDiagnostic::NoSuchField {
                    receiver_ty: Ty::Named(named),
                    ..
                } => item_source(db, named.decl.to_id(db))
                    .and_then(|it| it.body())
                    .map(|decl_body| {
                        let message = if ty::enum_variants(db, named.decl.to_id(db)).is_some() {
                            format!(
                                "`{}` is an `enum`, declared here — it has variants, not fields",
                                named.decl.display_name()
                            )
                        } else {
                            format!(
                                "the fields of `{}` are declared here",
                                named.decl.display_name()
                            )
                        };
                        vec![RelatedInfo {
                            file: named.decl.file,
                            range: decl_body.syntax().text_range(),
                            message,
                        }]
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            // A hint enclosing the squiggle adds nothing — the user is
            // already looking at it (e.g. "this call requires `str`" on the
            // very call whose argument carries the mismatch).
            related.retain(|r| !(r.file == file && r.range.contains_range(range)));
            let fix = match diag {
                // Insert `mut ` right before the binding's name, whether it
                // came from a `let` or a parameter — both render the fixed
                // source as `let mut x = …` / `fn (mut n: usize)`. A hole
                // (`_`) never resolves as an assignment target, so this
                // shouldn't fire for one; skip defensively rather than offer
                // a nonsensical `mut _`.
                InferenceDiagnostic::AssignToImmutable { binding, name, .. }
                | InferenceDiagnostic::AddrOfMutImmutable { binding, name, .. }
                    if name != "_" =>
                {
                    source_map
                        .node_for_binding(*binding)
                        .map(|ptr| syntax::Fix {
                            label: format!("Make `{name}` mutable"),
                            edits: vec![syntax::TextEdit {
                                range: TextRange::empty(ptr.text_range().start()),
                                insert: "mut ".to_owned(),
                            }],
                        })
                }
                _ => None,
            };
            diagnostics.push(Diagnostic {
                range,
                severity: diag.severity(),
                message: diag.message(),
                fix,
                related,
            });
        }

        // Unsafe-check findings: a raw-pointer deref outside any
        // `unsafe { ... }` block. Messages render in
        // `UnsafeCheckDiagnostic::message` (shared with MIR's traps).
        for diag in unsafe_check::unsafe_check(db, item) {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                severity: Severity::Error,
                message: diag.message(),
                fix: None,
                related: Vec::new(),
            });
        }

        for diag in const_check::const_check(db, item) {
            let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                continue;
            };
            // Messages render in `ConstCheckDiagnostic::message` (via
            // `diag`, shared with MIR's traps); only ranges and related
            // locations attach here.
            let mut related = match diag {
                ConstCheckDiagnostic::NonConstFnCall { item: target, .. } => item_name(target)
                    .map(|name| {
                        vec![RelatedInfo {
                            file: target.file,
                            range: name.syntax().text_range(),
                            message: format!("`{}` is defined here", target.display_name()),
                        }]
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            // Every const-check finding fires *at* a call inside a const
            // context; a second hint explains *why* that location is one —
            // the enclosing `const { ... }` block's keyword, the enclosing
            // `const fn`'s marker, or (neither found climbing from the
            // callee) the item's own initializer.
            related.extend(const_context_reason(
                db,
                item,
                file,
                ptr.to_node(&syntax_root),
            ));
            // `NonConstFnCall` names exactly the fix: the callee item's
            // initializer is (by construction of the diagnostic — see
            // `root_fn_is_const`) a plain `fn` literal; inserting `const `
            // right before it is always well-typed. Offered only when the
            // declaration is edited in the same file as the fix's range
            // (`Fix`/`TextEdit` carry no file of their own).
            let fix = match diag {
                ConstCheckDiagnostic::NonConstFnCall { item: target, .. }
                    if target.file == file =>
                {
                    item_source(db, target.to_id(db))
                        .and_then(|it| it.body())
                        .and_then(|body| match body {
                            ast::Expr::FnLiteral(fn_lit) if !fn_lit.is_const() => Some(fn_lit),
                            _ => None,
                        })
                        .map(|fn_lit| syntax::Fix {
                            label: format!("Mark `{}` as `const fn`", target.display_name()),
                            edits: vec![syntax::TextEdit {
                                range: TextRange::empty(fn_lit.syntax().text_range().start()),
                                insert: "const ".to_owned(),
                            }],
                        })
                }
                _ => None,
            };
            diagnostics.push(Diagnostic {
                range: ptr.text_range(),
                severity: Severity::Error,
                message: diag.message(),
                fix,
                related,
            });
        }
    }

    // Tripwire (rustc's "delayed bug" pattern): the invariant is that every
    // `{error}` in the file is downstream of at least one diagnostic above —
    // that's what makes "no diagnostics" mean "lowerable". If types are
    // broken but the file looks clean, a diagnostic is missing somewhere;
    // say so loudly instead of leaving hover-only weirdness. Warnings
    // (unreachable arms) don't justify an `{error}`, so they don't disarm it.
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        'items: for item in all_checkable_items(db, file) {
            let (_, source_map) = body_with_source_map(db, item);
            for (expr, ty) in infer::infer(db, item).type_of_expr.iter() {
                if !ty.contains_error() {
                    continue;
                }
                let Some(ptr) = source_map.node_for_expr(expr) else {
                    continue;
                };
                diagnostics.push(Diagnostic {
                    range: ptr.text_range(),
                    severity: Severity::Error,
                    message: "internal error: this expression has type `{error}` but no \
                              error was reported — this is a bug in the Must language server"
                        .to_owned(),
                    fix: None,
                    related: Vec::new(),
                });
                break 'items;
            }
        }
    }

    // Hole-named items (`static _ = ...` / `const _ = ...`) bind nothing, so
    // their value can never be referenced — but they are still evaluated at
    // check time (statics and consts evaluate eagerly; see
    // `eval::const_value`/`const_block_values`, driven from `ide`), so a
    // panicking initializer still reports its own error alongside this
    // warning. A broken item with no name at all (a parse error, already
    // reported above) gets nothing: `_` is a real, distinct token from an
    // absent name (see `ast::Name::is_hole`), so this never fires for it.
    // Runs after the tripwire above so a hole item alone never masks it.
    for &item in file_item_ids(db, file) {
        let Some(name) = item_source(db, item).and_then(|it| it.name()) else {
            continue;
        };
        if !name.is_hole() {
            continue;
        }
        diagnostics.push(Diagnostic {
            range: name.syntax().text_range(),
            severity: Severity::Warning,
            message: "this item binds nothing and its value cannot be used".to_owned(),
            fix: None,
            related: Vec::new(),
        });
    }

    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    diagnostics
}

/// Definition-site rules for inherent members, reported at the member's
/// own syntax: the fully-annotated rule (member signatures are always
/// annotation-derived — dot-call resolution reads heads without
/// inference) and duplicate member names. Fields and members are SEPARATE
/// namespaces (G13): a member named like a field is legal — call syntax
/// selects the member, bare access the field — so no collision rule
/// lives here.
fn member_definition_diagnostics(db: &dyn Db, file: SourceFile, diagnostics: &mut Vec<Diagnostic>) {
    for &owner in file_item_ids(db, file) {
        let members = item_tree::type_members(db, owner);
        if members.is_empty() {
            continue;
        }
        let Some(ast::Item::TypeItem(decl)) = item_source(db, owner) else {
            continue;
        };
        let sources = item_tree::semantic_member_sources(&decl);
        let member_name_range = |name: &str, dis: u32| {
            sources
                .iter()
                .find(|(n, d, _)| n == name && *d == dis)
                .map(|(_, _, member)| {
                    member
                        .name()
                        .map(|n| n.syntax().text_range())
                        .unwrap_or_else(|| member.syntax().text_range())
                })
        };
        for member in members {
            let Some(range) = member_name_range(&member.name, member.disambiguator) else {
                continue;
            };
            if member.type_ref.is_none() {
                diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Error,
                    message: format!(
                        "member `{}` must spell its full signature: \
                         every parameter and the return type",
                        member.name
                    ),
                    fix: None,
                    related: Vec::new(),
                });
            }
            if member.disambiguator > 0 {
                let related = member_name_range(&member.name, 0)
                    .map(|first| {
                        vec![RelatedInfo {
                            file,
                            range: first,
                            message: "first defined here".to_owned(),
                        }]
                    })
                    .unwrap_or_default();
                diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Error,
                    message: format!("duplicate member `{}`", member.name),
                    fix: None,
                    related,
                });
            }
        }
    }
}

/// The error for `name` in *type* position, if any. Mirrors the resolution
/// order of `ty::lower_type_path` exactly — a `type` item wins, then the
/// builtin types, then a value item is "not a type" — so every silent
/// `Ty::Error` the lowering produces has a diagnostic from here. (Only the
/// *message choice* consults the full [`file_scope`]; this function runs in
/// the diagnostics aggregator, outside the inference firewall.)
/// Why `callee_node` sits in a const context, for a const-check finding's
/// second [`RelatedInfo`]: climbing from the callee, the nearest enclosing
/// `const { ... }` block or `const fn` literal, or — climbing all the way
/// out without finding either — the item's own initializer (every
/// `static`/`const` item's value is a const context to begin with). A
/// plain (non-`const`) `fn` literal exits the const context, so climbing
/// stops there without a match (const-checking itself never marks
/// anything inside one `in_const`, so a finding can't be *inside* a plain
/// fn's body without also being inside one of the two reasons above,
/// nested within it).
fn const_context_reason(
    db: &dyn Db,
    item: ItemId<'_>,
    file: SourceFile,
    callee_node: syntax::SyntaxNode,
) -> Option<RelatedInfo> {
    // `ancestors()` yields the node itself first; skip it — a directly
    // *called* plain `fn` literal is itself the finding (never the const
    // context's boundary: that reasoning only applies to an *enclosing*
    // literal the callee sits inside of).
    for ancestor in callee_node.ancestors().skip(1) {
        if let Some(block) = ast::ConstBlockExpr::cast(ancestor.clone()) {
            let token = block.const_token()?;
            return Some(RelatedInfo {
                file,
                range: token.text_range(),
                message: "this `const` block is a const context".to_owned(),
            });
        }
        if let Some(fn_lit) = ast::FnLiteral::cast(ancestor.clone()) {
            if fn_lit.is_const() {
                let token = fn_lit.const_token()?;
                return Some(RelatedInfo {
                    file,
                    range: token.text_range(),
                    message: "this `const fn` is always a const context".to_owned(),
                });
            }
            // A plain `fn` literal boundary: nothing further out is the
            // reason for a callee actually inside it.
            break;
        }
    }
    let token = item_source(db, item)?.syntax().first_token()?;
    Some(RelatedInfo {
        file,
        range: token.text_range(),
        message: "this item's initializer is a const context".to_owned(),
    })
}

/// Whether `node` sits inside a `with`-chain but OUTSIDE the one
/// semantically supported context (an `impl Self` member of a plain
/// group) — reserved territory the annotation mirror stays quiet about.
fn in_reserved_with_region(node: &syntax::SyntaxNode) -> bool {
    node.ancestors().any(|n| ast::WithGroup::can_cast(n.kind()))
        && !syntax::in_inherent_member_context(node)
}

/// The generic binder scoping a MEMBER context: the OWNER type
/// declaration's binder list (on its RHS `struct`/`enum` literal), found by
/// climbing from a `with`-group to its `type` item. `None` outside member
/// contexts.
fn member_owner_binder_list(group: &ast::WithGroup) -> Option<ast::GenericParamList> {
    let item = group.syntax().parent().and_then(ast::TypeItem::cast)?;
    match item.body()? {
        ast::Expr::RecordExpr(record) => record.generic_param_list(),
        ast::Expr::EnumExpr(en) => en.generic_param_list(),
        _ => None,
    }
}

/// How `name`, at the position of `path_type`, relates to the enclosing
/// generic binders — the syntactic mirror of the param scope inference
/// lowers annotations under.
enum TypeParamBinding {
    /// Declared by an enclosing fn literal's binder; the mention is inside
    /// the literal (its param/return annotations or its body).
    Bound,
    /// Declared by a binder the mention sits INSIDE of (a const param's
    /// declared type): a dependent const param, deferred (TR06).
    InOwnBinder,
    NotBound,
}

fn type_param_binding(path_type: &ast::PathType, name: &str) -> TypeParamBinding {
    // The binder list is a direct child of its literal, so climbing hits
    // the list (if the path is inside one) strictly before that literal.
    let mut inside_binder = true;
    let mut passed_a_binder_list = false;
    for ancestor in path_type.syntax().ancestors() {
        if ast::GenericParamList::can_cast(ancestor.kind()) {
            passed_a_binder_list = true;
        }
        // A binder can sit on a fn literal or (for type declarations) on a
        // `struct`/`enum` literal — all three are climbed the same way. A
        // MEMBER context (climbing reaches a `with`-group) is scoped by
        // the OWNER's binder, plus `Self`.
        let list = if let Some(fn_literal) = ast::FnLiteral::cast(ancestor.clone()) {
            fn_literal.generic_param_list()
        } else if let Some(record) = ast::RecordExpr::cast(ancestor.clone()) {
            match record.generic_param_list() {
                Some(list) => Some(list),
                // A nested (binder-less) record literal is transparent —
                // the item-level binder scopes the whole declaration.
                None => continue,
            }
        } else if let Some(en) = ast::EnumExpr::cast(ancestor.clone()) {
            match en.generic_param_list() {
                Some(list) => Some(list),
                None => continue,
            }
        } else if let Some(group) = ast::WithGroup::cast(ancestor.clone()) {
            if name == "Self" {
                return TypeParamBinding::Bound;
            }
            match member_owner_binder_list(&group) {
                Some(list) => Some(list),
                None => continue,
            }
        } else {
            continue;
        };
        let declares = list
            .into_iter()
            .flat_map(|list| list.params())
            .any(|param| match param {
                ast::GenericParam::TypeParam(it) => it.name().is_some_and(|n| n.text() == name),
                ast::GenericParam::ConstParam(_) => false,
            });
        if declares {
            return if inside_binder && passed_a_binder_list {
                TypeParamBinding::InOwnBinder
            } else {
                TypeParamBinding::Bound
            };
        }
        // Past the first literal, any binder list we crossed belonged
        // to it, not to a literal further out.
        inside_binder = false;
    }
    TypeParamBinding::NotBound
}

/// The generic-binder arity of the type item `loc` (0 for non-generic).
fn decl_generics_len(db: &dyn Db, loc: &ItemLoc) -> usize {
    item_data(db, loc.to_id(db))
        .as_ref()
        .map(|data| data.generics.len())
        .unwrap_or(0)
}

/// The "declared here" hint pointing at `loc`'s name — the same related
/// info the expression-side `GenericArgCount` renders.
fn declared_here(db: &dyn Db, loc: &ItemLoc) -> Vec<RelatedInfo> {
    item_source(db, loc.to_id(db))
        .and_then(|it| it.name())
        .map(|name| {
            vec![RelatedInfo {
                file: loc.file,
                range: name.syntax().text_range(),
                message: "declared here".to_owned(),
            }]
        })
        .unwrap_or_default()
}

/// The enclosing generic binder's params, read syntactically — the mirror
/// of the [`crate::ty`] `ParamScope` annotation lowering runs under.
#[derive(Default)]
struct BinderInfo {
    type_params: Vec<String>,
    /// `(name, declared type)` per const param.
    const_params: Vec<(String, Option<ast::Type>)>,
}

impl BinderInfo {
    fn names_type_param(&self, name: &str) -> bool {
        self.type_params.iter().any(|param| param == name)
    }
    fn names_const_param(&self, name: &str) -> bool {
        self.const_params.iter().any(|(param, _)| param == name)
    }
    fn const_param_ty(&self, name: &str) -> Option<&ast::Type> {
        self.const_params
            .iter()
            .find(|(param, _)| param == name)
            .and_then(|(_, ty)| ty.as_ref())
    }
}

/// The binder of the first enclosing generic literal — a `fn::<...>`, or a
/// type declaration's `struct::<...>`/`enum::<...>`. Literals without a
/// binder are climbed through (the item-level binder scopes the whole
/// body/declaration).
fn enclosing_binder_info(node: &syntax::SyntaxNode) -> BinderInfo {
    for ancestor in node.ancestors() {
        // A MEMBER context: the OWNER's binder scopes member signatures
        // and bodies, and `Self` is a bound type name of its own.
        let member_context = ast::WithGroup::cast(ancestor.clone());
        let list = if let Some(group) = &member_context {
            match member_owner_binder_list(group) {
                Some(list) => Some(list),
                None => {
                    let mut info = BinderInfo::default();
                    info.type_params.push("Self".to_owned());
                    return info;
                }
            }
        } else if let Some(fn_literal) = ast::FnLiteral::cast(ancestor.clone()) {
            fn_literal.generic_param_list()
        } else if let Some(record) = ast::RecordExpr::cast(ancestor.clone()) {
            record.generic_param_list()
        } else if let Some(en) = ast::EnumExpr::cast(ancestor.clone()) {
            en.generic_param_list()
        } else {
            continue;
        };
        let Some(list) = list else {
            continue;
        };
        let mut info = BinderInfo::default();
        if member_context.is_some() {
            info.type_params.push("Self".to_owned());
        }
        for param in list.params() {
            match param {
                ast::GenericParam::TypeParam(it) => {
                    if let Some(name) = it.name() {
                        info.type_params.push(name.text());
                    }
                }
                ast::GenericParam::ConstParam(it) => {
                    if let Some(name) = it.name() {
                        info.const_params.push((name.text(), it.ty()));
                    }
                }
            }
        }
        return info;
    }
    BinderInfo::default()
}

/// Whether `path_type` is the written form of a CONST argument (`N` in
/// `Buf::<N>` parses as a type arg): the owning mention's binder declares a
/// const param at its position. Such a node is judged by the owner's
/// argument checks, never as a type of its own.
fn in_const_arg_position(db: &dyn Db, file: SourceFile, path_type: &ast::PathType) -> bool {
    let Some(type_arg) = path_type.syntax().parent().and_then(ast::TypeArg::cast) else {
        return false;
    };
    let Some(list) = type_arg
        .syntax()
        .parent()
        .and_then(ast::GenericArgList::cast)
    else {
        return false;
    };
    let Some(index) = list.args().position(|arg| match &arg {
        ast::GenericArg::TypeArg(it) => it.syntax() == type_arg.syntax(),
        ast::GenericArg::ConstArg(_) => false,
    }) else {
        return false;
    };
    let Some(owner) = list.syntax().parent() else {
        return false;
    };
    // The owner is a PATH_TYPE or PATH_EXPR; its first NameRef is the base.
    let Some(base) = owner.children().find_map(ast::NameRef::cast) else {
        return false;
    };
    let name = base.text();
    let loc = match type_scope(db, file).resolve(&name) {
        Some(Resolution::TypeItem(loc)) => loc,
        _ => match file_scope(db, file).resolve(&name) {
            Some(Resolution::Item(loc)) => loc,
            _ => return false,
        },
    };
    item_data(db, loc.to_id(db))
        .as_ref()
        .and_then(|data| data.generics.get(index))
        .is_some_and(|param| matches!(param.kind, item_tree::GenericParamKind::Const(_)))
}

/// The annotation MIRROR of `ty::lower_apply`: every silent `Ty::Error`
/// (or `Error`-slotted argument) the eval-free lowering of a type-position
/// turbofish can produce gets its diagnostic here — same resolution order,
/// same argument judgement. Also handles a bare const-param name used as a
/// type (`len: N`).
#[allow(clippy::too_many_arguments)]
fn apply_position_diagnostics(
    db: &dyn Db,
    file: SourceFile,
    node: &syntax::SyntaxNode,
    name: &str,
    list: Option<&ast::GenericArgList>,
    binder: &BinderInfo,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let whole = node.text_range();
    let simple = |range: TextRange, message: String| Diagnostic {
        range,
        severity: Severity::Error,
        message,
        fix: None,
        related: Vec::new(),
    };
    if binder.names_type_param(name) {
        if list.is_some() {
            diagnostics.push(simple(whole, diag::takes_no_generic_args(name)));
        }
        return;
    }
    if binder.names_const_param(name) {
        diagnostics.push(simple(
            whole,
            format!("`{name}` is a const parameter, not a type"),
        ));
        return;
    }
    let target = match type_scope(db, file).resolve(name) {
        Some(Resolution::TypeItem(loc)) => loc,
        // Ambiguous: the duplicate definitions carry the diagnostics.
        Some(_) => return,
        None => {
            if ty::builtin_type_by_name(name).is_some() {
                if list.is_some() {
                    diagnostics.push(simple(whole, diag::takes_no_generic_args(name)));
                }
                return;
            }
            if let Some(message) = type_position_error(db, file, name) {
                diagnostics.push(simple(whole, message));
            }
            return;
        }
    };
    let generics = item_data(db, target.to_id(db))
        .as_ref()
        .map(|data| data.generics.clone())
        .unwrap_or_default();
    if generics.is_empty() {
        if list.is_some() {
            diagnostics.push(Diagnostic {
                range: whole,
                severity: Severity::Error,
                message: diag::takes_no_generic_args(name),
                fix: None,
                related: declared_here(db, &target),
            });
        }
        return;
    }
    let written: Vec<ast::GenericArg> = list.map(|l| l.args().collect()).unwrap_or_default();
    if written.len() != generics.len() {
        diagnostics.push(Diagnostic {
            range: whole,
            severity: Severity::Error,
            message: diag::generic_arg_count(name, generics.len(), written.len()),
            fix: None,
            related: declared_here(db, &target),
        });
        return;
    }
    for (param, arg) in generics.iter().zip(&written) {
        let arg_range = match arg {
            ast::GenericArg::TypeArg(it) => it.syntax().text_range(),
            ast::GenericArg::ConstArg(it) => it.syntax().text_range(),
        };
        match (&param.kind, arg) {
            // Inner type args are PathTypes of their own — the pass visits
            // them independently; nothing to add here.
            (item_tree::GenericParamKind::Type, ast::GenericArg::TypeArg(_)) => {}
            (item_tree::GenericParamKind::Type, ast::GenericArg::ConstArg(_)) => {
                diagnostics.push(simple(arg_range, diag::type_param_needs_type(&param.name)));
            }
            (item_tree::GenericParamKind::Const(declared), ast::GenericArg::ConstArg(arg)) => {
                if let Some(message) =
                    const_annotation_arg_error(db, file, &target, declared, arg, binder)
                {
                    diagnostics.push(simple(arg_range, message));
                }
            }
            (item_tree::GenericParamKind::Const(declared), ast::GenericArg::TypeArg(ty_arg)) => {
                match ty_arg.ty() {
                    // `_`: const args are never inferred (TR06).
                    Some(ast::Type::HoleType(_)) => {
                        diagnostics.push(simple(arg_range, diag::CONST_ARG_HOLE.to_owned()));
                    }
                    // A bare `N`: an in-scope const-param name.
                    Some(ast::Type::PathType(path))
                        if path.variant_name_ref().is_none()
                            && path.generic_arg_list().is_none()
                            && path
                                .name_ref()
                                .is_some_and(|n| binder.names_const_param(&n.text())) =>
                    {
                        let param_name = path.name_ref().expect("checked above").text();
                        if let Some(message) = const_param_agreement_error(
                            db,
                            file,
                            &target,
                            declared,
                            binder,
                            &param_name,
                        ) {
                            diagnostics.push(simple(arg_range, message));
                        }
                    }
                    _ => {
                        diagnostics.push(simple(
                            arg_range,
                            diag::const_param_needs_value(&param.name),
                        ));
                    }
                }
            }
        }
    }
}

/// Judge one `ConstArg` in annotation position against the const param's
/// declared type — mirroring `ty::lower_const_arg_ref` case by case:
/// literals type-check by their literal kind, `const N` must name an
/// in-scope const param of the same declared type, and blocks are outside
/// the annotation domain entirely (the firewall).
fn const_annotation_arg_error(
    db: &dyn Db,
    file: SourceFile,
    target: &ItemLoc,
    declared: &TypeRef,
    arg: &ast::ConstArg,
    binder: &BinderInfo,
) -> Option<String> {
    match arg.expr() {
        Some(ast::Expr::Literal(lit)) => {
            let found = match lit.kind()? {
                ast::LiteralKind::Int(token) => {
                    let Ok(value) = token.text().replace('_', "").parse::<u128>() else {
                        return Some(diag::INT_LITERAL_TOO_LARGE.to_owned());
                    };
                    // An integer literal type-checks against ANY declared
                    // integer type (literal typing is inferred, never
                    // defaulted) — but must fit its range.
                    let expected = ty::lower_const_decl_ty(db, target.file, declared);
                    return match expected {
                        Ty::Int(kind) => {
                            let fits = i128::try_from(value)
                                .ok()
                                .and_then(|v| IntValue::new(kind, v))
                                .is_some();
                            (!fits).then(|| format!("`{value}` does not fit in `{}`", kind.name()))
                        }
                        expected if expected.contains_error() => None,
                        expected => Some(format!(
                            "type mismatch: expected `{}`, found `{}`",
                            expected.display(),
                            Ty::UnresolvedNumber.display()
                        )),
                    };
                }
                ast::LiteralKind::Str(_) => Ty::Str,
                ast::LiteralKind::Bool(_) => Ty::Bool,
            };
            let expected = ty::lower_const_decl_ty(db, target.file, declared);
            if expected.contains_error() || expected == found {
                return None;
            }
            Some(format!(
                "type mismatch: expected `{}`, found `{}`",
                expected.display(),
                found.display()
            ))
        }
        Some(ast::Expr::PathExpr(path)) => {
            let name = path.name_ref()?.text();
            if binder.names_const_param(&name) {
                const_param_agreement_error(db, file, target, declared, binder, &name)
            } else {
                Some(diag::TYPE_CONST_ARG_NOT_LITERAL.to_owned())
            }
        }
        Some(ast::Expr::ConstBlockExpr(_)) => Some(diag::CONST_BLOCK_TYPE_ARG.to_owned()),
        // Broken source: the parse errors cover it.
        None => None,
        Some(_) => Some(diag::TYPE_CONST_ARG_NOT_LITERAL.to_owned()),
    }
}

/// Judge an array type's length in annotation position (`[T; N]`) — the
/// array sibling of [`const_annotation_arg_error`], against the one
/// declared type a length can have: `usize`. Mirrors the silent
/// `ConstArgValue` cases of `ty`'s lowering case by case.
fn array_len_annotation_error(
    db: &dyn Db,
    file: SourceFile,
    len: &ast::ConstArg,
    binder: &BinderInfo,
) -> Option<String> {
    array_len_expr_error(db, file, len.expr(), binder)
}

/// The expression-shaped core of [`array_len_annotation_error`] — shared
/// with type declarations, where `[usize; N]` parses as an ARRAY_EXPR and
/// the length is a plain child expression.
fn array_len_expr_error(
    db: &dyn Db,
    file: SourceFile,
    len: Option<ast::Expr>,
    binder: &BinderInfo,
) -> Option<String> {
    match len {
        Some(ast::Expr::Literal(lit)) => {
            let found = match lit.kind()? {
                ast::LiteralKind::Int(token) => {
                    if token.text().replace('_', "").parse::<u128>().is_err() {
                        return Some(diag::INT_LITERAL_TOO_LARGE.to_owned());
                    }
                    return None;
                }
                ast::LiteralKind::Str(_) => Ty::Str,
                ast::LiteralKind::Bool(_) => Ty::Bool,
            };
            Some(format!(
                "type mismatch: expected `usize`, found `{}`",
                found.display()
            ))
        }
        Some(ast::Expr::PathExpr(path)) => {
            let name = path.name_ref()?.text();
            if !binder.names_const_param(&name) {
                return Some(diag::TYPE_CONST_ARG_NOT_LITERAL.to_owned());
            }
            let own_declared = TypeRef::from_ast(binder.const_param_ty(&name)?.clone());
            let found = ty::lower_const_decl_ty(db, file, &own_declared);
            if found.contains_error() || found == Ty::Int(ty::IntKind::Usize) {
                return None;
            }
            Some(format!(
                "type mismatch: expected `usize`, found `{}`",
                found.display()
            ))
        }
        Some(ast::Expr::ConstBlockExpr(_)) => Some(diag::CONST_BLOCK_TYPE_ARG.to_owned()),
        // Broken source: the parse errors cover it.
        None => None,
        Some(_) => Some(diag::TYPE_CONST_ARG_NOT_LITERAL.to_owned()),
    }
}

/// `Buf::<N>` forwarding the enclosing binder's const param `N`: the two
/// declared types must agree — there is no subtyping (or conversion) in
/// the const-arg domain either.
fn const_param_agreement_error(
    db: &dyn Db,
    file: SourceFile,
    target: &ItemLoc,
    declared: &TypeRef,
    binder: &BinderInfo,
    param_name: &str,
) -> Option<String> {
    let expected = ty::lower_const_decl_ty(db, target.file, declared);
    let own_declared = TypeRef::from_ast(binder.const_param_ty(param_name)?.clone());
    let found = ty::lower_const_decl_ty(db, file, &own_declared);
    if expected.contains_error() || found.contains_error() || expected == found {
        return None;
    }
    Some(format!(
        "type mismatch: expected `{}`, found `{}`",
        expected.display(),
        found.display()
    ))
}

fn type_position_error(db: &dyn Db, file: SourceFile, name: &str) -> Option<String> {
    match type_scope(db, file).resolve(name) {
        // A type item; or a duplicate name, whose definitions already
        // carry the diagnostics.
        Some(_) => None,
        None => {
            if ty::builtin_type_by_name(name).is_some() {
                return None;
            }
            match file_scope(db, file).resolve(name) {
                // Also covers value-item duplicates: whichever way the
                // ambiguity resolves, it is not a type.
                Some(_) => Some(format!("`{name}` is not a type")),
                None => Some(format!("unknown type `{name}`")),
            }
        }
    }
}

/// The error for `Enum::Variant` in *type* position, if any. Mirrors
/// [`ty::lower_variant_type_path`] exactly (the same silent-`Ty::Error`
/// contract as [`type_position_error`]): the base must name an enum `type`
/// item and the variant must exist in it. An unknown or ambiguous base is
/// covered by [`type_position_error`]-style reporting here too, so a
/// two-segment path never needs a second pass over its first segment.
fn variant_position_error(
    db: &dyn Db,
    file: SourceFile,
    base: &str,
    variant: &str,
) -> Option<String> {
    match type_scope(db, file).resolve(base) {
        Some(Resolution::TypeItem(loc)) => {
            let item = loc.to_id(db);
            // A generic enum's variant types have no annotation spelling
            // yet (`Option::<usize>::Some` parses only in expression
            // position) — the lowering is `Ty::Error`, this is its mirror.
            if decl_generics_len(db, &loc) > 0 {
                return Some(format!(
                    "`{base}` is generic; a generic enum's variant types \
                     cannot be written in annotations yet"
                ));
            }
            match ty::enum_variants(db, item) {
                Some(variants) => {
                    if variant.is_empty() {
                        // `Shape::` — the parse error covers it.
                        return None;
                    }
                    if variants.iter().any(|(name, _)| name == variant) {
                        None
                    } else {
                        Some(format!(
                            "`{}` has no variant `{variant}`",
                            loc.display_name()
                        ))
                    }
                }
                None => {
                    if matches!(type_decl(db, item), Some(TypeDeclData::Struct { .. })) {
                        Some(format!(
                            "`{}` has no variants (it is a `struct` type)",
                            loc.display_name()
                        ))
                    } else {
                        // A broken declaration carries its own diagnostics.
                        None
                    }
                }
            }
        }
        // The duplicate definitions carry the diagnostics.
        Some(_) => None,
        None => {
            if ty::builtin_type_by_name(base).is_some() {
                return Some(format!("`{base}` has no variants (it is a builtin type)"));
            }
            // Reuse the single-segment wording for a base that is no type
            // at all ("unknown type" / "is not a type").
            type_position_error(db, file, base)
        }
    }
}

/// Check the variant payloads of an `enum` literal used as a type
/// declaration: payload positions are real type syntax, so unknown names
/// are covered by the file-wide `PathType` pass — but a payload written
/// with a hole (`_`, or an `fn` type without a return) would silently
/// lower to an erased inference variable ([`ty::enum_variants`] erases it
/// to `Ty::Error`); this is that error's diagnostic.
fn enum_decl_payload_diagnostics(en: &ast::EnumExpr, diagnostics: &mut Vec<Diagnostic>) {
    for variant in en.variants() {
        for payload in variant.payload_types() {
            if TypeRef::from_ast(payload.clone()).is_fully_typed() {
                continue;
            }
            diagnostics.push(Diagnostic {
                range: payload.syntax().text_range(),
                severity: Severity::Error,
                message: "a variant payload must be a fully written type; \
                          a declaration has nothing to infer `_` from"
                    .to_owned(),
                fix: None,
                related: Vec::new(),
            });
        }
    }
}

/// Check the fields of a `struct` literal used as a type declaration:
/// each field must declare a fully written type (`name: Type`) and no
/// value — the field types are real type syntax now (equals-defines
/// respell), so name resolution, arity and turbofish problems inside them
/// are the ordinary annotation mirror's business; only the field-shape
/// rules live here.
fn type_decl_field_diagnostics(record: &ast::RecordExpr, diagnostics: &mut Vec<Diagnostic>) {
    for field in record.fields() {
        let Some(name_ref) = field.name_ref() else {
            // No field name: broken source, the parse error covers it.
            continue;
        };
        if let Some(eq) = field.eq_token() {
            let end = field
                .expr()
                .map(|value| value.syntax().text_range().end())
                .unwrap_or_else(|| eq.text_range().end());
            diagnostics.push(simple_error(
                TextRange::new(eq.text_range().start(), end),
                "a `type` declaration's field declares a type, not a value".to_owned(),
            ));
        }
        match field.ty() {
            // Shorthand, or the retired `name: value` spelling (its parse
            // error already told the respell story — don't pile on).
            None => {
                if field.colon_token().is_none() {
                    diagnostics.push(simple_error(
                        field.syntax().text_range(),
                        format!(
                            "expected a type for field `{}`: `name: Type`",
                            name_ref.text()
                        ),
                    ));
                }
            }
            Some(ty) => {
                if !TypeRef::from_ast(ty.clone()).is_fully_typed() {
                    diagnostics.push(simple_error(
                        ty.syntax().text_range(),
                        "a field's type must be a fully written type; \
                         a declaration has nothing to infer `_` from"
                            .to_owned(),
                    ));
                }
            }
        }
    }
}

fn simple_error(range: TextRange, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Severity::Error,
        message,
        fix: None,
        related: Vec::new(),
    }
}
