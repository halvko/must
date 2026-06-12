//! Conversions from `lsp_types` to `ide` types.

use ide::LineIndex;
use line_index::{LineCol, WideEncoding, WideLineCol};
use syntax::TextSize;

pub(crate) fn offset(line_index: &LineIndex, position: lsp_types::Position) -> Option<TextSize> {
    // A line or column past the end of the document is legal client input —
    // the spec says an overshooting column "defaults back to the line
    // length" — and must be clamped into the requested line, not taken
    // literally: taken literally it lands on a later line, mid-character, or
    // past the end of the text, and a mid-character offset panics in the
    // syntax tree.
    let last_line = line_index.try_line_col(line_index.len())?.line;
    let (line, range) = match line_index.line(position.line) {
        Some(range) => (position.line, range),
        // No requested line to default into: the last line stands in for it.
        None => (last_line, line_index.line(last_line)?),
    };
    // `line()`'s range includes the terminator on every line but the last;
    // a position stops before it.
    let content_end = if line < last_line {
        TextSize::new(u32::from(range.end()) - 1)
    } else {
        range.end()
    };
    let wide = WideLineCol {
        line,
        // Clamp the raw column first: one like `u32::MAX` would overflow
        // `to_utf8`'s arithmetic into `None` instead of clamping.
        col: position
            .character
            .min(u32::from(range.end() - range.start())),
    };
    let col = line_index.to_utf8(WideEncoding::Utf16, wide)?.col;
    let mut offset = line_index.offset(LineCol { line, col })?.min(content_end);
    // A column inside a multi-byte character (a split surrogate pair)
    // converts into the middle of it; back up to the character itself.
    while line_index.try_line_col(offset).is_none() {
        offset -= TextSize::new(1);
    }
    Some(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntax::TextRange;

    #[test]
    fn overshooting_columns_clamp_to_the_requested_line() {
        // Line 0 is "ab"; line 1 is "cdé", whose `é` spans bytes 5..7.
        let index = LineIndex::new("ab\ncdé");

        // Taken literally, column 6 on line 0 is offset 6 — mid-character
        // inside line 1's `é`. It clamps to line 0's text end instead.
        assert_eq!(
            offset(&index, lsp_types::Position::new(0, 6)),
            Some(TextSize::new(2))
        );
        // The line's exact length lands on its end, before the newline.
        assert_eq!(
            offset(&index, lsp_types::Position::new(0, 2)),
            Some(TextSize::new(2))
        );
        // In-range columns on line 1 are untouched; overshooting ones clamp
        // to the end of the text.
        assert_eq!(
            offset(&index, lsp_types::Position::new(1, 2)),
            Some(TextSize::new(5))
        );
        assert_eq!(
            offset(&index, lsp_types::Position::new(1, 9999)),
            Some(TextSize::new(7))
        );
    }

    #[test]
    fn columns_inside_a_multibyte_character_resolve_to_its_boundary() {
        // `é` is 2 bytes / 1 UTF-16 unit, the emoji 4 bytes / 2 units: line
        // 0's text is 6 bytes / 3 UTF-16 units before the newline.
        let index = LineIndex::new("é😀\nx");

        // The emoji's low-surrogate column: the character's start, not its
        // middle.
        assert_eq!(
            offset(&index, lsp_types::Position::new(0, 2)),
            Some(TextSize::new(2))
        );
        // Past the emoji: the end of the line's text, before the newline.
        assert_eq!(
            offset(&index, lsp_types::Position::new(0, 3)),
            Some(TextSize::new(6))
        );
        assert_eq!(
            offset(&index, lsp_types::Position::new(0, 9999)),
            Some(TextSize::new(6))
        );
    }

    #[test]
    fn nonexistent_lines_resolve_to_the_last_line() {
        let index = LineIndex::new("ab\ncdé");

        assert_eq!(
            offset(&index, lsp_types::Position::new(9999, 0)),
            Some(TextSize::new(3))
        );
        assert_eq!(
            offset(&index, lsp_types::Position::new(9999, 9999)),
            Some(TextSize::new(7))
        );

        // A trailing newline's empty final line is real; one past it falls
        // back to it rather than vanishing.
        let trailing = LineIndex::new("ab\n");
        assert_eq!(
            offset(&trailing, lsp_types::Position::new(2, 0)),
            Some(TextSize::new(3))
        );
        assert_eq!(
            offset(&trailing, lsp_types::Position::new(3, 0)),
            Some(TextSize::new(3))
        );
    }

    #[test]
    fn resolved_offsets_always_land_on_a_boundary_in_the_requested_line() {
        let index = LineIndex::new("aé\nb😀\nc\n");
        // Per-line content ranges, the end being the terminator's position
        // (or the text's) — which the clamp lands on exactly, hence
        // inclusive here.
        let lines = [
            TextRange::new(TextSize::new(0), TextSize::new(3)), // "aé"
            TextRange::new(TextSize::new(4), TextSize::new(9)), // "b😀"
            TextRange::new(TextSize::new(10), TextSize::new(11)), // "c"
            TextRange::new(TextSize::new(12), TextSize::new(12)), // ""
        ];

        for requested in 0..6u32 {
            let effective = (requested as usize).min(lines.len() - 1) as u32;
            for character in [0, 1, 2, 3, 6, 9999] {
                let resolved = offset(&index, lsp_types::Position::new(requested, character))
                    .expect("every position resolves");
                assert!(
                    lines[effective as usize].contains_inclusive(resolved),
                    "({requested}, {character}) resolved to {resolved:?}, outside line {effective}"
                );
                assert!(
                    index.try_line_col(resolved).is_some(),
                    "({requested}, {character}) resolved to non-boundary {resolved:?}"
                );
            }
        }
    }
}
