use zed_extension_api as zed;

struct MustExtension;

impl zed::Extension for MustExtension {
    fn new() -> Self {
        MustExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        // A `must-lsp` on PATH (e.g. `cargo install --path crates/must-lsp`)
        // wins; otherwise assume the worktree is this repo and use its build
        // output, so dogfooding works from a plain clone + `cargo build`.
        let command = worktree.which("must-lsp").unwrap_or_else(|| {
            format!("{}/target/debug/must-lsp", worktree.root_path())
        });
        Ok(zed::Command {
            command,
            args: Vec::new(),
            // Shell env so MUST_LSP_LOG reaches the server.
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(MustExtension);
