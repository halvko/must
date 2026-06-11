//! The Must language server: a thin LSP transport over the `ide` crate.
//!
//! The main loop is single-threaded for now; every feature already goes
//! through [`ide::Analysis`] snapshots so request handling can move to a
//! worker pool without touching feature code.

mod to_proto;

use std::collections::HashMap;
use std::error::Error;

use base_db::SourceFile;
use ide::AnalysisHost;
use lsp_server::{Connection, ErrorCode, Message, Notification, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn server_capabilities() -> lsp_types::ServerCapabilities {
    lsp_types::ServerCapabilities {
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::FULL,
        )),
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
    files: HashMap<lsp_types::Uri, SourceFile>,
    sender: crossbeam_channel::Sender<Message>,
}

impl GlobalState {
    fn new(connection: &Connection) -> GlobalState {
        GlobalState {
            host: AnalysisHost::new(),
            files: HashMap::new(),
            sender: connection.sender.clone(),
        }
    }

    fn main_loop(&mut self, connection: &Connection) -> ServerResult<()> {
        for msg in &connection.receiver {
            match msg {
                Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    tracing::debug!(method = %req.method, "unhandled request");
                    let resp = Response::new_err(
                        req.id,
                        ErrorCode::MethodNotFound as i32,
                        format!("unhandled method: {}", req.method),
                    );
                    self.sender.send(resp.into())?;
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

    fn handle_notification(&mut self, not: Notification) -> ServerResult<()> {
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(not.params)?;
                let doc = params.text_document;
                let file = self.set_file_text(doc.uri.clone(), doc.text);
                self.publish_diagnostics(doc.uri, file)?;
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
                self.publish_diagnostics(uri, file)?;
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams =
                    serde_json::from_value(not.params)?;
                // The salsa input stays around (inputs cannot be deleted),
                // but clear stale squiggles and forget the mapping.
                self.files.remove(&params.text_document.uri);
                self.send_diagnostics(params.text_document.uri, Vec::new())?;
            }
            _ => tracing::debug!(method = %not.method, "unhandled notification"),
        }
        Ok(())
    }

    fn set_file_text(&mut self, uri: lsp_types::Uri, text: String) -> SourceFile {
        match self.files.get(&uri) {
            Some(&file) => {
                self.host.set_file_text(file, text);
                file
            }
            None => {
                let file = self.host.create_file(uri.to_string(), text);
                self.files.insert(uri, file);
                file
            }
        }
    }

    fn publish_diagnostics(&self, uri: lsp_types::Uri, file: SourceFile) -> ServerResult<()> {
        let analysis = self.host.snapshot();
        let line_index = analysis.line_index(file);
        let diagnostics = analysis
            .diagnostics(file)
            .into_iter()
            .map(|d| to_proto::diagnostic(&line_index, d))
            .collect();
        self.send_diagnostics(uri, diagnostics)
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
