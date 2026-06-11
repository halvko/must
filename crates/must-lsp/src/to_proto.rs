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
