use color_eyre::Result;
use crossterm::event::MouseEventKind;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    cli::{CliProcess, CompletionProcess, Interactive},
    component::{Component, EmptyComponent},
    config::{Config, KeyBindingsConfig},
    errors::AppError,
    process::{InteractiveProcess, Process, ProcessOutput},
    service::IntelliShellService,
    tui::{Event, Tui},
};

/// Represents actions that components or the application can signal to change the application state or flow.
#[derive(Default)]
pub enum Action {
    /// No-op action, nothing has to be done
    #[default]
    NoOp,
    /// Signals that the application should quit, providing the output
    Quit(ProcessOutput),
    /// Signals that the active component should be switched to the provided one
    SwitchComponent(Box<dyn Component>),
}

/// The main application struct, holding configuration and managing the application flow
pub struct App {
    cancellation_token: CancellationToken,
    active_component: Box<dyn Component>,
}
impl App {
    /// Creates a new instance of the application
    pub fn new(cancellation_token: CancellationToken) -> Result<Self> {
        Ok(Self {
            cancellation_token,
            active_component: Box::new(EmptyComponent),
        })
    }

    /// Runs the main application logic based on the parsed CLI arguments.
    ///
    /// This method dispatches to either an interactive or non-interactive process execution based on the provided `Cli`
    /// arguments and the specific subcommand.
    ///
    /// It returns the final [ProcessOutput] when the application finishes.
    #[instrument(skip_all)]
    pub async fn run(
        self,
        config: Config,
        service: IntelliShellService,
        process: CliProcess,
        extra_line: bool,
    ) -> Result<ProcessOutput> {
        match process {
            #[cfg(debug_assertions)]
            CliProcess::Query(query_process) => {
                tracing::info!("Running 'query' process");
                tracing::debug!("Options: {:?}", query_process);
                service.load_workspace_items().await.map_err(AppError::into_report)?;
                self.run_non_interactive(query_process, config, service, extra_line)
                    .await
            }
            CliProcess::Init(_) | CliProcess::Config(_) | CliProcess::Logs(_) => unreachable!("Handled in main"),
            CliProcess::New(bookmark_command) => {
                tracing::info!("Running 'new' process");
                tracing::debug!("Options: {:?}", bookmark_command);
                self.run_interactive(bookmark_command, config, service, extra_line)
                    .await
            }
            CliProcess::Search(search_commands) => {
                tracing::info!("Running 'search' process");
                tracing::debug!("Options: {:?}", search_commands);
                service.load_workspace_items().await.map_err(AppError::into_report)?;
                self.run_interactive(search_commands, config, service, extra_line).await
            }
            CliProcess::Replace(variable_replace) => {
                tracing::info!("Running 'replace' process");
                tracing::debug!("Options: {:?}", variable_replace);
                service.load_workspace_items().await.map_err(AppError::into_report)?;
                self.run_interactive(variable_replace, config, service, extra_line)
                    .await
            }
            CliProcess::Fix(fix_command) => {
                tracing::info!("Running 'fix' process");
                tracing::debug!("Options: {:?}", fix_command);
                self.run_non_interactive(fix_command, config, service, extra_line).await
            }
            CliProcess::Export(export_commands) => {
                tracing::info!("Running 'export' process");
                tracing::debug!("Options: {:?}", export_commands);
                self.run_interactive(export_commands, config, service, extra_line).await
            }
            CliProcess::Import(import_commands) => {
                tracing::info!("Running 'import' process");
                tracing::debug!("Options: {:?}", import_commands);
                self.run_interactive(import_commands, config, service, extra_line).await
            }
            #[cfg(feature = "tldr")]
            CliProcess::Tldr(crate::cli::TldrProcess::Fetch(tldr_fetch)) => {
                tracing::info!("Running tldr 'fetch' process");
                tracing::debug!("Options: {:?}", tldr_fetch);
                self.run_non_interactive(tldr_fetch, config, service, extra_line).await
            }
            #[cfg(feature = "tldr")]
            CliProcess::Tldr(crate::cli::TldrProcess::Clear(tldr_clear)) => {
                tracing::info!("Running tldr 'clear' process");
                tracing::debug!("Options: {:?}", tldr_clear);
                self.run_non_interactive(tldr_clear, config, service, extra_line).await
            }
            CliProcess::Completion(CompletionProcess::New(completion_new)) => {
                tracing::info!("Running 'completion new' process");
                tracing::debug!("Options: {:?}", completion_new);
                self.run_interactive(completion_new, config, service, extra_line).await
            }
            CliProcess::Completion(CompletionProcess::Delete(completion_delete)) => {
                tracing::info!("Running 'completion delete' process");
                tracing::debug!("Options: {:?}", completion_delete);
                self.run_non_interactive(completion_delete, config, service, extra_line)
                    .await
            }
            CliProcess::Completion(CompletionProcess::List(completion_list)) => {
                tracing::info!("Running 'completion list' process");
                tracing::debug!("Options: {:?}", completion_list);
                service.load_workspace_items().await.map_err(AppError::into_report)?;
                self.run_interactive(completion_list, config, service, extra_line).await
            }
            #[cfg(feature = "self-update")]
            CliProcess::Update(update) => {
                tracing::info!("Running 'update' process");
                tracing::debug!("Options: {:?}", update);
                self.run_non_interactive(update, config, service, extra_line).await
            }
        }
    }

    /// Executes a process in non-interactive mode.
    ///
    /// Simply calls the `execute` method on the given [Process] implementation.
    async fn run_non_interactive(
        self,
        process: impl Process,
        config: Config,
        service: IntelliShellService,
        extra_line: bool,
    ) -> Result<ProcessOutput> {
        if extra_line {
            println!();
        }
        process.execute(config, service, self.cancellation_token).await
    }

    /// Executes a process that might require an interactive TUI
    async fn run_interactive(
        mut self,
        it: Interactive<impl InteractiveProcess>,
        config: Config,
        service: IntelliShellService,
        extra_line: bool,
    ) -> Result<ProcessOutput> {
        // If the process hasn't enabled the interactive flag, just run it
        if !it.opts.interactive {
            return self.run_non_interactive(it.process, config, service, extra_line).await;
        }

        // Converts the process into the renderable component and initializes it
        let inline = it.opts.inline || (!it.opts.full_screen && config.inline);
        let keybindings = config.keybindings.clone();
        self.active_component = it
            .process
            .into_component(config, service, inline, self.cancellation_token.clone())?;

        // Initialize and peek into the component, in case we can give a straight result
        let peek_action = self.active_component.init_and_peek().await?;
        if let Some(output) = self.process_action(peek_action).await? {
            tracing::debug!("A result was received from `init_and_peek`, returning it");
            return Ok(output);
        }

        // Enter the TUI (inline or fullscreen)
        let mut tui = Tui::new(self.cancellation_token.clone())?.paste(true).mouse(true);
        if inline {
            tracing::debug!("Displaying inline {} interactively", self.active_component.name());
            tui.enter_inline(extra_line, self.active_component.min_inline_height())?;
        } else {
            tracing::debug!("Displaying full-screen {} interactively", self.active_component.name());
            tui.enter()?;
        }

        // Main loop
        loop {
            tokio::select! {
                biased;
                // If the token is cancelled, close the main loop and return
                _ = self.cancellation_token.cancelled() => {
                    tracing::info!("Cancellation token received, exiting TUI loop");
                    return Ok(ProcessOutput::fail());
                }
                // Otherwise, wait for the next event to come in
                maybe_event = tui.next_event() => {
                    let Some(tui_event) = maybe_event else {
                        tracing::error!("TUI closed unexpectedly, no event received");
                        break;
                    };
                    // Handle the event
                    let action = self.handle_tui_event(tui_event, &mut tui, &keybindings).await?;
                    // Process the action
                    if let Some(output) = self.process_action(action).await? {
                        // If the action generated an output, exit the loop by returning it
                        return Ok(output);
                    }
                }
            }
        }

        Ok(ProcessOutput::success())
    }

    /// Handles a single TUI event by dispatching it to the active component.
    ///
    /// Based on the type of [Event], calls the corresponding method on the currently active [Component].
    ///
    /// Returns an [Action] indicating the result of the event processing.
    #[instrument(skip_all)]
    async fn handle_tui_event(
        &mut self,
        event: Event,
        tui: &mut Tui,
        keybindings: &KeyBindingsConfig,
    ) -> Result<Action> {
        if event != Event::Tick
            && event != Event::Render
            && !matches!(event, Event::Mouse(m) if m.kind == MouseEventKind::Moved )
        {
            tracing::trace!("{event:?}");
        }
        let ac = &mut self.active_component;
        Ok(match event {
            // Render the active component using the TUI renderer
            Event::Render => {
                tui.render(|frame, area| ac.render(frame, area))?;
                Action::NoOp
            }
            // Dispatch other events to the active component
            Event::Tick => ac.tick()?,
            Event::FocusGained => ac.focus_gained()?,
            Event::FocusLost => ac.focus_lost()?,
            Event::Resize(width, height) => ac.resize(width, height)?,
            Event::Paste(content) => ac.process_paste_event(content)?,
            Event::Key(key) => ac.process_key_event(keybindings, key).await?,
            Event::Mouse(mouse) => ac.process_mouse_event(mouse)?,
        })
    }

    /// Processes an [Action] returned by a component.
    ///
    /// Returns an optional [ProcessOutput] if the action signals the application should exit.
    #[instrument(skip_all)]
    async fn process_action(&mut self, action: Action) -> Result<Option<ProcessOutput>> {
        match action {
            Action::NoOp => (),
            Action::Quit(output) => return Ok(Some(output)),
            Action::SwitchComponent(next_component) => {
                tracing::debug!(
                    "Switching active component: {} -> {}",
                    self.active_component.name(),
                    next_component.name()
                );
                self.active_component = next_component;
                // Initialize and peek into the new component to see if it can provide an immediate result
                let peek_action = self.active_component.init_and_peek().await?;
                if let Some(output) = Box::pin(self.process_action(peek_action)).await? {
                    tracing::debug!("A result was received from `init_and_peek`, returning it");
                    return Ok(Some(output));
                }
            }
        }
        Ok(None)
    }
}
