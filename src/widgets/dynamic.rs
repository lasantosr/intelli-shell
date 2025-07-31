use std::{
    fmt::{Display, Formatter},
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
    model::{CommandPart, DynamicCommand},
};

/// The widget for a command containing variables
#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone)]
pub struct DynamicCommandWidget {
    /// The dynamic command
    pub command: DynamicCommand,
    // Internal fields
    block: Option<Block<'static>>,
    primary_style: Style,
    secondary_style: Style,
}

impl DynamicCommandWidget {
    /// Creates a new widget for the dynamic command
    pub fn new(theme: &Theme, inline: bool, command: DynamicCommand) -> Self {
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
            command,
            block,
            primary_style: theme.primary.into(),
            secondary_style: theme.secondary.into(),
        }
    }
}

impl Display for DynamicCommandWidget {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command)
    }
}

impl Widget for &DynamicCommandWidget {
    fn render(self, mut area: Rect, buf: &mut Buffer) {
        if let Some(block) = &self.block {
            block.render(area, buf);
            area = block.inner(area);
        }

        let mut first_variable_found = false;
        Line::from_iter(self.command.parts.iter().map(|p| match p {
            CommandPart::Text(t) | CommandPart::VariableValue(_, t) => Span::styled(t, self.secondary_style),
            CommandPart::Variable(v) => {
                let style = if !first_variable_found {
                    first_variable_found = true;
                    self.primary_style
                } else {
                    self.secondary_style
                };
                Span::styled(format!("{{{{{}}}}}", v.name), style)
            }
        }))
        .render(area, buf);
    }
}

impl Deref for DynamicCommandWidget {
    type Target = DynamicCommand;

    fn deref(&self) -> &Self::Target {
        &self.command
    }
}

impl DerefMut for DynamicCommandWidget {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.command
    }
}
