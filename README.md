# Ratcode

A deliberately small Codex-style terminal coding agent built with Rig and
Ratatui.

It demonstrates four things:

- Rig's agent builder and OpenAI provider
- typed Tool implementations for reading, writing, and shell commands
- Rig's automatic multi-turn tool loop
- streaming text, tool events, conversation history, and token usage

## Run

Create a .env file, then run cargo run:

    OPENAI_API_KEY=sk-...
    OPENAI_MODEL=gpt-4.1-mini

OPENAI_MODEL is optional and defaults to gpt-4o-mini.

Type a prompt and press Enter. Press Escape or Ctrl-C to quit. The agent works
in the directory from which it is launched and its tools can modify files and
run shell commands there.

## Livestream discussion starters

- When should an app keep Vec<Message> itself versus using Rig memory?
- Should tool progress be observed through stream items or agent hooks?
- How would approval-before-write fit into the automatic multi-turn loop?
- What is the intended cancellation story for a streaming model call or a
  long-running tool?
- How portable are tool definitions and stream events across providers?
