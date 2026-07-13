mod app;
mod markdown;
mod runtime;
mod session_detail;
mod session_list;

use app::App;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

pub(crate) use runtime::run;

fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    let Some(session) = app.active_session() else {
        session_list::render_header(frame, app.search(), app.notice(), header);
        session_list::render_sessions(frame, &app.visible_sessions(), app.selected(), body);
        session_list::render_footer(frame, footer);
        return;
    };

    session_detail::render_header(frame, session, header);
    session_detail::render_messages(
        frame,
        &session.messages,
        app.commentary_visible(),
        app.detail_scroll(),
        body,
    );
    session_detail::render_footer(frame, app.commentary_visible(), footer);
}

#[cfg(test)]
mod tests {
    use super::{App, render};
    use crate::agent::AgentKind;
    use crate::session::{
        MessagePhase, MessageRole, SessionDetail, SessionLocator, SessionMessage, SessionSummary,
        SessionTimestamp,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use std::path::PathBuf;

    const TERMINAL_WIDTH: u16 = 100;
    const TERMINAL_HEIGHT: u16 = 10;

    #[test]
    fn renders_session_list_components_in_their_layout_regions() {
        let mut terminal = test_terminal();
        let app = App::new(vec![summary()], None);

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer_row(buffer, 0).starts_with("Type to search"));
        assert!(buffer_row(buffer, 2).contains("/work/project"));
        assert!(buffer_row(buffer, 8).starts_with("enter read"));
    }

    #[test]
    fn renders_session_detail_components_in_their_layout_regions() {
        let mut terminal = test_terminal();
        let mut app = App::new(Vec::new(), None);
        app.show_session(detail(vec![
            message(MessagePhase::Commentary, "Hidden commentary"),
            message(MessagePhase::FinalAnswer, "Rendered answer"),
        ]));

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer_row(buffer, 0).starts_with("Codex  /work/project"));
        assert!(buffer_row(buffer, 2).starts_with("assistant"));
        assert_eq!(buffer_row(buffer, 3), "Rendered answer");
        assert!(!buffer_contains(buffer, "Hidden commentary"));
        assert!(buffer_row(buffer, 8).contains("ctrl+o show commentary"));

        app.toggle_commentary_visibility();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer_contains(buffer, "Hidden commentary"));
        assert!(buffer_row(buffer, 8).contains("ctrl+o hide commentary"));
    }

    fn test_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(TERMINAL_WIDTH, TERMINAL_HEIGHT)).unwrap()
    }

    fn summary() -> SessionSummary {
        SessionSummary {
            id: "session".to_string(),
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from("/work/project"),
            first_message: "First message".to_string(),
            locator: SessionLocator::new(PathBuf::from("/sessions/session.jsonl")),
        }
    }

    fn detail(messages: Vec<SessionMessage>) -> SessionDetail {
        SessionDetail {
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from("/work/project"),
            messages,
        }
    }

    fn message(phase: MessagePhase, text: &str) -> SessionMessage {
        SessionMessage {
            timestamp: SessionTimestamp::new("2026-07-13T01:01:00Z"),
            role: MessageRole::Assistant,
            text: text.to_string(),
            phase: Some(phase),
            tool_path: None,
            tool_contents: Vec::new(),
        }
    }

    fn buffer_contains(buffer: &Buffer, expected: &str) -> bool {
        (0..buffer.area.height as usize).any(|row| buffer_row(buffer, row).contains(expected))
    }

    fn buffer_row(buffer: &Buffer, row: usize) -> String {
        let width = buffer.area.width as usize;
        buffer.content()[row * width..(row + 1) * width]
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}
