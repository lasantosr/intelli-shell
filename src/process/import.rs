use std::time::Duration;

use color_eyre::Result;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio_util::sync::CancellationToken;

use super::{Process, ProcessOutput};
use crate::{
    cli::ImportItemsProcess,
    component::{
        Component,
        pick::{ImportExportPickerComponent, ImportExportPickerComponentMode},
    },
    config::Config,
    errors::AppError,
    format_error,
    process::InteractiveProcess,
    service::IntelliShellService,
    widgets::SPINNER_CHARS,
};

impl Process for ImportItemsProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        let dry_run = self.dry_run;

        // If AI is enabled
        let res = if self.ai {
            // Setup the progress bar
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                    .unwrap()
                    .tick_strings(&SPINNER_CHARS),
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            pb.set_message("Thinking ...");

            // Retrieve items using AI
            let res = service
                .get_items_from_location(self, config.gist, cancellation_token)
                .await;

            // Clear the spinner
            pb.finish_and_clear();

            res
        } else {
            // If no AI, it's fast enough to not require a spinner
            service
                .get_items_from_location(self, config.gist, cancellation_token)
                .await
        };

        // Check retrieval result
        let mut items = match res {
            Ok(s) => s,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };

        // Print them out when in dry-run
        if dry_run {
            let mut stdout = String::new();
            while let Some(item) = items.next().await {
                stdout += &item.map_err(AppError::into_report)?.to_string();
                stdout += "\n";
            }
            if stdout.is_empty() {
                Ok(ProcessOutput::fail().stderr(format_error!(&config.theme, "No commands or completions were found")))
            } else {
                Ok(ProcessOutput::success().stdout(stdout))
            }
        } else {
            // Or import them when not dry-run
            match service.import_items(items, false).await {
                Ok(stats) => Ok(stats.into_output(&config.theme)),
                Err(AppError::UserFacing(err)) => {
                    Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")))
                }
                Err(AppError::Unexpected(report)) => Err(report),
            }
        }
    }
}

impl InteractiveProcess for ImportItemsProcess {
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
        cancellation_token: CancellationToken,
    ) -> Result<Box<dyn Component>> {
        Ok(Box::new(ImportExportPickerComponent::new(
            service,
            config,
            inline,
            ImportExportPickerComponentMode::Import { input: self },
            cancellation_token,
        )))
    }
}
