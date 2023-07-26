use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lay::{
    crypto::KeyPair,
    profile::{Profile, ProfileRequest},
    text::{Post, PostRequest},
    Signed,
};
use ratatui::{
    prelude::{Backend, Constraint, CrosstermBackend, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame, Terminal,
};
use reqwest::Client;
use tokio::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone)]
struct Message {
    sender: String,
    content: String,
}

#[derive(Clone)]
struct ProfileDisplay {
    key: String,
    name: String,
    verified: bool,
}

enum FrontendCommand {
    DisplayMessages { messages: Vec<Message> },
    RespondProfile { profile: ProfileDisplay },
}

enum Mode {
    Normal,
    Input,
    Command,
}

struct State {
    mode: Mode,
    input: String,
    messages: Vec<Message>,
    cursor_position: usize,
    autoscroll: bool,
    vertical_scroll_state: ScrollbarState,
    horizontal_scroll_state: ScrollbarState,
    vertical_scroll: usize,
    horizontal_scroll: usize,
    command_buffer: String,
    users: HashMap<String, ProfileDisplay>,
    unknown_users: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            input: String::new(),
            messages: Vec::new(),
            cursor_position: 0,
            autoscroll: true,
            vertical_scroll_state: ScrollbarState::default(),
            horizontal_scroll_state: ScrollbarState::default(),
            vertical_scroll: 0,
            horizontal_scroll: 0,
            command_buffer: String::new(),
            users: HashMap::new(),
            unknown_users: Vec::new(),
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

fn draw_ui<B: Backend>(state: &mut State, frame: &mut Frame<B>, server: &str) {
    let chunks = Layout::default()
        .direction(ratatui::prelude::Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(frame.size());

    let input_count = format!("  {}", state.input.len());
    let server = format!(" {server}");

    let (msg, style) = match state.mode {
        Mode::Normal => (
            vec![
                "NORMAL".bg(Color::DarkGray).fg(Color::White),
                server.as_str().into(),
            ],
            Style::default(),
        ),
        Mode::Input => (
            vec![
                "INPUT".bg(Color::LightBlue).fg(Color::Black),
                input_count.as_str().into(),
            ],
            Style::default(),
        ),
        Mode::Command => (
            vec![
                "COMMAND".bg(Color::Magenta).fg(Color::White),
                server.as_str().into(),
            ],
            Style::default(),
        ),
    };

    let mut text = Text::from(Line::from(msg));
    text.patch_style(style);

    let mode_message = Paragraph::new(text);
    frame.render_widget(mode_message, chunks[0]);

    let (msg, style) = match state.mode {
        Mode::Normal | Mode::Command => {
            (vec![state.command_buffer.as_str().into()], Style::default())
        }
        Mode::Input => (vec![], Style::default()),
    };

    let mut text = Text::from(Line::from(msg));
    text.patch_style(style);

    let help_message = Paragraph::new(text);
    frame.render_widget(help_message, chunks[3]);

    let input = Paragraph::new(state.input.as_str())
        .style(match state.mode {
            Mode::Normal => Style::default(),
            Mode::Input => Style::default().fg(Color::Gray),
            Mode::Command => Style::default(),
        })
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(input, chunks[2]);

    match state.mode {
        Mode::Normal => {}
        Mode::Input => frame.set_cursor(
            chunks[2].x + state.cursor_position as u16 + 1,
            chunks[2].y + 1,
        ),
        Mode::Command => {}
    }

    let messages: Vec<Line> = state
        .messages
        .iter()
        .map(|m| {
            let sender = if state.users.contains_key(&m.sender) {
                state.users[&m.sender].name.as_str()
            } else {
                state.unknown_users.push(m.sender.clone());

                "Guest"
            };

            Line::from(Span::raw(format!("{}: {}", sender, m.content)))
        })
        .collect();

    state.vertical_scroll_state = state
        .vertical_scroll_state
        .content_length(messages.len() as u16);
    state.horizontal_scroll_state = state.horizontal_scroll_state.content_length(50);

    let messages = Paragraph::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Messages"))
        .scroll((state.vertical_scroll as u16, state.horizontal_scroll as u16));

    // handle autoscroll
    if state.autoscroll && state.messages.len() > chunks[1].height as usize {
        state.vertical_scroll = state.messages.len() - chunks[1].height as usize + 2;
        state.vertical_scroll_state = state
            .vertical_scroll_state
            .position(state.vertical_scroll as u16);
    }

    frame.render_widget(messages, chunks[1]);
    frame.render_stateful_widget(
        Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
        chunks[1],
        &mut state.vertical_scroll_state,
    );
    frame.render_stateful_widget(
        Scrollbar::default().orientation(ScrollbarOrientation::HorizontalBottom),
        chunks[1],
        &mut state.horizontal_scroll_state,
    );
}

async fn frontend<B: Backend>(
    mut chan: (Sender<BackendCommand>, Receiver<FrontendCommand>),
    terminal: &mut Terminal<B>,
    server: String,
) {
    let mut state = State::default();

    'l: loop {
        let mut redraw = false;

        // process commands
        while let Ok(cmd) = chan.1.try_recv() {
            redraw = true;

            match cmd {
                FrontendCommand::DisplayMessages { messages } => {
                    // TODO: This should append new messages.
                    state.messages = messages;
                }
                FrontendCommand::RespondProfile { profile } => {
                    state.users.insert(profile.key.clone(), profile);
                }
            }
        }

        // update profile requests
        for target in state.unknown_users.drain(..) {
            chan.0
                .send(BackendCommand::RequestProfile { target })
                .await
                .unwrap();
        }

        // check for input
        if event::poll(Duration::from_millis(1)).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                redraw = true;

                match state.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('i') => {
                            state.mode = Mode::Input;
                            state.command_buffer.clear();
                        }
                        KeyCode::Char(':') => {
                            state.mode = Mode::Command;
                            state.command_buffer = ":".to_string();
                        }
                        KeyCode::Char('q') => {
                            chan.0.send(BackendCommand::Exit).await.unwrap();
                            break 'l;
                        }
                        KeyCode::Char('h') | KeyCode::Left => {
                            let offset = state.command_buffer.parse::<usize>().unwrap_or(1);
                            state.command_buffer.clear();

                            state.autoscroll = false;

                            state.horizontal_scroll =
                                state.horizontal_scroll.saturating_sub(offset);
                            state.horizontal_scroll_state = state
                                .horizontal_scroll_state
                                .position(state.horizontal_scroll as u16);
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            let offset = state.command_buffer.parse::<usize>().unwrap_or(1);
                            state.command_buffer.clear();

                            state.autoscroll = false;

                            state.vertical_scroll = state.vertical_scroll.saturating_add(offset);
                            state.vertical_scroll_state = state
                                .vertical_scroll_state
                                .position(state.vertical_scroll as u16);
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            let offset = state.command_buffer.parse::<usize>().unwrap_or(1);
                            state.command_buffer.clear();

                            state.autoscroll = false;

                            state.vertical_scroll = state.vertical_scroll.saturating_sub(offset);
                            state.vertical_scroll_state = state
                                .vertical_scroll_state
                                .position(state.vertical_scroll as u16);
                        }
                        KeyCode::Char('l') | KeyCode::Right => {
                            let offset = state.command_buffer.parse::<usize>().unwrap_or(1);
                            state.command_buffer.clear();

                            state.autoscroll = false;

                            state.horizontal_scroll =
                                state.horizontal_scroll.saturating_add(offset);
                            state.horizontal_scroll_state = state
                                .horizontal_scroll_state
                                .position(state.horizontal_scroll as u16);
                        }
                        KeyCode::Char('s') => state.autoscroll = true,
                        KeyCode::Char(c) if c.is_digit(10) => state.command_buffer.push(c),
                        _ => {}
                    },
                    Mode::Input if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Enter if state.input.len() > 0 => {
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
                    Mode::Command => match key.code {
                        KeyCode::Enter => {
                            // TODO: Proper command API.
                            let args: Vec<&str> = state.command_buffer.split_whitespace().collect();

                            match args[0] {
                                ":profile" if args.len() == 2 => {
                                    chan.0
                                        .send(BackendCommand::SendProfile {
                                            name: args[1].to_string(),
                                        })
                                        .await
                                        .unwrap();
                                }
                                ":refresh-profiles" if args.len() == 1 => state.users.clear(),
                                _ => {}
                            }

                            state.command_buffer.clear();
                            state.mode = Mode::Normal;
                        }
                        KeyCode::Backspace => {
                            state.command_buffer.pop();
                        }
                        KeyCode::Char(c) => state.command_buffer.push(c),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        // draw ui
        if redraw {
            terminal.draw(|f| draw_ui(&mut state, f, &server)).unwrap();
        }
    }
}

enum BackendCommand {
    Exit,
    SendMessage { content: String },
    SendProfile { name: String },
    RequestProfile { target: String },
}

async fn backend(
    mut chan: (Sender<FrontendCommand>, Receiver<BackendCommand>),
    key_pair: KeyPair,
    server: String,
) {
    let client = Client::new();
    let text_url = format!("{server}/text");
    let profile_url = format!("{server}/profile");

    'l: loop {
        // process commands
        while let Ok(cmd) = chan.1.try_recv() {
            match cmd {
                BackendCommand::Exit => break 'l,
                BackendCommand::SendMessage { content } => {
                    let post = Signed::new(
                        &key_pair,
                        server.clone(),
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
                        .post(&text_url)
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_string(&post).unwrap())
                        .send()
                        .await
                        .unwrap();
                }
                BackendCommand::SendProfile { name } => {
                    let profile = Signed::new(
                        &key_pair,
                        server.clone(),
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        Profile {
                            name,
                            metadata: None,
                        },
                    )
                    .unwrap();

                    client
                        .post(&profile_url)
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_string(&profile).unwrap())
                        .send()
                        .await
                        .unwrap();
                }
                BackendCommand::RequestProfile { target } => {
                    let req = Signed::new(
                        &key_pair,
                        server.clone(),
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        ProfileRequest {
                            target_key: target.clone(),
                        },
                    )
                    .unwrap();

                    let res = client
                        .get(&profile_url)
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_string(&req).unwrap())
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap();

                    if let Ok(profile) = serde_json::from_str::<Signed<Profile>>(&res) {
                        let verified = profile.verify();

                        chan.0
                            .send(FrontendCommand::RespondProfile {
                                profile: ProfileDisplay {
                                    key: profile.key,
                                    name: profile.data.name,
                                    verified,
                                },
                            })
                            .await
                            .unwrap();
                    } else {
                        // TODO: This is stupid.
                        chan.0
                            .send(FrontendCommand::RespondProfile {
                                profile: ProfileDisplay {
                                    key: target.clone(),
                                    name: "Guest".to_string(),
                                    verified: false,
                                },
                            })
                            .await
                            .unwrap();
                    }
                }
            }
        }

        // poll messages
        let req = Signed::new(
            &key_pair,
            server.clone(),
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
            .get(&text_url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&req).unwrap())
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        let messages: Vec<Signed<Post>> = serde_json::from_str(&resp).unwrap();

        if chan
            .0
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
            .is_err()
        {
            break;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::main]
async fn main() {
    // load config
    let server = std::env::var("IP").unwrap_or("http://0.0.0.0:3000".to_string());

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

    let backend_server = server.clone();

    let handle = tokio::spawn(async move { backend((fs, br), key_pair, backend_server).await });

    frontend((bs, fr), &mut terminal, server).await;

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
