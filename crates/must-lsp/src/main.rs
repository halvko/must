use lsp_server::Notification;

fn main() {
    let (conn, io_threads) = lsp_server::Connection::stdio();
    conn.sender
        .send(lsp_server::Message::Notification(Notification {
            method: "window/logMessage".into(),
            params: serde_json::json!(lsp_types::LogMessageParams {
                typ: lsp_types::MessageType::WARNING,
                message: "hello!".into(),
            }),
        }))
        .unwrap();
    let (id, result) = conn.initialize_start().unwrap();
    conn.sender
        .send(lsp_server::Message::Notification(Notification {
            method: "window/logMessage".into(),
            params: serde_json::json!(lsp_types::LogMessageParams {
                typ: lsp_types::MessageType::WARNING,
                message: serde_json::to_string(&result).unwrap(),
            }),
        }))
        .unwrap();
}
