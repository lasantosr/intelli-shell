use color_eyre::eyre::Result;

use super::{Process, ProcessOutput};
use crate::{cli::QueryProcess, config::Config, service::IntelliShellService};

impl Process for QueryProcess {
    async fn execute(self, _config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let sql = self.sql.contents()?;
        let output = service.query(sql).await?;
        Ok(ProcessOutput::success().stdout(output))
    }
}
