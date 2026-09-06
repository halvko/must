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
            if let Message::Notification(not) = self.recv()
                && not.method == lsp_types::notification::PublishDiagnostics::METHOD
            {
                return serde_json::from_value(not.params).unwrap();
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

    fn close(&self, uri: &lsp_types::Uri) {
        self.notify::<lsp_types::notification::DidCloseTextDocument>(
            lsp_types::DidCloseTextDocumentParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
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
    client.open(
        &file,
        "static main = fn {
    let a = \"x\"
    print(a);
}
",
    );
    let diags = client.next_diagnostics();
    assert_eq!(diags.uri, file);
    assert_eq!(diags.version, Some(0));
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
    assert_eq!(diags.version, Some(1));
    assert_eq!(diags.diagnostics, vec![]);

    drop(client);
}

#[test]
fn diagnostic_positions_use_utf16_code_units() {
    let client = TestClient::start();
    let file = uri("file:///unicode.must");

    // Missing `;` after the let; the diagnostic anchors on the string's last
    // character (the closing quote). The string's `é` is 2 bytes / 1 UTF-16
    // unit and its emoji is 4 bytes / 2 UTF-16 units, so the token ends at
    // byte column 20 but UTF-16 column 17 — the anchor is 16..17.
    client.open(
        &file,
        "static main = fn {\n    let a = \"é😀\" print(a);\n}\n",
    );
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics.len(), 1);
    let diag = &diags.diagnostics[0];
    assert_eq!(diag.message, "expected `;`");
    assert_eq!(diag.range.start, lsp_types::Position::new(1, 16));
    assert_eq!(diag.range.end, lsp_types::Position::new(1, 17));

    drop(client);
}

#[test]
fn goto_definition_over_protocol() {
    let mut client = TestClient::start();
    let file = uri("file:///def.must");

    // `b` on line 0 column 15 refers to the item on line 1.
    client.open(&file, "const a = fn { b() };\nstatic b = fn { a() };\n");
    client.next_diagnostics();

    let response =
        client.request::<lsp_types::request::GotoDefinition>(lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
                position: lsp_types::Position::new(0, 15),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
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

    client.open(
        &file,
        "static main = fn {\n    let s = \"hello\";\n    print(s);\n}\n",
    );
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

    client.open(&file, "static f = fn true;");
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics.len(), 1);
    let diag_range = diags.diagnostics[0].range;

    let response =
        client.request::<lsp_types::request::CodeActionRequest>(lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            range: diag_range,
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
    let actions = response.expect("expected code actions");
    assert_eq!(actions.len(), 1);
    let lsp_types::CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected a code action, got {actions:?}");
    };
    assert_eq!(action.title, "Wrap in `{ }`");

    // Uri-keyed maps are the shape the LSP protocol mandates.
    #[allow(clippy::mutable_key_type)]
    let changes = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .expect("action has a workspace edit");
    let edits = &changes[&file];
    assert_eq!(edits.len(), 2);
    // `true` spans columns 14..18 on line 0.
    assert_eq!(edits[0].range.start, lsp_types::Position::new(0, 14));
    assert_eq!(edits[0].new_text, "{ ");
    assert_eq!(edits[1].range.start, lsp_types::Position::new(0, 18));
    assert_eq!(edits[1].new_text, " }");

    drop(client);
}

#[test]
fn quick_fix_inserts_semicolon_from_cursor_on_anchor() {
    let mut client = TestClient::start();
    let file = uri("file:///semicolon.must");

    client.open(
        &file,
        "static main = fn {\n    let a = \"x\"\n    print(a);\n}\n",
    );
    client.next_diagnostics();

    // The diagnostic anchors on the string's last character (columns
    // 14..15), so the Insert `;` fix is reachable from a cursor there —
    // no longer from anywhere else on the token. Here the last character.
    let response =
        client.request::<lsp_types::request::CodeActionRequest>(lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            range: lsp_types::Range::new(
                lsp_types::Position::new(1, 14),
                lsp_types::Position::new(1, 14),
            ),
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
    let actions = response.expect("expected code actions");
    assert_eq!(actions.len(), 1);
    let lsp_types::CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected a code action, got {actions:?}");
    };
    assert_eq!(action.title, "Insert `;`");

    let changes = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .expect("action has a workspace edit");
    let edits = &changes[&file];
    assert_eq!(edits.len(), 1);
    // The `;` still inserts at the token's end.
    assert_eq!(edits[0].range.start, lsp_types::Position::new(1, 15));
    assert_eq!(edits[0].new_text, ";");

    drop(client);
}

#[test]
fn quick_fix_makes_an_immutable_binding_mutable() {
    let mut client = TestClient::start();
    let file = uri("file:///make-mut.must");

    client.open(&file, "static f = fn { let x: usize = 1; x = 2; };");
    let diags = client.next_diagnostics();
    assert_eq!(diags.diagnostics.len(), 2); // the error + the companion hint
    let diag = diags
        .diagnostics
        .iter()
        .find(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
        .expect("has the error diagnostic");
    assert_eq!(
        diag.message,
        "cannot assign to `x`: it is not declared `mut`"
    );

    let response =
        client.request::<lsp_types::request::CodeActionRequest>(lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            range: diag.range,
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
    let actions = response.expect("expected code actions");
    assert_eq!(actions.len(), 1);
    let lsp_types::CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected a code action, got {actions:?}");
    };
    assert_eq!(action.title, "Make `x` mutable");

    // Uri-keyed maps are the shape the LSP protocol mandates.
    #[allow(clippy::mutable_key_type)]
    let changes = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .expect("action has a workspace edit");
    let edits = &changes[&file];
    assert_eq!(edits.len(), 1);
    // A pure insertion of `mut ` right before the binding's name `x`
    // (line 0, column 20): applying it yields `let mut x = 1;`.
    assert_eq!(edits[0].range.start, lsp_types::Position::new(0, 20));
    assert_eq!(edits[0].range.end, lsp_types::Position::new(0, 20));
    assert_eq!(edits[0].new_text, "mut ");

    drop(client);
}

#[test]
fn duplicate_definition_links_to_the_first_one() {
    let client = TestClient::start();
    let file = uri("file:///dup.must");

    client.open(&file, "static name: usize = 1;\nstatic name: usize = 2;\n");
    let diags = client.next_diagnostics();
    // The error, plus a companion hint at the first definition.
    assert_eq!(diags.diagnostics.len(), 2);
    let diag = diags
        .diagnostics
        .iter()
        .find(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
        .expect("has the error diagnostic");
    assert_eq!(diag.message, "`name` is defined multiple times");
    // On the second definition's name...
    assert_eq!(diag.range.start, lsp_types::Position::new(1, 7));
    assert_eq!(diag.range.end, lsp_types::Position::new(1, 11));
    // ...with a clickable pointer back to the first.
    let related = diag.related_information.as_ref().expect("has related info");
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].message, "first defined here");
    assert_eq!(related[0].location.uri, file);
    assert_eq!(
        related[0].location.range.start,
        lsp_types::Position::new(0, 7)
    );
    assert_eq!(
        related[0].location.range.end,
        lsp_types::Position::new(0, 11)
    );
    // The companion sits at the first definition, explains the connection
    // in prose, and links back to the error.
    let companion = diags
        .diagnostics
        .iter()
        .find(|d| d.severity == Some(lsp_types::DiagnosticSeverity::INFORMATION))
        .expect("has the companion hint");
    assert_eq!(companion.range.start, lsp_types::Position::new(0, 7));
    assert_eq!(
        companion.message,
        "first defined here — causes the error on line 2: `name` is defined multiple times"
    );
    let back = companion
        .related_information
        .as_ref()
        .expect("companion links back");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].message, "the error reported here");
    assert_eq!(back[0].location.range.start, lsp_types::Position::new(1, 7));

    drop(client);
}

#[test]
fn semantic_tokens_over_protocol() {
    let mut client = TestClient::start();
    let file = uri("file:///tokens.must");

    client.open(&file, "static x: usize = 1;");
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
        (0, 0, 6, 3, 0), // `static`
        (0, 7, 1, 6, 3), // `x`: variable, declaration|static
        (0, 3, 5, 8, 4), // `usize`: type, defaultLibrary
        (0, 6, 1, 4, 0), // `=`
        (0, 2, 1, 2, 0), // `1`
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
fn semantic_tokens_carry_the_mutable_modifier() {
    let mut client = TestClient::start();
    let file = uri("file:///mut-tokens.must");

    client.open(&file, "static f = fn { let mut x = 1; x = 2; };");
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
    // Modifier bits: declaration = 1, static = 2, defaultLibrary = 4,
    // mutable = 8 (the legend's fourth entry).
    let expected = [
        // delta_line, delta_start, length, token_type, modifiers
        (0, 0, 6, 3, 0), // `static`
        (0, 7, 1, 5, 3), // `f`: function, declaration|static
        (0, 2, 1, 4, 0), // `=`
        (0, 2, 2, 3, 0), // `fn`
        (0, 5, 3, 3, 0), // `let`
        (0, 4, 3, 3, 0), // `mut`
        (0, 4, 1, 6, 9), // `x`: variable, declaration|mutable
        (0, 2, 1, 4, 0), // `=`
        (0, 2, 1, 2, 0), // `1`
        (0, 3, 1, 6, 8), // `x` (the assignment target): variable, mutable
        (0, 2, 1, 4, 0), // `=`
        (0, 2, 1, 2, 0), // `2`
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
fn out_of_range_positions_clamp_into_the_requested_line() {
    let mut client = TestClient::start();
    let file = uri("file:///clamp.must");

    // Line 1 carries `delta` and multi-byte characters: a column past line
    // 0's end, taken literally, lands on line 1's text — inside `delta`, or
    // mid-character inside the emoji.
    client.open(&file, "static main = fn {\n    let delta = \"é😀\";\n}\n");
    client.next_diagnostics();

    fn hover_at(
        client: &mut TestClient,
        file: &lsp_types::Uri,
        position: lsp_types::Position,
    ) -> Option<lsp_types::Hover> {
        client.request::<lsp_types::request::HoverRequest>(lsp_types::HoverParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
        })
    }

    // Columns past line 0's end (27 would land on `delta`, 40 mid-emoji on
    // line 1) and lines past EOF must resolve into the requested line: line
    // 0 ends at `{\n`, a nonexistent line falls back to the last, empty one
    // — no identifier either way, so no hover, and no error.
    for position in [
        lsp_types::Position::new(0, 27),
        lsp_types::Position::new(0, 9999),
        lsp_types::Position::new(9999, 0),
        lsp_types::Position::new(9999, 9999),
    ] {
        assert!(
            hover_at(&mut client, &file, position).is_none(),
            "position {position:?} leaked out of its requested line"
        );
    }

    // The clamping did not break real answers: line 1's `delta` (UTF-16
    // columns 8..13) and line 0's `main` (7..11) still hover.
    let response = hover_at(&mut client, &file, lsp_types::Position::new(1, 9))
        .expect("hover on line 1's `delta` disappeared");
    let lsp_types::HoverContents::Markup(content) = response.contents else {
        panic!("expected markup hover contents");
    };
    assert_eq!(content.value, "```must\ndelta: str\n```");
    assert!(
        hover_at(&mut client, &file, lsp_types::Position::new(0, 7)).is_some(),
        "hover on line 0's `main` disappeared"
    );

    drop(client);
}

#[test]
fn diagnostics_carry_the_document_version() {
    let client = TestClient::start();
    let file = uri("file:///versioned.must");

    client.open(&file, "static = true;");
    assert_eq!(client.next_diagnostics().version, Some(0));

    client.change(&file, 7, "static x: usize = 1;");
    assert_eq!(client.next_diagnostics().version, Some(7));

    drop(client);
}

#[test]
fn close_clears_diagnostics() {
    let client = TestClient::start();
    let file = uri("file:///broken.must");

    client.open(&file, "static = true;");
    assert_eq!(client.next_diagnostics().diagnostics.len(), 1);

    client.close(&file);
    let diags = client.next_diagnostics();
    assert_eq!(diags.version, None);
    assert_eq!(diags.diagnostics, vec![]);

    drop(client);
}

#[test]
fn reopening_a_closed_file_resumes_diagnostics_and_edits() {
    let client = TestClient::start();
    let file = uri("file:///reopened.must");

    client.open(&file, "static = true;");
    assert_eq!(client.next_diagnostics().diagnostics.len(), 1);
    client.close(&file);
    assert_eq!(client.next_diagnostics().diagnostics, vec![]);

    // The server emptied this document's text on close; reopening has to
    // refill the parked input rather than answer from what is left of it,
    // which would be an empty file with no diagnostics.
    client.open(&file, "static = true;");
    let diags = client.next_diagnostics();
    assert_eq!(diags.uri, file);
    assert_eq!(diags.diagnostics.len(), 1);

    // Edits still land on the revived handle.
    client.change(&file, 1, "static a: usize = 1;");
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

    client.open(&file, "static x: usize = 1;");
    client.next_diagnostics();

    let response = client.request::<lsp_types::request::SemanticTokensFullRequest>(params);
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

    client.open(&file, "static x: usize = 1;");
    client.next_diagnostics();
    let first =
        tokens(client.request::<lsp_types::request::SemanticTokensFullRequest>(params.clone()));
    assert!(!first.is_empty());

    client.notify::<lsp_types::notification::DidCloseTextDocument>(
        lsp_types::DidCloseTextDocumentParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
        },
    );
    client.next_diagnostics();

    client.open(&file, "static x: usize = 1;");
    client.next_diagnostics();
    let second = tokens(client.request::<lsp_types::request::SemanticTokensFullRequest>(params));
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

    client.open(&file, "static x: usize = 1;");

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

#[test]
fn completion_capability_is_advertised_with_dot_and_colon_triggers() {
    // `{` is deliberately NOT here: opening a `{` — a function body chief
    // among them — is exactly where predictions are least wanted. The
    // arm-list template at `match s {|}` still exists — see
    // `snippet_capable_client_receives_the_match_arm_template_inside_empty_braces`
    // below — it's just reachable only by an explicit invoke, not a
    // spontaneous one fired the instant the client auto-closes `{`.
    let capabilities = must_lsp::server_capabilities();
    let completion = capabilities
        .completion_provider
        .expect("completion_provider capability advertised");
    assert_eq!(completion.resolve_provider, Some(false));
    assert_eq!(
        completion.trigger_characters,
        Some(vec![".".to_owned(), ":".to_owned()])
    );
}

#[test]
fn completion_over_protocol_returns_ranked_items_with_a_text_edit() {
    let mut client = TestClient::start();
    let file = uri("file:///completion.must");

    client.open(
        &file,
        "static main = fn {\n    let x = \"hi\";\n    print(x);\n}\n",
    );
    client.next_diagnostics();

    // Right before the `x` argument on line 2 (`    print(x);`, column 10):
    // an expression position with a visible local, the enclosing `main`
    // itself as a file item, and the builtin functions/keywords.
    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(2, 10),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };

    let local = items
        .iter()
        .find(|it| it.label == "x")
        .expect("the local `x` is offered");
    assert_eq!(local.kind, Some(lsp_types::CompletionItemKind::VARIABLE));
    // `2`: the type tier — no typed prefix here and a call-argument
    // hole carries no provable expectation, so every candidate ties at the
    // worst tier. `10`: the `Local` provenance tier — two digits wide,
    // spaced by tens, to leave room for the `Gold` tier (member/match-arm
    // completions' single best answer) below it. See `ide::completions`'s
    // module doc.
    assert_eq!(local.sort_text.as_deref(), Some("2_10_x"));
    let edit = match local.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    // No prefix typed (the `(` right before the cursor isn't an identifier
    // character), so the edit is a pure insertion right at the cursor.
    assert_eq!(edit.range.start, lsp_types::Position::new(2, 10));
    assert_eq!(edit.range.end, lsp_types::Position::new(2, 10));
    assert_eq!(edit.new_text, "x");

    let main_item = items
        .iter()
        .find(|it| it.label == "main")
        .expect("the enclosing item itself is offered (recursion is legal)");
    assert_eq!(
        main_item.kind,
        Some(lsp_types::CompletionItemKind::FUNCTION)
    );
    assert_eq!(main_item.detail.as_deref(), Some("fn()"));

    let print_item = items
        .iter()
        .find(|it| it.label == "print")
        .expect("the builtin `print` is offered");
    assert_eq!(
        print_item.kind,
        Some(lsp_types::CompletionItemKind::FUNCTION)
    );

    // Not a fresh statement (nested inside `print(...)`'s argument list), so
    // no `let` — but the ordinary expression-position keywords are there.
    assert!(!items.iter().any(|it| it.label == "let"));
    let if_kw = items
        .iter()
        .find(|it| it.label == "if")
        .expect("keywords are offered too");
    assert_eq!(if_kw.kind, Some(lsp_types::CompletionItemKind::KEYWORD));

    drop(client);
}

/// Type tier end to end: in a typed context (the typed prefix `g` in an
/// annotated `let`'s initializer expects `str`), `sortText` leads with the
/// type tier — a function whose *return* satisfies the expectation
/// outranks one whose doesn't, across provenance-equal candidates.
#[test]
fn completion_sort_text_reflects_the_type_expectation() {
    let mut client = TestClient::start();
    let file = uri("file:///typed_completion.must");

    client.open(
        &file,
        "static get_s: fn() -> str = fn () -> str { \"s\" };\n\
         static get_n: fn() -> usize = fn () -> usize { 1 };\n\
         static main = fn {\n    let x: str = g;\n};\n",
    );
    client.next_diagnostics();

    // Right after the `g` on line 3 (`    let x: str = g;`, column 18).
    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(3, 18),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };

    let sort_text = |label: &str| {
        items
            .iter()
            .find(|it| it.label == label)
            .unwrap_or_else(|| panic!("`{label}` is offered: {items:?}"))
            .sort_text
            .clone()
            .expect("sortText is set")
    };
    // Calling `get_s` yields the expected `str`: type tier 1. `get_n`
    // can't satisfy the position: tier 2, like an untyped position.
    assert_eq!(sort_text("get_s"), "1_20_get_s");
    assert_eq!(sort_text("get_n"), "2_20_get_n");
    assert!(
        sort_text("get_s") < sort_text("get_n"),
        "the satisfying candidate sorts first"
    );

    drop(client);
}

#[test]
fn dot_triggered_completion_over_protocol_returns_field_items() {
    let mut client = TestClient::start();
    let file = uri("file:///dot_completion.must");

    client.open(
        &file,
        "type Point = struct { x: usize, y: usize };\nstatic main = fn {\n    let p = Point(struct { x = 1, y = 2 });\n    p.\n}\n",
    );
    client.next_diagnostics();

    // Right after `p.` on line 3 (`    p.`, column 6): a field-access
    // position over a record local.
    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(3, 6),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };

    assert_eq!(items.len(), 2, "expected exactly the two fields: {items:?}");
    let x_field = items
        .iter()
        .find(|it| it.label == "x")
        .expect("field `x` is offered");
    assert_eq!(x_field.kind, Some(lsp_types::CompletionItemKind::FIELD));
    assert_eq!(x_field.detail.as_deref(), Some("usize"));
    let y_field = items
        .iter()
        .find(|it| it.label == "y")
        .expect("field `y` is offered");
    assert_eq!(y_field.kind, Some(lsp_types::CompletionItemKind::FIELD));
    assert_eq!(y_field.detail.as_deref(), Some("usize"));

    drop(client);
}

/// `InitializeParams` advertising (or not) `snippetSupport` — the one
/// capability snippet insertion gates on (`GlobalState::new`'s
/// `snippet_support`).
fn init_with_snippet_support(support: bool) -> lsp_types::InitializeParams {
    lsp_types::InitializeParams {
        capabilities: lsp_types::ClientCapabilities {
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                completion: Some(lsp_types::CompletionClientCapabilities {
                    completion_item: Some(lsp_types::CompletionItemCapability {
                        snippet_support: Some(support),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Fetches the `add` completion item at the fixed spot both snippet tests
/// share: a fresh statement in `main`'s body, where `add` (two params) is
/// visible as a file item.
fn add_completion_item(
    client: &mut TestClient,
    file: &lsp_types::Uri,
) -> lsp_types::CompletionItem {
    client.open(
        file,
        "static add = fn (a: usize, b: usize) -> usize { a };\nstatic main = fn {\n    \n};\n",
    );
    client.next_diagnostics();

    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(2, 4),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };
    items
        .into_iter()
        .find(|it| it.label == "add")
        .expect("`add` is offered")
}

#[test]
fn snippet_capable_client_receives_snippet_format_items() {
    let mut client = TestClient::start_with(init_with_snippet_support(true));
    let file = uri("file:///snippet_capable.must");

    let add = add_completion_item(&mut client, &file);
    assert_eq!(
        add.insert_text_format,
        Some(lsp_types::InsertTextFormat::SNIPPET)
    );
    let edit = match add.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    assert_eq!(edit.new_text, "add($1)");

    drop(client);
}

#[test]
fn snippet_incapable_client_receives_the_plain_fallback() {
    // `TestClient::start` uses `InitializeParams::default()` — no
    // `snippetSupport` at all — but spelling it out here documents exactly
    // what's under test, matching the capable variant above.
    let mut client = TestClient::start_with(init_with_snippet_support(false));
    let file = uri("file:///snippet_incapable.must");

    let add = add_completion_item(&mut client, &file);
    assert_eq!(add.insert_text_format, None);
    let edit = match add.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    // The plain fallback is still the call form (`add()`), not the bare
    // literal snippet text (`add($1)`) and not the bare name either.
    assert_eq!(edit.new_text, "add()");

    drop(client);
}

/// Fetches the `match arms` template completion at `position` in `text` —
/// the multi-line snippet the capability tests below check, at either
/// shape of the arm-list slot (see `ide::completions::ArmListShape`).
fn match_template_item(
    client: &mut TestClient,
    file: &lsp_types::Uri,
    text: &str,
    position: lsp_types::Position,
) -> lsp_types::CompletionItem {
    client.open(file, text);
    client.next_diagnostics();

    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };
    items
        .into_iter()
        .find(|it| it.label == "match arms")
        .expect("the arm-list template is offered")
}

const MATCH_TEMPLATE_NO_BRACES_SOURCE: &str =
    "type Shape = enum { Circle(usize), Point };\nstatic f = fn (s: Shape) {\n    match s \n};\n";

#[test]
fn snippet_capable_client_receives_the_match_arm_template() {
    let mut client = TestClient::start_with(init_with_snippet_support(true));
    let file = uri("file:///match_template_capable.must");

    let template = match_template_item(
        &mut client,
        &file,
        MATCH_TEMPLATE_NO_BRACES_SOURCE,
        lsp_types::Position::new(2, 12),
    );
    assert_eq!(
        template.kind,
        Some(lsp_types::CompletionItemKind::SNIPPET),
        "the template announces itself as one"
    );
    assert_eq!(
        template.insert_text_format,
        Some(lsp_types::InsertTextFormat::SNIPPET)
    );
    let edit = match template.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    // Absolute indentation, measured off the `match` keyword's own line —
    // see `ide::completions::match_template` for why it is not relative.
    assert_eq!(
        edit.new_text,
        "{\n        ::Circle($1) => $2,\n        ::Point => $3,\n    }"
    );

    drop(client);
}

#[test]
fn snippet_incapable_client_receives_the_match_template_fallback() {
    // The one client-visible thing that must never happen: literal `$1`
    // text pasted into a buffer by a client that cannot expand it.
    let mut client = TestClient::start_with(init_with_snippet_support(false));
    let file = uri("file:///match_template_incapable.must");

    let template = match_template_item(
        &mut client,
        &file,
        MATCH_TEMPLATE_NO_BRACES_SOURCE,
        lsp_types::Position::new(2, 12),
    );
    assert_eq!(template.insert_text_format, None);
    let edit = match template.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    assert!(
        !edit.new_text.contains('$'),
        "snippet syntax leaked to a snippet-incapable client: {:?}",
        edit.new_text
    );
    assert_eq!(
        edit.new_text,
        "{\n        ::Circle => ,\n        ::Point => ,\n    }"
    );

    drop(client);
}

/// The other shape of the template slot, end to end: `match s {}` with the
/// cursor already inside the pair an auto-closing editor supplied. `{` is
/// NOT a registered trigger character (see
/// `completion_capability_is_advertised_with_dot_and_colon_triggers`), so a
/// real editor does not fire this request spontaneously the instant `{` is
/// typed — but the position still answers the template on an explicit
/// invoke, which is exactly what this request represents.
#[test]
fn snippet_capable_client_receives_the_match_arm_template_inside_empty_braces() {
    let mut client = TestClient::start_with(init_with_snippet_support(true));
    let file = uri("file:///match_template_empty_braces.must");

    // Line 2 is `    match s {}`; column 13 sits right between the `{` and
    // the `}` an auto-closing client already inserted.
    let template = match_template_item(
        &mut client,
        &file,
        "type Shape = enum { Circle(usize), Point };\nstatic f = fn (s: Shape) {\n    match s {}\n};\n",
        lsp_types::Position::new(2, 13),
    );
    assert_eq!(
        template.insert_text_format,
        Some(lsp_types::InsertTextFormat::SNIPPET)
    );
    let edit = match template.text_edit.as_ref().expect("has a text edit") {
        lsp_types::CompletionTextEdit::Edit(edit) => edit,
        other => panic!("expected a plain edit, got {other:?}"),
    };
    // No braces of its own: the pair already in the buffer belongs to the
    // client's auto-close, and the server has no way to ask it to delete a
    // second one — so the template writes arms only, opening on its own
    // line same as the no-braces shape, and the existing `}` ends up at
    // `indent` right after the last arm.
    assert_eq!(
        edit.new_text,
        "\n        ::Circle($1) => $2,\n        ::Point => $3,\n    "
    );

    drop(client);
}

/// Dogfood: open a real example file and ask for completions at a
/// hand-picked, unremarkable spot (a fresh statement at the top of
/// `run_lights`'s body). Not a snapshot of the whole list — examples are
/// free to grow — just: several distinct completion kinds show up (an item
/// function, a type, a keyword, a builtin) and the server doesn't panic
/// answering a real file.
#[test]
fn dogfood_state_machine_example_offers_sensible_completions() {
    let mut client = TestClient::start();
    let file = uri("file:///state_machine.must");

    let text = include_str!("../../../examples/state_machine.must");
    client.open(&file, text);
    let diags = client.next_diagnostics();
    assert_eq!(
        diags.diagnostics,
        vec![],
        "the example is expected to be clean"
    );

    // Right before `let mut l = Light::Red;` in `run_lights`'s body.
    let target_line = text
        .lines()
        .position(|line| line.trim_start() == "let mut l = Light::Red;")
        .expect("fixture still has this exact statement — update the offset if it moves")
        as u32;
    let response = client.request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file.clone() },
            position: lsp_types::Position::new(target_line, 4),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    });
    let Some(lsp_types::CompletionResponse::Array(items)) = response else {
        panic!("expected a plain array of completion items, got {response:?}");
    };
    assert!(!items.is_empty(), "expected some completions, got none");

    let has = |label: &str| items.iter().any(|it| it.label == label);
    assert!(has("let"), "statement-start keyword `let` is offered");
    assert!(has("Light"), "the enum type item is offered");
    assert!(
        has("run_lights"),
        "recursion into the enclosing fn is legal"
    );
    assert!(has("describe"), "sibling fn items are offered");
    assert!(has("print"), "the builtin fn is offered");

    let kinds: std::collections::HashSet<_> =
        items.iter().map(|it| format!("{:?}", it.kind)).collect();
    assert!(
        kinds.len() >= 3,
        "expected several distinct completion kinds, got {kinds:?}"
    );

    drop(client);
}
