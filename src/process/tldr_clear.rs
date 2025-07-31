use color_eyre::eyre::Result;

use super::{Process, ProcessOutput};
use crate::{cli::TldrClearProcess, config::Config, format_msg, service::IntelliShellService};

impl Process for TldrClearProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        match service.clear_tldr_commands(self.category).await {
            Ok(0) => Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "No commands were found"))),
            Ok(deleted) => {
                Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Removed {deleted} tldr commands")))
            }
            Err(report) => Err(report),
        }
    }
}
