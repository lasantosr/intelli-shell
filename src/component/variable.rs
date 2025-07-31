use std::ops::DerefMut;

use async_trait::async_trait;
use color_eyre::{Result, eyre::eyre};
use crossterm::event::{MouseEvent, MouseEventKind};
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
    model::{DynamicCommand, VariableSuggestion, VariableValue},
    process::ProcessOutput,
    service::IntelliShellService,
    utils::format_env_var,
    widgets::{
        CustomList, CustomTextArea, DynamicCommandWidget, ErrorPopup, ExistingVariableValue, LiteralVariableValue,
        NewVariableValue, NewVersionBanner, VariableSuggestionRow,
    },
};

/// A component for replacing the variables of a command
pub struct VariableReplacementComponent {
    /// Visual theme for styling the component
    theme: Theme,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// Layout for arranging the input fields
    layout: Layout,
    /// Whether the command must be executed after replacing thw variables or just output it
    execute_mode: bool,
    /// Whether this component is part of the replace process (or maybe rendered after another process)
    replace_process: bool,
    /// The command with variables to be replaced
    command: DynamicCommandWidget,
    /// Context of the current variable of the command or `None` if there are no more variables to replace
    variable_ctx: Option<CurrentVariableContext>,
    /// Widget list of filtered suggestions for the variable value
    suggestions: CustomList<'static, VariableSuggestionRow<'static>>,
    /// The new version banner
    new_version: NewVersionBanner,
    /// Popup for displaying error messages
    error: ErrorPopup<'static>,
}
struct CurrentVariableContext {
    /// Name of the variable being replaced
    name: String,
    /// Full list of suggestions for the variable value
    suggestions: Vec<VariableSuggestionRow<'static>>,
}

impl VariableReplacementComponent {
    /// Creates a new [`VariableReplacementComponent`]
    pub fn new(
        service: IntelliShellService,
        theme: Theme,
        inline: bool,
        execute_mode: bool,
        replace_process: bool,
        new_version: Option<Version>,
        command: DynamicCommand,
    ) -> Self {
        let command = DynamicCommandWidget::new(&theme, inline, command);

        let suggestions = CustomList::new(theme.primary, inline, Vec::new())
            .highlight_symbol(theme.highlight_symbol.clone())
            .highlight_symbol_style(theme.highlight_primary_full().into());

        let new_version = NewVersionBanner::new(&theme, new_version);
        let error = ErrorPopup::empty(&theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        Self {
            theme,
            service,
            layout,
            execute_mode,
            replace_process,
            command,
            variable_ctx: None,
            suggestions,
            new_version,
            error,
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
        1 + 3
    }

    async fn init(&mut self) -> Result<()> {
        self.update_variable_context(false).await?;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn peek(&mut self) -> Result<Action> {
        if self.variable_ctx.is_none() {
            tracing::info!("The command has no variables to replace");
            self.quit_action(true)
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [cmd_area, suggestions_area] = self.layout.areas(area);

        // Render the command widget
        frame.render_widget(&self.command, cmd_area);

        // Render the suggestions
        frame.render_widget(&mut self.suggestions, suggestions_area);

        // Render the new version banner and error message as an overlay
        self.new_version.render_in(frame, area);
        self.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        self.error.tick();

        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Option<ProcessOutput>> {
        if let Some(VariableSuggestionRow::Existing(e)) = self.suggestions.selected_mut() {
            if e.editing.is_some() {
                tracing::debug!("Closing variable value edit mode: user request");
                e.editing = None;
                return Ok(None);
            }
        }
        tracing::info!("User requested to exit");
        Ok(Some(ProcessOutput::success().fileout(self.command.to_string())))
    }

    fn process_mouse_event(&mut self, mouse: MouseEvent) -> Result<Action> {
        match mouse.kind {
            MouseEventKind::ScrollDown => Ok(self.move_next()?),
            MouseEventKind::ScrollUp => Ok(self.move_prev()?),
            _ => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        match self.suggestions.selected() {
            Some(VariableSuggestionRow::Existing(e)) if e.editing.is_some() => (),
            _ => self.suggestions.select_prev(),
        }
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        match self.suggestions.selected() {
            Some(VariableSuggestionRow::Existing(e)) if e.editing.is_some() => (),
            _ => self.suggestions.select_next(),
        }
        Ok(Action::NoOp)
    }

    fn move_left(&mut self, word: bool) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.move_cursor_left(word);
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.move_cursor_left(word);
                }
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn move_right(&mut self, word: bool) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.move_cursor_right(word);
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.move_cursor_right(word);
                }
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
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.move_home(absolute);
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.move_home(absolute);
                }
            }
            _ => self.suggestions.select_first(),
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.move_end(absolute);
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.move_end(absolute);
                }
            }
            _ => self.suggestions.select_last(),
        }
        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.undo();
                if !n.is_secret() {
                    let query = n.lines_as_string();
                    self.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.undo();
                }
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.redo();
                if !n.is_secret() {
                    let query = n.lines_as_string();
                    self.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.redo();
                }
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, mut text: String) -> Result<Action> {
        if let Some(variable) = self.command.current_variable() {
            text = variable.apply_functions_to(text);
        }
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.insert_str(text);
                if !n.is_secret() {
                    let query = n.lines_as_string();
                    self.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.insert_str(text);
                }
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        let insert_content = |ta: &mut CustomTextArea<'_>| {
            if let Some(variable) = self.command.current_variable()
                && let Some(r) = variable.check_functions_char(c)
            {
                ta.insert_str(&r);
            } else {
                ta.insert_char(c);
            }
        };
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                insert_content(n.deref_mut());
                if !n.is_secret() {
                    let query = n.lines_as_string();
                    self.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionRow::Existing(e)) if e.editing.is_some() => {
                if let Some(ref mut ta) = e.editing {
                    insert_content(ta);
                }
            }
            _ => {
                if let Some(VariableSuggestionRow::New(_)) = self.suggestions.items().iter().next() {
                    self.suggestions.select_first();
                    if let Some(VariableSuggestionRow::New(n)) = self.suggestions.selected_mut() {
                        insert_content(n.deref_mut());
                        if !n.is_secret() {
                            let query = n.lines_as_string();
                            self.filter_suggestions(&query);
                        }
                    }
                }
            }
        }
        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::New(n)) => {
                n.delete(backspace, word);
                if !n.is_secret() {
                    let query = n.lines_as_string();
                    self.filter_suggestions(&query);
                }
            }
            Some(VariableSuggestionRow::Existing(e)) => {
                if let Some(ref mut ta) = e.editing {
                    ta.delete(backspace, word);
                }
            }
            _ => (),
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_delete(&mut self) -> Result<Action> {
        let suggestion = match self.suggestions.selected_mut() {
            Some(VariableSuggestionRow::Existing(e)) if e.editing.is_none() => self.suggestions.delete_selected(),
            _ => return Ok(Action::NoOp),
        };

        let Some(VariableSuggestionRow::Existing(e)) = suggestion else {
            return Err(eyre!("Unexpected selected suggestion after removal"));
        };

        self.service.delete_variable_value(e.value.id.unwrap()).await?;

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        if let Some(VariableSuggestionRow::Existing(e)) = self.suggestions.selected_mut()
            && e.editing.is_none()
        {
            tracing::debug!(
                "Entering edit mode for existing variable value: {}",
                e.value.id.unwrap_or_default()
            );
            e.enter_edit_mode();
        }
        Ok(Action::NoOp)
    }

    async fn selection_confirm(&mut self) -> Result<Action> {
        match self.suggestions.selected_mut() {
            None => Ok(Action::NoOp),
            Some(VariableSuggestionRow::New(n)) if n.is_secret() => {
                let value = n.lines_as_string();
                self.confirm_new_secret_value(value).await
            }
            Some(VariableSuggestionRow::New(n)) => {
                let value = n.lines_as_string();
                self.confirm_new_regular_value(value).await
            }
            Some(VariableSuggestionRow::Existing(e)) => match e.editing.take() {
                Some(ta) => {
                    let value = e.value.clone();
                    let new_value = ta.lines_as_string();
                    self.confirm_existing_edition(value, new_value).await
                }
                None => {
                    let value = e.value.clone();
                    self.confirm_existing_value(value, false).await
                }
            },
            Some(VariableSuggestionRow::Environment(l, false)) => {
                let value = l.to_string();
                self.confirm_literal_value(value, false).await
            }
            Some(VariableSuggestionRow::Environment(l, true)) | Some(VariableSuggestionRow::Derived(l)) => {
                let value = l.to_string();
                self.confirm_literal_value(value, true).await
            }
        }
    }
}

impl VariableReplacementComponent {
    /// Filters the suggestions widget based on the query
    fn filter_suggestions(&mut self, query: &str) {
        if let Some(ref mut ctx) = self.variable_ctx {
            tracing::debug!("Filtering suggestions for: {query}");
            // From the original variable suggestions, keep those matching the query only
            let mut filtered_suggestions = ctx.suggestions.clone();
            filtered_suggestions.retain(|s| match s {
                VariableSuggestionRow::New(_) => false,
                VariableSuggestionRow::Existing(e) => e.value.value.contains(query),
                VariableSuggestionRow::Environment(l, _) | VariableSuggestionRow::Derived(l) => l.contains(query),
            });
            // Find and insert the new row, which contains the query
            let new_row = self
                .suggestions
                .items()
                .iter()
                .find(|s| matches!(s, VariableSuggestionRow::New(_)));
            if let Some(new_row) = new_row.cloned() {
                filtered_suggestions.insert(0, new_row);
            }
            // Update the items
            self.suggestions.update_items(filtered_suggestions);
        } else if !self.suggestions.is_empty() {
            self.suggestions.update_items(Vec::new());
        }
    }

    /// Updates the variable context and the suggestions widget, or returns an acton
    async fn update_variable_context(&mut self, quit_action: bool) -> Result<Action> {
        let Some(current_variable) = self.command.current_variable() else {
            if quit_action {
                tracing::info!("There are no more variables");
                return self.quit_action(false);
            } else {
                return Ok(Action::NoOp);
            }
        };

        // Search for suggestions
        let suggestions = self
            .service
            .search_variable_suggestions(
                &self.command.root,
                current_variable,
                self.command.current_variable_context(),
            )
            .await?;

        // Map the suggestions to the widget rows
        let suggestion_widgets = suggestions
            .into_iter()
            .map(|s| match s {
                VariableSuggestion::Secret => VariableSuggestionRow::New(NewVariableValue::new(&self.theme, true)),
                VariableSuggestion::New => VariableSuggestionRow::New(NewVariableValue::new(&self.theme, false)),
                VariableSuggestion::Environment { env_var_name, value } => {
                    if let Some(value) = value {
                        VariableSuggestionRow::Environment(LiteralVariableValue::new(&self.theme, value), true)
                    } else {
                        VariableSuggestionRow::Environment(
                            LiteralVariableValue::new(&self.theme, format_env_var(env_var_name)),
                            false,
                        )
                    }
                }
                VariableSuggestion::Existing(value) => {
                    VariableSuggestionRow::Existing(ExistingVariableValue::new(&self.theme, value))
                }
                VariableSuggestion::Derived(value) => {
                    VariableSuggestionRow::Derived(LiteralVariableValue::new(&self.theme, value))
                }
            })
            .collect::<Vec<_>>();

        // Update the context
        self.variable_ctx = Some(CurrentVariableContext {
            name: current_variable.name.clone(),
            suggestions: suggestion_widgets.clone(),
        });

        // Update the suggestions list
        self.suggestions.update_items(suggestion_widgets);
        self.suggestions.reset_selection();

        // Pre-select the first environment or existing suggestion
        if let Some(idx) = self
            .suggestions
            .items()
            .iter()
            .position(|s| !matches!(s, VariableSuggestionRow::New(_) | VariableSuggestionRow::Derived(_)))
        {
            self.suggestions.select(idx);
        }

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn confirm_new_secret_value(&mut self, value: String) -> Result<Action> {
        tracing::debug!("Secret variable value selected");
        self.command.set_next_variable(value);
        self.update_variable_context(true).await
    }

    #[instrument(skip_all)]
    async fn confirm_new_regular_value(&mut self, value: String) -> Result<Action> {
        if !value.trim().is_empty() {
            let variable_name = &self.variable_ctx.as_ref().unwrap().name;
            match self
                .service
                .insert_variable_value(self.command.new_variable_value_for(variable_name, &value))
                .await
            {
                Ok(v) => {
                    tracing::debug!("New variable value stored");
                    self.confirm_existing_value(v, true).await
                }
                Err(InsertError::Invalid(err)) => {
                    tracing::warn!("{err}");
                    self.error.set_temp_message(err);
                    Ok(Action::NoOp)
                }
                Err(InsertError::AlreadyExists) => {
                    tracing::warn!("The value already exists");
                    self.error.set_temp_message("The value already exists");
                    Ok(Action::NoOp)
                }
                Err(InsertError::Unexpected(report)) => Err(report),
            }
        } else {
            tracing::debug!("New empty variable value selected");
            self.command.set_next_variable(value);
            self.update_variable_context(true).await
        }
    }

    #[instrument(skip_all)]
    async fn confirm_existing_edition(&mut self, mut value: VariableValue, new_value: String) -> Result<Action> {
        value.value = new_value;
        match self.service.update_variable_value(value).await {
            Ok(v) => {
                if let VariableSuggestionRow::Existing(e) = self.suggestions.selected_mut().unwrap() {
                    e.value = v;
                };
                Ok(Action::NoOp)
            }
            Err(UpdateError::Invalid(err)) => {
                tracing::warn!("{err}");
                self.error.set_temp_message(err);
                Ok(Action::NoOp)
            }
            Err(UpdateError::AlreadyExists) => {
                tracing::warn!("The value already exists");
                self.error.set_temp_message("The value already exists");
                Ok(Action::NoOp)
            }
            Err(UpdateError::Unexpected(report)) => Err(report),
        }
    }

    #[instrument(skip_all)]
    async fn confirm_existing_value(&mut self, value: VariableValue, new: bool) -> Result<Action> {
        let value_id = value.id.expect("existing must have id");
        match self
            .service
            .increment_variable_value_usage(value_id, self.command.current_variable_context())
            .await
            .map_err(UpdateError::into_report)
        {
            Ok(_) => {
                if !new {
                    tracing::debug!("Existing variable value selected");
                }
                self.command.set_next_variable(value.value);
                self.update_variable_context(true).await
            }
            Err(report) => Err(report),
        }
    }

    #[instrument(skip_all)]
    async fn confirm_literal_value(&mut self, value: String, store: bool) -> Result<Action> {
        if store && !value.trim().is_empty() {
            let variable_name = &self.variable_ctx.as_ref().unwrap().name;
            match self
                .service
                .insert_variable_value(self.command.new_variable_value_for(variable_name, &value))
                .await
            {
                Ok(v) => {
                    tracing::debug!("Literal variable value selected and stored");
                    self.confirm_existing_value(v, true).await
                }
                Err(InsertError::Invalid(_) | InsertError::AlreadyExists) => {
                    tracing::debug!("Literal variable value selected but couldn't be stored");
                    self.command.set_next_variable(value);
                    self.update_variable_context(true).await
                }
                Err(InsertError::Unexpected(report)) => Err(report),
            }
        } else {
            tracing::debug!("Literal variable value selected");
            self.command.set_next_variable(value);
            self.update_variable_context(true).await
        }
    }

    /// Returns an action to quit the component, with the current variable content
    fn quit_action(&self, peek: bool) -> Result<Action> {
        let cmd = self.command.to_string();
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
