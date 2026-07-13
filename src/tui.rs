use crate::agent::provider::ProviderEnum;
use crate::agent::session::{Session, SessionMessage, SessionRepository};
use crate::renderer::render_markdown;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};
use std::io::{self, Stdout};

pub(crate) fn run(repository: &SessionRepository<'_>) -> Result<()> {
    let mut sessions = repository.list_sessions()?;
    sessions.sort_by(|a, b| b.ts.cmp(&a.ts));

    let mut app = App {
        sessions,
        selected: 0,
        search: String::new(),
        active_session: None,
        detail_scroll: 0,
        error: None,
    };

    let mut terminal = enter_terminal()?;
    let result = run_app(&mut terminal, &mut app, repository);
    leave_terminal(&mut terminal)?;
    result
}

struct App {
    sessions: Vec<Session>,
    selected: usize,
    search: String,
    active_session: Option<Session>,
    detail_scroll: u16,
    error: Option<String>,
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    repository: &SessionRepository<'_>,
) -> Result<()> {
    loop {
        terminal
            .draw(|frame| {
                if let Some(session) = app.active_session.as_ref() {
                    render_session(frame, app, session);
                    return;
                }

                let [header, list, footer] = Layout::vertical([
                    Constraint::Length(2),
                    Constraint::Min(1),
                    Constraint::Length(2),
                ])
                .areas(frame.area());

                let search = if app.search.is_empty() {
                    Span::styled("Type to search", Style::default().fg(Color::DarkGray))
                } else {
                    Span::raw(app.search.as_str())
                };

                let mut header_lines = vec![Line::from(vec![
                    search,
                    Span::raw("    Filter: "),
                    Span::styled("[Cwd]", Style::default().fg(Color::LightMagenta)),
                    Span::raw(" All    Sort: "),
                    Span::styled("[Updated]", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" Created"),
                ])];
                if let Some(error) = app.error.as_ref() {
                    header_lines.push(Line::styled(error, Style::default().fg(Color::LightRed)));
                }
                frame.render_widget(Paragraph::new(header_lines), header);

                let visible = app.visible_sessions();
                let row_layout = RowLayout::new(list.width as usize, &visible);
                let items = visible
                    .iter()
                    .enumerate()
                    .map(|(row, session)| session_row(session, row == app.selected, row_layout))
                    .collect::<Vec<_>>();

                frame.render_widget(List::new(items), list);

                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("enter", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" read    "),
                        Span::styled("esc", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" quit    "),
                        Span::styled("ctrl+c", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" quit    "),
                        Span::styled("up/down", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" browse"),
                    ])),
                    footer,
                );
            })
            .context("failed to draw the terminal UI")?;

        if let Event::Key(key) = event::read().context("failed to read terminal input")? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break;
            }

            if app.active_session.is_some() {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        app.active_session = None;
                        app.detail_scroll = 0;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.detail_scroll = app.detail_scroll.saturating_add(1)
                    }
                    KeyCode::PageUp => app.detail_scroll = app.detail_scroll.saturating_sub(10),
                    KeyCode::PageDown => app.detail_scroll = app.detail_scroll.saturating_add(10),
                    KeyCode::Home => app.detail_scroll = 0,
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Esc => break,
                KeyCode::Enter => app.open_selected(repository),
                KeyCode::Char(ch) => {
                    app.search.push(ch);
                    app.selected = 0;
                    app.error = None;
                }
                KeyCode::Backspace => {
                    app.search.pop();
                    app.selected = 0;
                    app.error = None;
                }
                KeyCode::Up => app.select_previous(),
                KeyCode::Down => app.select_next(),
                _ => {}
            }
        }
    }

    Ok(())
}

impl App {
    fn visible_sessions(&self) -> Vec<&Session> {
        if self.search.is_empty() {
            return self.sessions.iter().collect();
        }

        let search = self.search.to_lowercase();
        self.sessions
            .iter()
            .filter(|session| session.cwd.to_lowercase().contains(&search))
            .collect()
    }

    fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn select_next(&mut self) {
        let len = self.visible_sessions().len();
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    fn open_selected(&mut self, repository: &SessionRepository<'_>) {
        let selected = self
            .visible_sessions()
            .get(self.selected)
            .map(|session| (session.provider, session.id.clone()));
        let Some((provider, session_id)) = selected else {
            return;
        };

        match repository.load_session(provider, &session_id) {
            Ok(session) => {
                self.active_session = Some(session);
                self.detail_scroll = 0;
                self.error = None;
            }
            Err(err) => self.error = Some(format!("Unable to load session: {err:#}")),
        }
    }
}

fn render_session(frame: &mut ratatui::Frame, app: &App, session: &Session) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    provider_name(&session.provider),
                    Style::default()
                        .fg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(&session.cwd, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::styled(&session.ts, Style::default().fg(Color::DarkGray)),
        ]),
        header,
    );

    let lines = session_message_lines(session.messages.as_deref().unwrap_or_default());

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        body,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("up/down", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" scroll    "),
            Span::styled(
                "page up/down",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" faster    "),
            Span::styled("esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" back"),
        ])),
        footer,
    );
}

const USER_ROLE: &str = "user";
const ASSISTANT_ROLE: &str = "assistant";
const CODEX_COMMENTARY_PHASE: &str = "commentary";
const COMMENTARY_BULLET: &str = "• ";
const EMPTY_SESSION_MESSAGE: &str = "No readable user or assistant messages in this session.";

fn session_message_lines(messages: &[SessionMessage]) -> Vec<Line<'_>> {
    if messages.is_empty() {
        return vec![Line::styled(
            EMPTY_SESSION_MESSAGE,
            Style::default().fg(Color::DarkGray),
        )];
    }

    let mut lines = Vec::new();
    let mut messages = messages.iter().peekable();
    while let Some(message) = messages.next() {
        lines.push(message_header(message));
        if is_codex_commentary(message) {
            lines.extend(commentary_lines(message));
            while let Some(commentary) = messages.next_if(|message| is_codex_commentary(message)) {
                lines.extend(commentary_lines(commentary));
            }
        } else {
            lines.extend(render_markdown(&message.text).lines);
        }
        lines.push(Line::default());
    }

    lines
}

fn message_header(message: &SessionMessage) -> Line<'_> {
    let color = match message.role.as_str() {
        USER_ROLE => Color::LightCyan,
        ASSISTANT_ROLE => Color::LightGreen,
        _ => Color::Gray,
    };
    Line::from(vec![
        Span::styled(
            message.role.as_str(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(&message.ts, Style::default().fg(Color::DarkGray)),
    ])
}

fn is_codex_commentary(message: &SessionMessage) -> bool {
    message.provider == ProviderEnum::Codex
        && message.role == ASSISTANT_ROLE
        && message.phase.as_deref() == Some(CODEX_COMMENTARY_PHASE)
}

fn commentary_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    let mut rendered = render_markdown(&message.text).lines;
    let Some(first_line) = rendered.first_mut() else {
        return vec![Line::from(COMMENTARY_BULLET)];
    };
    first_line.spans.insert(0, Span::raw(COMMENTARY_BULLET));
    rendered
}

#[derive(Clone, Copy)]
struct RowLayout {
    width: usize,
    cwd_width: usize,
}

impl RowLayout {
    fn new(width: usize, sessions: &[&Session]) -> Self {
        let max_cwd_width = sessions
            .iter()
            .map(|session| display_width(&session.cwd))
            .max()
            .unwrap_or_default();
        let fixed_width =
            MARKER_WIDTH + PROVIDER_WIDTH + ELAPSED_WIDTH + MESSAGE_GAP_WIDTH + MIN_MESSAGE_WIDTH;
        let cwd_width = max_cwd_width
            .min(MAX_CWD_WIDTH)
            .min(width.saturating_sub(fixed_width));

        Self { width, cwd_width }
    }

    fn message_width(self) -> usize {
        self.width.saturating_sub(
            MARKER_WIDTH + PROVIDER_WIDTH + ELAPSED_WIDTH + self.cwd_width + MESSAGE_GAP_WIDTH,
        )
    }
}

const MARKER_WIDTH: usize = 2;
const PROVIDER_WIDTH: usize = 7;
const ELAPSED_WIDTH: usize = 9;
const MESSAGE_GAP_WIDTH: usize = 4;
const MIN_MESSAGE_WIDTH: usize = 2;
const MAX_CWD_WIDTH: usize = 40;
const OVERFLOW_MARKER: &str = "..";

fn session_row(session: &Session, selected: bool, layout: RowLayout) -> ListItem<'static> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::LightYellow)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let cwd = fixed_width(&session.cwd, layout.cwd_width);
    let first_message = truncate_end(&session.first_message, layout.message_width());

    ListItem::new(Line::from(vec![
        Span::raw(marker),
        Span::styled(
            fixed_width(provider_name(&session.provider), PROVIDER_WIDTH),
            style,
        ),
        Span::styled(fixed_width(&elapsed(&session.ts), ELAPSED_WIDTH), style),
        Span::styled(cwd, style),
        Span::styled(" ".repeat(MESSAGE_GAP_WIDTH), style),
        Span::styled(first_message, style),
    ]))
}

fn fixed_width(value: &str, width: usize) -> String {
    let value = truncate_end(value, width);
    let padding = width.saturating_sub(display_width(&value));
    format!("{}{}", value, " ".repeat(padding))
}

fn truncate_end(value: &str, max_width: usize) -> String {
    let value = value.replace(['\r', '\n'], " ");
    if display_width(&value) <= max_width {
        return value;
    }

    if max_width <= OVERFLOW_MARKER.len() {
        return ".".repeat(max_width);
    }

    let target_width = max_width - OVERFLOW_MARKER.len();
    let mut truncated = String::new();
    let mut width = 0;

    for ch in value.chars() {
        let ch_width = display_width(&ch.to_string());
        if width + ch_width > target_width {
            break;
        }

        truncated.push(ch);
        width += ch_width;
    }

    truncated.push_str(OVERFLOW_MARKER);
    truncated
}

fn display_width(value: &str) -> usize {
    Span::raw(value).width()
}

fn provider_name(provider: &ProviderEnum) -> &'static str {
    match provider {
        ProviderEnum::Codex => "Codex",
        ProviderEnum::Pi => "Pi",
    }
}

fn elapsed(timestamp: &str) -> String {
    let Ok(ts) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };

    let duration = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
    if duration.num_days() > 0 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m ago", duration.num_minutes())
    } else {
        "now".to_string()
    }
}

fn enter_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("failed to enable terminal raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen)
        .context("failed to enter the alternate terminal screen")?;
    Terminal::new(CrosstermBackend::new(io::stdout())).context("failed to initialize the terminal")
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable terminal raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave the alternate terminal screen")?;
    terminal
        .show_cursor()
        .context("failed to restore the terminal cursor")
}

#[cfg(test)]
mod tests {
    use super::{ASSISTANT_ROLE, EMPTY_SESSION_MESSAGE, session_message_lines};
    use crate::agent::provider::ProviderEnum;
    use crate::agent::session::SessionMessage;
    use ratatui::style::Modifier;

    #[test]
    fn renders_session_message_markdown() {
        let messages = [SessionMessage {
            id: "message-1".to_string(),
            provider: ProviderEnum::Codex,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: "assistant".to_string(),
            text: "A **bold** answer".to_string(),
            phase: None,
        }];

        let lines = session_message_lines(&messages);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "assistant");
        assert_eq!(lines[1].spans[0].content, "A ");
        assert_eq!(lines[1].spans[1].content, "bold");
        assert!(
            lines[1].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[2].content, " answer");
    }

    #[test]
    fn renders_empty_session_message() {
        let lines = session_message_lines(&[]);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, EMPTY_SESSION_MESSAGE);
    }

    #[test]
    fn renders_continuous_codex_commentary_as_one_bulleted_assistant_message() {
        let messages = [
            SessionMessage {
                id: "commentary-1".to_string(),
                provider: ProviderEnum::Codex,
                ts: "2026-07-13T01:00:00Z".to_string(),
                role: "assistant".to_string(),
                text: "Inspecting the repository".to_string(),
                phase: Some("commentary".to_string()),
            },
            SessionMessage {
                id: "commentary-2".to_string(),
                provider: ProviderEnum::Codex,
                ts: "2026-07-13T01:01:00Z".to_string(),
                role: "assistant".to_string(),
                text: "Running the focused tests".to_string(),
                phase: Some("commentary".to_string()),
            },
        ];

        let lines = session_message_lines(&messages);
        let rendered_lines = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            rendered_lines
                .iter()
                .filter(|line| line.starts_with(ASSISTANT_ROLE))
                .count(),
            1
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line.contains("Inspecting the repository") && line.contains('•'))
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line.contains("Running the focused tests") && line.contains('•'))
        );
    }

    #[test]
    fn does_not_group_codex_commentary_across_message_boundaries() {
        let messages = [
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "First update",
            ),
            message(
                ProviderEnum::Codex,
                "assistant",
                "final_answer",
                "First answer",
            ),
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "Second update",
            ),
            message(ProviderEnum::Codex, "user", "", "Continue"),
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "Third update",
            ),
        ];

        let rendered_lines = rendered_lines(&messages);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 4);
        assert_eq!(role_header_count(&rendered_lines, "user"), 1);
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "First answer" && !line.contains('•'))
        );
    }

    #[test]
    fn keeps_pi_assistant_messages_separate() {
        let messages = [
            message(ProviderEnum::Pi, "assistant", "commentary", "First"),
            message(ProviderEnum::Pi, "assistant", "commentary", "Second"),
        ];

        let rendered_lines = rendered_lines(&messages);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 2);
        assert!(rendered_lines.iter().all(|line| !line.contains('•')));
    }

    fn message(provider: ProviderEnum, role: &str, phase: &str, text: &str) -> SessionMessage {
        SessionMessage {
            id: format!("{role}-{phase}-{text}"),
            provider,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: role.to_string(),
            text: text.to_string(),
            phase: (!phase.is_empty()).then(|| phase.to_string()),
        }
    }

    fn rendered_lines(messages: &[SessionMessage]) -> Vec<String> {
        session_message_lines(messages)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    fn role_header_count(lines: &[String], role: &str) -> usize {
        lines.iter().filter(|line| line.starts_with(role)).count()
    }
}
