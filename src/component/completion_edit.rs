use std::{mem, sync::Arc};

use async_trait::async_trait;
use color_eyre::Result;
use enum_cycling::EnumCycle;
use itertools::Itertools;
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tracing::instrument;

use super::Component;
use crate::{
    app::Action,
    config::Theme,
    errors::AppError,
    format_msg,
    model::VariableCompletion,
    process::ProcessOutput,
    service::{FORBIDDEN_COMPLETION_ROOT_CMD_CHARS, FORBIDDEN_COMPLETION_VARIABLE_CHARS, IntelliShellService},
    utils::resolve_completion,
    widgets::{CustomTextArea, ErrorPopup, NewVersionBanner},
};

/// Defines the operational mode of the [`EditCompletionComponent`]
#[derive(strum::EnumIs)]
pub enum EditCompletionComponentMode {
    /// The component is used to create a new completion
    New { ai: bool },
    /// The component is used to edit an existing completion
    /// It holds the parent component to switch back to after editing is complete
    Edit { parent: Box<dyn Component> },
    /// The component is used to edit a completion in memory.
    /// It holds the parent component to switch back to after editing is complete and a callback to run.
    EditMemory {
        parent: Box<dyn Component>,
        callback: Arc<dyn Fn(VariableCompletion) -> Result<()> + Send + Sync>,
    },
    /// A special case to be used in mem::replace to extract the owned variables
    Empty,
}

/// A component for creating or editing a [`VariableCompletion`]
pub struct EditCompletionComponent {
    /// The visual theme for styling the component
    theme: Theme,
    /// Whether the TUI is in inline mode or not
    inline: bool,
    /// Service for interacting with storage
    service: IntelliShellService,
    /// The layout for arranging the input fields
    layout: Layout,
    /// The operational mode
    mode: EditCompletionComponentMode,
    /// The state of the component
    state: Arc<RwLock<EditCompletionComponentState<'static>>>,
}
struct EditCompletionComponentState<'a> {
    /// The completion being edited or created
    completion: VariableCompletion,
    /// The currently focused input field
    active_field: ActiveField,
    /// Text area for the completion root cmd
    root_cmd: CustomTextArea<'a>,
    /// Text area for the completion variable
    variable: CustomTextArea<'a>,
    /// Text area for the command to provide suggestions
    suggestions_provider: CustomTextArea<'a>,
    /// The output of the last execution
    last_output: Option<Result<String, String>>,
    /// A flag to track if the text content has been modified since the last test
    is_dirty: bool,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
}

/// Represents the currently active (focused) input field
#[derive(Clone, Copy, PartialEq, Eq, EnumCycle)]
enum ActiveField {
    RootCmd,
    Variable,
    SuggestionsCommand,
}

impl EditCompletionComponent {
    /// Creates a new [`EditCompletionComponent`]
    pub fn new(
        service: IntelliShellService,
        theme: Theme,
        inline: bool,
        completion: VariableCompletion,
        mode: EditCompletionComponentMode,
    ) -> Self {
        let mut root_cmd = CustomTextArea::new(theme.secondary, inline, false, "")
            .title(if inline { "Command:" } else { " Command " })
            .forbidden_chars_regex(FORBIDDEN_COMPLETION_ROOT_CMD_CHARS.clone())
            .focused();
        let mut variable = CustomTextArea::new(theme.primary, inline, false, "")
            .title(if inline { "Variable:" } else { " Variable " })
            .forbidden_chars_regex(FORBIDDEN_COMPLETION_VARIABLE_CHARS.clone())
            .focused();
        let mut suggestions_provider = CustomTextArea::new(theme.primary, inline, false, "")
            .title(if inline {
                "Suggestions Provider:"
            } else {
                " Suggestions Provider "
            })
            .focused();

        root_cmd.insert_str(&completion.root_cmd);
        variable.insert_str(&completion.variable);
        suggestions_provider.insert_str(&completion.suggestions_provider);

        let active_field = if completion.root_cmd.is_empty() && completion.variable.is_empty() {
            root_cmd.set_focus(true);
            variable.set_focus(false);
            suggestions_provider.set_focus(false);
            ActiveField::RootCmd
        } else if completion.variable.is_empty() {
            root_cmd.set_focus(false);
            variable.set_focus(true);
            suggestions_provider.set_focus(false);
            ActiveField::Variable
        } else {
            root_cmd.set_focus(false);
            variable.set_focus(false);
            suggestions_provider.set_focus(true);
            ActiveField::SuggestionsCommand
        };

        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(3),
            ])
        } else {
            Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(3),
            ])
            .margin(1)
        };

        Self {
            theme,
            inline,
            service,
            layout,
            mode,
            state: Arc::new(RwLock::new(EditCompletionComponentState {
                completion,
                active_field,
                root_cmd,
                variable,
                suggestions_provider,
                last_output: None,
                is_dirty: true,
                error,
            })),
        }
    }
}
impl<'a> EditCompletionComponentState<'a> {
    /// Returns a mutable reference to the currently active input
    fn active_input(&mut self) -> &mut CustomTextArea<'a> {
        match self.active_field {
            ActiveField::RootCmd => &mut self.root_cmd,
            ActiveField::Variable => &mut self.variable,
            ActiveField::SuggestionsCommand => &mut self.suggestions_provider,
        }
    }

    /// Updates the focus state of the input fields based on `active_field`
    fn update_focus(&mut self) {
        self.root_cmd.set_focus(false);
        self.variable.set_focus(false);
        self.suggestions_provider.set_focus(false);

        self.active_input().set_focus(true);
    }

    /// Marks the completion as dirty, that means the provider has to be tested before completion
    fn mark_as_dirty(&mut self) {
        self.is_dirty = true;
        self.last_output = None;
    }
}

#[async_trait]
impl Component for EditCompletionComponent {
    fn name(&self) -> &'static str {
        "CompletionEditComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // Root Cmd + Variable + Suggestions Provider + Output
        1 + 1 + 1 + 5
    }

    #[instrument(skip_all)]
    async fn init_and_peek(&mut self) -> Result<Action> {
        // If AI mode is enabled, prompt for command suggestions
        if let EditCompletionComponentMode::New { ai } = &self.mode
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
        let [root_cmd_area, variable_area, suggestions_provider_area, output_area] = self.layout.areas(area);

        // Render widgets
        frame.render_widget(&state.root_cmd, root_cmd_area);
        frame.render_widget(&state.variable, variable_area);
        frame.render_widget(&state.suggestions_provider, suggestions_provider_area);

        // Render the output
        if let Some(out) = &state.last_output {
            let is_err = out.is_err();
            let (output, style) = match out {
                Ok(o) => (o, self.theme.secondary),
                Err(err) => (err, self.theme.error),
            };
            let output_paragraph = Paragraph::new(output.as_str())
                .style(style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Preview ")
                        .title_alignment(if self.inline { Alignment::Right } else { Alignment::Left })
                        .style(style),
                )
                .wrap(Wrap { trim: is_err });
            frame.render_widget(output_paragraph, output_area);
        }

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.poll_new_version() {
            NewVersionBanner::new(&self.theme, new_version).render_in(frame, area);
        }
        state.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.error.tick();
        state.root_cmd.tick();
        state.variable.tick();
        state.suggestions_provider.tick();

        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        // Based on the component mode
        match &self.mode {
            // Quit the component without saving
            EditCompletionComponentMode::New { .. } => Ok(Action::Quit(ProcessOutput::success())),
            // Switch back to the parent, without storing the completion
            EditCompletionComponentMode::Edit { .. } => Ok(Action::SwitchComponent(
                match mem::replace(&mut self.mode, EditCompletionComponentMode::Empty) {
                    EditCompletionComponentMode::Edit { parent } => parent,
                    EditCompletionComponentMode::Empty
                    | EditCompletionComponentMode::New { .. }
                    | EditCompletionComponentMode::EditMemory { .. } => {
                        unreachable!()
                    }
                },
            )),
            // Switch back to the parent, without calling the callback
            EditCompletionComponentMode::EditMemory { .. } => Ok(Action::SwitchComponent(
                match mem::replace(&mut self.mode, EditCompletionComponentMode::Empty) {
                    EditCompletionComponentMode::EditMemory { parent, .. } => parent,
                    EditCompletionComponentMode::Empty
                    | EditCompletionComponentMode::New { .. }
                    | EditCompletionComponentMode::Edit { .. } => {
                        unreachable!()
                    }
                },
            )),
            // This should never happen, but just in case
            EditCompletionComponentMode::Empty => Ok(Action::NoOp),
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
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().redo();
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, text: String) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_str(text);
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_char(c);
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    fn insert_newline(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().insert_newline();
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.active_input().delete(backspace, word);
        state.mark_as_dirty();

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        let completion = {
            let mut state = self.state.write();
            if state.active_input().is_ai_loading() {
                return Ok(Action::NoOp);
            }

            // Update the completion with the input data
            state
                .completion
                .clone()
                .with_root_cmd(state.root_cmd.lines_as_string())
                .with_variable(state.variable.lines_as_string())
                .with_suggestions_provider(state.suggestions_provider.lines_as_string())
        };

        if self.state.read().is_dirty {
            self.test_provider_command(&completion).await?;
            self.state.write().is_dirty = false;
            return Ok(Action::NoOp);
        }

        // Based on the component mode
        match &self.mode {
            // Insert the new completion
            EditCompletionComponentMode::New { .. } => {
                match self.service.create_variable_completion(completion).await {
                    Ok(c) if c.is_global() => Ok(Action::Quit(ProcessOutput::success().stderr(format_msg!(
                        self.theme,
                        "Completion for global {} variable stored: {}",
                        self.theme.secondary.apply(&c.flat_variable),
                        self.theme.secondary.apply(&c.suggestions_provider)
                    )))),
                    Ok(c) => Ok(Action::Quit(ProcessOutput::success().stderr(format_msg!(
                        self.theme,
                        "Completion for {} variable within {} commands stored: {}",
                        self.theme.secondary.apply(&c.flat_variable),
                        self.theme.secondary.apply(&c.flat_root_cmd),
                        self.theme.secondary.apply(&c.suggestions_provider)
                    )))),
                    Err(AppError::UserFacing(err)) => {
                        tracing::warn!("{err}");
                        let mut state = self.state.write();
                        state.error.set_temp_message(err.to_string());
                        Ok(Action::NoOp)
                    }
                    Err(AppError::Unexpected(report)) => Err(report),
                }
            }
            // Edit the existing completion
            EditCompletionComponentMode::Edit { .. } => {
                match self.service.update_variable_completion(completion).await {
                    Ok(_) => {
                        // Extract the owned parent component, leaving a placeholder on its place
                        Ok(Action::SwitchComponent(
                            match mem::replace(&mut self.mode, EditCompletionComponentMode::Empty) {
                                EditCompletionComponentMode::Edit { parent } => parent,
                                EditCompletionComponentMode::Empty
                                | EditCompletionComponentMode::New { .. }
                                | EditCompletionComponentMode::EditMemory { .. } => {
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
            // Edit the completion in memory and run the callback
            EditCompletionComponentMode::EditMemory { callback, .. } => {
                // Run the callback
                callback(completion)?;

                // Extract the owned parent component, leaving a placeholder on its place
                Ok(Action::SwitchComponent(
                    match mem::replace(&mut self.mode, EditCompletionComponentMode::Empty) {
                        EditCompletionComponentMode::EditMemory { parent, .. } => parent,
                        EditCompletionComponentMode::Empty
                        | EditCompletionComponentMode::New { .. }
                        | EditCompletionComponentMode::Edit { .. } => {
                            unreachable!()
                        }
                    },
                ))
            }
            // This should never happen, but just in case
            EditCompletionComponentMode::Empty => Ok(Action::NoOp),
        }
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }

    async fn prompt_ai(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if state.active_field != ActiveField::SuggestionsCommand || state.active_input().is_ai_loading() {
            return Ok(Action::NoOp);
        }

        let root_cmd = state.root_cmd.lines_as_string();
        let variable = state.variable.lines_as_string();
        let suggestions_provider = state.suggestions_provider.lines_as_string();

        state.suggestions_provider.set_ai_loading(true);
        let cloned_service = self.service.clone();
        let cloned_state = self.state.clone();
        tokio::spawn(async move {
            let res = cloned_service
                .suggest_completion(&root_cmd, &variable, &suggestions_provider)
                .await;
            let mut state = cloned_state.write();
            state.suggestions_provider.set_ai_loading(false);
            match res {
                Ok(s) if s.is_empty() => {
                    state.error.set_temp_message("AI generated an empty response");
                }
                Ok(suggestion) => {
                    if !suggestions_provider.is_empty() {
                        state.suggestions_provider.select_all();
                        state.suggestions_provider.cut();
                    }
                    state.suggestions_provider.insert_str(&suggestion);
                    state.mark_as_dirty();
                }
                Err(AppError::UserFacing(err)) => {
                    tracing::warn!("{err}");
                    state.error.set_temp_message(err.to_string());
                }
                Err(AppError::Unexpected(err)) => {
                    panic!("Error prompting for completion suggestions: {err:?}")
                }
            }
        });

        Ok(Action::NoOp)
    }
}

impl EditCompletionComponent {
    /// Runs the provider command and updates the state with the output
    async fn test_provider_command(&mut self, completion: &VariableCompletion) -> Result<bool> {
        match resolve_completion(completion, None).await {
            Ok(suggestions) if suggestions.is_empty() => {
                let mut state = self.state.write();
                state.last_output = Some(Ok("... empty output ...".to_string()));
                Ok(true)
            }
            Ok(suggestions) => {
                let mut state = self.state.write();
                state.last_output = Some(Ok(suggestions.iter().join("\n")));
                Ok(true)
            }
            Err(err) => {
                let mut state = self.state.write();
                state.last_output = Some(Err(err));
                Ok(false)
            }
        }
    }
}
