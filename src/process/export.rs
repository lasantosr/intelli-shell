use color_eyre::Result;

use super::{Process, ProcessOutput};
use crate::{
    cli::ExportCommandsProcess,
    component::{
        Component,
        pick::{CommandsPickerComponent, CommandsPickerComponentMode},
    },
    config::Config,
    errors::AppError,
    format_error, format_msg,
    process::InteractiveProcess,
    service::IntelliShellService,
};

impl Process for ExportCommandsProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        let commands = match service.prepare_commands_export(self.filter.clone()).await {
            Ok(s) => s,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };
        match service.export_commands(commands, self, config.gist).await {
            Ok((0, _)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "No commands to export"))),
            Ok((exported, None)) => {
                Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Exported {exported} commands")))
            }
            Ok((exported, Some(stdout))) => Ok(ProcessOutput::success()
                .stdout(stdout)
                .stderr(format_msg!(config.theme, "Exported {exported} commands"))),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
impl InteractiveProcess for ExportCommandsProcess {
    fn into_component(self, config: Config, service: IntelliShellService, inline: bool) -> Result<Box<dyn Component>> {
        Ok(Box::new(CommandsPickerComponent::new(
            service,
            config,
            inline,
            CommandsPickerComponentMode::Export { input: self },
        )))
    }
}
