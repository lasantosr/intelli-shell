use std::{env, fs::File, path::PathBuf};

use color_eyre::{Result, eyre::Context};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::config::Config;

/// Resolves the log path and filter based on the config and environment variable.
/// If logging is disabled, returns `None` for the filter.
pub fn resolve_path_and_filter(config: &Config) -> (PathBuf, Option<String>) {
    let env_filter = env::var("INTELLI_LOG").ok();
    let logs_path = config.data_dir.join("intelli-shell.log");
    let filter = (config.logs.enabled || env_filter.is_some())
        .then(move || env_filter.unwrap_or_else(|| config.logs.filter.clone()));
    (logs_path, filter)
}

/// Initializes the tracing subscriber to output logs to a file
pub fn init(logs_path: PathBuf, filter: Option<String>) -> Result<()> {
    if let Some(filter) = filter {
        // Create the log file under the data dir
        let log_file = File::create(&logs_path)
            .wrap_err_with(|| format!("Couldn't create the log file: {}", logs_path.display()))?;
        // Initialize the env filter
        let env_filter = EnvFilter::builder()
            .with_default_directive(tracing::Level::WARN.into())
            .parse(filter)
            .wrap_err("Couldn't parse the log filter")?;
        // Subscribe logs to the file
        let file_subscriber = fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_writer(log_file)
            .with_target(false)
            .with_ansi(false)
            .with_filter(env_filter);
        tracing_subscriber::registry()
            .with(file_subscriber)
            .with(ErrorLayer::default())
            .init();
    }
    Ok(())
}
