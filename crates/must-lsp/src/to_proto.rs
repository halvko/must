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
