//! Conversions from `lsp_types` to `ide` types.

use ide::LineIndex;
use line_index::{LineCol, WideEncoding, WideLineCol};
use syntax::TextSize;

pub(crate) fn offset(line_index: &LineIndex, position: lsp_types::Position) -> Option<TextSize> {
    let wide = WideLineCol {
        line: position.line,
        col: position.character,
    };
    let line_col = line_index
        .to_utf8(WideEncoding::Utf16, wide)
        .unwrap_or(LineCol {
            line: position.line,
            col: position.character,
        });
    line_index.offset(line_col)
}
