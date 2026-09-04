//! Conversions from `ide` types to `lsp_types`. The only place (together
//! with the future `from_proto`) where the two worlds meet.

use ide::LineIndex;
use line_index::WideEncoding;
use syntax::{TextRange, TextSize};

pub(crate) fn position(line_index: &LineIndex, offset: TextSize) -> lsp_types::Position {
    let line_col = line_index.line_col(offset);
    // Clients default to UTF-16 column units.
    let wide = line_index
        .to_wide(WideEncoding::Utf16, line_col)
        .expect("a valid byte offset must convert to a UTF-16 position");
    lsp_types::Position::new(wide.line, wide.col)
}

pub(crate) fn range(line_index: &LineIndex, range: TextRange) -> lsp_types::Range {
    lsp_types::Range::new(
        position(line_index, range.start()),
        position(line_index, range.end()),
    )
}

pub(crate) fn text_edit(line_index: &LineIndex, edit: &ide::TextEdit) -> lsp_types::TextEdit {
    lsp_types::TextEdit {
        range: range(line_index, edit.range),
        new_text: edit.insert.clone(),
    }
}

/// The legend advertised in the server capabilities. Token-type order must
/// match [`token_type_index`]; modifier order must match the bit positions
/// in `ide::HlMods`.
///
/// Only STANDARD LSP token types appear here, and new ones are APPENDED —
/// never inserted, never reordered: a client caches the legend by index, and
/// standard names are the ones clients know how to fall back on (Zed styles
/// `interface` as `type.interface`/`interface`/`type` and `typeParameter` as
/// `type.parameter`/`type`, so an unstyled theme still gets the type color
/// rather than nothing).
pub(crate) fn semantic_tokens_legend() -> lsp_types::SemanticTokensLegend {
    use lsp_types::{SemanticTokenModifier, SemanticTokenType};
    lsp_types::SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::COMMENT,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::KEYWORD,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::TYPE,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::TYPE_PARAMETER,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::STATIC,
            SemanticTokenModifier::DEFAULT_LIBRARY,
            SemanticTokenModifier::new("mutable"),
        ],
    }
}

fn token_type_index(tag: ide::HlTag) -> u32 {
    match tag {
        ide::HlTag::Comment => 0,
        ide::HlTag::String => 1,
        ide::HlTag::Number => 2,
        ide::HlTag::Keyword => 3,
        ide::HlTag::Operator => 4,
        ide::HlTag::Function => 5,
        ide::HlTag::Variable => 6,
        ide::HlTag::Parameter => 7,
        ide::HlTag::Type => 8,
        ide::HlTag::EnumMember => 9,
        ide::HlTag::Trait => 10,
        ide::HlTag::TypeParameter => 11,
    }
}

/// Delta-encodes highlights into the wire format: per token
/// `[Δline, Δstart, length, type, modifiers]`, positions and lengths in
/// UTF-16 code units. Relies on `ide` emitting sorted, single-line ranges.
pub(crate) fn semantic_tokens(
    line_index: &LineIndex,
    highlights: &[ide::HlRange],
) -> lsp_types::SemanticTokens {
    let mut data = Vec::with_capacity(highlights.len());
    let mut prev = lsp_types::Position::new(0, 0);
    for hl in highlights {
        let start = position(line_index, hl.range.start());
        let end = position(line_index, hl.range.end());
        debug_assert_eq!(start.line, end.line, "ide must emit single-line ranges");
        data.push(lsp_types::SemanticToken {
            delta_line: start.line - prev.line,
            delta_start: if start.line == prev.line {
                start.character - prev.character
            } else {
                start.character
            },
            length: end.character - start.character,
            token_type: token_type_index(hl.tag),
            token_modifiers_bitset: hl.mods.0,
        });
        prev = start;
    }
    lsp_types::SemanticTokens {
        result_id: None,
        data,
    }
}

fn completion_item_kind(kind: ide::CompletionItemKind) -> lsp_types::CompletionItemKind {
    match kind {
        ide::CompletionItemKind::Function => lsp_types::CompletionItemKind::FUNCTION,
        ide::CompletionItemKind::Variable => lsp_types::CompletionItemKind::VARIABLE,
        ide::CompletionItemKind::Field => lsp_types::CompletionItemKind::FIELD,
        ide::CompletionItemKind::EnumMember => lsp_types::CompletionItemKind::ENUM_MEMBER,
        ide::CompletionItemKind::Struct => lsp_types::CompletionItemKind::STRUCT,
        ide::CompletionItemKind::Enum => lsp_types::CompletionItemKind::ENUM,
        ide::CompletionItemKind::Constant => lsp_types::CompletionItemKind::CONSTANT,
        ide::CompletionItemKind::Keyword => lsp_types::CompletionItemKind::KEYWORD,
    }
}

/// Picks between a snippet's two spellings per the client's own
/// `snippet_support` capability (read once at `initialize`, see
/// `GlobalState::new`) — a snippet-incapable client must never see literal
/// `$1`s, so it gets the candidate's plain fallback instead, with no
/// `insertTextFormat` set (defaults to plain text per the LSP spec).
pub(crate) fn completion_item(
    line_index: &LineIndex,
    item: ide::CompletionItem,
    snippet_support: bool,
) -> lsp_types::CompletionItem {
    let (new_text, insert_text_format) = match item.text_edit.insert {
        ide::InsertText::Plain(text) => (text, None),
        ide::InsertText::Snippet { snippet, plain } => {
            if snippet_support {
                (snippet, Some(lsp_types::InsertTextFormat::SNIPPET))
            } else {
                (plain, None)
            }
        }
    };
    lsp_types::CompletionItem {
        label: item.label,
        kind: Some(completion_item_kind(item.kind)),
        detail: item.detail,
        sort_text: Some(item.sort_text),
        filter_text: Some(item.filter_text),
        insert_text_format,
        text_edit: Some(lsp_types::CompletionTextEdit::Edit(lsp_types::TextEdit {
            range: range(line_index, item.text_edit.range),
            new_text,
        })),
        ..Default::default()
    }
}

pub(crate) fn diagnostic(
    snapshot: &crate::Snapshot,
    line_index: &LineIndex,
    d: ide::Diagnostic,
) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: range(line_index, d.range),
        severity: Some(match d.severity {
            ide::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            ide::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            ide::Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        source: Some("must".to_owned()),
        message: d.message,
        related_information: (!d.related.is_empty()).then(|| {
            d.related
                .iter()
                .filter_map(|r| {
                    // Every related location resolves its *own* file's URI
                    // and line index; positions in one file must never be
                    // computed with another file's index.
                    let uri = snapshot.uri_for(r.file)?;
                    let target_index = snapshot.analysis.line_index(r.file);
                    Some(lsp_types::DiagnosticRelatedInformation {
                        location: lsp_types::Location {
                            uri,
                            range: range(&target_index, r.range),
                        },
                        message: r.message.clone(),
                    })
                })
                .collect()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_uses_utf16_code_units() {
        let index = LineIndex::new("aé😀b");

        // `b` starts at byte column 7, but UTF-16 column 4:
        // one code unit each for `a` and `é`, and two for the emoji.
        assert_eq!(
            position(&index, TextSize::from(7)),
            lsp_types::Position::new(0, 4)
        );
    }
}
