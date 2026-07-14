use crate::session::SessionSummary;
use chrono::{DateTime, TimeDelta, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use std::time::Duration;

const SEARCH_PLACEHOLDER: &str = "Type to search";
const SELECTED_MARKER: &str = "▸ ";
const MARKER_WIDTH: usize = 2;
const AGENT_GAP_WIDTH: usize = 2;
const ELAPSED_WIDTH: usize = 9;
const MESSAGE_GAP_WIDTH: usize = 4;
const MIN_MESSAGE_WIDTH: usize = 2;
const MAX_CWD_WIDTH: usize = 40;
const OVERFLOW_MARKER: &str = "..";

pub(super) struct SessionListState {
    list: ListState,
    cache: Option<CachedSessionList>,
    #[cfg(test)]
    cache_builds: usize,
}

impl SessionListState {
    pub(super) fn new(selected: Option<usize>) -> Self {
        Self {
            list: ListState::default().with_selected(selected),
            cache: None,
            #[cfg(test)]
            cache_builds: 0,
        }
    }

    pub(super) fn selected(&self) -> Option<usize> {
        self.list.selected()
    }

    pub(super) fn select(&mut self, selected: Option<usize>) {
        self.list.select(selected);
    }

    pub(super) fn reset(&mut self, selected: Option<usize>) {
        self.list.select(selected);
        *self.list.offset_mut() = 0;
        self.cache = None;
    }

    pub(super) fn refresh_timeout(&self) -> Option<Duration> {
        self.refresh_timeout_at(Utc::now())
    }

    fn refresh_timeout_at(&self, now: DateTime<Utc>) -> Option<Duration> {
        let refresh_at = self.cache.as_ref()?.refresh_at?;
        Some(
            refresh_at
                .signed_duration_since(now)
                .to_std()
                .unwrap_or_default(),
        )
    }

    #[cfg(test)]
    pub(super) fn cache_builds(&self) -> usize {
        self.cache_builds
    }
}

struct CachedSessionList {
    width: u16,
    refresh_at: Option<DateTime<Utc>>,
    row_widths: Vec<u16>,
    widget: List<'static>,
}

pub(super) fn render_header(frame: &mut Frame, search: &str, error: Option<&str>, area: Rect) {
    let search = if search.is_empty() {
        Span::styled(SEARCH_PLACEHOLDER, Style::default().fg(Color::DarkGray))
    } else {
        Span::raw(search.to_string())
    };
    let mut lines = vec![Line::from(vec![
        search,
        Span::raw("    Filter: "),
        Span::styled("[Message]", Style::default().fg(Color::LightMagenta)),
        Span::raw(" All    Sort: "),
        Span::styled("[Updated]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Created"),
    ])];
    if let Some(error) = error {
        lines.push(Line::styled(error, Style::default().fg(Color::LightRed)));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn render_sessions<'a>(
    frame: &mut Frame,
    sessions: impl ExactSizeIterator<Item = &'a SessionSummary> + Clone,
    state: &mut SessionListState,
    area: Rect,
) {
    render_sessions_at(frame, sessions, state, area, Utc::now());
}

fn render_sessions_at<'a>(
    frame: &mut Frame,
    sessions: impl ExactSizeIterator<Item = &'a SessionSummary> + Clone,
    state: &mut SessionListState,
    area: Rect,
    now: DateTime<Utc>,
) {
    if state.cache.as_ref().is_none_or(|cache| {
        cache.width != area.width || cache.refresh_at.is_some_and(|refresh| now >= refresh)
    }) {
        let layout = RowLayout::new(area.width as usize, sessions.clone());
        let refresh_at = sessions
            .clone()
            .filter_map(|session| next_elapsed_change(session.timestamp.as_str(), now))
            .min();
        let (widget, row_widths) = session_list_at(sessions, layout, now);
        state.cache = Some(CachedSessionList {
            width: area.width,
            refresh_at,
            row_widths,
            widget,
        });
        #[cfg(test)]
        {
            state.cache_builds += 1;
        }
    }

    let cache = state
        .cache
        .as_ref()
        .expect("session list cache initialized");
    frame.render_stateful_widget(&cache.widget, area, &mut state.list);
    let Some(selected) = state.list.selected() else {
        return;
    };
    let Some(row) = selected.checked_sub(state.list.offset()) else {
        return;
    };
    if row >= area.height as usize {
        return;
    }

    let width = cache
        .row_widths
        .get(selected)
        .copied()
        .unwrap_or_default()
        .min(area.width.saturating_sub(MARKER_WIDTH as u16));
    let selected_area = Rect::new(
        area.x.saturating_add(MARKER_WIDTH as u16),
        area.y.saturating_add(row as u16),
        width,
        1,
    );
    frame
        .buffer_mut()
        .set_style(selected_area, selected_style());
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
    agent_width: usize,
    cwd_width: usize,
}

impl RowLayout {
    fn new<'a>(width: usize, sessions: impl Iterator<Item = &'a SessionSummary> + Clone) -> Self {
        let max_agent_width = sessions
            .clone()
            .map(|session| display_width(&session.agent.to_string()))
            .max()
            .unwrap_or_default();
        let minimum_fixed_width =
            MARKER_WIDTH + AGENT_GAP_WIDTH + ELAPSED_WIDTH + MESSAGE_GAP_WIDTH + MIN_MESSAGE_WIDTH;
        let agent_width = max_agent_width.min(width.saturating_sub(minimum_fixed_width));
        let max_cwd_width = sessions
            .map(|session| display_width(&session.cwd.to_string_lossy()))
            .max()
            .unwrap_or_default();
        let fixed_width = minimum_fixed_width + agent_width;
        let cwd_width = max_cwd_width
            .min(MAX_CWD_WIDTH)
            .min(width.saturating_sub(fixed_width));

        Self {
            width,
            agent_width,
            cwd_width,
        }
    }

    fn message_width(self) -> usize {
        self.width.saturating_sub(
            MARKER_WIDTH
                + self.agent_width
                + AGENT_GAP_WIDTH
                + ELAPSED_WIDTH
                + self.cwd_width
                + MESSAGE_GAP_WIDTH,
        )
    }
}

fn session_list_at<'a>(
    sessions: impl Iterator<Item = &'a SessionSummary>,
    layout: RowLayout,
    now: DateTime<Utc>,
) -> (List<'static>, Vec<u16>) {
    let (items, row_widths) = sessions
        .map(|session| session_row(session, layout, now))
        .unzip::<_, _, Vec<_>, Vec<_>>();
    (
        List::new(items).highlight_symbol(SELECTED_MARKER),
        row_widths,
    )
}

fn session_row(
    session: &SessionSummary,
    layout: RowLayout,
    now: DateTime<Utc>,
) -> (ListItem<'static>, u16) {
    let style = Style::default().fg(Color::Gray);
    let cwd = fixed_width(&session.cwd.to_string_lossy(), layout.cwd_width);
    let first_message = truncate_end(&session.first_message, layout.message_width());

    let line = Line::from(vec![
        Span::styled(
            fixed_width(&session.agent.to_string(), layout.agent_width),
            style,
        ),
        Span::styled(" ".repeat(AGENT_GAP_WIDTH), style),
        Span::styled(
            fixed_width(&elapsed_at(session.timestamp.as_str(), now), ELAPSED_WIDTH),
            style,
        ),
        Span::styled(cwd, style),
        Span::styled(" ".repeat(MESSAGE_GAP_WIDTH), style),
        Span::styled(first_message, style),
    ]);
    let width = line.width().min(u16::MAX as usize) as u16;
    (ListItem::new(line), width)
}

fn selected_style() -> Style {
    Style::default()
        .fg(Color::LightYellow)
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
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

fn next_elapsed_change(timestamp: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .ok()?
        .with_timezone(&Utc);
    let duration = now.signed_duration_since(timestamp);
    let step = if duration.num_days() > 0 {
        TimeDelta::try_days(duration.num_days().saturating_add(1))?
    } else if duration.num_hours() > 0 {
        TimeDelta::try_hours(duration.num_hours().saturating_add(1))?
    } else if duration.num_minutes() > 0 {
        TimeDelta::try_minutes(duration.num_minutes().saturating_add(1))?
    } else {
        TimeDelta::try_minutes(1)?
    };
    timestamp.checked_add_signed(step)
}

#[cfg(test)]
mod tests {
    use super::{
        AGENT_GAP_WIDTH, ELAPSED_WIDTH, MARKER_WIDTH, MAX_CWD_WIDTH, MESSAGE_GAP_WIDTH,
        MIN_MESSAGE_WIDTH, RowLayout, SessionListState, display_width, elapsed_at, fixed_width,
        next_elapsed_change, render_sessions_at, session_list_at, truncate_end,
    };
    use crate::agent::AgentKind;
    use crate::session::{SessionLocator, SessionSummary, SessionTimestamp};
    use chrono::{TimeZone, Utc};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::{List, ListState, StatefulWidget};
    use std::time::Duration;

    #[test]
    fn limits_cwd_width_and_preserves_message_space() {
        let session = session(&"a".repeat(MAX_CWD_WIDTH + 10));
        let layout = RowLayout::new(80, [&session].into_iter());

        assert_eq!(layout.cwd_width, MAX_CWD_WIDTH);
        assert!(layout.message_width() >= 2);
    }

    #[test]
    fn sizes_agent_columns_for_all_agents_and_terminal_widths() {
        let claude = session_for(AgentKind::Claude, "/work/project");
        let codex = session_for(AgentKind::Codex, "/work/project");
        let pi = session_for(AgentKind::Pi, "/work/project");
        let sessions = [&claude, &codex, &pi];
        let widest_agent = sessions
            .iter()
            .map(|session| display_width(&session.agent.to_string()))
            .max()
            .unwrap();
        let reserved_width =
            MARKER_WIDTH + AGENT_GAP_WIDTH + ELAPSED_WIDTH + MESSAGE_GAP_WIDTH + MIN_MESSAGE_WIDTH;
        let boundary_width = reserved_width + widest_agent;
        let layout = RowLayout::new(boundary_width, sessions.iter().copied());
        let agent_start = MARKER_WIDTH;
        let agent_end = agent_start + widest_agent;
        let gap_end = agent_end + AGENT_GAP_WIDTH;
        let message_start = boundary_width - MIN_MESSAGE_WIDTH;

        assert_eq!(layout.agent_width, widest_agent);
        assert_eq!(layout.cwd_width, 0);
        assert_eq!(layout.message_width(), MIN_MESSAGE_WIDTH);
        for session in sessions {
            let area = Rect::new(0, 0, boundary_width as u16, 1);
            let mut buffer = Buffer::empty(area);
            let mut state = ListState::default().with_selected(Some(0));
            StatefulWidget::render(
                session_list([session].into_iter(), layout),
                area,
                &mut buffer,
                &mut state,
            );
            let cells: Vec<&str> = buffer
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();

            assert_eq!(
                cells[agent_start..agent_end].join(""),
                fixed_width(&session.agent.to_string(), widest_agent)
            );
            assert_eq!(cells[agent_end..gap_end].join(""), " ".repeat(AGENT_GAP_WIDTH));
            assert_ne!(cells[gap_end], " ");
            assert_eq!(
                cells[message_start..].join(""),
                truncate_end(&session.first_message, MIN_MESSAGE_WIDTH)
            );
        }

        let narrow_layout = RowLayout::new(boundary_width - 1, sessions.iter().copied());

        assert_eq!(narrow_layout.agent_width, widest_agent - 1);
        assert_eq!(narrow_layout.cwd_width, 0);
        assert_eq!(narrow_layout.message_width(), MIN_MESSAGE_WIDTH);
    }

    #[test]
    fn truncates_and_pads_display_text() {
        assert_eq!(truncate_end("abcdef", 5), "abc..");
        assert_eq!(truncate_end("first\nsecond", 8), "first ..");
        assert_eq!(truncate_end("abcdef", 2), "..");
        assert_eq!(fixed_width("abc", 5), "abc  ");
    }

    #[test]
    fn renders_full_claude_code_agent_name() {
        let session = session_for(AgentKind::Claude, "/work/project");
        let area = Rect::new(0, 0, 80, 1);
        let layout = RowLayout::new(area.width as usize, [&session].into_iter());
        let mut buffer = Buffer::empty(area);
        let mut state = ListState::default().with_selected(Some(0));

        StatefulWidget::render(
            session_list([&session].into_iter(), layout),
            area,
            &mut buffer,
            &mut state,
        );

        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            rendered.contains("Claude Code"),
            "rendered row: {rendered:?}"
        );
    }

    #[test]
    fn scrolls_to_keep_the_selected_session_in_view() {
        let sessions = (0..4)
            .map(|index| session(&format!("/work/{index}")))
            .collect::<Vec<_>>();
        let area = Rect::new(0, 0, 80, 2);
        let layout = RowLayout::new(area.width as usize, sessions.iter());
        let mut buffer = Buffer::empty(area);
        let mut state = ListState::default().with_selected(Some(3));

        StatefulWidget::render(
            session_list(sessions.iter(), layout),
            area,
            &mut buffer,
            &mut state,
        );

        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(state.offset(), 2);
        assert!(rendered.contains("/work/2"));
        assert!(rendered.contains("/work/3"));
        assert!(!rendered.contains("/work/0"));
    }

    #[test]
    fn formats_elapsed_time_against_a_fixed_clock() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap();

        assert_eq!(elapsed_at("2026-07-11T12:00:00Z", now), "2d ago");
        assert_eq!(elapsed_at("2026-07-13T10:00:00Z", now), "2h ago");
        assert_eq!(elapsed_at("2026-07-13T11:45:00Z", now), "15m ago");
        assert_eq!(elapsed_at("invalid", now), "invalid");
    }

    #[test]
    fn refreshes_elapsed_time_at_the_session_relative_boundary() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 20).unwrap();
        let refresh = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 30).unwrap();
        let timestamp = "2026-07-13T11:45:30Z";

        assert_eq!(elapsed_at(timestamp, now), "14m ago");
        assert_eq!(next_elapsed_change(timestamp, now), Some(refresh));
        assert_eq!(elapsed_at(timestamp, refresh), "15m ago");
    }

    #[test]
    fn rebuilds_cache_for_elapsed_transitions_and_width_changes() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 20).unwrap();
        let refresh = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 30).unwrap();
        let sessions = [session_for_at(
            AgentKind::Codex,
            "/work/project",
            "2026-07-13T11:45:30Z",
        )];
        let mut state = SessionListState::new(Some(0));
        let mut terminal = Terminal::new(TestBackend::new(80, 2)).unwrap();

        terminal
            .draw(|frame| {
                render_sessions_at(frame, sessions.iter(), &mut state, frame.area(), now);
            })
            .unwrap();
        assert_eq!(state.cache_builds(), 1);
        assert_eq!(state.refresh_timeout_at(now), Some(Duration::from_secs(10)));

        terminal
            .draw(|frame| {
                render_sessions_at(frame, sessions.iter(), &mut state, frame.area(), refresh);
            })
            .unwrap();
        assert_eq!(state.cache_builds(), 2);

        terminal.backend_mut().resize(70, 2);
        terminal.autoresize().unwrap();
        terminal
            .draw(|frame| {
                render_sessions_at(frame, sessions.iter(), &mut state, frame.area(), refresh);
            })
            .unwrap();
        assert_eq!(state.cache_builds(), 3);
    }

    use std::path::PathBuf;

    fn session_list<'a>(
        sessions: impl Iterator<Item = &'a SessionSummary>,
        layout: RowLayout,
    ) -> List<'static> {
        session_list_at(sessions, layout, Utc::now()).0
    }

    fn session(cwd: &str) -> SessionSummary {
        session_for(AgentKind::Codex, cwd)
    }

    fn session_for(agent: AgentKind, cwd: &str) -> SessionSummary {
        session_for_at(agent, cwd, "2026-07-13T01:00:00Z")
    }

    fn session_for_at(agent: AgentKind, cwd: &str, timestamp: &str) -> SessionSummary {
        SessionSummary {
            id: "session".to_string(),
            agent,
            timestamp: SessionTimestamp::new(timestamp),
            cwd: PathBuf::from(cwd),
            first_message: "First message".to_string(),
            locator: SessionLocator::new(PathBuf::from("/sessions/session.jsonl")),
        }
    }
}
