//! Lossless, error-resilient syntax trees for Must.
//!
//! This crate is deliberately free of salsa and LSP dependencies: it maps
//! `&str` to a rowan green tree plus a list of errors, and nothing else.
//! Broken code still produces a tree covering every byte of the input.

pub mod ast;
mod builder;
mod grammar;
mod lexer;
mod parser;
mod syntax_kind;
mod validation;

use std::sync::Arc;

pub use lexer::{Token, char_literal_value, tokenize, unescape_char};
pub use rowan::{TextRange, TextSize};
pub use syntax_kind::{KEYWORDS, SyntaxKind};
pub use validation::{
    CAN_ONLY_ASSIGN_TO_A_VARIABLE, CAPABILITIES, Capability, MemberContext, capability_named,
    impl_element_bare_head, semantic_member_context, static_item_errors,
};

/// The brace rule's wording. The grammar's recovery error and validation's
/// fix-bearing error both report it, so the phrasing is shared and the
/// variants derived, keeping the two from drifting apart.
pub(crate) const BRACE_RULE: &str = "function bodies are blocks";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
    /// A machine-applicable fix, when one is known.
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fix {
    /// Shown to the user, e.g. as a quick-fix title.
    pub label: String,
    pub edits: Vec<TextEdit>,
}

/// Replace `range` with `insert`; an empty range is a pure insertion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextEdit {
    pub range: TextRange,
    pub insert: String,
}

/// One level of indentation for a generated line — house style, as every
/// example file writes it. Shared by every generator of multi-line source
/// text: `ide::completions`'s match-arm snippet template, and `hir`'s
/// "Add missing match arms" [`Fix`].
pub const INDENT_UNIT: &str = "    ";

/// The indentation of the line `offset` sits on — the leading run of spaces
/// and tabs, copied verbatim so a hard-tab file gets hard tabs back. Only
/// the base: a nested generated line adds [`INDENT_UNIT`] on top of it,
/// which is always spaces, so a hard-tab file gets a hard-tab base under a
/// spaces-indented body rather than hard tabs throughout.
pub fn line_indent(text: &str, offset: TextSize) -> String {
    let before = &text[..usize::from(offset)];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MustLanguage {}

impl rowan::Language for MustLanguage {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        raw.into()
    }
    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<MustLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<MustLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<MustLanguage>;
pub type SyntaxNodePtr = rowan::ast::SyntaxNodePtr<MustLanguage>;

/// The result of parsing: a green tree (cheap to clone, `Send`) plus errors.
/// Red [`SyntaxNode`]s are materialized on demand and must not cross threads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    green: rowan::GreenNode,
    errors: Arc<[SyntaxError]>,
    /// Per error, the range of the node the parser was building when it
    /// reported it; `None` for lexer and validation errors.
    owners: Arc<[Option<TextRange>]>,
}

impl Parse {
    pub fn syntax_node(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn tree(&self) -> ast::SourceFile {
        use ast::AstNode;
        ast::SourceFile::cast(self.syntax_node()).unwrap()
    }

    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// The range of the construct `err` belongs to: the node the parser was
    /// building when it reported it, which an error naming a missing token
    /// can lie outside of. The error's own range for every other error.
    pub fn blame(&self, err: &SyntaxError) -> TextRange {
        self.errors
            .iter()
            .position(|e| e == err)
            .and_then(|index| self.owners[index])
            .unwrap_or(err.range)
    }

    /// Tree + errors as text; the format snapshot tests assert against.
    pub fn debug_dump(&self) -> String {
        let mut out = format!("{:#?}", self.syntax_node());
        for err in self.errors.iter() {
            out.push_str(&format!("error {:?}: {}\n", err.range, err.message));
        }
        out
    }
}

pub fn parse(text: &str) -> Parse {
    let (tokens, lex_errors) = lexer::tokenize(text);
    // The parser sees KINDS, plus the text of each one: a retired spelling
    // is an ordinary identifier to the lexer, and refusing it by name is
    // the only way to say what replaced it (`grammar`'s `without`).
    let mut kinds: Vec<SyntaxKind> = Vec::with_capacity(tokens.len());
    let mut texts: Vec<&str> = Vec::with_capacity(tokens.len());
    let mut offset = TextSize::new(0);
    for token in &tokens {
        let end = offset + token.len;
        if !token.kind.is_trivia() {
            kinds.push(token.kind);
            texts.push(&text[TextRange::new(offset, end)]);
        }
        offset = end;
    }
    let mut parser = parser::Parser::new(&kinds, &texts);
    grammar::source_file(&mut parser);
    let events = parser.finish();
    let (green, errors, owners) = builder::build(text, &tokens, events, lex_errors);
    let mut errors: Vec<(SyntaxError, Option<TextRange>)> =
        errors.into_iter().zip(owners).collect();
    // Things the grammar accepts (for resilience and fixes) but the
    // language rejects.
    errors.extend(
        validation::validate(&SyntaxNode::new_root(green.clone()))
            .into_iter()
            .map(|err| (err, None)),
    );
    // One error per position: errors are reported most-fundamental-first
    // (lexer before parser, "expected a name" before "expected `=`"), and
    // editors tend to surface only one diagnostic per spot anyway. The
    // likelier collision — the missing-`;` hint against an error on the
    // token it anchors to — is suppressed in the builder before it is ever
    // pushed (see `Builder::error`), so the dedup here only settles
    // residual same-range collisions. The sort is stable, so those are
    // settled by push order: lexer errors seed the vec first, and flipping
    // that order would flip winners.
    errors.sort_by_key(|(err, _)| (err.range.start(), err.range.end()));
    errors.dedup_by(|(next, _), (prev, _)| next.range == prev.range);
    let (errors, owners): (Vec<_>, Vec<_>) = errors.into_iter().unzip();
    Parse {
        green,
        errors: errors.into(),
        owners: owners.into(),
    }
}

#[cfg(test)]
mod tests;
