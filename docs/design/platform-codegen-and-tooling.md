# Platform, codegen and tooling

## Conclusions

- **P09** The command surface. `run` exits 0, 1 for a trap or runtime error, or 2 for a
  usage or file-IO failure. Failure kinds have fixed prefix words.
- **P11** The debugger runs in-process on the const-eval interpreter (X08): same MIR, same
  machine, same traps.

## Discarded

## Re-evaluate when
