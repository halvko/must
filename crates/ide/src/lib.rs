//! Editor-agnostic IDE features over the analysis database.
//!
//! This crate speaks `TextSize`/`TextRange` and its own result types; the
//! LSP layer converts at the boundary. It must never depend on lsp-types.

mod goto_definition;
mod hover;
mod syntax_highlighting;

use base_db::{RootDatabase, SourceFile};
pub use goto_definition::NavigationTarget;
pub use hir::RelatedInfo;
pub use hover::HoverResult;
pub use syntax_highlighting::{HlMods, HlRange, HlTag};
pub use line_index::LineIndex;
pub use syntax::{Fix, TextEdit};
use syntax::{TextRange, TextSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePosition {
    pub file: SourceFile,
    pub offset: TextSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: Severity,
    pub message: String,
    pub fix: Option<Fix>,
    pub related: Vec<RelatedInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Owns the mutable database. The LSP main loop applies edits through
/// [`AnalysisHost::set_file_text`] (bumping the salsa revision), drops the
/// text of closed documents through [`AnalysisHost::close_file`], and answers
/// requests off [`AnalysisHost::snapshot`]s.
///
/// Every mutation of the database goes through a named operation here. There
/// is deliberately no `&mut RootDatabase` accessor: each of these bumps the
/// revision and so cancels whatever is in flight on outstanding snapshots,
/// and that is not something a caller should be able to do unnamed.
#[derive(Default)]
pub struct AnalysisHost {
    db: RootDatabase,
}

impl AnalysisHost {
    pub fn new() -> AnalysisHost {
        AnalysisHost::default()
    }

    pub fn snapshot(&self) -> Analysis {
        Analysis {
            db: self.db.clone(),
        }
    }

    pub fn create_file(&mut self, path: String, text: String) -> SourceFile {
        SourceFile::new(&self.db, path, text)
    }

    /// Applies an edit: bumps the salsa revision, cancelling in-flight
    /// queries on outstanding snapshots.
    pub fn set_file_text(&mut self, file: SourceFile, text: String) {
        use salsa::Setter as _;
        file.set_text(&mut self.db).to(text);
    }

    /// Empties the text of a document the client has closed. The handle stays
    /// valid and reads as an empty file until [`AnalysisHost::set_file_text`]
    /// gives it content again.
    ///
    /// The buffer is all that goes. Everything derived from it stays memoized
    /// under this file until a reopen displaces it — salsa has no sweep, so
    /// handing a single file's memos back would mean enumerating every
    /// file-keyed query of every crate by hand, forever.
    pub fn close_file(&mut self, file: SourceFile) {
        self.set_file_text(file, String::new());
    }
}

/// A read-only snapshot of the analysis state at some revision. Cheap to
/// create; queries on it unwind with `salsa::Cancelled` if the host applies
/// an edit in the meantime.
///
/// A closed document ([`AnalysisHost::close_file`]) answers here as an empty
/// file, and nothing in these results distinguishes it from a genuinely empty
/// one. That is sound only while the server's file universe is exactly what
/// the client has opened: anything that reaches a file the client does not
/// have open — a disk-backed VFS, say — has to give a closed file the text
/// that is on disk instead of `""`, or closed files will silently analyze as
/// empty.
pub struct Analysis {
    db: RootDatabase,
}

impl Analysis {
    pub fn diagnostics(&self, file: SourceFile) -> Vec<Diagnostic> {
        hir::file_diagnostics(&self.db, file)
            .into_iter()
            .map(|d| Diagnostic {
                range: d.range,
                severity: Severity::Error,
                message: d.message,
                fix: d.fix,
                related: d.related,
            })
            .collect()
    }

    pub fn goto_definition(&self, pos: FilePosition) -> Option<NavigationTarget> {
        goto_definition::goto_definition(&self.db, pos)
    }

    pub fn hover(&self, pos: FilePosition) -> Option<HoverResult> {
        hover::hover(&self.db, pos)
    }

    /// Semantic highlighting for the whole file.
    pub fn highlight(&self, file: SourceFile) -> Vec<HlRange> {
        syntax_highlighting::highlight(&self.db, file)
    }

    pub fn line_index(&self, file: SourceFile) -> LineIndex {
        LineIndex::new(file.text(&self.db))
    }
}

/// Run `f`, turning a salsa cancellation unwind (an edit invalidated this
/// snapshot) into `None`. Any other panic propagates.
pub fn cancellable<T>(f: impl FnOnce() -> T) -> Option<T> {
    salsa::Cancelled::catch(std::panic::AssertUnwindSafe(f)).ok()
}

#[cfg(test)]
mod tests;
