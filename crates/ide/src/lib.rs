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
pub use line_index::LineIndex;
pub use syntax::TextEdit;
use syntax::ast::AstNode as _;
use syntax::{TextRange, TextSize};
pub use syntax_highlighting::{HlMods, HlRange, HlTag};

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
    /// Companion diagnostics at locations that explain an error elsewhere
    /// (see the synthesis in [`Analysis::diagnostics`]). Information rather
    /// than hint severity: editors render information as a visible (blue)
    /// underline, while hints are typically not drawn at all (Zed).
    Info,
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
        let mut diagnostics: Vec<Diagnostic> = hir::file_diagnostics(&self.db, file)
            .into_iter()
            .map(|d| Diagnostic {
                range: d.range,
                severity: match d.severity {
                    hir::Severity::Error => Severity::Error,
                    hir::Severity::Warning => Severity::Warning,
                },
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
            let Some(message) = const_eval_message(err) else {
                continue;
            };
            // Report at the error's origin — which may be inside another
            // item (e.g. the partner of a cycle). A diagnostic of `file`
            // must never carry a range from a different file, so a foreign
            // origin falls back to this item's own initializer.
            let origin_in_file = err
                .origin
                .clone()
                .filter(|(loc, _)| loc.file == file)
                .or_else(|| {
                    let root = hir::body::body(&self.db, item).root?;
                    Some((hir::item_loc(&self.db, item), root))
                });
            let Some(range) = origin_in_file.and_then(|(loc, expr)| {
                let (_, source_map) = hir::body_with_source_map(&self.db, loc.to_id(&self.db));
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
        // `const { … }` blocks are compile-time wherever they sit —
        // including inside functions nothing calls. Their check-time
        // evaluations surface failures here, same shape as the item loop
        // above. When a block at initializer level fails, `const_value`
        // reports the identical error at the identical origin; the final
        // dedup collapses the pair.
        for &item in hir::file_item_ids(&self.db, file) {
            for (block_expr, result) in eval::const_block_values(&self.db, item) {
                let Err(err) = result else {
                    continue;
                };
                let Some(message) = const_eval_message(err) else {
                    continue;
                };
                // A foreign origin (the failure lives in another file's
                // item) falls back to the const block itself, which is
                // always in this file.
                let (loc, expr) = err
                    .origin
                    .clone()
                    .filter(|(loc, _)| loc.file == file)
                    .unwrap_or_else(|| (hir::item_loc(&self.db, item), *block_expr));
                let (_, source_map) = hir::body_with_source_map(&self.db, loc.to_id(&self.db));
                let Some(range) = source_map.node_for_expr(expr).map(|ptr| ptr.text_range()) else {
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
        }
        // Related locations only travel as `DiagnosticRelatedInformation`,
        // which some editors render as bare underlines with no visible link
        // back to the error. Give each one a companion information-severity
        // diagnostic that spells out the connection and points back —
        // hovering the underline then explains itself.
        let line_index = self.line_index(file);
        let companions: Vec<Diagnostic> = diagnostics
            .iter()
            .flat_map(|d| {
                let line = line_index.line_col(d.range.start()).line + 1;
                d.related
                    .iter()
                    // A related location in another file belongs to that
                    // file's diagnostics; synthesize it there once related
                    // locations can cross files.
                    .filter(|r| r.file == file)
                    .map(move |r| Diagnostic {
                        range: r.range,
                        severity: Severity::Info,
                        message: format!(
                            "{} — causes the error on line {}: {}",
                            r.message, line, d.message
                        ),
                        fix: None,
                        related: vec![RelatedInfo {
                            file,
                            range: d.range,
                            message: "the error reported here".to_owned(),
                        }],
                    })
            })
            .collect();
        diagnostics.extend(companions);
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

    pub fn file_text(&self, file: SourceFile) -> String {
        file.text(&self.db).clone()
    }

    /// Zero-parameter function items: the candidates for a ▶ run lens.
    /// (Functions with parameters need arguments — that's the debugger's
    /// entry-expression flow, which the editor launches, not the server.)
    pub fn run_lenses(&self, file: SourceFile) -> Vec<RunLens> {
        let mut lenses = Vec::new();
        for &item in hir::file_item_ids(&self.db, file) {
            let name = item.name(&self.db);
            if name.is_empty() {
                continue;
            }
            let hir::Ty::Fn(f) = hir::signature(&self.db, item) else {
                continue;
            };
            if !f.params.is_empty() {
                continue;
            }
            let Some(name_node) = hir::item_source(&self.db, item).and_then(|it| it.name()) else {
                continue;
            };
            lenses.push(RunLens {
                range: name_node.syntax().text_range(),
                name: name.clone(),
            });
        }
        lenses
    }
}

/// A runnable item: its name and where the lens anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLens {
    pub range: TextRange,
    pub name: String,
}

/// The user-facing message for a const-eval failure, or `None` for
/// [`eval::EvalErrorKind::Trap`]: a trap *is* an already-reported diagnostic
/// that execution ran into, so the const-eval layer must add nothing.
fn const_eval_message(err: &eval::EvalError) -> Option<String> {
    Some(match err.kind {
        eval::EvalErrorKind::Trap => return None,
        eval::EvalErrorKind::Panic => {
            format!("constant evaluation panicked: {}", err.message)
        }
        eval::EvalErrorKind::Runtime | eval::EvalErrorKind::NotConst => {
            format!("constant evaluation failed: {}", err.message)
        }
    })
}

/// Run `f`, turning a salsa cancellation unwind (an edit invalidated this
/// snapshot) into `None`. Any other panic propagates.
pub fn cancellable<T>(f: impl FnOnce() -> T) -> Option<T> {
    salsa::Cancelled::catch(std::panic::AssertUnwindSafe(f)).ok()
}

#[cfg(test)]
mod tests;
