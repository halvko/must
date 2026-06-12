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
    let path_expr = name_ref.syntax().parent().and_then(ast::PathExpr::cast)?;

    // Which item are we inside?
    let item_node = path_expr
        .syntax()
        .ancestors()
        .find(|n| n.kind() == SyntaxKind::STATIC_ITEM)?;
    let item_index = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::STATIC_ITEM)
        .position(|n| n == item_node)?;
    let item = *hir::file_item_ids(db, file).get(item_index)?;

    let (_, source_map) = hir::body_with_source_map(db, item);
    let expr = source_map.expr_for_node(SyntaxNodePtr::new(path_expr.syntax()))?;
    match hir::resolutions(db, item).get(expr)? {
        Resolution::Local(binding) => {
            let name_ptr = source_map.node_for_binding(*binding)?;
            // The name's node is the focus; its enclosing let/param is the
            // full range.
            let name_node = name_ptr.to_node(&root);
            let full_range = name_node
                .ancestors()
                .find(|n| {
                    matches!(n.kind(), SyntaxKind::LET_STMT | SyntaxKind::PARAM)
                })
                .map(|n| n.text_range())
                .unwrap_or_else(|| name_ptr.text_range());
            Some(NavigationTarget {
                file,
                full_range,
                focus_range: name_ptr.text_range(),
            })
        }
        // Ambiguous: jump to the first definition — better than going dead.
        Resolution::Item(loc) | Resolution::Ambiguous(loc) => {
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
        // Builtins have no source to jump to.
        Resolution::Builtin(_) => None,
    }
}
