use zed_extension_api::{
    self as zed, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario,
    DebugTaskDefinition, StartDebuggingRequestArguments, StartDebuggingRequestArgumentsRequest,
    Worktree,
};

struct MustExtension;

/// `must-lsp` on PATH (e.g. `cargo install --path crates/must-lsp`) wins;
/// otherwise assume the worktree is this repo and use its build output, so
/// dogfooding works from a plain clone + `cargo build`.
fn server_path(worktree: &Worktree) -> String {
    worktree
        .which("must-lsp")
        .unwrap_or_else(|| format!("{}/target/debug/must-lsp", worktree.root_path()))
}

impl zed::Extension for MustExtension {
    fn new() -> Self {
        MustExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<zed::Command> {
        Ok(zed::Command {
            command: server_path(worktree),
            args: Vec::new(),
            // Shell env so MUST_LSP_LOG reaches the server.
            env: worktree.shell_env(),
        })
    }

    /// The debug adapter is the same binary in `dap` mode, over stdio
    /// (`connection: None`).
    fn get_dap_binary(
        &mut self,
        _adapter_name: String,
        config: DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &Worktree,
    ) -> Result<DebugAdapterBinary, String> {
        let command = user_provided_debug_adapter_path.unwrap_or_else(|| server_path(worktree));
        // Validate the config is JSON and resolve a relative `program` path
        // against the worktree.
        let mut configuration: serde_json::Value = serde_json::from_str(&config.config)
            .map_err(|err| format!("debug configuration is not valid JSON: {err}"))?;
        if let Some(object) = configuration.as_object_mut() {
            if let Some(program) = object.get("program").and_then(|p| p.as_str()) {
                if !program.starts_with('/') {
                    let absolute = format!("{}/{program}", worktree.root_path());
                    object.insert("program".to_owned(), absolute.into());
                }
            }
        }
        Ok(DebugAdapterBinary {
            command: Some(command),
            arguments: vec!["dap".to_owned()],
            envs: worktree.shell_env(),
            cwd: Some(worktree.root_path()),
            connection: None,
            request_args: StartDebuggingRequestArguments {
                configuration: configuration.to_string(),
                request: StartDebuggingRequestArgumentsRequest::Launch,
            },
        })
    }

    fn dap_request_kind(
        &mut self,
        _adapter_name: String,
        _config: serde_json::Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        // The adapter *is* the runtime: there is never a process to attach
        // to, so every session is a launch.
        Ok(StartDebuggingRequestArgumentsRequest::Launch)
    }

    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario, String> {
        match config.request {
            DebugRequest::Launch(launch) => {
                let scenario_config = serde_json::json!({
                    "request": "launch",
                    "program": launch.program,
                });
                Ok(DebugScenario {
                    adapter: config.adapter,
                    label: config.label,
                    config: scenario_config.to_string(),
                    tcp_connection: None,
                    build: None,
                })
            }
            DebugRequest::Attach(_) => {
                Err("Must programs run in the adapter itself; there is nothing to attach to"
                    .to_owned())
            }
        }
    }
}

zed::register_extension!(MustExtension);
