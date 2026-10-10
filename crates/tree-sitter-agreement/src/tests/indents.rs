//! The indent queries, run over the house-formatted corpus by a port of
//! each editor's indenter (Zed's `suggest_autoindents`, Helix's tree-sitter
//! heuristic as far as the query uses it): every line must be suggested the
//! indentation it has, which is where a pasted or typed line then lands.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use tree_sitter::{
    Node, Query, QueryCursor, QueryMatch, QueryPredicateArg, StreamingIterator, Tree,
};

use super::{corpus, language, line_col, parse, read, repo_root};

const HELIX: &str = "editors/tree-sitter-must/queries/indents.scm";
const ZED: &str = "editors/zed/languages/must/indents.scm";

/// `syntax::INDENT_UNIT`'s width, the `indent` both editors are given.
const UNIT: usize = 4;

fn query(path: &str) -> Query {
    Query::new(&language(), &read(&repo_root().join(path)))
        .unwrap_or_else(|e| panic!("{path} does not compile against the grammar: {e}"))
}

/// One corpus file, cut into lines.
struct Lines<'a> {
    text: &'a str,
    /// The byte each line starts at.
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Lines { text, starts }
    }

    fn line(&self, row: usize) -> &'a str {
        let end = self.starts.get(row + 1).map_or(self.text.len(), |&e| e - 1);
        &self.text[self.starts[row]..end]
    }

    fn indent(&self, row: usize) -> usize {
        let line = self.line(row);
        line.len() - line.trim_start_matches(' ').len()
    }

    fn is_blank(&self, row: usize) -> bool {
        self.line(row).trim().is_empty()
    }

    /// The line's first non-blank byte.
    fn first_byte(&self, row: usize) -> usize {
        self.starts[row] + self.indent(row)
    }
}

/// The rows that begin inside a comment or string opened on an earlier
/// row: their leading whitespace is content, not indentation.
fn literal_rows(tree: &Tree) -> BTreeSet<usize> {
    fn walk(node: Node<'_>, rows: &mut BTreeSet<usize>) {
        if matches!(node.kind(), "block_comment" | "string") {
            rows.extend(node.start_position().row + 1..=node.end_position().row);
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, rows);
        }
    }
    let mut rows = BTreeSet::new();
    walk(tree.root_node(), &mut rows);
    rows
}

/// Each line's indentation as the editor suggests it, `None` where it
/// suggests nothing.
type Model = fn(&Query, &Tree, &Lines<'_>) -> Vec<Option<usize>>;

/// Zed: a line goes one level past the previous non-blank line when an
/// `@indent` range starts on that line and runs past this one's start; it
/// goes back to the line a range started on when that range ends (at its
/// `@end`, else the node's end) between the previous line's start and its
/// own.
fn zed(query: &Query, tree: &Tree, lines: &Lines<'_>) -> Vec<Option<usize>> {
    type Point = (usize, usize);
    let point = |p: tree_sitter::Point| (p.row, p.column);
    let names = query.capture_names();
    let mut ranges: Vec<(Point, Point)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), lines.text.as_bytes());
    while let Some(m) = matches.next() {
        let (mut start, mut end) = (None, None);
        for capture in m.captures {
            let node = capture.node;
            match names[capture.index as usize] {
                "indent" => {
                    start.get_or_insert(point(node.start_position()));
                    end.get_or_insert(point(node.end_position()));
                }
                "start" => start = Some(point(node.end_position())),
                "end" => end = Some(point(node.start_position())),
                other => panic!("{ZED} uses @{other}, which this model does not know"),
            }
        }
        if let (Some(start), Some(end)) = (start, end)
            && start.0 != end.0
        {
            match ranges.binary_search_by_key(&start, |r| r.0) {
                Err(ix) => ranges.insert(ix, (start, end)),
                Ok(ix) => ranges[ix].1 = ranges[ix].1.max(end),
            }
        }
    }

    (0..lines.starts.len())
        .map(|row| {
            let prev = (0..row).rev().find(|&r| !lines.is_blank(r))?;
            let row_start = (row, lines.indent(row));
            let prev_start = (prev, lines.indent(prev));
            let mut indent = false;
            let mut outdent_to = usize::MAX;
            for &(start, end) in ranges.iter().take_while(|r| r.0.0 < row) {
                if start.0 == prev && end > row_start {
                    indent = true;
                }
                if end > prev_start && end <= row_start {
                    outdent_to = outdent_to.min(start.0);
                }
            }
            Some(if outdent_to == prev {
                lines.indent(prev)
            } else if indent {
                lines.indent(prev) + UNIT
            } else if outdent_to < prev {
                lines.indent(outdent_to)
            } else {
                lines.indent(prev)
            })
        })
        .collect()
}

/// Helix: a line is one level in for every earlier line on which an
/// `@indent` node containing it starts, and one level out when it starts
/// with an `@outdent` token. `#not-same-line?` is the one predicate the
/// query uses; `@sign` and `@value` exist only to feed it.
fn helix(query: &Query, tree: &Tree, lines: &Lines<'_>) -> Vec<Option<usize>> {
    let names = query.capture_names();
    let mut indents = Vec::new();
    let mut outdents = BTreeSet::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), lines.text.as_bytes());
    while let Some(m) = matches.next() {
        if !holds(query, m) {
            continue;
        }
        for capture in m.captures {
            match names[capture.index as usize] {
                "indent" => indents.push(capture.node.byte_range()),
                "outdent" => {
                    outdents.insert(capture.node.start_byte());
                }
                "sign" | "value" => {}
                other => panic!("{HELIX} uses @{other}, which this model does not know"),
            }
        }
    }

    (0..lines.starts.len())
        .map(|row| {
            let at = lines.first_byte(row);
            let opened: BTreeSet<usize> = indents
                .iter()
                .filter(|r| r.start < lines.starts[row] && r.end > at)
                .map(|r| line_col(lines.text, r.start).0)
                .collect();
            let levels = opened.len() - usize::from(outdents.contains(&at) && !opened.is_empty());
            Some(levels * UNIT)
        })
        .collect()
}

/// Corpus lines laid out by hand rather than by the house style, as
/// (sample, first line, last line, why).
const HAND_LAID_OUT: &[(&str, usize, usize, &str)] = &[(
    "examples/borrows.must",
    206,
    210,
    "an `else { if` chain kept flat, one condition per line",
)];

/// Whether `m` satisfies its pattern's `#not-same-line?` predicates, the
/// only kind the queries use (the tree-sitter crate leaves them to us).
fn holds(query: &Query, m: &QueryMatch<'_, '_>) -> bool {
    let node_of = |ix: u32| m.captures.iter().find(|c| c.index == ix).map(|c| c.node);
    query.general_predicates(m.pattern_index).iter().all(|p| {
        assert_eq!(&*p.operator, "not-same-line?", "unknown predicate");
        let [QueryPredicateArg::Capture(a), QueryPredicateArg::Capture(b)] = &p.args[..] else {
            panic!("unexpected arguments to #{}", p.operator);
        };
        match (node_of(*a), node_of(*b)) {
            (Some(a), Some(b)) => a.start_position().row != b.start_position().row,
            _ => true,
        }
    })
}

fn check(path: &str, model: Model) {
    let query = query(path);
    let mut failures = String::new();
    for sample in corpus::samples() {
        let tree = parse(&sample.text);
        let lines = Lines::new(&sample.text);
        let literal = literal_rows(&tree);
        let suggested = model(&query, &tree, &lines);
        for (row, suggestion) in suggested.into_iter().enumerate() {
            let Some(suggestion) = suggestion else {
                continue;
            };
            // Zed measures from the line above, so a line after one whose
            // indentation is content has no basis either.
            let prev = (0..row).rev().find(|&r| !lines.is_blank(r));
            let hand_laid_out = HAND_LAID_OUT.iter().any(|&(name, first, last, _)| {
                name == sample.name && (first..=last).contains(&(row + 1))
            });
            if lines.is_blank(row)
                || literal.contains(&row)
                || prev.is_some_and(|prev| literal.contains(&prev))
                || hand_laid_out
                || suggestion == lines.indent(row)
            {
                continue;
            }
            writeln!(
                failures,
                "{}:{}: indented {}, suggested {suggestion}: {}",
                sample.name,
                row + 1,
                lines.indent(row),
                lines.line(row).trim(),
            )
            .unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "{path} re-indents lines of the corpus away from where they are:\n{failures}"
    );
}

#[test]
fn zed_indents_reproduce_the_corpus() {
    check(ZED, zed);
}

#[test]
fn helix_indents_reproduce_the_corpus() {
    check(HELIX, helix);
}

/// Every pattern of both indent queries matches somewhere in the corpus.
#[test]
fn every_indent_pattern_is_matched() {
    let mut unmatched = String::new();
    for path in [HELIX, ZED] {
        let query = query(path);
        let source = read(&repo_root().join(path));
        let mut matched = BTreeSet::new();
        for sample in corpus::samples() {
            let tree = parse(&sample.text);
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, tree.root_node(), sample.text.as_bytes());
            while let Some(m) = matches.next() {
                if holds(&query, m) {
                    matched.insert(m.pattern_index);
                }
            }
        }
        for pattern in (0..query.pattern_count()).filter(|p| !matched.contains(p)) {
            let start = query.start_byte_for_pattern(pattern);
            let (line, _) = line_col(&source, start);
            let text = source[start..].lines().next().unwrap_or_default();
            writeln!(unmatched, "{path}:{line}: {text}").unwrap();
        }
    }
    assert!(
        unmatched.is_empty(),
        "nothing in the corpus matches these indent patterns, so nobody checks them. \
         Add a snippet to crates/tree-sitter-agreement/src/tests/corpus.rs or delete \
         the pattern:\n{unmatched}"
    );
}
