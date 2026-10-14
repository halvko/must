//! Editor-agnostic IDE features over the analysis database.
//!
//! This crate speaks `TextSize`/`TextRange` and its own result types; the
//! LSP layer converts at the boundary. It must never depend on lsp-types.

mod completions;
mod goto_definition;
mod hover;
mod syntax_highlighting;

use base_db::{RootDatabase, SourceFile};
pub use completions::{CompletionItem, CompletionItemKind, CompletionTextEdit, InsertText};
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

/// Inferred text shown inline where it could be written, with the edits
/// that write the whole signature, whichever of its hints is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHint {
    pub offset: TextSize,
    pub label: String,
    /// Whether a space separates the hint from the text before it.
    pub padding_left: bool,
    pub fix_label: String,
    /// In the hint's own file.
    pub edits: Vec<TextEdit>,
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

    /// The expression count past which an inference group warns, `None`
    /// for never (see [`base_db::Settings::big_inference_group`]).
    pub fn set_big_inference_group(&mut self, expressions: Option<u32>) {
        use salsa::Setter as _;
        base_db::settings(&self.db)
            .set_big_inference_group(&mut self.db)
            .to(expressions);
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
    /// Wraps a database that has no [`AnalysisHost`] of its own — a batch
    /// tool that built its `RootDatabase` and files directly (`must
    /// compile`, checking a file it parsed itself, is the current example)
    /// still gets the full diagnostic pipeline (hir, mir, const-eval)
    /// rather than a hand-rolled subset of it.
    pub fn new(db: RootDatabase) -> Analysis {
        Analysis { db }
    }

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
                fix: d.fix.map(|fix| in_file(file, fix)),
                related: d.related,
            })
            .collect();
        // MIR is a diagnostic producer like any other analysis (it finds
        // what only the CFG can see — unsupported captures, the loan
        // liveness half of the borrow checker, and the path-sensitive half
        // of the linear check). Same aggregator pattern:
        // findings travel with the query value, only ranges attach here.
        // Lives above hir because hir can't see mir.
        let top_level = hir::file_item_ids(&self.db, file);
        for item in hir::all_checkable_items(&self.db, file) {
            // TODO: report members' lowering diagnostics too; `map` in
            // `examples/option.must` captures `f` (halvko/must#94).
            let lowered = &mir::mir_lowered(&self.db, item).diagnostics[..];
            let lowered = if top_level.contains(&item) {
                lowered
            } else {
                &[]
            };
            let loans = mir::loan_check(&self.db, item);
            let moves = mir::move_check(&self.db, item);
            let unused = mir::unused_check(&self.db, item);
            if lowered.is_empty() && loans.is_empty() && moves.is_empty() && unused.is_empty() {
                continue;
            }
            let (hir_body, source_map) = hir::body_with_source_map(&self.db, item);
            for diag in lowered {
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
            // A stale-loan refusal names three sites and the squiggle can
            // sit on only one. The companions carry the interpreter's own
            // vocabulary ("this borrow was created here"), so the static
            // refusal and the dynamic detection read as one story told at
            // two different times.
            for diag in loans {
                let Some(range) = diag.range(source_map) else {
                    continue;
                };
                let related = diag
                    .related(source_map)
                    .into_iter()
                    .filter(|(at, _)| *at != range)
                    .map(|(at, message)| RelatedInfo {
                        file,
                        range: at,
                        message: message.to_owned(),
                    })
                    .collect();
                diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Error,
                    message: diag.message(),
                    fix: None,
                    related,
                });
            }
            // A leak or a use after a move names the binding it is about,
            // so its declaration is the first note: "this is where the
            // obligation started".
            for diag in moves {
                let Some(range) = diag.range(source_map) else {
                    continue;
                };
                let related = diag
                    .related(&self.db, item, source_map)
                    .into_iter()
                    .map(|(at, message)| RelatedInfo {
                        file,
                        range: at,
                        message,
                    })
                    .collect();
                diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Error,
                    message: diag.message(&self.db, item),
                    fix: None,
                    related,
                });
            }
            for diag in unused {
                let Some(range) = diag.range(source_map) else {
                    continue;
                };
                let fix = match diag {
                    mir::UnusedDiagnostic::Unused { binding } => {
                        prefix_underscore_fix(&self.db, file, hir_body, source_map, *binding)
                    }
                    _ => None,
                };
                diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Warning,
                    message: diag.message(hir_body),
                    fix,
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
        // Turbofish const arguments are compile-time exactly like `const`
        // blocks — same loop, same shapes: failures at check time are
        // diagnostics (a panicking argument squiggles the argument), and
        // when the failing mention also sits in a forced initializer,
        // `const_value` reports the identical error at the identical
        // origin — the final dedup collapses the pair. Arguments whose
        // value needs an instance (`Uninstantiated`, inside generic
        // bodies) are skipped by `const_eval_message`.
        for &item in hir::file_item_ids(&self.db, file) {
            for (arg_expr, result) in eval::const_arg_values(&self.db, item) {
                let Err(err) = result else {
                    continue;
                };
                let Some(message) = const_eval_message(err) else {
                    continue;
                };
                // A foreign origin (the failure lives in another file's
                // item) falls back to the argument itself, which is
                // always in this file.
                let (loc, expr) = err
                    .origin
                    .clone()
                    .filter(|(loc, _)| loc.file == file)
                    .unwrap_or_else(|| (hir::item_loc(&self.db, item), *arg_expr));
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

    /// The code actions at `range` that fix no diagnostic: rewrites the
    /// user asks for, such as inserting the inferred signature of an item
    /// whose head `range` is on.
    pub fn assists(&self, file: SourceFile, range: TextRange) -> Vec<Fix> {
        hir::file_item_ids(&self.db, file)
            .iter()
            .filter_map(|&item| hir::insert_inferred_signature(&self.db, item, Some(range)))
            .map(|fix| in_file(file, fix))
            .collect()
    }

    /// The inferred parts of each signature in `file` that is not fully
    /// annotated, placed where "insert inferred signature" would write
    /// them. A filled `_` gets no hint: the user wrote it.
    pub fn inlay_hints(&self, file: SourceFile) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        for &item in hir::file_item_ids(&self.db, file) {
            let Some(sig) = hir::inferred_signature(&self.db, item, None) else {
                continue;
            };
            let edits: Vec<TextEdit> = sig.parts.iter().map(hir::SignaturePart::edit).collect();
            hints.extend(
                sig.parts
                    .iter()
                    .filter(|part| part.range.is_empty())
                    .map(|part| InlayHint {
                        offset: part.range.start(),
                        label: part.text.clone(),
                        padding_left: part.spaced,
                        fix_label: sig.label.to_owned(),
                        edits: edits.clone(),
                    }),
            );
        }
        hints
    }

    pub fn goto_definition(&self, pos: FilePosition) -> Option<NavigationTarget> {
        goto_definition::goto_definition(&self.db, pos)
    }

    pub fn hover(&self, pos: FilePosition) -> Option<HoverResult> {
        hover::hover(&self.db, pos)
    }

    /// Completions at `pos`: a speculative-parse core over this snapshot's
    /// memoized queries — see `completions` module docs.
    pub fn completions(&self, pos: FilePosition) -> Vec<CompletionItem> {
        completions::completions(&self.db, pos)
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
            // An `unsafe fn` item — a nullary `extern` item, say — has
            // exactly one possible answer to ▶, the unsafe-block refusal.
            // Offering a button whose only outcome is a diagnostic is worse
            // than offering none.
            if f.unsafe_to_call {
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
        // Not an error: a compile-time body inside a generic item read a
        // const param — its value exists only per instance (TR06), where
        // the mention's own evaluation checks it.
        eval::EvalErrorKind::Uninstantiated => return None,
        eval::EvalErrorKind::Panic => {
            format!("constant evaluation panicked: {}", err.message)
        }
        eval::EvalErrorKind::Runtime | eval::EvalErrorKind::NotConst => {
            format!("constant evaluation failed: {}", err.message)
        }
        // UB has no squiggle of its own to re-fire (unlike a trap) — in
        // const eval it IS discovered here, so it surfaces as a
        // diagnostic like a compile-time panic does. Const evaluation
        // cannot *exhibit* UB, only report it.
        eval::EvalErrorKind::UndefinedBehavior => {
            format!(
                "constant evaluation hit undefined behavior: {}",
                err.message
            )
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

/// A syntax-level fix, whose edits are all in `file`.
fn in_file(file: SourceFile, fix: syntax::Fix) -> Fix {
    Fix {
        label: fix.label,
        edits: fix
            .edits
            .into_iter()
            .map(|edit| FileEdit { file, edit })
            .collect(),
    }
}

/// "Prefix with `_`" for an unused binding: renames `x` to `_x`, or, where
/// one name both selects a record field and binds it (`{ x }`), keeps the
/// field and renames the binding (`{ x as _x }`).
fn prefix_underscore_fix(
    db: &RootDatabase,
    file: SourceFile,
    body: &hir::Body,
    source_map: &hir::BodySourceMap,
    binding: hir::BindingId,
) -> Option<Fix> {
    let name = body.bindings[binding].written_name()?;
    let root = base_db::parse(db, file).syntax_node();
    let node = source_map.node_for_binding(binding)?.to_node(&root);
    let range = node.text_range();
    let shorthand = node
        .parent()
        .and_then(syntax::ast::RecordPatField::cast)
        .is_some_and(|field| field.as_token().is_none());
    let edit = if shorthand {
        TextEdit {
            range: TextRange::empty(range.end()),
            insert: format!(" as _{name}"),
        }
    } else {
        TextEdit {
            range,
            insert: format!("_{name}"),
        }
    };
    Some(Fix {
        label: format!("Rename to `_{name}`"),
        edits: vec![FileEdit { file, edit }],
    })
}
