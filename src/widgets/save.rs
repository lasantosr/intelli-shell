use anyhow::Result;
use crossterm::event::Event;
use tui::{
    backend::Backend,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    common::{EditableText, InputWidget, OverflowText},
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
    storage: &'s SqliteStorage,
    /// Command to save
    command: String,
    /// Provided description of the command
    description: Option<String>,
    /// Current command description
    current_description: EditableText,
}

impl<'s> SaveCommandWidget<'s> {
    pub fn new(storage: &'s SqliteStorage, command: String, description: Option<String>) -> Self {
        Self {
            storage,
            command,
            description,
            current_description: Default::default(),
        }
    }

    /// Inserts a new [Command] with provided fields on [USER_CATEGORY]
    fn insert_command(
        storage: &SqliteStorage,
        command: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<WidgetOutput> {
        let cmd = command.into();
        let mut command = Command::new(USER_CATEGORY, cmd, description);
        Ok(match storage.insert_command(&mut command)? {
            true => WidgetOutput::new(" -> Command was saved successfully", command.cmd),
            false => WidgetOutput::new(" -> Command already existed, so it was updated", command.cmd),
        })
    }
}

impl<'s> Widget for SaveCommandWidget<'s> {
    fn min_height(&self) -> usize {
        1
    }

    fn peek(&mut self) -> Result<Option<WidgetOutput>> {
        if self.command.is_empty() {
            Ok(Some(WidgetOutput::message(" -> A command must be typed first!")))
        } else {
            match &self.description {
                Some(d) => Ok(Some(Self::insert_command(self.storage, &self.command, d)?)),
                None => Ok(None),
            }
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect, inline: bool, theme: Theme) {
        // Display description prompt
        let mut description_offset = self.current_description.offset();
        let max_width = area.width as usize - 1 - (2 * (!inline as usize));
        let text_inline = format!("Description: {}", self.current_description);
        let description_text = if inline {
            description_offset += 13;
            OverflowText::new(max_width, &text_inline)
        } else {
            OverflowText::new(max_width, self.current_description.as_str())
        };
        let description_text_width = description_text.width();
        if text_inline.len() > description_text_width {
            let overflow = text_inline.len() as i32 - description_text_width as i32;
            if overflow < description_offset as i32 {
                description_offset -= overflow as usize;
            } else {
                description_offset = 0;
            }
        }
        let mut description_input = Paragraph::new(description_text).style(Style::default().fg(theme.main));
        if !inline {
            description_input = description_input.block(Block::default().borders(Borders::ALL).title(" Description "));
        }
        frame.render_widget(description_input, area);

        // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
        frame.set_cursor(
            // Put cursor past the end of the input text
            area.x + description_offset as u16 + (!inline as u16),
            // Move one line down, from the border to the input line
            area.y + (!inline as u16),
        );
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<WidgetOutput>> {
        self.process_event(event)
    }
}

impl<'s> InputWidget for SaveCommandWidget<'s> {
    fn move_up(&mut self) {}

    fn move_down(&mut self) {}

    fn move_left(&mut self) {}

    fn move_right(&mut self) {}

    fn prev(&mut self) {}

    fn next(&mut self) {}

    fn insert_text(&mut self, text: String) -> Result<()> {
        self.current_description.insert_text(text);
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        self.current_description.insert_char(c);
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        self.current_description.delete_char(backspace);
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<WidgetOutput>> {
        if !self.current_description.as_str().is_empty() {
            // Exit after saving the command
            Ok(Some(Self::insert_command(
                self.storage,
                &self.command,
                self.current_description.as_str(),
            )?))
        } else {
            // Keep waiting for input
            Ok(None)
        }
    }

    fn exit(&mut self) -> Result<WidgetOutput> {
        Ok(WidgetOutput::output(self.command.clone()))
    }
}
