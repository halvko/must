---
model: sonnet
---

Judge the tests the diff adds or changes.

- Is the expected result right? Read each test's program as someone
  writing must would and say whether they would expect that output,
  error or trap. A test that pins a wrong result pins a bug, whatever its
  name says. A known limitation is pinned only under a `TODO:` naming
  its issue (`docs/conventions.md`); flag one pinned without.
- A changed expectation in an existing test: does the PR explain it, or
  does it hide a regression or step around a bug? Say which.
- A removed or moved test: is what it checked still checked somewhere?
- Each test checks one clear thing, and its name and doc comment say what.
  A reader should be able to tell which failure it would catch.

For each finding, say what the test should check instead.
