use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use tui::{
    backend::Backend,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    common::OverflowText,
    model::Command,
    storage::{SqliteStorage, USER_CATEGORY},
    theme::Theme,
    Widget, WidgetOutput,
};

/// Widget to save a new [Command]
///
/// If both command and description are provided upon initialization, this widget will show no UI.
/// If the description is missing, it will ask for it.
pub struct SaveCommandWidget<'s> {
    /// Storage
    storage: &'s mut SqliteStorage,
    /// Command to save
    command: String,
    /// Provided description of the command
    description: Option<String>,
    /// Current command description for UI
    current_description: String,
}

impl<'s> SaveCommandWidget<'s> {
    pub fn new(storage: &'s mut SqliteStorage, command: String, description: Option<String>) -> Self {
        Self {
            storage,
            command,
            description,
            current_description: Default::default(),
        }
    }

    /// Inserts a new [Command] with provided fields on [USER_CATEGORY]
    fn insert_command(
        storage: &mut SqliteStorage,
        command: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<WidgetOutput> {
        let cmd = command.into();
        let mut command = Command::new(USER_CATEGORY, &cmd, description);
        Ok(match storage.insert_command(&mut command)? {
            true => WidgetOutput::new("Command was saved successfully", cmd),
            false => WidgetOutput::new("Command already existed, so it was updated", cmd),
        })
    }
}

impl<'s> Widget for SaveCommandWidget<'s> {
    fn min_height(&self) -> usize {
        1
    }

    fn peek(&mut self) -> Result<Option<WidgetOutput>> {
        match &self.description {
            Some(d) => Ok(Some(Self::insert_command(self.storage, &self.command, d)?)),
            None => Ok(None),
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect, inline: bool, theme: Theme) {
        // Display description prompt
        let max_width = area.width as usize - 1 - (2 * (!inline as usize));
        let text_inline = format!("Description: {}", self.current_description);
        let description_text = if inline {
            OverflowText::new(max_width, &text_inline)
        } else {
            OverflowText::new(max_width, &self.current_description)
        };
        let description_text_width = description_text.width() as u16;
        let mut description_input = Paragraph::new(description_text).style(Style::default().fg(theme.main));
        if !inline {
            description_input = description_input.block(Block::default().borders(Borders::ALL).title(" Description "));
        }
        frame.render_widget(description_input, area);

        // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
        frame.set_cursor(
            // Put cursor past the end of the input text
            area.x + description_text_width + (!inline as u16),
            // Move one line down, from the border to the input line
            area.y + (!inline as u16),
        );
    }

    fn process_event(&mut self, event: Event) -> Result<Option<WidgetOutput>> {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Tab => {
                    if !self.current_description.is_empty() {
                        // Exit after saving the command
                        return Ok(Some(Self::insert_command(
                            self.storage,
                            &self.command,
                            &self.current_description,
                        )?));
                    }
                }
                KeyCode::Char(c) => {
                    self.current_description.push(c);
                }
                KeyCode::Backspace => {
                    self.current_description.pop();
                }
                KeyCode::Esc => {
                    // Exit without saving
                    return Ok(Some(WidgetOutput::output(self.command.clone())));
                }
                _ => (),
            }
        }
        // Continue waiting for input
        Ok(None)
    }
}