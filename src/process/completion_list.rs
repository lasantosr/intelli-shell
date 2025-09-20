use color_eyre::Result;
use itertools::Itertools;
use tokio_util::sync::CancellationToken;

use super::{Process, ProcessOutput};
use crate::{
    cli::CompletionListProcess,
    component::{Component, completion_list::CompletionListComponent},
    config::Config,
    errors::AppError,
    format_error,
    process::InteractiveProcess,
    service::IntelliShellService,
};

impl Process for CompletionListProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        _cancellation_token: CancellationToken,
    ) -> Result<ProcessOutput> {
        match service.list_variable_completions(self.command.as_deref()).await {
            Ok(completions) => {
                Ok(ProcessOutput::success().stdout(completions.into_iter().map(|c| c.to_string()).join("\n")))
            }
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}

impl InteractiveProcess for CompletionListProcess {
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
        cancellation_token: CancellationToken,
    ) -> Result<Box<dyn Component>> {
        Ok(Box::new(CompletionListComponent::new(
            service,
            config,
            inline,
            self.command,
            cancellation_token,
        )))
    }
}
