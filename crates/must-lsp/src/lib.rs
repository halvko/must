//! The Must language server: a thin LSP transport over the `ide` crate.
//!
//! The main loop is single-threaded for now; every feature already goes
//! through [`ide::Analysis`] snapshots so request handling can move to a
//! worker pool without touching feature code.

mod to_proto;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
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

/// What the server knows about one document, and the only place it says so.
///
/// The map below holds one entry per URI and one salsa input per entry, so a
/// document cannot be open and closed at the same time, cannot be tracked
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
    files: HashMap<lsp_types::Uri, FileState>,
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

    fn handle_notification(&mut self, notification: Notification) -> ServerResult<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let doc = params.text_document;
                let file = self.open_file(doc.uri.clone(), doc.text);
                self.publish_diagnostics(doc.uri, file, doc.version)?;
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
                let Some(FileState::Open(file)) = self.files.get(&uri).copied() else {
                    tracing::warn!(uri = %uri.as_str(), "didChange for a document that is not open");
                    return Ok(());
                };
                self.host.set_file_text(file, change.text);
                self.publish_diagnostics(uri, file, version)?;
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
                    self.send_diagnostics(uri, Vec::new(), None)?;
                }
            }
            _ => tracing::debug!(method = %notification.method, "unhandled notification"),
        }
        Ok(())
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
        match self.files.entry(uri) {
            Entry::Occupied(mut entry) => {
                // Fill the input first, then record the state, so this reads
                // the same way round as `close_file` below.
                let file = entry.get().source_file();
                self.host.set_file_text(file, text);
                entry.insert(FileState::Open(file));
                file
            }
            Entry::Vacant(entry) => {
                let file = self.host.create_file(entry.key().to_string(), text);
                entry.insert(FileState::Open(file));
                file
            }
        }
    }

    /// Gives the buffer back: empties the document's text and parks its
    /// handle. Returns whether this is a document the server tracks, which is
    /// what decides if the client hears about it.
    fn close_file(&mut self, uri: &lsp_types::Uri) -> bool {
        match self.files.get_mut(uri) {
            Some(state) => {
                if let FileState::Open(file) = *state {
                    self.host.close_file(file);
                    *state = FileState::Closed(file);
                } else {
                    // Closing twice: there is nothing left to empty, but the
                    // client still gets its cleared diagnostics.
                    tracing::warn!(uri = %uri.as_str(), "didClose for a document that is not open");
                }
                true
            }
            None => {
                tracing::warn!(uri = %uri.as_str(), "didClose for an unknown document");
                false
            }
        }
    }

    fn publish_diagnostics(
        &self,
        uri: lsp_types::Uri,
        file: SourceFile,
        version: i32,
    ) -> ServerResult<()> {
        let analysis = self.host.snapshot();
        let line_index = analysis.line_index(file);
        let diagnostics = analysis
            .diagnostics(file)
            .into_iter()
            .map(|d| to_proto::diagnostic(&line_index, d))
            .collect();
        self.send_diagnostics(uri, diagnostics, Some(version))
    }

    fn send_diagnostics(
        &self,
        uri: lsp_types::Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) -> ServerResult<()> {
        let params = lsp_types::PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        };
        self.sender
            .send(Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROKEN: &str = "static = 1;";

    fn uri(s: &str) -> lsp_types::Uri {
        s.parse().unwrap()
    }

    /// A server state wired to a channel the test holds but mostly ignores;
    /// these tests are about the file table, not the wire. The `Connection`
    /// comes back so the receiving end stays alive.
    fn server() -> (GlobalState, Connection) {
        let (server, client) = Connection::memory();
        (GlobalState::new(&server), client)
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

    #[test]
    fn closing_parks_the_handle_in_place() {
        let (mut state, _client) = server();
        let doc = uri("file:///a.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files[&doc].source_file();
        assert!(matches!(state.files[&doc], FileState::Open(_)));

        close(&mut state, &doc);

        assert_eq!(
            state.files.len(),
            1,
            "the entry is what keeps the input reusable"
        );
        assert!(matches!(state.files[&doc], FileState::Closed(_)));
        assert!(
            state.files[&doc].source_file() == file,
            "closing swapped the salsa input"
        );
    }

    #[test]
    fn reopening_reuses_the_parked_input_and_analysis_resumes() {
        let (mut state, _client) = server();
        let doc = uri("file:///a.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files[&doc].source_file();
        close(&mut state, &doc);

        open(&mut state, &doc, BROKEN);

        assert!(matches!(state.files[&doc], FileState::Open(_)));
        assert!(
            state.files[&doc].source_file() == file,
            "reopening minted a second input for the same document"
        );
        assert_eq!(state.host.snapshot().diagnostics(file).len(), 1);
    }

    #[test]
    fn open_close_cycles_leave_one_entry_and_one_input() {
        let (mut state, _client) = server();
        let doc = uri("file:///churn.must");
        open(&mut state, &doc, BROKEN);
        let file = state.files[&doc].source_file();

        for _ in 0..50 {
            close(&mut state, &doc);
            open(&mut state, &doc, BROKEN);
        }

        assert_eq!(state.files.len(), 1);
        assert!(
            state.files[&doc].source_file() == file,
            "a cycle minted a new salsa input; memory would grow per open event"
        );
    }

    #[test]
    fn closing_says_nothing_about_documents_the_server_never_saw() {
        let (mut state, client) = server();
        let doc = uri("file:///phantom.must");

        close(&mut state, &doc);
        assert!(state.files.is_empty(), "a close created a document");
        assert!(
            client.receiver.try_recv().is_err(),
            "published diagnostics for a document the client never opened"
        );

        // A document the server does know about still gets its squiggles
        // cleared, even when the close is a repeat.
        open(&mut state, &doc, BROKEN);
        close(&mut state, &doc);
        while client.receiver.try_recv().is_ok() {}
        close(&mut state, &doc);
        assert!(
            client.receiver.try_recv().is_ok(),
            "a repeated close left the client's squiggles in place"
        );
    }

    #[test]
    fn changes_to_documents_that_are_not_open_are_ignored() {
        let (mut state, client) = server();
        let doc = uri("file:///stray.must");

        change(&mut state, &doc, BROKEN);
        assert!(state.files.is_empty(), "a change created a document");
        assert!(
            client.receiver.try_recv().is_err(),
            "published diagnostics for a document the client never opened"
        );

        open(&mut state, &doc, BROKEN);
        close(&mut state, &doc);
        while client.receiver.try_recv().is_ok() {}

        change(&mut state, &doc, BROKEN);
        assert!(
            matches!(state.files[&doc], FileState::Closed(_)),
            "a stray change refilled a buffer the client had given back"
        );
        assert!(client.receiver.try_recv().is_err());
    }
}
