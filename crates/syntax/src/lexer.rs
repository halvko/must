//! Hand-written lexer. Lexing never fails: every byte of the input is covered
//! by exactly one token, malformed input becomes `ERROR_TOKEN` (or a token
//! carrying an error, e.g. an unterminated string).

use crate::{Fix, SyntaxError, SyntaxKind, TextEdit};
use text_size::{TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub len: TextSize,
}

/// An error at a range *inside* a token — a bad escape in a string literal,
/// or each still-open `/*` inside an unterminated block comment. `range`,
/// and any `fix`'s edits, are relative to the token's start. Token
/// boundaries never depend on these: a malformed escape is a diagnostic,
/// not a re-lex.
struct InnerError {
    range: std::ops::Range<usize>,
    message: String,
    /// A machine-applicable fix, when one is known — same idea as
    /// [`crate::Fix`], but its edits are still token-relative like `range`
    /// above, so they get the identical `pos`-shift in [`tokenize`] rather
    /// than needing an absolute offset this deep in the scan.
    fix: Option<InnerFix>,
}

struct InnerFix {
    label: String,
    /// (edit range relative to the token start, text to insert)
    edits: Vec<(std::ops::Range<usize>, String)>,
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
        for InnerError {
            range,
            message,
            fix,
        } in inner.drain(..)
        {
            let shift = |r: std::ops::Range<usize>| {
                TextRange::new(
                    TextSize::new((pos + r.start) as u32),
                    TextSize::new((pos + r.end) as u32),
                )
            };
            let fix = fix.map(|InnerFix { label, edits }| Fix {
                label,
                edits: edits
                    .into_iter()
                    .map(|(range, insert)| TextEdit {
                        range: shift(range),
                        insert,
                    })
                    .collect(),
            });
            errors.push(SyntaxError {
                message,
                range: shift(range),
                fix,
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
        '/' if rest.as_bytes().get(1) == Some(&b'*') => scan_block_comment(rest, inner),
        '"' => scan_string(rest, inner),
        '\'' => scan_char(rest),
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

/// The escape sequences BOTH literal forms share, C/Rust-conventional:
/// `\n`, `\t`, `\r`, `\0` and `\\`. Each form adds its own closing quote on
/// top ([`unescape_char`] adds `\"`, [`unescape_char_literal`] adds `\'`) —
/// only the delimiter that would otherwise end the literal needs escaping,
/// so `"it's"` and `'"'` both stay spellable without one.
fn unescape_common(c: char) -> Option<char> {
    Some(match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        _ => return None,
    })
}

/// The escape sequences a STRING literal may contain: [`unescape_common`]
/// plus `\"`. `None` means "not an escape" — the lexer reports those as
/// `unknown escape sequence`, so a backslash is never silently literal.
/// Shared with HIR's literal lowering, which cooks the token text into the
/// string's value: one table, one answer.
pub fn unescape_char(c: char) -> Option<char> {
    match c {
        '"' => Some('"'),
        c => unescape_common(c),
    }
}

/// The escape sequences a CHARACTER literal may contain:
/// [`unescape_common`] plus `\'`. [`unescape_char`]'s twin, and crate-local
/// where that one is not: HIR reads a character literal through
/// [`char_literal_value`], which cooks a whole token off this table, rather
/// than one escape at a time.
fn unescape_char_literal(c: char) -> Option<char> {
    match c {
        '\'' => Some('\''),
        c => unescape_common(c),
    }
}

/// Cook a whole `'x'` token's text into the Unicode scalar value it spells.
/// `None` for every shape the lexer already errored on (unterminated,
/// empty, multi-character, unknown escape) — the diagnostic is the lexer's,
/// so HIR lowering only has to know that there is no value here.
///
/// Lives beside [`scan_char`] on purpose: the scanner decides what is
/// well-formed and this decides what it means, off the same two tables, so
/// the two can never disagree about which literals have a value.
pub fn char_literal_value(text: &str) -> Option<char> {
    let body = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut chars = body.chars();
    let value = match chars.next()? {
        '\\' => unescape_char_literal(chars.next()?)?,
        c => c,
    };
    chars.next().is_none().then_some(value)
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
/// `\` here, nor the caller's own closing quote — both are valid escapes in
/// whichever literal form is asking.
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
                    message: escape_error(e),
                    fix: None,
                }),
                // Only reachable at end of input: before a closing quote a
                // backslash would have escaped that quote instead.
                None => inner.push(InnerError {
                    range: i..i + 1,
                    message: "a string cannot end with a lone `\\`".to_owned(),
                    fix: None,
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

/// `'x'` — a character literal, holding exactly one Unicode scalar value
/// (any of them: `'æ'` and `'🦀'` are ordinary multi-byte literals).
///
/// Unlike a string, a character literal ENDS AT THE LINE: the closing quote
/// is looked for on this line only, and when there is none the token is the
/// lone quote. That asymmetry is deliberate and is what makes the freed `'`
/// safe to type — a lone apostrophe, a half-typed literal in the editor, or
/// a stale lifetime spelling costs ONE odd token and one honest message
/// instead of swallowing the rest of the file (which is what a string's
/// multiline rule would do here).
///
/// The bound is on the LINE, not on the token count: two odd quotes on one
/// line pair up into a single literal, which is why `&'a T, b: &'b T` — two
/// retired lifetimes in one parameter list — recovers poorly (the first
/// quote closes on the second, swallowing what sits between). That case is
/// pinned by a test; a heuristic that broke the pairing would have to give
/// up the "you meant a string" message for `'hello world'`, which is a far
/// more common mistake than a doubly-retired signature.
///
/// Every malformed shape still lexes as [`SyntaxKind::CHAR`], the way an
/// unterminated string stays a `STRING`: the parser keeps seeing a literal
/// where the user wrote one, and [`char_literal_value`] answers `None` for
/// exactly the shapes that error here.
fn scan_char(rest: &str) -> (SyntaxKind, usize, Option<String>) {
    use SyntaxKind::CHAR;
    let mut chars = rest.char_indices().skip(1).peekable();
    let mut end = None;
    while let Some((i, c)) = chars.next() {
        match c {
            '\n' => break,
            // A backslash consumes the next character — that is what keeps
            // `'\''` one token — but NEVER a newline. Consuming one would
            // carry the scan onto the following line and eat a whole
            // innocent statement, which is precisely what the line bound
            // exists to prevent: `let c = '\` at end of line must cost this
            // line and no other.
            '\\' => match chars.peek() {
                Some(&(_, '\n')) | None => break,
                Some(_) => {
                    chars.next();
                }
            },
            '\'' => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return (
            CHAR,
            1,
            Some("unterminated character literal: expected a closing `'`".to_owned()),
        );
    };
    let len = end + 1;
    let body = &rest[1..end];
    let mut body_chars = body.chars();
    let error = match body_chars.next() {
        None => Some(
            "empty character literal: a character literal holds exactly one character".to_owned(),
        ),
        // A lone `\` cannot reach here: it would have escaped the quote
        // that ended the token.
        Some('\\') => {
            let escape = body_chars.next().expect("a lone `\\` escapes the quote");
            if unescape_char_literal(escape).is_none() {
                Some(escape_error(escape))
            } else {
                multi_char_error(body_chars.next().is_some())
            }
        }
        Some(_) => multi_char_error(body_chars.next().is_some()),
    };
    (CHAR, len, error)
}

/// The message for a backslash introducing no escape this language has.
///
/// Two shapes, and the split is the point. An escape the design has already
/// RESERVED room for (`\u{...}` and byte escapes — G17) gets a "not
/// supported yet": calling a spelling that is already spoken for "unknown"
/// tells the user to go find another one, which is the opposite of true.
/// Anything else is genuinely unknown.
///
/// Shared by both literal forms, so a reserved escape can never read as
/// reserved in a string and unknown in a character.
fn escape_error(e: char) -> String {
    match e {
        'u' => "`\\u{...}` escapes are not supported yet".to_owned(),
        'x' => "byte escapes (`\\xNN`) are not supported yet".to_owned(),
        e => format!("unknown escape sequence `{}`", shown_escape(e)),
    }
}

/// The "you meant a string" message, when a character literal holds more
/// than one character. Naming the string spelling matters more than naming
/// the rule: `'ab'` is almost never a mistake about characters, it is a
/// string written with the wrong quotes.
///
/// The offending text is deliberately NOT quoted back. It is source text
/// that may already contain escapes (`'ab\n'`), so re-spelling it inside
/// `"..."` would either be wrong about what the string means or smuggle a
/// raw control character into an LSP diagnostic — see [`shown_escape`] for
/// the same hazard one level down.
fn multi_char_error(too_long: bool) -> Option<String> {
    too_long.then(|| {
        "a character literal holds exactly one character; \
         use a string (`\"...\"`) to hold more"
            .to_owned()
    })
}

/// `/* ... */` — a block comment, NESTING the way Rust's does: a `/*` met
/// while a block comment is already open starts another level rather than
/// reading as ordinary text, and it takes a matching count of `*/` to close
/// back out to zero. That is the whole point of nesting: the use case is
/// commenting out a chunk of code that itself contains a block comment, and
/// a non-nesting scanner would have the FIRST `*/` inside it end the outer
/// comment early, spilling whatever follows into real tokens.
///
/// Depth is tracked as a STACK of each open `/*`'s offset, not a bare
/// counter: every entry still on the stack at EOF is its OWN independently
/// unclosed comment, and gets its OWN diagnostic (see the fallthrough
/// below) — N obligations, N errors, rather than blaming a single one
/// (outermost or innermost) and leaving the rest to be rediscovered one
/// fix at a time. The single-blame alternatives, and why report-all beats
/// both, are recorded under G20 (Discarded, in
/// `docs/design/grammar-and-syntax.md`).
///
/// No comment-start recognition happens inside a string or character
/// literal, and this scanner doesn't need any logic to secure that: it
/// only ever runs once a block comment has already started (dispatched on
/// the token's own first two bytes in [`next_token`]), and from inside it
/// every byte other than a `/*`/`*/` pair is inert text — a `"` in a
/// comment body starts nothing, exactly as a `/*` inside a string
/// (`"/*"`, scanned by [`scan_string`] before this function is ever
/// reached) starts nothing either.
///
/// A body that happens to start with `*` (`/** ... */`) earns no special
/// status: Must has no doc-comment convention today (no `///`, no
/// `/** */`), so this is an ordinary nested-capable comment like any
/// other — the leading `*` is just its first character of body text.
///
/// Unterminated swallows the rest of the file — the same call as an
/// unterminated string ([`scan_string`]): there is no sane place to resume
/// lexing after an unclosed comment, and editor auto-close makes the case
/// rare in practice (Zed's own `must` extension declares `/*`/`*/` for
/// exactly this reason).
///
/// Only the INNERMOST still-open `/*` (the last one reported, below) gets
/// a fix: appending `*/` right at EOF closes exactly that level, because
/// it is the one still open at the moment the file runs out. Any OTHER
/// still-open `/*` would need its `*/` inserted somewhere back inside the
/// file — a position this scan has no principled way to choose — so those
/// stay a diagnostic with no fix: a wrong guess is worse than an honest gap.
fn scan_block_comment(
    rest: &str,
    inner: &mut Vec<InnerError>,
) -> (SyntaxKind, usize, Option<String>) {
    let mut opens = vec![0usize];
    let mut chars = rest.char_indices().skip(2).peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|&(_, c)| c);
        match (c, next) {
            ('/', Some('*')) => {
                opens.push(i);
                chars.next();
            }
            ('*', Some('/')) => {
                chars.next();
                opens.pop();
                if opens.is_empty() {
                    return (SyntaxKind::COMMENT, i + 2, None);
                }
            }
            _ => {}
        }
    }
    // Reaching here means the scan ran out of input before `opens` ever
    // emptied out (the empty-return above is the only other way out).
    // Every remaining entry is its own unclosed `/*`, oldest (outermost)
    // first since that's push order — report one diagnostic each.
    let innermost = opens.len() - 1;
    for (i, start) in opens.into_iter().enumerate() {
        let fix = (i == innermost).then(|| InnerFix {
            label: "Insert `*/`".to_owned(),
            edits: vec![(rest.len()..rest.len(), "*/".to_owned())],
        });
        inner.push(InnerError {
            range: start..start + 2,
            message: "unterminated block comment: expected a closing `*/`".to_owned(),
            fix,
        });
    }
    (SyntaxKind::COMMENT, rest.len(), None)
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
