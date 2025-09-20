use std::sync::Arc;

use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{MouseEvent, MouseEventKind};
use itertools::Itertools;
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::{
    Component,
    completion_edit::{EditCompletionComponent, EditCompletionComponentMode},
};
use crate::{
    app::Action,
    config::{Config, Theme},
    errors::AppError,
    format_msg,
    model::{SOURCE_WORKSPACE, VariableCompletion},
    process::ProcessOutput,
    service::IntelliShellService,
    utils::resolve_completion,
    widgets::{CustomList, ErrorPopup, NewVersionBanner},
};

const GLOBAL_ROOT_CMD: &str = "[GLOBAL]";
const EMPTY_STORAGE_MESSAGE: &str = "There are no stored variable completions!";

/// A component for listing and managing [`VariableCompletion`]
#[derive(Clone)]
pub struct CompletionListComponent {
    /// The visual theme for styling the component
    theme: Theme,
    /// Whether the TUI is in inline mode or not
    inline: bool,
    /// Service for interacting with storage
    service: IntelliShellService,
    /// The component layout
    layout: Layout,
    /// Global cancellation token
    global_cancellation_token: CancellationToken,
    /// The state of the component
    state: Arc<RwLock<CompletionListComponentState<'static>>>,
}
struct CompletionListComponentState<'a> {
    /// The root cmd to be initially selected
    initial_root_cmd: Option<String>,
    /// The currently focused list of the component
    active_list: ActiveList,
    /// The list of root commands
    root_cmds: CustomList<'a, String>,
    /// The list of completions for the selected root command
    completions: CustomList<'a, VariableCompletion>,
    /// The output of the completion
    preview: Option<Result<String, String>>,
    /// Popup for displaying error messages
    error: ErrorPopup<'a>,
}

/// Represents the currently active (focused) list
#[derive(Copy, Clone, PartialEq, Eq)]
enum ActiveList {
    RootCmds,
    Completions,
}

impl CompletionListComponent {
    /// Creates a new [`CompletionListComponent`]
    pub fn new(
        service: IntelliShellService,
        config: Config,
        inline: bool,
        root_cmd: Option<String>,
        cancellation_token: CancellationToken,
    ) -> Self {
        let root_cmds = CustomList::new(config.theme.clone(), inline, Vec::new()).title(" Commands ");
        let completions = CustomList::new(config.theme.clone(), inline, Vec::new()).title(" Completions ");

        let error = ErrorPopup::empty(&config.theme);

        let layout = if inline {
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(3), Constraint::Fill(2)])
        } else {
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(3), Constraint::Fill(2)]).margin(1)
        };

        let mut state = CompletionListComponentState {
            initial_root_cmd: root_cmd,
            active_list: ActiveList::RootCmds,
            root_cmds,
            completions,
            preview: None,
            error,
        };
        state.update_active_list(ActiveList::RootCmds);

        Self {
            theme: config.theme,
            inline,
            service,
            layout,
            global_cancellation_token: cancellation_token,
            state: Arc::new(RwLock::new(state)),
        }
    }
}
impl<'a> CompletionListComponentState<'a> {
    /// Updates the active list and the focused list
    fn update_active_list(&mut self, active: ActiveList) {
        self.active_list = active;

        self.root_cmds.set_focus(active == ActiveList::RootCmds);
        self.completions.set_focus(active == ActiveList::Completions);
    }
}

#[async_trait]
impl Component for CompletionListComponent {
    fn name(&self) -> &'static str {
        "CompletionListComponent"
    }

    fn min_inline_height(&self) -> u16 {
        5
    }

    async fn init_and_peek(&mut self) -> Result<Action> {
        self.refresh_lists(true).await
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area according to the layout
        let [root_cmds_area, completions_area, preview_area] = self.layout.areas(area);

        let mut state = self.state.write();

        // Render the lists
        frame.render_widget(&mut state.root_cmds, root_cmds_area);
        frame.render_widget(&mut state.completions, completions_area);

        // Render the preview
        if let Some(res) = &state.preview {
            let is_err = res.is_err();
            let (output, style) = match res {
                Ok(o) => (o, self.theme.secondary),
                Err(err) => (err, self.theme.error),
            };
            let mut preview_paragraph = Paragraph::new(output.as_str()).style(style).wrap(Wrap { trim: is_err });
            if !self.inline {
                preview_paragraph =
                    preview_paragraph.block(Block::default().borders(Borders::ALL).title(" Preview ").style(style));
            }
            frame.render_widget(preview_paragraph, preview_area);
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
        Ok(Action::NoOp)
    }

    fn exit(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match &state.active_list {
            ActiveList::RootCmds => Ok(Action::Quit(ProcessOutput::success())),
            ActiveList::Completions => {
                state.update_active_list(ActiveList::RootCmds);
                Ok(Action::NoOp)
            }
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
        match &state.active_list {
            ActiveList::RootCmds => state.root_cmds.select_prev(),
            ActiveList::Completions => state.completions.select_prev(),
        }
        self.debounced_refresh_lists();
        Ok(Action::NoOp)
    }

    fn move_down(&mut self) -> Result<Action> {
        let mut state = self.state.write();
        match &state.active_list {
            ActiveList::RootCmds => state.root_cmds.select_next(),
            ActiveList::Completions => state.completions.select_next(),
        }
        self.debounced_refresh_lists();

        Ok(Action::NoOp)
    }

    fn move_left(&mut self, _word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match &state.active_list {
            ActiveList::RootCmds => (),
            ActiveList::Completions => state.update_active_list(ActiveList::RootCmds),
        }
        Ok(Action::NoOp)
    }

    fn move_right(&mut self, _word: bool) -> Result<Action> {
        let mut state = self.state.write();
        match &state.active_list {
            ActiveList::RootCmds => state.update_active_list(ActiveList::Completions),
            ActiveList::Completions => (),
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
        if absolute {
            let mut state = self.state.write();
            match &state.active_list {
                ActiveList::RootCmds => state.root_cmds.select_first(),
                ActiveList::Completions => state.completions.select_first(),
            }
            self.debounced_refresh_lists();
        }
        Ok(Action::NoOp)
    }

    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        if absolute {
            let mut state = self.state.write();
            match &state.active_list {
                ActiveList::RootCmds => state.root_cmds.select_last(),
                ActiveList::Completions => state.completions.select_last(),
            }
            self.debounced_refresh_lists();
        }
        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_delete(&mut self) -> Result<Action> {
        let data = {
            let mut state = self.state.write();
            if state.active_list == ActiveList::Completions
                && let Some(selected) = state.completions.selected()
            {
                if selected.source != SOURCE_WORKSPACE {
                    state
                        .completions
                        .delete_selected()
                        .map(|(_, c)| (c, state.completions.is_empty()))
                } else {
                    state.error.set_temp_message("Workspace completions can't be deleted");
                    return Ok(Action::NoOp);
                }
            } else {
                None
            }
        };
        if let Some((completion, is_now_empty)) = data {
            self.service
                .delete_variable_completion(completion.id)
                .await
                .map_err(AppError::into_report)?;
            if is_now_empty {
                self.state.write().update_active_list(ActiveList::RootCmds);
            }
            return self.refresh_lists(false).await;
        }

        Ok(Action::NoOp)
    }

    #[instrument(skip_all)]
    async fn selection_update(&mut self) -> Result<Action> {
        let completion = {
            let state = self.state.read();
            if state.active_list == ActiveList::Completions {
                state.completions.selected().cloned()
            } else {
                None
            }
        };
        if let Some(completion) = completion {
            if completion.source != SOURCE_WORKSPACE {
                tracing::info!("Entering completion update for: {completion}");
                Ok(Action::SwitchComponent(Box::new(EditCompletionComponent::new(
                    self.service.clone(),
                    self.theme.clone(),
                    self.inline,
                    completion,
                    EditCompletionComponentMode::Edit {
                        parent: Box::new(self.clone()),
                    },
                    self.global_cancellation_token.clone(),
                ))))
            } else {
                self.state
                    .write()
                    .error
                    .set_temp_message("Workspace completions can't be updated");
                Ok(Action::NoOp)
            }
        } else {
            Ok(Action::NoOp)
        }
    }

    #[instrument(skip_all)]
    async fn selection_confirm(&mut self) -> Result<Action> {
        self.move_right(false)
    }

    async fn selection_execute(&mut self) -> Result<Action> {
        self.selection_confirm().await
    }
}

impl CompletionListComponent {
    /// Immediately starts a debounced refresh of the lists
    fn debounced_refresh_lists(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(err) = this.refresh_lists(false).await {
                panic!("Error refreshing lists: {err:?}");
            }
        });
    }

    /// Refresh the lists
    #[instrument(skip_all)]
    async fn refresh_lists(&self, init: bool) -> Result<Action> {
        // Refresh root cmds
        let root_cmds = self
            .service
            .list_variable_completion_root_cmds()
            .await
            .map_err(AppError::into_report)?
            .into_iter()
            .map(|r| {
                if r.trim().is_empty() {
                    GLOBAL_ROOT_CMD.to_string()
                } else {
                    r
                }
            })
            .collect::<Vec<_>>();
        if root_cmds.is_empty() && init {
            return Ok(Action::Quit(
                ProcessOutput::success().stderr(format_msg!(self.theme, "{EMPTY_STORAGE_MESSAGE}")),
            ));
        } else if root_cmds.is_empty() {
            return Ok(Action::Quit(ProcessOutput::success()));
        }
        let root_cmd = {
            let mut state = self.state.write();
            state.root_cmds.update_items(root_cmds, true);
            if init && let Some(root_cmd) = state.initial_root_cmd.take() {
                let mut irc = root_cmd.as_str();
                if irc.is_empty() {
                    irc = GLOBAL_ROOT_CMD;
                }
                if state.root_cmds.select_matching(|rc| rc == irc) {
                    state.update_active_list(ActiveList::Completions);
                }
            }
            let Some(root_cmd) = state.root_cmds.selected().cloned() else {
                return Ok(Action::Quit(ProcessOutput::success()));
            };
            root_cmd
        };

        // Refresh completions
        let root_cmd_filter = if root_cmd.is_empty() || root_cmd == GLOBAL_ROOT_CMD {
            Some("")
        } else {
            Some(root_cmd.as_str())
        };
        let completions = self
            .service
            .list_variable_completions(root_cmd_filter)
            .await
            .map_err(AppError::into_report)?;
        let completion = {
            let mut state = self.state.write();
            state.completions.update_items(completions, true);
            let Some(completion) = state.completions.selected().cloned() else {
                return Ok(Action::NoOp);
            };
            completion
        };

        // Refresh suggestions preview
        self.state.write().preview = match resolve_completion(&completion, None).await {
            Ok(suggestions) if suggestions.is_empty() => {
                let msg = "... empty output ...";
                Some(Ok(msg.to_string()))
            }
            Ok(suggestions) => Some(Ok(suggestions.iter().join("\n"))),
            Err(err) => Some(Err(err)),
        };

        Ok(Action::NoOp)
    }
}
