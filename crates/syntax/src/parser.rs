//! Parser infrastructure: an event-emitting parser over a trivia-free token
//! stream, with markers for retrofitting parents (matklad's resilient-LL
//! pattern). The grammar itself lives in `grammar.rs`; turning events into a
//! green tree lives in `builder.rs`.

use crate::SyntaxKind::{self, *};
use crate::lexer::Token;
use rowan::{TextRange, TextSize};

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
        /// Becomes a quick fix: inserted at the previous token's end for
        /// an `after_prev` error, else replacing the token looked at.
        fix: Option<TokenFix>,
    },
    /// Sits right after a number literal's token, inside its `ERROR` node:
    /// the next `tokens` tokens are its `,`-separated digit groups. One
    /// error spans them all, with a fix that joins the groups with `_`.
    DigitGroupCommas {
        tokens: usize,
    },
}

#[derive(Debug)]
pub(crate) struct TokenFix {
    pub(crate) label: String,
    pub(crate) insert: String,
}

pub(crate) struct Parser<'t> {
    /// The kinds of `lexed`'s non-trivia tokens: what the grammar sees.
    tokens: Vec<SyntaxKind>,
    pos: usize,
    source: &'t str,
    /// The lexer's tokens over `source`, trivia included. Read only where
    /// kinds can't tell: a retired spelling ([`Parser::at_word`]) and `,`
    /// digit groups ([`Parser::eat_digit_group_commas`]).
    lexed: &'t [Token],
    /// Index in `lexed` and start offset of the current token `tokens[pos]`;
    /// kept in step with `pos` by [`Parser::bump_any`] and `skip_trivia`.
    lexed_pos: usize,
    offset: TextSize,
    events: Vec<Event>,
}

impl<'t> Parser<'t> {
    pub(crate) fn new(source: &'t str, lexed: &'t [Token]) -> Parser<'t> {
        let tokens = lexed
            .iter()
            .map(|token| token.kind)
            .filter(|kind| !kind.is_trivia())
            .collect();
        let mut p = Parser {
            tokens,
            pos: 0,
            source,
            lexed,
            lexed_pos: 0,
            offset: TextSize::new(0),
            events: Vec::new(),
        };
        p.skip_trivia();
        p
    }

    fn skip_trivia(&mut self) {
        while let Some(token) = self.lexed.get(self.lexed_pos)
            && token.kind.is_trivia()
        {
            self.offset += token.len;
            self.lexed_pos += 1;
        }
    }

    /// Whether the current token is an identifier spelling `word`. Every
    /// `word` asked about is a retired spelling: a spelling the language has
    /// is a kind.
    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.at(IDENT)
            && self.source[TextRange::at(self.offset, self.lexed[self.lexed_pos].len)] == *word
    }

    /// The non-trivia tokens from the one before the current token onwards,
    /// each as its kind, its text and whether it touches the token before
    /// it with no trivia between. Empty when no token precedes the current.
    fn tokens_from_prev(&self) -> impl Iterator<Item = (SyntaxKind, &'t str, bool)> + use<'t> {
        let (source, lexed) = (self.source, self.lexed);
        let (mut i, mut offset) = (self.lexed_pos, self.offset);
        loop {
            let Some(j) = i.checked_sub(1) else {
                i = lexed.len();
                break;
            };
            i = j;
            offset -= lexed[i].len;
            if !lexed[i].kind.is_trivia() {
                break;
            }
        }
        std::iter::from_fn(move || {
            let mut joined = true;
            while let Some(token) = lexed.get(i) {
                let range = TextRange::at(offset, token.len);
                i += 1;
                offset = range.end();
                if !token.kind.is_trivia() {
                    return Some((token.kind, &source[range], joined));
                }
                joined = false;
            }
            None
        })
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
            self.offset += self.lexed[self.lexed_pos].len;
            self.lexed_pos += 1;
            self.skip_trivia();
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
            fix: None,
        });
    }

    /// Report an error at the current token whose quick fix replaces that
    /// token with `insert` (an empty `insert` removes it).
    pub(crate) fn error_replacing_current(
        &mut self,
        msg: impl Into<String>,
        label: impl Into<String>,
        insert: impl Into<String>,
    ) {
        self.events.push(Event::Error {
            msg: msg.into(),
            after_prev: false,
            fix: Some(TokenFix {
                label: label.into(),
                insert: insert.into(),
            }),
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
        if kind == SEMICOLON && self.eat_digit_group_commas() && self.eat(kind) {
            return true;
        }
        self.error_after_prev(kind);
        false
    }

    pub(crate) fn error_after_prev(&mut self, kind: SyntaxKind) {
        let insert = kind.spelling().expect("a closer has one spelling");
        self.events.push(Event::Error {
            msg: format!("expected `{insert}`"),
            after_prev: true,
            fix: Some(TokenFix {
                label: format!("Insert `{insert}`"),
                insert: insert.to_owned(),
            }),
        });
    }

    /// `2,147,483,647` where a `;` is owed, so no `,` can continue what came
    /// before: the number literal just consumed and every `,ddd` group
    /// touching it become one error. Only 1–3 digits then exact 3-digit
    /// groups, all unspaced, so `1, 234` and `3,14` keep their usual errors.
    /// Events after the literal shift, so no marker past it may be held.
    pub(crate) fn eat_digit_group_commas(&mut self) -> bool {
        let digits = |text: &str, len: std::ops::RangeInclusive<usize>| {
            len.contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
        };
        if !self.at(COMMA) {
            return false;
        }
        let mut tokens = self.tokens_from_prev();
        if !tokens
            .next()
            .is_some_and(|(kind, lit, _)| kind == INT_NUMBER && digits(lit, 1..=3))
        {
            return false;
        }
        let mut n = 0;
        while let (Some((COMMA, _, true)), Some((INT_NUMBER, group, true))) =
            (tokens.next(), tokens.next())
            && digits(group, 3..=3)
        {
            n += 2;
        }
        if n == 0 {
            return false;
        }
        // The literal's node: its `Start` sits right before its one token.
        let Some(start) = self
            .events
            .iter()
            .rposition(|e| matches!(e, Event::Token))
            .and_then(|token| token.checked_sub(1))
            .filter(|&i| matches!(self.events[i], Event::Start { kind: LITERAL, .. }))
        else {
            return false;
        };
        self.err_digit_group_commas(start, n);
        true
    }

    /// Reopen the literal whose `Start` is at `start` as one `ERROR` node
    /// holding the next `tokens` tokens too ([`Event::DigitGroupCommas`]).
    /// The splice shifts later events and patches only `forward_parent`, so
    /// no live marker may start after `start`.
    fn err_digit_group_commas(&mut self, start: usize, tokens: usize) {
        if let Event::Start { kind, .. } = &mut self.events[start] {
            *kind = ERROR;
        }
        let at = start + 2;
        let mut rest = self.events.split_off(at);
        self.events.push(Event::DigitGroupCommas { tokens });
        for _ in 0..tokens {
            self.bump_any();
        }
        let inserted = (self.events.len() - at) as u32;
        // A wrapper preceded onto an earlier node may start past `at`.
        for (i, event) in self.events[..at].iter_mut().enumerate() {
            if let Event::Start {
                forward_parent: Some(fp),
                ..
            } = event
                && i + *fp as usize >= at
            {
                *fp += inserted;
            }
        }
        self.events.append(&mut rest);
    }

    /// Report an error and wrap the offending token in an `ERROR` node.
    pub(crate) fn err_and_bump(&mut self, msg: impl Into<String>) {
        let m = self.start();
        self.error(msg);
        self.bump_any();
        m.complete(self, ERROR);
    }

    /// Report an error whose quick fix replaces the current token with
    /// `insert` (an empty `insert` removes it), and wrap that token in an
    /// `ERROR` node.
    pub(crate) fn err_and_bump_replacing(
        &mut self,
        msg: impl Into<String>,
        label: impl Into<String>,
        insert: impl Into<String>,
    ) {
        let m = self.start();
        self.error_replacing_current(msg, label, insert);
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

    /// Hold a place at the current token for an error decided later, once
    /// what follows the token is parsed. A slot never filled reports
    /// nothing.
    pub(crate) fn reserve_error(&mut self) -> ErrorSlot {
        ErrorSlot {
            pos: self.start().pos,
        }
    }
}

/// An error held at the token it was reserved on ([`Parser::reserve_error`]).
pub(crate) struct ErrorSlot {
    pos: u32,
}

impl ErrorSlot {
    /// Report `msg` on the slot's token, with a quick fix that replaces the
    /// token with `insert`.
    pub(crate) fn fill_replacing(
        self,
        p: &mut Parser<'_>,
        msg: impl Into<String>,
        label: impl Into<String>,
        insert: impl Into<String>,
    ) {
        p.events[self.pos as usize] = Event::Error {
            msg: msg.into(),
            after_prev: false,
            fix: Some(TokenFix {
                label: label.into(),
                insert: insert.into(),
            }),
        };
    }

    /// Report `msg` on the slot's token.
    pub(crate) fn fill(self, p: &mut Parser<'_>, msg: impl Into<String>) {
        p.events[self.pos as usize] = Event::Error {
            msg: msg.into(),
            after_prev: false,
            fix: None,
        };
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

#[derive(Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: u32,
}

impl CompletedMarker {
    /// The kind this node was completed as.
    pub(crate) fn kind(self, p: &Parser<'_>) -> SyntaxKind {
        match p.events[self.pos as usize] {
            Event::Start { kind, .. } => kind,
            _ => unreachable!(),
        }
    }

    /// The node [`Self::precede`] wrapped around this one, if any.
    pub(crate) fn parent(self, p: &Parser<'_>) -> Option<CompletedMarker> {
        match p.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => forward_parent.map(|distance| CompletedMarker {
                pos: self.pos + distance,
            }),
            _ => unreachable!(),
        }
    }

    /// Re-kind a completed node whose children read the same either way
    /// (`PATH_TYPE` and `PATH_EXPR`, say).
    pub(crate) fn change_kind(self, p: &mut Parser<'_>, new: SyntaxKind) {
        match &mut p.events[self.pos as usize] {
            Event::Start { kind, .. } => *kind = new,
            _ => unreachable!(),
        }
    }

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
