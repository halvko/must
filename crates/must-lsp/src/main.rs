use tracing_subscriber::EnvFilter;

fn main() -> must_lsp::ServerResult<()> {
    // Top-level dispatch lives in `must_lsp::cli` so it can be tested
    // without spawning a process; this carries out the verdict. `run`'s
    // own `-e`/file parsing stays below in `run_command`, unlike the rest
    // of the surface — it is the exception to "the whole surface is in
    // `cli`", not yet moved over.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match must_lsp::cli::parse(&args) {
        must_lsp::cli::Command::Run(rest) => std::process::exit(run_command(rest)),
        must_lsp::cli::Command::Check(rest) => std::process::exit(must_lsp::check::check(rest)),
        must_lsp::cli::Command::Dap => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            must_lsp::dap::run(stdin.lock(), stdout.lock())?;
            return Ok(());
        }
        must_lsp::cli::Command::Message { text, code } => {
            if code == 0 {
                print!("{text}");
            } else {
                eprint!("{text}");
            }
            std::process::exit(code);
        }
        must_lsp::cli::Command::Serve => {}
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
                eprintln!("try `must-lsp --help`");
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
