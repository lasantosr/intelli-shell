use std::sync::Arc;

use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use futures_util::{StreamExt, TryStreamExt, stream};
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
    cli::{ExportItemsProcess, ImportItemsProcess},
    component::{
        completion_edit::{EditCompletionComponent, EditCompletionComponentMode},
        edit::{EditCommandComponent, EditCommandComponentMode},
    },
    config::{Config, KeyBindingsConfig},
    errors::{AppError, UserFacingError},
    format_error,
    model::{Command, ImportExportItem, VariableCompletion},
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{CustomList, ErrorPopup, LoadingSpinner, NewVersionBanner, items::PlainStyleImportExportItem},
};

/// Defines the operational mode of the [`ImportExportPickerComponent`]
#[derive(Clone, strum::EnumIs)]
pub enum ImportExportPickerComponentMode {
    /// The component is used to import picked items
    Import { input: ImportItemsProcess },
    /// The component is used to export picked items
    Export { input: ExportItemsProcess },
}

/// A component for interactive picking [`ImportExportItem`]
#[derive(Clone)]
pub struct ImportExportPickerComponent {
    /// The app config
    config: Config,
    /// Service for interacting with storage
    service: IntelliShellService,
    /// Whether the component is displayed inline
    inline: bool,
    /// The component layout
    layout: Layout,
    /// The operational mode
    mode: ImportExportPickerComponentMode,
    /// Whether the component has been initialized
    initialized: bool,
    /// Global cancellation token
    global_cancellation_token: CancellationToken,
    /// The state of the component
    state: Arc<RwLock<ImportExportPickerComponentState<'static>>>,
}
struct ImportExportPickerComponentState<'a> {
    /// The list of items to be imported
    items: CustomList<'a, PlainStyleImportExportItem>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
    /// Widget for displaying a loading spinner
    loading_spinner: LoadingSpinner<'a>,
    /// Whether the component is currently fetching
    is_loading: bool,
    /// The result of the loading process, if any
    loading_result: Option<Result<ProcessOutput, AppError>>,
}

impl ImportExportPickerComponent {
    /// Creates a new [`ImportExportPickerComponent`]
    pub fn new(
        service: IntelliShellService,
        config: Config,
        inline: bool,
        mode: ImportExportPickerComponentMode,
        cancellation_token: CancellationToken,
    ) -> Self {
        let title = match &mode {
            ImportExportPickerComponentMode::Import { .. } => " Import (Space to discard, Enter to continue) ",
            ImportExportPickerComponentMode::Export { .. } => " Export (Space to discard, Enter to continue) ",
        };
        let items = CustomList::new(config.theme.clone(), inline, Vec::new()).title(title);

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
            global_cancellation_token: cancellation_token,
            state: Arc::new(RwLock::new(ImportExportPickerComponentState {
                items,
                error,
                loading_spinner,
                is_loading: false,
                loading_result: None,
            })),
        }
    }
}

#[async_trait]
impl Component for ImportExportPickerComponent {
    fn name(&self) -> &'static str {
        "ImportExportPickerComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // 10 Items
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
            ImportExportPickerComponentMode::Import { input } => {
                self.state.write().is_loading = true;

                // Spawn a background task to fetch items, which can be slow on ai mode
                let this = self.clone();
                let input = input.clone();
                tokio::spawn(async move {
                    // Fetch items from the given import location
                    let items: Result<Vec<ImportExportItem>, AppError> = match this
                        .service
                        .get_items_from_location(
                            input,
                            this.config.gist.clone(),
                            this.global_cancellation_token.clone(),
                        )
                        .await
                    {
                        Ok(c) => c.try_collect().await,
                        Err(err) => Err(err),
                    };
                    match items {
                        Ok(items) => {
                            // If items were fetched successfully, update the state
                            let mut state = this.state.write();
                            if items.is_empty() {
                                state.loading_result = Some(Ok(ProcessOutput::fail().stderr(format_error!(
                                    this.config.theme,
                                    "No commands or completions were found"
                                ))));
                            } else {
                                state.items.update_items(
                                    items.into_iter().map(PlainStyleImportExportItem::from).collect(),
                                    false,
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
            ImportExportPickerComponentMode::Export { input } => {
                // Prepare items for export
                let res = match self.service.prepare_items_export(input.filter.clone()).await {
                    Ok(s) => s.try_collect().await,
                    Err(err) => Err(err),
                };
                let items: Vec<ImportExportItem> = match res {
                    Ok(c) => c,
                    Err(AppError::UserFacing(err)) => {
                        return Ok(Action::Quit(
                            ProcessOutput::fail().stderr(format_error!(self.config.theme, "{err}")),
                        ));
                    }
                    Err(AppError::Unexpected(report)) => return Err(report),
                };

                if items.is_empty() {
                    return Ok(Action::Quit(ProcessOutput::fail().stderr(format_error!(
                        self.config.theme,
                        "No commands or completions to export"
                    ))));
                } else {
                    let mut state = self.state.write();
                    state
                        .items
                        .update_items(items.into_iter().map(PlainStyleImportExportItem::from).collect(), false);
                }
            }
        }

        // Mark the component as initialized, as it doesn't fetch items again on component switch
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
            // Render the items list when not loading
            frame.render_widget(&mut state.items, main_area);
        }

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.poll_new_version() {
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
            let mut state = self.state.write();
            if key.modifiers == KeyModifiers::CONTROL {
                state.items.toggle_discard_all();
            } else {
                state.items.toggle_discard_selected();
            }
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
        state.items.select_prev();
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.items.select_next();
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
            state.items.select_first();
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        if absolute {
            state.items.select_last();
        }
        Ok(Action::NoOp)
    }

    async fn selection_delete(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.items.delete_selected();
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        // Do nothing if the component is in a loading state
        if self.state.read().is_loading {
            return Ok(Action::NoOp);
        }

        // Get the selected item and its index, if any
        let selected_data = {
            let state = self.state.read();
            state
                .items
                .selected_with_index()
                .map(|(index, item)| (index, ImportExportItem::from(item.clone())))
        };

        if let Some((index, item)) = selected_data {
            // Clone the current component to serve as the parent to return to after editing is done
            let parent_component = Box::new(self.clone());

            let this = self.clone();
            match item {
                ImportExportItem::Command(command) => {
                    // Prepare the callback to be run after the command is updated
                    let callback = Arc::new(move |updated_command: Command| -> Result<()> {
                        let mut state = this.state.write();

                        // Replace the old widget with the new one at the same position in the list
                        if let Some(widget_ref) = state.items.items_mut().get_mut(index) {
                            *widget_ref = PlainStyleImportExportItem::Command(updated_command.into());
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
                        self.global_cancellation_token.clone(),
                    ))))
                }
                ImportExportItem::Completion(completion) => {
                    // Prepare the callback to be run after the completion is updated
                    let callback = Arc::new(move |updated_completion: VariableCompletion| -> Result<()> {
                        let mut state = this.state.write();

                        // Replace the old widget with the new one at the same position in the list
                        if let Some(widget_ref) = state.items.items_mut().get_mut(index) {
                            *widget_ref = PlainStyleImportExportItem::Completion(updated_completion.into());
                        }

                        Ok(())
                    });

                    // Switch to the editor component
                    Ok(Action::SwitchComponent(Box::new(EditCompletionComponent::new(
                        self.service.clone(),
                        self.config.theme.clone(),
                        self.inline,
                        completion,
                        EditCompletionComponentMode::EditMemory {
                            parent: parent_component,
                            callback,
                        },
                        self.global_cancellation_token.clone(),
                    ))))
                }
            }
        } else {
            // No item was selected, do nothing
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        // Collect all items that were NOT discarded by the user
        let non_discarded_items: Vec<ImportExportItem> = {
            let state = self.state.read();
            // Do nothing if the component is in a loading state
            if state.is_loading {
                return Ok(Action::NoOp);
            }
            state
                .items
                .non_discarded_items()
                .cloned()
                .map(ImportExportItem::from)
                .collect()
        };
        match &self.mode {
            ImportExportPickerComponentMode::Import { input } => {
                let output = if input.dry_run {
                    // If dry run, just print the items to the console
                    let mut items = String::new();
                    for item in non_discarded_items {
                        items += &item.to_string();
                        items += "\n";
                    }
                    if items.is_empty() {
                        ProcessOutput::fail().stderr(format_error!(
                            &self.config.theme,
                            "No commands or completions were found"
                        ))
                    } else {
                        ProcessOutput::success().stdout(items)
                    }
                } else {
                    // If not dry run, import the items
                    match self
                        .service
                        .import_items(stream::iter(non_discarded_items.into_iter().map(Ok)).boxed(), false)
                        .await
                    {
                        Ok(stats) => stats.into_output(&self.config.theme),
                        Err(AppError::UserFacing(err)) => {
                            ProcessOutput::fail().stderr(format_error!(&self.config.theme, "{err}"))
                        }
                        Err(AppError::Unexpected(report)) => return Err(report),
                    }
                };

                Ok(Action::Quit(output))
            }
            ImportExportPickerComponentMode::Export { input } => {
                match self
                    .service
                    .export_items(
                        stream::iter(non_discarded_items.into_iter().map(Ok)).boxed(),
                        input.clone(),
                        self.config.gist.clone(),
                    )
                    .await
                {
                    Ok(stats) => Ok(Action::Quit(stats.into_output(&self.config.theme))),
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
