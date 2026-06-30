use crate::agent::provider::ProviderEnum;
use crate::agent::session::Session;
use crate::{Config, RuntimeErr};
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Terminal;
use std::io::{self, Stdout};

pub(crate) fn run(config: &Config) -> Result<(), RuntimeErr> {
    let mut sessions = config.list_sessions();
    sessions.sort_by(|a, b| b.ts.cmp(&a.ts));

    let mut app = App {
        sessions,
        selected: 0,
        search: String::new(),
    };

    let mut terminal = enter_terminal()?;
    let result = run_app(&mut terminal, &mut app);
    leave_terminal(&mut terminal)?;
    result
}

struct App {
    sessions: Vec<Session>,
    selected: usize,
    search: String,
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<(), RuntimeErr> {
    loop {
        terminal
            .draw(|frame| {
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

                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        search,
                        Span::raw("    Filter: "),
                        Span::styled("[Cwd]", Style::default().fg(Color::LightMagenta)),
                        Span::raw(" All    Sort: "),
                        Span::styled("[Updated]", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(" Created"),
                    ])),
                    header,
                );

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
                        Span::raw(" resume    "),
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
            .map_err(|err| RuntimeErr::Generic(err.to_string()))?;

        if let Event::Key(key) =
            event::read().map_err(|err| RuntimeErr::Generic(err.to_string()))?
        {
            match key.code {
                KeyCode::Esc => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char(ch) => {
                    app.search.push(ch);
                    app.selected = 0;
                }
                KeyCode::Backspace => {
                    app.search.pop();
                    app.selected = 0;
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

fn enter_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>, RuntimeErr> {
    enable_raw_mode().map_err(|err| RuntimeErr::Generic(err.to_string()))?;
    execute!(io::stdout(), EnterAlternateScreen)
        .map_err(|err| RuntimeErr::Generic(err.to_string()))?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
        .map_err(|err| RuntimeErr::Generic(err.to_string()))
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), RuntimeErr> {
    disable_raw_mode().map_err(|err| RuntimeErr::Generic(err.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|err| RuntimeErr::Generic(err.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|err| RuntimeErr::Generic(err.to_string()))
}
