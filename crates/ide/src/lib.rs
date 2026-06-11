//! Editor-agnostic IDE features over the analysis database.
//!
//! This crate speaks `TextSize`/`TextRange` and its own result types; the
//! LSP layer converts at the boundary. It must never depend on lsp-types.

use base_db::{RootDatabase, SourceFile};
pub use line_index::LineIndex;
use syntax::TextRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Owns the mutable database. The LSP main loop applies edits through
/// [`AnalysisHost::db_mut`] (bumping the salsa revision) and answers
/// requests off [`AnalysisHost::snapshot`]s.
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

    pub fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
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
}

/// A read-only snapshot of the analysis state at some revision. Cheap to
/// create; queries on it unwind with `salsa::Cancelled` if the host applies
/// an edit in the meantime.
pub struct Analysis {
    db: RootDatabase,
}

impl Analysis {
    pub fn diagnostics(&self, file: SourceFile) -> Vec<Diagnostic> {
        base_db::parse(&self.db, file)
            .errors()
            .iter()
            .map(|err| Diagnostic {
                range: err.range,
                severity: Severity::Error,
                message: err.message.clone(),
            })
            .collect()
    }

    pub fn line_index(&self, file: SourceFile) -> LineIndex {
        LineIndex::new(file.text(&self.db))
    }
}
