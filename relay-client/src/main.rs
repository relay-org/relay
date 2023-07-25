use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lay::{
    crypto::KeyPair,
    text::{Post, PostRequest},
    Signed,
};
use ratatui::{
    prelude::{Backend, Constraint, CrosstermBackend, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use reqwest::Client;
use tokio::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone)]
struct Message {
    sender: String,
    content: String,
}

enum FrontendCommand {
    DisplayMessages { messages: Vec<Message> },
}

enum Mode {
    Normal,
    Input,
}

struct State {
    mode: Mode,
    input: String,
    messages: Vec<Message>,
    cursor_position: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            input: String::new(),
            messages: Vec::new(),
            cursor_position: 0,
        }
    }
}

impl State {
    fn clamp_cursor(&self, position: usize) -> usize {
        position.clamp(0, self.input.len())
    }

    fn move_cursor_left(&mut self) {
        let left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(left);
    }

    fn move_cursor_right(&mut self) {
        let right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(right);
    }

    fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        if self.cursor_position != 0 {
            self.input.remove(self.cursor_position - 1);
            self.move_cursor_left();
        }
    }

    fn reset_cursor(&mut self) {
        self.cursor_position = 0;
    }
}

fn draw_ui<B: Backend>(state: &State, frame: &mut Frame<B>) {
    let chunks = Layout::default()
        .direction(ratatui::prelude::Direction::Vertical)
        .constraints(
            [
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(frame.size());

    let (msg, style) = match state.mode {
        Mode::Normal => (
            vec![
                "Press ".into(),
                "q".bold(),
                " to exit, ".into(),
                "i".bold(),
                " to input.".bold(),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        Mode::Input => (
            vec![
                "Press ".into(),
                "Esc".bold(),
                " to stop input, ".into(),
                "Enter".bold(),
                " to send the input.".into(),
            ],
            Style::default(),
        ),
    };

    let mut text = Text::from(Line::from(msg));
    text.patch_style(style);

    let help_message = Paragraph::new(text);
    frame.render_widget(help_message, chunks[1]);

    let input = Paragraph::new(state.input.as_str())
        .style(match state.mode {
            Mode::Normal => Style::default(),
            Mode::Input => Style::default().fg(Color::LightBlue),
        })
        .block(Block::default().borders(Borders::ALL).title("input"));
    frame.render_widget(input, chunks[2]);

    match state.mode {
        Mode::Normal => {}
        Mode::Input => frame.set_cursor(
            chunks[2].x + state.cursor_position as u16 + 1,
            chunks[2].y + 1,
        ),
    }

    let messages: Vec<ListItem> = state
        .messages
        .iter()
        .map(|m| {
            let content = Line::from(Span::raw(format!("{}: {}", m.sender, m.content)));
            ListItem::new(content)
        })
        .collect();

    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
    frame.render_widget(messages, chunks[0]);
}

async fn frontend<B: Backend>(
    mut chan: (Sender<BackendCommand>, Receiver<FrontendCommand>),
    terminal: &mut Terminal<B>,
) {
    let mut state = State::default();

    'l: loop {
        // process commands
        while let Ok(cmd) = chan.1.try_recv() {
            match cmd {
                FrontendCommand::DisplayMessages { messages } => {
                    // TODO: This should append new messages.
                    state.messages = messages;
                }
            }
        }

        // check for input
        if event::poll(Duration::from_millis(1)).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                match state.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('i') => state.mode = Mode::Input,
                        KeyCode::Char('q') => {
                            chan.0.send(BackendCommand::Exit).await.unwrap();
                            break 'l;
                        }
                        _ => {}
                    },
                    Mode::Input if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Enter => {
                            chan.0
                                .send(BackendCommand::SendMessage {
                                    content: state.input.clone(),
                                })
                                .await
                                .unwrap();
                            state.input.clear();
                            state.reset_cursor();
                        }
                        KeyCode::Char(c) => state.insert_char(c),
                        KeyCode::Backspace => state.delete_char(),
                        KeyCode::Left => state.move_cursor_left(),
                        KeyCode::Right => state.move_cursor_right(),
                        KeyCode::Up => state.reset_cursor(),
                        KeyCode::Down => state.cursor_position = state.input.len(),
                        KeyCode::Esc => state.mode = Mode::Normal,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        // draw ui
        terminal.draw(|f| draw_ui(&state, f)).unwrap();
    }
}

enum BackendCommand {
    Exit,
    SendMessage { content: String },
}

async fn backend(mut chan: (Sender<FrontendCommand>, Receiver<BackendCommand>), key_pair: KeyPair) {
    let client = Client::new();

    'l: loop {
        // process commands
        while let Ok(cmd) = chan.1.try_recv() {
            match cmd {
                BackendCommand::Exit => break 'l,
                BackendCommand::SendMessage { content } => {
                    let post = Signed::new(
                        &key_pair,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        Post {
                            channel: "general".to_string(),
                            content,
                            metadata: None,
                        },
                    )
                    .unwrap();

                    client
                        .post("http://0.0.0.0:3000/text")
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_string(&post).unwrap())
                        .send()
                        .await
                        .unwrap();
                }
            }
        }

        // poll messages
        let req = Signed::new(
            &key_pair,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            PostRequest {
                channel: "general".to_string(),
                metadata: None,
            },
        )
        .unwrap();

        let resp = client
            .get("http://0.0.0.0:3000/text")
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&req).unwrap())
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        let messages: Vec<Signed<Post>> = serde_json::from_str(&resp).unwrap();

        chan.0
            .send(FrontendCommand::DisplayMessages {
                messages: messages
                    .iter()
                    .map(|m| Message {
                        sender: m.key.clone(),
                        content: m.data.content.clone(),
                    })
                    .collect(),
            })
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::main]
async fn main() {
    // load config
    let pkcs8 = KeyPair::generate_pkcs8().unwrap();
    let key_pair = KeyPair::from_pkcs8(&pkcs8).unwrap();

    // begin terminal
    enable_raw_mode().unwrap();

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();

    let terminal_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(terminal_backend).unwrap();

    // spawn task
    let (fs, fr) = mpsc::channel(32);
    let (bs, br) = mpsc::channel(32);

    let handle = tokio::spawn(async move { backend((fs, br), key_pair).await });

    frontend((bs, fr), &mut terminal).await;

    handle.await.unwrap();

    // end terminal
    disable_raw_mode().unwrap();

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();
}
