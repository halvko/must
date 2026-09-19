//! Keeps the tree-sitter grammar in `editors/tree-sitter-must` honest.
//!
//! Zed shows the server's semantic tokens and Helix shows tree-sitter
//! captures, so nobody looks at both. The tests here do: over a corpus they
//! require that the two highlighters agree token by token, that the
//! generated parser is current, and that the editors' query copies follow
//! the canonical one.

#[cfg(test)]
mod tests;
