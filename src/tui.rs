use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
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
        }
    }

    fn push(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(ChatMessage {
            role,
            text: text.into(),
        });
    }
}

pub async fn run(agent: rig::agent::Agent, model: &str) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, agent, model).await;
    ratatui::restore();
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
        terminal.draw(|frame| draw(frame, &app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => break,
            (KeyCode::Enter, _) if !app.input.trim().is_empty() => {
                let prompt = std::mem::take(&mut app.input);
                app.push(Role::User, prompt.clone());
                app.push(Role::Assistant, String::new());
                app.status = "thinking".into();

                let mut stream = agent
                    .stream_prompt(prompt.as_str())
                    .history(history.clone())
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
                        Ok(MultiTurnStreamItem::ToolExecutionCommitted { tool_call, .. }) => {
                            app.status = format!("used {}", tool_call.function.name);
                        }
                        Ok(MultiTurnStreamItem::FinalResponse(final_response)) => {
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
                    terminal.draw(|frame| draw(frame, &app))?;
                }

                if let Some(messages) = final_messages {
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

fn draw(frame: &mut Frame, app: &App) {
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
        let (label, style) = match message.role {
            Role::User => (
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Role::Assistant => ("  ", Style::default()),
            Role::Event => ("· ", Style::default().fg(Color::DarkGray)),
        };
        lines.push(Line::from(vec![
            Span::styled(label, style),
            Span::raw(&message.text),
        ]));
        lines.push(Line::default());
    }

    let line_count = lines.len();
    let transcript = Paragraph::new(Text::from(lines))
        .block(Block::default().title(" Ratcode ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    let scroll = line_count.saturating_sub(chat.height.saturating_sub(2) as usize) as u16;
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
            Span::styled("   Esc quit", Style::default().fg(Color::DarkGray)),
        ])),
        footer,
    );
}
