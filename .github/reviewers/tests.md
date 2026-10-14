---
model: sonnet
---

Judge the tests the diff adds or changes.

- A changed expectation in an existing test: does the PR explain it, or
  does it hide a regression or step around a bug? Say which.
- A removed or moved test: is what it checked still checked somewhere?
- Each test checks one clear thing, and its name and doc comment say what.
  A reader should be able to tell which failure it would catch.

For each finding, say what the test should check instead.
