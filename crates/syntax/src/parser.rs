//! Parser infrastructure: an event-emitting parser over a trivia-free token
//! stream, with markers for retrofitting parents (matklad's resilient-LL
//! pattern). The grammar itself lives in `grammar.rs`; turning events into a
//! green tree lives in `builder.rs`.

use crate::SyntaxKind::{self, *};

#[derive(Debug)]
pub(crate) enum Event {
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    Token,
    Finish,
    Error {
        msg: String,
        /// Anchor on the previous token's last character (a visible,
        /// cursor-targetable range) instead of the token the parser is
        /// looking at; a fix inserts at the token's end.
        after_prev: bool,
        /// Text whose insertion at the error position fixes the error;
        /// becomes a quick fix.
        fix_insert: Option<String>,
    },
    /// The number literal just consumed continues into `,`-separated digit
    /// groups over the next `tokens` tokens: one error spanning them all,
    /// with a fix that joins the groups with `_`.
    DigitGroupCommas {
        tokens: usize,
    },
}

pub(crate) struct Parser<'t> {
    tokens: &'t [SyntaxKind],
    /// The source text of each token in [`Self::tokens`], same indexing.
    /// Read only where kinds can't tell: a retired spelling
    /// ([`Parser::at_word`]) and `,` digit groups ([`Parser::nth_joined`]).
    texts: &'t [&'t str],
    pos: usize,
    events: Vec<Event>,
}

impl<'t> Parser<'t> {
    pub(crate) fn new(tokens: &'t [SyntaxKind], texts: &'t [&'t str]) -> Parser<'t> {
        debug_assert_eq!(tokens.len(), texts.len());
        Parser {
            tokens,
            texts,
            pos: 0,
            events: Vec::new(),
        }
    }

    /// Whether the current token is an identifier spelling `word` — the
    /// contextual test, for retired spellings only. Everything the language
    /// still HAS is a kind; nothing here should ever grow a second user
    /// without a keyword to go with it.
    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.at(IDENT) && self.texts.get(self.pos).is_some_and(|t| *t == word)
    }

    /// The source text of the `n`th token ahead; empty at the end.
    fn nth_text(&self, n: usize) -> &'t str {
        self.texts.get(self.pos + n).copied().unwrap_or("")
    }

    /// Whether the `n`th token ahead touches the token before it, with no
    /// trivia between. Every text is a slice of the one source, so two
    /// tokens touch exactly when one's end address is the other's start.
    fn nth_joined(&self, n: usize) -> bool {
        let i = self.pos + n;
        match (
            i.checked_sub(1).and_then(|j| self.texts.get(j)),
            self.texts.get(i),
        ) {
            (Some(a), Some(b)) => a.as_ptr().wrapping_add(a.len()) == b.as_ptr(),
            _ => false,
        }
    }

    pub(crate) fn finish(self) -> Vec<Event> {
        self.events
    }

    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens.get(self.pos + n).copied().unwrap_or(EOF)
    }

    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// Token-stream position; used by loops to assert progress.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Kind of the most recently consumed token.
    pub(crate) fn prev(&self) -> Option<SyntaxKind> {
        self.pos.checked_sub(1).map(|i| self.tokens[i])
    }

    pub(crate) fn bump_any(&mut self) {
        if !self.at(EOF) {
            self.pos += 1;
            self.events.push(Event::Token);
        }
    }

    pub(crate) fn bump(&mut self, kind: SyntaxKind) {
        assert!(self.at(kind), "expected to be at {kind:?}");
        self.bump_any();
    }

    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump_any();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, kind: SyntaxKind, human: &str) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error(format!("expected {human}"));
        false
    }

    /// Report an error at the current token without consuming anything.
    pub(crate) fn error(&mut self, msg: impl Into<String>) {
        self.events.push(Event::Error {
            msg: msg.into(),
            after_prev: false,
            fix_insert: None,
        });
    }

    /// Expect a closing or separator token (`;`, `}`, `)`). A missing one
    /// anchors on the previous token's last character — the token it
    /// belongs after — with an insert fix at the token's end, and is
    /// suppressed when that token already carries an error.
    pub(crate) fn expect_after_prev(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        if kind == SEMICOLON && self.eat_digit_group_commas() {
            return self.expect_after_prev(kind);
        }
        self.error_after_prev(kind);
        false
    }

    pub(crate) fn error_after_prev(&mut self, kind: SyntaxKind) {
        let insert = match kind {
            SEMICOLON => ";",
            R_PAREN => ")",
            R_BRACE => "}",
            R_ANGLE => ">",
            R_BRACKET => "]",
            _ => unreachable!("{kind:?} is not a closer"),
        };
        self.events.push(Event::Error {
            msg: format!("expected `{insert}`"),
            after_prev: true,
            fix_insert: Some(insert.to_owned()),
        });
    }

    /// `2,147,483,647` where a `;` is owed, so no `,` can continue what came
    /// before: the number literal just consumed and every `,ddd` group
    /// touching it become one error. Only 1–3 digits then exact 3-digit
    /// groups, all unspaced, so `1, 234` and `3,14` keep their usual errors.
    pub(crate) fn eat_digit_group_commas(&mut self) -> bool {
        let digits = |text: &str, len: std::ops::RangeInclusive<usize>| {
            len.contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
        };
        let Some(lit) = self.pos.checked_sub(1).map(|i| self.texts[i]) else {
            return false;
        };
        if self.prev() != Some(INT_NUMBER) || !digits(lit, 1..=3) {
            return false;
        }
        let mut n = 0;
        while self.nth(n) == COMMA
            && self.nth(n + 1) == INT_NUMBER
            && self.nth_joined(n)
            && self.nth_joined(n + 1)
            && digits(self.nth_text(n + 1), 3..=3)
        {
            n += 2;
        }
        if n == 0 {
            return false;
        }
        self.err_digit_group_commas(n);
        true
    }

    /// Report the digit groups over the next `tokens` tokens (see
    /// [`Event::DigitGroupCommas`]) and wrap them in an `ERROR` node.
    fn err_digit_group_commas(&mut self, tokens: usize) {
        // The number's own node becomes an `ERROR` too, so the expression it
        // stood for lowers as missing and traps (X06) instead of running as
        // its first group.
        if let Some(number) = self.events.iter().rposition(|e| matches!(e, Event::Token))
            && let Some(Event::Start {
                kind: kind @ LITERAL,
                ..
            }) = number.checked_sub(1).map(|i| &mut self.events[i])
        {
            *kind = ERROR;
        }
        let m = self.start();
        self.events.push(Event::DigitGroupCommas { tokens });
        for _ in 0..tokens {
            self.bump_any();
        }
        m.complete(self, ERROR);
    }

    /// Report an error and wrap the offending token in an `ERROR` node.
    pub(crate) fn err_and_bump(&mut self, msg: impl Into<String>) {
        let m = self.start();
        self.error(msg);
        self.bump_any();
        m.complete(self, ERROR);
    }

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Start {
            kind: TOMBSTONE,
            forward_parent: None,
        });
        Marker { pos }
    }
}

pub(crate) struct Marker {
    pos: u32,
}

impl Marker {
    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        match &mut p.events[self.pos as usize] {
            Event::Start { kind: slot, .. } => {
                debug_assert_eq!(*slot, TOMBSTONE);
                *slot = kind;
            }
            _ => unreachable!(),
        }
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Undo this marker: its children (if any) attach to the enclosing node.
    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        if self.pos as usize == p.events.len() - 1 {
            match p.events.pop() {
                Some(Event::Start {
                    kind: TOMBSTONE,
                    forward_parent: None,
                }) => {}
                _ => unreachable!(),
            }
        }
        // Otherwise leave the TOMBSTONE start in place; the builder skips it.
    }
}

pub(crate) struct CompletedMarker {
    pos: u32,
}

impl CompletedMarker {
    /// Start a new node that will wrap this completed one.
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let new = p.start();
        match &mut p.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => {
                debug_assert!(forward_parent.is_none());
                *forward_parent = Some(new.pos - self.pos);
            }
            _ => unreachable!(),
        }
        new
    }
}
