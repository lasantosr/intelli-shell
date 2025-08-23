use super::{Process, ProcessOutput};
use crate::{cli::QueryProcess, config::Config, errors::AppError, format_error, service::IntelliShellService};

impl Process for QueryProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        let sql = self.sql.contents()?;
        match service.query(sql).await {
            Ok(output) => Ok(ProcessOutput::success().stdout(output)),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
