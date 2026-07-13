use crate::agent::session::{Session, SessionRepository};

const LOAD_SESSION_ERROR_PREFIX: &str = "Unable to load session";

pub(super) struct App {
    sessions: Vec<Session>,
    selected: usize,
    search: String,
    active_session: Option<Session>,
    detail_scroll: u16,
    error: Option<String>,
}

impl App {
    pub(super) fn new(mut sessions: Vec<Session>) -> Self {
        sessions.sort_by(|left, right| right.ts.cmp(&left.ts));

        Self {
            sessions,
            selected: 0,
            search: String::new(),
            active_session: None,
            detail_scroll: 0,
            error: None,
        }
    }

    pub(super) fn visible_sessions(&self) -> Vec<&Session> {
        if self.search.is_empty() {
            return self.sessions.iter().collect();
        }

        let search = self.search.to_lowercase();
        self.sessions
            .iter()
            .filter(|session| session.cwd.to_lowercase().contains(&search))
            .collect()
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn search(&self) -> &str {
        &self.search
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn active_session(&self) -> Option<&Session> {
        self.active_session.as_ref()
    }

    pub(super) fn detail_scroll(&self) -> u16 {
        self.detail_scroll
    }

    pub(super) fn append_search(&mut self, character: char) {
        self.search.push(character);
        self.reset_search_state();
    }

    pub(super) fn remove_search_character(&mut self) {
        self.search.pop();
        self.reset_search_state();
    }

    pub(super) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(super) fn select_next(&mut self) {
        if self.selected + 1 < self.visible_sessions().len() {
            self.selected += 1;
        }
    }

    pub(super) fn open_selected(&mut self, repository: &SessionRepository<'_>) {
        let selected = self
            .visible_sessions()
            .get(self.selected)
            .map(|session| (session.provider, session.id.clone()));
        let Some((provider, session_id)) = selected else {
            return;
        };

        match repository.load_session(provider, &session_id) {
            Ok(session) => self.show_session(session),
            Err(error) => {
                self.error = Some(format!("{LOAD_SESSION_ERROR_PREFIX}: {error:#}"));
            }
        }
    }

    pub(super) fn show_session(&mut self, session: Session) {
        self.active_session = Some(session);
        self.detail_scroll = 0;
        self.error = None;
    }

    pub(super) fn close_active_session(&mut self) {
        self.active_session = None;
        self.detail_scroll = 0;
    }

    pub(super) fn scroll_detail_up(&mut self, rows: u16) {
        self.detail_scroll = self.detail_scroll.saturating_sub(rows);
    }

    pub(super) fn scroll_detail_down(&mut self, rows: u16) {
        self.detail_scroll = self.detail_scroll.saturating_add(rows);
    }

    pub(super) fn scroll_detail_home(&mut self) {
        self.detail_scroll = 0;
    }

    fn reset_search_state(&mut self) {
        self.selected = 0;
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{App, LOAD_SESSION_ERROR_PREFIX};
    use crate::agent::provider::{Provider, ProviderEnum};
    use crate::agent::session::{Session, SessionRepository};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn sorts_sessions_by_most_recent_first() {
        let app = App::new(vec![
            session("older", "/work/older", "2026-07-12T01:00:00Z"),
            session("newer", "/work/newer", "2026-07-13T01:00:00Z"),
        ]);

        let visible = app.visible_sessions();

        assert_eq!(visible[0].id, "newer");
        assert_eq!(visible[1].id, "older");
    }

    #[test]
    fn filters_sessions_case_insensitively_and_resets_selection() {
        let mut app = App::new(vec![
            session("frontend", "/work/Frontend", "2026-07-13T01:00:00Z"),
            session("backend", "/work/backend", "2026-07-12T01:00:00Z"),
        ]);
        app.select_next();
        app.error = Some("previous error".to_string());

        for character in "FRONT".chars() {
            app.append_search(character);
        }

        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "frontend");
        assert_eq!(app.selected(), 0);
        assert_eq!(app.error(), None);

        app.remove_search_character();
        assert_eq!(app.search(), "FRON");
    }

    #[test]
    fn keeps_selection_within_visible_sessions() {
        let mut app = App::new(vec![
            session("first", "/work/first", "2026-07-13T01:00:00Z"),
            session("second", "/work/second", "2026-07-12T01:00:00Z"),
        ]);

        app.select_next();
        app.select_next();
        assert_eq!(app.selected(), 1);

        app.select_previous();
        app.select_previous();
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn scrolls_detail_safely_and_resets_when_closed() {
        let mut app = App::new(Vec::new());
        app.active_session = Some(session("active", "/work/active", "2026-07-13T01:00:00Z"));

        app.scroll_detail_down(10);
        app.scroll_detail_up(3);
        assert_eq!(app.detail_scroll(), 7);

        app.scroll_detail_up(20);
        assert_eq!(app.detail_scroll(), 0);

        app.scroll_detail_down(5);
        app.scroll_detail_home();
        assert_eq!(app.detail_scroll(), 0);

        app.scroll_detail_down(5);
        app.close_active_session();
        assert!(app.active_session().is_none());
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn exposes_session_loading_errors() {
        let mut app = App::new(vec![session(
            "missing",
            "/work/missing",
            "2026-07-13T01:00:00Z",
        )]);
        let repository = SessionRepository::new(&[]).unwrap();

        app.open_selected(&repository);

        assert!(app.error().unwrap().starts_with(LOAD_SESSION_ERROR_PREFIX));
        assert!(app.active_session().is_none());
    }

    #[test]
    fn opens_the_selected_session() {
        let directory = std::env::temp_dir().join(format!(
            "his-tui-app-test-{}-{}",
            std::process::id(),
            NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("session.jsonl"),
            concat!(
                r#"{"type":"session","version":3,"id":"selected","timestamp":"2026-07-13T01:00:00Z","cwd":"/work/selected"}"#,
                "\n",
                r#"{"type":"message","id":"user","parentId":null,"timestamp":"2026-07-13T01:01:00Z","message":{"role":"user","content":[{"type":"text","text":"Hello"}],"timestamp":1}}"#,
            ),
        )
        .unwrap();
        let providers = [Provider {
            name: ProviderEnum::Pi,
            dir: directory.to_string_lossy().into_owned(),
        }];
        let repository = SessionRepository::new(&providers).unwrap();
        let mut app = App::new(vec![Session {
            provider: ProviderEnum::Pi,
            ..session("selected", "/work/selected", "2026-07-13T01:00:00Z")
        }]);

        app.open_selected(&repository);

        assert_eq!(app.active_session().unwrap().id, "selected");
        assert_eq!(app.error(), None);
        fs::remove_dir_all(directory).unwrap();
    }

    fn session(id: &str, cwd: &str, timestamp: &str) -> Session {
        Session {
            id: id.to_string(),
            provider: ProviderEnum::Codex,
            ts: timestamp.to_string(),
            cwd: cwd.to_string(),
            messages: None,
            first_message: format!("First message for {id}"),
        }
    }
}
