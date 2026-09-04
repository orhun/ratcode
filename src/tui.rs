use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::Stylize,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use rig::{
    agent::MultiTurnStreamItem,
    message::Message,
    streaming::{StreamedAssistantContent, StreamingPrompt},
};

#[derive(Clone, Copy)]
enum Role {
    User,
    Assistant,
    Event,
}

struct ChatMessage {
    role: Role,
    text: String,
}

struct App {
    input: String,
    messages: Vec<ChatMessage>,
    status: String,
    usage: String,
    scroll_from_bottom: u16,
    max_scroll: u16,
    page_size: u16,
}

impl App {
    fn new(model: &str) -> Self {
        Self {
            input: String::new(),
            messages: vec![ChatMessage {
                role: Role::Event,
                text: "Ask me to inspect, explain, or edit this project.".into(),
            }],
            status: format!("ready · {model}"),
            usage: String::new(),
            scroll_from_bottom: 0,
            max_scroll: 0,
            page_size: 1,
        }
    }

    fn push(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(ChatMessage {
            role,
            text: text.into(),
        });
    }

    fn scroll_up(&mut self, amount: u16) {
        self.scroll_from_bottom = self
            .scroll_from_bottom
            .saturating_add(amount)
            .min(self.max_scroll);
    }

    fn scroll_down(&mut self, amount: u16) {
        self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(amount);
    }

    fn scroll_to_top(&mut self) {
        self.scroll_from_bottom = self.max_scroll;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_from_bottom = 0;
    }
}

pub async fn run(agent: rig::agent::Agent, model: &str) -> Result<()> {
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(error.into());
    }

    let result = run_loop(&mut terminal, agent, model).await;
    let mouse_result = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();

    mouse_result?;
    result
}

async fn run_loop(
    terminal: &mut DefaultTerminal,
    agent: rig::agent::Agent,
    model: &str,
) -> Result<()> {
    let mut app = App::new(model);
    let mut history: Vec<Message> = Vec::new();

    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;

        let event = event::read()?;
        if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::ScrollUp => app.scroll_up(3),
                MouseEventKind::ScrollDown => app.scroll_down(3),
                _ => {}
            }
            continue;
        }
        let Event::Key(key) = event else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => break,
            (KeyCode::Up, _) => app.scroll_up(1),
            (KeyCode::Down, _) => app.scroll_down(1),
            (KeyCode::PageUp, _) => app.scroll_up(app.page_size),
            (KeyCode::PageDown, _) => app.scroll_down(app.page_size),
            (KeyCode::Home, _) => app.scroll_to_top(),
            (KeyCode::End, _) => app.scroll_to_bottom(),
            (KeyCode::Enter, _) if !app.input.trim().is_empty() => {
                app.scroll_to_bottom();
                let prompt = std::mem::take(&mut app.input);
                app.push(Role::User, prompt.clone());
                app.push(Role::Assistant, String::new());
                app.status = "thinking".into();

                // TODO: Rig - Manual conversation history supplied to each prompt.
                let mut stream = agent
                    .stream_prompt(prompt.as_str())
                    .history(history.clone())
                    // TODO: Rig - Automatic multi-turn tool execution, limited to 20 turns.
                    .max_turns(20)
                    .await;
                let mut response = String::new();
                let mut final_messages = None;

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::Text(text),
                        )) => {
                            response.push_str(&text.text);
                            if let Some(last) = app.messages.last_mut() {
                                last.text = response.clone();
                            }
                        }
                        Ok(MultiTurnStreamItem::StreamAssistantItem(
                            StreamedAssistantContent::ToolCall { tool_call, .. },
                        )) => app.status = format!("calling {}", tool_call.function.name),
                        // TODO: Rig - Streaming event emitted after tool execution is committed.
                        Ok(MultiTurnStreamItem::ToolExecutionCommitted { tool_call, .. }) => {
                            app.status = format!("used {}", tool_call.function.name);
                        }
                        Ok(MultiTurnStreamItem::FinalResponse(final_response)) => {
                            // TODO: Rig - Final response provides token usage and generated messages.
                            let usage = final_response.usage();
                            app.usage =
                                format!("{} in · {} out", usage.input_tokens, usage.output_tokens);
                            final_messages = final_response.messages().map(|m| m.to_vec());
                        }
                        Err(error) => {
                            app.push(Role::Event, format!("error: {error}"));
                            break;
                        }
                        _ => {}
                    }
                    terminal.draw(|frame| draw(frame, &mut app))?;
                }

                if let Some(messages) = final_messages {
                    // TODO: Rig - Persist Rig-generated messages for the next prompt.
                    history.extend(messages);
                } else {
                    history.push(Message::user(prompt));
                    history.push(Message::assistant(response));
                }
                app.status = "ready".into();
            }
            (KeyCode::Backspace, _) => {
                app.input.pop();
            }
            (KeyCode::Char(character), _) => app.input.push(character),
            _ => {}
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, app: &mut App) {
    let [chat, input, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(frame.area());

    let mut lines = Vec::new();
    for message in &app.messages {
        let label = match message.role {
            Role::User => "> ".cyan().bold(),
            Role::Assistant => "  ".into(),
            Role::Event => "· ".dark_gray(),
        };
        lines.push(Line::from(vec![label, Span::raw(&message.text)]));
        lines.push(Line::default());
    }

    let transcript = Paragraph::new(Text::from(lines))
        .block(Block::default().title(" Ratcode ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    let viewport_height = chat.height.saturating_sub(2);
    let wrapped_line_count = transcript.line_count(chat.width);
    app.max_scroll = wrapped_line_count
        .saturating_sub(viewport_height as usize)
        .min(u16::MAX as usize) as u16;
    app.page_size = viewport_height.max(1);
    app.scroll_from_bottom = app.scroll_from_bottom.min(app.max_scroll);
    let scroll = app.max_scroll.saturating_sub(app.scroll_from_bottom);
    frame.render_widget(transcript.scroll((scroll, 0)), chat);

    frame.render_widget(
        Paragraph::new(app.input.as_str())
            .block(Block::default().title(" Prompt ").borders(Borders::ALL)),
        input,
    );
    frame.set_cursor_position((input.x + app.input.chars().count() as u16 + 1, input.y + 1));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(format!(" {}", app.status)),
            Span::raw(if app.usage.is_empty() {
                "".into()
            } else {
                format!(" · {}", app.usage)
            }),
            "   ↑/↓ scroll · PgUp/PgDn · Esc quit".dark_gray(),
        ])),
        footer,
    );
}
