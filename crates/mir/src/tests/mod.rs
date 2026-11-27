mod basics;
mod borrows;
mod enums_and_match;
mod escapes;
mod generics;
mod loans;
mod loops_and_return;
mod places;
mod pointers_and_arrays;
mod storage;

use base_db::{RootDatabase, SourceFile};
use expect_test::{Expect, expect};

/// Renders every item's lowered MIR, followed by MIR's own diagnostics with
/// ranges attached the way the aggregator does it (via the source map).
fn check_mir(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let name = item.name(&db);
        rendered.push_str(&format!(
            "item {}:\n",
            if name.is_empty() { "<unnamed>" } else { name }
        ));
        let lowered = crate::mir_lowered(&db, item);
        rendered.push_str(&crate::pretty::render(lowered));
        let (_, source_map) = hir::body_with_source_map(&db, item);
        for diag in &lowered.diagnostics {
            if let Some(ptr) = source_map.node_for_expr(diag.expr()) {
                rendered.push_str(&format!(
                    "mir diagnostic at {:?}: {}\n",
                    ptr.text_range(),
                    diag.message()
                ));
            }
        }
    }
    expect.assert_eq(&rendered);
}

// ---- loan liveness: the static exclusivity fence -------------------------

/// Renders every loan-check finding in the file, then every move-check
/// finding — range, message, then each companion as a note with its range
/// — after any hir diagnostic, so a program that fails to type-check
/// cannot pass as "accepted".
fn check_loans(text: &str, expect: Expect) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let mut rendered = String::new();
    for diag in hir::file_diagnostics(&db, file) {
        rendered.push_str(&format!("{:?}: {}\n", diag.range, diag.message));
    }
    for item in hir::all_checkable_items(&db, file) {
        let (_, source_map) = hir::body_with_source_map(&db, item);
        for diag in crate::loan_check(&db, item) {
            let Some(range) = diag.range(&db, item) else {
                continue;
            };
            rendered.push_str(&format!("{range:?}: {}\n", diag.message()));
            for (range, note) in diag.related(&db, item) {
                rendered.push_str(&format!("  note at {range:?}: {note}\n"));
            }
        }
        for diag in crate::move_check(&db, item) {
            let Some(range) = diag.range(source_map) else {
                continue;
            };
            rendered.push_str(&format!("{range:?}: {}\n", diag.message(&db, item)));
            for (range, note) in diag.related(&db, item, source_map) {
                rendered.push_str(&format!("  note at {range:?}: {note}\n"));
            }
        }
    }
    expect.assert_eq(&rendered);
}

/// Where `mir_checked` plants each trap one CFG checker found: the
/// operation the trap's target block starts with must be the one the
/// finding names in the lowered MIR.
pub(crate) fn check_traps(
    text: &str,
    traps: for<'db> fn(&'db dyn base_db::Db, hir::ItemId<'db>) -> &'db [crate::CheckTrap],
    expect: Expect,
) {
    let db = RootDatabase::default();
    let file = SourceFile::new(&db, "test.must".to_owned(), text.to_owned());
    let operation = |block: &crate::BlockData, index: usize| match block.statements.get(index) {
        Some(stmt) => ("statement", stmt.origin),
        None => ("terminator", block.terminator.origin),
    };
    let mut rendered = String::new();
    for &item in hir::file_item_ids(&db, file) {
        let (_, source_map) = hir::body_with_source_map(&db, item);
        let lowered = crate::mir_lowered(&db, item);
        let mut named: Vec<_> = traps(&db, item)
            .iter()
            .map(|trap| {
                let (kind, origin) =
                    operation(&lowered.bodies[trap.body].blocks[trap.block], trap.index);
                (trap.message.clone(), kind, origin)
            })
            .collect();
        let messages: Vec<&String> = named.iter().map(|(message, ..)| message).collect();
        let mut planted = Vec::new();
        for (_, body) in crate::mir_checked(&db, item).bodies.iter() {
            for (_, block) in body.blocks.iter() {
                if let crate::TerminatorKind::Trap {
                    message, target, ..
                } = &block.terminator.kind
                    && messages.contains(&message)
                {
                    let (kind, origin) = operation(&body.blocks[*target], 0);
                    planted.push((message.clone(), kind, origin));
                }
            }
        }
        named.sort_by_key(|(message, _, origin)| (message.clone(), u32::from(origin.into_raw())));
        planted.sort_by_key(|(message, _, origin)| (message.clone(), u32::from(origin.into_raw())));
        assert_eq!(named, planted, "a trap sits before the wrong operation");
        for (message, kind, origin) in planted {
            let range = source_map.node_for_expr(origin).map(|ptr| ptr.text_range());
            rendered.push_str(&format!("{range:?} {kind}: {message}\n"));
        }
    }
    expect.assert_eq(&rendered);
}
