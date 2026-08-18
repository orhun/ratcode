mod tools;
mod tui;

// Inspired by <https://dev.to/joshmo_dev/rewriting-claude-code-in-rust-jbm>

use anyhow::Result;
use rig::{prelude::*, providers::openai};
use tools::{ReadFile, RunShell, WriteFile};

const SYSTEM_PROMPT: &str = r#"You are Ratcode, a small coding agent running in a terminal.
Use your tools to inspect and change the project in the current working directory.
Read files before changing them, keep changes focused, run relevant checks, and answer concisely."#;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| openai::GPT_4O_MINI.into());

    // TODO: Rig - OpenAI provider using the Chat Completions API.
    let agent = openai::CompletionsClient::from_env()?
        .agent(&model)
        .preamble(SYSTEM_PROMPT)
        // TODO: Rig - Agent builder registers tools for automatic tool calling.
        .tool(ReadFile)
        .tool(WriteFile)
        .tool(RunShell)
        .max_tokens(8_192)
        .build();

    tui::run(agent, &model).await
}
