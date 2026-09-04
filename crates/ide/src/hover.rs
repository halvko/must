use base_db::{RootDatabase, SourceFile, parse};
use syntax::ast::{self, AstNode};
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::FilePosition;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverResult {
    /// Markdown.
    pub markup: String,
    pub range: TextRange,
}

pub(crate) fn hover(
    db: &RootDatabase,
    FilePosition { file, offset }: FilePosition,
) -> Option<HoverResult> {
    let root = parse(db, file).syntax_node();
    if let Some(result) = const_block_hover(db, file, &root, offset) {
        return Some(result);
    }
    if let Some(result) = loop_hover(db, file, &root, offset) {
        return Some(result);
    }
    let token = root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::IDENT)?;
    let parent = token.parent()?;

    // A type name in annotation position: show the declaration. The
    // variant segment of `p: Shape::Circle` shows the variant instead.
    if let Some(name_ref) = ast::NameRef::cast(parent.clone())
        && let Some(path_type) = name_ref.syntax().parent().and_then(ast::PathType::cast)
    {
        let base = path_type.name_ref()?;
        let hir::Resolution::TypeItem(loc) = hir::file_scope(db, file).resolve(&base.text())?
        else {
            return None;
        };
        if path_type.variant_name_ref().is_some_and(|v| v == name_ref) {
            return variant_hover(db, &loc, &name_ref.text(), name_ref.syntax().text_range());
        }
        return type_item_hover(db, loc.to_id(db), name_ref.syntax().text_range());
    }

    // A variant pattern: the variant segment shows the variant as
    // declared; the qualified spelling's base shows the enum declaration.
    if let Some(name_ref) = ast::NameRef::cast(parent.clone())
        && let Some(variant_pat) = name_ref.syntax().parent().and_then(ast::VariantPat::cast)
    {
        if variant_pat.enum_name_ref().is_some_and(|n| n == name_ref) {
            let hir::Resolution::TypeItem(loc) =
                hir::file_scope(db, file).resolve(&name_ref.text())?
            else {
                return None;
            };
            return type_item_hover(db, loc.to_id(db), name_ref.syntax().text_range());
        }
        let item = hir::checkable_item_at(db, file, variant_pat.syntax())?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let pat = source_map.pat_for_node(SyntaxNodePtr::new(variant_pat.syntax()))?;
        let variant = hir::infer::infer(db, item).variant_of_pat.get(pat)?;
        return variant_hover(
            db,
            &variant.decl,
            &variant.name,
            name_ref.syntax().text_range(),
        );
    }

    // The field name of a field access: the type of the whole access — the
    // field's type — under the field's name.
    if let Some(name_ref) = ast::NameRef::cast(parent.clone())
        && let Some(field_expr) = name_ref.syntax().parent().and_then(ast::FieldExpr::cast)
    {
        let item = hir::checkable_item_at(db, file, field_expr.syntax())?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let expr = source_map.expr_for_node(SyntaxNodePtr::new(field_expr.syntax()))?;
        let ty = hir::infer::infer(db, item).type_of_expr.get(expr)?.clone();
        return Some(HoverResult {
            markup: format!("```must\n{}: {}\n```", name_ref.text(), ty.display()),
            range: name_ref.syntax().text_range(),
        });
    }

    let (name, ty, range, value, mutable) =
        if let Some(name_ref) = ast::NameRef::cast(parent.clone()) {
            // A use: the type of the expression.
            let path_expr = name_ref.syntax().parent().and_then(ast::PathExpr::cast)?;
            let item = hir::checkable_item_at(db, file, path_expr.syntax())?;
            let (body, source_map) = hir::body_with_source_map(db, item);
            // The first segment of `Shape::Circle` has its own expression
            // on the segment's node; anything else is the whole path.
            let expr = source_map
                .expr_for_node(SyntaxNodePtr::new(name_ref.syntax()))
                .or_else(|| source_map.expr_for_node(SyntaxNodePtr::new(path_expr.syntax())))?;
            let resolution = hir::resolutions(db, item).get(expr).cloned();
            // A type name in expression position (a construction head, a
            // variant path's base, or a stray use the diagnostics call
            // out): show the declaration, not the constructor's function
            // type. Checked before the expression's type is demanded — a
            // variant path's base has none.
            if let Some(hir::Resolution::TypeItem(ref loc)) = resolution {
                return type_item_hover(db, loc.to_id(db), name_ref.syntax().text_range());
            }
            let ty = hir::infer::infer(db, item).type_of_expr.get(expr)?.clone();
            // A use of another item also shows that item's const value.
            let value = match resolution {
                Some(hir::Resolution::Item(ref loc)) => const_display(db, loc.to_id(db)),
                _ => None,
            };
            let mutable = matches!(
                resolution,
                Some(hir::Resolution::Local(binding)) if body.bindings[binding].mutable
            );
            (
                name_ref.text(),
                ty,
                name_ref.syntax().text_range(),
                value,
                mutable,
            )
        } else {
            let name = ast::Name::cast(parent)?;
            let item = hir::checkable_item_at(db, file, name.syntax())?;
            if name
                .syntax()
                .parent()
                .is_some_and(|p| p.kind() == SyntaxKind::TYPE_ITEM)
            {
                return type_item_hover(db, item, name.syntax().text_range());
            }
            if name
                .syntax()
                .parent()
                .is_some_and(|p| p.kind() == SyntaxKind::STATIC_ITEM)
            {
                let ty = hir::signature(db, item);
                let value = const_display(db, item);
                (name.text(), ty, name.syntax().text_range(), value, false)
            } else if name
                .syntax()
                .parent()
                .is_some_and(|p| p.kind() == SyntaxKind::MEMBER)
            {
                // A member's name declaration: its signature, like a
                // static fn's.
                let ty = hir::signature(db, item);
                (name.text(), ty, name.syntax().text_range(), None, false)
            } else {
                // A local binding (let or parameter).
                let (body, source_map) = hir::body_with_source_map(db, item);
                let binding = source_map.binding_for_node(SyntaxNodePtr::new(name.syntax()))?;
                let ty = hir::infer::infer(db, item)
                    .type_of_binding
                    .get(binding)?
                    .clone();
                let mutable = body.bindings[binding].mutable;
                (name.text(), ty, name.syntax().text_range(), None, mutable)
            }
        };

    let value = value.map(|v| format!(" = {v}")).unwrap_or_default();
    let mut_prefix = if mutable { "mut " } else { "" };
    Some(HoverResult {
        markup: format!("```must\n{mut_prefix}{name}: {}{value}\n```", ty.display()),
        range,
    })
}

/// Hover for a `type` item (on its declaration or any reference): the whole
/// declaration — the underlying record in canonical (sorted) form for a
/// struct type, the variant list in declaration order for an enum. A broken
/// declaration shows just the head — its diagnostic explains the rest.
fn type_item_hover(
    db: &RootDatabase,
    item: hir::ItemId<'_>,
    range: TextRange,
) -> Option<HoverResult> {
    let name = item.name(db);
    let markup = if let Some(underlying) = hir::type_underlying(db, item) {
        format!("```must\ntype {name} = {}\n```", underlying.display())
    } else if let Some(variants) = hir::enum_variants(db, item) {
        format!(
            "```must\ntype {name} = enum {{ {} }}\n```",
            variants
                .iter()
                .map(|(name, payload)| render_variant(name, payload))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("```must\ntype {name}\n```")
    };
    Some(HoverResult { markup, range })
}

/// Hover for one variant (the second segment of a `::` path in type
/// position): the variant as declared, qualified by its enum.
fn variant_hover(
    db: &RootDatabase,
    loc: &hir::ItemLoc,
    name: &str,
    range: TextRange,
) -> Option<HoverResult> {
    let variants = hir::enum_variants(db, loc.to_id(db)).as_ref()?;
    let (variant, payload) = variants.iter().find(|(variant, _)| variant == name)?;
    Some(HoverResult {
        markup: format!(
            "```must\n{}::{}\n```",
            loc.display_name(),
            render_variant(variant, payload)
        ),
        range,
    })
}

/// `Circle(usize)` / `Point` — a variant as its declaration writes it.
/// `pub(crate)`: `completions`'s `::`-segment and match-arm-pattern
/// candidates render a variant's payload signature the same way.
pub(crate) fn render_variant(name: &str, payload: &[hir::Ty]) -> String {
    if payload.is_empty() {
        return name.to_owned();
    }
    format!(
        "{name}({})",
        payload
            .iter()
            .map(hir::Ty::display)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Hovering the `const` keyword of a `const { … }` block shows the block's
/// computed compile-time value, the way hovering an item shows its const
/// value. Failures show nothing — they already carry a diagnostic.
fn const_block_hover(
    db: &RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    offset: TextSize,
) -> Option<HoverResult> {
    let token = root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::CONST_KW)?;
    let block = ast::ConstBlockExpr::cast(token.parent()?)?;
    let item = hir::checkable_item_at(db, file, block.syntax())?;
    let (_, source_map) = hir::body_with_source_map(db, item);
    let expr = source_map.expr_for_node(SyntaxNodePtr::new(block.syntax()))?;
    let ty = hir::infer::infer(db, item).type_of_expr.get(expr)?.clone();
    let (_, value) = eval::const_block_values(db, item)
        .iter()
        .find(|(e, _)| *e == expr)?;
    let value = value.as_ref().ok()?;
    Some(HoverResult {
        markup: format!(
            "```must\nconst {{ … }}: {} = {}\n```",
            ty.display(),
            value.display()
        ),
        range: token.text_range(),
    })
}

/// Hovering the `loop` keyword shows the loop's type — the join of its
/// `break` values, or `!` for a loop no value-carrying break ever exits.
fn loop_hover(
    db: &RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    offset: TextSize,
) -> Option<HoverResult> {
    let token = root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::LOOP_KW)?;
    let loop_expr = ast::LoopExpr::cast(token.parent()?)?;
    let item = hir::checkable_item_at(db, file, loop_expr.syntax())?;
    let (_, source_map) = hir::body_with_source_map(db, item);
    let expr = source_map.expr_for_node(SyntaxNodePtr::new(loop_expr.syntax()))?;
    let ty = hir::infer::infer(db, item).type_of_expr.get(expr)?.clone();
    Some(HoverResult {
        markup: format!("```must\nloop {{ … }}: {}\n```", ty.display()),
        range: token.text_range(),
    })
}

/// The item's const value, when it adds information beyond the type: a `fn`
/// value is fully described by its signature, and a failed evaluation has
/// its own diagnostic.
fn const_display(db: &RootDatabase, item: hir::ItemId<'_>) -> Option<String> {
    match eval::const_value(db, item) {
        Ok(eval::Value::Fn(_) | eval::Value::Builtin(_)) | Err(_) => None,
        Ok(value) => Some(value.display()),
    }
}
