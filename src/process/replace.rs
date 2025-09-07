use color_eyre::eyre::Result;
use tracing::instrument;

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::VariableReplaceProcess,
    component::{Component, variable::VariableReplacementComponent},
    config::Config,
    format_error,
    model::CommandTemplate,
    service::IntelliShellService,
};

impl Process for VariableReplaceProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        match service.replace_command_variables(self.command.into_inner(), self.values, self.use_env) {
            Ok(command) => Ok(ProcessOutput::success().stdout(&command).fileout(command)),
            Err(missing) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Missing variable values: {}",
                missing.join(", ")
            ))),
        }
    }
}

impl InteractiveProcess for VariableReplaceProcess {
    #[instrument(skip_all)]
    fn into_component(self, config: Config, service: IntelliShellService, inline: bool) -> Result<Box<dyn Component>> {
        let command = CommandTemplate::parse(self.command.into_inner(), true);
        Ok(Box::new(VariableReplacementComponent::new(
            service,
            config.theme,
            inline,
            false,
            true,
            command,
        )))
    }
}
