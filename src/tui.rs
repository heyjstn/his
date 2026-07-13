mod app;
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
        session_list::render_header(frame, app.search(), app.error(), header);
        session_list::render_sessions(frame, &app.visible_sessions(), app.selected(), body);
        session_list::render_footer(frame, footer);
        return;
    };

    session_detail::render_header(frame, session, header);
    session_detail::render_messages(
        frame,
        session.messages.as_deref().unwrap_or_default(),
        app.commentary_visible(),
        app.detail_scroll(),
        body,
    );
    session_detail::render_footer(frame, app.commentary_visible(), footer);
}

#[cfg(test)]
mod tests {
    use super::{App, render};
    use crate::agent::provider::ProviderEnum;
    use crate::agent::session::{Session, SessionMessage};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    const TERMINAL_WIDTH: u16 = 100;
    const TERMINAL_HEIGHT: u16 = 10;

    #[test]
    fn renders_session_list_components_in_their_layout_regions() {
        let mut terminal = test_terminal();
        let app = App::new(vec![session(None)]);

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer_row(buffer, 0).starts_with("Type to search"));
        assert!(buffer_row(buffer, 2).contains("/work/project"));
        assert!(buffer_row(buffer, 8).starts_with("enter read"));
    }

    #[test]
    fn renders_session_detail_components_in_their_layout_regions() {
        let mut terminal = test_terminal();
        let mut app = App::new(Vec::new());
        app.show_session(session(Some(vec![
            message("commentary", "Hidden commentary"),
            message("final_answer", "Rendered answer"),
        ])));

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

    fn session(messages: Option<Vec<SessionMessage>>) -> Session {
        Session {
            id: "session".to_string(),
            provider: ProviderEnum::Codex,
            ts: "2026-07-13T01:00:00Z".to_string(),
            cwd: "/work/project".to_string(),
            messages,
            first_message: "First message".to_string(),
        }
    }

    fn message(phase: &str, text: &str) -> SessionMessage {
        SessionMessage {
            id: format!("{phase}-{text}"),
            provider: ProviderEnum::Codex,
            ts: "2026-07-13T01:01:00Z".to_string(),
            role: "assistant".to_string(),
            text: text.to_string(),
            phase: Some(phase.to_string()),
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
