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
        /// Report at the end of the previous token (where something is
        /// missing) instead of at the token the parser is looking at.
        after_prev: bool,
        /// Text whose insertion at the error position fixes the error;
        /// becomes a quick fix.
        fix_insert: Option<String>,
    },
}

pub(crate) struct Parser<'t> {
    tokens: &'t [SyntaxKind],
    pos: usize,
    events: Vec<Event>,
}

impl<'t> Parser<'t> {
    pub(crate) fn new(tokens: &'t [SyntaxKind]) -> Parser<'t> {
        Parser {
            tokens,
            pos: 0,
            events: Vec::new(),
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

    /// Expect a `;`. A missing one is reported right after the previous
    /// token — where it should be typed — with an insert fix.
    pub(crate) fn expect_semicolon(&mut self) -> bool {
        if self.eat(SEMICOLON) {
            return true;
        }
        self.error_missing_semicolon();
        false
    }

    pub(crate) fn error_missing_semicolon(&mut self) {
        self.events.push(Event::Error {
            msg: "expected `;`".to_owned(),
            after_prev: true,
            fix_insert: Some(";".to_owned()),
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
