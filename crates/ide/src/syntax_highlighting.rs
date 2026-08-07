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
    /// A trait name, wherever one can be written: the `trait N = ...`
    /// declaration, a bound (`T: Display`), an impl head in either home
    /// (`impl Display` in a type's chain, `impl Write`'s own chain) and the
    /// base of a qualified member call (`Display::fmt`). Renders through the
    /// LSP `interface` token type — traits ARE the interface concept, and
    /// clients that don't style `interface` fall back to `type`.
    Trait,
    /// A generic parameter — a type param or a const param — at its binder
    /// declaration (`fn::<T: Display>`, `struct::<T>`, `fn::<const N: usize>`)
    /// and at every use. LSP `typeParameter`.
    TypeParameter,
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
    if token.kind() == SyntaxKind::IDENT {
        return classify_ident(db, file, root, token);
    }
    Some((lexical_tag(token.kind())?, HlMods::NONE))
}

/// The purely lexical classes — everything a token's KIND alone decides.
///
/// Keyword-ness is asked of [`SyntaxKind::is_keyword`], which the syntax
/// crate generates from its one canonical keyword table, so a keyword added
/// to the language highlights here with no edit to this file. Enumerating
/// keyword kinds by hand is what left `trait`/`requires` (and before them
/// any other new keyword) silently unstyled; `crate::tests`' drift guard
/// pins that this stays table-driven.
pub(crate) fn lexical_tag(kind: SyntaxKind) -> Option<HlTag> {
    use SyntaxKind::*;
    if kind.is_keyword() {
        return Some(HlTag::Keyword);
    }
    Some(match kind {
        COMMENT => HlTag::Comment,
        // A character literal is string-like, and colored like one: the
        // palette has no character class, and every editor theme already
        // paints quoted text the same way whichever quote it is.
        STRING | CHAR => HlTag::String,
        INT_NUMBER => HlTag::Number,
        PLUS | MINUS | STAR | SLASH | EQ | THIN_ARROW | FAT_ARROW | AMP | EQ2 | NEQ | L_ANGLE
        | R_ANGLE | LTEQ | GTEQ => HlTag::Operator,
        // `!` only exists as the never type today.
        BANG => HlTag::Type,
        // Punctuation (braces, parens, `;`, `,`, `:`) stays unstyled, and
        // identifiers are classified through hir, not by kind.
        _ => return None,
    })
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
            let item = hir::checkable_item_at(db, file, &owner)?;
            let tag = if item_is_fn(db, item) {
                HlTag::Function
            } else {
                HlTag::Variable
            };
            Some((tag, HlMods(HlMods::DECLARATION | HlMods::STATIC)))
        }
        // A `type` item's name declaration is a type, through and through.
        (NAME, TYPE_ITEM) => Some((HlTag::Type, HlMods(HlMods::DECLARATION))),
        // A `trait` item's name declaration — the one place a trait is
        // BORN; every other trait mention (bounds, impl heads, qualified
        // call bases) resolves back to it and renders the same class.
        (NAME, TRAIT_ITEM) => Some((HlTag::Trait, HlMods(HlMods::DECLARATION))),
        // A member's name declaration. All three homes are the same thing
        // and render alike: an inherent member (`impl Self { ... }`), a
        // trait requirement (`requires { fmt: fn(...); }`) and an impl's
        // definition of one (`impl Display { fmt = fn... }`).
        (NAME, MEMBER) => Some((HlTag::Function, HlMods(HlMods::DECLARATION))),
        // A generic parameter's binder declaration — a type param (`T`,
        // bounds and all) or a const param (`const N: usize`). Both are
        // binder-supplied names fixed per instantiation, not runtime
        // values, so both take the generic-parameter class.
        (NAME, TYPE_PARAM) | (NAME, CONST_PARAM) => {
            Some((HlTag::TypeParameter, HlMods(HlMods::DECLARATION)))
        }
        // A fn TYPE's parameter name (`unsafe fn(buf: u8.&raw mut, ...)`).
        // It binds nothing — it is a signature's spelling — but it is a
        // parameter name all the same, and reading it unstyled next to
        // every other `fn`'s parameters is just a hole. (A colon-declared
        // member's signature reaches the same class through `BIND_PAT`
        // below, for the same reason.)
        (NAME, PARAM) => Some((HlTag::Parameter, HlMods(HlMods::DECLARATION))),
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
            let is_param = parent.ancestors().any(|n| n.kind() == PARAM);
            let binding = hir::checkable_item_at(db, file, &owner).and_then(|item| {
                let (_, source_map) = hir::body_with_source_map(db, item);
                Some((
                    item,
                    source_map.binding_for_node(SyntaxNodePtr::new(&parent))?,
                ))
            });
            let Some((item, binding)) = binding else {
                // No binding behind it: a requirement signature's parameter
                // names (`push: fn(s: str, w: Self);`) are a signature's
                // spelling, not a body's bindings — but they are parameter
                // names all the same, and reading them unstyled next to
                // every other `fn`'s parameters is just a hole.
                return is_param.then_some((HlTag::Parameter, HlMods(HlMods::DECLARATION)));
            };
            let (body, _) = hir::body_with_source_map(db, item);
            let mut mods = HlMods::DECLARATION;
            if body.bindings[binding].mutable {
                mods |= HlMods::MUTABLE;
            }
            let tag = if is_param {
                HlTag::Parameter
            } else {
                HlTag::Variable
            };
            Some((tag, HlMods(mods)))
        }
        // `Foo` in a newtype-unwrapping pattern (`let Foo(x) = ...`):
        // exactly the type name a construction call's callee is.
        (NAME_REF, NEWTYPE_PAT) => Some(classify_type_position(db, file, &parent)),
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
            Some(classify_type_position(db, file, &parent))
        }
        // The constrained name of a `with`/`requires` clause (`T: Bound`,
        // `Self: Iterator` — both reserved): a type mention like any other,
        // so the same classifier answers it. The clause's BOUNDS are
        // `PATH_TYPE`s and land in the arm below.
        (NAME_REF, WITH_CLAUSE) | (NAME_REF, REQUIRES_CLAUSE) => {
            Some(classify_type_position(db, file, &parent))
        }
        // Type position: user-declared types render as plain types, the
        // builtins (`usize`, `str`, ...) keep their library modifier, an
        // in-scope binder name is a generic parameter and a trait name is a
        // trait — which is what a bound (`T: Display`), an impl head
        // (`impl Display`) and a trait-alias RHS all are. The variant
        // segment of `Shape::Circle` is an enum member — the position alone
        // says so, resolved or not (diagnostics carry the news, same stance
        // as unknown type names).
        (NAME_REF, PATH_TYPE) => {
            if is_variant_segment(&parent) {
                return Some((HlTag::EnumMember, HlMods::NONE));
            }
            Some(classify_type_position(db, file, &parent))
        }
        // A named generic argument's name (`Self` in
        // `Display::<Self = Foo>::fmt`): it names the trait's `Self`
        // parameter, so it reads like the type name it stands for.
        (NAME_REF, NAMED_ARG) => Some(classify_type_name(db, file, token.text())),
        // The sigil's sole segment (`::Circle` in expression position) is
        // an enum member, exactly as the second segment of the qualified
        // spelling is — the node kind alone says so.
        (NAME_REF, ELIDED_VARIANT_EXPR) => Some((HlTag::EnumMember, HlMods::NONE)),
        (NAME_REF, PATH_EXPR) => {
            if is_variant_segment(&parent) {
                // A qualified MEMBER path wears the same two-segment shape
                // as an enum's `Shape::Circle` but names a FUNCTION: the
                // type's own member (`Point::len`) or a trait's
                // (`Display::fmt`, in either form). What tells the two
                // shapes apart is the resolution, not the spelling.
                if qualified_member_segment(db, file, &owner) {
                    return Some((HlTag::Function, HlMods::NONE));
                }
                return Some((HlTag::EnumMember, HlMods::NONE));
            }
            // Inside a `type` declaration's RHS every "expression" is
            // really type syntax (`type Foo = struct { x: usize };`) — but
            // a `with`-chain's member bodies are real expression code, so
            // they are exempt.
            if owner.ancestors().any(|n| n.kind() == TYPE_ITEM)
                && !owner.ancestors().any(|n| n.kind() == WITH_GROUP)
            {
                return Some(classify_type_position(db, file, &parent));
            }
            let item = hir::checkable_item_at(db, file, &owner)?;
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
                hir::Resolution::Item(ref loc) | hir::Resolution::Ambiguous(ref loc) => {
                    let infer = hir::infer::infer(db, item);
                    // A GENERIC item's mention is the base of an
                    // application (`show::<usize>(42)`), and inference
                    // types that base with the uninstantiated scheme rather
                    // than a `Ty::Fn` — so the item's own signature is what
                    // says "function" for it.
                    let is_fn = matches!(infer.type_of_expr.get(expr), Some(hir::Ty::Fn(_)))
                        || matches!(hir::signature(db, loc.to_id(db)), hir::Ty::Fn(_));
                    let tag = if is_fn {
                        HlTag::Function
                    } else {
                        HlTag::Variable
                    };
                    Some((tag, HlMods(HlMods::STATIC)))
                }
                // A construction head (`Foo(...)`) — or a stray value use,
                // which the diagnostics call out; either way the name *is*
                // a type.
                hir::Resolution::TypeItem(_) => Some((HlTag::Type, HlMods::NONE)),
                // A qualified call's base (`Display::fmt(w, x)`) — the one
                // expression position a trait name may appear in.
                hir::Resolution::TraitItem(_) => Some((HlTag::Trait, HlMods::NONE)),
                // A const param: a binder-supplied name, fixed per
                // instantiation — the same generic-parameter class its
                // binder declaration wears, not a runtime parameter.
                hir::Resolution::ConstParam(_) => Some((HlTag::TypeParameter, HlMods::NONE)),
                hir::Resolution::Builtin(_) => {
                    Some((HlTag::Function, HlMods(HlMods::DEFAULT_LIBRARY)))
                }
            }
        }
        // The field name of a dot-call that resolved to a member: a
        // function. Both dispatch shapes count — the STRUCTURAL one (an
        // inherent member, or a trait impl's member on a concrete receiver)
        // and the BOUND-DIRECTED one (`x.fmt(w)` on a rigid `T: Display`,
        // which goes through the body's hidden dictionary). A reader
        // shouldn't have to know which machinery answered. (Plain field
        // accesses stay unstyled, as before.)
        (NAME_REF, FIELD_EXPR) => {
            let item = hir::checkable_item_at(db, file, &owner)?;
            let (_, source_map) = hir::body_with_source_map(db, item);
            let call = owner.parent().filter(|p| p.kind() == CALL_EXPR)?;
            let call_expr = source_map.expr_for_node(SyntaxNodePtr::new(&call))?;
            let infer = hir::infer::infer(db, item);
            let resolved = infer.member_of_expr.get(call_expr).is_some()
                || infer.bound_member_of_expr.get(call_expr).is_some();
            resolved.then_some((HlTag::Function, HlMods::NONE))
        }
        // Unresolved or junk: leave it plain; diagnostics carry the news.
        _ => None,
    }
}

/// Whether a two-segment path expression names a MEMBER — the type's own
/// (`Point::len`) or a trait's (`Display::fmt`, in either form: the
/// qualified short form and the named-`Self` spelling
/// `Display::<Self = Foo>::fmt`) — rather than an enum variant. Both shapes
/// are `Name::name`, so only the resolution tells them apart.
fn qualified_member_segment(db: &RootDatabase, file: SourceFile, path: &SyntaxNode) -> bool {
    let Some(item) = hir::checkable_item_at(db, file, path) else {
        return false;
    };
    let (_, source_map) = hir::body_with_source_map(db, item);
    let Some(expr) = source_map.expr_for_node(SyntaxNodePtr::new(path)) else {
        return false;
    };
    let infer = hir::infer::infer(db, item);
    if infer.member_value_of_expr.get(expr).is_some() {
        return true;
    }
    // A directly-called qualified form resolves on the CALL — dispatched
    // to an impl member, or through the enclosing dictionary.
    path.parent()
        .filter(|p| p.kind() == SyntaxKind::CALL_EXPR)
        .and_then(|call| source_map.expr_for_node(SyntaxNodePtr::new(&call)))
        .is_some_and(|call| {
            infer.qualified_member_of_expr.get(call).is_some()
                || infer.bound_member_of_expr.get(call).is_some()
        })
}

/// The identifier a `NAME`/`NAME_REF` node wraps (`None` for the `_` hole).
fn ident_text(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == SyntaxKind::IDENT)
        .map(|it| it.text().to_owned())
}

/// Whether a generic binder in scope declares `name`. Walks the ancestors
/// and inspects each one's own `GENERIC_PARAM_LIST` child, which is how
/// EVERY binder home spells itself — fn literals, member fn signatures,
/// `struct::<T>`/`enum::<T>` type literals, `requires ::<...>`,
/// `with ::<...>` — so this file names none of them and a future binder
/// home is covered the day it parses.
///
/// This re-derives, syntactically, a scoping rule hir already owns
/// (`ParamScope`/`own_generics`) — until hir maps type refs back to a
/// source node, there is no query to ask instead, so the highlighter
/// walks the tree itself. Move this to a hir query the day one exists.
fn binder_declares(node: &SyntaxNode, name: &str) -> bool {
    use SyntaxKind::*;
    node.ancestors().any(|ancestor| {
        let mut lists: Vec<SyntaxNode> = ancestor
            .children()
            .filter(|c| c.kind() == GENERIC_PARAM_LIST)
            .collect();
        // A `type`/`trait` declaration writes its OWN binder on its RHS
        // literal (`type Pair = struct::<T> { ... }`), one level down — and
        // that binder stays in scope across the whole declaration, the
        // trailing `with`-chain's members included, even though they are
        // siblings of the literal rather than children of it. A `with`
        // group's own binder is scoped to that group, which is an ancestor
        // in its own right whenever we are inside it, so it is skipped here
        // instead of leaking to its siblings.
        if matches!(ancestor.kind(), TYPE_ITEM | TRAIT_ITEM) {
            lists.extend(
                ancestor
                    .children()
                    .filter(|c| c.kind() != WITH_GROUP)
                    .flat_map(|c| c.children())
                    .filter(|c| c.kind() == GENERIC_PARAM_LIST),
            );
        }
        lists
            .iter()
            .flat_map(|list| list.children())
            .filter(|p| matches!(p.kind(), TYPE_PARAM | CONST_PARAM))
            .filter_map(|p| p.children().find(|c| c.kind() == NAME))
            .any(|n| ident_text(&n).as_deref() == Some(name))
    })
}

/// A name written in type position: a generic parameter when a binder in
/// scope declares it, otherwise the name of a type or trait.
fn classify_type_position(
    db: &RootDatabase,
    file: SourceFile,
    name_ref: &SyntaxNode,
) -> (HlTag, HlMods) {
    let Some(text) = ident_text(name_ref) else {
        return (HlTag::Type, HlMods(HlMods::DEFAULT_LIBRARY));
    };
    if binder_declares(name_ref, &text) {
        return (HlTag::TypeParameter, HlMods::NONE);
    }
    classify_type_name(db, file, &text)
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

/// A name used as a type: a `type` item reference, a trait name (bounds,
/// impl heads, alias RHSs), or a builtin type name with the library
/// modifier. Unknown names still color as types — that's what the position
/// says they were meant to be; diagnostics carry the news.
fn classify_type_name(db: &RootDatabase, file: SourceFile, name: &str) -> (HlTag, HlMods) {
    // `Self` NAMES the enclosing type (TR01), so it reads
    // as that type: a plain type, not a keyword and not a builtin. It has
    // no file-scope entry of its own, which is why it needs saying here —
    // otherwise it would fall through to the `defaultLibrary` case and
    // render like `usize`.
    if name == "Self" {
        return (HlTag::Type, HlMods::NONE);
    }
    match hir::file_scope(db, file).resolve(name) {
        Some(hir::Resolution::TypeItem(_)) => (HlTag::Type, HlMods::NONE),
        Some(hir::Resolution::TraitItem(_)) => (HlTag::Trait, HlMods::NONE),
        _ => (HlTag::Type, HlMods(HlMods::DEFAULT_LIBRARY)),
    }
}

/// Push the token's range, split at line breaks: LSP clients aren't required
/// to handle multiline tokens, and Must strings and block comments both span
/// lines.
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
