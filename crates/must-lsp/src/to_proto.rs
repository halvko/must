//! Conversions from `ide` types to `lsp_types`. The only place (together
//! with the future `from_proto`) where the two worlds meet.

use ide::LineIndex;
use line_index::WideEncoding;
use syntax::{TextRange, TextSize};

pub(crate) fn position(line_index: &LineIndex, offset: TextSize) -> lsp_types::Position {
    let line_col = line_index.line_col(offset);
    // Clients default to UTF-16 column units.
    match line_index.to_wide(WideEncoding::Utf16, line_col) {
        Some(wide) => lsp_types::Position::new(wide.line, wide.col),
        None => lsp_types::Position::new(line_col.line, line_col.col),
    }
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
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::STATIC,
            SemanticTokenModifier::DEFAULT_LIBRARY,
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

pub(crate) fn diagnostic(line_index: &LineIndex, d: ide::Diagnostic) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: range(line_index, d.range),
        severity: Some(match d.severity {
            ide::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            ide::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
        }),
        source: Some("must".to_owned()),
        message: d.message,
        ..Default::default()
    }
}
