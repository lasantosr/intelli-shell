use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio_util::sync::CancellationToken;
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
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        let BookmarkCommandProcess {
            mut command,
            alias,
            mut description,
            ai,
        } = self;

        // If AI is enabled, we expect a command or description to be provided
        if ai {
            let cmd = command.clone().unwrap_or_default();
            let desc = description.clone().unwrap_or_default();

            if cmd.trim().is_empty() && desc.trim().is_empty() {
                return Ok(ProcessOutput::fail().stderr(format_error!(
                    config.theme,
                    "{}",
                    UserFacingError::AiEmptyCommand
                )));
            }

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
            let res = service.suggest_command(cmd, desc, cancellation_token).await;

            // Clear the spinner
            pb.finish_and_clear();

            // Handle the result
            match res {
                Ok(Some(suggestion)) => {
                    command = Some(suggestion.cmd);
                    description = suggestion.description;
                }
                Ok(None) => {
                    return Ok(
                        ProcessOutput::fail().stderr(format_error!(config.theme, "AI didn't generate any command"))
                    );
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
        cancellation_token: CancellationToken,
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
            cancellation_token,
        )))
    }
}
