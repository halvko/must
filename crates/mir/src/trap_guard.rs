//! Every error traps: for each error the toolchain reports on a corpus of
//! programs, some MIR body of the file carries a trap with exactly that
//! message. The corpus is `examples/` plus every program the hir and syntax
//! diagnostic tests check.

use std::collections::BTreeSet;

use base_db::{RootDatabase, SourceFile};
use expect_test::expect;

use crate::TerminatorKind;

/// The error messages of `text` that no trap in the file carries.
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
        for (_, body) in lowered.bodies.iter() {
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
    errors.retain(|message| !traps.contains(message));
    errors
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
    let mut gaps = BTreeSet::new();
    for text in &corpus {
        gaps.extend(untrapped(text));
    }
    let rendered: String = gaps.iter().map(|message| format!("{message}\n")).collect();
    // TODO: halvko/must#52 — errors in a declaration with no body (a
    // `type`, a `trait`, an impl head), errors outside every item, and loan
    // errors do not trap. This list may only shrink.
    expect![[r#"
        `D` is a trait; an impl in a trait's `with`-chain names the IMPLEMENTING type
        `Gen` is a reserved generic trait (generic traits are not supported yet); it cannot be implemented
        `N` is a const parameter, not a type
        `Pair` takes 1 generic argument, found 0
        `five` is not a type
        `forget` is the default ceiling: a type whose values may be dropped on the floor needs no `only` clause
        `move` and `forget` are rungs of the same ladder — a declaration ceils one ladder once
        `move` is already this declaration's ceiling
        `unsafe` trait members are not supported yet
        `usize` is not a trait
        `with T: ...` constrained groups are not supported yet
        `x` is not a type
        a `const { ... }` block cannot parameterize a type; pass the value through a generic function's const parameter instead
        a `type` declaration's field declares a type, not a value
        a `type` declaration's parameters carry no capability bounds: a container is linear when what it holds is
        a capability ceiling belongs on a `type` declaration, not on a `trait`
        a capability clause has no place on a const parameter: a const parameter's values are always plain data
        a capability clause has no place on a region parameter: a region names a duration, not a value, so it has no capability to speak of
        a declare-only inherent member is an unimplementable promise; define it: `name = fn(...) -> ... { ... };`
        a field's type must be a fully written type; a declaration has nothing to infer `_` from
        a safe borrow must name its region (`T.&::<@a>`); regions are not elided in a signature yet
        a type's const argument must be a literal or a const parameter name
        a variant payload must be a fully written type; a declaration has nothing to infer `_` from
        associated types are not supported yet
        borrow types are spelled postfix: `T.&` / `T.&mut`
        borrowed value does not live long enough: this borrows a local, but the borrow is still live when the body returns and the local is gone by then
        bounds on a `type` declaration's binder are not supported yet
        duplicate generic parameter `T`
        duplicate impl of `D` for `P`
        duplicate impl of `D` for `usize`
        duplicate requirement `m`
        expected `=` followed by the item's value
        expected `extern static` after `unsafe`: the marker vouches for an `extern` item's declared signature, and only an `extern` item declares one
        expected `}`
        expected a capability name after `only` (`only move`)
        expected a type
        expected a type for field `b`: `name: Type`
        expected an item (`static`, `const`, `type`, `trait`, `extern` or `unsafe`)
        field visibility is not supported yet
        generic traits are not supported yet
        impls for generic types are not supported yet
        only a `static` can be `extern`: an `extern` item declares one name with one type
        only a `struct` or `enum` literal can declare a type
        only an `extern static` can be `unsafe`: the marker vouches for an `extern` item's declared signature, and nothing else declares one
        record fields are defined with `=` (`name = value`); `:` annotates a type
        region parameters come first in a binder; move `@a` before `T`
        region parameters on type declarations are not supported yet
        requirement `go` must spell its full signature: every parameter and the return type
        requirement `n` must spell its full signature: every parameter and the return type
        the `access` capability does not exist yet; `move` is the only ceiling that can be written
        the `send` capability does not exist yet; `move` is the only ceiling that can be written
        the `without` clause is retired: write the ceiling instead (`only move` where you wrote `without forget`)
        this impl of `D` is missing the member `m`
        unexpected character `\`
        unknown capability `leak`; `move` is the only ceiling that can be written
        unknown trait `Display`
        unknown trait `Missing`
        unknown type `Missing`
        unknown type `Unknown`
        unknown type `missing`
        unterminated block comment: expected a closing `*/`
        unterminated character literal: expected a closing `'`
    "#]].assert_eq(&rendered);
}
