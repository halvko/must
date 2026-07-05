# Allocators and the heap

## Conclusions

- **A07** Type-level allocator brands are rejected: a brand parameter is viral through every
  signature that stays precise. Static instance-distinctness is deferred to regions, where it
  should fall out of region identity rather than be a second mechanism. The runtime
  same-allocator assert stays even then: region equality proves outlives, the comparison
  proves identity, and two handles can share a region without being the same allocator.

## Discarded

## Re-evaluate when
