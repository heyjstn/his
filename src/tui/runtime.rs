use super::app::App;
use crate::repository::SessionRepository;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    None,
    OpenSelected,
    Quit,
}

pub(crate) fn run(repository: &SessionRepository) -> Result<()> {
    let catalog = repository.list_sessions();
    for warning in &catalog.warnings {
        eprintln!("warning: {warning}");
    }
    let notice = catalog.warning_message();
    let mut app = App::new(catalog.sessions, notice);
    let mut terminal = TerminalGuard::enter()?;
    let result = run_app(terminal.terminal(), &mut app, repository);
    let restore_result = terminal.restore();

    match result {
        Ok(()) => restore_result,
        Err(error) => {
            let _ = restore_result;
            Err(error)
        }
    }
}

fn run_app(
    terminal: &mut TuiTerminal,
    app: &mut App,
    repository: &SessionRepository,
) -> Result<()> {
    loop {
        terminal
            .draw(|frame| super::render(frame, app))
            .context("failed to draw the terminal UI")?;

        let Event::Key(key) = event::read().context("failed to read terminal input")? else {
            continue;
        };

        match handle_key(app, key) {
            Action::None => {}
            Action::OpenSelected => open_selected(app, repository),
            Action::Quit => return Ok(()),
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }

    if app.active_session().is_some() {
        handle_detail_key(app, key);
        return Action::None;
    }

    match key.code {
        KeyCode::Esc => return Action::Quit,
        KeyCode::Enter => return Action::OpenSelected,
        KeyCode::Char(character) => app.append_search(character),
        KeyCode::Backspace => app.remove_search_character(),
        KeyCode::Up => app.select_previous(),
        KeyCode::Down => app.select_next(),
        _ => {}
    }

    Action::None
}

fn open_selected(app: &mut App, repository: &SessionRepository) {
    let result = app
        .selected_session()
        .map(|summary| repository.load_session(summary));
    let Some(result) = result else {
        return;
    };

    match result {
        Ok(session) => app.show_session(session),
        Err(error) => app.show_load_error(&error),
    }
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

struct TerminalGuard {
    terminal: TuiTerminal,
    restoration: RestorationState,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable terminal raw mode")?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error).context("failed to enter the alternate terminal screen");
        }

        let terminal = match Terminal::new(CrosstermBackend::new(io::stdout())) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                return Err(error).context("failed to initialize the terminal");
            }
        };
        Ok(Self {
            terminal,
            restoration: RestorationState::active(),
        })
    }

    fn terminal(&mut self) -> &mut TuiTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) -> Result<()> {
        self.restoration.restore(&mut CrosstermRestorer {
            terminal: &mut self.terminal,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct RestorationState {
    raw_mode: bool,
    alternate_screen: bool,
    cursor: bool,
}

impl RestorationState {
    fn active() -> Self {
        Self {
            raw_mode: true,
            alternate_screen: true,
            cursor: true,
        }
    }

    fn restore(&mut self, operations: &mut impl RestoreOperations) -> Result<()> {
        let mut first_error = None;
        if self.raw_mode {
            restore_operation(
                &mut self.raw_mode,
                operations.disable_raw_mode(),
                &mut first_error,
            );
        }
        if self.alternate_screen {
            restore_operation(
                &mut self.alternate_screen,
                operations.leave_alternate_screen(),
                &mut first_error,
            );
        }
        if self.cursor {
            restore_operation(&mut self.cursor, operations.show_cursor(), &mut first_error);
        }

        first_error.map_or(Ok(()), Err)
    }
}

fn restore_operation(
    active: &mut bool,
    result: Result<()>,
    first_error: &mut Option<anyhow::Error>,
) {
    match result {
        Ok(()) => *active = false,
        Err(error) if first_error.is_none() => *first_error = Some(error),
        Err(_) => {}
    }
}

trait RestoreOperations {
    fn disable_raw_mode(&mut self) -> Result<()>;
    fn leave_alternate_screen(&mut self) -> Result<()>;
    fn show_cursor(&mut self) -> Result<()>;
}

struct CrosstermRestorer<'a> {
    terminal: &'a mut TuiTerminal,
}

impl RestoreOperations for CrosstermRestorer<'_> {
    fn disable_raw_mode(&mut self) -> Result<()> {
        disable_raw_mode().context("failed to disable terminal raw mode")
    }

    fn leave_alternate_screen(&mut self) -> Result<()> {
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
            .context("failed to leave the alternate terminal screen")
    }

    fn show_cursor(&mut self) -> Result<()> {
        self.terminal
            .show_cursor()
            .context("failed to restore the terminal cursor")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Action, PAGE_SCROLL_ROWS, RestorationState, RestoreOperations, handle_detail_key,
        handle_key,
    };
    use crate::agent::AgentKind;
    use crate::session::{SessionDetail, SessionTimestamp};
    use crate::tui::app::App;
    use anyhow::{Result, anyhow};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    #[test]
    fn retries_only_terminal_restoration_operations_that_failed() {
        let mut restoration = RestorationState::active();
        let mut operations = FakeRestoreOperations {
            fail_raw_mode: true,
            ..FakeRestoreOperations::default()
        };

        let error = restoration.restore(&mut operations).unwrap_err();

        assert_eq!(error.to_string(), "raw mode failure");
        assert!(restoration.raw_mode);
        assert!(!restoration.alternate_screen);
        assert!(!restoration.cursor);
        assert_eq!(operations.calls, [1, 1, 1]);

        operations.fail_raw_mode = false;
        restoration.restore(&mut operations).unwrap();

        assert!(!restoration.raw_mode);
        assert_eq!(operations.calls, [2, 1, 1]);
    }

    #[test]
    fn maps_escape_and_control_c_to_quit() {
        let mut app = App::new(Vec::new(), None);

        assert_eq!(
            handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::Quit
        );
        assert_eq!(
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            Action::Quit
        );
    }

    #[test]
    fn maps_enter_to_an_effect_without_loading_a_session() {
        let mut app = App::new(Vec::new(), None);

        let action = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(action, Action::OpenSelected);
        assert!(app.active_session().is_none());
    }

    #[test]
    fn updates_search_from_character_and_backspace_keys() {
        let mut app = App::new(Vec::new(), None);

        assert_eq!(
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)
            ),
            Action::None
        );
        assert_eq!(app.search(), "h");

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
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
            let mut app = App::new(Vec::new(), None);
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
        let mut app = App::new(Vec::new(), None);
        app.show_session(session());

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(app.commentary_visible());

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(!app.commentary_visible());
    }

    fn session() -> SessionDetail {
        SessionDetail {
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from("/work/project"),
            messages: Vec::new(),
        }
    }

    #[derive(Default)]
    struct FakeRestoreOperations {
        fail_raw_mode: bool,
        calls: [u8; 3],
    }

    impl RestoreOperations for FakeRestoreOperations {
        fn disable_raw_mode(&mut self) -> Result<()> {
            self.calls[0] += 1;
            if self.fail_raw_mode {
                return Err(anyhow!("raw mode failure"));
            }
            Ok(())
        }

        fn leave_alternate_screen(&mut self) -> Result<()> {
            self.calls[1] += 1;
            Ok(())
        }

        fn show_cursor(&mut self) -> Result<()> {
            self.calls[2] += 1;
            Ok(())
        }
    }
}
