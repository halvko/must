//! Completions: a speculative-parse core over the real file's memoized
//! queries.
//!
//! The request splices a marker identifier into a *copy* of the real text at
//! the cursor, parses the copy with the pure [`syntax::parse`] (never a
//! salsa input — this runs on a read-only [`crate::Analysis`] snapshot), and
//! classifies the marker's position from its ancestor chain in that
//! speculative tree. Candidates then come from the REAL file's memoized
//! queries ([`hir::file_scope`], [`hir::type_scope`], [`hir::expr_scopes`],
//! [`hir::infer::infer`]): only the *position* is speculative, every fact
//! about the program is real.
//!
//! Back-mapping from the speculative tree to the real one leans on one
//! property: a pure insertion changes no *text* before the cursor, so any
//! real-tree node whose range ends at-or-before the cursor is byte-for-byte
//! identical to its speculative counterpart. Local-variable scope lookup only
//! needs this for an *offset* (via [`real_anchor`]); member and pattern
//! positions need it for whole *expressions* — a `.` receiver, a `match`
//! scrutinee — so [`expr_for_range`] generalizes it to a range: it climbs from
//! the range's start token until it finds the real-tree node the body source
//! map actually registered (skipping wrapper nodes like `NameRef`/`ExprStmt`
//! that share the same range but were never themselves passed to
//! `alloc_expr`).
//!
//! ## Ranking
//!
//! One lexicographic sort key, three components: `"{type}_{provenance}_
//! {label}"`.
//!
//! The leading TYPE tier (one digit) is what the position's *expected
//! type* says about a candidate — read from
//! [`hir::InferenceResult::expectation_of_expr`] via [`expected_type_at`],
//! never reconstructed syntactically (no expectation ⇒ every candidate
//! ties at the worst tier and the provenance order below decides alone):
//! `0` — the candidate's type equals the expectation (a `fn`-typed
//! candidate matches a `fn`-typed expectation as a value: *passing*, not
//! calling); `1` — it widens to it ([`hir::widens_to`]: variant→its enum,
//! `!`→anything), or it is a `fn` whose *return* type matches/widens
//! (calling it would satisfy the position); `2` — no match, or nothing
//! known (keywords always; any candidate without an expectation).
//!
//! [`Provenance`] tiers are two digits wide and spaced by tens (`00`, `10`,
//! `20`, …), not consecutive integers: `Gold` — a context's single best
//! answer, when it can prove one (an *uncovered* `match` variant, a local
//! whose name matches a missing record-literal field) — sits in the gap
//! below every general-purpose tier, so it always sorts first without
//! renumbering `Local`/`Item`/`Builtin`/`Keyword`. Gold candidates keep
//! their provenance *and* get the type tier applied like everyone else
//! (an uncovered variant under an enum-typed scrutinee trivially widens —
//! tier `1` — so gold stays ahead within its context).
//!
//! ## Snippets
//!
//! A handful of candidates carry an [`InsertText::Snippet`] instead of
//! plain text: a parameterful `fn` item/builtin in expression position
//! (`name($1)`, unless the position expects the `fn` itself as a *value*,
//! never called — then it's the bare name), a `type X = …` RHS
//! (`struct { $1 }` / `enum { $1 }`), a payload-carrying variant pattern in
//! a match arm (`::Circle($1)` / `Circle($1)`), and a missing record-literal
//! field (`x: $1`). Every snippet carries its own plain fallback for a
//! client without `snippetSupport` — `must-lsp::to_proto` picks between the
//! two per client capability; `ide` itself has no notion of "the client",
//! only the two spellings.
//!
//! ## Detail, and why there is no `completionItem/resolve`
//!
//! Every candidate carries its `detail` eagerly, computed in one pass here:
//! the fully rendered type for a value/`fn`/local (`fn_call_insert`'s
//! candidates and `local_detail`), the declaration shape for a `type` item
//! (`type_item_detail` — a `struct { … }`/`enum { … }` body, the type-item
//! provenance its `Struct`/`Enum` kind already names), a variant's payload
//! signature for a variant candidate (`hover::render_variant`), and the
//! field's own type for a field completion (field access, record-literal
//! fields, and record-pattern fields alike). All of it is already
//! memoized on the real snapshot (`infer`, `type_underlying`,
//! `enum_variants`), so it is a cheap read, not lazy work.
//!
//! Lazy `completionItem/resolve` exists to defer *expensive* per-item work
//! (fetching doc comments, computing import edits) off the initial list.
//! Must has neither: no doc comments in the grammar, and no imports. The
//! only thing a resolve pass could fill in is exactly this type detail —
//! already cheap and already eager — so wiring resolve would add a round
//! trip and a server-side item cache to recompute what the first response
//! already carried. `must-lsp` therefore advertises `resolve_provider:
//! Some(false)` and never registers a resolve handler; revisit only if a
//! genuinely lazy field (docs, import edits) ever lands.

use base_db::{RootDatabase, SourceFile, parse};
use syntax::ast::{self, AstNode as _};
use syntax::{SyntaxKind, SyntaxNode, SyntaxNodePtr, TextRange, TextSize};

use crate::FilePosition;

/// The identifier spliced into the speculative copy at the cursor. Deletion-
/// resistant (no editor-typed prefix collides with it) and self-describing
/// in debug dumps.
const MARKER: &str = "mustCompletionMarker";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    /// `"{type_tier}_{provenance:02}_{label}"`: expectation-matching
    /// candidates first (see the module doc's TYPE tier), then gold answers
    /// before locals before items before builtins before keywords,
    /// alphabetically within each tier.
    pub sort_text: String,
    /// Usually `label` (the editor filters client-side). The one exception:
    /// a bare match-arm slot's variant items label/insert the sigil form
    /// `::Variant` but filter on the bare variant name — what the user
    /// actually types.
    pub filter_text: String,
    /// Replaces the typed prefix (if any) with [`InsertText`]: a plain
    /// label most of the time, a tab-stop snippet for the handful of rules
    /// in the module doc's "Snippets" section.
    pub text_edit: CompletionTextEdit,
}

/// A completion's replacement range plus what to insert there. Same shape
/// as [`crate::TextEdit`] (quick fixes' plain-string edit) but the insert
/// side is an [`InsertText`] — plain or snippet — since one completion
/// candidate must carry both spellings for `to_proto` to pick between per
/// client capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTextEdit {
    pub range: TextRange,
    pub insert: InsertText,
}

/// What a completion inserts. `ide` has no `lsp_types` dependency, so
/// this is not `InsertTextFormat` — just the two shapes a client capability
/// check needs to pick between: literal text, or a tab-stop snippet in LSP
/// snippet syntax (`$1`, …) alongside its own plain fallback for a client
/// that lacks snippet support (`to_proto::completion_item` does the
/// picking, gated on `snippet_support` read at `initialize`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertText {
    Plain(String),
    Snippet { snippet: String, plain: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionItemKind {
    Function,
    Variable,
    Field,
    EnumMember,
    Struct,
    Enum,
    Constant,
    Keyword,
}

/// Where a candidate comes from — the sort tier, low to high. See the
/// module doc for why the values are spaced tens apart.
#[derive(Debug, Clone, Copy)]
enum Provenance {
    /// The single best answer a context can *prove*: an uncovered
    /// match-arm variant, or a local whose name matches a missing
    /// record-literal field (the shorthand case, in the flesh).
    Gold = 0,
    Local = 10,
    Item = 20,
    Builtin = 30,
    Keyword = 40,
}

/// The worst TYPE tier: no expectation at the position, no type known for
/// the candidate, or a plain mismatch — all rank the same, leaving the
/// provenance order to decide.
const TYPE_TIER_NONE: u8 = 2;

/// The leading component of the sort key — see the module doc's TYPE tier
/// for the 0/1/2 scheme. Both types arrive fully resolved (expectations by
/// `InferCtx::finish`, candidate types by their own queries), so plain
/// equality is the "same type" test.
fn type_tier(candidate: Option<&hir::Ty>, expected: Option<&hir::Ty>) -> u8 {
    let (Some(candidate), Some(expected)) = (candidate, expected) else {
        return TYPE_TIER_NONE;
    };
    if candidate == expected {
        return 0;
    }
    if hir::widens_to(candidate, expected) {
        return 1;
    }
    // A `fn` candidate that doesn't match as a value (the arms above) may
    // still satisfy the position when *called*.
    if let hir::Ty::Fn(f) = candidate
        && (f.ret == *expected || hir::widens_to(&f.ret, expected))
    {
        return 1;
    }
    TYPE_TIER_NONE
}

fn completion_item(
    label: impl Into<String>,
    kind: CompletionItemKind,
    provenance: Provenance,
    type_tier: u8,
    detail: Option<String>,
    edit_range: TextRange,
) -> CompletionItem {
    let label = label.into();
    CompletionItem {
        sort_text: format!("{type_tier}_{:02}_{label}", provenance as u8),
        filter_text: label.clone(),
        text_edit: CompletionTextEdit {
            range: edit_range,
            insert: InsertText::Plain(label.clone()),
        },
        label,
        kind,
        detail,
    }
}

/// The fn-call snippet for a `fn`-typed candidate (a file item or a
/// builtin): `name($1)` when it takes parameters, `name()` when it's
/// zero-arity — except when `tier` is 0, meaning the candidate's own type
/// (not its return type) matched the expectation: the position wants the
/// `fn` itself as a *value* (passed, not called), so the bare name inserts
/// with no call syntax at all. The zero-arity and snippet-incapable-client
/// cases share one plain spelling (`name()`), computed once here.
fn fn_call_insert(name: &str, fn_ty: &hir::FnTy, tier: u8) -> InsertText {
    if tier == 0 {
        return InsertText::Plain(name.to_owned());
    }
    let plain = format!("{name}()");
    if fn_ty.params.is_empty() {
        InsertText::Plain(plain)
    } else {
        InsertText::Snippet {
            snippet: format!("{name}($1)"),
            plain,
        }
    }
}

/// A local's own completion detail: its inferred type, with a
/// `mut ` prefix when it's declared mutable — mirrors `hover`'s
/// `mut_prefix` convention. Falls back to the bare `mut` marker (or
/// nothing) when the type isn't known, matching what this looked like
/// before the type was added to the mix.
fn local_detail(ty: Option<&hir::Ty>, mutable: bool) -> Option<String> {
    let mut_prefix = if mutable { "mut " } else { "" };
    match ty {
        Some(ty) => Some(format!("{mut_prefix}{}", ty.display())),
        None => mutable.then(|| "mut".to_owned()),
    }
}

/// A `type` item's own completion detail: the underlying shape
/// in short form — mirrors `hover::type_item_hover`'s rendering minus the
/// `type NAME = ` prefix (the label already carries the name). A broken
/// declaration (neither a struct nor an enum shape) falls back to the bare
/// `type` keyword.
fn type_item_detail(db: &RootDatabase, item: hir::ItemId<'_>) -> Option<String> {
    if let Some(underlying) = hir::type_underlying(db, item) {
        return Some(underlying.display());
    }
    if let Some(variants) = hir::enum_variants(db, item).as_ref() {
        return Some(format!(
            "enum {{ {} }}",
            variants
                .iter()
                .map(|(name, payload)| crate::hover::render_variant(name, payload))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some("type".to_owned())
}

/// The marker's classified position.
enum Context {
    /// The nearest classifiable ancestor is a broken item directly under
    /// `SOURCE_FILE` — a bare identifier at the top level.
    ItemKeyword,
    /// The first segment of a `PathType`.
    TypePosition,
    /// The first segment of a single-segment `PathExpr`.
    Expression {
        /// Sits directly under a `BlockExpr` (or an `ExprStmt` that does):
        /// a fresh statement/would-be tail, not nested in another
        /// expression.
        statement_start: bool,
        /// A `LOOP_EXPR` ancestor exists before any enclosing `FN_LITERAL`/
        /// `CONST_BLOCK_EXPR` boundary — mirrors the const/loop-boundary
        /// reasoning `infer.rs` uses for `BreakOutsideLoop`.
        in_loop: bool,
    },
    /// `receiver.NAME`, the marker is the field name. Carries the
    /// receiver's speculative-tree range: it ends before the cursor, so
    /// (per the module doc) it is byte-identical in the real tree, and
    /// [`expr_for_range`] back-maps it straight to an `ExprId`.
    FieldAccess { receiver_range: TextRange },
    /// The second segment of a `::` path, in expression (`PathExpr`)
    /// or type (`PathType`) position. `base_name` is the *first* segment's
    /// text, read straight off the speculative tree — it sits before the
    /// cursor, untouched by the splice, so no back-mapping is needed to
    /// resolve it by name.
    VariantSegment {
        base_name: String,
        in_type_position: bool,
    },
    /// A match arm's own pattern slot: either a bare name (`BindPat`,
    /// `bare: true`) or a `VariantPat` already past a `::` (elided sigil or
    /// `Enum`-qualified, `bare: false`). Carries the scrutinee's
    /// speculative-tree range for the same back-mapping reason as
    /// `FieldAccess`.
    MatchArmPattern {
        scrutinee_range: TextRange,
        bare: bool,
    },
    /// A shorthand field name (`x` meaning `x: x`) inside a record
    /// literal. Carries the literal's own start offset — its opening
    /// token sits before the cursor, so (unlike its not-yet-closed end)
    /// that start back-maps directly to the real tree.
    RecordLiteralField { record_start: TextSize },
    /// A field-name slot inside a record-destructuring pattern
    /// (`struct { <cursor> }` in a `let`/parameter binding, or nested one
    /// level down inside a `Name(…)` newtype pattern). Completes the record
    /// type's field names; the `..` rest, `mut`, and `as`-rename spellings
    /// are all still valid around the completed shorthand binding. Carries
    /// the record pattern's own start offset — its `struct` keyword sits
    /// before the cursor, so that start back-maps directly to the real tree
    /// (the same trick `RecordLiteralField` uses). The `as`-rename slot
    /// (`x as <cursor>`) is a *fresh* binding name and never classifies
    /// here.
    RecordPatternField { record_pat_start: TextSize },
    /// The whole RHS of a `type X = …` declaration: offers exactly
    /// the `struct`/`enum` keywords, not the general expression-position
    /// candidates the marker would otherwise classify as.
    TypeItemRhs,
}

pub(crate) fn completions(
    db: &RootDatabase,
    FilePosition { file, offset }: FilePosition,
) -> Vec<CompletionItem> {
    let real_text = file.text(db);
    let real_root = parse(db, file).syntax_node();
    // A completion request inside a comment or a string literal offers
    // nothing — the token at the cursor in the *real* tree (unaffected by
    // whatever we're about to splice in) already answers this.
    if real_root
        .token_at_offset(offset)
        .any(|t| matches!(t.kind(), SyntaxKind::COMMENT | SyntaxKind::STRING))
    {
        return Vec::new();
    }

    let edit_range = prefix_range(real_text, offset);

    let mut spliced = real_text.clone();
    spliced.insert_str(usize::from(offset), MARKER);
    let speculative_root = syntax::parse(&spliced).syntax_node();
    let marker_end = offset + TextSize::of(MARKER);
    // The token wholly covering the marker — whether it's a bare insertion
    // or glued onto a typed prefix and/or a following identifier suffix
    // (mid-identifier completion), it's always exactly one IDENT token
    // spanning at least the marker's own range.
    let Some(marker_token) = speculative_root
        .token_at_offset(offset)
        .find(|t| t.kind() == SyntaxKind::IDENT && t.text_range().end() >= marker_end)
    else {
        return Vec::new();
    };
    let Some(parent) = marker_token.parent() else {
        return Vec::new();
    };

    // The position's expected type, when the real tree can prove one.
    // Independent of the classified context: every value-producing context
    // below ranks its candidates against it (type positions and keywords
    // ignore it).
    let expected_ty = expected_type_at(db, file, &real_root, offset, edit_range);
    let expected_ty = expected_ty.as_ref();

    let mut items = match classify(&parent) {
        Some(Context::ItemKeyword) => keyword_items(&["static", "const", "type"], edit_range),
        Some(Context::TypePosition) => {
            let mut items = type_scope_items(db, file, edit_range);
            items.extend(builtin_type_items(edit_range));
            items.extend(keyword_items(TYPE_KEYWORDS, edit_range));
            items
        }
        Some(Context::Expression {
            statement_start,
            in_loop,
        }) => expression_position_items(
            db,
            file,
            &real_root,
            offset,
            edit_range,
            statement_start,
            in_loop,
            expected_ty,
        ),
        Some(Context::FieldAccess { receiver_range }) => field_items(
            db,
            file,
            &real_root,
            receiver_range,
            edit_range,
            expected_ty,
        ),
        Some(Context::VariantSegment {
            base_name,
            in_type_position,
        }) => variant_segment_items(
            db,
            file,
            &base_name,
            in_type_position,
            edit_range,
            expected_ty,
        ),
        Some(Context::MatchArmPattern {
            scrutinee_range,
            bare,
        }) => match_arm_items(
            db,
            file,
            &real_root,
            offset,
            scrutinee_range,
            bare,
            edit_range,
        ),
        Some(Context::RecordLiteralField { record_start }) => record_literal_items(
            db,
            file,
            &real_root,
            offset,
            record_start,
            edit_range,
            expected_ty,
        ),
        Some(Context::RecordPatternField { record_pat_start }) => {
            record_pattern_items(db, file, &real_root, offset, record_pat_start, edit_range)
        }
        Some(Context::TypeItemRhs) => type_item_rhs_items(edit_range),
        None => Vec::new(),
    };
    items.sort_by(|a, b| a.sort_text.cmp(&b.sort_text));
    items
}

/// Classify the marker's parent node (in the speculative tree) into one of
/// the contexts above. `None` for anything else (a brand-new `let`/param
/// binding name, the enum segment of a qualified variant pattern before its
/// `::`, …) — there's nothing sound to complete there.
fn classify(parent: &SyntaxNode) -> Option<Context> {
    if parent.kind() == SyntaxKind::ERROR {
        return parent
            .parent()
            .is_some_and(|gp| gp.kind() == SyntaxKind::SOURCE_FILE)
            .then_some(Context::ItemKeyword);
    }

    // A bare pattern name: `BIND_PAT` wrapping a declaration `Name` (never
    // a `NameRef` — that's why this is checked before the `NameRef` cast
    // below). Only a match arm's own pattern is a completion context here:
    // a `let`/parameter binding name is a brand-new name, nothing to
    // complete — this falls through to `None` for that
    // case since `parent` then fails the `NameRef` cast too.
    if let Some(name) = ast::Name::cast(parent.clone())
        && let Some(bind_pat) = name.syntax().parent().and_then(ast::BindPat::cast)
        && let Some(arm) = bind_pat.syntax().parent().and_then(ast::MatchArm::cast)
    {
        let match_expr = ast::MatchExpr::cast(arm.syntax().parent()?)?;
        let scrutinee = match_expr.scrutinee()?;
        return Some(Context::MatchArmPattern {
            scrutinee_range: scrutinee.syntax().text_range(),
            bare: true,
        });
    }

    // A field-name slot inside a record-destructuring pattern: the marker's
    // declaration `Name` is a `RECORD_PAT_FIELD`'s *field* name (the first
    // `Name`; never its `as`-rename, the second — that's a brand-new binding
    // name, nothing to complete, same as a bare `let`/param name). Checked
    // here (before the `NameRef` cast) for the same reason as the match-arm
    // bind above: a pattern field name is a `Name`, not a `NameRef`.
    if let Some(name) = ast::Name::cast(parent.clone())
        && let Some(field) = name.syntax().parent().and_then(ast::RecordPatField::cast)
        && field
            .field_name()
            .is_some_and(|n| n.syntax() == name.syntax())
    {
        let record_pat = ast::RecordPat::cast(field.syntax().parent()?)?;
        return Some(Context::RecordPatternField {
            record_pat_start: record_pat.syntax().text_range().start(),
        });
    }

    let name_ref = ast::NameRef::cast(parent.clone())?;
    let grandparent = name_ref.syntax().parent()?;

    if let Some(path_type) = ast::PathType::cast(grandparent.clone()) {
        if path_type.name_ref().as_ref() == Some(&name_ref) {
            // Only the first segment; the second segment of `Shape::Circle`
            // in type position is the `VariantSegment` case below.
            return Some(Context::TypePosition);
        }
        if path_type.variant_name_ref().as_ref() == Some(&name_ref) {
            return Some(Context::VariantSegment {
                base_name: path_type.name_ref()?.text(),
                in_type_position: true,
            });
        }
        return None;
    }

    if let Some(field_expr) = ast::FieldExpr::cast(grandparent.clone()) {
        if field_expr.name_ref().as_ref() != Some(&name_ref) {
            return None;
        }
        let receiver = field_expr.receiver()?;
        return Some(Context::FieldAccess {
            receiver_range: receiver.syntax().text_range(),
        });
    }

    if let Some(record_field) = ast::RecordExprField::cast(grandparent.clone()) {
        if record_field.name_ref().as_ref() != Some(&name_ref) || !record_field.is_shorthand() {
            return None;
        }
        let record_expr = ast::RecordExpr::cast(record_field.syntax().parent()?)?;
        return Some(Context::RecordLiteralField {
            record_start: record_expr.syntax().text_range().start(),
        });
    }

    if let Some(variant_pat) = ast::VariantPat::cast(grandparent.clone()) {
        if variant_pat.variant_name_ref().as_ref() != Some(&name_ref) {
            return None;
        }
        let arm = ast::MatchArm::cast(variant_pat.syntax().parent()?)?;
        let match_expr = ast::MatchExpr::cast(arm.syntax().parent()?)?;
        let scrutinee = match_expr.scrutinee()?;
        return Some(Context::MatchArmPattern {
            scrutinee_range: scrutinee.syntax().text_range(),
            bare: false,
        });
    }

    let path_expr = ast::PathExpr::cast(grandparent)?;
    if path_expr.variant_name_ref().as_ref() == Some(&name_ref) {
        return Some(Context::VariantSegment {
            base_name: path_expr.name_ref()?.text(),
            in_type_position: false,
        });
    }
    // Same first-segment-only restriction as `PathType` above.
    if path_expr.name_ref().as_ref() != Some(&name_ref) {
        return None;
    }
    if path_expr
        .syntax()
        .parent()
        .is_some_and(|p| p.kind() == SyntaxKind::TYPE_ITEM)
    {
        return Some(Context::TypeItemRhs);
    }
    let statement_start = path_expr.syntax().parent().is_some_and(|p| {
        p.kind() == SyntaxKind::BLOCK_EXPR
            || (p.kind() == SyntaxKind::EXPR_STMT
                && p.parent()
                    .is_some_and(|pp| pp.kind() == SyntaxKind::BLOCK_EXPR))
    });
    Some(Context::Expression {
        statement_start,
        in_loop: in_loop(path_expr.syntax()),
    })
}

/// Whether `node` sits inside a `loop` body without crossing a fn-literal
/// or `const` block boundary first — the same reach `break`/`continue`
/// actually have (see `hir::infer`'s `loop_sinks` save/restore around both
/// boundaries). Takes any node, speculative or real — it only reads
/// ancestor *kinds*, which don't depend on which tree it came from.
fn in_loop(node: &SyntaxNode) -> bool {
    for ancestor in node.ancestors() {
        match ancestor.kind() {
            SyntaxKind::LOOP_EXPR => return true,
            SyntaxKind::FN_LITERAL | SyntaxKind::CONST_BLOCK_EXPR => return false,
            _ => {}
        }
    }
    false
}

/// The longest run of identifier characters ending exactly at `offset` — the
/// already-typed prefix the completion's text edit should replace.
fn prefix_range(text: &str, offset: TextSize) -> TextRange {
    let before = &text[..usize::from(offset)];
    let mut start = usize::from(offset);
    for (i, c) in before.char_indices().rev() {
        if c.is_alphanumeric() || c == '_' {
            start = i;
        } else {
            break;
        }
    }
    TextRange::new(TextSize::new(start as u32), offset)
}

/// The keywords that spell a TYPE where a type is expected: an `fn` type
/// and a structural record. `enum` is deliberately absent — an enum literal
/// declares a type, so it is spellable only on a `type` item's right-hand
/// side ([`type_item_rhs_items`]), never in an annotation.
const TYPE_KEYWORDS: &[&str] = &["fn", "struct"];

fn keyword_items(words: &[&str], edit_range: TextRange) -> Vec<CompletionItem> {
    words
        .iter()
        .map(|w| {
            completion_item(
                *w,
                CompletionItemKind::Keyword,
                Provenance::Keyword,
                TYPE_TIER_NONE,
                None,
                edit_range,
            )
        })
        .collect()
}

/// The `type X = …` RHS keywords, snippeted: `struct { $1 }` /
/// `enum { $1 }` — a snippet-incapable client falls back to the bare shell
/// (`struct { }` / `enum { }`), still more than the plain keyword alone.
fn type_item_rhs_items(edit_range: TextRange) -> Vec<CompletionItem> {
    [
        ("struct", "struct { $1 }", "struct { }"),
        ("enum", "enum { $1 }", "enum { }"),
    ]
    .into_iter()
    .map(|(word, snippet, plain)| {
        let mut item = completion_item(
            word,
            CompletionItemKind::Keyword,
            Provenance::Keyword,
            TYPE_TIER_NONE,
            None,
            edit_range,
        );
        item.text_edit.insert = InsertText::Snippet {
            snippet: snippet.to_owned(),
            plain: plain.to_owned(),
        };
        item
    })
    .collect()
}

/// `usize`/`str`/`string`/`bool` — the nameable builtin types (mirrors
/// [`hir::ty::builtin_type_by_name`]'s name set). Rendered with the keyword
/// kind (they're not declarations to navigate to) but the builtin sort
/// tier.
fn builtin_type_items(edit_range: TextRange) -> Vec<CompletionItem> {
    ["usize", "str", "string", "bool"]
        .iter()
        .map(|w| {
            completion_item(
                *w,
                CompletionItemKind::Keyword,
                Provenance::Builtin,
                TYPE_TIER_NONE,
                None,
                edit_range,
            )
        })
        .collect()
}

/// The builtin functions (mirrors [`hir::scopes::Builtin`]'s name set and
/// `hir::infer`'s signatures for them). The generic and
/// flavor-polymorphic ones have no ONE `hir::Ty`; their detail strings
/// spell the scheme by hand and a same-arity placeholder fn type drives
/// the call snippet (its `{error}` params never render — only the arity is
/// consumed).
fn builtin_fn_items(edit_range: TextRange, expected: Option<&hir::Ty>) -> Vec<CompletionItem> {
    let placeholder_fn = |arity: usize| {
        hir::Ty::fn_type(
            std::iter::repeat_n(hir::Ty::Error, arity).collect(),
            hir::Ty::Error,
        )
    };
    [
        (
            "print",
            hir::Ty::fn_type(vec![hir::Ty::Str], hir::Ty::Unit),
            None,
        ),
        (
            "panic",
            hir::Ty::fn_type(vec![hir::Ty::Str], hir::Ty::Never),
            None,
        ),
        (
            "alloc_array",
            placeholder_fn(1),
            Some("fn::<T>(usize) -> AllocResult::<T>"),
        ),
        (
            "dealloc_array",
            placeholder_fn(2),
            Some("unsafe fn::<T>(&raw mut T, usize)"),
        ),
        (
            "offset",
            placeholder_fn(2),
            Some("fn(&raw [mut] T, usize) -> &raw [mut] T"),
        ),
        (
            "copy",
            placeholder_fn(3),
            Some("unsafe fn(&raw [mut] T, &raw mut T, usize)"),
        ),
        (
            "dangling",
            placeholder_fn(0),
            Some("fn::<T>() -> &raw mut T"),
        ),
    ]
    .into_iter()
    .map(|(name, ty, detail)| {
        let has_real_type = detail.is_none();
        let tier = if has_real_type {
            type_tier(Some(&ty), expected)
        } else {
            type_tier(None, expected)
        };
        let detail = detail.map(str::to_owned).unwrap_or_else(|| ty.display());
        let mut candidate = completion_item(
            name,
            CompletionItemKind::Function,
            Provenance::Builtin,
            tier,
            Some(detail),
            edit_range,
        );
        if let hir::Ty::Fn(f) = &ty {
            candidate.text_edit.insert = fn_call_insert(&candidate.label, f, tier);
        }
        candidate
    })
    .collect()
}

/// Whether a `type` item is best offered as `Enum` or `Struct` — the same
/// test `hir::file_diagnostics` uses to word its "not a struct/enum"
/// messages.
fn type_item_kind(db: &RootDatabase, item: hir::ItemId<'_>) -> CompletionItemKind {
    if hir::enum_variants(db, item).is_some() {
        CompletionItemKind::Enum
    } else {
        CompletionItemKind::Struct
    }
}

/// Expression-position candidates from the file's top-level items: value
/// items (constructors excluded — not lowered yet) as
/// `Function`/`Constant`, type items as `Struct`/`Enum` (legal as
/// construction heads).
fn file_value_and_type_items(
    db: &RootDatabase,
    file: SourceFile,
    edit_range: TextRange,
    expected: Option<&hir::Ty>,
) -> Vec<CompletionItem> {
    hir::file_scope(db, file)
        .iter()
        .filter_map(|(name, resolution)| match resolution {
            hir::Resolution::Item(loc) => {
                let item = loc.to_id(db);
                let ty = hir::signature(db, item);
                let tier = type_tier(Some(&ty), expected);
                let kind = if matches!(ty, hir::Ty::Fn(_)) {
                    CompletionItemKind::Function
                } else {
                    CompletionItemKind::Constant
                };
                let mut candidate = completion_item(
                    name,
                    kind,
                    Provenance::Item,
                    tier,
                    Some(ty.display()),
                    edit_range,
                );
                if let hir::Ty::Fn(f) = &ty {
                    candidate.text_edit.insert = fn_call_insert(&candidate.label, f, tier);
                }
                Some(candidate)
            }
            hir::Resolution::TypeItem(loc) => {
                let item = loc.to_id(db);
                // A type name is not a value — there is no type to rank
                // against the expectation (its *construction* would have
                // one, but that's a different edit than inserting the
                // name).
                Some(completion_item(
                    name,
                    type_item_kind(db, item),
                    Provenance::Item,
                    TYPE_TIER_NONE,
                    type_item_detail(db, item),
                    edit_range,
                ))
            }
            _ => None,
        })
        .collect()
}

/// Type-position candidates: only the file's declared types.
fn type_scope_items(
    db: &RootDatabase,
    file: SourceFile,
    edit_range: TextRange,
) -> Vec<CompletionItem> {
    hir::type_scope(db, file)
        .iter()
        .map(|(name, loc)| {
            let item = loc.to_id(db);
            completion_item(
                name,
                type_item_kind(db, item),
                Provenance::Item,
                TYPE_TIER_NONE,
                type_item_detail(db, item),
                edit_range,
            )
        })
        .collect()
}

/// Expression/statement-position candidates: file items, builtins,
/// keywords (`let` only at a fresh statement, `break`/`continue` only
/// inside a reachable `loop`), and visible locals. Shared by the ordinary
/// `Expression` context and — with `statement_start: false` — the
/// record-literal fallback when no field expectation is derivable (see
/// [`record_literal_items`]).
#[allow(clippy::too_many_arguments)]
fn expression_position_items(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    edit_range: TextRange,
    statement_start: bool,
    in_loop: bool,
    expected: Option<&hir::Ty>,
) -> Vec<CompletionItem> {
    let mut items = file_value_and_type_items(db, file, edit_range, expected);
    items.extend(builtin_fn_items(edit_range, expected));
    let mut words: Vec<&str> = vec![
        "if", "match", "loop", "fn", "true", "false", "struct", "const", "unsafe",
    ];
    if statement_start {
        words.push("let");
    }
    if in_loop {
        words.push("break");
        words.push("continue");
    }
    items.extend(keyword_items(&words, edit_range));
    items.extend(
        locals_for(db, file, real_root, offset, statement_start)
            .into_iter()
            .map(|(name, mutable, ty)| {
                completion_item(
                    name,
                    CompletionItemKind::Variable,
                    Provenance::Local,
                    type_tier(ty.as_ref(), expected),
                    local_detail(ty.as_ref(), mutable),
                    edit_range,
                )
            }),
    );
    items
}

/// `receiver.field` completions: the receiver's fields directly
/// (`Ty::Record`) or through its declaration (`Ty::Named`); empty for
/// anything else (no record shape to project through — the diagnostic for
/// a genuinely wrong receiver already squiggles it, this just offers no
/// candidates instead of a wrong one).
fn field_items(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    receiver_range: TextRange,
    edit_range: TextRange,
    expected: Option<&hir::Ty>,
) -> Vec<CompletionItem> {
    let Some(anchor) = real_anchor(real_root, receiver_range.start()) else {
        return Vec::new();
    };
    let Some(item) = item_at(db, file, real_root, &anchor) else {
        return Vec::new();
    };
    let (_, source_map) = hir::body_with_source_map(db, item);
    let Some(expr) = expr_for_range(source_map, real_root, receiver_range) else {
        return Vec::new();
    };
    let inference = hir::infer::infer(db, item);
    let Some(ty) = inference.type_of_expr.get(expr) else {
        return Vec::new();
    };
    let record = match ty {
        hir::Ty::Record(rec) => Some(rec.clone()),
        hir::Ty::Named(named) => match hir::type_underlying_for(db, named) {
            Some(hir::Ty::Record(rec)) => Some(rec),
            _ => None,
        },
        _ => None,
    };
    let Some(record) = record else {
        return Vec::new();
    };
    record
        .fields
        .iter()
        .map(|(name, ty)| {
            completion_item(
                name.clone(),
                CompletionItemKind::Field,
                Provenance::Item,
                type_tier(Some(ty), expected),
                Some(ty.display()),
                edit_range,
            )
        })
        .collect()
}

/// The second segment of a `::` path (`Shape::Circle`), in expression
/// or type position: the base name's own enum variants, detail rendered
/// like `hover`'s variant hover. Empty for anything the base doesn't
/// resolve to an enum `type` item (a plain value item, a struct-type item,
/// or nothing at all) — those already get their own squiggle.
fn variant_segment_items(
    db: &RootDatabase,
    file: SourceFile,
    base_name: &str,
    in_type_position: bool,
    edit_range: TextRange,
    expected: Option<&hir::Ty>,
) -> Vec<CompletionItem> {
    let resolution = if in_type_position {
        hir::type_scope(db, file).resolve(base_name)
    } else {
        hir::file_scope(db, file).resolve(base_name)
    };
    let Some(hir::Resolution::TypeItem(loc)) = resolution else {
        return Vec::new();
    };
    let Some(variants) = hir::enum_variants(db, loc.to_id(db)).as_ref() else {
        return Vec::new();
    };
    variants
        .iter()
        .enumerate()
        .map(|(index, (name, payload))| {
            // The candidate's type is the variant type itself: exact under
            // a variant-typed expectation, `widens_to` (tier 1) under the
            // enum's.
            // No mention args exist at `Enum::<cursor>` — an empty list
            // is fine for ranking/display (a generic enum renders bare).
            let variant_ty = hir::Ty::Variant(hir::VariantTy {
                decl: loc.clone(),
                args: Vec::new(),
                index: index as u32,
                name: std::sync::Arc::from(name.as_str()),
            });
            completion_item(
                name.clone(),
                CompletionItemKind::EnumMember,
                Provenance::Item,
                type_tier(Some(&variant_ty), expected),
                Some(crate::hover::render_variant(name, payload)),
                edit_range,
            )
        })
        .collect()
}

/// Match-arm pattern candidates: the scrutinee's enum variants,
/// ranked uncovered-first over the other (already-written) arms' patterns
/// — read via `InferenceResult::variant_of_pat`, never recomputed — then
/// `_`. Non-enum (or unknown) scrutinee: no variants to suggest, so just
/// `_` in a bare slot, nothing at all past a `::` (there's no variant to
/// even guess at).
///
/// `bare` picks the insertion spelling: a bare pattern slot has no `::` at
/// all yet, so inserting a bare variant name would just bind a fresh local
/// under that name (bare names always bind, never reinterpret as a
/// variant — G25) — it inserts the sigil form `::Variant` instead (the
/// terser of the two safe spellings, mirroring how the sigil elides the
/// enum in handwritten patterns; the qualified `Enum::Variant` stays
/// valid syntax, it's just not what completion inserts). Label matches
/// the insertion (`::Variant` — what you see is what inserts) while
/// `filter_text` stays the bare variant name so a typed prefix like
/// `Cir` still matches. A pattern
/// already past a `::` (elided sigil or `Enum::`-qualified) inserts just
/// the variant name; the sigil is already on screen.
fn match_arm_items(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    scrutinee_range: TextRange,
    bare: bool,
    edit_range: TextRange,
) -> Vec<CompletionItem> {
    let wildcard = || {
        vec![completion_item(
            "_",
            CompletionItemKind::Keyword,
            Provenance::Keyword,
            TYPE_TIER_NONE,
            None,
            edit_range,
        )]
    };

    let Some(anchor) = real_anchor(real_root, scrutinee_range.start()) else {
        return Vec::new();
    };
    let Some(item) = item_at(db, file, real_root, &anchor) else {
        return Vec::new();
    };
    let (_, source_map) = hir::body_with_source_map(db, item);
    let Some(scrutinee_expr) = expr_for_range(source_map, real_root, scrutinee_range) else {
        return Vec::new();
    };
    let inference = hir::infer::infer(db, item);

    let enum_named = match inference.type_of_expr.get(scrutinee_expr) {
        Some(hir::Ty::Named(named)) if hir::enum_variants(db, named.decl.to_id(db)).is_some() => {
            named.clone()
        }
        _ => return if bare { wildcard() } else { Vec::new() },
    };
    let Some(variants) = hir::enum_variants(db, enum_named.decl.to_id(db)).as_ref() else {
        return if bare { wildcard() } else { Vec::new() };
    };

    // The other (already-written) arms' patterns: back-mapped to `PatId`s
    // so the covered set reads what `infer` already resolved each one to,
    // rather than recomputing variant resolution here.
    let Some(scrutinee_node) = source_map
        .node_for_expr(scrutinee_expr)
        .map(|ptr| ptr.to_node(real_root))
    else {
        return Vec::new();
    };
    let Some(match_expr) = scrutinee_node.ancestors().find_map(ast::MatchExpr::cast) else {
        return Vec::new();
    };
    let mut covered = vec![false; variants.len()];
    for arm in match_expr.arms() {
        // The arm being typed right now (its range contains the cursor) is
        // not an "other" arm — its own (possibly nonsensical, half-typed)
        // pattern must not count as covering anything.
        if arm.syntax().text_range().contains(offset) {
            continue;
        }
        let Some(pat) = arm.pat() else { continue };
        let Some(pat_id) = source_map.pat_for_node(SyntaxNodePtr::new(pat.syntax())) else {
            continue;
        };
        if let Some(variant) = inference.variant_of_pat.get(pat_id) {
            covered[variant.index as usize] = true;
        }
    }

    // The pattern-side "expectation" is the scrutinee's own enum type:
    // every variant candidate widens to it (tier 1, gold or not), keeping
    // the pattern slot's candidates ahead of where an unrankable candidate
    // would sort while preserving gold-before-covered within the tier.
    let scrutinee_ty = hir::Ty::Named(enum_named.clone());
    let mut items: Vec<CompletionItem> = variants
        .iter()
        .enumerate()
        .map(|(index, (name, payload))| {
            let insert = if bare {
                format!("::{name}")
            } else {
                name.clone()
            };
            let provenance = if covered[index] {
                Provenance::Item
            } else {
                Provenance::Gold
            };
            let variant_ty = hir::Ty::Variant(hir::VariantTy {
                decl: enum_named.decl.clone(),
                args: enum_named.args.clone(),
                index: index as u32,
                name: std::sync::Arc::from(name.as_str()),
            });
            let mut item = completion_item(
                insert.clone(),
                CompletionItemKind::EnumMember,
                provenance,
                type_tier(Some(&variant_ty), Some(&scrutinee_ty)),
                Some(crate::hover::render_variant(name, payload)),
                edit_range,
            );
            if bare {
                // The label/insertion carry the sigil, but filtering must
                // match what the user actually types (`Cir`, not `::Cir`).
                item.filter_text = name.clone();
            }
            // A payload-carrying variant inserts a snippet with a
            // tab-stop for the payload (`::Circle($1)` / `Circle($1)`,
            // matching the context's own spelling); a snippet-incapable
            // client falls back to the bare insertion (no parens —
            // there's no sound single value to place there without one).
            if !payload.is_empty() {
                item.text_edit.insert = InsertText::Snippet {
                    snippet: format!("{insert}($1)"),
                    plain: insert,
                };
            }
            item
        })
        .collect();
    if bare {
        items.extend(wildcard());
    }
    items
}

/// A shorthand record-literal field name: the missing fields of the
/// expected record type (construction `Foo(struct { … })`, or an
/// annotated `let`/`static`/`const` initializer — both read straight off
/// the syntax in [`expected_record`]; `type_underlying` is the only
/// semantic lookup, real bidirectional expectations are
/// [`expected_type_at`]'s job). Fields already written elsewhere in the
/// literal are excluded. A visible local
/// whose name matches a missing field is the shorthand case in the flesh —
/// it ranks above every field (`Provenance::Gold`).
///
/// No expectation derivable at all (an un-annotated `let`, a value passed
/// to something that isn't a bare-name construction, …): the record's
/// shape says nothing here, so this falls back to the plain
/// expression-position candidates — it's still an expression position,
/// just one this pass can't type. (The alternative — offering nothing —
/// is defensible too, but silently going quiet on every un-derivable
/// expectation would be a worse floor than the ordinary expression
/// candidates a user would otherwise get typing anywhere else.)
fn record_literal_items(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    record_start: TextSize,
    edit_range: TextRange,
    expected: Option<&hir::Ty>,
) -> Vec<CompletionItem> {
    let Some(anchor) = real_anchor(real_root, record_start) else {
        return Vec::new();
    };
    let Some(record_expr) = anchor.ancestors().find_map(ast::RecordExpr::cast) else {
        return Vec::new();
    };

    let already_written: Vec<String> = record_expr
        .fields()
        .filter(|field| !field.syntax().text_range().contains(offset))
        .filter_map(|field| field.name_ref())
        .map(|name_ref| name_ref.text())
        .collect();

    let Some(hir::Ty::Record(rec)) = expected_record(db, file, &record_expr) else {
        return expression_position_items(
            db,
            file,
            real_root,
            offset,
            edit_range,
            false,
            in_loop(&anchor),
            expected,
        );
    };

    let missing: Vec<&(String, hir::Ty)> = rec
        .fields
        .iter()
        .filter(|(name, _)| !already_written.contains(name))
        .collect();

    let mut items: Vec<CompletionItem> = locals_for(db, file, real_root, offset, false)
        .into_iter()
        .filter_map(|(name, mutable, ty)| {
            // The gold local's own expectation is the field it would fill
            // (shorthand: `x` means `x: x`): matching the field's *type*
            // too ranks it tier 0.
            let (_, field_ty) = missing.iter().find(|(field, _)| **field == name)?;
            Some(completion_item(
                name,
                CompletionItemKind::Variable,
                Provenance::Gold,
                type_tier(ty.as_ref(), Some(field_ty)),
                local_detail(ty.as_ref(), mutable),
                edit_range,
            ))
        })
        .collect();
    items.extend(missing.iter().map(|(name, ty)| {
        // A field *name* is not a value — nothing to rank (its detail
        // already shows the type the value must have).
        let mut item = completion_item(
            name.clone(),
            CompletionItemKind::Field,
            Provenance::Item,
            TYPE_TIER_NONE,
            Some(ty.display()),
            edit_range,
        );
        // `x: $1` snippets past the field name, tab-stop ready for the
        // value; a snippet-incapable client gets just `x: ` (the shorthand
        // gold-local match above stays a plain bare name — unchanged).
        item.text_edit.insert = InsertText::Snippet {
            snippet: format!("{name}: $1"),
            plain: format!("{name}: "),
        };
        item
    }));
    items
}

/// The record type a literal at `record_expr` is expected to have —
/// syntactic only: either the single argument of a bare-name construction
/// call (`Foo(struct { … })`) or the initializer of a `let`/`static`/
/// `const` item annotated with a bare type name. Anything else (no
/// annotation, a `RecordType`-literal or variant-typed annotation, a
/// callee that isn't a plain name) yields `None` — [`expected_type_at`]'s
/// job, not this pass's.
fn expected_record(
    db: &RootDatabase,
    file: SourceFile,
    record_expr: &ast::RecordExpr,
) -> Option<hir::Ty> {
    let parent = record_expr.syntax().parent()?;
    if parent.kind() == SyntaxKind::ARG_LIST {
        let call = ast::CallExpr::cast(parent.parent()?)?;
        let ast::Expr::PathExpr(callee) = call.callee()? else {
            return None;
        };
        if callee.colon2_token().is_some() {
            return None;
        }
        let name = callee.name_ref()?.text();
        let hir::Resolution::TypeItem(loc) = hir::file_scope(db, file).resolve(&name)? else {
            return None;
        };
        return hir::type_underlying(db, loc.to_id(db));
    }
    let ty = if let Some(let_stmt) = ast::LetStmt::cast(parent.clone()) {
        let_stmt.ty()
    } else if let Some(static_item) = ast::StaticItem::cast(parent) {
        static_item.ty()
    } else {
        None
    }?;
    let ast::Type::PathType(path_ty) = ty else {
        return None;
    };
    if path_ty.variant_name_ref().is_some() {
        return None;
    }
    let name = path_ty.name_ref()?.text();
    let hir::Resolution::TypeItem(loc) = hir::type_scope(db, file).resolve(&name)? else {
        return None;
    };
    hir::type_underlying(db, loc.to_id(db))
}

/// A field-name slot inside a record-destructuring pattern
/// (`let struct { <cursor> } = p;`, a `fn (struct { <cursor> }: T)`
/// parameter, or the inner `struct { <cursor> }` of a `Name(…)` newtype
/// pattern): the field names of the record type flowing into the pattern,
/// minus the fields already bound elsewhere in the *same* pattern.
///
/// The type comes from the real snapshot's `InferenceResult::type_of_pat`
/// — the record type `check_pat` matched this pattern against (the
/// initializer's type for a `let`, the parameter's annotation, the
/// newtype's underlying record) — never reconstructed syntactically. A
/// completed field inserts just its own name (the shorthand binding); the
/// surrounding `mut`, `..` rest, and `as`-rename spellings all stay valid
/// around it, so no snippet is needed. Detail is the field's own type,
/// like every other field completion.
///
/// When the pattern's type isn't a record we can project through (unknown,
/// still-inferring, or genuinely non-record — each already carries its own
/// squiggle), this offers no field items rather than guessing; a record
/// pattern's field slot has no other generally-valid candidates, so that
/// leaves the same empty result the classifier gave before this.
fn record_pattern_items(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    record_pat_start: TextSize,
    edit_range: TextRange,
) -> Vec<CompletionItem> {
    let Some(anchor) = real_anchor(real_root, record_pat_start) else {
        return Vec::new();
    };
    let Some(record_pat) = anchor.ancestors().find_map(ast::RecordPat::cast) else {
        return Vec::new();
    };

    // Fields already bound earlier in this pattern — excluding the field
    // slot the cursor sits in (its own half-typed name must not exclude
    // itself). Mirrors `record_literal_items`' already-written filter.
    let already_bound: Vec<String> = record_pat
        .fields()
        .filter(|field| !field.syntax().text_range().contains(offset))
        .filter_map(|field| field.field_name())
        .map(|name| name.text())
        .collect();

    let Some(item) = item_at(db, file, real_root, &anchor) else {
        return Vec::new();
    };
    let (_, source_map) = hir::body_with_source_map(db, item);
    let Some(pat_id) = source_map.pat_for_node(SyntaxNodePtr::new(record_pat.syntax())) else {
        return Vec::new();
    };
    let inference = hir::infer::infer(db, item);
    let Some(ty) = inference.type_of_pat.get(pat_id) else {
        return Vec::new();
    };
    // The pattern's type directly (`Ty::Record`) or through a named type's
    // declaration (`Ty::Named`) — same projection as `field_items`. Anything
    // else has no record shape to complete against.
    let record = match ty {
        hir::Ty::Record(rec) => Some(rec.clone()),
        hir::Ty::Named(named) => match hir::type_underlying_for(db, named) {
            Some(hir::Ty::Record(rec)) => Some(rec),
            _ => None,
        },
        _ => None,
    };
    let Some(record) = record else {
        return Vec::new();
    };
    record
        .fields
        .iter()
        .filter(|(name, _)| !already_bound.contains(name))
        .map(|(name, ty)| {
            // A field *name* binds a value, it is not itself a value — there
            // is nothing to rank against an expectation (its detail already
            // shows the type the binding will have). Like `record_literal_
            // items`' field candidates, it stays at `TYPE_TIER_NONE`.
            completion_item(
                name.clone(),
                CompletionItemKind::Field,
                Provenance::Item,
                TYPE_TIER_NONE,
                Some(ty.display()),
                edit_range,
            )
        })
        .collect()
}

/// The type the expression completed at `offset` is expected to have, when
/// the REAL tree can prove one — read from the persisted
/// [`hir::InferenceResult::expectation_of_expr`]. Exactly two provable
/// shapes; everything else is `None` (no syntactic reconstruction — type
/// ranking simply doesn't apply then):
///
/// - A typed prefix (`let s: str = pri|`, `f(pri|)`, `x.fi|`): the prefix
///   is part of a real, registered expression ending exactly at the cursor
///   (`pri` as a `PathExpr`, `x.fi` as a `FieldExpr`) — climb from the
///   prefix's start token to the innermost such expression and read its
///   expectation.
/// - A fresh hole right after an annotated `let`'s `=` (`let s: str = |`):
///   the absent initializer lowers as `ExprData::Missing`, which IS
///   checked against the annotation and so carries the expectation — but a
///   missing expression has no syntax node to back-map through, so it is
///   reached structurally instead: the `let`'s pattern maps to its
///   `PatId`, and the body's one `Stmt::Let` holding that pattern names
///   the initializer's `ExprId`.
fn expected_type_at(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    edit_range: TextRange,
) -> Option<hir::Ty> {
    if !edit_range.is_empty() {
        let anchor = real_anchor(real_root, edit_range.start())?;
        let item = item_at(db, file, real_root, &anchor)?;
        let (_, source_map) = hir::body_with_source_map(db, item);
        let inference = hir::infer::infer(db, item);
        let mut node = anchor;
        loop {
            if node.text_range().end() == offset
                && let Some(expr) = source_map.expr_for_node(SyntaxNodePtr::new(&node))
            {
                return inference.expectation_of_expr.get(expr).cloned();
            }
            if node.text_range().end() > offset {
                return None;
            }
            node = node.parent()?;
        }
    }

    // No prefix: the only provable hole is a `let`'s missing initializer —
    // the nearest non-trivia token left of the cursor must be its `=`.
    let mut token = real_root.token_at_offset(offset).left_biased()?;
    while matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
        token = token.prev_token()?;
    }
    if token.kind() != SyntaxKind::EQ {
        return None;
    }
    let let_stmt = ast::LetStmt::cast(token.parent()?)?;
    if let_stmt.initializer().is_some() {
        return None;
    }
    let pat = let_stmt.pat()?;
    let item = item_at(db, file, real_root, let_stmt.syntax())?;
    let (body, source_map) = hir::body_with_source_map(db, item);
    let pat_id = source_map.pat_for_node(SyntaxNodePtr::new(pat.syntax()))?;
    let inference = hir::infer::infer(db, item);
    for (_, data) in body.exprs.iter() {
        let hir::body::ExprData::Block { stmts, .. } = data else {
            continue;
        };
        for stmt in stmts {
            if let hir::body::Stmt::Let { pat, init, .. } = stmt
                && *pat == pat_id
            {
                return inference.expectation_of_expr.get(*init).cloned();
            }
        }
    }
    None
}

/// The real-tree node whose position best represents `offset`: the token
/// at-or-after the cursor (see the module doc — trivia already attaches to
/// the correct enclosing node, so this needs no further back-mapping).
fn real_anchor(real_root: &SyntaxNode, offset: TextSize) -> Option<SyntaxNode> {
    real_root
        .token_at_offset(offset)
        .right_biased()
        .and_then(|t| t.parent())
}

/// The real-tree `ExprId` for a speculative-tree range that ends before the
/// cursor (see the module doc): starts at the real token at `range`'s own
/// start, then climbs ancestors until one both has the exact same range
/// and is registered in `source_map` — skipping wrapper nodes (`NameRef`,
/// a parenthesized expression's own paren node, …) that share the range
/// but were never themselves the node passed to `alloc_expr`. Bails once a
/// node's range grows past `range`'s end without a match — the target was
/// never registered (broken source, or a range that wasn't actually
/// identical between the two trees after all).
fn expr_for_range(
    source_map: &hir::BodySourceMap,
    real_root: &SyntaxNode,
    range: TextRange,
) -> Option<hir::ExprId> {
    let mut node = real_anchor(real_root, range.start())?;
    loop {
        if node.text_range() == range
            && let Some(expr) = source_map.expr_for_node(SyntaxNodePtr::new(&node))
        {
            return Some(expr);
        }
        if node.text_range().end() > range.end() {
            return None;
        }
        node = node.parent()?;
    }
}

/// The item whose body contains `node`, by position (mirrors
/// `hover`/`goto_definition`'s identical helper).
fn item_at<'db>(
    db: &'db RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    node: &SyntaxNode,
) -> Option<hir::ItemId<'db>> {
    let item_node = node.ancestors().find(|n| ast::Item::can_cast(n.kind()))?;
    let index = real_root
        .children()
        .filter(|n| ast::Item::can_cast(n.kind()))
        .position(|n| n == item_node)?;
    hir::file_item_ids(db, file).get(index).copied()
}

/// Visible locals at `offset`, each with whether it's `mut` and its
/// inferred type (for the sort key's TYPE tier; `None` when inference has
/// no entry) — the candidate source for expression/statement-start
/// completions. Empty when `offset` isn't inside any item's body
/// (defensive; shouldn't happen for a position classified as
/// `Expression`).
fn locals_for(
    db: &RootDatabase,
    file: SourceFile,
    real_root: &SyntaxNode,
    offset: TextSize,
    statement_start: bool,
) -> Vec<(String, bool, Option<hir::Ty>)> {
    let Some(anchor) = real_anchor(real_root, offset) else {
        return Vec::new();
    };
    let Some(item) = item_at(db, file, real_root, &anchor) else {
        return Vec::new();
    };
    let (body, source_map) = hir::body_with_source_map(db, item);
    let expr_scopes = hir::expr_scopes(db, item);
    let bindings = if statement_start {
        let Some(block) = anchor.ancestors().find_map(ast::BlockExpr::cast) else {
            return Vec::new();
        };
        locals_in_block(body, source_map, expr_scopes, &block, offset)
    } else {
        let Some(expr_id) = anchor
            .ancestors()
            .find_map(|n| source_map.expr_for_node(SyntaxNodePtr::new(&n)))
        else {
            return Vec::new();
        };
        let Some(scope) = expr_scopes.scope_of(expr_id) else {
            return Vec::new();
        };
        expr_scopes.visible_bindings(scope)
    };
    let inference = hir::infer::infer(db, item);
    bindings
        .into_iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, binding)| {
            (
                name,
                body.bindings[binding].mutable,
                inference.type_of_binding.get(binding).cloned(),
            )
        })
        .collect()
}

/// Visible bindings right before `offset` inside `block` (a real, already-
/// parsed block): the block's own entry scope, with each `let` before the
/// cursor overlaid in source order (shadowing later ones) — reconstructs
/// exactly the scope `hir::scopes::compute_expr_scopes` would have handed a
/// statement written at `offset`, without needing an `ExprId` there (there
/// usually isn't one — that's the position we're completing).
fn locals_in_block(
    body: &hir::Body,
    source_map: &hir::BodySourceMap,
    expr_scopes: &hir::ExprScopes,
    block: &ast::BlockExpr,
    offset: TextSize,
) -> Vec<(String, hir::BindingId)> {
    let Some(block_id) = source_map.expr_for_node(SyntaxNodePtr::new(block.syntax())) else {
        return Vec::new();
    };
    let Some(scope) = expr_scopes.scope_of(block_id) else {
        return Vec::new();
    };
    let mut current = expr_scopes.visible_bindings(scope);
    for stmt in block.statements() {
        if stmt.syntax().text_range().start() >= offset {
            break;
        }
        let ast::Stmt::LetStmt(let_stmt) = &stmt else {
            // Neither `Assign` nor a plain `Expr` statement changes scope.
            continue;
        };
        let Some(pat) = let_stmt.pat() else { continue };
        let Some(pat_id) = source_map.pat_for_node(SyntaxNodePtr::new(pat.syntax())) else {
            continue;
        };
        for (name, binding) in body.pat_bindings(pat_id) {
            match current.iter_mut().find(|(n, _)| *n == name) {
                Some(slot) => slot.1 = binding,
                None => current.push((name, binding)),
            }
        }
    }
    current
}
