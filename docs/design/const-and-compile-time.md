# Const and compile time

## Conclusions

- **C02** Constness is written, not inferred. A fn literal is const only with an explicit
  `const fn` marker, and a call whose value is not provably const is rejected rather than
  speculatively evaluated.
- **C03** Const evaluation runs on a fuel budget, per item and with failures memoized, so
  a runaway item costs its own budget once; run mode is unfueled. Const blocks inside
  uncalled functions are forced at check time, so their fuel-outs and traps are editor
  diagnostics rather than latent runtime crashes; dead code can emit const-eval errors.

## Discarded

## Re-evaluate when
