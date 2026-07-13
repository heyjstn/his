use super::app::App;
use crate::session::SessionRepository;
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};

const LINE_SCROLL_ROWS: u16 = 1;
const PAGE_SCROLL_ROWS: u16 = 10;

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub(crate) fn run(repository: &SessionRepository<'_>) -> Result<()> {
    let mut app = App::new(repository.list_sessions()?);
    let mut terminal = enter_terminal()?;
    let result = run_app(&mut terminal, &mut app, repository);
    leave_terminal(&mut terminal)?;
    result
}

fn run_app(
    terminal: &mut TuiTerminal,
    app: &mut App,
    repository: &SessionRepository<'_>,
) -> Result<()> {
    loop {
        terminal
            .draw(|frame| super::render(frame, app))
            .context("failed to draw the terminal UI")?;

        let Event::Key(key) = event::read().context("failed to read terminal input")? else {
            continue;
        };

        if handle_key(app, repository, key) {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, repository: &SessionRepository<'_>, key: KeyEvent) -> bool {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }

    if app.active_session().is_some() {
        handle_detail_key(app, key);
        return false;
    }

    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter => app.open_selected(repository),
        KeyCode::Char(character) => app.append_search(character),
        KeyCode::Backspace => app.remove_search_character(),
        KeyCode::Up => app.select_previous(),
        KeyCode::Down => app.select_next(),
        _ => {}
    }

    false
}

fn handle_detail_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.toggle_commentary_visibility();
        return;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_active_session(),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_detail_up(LINE_SCROLL_ROWS),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_detail_down(LINE_SCROLL_ROWS),
        KeyCode::PageUp => app.scroll_detail_up(PAGE_SCROLL_ROWS),
        KeyCode::PageDown => app.scroll_detail_down(PAGE_SCROLL_ROWS),
        KeyCode::Home => app.scroll_detail_home(),
        _ => {}
    }
}

fn enter_terminal() -> Result<TuiTerminal> {
    enable_raw_mode().context("failed to enable terminal raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen)
        .context("failed to enter the alternate terminal screen")?;
    Terminal::new(CrosstermBackend::new(io::stdout())).context("failed to initialize the terminal")
}

fn leave_terminal(terminal: &mut TuiTerminal) -> Result<()> {
    disable_raw_mode().context("failed to disable terminal raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave the alternate terminal screen")?;
    terminal
        .show_cursor()
        .context("failed to restore the terminal cursor")
}

#[cfg(test)]
mod tests {
    use super::{PAGE_SCROLL_ROWS, handle_detail_key, handle_key};
    use crate::agent::AgentKind;
    use crate::session::{Session, SessionRepository};
    use crate::tui::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn exits_on_escape_or_control_c() {
        let repository = SessionRepository::new(&[]).unwrap();
        let mut app = App::new(Vec::new());

        assert!(handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ));
        assert!(handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ));
    }

    #[test]
    fn updates_search_from_character_and_backspace_keys() {
        let repository = SessionRepository::new(&[]).unwrap();
        let mut app = App::new(Vec::new());

        assert!(!handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        ));
        assert_eq!(app.search(), "h");

        assert!(!handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        ));
        assert!(app.search().is_empty());
    }

    #[test]
    fn maps_detail_keys_to_navigation_transitions() {
        let cases = [
            (KeyCode::Up, 1, 0, false),
            (KeyCode::Char('k'), 1, 0, false),
            (KeyCode::Down, 0, 1, false),
            (KeyCode::Char('j'), 0, 1, false),
            (KeyCode::PageUp, PAGE_SCROLL_ROWS + 5, 5, false),
            (KeyCode::PageDown, 0, PAGE_SCROLL_ROWS, false),
            (KeyCode::Home, 5, 0, false),
            (KeyCode::Esc, 0, 0, true),
            (KeyCode::Char('q'), 0, 0, true),
        ];

        for (key, initial_scroll, expected_scroll, closes_session) in cases {
            let mut app = App::new(Vec::new());
            app.show_session(session());
            app.scroll_detail_down(initial_scroll);

            handle_detail_key(&mut app, KeyEvent::new(key, KeyModifiers::NONE));

            assert_eq!(app.detail_scroll(), expected_scroll, "key: {key:?}");
            assert_eq!(
                app.active_session().is_none(),
                closes_session,
                "key: {key:?}"
            );
        }
    }

    #[test]
    fn control_o_toggles_commentary_visibility_in_session_detail() {
        let repository = SessionRepository::new(&[]).unwrap();
        let mut app = App::new(Vec::new());
        app.show_session(session());

        assert!(!handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        ));
        assert!(app.commentary_visible());

        assert!(!handle_key(
            &mut app,
            &repository,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        ));
        assert!(!app.commentary_visible());
    }

    fn session() -> Session {
        Session {
            id: "session".to_string(),
            agent: AgentKind::Codex,
            ts: "2026-07-13T01:00:00Z".to_string(),
            cwd: "/work/project".to_string(),
            messages: None,
            first_message: "First message".to_string(),
        }
    }
}
