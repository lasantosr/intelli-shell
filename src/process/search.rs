use anyhow::Result;
use crossterm::event::Event;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    Frame,
};

use super::LabelProcess;
use crate::{
    common::{
        widget::{
            CustomParagraph, CustomStatefulList, CustomStatefulWidget, CustomWidget, TextInput,
            DEFAULT_HIGHLIGHT_SYMBOL_PREFIX,
        },
        ExecutionContext, InteractiveProcess, Process,
    },
    model::{AsLabeledCommand, Command},
    storage::SqliteStorage,
    ProcessOutput,
};

/// Process to search for [Command]
pub struct SearchProcess<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Current value of the filter box
    filter: CustomParagraph<TextInput>,
    /// Command list of results
    commands: CustomStatefulList<Command>,
    /// Delegate label widget
    delegate_label: Option<LabelProcess<'s>>,
    // Execution context
    ctx: ExecutionContext,
}

impl<'s> SearchProcess<'s> {
    pub fn new(storage: &'s SqliteStorage, filter: String, ctx: ExecutionContext) -> Result<Self> {
        let commands = storage.find_commands(&filter)?;

        let filter = CustomParagraph::new(TextInput::new(filter))
            .inline(ctx.inline)
            .focus(true)
            .inline_title("(filter)")
            .block_title("Filter")
            .style(Style::default().fg(ctx.theme.main));

        let commands = CustomStatefulList::new(commands)
            .inline(ctx.inline)
            .block_title("Commands")
            .style(Style::default().fg(ctx.theme.main))
            .highlight_style(
                Style::default()
                    .bg(ctx.theme.selected_background)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(DEFAULT_HIGHLIGHT_SYMBOL_PREFIX);

        Ok(Self {
            commands,
            filter,
            storage,
            delegate_label: None,
            ctx,
        })
    }

    fn exit_or_label_replace(&mut self, output: ProcessOutput) -> Result<Option<ProcessOutput>> {
        if let Some(cmd) = &output.output {
            if let Some(labeled_cmd) = cmd.as_labeled_command() {
                let w = LabelProcess::new(self.storage, labeled_cmd, self.ctx)?;
                self.delegate_label = Some(w);
                return Ok(None);
            }
        }
        Ok(Some(output))
    }
}

impl<'s> Process for SearchProcess<'s> {
    fn min_height(&self) -> usize {
        (self.commands.len() + 1).clamp(4, 15)
    }

    fn peek(&mut self) -> Result<Option<ProcessOutput>> {
        if self.storage.is_empty()? {
            let message = indoc::indoc! { r#"
                -> There are no stored commands yet!
                    - Try to bookmark some command with 'Ctrl + B'
                    - Or execute 'intelli-shell fetch' to download a bunch of tldr's useful commands"# 
            };
            Ok(Some(ProcessOutput::message(message)))
        } else if !self.filter.inner().as_str().is_empty() && self.commands.len() == 1 {
            if let Some(command) = self.commands.current_mut() {
                command.increment_usage();
                self.storage.update_command(command)?;
                let cmd = command.cmd.clone();
                self.exit_or_label_replace(ProcessOutput::output(cmd))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        // If there's a delegate active, forward to it
        if let Some(delegate) = &mut self.delegate_label {
            delegate.render(frame, area);
            return;
        }

        // Prepare main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(!self.ctx.inline as u16)
            .constraints([Constraint::Length(self.filter.min_size().height), Constraint::Min(1)])
            .split(area);

        let header = chunks[0];
        let body = chunks[1];

        // Render filter
        self.filter.render_in(frame, header, self.ctx.theme);

        // Render command list
        self.commands.render_in(frame, body, self.ctx.theme);
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<ProcessOutput>> {
        // If there's a delegate active, forward to it
        if let Some(delegate) = &mut self.delegate_label {
            delegate.process_event(event)
        } else {
            self.process_event(event)
        }
    }
}

impl<'s> InteractiveProcess for SearchProcess<'s> {
    fn move_up(&mut self) {
        self.commands.previous()
    }

    fn move_down(&mut self) {
        self.commands.next()
    }

    fn move_left(&mut self) {
        self.filter.inner_mut().move_left()
    }

    fn move_right(&mut self) {
        self.filter.inner_mut().move_right()
    }

    fn prev(&mut self) {
        self.commands.previous()
    }

    fn next(&mut self) {
        self.commands.next()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        self.filter.inner_mut().insert_text(text);
        self.commands
            .update_items(self.storage.find_commands(self.filter.inner().as_str())?);
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        self.filter.inner_mut().insert_char(c);
        self.commands
            .update_items(self.storage.find_commands(self.filter.inner().as_str())?);
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        if self.filter.inner_mut().delete_char(backspace) {
            self.commands
                .update_items(self.storage.find_commands(self.filter.inner().as_str())?);
        }
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        if let Some(command) = self.commands.delete_current() {
            self.storage.delete_command(command.id)?;
        }
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<ProcessOutput>> {
        if let Some(command) = self.commands.current_mut() {
            command.increment_usage();
            self.storage.update_command(command)?;
            let cmd = command.cmd.clone();
            self.exit_or_label_replace(ProcessOutput::output(cmd))
        } else if !self.filter.inner().as_str().is_empty() {
            self.exit_or_label_replace(ProcessOutput::output(self.filter.inner().as_str()))
        } else {
            Ok(Some(ProcessOutput::empty()))
        }
    }

    fn exit(&mut self) -> Result<ProcessOutput> {
        if self.filter.inner().as_str().is_empty() {
            Ok(ProcessOutput::empty())
        } else {
            Ok(ProcessOutput::output(self.filter.inner().as_str()))
        }
    }
}
