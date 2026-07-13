mod codex;
mod pi;

use crate::session::{MessageRole, SessionMessage, SessionTimestamp};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use codex::CodexRecord;
use pi::PiRecord;

const EMPTY_SESSION_MESSAGE: &str = "(no text messages)";

#[derive(Clone, Copy, Deserialize, Debug, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentKind {
    Codex,
    Pi,
}

impl AgentKind {
    pub(crate) fn parse_summary(self, path: &Path) -> Result<ParsedSessionSummary> {
        match self {
            Self::Codex => CodexRecord::parse_summary(path),
            Self::Pi => PiRecord::parse_summary(path),
        }
    }

    pub(crate) fn parse_detail(self, path: &Path) -> Result<ParsedSession> {
        match self {
            Self::Codex => CodexRecord::parse_session(path),
            Self::Pi => PiRecord::parse_session(path),
        }
    }
}

impl fmt::Display for AgentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codex => formatter.write_str("Codex"),
            Self::Pi => formatter.write_str("Pi"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ParsedSessionSummary {
    pub(crate) id: String,
    pub(crate) timestamp: SessionTimestamp,
    pub(crate) cwd: PathBuf,
    pub(crate) first_message: String,
}

#[derive(Debug)]
pub(crate) struct ParsedSession {
    pub(crate) id: String,
    pub(crate) timestamp: SessionTimestamp,
    pub(crate) cwd: PathBuf,
    pub(crate) messages: Vec<SessionMessage>,
}

#[derive(Debug)]
pub(crate) enum ParsedRecord {
    Session {
        id: String,
        timestamp: SessionTimestamp,
        cwd: Option<PathBuf>,
    },
    Message(SessionMessage),
    Ignored,
}

pub(crate) trait AgentRecord: Sized + DeserializeOwned {
    fn into_records(self) -> Vec<ParsedRecord>
    where
        ParsedRecord: From<Self>,
    {
        vec![self.into()]
    }

    fn reader(path: &Path) -> Result<BufReader<File>> {
        File::open(path)
            .map(BufReader::new)
            .with_context(|| format!("failed to read session file {}", path.display()))
    }

    fn parse_records(path: &Path) -> Result<Vec<ParsedRecord>>
    where
        ParsedRecord: From<Self>,
    {
        let reader = Self::reader(path)?;
        let mut records = Vec::new();
        for record in serde_json::Deserializer::from_reader(reader).into_iter::<Self>() {
            let record = record
                .with_context(|| format!("failed to parse session file {}", path.display()))?;
            records.extend(record.into_records());
        }
        Ok(records)
    }

    fn parse_summary(path: &Path) -> Result<ParsedSessionSummary>
    where
        ParsedRecord: From<Self>,
    {
        let reader = Self::reader(path)?;
        let mut metadata = None;
        let mut first_message = None;
        let mut first_user_message = None;

        for record in serde_json::Deserializer::from_reader(reader).into_iter::<Self>() {
            let record = record
                .with_context(|| format!("failed to parse session file {}", path.display()))?;
            for record in record.into_records() {
                match record {
                    ParsedRecord::Session { id, timestamp, cwd } if metadata.is_none() => {
                        metadata = Some((id, timestamp, cwd));
                    }
                    ParsedRecord::Message(message) => {
                        if first_message.is_none() {
                            first_message = Some(message.text.clone());
                        }
                        if message.role == MessageRole::User && first_user_message.is_none() {
                            first_user_message = Some(message.text);
                        }
                    }
                    ParsedRecord::Session { .. } | ParsedRecord::Ignored => {}
                }
            }

            if metadata.is_some() && first_user_message.is_some() {
                break;
            }
        }

        let (id, timestamp, cwd) =
            metadata.with_context(|| format!("missing session metadata in {}", path.display()))?;
        Ok(ParsedSessionSummary {
            id,
            timestamp,
            cwd: cwd.with_context(|| format!("missing cwd in {}", path.display()))?,
            first_message: first_user_message
                .or(first_message)
                .unwrap_or_else(|| EMPTY_SESSION_MESSAGE.to_string()),
        })
    }

    fn parse_session(path: &Path) -> Result<ParsedSession>
    where
        ParsedRecord: From<Self>,
    {
        parsed_session(Self::parse_records(path)?, path)
    }
}

pub(crate) fn parsed_session(records: Vec<ParsedRecord>, path: &Path) -> Result<ParsedSession> {
    let mut metadata = None;
    let mut messages = Vec::new();

    for record in records {
        match record {
            ParsedRecord::Session { id, timestamp, cwd } if metadata.is_none() => {
                metadata = Some((id, timestamp, cwd));
            }
            ParsedRecord::Message(message) => messages.push(message),
            ParsedRecord::Session { .. } | ParsedRecord::Ignored => {}
        }
    }

    let (id, timestamp, cwd) =
        metadata.with_context(|| format!("missing session metadata in {}", path.display()))?;
    Ok(ParsedSession {
        id,
        timestamp,
        cwd: cwd.with_context(|| format!("missing cwd in {}", path.display()))?,
        messages,
    })
}

#[cfg(test)]
mod tests {
    use super::AgentKind;

    #[test]
    fn formats_agent_names_for_display() {
        assert_eq!(AgentKind::Codex.to_string(), "Codex");
        assert_eq!(AgentKind::Pi.to_string(), "Pi");
    }
}
