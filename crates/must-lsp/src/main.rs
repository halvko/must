use tracing_subscriber::EnvFilter;

fn main() -> must_lsp::ServerResult<()> {
    // `must-lsp run file.must [-e EXPR]` evaluates instead of serving;
    // `must-lsp dap` speaks the Debug Adapter Protocol over stdio.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("run") {
        std::process::exit(run_command(&args[1..]));
    }
    if args.first().map(String::as_str) == Some("dap") {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        must_lsp::dap::run(stdin.lock(), stdout.lock())?;
        return Ok(());
    }

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

fn run_command(args: &[String]) -> i32 {
    let mut file = None;
    let mut expr = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-e" | "--entry" => match args.next() {
                Some(e) => expr = Some(e.clone()),
                None => {
                    eprintln!("error: {arg} needs an expression");
                    return 2;
                }
            },
            _ if file.is_none() => file = Some(arg.clone()),
            _ => {
                eprintln!("error: unexpected argument `{arg}`");
                return 2;
            }
        }
    }
    let Some(file) = file else {
        eprintln!("usage: must-lsp run <file.must> [-e <expression>]");
        return 2;
    };
    must_lsp::runner::run(&file, expr.as_deref().unwrap_or("main()"))
}
