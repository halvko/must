# Design docs

What Must decided and why, one file per area. Each file has three registers:

- **Conclusions** — the rulings in force, each tagged with a stable id (`X02`, `G01`, ...) so
  other docs and code comments can cite it.
- **Discarded** — the alternatives that lost, with the reason, so they are not re-litigated
  without new information.
- **Re-evaluate when** — the conditions under which a ruling is worth reopening.

The language reference is `docs/main.typ`; these files record decisions, not usage.
