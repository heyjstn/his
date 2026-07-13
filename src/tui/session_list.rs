use crate::session::SessionSummary;
use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

const SEARCH_PLACEHOLDER: &str = "Type to search";
const SELECTED_MARKER: &str = "> ";
const UNSELECTED_MARKER: &str = "  ";
const MARKER_WIDTH: usize = 2;
const AGENT_WIDTH: usize = 7;
const ELAPSED_WIDTH: usize = 9;
const MESSAGE_GAP_WIDTH: usize = 4;
const MIN_MESSAGE_WIDTH: usize = 2;
const MAX_CWD_WIDTH: usize = 40;
const OVERFLOW_MARKER: &str = "..";

pub(super) fn render_header(frame: &mut Frame, search: &str, error: Option<&str>, area: Rect) {
    let search = if search.is_empty() {
        Span::styled(SEARCH_PLACEHOLDER, Style::default().fg(Color::DarkGray))
    } else {
        Span::raw(search.to_string())
    };
    let mut lines = vec![Line::from(vec![
        search,
        Span::raw("    Filter: "),
        Span::styled("[Cwd]", Style::default().fg(Color::LightMagenta)),
        Span::raw(" All    Sort: "),
        Span::styled("[Updated]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Created"),
    ])];
    if let Some(error) = error {
        lines.push(Line::styled(error, Style::default().fg(Color::LightRed)));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn render_sessions(
    frame: &mut Frame,
    sessions: &[&SessionSummary],
    selected: usize,
    area: Rect,
) {
    let layout = RowLayout::new(area.width as usize, sessions);
    let items = sessions
        .iter()
        .enumerate()
        .map(|(row, session)| session_row(session, row == selected, layout))
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items), area);
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" read    "),
            Span::styled("esc/ctrl+c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit    "),
            Span::styled("up/down", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" browse"),
        ])),
        area,
    );
}

#[derive(Clone, Copy)]
struct RowLayout {
    width: usize,
    cwd_width: usize,
}

impl RowLayout {
    fn new(width: usize, sessions: &[&SessionSummary]) -> Self {
        let max_cwd_width = sessions
            .iter()
            .map(|session| display_width(&session.cwd.to_string_lossy()))
            .max()
            .unwrap_or_default();
        let fixed_width =
            MARKER_WIDTH + AGENT_WIDTH + ELAPSED_WIDTH + MESSAGE_GAP_WIDTH + MIN_MESSAGE_WIDTH;
        let cwd_width = max_cwd_width
            .min(MAX_CWD_WIDTH)
            .min(width.saturating_sub(fixed_width));

        Self { width, cwd_width }
    }

    fn message_width(self) -> usize {
        self.width.saturating_sub(
            MARKER_WIDTH + AGENT_WIDTH + ELAPSED_WIDTH + self.cwd_width + MESSAGE_GAP_WIDTH,
        )
    }
}

fn session_row(session: &SessionSummary, selected: bool, layout: RowLayout) -> ListItem<'static> {
    let marker = if selected {
        SELECTED_MARKER
    } else {
        UNSELECTED_MARKER
    };
    let style = if selected {
        Style::default()
            .fg(Color::LightYellow)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let cwd = fixed_width(&session.cwd.to_string_lossy(), layout.cwd_width);
    let first_message = truncate_end(&session.first_message, layout.message_width());

    ListItem::new(Line::from(vec![
        Span::raw(marker),
        Span::styled(fixed_width(&session.agent.to_string(), AGENT_WIDTH), style),
        Span::styled(
            fixed_width(&elapsed(session.timestamp.as_str()), ELAPSED_WIDTH),
            style,
        ),
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

    for character in value.chars() {
        let character_width = display_width(&character.to_string());
        if width + character_width > target_width {
            break;
        }

        truncated.push(character);
        width += character_width;
    }

    truncated.push_str(OVERFLOW_MARKER);
    truncated
}

fn display_width(value: &str) -> usize {
    Span::raw(value).width()
}

fn elapsed(timestamp: &str) -> String {
    elapsed_at(timestamp, Utc::now())
}

fn elapsed_at(timestamp: &str, now: DateTime<Utc>) -> String {
    let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_string();
    };

    let duration = now.signed_duration_since(timestamp.with_timezone(&Utc));
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

#[cfg(test)]
mod tests {
    use super::{MAX_CWD_WIDTH, RowLayout, elapsed_at, fixed_width, truncate_end};
    use crate::agent::AgentKind;
    use crate::session::{SessionLocator, SessionSummary, SessionTimestamp};
    use chrono::{TimeZone, Utc};

    #[test]
    fn limits_cwd_width_and_preserves_message_space() {
        let session = session(&"a".repeat(MAX_CWD_WIDTH + 10));
        let layout = RowLayout::new(80, &[&session]);

        assert_eq!(layout.cwd_width, MAX_CWD_WIDTH);
        assert!(layout.message_width() >= 2);
    }

    #[test]
    fn truncates_and_pads_display_text() {
        assert_eq!(truncate_end("abcdef", 5), "abc..");
        assert_eq!(truncate_end("first\nsecond", 8), "first ..");
        assert_eq!(truncate_end("abcdef", 2), "..");
        assert_eq!(fixed_width("abc", 5), "abc  ");
    }

    #[test]
    fn formats_elapsed_time_against_a_fixed_clock() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap();

        assert_eq!(elapsed_at("2026-07-11T12:00:00Z", now), "2d ago");
        assert_eq!(elapsed_at("2026-07-13T10:00:00Z", now), "2h ago");
        assert_eq!(elapsed_at("2026-07-13T11:45:00Z", now), "15m ago");
        assert_eq!(elapsed_at("invalid", now), "invalid");
    }

    use std::path::PathBuf;

    fn session(cwd: &str) -> SessionSummary {
        SessionSummary {
            id: "session".to_string(),
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from(cwd),
            first_message: "First message".to_string(),
            locator: SessionLocator::new(PathBuf::from("/sessions/session.jsonl")),
        }
    }
}
