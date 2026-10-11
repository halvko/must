# Modules

## Conclusions

- **M01** A file is a module and its own namespace: it sees its own items, the prelude (M03), and
  what it imports with `use` (G30), nothing else. The same name declared in two files is two items.
  An import that names a module the library lacks, or an item that module does not declare or
  declares more than once, is an error at the `use`, and an unresolved name points at the modules
  that declare a name of a kind the position takes (a type for an annotation, a trait for a bound, a
  type or a trait before `::`). A name the file declares or imports more than once is an error at
  every place it comes from, none more to blame than another; `as` renames an import to resolve
  that. An import names a top-level item of the imported module, never that module's own imports. A
  file in no library is a library by itself.
- **M02** A library is the files of one directory tree. `foo.must` is module `foo`, and the
  directory `foo/` beside it holds `foo`'s children: `foo/bar.must` is `foo::bar`. There are no
  `mod` declarations and no `mod.rs`: the files are the module tree. A `use` path is the
  module's path from the library's root, the same from every file, or, starting with `self` or
  `super`, from the importer: `self::x` is its child `x`, and each leading `super` goes one
  module up, a root module's parent being the library. An item is private unless it is marked
  `pub`, and a private item is visible to its module's parent and everything under it (ruled,
  not built); a root module's parent is the library, so only nesting draws a boundary.
  Encapsulation is wider than one file, so code whose soundness depends on what can touch it,
  such as an unsafe abstraction, keeps that guarantee while spread over several files.
- **M05** What spans the library is what no name has to reach: binding groups (T11) follow the
  resolved names across files, so a fully typed caller in one file fills a hole in another,
  and trait impls and coherence are library-wide, so a dot-call needs no import of the trait
  that answers it. A trait is imported only to name it, as a bound or an impl head, or to tell
  two traits apart. Items exported across the library boundary need written signatures.
- **M03** The compiler-provided declarations (T23) live in one prelude file per database, so
  every module names the same `ReadLineResult`.
- **M04** References are indexed eagerly: as files are analysed, each records what it refers
  to, and the reverse map answers "who refers to this" (ruled, not built). Find-references,
  rename and unused-item diagnostics read it, and a library can ship its index precomputed.

## Discarded

- **One scope shared by every file of a directory** — splitting a file needs no imports, but a
  file collides with names it never asked for, and a reader cannot tell where a name comes
  from. **M01**
- **Glob imports (`use file::*`)** — convenient for a standard library's common types, but
  they bring back the collisions M01 removes. **M01**
- **Fully typed callers fill holes only in their own file** — Flow removed cross-file call-site
  inference for speed and for errors in unopened files. Here the big-group warning and "insert
  inferred signature" bound the cost, and #149 shows every file's errors. **M05**
- **A dot-call needs its trait imported** — the impl is already unique library-wide, so the
  import would only repeat it. **M05**
- **`mod` declarations, or `mod.rs` beside `foo.rs`** — two ways to lay out one tree, and a
  declaration that only repeats what the files already say. **M02**
- **One flat scope per library, no nesting** — simplest, but leaves no boundary inside a library
  to encapsulate behind. **M02**
- **A file as the unit of privacy** — ties an encapsulation boundary to how code happens to be
  split into files. **M02**
- **Callers fill holes across a library without written signatures at its boundary** — a
  library's users would see its types change with their own code. **M05**
- **Finding references by a per-file name prefilter, resolving on demand** — cheaper to keep up
  to date, but it cannot be precomputed for a library the way an index can. **M04**

## Re-evaluate when

- **Libraries depend on each other** — imports across the library boundary, `pub`, and the
  written contracts exported items need (T11) are ruled together. So is whether a library's
  impl of its own trait for another library's type answers dot-calls without importing the
  trait: the type should not grow methods because some library in the graph exists. **M02
  M05**
- **Groups across files are slow in practice** — callers stop filling holes across files, and
  the big-group warning says to insert signatures. **M05**
- **A standard library exists** — a prelude of its common types may want a glob-like import
  after all. **M01**
- **One namespace wants to span files** — a glob import of a sibling module is the natural way
  to extend it, so the collisions M01 avoids may be worth it there. **M01**
