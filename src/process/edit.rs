use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    Frame,
};

use crate::{
    common::{
        widget::{CustomParagraph, CustomWidget, TextInput},
        ExecutionContext, InteractiveProcess,
    },
    model::Command,
    storage::SqliteStorage,
    Process, ProcessOutput,
};

/// Process to edit a [Command]
pub struct EditCommandProcess<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Initial command
    command: Command,
    /// Command alias
    alias: CustomParagraph<TextInput>,
    /// Command itself
    cmd: CustomParagraph<TextInput>,
    /// Command description
    description: CustomParagraph<TextInput>,
    /// Kind of field currently active
    active_field_kind: ActiveFieldKind,
    /// Execution context
    ctx: ExecutionContext,
}

pub enum ActiveFieldKind {
    Alias,
    Command,
    Description,
}

impl<'s> EditCommandProcess<'s> {
    pub fn new(storage: &'s SqliteStorage, command: Command, ctx: ExecutionContext) -> Result<Self> {
        let active_field_kind = if !command.cmd.is_empty() && command.description.is_empty() {
            ActiveFieldKind::Description
        } else {
            ActiveFieldKind::Command
        };

        let mut alias = CustomParagraph::new(TextInput::new(command.alias.as_deref().unwrap_or_default()))
            .inline(ctx.inline)
            .inline_title("(alias)")
            .block_title("Alias")
            .style(Style::default().fg(ctx.theme.secondary));

        let mut cmd = CustomParagraph::new(TextInput::new(&command.cmd))
            .inline(ctx.inline)
            .inline_title("Command:")
            .block_title("Command")
            .style(Style::default());

        let mut description = CustomParagraph::new(TextInput::new(&command.description))
            .inline(ctx.inline)
            .inline_title("Description:")
            .block_title("Description")
            .style(Style::default());

        match active_field_kind {
            ActiveFieldKind::Alias => alias.set_focus(true),
            ActiveFieldKind::Command => cmd.set_focus(true),
            ActiveFieldKind::Description => description.set_focus(true),
        };

        Ok(Self {
            storage,
            command,
            alias,
            cmd,
            description,
            active_field_kind,
            ctx,
        })
    }

    fn active_input(&mut self) -> &mut CustomParagraph<TextInput> {
        match self.active_field_kind {
            ActiveFieldKind::Alias => &mut self.alias,
            ActiveFieldKind::Command => &mut self.cmd,
            ActiveFieldKind::Description => &mut self.description,
        }
    }

    fn update_focus(&mut self) {
        self.alias.set_focus(false);
        self.cmd.set_focus(false);
        self.description.set_focus(false);

        self.active_input().set_focus(true);
    }

    fn finish(&mut self) -> Result<ProcessOutput> {
        // Edit command
        self.command.alias = if self.alias.inner().as_str().is_empty() {
            None
        } else {
            Some(self.alias.inner().as_str().to_owned())
        };
        self.command.cmd = self.cmd.inner().as_str().to_owned();
        self.command.description = self.description.inner().as_str().to_owned();

        // Insert / update
        Ok(if self.command.is_persisted() {
            match self.storage.update_command(&self.command)? {
                true => ProcessOutput::new(" -> Command was updated successfully", &self.command.cmd),
                false => ProcessOutput::new(" -> Error: Command didn't exist", &self.command.cmd),
            }
        } else {
            match self.storage.insert_command(&mut self.command)? {
                true => ProcessOutput::new(" -> Command was saved successfully", &self.command.cmd),
                false => ProcessOutput::new(" -> Command already existed, so it was updated", &self.command.cmd),
            }
        })
    }
}

impl<'s> Process for EditCommandProcess<'s> {
    fn min_height(&self) -> usize {
        (self.alias.min_size().height + self.cmd.min_size().height + self.description.min_size().height) as usize
    }

    fn peek(&mut self) -> Result<Option<ProcessOutput>> {
        if !self.command.is_persisted() && !self.command.cmd.is_empty() && !self.command.description.is_empty() {
            Ok(Some(self.finish()?))
        } else {
            Ok(None)
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        // Prepare main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(!self.ctx.inline as u16)
            .constraints([
                Constraint::Length(self.alias.min_size().height),
                Constraint::Length(self.cmd.min_size().height),
                Constraint::Length(self.description.min_size().height),
            ])
            .split(area);

        let alias_area = chunks[0];
        let command_area = chunks[1];
        let description_area = chunks[2];

        // Render components
        self.alias.render_in(frame, alias_area, self.ctx.theme);
        self.cmd.render_in(frame, command_area, self.ctx.theme);
        self.description.render_in(frame, description_area, self.ctx.theme);
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<ProcessOutput>> {
        self.process_event(event)
    }
}

impl<'s> InteractiveProcess for EditCommandProcess<'s> {
    fn move_up(&mut self) {
        self.active_field_kind = match self.active_field_kind {
            ActiveFieldKind::Alias => ActiveFieldKind::Description,
            ActiveFieldKind::Command => ActiveFieldKind::Alias,
            ActiveFieldKind::Description => ActiveFieldKind::Command,
        };
        self.update_focus();
    }

    fn move_down(&mut self) {
        self.active_field_kind = match self.active_field_kind {
            ActiveFieldKind::Alias => ActiveFieldKind::Command,
            ActiveFieldKind::Command => ActiveFieldKind::Description,
            ActiveFieldKind::Description => ActiveFieldKind::Alias,
        };
        self.update_focus();
    }

    fn move_left(&mut self) {
        self.active_input().inner_mut().move_left()
    }

    fn move_right(&mut self) {
        self.active_input().inner_mut().move_right()
    }

    fn prev(&mut self) {
        self.move_up()
    }

    fn next(&mut self) {
        self.move_down()
    }

    fn home(&mut self) {
        self.active_input().inner_mut().move_beginning()
    }

    fn end(&mut self) {
        self.active_input().inner_mut().move_end()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        self.active_input().inner_mut().insert_text(text);
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        self.active_input().inner_mut().insert_char(c);
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        self.active_input().inner_mut().delete_char(backspace);
        Ok(())
    }

    fn edit_current(&mut self) -> Result<()> {
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<ProcessOutput>> {
        if !self.cmd.inner().as_str().is_empty() && !self.description.inner().as_str().is_empty() {
            // Exit after saving the command
            Ok(Some(self.finish()?))
        } else {
            // Keep waiting for input
            Ok(None)
        }
    }

    fn exit(&mut self) -> Result<ProcessOutput> {
        Ok(ProcessOutput::output(self.cmd.inner().as_str()))
    }
}
