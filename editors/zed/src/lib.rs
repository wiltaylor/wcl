use zed::LanguageServerId;
use zed_extension_api::{self as zed, Result};

struct WclExtension;

impl zed::Extension for WclExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let path = worktree
            .which("wcl")
            .ok_or_else(|| "wcl is not installed. Install it with: cargo install wcl".to_string())?;

        Ok(zed::Command {
            command: path,
            args: vec!["lsp".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(WclExtension);
