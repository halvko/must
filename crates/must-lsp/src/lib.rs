//! The Must language server: a thin LSP transport over the `ide` crate.
//!
//! Threading model: the main loop owns the mutable [`AnalysisHost`] and
//! applies edits; requests and diagnostics run on a small worker pool over
//! [`Snapshot`]s (database clones). An edit bumps the salsa revision, which
//! unwinds in-flight queries on snapshots with `salsa::Cancelled`; cancelled
//! requests answer `ContentModified`, cancelled diagnostics are simply
//! dropped (the new revision recomputes them).

mod from_proto;
mod pool;
pub mod runner;
mod to_proto;

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use base_db::SourceFile;
use ide::AnalysisHost;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, GotoDefinition, HoverRequest, Request as _, SemanticTokensFullRequest,
};

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn server_capabilities() -> lsp_types::ServerCapabilities {
    lsp_types::ServerCapabilities {
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::FULL,
        )),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp_types::SemanticTokensOptions {
                    legend: to_proto::semantic_tokens_legend(),
                    full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                    ..Default::default()
                },
            ),
        ),
        ..Default::default()
    }
}

/// Run the server over `connection` until the client asks it to exit.
pub fn run(connection: Connection) -> ServerResult<()> {
    let _params = connection.initialize(serde_json::to_value(server_capabilities())?)?;
    tracing::info!("initialized");
    GlobalState::new(&connection).main_loop(&connection)
}

struct GlobalState {
    host: AnalysisHost,
    /// Shared, replaced wholesale on change: snapshots grab an `Arc` clone.
    files: Arc<FileMaps>,
    sender: crossbeam_channel::Sender<Message>,
    pool: pool::TaskPool,
}

#[derive(Default, Clone)]
struct FileMaps {
    by_uri: HashMap<lsp_types::Uri, SourceFile>,
    by_file: HashMap<SourceFile, lsp_types::Uri>,
}

/// Everything a request handler needs, detached from the main loop.
struct Snapshot {
    analysis: ide::Analysis,
    files: Arc<FileMaps>,
}

impl GlobalState {
    fn new(connection: &Connection) -> GlobalState {
        GlobalState {
            host: AnalysisHost::new(),
            files: Arc::new(FileMaps::default()),
            sender: connection.sender.clone(),
            pool: pool::TaskPool::new(2),
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            analysis: self.host.snapshot(),
            files: Arc::clone(&self.files),
        }
    }

    fn main_loop(&mut self, connection: &Connection) -> ServerResult<()> {
        for msg in &connection.receiver {
            match msg {
                Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    self.dispatch_request(req);
                }
                Message::Notification(not) => {
                    if let Err(err) = self.handle_notification(not) {
                        tracing::error!("notification handling failed: {err}");
                    }
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn dispatch_request(&mut self, req: Request) {
        match req.method.as_str() {
            GotoDefinition::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.goto_definition(params)).ok()
                });
            }
            HoverRequest::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.hover(params)).ok()
                });
            }
            CodeActionRequest::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.code_actions(params)).ok()
                });
            }
            SemanticTokensFullRequest::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.semantic_tokens(params)).ok()
                });
            }
            method => {
                tracing::debug!(%method, "unhandled request");
                let resp = Response::new_err(
                    req.id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unhandled method: {method}"),
                );
                let _ = self.sender.send(resp.into());
            }
        }
    }

    /// Parse params on the main thread, run the handler on the pool, and
    /// answer `ContentModified` if an edit cancels it mid-flight.
    fn spawn_request<P: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        req: Request,
        handler: fn(&Snapshot, P) -> Option<serde_json::Value>,
    ) {
        let id = req.id;
        let params: P = match serde_json::from_value(req.params) {
            Ok(params) => params,
            Err(err) => {
                let resp = Response::new_err(id, ErrorCode::InvalidParams as i32, err.to_string());
                let _ = self.sender.send(resp.into());
                return;
            }
        };
        let snapshot = self.snapshot();
        let sender = self.sender.clone();
        self.pool.spawn(move || {
            let resp = match cancellable(|| handler(&snapshot, params)) {
                Some(value) => Response::new_ok(id, value),
                None => content_modified(id),
            };
            let _ = sender.send(resp.into());
        });
    }

    fn handle_notification(&mut self, not: Notification) -> ServerResult<()> {
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(not.params)?;
                let doc = params.text_document;
                let file = self.set_file_text(doc.uri.clone(), doc.text);
                self.publish_diagnostics(doc.uri, file);
            }
            DidChangeTextDocument::METHOD => {
                let mut params: lsp_types::DidChangeTextDocumentParams =
                    serde_json::from_value(not.params)?;
                // Full sync: the last change carries the complete text.
                let Some(change) = params.content_changes.pop() else {
                    return Ok(());
                };
                let uri = params.text_document.uri;
                let file = self.set_file_text(uri.clone(), change.text);
                self.publish_diagnostics(uri, file);
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams =
                    serde_json::from_value(not.params)?;
                // The salsa input stays around (inputs cannot be deleted),
                // but clear stale squiggles and forget the mappings.
                let uri = params.text_document.uri;
                let maps = self.files_mut();
                if let Some(file) = maps.by_uri.remove(&uri) {
                    maps.by_file.remove(&file);
                }
                self.send_diagnostics(uri, Vec::new())?;
            }
            _ => tracing::debug!(method = %not.method, "unhandled notification"),
        }
        Ok(())
    }

    fn files_mut(&mut self) -> &mut FileMaps {
        // Snapshots hold the old Arc; copy-on-write keeps them consistent.
        Arc::make_mut(&mut self.files)
    }

    fn set_file_text(&mut self, uri: lsp_types::Uri, text: String) -> SourceFile {
        match self.files.by_uri.get(&uri) {
            Some(&file) => {
                self.host.set_file_text(file, text);
                file
            }
            None => {
                let file = self.host.create_file(uri.to_string(), text);
                let maps = self.files_mut();
                maps.by_uri.insert(uri.clone(), file);
                maps.by_file.insert(file, uri);
                file
            }
        }
    }

    fn publish_diagnostics(&self, uri: lsp_types::Uri, file: SourceFile) {
        let snapshot = self.snapshot();
        let sender = self.sender.clone();
        self.pool.spawn(move || {
            let diagnostics = cancellable(|| {
                let line_index = snapshot.analysis.line_index(file);
                snapshot
                    .analysis
                    .diagnostics(file)
                    .into_iter()
                    .map(|d| to_proto::diagnostic(&line_index, &uri, d))
                    .collect::<Vec<_>>()
            });
            // Cancelled: a newer revision exists and will publish instead.
            if let Some(diagnostics) = diagnostics {
                let params = lsp_types::PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: None,
                };
                let _ = sender
                    .send(Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into());
            }
        });
    }

    fn send_diagnostics(
        &self,
        uri: lsp_types::Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> ServerResult<()> {
        let params = lsp_types::PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        };
        self.sender
            .send(Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into())?;
        Ok(())
    }
}

impl Snapshot {
    fn goto_definition(
        &self,
        params: lsp_types::GotoDefinitionParams,
    ) -> Option<lsp_types::GotoDefinitionResponse> {
        let doc = params.text_document_position_params;
        let &file = self.files.by_uri.get(&doc.text_document.uri)?;
        let line_index = self.analysis.line_index(file);
        let offset = from_proto::offset(&line_index, doc.position)?;
        let nav = self.analysis.goto_definition(ide::FilePosition { file, offset })?;
        let target_uri = self.files.by_file.get(&nav.file)?.clone();
        // Same-file navigation for now, so reuse the line index.
        let location = lsp_types::Location {
            uri: target_uri,
            range: to_proto::range(&line_index, nav.focus_range),
        };
        Some(lsp_types::GotoDefinitionResponse::Scalar(location))
    }

    fn code_actions(
        &self,
        params: lsp_types::CodeActionParams,
    ) -> Option<Vec<lsp_types::CodeActionOrCommand>> {
        let uri = params.text_document.uri;
        let &file = self.files.by_uri.get(&uri)?;
        let line_index = self.analysis.line_index(file);
        let start = from_proto::offset(&line_index, params.range.start)?;
        let end = from_proto::offset(&line_index, params.range.end)?;
        let query = syntax::TextRange::new(start, end);

        let mut actions = Vec::new();
        for diagnostic in self.analysis.diagnostics(file) {
            let Some(fix) = diagnostic.fix.clone() else {
                continue;
            };
            if diagnostic.range.intersect(query).is_none() {
                continue;
            }
            let edits = fix
                .edits
                .iter()
                .map(|edit| to_proto::text_edit(&line_index, edit))
                .collect();
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);
            actions.push(lsp_types::CodeActionOrCommand::CodeAction(
                lsp_types::CodeAction {
                    title: fix.label,
                    kind: Some(lsp_types::CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![to_proto::diagnostic(&line_index, &uri, diagnostic)]),
                    edit: Some(lsp_types::WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ));
        }
        Some(actions)
    }

    fn semantic_tokens(
        &self,
        params: lsp_types::SemanticTokensParams,
    ) -> Option<lsp_types::SemanticTokensResult> {
        let &file = self.files.by_uri.get(&params.text_document.uri)?;
        let line_index = self.analysis.line_index(file);
        let highlights = self.analysis.highlight(file);
        Some(lsp_types::SemanticTokensResult::Tokens(
            to_proto::semantic_tokens(&line_index, &highlights),
        ))
    }

    fn hover(&self, params: lsp_types::HoverParams) -> Option<lsp_types::Hover> {
        let doc = params.text_document_position_params;
        let &file = self.files.by_uri.get(&doc.text_document.uri)?;
        let line_index = self.analysis.line_index(file);
        let offset = from_proto::offset(&line_index, doc.position)?;
        let hover = self.analysis.hover(ide::FilePosition { file, offset })?;
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: hover.markup,
            }),
            range: Some(to_proto::range(&line_index, hover.range)),
        })
    }
}

use ide::cancellable;

fn content_modified(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::ContentModified as i32,
        "content modified".to_owned(),
    )
}
