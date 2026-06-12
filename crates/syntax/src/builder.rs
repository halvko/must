//! Turns parser events plus the raw (trivia-carrying) token stream back into
//! a lossless rowan green tree, and gives parser errors their text positions.

use crate::SyntaxKind::{self, *};
use crate::lexer::Token;
use crate::parser::Event;
use crate::SyntaxError;
use rowan::{GreenNode, GreenNodeBuilder};
use text_size::{TextRange, TextSize};

pub(crate) fn build(
    text: &str,
    tokens: &[Token],
    mut events: Vec<Event>,
    mut errors: Vec<SyntaxError>,
) -> (GreenNode, Vec<SyntaxError>) {
    let mut builder = Builder {
        inner: GreenNodeBuilder::new(),
        text,
        tokens,
        raw_pos: 0,
        offset: TextSize::new(0),
        depth: 0,
        errors: &mut errors,
    };

    const CONSUMED: Event = Event::Start {
        kind: TOMBSTONE,
        forward_parent: None,
    };

    let mut forward_kinds = Vec::new();
    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], CONSUMED) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                // Resolve the forward-parent chain: nodes retrofitted around
                // this one (via `precede`) must be started first.
                forward_kinds.clear();
                forward_kinds.push(kind);
                let mut fp = forward_parent;
                let mut idx = i;
                while let Some(dist) = fp {
                    idx += dist as usize;
                    fp = match std::mem::replace(&mut events[idx], CONSUMED) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            forward_kinds.push(kind);
                            forward_parent
                        }
                        _ => unreachable!(),
                    };
                }
                for kind in forward_kinds.drain(..).rev() {
                    if kind != TOMBSTONE {
                        builder.start_node(kind);
                    }
                }
            }
            Event::Token => builder.token(),
            Event::Finish => builder.finish_node(),
            Event::Error { msg } => builder.error(msg),
        }
    }

    (builder.inner.finish(), errors)
}

struct Builder<'a> {
    inner: GreenNodeBuilder<'static>,
    text: &'a str,
    tokens: &'a [Token],
    raw_pos: usize,
    offset: TextSize,
    depth: usize,
    errors: &'a mut Vec<SyntaxError>,
}

impl Builder<'_> {
    fn start_node(&mut self, kind: SyntaxKind) {
        // Attach pending trivia to the parent, so node ranges start at real
        // content. The root must cover the whole text, so trivia before it
        // stays pending until the first inner node or token.
        if self.depth > 0 {
            self.eat_trivia();
        }
        self.inner.start_node(kind.into());
        self.depth += 1;
    }

    fn finish_node(&mut self) {
        self.depth -= 1;
        if self.depth == 0 {
            // Trailing trivia belongs to the root.
            self.eat_trivia();
        }
        self.inner.finish_node();
    }

    fn token(&mut self) {
        self.eat_trivia();
        self.do_token();
    }

    fn error(&mut self, message: String) {
        // Point at the token the parser was looking at; an empty range at the
        // end of the text if there is none.
        let mut pos = self.raw_pos;
        let mut offset = self.offset;
        while let Some(token) = self.tokens.get(pos) {
            if !token.kind.is_trivia() {
                break;
            }
            offset += token.len;
            pos += 1;
        }
        let range = match self.tokens.get(pos) {
            Some(token) => TextRange::at(offset, token.len),
            None => TextRange::empty(TextSize::of(self.text)),
        };
        self.errors.push(SyntaxError {
            message,
            range,
            fix: None,
        });
    }

    fn eat_trivia(&mut self) {
        while let Some(token) = self.tokens.get(self.raw_pos) {
            if !token.kind.is_trivia() {
                break;
            }
            self.do_token();
        }
    }

    fn do_token(&mut self) {
        let token = self.tokens[self.raw_pos];
        let range = TextRange::at(self.offset, token.len);
        self.inner
            .token(token.kind.into(), &self.text[range]);
        self.offset += token.len;
        self.raw_pos += 1;
    }
}
