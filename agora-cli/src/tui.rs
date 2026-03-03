use std::collections::BTreeMap;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::client::AgoraClient;

struct TuiState {
    rooms: BTreeMap<String, RoomInfo>,
    active_room: Option<String>,
    input: String,
    messages: Vec<DisplayMessage>,
    should_quit: bool,
    status: String,
    scroll_offset: u16,
    follow_tail: bool,
    msg_area_height: u16,
}

struct RoomInfo {
    name: String,
}

struct DisplayMessage {
    sender: String,
    body: String,
    event_type: String,
}

pub async fn run_tui(client: &mut AgoraClient) -> Result<(), Box<dyn std::error::Error>> {
    if client.token().is_none() {
        return Err("not logged in — use `agora login` first".into());
    }

    // Initial sync to get room list.
    let initial = client.sync(None, 0).await?;
    let mut since = initial.next_batch.clone();

    let mut state = TuiState {
        rooms: BTreeMap::new(),
        active_room: None,
        input: String::new(),
        messages: Vec::new(),
        should_quit: false,
        status: "connected  |  PgUp/PgDn to scroll  |  Tab switch room  |  Esc quit".to_owned(),
        scroll_offset: 0,
        follow_tail: true,
        msg_area_height: 0,
    };

    for (room_id, room) in &initial.rooms.join {
        let name = room
            .state
            .events
            .iter()
            .find(|e| e.event_type == "m.room.name")
            .and_then(|e| e.content.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(room_id)
            .to_owned();
        state.rooms.insert(room_id.clone(), RoomInfo { name });
    }

    if let Some(first) = state.rooms.keys().next().cloned() {
        select_room(&mut state, client, &first).await?;
    }

    // Terminal setup.
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    loop {
        terminal.draw(|f| draw_ui(f, &mut state))?;

        // Poll for keyboard input with a short timeout so we can sync.
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.should_quit = true;
                    }
                    KeyCode::Esc => {
                        state.should_quit = true;
                    }
                    KeyCode::Enter => {
                        if !state.input.is_empty() {
                            if let Some(room_id) = &state.active_room {
                                let input = state.input.drain(..).collect::<String>();
                                if input.starts_with("/join ") {
                                    let target = input.strip_prefix("/join ").unwrap().trim();
                                    match client.join_room(target).await {
                                        Ok(resp) => {
                                            state.status = format!("joined {}", resp.room_id);
                                            select_room(&mut state, client, resp.room_id.as_str()).await?;
                                        }
                                        Err(e) => state.status = format!("join error: {e}"),
                                    }
                                } else if input.starts_with("/create ") {
                                    let name = input.strip_prefix("/create ").unwrap().trim();
                                    match client.create_room(Some(name), None).await {
                                        Ok(resp) => {
                                            let rid = resp.room_id.as_str().to_owned();
                                            state.rooms.insert(
                                                rid.clone(),
                                                RoomInfo { name: name.to_owned() },
                                            );
                                            state.status = format!("created {rid}");
                                            select_room(&mut state, client, &rid).await?;
                                        }
                                        Err(e) => state.status = format!("create error: {e}"),
                                    }
                                } else {
                                    match client.send_message(&room_id.clone(), &input).await {
                                        Ok(_) => {}
                                        Err(e) => state.status = format!("send error: {e}"),
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        state.input.pop();
                    }
                    KeyCode::Tab => {
                        // Cycle through rooms.
                        let keys: Vec<_> = state.rooms.keys().cloned().collect();
                        if let Some(current) = &state.active_room {
                            let idx = keys.iter().position(|k| k == current).unwrap_or(0);
                            let next = &keys[(idx + 1) % keys.len()];
                            let next = next.clone();
                            select_room(&mut state, client, &next).await?;
                        }
                    }
                    KeyCode::PageUp => {
                        let step = state.msg_area_height.saturating_sub(2).max(1);
                        state.scroll_offset = state.scroll_offset.saturating_add(step);
                        state.follow_tail = false;
                    }
                    KeyCode::PageDown => {
                        let step = state.msg_area_height.saturating_sub(2).max(1);
                        state.scroll_offset = state.scroll_offset.saturating_sub(step);
                        if state.scroll_offset == 0 {
                            state.follow_tail = true;
                        }
                    }
                    KeyCode::Up if state.input.is_empty() => {
                        state.scroll_offset = state.scroll_offset.saturating_add(3);
                        state.follow_tail = false;
                    }
                    KeyCode::Down if state.input.is_empty() => {
                        state.scroll_offset = state.scroll_offset.saturating_sub(3);
                        if state.scroll_offset == 0 {
                            state.follow_tail = true;
                        }
                    }
                    KeyCode::End => {
                        state.scroll_offset = 0;
                        state.follow_tail = true;
                    }
                    KeyCode::Home => {
                        state.follow_tail = false;
                    }
                    KeyCode::Char(c) => {
                        state.input.push(c);
                    }
                    _ => {}
                }
            }
        }

        if state.should_quit {
            break;
        }

        // Background sync for new messages.
        match client.sync(Some(&since), 0).await {
            Ok(resp) => {
                since = resp.next_batch;
                for (room_id, room) in &resp.rooms.join {
                    if !state.rooms.contains_key(room_id) {
                        let name = room
                            .state
                            .events
                            .iter()
                            .find(|e| e.event_type == "m.room.name")
                            .and_then(|e| e.content.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(room_id)
                            .to_owned();
                        state.rooms.insert(room_id.clone(), RoomInfo { name });
                    }
                    if Some(room_id) == state.active_room.as_ref() {
                        for event in &room.timeline.events {
                            state.messages.push(DisplayMessage {
                                sender: event.sender.localpart().to_owned(),
                                body: event
                                    .content
                                    .get("body")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&format!("[{}]", event.event_type))
                                    .to_owned(),
                                event_type: event.event_type.clone(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                state.status = format!("sync error: {e}");
            }
        }
    }

    // Cleanup.
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}

async fn select_room(
    state: &mut TuiState,
    client: &AgoraClient,
    room_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    state.active_room = Some(room_id.to_owned());
    state.messages.clear();
    state.scroll_offset = 0;
    state.follow_tail = true;

    let resp = client.get_messages(room_id, 50).await?;
    for event in resp.chunk.iter().rev() {
        state.messages.push(DisplayMessage {
            sender: event.sender.localpart().to_owned(),
            body: event
                .content
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("[{}]", event.event_type))
                .to_owned(),
            event_type: event.event_type.clone(),
        });
    }

    if let Some(info) = state.rooms.get(room_id) {
        state.status = format!("room: {}", info.name);
    }

    Ok(())
}

fn draw_ui(f: &mut Frame, state: &mut TuiState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // status bar
            Constraint::Min(5),    // main area
            Constraint::Length(3), // input
        ])
        .split(f.area());

    // Status bar.
    let status = Paragraph::new(format!(" agora  |  {} ", state.status))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status, outer[0]);

    // Main area: room list + messages.
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(30)])
        .split(outer[1]);

    // Room list.
    let items: Vec<ListItem> = state
        .rooms
        .iter()
        .map(|(id, info)| {
            let is_active = state.active_room.as_deref() == Some(id);
            let style = if is_active {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default()
            };
            let prefix = if is_active { "> " } else { "  " };
            ListItem::new(format!("{prefix}{}", info.name)).style(style)
        })
        .collect();

    let room_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" rooms "));
    f.render_widget(room_list, main[0]);

    // Messages.
    let msg_area = main[1];
    let inner_height = msg_area.height.saturating_sub(2); // subtract borders
    let inner_width = msg_area.width.saturating_sub(2) as usize;
    state.msg_area_height = inner_height;

    let msg_lines: Vec<Line> = state
        .messages
        .iter()
        .map(|m| {
            let sender_style = Style::default().fg(Color::Green).bold();
            let body_style = if m.event_type.starts_with("agora.") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(format!("<{}> ", m.sender), sender_style),
                Span::styled(&m.body, body_style),
            ])
        })
        .collect();

    // Estimate total wrapped lines for scroll calculation.
    let total_wrapped: u16 = msg_lines
        .iter()
        .map(|line| {
            if inner_width == 0 {
                return 1u16;
            }
            let line_len: usize = line.spans.iter().map(|s| s.content.len()).sum();
            ((line_len as f64 / inner_width as f64).ceil() as u16).max(1)
        })
        .sum();

    // Auto-scroll to bottom when following tail.
    let scroll_pos = if state.follow_tail {
        total_wrapped.saturating_sub(inner_height)
    } else {
        // scroll_offset is "lines from the bottom", convert to "lines from the top"
        let max_scroll = total_wrapped.saturating_sub(inner_height);
        max_scroll.saturating_sub(state.scroll_offset)
    };

    let messages = Paragraph::new(msg_lines)
        .block(Block::default().borders(Borders::ALL).title(" messages "))
        .wrap(Wrap { trim: false })
        .scroll((scroll_pos, 0));
    f.render_widget(messages, msg_area);

    // Input.
    let input = Paragraph::new(state.input.as_str())
        .block(Block::default().borders(Borders::ALL).title(" send "));
    f.render_widget(input, outer[2]);
}
