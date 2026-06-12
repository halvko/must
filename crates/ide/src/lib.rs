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
pub use syntax::TextEdit;
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

/// A quick fix whose edits each name their target file. The syntax layer's
/// fixes carry bare ranges (it has no notion of files); the ide layer is
/// where they get pinned to a file — so the LSP layer *cannot* apply an
/// edit to the wrong document, the type forces the resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub label: String,
    pub edits: Vec<FileEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub file: SourceFile,
    pub edit: TextEdit,
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
        let mut diagnostics: Vec<Diagnostic> = hir::file_diagnostics(&self.db, file)
            .into_iter()
            .map(|d| Diagnostic {
                range: d.range,
                severity: Severity::Error,
                message: d.message,
                // Syntax-level fixes always edit the file they diagnosed.
                fix: d.fix.map(|fix| Fix {
                    label: fix.label,
                    edits: fix
                        .edits
                        .into_iter()
                        .map(|edit| FileEdit { file, edit })
                        .collect(),
                }),
                related: d.related,
            })
            .collect();
        // MIR is a diagnostic producer like any other analysis (it finds
        // what only the CFG can see — today: unsupported captures). Same
        // aggregator pattern: findings travel with the query value, only
        // ranges attach here. Lives above hir because hir can't see mir.
        for &item in hir::file_item_ids(&self.db, file) {
            let lowered = mir::mir_lowered(&self.db, item);
            if lowered.diagnostics.is_empty() {
                continue;
            }
            let (_, source_map) = hir::body_with_source_map(&self.db, item);
            for diag in &lowered.diagnostics {
                let Some(ptr) = source_map.node_for_expr(diag.expr()) else {
                    continue;
                };
                diagnostics.push(Diagnostic {
                    range: ptr.text_range(),
                    severity: Severity::Error,
                    message: diag.message(),
                    fix: None,
                    related: Vec::new(),
                });
            }
        }
        // Const-eval failures. `Trap` errors are skipped: a trap *is* an
        // already-reported diagnostic that execution ran into.
        for &item in hir::file_item_ids(&self.db, file) {
            let Err(err) = eval::const_value(&self.db, item) else {
                continue;
            };
            let message = match err.kind {
                eval::EvalErrorKind::Trap => continue,
                eval::EvalErrorKind::Panic => {
                    format!("constant evaluation panicked: {}", err.message)
                }
                eval::EvalErrorKind::Runtime | eval::EvalErrorKind::NotConst => {
                    format!("constant evaluation failed: {}", err.message)
                }
            };
            // Report at the error's origin — which may be inside another
            // item (e.g. the partner of a cycle). A diagnostic of `file`
            // must never carry a range from a different file, so a foreign
            // origin falls back to this item's own initializer.
            let origin_in_file = err
                .origin
                .filter(|(loc, _)| loc.file == file)
                .or_else(|| {
                    let root = hir::body::body(&self.db, item).root?;
                    Some((hir::item_loc(&self.db, item), root))
                });
            let Some(range) = origin_in_file.and_then(|(loc, expr)| {
                let origin_item = loc.to_id(&self.db)?;
                let (_, source_map) = hir::body_with_source_map(&self.db, origin_item);
                Some(source_map.node_for_expr(expr)?.text_range())
            }) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                range,
                severity: Severity::Error,
                message,
                fix: None,
                related: Vec::new(),
            });
        }
        diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
        // Every item that (transitively) uses a failing constant propagates
        // the same error with the same origin; report it once.
        diagnostics.dedup_by(|a, b| a.range == b.range && a.message == b.message);
        diagnostics
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
