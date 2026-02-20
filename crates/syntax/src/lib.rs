//! Lossless, error-resilient syntax trees for Must.
//!
//! This crate is deliberately free of salsa and LSP dependencies: it maps
//! `&str` to a rowan green tree plus a list of errors, and nothing else.
