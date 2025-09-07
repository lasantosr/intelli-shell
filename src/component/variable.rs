use std::{cmp::Ordering, collections::HashSet, sync::Arc};

use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{MouseEvent, MouseEventKind};
use futures_util::StreamExt;
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
    model::{CommandTemplate, VariableValue},
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{
        CommandTemplateWidget, CustomList, CustomTextArea, ErrorPopup, LoadingSpinner, NewVersionBanner,
        items::VariableSuggestionItem,
    },
};

/// A component for replacing the variables of a command
pub struct VariableReplacementComponent {
    /// Visual theme for styling the component
    theme: Theme,
    /// Whether the TUI is in inline mode or not
    inline: bool,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// Layout for arranging the input fields
    layout: Layout,
    /// Whether the command must be executed after replacing thw variables or just output it
    execute_mode: bool,
    /// Whether this component is part of the replace process (or maybe rendered after another process)
    replace_process: bool,
    /// Cancellation token for the background completions task
    cancellation_token: CancellationToken,
    /// The state of the component
    state: Arc<RwLock<VariableReplacementComponentState<'static>>>,
}
struct VariableReplacementComponentState<'a> {
    /// The command with variables to be replaced
    template: CommandTemplateWidget,
    /// Flat name of the current variable being set
    flat_variable_name: String,
    /// Full list of suggestions for the current variable
    variable_suggestions: Vec<VariableSuggestionItem<'static>>,
    /// Widget list of filtered suggestions for the variable value
    suggestions: CustomList<'a, VariableSuggestionItem<'a>>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
    /// A spinner to indicate that completions are being fetched
    loading: Option<LoadingSpinner<'a>>,
}

impl VariableReplacementComponent {
    /// Creates a new [`VariableReplacementComponent`]
    pub fn new(
        service: IntelliShellService,
        theme: Theme,
        inline: bool,
        execute_mode: bool,
        replace_process: bool,
        command: CommandTemplate,
    ) -> Self {
        let command = CommandTemplateWidget::new(&theme, inline, command);

        let suggestions = CustomList::new(theme.clone(), inline, Vec::new());

        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        Self {
            theme,
            inline,
            service,
            layout,
            execute_mode,
            replace_process,
            cancellation_token: CancellationToken::new(),
            state: Arc::new(RwLock::new(VariableReplacementComponentState {
                template: command,
                flat_variable_name: String::new(),
                variable_suggestions: Vec::new(),
                suggestions,
                error,
                loading: None,
            })),
        }
    }
}

#[async_trait]
impl Component for VariableReplacementComponent {
    fn name(&self) -> &'static str {
        "VariableReplacementComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // Command + Values
        1 + 5
    }

    #[instrument(skip_all)]
    async fn init_and_peek(&mut self) -> Result<Action> {
        self.update_variable_context(true).await
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [cmd_area, suggestions_area] = self.layout.areas(area);

        let mut state = self.state.write();

        // Render the command widget
        frame.render_widget(&state.template, cmd_area);

        // Render the suggestions
        frame.render_widget(&mut state.suggestions, suggestions_area);

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.check_new_version() {
            NewVersionBanner::new(&self.theme, new_version).render_in(frame, area);
        }
        state.error.render_in(frame, area);
        // Display the loading spinner, if any
        if let Some(loading) = &state.loading {
            let loading_area = if self.inline {
                Rect {
                    x: suggestions_area.x,
                    y: suggestions_area.y + suggestions_area.height.saturating_sub(1),
                    width: 1,
                    height: 1,
                }
            } else {
                Rect {
                    x: suggestions_area.x.saturating_add(1),
                    y: suggestions_area.y + suggestions_area.height.saturating_sub(2),
                    width: 1,
                    height: 1,
                }
            };
            loading.render_in(frame, loading_area);
        }
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.error.tick();
        if let Some(loading) = &mut state.loading {
            loading.tick();
        }

        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        self.cancellation_token.cancel();
        let mut state = self.state.write();
        if let Some(VariableSuggestionItem::Existing { editing, .. }) = state.suggestions.selected_mut()
            && editing.is_some()
        {
            tracing::debug!("Closing variable value edit mode: user request");
            *editing = None;
            Ok(Action::NoOp)
        } else {
            tracing::info!("User requested to exit");
            Ok(Action::Quit(
                ProcessOutput::success().fileout(state.template.to_string()),
            ))
        }
    }

    fn process_mouse_event(&mut self, mouse: MouseEvent) -> Result<Action> {
        match mouse.kind {
            MouseEventKind::ScrollDown => Ok(self.move_next()?),
            MouseEventKind::ScrollUp => Ok(self.move_prev()?),
            _ => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected() {
            Some(VariableSuggestionItem::Existing { editing: Some(_), .. }) => (),
            _ => state.suggestions.select_prev(),
        }
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected() {
            Some(VariableSuggestionItem::Existing { editing: Some(_), .. }) => (),
            _ => state.suggestions.select_next(),
        }
        Ok(Action::NoOp)
    }

    fn move_left(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.move_cursor_left(word);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.move_cursor_left(word);
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn move_right(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.move_cursor_right(word);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.move_cursor_right(word);
            }
            _ => (),
        }
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
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.move_home(absolute);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.move_home(absolute);
            }
            _ => state.suggestions.select_first(),
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.move_end(absolute);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.move_end(absolute);
            }
            _ => state.suggestions.select_last(),
        }
        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New {
                textarea, is_secret, ..
            }) => {
                textarea.undo();
                if !*is_secret {
                    let query = textarea.lines_as_string();
                    state.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.undo();
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New {
                textarea, is_secret, ..
            }) => {
                textarea.redo();
                if !*is_secret {
                    let query = textarea.lines_as_string();
                    state.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.redo();
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, mut text: String) -> Result<Action> {
        let mut state = self.state.write();
        if let Some(variable) = state.template.current_variable() {
            text = variable.apply_functions_to(text);
        }
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New {
                textarea, is_secret, ..
            }) => {
                textarea.insert_str(text);
                if !*is_secret {
                    let query = textarea.lines_as_string();
                    state.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.insert_str(text);
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        let mut state = self.state.write();
        let maybe_replacement = state
            .template
            .current_variable()
            .and_then(|variable| variable.check_functions_char(c));
        let insert_content = |ta: &mut CustomTextArea<'_>| {
            if let Some(r) = &maybe_replacement {
                ta.insert_str(r);
            } else {
                ta.insert_char(c);
            }
        };
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New {
                textarea, is_secret, ..
            }) => {
                insert_content(textarea);
                if !*is_secret {
                    let query = textarea.lines_as_string();
                    state.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                insert_content(ta);
            }
            _ => {
                if let Some(VariableSuggestionItem::New { .. }) = state.suggestions.items().iter().next() {
                    state.suggestions.select_first();
                    if let Some(VariableSuggestionItem::New {
                        textarea, is_secret, ..
                    }) = state.suggestions.selected_mut()
                    {
                        insert_content(textarea);
                        if !*is_secret {
                            let query = textarea.lines_as_string();
                            state.filter_suggestions(&query);
                        }
                    }
                }
            }
        }
        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New {
                textarea, is_secret, ..
            }) => {
                textarea.delete(backspace, word);
                if !*is_secret {
                    let query = textarea.lines_as_string();
                    state.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.delete(backspace, word);
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_delete(&mut self) -> Result<Action> {
        let deleted_id = {
            let mut state = self.state.write();
            match state.suggestions.selected_mut() {
                Some(VariableSuggestionItem::New { .. }) => return Ok(Action::NoOp),
                Some(VariableSuggestionItem::Existing {
                    value: VariableValue { id: Some(id), .. },
                    editing,
                    ..
                }) => {
                    if editing.is_none() {
                        let id = *id;
                        state.suggestions.delete_selected();
                        id
                    } else {
                        return Ok(Action::NoOp);
                    }
                }
                _ => {
                    state.error.set_temp_message("This value is not yet stored");
                    return Ok(Action::NoOp);
                }
            }
        };

        self.service
            .delete_variable_value(deleted_id)
            .await
            .map_err(AppError::into_report)?;

        self.state
            .write()
            .variable_suggestions
            .retain(|s| !matches!(s, VariableSuggestionItem::Existing { value, .. } if value.id == Some(deleted_id)));

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        let mut state = self.state.write();

        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { .. }) => (),
            Some(i @ VariableSuggestionItem::Existing { .. }) => {
                if let VariableSuggestionItem::Existing { value, editing, .. } = i {
                    if let Some(id) = value.id {
                        if editing.is_none() {
                            tracing::debug!("Entering edit mode for existing variable value: {id}");
                            i.enter_edit_mode();
                        }
                    } else {
                        state.error.set_temp_message("This value is not yet stored");
                    }
                }
            }
            _ => state.error.set_temp_message("This value is not yet stored"),
        }
        Ok(Action::NoOp)
    }

    async fn selection_confirm(&mut self) -> Result<Action> {
        self.cancellation_token.cancel();

        // Helper enum to hold the data extracted from the lock
        enum NextAction {
            NoOp,
            ConfirmNewSecret(String),
            ConfirmNewRegular(String),
            ConfirmExistingEdition(VariableValue, String),
            ConfirmExistingValue(VariableValue),
            ConfirmLiteral(String, bool),
        }

        let next_action = {
            let mut state = self.state.write();
            match state.suggestions.selected_mut() {
                None => NextAction::NoOp,
                Some(VariableSuggestionItem::New {
                    textarea,
                    is_secret: true,
                    ..
                }) => NextAction::ConfirmNewSecret(textarea.lines_as_string()),
                Some(VariableSuggestionItem::New {
                    textarea,
                    is_secret: false,
                    ..
                }) => NextAction::ConfirmNewRegular(textarea.lines_as_string()),
                Some(VariableSuggestionItem::Existing { value, editing, .. }) => match editing.take() {
                    Some(ta) => NextAction::ConfirmExistingEdition(value.clone(), ta.lines_as_string()),
                    None => NextAction::ConfirmExistingValue(value.clone()),
                },
                Some(VariableSuggestionItem::Environment {
                    content,
                    is_value: false,
                    ..
                }) => NextAction::ConfirmLiteral(content.clone(), false),
                Some(VariableSuggestionItem::Environment {
                    content: value,
                    is_value: true,
                    ..
                })
                | Some(VariableSuggestionItem::Completion { value, .. })
                | Some(VariableSuggestionItem::Derived { value, .. }) => {
                    NextAction::ConfirmLiteral(value.clone(), true)
                }
            }
        };

        match next_action {
            NextAction::NoOp => Ok(Action::NoOp),
            NextAction::ConfirmNewSecret(value) => self.confirm_new_secret_value(value).await,
            NextAction::ConfirmNewRegular(value) => self.confirm_new_regular_value(value).await,
            NextAction::ConfirmExistingEdition(val, new_val) => self.confirm_existing_edition(val, new_val).await,
            NextAction::ConfirmExistingValue(val) => self.confirm_existing_value(val, false).await,
            NextAction::ConfirmLiteral(val, is_value) => self.confirm_literal_value(val, is_value).await,
        }
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }
}

impl<'a> VariableReplacementComponentState<'a> {
    /// Filters the suggestions widget based on the query
    fn filter_suggestions(&mut self, query: &str) {
        tracing::debug!("Filtering suggestions for: {query}");
        // From the original variable suggestions, keep those matching the query only
        let mut filtered_suggestions = self.variable_suggestions.clone();
        filtered_suggestions.retain(|s| match s {
            VariableSuggestionItem::New { .. } => false,
            VariableSuggestionItem::Existing { value, .. } => value.value.contains(query),
            VariableSuggestionItem::Environment { content: value, .. }
            | VariableSuggestionItem::Completion { value, .. }
            | VariableSuggestionItem::Derived { value, .. } => value.contains(query),
        });

        // Find and insert the new row, which contains the query
        let new_row = self
            .suggestions
            .items()
            .iter()
            .find(|s| matches!(s, VariableSuggestionItem::New { .. }));
        if let Some(new_row) = new_row.cloned() {
            filtered_suggestions.insert(0, new_row);
        }
        // Retrieve the identifier for the selected item
        let selected_id = self.suggestions.selected().map(|s| s.identifier());
        // Update the items
        self.suggestions.update_items(filtered_suggestions, false);
        // Restore the same selected item
        if let Some(selected_id) = selected_id {
            self.suggestions.select_matching(|i| i.identifier() == selected_id);
        }
    }
}

impl VariableReplacementComponent {
    /// Updates the variable context and the suggestions widget, or returns an acton
    async fn update_variable_context(&mut self, peek: bool) -> Result<Action> {
        // Cancels previous completion task
        self.cancellation_token.cancel();
        self.cancellation_token = CancellationToken::new();

        // Retrieves the current variable and its context
        let (flat_root_cmd, current_variable, context) = {
            let state = self.state.read();
            match state.template.current_variable().cloned() {
                Some(variable) => (
                    state.template.flat_root_cmd.clone(),
                    variable,
                    state.template.current_variable_context(),
                ),
                None => {
                    if peek {
                        tracing::info!("There are no variables to replace");
                    } else {
                        tracing::info!("There are no more variables");
                    }
                    return self.quit_action(peek, state.template.to_string());
                }
            }
        };

        // Search for the variable suggestions
        let (initial_suggestions, completion_stream) = self
            .service
            .search_variable_suggestions(&flat_root_cmd, &current_variable, context)
            .await
            .map_err(AppError::into_report)?;

        // Update the context
        let mut state = self.state.write();
        let suggestions = initial_suggestions
            .into_iter()
            .map(VariableSuggestionItem::from)
            .collect::<Vec<_>>();
        state.flat_variable_name = current_variable.flat_name.clone();
        state.variable_suggestions = suggestions.clone();

        // And the displayed items
        state.suggestions.update_items(suggestions, false);

        // Pre-select the first non-derived suggestion
        if let Some(idx) = state.suggestions.items().iter().position(|s| {
            !matches!(
                s,
                VariableSuggestionItem::New { .. } | VariableSuggestionItem::Derived { .. }
            )
        }) {
            state.suggestions.select(idx);
        }

        // If there's some completions stream
        if let Some(mut stream) = completion_stream {
            let token = self.cancellation_token.clone();
            let state_clone = self.state.clone();

            // Show the loading spinner
            state.loading = Some(LoadingSpinner::new(&self.theme));

            // Spawn a background task to wait for them
            tokio::spawn(async move {
                while let Some((score_boost, result)) = tokio::select! {
                    biased;
                    _ = token.cancelled() => None,
                    item = stream.next() => item,
                } {
                    match result {
                        // If an error happens while resolving the completion, display the first line
                        Err(err) => {
                            let mut state = state_clone.write();
                            if let Some(line) = err.lines().next() {
                                state.error.set_temp_message(line.to_string());
                            }
                        }
                        // Otherwise, merge suggestions
                        Ok(completion_suggestions) => {
                            let mut state = state_clone.write();

                            // Retrieve the current set of suggestions
                            let master_suggestions = &mut state.variable_suggestions;

                            // Remove all `Derived` items that are about to be added as a `Completion`
                            let completion_set = completion_suggestions.iter().collect::<HashSet<_>>();
                            master_suggestions.retain_mut(|item| {
                                !matches!(
                                    item,
                                    VariableSuggestionItem::Derived { value, .. }
                                        if completion_set.contains(value)
                                )
                            });

                            // For each new suggestion given by the completion
                            for suggestion in completion_suggestions {
                                // Check if there's already a suggestion for the same value
                                let mut skip_completion = false;
                                for item in master_suggestions.iter_mut() {
                                    match item {
                                        // `New` items doesn't affect
                                        VariableSuggestionItem::New { .. } => (),
                                        // `Derived` are already handled above
                                        VariableSuggestionItem::Derived { .. } => (),
                                        // If already an environment, just skip completion
                                        VariableSuggestionItem::Environment { content, is_value, .. } => {
                                            if *is_value && content == &suggestion {
                                                skip_completion = true;
                                                break;
                                            }
                                        }
                                        // If already an existing, boost its score and skip completion
                                        VariableSuggestionItem::Existing {
                                            value,
                                            score,
                                            completion_merged,
                                            ..
                                        } => {
                                            if value.value == suggestion {
                                                if !*completion_merged {
                                                    *score += score_boost;
                                                    *completion_merged = true;
                                                }
                                                skip_completion = true;
                                                break;
                                            }
                                        }
                                        // If already a completion, keep the maximum score and skip this one
                                        VariableSuggestionItem::Completion { value, score, .. } => {
                                            if value == &suggestion {
                                                *score += score_boost.max(*score);
                                                skip_completion = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                                if skip_completion {
                                    continue;
                                }

                                // Add the new suggestion
                                master_suggestions.push(VariableSuggestionItem::Completion {
                                    sort_index: 3,
                                    value: suggestion,
                                    score: score_boost,
                                });
                            }

                            // Re-sort suggestions
                            master_suggestions.sort_by(|a, b| {
                                a.sort_index()
                                    .cmp(&b.sort_index())
                                    .then_with(|| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal))
                            });

                            // After sorting, filter suggestions
                            let query = state
                                .suggestions
                                .items()
                                .iter()
                                .find_map(|s| match s {
                                    VariableSuggestionItem::New {
                                        textarea,
                                        is_secret: false,
                                        ..
                                    } => Some(textarea.lines_as_string()),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            state.filter_suggestions(&query);
                        }
                    }
                }
                state_clone.write().loading = None;
            });
        }

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn confirm_new_secret_value(&mut self, value: String) -> Result<Action> {
        tracing::debug!("Secret variable value selected");
        self.state.write().template.set_next_variable(value);
        self.update_variable_context(false).await
    }

    #[instrument(skip_all)]
    async fn confirm_new_regular_value(&mut self, value: String) -> Result<Action> {
        if !value.trim().is_empty() {
            let variable_value = {
                let state = self.state.read();
                state.template.new_variable_value_for(&state.flat_variable_name, &value)
            };
            match self.service.insert_variable_value(variable_value).await {
                Ok(v) => {
                    tracing::debug!("New variable value stored");
                    self.confirm_existing_value(v, true).await
                }
                Err(AppError::UserFacing(err)) => {
                    tracing::warn!("{err}");
                    self.state.write().error.set_temp_message(err.to_string());
                    Ok(Action::NoOp)
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        } else {
            tracing::debug!("New empty variable value selected");
            self.state.write().template.set_next_variable(value);
            self.update_variable_context(false).await
        }
    }

    #[instrument(skip_all)]
    async fn confirm_existing_edition(&mut self, mut value: VariableValue, new_value: String) -> Result<Action> {
        value.value = new_value;
        match self.service.update_variable_value(value).await {
            Ok(v) => {
                let mut state = self.state.write();
                if let VariableSuggestionItem::Existing { value, .. } = state.suggestions.selected_mut().unwrap() {
                    *value = v;
                };
                Ok(Action::NoOp)
            }
            Err(AppError::UserFacing(err)) => {
                tracing::warn!("{err}");
                self.state.write().error.set_temp_message(err.to_string());
                Ok(Action::NoOp)
            }
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }

    #[instrument(skip_all)]
    async fn confirm_existing_value(&mut self, mut value: VariableValue, new: bool) -> Result<Action> {
        let value_id = match value.id {
            Some(id) => id,
            None => {
                value = self
                    .service
                    .insert_variable_value(value)
                    .await
                    .map_err(AppError::into_report)?;
                value.id.expect("just inserted")
            }
        };
        let context = self.state.read().template.current_variable_context();
        match self
            .service
            .increment_variable_value_usage(value_id, context)
            .await
            .map_err(AppError::into_report)
        {
            Ok(_) => {
                if !new {
                    tracing::debug!("Existing variable value selected");
                }
                self.state.write().template.set_next_variable(value.value);
                self.update_variable_context(false).await
            }
            Err(report) => Err(report),
        }
    }

    #[instrument(skip_all)]
    async fn confirm_literal_value(&mut self, value: String, store: bool) -> Result<Action> {
        if store && !value.trim().is_empty() {
            let variable_value = {
                let state = self.state.read();
                state.template.new_variable_value_for(&state.flat_variable_name, &value)
            };
            match self.service.insert_variable_value(variable_value).await {
                Ok(v) => {
                    tracing::debug!("Literal variable value selected and stored");
                    self.confirm_existing_value(v, true).await
                }
                Err(AppError::UserFacing(err)) => {
                    tracing::debug!("Literal variable value selected but couldn't be stored: {err}");
                    self.state.write().template.set_next_variable(value);
                    self.update_variable_context(false).await
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        } else {
            tracing::debug!("Literal variable value selected");
            self.state.write().template.set_next_variable(value);
            self.update_variable_context(false).await
        }
    }

    /// Returns an action to quit the component, with the current variable content
    fn quit_action(&self, peek: bool, cmd: String) -> Result<Action> {
        if self.execute_mode {
            Ok(Action::Quit(ProcessOutput::execute(cmd)))
        } else if self.replace_process && peek {
            Ok(Action::Quit(
                ProcessOutput::success()
                    .stderr(format_msg!(self.theme, "There are no variables to replace"))
                    .stdout(&cmd)
                    .fileout(cmd),
            ))
        } else {
            Ok(Action::Quit(ProcessOutput::success().stdout(&cmd).fileout(cmd)))
        }
    }
}
