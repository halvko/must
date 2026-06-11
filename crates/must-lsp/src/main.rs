use tracing_subscriber::EnvFilter;

fn main() -> must_lsp::ServerResult<()> {
    // stdout is the LSP channel; logs go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_env("MUST_LSP_LOG"))
        .with_ansi(false)
        .init();

    tracing::info!("must-lsp starting");
    let (connection, io_threads) = lsp_server::Connection::stdio();
    must_lsp::run(connection)?;
    io_threads.join()?;
    tracing::info!("must-lsp exiting");
    Ok(())
}
