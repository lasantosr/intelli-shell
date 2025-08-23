use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use tracing::instrument;

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::SearchCommandsProcess,
    component::{Component, search::SearchCommandsComponent},
    config::Config,
    errors::{AppError, UserFacingError},
    format_error,
    service::IntelliShellService,
    widgets::SPINNER_CHARS,
};

impl Process for SearchCommandsProcess {
    #[instrument(skip_all)]
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        // Different behaviors based on ai flag
        if self.ai {
            // Validate we've a query
            let prompt = self.query.as_deref().unwrap_or_default();
            if prompt.trim().is_empty() {
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
            let res = service.suggest_commands(prompt).await;

            // Clear the spinner
            pb.finish_and_clear();

            // Handle the result
            match res {
                Ok(commands) => Ok(ProcessOutput::success().stdout(commands.into_iter().map(|c| c.cmd).join("\n"))),
                Err(AppError::UserFacing(err)) => {
                    Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")))
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        } else {
            // Merge config with args
            let (config, query) = merge_config(self, config);

            // Search for commands and handle result
            match service
                .search_commands(config.search.mode, config.search.user_only, &query)
                .await
            {
                Ok((commands, _)) => {
                    Ok(ProcessOutput::success().stdout(commands.into_iter().map(|c| c.cmd).join("\n")))
                }
                Err(AppError::UserFacing(err)) => {
                    Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")))
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        }
    }
}

impl InteractiveProcess for SearchCommandsProcess {
    #[instrument(skip_all)]
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
    ) -> color_eyre::Result<Box<dyn Component>> {
        let ai = self.ai;
        let (config, query) = merge_config(self, config);
        Ok(Box::new(SearchCommandsComponent::new(
            service, config, inline, query, ai,
        )))
    }
}

fn merge_config(p: SearchCommandsProcess, mut config: Config) -> (Config, String) {
    let SearchCommandsProcess {
        query,
        mode,
        user_only,
        ai: _,
    } = p;
    config.search.mode = mode.unwrap_or(config.search.mode);
    config.search.user_only = user_only || config.search.user_only;
    (config, query.unwrap_or_default())
}
