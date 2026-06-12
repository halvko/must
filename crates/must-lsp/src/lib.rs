//! The Must language server: a thin LSP transport over the `ide` crate.
//!
//! Threading model: the main loop owns the mutable [`AnalysisHost`] and
//! applies edits; requests and diagnostics run on a small worker pool over
//! [`Snapshot`]s (database clones). An edit bumps the salsa revision, which
//! unwinds in-flight queries on snapshots with `salsa::Cancelled`; cancelled
//! requests answer `ContentModified`, cancelled diagnostics are simply
//! dropped (the new revision recomputes them).

pub mod dap;
mod from_proto;
mod pool;
pub mod runner;
mod to_proto;

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use base_db::SourceFile;
use ide::AnalysisHost;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, CodeLensRequest, ExecuteCommand, GotoDefinition, HoverRequest,
    Request as _, SemanticTokensFullRequest, SemanticTokensRefresh,
};

/// The workspace command behind the ▶ run code lens: arguments are
/// `[uri, entry expression]`; the program is the *current buffer*, run on
/// the interpreter in-process, with the result reported via showMessage.
pub const RUN_COMMAND: &str = "must.run";

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn server_capabilities() -> lsp_types::ServerCapabilities {
    lsp_types::ServerCapabilities {
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::FULL,
        )),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
        code_lens_provider: Some(lsp_types::CodeLensOptions {
            resolve_provider: Some(false),
        }),
        execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
            commands: vec![RUN_COMMAND.to_owned()],
            ..Default::default()
        }),
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
    let init = connection.initialize(serde_json::to_value(server_capabilities())?)?;
    let init: lsp_types::InitializeParams = serde_json::from_value(init)?;
    tracing::info!("initialized");
    GlobalState::new(&connection, &init.capabilities).main_loop(&connection)
}

struct GlobalState {
    host: AnalysisHost,
    /// Shared, replaced wholesale on change: snapshots grab an `Arc` clone.
    files: Arc<FileMaps>,
    sender: crossbeam_channel::Sender<Message>,
    pool: pool::TaskPool,
    /// Per-document publish generation. didClose doesn't bump the salsa
    /// revision, so an in-flight diagnostics task survives it and would
    /// republish stale squiggles after the close (or after a reopen with
    /// different text). Each task captures its generation and drops itself
    /// at send time if a newer one exists.
    diagnostics_generation: Arc<Mutex<HashMap<lsp_types::Uri, u64>>>,
    /// Client supports `workspace/semanticTokens/refresh`.
    semantic_tokens_refresh: bool,
    /// Counter for ids of server→client requests (own namespace).
    outgoing_requests: i32,
}

#[derive(Default, Clone)]
struct FileMaps {
    pub(crate) by_uri: HashMap<lsp_types::Uri, SourceFile>,
    pub(crate) by_file: HashMap<SourceFile, lsp_types::Uri>,
}

/// Everything a request handler needs, detached from the main loop.
struct Snapshot {
    pub(crate) analysis: ide::Analysis,
    pub(crate) files: Arc<FileMaps>,
}

impl Snapshot {
    /// The URI a result file maps to. `None` for files the client no longer
    /// has open — results pointing there are dropped, not misattributed.
    pub(crate) fn uri_for(&self, file: SourceFile) -> Option<lsp_types::Uri> {
        self.files.by_file.get(&file).cloned()
    }
}

impl GlobalState {
    fn new(connection: &Connection, client: &lsp_types::ClientCapabilities) -> GlobalState {
        GlobalState {
            host: AnalysisHost::new(),
            files: Arc::new(FileMaps::default()),
            sender: connection.sender.clone(),
            pool: pool::TaskPool::new(2),
            diagnostics_generation: Arc::new(Mutex::new(HashMap::new())),
            semantic_tokens_refresh: client
                .workspace
                .as_ref()
                .and_then(|w| w.semantic_tokens.as_ref())
                .and_then(|st| st.refresh_support)
                .unwrap_or(false),
            outgoing_requests: 0,
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
        tracing::debug!(method = %req.method, id = ?req.id, "request");
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
            CodeLensRequest::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.code_lenses(params)).ok()
                });
            }
            ExecuteCommand::METHOD => {
                self.spawn_execute_command(req);
            }
            SemanticTokensFullRequest::METHOD => {
                let id = req.id;
                let params: lsp_types::SemanticTokensParams =
                    match serde_json::from_value(req.params) {
                        Ok(params) => params,
                        Err(err) => {
                            let resp = Response::new_err(
                                id,
                                ErrorCode::InvalidParams as i32,
                                err.to_string(),
                            );
                            let _ = self.sender.send(resp.into());
                            return;
                        }
                    };
                // The pull raced ahead of the document's `didOpen` (Zed
                // does this consistently on open). Answer retryably; the
                // refresh nudge sent on `didOpen` makes the client re-pull
                // once the document is known.
                let uri = params.text_document.uri;
                if !self.files.by_uri.contains_key(&uri) {
                    tracing::debug!(uri = %uri.as_str(), "semantic tokens pull before didOpen");
                    let _ = self.sender.send(content_modified(id).into());
                    return;
                }
                self.spawn_semantic_tokens(id, uri);
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

    /// `workspace/executeCommand` for the ▶ run lens: evaluate the entry
    /// expression against the current buffer and report through
    /// `window/showMessage` (a lens click has no other output channel).
    fn spawn_execute_command(&self, req: Request) {
        let id = req.id;
        let params: lsp_types::ExecuteCommandParams = match serde_json::from_value(req.params) {
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
            let resp = match cancellable(|| run_command(&snapshot, &params)) {
                Some(Ok(message)) => {
                    let _ = sender.send(show_message(lsp_types::MessageType::INFO, message));
                    Response::new_ok(id, serde_json::Value::Null)
                }
                Some(Err(message)) => {
                    let _ = sender
                        .send(show_message(lsp_types::MessageType::ERROR, message.clone()));
                    Response::new_err(id, ErrorCode::RequestFailed as i32, message)
                }
                None => content_modified(id),
            };
            let _ = sender.send(resp.into());
        });
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
        self.spawn_parsed_request(id, params, handler);
    }

    fn spawn_parsed_request<P: Send + 'static>(
        &self,
        id: RequestId,
        params: P,
        handler: fn(&Snapshot, P) -> Option<serde_json::Value>,
    ) {
        let snapshot = self.snapshot();
        let sender = self.sender.clone();
        self.pool.spawn(move || {
            // Every request id gets exactly one response, panics included —
            // a swallowed id hangs that request in the client forever.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cancellable(|| handler(&snapshot, params))
            }));
            let resp = match outcome {
                Ok(Some(value)) => Response::new_ok(id, value),
                Ok(None) => content_modified(id),
                Err(_) => Response::new_err(
                    id,
                    ErrorCode::InternalError as i32,
                    "request handler panicked — this is a bug in the Must language server"
                        .to_owned(),
                ),
            };
            let _ = sender.send(resp.into());
        });
    }

    fn handle_notification(&mut self, not: Notification) -> ServerResult<()> {
        tracing::debug!(method = %not.method, "notification");
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(not.params)?;
                let doc = params.text_document;
                let file = self.set_file_text(doc.uri.clone(), doc.text);
                self.publish_diagnostics(doc.uri, file, doc.version);
                // The refresh covers both ways the initial pull goes
                // missing: Zed pulls *before* `didOpen` (answered
                // `ContentModified` above), and for reopened buffers it
                // doesn't pull at all until an edit (zed#57651). It
                // advertises and handles refresh, so nudge it to re-pull
                // now that the document is known.
                self.request_semantic_tokens_refresh();
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
                self.publish_diagnostics(uri, file, params.text_document.version);
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
                // Invalidate in-flight publishes before clearing, or a slow
                // task can resurrect squiggles on the closed document.
                self.bump_generation(&uri);
                self.send_diagnostics(uri, Vec::new())?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Ask the client to re-pull semantic tokens for visible editors (a
    /// server→client request; the response is ignored in the main loop).
    fn request_semantic_tokens_refresh(&mut self) {
        if !self.semantic_tokens_refresh {
            tracing::debug!("client lacks semanticTokens refresh support, not requesting");
            return;
        }
        self.outgoing_requests += 1;
        tracing::debug!(
            n = self.outgoing_requests,
            "requesting semantic tokens refresh"
        );
        let req = Request::new(
            RequestId::from(format!("must-lsp/{}", self.outgoing_requests)),
            SemanticTokensRefresh::METHOD.to_owned(),
            serde_json::Value::Null,
        );
        let _ = self.sender.send(req.into());
    }

    fn spawn_semantic_tokens(&self, id: RequestId, uri: lsp_types::Uri) {
        let params = lsp_types::SemanticTokensParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        self.spawn_parsed_request(id, params, |snapshot, params| {
            serde_json::to_value(snapshot.semantic_tokens(params)).ok()
        });
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

    fn bump_generation(&self, uri: &lsp_types::Uri) -> u64 {
        let mut generations = self.diagnostics_generation.lock().unwrap();
        let generation = generations.entry(uri.clone()).or_insert(0);
        *generation += 1;
        *generation
    }

    fn publish_diagnostics(&self, uri: lsp_types::Uri, file: SourceFile, version: i32) {
        let generation = self.bump_generation(&uri);
        let generations = Arc::clone(&self.diagnostics_generation);
        let snapshot = self.snapshot();
        let sender = self.sender.clone();
        self.pool.spawn(move || {
            let diagnostics = cancellable(|| {
                let line_index = snapshot.analysis.line_index(file);
                snapshot
                    .analysis
                    .diagnostics(file)
                    .into_iter()
                    .map(|d| to_proto::diagnostic(&snapshot, &line_index, d))
                    .collect::<Vec<_>>()
            });
            // Cancelled: a newer revision exists and will publish instead.
            let Some(diagnostics) = diagnostics else {
                return;
            };
            // Superseded while computing (close, reopen): drop, don't
            // resurrect.
            if generations.lock().unwrap().get(&uri) != Some(&generation) {
                return;
            }
            let params = lsp_types::PublishDiagnosticsParams {
                uri,
                diagnostics,
                // The client uses this to discard publishes that arrive
                // after the document moved on.
                version: Some(version),
            };
            let _ = sender
                .send(Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into());
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
        let nav = self
            .analysis
            .goto_definition(ide::FilePosition { file, offset })?;
        // The target's positions resolve through the target's own file.
        let target_uri = self.uri_for(nav.file)?;
        let target_index = self.analysis.line_index(nav.file);
        let location = lsp_types::Location {
            uri: target_uri,
            range: to_proto::range(&target_index, nav.focus_range),
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
            // Each edit names its target file; resolve every one through
            // its own URI and line index. An edit whose file the client
            // doesn't have open drops the whole action — applying half a
            // fix is worse than offering none.
            let mut changes: HashMap<lsp_types::Uri, Vec<lsp_types::TextEdit>> = HashMap::new();
            let mut all_resolved = true;
            for file_edit in &fix.edits {
                let Some(target_uri) = self.uri_for(file_edit.file) else {
                    all_resolved = false;
                    break;
                };
                let target_index = self.analysis.line_index(file_edit.file);
                changes
                    .entry(target_uri)
                    .or_default()
                    .push(to_proto::text_edit(&target_index, &file_edit.edit));
            }
            if !all_resolved {
                continue;
            }
            actions.push(lsp_types::CodeActionOrCommand::CodeAction(
                lsp_types::CodeAction {
                    title: fix.label,
                    kind: Some(lsp_types::CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![to_proto::diagnostic(self, &line_index, diagnostic)]),
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
        let tokens = to_proto::semantic_tokens(&line_index, &highlights);
        tracing::debug!(
            uri = %params.text_document.uri.as_str(),
            tokens = tokens.data.len(),
            "answering semantic tokens"
        );
        Some(lsp_types::SemanticTokensResult::Tokens(tokens))
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

impl Snapshot {
    fn code_lenses(
        &self,
        params: lsp_types::CodeLensParams,
    ) -> Option<Vec<lsp_types::CodeLens>> {
        let uri = params.text_document.uri;
        let &file = self.files.by_uri.get(&uri)?;
        let line_index = self.analysis.line_index(file);
        let lenses = self
            .analysis
            .run_lenses(file)
            .into_iter()
            .map(|lens| {
                let entry = format!("{}()", lens.name);
                lsp_types::CodeLens {
                    range: to_proto::range(&line_index, lens.range),
                    command: Some(lsp_types::Command {
                        title: format!("▶ run {entry}"),
                        command: RUN_COMMAND.to_owned(),
                        arguments: Some(vec![
                            serde_json::Value::String(uri.to_string()),
                            serde_json::Value::String(entry),
                        ]),
                    }),
                    data: None,
                }
            })
            .collect();
        Some(lenses)
    }
}

/// Execute the ▶ run command: `[uri, entry]`. `Ok`/`Err` are both
/// user-facing message texts.
fn run_command(
    snapshot: &Snapshot,
    params: &lsp_types::ExecuteCommandParams,
) -> Result<String, String> {
    if params.command != RUN_COMMAND {
        return Err(format!("unknown command `{}`", params.command));
    }
    let [uri, entry] = params.arguments.as_slice() else {
        return Err("must.run expects [uri, entry] arguments".to_owned());
    };
    let (Some(uri), Some(entry)) = (uri.as_str(), entry.as_str()) else {
        return Err("must.run expects string arguments".to_owned());
    };
    let parsed: lsp_types::Uri = uri
        .parse()
        .map_err(|_| format!("invalid uri `{uri}`"))?;
    let &file = snapshot
        .files
        .by_uri
        .get(&parsed)
        .ok_or_else(|| format!("`{uri}` is not open"))?;
    // The *buffer* runs, not the file on disk — what you see is what runs.
    let text = snapshot.analysis.file_text(file);
    let mut output = Vec::new();
    let result = crate::runner::evaluate(text, uri, entry, &mut output);
    let mut message = format!("{entry}
");
    message.push_str(&String::from_utf8_lossy(&output));
    match result {
        Ok(Some(value)) => message.push_str(&format!("=> {value}")),
        Ok(None) => message.push_str("✓"),
        Err(rendered) => {
            message.push_str(&rendered);
            return Err(truncate(message));
        }
    }
    Ok(truncate(message))
}

/// showMessage is a toast, not a terminal: keep it skimmable.
fn truncate(mut message: String) -> String {
    const LIMIT: usize = 600;
    if message.len() > LIMIT {
        let mut end = LIMIT;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("…");
    }
    message
}

fn show_message(typ: lsp_types::MessageType, message: String) -> Message {
    Notification::new(
        lsp_types::notification::ShowMessage::METHOD.to_owned(),
        lsp_types::ShowMessageParams { typ, message },
    )
    .into()
}

use ide::cancellable;

fn content_modified(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::ContentModified as i32,
        "content modified".to_owned(),
    )
}
