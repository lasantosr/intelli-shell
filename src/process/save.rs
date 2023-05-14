use anyhow::Result;
use crossterm::event::Event;
use tui::{backend::Backend, layout::Rect, style::Style, Frame};

use crate::{
    common::{
        widget::{CustomParagraph, CustomWidget, TextInput},
        ExecutionContext, InteractiveProcess,
    },
    model::Command,
    storage::{SqliteStorage, USER_CATEGORY},
    Process, ProcessOutput,
};

/// Process to save a new [Command]
///
/// If both command and description are provided upon initialization, this process will show no UI.
/// If the description is missing, it will ask for it.
pub struct SaveCommandProcess<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Command to save
    command: String,
    /// Provided description of the command
    description: Option<String>,
    /// Current command description
    current_description: CustomParagraph<TextInput>,
    // Execution context
    ctx: ExecutionContext,
}

impl<'s> SaveCommandProcess<'s> {
    pub fn new(
        storage: &'s SqliteStorage,
        command: String,
        description: Option<String>,
        ctx: ExecutionContext,
    ) -> Self {
        let current_description = CustomParagraph::new(TextInput::default())
            .inline(ctx.inline)
            .focus(true)
            .inline_title("Description:")
            .block_title("Description")
            .style(Style::default().fg(ctx.theme.main));

        Self {
            storage,
            command,
            description,
            current_description,
            ctx,
        }
    }

    /// Inserts a new [Command] with provided fields on [USER_CATEGORY]
    fn insert_command(
        storage: &SqliteStorage,
        command: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<ProcessOutput> {
        let cmd = command.into();
        let mut command = Command::new(USER_CATEGORY, cmd, description);
        Ok(match storage.insert_command(&mut command)? {
            true => ProcessOutput::new(" -> Command was saved successfully", command.cmd),
            false => ProcessOutput::new(" -> Command already existed, so it was updated", command.cmd),
        })
    }
}

impl<'s> Process for SaveCommandProcess<'s> {
    fn min_height(&self) -> usize {
        5
    }

    fn peek(&mut self) -> Result<Option<ProcessOutput>> {
        if self.command.is_empty() {
            Ok(Some(ProcessOutput::message(" -> A command must be typed first!")))
        } else {
            match &self.description {
                Some(d) => Ok(Some(Self::insert_command(self.storage, &self.command, d)?)),
                None => Ok(None),
            }
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        self.current_description.render_in(frame, area, self.ctx.theme);
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<ProcessOutput>> {
        self.process_event(event)
    }
}

impl<'s> InteractiveProcess for SaveCommandProcess<'s> {
    fn move_up(&mut self) {
        self.current_description.inner_mut().move_up()
    }

    fn move_down(&mut self) {
        self.current_description.inner_mut().move_down()
    }

    fn move_left(&mut self) {
        self.current_description.inner_mut().move_left()
    }

    fn move_right(&mut self) {
        self.current_description.inner_mut().move_right()
    }

    fn prev(&mut self) {}

    fn next(&mut self) {}

    fn home(&mut self) {
        self.current_description.inner_mut().move_beginning()
    }

    fn end(&mut self) {
        self.current_description.inner_mut().move_end()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        self.current_description.inner_mut().insert_text(text);
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        self.current_description.inner_mut().insert_char(c);
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        self.current_description.inner_mut().delete_char(backspace);
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<ProcessOutput>> {
        if !self.current_description.inner().as_str().is_empty() {
            // Exit after saving the command
            Ok(Some(Self::insert_command(
                self.storage,
                &self.command,
                self.current_description.inner().as_str(),
            )?))
        } else {
            // Keep waiting for input
            Ok(None)
        }
    }

    fn exit(&mut self) -> Result<ProcessOutput> {
        Ok(ProcessOutput::output(self.command.clone()))
    }
}
