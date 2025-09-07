use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Style,
    text::{Line, Span, Text},
    widgets::Widget,
};

use crate::{
    config::Theme,
    model::Command,
    utils::{COMMAND_VARIABLE_REGEX, SplitCaptures, SplitItem, truncate_spans_with_ellipsis},
};

const DEFAULT_STYLE: Style = Style::new();

/// How much width the description is allowed to take if the command doesn't fit
const DESCRIPTION_WIDTH_PERCENT: f32 = 0.3;

/// Widget to render a [`Command`]
#[derive(Clone)]
pub struct CommandWidget<'a>(CommandWidgetInner<'a>, Size);
impl<'a> CommandWidget<'a> {
    /// Builds a new [`CommandWidget`]
    pub fn new(
        command: &'a Command,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
        plain_style: bool,
    ) -> Self {
        let mut line_style = DEFAULT_STYLE;
        if is_highlighted && let Some(bg_color) = theme.highlight {
            line_style = line_style.bg(bg_color.into());
        }
        // Determine the right styles to use based on highlighted and discarded status
        let (primary_style, secondary_style, comment_style, accent_style) =
            match (plain_style, is_discarded, is_highlighted) {
                // Discarded
                (_, true, false) => (theme.secondary, theme.secondary, theme.secondary, theme.secondary),
                // Discarded & highlighted
                (_, true, true) => (
                    theme.highlight_secondary,
                    theme.highlight_secondary,
                    theme.highlight_secondary,
                    theme.highlight_secondary,
                ),
                // Plain style, regular
                (true, false, false) => (theme.primary, theme.primary, theme.comment, theme.accent),
                // Plain style, highlighted
                (true, false, true) => (
                    theme.highlight_primary,
                    theme.highlight_primary,
                    theme.highlight_comment,
                    theme.highlight_accent,
                ),
                // Regular
                (false, false, false) => (theme.primary, theme.secondary, theme.comment, theme.accent),
                // Highlighted
                (false, false, true) => (
                    theme.highlight_primary,
                    theme.highlight_secondary,
                    theme.highlight_comment,
                    theme.highlight_accent,
                ),
            };

        // Build command spans
        let cmd_splitter = SplitCaptures::new(&COMMAND_VARIABLE_REGEX, &command.cmd);
        let cmd_spans = cmd_splitter
            .map(|e| match e {
                SplitItem::Unmatched(t) => Span::styled(t, primary_style),
                SplitItem::Captured(l) => Span::styled(l.get(0).unwrap().as_str(), secondary_style),
            })
            .collect::<Vec<_>>();

        if inline {
            // When inline, display a single line with the command, alias and the first line of the description
            let mut description_spans = Vec::new();
            if command.description.is_some() || command.alias.is_some() {
                description_spans.push(Span::styled(" # ", comment_style));
                if let Some(ref alias) = command.alias {
                    description_spans.push(Span::styled("[", accent_style));
                    description_spans.push(Span::styled(alias, accent_style));
                    description_spans.push(Span::styled("] ", accent_style));
                }
                if let Some(ref description) = command.description
                    && let Some(line) = description.lines().next()
                {
                    description_spans.push(Span::styled(line, comment_style));
                }
            }

            // Calculate total size for the list view's layout engine
            let total_width = cmd_spans.iter().map(|s| s.width() as u16).sum::<u16>()
                + description_spans.iter().map(|s| s.width() as u16).sum::<u16>();

            let renderer = InlineCommandRenderer {
                cmd_spans,
                description_spans,
                line_style,
            };
            Self(CommandWidgetInner::Inline(renderer), Size::new(total_width, 1))
        } else {
            // When not inline, display the full description including the alias followed by the command
            let mut lines = Vec::new();
            if let Some(ref description) = command.description {
                let mut alias_included = command.alias.is_none();
                for line in description.lines() {
                    if !alias_included && let Some(ref alias) = command.alias {
                        let parts = vec![
                            Span::styled("# ", comment_style),
                            Span::styled("[", accent_style),
                            Span::styled(alias, accent_style),
                            Span::styled("] ", accent_style),
                            Span::styled(line, comment_style),
                        ];
                        lines.push(Line::from(parts));
                        alias_included = true;
                    } else {
                        lines.push(Line::from(vec![Span::raw("# "), Span::raw(line)]).style(comment_style));
                    }
                }
            } else if let Some(ref alias) = command.alias {
                let parts = vec![
                    Span::styled("# ", comment_style),
                    Span::styled("[", accent_style),
                    Span::styled(alias, accent_style),
                    Span::styled("]", accent_style),
                ];
                lines.push(Line::from(parts));
            }
            let mut parts = Vec::new();
            cmd_spans.into_iter().for_each(|s| parts.push(s));
            lines.push(Line::from(parts));

            let text = Text::from(lines).style(line_style);
            let width = text.width() as u16;
            let height = text.height() as u16;

            Self(CommandWidgetInner::Block(text), Size::new(width, height))
        }
    }

    /// Retrieves the size of this widget
    pub fn size(&self) -> Size {
        self.1
    }
}

impl<'a> Widget for CommandWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        match self.0 {
            CommandWidgetInner::Inline(w) => w.render(area, buf),
            CommandWidgetInner::Block(w) => w.render(area, buf),
        }
    }
}

/// An enum to dispatch rendering to the correct widget implementation
#[derive(Clone)]
enum CommandWidgetInner<'a> {
    Inline(InlineCommandRenderer<'a>),
    Block(Text<'a>),
}

/// A widget to render a command in a single line, intelligently truncating parts
#[derive(Clone)]
struct InlineCommandRenderer<'a> {
    cmd_spans: Vec<Span<'a>>,
    description_spans: Vec<Span<'a>>,
    line_style: Style,
}
impl<'a> Widget for InlineCommandRenderer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Apply the base background style across the whole line
        buf.set_style(area, self.line_style);

        // Calculate the total required width of all spans
        let cmd_width: u16 = self.cmd_spans.iter().map(|s| s.width() as u16).sum();
        let desc_width: u16 = self.description_spans.iter().map(|s| s.width() as u16).sum();
        let total_width = cmd_width.saturating_add(desc_width);

        // If everything fits on the line, render them sequentially
        if total_width <= area.width {
            let mut combined_spans = self.cmd_spans;
            combined_spans.extend(self.description_spans);
            buf.set_line(area.x, area.y, &Line::from(combined_spans), area.width);
        } else {
            // Otherwise, truncate if required
            let min_description_width = (area.width as f32 * DESCRIPTION_WIDTH_PERCENT).floor() as u16;
            let desired_desc_width = desc_width.min(min_description_width);
            let available_space_for_cmd = area.width.saturating_sub(desired_desc_width);

            // If command fits fully, the description fills the remaining space
            if cmd_width <= available_space_for_cmd {
                // Render the full command
                buf.set_line(area.x, area.y, &Line::from(self.cmd_spans), cmd_width);

                // Truncate the description to whatever space is left and render it
                let remaining_space = area.width.saturating_sub(cmd_width);
                if remaining_space > 0 {
                    let (truncated_desc_spans, _) =
                        truncate_spans_with_ellipsis(&self.description_spans, remaining_space);
                    buf.set_line(
                        area.x + cmd_width,
                        area.y,
                        &Line::from(truncated_desc_spans),
                        remaining_space,
                    );
                }
            } else {
                // Otherwise, the command is too long and must be truncated to accomodate some room for the description
                let (truncated_desc_spans, truncated_desc_width) =
                    truncate_spans_with_ellipsis(&self.description_spans, desired_desc_width);

                if truncated_desc_width > 0 {
                    let desc_start_x = area.x + area.width.saturating_sub(truncated_desc_width);
                    buf.set_line(
                        desc_start_x,
                        area.y,
                        &Line::from(truncated_desc_spans),
                        truncated_desc_width,
                    );
                }

                let final_cmd_width = area.width.saturating_sub(truncated_desc_width);
                if final_cmd_width > 0 {
                    let (truncated_cmd_spans, _) = truncate_spans_with_ellipsis(&self.cmd_spans, final_cmd_width);
                    buf.set_line(area.x, area.y, &Line::from(truncated_cmd_spans), final_cmd_width);
                }
            }
        }
    }
}
