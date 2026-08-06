//! The Must language server: a thin LSP transport over the `ide` crate.
//!
//! Threading model: the main loop owns the mutable [`AnalysisHost`] and
//! applies edits; requests and diagnostics run on a small worker pool over
//! [`Snapshot`]s (database clones). An edit bumps the salsa revision, which
//! unwinds in-flight queries on snapshots with `salsa::Cancelled`; cancelled
//! requests answer `ContentModified`, cancelled diagnostics are dropped.
//! Only the edit that did the cancelling republishes, and only for the
//! document it edited — diagnostics cancelled by another document's edit
//! stay stale until that document changes again.

pub mod check;
pub mod cli;
pub mod compile;
pub mod dap;
mod from_proto;
mod pool;
pub mod runner;
mod to_proto;

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use base_db::SourceFile;
use ide::{AnalysisHost, cancellable};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, CodeLensRequest, Completion, ExecuteCommand, GotoDefinition, HoverRequest,
    Request as _, SemanticTokensFullRequest, SemanticTokensRefresh,
};

/// The workspace command behind the ▶ run code lens: arguments are
/// `[uri, entry expression]`; the program is the *current buffer*, run on
/// the interpreter in-process, with the result reported via showMessage.
pub const RUN_COMMAND: &str = "must.run";

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Enough that one slow request does not hold up the next; the main loop is
/// never one of them.
const WORKER_THREADS: usize = 2;

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
        // Trigger chars registered for `.`/`::`/`{` — member/variant
        // candidates classify the first two; `{` catches the auto-closed
        // `match s {|}` empty-arm-list slot (`ArmListShape::EmptyBraces`).
        completion_provider: Some(lsp_types::CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![".".to_owned(), ":".to_owned(), "{".to_owned()]),
            ..Default::default()
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

/// What the server knows about one document, and the only place it says so.
///
/// [`FileMaps::by_uri`] holds one entry per URI and one salsa input per entry,
/// so a document cannot be open and closed at the same time, cannot be tracked
/// without an input, and cannot be closed without first having been opened —
/// none of those states can be written down. The database is deliberately not
/// a second opinion on any of this: `SourceFile` carries no open flag.
// (No `Debug`: a salsa input can only be formatted with its database
// attached, and this type has no reason to reach for one.)
#[derive(Clone, Copy)]
enum FileState {
    /// The client owns the buffer, and the input's text is the client's text.
    Open(SourceFile),
    /// The client has given the buffer back. The input's text has been
    /// emptied and the handle is parked here for the next open.
    ///
    /// Parking it is what bounds the leak. Salsa inputs cannot be deleted, so
    /// dropping the entry would not free anything — it would strand the input
    /// beyond reach and force the next open of the same URI to mint another
    /// one, making memory grow with open *events* instead of with distinct
    /// documents.
    ///
    /// Emptying the text is the whole of what closing does. Everything derived
    /// from that text stays memoized behind this handle until a reopen
    /// displaces it: salsa has no sweep, so releasing one file's memos could
    /// only be an enumeration of every file-keyed query in every crate, kept
    /// correct by hand forever. The honest central mechanism is a database per
    /// project, and there are no projects yet; a VFS may dissolve the question
    /// instead, since a closed file stays part of its project and only flips
    /// its text from buffer to disk. Deferred until then.
    Closed(SourceFile),
}

impl FileState {
    fn source_file(self) -> SourceFile {
        match self {
            FileState::Open(file) | FileState::Closed(file) => file,
        }
    }
}

struct GlobalState {
    host: AnalysisHost,
    /// Shared, and replaced wholesale on every change: a snapshot takes an
    /// `Arc` clone, so a worker goes on reading the maps as they were when
    /// its task was handed out. The main loop is the only writer.
    files: Arc<FileMaps>,
    sender: crossbeam_channel::Sender<Message>,
    pool: pool::TaskPool,
    /// Per-document publish generation. Cancellation only saves a task that
    /// is still computing: one that has finished can be beaten to the send by
    /// the close or reopen it raced, and would then resurrect squiggles on a
    /// document the client has moved on from. Each task captures its
    /// generation and re-checks it at send time under this lock, held across
    /// the send: a close therefore orders strictly before or after an
    /// in-flight publish, never between its check and its send.
    diagnostics_generation: Arc<Mutex<HashMap<lsp_types::Uri, u64>>>,
    /// Issues [`GlobalState::diagnostics_generation`] values, main loop only.
    /// The counter runs for the whole session, across documents and
    /// close/reopen cycles: a close drops the document's entry, and a number
    /// that could be reissued after that would let a pre-close task pass as
    /// the reopened document's first publish.
    next_generation: u64,
    /// Client supports `workspace/semanticTokens/refresh`.
    semantic_tokens_refresh: bool,
    /// Client's `textDocument.completion.completionItem.snippetSupport`
    /// gates every would-be snippet insertion — see
    /// `to_proto::completion_item`.
    snippet_support: bool,
    /// Counter for ids of server→client requests (own namespace).
    outgoing_requests: i32,
}

/// Both directions of the file table, replaced as one so that no reader can
/// catch them disagreeing.
#[derive(Default, Clone)]
struct FileMaps {
    /// What the server knows about every document the client has named.
    by_uri: HashMap<lsp_types::Uri, FileState>,
    /// The other direction: what URI a salsa handle came from, for turning a
    /// navigation target back into something the client can open.
    ///
    /// One entry per distinct document, minted with its input and never
    /// removed — a closed file keeps its handle (see [`FileState::Closed`]),
    /// so the mapping stays true through any number of open/close cycles and
    /// re-opening finds the entry already there. Nothing here says whether the
    /// document is open; `by_uri` is the single answer to that.
    by_file: HashMap<SourceFile, lsp_types::Uri>,
}

/// Everything a request handler needs, detached from the main loop.
///
/// The analysis and the file maps are taken in the same instant, and a handler
/// answers about that instant: the maps it reads may already be older than the
/// main loop's, which is what the `Arc` is for rather than a staleness bug. An
/// edit that would make the answer wrong cancels the query outright.
struct Snapshot {
    pub(crate) analysis: ide::Analysis,
    files: Arc<FileMaps>,
    /// See `GlobalState::snippet_support`.
    snippet_support: bool,
}

impl Snapshot {
    /// The URI a result file maps to. `None` only for files the client never
    /// named — entries survive close cycles, so closed files still map. Those
    /// unknown-file results are dropped, not misattributed.
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
            pool: pool::TaskPool::new(WORKER_THREADS),
            diagnostics_generation: Arc::new(Mutex::new(HashMap::new())),
            next_generation: 0,
            semantic_tokens_refresh: client
                .workspace
                .as_ref()
                .and_then(|w| w.semantic_tokens.as_ref())
                .and_then(|st| st.refresh_support)
                .unwrap_or(false),
            snippet_support: client
                .text_document
                .as_ref()
                .and_then(|td| td.completion.as_ref())
                .and_then(|c| c.completion_item.as_ref())
                .and_then(|ci| ci.snippet_support)
                .unwrap_or(false),
            outgoing_requests: 0,
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            analysis: self.host.snapshot(),
            files: Arc::clone(&self.files),
            snippet_support: self.snippet_support,
        }
    }

    fn main_loop(&mut self, connection: &Connection) -> ServerResult<()> {
        for msg in &connection.receiver {
            match msg {
                Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        // Pool tasks still in flight are not drained: a
                        // request dispatched just before shutdown may go
                        // unanswered. The server is exiting either way, and
                        // the closed connection tells the client its pending
                        // answers are gone.
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
            Completion::METHOD => {
                self.spawn_request(req, |snapshot, params| {
                    serde_json::to_value(snapshot.completions(params)).ok()
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
                if !matches!(self.files.by_uri.get(&uri), Some(FileState::Open(_))) {
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
            // Exactly one response per id, panics included, as in
            // `spawn_parsed_request` — a swallowed id hangs the request.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cancellable(|| run_command(&snapshot, &params))
            }));
            let resp = match outcome {
                Ok(Some(Ok(message))) => {
                    let _ = sender.send(show_message(lsp_types::MessageType::INFO, message));
                    Response::new_ok(id, serde_json::Value::Null)
                }
                Ok(Some(Err(message))) => {
                    let _ =
                        sender.send(show_message(lsp_types::MessageType::ERROR, message.clone()));
                    Response::new_err(id, ErrorCode::RequestFailed as i32, message)
                }
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

    fn handle_notification(&mut self, notification: Notification) -> ServerResult<()> {
        tracing::debug!(method = %notification.method, "notification");
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let doc = params.text_document;
                let file = self.open_file(doc.uri.clone(), doc.text);
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
                    serde_json::from_value(notification.params)?;
                // Full sync: the last change carries the complete text.
                let Some(change) = params.content_changes.pop() else {
                    return Ok(());
                };
                let uri = params.text_document.uri;
                let version = params.text_document.version;
                // `didOpen` always precedes `didChange`, so anything else is a
                // client bug. Applying it anyway would publish diagnostics for
                // a document the client does not consider open, and would
                // refill a buffer the client has given back.
                let Some(FileState::Open(file)) = self.files.by_uri.get(&uri).copied() else {
                    tracing::warn!(uri = %uri.as_str(), "didChange for a document that is not open");
                    return Ok(());
                };
                self.host.set_file_text(file, change.text);
                self.publish_diagnostics(uri, file, version);
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                // Only documents this server actually tracks get their
                // squiggles cleared, for the same reason `didChange` above
                // ignores unknown ones: never publish about a document the
                // client did not tell us to open.
                if self.close_file(&uri) {
                    // Invalidate in-flight publishes before clearing, or a
                    // slow task can resurrect squiggles on the closed
                    // document. Retiring the entry outright (rather than
                    // bumping it) also keeps the map bounded by open
                    // documents.
                    self.retire_generation(&uri);
                    self.send_diagnostics(uri, Vec::new(), None)?;
                }
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

    /// The one way to write the file maps: snapshots hold the old `Arc`, so
    /// copy-on-write leaves what they already handed out alone.
    fn files_mut(&mut self) -> &mut FileMaps {
        Arc::make_mut(&mut self.files)
    }

    /// Takes ownership of `uri`'s buffer: mints its salsa input the first time
    /// the document is opened, and revives the parked handle on every reopen,
    /// so a session's inputs are bounded by the documents it has touched
    /// rather than by how often the editor opened them.
    ///
    /// A `didOpen` for a document that is already open is a client bug; the
    /// most useful reading of it is a full-text reset, which is what reusing
    /// this path does.
    fn open_file(&mut self, uri: lsp_types::Uri, text: String) -> SourceFile {
        match self.files.by_uri.get(&uri).copied() {
            Some(state) => {
                // Fill the input first, then record the state, so this reads
                // the same way round as `close_file` below.
                let file = state.source_file();
                self.host.set_file_text(file, text);
                self.files_mut().by_uri.insert(uri, FileState::Open(file));
                file
            }
            None => {
                let file = self.host.create_file(uri.to_string(), text);
                let maps = self.files_mut();
                maps.by_uri.insert(uri.clone(), FileState::Open(file));
                // The only place an input is minted is the only place the
                // reverse map is written: one entry per document, for the life
                // of the session. The reopen arm above deliberately adds
                // nothing — it reuses a handle that is already mapped.
                maps.by_file.insert(file, uri);
                file
            }
        }
    }

    /// Gives the buffer back: empties the document's text and parks its
    /// handle. Returns whether this is a document the server tracks, which is
    /// what decides if the client hears about it.
    ///
    /// Neither map loses an entry here. `by_uri` keeps the parked handle for
    /// the next open, and `by_file` keeps the name a navigation target would
    /// have to be reported under.
    fn close_file(&mut self, uri: &lsp_types::Uri) -> bool {
        match self.files.by_uri.get(uri).copied() {
            Some(FileState::Open(file)) => {
                self.host.close_file(file);
                self.files_mut()
                    .by_uri
                    .insert(uri.clone(), FileState::Closed(file));
                true
            }
            Some(FileState::Closed(_)) => {
                // Closing twice: there is nothing left to empty, but the
                // client still gets its cleared diagnostics.
                tracing::warn!(uri = %uri.as_str(), "didClose for a document that is not open");
                true
            }
            None => {
                tracing::warn!(uri = %uri.as_str(), "didClose for an unknown document");
                false
            }
        }
    }

    /// Stamps `uri`'s next publish with a fresh, session-unique generation.
    fn bump_generation(&mut self, uri: &lsp_types::Uri) -> u64 {
        self.next_generation += 1;
        self.diagnostics_generation
            .lock()
            .unwrap()
            .insert(uri.clone(), self.next_generation);
        self.next_generation
    }

    /// Retires a closed document's publish generation: in-flight publishes
    /// for it drop at send time, and the map stays bounded by open documents.
    fn retire_generation(&self, uri: &lsp_types::Uri) {
        self.diagnostics_generation.lock().unwrap().remove(uri);
    }

    /// Computes this document's diagnostics on the pool.
    ///
    /// `version` rides along with the task and is published with the result:
    /// off the main loop the answer can land after the client has typed on,
    /// and the version is what lets the client drop squiggles computed for
    /// text it no longer has.
    fn publish_diagnostics(&mut self, uri: lsp_types::Uri, file: SourceFile, version: i32) {
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
            // Cancelled: some edit bumped the revision. Only the edited
            // document gets republished, so if the edit was to another
            // document these diagnostics stay stale until it changes again.
            let Some(diagnostics) = diagnostics else {
                return;
            };
            // Superseded since spawn (a newer publish, or a close, which
            // retired the entry outright): drop, don't resurrect. The lock is
            // held across the send — an unbounded send never blocks, so
            // nothing deadlocks — which orders a close's retire strictly
            // before this send (this drops) or after it (its clear lands
            // after these diagnostics), never between check and send.
            let generations = generations.lock().unwrap();
            if generations.get(&uri) != Some(&generation) {
                return;
            }
            let _ = sender.send(diagnostics_notification(uri, diagnostics, Some(version)).into());
        });
    }

    fn send_diagnostics(
        &self,
        uri: lsp_types::Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) -> ServerResult<()> {
        self.sender
            .send(diagnostics_notification(uri, diagnostics, version).into())?;
        Ok(())
    }
}

impl Snapshot {
    /// Answers only for documents the client had open when this snapshot was
    /// taken. A closed document has had its text emptied, so the tree behind
    /// its handle is the parse of `""`: answering from it would not fail, it
    /// would quietly report that a file full of definitions has none.
    fn goto_definition(
        &self,
        params: lsp_types::GotoDefinitionParams,
    ) -> Option<lsp_types::GotoDefinitionResponse> {
        let doc = params.text_document_position_params;
        let uri = doc.text_document.uri;
        let Some(FileState::Open(file)) = self.files.by_uri.get(&uri).copied() else {
            tracing::warn!(uri = %uri.as_str(), "goto definition for a document that is not open");
            return None;
        };
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
        let Some(FileState::Open(file)) = self.files.by_uri.get(&uri).copied() else {
            tracing::warn!(uri = %uri.as_str(), "code action for a document that is not open");
            return None;
        };
        // Every fix this server offers is a quick fix, so a client that
        // asked for other kinds gets nothing.
        if let Some(only) = &params.context.only {
            if !only
                .iter()
                .any(|kind| kind == &lsp_types::CodeActionKind::QUICKFIX)
            {
                return Some(Vec::new());
            }
        }
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
            // Uri-keyed maps are the shape the LSP protocol mandates; the
            // interior mutability clippy worries about is never exercised.
            #[allow(clippy::mutable_key_type)]
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
        let uri = params.text_document.uri;
        let Some(FileState::Open(file)) = self.files.by_uri.get(&uri).copied() else {
            tracing::warn!(uri = %uri.as_str(), "semantic tokens for a document that is not open");
            return None;
        };
        let line_index = self.analysis.line_index(file);
        let highlights = self.analysis.highlight(file);
        let tokens = to_proto::semantic_tokens(&line_index, &highlights);
        tracing::debug!(
            uri = %uri.as_str(),
            tokens = tokens.data.len(),
            "answering semantic tokens"
        );
        Some(lsp_types::SemanticTokensResult::Tokens(tokens))
    }

    fn hover(&self, params: lsp_types::HoverParams) -> Option<lsp_types::Hover> {
        let doc = params.text_document_position_params;
        // Answered only while the document is open: a closed file's text is
        // cleared, so answering from it would describe an empty file.
        let Some(FileState::Open(file)) = self.files.by_uri.get(&doc.text_document.uri).copied()
        else {
            tracing::warn!(uri = %doc.text_document.uri.as_str(), "hover for a document that is not open");
            return None;
        };
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

    fn completions(
        &self,
        params: lsp_types::CompletionParams,
    ) -> Option<lsp_types::CompletionResponse> {
        let doc = params.text_document_position;
        let Some(FileState::Open(file)) = self.files.by_uri.get(&doc.text_document.uri).copied()
        else {
            tracing::warn!(uri = %doc.text_document.uri.as_str(), "completions for a document that is not open");
            return None;
        };
        let line_index = self.analysis.line_index(file);
        let offset = from_proto::offset(&line_index, doc.position)?;
        let items = self
            .analysis
            .completions(ide::FilePosition { file, offset });
        Some(lsp_types::CompletionResponse::Array(
            items
                .into_iter()
                .map(|item| to_proto::completion_item(&line_index, item, self.snippet_support))
                .collect(),
        ))
    }
}

fn diagnostics_notification(
    uri: lsp_types::Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) -> Notification {
    let params = lsp_types::PublishDiagnosticsParams {
        uri,
        diagnostics,
        version,
    };
    Notification::new(PublishDiagnostics::METHOD.to_owned(), params)
}

impl Snapshot {
    fn code_lenses(&self, params: lsp_types::CodeLensParams) -> Option<Vec<lsp_types::CodeLens>> {
        let uri = params.text_document.uri;
        let Some(FileState::Open(file)) = self.files.by_uri.get(&uri).copied() else {
            tracing::warn!(uri = %uri.as_str(), "code lenses for a document that is not open");
            return None;
        };
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
    let parsed: lsp_types::Uri = uri.parse().map_err(|_| format!("invalid uri `{uri}`"))?;
    let Some(FileState::Open(file)) = snapshot.files.by_uri.get(&parsed).copied() else {
        return Err(format!("`{uri}` is not open"));
    };
    // The *buffer* runs, not the file on disk — what you see is what runs.
    let text = snapshot.analysis.file_text(file);
    let mut output = Vec::new();
    // The ▶ run lens has no terminal underneath it — its result lands in a
    // showMessage toast — so there is no stdin to hand it and `read_line`
    // reads immediate end-of-input rather than blocking on nothing.
    let result = crate::runner::evaluate(text, uri, entry, &mut output, std::io::empty());
    let mut message = format!(
        "{entry}
"
    );
    message.push_str(&String::from_utf8_lossy(&output));
    match result {
        Ok(Some(value)) => message.push_str(&format!("=> {value}")),
        Ok(None) => message.push('✓'),
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
        message.push('…');
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

fn content_modified(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::ContentModified as i32,
        "content modified".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const BROKEN: &str = "static = true;";

    fn uri(s: &str) -> lsp_types::Uri {
        s.parse().unwrap()
    }

    /// A server state wired to a channel the test holds; these tests are about
    /// the file table, not the wire, but answers now come back over it from
    /// the pool. The `Connection` comes back so the receiving end stays alive.
    fn server() -> (GlobalState, Connection) {
        let (server, client) = Connection::memory();
        let init = lsp_types::InitializeParams::default();
        (GlobalState::new(&server, &init.capabilities), client)
    }

    /// Waits until everything queued on the pool has run, which is what lets a
    /// test tell "nothing was published" from "nothing has been published
    /// *yet*": a worker only reaches the barrier once it is out of tasks that
    /// were queued ahead of this one, and the barrier only opens once every
    /// worker is there.
    fn quiesce(state: &GlobalState) {
        let barrier = Arc::new(std::sync::Barrier::new(WORKER_THREADS + 1));
        for _ in 0..WORKER_THREADS {
            let barrier = Arc::clone(&barrier);
            state.pool.spawn(move || {
                barrier.wait();
            });
        }
        barrier.wait();
    }

    /// Everything the server has to say by now, dropped.
    fn drain(state: &GlobalState, client: &Connection) {
        quiesce(state);
        while client.receiver.try_recv().is_ok() {}
    }

    /// Whether the server has sent the client anything at all.
    fn published(state: &GlobalState, client: &Connection) -> bool {
        quiesce(state);
        client.receiver.try_recv().is_ok()
    }

    /// Everything below goes through `handle_notification`, so what is under
    /// test is the production path, not a test-only shortcut into it.
    fn notify<N: lsp_types::notification::Notification>(
        state: &mut GlobalState,
        params: N::Params,
    ) {
        state
            .handle_notification(Notification::new(N::METHOD.to_owned(), params))
            .expect("notification handling failed");
    }

    fn open(state: &mut GlobalState, uri: &lsp_types::Uri, text: &str) {
        notify::<DidOpenTextDocument>(
            state,
            lsp_types::DidOpenTextDocumentParams {
                text_document: lsp_types::TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "must".to_owned(),
                    version: 0,
                    text: text.to_owned(),
                },
            },
        );
    }

    fn close(state: &mut GlobalState, uri: &lsp_types::Uri) {
        notify::<DidCloseTextDocument>(
            state,
            lsp_types::DidCloseTextDocumentParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
            },
        );
    }

    fn change(state: &mut GlobalState, uri: &lsp_types::Uri, text: &str) {
        notify::<DidChangeTextDocument>(
            state,
            lsp_types::DidChangeTextDocumentParams {
                text_document: lsp_types::VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 1,
                },
                content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_owned(),
                }],
            },
        );
    }

    fn goto_params(
        uri: &lsp_types::Uri,
        position: lsp_types::Position,
    ) -> lsp_types::GotoDefinitionParams {
        lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    /// The answer to `id`, skipping whatever diagnostics the pool published on
    /// the way.
    fn response(client: &Connection, id: &RequestId) -> Response {
        loop {
            let msg = client
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("no response within 10s");
            if let Message::Response(resp) = msg
                && resp.id == *id
            {
                return resp;
            }
        }
    }

    /// Likewise through `dispatch_request`, which hands the work to the pool,
    /// so the answer arrives over the wire. Unwrapped the way a client would:
    /// a request that answers nothing is an `Ok` response carrying `null`, not
    /// an error.
    fn goto(
        state: &mut GlobalState,
        client: &Connection,
        uri: &lsp_types::Uri,
        position: lsp_types::Position,
    ) -> Option<lsp_types::GotoDefinitionResponse> {
        let id = RequestId::from(1);
        state.dispatch_request(Request::new(
            id.clone(),
            GotoDefinition::METHOD.to_owned(),
            goto_params(uri, position),
        ));
        let response = response(client, &id);
        assert!(
            response.error.is_none(),
            "error response: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap_or_default()).unwrap()
    }

    #[test]
    fn closing_parks_the_handle_in_place() {
        let (mut state, _client) = server();
        let doc = uri("file:///a.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files.by_uri[&doc].source_file();
        assert!(matches!(state.files.by_uri[&doc], FileState::Open(_)));

        close(&mut state, &doc);

        assert_eq!(
            state.files.by_uri.len(),
            1,
            "the entry is what keeps the input reusable"
        );
        assert!(matches!(state.files.by_uri[&doc], FileState::Closed(_)));
        assert!(
            state.files.by_uri[&doc].source_file() == file,
            "closing swapped the salsa input"
        );
    }

    #[test]
    fn reopening_reuses_the_parked_input_and_analysis_resumes() {
        let (mut state, _client) = server();
        let doc = uri("file:///a.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files.by_uri[&doc].source_file();
        close(&mut state, &doc);

        open(&mut state, &doc, BROKEN);

        assert!(matches!(state.files.by_uri[&doc], FileState::Open(_)));
        assert!(
            state.files.by_uri[&doc].source_file() == file,
            "reopening minted a second input for the same document"
        );
        assert_eq!(state.host.snapshot().diagnostics(file).len(), 1);
    }

    #[test]
    fn open_close_cycles_leave_one_entry_and_one_input() {
        let (mut state, _client) = server();
        let doc = uri("file:///churn.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files.by_uri[&doc].source_file();

        for _ in 0..50 {
            close(&mut state, &doc);
            open(&mut state, &doc, BROKEN);
        }

        assert_eq!(state.files.by_uri.len(), 1);
        assert!(
            state.files.by_uri[&doc].source_file() == file,
            "a cycle minted a new salsa input; memory would grow per open event"
        );
    }

    #[test]
    fn closing_says_nothing_about_documents_the_server_never_saw() {
        let (mut state, client) = server();
        let doc = uri("file:///phantom.must");

        close(&mut state, &doc);
        assert!(state.files.by_uri.is_empty(), "a close created a document");
        assert!(
            !published(&state, &client),
            "published diagnostics for a document the client never opened"
        );

        // A document the server does know about still gets its squiggles
        // cleared, even when the close is a repeat.
        open(&mut state, &doc, BROKEN);
        close(&mut state, &doc);
        drain(&state, &client);
        close(&mut state, &doc);
        assert!(
            published(&state, &client),
            "a repeated close left the client's squiggles in place"
        );
    }

    #[test]
    fn changes_to_documents_that_are_not_open_are_ignored() {
        let (mut state, client) = server();
        let doc = uri("file:///stray.must");

        change(&mut state, &doc, BROKEN);
        assert!(state.files.by_uri.is_empty(), "a change created a document");
        assert!(
            !published(&state, &client),
            "published diagnostics for a document the client never opened"
        );

        open(&mut state, &doc, BROKEN);
        close(&mut state, &doc);
        drain(&state, &client);

        change(&mut state, &doc, BROKEN);
        assert!(
            matches!(state.files.by_uri[&doc], FileState::Closed(_)),
            "a stray change refilled a buffer the client had given back"
        );
        assert!(!published(&state, &client));
    }

    #[test]
    fn requests_are_answered_only_while_the_document_is_open() {
        let (mut state, client) = server();
        let doc = uri("file:///def.must");
        // `b` on line 0, column 15, defined on line 1.
        let text = "const a = fn { b() };\nstatic b = fn { a() };\n";
        let cursor = lsp_types::Position::new(0, 15);
        open(&mut state, &doc, text);

        let found = goto(&mut state, &client, &doc, cursor);
        assert!(found.is_some(), "goto-definition found nothing to go to");

        close(&mut state, &doc);

        // The document's text is emptied on close, so its handle now parses
        // as an empty file: answering from it would not fail, it would report
        // that a file full of definitions has none. The worker reads the
        // state out of the snapshot's maps, so a request that races the close
        // is answered against whichever side of it the snapshot fell on.
        assert_eq!(goto(&mut state, &client, &doc, cursor), None);

        open(&mut state, &doc, text);
        assert_eq!(
            goto(&mut state, &client, &doc, cursor),
            found,
            "reopening did not make the document answerable again"
        );
    }

    #[test]
    fn an_edit_during_a_request_is_answered_and_leaves_the_pool_alive() {
        let (mut state, client) = server();
        let doc = uri("file:///race.must");
        let text = "const a = fn { b() };\nstatic b = fn { a() };\n";
        let cursor = lsp_types::Position::new(0, 15);
        open(&mut state, &doc, text);

        for i in 0..20i32 {
            let id = RequestId::from(i);
            state.dispatch_request(Request::new(
                id.clone(),
                GotoDefinition::METHOD.to_owned(),
                goto_params(&doc, cursor),
            ));
            // Straight into the request's back: either the handler finishes
            // first, or the edit unwinds it with `salsa::Cancelled`, which the
            // worker has to catch rather than die of.
            change(&mut state, &doc, text);
            let response = response(&client, &id);
            if let Some(err) = &response.error {
                assert_eq!(
                    err.code,
                    ErrorCode::ContentModified as i32,
                    "unexpected error response: {err:?}"
                );
            }
        }

        drain(&state, &client);
        assert!(
            goto(&mut state, &client, &doc, cursor).is_some(),
            "the pool stopped answering after the races"
        );
    }

    #[test]
    fn the_uri_map_gets_one_entry_per_document_and_keeps_it() {
        let (mut state, _client) = server();
        let a = uri("file:///a.must");
        let b = uri("file:///b.must");
        open(&mut state, &a, BROKEN);
        open(&mut state, &b, BROKEN);
        let file_a = state.files.by_uri[&a].source_file();

        for _ in 0..5 {
            close(&mut state, &a);
            open(&mut state, &a, BROKEN);
        }
        close(&mut state, &b);

        assert_eq!(
            state.files.by_file.len(),
            2,
            "the reverse map grew with open events instead of with documents"
        );
        assert_eq!(state.files.by_file[&file_a], a);
        // A closed document keeps its entry: the handle is still valid, and a
        // navigation target that lands on it still has to name a URI.
        assert_eq!(
            state.files.by_file[&state.files.by_uri[&b].source_file()],
            b
        );
    }
}
