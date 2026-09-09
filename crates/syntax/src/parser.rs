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
}

pub(crate) struct Parser<'t> {
    tokens: &'t [SyntaxKind],
    /// The source text of each token in [`Self::tokens`], same indexing.
    /// The parser is KIND-driven everywhere but one place: a RETIRED
    /// spelling lexes as an ordinary identifier, and the only way to
    /// refuse it by name — instead of desyncing on tokens that are each
    /// individually fine — is to look at the word (see
    /// [`Parser::at_word`]).
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
