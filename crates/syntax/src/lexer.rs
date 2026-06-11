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

pub fn tokenize(text: &str) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut pos = 0;

    while pos < text.len() {
        let rest = &text[pos..];
        let (kind, len, error) = next_token(rest);
        debug_assert!(len > 0, "lexer must always make progress");
        if let Some(message) = error {
            let start = TextSize::new(pos as u32);
            let range = TextRange::at(start, TextSize::new(len as u32));
            errors.push(SyntaxError { message, range });
        }
        tokens.push(Token {
            kind,
            len: TextSize::new(len as u32),
        });
        pos += len;
    }

    (tokens, errors)
}

/// Lex one token from the start of `rest`. Returns (kind, length in bytes, error).
fn next_token(rest: &str) -> (SyntaxKind, usize, Option<String>) {
    use SyntaxKind::*;

    let c = rest.chars().next().unwrap();
    match c {
        c if c.is_whitespace() => (WHITESPACE, scan_while(rest, char::is_whitespace), None),
        '/' if rest.as_bytes().get(1) == Some(&b'/') => {
            let len = rest.find('\n').unwrap_or(rest.len());
            (COMMENT, len, None)
        }
        '"' => scan_string(rest),
        '\'' => scan_lifetime(rest),
        c if is_ident_start(c) => {
            let len = scan_while(rest, is_ident_continue);
            let kind = SyntaxKind::from_keyword(&rest[..len]).unwrap_or(IDENT);
            (kind, len, None)
        }
        c if c.is_ascii_digit() => {
            let len = scan_while(rest, |c| c.is_ascii_digit() || c == '_');
            (INT_NUMBER, len, None)
        }
        '-' if rest.as_bytes().get(1) == Some(&b'>') => (THIN_ARROW, 2, None),
        '(' => (L_PAREN, 1, None),
        ')' => (R_PAREN, 1, None),
        '{' => (L_BRACE, 1, None),
        '}' => (R_BRACE, 1, None),
        ':' => (COLON, 1, None),
        ';' => (SEMICOLON, 1, None),
        ',' => (COMMA, 1, None),
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

fn scan_string(rest: &str) -> (SyntaxKind, usize, Option<String>) {
    let mut chars = rest.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return (SyntaxKind::STRING, i + 1, None),
            '\\' => {
                // Escapes are not validated yet; skip whatever follows.
                chars.next();
            }
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

fn scan_while(text: &str, pred: impl Fn(char) -> bool) -> usize {
    text.find(|c| !pred(c)).unwrap_or(text.len())
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
