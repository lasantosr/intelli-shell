use color_eyre::eyre::Result;
use itertools::Itertools;
use semver::Version;
use tracing::instrument;

use super::{InteractiveProcess, Process, ProcessOutput};
use crate::{
    cli::SearchCommandsProcess,
    component::{Component, search::SearchCommandsComponent},
    config::Config,
    errors::SearchError,
    format_error,
    service::IntelliShellService,
};

impl Process for SearchCommandsProcess {
    #[instrument(skip_all)]
    async fn execute(self, config: Config, service: IntelliShellService) -> Result<ProcessOutput> {
        let (config, query) = merge_config(self, config);

        match service
            .search_commands(config.search.mode, config.search.user_only, &query)
            .await
        {
            Ok((commands, _)) => Ok(ProcessOutput::success().stdout(commands.into_iter().map(|c| c.cmd).join("\n"))),
            Err(SearchError::InvalidFuzzy) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "Invalid fuzzy search term")))
            }
            Err(SearchError::InvalidRegex(err)) => {
                Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "Invalid regex pattern: {}", err)))
            }
            Err(SearchError::Unexpected(report)) => Err(report),
        }
    }
}

impl InteractiveProcess for SearchCommandsProcess {
    #[instrument(skip_all)]
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
        new_version: Option<Version>,
    ) -> Result<Box<dyn Component>> {
        let (config, query) = merge_config(self, config);
        Ok(Box::new(SearchCommandsComponent::new(
            service,
            config,
            inline,
            new_version,
            query,
        )))
    }
}

fn merge_config(p: SearchCommandsProcess, mut config: Config) -> (Config, String) {
    let SearchCommandsProcess { query, mode, user_only } = p;
    config.search.mode = mode.unwrap_or(config.search.mode);
    config.search.user_only = user_only || config.search.user_only;
    (config, query.unwrap_or_default())
}
