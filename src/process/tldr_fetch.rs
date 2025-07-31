use std::io::{BufRead, BufReader};

use color_eyre::eyre::{Context, Result};

use super::{Process, ProcessOutput};
use crate::{cli::TldrFetchProcess, config::Config, format_error, format_msg, service::IntelliShellService};

impl Process for TldrFetchProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let mut commands = self.commands;
        if let Some(filter_commands) = self.filter_commands {
            let content = filter_commands
                .into_reader()
                .wrap_err("Couldn't read filter commands file")?;
            let reader = BufReader::new(content);
            for line in reader.lines() {
                let line = line.wrap_err("Failed to read line from filter commands")?;
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("#") && !trimmed.starts_with("//") {
                    commands.push(trimmed.to_string());
                }
            }
        }
        match service.fetch_tldr_commands(self.category, commands).await {
            Ok((0, 0)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "No commands were found"))),
            Ok((0, updated)) => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "No new commands imported, {updated} already existed"
            ))),
            Ok((imported, 0)) => {
                Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Imported {imported} new commands")))
            }
            Ok((imported, updated)) => Ok(ProcessOutput::success().stderr(format_msg!(
                config.theme,
                "Imported {imported} new commands {}",
                config.theme.secondary.apply(format!("({updated} already existed)"))
            ))),
            Err(report) => Err(report),
        }
    }
}
