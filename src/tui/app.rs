use crate::session::{SessionDetail, SessionSummary};

const LOAD_SESSION_ERROR_PREFIX: &str = "Unable to load session";

pub(super) struct App {
    sessions: Vec<SessionSummary>,
    selected: usize,
    search: String,
    active_session: Option<SessionDetail>,
    detail_scroll: u16,
    commentary_visible: bool,
    notice: Option<String>,
}

impl App {
    pub(super) fn new(sessions: Vec<SessionSummary>, notice: Option<String>) -> Self {
        Self {
            sessions,
            selected: 0,
            search: String::new(),
            active_session: None,
            detail_scroll: 0,
            commentary_visible: false,
            notice,
        }
    }

    pub(super) fn visible_sessions(&self) -> Vec<&SessionSummary> {
        if self.search.is_empty() {
            return self.sessions.iter().collect();
        }

        let search = self.search.to_lowercase();
        self.sessions
            .iter()
            .filter(|session| {
                session
                    .cwd
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&search)
            })
            .collect()
    }

    pub(super) fn selected_session(&self) -> Option<&SessionSummary> {
        self.visible_sessions().get(self.selected).copied()
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn search(&self) -> &str {
        &self.search
    }

    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub(super) fn active_session(&self) -> Option<&SessionDetail> {
        self.active_session.as_ref()
    }

    pub(super) fn detail_scroll(&self) -> u16 {
        self.detail_scroll
    }

    pub(super) fn commentary_visible(&self) -> bool {
        self.commentary_visible
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

    pub(super) fn show_session(&mut self, session: SessionDetail) {
        self.active_session = Some(session);
        self.detail_scroll = 0;
        self.commentary_visible = false;
        self.notice = None;
    }

    pub(super) fn show_load_error(&mut self, error: &anyhow::Error) {
        self.notice = Some(format!("{LOAD_SESSION_ERROR_PREFIX}: {error:#}"));
    }

    pub(super) fn close_active_session(&mut self) {
        self.active_session = None;
        self.detail_scroll = 0;
        self.commentary_visible = false;
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

    pub(super) fn toggle_commentary_visibility(&mut self) {
        self.commentary_visible = !self.commentary_visible;
        self.detail_scroll = 0;
    }

    fn reset_search_state(&mut self) {
        self.selected = 0;
        self.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{App, LOAD_SESSION_ERROR_PREFIX};
    use crate::agent::AgentKind;
    use crate::session::{SessionDetail, SessionLocator, SessionSummary, SessionTimestamp};
    use anyhow::anyhow;
    use std::path::PathBuf;

    #[test]
    fn preserves_repository_order() {
        let app = App::new(
            vec![
                summary("newer", "/work/newer", "2026-07-13T01:00:00Z"),
                summary("older", "/work/older", "2026-07-12T01:00:00Z"),
            ],
            None,
        );

        let visible = app.visible_sessions();

        assert_eq!(visible[0].id, "newer");
        assert_eq!(visible[1].id, "older");
    }

    #[test]
    fn filters_sessions_case_insensitively_and_resets_selection() {
        let mut app = App::new(
            vec![
                summary("frontend", "/work/Frontend", "2026-07-13T01:00:00Z"),
                summary("backend", "/work/backend", "2026-07-12T01:00:00Z"),
            ],
            Some("previous warning".to_string()),
        );
        app.select_next();

        for character in "FRONT".chars() {
            app.append_search(character);
        }

        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "frontend");
        assert_eq!(app.selected(), 0);
        assert_eq!(app.notice(), None);

        app.remove_search_character();
        assert_eq!(app.search(), "FRON");
    }

    #[test]
    fn keeps_selection_within_visible_sessions() {
        let mut app = App::new(
            vec![
                summary("first", "/work/first", "2026-07-13T01:00:00Z"),
                summary("second", "/work/second", "2026-07-12T01:00:00Z"),
            ],
            None,
        );

        app.select_next();
        app.select_next();
        assert_eq!(app.selected(), 1);

        app.select_previous();
        app.select_previous();
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn manages_detail_state_without_repository_access() {
        let mut app = App::new(Vec::new(), None);
        app.show_session(detail("active"));

        app.scroll_detail_down(10);
        app.scroll_detail_up(3);
        assert_eq!(app.detail_scroll(), 7);

        app.toggle_commentary_visibility();
        assert!(app.commentary_visible());
        assert_eq!(app.detail_scroll(), 0);

        app.close_active_session();
        assert!(app.active_session().is_none());
        assert_eq!(app.detail_scroll(), 0);
        assert!(!app.commentary_visible());
    }

    #[test]
    fn exposes_session_loading_errors() {
        let mut app = App::new(Vec::new(), None);

        app.show_load_error(&anyhow!("not found"));

        assert!(app.notice().unwrap().starts_with(LOAD_SESSION_ERROR_PREFIX));
        assert!(app.active_session().is_none());
    }

    fn summary(id: &str, cwd: &str, timestamp: &str) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new(timestamp),
            cwd: PathBuf::from(cwd),
            first_message: format!("First message for {id}"),
            locator: SessionLocator::new(PathBuf::from(format!("/sessions/{id}.jsonl"))),
        }
    }

    fn detail(id: &str) -> SessionDetail {
        SessionDetail {
            agent: AgentKind::Codex,
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            cwd: PathBuf::from(format!("/work/{id}")),
            messages: Vec::new(),
        }
    }
}
