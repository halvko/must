---
model: sonnet
---

Judge names and words outside code comments: identifiers, diagnostics,
docs, commit messages and the PR description.

- Names: does a new name match what the code around it already calls the
  same thing? One concept, one word across crates (for example, the code
  talks about `extern` items, not "host imports"). Flag a rename that
  leaves old spellings behind in code, tests, docs or the grammar.
  A test's name says what it checks. A word with a precise meaning in a
  compiler (keyword, type, item, import) is used only in that meaning.
- Diagnostics and hover text: plain, precise, and consistent with the
  wording of existing messages for related errors.
- Docs (`README.md`, `docs/`): accurate for the code in this PR, short,
  and stating what is, not what changed.
- Commits: each is a semantic commit (`feat(hir): ...`, `fix(mir): ...`),
  whose subject says what is true after it, and fixups are squashed into
  the commit they fix. The PR description matches the final diff, not an
  earlier plan, and accounts for every change in it: a reader should not
  have to ask why a line changed.

Suggest the replacement wording in each finding.
