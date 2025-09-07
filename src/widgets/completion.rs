use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Style,
    text::{Line, Span, Text},
    widgets::Widget,
};

use crate::{config::Theme, model::VariableCompletion};

const DEFAULT_STYLE: Style = Style::new();

/// Widget to render a [`VariableCompletion`]
#[derive(Clone)]
pub struct VariableCompletionWidget<'a>(Text<'a>, Size);
impl<'a> VariableCompletionWidget<'a> {
    /// Builds a new [`VariableCompletionWidget`]
    pub fn new(
        completion: &'a VariableCompletion,
        theme: &Theme,
        is_highlighted: bool,
        is_discarded: bool,
        plain_style: bool,
        full_content: bool,
    ) -> Self {
        let mut line_style = DEFAULT_STYLE;
        if is_highlighted && let Some(bg_color) = theme.highlight {
            line_style = line_style.bg(bg_color.into());
        }
        // Determine the right styles to use based on highlighted and discarded status
        let (primary_style, secondary_style) = match (plain_style, is_discarded, is_highlighted) {
            // Discarded
            (_, true, false) => (theme.secondary, theme.secondary),
            // Discarded & highlighted
            (_, true, true) => (theme.highlight_secondary, theme.highlight_secondary),
            // Plain style, regular
            (true, false, false) => (theme.primary, theme.primary),
            // Plain style, highlighted
            (true, false, true) => (theme.highlight_primary, theme.highlight_primary),
            // Regular
            (false, false, false) => (theme.primary, theme.secondary),
            // Highlighted
            (false, false, true) => (theme.highlight_primary, theme.highlight_secondary),
        };

        // Setup the parts always present: variable and provider
        let mut parts = vec![
            Span::styled(&completion.variable, primary_style),
            Span::styled(": ", primary_style),
            Span::styled(&completion.suggestions_provider, secondary_style),
        ];

        // If the full content has to be rendered
        if full_content {
            // Include the prefix
            parts.insert(0, Span::styled("$ ", primary_style));
            // And the root command for non-global completions
            if !completion.is_global() {
                parts.insert(1, Span::styled("(", primary_style));
                parts.insert(2, Span::styled(&completion.root_cmd, primary_style));
                parts.insert(3, Span::styled(") ", primary_style));
            }
        }

        let text = Text::from(vec![Line::from(parts)]).style(line_style);
        let width = text.width() as u16;
        let height = text.height() as u16;
        VariableCompletionWidget(text, Size::new(width, height))
    }

    /// Retrieves the size of this widget
    pub fn size(&self) -> Size {
        self.1
    }
}

impl<'a> Widget for VariableCompletionWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.0.render(area, buf);
    }
}
