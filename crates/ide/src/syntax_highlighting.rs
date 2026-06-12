//! Full-file highlighting. With no tree-sitter grammar in the editor, this
//! is the *only* coloring source, so it covers the lexical classes too —
//! but names are classified through hir: a reference renders as a function
//! because inference says its type is `fn(...)`, not because of a textual
//! heuristic.

use base_db::{RootDatabase, SourceFile, parse};
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, SyntaxToken, TextRange, TextSize};

/// A classified range. Ranges are sorted, non-overlapping, and never span a
/// line break (multiline tokens are split) — exactly what the LSP
/// semantic-token delta encoding wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlRange {
    pub range: TextRange,
    pub tag: HlTag,
    pub mods: HlMods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlTag {
    Comment,
    String,
    Number,
    Keyword,
    Operator,
    Function,
    Variable,
    Parameter,
    Type,
}

/// Modifier bitset. Bit positions are public API: the LSP legend lists its
/// modifiers in this order (`to_proto` in `must-lsp` must stay in sync).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlMods(pub u32);

impl HlMods {
    pub const NONE: HlMods = HlMods(0);
    pub const DECLARATION: u32 = 1 << 0;
    pub const STATIC: u32 = 1 << 1;
    pub const DEFAULT_LIBRARY: u32 = 1 << 2;

    pub fn contains(self, flag: u32) -> bool {
        self.0 & flag != 0
    }
}

pub(crate) fn highlight(db: &RootDatabase, file: SourceFile) -> Vec<HlRange> {
    let root = parse(db, file).syntax_node();
    let mut result = Vec::new();
    for element in root.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        let Some((tag, mods)) = classify(db, file, &root, &token) else {
            continue;
        };
        push_line_split(&mut result, &token, tag, mods);
    }
    result
}

fn classify(
    db: &RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    token: &SyntaxToken,
) -> Option<(HlTag, HlMods)> {
    use SyntaxKind::*;
    let tag = match token.kind() {
        COMMENT => HlTag::Comment,
        STRING => HlTag::String,
        INT_NUMBER => HlTag::Number,
        FN_KW | STATIC_KW | CONST_KW | LET_KW | IF_KW | ELSE_KW | TRUE_KW | FALSE_KW => {
            HlTag::Keyword
        }
        PLUS | MINUS | STAR | SLASH | EQ | THIN_ARROW | AMP | EQ2 | NEQ | L_ANGLE | R_ANGLE
        | LTEQ | GTEQ => HlTag::Operator,
        // `!` only exists as the never type today.
        BANG => HlTag::Type,
        IDENT => return classify_ident(db, file, root, token),
        // Punctuation (braces, parens, `;`, `,`, `:`) stays unstyled.
        _ => return None,
    };
    Some((tag, HlMods::NONE))
}

fn classify_ident(
    db: &RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    token: &SyntaxToken,
) -> Option<(HlTag, HlMods)> {
    use SyntaxKind::*;
    let parent = token.parent()?;
    let owner = parent.parent()?;
    match (parent.kind(), owner.kind()) {
        (NAME, STATIC_ITEM) => {
            let item = item_of(db, file, root, &owner)?;
            let tag = if item_is_fn(db, item) {
                HlTag::Function
            } else {
                HlTag::Variable
            };
            Some((tag, HlMods(HlMods::DECLARATION | HlMods::STATIC)))
        }
        (NAME, PARAM) => Some((HlTag::Parameter, HlMods(HlMods::DECLARATION))),
        (NAME, LET_STMT) => Some((HlTag::Variable, HlMods(HlMods::DECLARATION))),
        // All nameable types are builtin for now (`usize`, `str`, ...).
        (NAME_REF, PATH_TYPE) => Some((HlTag::Type, HlMods(HlMods::DEFAULT_LIBRARY))),
        (NAME_REF, PATH_EXPR) => {
            let item = item_of(db, file, root, &owner)?;
            let (_, source_map) = hir::body_with_source_map(db, item);
            let expr = source_map.expr_for_node(SyntaxNodePtr::new(&owner))?;
            match *hir::resolutions(db, item).get(expr)? {
                hir::Resolution::Local(binding) => {
                    let def = source_map.node_for_binding(binding)?.to_node(root);
                    let is_param = def.kind() == PARAM
                        || def.parent().is_some_and(|p| p.kind() == PARAM);
                    let tag = if is_param {
                        HlTag::Parameter
                    } else {
                        HlTag::Variable
                    };
                    Some((tag, HlMods::NONE))
                }
                hir::Resolution::Item(_) | hir::Resolution::Ambiguous(_) => {
                    let infer = hir::infer::infer(db, item);
                    let tag = match infer.type_of_expr.get(expr) {
                        Some(hir::Ty::Fn(_)) => HlTag::Function,
                        _ => HlTag::Variable,
                    };
                    Some((tag, HlMods(HlMods::STATIC)))
                }
                hir::Resolution::Builtin(_) => {
                    Some((HlTag::Function, HlMods(HlMods::DEFAULT_LIBRARY)))
                }
            }
        }
        // Unresolved or junk: leave it plain; diagnostics carry the news.
        _ => None,
    }
}

/// Whether the item's value is function-typed (its name then colors as a
/// function at the definition, matching how references to it color).
fn item_is_fn(db: &RootDatabase, item: hir::ItemId<'_>) -> bool {
    let Some(root) = hir::body::body(db, item).root else {
        return false;
    };
    matches!(
        hir::infer::infer(db, item).type_of_expr.get(root),
        Some(hir::Ty::Fn(_))
    )
}

fn item_of<'db>(
    db: &'db RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    node: &SyntaxNode,
) -> Option<hir::ItemId<'db>> {
    let item_node = node
        .ancestors()
        .find(|n| n.kind() == SyntaxKind::STATIC_ITEM)?;
    let index = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::STATIC_ITEM)
        .position(|n| n == item_node)?;
    hir::file_item_ids(db, file).get(index).copied()
}

/// Push the token's range, split at line breaks: LSP clients aren't required
/// to handle multiline tokens, and Must strings (and future comments) can
/// span lines.
fn push_line_split(acc: &mut Vec<HlRange>, token: &SyntaxToken, tag: HlTag, mods: HlMods) {
    let text = token.text();
    let base = token.text_range().start();
    let mut line_start = 0;
    for (newline, _) in text.match_indices('\n') {
        let line_end = newline - if text[..newline].ends_with('\r') { 1 } else { 0 };
        if line_end > line_start {
            acc.push(HlRange {
                range: TextRange::new(
                    base + TextSize::new(line_start as u32),
                    base + TextSize::new(line_end as u32),
                ),
                tag,
                mods,
            });
        }
        line_start = newline + 1;
    }
    if text.len() > line_start {
        acc.push(HlRange {
            range: TextRange::new(
                base + TextSize::new(line_start as u32),
                base + TextSize::new(text.len() as u32),
            ),
            tag,
            mods,
        });
    }
}
