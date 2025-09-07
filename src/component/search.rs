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
    errors::AppError,
    format_msg,
    model::{Command, CommandTemplate, SOURCE_WORKSPACE, SearchMode},
    process::ProcessOutput,
    service::IntelliShellService,
    widgets::{CustomList, CustomTextArea, ErrorPopup, NewVersionBanner, items::string::CommentString},
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
    /// Whether to directly execute the command if it matches an alias exactly
    exec_on_alias_match: bool,
    /// Service for interacting with command storage
    service: IntelliShellService,
    /// The component layout
    layout: Layout,
    /// The delay before triggering a search after user input
    search_delay: Duration,
    /// Cancellation token for the current refresh task
    refresh_token: Arc<Mutex<Option<CancellationToken>>>,
    /// The state of the component
    state: Arc<RwLock<SearchCommandsComponentState<'static>>>,
}
struct SearchCommandsComponentState<'a> {
    /// The next component initialization must prompt AI
    initialize_with_ai: bool,
    /// The default search mode
    mode: SearchMode,
    /// Whether to search for user commands only by default (excluding tldr)
    user_only: bool,
    /// The active query
    query: CustomTextArea<'a>,
    /// Whether ai mode is currently enabled
    ai_mode: bool,
    /// List of tags, if currently editing a tag
    tags: Option<CustomList<'a, CommentString>>,
    /// Whether the command search was an alias match
    alias_match: bool,
    /// The list of commands
    commands: CustomList<'a, Command>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
}

impl SearchCommandsComponent {
    /// Creates a new [`SearchCommandsComponent`]
    pub fn new(
        service: IntelliShellService,
        config: Config,
        inline: bool,
        query: impl Into<String>,
        initialize_with_ai: bool,
    ) -> Self {
        let query = CustomTextArea::new(config.theme.primary, inline, false, query.into()).focused();

        let commands = CustomList::new(config.theme.clone(), inline, Vec::new());

        let error = ErrorPopup::empty(&config.theme);

        let layout = if inline {
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)])
        } else {
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).margin(1)
        };

        let SearchConfig {
            delay,
            mode,
            user_only,
            exec_on_alias_match,
        } = config.search;

        let ret = Self {
            theme: config.theme,
            inline,
            exec_on_alias_match,
            service,
            layout,
            search_delay: Duration::from_millis(delay),
            refresh_token: Arc::new(Mutex::new(None)),
            state: Arc::new(RwLock::new(SearchCommandsComponentState {
                initialize_with_ai,
                mode,
                user_only,
                query,
                ai_mode: false,
                tags: None,
                alias_match: false,
                commands,
                error,
            })),
        };

        ret.update_config(None, None, None);

        ret
    }

    /// Updates the search config
    fn update_config(&self, search_mode: Option<SearchMode>, user_only: Option<bool>, ai_mode: Option<bool>) {
        let mut state = self.state.write();
        if let Some(search_mode) = search_mode {
            state.mode = search_mode;
        }
        if let Some(user_only) = user_only {
            state.user_only = user_only;
        }
        if let Some(ai_mode) = ai_mode {
            state.ai_mode = ai_mode;
        }

        let search_mode = state.mode;
        let title = match (state.ai_mode, self.inline, state.user_only) {
            (true, true, _) => String::from("(ai)"),
            (false, true, true) => format!("({search_mode},user)"),
            (false, true, false) => format!("({search_mode})"),
            (true, false, _) => String::from(" Query (ai) "),
            (false, false, true) => format!(" Query ({search_mode},user) "),
            (false, false, false) => format!(" Query ({search_mode}) "),
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

    #[instrument(skip_all)]
    async fn init_and_peek(&mut self) -> Result<Action> {
        // Check if the component should initialize prompting the AI
        let initialize_with_ai = self.state.read().initialize_with_ai;
        if initialize_with_ai {
            let res = self.prompt_ai().await;
            self.state.write().initialize_with_ai = false;
            return res;
        }
        // If the storage is empty, quit with a message
        if self.service.is_storage_empty().await.map_err(AppError::into_report)? {
            Ok(Action::Quit(
                ProcessOutput::success().stderr(format_msg!(self.theme, "{EMPTY_STORAGE_MESSAGE}")),
            ))
        } else {
            // Otherwise initialize the tags or commands based on the current query
            let tags = {
                let state = self.state.read();
                state.query.lines_as_string() == "#"
            };
            if tags {
                self.refresh_tags().await?;
            } else {
                self.refresh_commands().await?;
                // And peek into the commands to check if we can give a straight answer without the TUI rendered
                let command = {
                    let state = self.state.read();
                    if state.alias_match && state.commands.len() == 1 {
                        state.commands.selected().cloned()
                    } else {
                        None
                    }
                };
                if let Some(command) = command {
                    tracing::info!("Found a single alias command: {}", command.cmd);
                    return self.confirm_command(command, self.exec_on_alias_match, false).await;
                }
            }
            Ok(Action::NoOp)
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
        if let Some(new_version) = self.service.check_new_version() {
            NewVersionBanner::new(&self.theme, new_version).render_in(frame, area);
        }
        state.error.render_in(frame, area);
    }

    fn tick(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        state.query.tick();
        state.error.tick();
        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        let (ai_mode, tags) = {
            let state = self.state.read();
            (state.ai_mode, state.tags.is_some())
        };
        if ai_mode {
            tracing::debug!("Closing ai mode: user request");
            self.update_config(None, None, Some(false));
            self.schedule_debounced_command_refresh();
            Ok(Action::NoOp)
        } else if tags {
            tracing::debug!("Closing tag mode: user request");
            let mut state = self.state.write();
            state.tags = None;
            state.commands.set_focus(true);
            self.schedule_debounced_command_refresh();
            Ok(Action::NoOp)
        } else {
            tracing::info!("User requested to exit");
            let state = self.state.read();
            let query = state.query.lines_as_string();
            Ok(Action::Quit(if query.trim().is_empty() {
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
            MouseEventKind::ScrollDown => Ok(self.move_down()?),
            MouseEventKind::ScrollUp => Ok(self.move_up()?),
            _ => Ok(Action::NoOp),
        }
    }

    fn move_up(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.query.is_ai_loading() {
            if let Some(ref mut tags) = state.tags {
                tags.select_prev();
            } else {
                state.commands.select_prev();
            }
        }
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.query.is_ai_loading() {
            if let Some(ref mut tags) = state.tags {
                tags.select_next();
            } else {
                state.commands.select_next();
            }
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
        if !state.query.is_ai_loading() {
            if let Some(ref mut tags) = state.tags {
                tags.select_first();
            } else if absolute {
                state.commands.select_first();
            } else {
                state.query.move_home(false);
            }
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let mut state = self.state.write();
        if !state.query.is_ai_loading() {
            if let Some(ref mut tags) = state.tags {
                tags.select_last();
            } else if absolute {
                state.commands.select_last();
            } else {
                state.query.move_end(false);
            }
        }
        Ok(Action::NoOp)
    }

    fn undo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.query.is_ai_loading() {
            state.query.undo();
            if state.tags.is_some() {
                self.debounced_refresh_tags();
            } else {
                self.schedule_debounced_command_refresh();
            }
        }
        Ok(Action::NoOp)
    }

    fn redo(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if !state.query.is_ai_loading() {
            state.query.redo();
            if state.tags.is_some() {
                self.debounced_refresh_tags();
            } else {
                self.schedule_debounced_command_refresh();
            }
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
        let (search_mode, ai_mode, tags) = {
            let state = self.state.read();
            if state.query.is_ai_loading() {
                return Ok(Action::NoOp);
            }
            (state.mode, state.ai_mode, state.tags.is_some())
        };
        if ai_mode {
            tracing::debug!("Closing ai mode: user toggled search mode");
            self.update_config(None, None, Some(false));
        } else {
            self.update_config(Some(search_mode.down()), None, None);
        }
        if tags {
            self.debounced_refresh_tags();
        } else {
            self.schedule_debounced_command_refresh();
        }
        Ok(Action::NoOp)
    }

    fn toggle_search_user_only(&mut self) -> Result<Action> {
        let (user_only, ai_mode, tags) = {
            let state = self.state.read();
            (state.user_only, state.ai_mode, state.tags.is_some())
        };
        if !ai_mode {
            self.update_config(None, Some(!user_only), None);
            if tags {
                self.debounced_refresh_tags();
            } else {
                self.schedule_debounced_command_refresh();
            }
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_delete(&mut self) -> Result<Action> {
        let command = {
            let mut state = self.state.write();
            if !state.ai_mode
                && let Some(selected) = state.commands.selected()
            {
                if selected.source != SOURCE_WORKSPACE {
                    state.commands.delete_selected()
                } else {
                    state.error.set_temp_message("Workspace commands can't be deleted");
                    return Ok(Action::NoOp);
                }
            } else {
                None
            }
        };

        if let Some((_, command)) = command {
            self.service
                .delete_command(command.id)
                .await
                .map_err(AppError::into_report)?;
        }

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        let command = {
            let state = self.state.read();
            if state.ai_mode {
                return Ok(Action::NoOp);
            }
            state.commands.selected().cloned()
        };
        if let Some(command) = command {
            if command.source != SOURCE_WORKSPACE {
                tracing::info!("Entering command update for: {}", command.cmd);
                Ok(Action::SwitchComponent(Box::new(EditCommandComponent::new(
                    self.service.clone(),
                    self.theme.clone(),
                    self.inline,
                    command,
                    EditCommandComponentMode::Edit {
                        parent: Box::new(self.clone()),
                    },
                ))))
            } else {
                self.state
                    .write()
                    .error
                    .set_temp_message("Workspace commands can't be updated");
                Ok(Action::NoOp)
            }
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        let (selected_tag, cursor_pos, query, command, ai_mode) = {
            let state = self.state.read();
            if state.query.is_ai_loading() {
                return Ok(Action::NoOp);
            }
            let selected_tag = state.tags.as_ref().and_then(|s| s.selected().cloned());
            (
                selected_tag.map(String::from),
                state.query.cursor().1,
                state.query.lines_as_string(),
                state.commands.selected().cloned(),
                state.ai_mode,
            )
        };

        if let Some(tag) = selected_tag {
            tracing::debug!("Selected tag: {tag}");
            self.confirm_tag(tag, query, cursor_pos).await
        } else if let Some(command) = command {
            tracing::info!("Selected command: {}", command.cmd);
            self.confirm_command(command, false, ai_mode).await
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_execute(&mut self) -> Result<Action> {
        let (command, ai_mode) = {
            let state = self.state.read();
            if state.query.is_ai_loading() {
                return Ok(Action::NoOp);
            }
            (state.commands.selected().cloned(), state.ai_mode)
        };
        if let Some(command) = command {
            tracing::info!("Selected command to execute: {}", command.cmd);
            self.confirm_command(command, true, ai_mode).await
        } else {
            Ok(Action::NoOp)
        }
    }

    async fn prompt_ai(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        if state.tags.is_some() || state.query.is_ai_loading() {
            return Ok(Action::NoOp);
        }
        let query = state.query.lines_as_string();
        if !query.is_empty() {
            state.query.set_ai_loading(true);
            drop(state);
            self.update_config(None, None, Some(true));
            let this = self.clone();
            tokio::spawn(async move {
                let res = this.service.suggest_commands(&query).await;
                let mut state = this.state.write();
                let commands = match res {
                    Ok(suggestions) => {
                        if !suggestions.is_empty() {
                            state.error.clear_message();
                            state.alias_match = false;
                            suggestions
                        } else {
                            state
                                .error
                                .set_temp_message("AI did not return any suggestion".to_string());
                            Vec::new()
                        }
                    }
                    Err(AppError::UserFacing(err)) => {
                        tracing::warn!("{err}");
                        state.error.set_temp_message(err.to_string());
                        Vec::new()
                    }
                    Err(AppError::Unexpected(err)) => panic!("Error prompting for command suggestions: {err:?}"),
                };
                state.commands.update_items(commands, true);
                state.query.set_ai_loading(false);
            });
        }
        Ok(Action::NoOp)
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
        let (mode, user_only, ai_mode, query) = {
            let state = self.state.read();
            (
                state.mode,
                state.user_only,
                state.ai_mode,
                state.query.lines_as_string(),
            )
        };

        // Skip when ai mode is enabled
        if ai_mode {
            return Ok(());
        }

        // Search for commands
        let res = self.service.search_commands(mode, user_only, &query).await;

        // Update the command list or display an error
        let mut state = self.state.write();
        let commands = match res {
            Ok((commands, alias_match)) => {
                state.error.clear_message();
                state.alias_match = alias_match;
                commands
            }
            Err(AppError::UserFacing(err)) => {
                tracing::warn!("{err}");
                state.error.set_perm_message(err.to_string());
                Vec::new()
            }
            Err(AppError::Unexpected(err)) => return Err(err),
        };
        state.commands.update_items(commands, true);

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
        let (mode, user_only, ai_mode, query, cursor_pos) = {
            let state = self.state.read();
            (
                state.mode,
                state.user_only,
                state.ai_mode,
                state.query.lines_as_string(),
                state.query.cursor().1,
            )
        };

        // Skip when ai mode is enabled
        if ai_mode {
            return Ok(());
        }

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
                    let tags = tags.into_iter().map(|(tag, _, _)| CommentString::from(tag)).collect();
                    let tags_list = if let Some(ref mut list) = state.tags {
                        list
                    } else {
                        tracing::debug!("Entering tag mode");
                        state
                            .tags
                            .insert(CustomList::new(self.theme.clone(), self.inline, Vec::new()))
                    };
                    tags_list.update_items(tags, true);
                    state.commands.set_focus(false);
                }

                Ok(())
            }
            Err(AppError::UserFacing(err)) => {
                tracing::warn!("{err}");
                state.error.set_perm_message(err.to_string());
                if state.tags.is_some() {
                    tracing::debug!("Closing tag mode");
                    state.tags = None;
                    state.commands.set_focus(true);
                }
                Ok(())
            }
            Err(AppError::Unexpected(err)) => Err(err),
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
    async fn confirm_command(&mut self, command: Command, execute: bool, ai_command: bool) -> Result<Action> {
        // Increment usage count
        if !ai_command && command.source != SOURCE_WORKSPACE {
            self.service
                .increment_command_usage(command.id)
                .await
                .map_err(AppError::into_report)?;
        }
        // Determine if the command has some variables
        let template = CommandTemplate::parse(&command.cmd, false);
        if template.has_pending_variable() {
            // If it does, switch to the variable replacement component
            Ok(Action::SwitchComponent(Box::new(VariableReplacementComponent::new(
                self.service.clone(),
                self.theme.clone(),
                self.inline,
                execute,
                false,
                template,
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
