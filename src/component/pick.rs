use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use futures_util::{StreamExt, TryStreamExt, stream};
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};
use tracing::instrument;

use super::Component;
use crate::{
    app::Action,
    cli::{ExportCommandsProcess, ImportCommandsProcess},
    component::edit::{EditCommandComponent, EditCommandComponentMode},
    config::{Config, KeyBindingsConfig},
    errors::{AppError, UserFacingError},
    format_error, format_msg,
    model::Command,
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{CommandWidget, CustomList, ErrorPopup, HighlightSymbolMode, LoadingSpinner, NewVersionBanner},
};

/// Defines the operational mode of the [`CommandsPickerComponent`]
#[derive(Clone, strum::EnumIs)]
pub enum CommandsPickerComponentMode {
    /// The component is used to import picked commands
    Import { input: ImportCommandsProcess },
    /// The component is used to export picked commands
    Export { input: ExportCommandsProcess },
}

/// A component for interactive picking [`Command`]
#[derive(Clone)]
pub struct CommandsPickerComponent {
    /// The app config
    config: Config,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// Whether the component is displayed inline
    inline: bool,
    /// The component layout
    layout: Layout,
    /// The operational mode
    mode: CommandsPickerComponentMode,
    /// Whether the component has been initialized
    initialized: bool,
    /// The state of the component
    state: Arc<RwLock<CommandsPickerComponentState<'static>>>,
}
struct CommandsPickerComponentState<'a> {
    /// The list of commands to be imported
    commands: CustomList<'a, CommandWidget>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
    /// Widget for displaying a loading spinner
    loading_spinner: LoadingSpinner<'a>,
    /// The indices of the commands discarded
    discarded_indices: HashSet<usize>,
    /// Whether the component is currently fetching commands
    is_loading: bool,
    /// The result of the loading process, if any
    loading_result: Option<Result<ProcessOutput, AppError>>,
}

impl CommandsPickerComponent {
    /// Creates a new [`CommandsPickerComponent`]
    pub fn new(service: IntelliShellService, config: Config, inline: bool, mode: CommandsPickerComponentMode) -> Self {
        let commands = CustomList::new(config.theme.primary, inline, Vec::new())
            .title(" Commands (Space to discard, Enter to continue) ")
            .highlight_symbol(config.theme.highlight_symbol.clone())
            .highlight_symbol_mode(HighlightSymbolMode::Last)
            .highlight_symbol_style(config.theme.highlight_primary_full().into());

        let error = ErrorPopup::empty(&config.theme);
        let loading_spinner = LoadingSpinner::new(&config.theme).with_message("Loading");

        let layout = if inline {
            Layout::vertical([Constraint::Min(1)])
        } else {
            Layout::vertical([Constraint::Min(3)]).margin(1)
        };

        Self {
            config,
            service,
            inline,
            layout,
            mode,
            initialized: false,
            state: Arc::new(RwLock::new(CommandsPickerComponentState {
                commands,
                error,
                loading_spinner,
                discarded_indices: HashSet::new(),
                is_loading: false,
                loading_result: None,
            })),
        }
    }

    fn toggle_discard(&mut self, toggle_all: bool) {
        let mut state = self.state.write();
        let items_len = state.commands.items().len();
        if let Some(selected_index) = state.commands.selected_index() {
            // Check if the command is already in the discarded set
            if state.discarded_indices.contains(&selected_index) {
                // If so, "un-discard"
                if toggle_all {
                    state.discarded_indices.clear();
                    for widget in state.commands.items_mut() {
                        widget.set_discarded(false);
                    }
                } else {
                    state.discarded_indices.remove(&selected_index);
                    if let Some(widget) = state.commands.selected_mut() {
                        widget.set_discarded(false);
                    }
                }
            } else {
                // Otherwise, add to the "discard" set
                if toggle_all {
                    state.discarded_indices.extend(0..items_len);
                    for widget in state.commands.items_mut() {
                        widget.set_discarded(true);
                    }
                } else {
                    state.discarded_indices.insert(selected_index);
                    if let Some(widget) = state.commands.selected_mut() {
                        widget.set_discarded(true);
                    }
                }
            }
        }
    }
}

#[async_trait]
impl Component for CommandsPickerComponent {
    fn name(&self) -> &'static str {
        "CommandsPickerComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // 10 Commands
        10
    }

    #[instrument(skip_all)]
    async fn init_and_peek(&mut self) -> Result<Action> {
        if self.initialized {
            // If already initialized, just return no action
            return Ok(Action::NoOp);
        }

        // Initialize the component state based on the mode
        match &self.mode {
            CommandsPickerComponentMode::Import { input } => {
                self.state.write().is_loading = true;

                // Spawn a background task to fetch commands, which can be slow on ai mode
                let this = self.clone();
                let input = input.clone();
                tokio::spawn(async move {
                    // Fetch commands from the given import location
                    let commands: Result<Vec<Command>, AppError> = match this
                        .service
                        .get_commands_from_location(input, this.config.gist.clone())
                        .await
                    {
                        Ok(c) => c.try_collect().await,
                        Err(err) => Err(err),
                    };
                    match commands {
                        Ok(commands) => {
                            // If commands were fetched successfully, update the state
                            let mut state = this.state.write();
                            if commands.is_empty() {
                                state.loading_result = Some(Ok(ProcessOutput::fail()
                                    .stderr(format_error!(this.config.theme, "No commands were found"))));
                            } else {
                                state.commands.update_items(
                                    commands
                                        .into_iter()
                                        .map(|c| {
                                            CommandWidget::new(&this.config.theme, this.inline, c).discarded(false)
                                        })
                                        .collect(),
                                );
                            }
                            state.is_loading = false;
                        }
                        Err(err) => {
                            // If an error occurred, set the error message and stop loading
                            let mut state = this.state.write();
                            state.loading_result = Some(Err(err));
                            state.is_loading = false;
                        }
                    }
                });
            }
            CommandsPickerComponentMode::Export { input } => {
                // Prepare commands for export
                let res = match self.service.prepare_commands_export(input.filter.clone()).await {
                    Ok(s) => s.try_collect().await,
                    Err(err) => Err(err),
                };
                let commands: Vec<Command> = match res {
                    Ok(c) => c,
                    Err(AppError::UserFacing(err)) => {
                        return Ok(Action::Quit(
                            ProcessOutput::fail().stderr(format_error!(self.config.theme, "{err}")),
                        ));
                    }
                    Err(AppError::Unexpected(report)) => return Err(report),
                };

                if commands.is_empty() {
                    return Ok(Action::Quit(
                        ProcessOutput::fail().stderr(format_error!(self.config.theme, "No commands to export")),
                    ));
                } else {
                    let mut state = self.state.write();
                    state.commands.update_items(
                        commands
                            .into_iter()
                            .map(|c| CommandWidget::new(&self.config.theme, self.inline, c).discarded(false))
                            .collect(),
                    );
                }
            }
        }

        // Mark the component as initialized, as it doesn't fetch commands again on component switch
        self.initialized = true;
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [main_area] = self.layout.areas(area);

        let mut state = self.state.write();

        if state.is_loading {
            // Render the loading spinner widget
            state.loading_spinner.render_in(frame, main_area);
        } else {
            // Render the commands list when not loading
            frame.render_widget(&mut state.commands, main_area);
        }

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.check_new_version() {
            NewVersionBanner::new(&self.config.theme, new_version).render_in(frame, area);
        }
        state.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();

        // If there is a loading result, handle it
        if let Some(res) = state.loading_result.take() {
            return match res {
                Ok(output) => Ok(Action::Quit(output)),
                Err(AppError::UserFacing(err)) => Ok(Action::Quit(
                    ProcessOutput::fail().stderr(format_error!(&self.config.theme, "{err}")),
                )),
                Err(AppError::Unexpected(err)) => Err(err),
            };
        }

        state.error.tick();
        state.loading_spinner.tick();
        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        Ok(Action::Quit(ProcessOutput::success()))
    }

    async fn process_key_event(&mut self, keybindings: &KeyBindingsConfig, key: KeyEvent) -> Result<Action> {
        // If space was hit, toggle discard status
        if key.code == KeyCode::Char(' ') {
            self.toggle_discard(key.modifiers == KeyModifiers::CONTROL);
            Ok(Action::NoOp)
        } else {
            // Otherwise, process default actions
            Ok(self
                .default_process_key_event(keybindings, key)
                .await?
                .unwrap_or_default())
        }
    }

    fn process_mouse_event(&mut self, mouse: MouseEvent) -> Result<Action> {
        match mouse.kind {
            MouseEventKind::ScrollDown => Ok(self.move_down()?),
            MouseEventKind::ScrollUp => Ok(self.move_up()?),
            _ => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.commands.select_prev();
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.commands.select_next();
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
        if absolute {
            state.commands.select_first();
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        if absolute {
            state.commands.select_last();
        }
        Ok(Action::NoOp)
    }

    async fn selection_delete(&mut self) -> Result<Action> {
        self.toggle_discard(false);
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        // Do nothing if the component is in a loading state
        if self.state.read().is_loading {
            return Ok(Action::NoOp);
        }

        // Get the selected command and its index, if any
        let selected_data = {
            let state = self.state.read();
            state.commands.selected_with_index().map(|(index, widget)| {
                let command: Command = widget.clone().into();
                (index, command)
            })
        };

        if let Some((index, command)) = selected_data {
            // Clone the current component to serve as the parent to return to after editing is done
            let parent_component = Box::new(self.clone());

            // Prepare the callback to be run after the command is updated
            let this = self.clone();
            let callback = Arc::new(move |updated_command: Command| -> Result<()> {
                let mut state = this.state.write();

                // Preserve the 'discarded' status of the command across edits
                let is_discarded = state.discarded_indices.contains(&index);

                // Replace the old widget with the new one at the same position in the list
                if let Some(widget_ref) = state.commands.items_mut().get_mut(index) {
                    *widget_ref =
                        CommandWidget::new(&this.config.theme, this.inline, updated_command).discarded(is_discarded);
                }

                Ok(())
            });

            // Switch to the editor component
            Ok(Action::SwitchComponent(Box::new(EditCommandComponent::new(
                self.service.clone(),
                self.config.theme.clone(),
                self.inline,
                command,
                EditCommandComponentMode::EditMemory {
                    parent: parent_component,
                    callback,
                },
            ))))
        } else {
            // No item was selected, do nothing
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        // Collect all commands that were NOT discarded by the user
        let non_discarded_commands: Vec<Command> = {
            let state = self.state.read();
            // Do nothing if the component is in a loading state
            if state.is_loading {
                return Ok(Action::NoOp);
            }
            state
                .commands
                .items()
                .iter()
                .enumerate()
                .filter_map(|(index, widget)| {
                    // Skip discarded commands
                    if state.discarded_indices.contains(&index) {
                        None
                    } else {
                        Some(widget.clone().into())
                    }
                })
                .collect()
        };
        match &self.mode {
            CommandsPickerComponentMode::Import { input } => {
                let output = if input.dry_run {
                    // If dry run, just print the commands to the console
                    let mut commands = String::new();
                    for command in non_discarded_commands {
                        commands += &command.to_string();
                        commands += "\n";
                    }
                    if commands.is_empty() {
                        ProcessOutput::fail().stderr(format_error!(&self.config.theme, "No commands were found"))
                    } else {
                        ProcessOutput::success().stdout(commands)
                    }
                } else {
                    // If not dry run, import the commands
                    match self
                        .service
                        .import_commands(stream::iter(non_discarded_commands.into_iter().map(Ok)).boxed(), false)
                        .await
                    {
                        Ok((0, 0)) => {
                            ProcessOutput::fail().stderr(format_error!(&self.config.theme, "No commands were found"))
                        }
                        Ok((0, skipped)) => ProcessOutput::success().stderr(format_msg!(
                            &self.config.theme,
                            "No commands imported, {skipped} already existed"
                        )),
                        Ok((imported, 0)) => ProcessOutput::success()
                            .stderr(format_msg!(&self.config.theme, "Imported {imported} new commands")),
                        Ok((imported, skipped)) => ProcessOutput::success().stderr(format_msg!(
                            &self.config.theme,
                            "Imported {imported} new commands {}",
                            &self
                                .config
                                .theme
                                .secondary
                                .apply(format!("({skipped} already existed)"))
                        )),
                        Err(AppError::UserFacing(err)) => {
                            ProcessOutput::fail().stderr(format_error!(&self.config.theme, "{err}"))
                        }
                        Err(AppError::Unexpected(report)) => return Err(report),
                    }
                };

                Ok(Action::Quit(output))
            }
            CommandsPickerComponentMode::Export { input } => {
                match self
                    .service
                    .export_commands(
                        stream::iter(non_discarded_commands.into_iter().map(Ok)).boxed(),
                        input.clone(),
                        self.config.gist.clone(),
                    )
                    .await
                {
                    Ok((0, _)) => Ok(Action::Quit(
                        ProcessOutput::fail().stderr(format_error!(&self.config.theme, "No commands to export")),
                    )),
                    Ok((exported, None)) => Ok(Action::Quit(
                        ProcessOutput::success()
                            .stderr(format_msg!(&self.config.theme, "Exported {exported} commands")),
                    )),
                    Ok((exported, Some(stdout))) => {
                        Ok(Action::Quit(ProcessOutput::success().stdout(stdout).stderr(
                            format_msg!(&self.config.theme, "Exported {exported} commands"),
                        )))
                    }
                    Err(AppError::UserFacing(UserFacingError::FileBrokenPipe)) => {
                        Ok(Action::Quit(ProcessOutput::success()))
                    }
                    Err(AppError::UserFacing(err)) => Ok(Action::Quit(
                        ProcessOutput::fail().stderr(format_error!(&self.config.theme, "{err}")),
                    )),
                    Err(AppError::Unexpected(report)) => Err(report),
                }
            }
        }
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }
}
