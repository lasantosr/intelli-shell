use std::{borrow::Cow, env, fs::File, path::PathBuf};

use color_eyre::{Result, eyre::Context};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::config::Config;

/// Initializes the tracing subscriber to output logs to a file
pub fn init(config: &Config) -> Result<Option<PathBuf>> {
    let env_filter = env::var("INTELLI_LOG").ok();
    if config.logs.enabled || env_filter.is_some() {
        // Create the log file under the data dir
        let log_path = config.data_dir.join("intelli-shell.log");
        let log_file = File::create(&log_path)
            .wrap_err_with(|| format!("Couldn't create the log file: {}", log_path.display()))?;
        // Initialize the env filter
        let filter = env_filter
            .map(Cow::from)
            .unwrap_or_else(|| Cow::from(&config.logs.filter));
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
        Ok(Some(log_path))
    } else {
        Ok(None)
    }
}
