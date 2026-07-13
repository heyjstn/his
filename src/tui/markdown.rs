use ratatui::text::Text;

pub(super) fn render(markdown: &str) -> Text<'_> {
    tui_markdown::from_str(markdown)
}

#[cfg(test)]
mod tests {
    use super::render;
    use ratatui::style::Modifier;

    #[test]
    fn renders_markdown_as_styled_terminal_text() {
        let text = render("# Agent response\n\nUse **cargo test**.");

        assert_eq!(text.lines.len(), 3);
        assert_eq!(text.lines[0].spans[0].content, "# ");
        assert_eq!(text.lines[0].spans[1].content, "Agent response");
        assert!(text.lines[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(text.lines[2].spans[0].content, "Use ");
        assert_eq!(text.lines[2].spans[1].content, "cargo test");
        assert!(
            text.lines[2].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(text.lines[2].spans[2].content, ".");
    }
}
