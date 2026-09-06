//! Hand-written lexer. Lexing never fails: every byte of the input is covered
//! by exactly one token, malformed input becomes `ERROR_TOKEN` (or a token
//! carrying an error, e.g. an unterminated string).

use crate::{SyntaxError, SyntaxKind};
use text_size::{TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub len: TextSize,
}

/// An error at a range *inside* a token — a bad escape in a string literal.
/// `range` is relative to the token's start. Token boundaries never depend
/// on these: a malformed escape is a diagnostic, not a re-lex.
struct InnerError {
    range: std::ops::Range<usize>,
    message: String,
}

pub fn tokenize(text: &str) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut inner = Vec::new();
    let mut pos = 0;

    while pos < text.len() {
        let rest = &text[pos..];
        inner.clear();
        let (kind, len, error) = next_token(rest, &mut inner);
        debug_assert!(len > 0, "lexer must always make progress");
        if let Some(message) = error {
            let start = TextSize::new(pos as u32);
            let range = TextRange::at(start, TextSize::new(len as u32));
            errors.push(SyntaxError {
                message,
                range,
                fix: None,
            });
        }
        for InnerError { range, message } in inner.drain(..) {
            errors.push(SyntaxError {
                message,
                range: TextRange::new(
                    TextSize::new((pos + range.start) as u32),
                    TextSize::new((pos + range.end) as u32),
                ),
                fix: None,
            });
        }
        tokens.push(Token {
            kind,
            len: TextSize::new(len as u32),
        });
        pos += len;
    }

    (tokens, errors)
}

/// Lex one token from the start of `rest`. Returns (kind, length in bytes,
/// whole-token error); errors covering only part of the token are pushed
/// onto `inner`.
fn next_token(rest: &str, inner: &mut Vec<InnerError>) -> (SyntaxKind, usize, Option<String>) {
    use SyntaxKind::*;

    let c = rest.chars().next().unwrap();
    match c {
        c if c.is_whitespace() => (WHITESPACE, scan_while(rest, char::is_whitespace), None),
        '/' if rest.as_bytes().get(1) == Some(&b'/') => {
            let len = rest.find('\n').unwrap_or(rest.len());
            (COMMENT, len, None)
        }
        '"' => scan_string(rest, inner),
        '\'' => scan_lifetime(rest),
        '@' => scan_region(rest),
        c if is_ident_start(c) => {
            let len = scan_while(rest, is_ident_continue);
            if len == 1 && c == '_' {
                return (HOLE, 1, None);
            }
            let kind = SyntaxKind::from_keyword(&rest[..len]).unwrap_or(IDENT);
            (kind, len, None)
        }
        c if c.is_ascii_digit() => {
            let len = scan_while(rest, |c| c.is_ascii_digit() || c == '_');
            (INT_NUMBER, len, None)
        }
        '-' if rest.as_bytes().get(1) == Some(&b'>') => (THIN_ARROW, 2, None),
        '=' if rest.as_bytes().get(1) == Some(&b'>') => (FAT_ARROW, 2, None),
        '=' if rest.as_bytes().get(1) == Some(&b'=') => (EQ2, 2, None),
        '!' if rest.as_bytes().get(1) == Some(&b'=') => (NEQ, 2, None),
        '<' if rest.as_bytes().get(1) == Some(&b'=') => (LTEQ, 2, None),
        '>' if rest.as_bytes().get(1) == Some(&b'=') => (GTEQ, 2, None),
        '<' => (L_ANGLE, 1, None),
        '>' => (R_ANGLE, 1, None),
        '(' => (L_PAREN, 1, None),
        ')' => (R_PAREN, 1, None),
        '{' => (L_BRACE, 1, None),
        '}' => (R_BRACE, 1, None),
        '[' => (L_BRACKET, 1, None),
        ']' => (R_BRACKET, 1, None),
        ':' if rest.as_bytes().get(1) == Some(&b':') => (COLON2, 2, None),
        ':' => (COLON, 1, None),
        ';' => (SEMICOLON, 1, None),
        ',' => (COMMA, 1, None),
        // `...` and `..` are single tokens; a lone `.` falls out as `DOT`.
        '.' if rest.as_bytes().get(1) == Some(&b'.') && rest.as_bytes().get(2) == Some(&b'.') => {
            (DOT3, 3, None)
        }
        '.' if rest.as_bytes().get(1) == Some(&b'.') => (DOT2, 2, None),
        '.' => (DOT, 1, None),
        '=' => (EQ, 1, None),
        '+' => (PLUS, 1, None),
        '-' => (MINUS, 1, None),
        '*' => (STAR, 1, None),
        '/' => (SLASH, 1, None),
        '&' => (AMP, 1, None),
        '!' => (BANG, 1, None),
        c => (
            ERROR_TOKEN,
            c.len_utf8(),
            Some(format!("unexpected character `{c}`")),
        ),
    }
}

/// The escape sequences a string literal may contain, C/Rust-conventional:
/// `\n`, `\t`, `\r`, `\0`, `\\` and `\"`. `None` means "not an escape" —
/// the lexer reports those as `unknown escape sequence`, so a backslash is
/// never silently literal. Shared with HIR's literal lowering, which cooks
/// the token text into the string's value: one table, one answer.
pub fn unescape_char(c: char) -> Option<char> {
    Some(match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        '"' => '"',
        _ => return None,
    })
}

/// Render the backslash-plus-`e` pair the user actually wrote, for a
/// diagnostic. A printable `e` gets the backslash put back in front
/// (`\q`). A CONTROL character (a literal newline, CR or tab — the C
/// line-continuation idiom, or a bare CRLF line ending) is rendered as its
/// `\u{..}` codepoint, never spelled `\n`/`\r`/`\t`: those two-character
/// spellings ARE valid escapes (`unescape_char` accepts them), so using
/// them here would tell the user their input is unknown while naming
/// something legal — copy-pasting the message would "fix" nothing.
/// Embedding the raw byte instead would split the message across lines, or
/// smuggle a bare CR from a CRLF file into an LSP diagnostic. `e` is never
/// `\` or `"` here — both are valid escapes.
fn shown_escape(e: char) -> String {
    if e.is_control() {
        format!("\\u{{{:x}}}", e as u32)
    } else {
        format!("\\{e}")
    }
}

// Strings are multiline, Rust-style — a literal newline inside a string is
// legal. An unterminated one swallows the rest of the file — accepting that
// beats the alternative: ending strings at newlines double-errors the common
// "closing quote on the next line" case, and editor quote auto-close makes
// unterminated strings rare in practice.
fn scan_string(rest: &str, inner: &mut Vec<InnerError>) -> (SyntaxKind, usize, Option<String>) {
    let mut chars = rest.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return (SyntaxKind::STRING, i + 1, None),
            // A backslash always consumes the next character — that is what
            // keeps `"\""` one token — but it must introduce a known escape.
            '\\' => match chars.next() {
                Some((_, e)) if unescape_char(e).is_some() => {}
                Some((j, e)) => inner.push(InnerError {
                    range: i..j + e.len_utf8(),
                    message: format!("unknown escape sequence `{}`", shown_escape(e)),
                }),
                // Only reachable at end of input: before a closing quote a
                // backslash would have escaped that quote instead.
                None => inner.push(InnerError {
                    range: i..i + 1,
                    message: "a string cannot end with a lone `\\`".to_owned(),
                }),
            },
            _ => {}
        }
    }
    (
        SyntaxKind::STRING,
        rest.len(),
        Some("unterminated string".into()),
    )
}

fn scan_lifetime(rest: &str) -> (SyntaxKind, usize, Option<String>) {
    match rest.chars().nth(1) {
        Some(c) if is_ident_start(c) => {
            let len = 1 + scan_while(&rest[1..], is_ident_continue);
            (SyntaxKind::LIFETIME_IDENT, len, None)
        }
        _ => (
            SyntaxKind::ERROR_TOKEN,
            1,
            Some("expected a lifetime name after `'`".into()),
        ),
    }
}

/// `@a` — a REGION name; `@_` — the region wildcard ("there is a region
/// here, infer it"). One token kind for both: `_` is an identifier start,
/// so the wildcard falls out of the same scan and the two are told apart by
/// text, exactly where the distinction matters (region lowering). `@` is
/// otherwise unclaimed in Must, so a `@` at token start can only be a
/// region — no lookahead, no ambiguity with anything else in the grammar.
fn scan_region(rest: &str) -> (SyntaxKind, usize, Option<String>) {
    match rest.chars().nth(1) {
        Some(c) if is_ident_start(c) => {
            let len = 1 + scan_while(&rest[1..], is_ident_continue);
            (SyntaxKind::REGION_IDENT, len, None)
        }
        _ => (
            SyntaxKind::ERROR_TOKEN,
            1,
            Some("expected a region name after `@` (`@a`, or `@_` to infer one)".into()),
        ),
    }
}

fn scan_while(text: &str, pred: impl Fn(char) -> bool) -> usize {
    text.find(|c| !pred(c)).unwrap_or(text.len())
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
