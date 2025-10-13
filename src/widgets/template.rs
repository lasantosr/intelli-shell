use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::{
    config::Theme,
    model::{CommandTemplate, TemplatePart},
};

/// The widget for a command containing variables
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct CommandTemplateWidget {
    /// The command template
    pub template: CommandTemplate,
    /// The index of the current variable being edited (for highlighting)
    pub current_variable_index: usize,
    // Internal fields
    block: Option<Block<'static>>,
    primary_style: Style,
    secondary_style: Style,
}

impl CommandTemplateWidget {
    /// Creates a new widget for the command template
    pub fn new(theme: &Theme, inline: bool, template: CommandTemplate) -> Self {
        let block = if !inline {
            Some(
                Block::default()
                    .borders(Borders::ALL)
                    .style(theme.primary)
                    .title(" Command "),
            )
        } else {
            None
        };
        Self {
            template,
            current_variable_index: 0,
            block,
            primary_style: theme.primary.into(),
            secondary_style: theme.secondary.into(),
        }
    }
}

impl fmt::Display for CommandTemplateWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.template)
    }
}

impl Widget for &CommandTemplateWidget {
    fn render(self, mut area: Rect, buf: &mut Buffer) {
        if let Some(block) = &self.block {
            block.render(area, buf);
            area = block.inner(area);
        }

        let mut variable_index = 0;
        Line::from_iter(self.template.parts.iter().map(|p| match p {
            TemplatePart::Text(t) => Span::styled(t, self.secondary_style),
            TemplatePart::Variable(v) => {
                let is_current = variable_index == self.current_variable_index;
                variable_index += 1;

                let style = if is_current {
                    self.primary_style
                } else {
                    self.secondary_style
                };

                Span::styled(format!("{{{{{}}}}}", v.display), style)
            }
            TemplatePart::VariableValue(_v, t) => {
                let is_current = variable_index == self.current_variable_index;
                variable_index += 1;

                let style = if is_current {
                    self.primary_style
                } else {
                    self.secondary_style
                };

                Span::styled(t, style)
            }
        }))
        .render(area, buf);
    }
}

impl Deref for CommandTemplateWidget {
    type Target = CommandTemplate;

    fn deref(&self) -> &Self::Target {
        &self.template
    }
}

impl DerefMut for CommandTemplateWidget {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.template
    }
}
