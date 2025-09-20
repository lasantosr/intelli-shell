use tokio_util::sync::CancellationToken;

use super::{Process, ProcessOutput};
use crate::{
    cli::TldrClearProcess, config::Config, errors::AppError, format_error, format_msg, service::IntelliShellService,
};

impl Process for TldrClearProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        _cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        match service.clear_tldr_commands(self.category).await {
            Ok(0) => Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "No commands were found"))),
            Ok(deleted) => {
                Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Removed {deleted} tldr commands")))
            }
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
