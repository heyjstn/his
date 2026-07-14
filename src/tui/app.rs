use super::session_list::SessionListState;
use crate::session::{SessionDetail, SessionSummary};
use std::time::Duration;

const LOAD_SESSION_ERROR_PREFIX: &str = "Unable to load session";

pub(super) struct App {
    sessions: Vec<SessionSummary>,
    visible_session_indices: Vec<usize>,
    session_list_state: SessionListState,
    search: String,
    active_session: Option<SessionDetail>,
    detail_scroll: u16,
    commentary_visible: bool,
    notice: Option<String>,
}

impl App {
    pub(super) fn new(sessions: Vec<SessionSummary>, notice: Option<String>) -> Self {
        let visible_session_indices = (0..sessions.len()).collect();
        let selected = (!sessions.is_empty()).then_some(0);
        Self {
            sessions,
            visible_session_indices,
            session_list_state: SessionListState::new(selected),
            search: String::new(),
            active_session: None,
            detail_scroll: 0,
            commentary_visible: false,
            notice,
        }
    }

    #[cfg(test)]
    pub(super) fn visible_sessions(
        &self,
    ) -> impl ExactSizeIterator<Item = &SessionSummary> + Clone {
        self.visible_session_indices
            .iter()
            .map(|index| &self.sessions[*index])
    }

    pub(super) fn session_list(
        &mut self,
    ) -> (
        impl ExactSizeIterator<Item = &SessionSummary> + Clone,
        &mut SessionListState,
    ) {
        let sessions = &self.sessions;
        let visible_session_indices = &self.visible_session_indices;
        let state = &mut self.session_list_state;
        let visible_sessions = visible_session_indices
            .iter()
            .map(move |index| &sessions[*index]);
        (visible_sessions, state)
    }

    pub(super) fn selected_session(&self) -> Option<&SessionSummary> {
        let selected = self.session_list_state.selected()?;
        let session_index = *self.visible_session_indices.get(selected)?;
        self.sessions.get(session_index)
    }

    #[cfg(test)]
    pub(super) fn selected(&self) -> usize {
        self.session_list_state.selected().unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn session_list_cache_builds(&self) -> usize {
        self.session_list_state.cache_builds()
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

    pub(super) fn session_list_refresh_timeout(&self) -> Option<Duration> {
        if self.active_session.is_some() {
            return None;
        }

        self.session_list_state.refresh_timeout()
    }

    pub(super) fn append_search(&mut self, character: char) -> bool {
        self.search.push(character);
        self.reset_search_state();
        true
    }

    pub(super) fn remove_search_character(&mut self) -> bool {
        let search_changed = self.search.pop().is_some();
        let selection_changed = self
            .session_list_state
            .selected()
            .is_some_and(|index| index > 0);
        if !search_changed && !selection_changed && self.notice.is_none() {
            return false;
        }

        self.reset_search_state();
        true
    }

    pub(super) fn select_previous(&mut self) -> bool {
        let Some(selected) = self.session_list_state.selected() else {
            return false;
        };
        let previous = selected.saturating_sub(1);
        if previous == selected {
            return false;
        }

        self.session_list_state.select(Some(previous));
        true
    }

    pub(super) fn select_next(&mut self) -> bool {
        let Some(selected) = self.session_list_state.selected() else {
            return false;
        };
        let next = selected.saturating_add(1);
        if next >= self.visible_session_indices.len() {
            return false;
        }

        self.session_list_state.select(Some(next));
        true
    }

    pub(super) fn show_session(&mut self, session: SessionDetail) {
        self.active_session = Some(session);
        self.detail_scroll = 0;
        self.commentary_visible = false;
        self.notice = None;
    }

    pub(super) fn show_load_error(&mut self, error: &anyhow::Error) -> bool {
        let notice = format!("{LOAD_SESSION_ERROR_PREFIX}: {error:#}");
        if self.notice.as_deref() == Some(notice.as_str()) {
            return false;
        }

        self.notice = Some(notice);
        true
    }

    pub(super) fn close_active_session(&mut self) -> bool {
        if self.active_session.is_none() {
            return false;
        }

        self.active_session = None;
        self.detail_scroll = 0;
        self.commentary_visible = false;
        true
    }

    pub(super) fn scroll_detail_up(&mut self, rows: u16) -> bool {
        let scroll = self.detail_scroll.saturating_sub(rows);
        if scroll == self.detail_scroll {
            return false;
        }

        self.detail_scroll = scroll;
        true
    }

    pub(super) fn scroll_detail_down(&mut self, rows: u16) -> bool {
        let scroll = self.detail_scroll.saturating_add(rows);
        if scroll == self.detail_scroll {
            return false;
        }

        self.detail_scroll = scroll;
        true
    }

    pub(super) fn scroll_detail_home(&mut self) -> bool {
        if self.detail_scroll == 0 {
            return false;
        }

        self.detail_scroll = 0;
        true
    }

    pub(super) fn toggle_commentary_visibility(&mut self) -> bool {
        self.commentary_visible = !self.commentary_visible;
        self.detail_scroll = 0;
        true
    }

    fn reset_search_state(&mut self) {
        let search = self.search.to_lowercase();
        self.visible_session_indices = if search.is_empty() {
            (0..self.sessions.len()).collect()
        } else {
            self.sessions
                .iter()
                .enumerate()
                .filter_map(|(index, session)| {
                    session
                        .first_message
                        .to_lowercase()
                        .contains(&search)
                        .then_some(index)
                })
                .collect()
        };
        let selected = (!self.visible_session_indices.is_empty()).then_some(0);
        self.session_list_state.reset(selected);
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

        let visible = app.visible_sessions().collect::<Vec<_>>();

        assert_eq!(visible[0].id, "newer");
        assert_eq!(visible[1].id, "older");
    }

    #[test]
    fn filters_sessions_by_first_message_case_insensitively_and_resets_selection() {
        let mut cwd_match = summary("cwd-match", "/work/Needle", "2026-07-13T01:00:00Z");
        cwd_match.first_message = "Unrelated task".to_string();
        let mut message_match = summary("message-match", "/work/other", "2026-07-12T01:00:00Z");
        message_match.first_message = "Find NEEDLE in the first message".to_string();
        let mut app = App::new(
            vec![cwd_match, message_match],
            Some("previous warning".to_string()),
        );
        app.select_next();

        for character in "needle".chars() {
            app.append_search(character);
        }

        let visible = app.visible_sessions().collect::<Vec<_>>();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "message-match");
        assert_eq!(app.selected(), 0);
        assert!(!app.select_next());
        assert_eq!(app.selected_session().unwrap().id, "message-match");
        assert_eq!(app.notice(), None);

        app.remove_search_character();
        assert_eq!(app.search(), "needl");
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

        assert!(app.select_next());
        assert!(!app.select_next());
        assert_eq!(app.selected(), 1);

        assert!(app.select_previous());
        assert!(!app.select_previous());
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
        let error = anyhow!("not found");

        assert!(app.show_load_error(&error));
        assert!(!app.show_load_error(&error));

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
