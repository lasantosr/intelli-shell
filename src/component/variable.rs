use std::{
    cmp::Ordering,
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

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

// If there's a completion stream, process fast completions before showing the list
const INITIAL_COMPLETION_WAIT: Duration = Duration::from_millis(250);

/// A component for replacing the variables of a command
#[derive(Clone)]
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
    /// Global cancellation token
    global_cancellation_token: CancellationToken,
    /// Cancellation token for the background completions task
    cancellation_token: Arc<Mutex<Option<CancellationToken>>>,
    /// The state of the component
    state: Arc<RwLock<VariableReplacementComponentState<'static>>>,
}
struct VariableReplacementComponentState<'a> {
    /// The command with variables to be replaced
    template: CommandTemplateWidget,
    /// Flat name of the current variable being set and if it's secret
    current_variable_ctx: (String, bool),
    /// Full list of suggestions for the current variable
    variable_suggestions: Vec<VariableSuggestionItem<'static>>,
    /// Widget list of filtered suggestions for the variable value
    suggestions: CustomList<'a, VariableSuggestionItem<'a>>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
    /// A spinner to indicate that completions are being fetched
    loading: Option<LoadingSpinner<'a>>,
    /// Index of the current variable being edited (0-based)
    current_variable_index: usize,
    /// Stored values for all variables (Some = set, None = not set)
    variable_values: Vec<Option<String>>,
    /// Stack of indices reflecting the order variables were confirmed
    confirmed_variables: Vec<usize>,
    /// Stack of undone values so redo can reinstate them
    redo_stack: Vec<(usize, String)>,
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
        cancellation_token: CancellationToken,
    ) -> Self {
        let command = CommandTemplateWidget::new(&theme, inline, command);

        let suggestions = CustomList::new(theme.clone(), inline, Vec::new());

        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        // Initialize variable tracking
        let total_vars = command.count_variables();
        let variable_values = vec![None; total_vars];

        Self {
            theme,
            inline,
            service,
            layout,
            execute_mode,
            replace_process,
            cancellation_token: Arc::new(Mutex::new(None)),
            global_cancellation_token: cancellation_token,
            state: Arc::new(RwLock::new(VariableReplacementComponentState {
                template: command,
                current_variable_ctx: (String::new(), true),
                variable_suggestions: Vec::new(),
                suggestions,
                error,
                loading: None,
                current_variable_index: 0,
                variable_values,
                confirmed_variables: Vec::new(),
                redo_stack: Vec::new(),
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

        // Sync the template parts with the current variable values
        let values = state.variable_values.clone();
        state.template.set_variable_values(&values);

        // Sync the current variable index with the widget for highlighting
        state.template.current_variable_index = state.current_variable_index;

        // Render the command widget
        frame.render_widget(&state.template, cmd_area);

        // Render the suggestions
        frame.render_widget(&mut state.suggestions, suggestions_area);

        // Render the new version banner and error message as an overlay
        if let Some(new_version) = self.service.poll_new_version() {
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
        {
            let mut token_guard = self.cancellation_token.lock().unwrap();
            if let Some(token) = token_guard.take() {
                token.cancel();
            }
        }
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

    fn move_prev_variable(&mut self) -> Result<Action> {
        let mut state = self.state.write();

        // Don't navigate if editing an existing value
        if matches!(
            state.suggestions.selected(),
            Some(VariableSuggestionItem::Existing { editing: Some(_), .. })
        ) {
            return Ok(Action::NoOp);
        }

        let total_vars = state.template.count_variables();
        if total_vars <= 1 {
            return Ok(Action::NoOp);
        }

        // Move to previous variable with wrapping
        if state.current_variable_index == 0 {
            state.current_variable_index = total_vars - 1; // Wrap to last
        } else {
            state.current_variable_index -= 1;
        }

        drop(state);
        self.debounced_update_variable_context();
        Ok(Action::NoOp)
    }

    fn move_next_variable(&mut self) -> Result<Action> {
        let mut state = self.state.write();

        // Don't navigate if editing an existing value
        if matches!(
            state.suggestions.selected(),
            Some(VariableSuggestionItem::Existing { editing: Some(_), .. })
        ) {
            return Ok(Action::NoOp);
        }

        let total_vars = state.template.count_variables();
        if total_vars <= 1 {
            return Ok(Action::NoOp);
        }

        // Move to next variable with wrapping
        state.current_variable_index += 1;
        if state.current_variable_index >= total_vars {
            state.current_variable_index = 0; // Wrap to first
        }

        drop(state);
        self.debounced_update_variable_context();
        Ok(Action::NoOp)
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
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.undo();
                let query = textarea.lines_as_string();
                state.filter_suggestions(&query);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.undo();
            }
            _ => {
                if let Some(last_index) = state.confirmed_variables.pop()
                    && let Some(value) = state.variable_values[last_index].take()
                {
                    state.redo_stack.push((last_index, value));
                    state.current_variable_index = last_index;
                    self.debounced_update_variable_context();
                }
            }
        }
        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.redo();
                let query = textarea.lines_as_string();
                state.filter_suggestions(&query);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                ta.redo();
            }
            _ => {
                if let Some((index, value)) = state.redo_stack.pop() {
                    state.variable_values[index] = Some(value.clone());
                    state.confirmed_variables.push(index);
                    state.current_variable_index = index + 1;
                    self.debounced_update_variable_context();
                }
            }
        }
        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, mut text: String) -> Result<Action> {
        let mut state = self.state.write();
        let current_index = state.current_variable_index;
        if let Some(variable) = state.template.variable_at(current_index) {
            text = variable.apply_functions_to(text);
        }
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.insert_str(text);
                let query = textarea.lines_as_string();
                state.filter_suggestions(&query);
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
        let current_index = state.current_variable_index;
        let maybe_replacement = state
            .template
            .variable_at(current_index)
            .and_then(|variable| variable.check_functions_char(c));
        let insert_content = |ta: &mut CustomTextArea<'_>| {
            if let Some(r) = &maybe_replacement {
                ta.insert_str(r);
            } else {
                ta.insert_char(c);
            }
        };
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                insert_content(textarea);
                let query = textarea.lines_as_string();
                state.filter_suggestions(&query);
            }
            Some(VariableSuggestionItem::Existing { editing: Some(ta), .. }) => {
                insert_content(ta);
            }
            _ => {
                if let Some(VariableSuggestionItem::New { .. }) = state.suggestions.items().iter().next() {
                    state.suggestions.select_first();
                    if let Some(VariableSuggestionItem::New { textarea, .. }) = state.suggestions.selected_mut() {
                        insert_content(textarea);
                        let query = textarea.lines_as_string();
                        state.filter_suggestions(&query);
                    }
                }
            }
        }
        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match state.suggestions.selected_mut() {
            Some(VariableSuggestionItem::New { textarea, .. }) => {
                textarea.delete(backspace, word);
                let query = textarea.lines_as_string();
                state.filter_suggestions(&query);
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
        {
            let mut token_guard = self.cancellation_token.lock().unwrap();
            if let Some(token) = token_guard.take() {
                token.cancel();
            }
        }

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
            let (_, is_secret) = state.current_variable_ctx;
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
                | Some(VariableSuggestionItem::Previous { value, .. })
                | Some(VariableSuggestionItem::Completion { value, .. })
                | Some(VariableSuggestionItem::Derived { value, .. }) => {
                    NextAction::ConfirmLiteral(value.clone(), !is_secret)
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
            VariableSuggestionItem::Existing { value, .. } => value_matches_filter_query(&value.value, query),
            VariableSuggestionItem::Previous { value, .. }
            | VariableSuggestionItem::Environment { content: value, .. }
            | VariableSuggestionItem::Completion { value, .. }
            | VariableSuggestionItem::Derived { value, .. } => value_matches_filter_query(value, query),
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

    /// Merges a new set of completion suggestions into the master list, re-sorts, and re-filters
    fn merge_completions(&mut self, score_boost: f64, completion_suggestions: Vec<String>) {
        // Retrieve the current set of suggestions
        let master_suggestions = &mut self.variable_suggestions;

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
                    // `New` items don't affect completions
                    VariableSuggestionItem::New { .. } => (),
                    // `Derived` are already handled above
                    VariableSuggestionItem::Derived { .. } => (),
                    // If already a previous value, skip completion
                    VariableSuggestionItem::Previous { value, .. } => {
                        if value == &suggestion {
                            skip_completion = true;
                            break;
                        }
                    }
                    // If already an environment value, skip completion
                    VariableSuggestionItem::Environment { content, is_value, .. } => {
                        if *is_value && content == &suggestion {
                            skip_completion = true;
                            break;
                        }
                    }
                    // If already an existing value, boost its score and skip completion
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
                            *score = score.max(score_boost);
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

        // After sorting, filter suggestions based on the current query in the "new" item textarea
        let query = self
            .suggestions
            .items()
            .iter()
            .find_map(|s| match s {
                VariableSuggestionItem::New { textarea, .. } => Some(textarea.lines_as_string()),
                _ => None,
            })
            .unwrap_or_default();
        self.filter_suggestions(&query);
    }
}

impl VariableReplacementComponent {
    /// Immediately starts a debounced task to update the variable context
    fn debounced_update_variable_context(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(err) = this.update_variable_context(false).await {
                tracing::error!("Error updating variable context: {err:?}");
            }
        });
    }

    /// Moves to the next variable after confirming a value.
    /// It will wrap around if there are still pending variables.
    fn move_to_next_variable_with_value(&self, value: String) {
        let mut state = self.state.write();

        // Store the confirmed value
        let current_index = state.current_variable_index;
        state.variable_values[current_index] = Some(value);
        state.confirmed_variables.push(current_index);
        state.redo_stack.clear();

        // Move to the next variable index
        state.current_variable_index += 1;

        // Check if we are at the end
        if state.current_variable_index >= state.variable_values.len() {
            // Check if there are any pending variables
            let has_pending = state.variable_values.iter().any(|v| v.is_none());
            if has_pending {
                // Wrap around to the first variable
                state.current_variable_index = 0;
            }
        }
    }

    /// Updates the variable context and the suggestions widget, or returns an acton
    async fn update_variable_context(&self, peek: bool) -> Result<Action> {
        // Sync the template with current variable values before checking variables
        {
            let mut state = self.state.write();
            let values = state.variable_values.clone();
            state.template.set_variable_values(&values);
        }

        // Cancels previous completion task and issue a new one
        let cancellation_token = {
            let mut token_guard = self.cancellation_token.lock().unwrap();
            if let Some(token) = token_guard.take() {
                token.cancel();
            }
            let new_token = CancellationToken::new();
            *token_guard = Some(new_token.clone());
            new_token
        };

        // Retrieves the current variable and its context using the index
        let (flat_root_cmd, previous_values, current_variable, context, current_stored_value) = {
            let state = self.state.read();
            let current_index = state.current_variable_index;

            match state.template.variable_at(current_index).cloned() {
                Some(variable) => (
                    state.template.flat_root_cmd.clone(),
                    state.template.previous_values_for(&variable.flat_name),
                    variable,
                    state.template.variable_context(),
                    state.variable_values.get(current_index).and_then(|v| v.clone()),
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
            .search_variable_suggestions(&flat_root_cmd, &current_variable, previous_values, context)
            .await
            .map_err(AppError::into_report)?;

        // Update the context with initial suggestions
        {
            let mut state = self.state.write();
            let suggestions = initial_suggestions
                .into_iter()
                .map(VariableSuggestionItem::from)
                .collect::<Vec<_>>();
            state.current_variable_ctx = (current_variable.flat_name.clone(), current_variable.secret);
            state.variable_suggestions = suggestions.clone();
            state.suggestions.update_items(suggestions, false);
        }

        // If there's a completion stream, process fast completions before showing the list
        let remaining_stream = if let Some(mut stream) = completion_stream {
            let sleep = tokio::time::sleep(INITIAL_COMPLETION_WAIT);
            tokio::pin!(sleep);

            let mut has_more_items = true;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut sleep => {
                        tracing::debug!(
                            "There are pending completions after initial {}ms wait, spawning a background task",
                            INITIAL_COMPLETION_WAIT.as_millis()
                        );
                        break;
                    }
                    item = stream.next() => {
                        if let Some((score_boost, result)) = item {
                            match result {
                                // If an error happens while resolving the completion, display the first line
                                Err(err) => {
                                    if let Some(line) = err.lines().next() {
                                        self.state.write().error.set_temp_message(line.to_string());
                                    }
                                }
                                // Otherwise, merge suggestions
                                Ok(completion_suggestions) => {
                                    self.state.write().merge_completions(score_boost, completion_suggestions);
                                }
                            }
                        } else {
                            // Stream finished before timeout
                            tracing::debug!(
                                "All completions were resolved on the initial {}ms window",
                                INITIAL_COMPLETION_WAIT.as_millis()
                            );
                            has_more_items = false;
                            break;
                        }
                    }
                }
            }
            if has_more_items { Some(stream) } else { None }
        } else {
            None
        };

        // Pre-select based on current stored value or first non-derived suggestion
        {
            let mut state = self.state.write();

            // Try to find and select the currently stored value
            let mut selected = false;
            if let Some(ref stored_value) = current_stored_value
                && let Some(idx) = state.suggestions.items().iter().position(|item| match item {
                    VariableSuggestionItem::Existing { value, .. } => &value.value == stored_value,
                    VariableSuggestionItem::Previous { value, .. } => value == stored_value,
                    VariableSuggestionItem::Environment { content, .. } => content == stored_value,
                    VariableSuggestionItem::Completion { value, .. } => value == stored_value,
                    VariableSuggestionItem::Derived { value, .. } => value == stored_value,
                    VariableSuggestionItem::New { .. } => false,
                })
            {
                state.suggestions.select(idx);
                selected = true;
            }

            // If no stored value matched, select first non-derived suggestion
            if !selected
                && let Some(idx) = state.suggestions.items().iter().position(|s| {
                    !matches!(
                        s,
                        VariableSuggestionItem::New { .. } | VariableSuggestionItem::Derived { .. }
                    )
                })
            {
                state.suggestions.select(idx);
            }
        }

        // If there are still pending completions, spawn a background task for them
        if let Some(mut stream) = remaining_stream {
            let token = cancellation_token.clone();
            let global_token = self.global_cancellation_token.clone();
            let state_clone = self.state.clone();

            // Show the loading spinner
            self.state.write().loading = Some(LoadingSpinner::new(&self.theme));

            // Spawn a background task to wait for them
            tokio::spawn(async move {
                while let Some((score_boost, result)) = tokio::select! {
                    biased;
                    _ = token.cancelled() => None,
                    _ = global_token.cancelled() => None,
                    item = stream.next() => item,
                } {
                    match result {
                        // If an error happens while resolving the completion, display the first line
                        Err(err) => {
                            if let Some(line) = err.lines().next() {
                                state_clone.write().error.set_temp_message(line.to_string());
                            }
                        }
                        // Otherwise, merge suggestions
                        Ok(completion_suggestions) => {
                            state_clone
                                .write()
                                .merge_completions(score_boost, completion_suggestions);
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
        self.move_to_next_variable_with_value(value);
        self.update_variable_context(false).await
    }

    #[instrument(skip_all)]
    async fn confirm_new_regular_value(&mut self, value: String) -> Result<Action> {
        if !value.trim().is_empty() {
            let variable_value = {
                let state = self.state.read();
                let (flat_variable_name, _) = &state.current_variable_ctx;
                state.template.new_variable_value_for(flat_variable_name, &value)
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
            self.move_to_next_variable_with_value(value);
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
        let context = self.state.read().template.variable_context();
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
                self.move_to_next_variable_with_value(value.value);
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
                let (flat_variable_name, _) = &state.current_variable_ctx;
                state.template.new_variable_value_for(flat_variable_name, &value)
            };
            match self.service.insert_variable_value(variable_value).await {
                Ok(v) => {
                    tracing::debug!("Literal variable value selected and stored");
                    self.confirm_existing_value(v, true).await
                }
                Err(AppError::UserFacing(err)) => {
                    tracing::debug!("Literal variable value selected but couldn't be stored: {err}");
                    self.move_to_next_variable_with_value(value);
                    self.update_variable_context(false).await
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        } else {
            tracing::debug!("Literal variable value selected");
            self.move_to_next_variable_with_value(value);
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

/// Checks if `value` contains all space-separated words from `query` in the same order.
///
/// This is a case-sensitive search.
///
/// ### Arguments
///
/// * `value`: The string to search within.
/// * `query`: A string of space-separated words to find in `value`.
///
/// ### Returns
///
/// `true` if all words in `query` are found in `value` in the correct order,
/// `false` otherwise.
fn value_matches_filter_query(value: &str, query: &str) -> bool {
    // This offset tracks our position in the `value` string
    let mut search_offset = 0;
    query.split_whitespace().all(|word| {
        // We search for the current `word` only in the slice of `value` that starts from our current `search_offset`
        if let Some(relative_pos) = value[search_offset..].find(word) {
            // If the word is found, we update the offset for the next search
            search_offset += relative_pos + 1;
            true
        } else {
            // If the word isn't found, return `false` immediately
            false
        }
    })
}
