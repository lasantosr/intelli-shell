use ratatui::prelude::*;

use crate::{config::Theme, widgets::AsWidget};

const DEFAULT_STYLE: Style = Style::new();

/// A widget for displaying a tag
#[derive(Clone)]
pub struct TagWidget {
    text: String,
    style: Style,
    highlight_color: Option<Color>,
    highlight_style: Style,
}

impl TagWidget {
    pub fn new(theme: &Theme, text: String) -> Self {
        Self {
            text,
            style: theme.comment.into(),
            highlight_color: theme.highlight.map(Into::into),
            highlight_style: theme.highlight_comment.into(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Widget for TagWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.as_widget(false).0.render(area, buf);
    }
}

impl AsWidget for TagWidget {
    fn as_widget<'a>(&'a self, is_highlighted: bool) -> (impl Widget + 'a, Size) {
        let (line_style, style) = if is_highlighted {
            let mut line_style = DEFAULT_STYLE;
            if let Some(color) = self.highlight_color {
                line_style = line_style.bg(color);
            }
            (line_style, self.highlight_style)
        } else {
            (DEFAULT_STYLE, self.style)
        };

        let line = Line::from(Span::styled(&self.text, style)).style(line_style);
        let width = line.width() as u16;
        (line, Size::new(width, 1))
    }
}
