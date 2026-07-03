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

    // A name in *type* position (`p: Foo`) resolves through the file scope
    // directly — annotations aren't body expressions, so there is no
    // resolution entry to look up.
    if name_ref
        .syntax()
        .parent()
        .is_some_and(|p| p.kind() == SyntaxKind::PATH_TYPE)
    {
        let resolution = hir::file_scope(db, file).resolve(&name_ref.text())?;
        let Resolution::TypeItem(loc) = resolution else {
            // Builtin types have no source; a value item in type position
            // is not a definition to jump to (a diagnostic already says
            // it's not a type).
            return None;
        };
        return nav_to_item(db, &loc);
    }

    let path_expr = name_ref.syntax().parent().and_then(ast::PathExpr::cast)?;

    // Which item are we inside?
    let item_node = path_expr
        .syntax()
        .ancestors()
        .find(|n| ast::Item::can_cast(n.kind()))?;
    let item_index = root
        .children()
        .filter(|n| ast::Item::can_cast(n.kind()))
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
        Resolution::Item(loc) | Resolution::Ambiguous(loc) | Resolution::TypeItem(loc) => {
            nav_to_item(db, loc)
        }
        // Builtins have no source to jump to.
        Resolution::Builtin(_) => None,
    }
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
