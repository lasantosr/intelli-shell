use std::time::Duration;

use color_eyre::Result;
use indicatif::{ProgressBar, ProgressStyle};

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::CompletionNewProcess,
    component::{
        Component,
        completion_edit::{EditCompletionComponent, EditCompletionComponentMode},
    },
    config::Config,
    errors::{AppError, UserFacingError},
    format_error, format_msg,
    model::{SOURCE_USER, VariableCompletion},
    service::IntelliShellService,
    widgets::SPINNER_CHARS,
};

impl Process for CompletionNewProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let root_cmd = self.command.unwrap_or_default();
        let suggestions_provider = self.provider.unwrap_or_default();
        let Some(variable_name) = self.variable.filter(|v| !v.trim().is_empty()) else {
            return Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "{}",
                UserFacingError::CompletionEmptyVariable
            )));
        };

        let mut completion = VariableCompletion::new(SOURCE_USER, root_cmd, variable_name, suggestions_provider);

        // If AI is enabled
        if self.ai {
            // Setup the progress bar
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                    .unwrap()
                    .tick_strings(&SPINNER_CHARS),
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            pb.set_message("Thinking ...");

            // Suggest provider using AI
            let res = service
                .suggest_completion(
                    &completion.root_cmd,
                    &completion.variable,
                    &completion.suggestions_provider,
                )
                .await;

            // Clear the spinner
            pb.finish_and_clear();

            // Handle the result
            match res {
                Ok(p) if p.is_empty() => {
                    return Ok(
                        ProcessOutput::fail().stderr(format_error!(config.theme, "AI generated an empty response"))
                    );
                }
                Ok(provider) => {
                    completion.suggestions_provider = provider;
                }
                Err(AppError::UserFacing(err)) => {
                    return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
                }
                Err(AppError::Unexpected(report)) => return Err(report),
            }
        }

        match service.create_variable_completion(completion).await {
            Ok(c) if c.is_global() => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "Completion for global {} variable stored: {}",
                config.theme.secondary.apply(&c.variable),
                config.theme.secondary.apply(&c.suggestions_provider)
            ))),
            Ok(c) => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "Completion for {} variable within {} commands stored: {}",
                config.theme.secondary.apply(&c.variable),
                config.theme.secondary.apply(&c.root_cmd),
                config.theme.secondary.apply(&c.suggestions_provider)
            ))),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}

impl InteractiveProcess for CompletionNewProcess {
    fn into_component(self, config: Config, service: IntelliShellService, inline: bool) -> Result<Box<dyn Component>> {
        let root_cmd = self.command.unwrap_or_default();
        let suggestions_provider = self.provider.unwrap_or_default();
        let variable_name = self.variable.unwrap_or_default();

        let completion = VariableCompletion::new(SOURCE_USER, root_cmd, variable_name, suggestions_provider);

        let component = EditCompletionComponent::new(
            service,
            config.theme,
            inline,
            completion,
            EditCompletionComponentMode::New { ai: self.ai },
        );
        Ok(Box::new(component))
    }
}
