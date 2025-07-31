use color_eyre::eyre::Result;

use super::{Process, ProcessOutput};
use crate::{
    cli::ImportCommandsProcess, config::Config, errors::ImportExportError, format_error, format_msg,
    service::IntelliShellService,
};

impl Process for ImportCommandsProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let dry_run = self.dry_run;
        match service.import_commands(self, config.gist).await {
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
                    Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Imported {imported} new commands")))
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
            Err(ImportExportError::NotAFile) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Symlinks and directories are not supported, provide a file instead"
            ))),
            Err(ImportExportError::FileNotFound) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "File not found")))
            }
            Err(ImportExportError::FileNotAccessible) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Cannot access the file, check read permissions"
            ))),
            Err(ImportExportError::FileBrokenPipe) => unreachable!(),
            Err(ImportExportError::HttpInvalidUrl) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "The provided URL is invalid, please provide a valid HTTP/S URL"
            ))),
            Err(ImportExportError::HttpRequestFailed(msg)) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "HTTP request failed: {msg}")))
            }
            Err(ImportExportError::GistMissingId) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "A gist id must be provided either on the arguments or the config file"
            ))),
            Err(ImportExportError::GistInvalidLocation) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "The provided gist is not valid, please provide a valid id or URL"
            ))),
            Err(ImportExportError::GistFileNotFound) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "The provided gist file was not found")))
            }
            Err(ImportExportError::GistLocationHasSha) => unreachable!(),
            Err(ImportExportError::GistMissingToken) => unreachable!(),
            Err(ImportExportError::GistRequestFailed(msg)) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "Gist request failed: {msg}")))
            }
            Err(ImportExportError::Unexpected(report)) => Err(report),
        }
    }
}
