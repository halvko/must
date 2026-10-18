mod arrays;
mod assignment;
mod bare_angle;
mod basics;
mod block_statements;
mod capabilities;
mod const_items;
mod destructuring;
mod expressions;
mod extern_items;
mod generics;
mod imports;
mod lexer;
mod loops_and_return;
mod matches;
mod patterns;
mod pointers;
mod records;
mod recovery;
mod regions;
mod traits;
mod turbofish;
mod type_decls;
mod unsafe_and_fn_types;
mod with_chains;

use expect_test::{Expect, expect};

fn check(input: &str, expect: Expect) {
    let parse = crate::parse(input);
    expect.assert_eq(&parse.debug_dump());
}

/// The tree's node/token kinds in order, with offsets and text dropped —
/// enough to say two parses have the same SHAPE without pinning the
/// offsets a `::` shifts.
fn node_kinds(input: &str) -> Vec<String> {
    crate::parse(input)
        .debug_dump()
        .lines()
        .filter_map(|line| line.split_once('@'))
        .map(|(kind, _)| kind.trim().to_owned())
        .collect()
}

fn without_colon2(kinds: Vec<String>) -> Vec<String> {
    kinds.into_iter().filter(|kind| kind != "COLON2").collect()
}

fn messages(input: &str) -> Vec<String> {
    crate::parse(input)
        .errors()
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

fn prepend(first: &str, rest: Vec<String>) -> Vec<String> {
    std::iter::once(first.to_owned()).chain(rest).collect()
}

/// The errors a snippet produces, one per line — for the character literal
/// shapes below, where the TREE is uninteresting (one `CHAR` token either
/// way) and the message is the whole point.
fn check_errors(input: &str, expect: Expect) {
    let parse = crate::parse(input);
    let text: String = parse
        .errors()
        .iter()
        .map(|err| {
            format!(
                "{:?}..{:?}: {}\n",
                u32::from(err.range.start()),
                u32::from(err.range.end()),
                err.message
            )
        })
        .collect();
    expect.assert_eq(&text);
}

/// Applies every edit in `fix` to `text` (rightmost-first, so earlier
/// offsets stay valid) and returns the result — the plainest way to pin
/// what a multi-edit `Fix` actually produces.
fn apply_fix(text: &str, fix: &crate::Fix) -> String {
    let mut edits: Vec<&crate::TextEdit> = fix.edits.iter().collect();
    edits.sort_by_key(|e| std::cmp::Reverse(u32::from(e.range.start())));
    let mut out = text.to_owned();
    for edit in edits {
        let range = usize::from(edit.range.start())..usize::from(edit.range.end());
        out.replace_range(range, &edit.insert);
    }
    out
}
