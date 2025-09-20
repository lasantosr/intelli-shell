use std::{mem, sync::Arc};

use async_trait::async_trait;
use color_eyre::Result;
use enum_cycling::EnumCycle;
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::Component;
use crate::{
    app::Action,
    config::Theme,
    errors::AppError,
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
    New { ai: bool },
    /// The component is used to edit an existing command
    /// It holds the parent component to switch back to after editing is complete
    Edit { parent: Box<dyn Component> },
    /// The component is used to edit a command in memory.
    /// It holds the parent component to switch back to after editing is complete and a callback to run.
    EditMemory {
        parent: Box<dyn Component>,
        callback: Arc<dyn Fn(Command) -> Result<()> + Send + Sync>,
    },
    /// A special case to be used in mem::replace to extract the owned variables
    Empty,
}

/// A component for creating or editing a [`Command`]
pub struct EditCommandComponent {
    /// The visual theme for styling the component
    theme: Theme,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// The layout for arranging the input fields
    layout: Layout,
    /// The operational mode
    mode: EditCommandComponentMode,
    /// Global cancellation token
    global_cancellation_token: CancellationToken,
    /// The state of the component
    state: Arc<RwLock<EditCommandComponentState<'static>>>,
}
struct EditCommandComponentState<'a> {
    /// The command being edited or created
    command: Command,
    /// The currently focused input field
    active_field: ActiveField,
    /// Text area for the command's alias
    alias: CustomTextArea<'a>,
    /// Text area for the command itself
    cmd: CustomTextArea<'a>,
    /// Text area for the command's description
    description: CustomTextArea<'a>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
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
        command: Command,
        mode: EditCommandComponentMode,
        cancellation_token: CancellationToken,
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

        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        Self {
            theme,
            service,
            layout,
            mode,
            global_cancellation_token: cancellation_token,
            state: Arc::new(RwLock::new(EditCommandComponentState {
                command,
                active_field,
                alias,
                cmd,
                description,
                error,
            })),
        }
    }
}
impl<'a> EditCommandComponentState<'a> {
    /// Returns a mutable reference to the currently active input
    fn active_input(&mut self) -> &mut CustomTextArea<'a> {
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
    async fn init_and_peek(&mut self) -> Result<Action> {
        // If AI mode is enabled, prompt for command suggestions
        if let EditCommandComponentMode::New { ai } = &self.mode
            && *ai
        {
            self.prompt_ai().await?;
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut state = self.state.write();

        // Split the area according to the layout
        let [alias_area, cmd_area, description_area] = self.layout.areas(area);

        // Render widgets
        frame.render_widget(&state.alias, alias_area);
        frame.render_widget(&state.cmd, cmd_area);
        frame.render_widget(&state.description, description_area);

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.poll_new_version() {
            NewVersionBanner::new(&self.theme, new_version).render_in(frame, area);
        }
        state.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.error.tick();
        state.alias.tick();
        state.cmd.tick();
        state.description.tick();

        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        // Based on the component mode
        match &self.mode {
            // Quit the component without saving
            EditCommandComponentMode::New { .. } => {
                let state = self.state.read();
                Ok(Action::Quit(
                    ProcessOutput::success().fileout(state.cmd.lines_as_string()),
                ))
            }
            // Switch back to the parent, without storing the command
            EditCommandComponentMode::Edit { .. } => Ok(Action::SwitchComponent(
                match mem::replace(&mut self.mode, EditCommandComponentMode::Empty) {
                    EditCommandComponentMode::Edit { parent } => parent,
                    EditCommandComponentMode::Empty
                    | EditCommandComponentMode::New { .. }
                    | EditCommandComponentMode::EditMemory { .. } => {
                        unreachable!()
                    }
                },
            )),
            // Switch back to the parent, without calling the callback
            EditCommandComponentMode::EditMemory { .. } => Ok(Action::SwitchComponent(
                match mem::replace(&mut self.mode, EditCommandComponentMode::Empty) {
                    EditCommandComponentMode::EditMemory { parent, .. } => parent,
                    EditCommandComponentMode::Empty
                    | EditCommandComponentMode::New { .. }
                    | EditCommandComponentMode::Edit { .. } => {
                        unreachable!()
                    }
                },
            )),
            // This should never happen, but just in case
            EditCommandComponentMode::Empty => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.active_input().is_ai_loading() {
            state.active_field = state.active_field.up();
            state.update_focus();
        }

        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.active_input().is_ai_loading() {
            state.active_field = state.active_field.down();
            state.update_focus();
        }

        Ok(Action::NoOp)
    }

    fn move_left(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().move_cursor_left(word);

        Ok(Action::NoOp)
    }

    fn move_right(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().move_cursor_right(word);

        Ok(Action::NoOp)
    }

    fn move_prev(&mut self) -> Result<Action> {
        self.move_up()
    }

    fn move_next(&mut self) -> Result<Action> {
        self.move_down()
    }

    fn move_home(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().move_home(absolute);

        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().move_end(absolute);

        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().undo();

        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().redo();

        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, text: String) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_str(text);

        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_char(c);

        Ok(Action::NoOp)
    }

    fn insert_newline(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_newline();

        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().delete(backspace, word);

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        let command = {
            let mut state = self.state.write();
            if state.active_input().is_ai_loading() {
                return Ok(Action::NoOp);
            }

            // Update the command with the input data
            state
                .command
                .clone()
                .with_alias(Some(state.alias.lines_as_string()))
                .with_cmd(state.cmd.lines_as_string())
                .with_description(Some(state.description.lines_as_string()))
        };

        // Based on the component mode
        match &self.mode {
            // Insert the new command
            EditCommandComponentMode::New { .. } => match self.service.insert_command(command).await {
                Ok(command) => Ok(Action::Quit(
                    ProcessOutput::success()
                        .stderr(format_msg!(
                            self.theme,
                            "Command stored: {}",
                            self.theme.secondary.apply(&command.cmd)
                        ))
                        .fileout(command.cmd),
                )),
                Err(AppError::UserFacing(err)) => {
                    tracing::warn!("{err}");
                    let mut state = self.state.write();
                    state.error.set_temp_message(err.to_string());
                    Ok(Action::NoOp)
                }
                Err(AppError::Unexpected(report)) => Err(report),
            },
            // Edit the existing command
            EditCommandComponentMode::Edit { .. } => {
                match self.service.update_command(command).await {
                    Ok(_) => {
                        // Extract the owned parent component, leaving a placeholder on its place
                        Ok(Action::SwitchComponent(
                            match mem::replace(&mut self.mode, EditCommandComponentMode::Empty) {
                                EditCommandComponentMode::Edit { parent } => parent,
                                EditCommandComponentMode::Empty
                                | EditCommandComponentMode::New { .. }
                                | EditCommandComponentMode::EditMemory { .. } => {
                                    unreachable!()
                                }
                            },
                        ))
                    }
                    Err(AppError::UserFacing(err)) => {
                        tracing::warn!("{err}");
                        let mut state = self.state.write();
                        state.error.set_temp_message(err.to_string());
                        Ok(Action::NoOp)
                    }
                    Err(AppError::Unexpected(report)) => Err(report),
                }
            }
            // Edit the command in memory and run the callback
            EditCommandComponentMode::EditMemory { callback, .. } => {
                // Run the callback
                callback(command)?;

                // Extract the owned parent component, leaving a placeholder on its place
                Ok(Action::SwitchComponent(
                    match mem::replace(&mut self.mode, EditCommandComponentMode::Empty) {
                        EditCommandComponentMode::EditMemory { parent, .. } => parent,
                        EditCommandComponentMode::Empty
                        | EditCommandComponentMode::New { .. }
                        | EditCommandComponentMode::Edit { .. } => {
                            unreachable!()
                        }
                    },
                ))
            }
            // This should never happen, but just in case
            EditCommandComponentMode::Empty => Ok(Action::NoOp),
        }
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }

    async fn prompt_ai(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if state.active_input().is_ai_loading() || state.active_field == ActiveField::Alias {
            return Ok(Action::NoOp);
        }

        let cmd = state.cmd.lines_as_string();
        let description = state.description.lines_as_string();

        if cmd.trim().is_empty() && description.trim().is_empty() {
            return Ok(Action::NoOp);
        }

        state.active_input().set_ai_loading(true);
        let cloned_service = self.service.clone();
        let cloned_state = self.state.clone();
        let cloned_token = self.global_cancellation_token.clone();
        tokio::spawn(async move {
            let res = cloned_service.suggest_command(&cmd, &description, cloned_token).await;
            let mut state = cloned_state.write();
            match res {
                Ok(Some(suggestion)) => {
                    state.cmd.set_focus(true);
                    state.cmd.set_ai_loading(false);
                    if !cmd.is_empty() {
                        state.cmd.select_all();
                        state.cmd.cut();
                    }
                    state.cmd.insert_str(&suggestion.cmd);
                    if let Some(suggested_description) = suggestion.description.as_deref() {
                        state.description.set_focus(true);
                        state.description.set_ai_loading(false);
                        if !description.is_empty() {
                            state.description.select_all();
                            state.description.cut();
                        }
                        state.description.insert_str(suggested_description);
                    }
                }
                Ok(None) => {
                    state
                        .error
                        .set_temp_message("AI did not return any suggestion".to_string());
                }
                Err(AppError::UserFacing(err)) => {
                    tracing::warn!("{err}");
                    state.error.set_temp_message(err.to_string());
                }
                Err(AppError::Unexpected(err)) => panic!("Error prompting for command suggestions: {err:?}"),
            }
            // Restore ai mode and focus
            state.active_input().set_ai_loading(false);
            state.update_focus();
        });

        Ok(Action::NoOp)
    }
}
