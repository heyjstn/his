use crate::agent::provider::ProviderEnum;
use crate::agent::session::{Session, SessionMessage};
use crate::renderer::render_markdown;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

const USER_ROLE: &str = "user";
const ASSISTANT_ROLE: &str = "assistant";
const CODEX_COMMENTARY_PHASE: &str = "commentary";
const COMMENTARY_BULLET: &str = "• ";
const EMPTY_SESSION_MESSAGE: &str = "No readable user or assistant messages in this session.";
const CODEX_COMMENTARY_FOREGROUND: Color = Color::Gray;

pub(super) fn render_header(frame: &mut Frame, session: &Session, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    provider_name(&session.provider),
                    Style::default()
                        .fg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(&session.cwd, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::styled(&session.ts, Style::default().fg(Color::DarkGray)),
        ]),
        area,
    );
}

pub(super) fn render_messages(
    frame: &mut Frame,
    messages: &[SessionMessage],
    scroll: u16,
    area: Rect,
) {
    frame.render_widget(
        Paragraph::new(session_message_lines(messages))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("up/down", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" scroll    "),
            Span::styled(
                "page up/down",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" faster    "),
            Span::styled("esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" back"),
        ])),
        area,
    );
}

fn session_message_lines(messages: &[SessionMessage]) -> Vec<Line<'_>> {
    if messages.is_empty() {
        return vec![Line::styled(
            EMPTY_SESSION_MESSAGE,
            Style::default().fg(Color::DarkGray),
        )];
    }

    let mut lines = Vec::new();
    let mut messages = messages.iter().peekable();
    while let Some(message) = messages.next() {
        lines.push(message_header(message));
        if is_codex_commentary(message) {
            lines.extend(commentary_lines(message));
            while let Some(commentary) = messages.next_if(|message| is_codex_commentary(message)) {
                lines.extend(commentary_lines(commentary));
            }
        } else {
            lines.extend(message_text_lines(message));
        }
        lines.push(Line::default());
    }

    lines
}

fn message_header(message: &SessionMessage) -> Line<'_> {
    let color = match message.role.as_str() {
        USER_ROLE => Color::LightCyan,
        ASSISTANT_ROLE => Color::LightGreen,
        _ => Color::Gray,
    };
    Line::from(vec![
        Span::styled(
            message.role.as_str(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(&message.ts, Style::default().fg(Color::DarkGray)),
    ])
}

fn is_codex_commentary(message: &SessionMessage) -> bool {
    message.provider == ProviderEnum::Codex
        && message.role == ASSISTANT_ROLE
        && message.phase.as_deref() == Some(CODEX_COMMENTARY_PHASE)
}

fn commentary_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    let mut rendered = message_text_lines(message);
    let Some(first_line) = rendered.first_mut() else {
        return vec![Line::styled(
            COMMENTARY_BULLET,
            codex_assistant_text_style(message),
        )];
    };
    first_line.spans.insert(0, Span::raw(COMMENTARY_BULLET));
    rendered
}

fn message_text_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    let style = codex_assistant_text_style(message);
    render_markdown(&message.text)
        .lines
        .into_iter()
        .map(|mut line| {
            line.style = style.patch(line.style);
            line
        })
        .collect()
}

fn codex_assistant_text_style(message: &SessionMessage) -> Style {
    if !is_codex_commentary(message) {
        return Style::default();
    }

    Style::default().fg(CODEX_COMMENTARY_FOREGROUND)
}

fn provider_name(provider: &ProviderEnum) -> &'static str {
    match provider {
        ProviderEnum::Codex => "Codex",
        ProviderEnum::Pi => "Pi",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ASSISTANT_ROLE, CODEX_COMMENTARY_FOREGROUND, EMPTY_SESSION_MESSAGE, session_message_lines,
    };
    use crate::agent::provider::ProviderEnum;
    use crate::agent::session::SessionMessage;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn renders_session_message_markdown() {
        let messages = [SessionMessage {
            id: "message-1".to_string(),
            provider: ProviderEnum::Codex,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: "assistant".to_string(),
            text: "A **bold** answer".to_string(),
            phase: Some("final_answer".to_string()),
        }];

        let lines = session_message_lines(&messages);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "assistant");
        assert_eq!(lines[1].spans[0].content, "A ");
        assert_eq!(lines[1].spans[1].content, "bold");
        assert!(lines[1].style.bg.is_none());
        assert!(
            lines[1].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[2].content, " answer");
    }

    #[test]
    fn renders_empty_session_message() {
        let lines = session_message_lines(&[]);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, EMPTY_SESSION_MESSAGE);
    }

    #[test]
    fn renders_continuous_codex_commentary_as_one_bulleted_assistant_message() {
        let messages = [
            SessionMessage {
                id: "commentary-1".to_string(),
                provider: ProviderEnum::Codex,
                ts: "2026-07-13T01:00:00Z".to_string(),
                role: "assistant".to_string(),
                text: "Inspecting the repository".to_string(),
                phase: Some("commentary".to_string()),
            },
            SessionMessage {
                id: "commentary-2".to_string(),
                provider: ProviderEnum::Codex,
                ts: "2026-07-13T01:01:00Z".to_string(),
                role: "assistant".to_string(),
                text: "Running the focused tests".to_string(),
                phase: Some("commentary".to_string()),
            },
        ];

        let lines = session_message_lines(&messages);
        let rendered_lines = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            rendered_lines
                .iter()
                .filter(|line| line.starts_with(ASSISTANT_ROLE))
                .count(),
            1
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line.contains("Inspecting the repository") && line.contains('•'))
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line.contains("Running the focused tests") && line.contains('•'))
        );
        assert_eq!(lines[1].style.fg, Some(CODEX_COMMENTARY_FOREGROUND));
        assert_eq!(lines[2].style.fg, Some(CODEX_COMMENTARY_FOREGROUND));
    }

    #[test]
    fn does_not_group_codex_commentary_across_message_boundaries() {
        let messages = [
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "First update",
            ),
            message(
                ProviderEnum::Codex,
                "assistant",
                "final_answer",
                "First answer",
            ),
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "Second update",
            ),
            message(ProviderEnum::Codex, "user", "", "Continue"),
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "Third update",
            ),
        ];

        let rendered_lines = rendered_lines(&messages);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 4);
        assert_eq!(role_header_count(&rendered_lines, "user"), 1);
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "First answer" && !line.contains('•'))
        );
    }

    #[test]
    fn keeps_pi_assistant_messages_separate() {
        let messages = [
            message(ProviderEnum::Pi, "assistant", "commentary", "First"),
            message(ProviderEnum::Pi, "assistant", "commentary", "Second"),
        ];

        let rendered_lines = rendered_lines(&messages);
        let lines = session_message_lines(&messages);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 2);
        assert!(rendered_lines.iter().all(|line| !line.contains('•')));
        assert!(lines.iter().all(|line| line.style.bg.is_none()));
    }

    #[test]
    fn does_not_apply_codex_assistant_styles_to_other_messages() {
        let messages = [
            message(ProviderEnum::Pi, "assistant", "final_answer", "Answer"),
            message(ProviderEnum::Codex, "user", "", "Question"),
        ];

        let lines = session_message_lines(&messages);

        for line in [&lines[1], &lines[4]] {
            assert!(line.style.fg.is_none());
            assert!(line.style.bg.is_none());
        }
    }

    #[test]
    fn preserves_markdown_colors_over_codex_message_styles() {
        let messages = [
            message(ProviderEnum::Codex, "assistant", "commentary", "## Update"),
            message(ProviderEnum::Codex, "assistant", "final_answer", "# Answer"),
        ];

        let lines = session_message_lines(&messages);

        assert_eq!(lines[1].style.fg, Some(Color::Cyan));
        assert_eq!(lines[4].style.bg, Some(Color::Cyan));
    }

    fn message(provider: ProviderEnum, role: &str, phase: &str, text: &str) -> SessionMessage {
        SessionMessage {
            id: format!("{role}-{phase}-{text}"),
            provider,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: role.to_string(),
            text: text.to_string(),
            phase: (!phase.is_empty()).then(|| phase.to_string()),
        }
    }

    fn rendered_lines(messages: &[SessionMessage]) -> Vec<String> {
        session_message_lines(messages)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    fn role_header_count(lines: &[String], role: &str) -> usize {
        lines.iter().filter(|line| line.starts_with(role)).count()
    }
}
