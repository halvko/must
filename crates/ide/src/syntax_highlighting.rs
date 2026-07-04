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
    /// An enum variant: its declaration inside an `enum` literal, and the
    /// second segment of a `Shape::Circle` path.
    EnumMember,
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
    pub const MUTABLE: u32 = 1 << 3;

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
        FN_KW | STATIC_KW | CONST_KW | TYPE_KW | STRUCT_KW | ENUM_KW | LET_KW | MUT_KW | IF_KW
        | ELSE_KW | MATCH_KW | LOOP_KW | BREAK_KW | CONTINUE_KW | TRUE_KW | FALSE_KW => {
            HlTag::Keyword
        }
        PLUS | MINUS | STAR | SLASH | EQ | THIN_ARROW | FAT_ARROW | AMP | EQ2 | NEQ | L_ANGLE
        | R_ANGLE | LTEQ | GTEQ => HlTag::Operator,
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
    use syntax::ast::{self, AstNode};
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
        // A `type` item's name declaration is a type, through and through.
        (NAME, TYPE_ITEM) => Some((HlTag::Type, HlMods(HlMods::DECLARATION))),
        // A variant declared inside an `enum` literal.
        (NAME, ENUM_VARIANT) => Some((HlTag::EnumMember, HlMods(HlMods::DECLARATION))),
        // A payload binding in a variant pattern declares a plain local.
        (NAME, VARIANT_PAT) => Some((HlTag::Variable, HlMods(HlMods::DECLARATION))),
        // A binding declared by a `let`/parameter pattern: a bare name
        // (`BIND_PAT`, nested under `PARAM`/`LET_STMT` directly, inside a
        // `NEWTYPE_PAT`, or as a match-arm pattern) or one field of a
        // record-destructuring pattern (`RECORD_PAT_FIELD` — the shorthand
        // field name doubles as its own binding's declaration; a rename's
        // field-name token is a second `NAME` child that is *not* bound,
        // and falls out of `binding_for_node` returning `None` for it).
        // Colors as a parameter when a `PARAM` sits somewhere above the
        // pattern, a plain local otherwise. A bare `BIND_PAT` name is
        // always a binding now (never reinterpreted as a variant), so it
        // always falls through to the binding/parameter coloring.
        (NAME, BIND_PAT) | (NAME, RECORD_PAT_FIELD) => {
            let item = item_of(db, file, root, &owner)?;
            let (body, source_map) = hir::body_with_source_map(db, item);
            let binding = source_map.binding_for_node(SyntaxNodePtr::new(&parent))?;
            let mut mods = HlMods::DECLARATION;
            if body.bindings[binding].mutable {
                mods |= HlMods::MUTABLE;
            }
            let tag = if parent.ancestors().any(|n| n.kind() == PARAM) {
                HlTag::Parameter
            } else {
                HlTag::Variable
            };
            Some((tag, HlMods(mods)))
        }
        // `Foo` in a newtype-unwrapping pattern (`let Foo(x) = ...`):
        // exactly the type name a construction call's callee is.
        (NAME_REF, NEWTYPE_PAT) => Some(classify_type_name(db, file, token.text())),
        // A variant pattern's path: the variant segment is an enum member
        // (the position says so, resolved or not); the qualified spelling's
        // enum segment is a type name. Decided via the `ast::VariantPat`
        // helpers rather than position-counting, so the elided sigil shape
        // (`::Circle`, one `NameRef` after the `COLON2`) classifies its
        // sole segment as the variant, not as a qualifying type.
        (NAME_REF, VARIANT_PAT) => {
            let variant_pat = ast::VariantPat::cast(owner.clone())?;
            if variant_pat
                .variant_name_ref()
                .is_some_and(|v| v.syntax() == &parent)
            {
                return Some((HlTag::EnumMember, HlMods::NONE));
            }
            Some(classify_type_name(db, file, token.text()))
        }
        // Type position: user-declared types render as plain types, the
        // builtins (`usize`, `str`, ...) keep their library modifier. The
        // variant segment of `Shape::Circle` is an enum member — the
        // position alone says so, resolved or not (diagnostics carry the
        // news, same stance as unknown type names).
        (NAME_REF, PATH_TYPE) => {
            if is_variant_segment(&parent) {
                return Some((HlTag::EnumMember, HlMods::NONE));
            }
            Some(classify_type_name(db, file, token.text()))
        }
        (NAME_REF, PATH_EXPR) => {
            if is_variant_segment(&parent) {
                return Some((HlTag::EnumMember, HlMods::NONE));
            }
            // Inside a `type` declaration's RHS every "expression" is
            // really type syntax (`type Foo = struct { x: usize };`), so
            // names there classify as type names, not values.
            if owner.ancestors().any(|n| n.kind() == TYPE_ITEM) {
                return Some(classify_type_name(db, file, token.text()));
            }
            let item = item_of(db, file, root, &owner)?;
            let (body, source_map) = hir::body_with_source_map(db, item);
            // The base of a `::` path has its own expression on the
            // segment's node; a plain path sits on the whole path node.
            let expr = source_map
                .expr_for_node(SyntaxNodePtr::new(&parent))
                .or_else(|| source_map.expr_for_node(SyntaxNodePtr::new(&owner)))?;
            match *hir::resolutions(db, item).get(expr)? {
                hir::Resolution::Local(binding) => {
                    let def = source_map.node_for_binding(binding)?.to_node(root);
                    let is_param = def.ancestors().any(|n| n.kind() == PARAM);
                    let tag = if is_param {
                        HlTag::Parameter
                    } else {
                        HlTag::Variable
                    };
                    let mods = if body.bindings[binding].mutable {
                        HlMods::MUTABLE
                    } else {
                        0
                    };
                    Some((tag, HlMods(mods)))
                }
                hir::Resolution::Item(_) | hir::Resolution::Ambiguous(_) => {
                    let infer = hir::infer::infer(db, item);
                    let tag = match infer.type_of_expr.get(expr) {
                        Some(hir::Ty::Fn(_)) => HlTag::Function,
                        _ => HlTag::Variable,
                    };
                    Some((tag, HlMods(HlMods::STATIC)))
                }
                // A construction head (`Foo(...)`) — or a stray value use,
                // which the diagnostics call out; either way the name *is*
                // a type.
                hir::Resolution::TypeItem(_) => Some((HlTag::Type, HlMods::NONE)),
                // A const param reads like an immutable parameter.
                hir::Resolution::ConstParam(_) => Some((HlTag::Parameter, HlMods::NONE)),
                hir::Resolution::Builtin(_) => {
                    Some((HlTag::Function, HlMods(HlMods::DEFAULT_LIBRARY)))
                }
            }
        }
        // Unresolved or junk: leave it plain; diagnostics carry the news.
        _ => None,
    }
}

/// Whether `name_ref` is the *second* segment of a two-segment `::` path
/// (`Circle` in `Shape::Circle`), in expression or type position.
fn is_variant_segment(name_ref: &SyntaxNode) -> bool {
    let Some(parent) = name_ref.parent() else {
        return false;
    };
    parent
        .children()
        .filter(|n| n.kind() == SyntaxKind::NAME_REF)
        .nth(1)
        .is_some_and(|second| second == *name_ref)
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

/// A name used as a type: a `type` item reference, or a builtin type name
/// with the library modifier. Unknown names still color as types — that's
/// what the position says they were meant to be; diagnostics carry the news.
fn classify_type_name(db: &RootDatabase, file: SourceFile, name: &str) -> (HlTag, HlMods) {
    match hir::file_scope(db, file).resolve(name) {
        Some(hir::Resolution::TypeItem(_)) => (HlTag::Type, HlMods::NONE),
        _ => (HlTag::Type, HlMods(HlMods::DEFAULT_LIBRARY)),
    }
}

fn item_of<'db>(
    db: &'db RootDatabase,
    file: SourceFile,
    root: &SyntaxNode,
    node: &SyntaxNode,
) -> Option<hir::ItemId<'db>> {
    let item_node = node.ancestors().find(is_item)?;
    let index = root
        .children()
        .filter(is_item)
        .position(|n| n == item_node)?;
    hir::file_item_ids(db, file).get(index).copied()
}

/// Whether the node is a top-level item — the positional index over these
/// must match `hir::file_item_ids`, which counts *all* item kinds.
fn is_item(node: &SyntaxNode) -> bool {
    matches!(node.kind(), SyntaxKind::STATIC_ITEM | SyntaxKind::TYPE_ITEM)
}

/// Push the token's range, split at line breaks: LSP clients aren't required
/// to handle multiline tokens, and Must strings (and future comments) can
/// span lines.
fn push_line_split(acc: &mut Vec<HlRange>, token: &SyntaxToken, tag: HlTag, mods: HlMods) {
    let text = token.text();
    let base = token.text_range().start();
    let mut line_start = 0;
    for (newline, _) in text.match_indices('\n') {
        let line_end = newline
            - if text[..newline].ends_with('\r') {
                1
            } else {
                0
            };
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
