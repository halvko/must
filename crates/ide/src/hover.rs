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
    let token = root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::IDENT)?;
    let parent = token.parent()?;

    let item_index = |node: &SyntaxNode| {
        let item_node = node
            .ancestors()
            .find(|n| n.kind() == SyntaxKind::STATIC_ITEM)?;
        root.children()
            .filter(|n| n.kind() == SyntaxKind::STATIC_ITEM)
            .position(|n| n == item_node)
    };

    let (name, ty, range, value) = if let Some(name_ref) = ast::NameRef::cast(parent.clone()) {
        // A use: the type of the expression.
        let path_expr = name_ref.syntax().parent().and_then(ast::PathExpr::cast)?;
        let item = *hir::file_item_ids(db, file).get(item_index(path_expr.syntax())?)?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let expr = source_map.expr_for_node(SyntaxNodePtr::new(path_expr.syntax()))?;
        let ty = hir::infer::infer(db, item).type_of_expr.get(expr)?.clone();
        // A use of another item also shows that item's const value.
        let value = match hir::resolutions(db, item).get(expr) {
            Some(hir::Resolution::Item(loc)) => const_display(db, loc.to_id(db)),
            _ => None,
        };
        (name_ref.text(), ty, name_ref.syntax().text_range(), value)
    } else {
        let name = ast::Name::cast(parent)?;
        let item = *hir::file_item_ids(db, file).get(item_index(name.syntax())?)?;
        if name
            .syntax()
            .parent()
            .is_some_and(|p| p.kind() == SyntaxKind::STATIC_ITEM)
        {
            let ty = hir::signature(db, item);
            let value = const_display(db, item);
            (name.text(), ty, name.syntax().text_range(), value)
        } else {
            // A local binding (let or parameter).
            let (_, source_map) = hir::body_with_source_map(db, item);
            let binding = source_map.binding_for_node(SyntaxNodePtr::new(name.syntax()))?;
            let ty = hir::infer::infer(db, item)
                .type_of_binding
                .get(binding)?
                .clone();
            (name.text(), ty, name.syntax().text_range(), None)
        }
    };

    let value = value.map(|v| format!(" = {v}")).unwrap_or_default();
    Some(HoverResult {
        markup: format!("```must\n{name}: {}{value}\n```", ty.display()),
        range,
    })
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
    let item_node = block
        .syntax()
        .ancestors()
        .find(|n| n.kind() == SyntaxKind::STATIC_ITEM)?;
    let index = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::STATIC_ITEM)
        .position(|n| n == item_node)?;
    let item = *hir::file_item_ids(db, file).get(index)?;
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

/// The item's const value, when it adds information beyond the type: a `fn`
/// value is fully described by its signature, and a failed evaluation has
/// its own diagnostic.
fn const_display(db: &RootDatabase, item: hir::ItemId<'_>) -> Option<String> {
    match eval::const_value(db, item) {
        Ok(eval::Value::Fn(_) | eval::Value::Builtin(_)) | Err(_) => None,
        Ok(value) => Some(value.display()),
    }
}
