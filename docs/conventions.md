# Conventions

Small rules for code in this repository, phrased so a reviewer can check a
diff against them one by one. Design decisions and their reasoning are not
here; they live in `docs/design/`.

## Comments

- A comment says what the code does, or why it has the shape it has. It
  records no provenance: no owner rulings, no dates, no "per the brief", no
  references to agents, reviews or rounds. Where a decision came from is a
  design-doc matter (`docs/design/*.md` keeps conclusions, discarded
  alternatives and re-evaluate lines).
- A comment is short: one or two lines is normal, five is the ceiling,
  module docs included. It does not retell the argument for a design; that
  lives in `docs/design/` and is cited by label (`M12`). It does not repeat
  what the code or a message string next to it already says.
- A comment carries no time. It describes the code as it is, not as a
  change ("no longer", "not any more", "now", "still": the commit message's
  job), and it does not say how long that will hold ("for now", "yet",
  "interim", "permanent", "until", "today"). Something meant to change is
  a `TODO:` that names its issue (`halvko/must#16`); everything else reads
  as standing.
- A comment describes the item it sits on. It does not compare the item to
  a sibling ("the twin of", "narrower than"): a comparison is a claim about
  both items and stops being true when either one changes.
- A test's doc comment says what the test checks, and stops there.
- A comment that says this code does the same as some other place is a
  sign the code should be that other place. Share the code and drop the
  comment.

## Tests

- A test program is a raw string, `r#"..."#`, opening on its own line with
  the program at column zero and indented by brace depth. No `"...\n\`
  line continuations: they hide every `"` behind a backslash and silently
  flatten the indentation the compiler sees.
- The program and its expectation stay together in the test (expect-test).
  A program lives in a `.must` file only when it ships in `examples/` and
  the test includes that file to check the shipped copy.
