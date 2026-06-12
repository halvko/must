//! In-process LSP tests: run the real main loop against a memory connection
//! and speak the protocol from the client side. No editor, no stdio.

use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as _;

struct TestClient {
    client: Connection,
    server: Option<std::thread::JoinHandle<()>>,
    next_id: i32,
}

impl TestClient {
    fn start() -> TestClient {
        TestClient::start_with(lsp_types::InitializeParams::default())
    }

    fn start_with(init: lsp_types::InitializeParams) -> TestClient {
        let (server_conn, client_conn) = Connection::memory();
        let server = std::thread::spawn(move || {
            must_lsp::run(server_conn).expect("server failed");
        });
        let mut this = TestClient {
            client: client_conn,
            server: Some(server),
            next_id: 0,
        };
        this.request::<lsp_types::request::Initialize>(init);
        this.notify::<lsp_types::notification::Initialized>(lsp_types::InitializedParams {});
        this
    }

    fn request<R: lsp_types::request::Request>(&mut self, params: R::Params) -> R::Result {
        let id = self.send_request::<R>(params);
        let resp = self.response_for(id);
        assert!(resp.error.is_none(), "error response: {:?}", resp.error);
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap()
    }

    /// Send a request without waiting for its response.
    fn send_request<R: lsp_types::request::Request>(&mut self, params: R::Params) -> RequestId {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.client
            .sender
            .send(Request::new(id.clone(), R::METHOD.to_owned(), params).into())
            .unwrap();
        id
    }

    fn response_for(&self, id: RequestId) -> Response {
        loop {
            match self.recv() {
                Message::Response(resp) if resp.id == id => return resp,
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
    // Anchored on the last character of `"x"` (the closing quote): visible
    // and cursor-targetable without implying the whole string is wrong.
    assert_eq!(diag.range.start, lsp_types::Position::new(1, 14));
    assert_eq!(diag.range.end, lsp_types::Position::new(1, 15));

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
fn quick_fix_wraps_fn_body_in_braces() {
    let mut client = TestClient::start();
    let file = uri("file:///fix.must");

    client.open(&file, "static f = fn 42;");
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics.len(), 1);
    let diag_range = diags.diagnostics[0].range;

    let response = client.request::<lsp_types::request::CodeActionRequest>(
        lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            range: diag_range,
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    );
    let actions = response.expect("expected code actions");
    assert_eq!(actions.len(), 1);
    let lsp_types::CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected a code action, got {actions:?}");
    };
    assert_eq!(action.title, "Wrap in `{ }`");

    let changes = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .expect("action has a workspace edit");
    let edits = &changes[&file];
    assert_eq!(edits.len(), 2);
    // `42` spans columns 14..16 on line 0.
    assert_eq!(edits[0].range.start, lsp_types::Position::new(0, 14));
    assert_eq!(edits[0].new_text, "{ ");
    assert_eq!(edits[1].range.start, lsp_types::Position::new(0, 16));
    assert_eq!(edits[1].new_text, " }");

    drop(client);
}

#[test]
fn duplicate_definition_links_to_the_first_one() {
    let client = TestClient::start();
    let file = uri("file:///dup.must");

    client.open(&file, "static name = 1;\nstatic name = 2;\n");
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics.len(), 1);
    let diag = &diags.diagnostics[0];
    assert_eq!(diag.message, "`name` is defined multiple times");
    // On the second definition's name...
    assert_eq!(diag.range.start, lsp_types::Position::new(1, 7));
    assert_eq!(diag.range.end, lsp_types::Position::new(1, 11));
    // ...with a clickable pointer back to the first.
    let related = diag.related_information.as_ref().expect("has related info");
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].message, "first defined here");
    assert_eq!(related[0].location.uri, file);
    assert_eq!(related[0].location.range.start, lsp_types::Position::new(0, 7));
    assert_eq!(related[0].location.range.end, lsp_types::Position::new(0, 11));

    drop(client);
}

#[test]
fn semantic_tokens_over_protocol() {
    let mut client = TestClient::start();
    let file = uri("file:///tokens.must");

    client.open(&file, "static x = 1;");
    client.next_diagnostics();

    let response = client.request::<lsp_types::request::SemanticTokensFullRequest>(
        lsp_types::SemanticTokensParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    );
    let Some(lsp_types::SemanticTokensResult::Tokens(tokens)) = response else {
        panic!("expected full semantic tokens, got {response:?}");
    };
    // Legend order: keyword = 3, operator = 4, variable = 6, number = 2;
    // modifiers: declaration = 1, static = 2.
    let expected = [
        // delta_line, delta_start, length, token_type, modifiers
        (0, 0, 6, 3, 0),  // `static`
        (0, 7, 1, 6, 3),  // `x`: variable, declaration|static
        (0, 2, 1, 4, 0),  // `=`
        (0, 2, 1, 2, 0),  // `1`
    ];
    let actual: Vec<_> = tokens
        .data
        .iter()
        .map(|t| {
            (
                t.delta_line,
                t.delta_start,
                t.length,
                t.token_type,
                t.token_modifiers_bitset,
            )
        })
        .collect();
    assert_eq!(actual, expected);

    drop(client);
}

#[test]
fn out_of_range_positions_are_clamped_not_fatal() {
    let mut client = TestClient::start();
    let file = uri("file:///clamp.must");

    client.open(&file, "static main = fn { print(\"hi\"); };\n");
    client.next_diagnostics();

    // Columns past the end of the line (and lines past EOF) are legal
    // client input; the server must answer, not die.
    for position in [
        lsp_types::Position::new(0, 9999),
        lsp_types::Position::new(9999, 0),
    ] {
        let _ = client.request::<lsp_types::request::HoverRequest>(lsp_types::HoverParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
        });
    }

    // And the server is still alive for real work afterwards.
    let response = client.request::<lsp_types::request::HoverRequest>(lsp_types::HoverParams {
        text_document_position_params: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(0, 7),
        },
        work_done_progress_params: Default::default(),
    });
    assert!(response.is_some(), "server should still answer hovers");

    drop(client);
}

#[test]
fn diagnostics_carry_the_document_version() {
    let client = TestClient::start();
    let file = uri("file:///versioned.must");

    client.open(&file, "static = 1;");
    assert_eq!(client.next_diagnostics().version, Some(0));

    client.change(&file, 7, "static x = 1;");
    assert_eq!(client.next_diagnostics().version, Some(7));

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

#[test]
fn early_semantic_tokens_pull_errors_retryably_then_succeeds_after_open() {
    let mut client = TestClient::start();
    let file = uri("file:///early.must");
    let params = lsp_types::SemanticTokensParams {
        text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    // Zed pulls tokens for a restoring buffer before its `didOpen` reaches
    // the server. That pull gets `ContentModified` (never a `null`, which
    // Zed caches as "no highlighting"); the refresh nudge on `didOpen` —
    // covered below — makes the client pull again, which must succeed.
    let early =
        client.send_request::<lsp_types::request::SemanticTokensFullRequest>(params.clone());
    let resp = client.response_for(early);
    let err = resp.error.expect("early pull answers an error, not null");
    assert_eq!(err.code, -32801, "ContentModified, so the client retries");

    client.open(&file, "static x = 1;");
    client.next_diagnostics();

    let response =
        client.request::<lsp_types::request::SemanticTokensFullRequest>(params);
    let Some(lsp_types::SemanticTokensResult::Tokens(tokens)) = response else {
        panic!("expected full tokens, got {response:?}");
    };
    assert!(!tokens.data.is_empty());

    drop(client);
}

#[test]
fn semantic_tokens_survive_close_and_reopen() {
    let mut client = TestClient::start();
    let file = uri("file:///reopen.must");
    let params = lsp_types::SemanticTokensParams {
        text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let tokens = |response: Option<lsp_types::SemanticTokensResult>| match response {
        Some(lsp_types::SemanticTokensResult::Tokens(tokens)) => tokens.data,
        other => panic!("expected full tokens, got {other:?}"),
    };

    client.open(&file, "static x = 1;");
    client.next_diagnostics();
    let first = tokens(
        client.request::<lsp_types::request::SemanticTokensFullRequest>(params.clone()),
    );
    assert!(!first.is_empty());

    client.notify::<lsp_types::notification::DidCloseTextDocument>(
        lsp_types::DidCloseTextDocumentParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
        },
    );
    client.next_diagnostics();

    client.open(&file, "static x = 1;");
    client.next_diagnostics();
    let second = tokens(
        client.request::<lsp_types::request::SemanticTokensFullRequest>(params),
    );
    assert_eq!(first, second);

    drop(client);
}


#[test]
fn did_open_triggers_semantic_tokens_refresh_when_supported() {
    let init = lsp_types::InitializeParams {
        capabilities: lsp_types::ClientCapabilities {
            workspace: Some(lsp_types::WorkspaceClientCapabilities {
                semantic_tokens: Some(lsp_types::SemanticTokensWorkspaceClientCapabilities {
                    refresh_support: Some(true),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let client = TestClient::start_with(init);
    let file = uri("file:///refresh.must");

    client.open(&file, "static x = 1;");

    // Zed doesn't issue the initial token pull for reopened buffers until
    // an edit (zed#57651): the server must nudge it to re-pull as soon as
    // the document is known.
    loop {
        match client.recv() {
            Message::Request(req) if req.method == "workspace/semanticTokens/refresh" => {
                client
                    .client
                    .sender
                    .send(Response::new_ok(req.id, serde_json::Value::Null).into())
                    .unwrap();
                break;
            }
            _ => continue,
        }
    }

    drop(client);
}

#[test]
fn run_lens_executes_the_buffer() {
    let mut client = TestClient::start();
    let file = uri("file:///lens.must");

    client.open(
        &file,
        "static main = fn {\n    print(\"lens says hi\");\n};\nstatic with_args = fn (n: usize) -> usize { n }\n",
    );
    client.next_diagnostics();

    // One lens: `main` (zero params). `with_args` needs arguments — that's
    // the debugger's entry-expression flow.
    let lenses = client
        .request::<lsp_types::request::CodeLensRequest>(lsp_types::CodeLensParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("code lenses");
    assert_eq!(lenses.len(), 1);
    let command = lenses[0].command.as_ref().expect("lens has a command");
    assert_eq!(command.title, "▶ run main()");
    assert_eq!(command.command, "must.run");
    // Anchored on `main`.
    assert_eq!(lenses[0].range.start, lsp_types::Position::new(0, 7));

    // Clicking the lens = executeCommand with the lens's arguments. The
    // showMessage report arrives before the response; collect both.
    let args = command.arguments.clone().unwrap();
    let id = client.send_request::<lsp_types::request::ExecuteCommand>(
        lsp_types::ExecuteCommandParams {
            command: command.command.clone(),
            arguments: args,
            work_done_progress_params: Default::default(),
        },
    );
    let mut shown: Option<lsp_types::ShowMessageParams> = None;
    loop {
        match client.recv() {
            lsp_server::Message::Notification(not) if not.method == "window/showMessage" => {
                shown = Some(serde_json::from_value(not.params).unwrap());
            }
            lsp_server::Message::Response(resp) if resp.id == id => {
                assert!(resp.error.is_none(), "run failed: {:?}", resp.error);
                break;
            }
            _ => {}
        }
    }
    let message = shown.expect("a showMessage report");
    assert_eq!(message.typ, lsp_types::MessageType::INFO);
    assert!(
        message.message.contains("lens says hi"),
        "message: {}",
        message.message
    );

    drop(client);
}
