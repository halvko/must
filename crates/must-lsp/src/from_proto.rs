//! Conversions from `lsp_types` to `ide` types.

use ide::LineIndex;
use line_index::{LineCol, WideEncoding, WideLineCol};
use syntax::TextSize;

pub(crate) fn offset(line_index: &LineIndex, position: lsp_types::Position) -> Option<TextSize> {
    let wide = WideLineCol {
        line: position.line,
        col: position.character,
    };
    // A column past the end of the line is legal client input — the spec
    // says it "defaults back to the line length" — and must be clamped, not
    // taken literally: an unchecked offset panics in the syntax tree.
    let line_col = line_index
        .to_utf8(WideEncoding::Utf16, wide)
        .or_else(|| end_of_line(line_index, position.line))?;
    let offset = line_index.offset(line_col)?;
    Some(offset.min(line_index.len()))
}

/// The last position on `line` (before its terminator), or `None` if the
/// line doesn't exist.
fn end_of_line(line_index: &LineIndex, line: u32) -> Option<LineCol> {
    let start = line_index.offset(LineCol { line, col: 0 })?;
    let end = line_index
        .offset(LineCol {
            line: line + 1,
            col: 0,
        })
        .map(|next_line| TextSize::new(u32::from(next_line).saturating_sub(1)))
        .unwrap_or_else(|| line_index.len());
    Some(line_index.line_col(end.max(start)))
}
