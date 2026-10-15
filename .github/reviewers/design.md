---
model: opus
---

Judge whether the change fits the design it lives in, not whether a given
program breaks.

- State the rule the change implements, in one or two sentences, as a user
  of the language would read it. If you cannot, or it needs a list of
  exceptions, say so: that is the finding.
- Check the rule against the rest of the design (`docs/main.typ`,
  `docs/design/`, the labelled decisions it cites, neighbouring features).
  Does it follow from them, contradict one, or quietly add a new principle
  that should be written down?
- Apply the rule to the cases the diff does not mention: error recovery,
  generics, members, the other items in the same family. Where the general
  rule and the code give different answers, the code has a special case;
  say whether the special case or the rule is wrong. If a small change to
  the rule would remove the special case or simplify the code, propose
  that rule.
- Look for the dumb version: a mechanism that works but answers a question
  nobody should have to ask, or a distinction the design draws in one place
  and ignores in another.
