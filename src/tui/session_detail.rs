use super::markdown;
use crate::session::{MessagePhase, MessageRole, SessionDetail, SessionMessage};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use std::path::Path;

const COMMENTARY_BULLET: &str = "• ";
const TOOL_CALL_MARKER: &str = "✳ ";
const CODE_FENCE: &str = "```";
const MIN_CODE_FENCE_LENGTH: usize = 3;
const EMPTY_SESSION_MESSAGE: &str = "No readable user or assistant messages in this session.";
const COMMENTARY_FOREGROUND: Color = Color::Gray;

pub(super) fn render_header(frame: &mut Frame, session: &SessionDetail, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    session.agent.to_string(),
                    Style::default()
                        .fg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    session.cwd.to_string_lossy(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::styled(
                session.timestamp.as_str(),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        area,
    );
}

pub(super) fn render_messages(
    frame: &mut Frame,
    messages: &[SessionMessage],
    commentary_visible: bool,
    scroll: u16,
    area: Rect,
) {
    frame.render_widget(
        Paragraph::new(session_message_lines(messages, commentary_visible))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

pub(super) fn render_footer(frame: &mut Frame, commentary_visible: bool, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("up/down", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" scroll    "),
            Span::styled(
                "page up/down",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" faster    "),
            Span::styled("ctrl+o", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(if commentary_visible {
                " hide commentary    "
            } else {
                " show commentary    "
            }),
            Span::styled("esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" back"),
        ])),
        area,
    );
}

fn session_message_lines(messages: &[SessionMessage], commentary_visible: bool) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut messages = messages
        .iter()
        .filter(|message| commentary_visible || !is_commentary(message))
        .peekable();
    while let Some(message) = messages.next() {
        lines.push(message_header(message));
        if is_commentary(message) {
            lines.extend(commentary_lines(message));
            while let Some(commentary) = messages.next_if(|message| is_commentary(message)) {
                lines.extend(commentary_lines(commentary));
            }
        } else {
            lines.extend(message_text_lines(message));
        }
        lines.push(Line::default());
    }

    if !lines.is_empty() {
        return lines;
    }

    vec![Line::styled(
        EMPTY_SESSION_MESSAGE,
        Style::default().fg(Color::DarkGray),
    )]
}

fn message_header(message: &SessionMessage) -> Line<'_> {
    let color = match &message.role {
        MessageRole::User => Color::LightCyan,
        MessageRole::Assistant => Color::LightGreen,
    };
    Line::from(vec![
        Span::styled(
            message.role.as_str(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            message.timestamp.as_str(),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn is_commentary(message: &SessionMessage) -> bool {
    message.role == MessageRole::Assistant
        && message
            .phase
            .as_ref()
            .is_some_and(MessagePhase::is_commentary)
}

fn commentary_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    if message.phase == Some(MessagePhase::ToolCall) {
        return edit_tool_lines(message);
    }

    let mut rendered = message_text_lines(message);
    let Some(first_line) = rendered.first_mut() else {
        return vec![Line::styled(
            COMMENTARY_BULLET,
            assistant_text_style(message),
        )];
    };
    first_line.spans.insert(0, Span::raw(COMMENTARY_BULLET));
    rendered
}

fn edit_tool_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    let style = assistant_text_style(message);
    let language = edit_language(message.tool_path.as_deref());
    let mut heading_spans = vec![Span::raw(TOOL_CALL_MARKER), Span::raw(&message.text)];
    if let Some(path) = &message.tool_path {
        heading_spans.push(Span::raw(" "));
        heading_spans.push(Span::styled(
            path.to_string_lossy(),
            Style::default().add_modifier(Modifier::UNDERLINED),
        ));
    }

    let mut heading = Line::from(heading_spans);
    heading.style = style;
    let mut lines = vec![heading];
    for content in &message.tool_contents {
        lines.extend(fenced_edit_content_lines(
            content,
            language.as_deref(),
            style,
        ));
    }
    lines
}

fn fenced_edit_content_lines(
    content: &str,
    language: Option<&str>,
    style: Style,
) -> Vec<Line<'static>> {
    let source_fence = source_code_fence(content);
    let language = language.unwrap_or_default();
    let markdown = format!("{source_fence}{language}\n{content}\n{source_fence}");
    markdown::render(&markdown)
        .lines
        .into_iter()
        .map(|line| Line {
            style: style.patch(line.style),
            alignment: line.alignment,
            spans: line
                .spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect(),
        })
        .collect()
}

fn edit_language(path: Option<&Path>) -> Option<String> {
    let extension = path?.extension()?.to_str()?;
    let is_safe = !extension.is_empty()
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+-_#".contains(character));
    is_safe.then(|| extension.to_ascii_lowercase())
}

fn source_code_fence(content: &str) -> String {
    let longest_backtick_run = content
        .as_bytes()
        .split(|byte| *byte != b'`')
        .map(<[u8]>::len)
        .max()
        .unwrap_or_default();
    if longest_backtick_run < MIN_CODE_FENCE_LENGTH {
        return CODE_FENCE.to_string();
    }

    "`".repeat(longest_backtick_run.saturating_add(1))
}

fn message_text_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    let style = assistant_text_style(message);
    markdown::render(&message.text)
        .lines
        .into_iter()
        .map(|mut line| {
            line.style = style.patch(line.style);
            line
        })
        .collect()
}

fn assistant_text_style(message: &SessionMessage) -> Style {
    if !is_commentary(message) {
        return Style::default();
    }

    Style::default().fg(COMMENTARY_FOREGROUND)
}

#[cfg(test)]
mod tests {
    use super::{
        CODE_FENCE, COMMENTARY_FOREGROUND, EMPTY_SESSION_MESSAGE, edit_language,
        session_message_lines, source_code_fence,
    };
    use crate::session::{MessagePhase, MessageRole, SessionMessage, SessionTimestamp};
    use ratatui::style::Modifier;
    use std::path::{Path, PathBuf};

    const ASSISTANT_ROLE: &str = "assistant";

    #[test]
    fn renders_markdown_and_empty_sessions() {
        let messages = [message(
            MessageRole::Assistant,
            Some(MessagePhase::FinalAnswer),
            "A **bold** answer",
        )];
        let lines = session_message_lines(&messages, false);

        assert_eq!(lines[0].spans[0].content, ASSISTANT_ROLE);
        assert_eq!(lines[1].spans[1].content, "bold");
        assert!(
            lines[1].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        let empty = session_message_lines(&[], false);
        assert_eq!(empty[0].spans[0].content, EMPTY_SESSION_MESSAGE);
    }

    #[test]
    fn groups_continuous_commentary_under_one_header() {
        let messages = [
            message(
                MessageRole::Assistant,
                Some(MessagePhase::Commentary),
                "First update",
            ),
            message(
                MessageRole::Assistant,
                Some(MessagePhase::Commentary),
                "Second update",
            ),
        ];

        let lines = session_message_lines(&messages, true);
        let rendered = rendered_lines(&messages, true);

        assert_eq!(
            rendered
                .iter()
                .filter(|line| line.starts_with(ASSISTANT_ROLE))
                .count(),
            1
        );
        assert!(rendered.iter().any(|line| line == "• First update"));
        assert!(rendered.iter().any(|line| line == "• Second update"));
        assert_eq!(lines[1].style.fg, Some(COMMENTARY_FOREGROUND));
    }

    #[test]
    fn does_not_group_commentary_across_other_messages() {
        let messages = [
            message(
                MessageRole::Assistant,
                Some(MessagePhase::Commentary),
                "First update",
            ),
            message(MessageRole::User, None, "Continue"),
            message(
                MessageRole::Assistant,
                Some(MessagePhase::Commentary),
                "Second update",
            ),
        ];

        let rendered = rendered_lines(&messages, true);

        assert_eq!(
            rendered
                .iter()
                .filter(|line| line.starts_with(ASSISTANT_ROLE))
                .count(),
            2
        );
    }

    #[test]
    fn renders_edit_paths_and_fenced_content() {
        let path = PathBuf::from("/tmp/plugin.lua");
        let mut edit = message(MessageRole::Assistant, Some(MessagePhase::ToolCall), "edit");
        edit.tool_path = Some(path.clone());
        edit.tool_contents = vec!["local enabled = true".to_string()];

        let messages = [edit];
        let lines = session_message_lines(&messages, true);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert!(rendered.iter().any(|line| line == "✳ edit /tmp/plugin.lua"));
        assert!(rendered.iter().any(|line| line == "```lua"));
        let path_span = lines
            .iter()
            .flat_map(|line| &line.spans)
            .find(|span| span.content == path.to_string_lossy())
            .unwrap();
        assert!(path_span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn detects_safe_edit_languages_and_fences() {
        assert_eq!(
            edit_language(Some(Path::new("/tmp/plugin.lua"))).as_deref(),
            Some("lua")
        );
        assert_eq!(
            edit_language(Some(Path::new("/tmp/main.RS"))).as_deref(),
            Some("rs")
        );
        assert_eq!(edit_language(Some(Path::new("/tmp/Makefile"))), None);
        assert_eq!(
            edit_language(Some(Path::new("/tmp/file.bad language"))),
            None
        );
        assert_eq!(source_code_fence("plain content"), CODE_FENCE);
        assert_eq!(source_code_fence("before ``` after"), "````");
    }

    #[test]
    fn hides_commentary_when_visibility_is_disabled() {
        let messages = [
            message(
                MessageRole::Assistant,
                Some(MessagePhase::Commentary),
                "Hidden update",
            ),
            message(
                MessageRole::Assistant,
                Some(MessagePhase::FinalAnswer),
                "Visible answer",
            ),
        ];

        let visible = rendered_lines(&messages, false);

        assert!(!visible.iter().any(|line| line.contains("Hidden update")));
        assert!(visible.iter().any(|line| line == "Visible answer"));
    }

    fn message(role: MessageRole, phase: Option<MessagePhase>, text: &str) -> SessionMessage {
        SessionMessage {
            timestamp: SessionTimestamp::new("2026-07-13T01:00:00Z"),
            role,
            text: text.to_string(),
            phase,
            tool_path: None,
            tool_contents: Vec::new(),
        }
    }

    fn rendered_lines(messages: &[SessionMessage], commentary_visible: bool) -> Vec<String> {
        session_message_lines(messages, commentary_visible)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }
}
