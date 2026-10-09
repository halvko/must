---
model: sonnet
---

Check the diff against `docs/conventions.md`, rule by rule. Read that file
first; it is written so each rule can be checked on its own.

In particular:

- Comments: no provenance (rulings, dates, "per the brief", agents,
  reviews), no time words ("now", "no longer", "still", "for now", "yet",
  "until"), no comparisons to sibling items, five lines at most, and no
  retelling of a design argument that belongs in `docs/design/`. A
  `TODO:` names its issue (`halvko/must#NN`).
- Tests: programs are raw strings at column zero, indented by brace
  depth, with the expectation beside them (expect-test).
- Fast path: work only a diagnostic needs happens where the diagnostic is
  produced, not on the path correct code takes.

Quote the rule each finding breaks. Report only lines the diff adds or
changes, not existing violations.
