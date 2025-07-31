use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use enum_cycling::EnumCycle;
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};
use semver::Version;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use tui_textarea::CursorMove;

use super::Component;
use crate::{
    app::Action,
    component::{
        edit::{EditCommandComponent, EditCommandComponentMode},
        variable::VariableReplacementComponent,
    },
    config::{Config, KeyBindingsConfig, SearchConfig, Theme},
    errors::{SearchError, UpdateError},
    format_msg,
    model::{Command, DynamicCommand, SOURCE_WORKSPACE, SearchMode},
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{
        CommandWidget, CustomList, CustomTextArea, ErrorPopup, HighlightSymbolMode, NewVersionBanner, TagWidget,
    },
};

const EMPTY_STORAGE_MESSAGE: &str = r#"There are no stored commands yet!
    - Try to bookmark some command with 'Ctrl + B'
    - Or execute 'intelli-shell tldr fetch' to download a bunch of tldr's useful commands"#;

/// A component for searching [`Command`]
#[derive(Clone)]
pub struct SearchCommandsComponent {
    /// The visual theme for styling the component
    theme: Theme,
    /// Whether the TUI is in inline mode or not
    inline: bool,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// The component layout
    layout: Layout,
    /// The new version banner
    new_version: NewVersionBanner,
    /// The delay before triggering a search after user input
    search_delay: Duration,
    /// Cancellation token for the current refresh task
    refresh_token: Arc<Mutex<Option<CancellationToken>>>,
    /// The state of the component
    state: Arc<RwLock<SearchCommandsComponentState<'static>>>,
}
struct SearchCommandsComponentState<'a> {
    /// The default search mode
    mode: SearchMode,
    /// Whether to search for user commands only by default (excluding tldr)
    user_only: bool,
    /// The active query
    query: CustomTextArea<'a>,
    /// List of tags, if currently editing a tag
    tags: Option<CustomList<'a, TagWidget>>,
    /// Whether the command search was an alias match
    alias_match: bool,
    /// The list of commands
    commands: CustomList<'a, CommandWidget>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
}

impl SearchCommandsComponent {
    /// Creates a new [`SearchCommandsComponent`]
    pub fn new(
        service: IntelliShellService,
        config: Config,
        inline: bool,
        new_version: Option<Version>,
        query: impl Into<String>,
    ) -> Self {
        let query = CustomTextArea::new(config.theme.primary, inline, false, query.into()).focused();

        let commands = CustomList::new(config.theme.primary, inline, Vec::new())
            .highlight_symbol(config.theme.highlight_symbol.clone())
            .highlight_symbol_mode(HighlightSymbolMode::Last)
            .highlight_symbol_style(config.theme.highlight_primary_full().into());

        let new_version = NewVersionBanner::new(&config.theme, new_version);
        let error = ErrorPopup::empty(&config.theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        let SearchConfig { delay, mode, user_only } = config.search;

        let ret = Self {
            theme: config.theme,
            inline,
            service,
            layout,
            new_version,
            search_delay: Duration::from_millis(delay),
            refresh_token: Arc::new(Mutex::new(None)),
            state: Arc::new(RwLock::new(SearchCommandsComponentState {
                mode,
                user_only,
                query,
                tags: None,
                alias_match: false,
                commands,
                error,
            })),
        };

        ret.update_config(config.search.mode, config.search.user_only);

        ret
    }

    /// Updates the search config
    fn update_config(&self, search_mode: SearchMode, user_only: bool) {
        let inline = self.inline;
        let mut state = self.state.write();
        state.mode = search_mode;
        state.user_only = user_only;

        let title = match (inline, user_only) {
            (true, true) => format!("({search_mode},user)"),
            (true, false) => format!("({search_mode})"),
            (false, true) => format!(" Query ({search_mode},user) "),
            (false, false) => format!(" Query ({search_mode}) "),
        };

        state.query.set_title(title);
    }
}

#[async_trait]
impl Component for SearchCommandsComponent {
    fn name(&self) -> &'static str {
        "SearchCommandsComponent"
    }

    fn min_inline_height(&self) -> u16 {
        // Query + 10 Commands
        1 + 10
    }

    async fn init(&mut self) -> Result<()> {
        let tags = {
            let state = self.state.read();
            state.query.lines_as_string() == "#"
        };
        if tags {
            self.refresh_tags().await
        } else {
            self.refresh_commands().await
        }
    }

    #[instrument(skip_all)]
    async fn peek(&mut self) -> Result<Action> {
        if self.service.is_storage_empty().await? {
            Ok(Action::Quit(
                ProcessOutput::success().stderr(format_msg!(self.theme, "{EMPTY_STORAGE_MESSAGE}")),
            ))
        } else {
            let command = {
                let state = self.state.read();
                if state.alias_match && state.commands.len() == 1 {
                    state.commands.selected().cloned().map(Command::from)
                } else {
                    None
                }
            };
            if let Some(command) = command {
                tracing::info!("Found a single alias command: {command}");
                self.confirm_command(command, false).await
            } else {
                Ok(Action::NoOp)
            }
        }
    }

    #[instrument(skip_all)]
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [query_area, suggestions_area] = self.layout.areas(area);

        let mut state = self.state.write();

        // Render the query widget
        frame.render_widget(&state.query, query_area);

        // Render the suggestions
        if let Some(ref mut tags) = state.tags {
            frame.render_widget(tags, suggestions_area);
        } else {
            frame.render_widget(&mut state.commands, suggestions_area);
        }

        // Render the new version banner and error message as an overlay
        self.new_version.render_in(frame, area);
        state.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.error.tick();
        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Option<ProcessOutput>> {
        let mut state = self.state.write();
        if state.tags.is_some() {
            tracing::debug!("Closing tag mode: user request");
            state.tags = None;
            state.commands.set_focus(true);
            self.schedule_debounced_command_refresh();
            Ok(None)
        } else {
            tracing::info!("User requested to exit");
            let query = state.query.lines_as_string();
            Ok(Some(if query.is_empty() {
                ProcessOutput::success()
            } else {
                ProcessOutput::success().fileout(query)
            }))
        }
    }

    async fn process_key_event(&mut self, keybindings: &KeyBindingsConfig, key: KeyEvent) -> Result<Action> {
        // If `ctrl+space` was hit, attempt to refresh tags if the cursor is on it
        if key.code == KeyCode::Char(' ') && key.modifiers == KeyModifiers::CONTROL {
            self.debounced_refresh_tags();
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
            MouseEventKind::ScrollDown => Ok(self.move_next()?),
            MouseEventKind::ScrollUp => Ok(self.move_prev()?),
            _ => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if let Some(ref mut tags) = state.tags {
            tags.select_prev();
        } else {
            state.commands.select_prev();
        }
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if let Some(ref mut tags) = state.tags {
            tags.select_next();
        } else {
            state.commands.select_next();
        }
        Ok(Action::NoOp)
    }

    fn move_left(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        if state.tags.is_none() {
            state.query.move_cursor_left(word);
        }
        Ok(Action::NoOp)
    }

    fn move_right(&mut self, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        if state.tags.is_none() {
            state.query.move_cursor_right(word);
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
        if let Some(ref mut tags) = state.tags {
            tags.select_first();
        } else if absolute {
            state.commands.select_first();
        } else {
            state.query.move_home(false);
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        if let Some(ref mut tags) = state.tags {
            tags.select_last();
        } else if absolute {
            state.commands.select_last();
        } else {
            state.query.move_end(false);
        }
        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.query.undo();
        if state.tags.is_some() {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.query.redo();
        if state.tags.is_some() {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn insert_text(&mut self, text: String) -> Result<Action> {
        let mut state = self.state.write();
        state.query.insert_str(text);
        if state.tags.is_some() {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn insert_char(&mut self, c: char) -> Result<Action> {
        let mut state = self.state.write();
        state.query.insert_char(c);
        if c == '#' || state.tags.is_some() {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        let mut state = self.state.write();
        state.query.delete(backspace, word);
        if state.tags.is_some() {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn toggle_search_mode(&mut self) -> Result<Action> {
        let (search_mode, user_only, tags) = {
            let state = self.state.read();
            (state.mode.down(), state.user_only, state.tags.is_some())
        };
        self.update_config(search_mode, user_only);
        if tags {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn toggle_search_user_only(&mut self) -> Result<Action> {
        let (search_mode, user_only, tags) = {
            let state = self.state.read();
            (state.mode, !state.user_only, state.tags.is_some())
        };
        self.update_config(search_mode, user_only);
        if tags {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_delete(&mut self) -> Result<Action> {
        let command = {
            let mut state = self.state.write();
            if let Some(selected) = state.commands.selected() {
                if selected.source != SOURCE_WORKSPACE {
                    state.commands.delete_selected()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(command) = command {
            self.service.delete_command(command.id).await?;
        }

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        let command = {
            let state = self.state.read();
            state.commands.selected().cloned().map(Command::from)
        };
        if let Some(command) = command
            && command.source != SOURCE_WORKSPACE
        {
            tracing::info!("Entering command update for: {command}");
            Ok(Action::SwitchComponent(Box::new(EditCommandComponent::new(
                self.service.clone(),
                self.theme.clone(),
                self.inline,
                self.new_version.inner().clone(),
                command,
                EditCommandComponentMode::Edit {
                    parent: Box::new(self.clone()),
                },
            ))))
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        let (selected_tag, cursor_pos, query, command) = {
            let state = self.state.read();
            let selected_tag = state.tags.as_ref().and_then(|s| s.selected().map(TagWidget::text));
            (
                selected_tag.map(String::from),
                state.query.cursor().1,
                state.query.lines_as_string(),
                state.commands.selected().cloned().map(Command::from),
            )
        };

        if let Some(tag) = selected_tag {
            tracing::debug!("Selected tag: {tag}");
            self.confirm_tag(tag, query, cursor_pos).await
        } else if let Some(command) = command {
            tracing::info!("Selected command: {command}");
            self.confirm_command(command, false).await
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_execute(&mut self) -> Result<Action> {
        let command = {
            let state = self.state.read();
            state.commands.selected().cloned().map(Command::from)
        };
        if let Some(command) = command {
            tracing::info!("Selected command to execute: {command}");
            self.confirm_command(command, true).await
        } else {
            Ok(Action::NoOp)
        }
    }
}

impl SearchCommandsComponent {
    /// Schedule a debounced refresh of the commands list
    fn schedule_debounced_command_refresh(&self) {
        let cancellation_token = {
            // Cancel previous token (if any)
            let mut token_guard = self.refresh_token.lock().unwrap();
            if let Some(token) = token_guard.take() {
                token.cancel();
            }
            // Issue a new one
            let new_token = CancellationToken::new();
            *token_guard = Some(new_token.clone());
            new_token
        };

        // Spawn a new task
        let this = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                biased;
                // That completes when the token is canceled
                _ = cancellation_token.cancelled() => {}
                // Or performs a command search after the configured delay
                _ = tokio::time::sleep(this.search_delay) => {
                    if let Err(err) = this.refresh_commands().await {
                        panic!("Error refreshing commands: {err:?}");
                    }
                }
            }
        });
    }

    /// Refresh the command list
    #[instrument(skip_all)]
    async fn refresh_commands(&self) -> Result<()> {
        // Retrieve the user query
        let (mode, user_only, query) = {
            let state = self.state.read();
            (state.mode, state.user_only, state.query.lines_as_string())
        };

        // Search for commands
        let res = self.service.search_commands(mode, user_only, &query).await;

        // Update the command list or display an error
        let mut state = self.state.write();
        let command_widgets = match res {
            Ok((commands, alias_match)) => {
                state.error.clear_message();
                state.alias_match = alias_match;
                commands
                    .into_iter()
                    .map(|c| CommandWidget::new(&self.theme, self.inline, c))
                    .collect()
            }
            Err(SearchError::InvalidFuzzy) => {
                tracing::warn!("Invalid fuzzy search");
                state.error.set_perm_message("Invalid fuzzy seach");
                Vec::new()
            }
            Err(SearchError::InvalidRegex(err)) => {
                tracing::warn!("Invalid regex search: {}", err);
                state.error.set_perm_message("Invalid regex search");
                Vec::new()
            }
            Err(SearchError::Unexpected(err)) => return Err(err),
        };
        state.commands.update_items(command_widgets);

        Ok(())
    }

    /// Immediately starts a debounced refresh of the tags list
    fn debounced_refresh_tags(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(err) = this.refresh_tags().await {
                panic!("Error refreshing tags: {err:?}");
            }
        });
    }

    /// Refresh the suggested tags list
    #[instrument(skip_all)]
    async fn refresh_tags(&self) -> Result<()> {
        // Retrieve the user query
        let (mode, user_only, query, cursor_pos) = {
            let state = self.state.read();
            (
                state.mode,
                state.user_only,
                state.query.lines_as_string(),
                state.query.cursor().1,
            )
        };

        // Find tags for that query
        let res = self.service.search_tags(mode, user_only, &query, cursor_pos).await;

        // Update the tags list
        let mut state = self.state.write();
        match res {
            Ok(None) => {
                tracing::trace!("No editing tags");
                if state.tags.is_some() {
                    tracing::debug!("Closing tag mode: no editing tag");
                    state.tags = None;
                    state.commands.set_focus(true);
                }
                self.schedule_debounced_command_refresh();
                Ok(())
            }
            Ok(Some(tags)) if tags.is_empty() => {
                tracing::trace!("No tags found");
                if state.tags.is_some() {
                    tracing::debug!("Closing tag mode: no tags found");
                    state.tags = None;
                    state.commands.set_focus(true);
                }
                self.schedule_debounced_command_refresh();
                Ok(())
            }
            Ok(Some(tags)) => {
                state.error.clear_message();
                if tags.len() == 1 && tags.iter().all(|(_, _, exact_match)| *exact_match) {
                    tracing::trace!("Exact tag found only");
                    if state.tags.is_some() {
                        tracing::debug!("Closing tag mode: exact tag found");
                        state.tags = None;
                        state.commands.set_focus(true);
                    }
                    self.schedule_debounced_command_refresh();
                } else {
                    tracing::trace!("Found {} tags", tags.len());
                    let tag_widgets = tags
                        .into_iter()
                        .map(|(tag, _, _)| TagWidget::new(&self.theme, tag))
                        .collect();
                    let tags_list = if let Some(ref mut list) = state.tags {
                        list
                    } else {
                        tracing::debug!("Entering tag mode");
                        state.tags.insert(
                            CustomList::new(self.theme.primary, self.inline, Vec::new())
                                .highlight_symbol(self.theme.highlight_symbol.clone())
                                .highlight_symbol_mode(HighlightSymbolMode::Last)
                                .highlight_symbol_style(self.theme.highlight_primary_full().into()),
                        )
                    };
                    tags_list.update_items(tag_widgets);
                    state.commands.set_focus(false);
                }

                Ok(())
            }
            Err(SearchError::InvalidFuzzy) => {
                tracing::warn!("Invalid fuzzy search");
                state.error.set_perm_message("Invalid fuzzy seach");
                if state.tags.is_some() {
                    tracing::debug!("Closing tag mode: invalid fuzzy search");
                    state.tags = None;
                    state.commands.set_focus(true);
                }
                Ok(())
            }
            Err(SearchError::InvalidRegex(err)) => {
                tracing::warn!("Invalid regex search: {}", err);
                state.error.set_perm_message("Invalid regex search");
                if state.tags.is_some() {
                    tracing::debug!("Closing tag mode: invalid regex search");
                    state.tags = None;
                    state.commands.set_focus(true);
                }
                Ok(())
            }
            Err(SearchError::Unexpected(err)) => Err(err),
        }
    }

    /// Confirms the tag by replacing the editing tag with the selected one
    #[instrument(skip_all)]
    async fn confirm_tag(&mut self, tag: String, query: String, cursor_pos: usize) -> Result<Action> {
        // Find the start and end of the current tag by looking both sides of the cursor
        let mut tag_start = cursor_pos.wrapping_sub(1);
        let chars: Vec<_> = query.chars().collect();
        while tag_start > 0 && chars[tag_start] != '#' {
            tag_start -= 1;
        }
        let mut tag_end = cursor_pos;
        while tag_end < chars.len() && chars[tag_end] != ' ' {
            tag_end += 1;
        }
        let mut state = self.state.write();
        if chars[tag_start] == '#' {
            // Replace the partial tag with the selected one
            state.query.select_all();
            state.query.cut();
            state
                .query
                .insert_str(format!("{}{} {}", &query[..tag_start], tag, &query[tag_end..]));
            state
                .query
                .move_cursor(CursorMove::Jump(0, (tag_start + tag.len() + 1) as u16));
        }
        state.tags = None;
        state.commands.set_focus(true);
        self.schedule_debounced_command_refresh();
        Ok(Action::NoOp)
    }

    /// Confirms the command by increasing the usage counter storing it and quits or switches to the variable
    /// replacement component if needed
    #[instrument(skip_all)]
    async fn confirm_command(&mut self, command: Command, execute: bool) -> Result<Action> {
        // Increment usage count
        if command.source != SOURCE_WORKSPACE {
            self.service
                .increment_command_usage(command.id)
                .await
                .map_err(UpdateError::into_report)?;
        }
        // Determine if the command has some variables
        let dynamic = DynamicCommand::parse(&command.cmd);
        if dynamic.has_pending_variable() {
            // If it does, switch to the variable replacement component
            Ok(Action::SwitchComponent(Box::new(VariableReplacementComponent::new(
                self.service.clone(),
                self.theme.clone(),
                self.inline,
                execute,
                false,
                self.new_version.inner().clone(),
                dynamic,
            ))))
        } else if execute {
            // If it doesn't and execute is true, execute the command
            Ok(Action::Quit(ProcessOutput::execute(command.cmd)))
        } else {
            // Otherwise just output it
            Ok(Action::Quit(
                ProcessOutput::success().stdout(&command.cmd).fileout(command.cmd),
            ))
        }
    }
}
