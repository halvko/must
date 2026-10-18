//! Every error traps: for each error the toolchain reports on a corpus of
//! programs, some MIR body of the file carries a trap with exactly that
//! message. The corpus is `examples/` plus every program the hir and syntax
//! diagnostic tests and mir's loan tests check.

use std::collections::{BTreeSet, HashSet};

use base_db::{RootDatabase, SourceFile};
use expect_test::expect;
use syntax::ast::AstNode as _;
use syntax::{SyntaxKind, SyntaxNode};

use crate::TerminatorKind;

/// The error messages of `text` that no trap in the file carries. An error
/// in a declaration nothing names, in a `use` whose names nothing names, or
/// outside every item in a file where no item has a body, traps nowhere and
/// is not counted.
fn untrapped(text: &str) -> Vec<String> {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut errors: Vec<String> = hir::file_diagnostics(&db, file)
        .into_iter()
        .filter(|diag| diag.severity == hir::Severity::Error)
        .map(|diag| diag.message)
        .collect();
    let mut traps = BTreeSet::new();
    for item in hir::all_checkable_items(&db, file) {
        let lowered = crate::mir_lowered(&db, item);
        errors.extend(lowered.diagnostics.iter().map(|diag| diag.message()));
        errors.extend(
            crate::loan_check(&db, item)
                .iter()
                .map(|diag| diag.message()),
        );
        for (_, body) in crate::mir_checked(&db, item).bodies.iter() {
            for (_, block) in body.blocks.iter() {
                match &block.terminator.kind {
                    TerminatorKind::Trap { message, .. }
                    | TerminatorKind::ConstTrap { message, .. } => {
                        traps.insert(message.clone());
                    }
                    _ => {}
                }
            }
        }
    }
    let owned = hir::error_owners(&db, file);
    let spelled = spelled_names(&db, file);
    for (loc, messages) in &owned.decls {
        if !declaration_spelled(&db, &spelled, loc) {
            traps.extend(messages.iter().cloned());
        }
    }
    for named in &owned.named {
        if !named.names.iter().any(|name| spelled.names.contains(name)) {
            traps.insert(named.message.clone());
        }
    }
    let runs = hir::all_checkable_items(&db, file)
        .into_iter()
        .any(|item| hir::body::body(&db, item).root.is_some());
    if !runs {
        traps.extend(owned.outside.iter().cloned());
    }
    errors.retain(|message| !traps.contains(message));
    errors
}

/// Whether an item with a body spells the declaration at `loc`: its name
/// anywhere, or one of its members' names after a `.`. Read off the syntax
/// alone, apart from the walk that plants the traps, so a mention that walk
/// misses fails the guard instead of exempting the error.
fn declaration_spelled(db: &RootDatabase, spelled: &Spelled, loc: &hir::ItemLoc) -> bool {
    let Some(decl) = hir::item_source(db, loc.to_id(db)) else {
        return false;
    };
    spelled.names.contains(&*loc.name)
        || decl
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::MEMBER)
            .filter_map(|member| first_ident(&member))
            .any(|member| spelled.after_dot.contains(&member))
}

/// The names the items with a body spell.
struct Spelled {
    /// Every name spelled anywhere.
    names: HashSet<String>,
    /// The names spelled right after a `.`.
    after_dot: HashSet<String>,
}

fn spelled_names(db: &RootDatabase, file: SourceFile) -> Spelled {
    let mut spelled = Spelled {
        names: HashSet::new(),
        after_dot: HashSet::new(),
    };
    let name_refs = hir::all_checkable_items(db, file)
        .into_iter()
        .filter(|&item| hir::body::body(db, item).root.is_some())
        .filter_map(|item| match item.member(db) {
            Some(_) => hir::item_tree::member_source(db, item).map(|it| it.syntax().clone()),
            None => hir::item_source(db, item).map(|it| it.syntax().clone()),
        })
        .flat_map(|syntax| syntax.descendants())
        .filter(|node| node.kind() == SyntaxKind::NAME_REF);
    for name_ref in name_refs {
        let Some(text) = first_ident(&name_ref) else {
            continue;
        };
        if name_ref
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::FIELD_EXPR)
        {
            spelled.after_dot.insert(text.clone());
        }
        spelled.names.insert(text);
    }
    spelled
}

fn first_ident(node: &SyntaxNode) -> Option<String> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_owned())
}

/// The programs a test file passes to `helper` as a string literal.
fn programs(source: &str, helper: &str) -> Vec<String> {
    let mut programs = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(helper) {
        rest = &rest[at + helper.len()..];
        if let Some(program) = string_literal(rest.trim_start()) {
            programs.push(program);
        }
    }
    programs
}

/// The value of the Rust string literal `text` starts with, raw or not.
fn string_literal(text: &str) -> Option<String> {
    if let Some(raw) = text.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let body = raw[hashes..].strip_prefix('"')?;
        let end = body.find(&format!("\"{}", "#".repeat(hashes)))?;
        return Some(body[..end].to_owned());
    }
    let mut chars = text.strip_prefix('"')?.chars();
    let mut value = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '0' => value.push('\0'),
                '\n' => {
                    let rest = chars.as_str().trim_start();
                    chars = rest.chars();
                }
                'u' => {
                    let rest = chars.as_str().strip_prefix('{')?;
                    let end = rest.find('}')?;
                    value.push(char::from_u32(u32::from_str_radix(&rest[..end], 16).ok()?)?);
                    chars = rest[end + 1..].chars();
                }
                other => value.push(other),
            },
            c => value.push(c),
        }
    }
    None
}

#[test]
fn every_error_traps() {
    let mut corpus = Vec::new();
    let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut paths: Vec<_> = std::fs::read_dir(examples)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "must"))
        .collect();
    paths.sort();
    for path in paths {
        corpus.push(std::fs::read_to_string(path).unwrap());
    }
    corpus.extend(programs(
        include_str!("../../hir/src/tests.rs"),
        "check_diagnostics(",
    ));
    corpus.extend(programs(
        include_str!("../../syntax/src/tests.rs"),
        "check_errors(",
    ));
    corpus.extend(programs(include_str!("tests.rs"), "check_loans("));
    // Each program builds its own database, so the corpus splits cleanly
    // across threads.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let chunk = corpus.len().div_ceil(threads).max(1);
    let gaps: BTreeSet<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = corpus
            .chunks(chunk)
            .map(|programs| {
                scope.spawn(|| {
                    programs
                        .iter()
                        .flat_map(|text| untrapped(text))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });
    let rendered: String = gaps.iter().map(|message| format!("{message}\n")).collect();
    expect![[r#""#]].assert_eq(&rendered);
}
