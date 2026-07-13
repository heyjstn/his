use crate::agent::provider::{COMMENTARY_PHASE, ProviderEnum, TOOL_CALL_PHASE};
use crate::agent::session::{Session, SessionMessage};
use crate::renderer::render_markdown;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

const USER_ROLE: &str = "user";
const ASSISTANT_ROLE: &str = "assistant";
const COMMENTARY_BULLET: &str = "• ";
const TOOL_CALL_MARKER: &str = "✳ ";
const CODE_FENCE: &str = "```";
const MIN_CODE_FENCE_LENGTH: usize = 3;
const EMPTY_SESSION_MESSAGE: &str = "No readable user or assistant messages in this session.";
const COMMENTARY_FOREGROUND: Color = Color::Gray;

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

fn is_commentary(message: &SessionMessage) -> bool {
    message.role == ASSISTANT_ROLE
        && matches!(
            message.phase.as_deref(),
            Some(COMMENTARY_PHASE | TOOL_CALL_PHASE)
        )
}

fn commentary_lines(message: &SessionMessage) -> Vec<Line<'_>> {
    if message.phase.as_deref() == Some(TOOL_CALL_PHASE) {
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
    let mut heading_spans = vec![Span::raw(TOOL_CALL_MARKER), Span::raw(&message.text)];
    if let Some(path) = &message.tool_path {
        heading_spans.push(Span::raw(" "));
        heading_spans.push(Span::styled(
            path,
            Style::default().add_modifier(Modifier::UNDERLINED),
        ));
    }

    let mut heading = Line::from(heading_spans);
    heading.style = style;
    let mut lines = vec![heading];
    for content in &message.tool_contents {
        lines.extend(fenced_edit_content_lines(content, style));
    }
    lines
}

fn fenced_edit_content_lines(content: &str, style: Style) -> Vec<Line<'static>> {
    let source_fence = source_code_fence(content);
    let markdown = format!("{source_fence}\n{content}\n{source_fence}");
    render_markdown(&markdown)
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
    render_markdown(&message.text)
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

fn provider_name(provider: &ProviderEnum) -> &'static str {
    match provider {
        ProviderEnum::Codex => "Codex",
        ProviderEnum::Pi => "Pi",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ASSISTANT_ROLE, CODE_FENCE, COMMENTARY_FOREGROUND, EMPTY_SESSION_MESSAGE,
        session_message_lines, source_code_fence,
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
            tool_path: None,
            tool_contents: Vec::new(),
        }];

        let lines = session_message_lines(&messages, false);

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
        let lines = session_message_lines(&[], false);

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
                tool_path: None,
                tool_contents: Vec::new(),
            },
            SessionMessage {
                id: "commentary-2".to_string(),
                provider: ProviderEnum::Codex,
                ts: "2026-07-13T01:01:00Z".to_string(),
                role: "assistant".to_string(),
                text: "Running the focused tests".to_string(),
                phase: Some("commentary".to_string()),
                tool_path: None,
                tool_contents: Vec::new(),
            },
        ];

        let lines = session_message_lines(&messages, true);
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
        assert_eq!(lines[1].style.fg, Some(COMMENTARY_FOREGROUND));
        assert_eq!(lines[2].style.fg, Some(COMMENTARY_FOREGROUND));
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

        let rendered_lines = rendered_lines(&messages, true);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 4);
        assert_eq!(role_header_count(&rendered_lines, "user"), 1);
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "First answer" && !line.contains('•'))
        );
    }

    #[test]
    fn renders_continuous_pi_thinking_as_one_bulleted_assistant_message() {
        let messages = [
            message(ProviderEnum::Pi, "assistant", "commentary", "First"),
            message(ProviderEnum::Pi, "assistant", "commentary", "Second"),
        ];

        let rendered_lines = rendered_lines(&messages, true);
        let lines = session_message_lines(&messages, true);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 1);
        assert_eq!(
            rendered_lines
                .iter()
                .filter(|line| line.contains('•'))
                .count(),
            2
        );
        assert_eq!(lines[1].style.fg, Some(COMMENTARY_FOREGROUND));
        assert_eq!(lines[2].style.fg, Some(COMMENTARY_FOREGROUND));
    }

    #[test]
    fn renders_edit_path_and_content_in_commentary() {
        const PI_PATH: &str = "/Users/triluu/dotfiles/nvim/lua/plugins/lsp.lua";
        let messages = [
            message(ProviderEnum::Codex, "assistant", "commentary", "Checking"),
            edit_message(
                ProviderEnum::Codex,
                "apply patch",
                None,
                &["*** Begin Patch\n+codex edit\n*** End Patch"],
            ),
            edit_message(
                ProviderEnum::Pi,
                "edit",
                Some(PI_PATH),
                &["local enabled = true\n", "", "return enabled"],
            ),
        ];

        let lines = session_message_lines(&messages, true);
        let rendered_lines = rendered_lines(&messages, true);

        assert_eq!(role_header_count(&rendered_lines, ASSISTANT_ROLE), 1);
        assert!(rendered_lines.iter().any(|line| line == "• Checking"));
        let codex_heading_index = rendered_lines
            .iter()
            .position(|line| line == "✳ apply patch")
            .unwrap();
        assert_eq!(
            &rendered_lines[codex_heading_index + 1..codex_heading_index + 6],
            [
                CODE_FENCE,
                "*** Begin Patch",
                "+codex edit",
                "*** End Patch",
                CODE_FENCE,
            ]
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == &format!("✳ edit {PI_PATH}"))
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "local enabled = true")
        );
        let pi_heading_index = rendered_lines
            .iter()
            .position(|line| line == &format!("✳ edit {PI_PATH}"))
            .unwrap();
        assert_eq!(
            &rendered_lines[pi_heading_index + 1..pi_heading_index + 5],
            [CODE_FENCE, "local enabled = true", "", CODE_FENCE]
        );
        assert_eq!(
            &rendered_lines[pi_heading_index + 5..pi_heading_index + 8],
            [CODE_FENCE, "", CODE_FENCE]
        );
        assert_eq!(
            &rendered_lines[pi_heading_index + 8..pi_heading_index + 11],
            [CODE_FENCE, "return enabled", CODE_FENCE]
        );

        let path_span = lines
            .iter()
            .flat_map(|line| &line.spans)
            .find(|span| span.content == PI_PATH)
            .unwrap();
        assert!(path_span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn uses_a_source_fence_longer_than_edit_content_backticks() {
        assert_eq!(source_code_fence("plain content"), CODE_FENCE);
        assert_eq!(source_code_fence("before ``` after"), "````");
        assert_eq!(source_code_fence("````\n```"), "`````");
    }

    #[test]
    fn does_not_apply_codex_assistant_styles_to_other_messages() {
        let messages = [
            message(ProviderEnum::Pi, "assistant", "final_answer", "Answer"),
            message(ProviderEnum::Codex, "user", "", "Question"),
        ];

        let lines = session_message_lines(&messages, true);

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

        let lines = session_message_lines(&messages, true);

        assert_eq!(lines[1].style.fg, Some(Color::Cyan));
        assert_eq!(lines[4].style.bg, Some(Color::Cyan));
    }

    #[test]
    fn hides_commentary_when_visibility_is_disabled() {
        let messages = [
            message(
                ProviderEnum::Codex,
                "assistant",
                "commentary",
                "Hidden update",
            ),
            message(
                ProviderEnum::Codex,
                "assistant",
                "final_answer",
                "Visible answer",
            ),
            message(
                ProviderEnum::Pi,
                "assistant",
                "tool_call",
                "Hidden tool call",
            ),
        ];

        let visible_lines = rendered_lines(&messages, false);

        assert!(
            !visible_lines
                .iter()
                .any(|line| line.contains("Hidden update"))
        );
        assert!(visible_lines.iter().any(|line| line == "Visible answer"));
        assert!(
            !visible_lines
                .iter()
                .any(|line| line.contains("Hidden tool call"))
        );

        let commentary_only = [message(
            ProviderEnum::Codex,
            "assistant",
            "commentary",
            "Hidden update",
        )];
        let visible_lines = rendered_lines(&commentary_only, false);
        assert_eq!(visible_lines, [EMPTY_SESSION_MESSAGE]);
    }

    fn message(provider: ProviderEnum, role: &str, phase: &str, text: &str) -> SessionMessage {
        SessionMessage {
            id: format!("{role}-{phase}-{text}"),
            provider,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: role.to_string(),
            text: text.to_string(),
            phase: (!phase.is_empty()).then(|| phase.to_string()),
            tool_path: None,
            tool_contents: Vec::new(),
        }
    }

    fn edit_message(
        provider: ProviderEnum,
        text: &str,
        tool_path: Option<&str>,
        tool_contents: &[&str],
    ) -> SessionMessage {
        SessionMessage {
            id: format!("edit-{text}"),
            provider,
            ts: "2026-07-13T01:00:00Z".to_string(),
            role: "assistant".to_string(),
            text: text.to_string(),
            phase: Some("tool_call".to_string()),
            tool_path: tool_path.map(str::to_string),
            tool_contents: tool_contents
                .iter()
                .map(|content| (*content).to_string())
                .collect(),
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

    fn role_header_count(lines: &[String], role: &str) -> usize {
        lines.iter().filter(|line| line.starts_with(role)).count()
    }
}
