use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ide::{AnalysisHost, HlTag};
use sha2::{Digest, Sha256};
use syntax::SyntaxKind;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

mod corpus;

unsafe extern "C" {
    fn tree_sitter_must() -> *const ();
}

fn language() -> Language {
    // SAFETY: `tree_sitter_must` is the entry point of the parser that
    // `build.rs` compiles and links into this crate.
    unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_must) }.into()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn grammar_dir() -> PathBuf {
    repo_root().join("editors/tree-sitter-must")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn canonical_highlights() -> String {
    read(&grammar_dir().join("queries/highlights.scm"))
}

fn parse(text: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&language())
        .expect("the generated parser's ABI is one the tree-sitter crate loads");
    parser.parse(text, None).expect("parsing without a timeout")
}

// ---- the agreement rule ------------------------------------------------------

/// The semantic class a capture stands for, by its longest dotted prefix.
/// A capture with no entry (`label`, `punctuation`, `variable.other.member`)
/// has no semantic counterpart and is only ever allowed through
/// [`TREE_SITTER_ONLY`] or [`REFINEMENTS`].
const CLASSES: &[(&str, HlTag)] = &[
    ("comment", HlTag::Comment),
    ("string", HlTag::String),
    ("constant.character", HlTag::String),
    ("constant.numeric", HlTag::Number),
    ("constant.builtin.boolean", HlTag::Keyword),
    ("keyword", HlTag::Keyword),
    ("operator", HlTag::Operator),
    ("function", HlTag::Function),
    ("variable.parameter", HlTag::Parameter),
    ("variable", HlTag::Variable),
    ("type.enum.variant", HlTag::EnumMember),
    ("type.parameter", HlTag::TypeParameter),
    ("type.interface", HlTag::Trait),
    ("type", HlTag::Type),
];

/// Captures in [`CLASSES`]' namespace that deliberately have no class.
const CLASSLESS: &[&str] = &["variable.other.member"];

/// One allowed disagreement. `position` scopes a row to one syntactic
/// position, written `parent_kind.field` as [`position`] renders it;
/// `only_in` scopes it to the one corpus file whose deliberately broken
/// code needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Row {
    position: Option<&'static str>,
    capture: Option<&'static str>,
    tag: Option<&'static str>,
    only_in: Option<&'static str>,
}

const fn refines(position: &'static str, capture: &'static str, tag: &'static str) -> Row {
    Row {
        position: Some(position),
        capture: Some(capture),
        tag: Some(tag),
        only_in: None,
    }
}

const fn tree_sitter_only(capture: &'static str) -> Row {
    Row {
        position: None,
        capture: Some(capture),
        tag: None,
        only_in: None,
    }
}

/// Semantic knows more: at this position the capture is the most syntax
/// can say, and the tag is what resolution or inference made of it. A
/// position syntax does decide has no row, so its capture must equal the tag.
const REFINEMENTS: &[Row] = &[
    refines("path_expr.head", "variable", "Parameter"),
    refines("path_expr.head", "variable", "Function"),
    refines("path_expr.head", "variable", "Type"),
    refines("path_expr.head", "variable", "Trait"),
    refines("path_expr.head", "variable", "TypeParameter"),
    refines("path_expr.segment", "variable", "Function"),
    refines("path_expr.segment", "variable", "EnumMember"),
    refines("static_item.name", "variable", "Function"),
    refines("path_type.head", "type", "Trait"),
    refines("path_type.head", "type", "TypeParameter"),
    refines("with_clause.name", "type", "TypeParameter"),
    refines("field_expr.field", "variable.other.member", "Function"),
];

/// Tree-sitter styles a token the semantic layer leaves plain.
const TREE_SITTER_ONLY: &[Row] = &[
    tree_sitter_only("punctuation.bracket"),
    tree_sitter_only("punctuation.delimiter"),
    tree_sitter_only("variable.other.member"),
    tree_sitter_only("label"),
    // A name that resolves to nothing gets no semantic tag.
    Row {
        position: Some("path_expr.head"),
        capture: Some("variable"),
        tag: None,
        only_in: Some("snippet `unresolved names`"),
    },
];

/// Semantic styles a token tree-sitter leaves plain.
const SEMANTIC_ONLY: &[Row] = &[];

fn class_of(capture: &str) -> Option<HlTag> {
    if CLASSLESS.contains(&capture) {
        return None;
    }
    CLASSES
        .iter()
        .filter(|(prefix, _)| {
            capture == *prefix
                || capture
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|&(_, tag)| tag)
}

fn table_rows() -> impl Iterator<Item = &'static Row> {
    REFINEMENTS
        .iter()
        .chain(TREE_SITTER_ONLY)
        .chain(SEMANTIC_ONLY)
}

/// Whether `capture` (a name and its position) on a token may sit beside
/// `tag`, recording the table row that allowed it.
fn agrees(
    file: &str,
    capture: Option<(&str, &str)>,
    tag: Option<HlTag>,
    used: &RefCell<BTreeSet<Row>>,
) -> bool {
    if capture.is_none() && tag.is_none() {
        return true;
    }
    if let (Some((name, _)), Some(tag)) = (capture, tag)
        && class_of(name) == Some(tag)
    {
        return true;
    }
    let tag = tag.map(|tag| format!("{tag:?}"));
    let allowed = table_rows().find(|row| {
        row.capture == capture.map(|(name, _)| name)
            && row.tag == tag.as_deref()
            && row
                .position
                .is_none_or(|at| capture.is_some_and(|(_, position)| at == position))
            && row.only_in.is_none_or(|only| only == file)
    });
    if let Some(row) = allowed {
        used.borrow_mut().insert(*row);
    }
    allowed.is_some()
}

// ---- tree-sitter's side -----------------------------------------------------

/// One capture of the highlights query.
struct Capture<'t> {
    name: String,
    pattern: usize,
    node: Node<'t>,
    depth: usize,
    position: String,
}

/// Where `node` sits: its parent's kind and the field it fills there.
fn position(node: Node<'_>) -> String {
    let Some(parent) = node.parent() else {
        return node.kind().to_owned();
    };
    let mut cursor = parent.walk();
    cursor.goto_first_child();
    while cursor.node() != node && cursor.goto_next_sibling() {}
    match cursor.field_name() {
        Some(field) => format!("{}.{field}", parent.kind()),
        None => parent.kind().to_owned(),
    }
}

fn depth(node: Node<'_>) -> usize {
    std::iter::successors(node.parent(), |n| n.parent()).count()
}

fn captures<'t>(query: &Query, tree: &'t Tree, text: &str) -> Vec<Capture<'t>> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());
    let mut found = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            found.push(Capture {
                name: query.capture_names()[capture.index as usize].to_owned(),
                pattern: m.pattern_index,
                node: capture.node,
                depth: depth(capture.node),
                position: position(capture.node),
            });
        }
    }
    found
}

/// The winning capture per byte, by the rule Helix (25.07 and later) and
/// Zed share: the innermost captured node wins, and on one node the later
/// pattern wins.
fn paint<'c>(captures: &'c [Capture<'_>], len: usize) -> Vec<Option<&'c Capture<'c>>> {
    let mut ordered: Vec<&Capture<'_>> = captures.iter().collect();
    ordered.sort_by_key(|c| (c.depth, c.pattern));
    let mut bytes = vec![None; len];
    for capture in ordered {
        for slot in &mut bytes[capture.node.byte_range()] {
            *slot = Some(capture);
        }
    }
    bytes
}

// ---- the comparison ---------------------------------------------------------

#[derive(Default)]
struct Comparison {
    failures: Vec<String>,
    used: RefCell<BTreeSet<Row>>,
    /// The query patterns that matched somewhere in the corpus.
    matched: BTreeSet<usize>,
}

fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let col = before.rfind('\n').map_or(offset, |nl| offset - nl - 1) + 1;
    (line, col)
}

fn compare(sample: &corpus::Sample, query: &Query, out: &mut Comparison) {
    let text = &sample.text;
    let tree = parse(text);
    let captures = captures(query, &tree, text);
    let painted = paint(&captures, text.len());
    out.matched.extend(captures.iter().map(|c| c.pattern));

    let mut host = AnalysisHost::new();
    let file = host.create_file(sample.name.clone(), text.clone());
    let semantic = host.snapshot().highlight(file);

    let (tokens, _) = syntax::tokenize(text);
    let mut start = 0;
    for token in tokens {
        let range = start..start + usize::from(token.len);
        start = range.end;
        if token.kind == SyntaxKind::WHITESPACE {
            continue;
        }
        let tags: BTreeSet<Option<String>> = {
            let inside: BTreeSet<_> = semantic
                .iter()
                .filter(|hl| range.contains(&usize::from(hl.range.start())))
                .map(|hl| Some(format!("{:?}", hl.tag)))
                .collect();
            if inside.is_empty() {
                BTreeSet::from([None])
            } else {
                inside
            }
        };
        let tag_of = |name: &Option<String>| {
            semantic
                .iter()
                .find(|hl| {
                    range.contains(&usize::from(hl.range.start()))
                        && Some(format!("{:?}", hl.tag)) == *name
                })
                .map(|hl| hl.tag)
        };
        // A multiline token's line breaks belong to no semantic range and
        // to whatever tree-sitter painted the token with.
        let winners: BTreeSet<Option<(&str, &str)>> = painted[range.clone()]
            .iter()
            .map(|c| c.map(|c| (c.name.as_str(), c.position.as_str())))
            .collect();
        for winner in &winners {
            for tag in &tags {
                if agrees(&sample.name, *winner, tag_of(tag), &out.used) {
                    continue;
                }
                let (line, col) = line_col(text, range.start);
                let mut message = format!(
                    "{}:{line}:{col} bytes {}..{} `{}`\n    tree-sitter: {}\n    semantic:    {}\n",
                    sample.name,
                    range.start,
                    range.end,
                    &text[range.clone()],
                    winner.map_or("(no capture)".to_owned(), |(name, position)| format!(
                        "{name} at {position}"
                    )),
                    tag.as_deref().unwrap_or("(no tag)"),
                );
                for capture in captures.iter().filter(|c| {
                    let node = c.node.byte_range();
                    node.start < range.end && range.start < node.end
                }) {
                    writeln!(
                        message,
                        "    captured as @{} by pattern {} on ({})",
                        capture.name,
                        capture.pattern,
                        capture.node.kind(),
                    )
                    .unwrap();
                }
                out.failures.push(message);
            }
        }
    }
}

fn highlights_query() -> Query {
    Query::new(&language(), &canonical_highlights())
        .expect("queries/highlights.scm compiles against the grammar")
}

fn compare_corpus() -> Comparison {
    let query = highlights_query();
    let mut comparison = Comparison::default();
    for sample in corpus::samples() {
        compare(&sample, &query, &mut comparison);
    }
    comparison
}

/// Every `ERROR` and `MISSING` node under `node`, as `line:col kind`.
fn broken_nodes(node: Node<'_>, found: &mut Vec<String>) {
    if node.is_error() || node.is_missing() {
        let at = node.start_position();
        let what = if node.is_missing() {
            "MISSING"
        } else {
            "ERROR"
        };
        found.push(format!(
            "{}:{} {what} {}",
            at.row + 1,
            at.column + 1,
            node.kind()
        ));
    }
    if node.has_error() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            broken_nodes(child, found);
        }
    }
}

/// The grammar parses every corpus file completely.
#[test]
fn corpus_parses_without_error_or_missing_nodes() {
    let mut failures = String::new();
    for sample in corpus::samples() {
        let mut found = Vec::new();
        broken_nodes(parse(&sample.text).root_node(), &mut found);
        for node in found {
            writeln!(failures, "{}:{node}", sample.name).unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "tree-sitter could not parse part of the corpus; teach \
         editors/tree-sitter-must/grammar.js the syntax, then `npm run generate`:\n{failures}"
    );
}

/// Every token's tree-sitter capture and semantic tag are the same class,
/// or a row of the tables allows the pair.
#[test]
fn tree_sitter_agrees_with_semantic_highlighting() {
    const SHOWN: usize = 40;
    let comparison = compare_corpus();
    assert!(
        comparison.failures.is_empty(),
        "{} tokens are highlighted differently by tree-sitter \
         (editors/tree-sitter-must/queries/highlights.scm) and by the server \
         (crates/ide/src/syntax_highlighting.rs); the first {SHOWN} at most:\n\n{}",
        comparison.failures.len(),
        comparison.failures[..comparison.failures.len().min(SHOWN)].join("\n"),
    );
}

/// No table row outlives the disagreement it was added for.
#[test]
fn every_agreement_table_row_is_exercised() {
    let comparison = compare_corpus();
    let used = comparison.used.borrow();
    let stale: Vec<_> = table_rows().filter(|row| !used.contains(row)).collect();
    assert!(
        stale.is_empty(),
        "stale table rows: nothing in the corpus needs them, so they only \
         hide future disagreements. Remove them or add a corpus snippet: {stale:#?}"
    );
}

/// Every pattern of the canonical query matches somewhere in the corpus, so
/// each of its claims is one the agreement test has checked.
#[test]
fn every_highlight_pattern_is_matched() {
    let query = highlights_query();
    let source = canonical_highlights();
    let matched = compare_corpus().matched;
    let mut unmatched = String::new();
    for pattern in (0..query.pattern_count()).filter(|p| !matched.contains(p)) {
        let start = query.start_byte_for_pattern(pattern);
        let (line, _) = line_col(&source, start);
        let text = source[start..].lines().next().unwrap_or_default();
        writeln!(unmatched, "highlights.scm:{line}: {text}").unwrap();
    }
    assert!(
        unmatched.is_empty(),
        "nothing in the corpus matches these patterns of \
         editors/tree-sitter-must/queries/highlights.scm, so nobody checks them. Add a \
         snippet to crates/tree-sitter-agreement/src/tests/corpus.rs or delete the \
         pattern:\n{unmatched}"
    );
}

/// Every capture name in the canonical query is one the agreement rule
/// knows how to judge.
#[test]
fn every_capture_has_a_class_or_a_table_row() {
    let query = highlights_query();
    for name in query.capture_names() {
        let known = class_of(name).is_some() || table_rows().any(|row| row.capture == Some(*name));
        assert!(
            known,
            "capture @{name} has no class in CLASSES and no table row"
        );
    }
}

/// The corpus uses every keyword of the language, so a new keyword fails
/// here until the corpus, and through it the grammar and the query, know it.
#[test]
fn corpus_exercises_every_keyword() {
    let mut seen = BTreeSet::new();
    for sample in corpus::samples() {
        let (tokens, _) = syntax::tokenize(&sample.text);
        seen.extend(tokens.iter().map(|t| t.kind).filter(|k| k.is_keyword()));
    }
    for &(text, kind) in syntax::KEYWORDS {
        assert!(
            seen.contains(&kind),
            "keyword `{text}` is not in the corpus: add a snippet that uses it to \
             crates/tree-sitter-agreement/src/tests/corpus.rs and teach \
             editors/tree-sitter-must/grammar.js and queries/highlights.scm the keyword"
        );
    }
}

/// `src/parser.c` was generated from the `grammar.js` next to it.
#[test]
fn generated_parser_matches_grammar_js() {
    let grammar = std::fs::read(grammar_dir().join("grammar.js")).unwrap();
    let actual: String = Sha256::digest(&grammar)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let recorded = read(&grammar_dir().join("src/grammar.js.sha256"));
    assert_eq!(
        recorded.trim(),
        actual,
        "editors/tree-sitter-must/grammar.js changed after src/parser.c was generated: \
         run `npm run generate` in editors/tree-sitter-must"
    );
}

// ---- the editors' copies ----------------------------------------------------

/// Zed's name for each capture of the canonical (Helix-named) query, by
/// longest dotted prefix.
const ZED_CAPTURES: &[(&str, &str)] = &[
    ("comment", "comment"),
    ("string", "string"),
    ("constant.character", "string"),
    ("constant.character.escape", "string.escape"),
    ("constant.numeric", "number"),
    ("constant.builtin.boolean", "boolean"),
    ("keyword", "keyword"),
    ("operator", "operator"),
    ("punctuation.bracket", "punctuation.bracket"),
    ("punctuation.delimiter", "punctuation.delimiter"),
    ("label", "lifetime"),
    ("function", "function"),
    ("variable", "variable"),
    ("variable.parameter", "variable.parameter"),
    ("variable.other.member", "property"),
    ("type", "type"),
    ("type.builtin", "type.builtin"),
    ("type.interface", "type.interface"),
    ("type.parameter", "type"),
    ("type.enum.variant", "variant"),
];

fn zed_capture(capture: &str) -> &'static str {
    ZED_CAPTURES
        .iter()
        .filter(|(prefix, _)| {
            capture == *prefix
                || capture
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|&(_, zed)| zed)
        .unwrap_or_else(|| panic!("capture @{capture} has no Zed name in ZED_CAPTURES"))
}

/// The canonical query with comments dropped and captures renamed for Zed.
fn zed_highlights(canonical: &str) -> String {
    let mut out = String::from(
        "; GENERATED from editors/tree-sitter-must/queries/highlights.scm. Do not edit:\n\
         ; change that file, then run\n\
         ; `UPDATE_EXPECT=1 cargo test -p tree-sitter-agreement`.\n\n",
    );
    for line in canonical.lines() {
        let mut rendered = String::new();
        let mut chars = line.chars().peekable();
        let mut in_string = false;
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    in_string = !in_string;
                    rendered.push(c);
                }
                '\\' if in_string => {
                    rendered.push(c);
                    rendered.extend(chars.next());
                }
                ';' if !in_string => break,
                '@' if !in_string => {
                    let mut name = String::new();
                    while let Some(&c) = chars.peek()
                        && (c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
                    {
                        name.push(c);
                        chars.next();
                    }
                    rendered.push('@');
                    rendered.push_str(zed_capture(&name));
                }
                c => rendered.push(c),
            }
        }
        let rendered = rendered.trim_end();
        if !rendered.is_empty() {
            out.push_str(rendered);
            out.push('\n');
        }
    }
    out
}

/// Zed's `highlights.scm` is the canonical query under Zed's capture names.
#[test]
fn zed_queries_are_the_canonical_queries_renamed() {
    let zed = repo_root().join("editors/zed/languages/must/highlights.scm");
    expect_test::expect_file![zed].assert_eq(&zed_highlights(&canonical_highlights()));
}

/// Every query file an editor loads compiles against the grammar.
#[test]
fn editor_query_files_compile() {
    for path in [
        "editors/tree-sitter-must/queries/highlights.scm",
        "editors/zed/languages/must/highlights.scm",
        "editors/zed/languages/must/brackets.scm",
    ] {
        if let Err(e) = Query::new(&language(), &read(&repo_root().join(path))) {
            panic!("{path} does not compile against the grammar: {e}");
        }
    }
}

/// The value of the first `rev = "..."` in a TOML file.
fn pinned_rev(path: &str) -> String {
    let text = read(&repo_root().join(path));
    let (_, after) = text
        .split_once("rev = \"")
        .unwrap_or_else(|| panic!("{path} pins no grammar `rev`"));
    after.split('"').next().unwrap().to_owned()
}

/// Zed and Helix build the grammar from the same commit.
#[test]
fn editors_pin_the_same_grammar_rev() {
    assert_eq!(
        pinned_rev("editors/zed/extension.toml"),
        pinned_rev("editors/helix/languages.toml"),
        "editors/zed/extension.toml and editors/helix/languages.toml must pin one `rev`"
    );
}
