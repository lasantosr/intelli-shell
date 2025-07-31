use color_eyre::eyre::Result;

use super::{Process, ProcessOutput};
use crate::{
    cli::ExportCommandsProcess, config::Config, errors::ImportExportError, format_error, format_msg,
    service::IntelliShellService,
};

impl Process for ExportCommandsProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        match service.export_commands(self, config.gist).await {
            Ok(0) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "No commands to export"))),
            Ok(exported) => {
                Ok(ProcessOutput::success().stderr(format_msg!(config.theme, "Exported {exported} commands")))
            }
            Err(ImportExportError::NotAFile) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "The path already exists and it's not a file"
            ))),
            Err(ImportExportError::FileNotFound) => Ok(
                ProcessOutput::fail().stderr(format_error!(config.theme, "The path of the file must already exist"))
            ),
            Err(ImportExportError::FileNotAccessible) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Cannot access the file, check write permissions"
            ))),
            Err(ImportExportError::FileBrokenPipe) => Ok(ProcessOutput::success()),
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
            Err(ImportExportError::GistLocationHasSha) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "Cannot export to a gist revision, provide a gist without a revision"
            ))),
            Err(ImportExportError::GistFileNotFound) => unreachable!(),
            Err(ImportExportError::GistMissingToken) => Ok(ProcessOutput::fail().stderr(format_error!(
                config.theme,
                "A GitHub token is required to export to a gist, set it in the config or GIST_TOKEN env variable"
            ))),
            Err(ImportExportError::GistRequestFailed(msg)) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "Gist request failed: {msg}")))
            }
            Err(ImportExportError::Unexpected(report)) => Err(report),
        }
    }
}
