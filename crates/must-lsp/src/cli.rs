//! Top-level argument dispatch for the `must-lsp` binary.
//!
//! Deliberately hand-rolled over `std`: the surface is four subcommands and
//! two flags, and a CLI dependency would buy nothing a `match` doesn't.
//! Parsing is separated from acting so it can be tested without spawning a
//! process — [`parse`] is total, and every rejection names `--help`.

/// What a top-level argument list asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    /// No arguments: speak LSP over stdio.
    Serve,
    /// `run` and its own arguments (the file, `-e`).
    Run(&'a [String]),
    /// `check` and the files to check.
    Check(&'a [String]),
    /// `dap`: speak the Debug Adapter Protocol over stdio.
    Dap,
    /// Print `text` and exit with `code` — `--help`/`--version` (0, stdout)
    /// and usage errors (2, stderr).
    Message { text: String, code: i32 },
}

pub const HELP: &str = "\
must-lsp — the language server and toolchain for Must

USAGE:
    must-lsp                              Serve LSP over stdio (the default)
    must-lsp run <file.must> [-e <expr>]  Evaluate an expression in a file's scope
    must-lsp check <file.must>...         Check files, printing diagnostics as text
    must-lsp dap                          Speak the Debug Adapter Protocol over stdio

OPTIONS:
    -h, --help                            Print this help and exit
    -V, --version                         Print the version and exit

RUN OPTIONS:
    -e, --entry <expr>                    Expression to evaluate; defaults to `main()`.
                                          It is evaluated in the file's item scope, so it
                                          can name any static in the file.

ENVIRONMENT:
    MUST_LSP_LOG                          Log filter for the server's stderr log; unset
                                          means silent. Examples: `info`, `must_lsp=debug`.

EXIT STATUS:
    0  success
    1  the program failed, or a checked file had errors
    2  bad usage, or a file could not be read
";

/// Classify `args` (the arguments after the program name). Total: every
/// input yields either something to do or a message to print.
pub fn parse(args: &[String]) -> Command<'_> {
    let Some(first) = args.first().map(String::as_str) else {
        return Command::Serve;
    };
    match first {
        "run" => Command::Run(&args[1..]),
        "check" => Command::Check(&args[1..]),
        "dap" => Command::Dap,
        "-h" | "--help" | "help" => Command::Message {
            text: HELP.to_owned(),
            code: 0,
        },
        "-V" | "--version" => Command::Message {
            text: format!("must-lsp {}\n", env!("CARGO_PKG_VERSION")),
            code: 0,
        },
        // Anything else would previously have started the language server
        // by accident — a typo'd subcommand silently hanging on stdin is
        // the worst possible answer.
        other => {
            let what = if other.starts_with('-') {
                "unknown option"
            } else {
                "unknown subcommand"
            };
            Command::Message {
                text: format!("error: {what} `{other}`\ntry `must-lsp --help`\n"),
                code: 2,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_owned(args: &[&str]) -> Command<'static> {
        // Leak so the borrow can outlive the call in these tiny tests.
        let owned: &'static [String] = Box::leak(
            args.iter()
                .map(|a| (*a).to_owned())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        parse(owned)
    }

    #[test]
    fn no_arguments_serves_lsp() {
        assert_eq!(parse_owned(&[]), Command::Serve);
    }

    #[test]
    fn help_exits_zero_and_documents_every_subcommand() {
        let Command::Message { text, code } = parse_owned(&["--help"]) else {
            panic!("--help prints a message");
        };
        assert_eq!(code, 0);
        for expected in [
            "must-lsp run <file.must>",
            "must-lsp check <file.must>...",
            "must-lsp dap",
            "MUST_LSP_LOG",
            "-e, --entry",
        ] {
            assert!(
                text.contains(expected),
                "help must mention {expected}:\n{text}"
            );
        }
        assert_eq!(parse_owned(&["-h"]), parse_owned(&["--help"]));
    }

    #[test]
    fn version_exits_zero() {
        let Command::Message { text, code } = parse_owned(&["--version"]) else {
            panic!("--version prints a message");
        };
        assert_eq!(code, 0);
        assert!(text.starts_with("must-lsp "), "{text}");
    }

    #[test]
    fn unknown_subcommand_exits_two_and_points_at_help() {
        let Command::Message { text, code } = parse_owned(&["frobnicate"]) else {
            panic!("an unknown subcommand is a usage error");
        };
        assert_eq!(code, 2);
        assert_eq!(
            text,
            "error: unknown subcommand `frobnicate`\ntry `must-lsp --help`\n"
        );
    }

    #[test]
    fn unknown_flag_exits_two_and_points_at_help() {
        let Command::Message { text, code } = parse_owned(&["--serve-harder"]) else {
            panic!("an unknown flag is a usage error");
        };
        assert_eq!(code, 2);
        assert_eq!(
            text,
            "error: unknown option `--serve-harder`\ntry `must-lsp --help`\n"
        );
    }

    #[test]
    fn subcommands_pass_their_own_arguments_through() {
        assert!(matches!(
            parse_owned(&["run", "a.must", "-e", "main()"]),
            Command::Run(rest) if rest.len() == 3
        ));
        assert!(matches!(
            parse_owned(&["check", "a.must", "b.must"]),
            Command::Check(rest) if rest.len() == 2
        ));
        assert_eq!(parse_owned(&["dap"]), Command::Dap);
    }
}
