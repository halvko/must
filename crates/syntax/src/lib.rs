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

pub use rowan::{TextRange, TextSize};
pub use syntax_kind::SyntaxKind;
pub use lexer::{Token, tokenize};

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
    let kinds: Vec<SyntaxKind> = tokens
        .iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivia())
        .collect();
    let mut parser = parser::Parser::new(&kinds);
    grammar::source_file(&mut parser);
    let events = parser.finish();
    let (green, mut errors) = builder::build(text, &tokens, events, lex_errors);
    // Things the grammar accepts (for resilience and fixes) but the
    // language rejects.
    errors.extend(validation::validate(&SyntaxNode::new_root(green.clone())));
    // One error per position: errors are reported most-fundamental-first
    // (lexer before parser, "expected a name" before "expected `=`"), and
    // editors tend to surface only one diagnostic per spot anyway.
    errors.sort_by_key(|err| (err.range.start(), err.range.end()));
    errors.dedup_by(|next, prev| next.range == prev.range);
    Parse {
        green,
        errors: errors.into(),
    }
}

#[cfg(test)]
mod tests;
