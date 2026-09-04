# Platform, codegen and tooling

## Conclusions

- **P09** The command surface. `run` exits 0, 1 for a trap, panic or runtime error, or 2
  for a usage or file-IO failure; `check` likewise. Warnings never affect the exit code.
  Failure kinds have fixed prefix words. The frame limit is 10,000, and the message quotes
  the number.
- **P11** The debugger runs in-process on the const-eval interpreter (X08): same MIR, same
  machine, same traps.

## Discarded

## Re-evaluate when

- **Two open debug-adapter bugs**, not decisions: with loops and unfueled run mode an infinite
  loop hangs the session with no interrupt path; and breakpoint arrivals are deduped by
  frame/line/column, so a breakpoint in a loop body fires once per frame. **P11**
