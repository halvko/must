//! In-process LSP tests: run the real main loop against a memory connection
//! and speak the protocol from the client side. No editor, no stdio.

use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::notification::Notification as _;

struct TestClient {
    client: Connection,
    server: Option<std::thread::JoinHandle<()>>,
    next_id: i32,
}

impl TestClient {
    fn start() -> TestClient {
        let (server_conn, client_conn) = Connection::memory();
        let server = std::thread::spawn(move || {
            must_lsp::run(server_conn).expect("server failed");
        });
        let mut this = TestClient {
            client: client_conn,
            server: Some(server),
            next_id: 0,
        };
        this.request::<lsp_types::request::Initialize>(lsp_types::InitializeParams::default());
        this.notify::<lsp_types::notification::Initialized>(lsp_types::InitializedParams {});
        this
    }

    fn request<R: lsp_types::request::Request>(&mut self, params: R::Params) -> R::Result {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.client
            .sender
            .send(Request::new(id.clone(), R::METHOD.to_owned(), params).into())
            .unwrap();
        loop {
            match self.recv() {
                Message::Response(resp) if resp.id == id => {
                    assert!(resp.error.is_none(), "error response: {:?}", resp.error);
                    return serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
                }
                _ => continue,
            }
        }
    }

    fn notify<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        self.client
            .sender
            .send(Notification::new(N::METHOD.to_owned(), params).into())
            .unwrap();
    }

    fn recv(&self) -> Message {
        self.client
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("no message from server within 10s")
    }

    fn next_diagnostics(&self) -> lsp_types::PublishDiagnosticsParams {
        loop {
            if let Message::Notification(not) = self.recv() {
                if not.method == lsp_types::notification::PublishDiagnostics::METHOD {
                    return serde_json::from_value(not.params).unwrap();
                }
            }
        }
    }

    fn open(&self, uri: &lsp_types::Uri, text: &str) {
        self.notify::<lsp_types::notification::DidOpenTextDocument>(
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

    fn change(&self, uri: &lsp_types::Uri, version: i32, text: &str) {
        self.notify::<lsp_types::notification::DidChangeTextDocument>(
            lsp_types::DidChangeTextDocumentParams {
                text_document: lsp_types::VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version,
                },
                content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_owned(),
                }],
            },
        );
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        self.request::<lsp_types::request::Shutdown>(());
        self.notify::<lsp_types::notification::Exit>(());
        if let Some(server) = self.server.take() {
            server.join().expect("server thread panicked");
        }
    }
}

fn uri(s: &str) -> lsp_types::Uri {
    s.parse().unwrap()
}

#[test]
fn publishes_parse_errors_on_open_and_change() {
    let client = TestClient::start();
    let file = uri("file:///test.must");

    // Broken file: missing `;` terminator on the let.
    client.open(&file, "static main = fn {\n    let a = \"x\"\n    print(a);\n}\n");
    let diags = client.next_diagnostics();
    assert_eq!(diags.uri, file);
    assert_eq!(diags.diagnostics.len(), 1);
    let diag = &diags.diagnostics[0];
    assert_eq!(diag.message, "expected `;`");
    // Points at `print` on line 2.
    assert_eq!(diag.range.start.line, 2);

    // Fix the file: diagnostics clear.
    client.change(
        &file,
        1,
        "static main = fn {\n    let a = \"x\";\n    print(a);\n}\n",
    );
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics, vec![]);

    drop(client);
}

#[test]
fn goto_definition_over_protocol() {
    let mut client = TestClient::start();
    let file = uri("file:///def.must");

    // `b` on line 0 column 15 refers to the item on line 1.
    client.open(&file, "const a = fn { b() };\nstatic b = fn { a() };\n");
    client.next_diagnostics();

    let response = client.request::<lsp_types::request::GotoDefinition>(
        lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
                position: lsp_types::Position::new(0, 15),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    );
    let Some(lsp_types::GotoDefinitionResponse::Scalar(location)) = response else {
        panic!("expected scalar definition response, got {response:?}");
    };
    assert_eq!(location.uri, file);
    assert_eq!(location.range.start, lsp_types::Position::new(1, 7));
    assert_eq!(location.range.end, lsp_types::Position::new(1, 8));

    drop(client);
}

#[test]
fn hover_over_protocol() {
    let mut client = TestClient::start();
    let file = uri("file:///hover.must");

    client.open(&file, "static main = fn {\n    let s = \"hello\";\n    print(s);\n}\n");
    client.next_diagnostics();

    // Hover `s` in `print(s)` on line 2.
    let response = client.request::<lsp_types::request::HoverRequest>(lsp_types::HoverParams {
        text_document_position_params: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(2, 10),
        },
        work_done_progress_params: Default::default(),
    });
    let hover = response.expect("expected a hover response");
    let lsp_types::HoverContents::Markup(content) = hover.contents else {
        panic!("expected markup hover contents");
    };
    assert_eq!(content.value, "```must\ns: str\n```");

    drop(client);
}

#[test]
fn close_clears_diagnostics() {
    let client = TestClient::start();
    let file = uri("file:///broken.must");

    client.open(&file, "static = 1;");
    assert_eq!(client.next_diagnostics().diagnostics.len(), 1);

    client.notify::<lsp_types::notification::DidCloseTextDocument>(
        lsp_types::DidCloseTextDocumentParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
        },
    );
    assert_eq!(client.next_diagnostics().diagnostics, vec![]);

    drop(client);
}
