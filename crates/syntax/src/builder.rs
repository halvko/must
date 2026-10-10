//! Turns parser events plus the raw (trivia-carrying) token stream back into
//! a lossless rowan green tree, and gives parser errors their text positions.

use crate::SyntaxKind::{self, *};
use crate::lexer::Token;
use crate::parser::Event;
use crate::{Fix, SyntaxError, TextEdit};
use rowan::{GreenNode, GreenNodeBuilder};
use text_size::{TextRange, TextSize};

/// The tree, the errors, and for each error the range of the node the
/// parser was building when it reported it (`None` for the lexer's).
pub(crate) fn build(
    text: &str,
    tokens: &[Token],
    mut events: Vec<Event>,
    mut errors: Vec<SyntaxError>,
) -> (GreenNode, Vec<SyntaxError>, Vec<Option<TextRange>>) {
    let lexer_errors = errors.len();
    let mut builder = Builder {
        inner: GreenNodeBuilder::new(),
        text,
        tokens,
        raw_pos: 0,
        offset: TextSize::new(0),
        prev_token_range: TextRange::empty(TextSize::new(0)),
        depth: 0,
        errors: &mut errors,
        open_starts: Vec::new(),
        error_owners: Vec::new(),
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
            Event::Error {
                msg,
                after_prev,
                fix_insert,
            } => builder.error(msg, after_prev, fix_insert),
        }
    }

    let green = builder.inner.finish();
    let mut owners = vec![None; lexer_errors];
    if !builder.error_owners.is_empty() {
        let root = crate::SyntaxNode::new_root(green.clone());
        owners.extend(
            builder
                .error_owners
                .into_iter()
                .map(|(start, depth)| Some(node_at(&root, start, depth).text_range())),
        );
    }
    (green, errors, owners)
}

/// The node `depth` levels below `root` that begins at `start`.
fn node_at(root: &crate::SyntaxNode, start: TextSize, depth: usize) -> crate::SyntaxNode {
    let mut node = root.clone();
    for _ in 0..depth {
        let child = node
            .children()
            .filter(|child| child.text_range().start() <= start)
            .last();
        match child {
            Some(child) => node = child,
            None => break,
        }
    }
    node
}

struct Builder<'a> {
    inner: GreenNodeBuilder<'static>,
    text: &'a str,
    tokens: &'a [Token],
    raw_pos: usize,
    offset: TextSize,
    /// The last non-trivia token emitted; "missing X after this" errors
    /// anchor on its last character (a visible, cursor-targetable range)
    /// and insert at its end.
    prev_token_range: TextRange,
    depth: usize,
    errors: &'a mut Vec<SyntaxError>,
    /// Where each open node begins, innermost last.
    open_starts: Vec<TextSize>,
    /// For each error this builder pushed, the node it was reported in, as
    /// its start and its depth below the root.
    error_owners: Vec<(TextSize, usize)>,
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
        self.open_starts.push(self.offset);
    }

    fn finish_node(&mut self) {
        self.depth -= 1;
        if self.depth == 0 {
            // Trailing trivia belongs to the root.
            self.eat_trivia();
        }
        self.inner.finish_node();
        self.open_starts.pop();
    }

    fn token(&mut self) {
        self.eat_trivia();
        self.do_token();
    }

    fn error(&mut self, message: String, after_prev: bool, fix_insert: Option<String>) {
        // "Missing X after this token" is noise when that token is itself
        // broken (e.g. an unterminated string). A prior after-prev error on
        // the same token intersects it directly, which is what drops the
        // bogus "expected `;`" for `static = fn {` (the "expected `}`" is
        // anchored on the `{`'s last character). Touching still counts for
        // the remaining EOF cases, where a plain error's empty range sits
        // at the token's end. The range-equal dedup in `parse` only
        // settles what this leaves behind.
        if after_prev
            && self
                .errors
                .iter()
                .any(|e| e.range.intersect(self.prev_token_range).is_some())
        {
            return;
        }
        let range = if after_prev {
            // Anchor on the previous token's *last character*: visible, a
            // cursor can sit on it, and it doesn't read as "this whole
            // token is wrong". The fix still inserts after it.
            last_char_range(self.text, self.prev_token_range)
        } else {
            // Point at the token the parser was looking at; an empty range
            // at the end of the text if there is none.
            let mut pos = self.raw_pos;
            let mut offset = self.offset;
            while let Some(token) = self.tokens.get(pos) {
                if !token.kind.is_trivia() {
                    break;
                }
                offset += token.len;
                pos += 1;
            }
            match self.tokens.get(pos) {
                Some(token) => TextRange::at(offset, token.len),
                None => TextRange::empty(TextSize::of(self.text)),
            }
        };
        let fix_at = if after_prev {
            TextRange::empty(self.prev_token_range.end())
        } else {
            range
        };
        let fix = fix_insert.map(|insert| Fix {
            label: format!("Insert `{insert}`"),
            edits: vec![TextEdit {
                range: fix_at,
                insert,
            }],
        });
        // The owner is the innermost open node that began before the
        // error: one begun AT the offending token may be the next construct.
        let depth = self
            .open_starts
            .iter()
            .rposition(|&start| start < range.start())
            .unwrap_or(0);
        let start = self.open_starts.get(depth).copied().unwrap_or_default();
        self.errors.push(SyntaxError {
            message,
            range,
            fix,
        });
        self.error_owners.push((start, depth));
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
        self.inner.token(token.kind.into(), &self.text[range]);
        self.offset += token.len;
        self.raw_pos += 1;
        if !token.kind.is_trivia() {
            self.prev_token_range = range;
        }
    }
}

/// The range of the last character (not byte) in `range`.
fn last_char_range(text: &str, range: TextRange) -> TextRange {
    match text[range].char_indices().last() {
        Some((i, _)) => TextRange::new(range.start() + TextSize::new(i as u32), range.end()),
        None => range,
    }
}
