use std::mem;

use async_trait::async_trait;
use color_eyre::Result;
use enum_cycling::EnumCycle;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};
use semver::Version;
use tracing::instrument;

use super::Component;
use crate::{
    app::Action,
    config::Theme,
    errors::{InsertError, UpdateError},
    format_msg,
    model::Command,
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{CustomTextArea, ErrorPopup, NewVersionBanner},
};

/// Defines the operational mode of the [`EditCommandComponent`]
#[derive(strum::EnumIs)]
pub enum EditCommandComponentMode {
    /// The component is used to create a new command
    New,
    /// The component is to edit an existing command
    /// It holds the parent component to switch back to after editing is complete.
    Edit { parent: Box<dyn Component> },
}

/// A component for creating or editing a [`Command`]
pub struct EditCommandComponent {
    /// The visual theme for styling the component
    theme: Theme,
    /// The operational mode
    mode: EditCommandComponentMode,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// The command being edited or created
    command: Command,
    /// The layout for arranging the input fields
    layout: Layout,
    /// The currently focused input field
    active_field: ActiveField,
    /// Text area for the command's alias
    alias: CustomTextArea<'static>,
    /// Text area for the command itself
    cmd: CustomTextArea<'static>,
    /// Text area for the command's description
    description: CustomTextArea<'static>,
    /// The new version banner
    new_version: NewVersionBanner,
    /// Popup for displaying error messages
    error: ErrorPopup<'static>,
}

/// Represents the currently active (focused) input field
#[derive(Clone, Copy, PartialEq, Eq, EnumCycle)]
enum ActiveField {
    Alias,
    Command,
    Description,
}

impl EditCommandComponent {
    /// Creates a new [`EditCommandComponent`]
    pub fn new(
        service: IntelliShellService,
        theme: Theme,
        inline: bool,
        new_version: Option<Version>,
        command: Command,
        mode: EditCommandComponentMode,
    ) -> Self {
        let alias = CustomTextArea::new(
            theme.secondary,
            inline,
            false,
            command.alias.clone().unwrap_or_default(),
        )
        .title(if inline { "Alias:" } else { " Alias " });
        let mut cmd = CustomTextArea::new(
            // Primary style
            theme.primary,
            inline,
            false,
            &command.cmd,
        )
        .title(if inline { "Command:" } else { " Command " });
        let mut description = CustomTextArea::new(
            theme.primary,
            inline,
            true,
            command.description.clone().unwrap_or_default(),
        )
        .title(if inline { "Description:" } else { " Description " });

        let active_field = if mode.is_new() && !command.cmd.is_empty() && command.description.is_none() {
            description.set_focus(true);
            ActiveField::Description
        } else {
            cmd.set_focus(true);
            ActiveField::Command
        };

        let new_version = NewVersionBanner::new(&theme, new_version);
        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        Self {
            theme,
            service,
            command,
            mode,
            layout,
            active_field,
            alias,
            cmd,
            description,
            new_version,
            error,
        }
    }

    /// Returns a mutable reference to the currently active input
    fn active_input(&mut self) -> &mut CustomTextArea<'static> {
        match self.active_field {
            ActiveField::Alias => &mut self.alias,
            ActiveField::Command => &mut self.cmd,
            ActiveField::Description => &mut self.description,
        }
    }

    /// Updates the focus state of the input fields based on `active_field`
    fn update_focus(&mut self) {
        self.alias.set_focus(false);
        self.cmd.set_focus(false);
        self.description.set_focus(false);

        self.active_input().set_focus(true);
    }
}

#[async_trait]
impl Component for EditCommandComponent {
    fn name(&self) -> &'static str {
        "EditCommandComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // Alias + Command + Description
        1 + 1 + 3
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [alias_area, cmd_area, description_area] = self.layout.areas(area);

        // Render widgets
        frame.render_widget(&self.alias, alias_area);
        frame.render_widget(&self.cmd, cmd_area);
        frame.render_widget(&self.description, description_area);

        // Render the new version banner and error message as an overlay
        self.new_version.render_in(frame, area);
        self.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        self.error.tick();

        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Option<ProcessOutput>> {
        Ok(Some(ProcessOutput::success().fileout(self.cmd.lines_as_string())))
    }

    fn move_up(&mut self) -> Result<Action> {
        self.active_field = self.active_field.up();
        self.update_focus();

        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        self.active_field = self.active_field.down();
        self.update_focus();

        Ok(Action::NoOp)
    }

    fn move_left(&mut self, word: bool) -> Result<Action> {
        self.active_input().move_cursor_left(word);

        Ok(Action::NoOp)
    }

    fn move_right(&mut self, word: bool) -> Result<Action> {
        self.active_input().move_cursor_right(word);

        Ok(Action::NoOp)
    }

    fn move_prev(&mut self) -> Result<Action> {
        self.move_up()
    }

    fn move_next(&mut self) -> Result<Action> {
        self.move_down()
    }

    fn move_home(&mut self, absolute: bool) -> Result<Action> {
        self.active_input().move_home(absolute);

        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        self.active_input().move_end(absolute);

        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        self.active_input().undo();

        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        self.active_input().redo();

        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, text: String) -> Result<Action> {
        self.active_input().insert_str(text);

        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        self.active_input().insert_char(c);

        Ok(Action::NoOp)
    }

    fn insert_newline(&mut self) -> Result<Action> {
        self.active_input().insert_newline();

        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        self.active_input().delete(backspace, word);

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        // Update the command with the input data
        let command = self
            .command
            .clone()
            .with_alias(Some(self.alias.lines_as_string()))
            .with_cmd(self.cmd.lines_as_string())
            .with_description(Some(self.description.lines_as_string()));

        // Based on the component mode
        match &self.mode {
            // Insert the new command
            EditCommandComponentMode::New => match self.service.insert_command(command).await {
                Ok(command) => Ok(Action::Quit(
                    ProcessOutput::success()
                        .stderr(format_msg!(
                            self.theme,
                            "Command stored: {}",
                            self.theme.secondary.apply(&command.cmd)
                        ))
                        .fileout(command.cmd),
                )),
                Err(InsertError::Invalid(err)) => {
                    tracing::warn!("{err}");
                    self.error.set_temp_message(err);
                    Ok(Action::NoOp)
                }
                Err(InsertError::AlreadyExists) => {
                    tracing::warn!("The command is already bookmarked");
                    self.error.set_temp_message("The command is already bookmarked");
                    Ok(Action::NoOp)
                }
                Err(InsertError::Unexpected(report)) => Err(report),
            },
            // Edit the existing command
            EditCommandComponentMode::Edit { .. } => {
                match self.service.update_command(command).await {
                    Ok(_) => {
                        // Extract the owned parent component, leaving a placeholder on its place
                        Ok(Action::SwitchComponent(
                            match mem::replace(&mut self.mode, EditCommandComponentMode::New) {
                                EditCommandComponentMode::Edit { parent } => parent,
                                EditCommandComponentMode::New => unreachable!(),
                            },
                        ))
                    }
                    Err(UpdateError::Invalid(err)) => {
                        tracing::warn!("{err}");
                        self.error.set_temp_message(err);
                        Ok(Action::NoOp)
                    }
                    Err(UpdateError::AlreadyExists) => {
                        tracing::warn!("The updated command already exists");
                        self.error.set_temp_message("The updated command already exists");
                        Ok(Action::NoOp)
                    }
                    Err(UpdateError::Unexpected(report)) => Err(report),
                }
            }
        }
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }
}
