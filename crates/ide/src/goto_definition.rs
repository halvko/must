use base_db::{RootDatabase, SourceFile, parse};
use hir::Resolution;
use syntax::ast::{self, AstNode};
use syntax::{SyntaxKind, SyntaxNodePtr, TextRange};

use crate::FilePosition;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationTarget {
    pub file: SourceFile,
    /// The whole definition (e.g. the full `static` item or `let` statement).
    pub full_range: TextRange,
    /// What to put the cursor on (the name).
    pub focus_range: TextRange,
}

pub(crate) fn goto_definition(
    db: &RootDatabase,
    FilePosition { file, offset }: FilePosition,
) -> Option<NavigationTarget> {
    let root = parse(db, file).syntax_node();
    let token = root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::IDENT)?;

    let name_ref = token.parent().and_then(ast::NameRef::cast)?;

    // A variant pattern (`::Circle(r)` / `Shape::Circle(r)`): the variant
    // segment resolved type-directed during inference; the qualified
    // spelling's base is the enum `type` item. A bare binding is just its
    // own declaration — nothing to jump to (it falls through to `None`).
    if let Some(variant_pat) = name_ref.syntax().parent().and_then(ast::VariantPat::cast) {
        if variant_pat.enum_name_ref().is_some_and(|n| n == name_ref) {
            let Resolution::TypeItem(loc) = hir::file_scope(db, file).resolve(&name_ref.text())?
            else {
                return None;
            };
            return nav_to_item(db, &loc);
        }
        let item = hir::checkable_item_at(db, file, variant_pat.syntax())?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let pat = source_map.pat_for_node(SyntaxNodePtr::new(variant_pat.syntax()))?;
        let variant = hir::infer::infer(db, item).variant_of_pat.get(pat)?;
        return nav_to_variant(db, variant);
    }

    // A name in *type* position (`p: Foo`, `p: Shape::Circle`) resolves
    // through the file scope directly — annotations aren't body
    // expressions, so there is no resolution entry to look up. The second
    // segment of a variant path resolves type-directed against the enum's
    // declaration, landing on the variant inside the `type` item.
    if let Some(path_type) = name_ref.syntax().parent().and_then(ast::PathType::cast) {
        let base = path_type.name_ref()?;
        let resolution = hir::file_scope(db, file).resolve(&base.text())?;
        // A trait name — a bound (`T: Display`) or a type-side impl head
        // (`impl Display`) — jumps to the trait declaration.
        if let Resolution::TraitItem(loc) = &resolution {
            return nav_to_item(db, loc);
        }
        let Resolution::TypeItem(loc) = resolution else {
            // Builtin types have no source; a value item in type position
            // is not a definition to jump to (a diagnostic already says
            // it's not a type).
            return None;
        };
        if path_type.variant_name_ref().is_some_and(|v| v == name_ref) {
            return nav_to_variant_by_name(db, &loc, &name_ref.text());
        }
        return nav_to_item(db, &loc);
    }

    // The field name of a dot-call that resolved to a member: jump to the
    // member's definition inside the owning `with`-chain (impl-directed),
    // or to the trait's requirement (bound-directed — the impl is only
    // known per instantiation).
    if let Some(field_expr) = name_ref.syntax().parent().and_then(ast::FieldExpr::cast) {
        let item = hir::checkable_item_at(db, file, field_expr.syntax())?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let call = field_expr
            .syntax()
            .parent()
            .filter(|p| ast::CallExpr::can_cast(p.kind()))?;
        let call_expr = source_map.expr_for_node(SyntaxNodePtr::new(&call))?;
        let infer = hir::infer::infer(db, item);
        if let Some(member) = infer.member_of_expr.get(call_expr) {
            return nav_to_member(db, member);
        }
        let bound_call = infer.bound_member_of_expr.get(call_expr)?;
        return nav_to_requirement(db, &bound_call.trait_, bound_call.member_index);
    }

    // `::Circle` in expression position: the sigil's sole segment is the
    // variant, resolved type-directed rather than by name — so the jump
    // reads the same `variant_of_expr` entry the qualified spelling
    // produces, and lands in the same place.
    if let Some(elided) = name_ref
        .syntax()
        .parent()
        .and_then(ast::ElidedVariantExpr::cast)
    {
        let item = hir::checkable_item_at(db, file, elided.syntax())?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let expr = source_map.expr_for_node(SyntaxNodePtr::new(elided.syntax()))?;
        let variant = hir::infer::infer(db, item).variant_of_expr.get(expr)?;
        return nav_to_variant(db, variant);
    }

    let path_expr = name_ref.syntax().parent().and_then(ast::PathExpr::cast)?;

    // Which item are we inside?
    let item = hir::checkable_item_at(db, file, path_expr.syntax())?;

    let (_, source_map) = hir::body_with_source_map(db, item);

    // The second segment of `Shape::Circle`: inference resolved it against
    // the enum's declaration; jump to the variant inside the `type` item.
    // The member segment of a qualified trait call (`Display::fmt`) jumps
    // to the impl member the call resolved to, or to the trait's
    // requirement when the resolution is bound-directed/unresolved.
    if path_expr.variant_name_ref().is_some_and(|v| v == name_ref) {
        let infer = hir::infer::infer(db, item);
        let path = source_map.expr_for_node(SyntaxNodePtr::new(path_expr.syntax()))?;
        if let Some(variant) = infer.variant_of_expr.get(path) {
            return nav_to_variant(db, variant);
        }
        // A qualified MEMBER reference (`Point::len`, or the named-Self
        // `Display::<Self = Foo>::fmt`) names exactly one member.
        if let Some(value) = infer.member_value_of_expr.get(path) {
            return nav_to_member(db, &value.member);
        }
        let base_name = path_expr.name_ref()?;
        if let Some(Resolution::TraitItem(trait_loc)) =
            hir::file_scope(db, file).resolve(&base_name.text())
        {
            if let Some(call) = path_expr
                .syntax()
                .parent()
                .filter(|p| ast::CallExpr::can_cast(p.kind()))
                && let Some(call_expr) = source_map.expr_for_node(SyntaxNodePtr::new(&call))
                && let Some(member) = infer.qualified_member_of_expr.get(call_expr)
            {
                return nav_to_member(db, member);
            }
            let requirements = hir::trait_requirements(db, trait_loc.to_id(db));
            let index = requirements
                .iter()
                .position(|req| req.name == name_ref.text())?;
            return nav_to_requirement(db, &trait_loc, index as u32);
        }
        return None;
    }
    // The first segment of `Shape::Circle` lowers as its own `NameRef`
    // expression on the segment's node; a single-segment path sits on the
    // whole path node.
    let expr = source_map
        .expr_for_node(SyntaxNodePtr::new(name_ref.syntax()))
        .or_else(|| source_map.expr_for_node(SyntaxNodePtr::new(path_expr.syntax())))?;
    match hir::resolutions(db, item).get(expr)? {
        Resolution::Local(binding) => {
            let name_ptr = source_map.node_for_binding(*binding)?;
            // The name's node is the focus; its enclosing let/param is the
            // full range.
            let name_node = name_ptr.to_node(&root);
            let full_range = name_node
                .ancestors()
                .find(|n| matches!(n.kind(), SyntaxKind::LET_STMT | SyntaxKind::PARAM))
                .map(|n| n.text_range())
                .unwrap_or_else(|| name_ptr.text_range());
            Some(NavigationTarget {
                file,
                full_range,
                focus_range: name_ptr.text_range(),
            })
        }
        // Ambiguous: jump to the first definition — better than going dead.
        // Type items navigate the same way (a construction call's head is a
        // reference to the declaration).
        // A trait name (a qualified call's base) navigates like a type's.
        Resolution::Item(loc)
        | Resolution::Ambiguous(loc)
        | Resolution::TypeItem(loc)
        | Resolution::TraitItem(loc) => nav_to_item(db, loc),
        // A const param: jump to its declaration in the enclosing
        // binder (`const N: usize` in `fn::<...>`).
        Resolution::ConstParam(index) => {
            // A member's const params live on the OWNER type's binder.
            let source_item = hir::member_owner(db, item).unwrap_or(item);
            let list = hir::item_source(db, source_item)
                .and_then(|it| it.body())
                .and_then(|body| match body {
                    ast::Expr::FnLiteral(fn_lit) => fn_lit.generic_param_list(),
                    ast::Expr::RecordExpr(record) => record.generic_param_list(),
                    ast::Expr::EnumExpr(en) => en.generic_param_list(),
                    _ => None,
                })?;
            let param = list.params().nth(*index as usize)?;
            let ast::GenericParam::ConstParam(const_param) = param else {
                return None;
            };
            let name = const_param.name()?;
            Some(NavigationTarget {
                file,
                full_range: const_param.syntax().text_range(),
                focus_range: name.syntax().text_range(),
            })
        }
        // Builtins have no source to jump to.
        Resolution::Builtin(_) => None,
    }
}

/// Navigate to a trait's requirement (by declaration index): the
/// requirement member node is the full range, its name the focus.
fn nav_to_requirement(
    db: &RootDatabase,
    trait_loc: &hir::ItemLoc,
    index: u32,
) -> Option<NavigationTarget> {
    let ast::Item::TraitItem(decl) = hir::item_source(db, trait_loc.to_id(db))? else {
        return None;
    };
    let requirements = hir::trait_requirements(db, trait_loc.to_id(db));
    let name = &requirements.get(index as usize)?.name;
    let member = decl
        .requires_def()?
        .members()
        .find(|member| member.name().is_some_and(|n| n.text() == *name))?;
    let focus_range = member
        .name()
        .map(|n| n.syntax().text_range())
        .unwrap_or_else(|| member.syntax().text_range());
    Some(NavigationTarget {
        file: trait_loc.file,
        full_range: member.syntax().text_range(),
        focus_range,
    })
}

/// Navigate to a member's definition inside its type's `with`-chain: the
/// member node is the full range, its name the focus.
fn nav_to_member(db: &RootDatabase, member: &hir::ItemLoc) -> Option<NavigationTarget> {
    let src = hir::item_tree::member_source(db, member.to_id(db))?;
    let full_range = src.syntax().text_range();
    let focus_range = src
        .name()
        .map(|n| n.syntax().text_range())
        .unwrap_or(full_range);
    Some(NavigationTarget {
        file: member.file,
        full_range,
        focus_range,
    })
}

/// Navigate to a variant's declaration inside its enum's `type` item: the
/// variant node is the full range, its name the focus.
fn nav_to_variant(db: &RootDatabase, variant: &hir::VariantTy) -> Option<NavigationTarget> {
    let ast::Item::TypeItem(decl) = hir::item_source(db, variant.decl.to_id(db))? else {
        return None;
    };
    let ast::Expr::EnumExpr(en) = decl.body()? else {
        return None;
    };
    let variant_node = en.variants().nth(variant.index as usize)?;
    let full_range = variant_node.syntax().text_range();
    let focus_range = variant_node
        .name()
        .map(|n| n.syntax().text_range())
        .unwrap_or(full_range);
    Some(NavigationTarget {
        file: variant.decl.file,
        full_range,
        focus_range,
    })
}

/// As [`nav_to_variant`], from a *type-position* path (`p: Shape::Circle`),
/// where no inference result carries the resolution: look the name up in
/// the declaration directly.
fn nav_to_variant_by_name(
    db: &RootDatabase,
    loc: &hir::ItemLoc,
    name: &str,
) -> Option<NavigationTarget> {
    let index = hir::enum_variants(db, loc.to_id(db))
        .as_ref()?
        .iter()
        .position(|(variant, _)| variant == name)?;
    nav_to_variant(
        db,
        &hir::VariantTy {
            decl: loc.clone(),
            args: Vec::new(),
            index: index as u32,
            name: name.into(),
        },
    )
}

fn nav_to_item(db: &RootDatabase, loc: &hir::ItemLoc) -> Option<NavigationTarget> {
    let target = loc.to_id(db);
    let src = hir::item_source(db, target)?;
    let full_range = src.syntax().text_range();
    let focus_range = src
        .name()
        .map(|n| n.syntax().text_range())
        .unwrap_or(full_range);
    Some(NavigationTarget {
        file: loc.file,
        full_range,
        focus_range,
    })
}
