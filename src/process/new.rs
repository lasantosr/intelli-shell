use color_eyre::eyre::Result;
use semver::Version;
use tracing::instrument;

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::BookmarkCommandProcess,
    component::{
        Component,
        edit::{EditCommandComponent, EditCommandComponentMode},
    },
    config::Config,
    errors::InsertError,
    format_error, format_msg,
    model::{CATEGORY_USER, Command, SOURCE_USER},
    service::IntelliShellService,
};

impl Process for BookmarkCommandProcess {
    #[instrument(skip_all)]
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let BookmarkCommandProcess {
            command,
            alias,
            description,
        } = self;

        let command = Command::new(CATEGORY_USER, SOURCE_USER, command.unwrap_or_default())
            .with_alias(alias)
            .with_description(description);

        match service.insert_command(command).await {
            Ok(command) => Ok(ProcessOutput::success()
                .stderr(format_msg!(
                    config.theme,
                    "Command stored: {}",
                    config.theme.secondary.apply(&command.cmd)
                ))
                .fileout(command.cmd)),
            Err(InsertError::Invalid(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(InsertError::AlreadyExists) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "The command is already bookmarked")))
            }
            Err(InsertError::Unexpected(report)) => Err(report),
        }
    }
}

impl InteractiveProcess for BookmarkCommandProcess {
    #[instrument(skip_all)]
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
        new_version: Option<Version>,
    ) -> Result<Box<dyn Component>> {
        let BookmarkCommandProcess {
            command,
            alias,
            description,
        } = self;

        let command = Command::new(CATEGORY_USER, SOURCE_USER, command.unwrap_or_default())
            .with_alias(alias)
            .with_description(description);

        Ok(Box::new(EditCommandComponent::new(
            service,
            config.theme,
            inline,
            new_version,
            command,
            EditCommandComponentMode::New,
        )))
    }
}
