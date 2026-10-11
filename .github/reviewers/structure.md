---
model: opus
---

Look for structural red flags: code whose shape makes it harder to reason
about, slower, or harder to abstract than it needs to be.

- Overlapping work along a call path: a caller and a callee (or several
  frames of the same trace) each validate the same input, look up the
  same data, recompute the same value, or re-check an invariant one of
  them already established. Follow the calls the diff adds or changes far
  enough to see it.
- Repeated, similarly shaped code: functions, match arms or blocks that
  differ only in a name, a type or a constant, within the diff or between
  the diff and existing code. Point at the copies.
- Logic in the wrong home: a pass or module doing work that belongs to
  another (and is likely done there too), or a value threaded through
  several layers only to be recomputed at the end.
- Work the caller does not need: a callee that builds, collects or
  computes more than its callers use (a full structure when one field is
  read, a sorted list when only the minimum is), or a loop that calls into
  something which walks a whole collection, tree or table on each
  iteration, often over data the loop does not need (a hidden quadratic, or
  a scan for one field of every item). Look at what the callee touches and
  produces, not just its signature.
- Hidden coupling: code that relies on an order, position or shape it does
  not own, such as the first entries of a list being of one kind.
- Checks bolted on afterwards: validating something after building it,
  where building it differently (parsing the shape, a type that cannot
  hold the bad state, one place that must handle every case) would rule
  the mistake out. Likewise a special case for one context when the rule
  holds in all of them, or a fallback that should fall out of the normal
  mechanism.

For each finding, name the places involved and say why it matters here:
harder to reason about, slower, or in the way of a better abstraction.
These are judgment calls, not rules: report a smell when in doubt and say
how sure you are, and the implementer weighs it.
