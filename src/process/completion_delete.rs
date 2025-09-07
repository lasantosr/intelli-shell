use color_eyre::Result;

use super::{Process, ProcessOutput};
use crate::{
    cli::CompletionDeleteProcess, config::Config, errors::AppError, format_error, format_msg,
    service::IntelliShellService,
};

impl Process for CompletionDeleteProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let root_cmd = self.command.as_deref().unwrap_or_default();
        let variable_name = &self.variable;
        match service.delete_variable_completion_by_key(root_cmd, variable_name).await {
            Ok(None) if root_cmd.trim().is_empty() => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Completion for global {} variable not found",
                config.theme.secondary.apply(variable_name),
            ))),
            Ok(None) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Completion for {} variable within {} commands not found",
                config.theme.secondary.apply(variable_name),
                config.theme.secondary.apply(&root_cmd),
            ))),
            Ok(Some(c)) if c.is_global() => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "Completion for global {} variable deleted",
                config.theme.secondary.apply(&c.variable),
            ))),
            Ok(Some(c)) => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "Completion for {} variable within {} commands deleted",
                config.theme.secondary.apply(&c.variable),
                config.theme.secondary.apply(&c.root_cmd),
            ))),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
