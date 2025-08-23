use color_eyre::Result;
use futures_util::StreamExt;

use super::{Process, ProcessOutput};
use crate::{
    cli::ImportCommandsProcess,
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

impl Process for ImportCommandsProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        let dry_run = self.dry_run;
        let mut commands = match service.get_commands_from_location(self, config.gist).await {
            Ok(s) => s,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };

        if dry_run {
            let mut stdout = String::new();
            while let Some(command) = commands.next().await {
                stdout += &command.map_err(AppError::into_report)?.to_string();
                stdout += "\n";
            }
            if stdout.is_empty() {
                Ok(ProcessOutput::fail().stderr(format_error!(&config.theme, "No commands were found")))
            } else {
                Ok(ProcessOutput::success().stdout(stdout))
            }
        } else {
            match service.import_commands(commands, false).await {
                Ok((0, 0)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "No commands were found"))),
                Ok((0, skipped)) => {
                    if dry_run {
                        Ok(ProcessOutput::success())
                    } else {
                        Ok(ProcessOutput::success().stderr(format_msg!(
                            config.theme,
                            "No commands imported, {skipped} already existed"
                        )))
                    }
                }
                Ok((imported, 0)) => {
                    if dry_run {
                        Ok(ProcessOutput::success())
                    } else {
                        Ok(ProcessOutput::success()
                            .stderr(format_msg!(config.theme, "Imported {imported} new commands")))
                    }
                }
                Ok((imported, skipped)) => {
                    if dry_run {
                        Ok(ProcessOutput::success())
                    } else {
                        Ok(ProcessOutput::success().stderr(format_msg!(
                            config.theme,
                            "Imported {imported} new commands {}",
                            config.theme.secondary.apply(format!("({skipped} already existed)"))
                        )))
                    }
                }
                Err(AppError::UserFacing(err)) => {
                    Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")))
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        }
    }
}

impl InteractiveProcess for ImportCommandsProcess {
    fn into_component(self, config: Config, service: IntelliShellService, inline: bool) -> Result<Box<dyn Component>> {
        Ok(Box::new(CommandsPickerComponent::new(
            service,
            config,
            inline,
            CommandsPickerComponentMode::Import { input: self },
        )))
    }
}
