use crate::agent::AgentKind;
use chrono::{DateTime, FixedOffset};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

const COMMENTARY_PHASE: &str = "commentary";
const FINAL_ANSWER_PHASE: &str = "final_answer";
const TOOL_CALL_PHASE: &str = "tool_call";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionTimestamp {
    raw: String,
    parsed: Option<DateTime<FixedOffset>>,
}

impl SessionTimestamp {
    pub(crate) fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let parsed = DateTime::parse_from_rfc3339(&raw).ok();
        Self { raw, parsed }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.raw
    }
}

impl Ord for SessionTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.parsed, &other.parsed) {
            (Some(left), Some(right)) => left.cmp(right).then_with(|| self.raw.cmp(&other.raw)),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => self.raw.cmp(&other.raw),
        }
    }
}

impl PartialOrd for SessionTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionLocator {
    path: PathBuf,
}

impl SessionLocator {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SessionSummary {
    pub(crate) id: String,
    pub(crate) agent: AgentKind,
    pub(crate) timestamp: SessionTimestamp,
    pub(crate) cwd: PathBuf,
    pub(crate) first_message: String,
    pub(crate) locator: SessionLocator,
}

#[derive(Debug)]
pub(crate) struct SessionDetail {
    pub(crate) agent: AgentKind,
    pub(crate) timestamp: SessionTimestamp,
    pub(crate) cwd: PathBuf,
    pub(crate) messages: Vec<SessionMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MessageRole {
    User,
    Assistant,
}

impl MessageRole {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MessagePhase {
    Commentary,
    ToolCall,
    FinalAnswer,
    Other(String),
}

impl MessagePhase {
    pub(crate) fn from_provider(value: &str) -> Self {
        match value {
            COMMENTARY_PHASE => Self::Commentary,
            TOOL_CALL_PHASE => Self::ToolCall,
            FINAL_ANSWER_PHASE => Self::FinalAnswer,
            value => Self::Other(value.to_string()),
        }
    }

    pub(crate) fn is_commentary(&self) -> bool {
        matches!(self, Self::Commentary | Self::ToolCall)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SessionMessage {
    pub(crate) timestamp: SessionTimestamp,
    pub(crate) role: MessageRole,
    pub(crate) text: String,
    pub(crate) phase: Option<MessagePhase>,
    pub(crate) tool_path: Option<PathBuf>,
    pub(crate) tool_contents: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{MessagePhase, SessionTimestamp};

    #[test]
    fn orders_valid_timestamps_chronologically() {
        let earlier = SessionTimestamp::new("2026-07-13T08:00:00+07:00");
        let later = SessionTimestamp::new("2026-07-13T02:00:00Z");

        assert!(earlier < later);
    }

    #[test]
    fn orders_invalid_timestamps_deterministically_before_valid_ones() {
        let invalid = SessionTimestamp::new("invalid");
        let valid = SessionTimestamp::new("2026-07-13T02:00:00Z");

        assert!(invalid < valid);
        assert_eq!(invalid.as_str(), "invalid");
    }

    #[test]
    fn preserves_unknown_phases() {
        assert_eq!(
            MessagePhase::from_provider("analysis"),
            MessagePhase::Other("analysis".to_string())
        );
    }
}
