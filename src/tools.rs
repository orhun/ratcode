use rig::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::json;

const MAX_OUTPUT: usize = 20_000;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolFailure(String);

fn truncate(mut text: String) -> String {
    if text.len() > MAX_OUTPUT {
        text.truncate(MAX_OUTPUT);
        while !text.is_char_boundary(text.len()) {
            text.pop();
        }
        text.push_str("\n… output truncated");
    }
    text
}

#[derive(Deserialize, Serialize)]
pub struct ReadFile;

#[derive(Deserialize)]
pub struct ReadFileArgs {
    path: String,
}

// TODO: Rig - Tool trait with typed arguments, output, error, and JSON schema.
impl Tool for ReadFile {
    const NAME: &'static str = "read_file";
    type Error = ToolFailure;
    type Args = ReadFileArgs;
    type Output = String;

    fn description(&self) -> String {
        "Read a UTF-8 file from the current project".into()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        })
    }

    async fn call(
        &self,
        // TODO: Rig - ToolContext is provided to every tool invocation.
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<String, ToolFailure> {
        tokio::fs::read_to_string(&args.path)
            .await
            .map(truncate)
            .map_err(|error| ToolFailure(format!("failed to read {}: {error}", args.path)))
    }
}

#[derive(Deserialize, Serialize)]
pub struct WriteFile;

#[derive(Deserialize)]
pub struct WriteFileArgs {
    path: String,
    content: String,
}

impl Tool for WriteFile {
    const NAME: &'static str = "write_file";
    type Error = ToolFailure;
    type Args = WriteFileArgs;
    type Output = String;

    fn description(&self) -> String {
        "Create or replace a UTF-8 file in the current project".into()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<String, ToolFailure> {
        tokio::fs::write(&args.path, args.content)
            .await
            .map(|_| format!("wrote {}", args.path))
            .map_err(|error| ToolFailure(format!("failed to write {}: {error}", args.path)))
    }
}

#[derive(Deserialize, Serialize)]
pub struct RunShell;

#[derive(Deserialize)]
pub struct RunShellArgs {
    command: String,
}

impl Tool for RunShell {
    const NAME: &'static str = "run_shell";
    type Error = ToolFailure;
    type Args = RunShellArgs;
    type Output = String;

    fn description(&self) -> String {
        "Run a shell command in the current project and return stdout and stderr".into()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<String, ToolFailure> {
        let output = tokio::process::Command::new("bash")
            .args(["-lc", &args.command])
            .output()
            .await
            .map_err(|error| ToolFailure(format!("failed to run command: {error}")))?;

        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            text.push_str("\nstderr:\n");
            text.push_str(&stderr);
        }
        if !output.status.success() {
            text.push_str(&format!("\nexit status: {}", output.status));
        }
        Ok(truncate(text))
    }
}
