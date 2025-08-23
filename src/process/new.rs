use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tracing::instrument;

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::BookmarkCommandProcess,
    component::{
        Component,
        edit::{EditCommandComponent, EditCommandComponentMode},
    },
    config::Config,
    errors::{AppError, UserFacingError},
    format_error, format_msg,
    model::{CATEGORY_USER, Command, SOURCE_USER},
    service::IntelliShellService,
    widgets::SPINNER_CHARS,
};

impl Process for BookmarkCommandProcess {
    #[instrument(skip_all)]
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        let BookmarkCommandProcess {
            mut command,
            alias,
            mut description,
            ai,
        } = self;

        // If AI is enabled, we expect a command or description to be provided
        if ai {
            let c = command.as_deref().filter(|c| !c.trim().is_empty());
            let d = description.as_deref().filter(|c| !c.trim().is_empty());
            let prompt = match (c, d) {
                (Some(cmd), Some(desc)) => format!("Write a command for: {desc} (cmd: {cmd})"),
                (Some(cmd), None) => format!("Write a command for: {cmd}"),
                (None, Some(desc)) => format!("Write a command for: {desc}"),
                (None, None) => {
                    return Ok(ProcessOutput::fail().stderr(format_error!(
                        config.theme,
                        "{}",
                        UserFacingError::AiEmptyCommand
                    )));
                }
            };

            // Setup the progress bar
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                    .unwrap()
                    .tick_strings(&SPINNER_CHARS),
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            pb.set_message("Thinking ...");

            // Suggest commands using AI
            let res = service.suggest_commands(&prompt).await;

            // Clear the spinner
            pb.finish_and_clear();

            // Handle the result
            match res {
                Ok(mut commands) => {
                    if !commands.is_empty() {
                        let c = commands.remove(0);
                        command = Some(c.cmd);
                        description = c.description.or(description);
                    } else {
                        return Ok(
                            ProcessOutput::fail().stderr(format_error!(config.theme, "AI didn't generate any command"))
                        );
                    }
                }
                Err(AppError::UserFacing(err)) => {
                    return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
                }
                Err(AppError::Unexpected(report)) => return Err(report),
            }
        }

        let command = Command::new(CATEGORY_USER, SOURCE_USER, command.unwrap_or_default())
            .with_alias(alias)
            .with_description(description);

        match service.insert_command(command).await {
            Ok(command) => Ok(ProcessOutput::success()
                .stderr(format_msg!(
                    config.theme,
                    "Command stored: {}",
                    config.theme.secondary.apply(&command.cmd)
                ))
                .fileout(command.cmd)),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}

impl InteractiveProcess for BookmarkCommandProcess {
    #[instrument(skip_all)]
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
    ) -> color_eyre::Result<Box<dyn Component>> {
        let BookmarkCommandProcess {
            command,
            alias,
            description,
            ai,
        } = self;

        let command = Command::new(CATEGORY_USER, SOURCE_USER, command.unwrap_or_default())
            .with_alias(alias)
            .with_description(description);

        Ok(Box::new(EditCommandComponent::new(
            service,
            config.theme,
            inline,
            command,
            EditCommandComponentMode::New { ai },
        )))
    }
}
