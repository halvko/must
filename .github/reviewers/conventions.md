---
model: sonnet
---

Check the diff against `docs/conventions.md`, rule by rule, except its
Comments section, which is judged on its own. Read that file first; it is
written so each rule can be checked on its own.

In particular:

- Tests: multi-line programs are raw strings at column zero, indented by
  brace depth, with the expectation beside them (expect-test). A test
  string with `\n` escapes that the diff touches becomes a raw string.
- Fast path: work only a diagnostic needs happens where the diagnostic is
  produced, not on the path correct code takes.

Quote the rule each finding breaks. Report only lines the diff adds or
changes, not existing violations.
