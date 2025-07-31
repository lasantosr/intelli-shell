use std::ops::{Deref, DerefMut};

use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::Widget,
};

use crate::{
    config::Theme,
    model::Command,
    utils::{COMMAND_VARIABLE_REGEX, SplitCaptures, SplitItem},
    widgets::AsWidget,
};

const DEFAULT_STYLE: Style = Style::new();

/// Widget to render a [`Command`]
#[derive(Clone)]
pub struct CommandWidget {
    inline: bool,
    primary_style: Style,
    secondary_style: Style,
    accent_style: Style,
    comment_style: Style,
    highlight_color: Option<Color>,
    highlight_primary_style: Style,
    highlight_secondary_style: Style,
    highlight_accent_style: Style,
    highlight_comment_style: Style,
    inner: Command,
}

impl CommandWidget {
    /// Creates a new [`CommandWidget`]
    pub fn new(theme: &Theme, inline: bool, command: Command) -> Self {
        Self {
            inline,
            primary_style: theme.primary.into(),
            secondary_style: theme.secondary.into(),
            accent_style: theme.accent.into(),
            comment_style: theme.comment.into(),
            highlight_color: theme.highlight.map(Into::into),
            highlight_primary_style: theme.highlight_primary.into(),
            highlight_secondary_style: theme.highlight_secondary.into(),
            highlight_accent_style: theme.highlight_accent.into(),
            highlight_comment_style: theme.highlight_comment.into(),
            inner: command,
        }
    }
}

impl AsWidget for CommandWidget {
    fn as_widget<'a>(&'a self, is_highlighted: bool) -> (impl Widget + 'a, Size) {
        // Determine the right styles to use based on highligted status
        let (line_style, primary_style, secondary_style, comment_style, accent_style) = if is_highlighted {
            let mut line_style = DEFAULT_STYLE;
            if let Some(color) = self.highlight_color {
                line_style = line_style.bg(color);
            }
            (
                line_style,
                self.highlight_primary_style,
                self.highlight_secondary_style,
                self.highlight_comment_style,
                self.highlight_accent_style,
            )
        } else {
            (
                DEFAULT_STYLE,
                self.primary_style,
                self.secondary_style,
                self.comment_style,
                self.accent_style,
            )
        };

        // Build the lines for the text
        let mut lines = Vec::new();

        // Build the command spans
        let cmd_splitter = SplitCaptures::new(&COMMAND_VARIABLE_REGEX, &self.cmd);
        let cmd_spans = cmd_splitter.map(|e| match e {
            SplitItem::Unmatched(t) => Span::styled(t, primary_style),
            SplitItem::Captured(l) => Span::styled(l.get(0).unwrap().as_str(), secondary_style),
        });

        if self.inline {
            // When inline, display a single line with the alias, command and the first line of the description
            let mut parts = Vec::new();
            if let Some(ref alias) = self.alias {
                parts.push(Span::styled("[", accent_style));
                parts.push(Span::styled(alias, accent_style));
                parts.push(Span::styled("] ", accent_style));
            }
            cmd_spans.for_each(|s| parts.push(s));
            if let Some(ref description) = self.description
                && let Some(line) = description.lines().next()
            {
                parts.push(Span::styled(" # ", comment_style));
                parts.push(Span::styled(line, comment_style));
            }
            lines.push(Line::from(parts));
        } else {
            // When not inline, display the full description followed by the alias and command
            if let Some(ref description) = self.description {
                for line in description.lines() {
                    lines.push(Line::from(vec![Span::raw("# "), Span::raw(line)]).style(comment_style));
                }
            }
            let mut parts = Vec::new();
            if let Some(ref alias) = self.alias {
                parts.push(Span::styled("[", accent_style));
                parts.push(Span::styled(alias, accent_style));
                parts.push(Span::styled("] ", accent_style));
            }
            cmd_spans.for_each(|s| parts.push(s));
            lines.push(Line::from(parts));
        }

        let text = Text::from(lines).style(line_style);
        let width = text.width() as u16;
        let height = text.height() as u16;

        (text, Size::new(width, height))
    }
}

impl Widget for CommandWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.as_widget(false).0.render(area, buf);
    }
}

impl Deref for CommandWidget {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CommandWidget {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<CommandWidget> for Command {
    fn from(value: CommandWidget) -> Self {
        value.inner
    }
}
