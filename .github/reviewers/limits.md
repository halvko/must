---
model: opus
---

Judge the limits the change puts on what a user can write: every new
error, trap, refusal, unsupported case or fallback a program can hit,
whether it is in code, a diagnostic or a test expectation.

- Would someone writing must expect the program to work? If so, is the
  limit a rule of the language (`docs/main.typ`, `docs/design/`) or a gap
  in this implementation? A gap is a finding: say what supporting it would
  take, or that it needs an issue.
- A message for one special shape (a nested pattern, one context) where
  the general mechanism should handle every shape is a gap, not a feature.
- An error or trap that stops more than the broken code (every item of a
  module for one bad import, say) is a limit too: it should reach only the
  code that uses what is broken.
- Two spellings of one program (`::Some(x)`, `Option::Some(x)`) that
  check differently are a gap.
- A test that pins a limit counts as the limit.
- A program the change accepts that should have been an error earlier (a
  signature naming a region nothing bounds, say) is a finding too.

Give each finding the program a user would write and what they would
expect it to do.
