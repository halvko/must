use zed_extension_api as zed;

struct MustExtension;

const DEV_SERVER_PATH: &str =
    "/Users/erikfc/code/github.com/halvko/must-lsp-server/target/debug/must-lsp";

impl zed::Extension for MustExtension {
    fn new() -> Self {
        MustExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let command = worktree
            .which("must-lsp")
            .unwrap_or_else(|| DEV_SERVER_PATH.to_owned());
        Ok(zed::Command {
            command,
            args: Vec::new(),
            // Shell env so MUST_LSP_LOG reaches the server.
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(MustExtension);
