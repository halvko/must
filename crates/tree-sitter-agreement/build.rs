//! Compiles the committed tree-sitter parser for Must, so the tests can
//! load the same C that Zed and Helix build.

use std::path::Path;

fn main() {
    let src = Path::new("../../editors/tree-sitter-must/src");
    let parser = src.join("parser.c");
    let scanner = src.join("scanner.c");
    println!("cargo:rerun-if-changed={}", parser.display());
    println!("cargo:rerun-if-changed={}", scanner.display());
    cc::Build::new()
        .include(src)
        .file(&parser)
        .file(&scanner)
        .warnings(false)
        .compile("tree-sitter-must");
}
