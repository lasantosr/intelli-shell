use color_eyre::Result;
use tokio_util::sync::CancellationToken;

use super::{Process, ProcessOutput};
use crate::{
    cli::ExportItemsProcess,
    component::{
        Component,
        pick::{ImportExportPickerComponent, ImportExportPickerComponentMode},
    },
    config::Config,
    errors::AppError,
    format_error,
    process::InteractiveProcess,
    service::IntelliShellService,
};

impl Process for ExportItemsProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        _cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        let items = match service.prepare_items_export(self.filter.clone()).await {
            Ok(s) => s,
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };
        match service.export_items(items, self, config.gist).await {
            Ok(stats) => Ok(stats.into_output(&config.theme)),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
impl InteractiveProcess for ExportItemsProcess {
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
            ImportExportPickerComponentMode::Export { input: self },
            cancellation_token,
        )))
    }
}
