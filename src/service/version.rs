use std::{env, time::Duration};

use chrono::Utc;
use color_eyre::{Result, eyre::Context};
use reqwest::header;
use semver::Version;
use serde::Deserialize;
use tracing::instrument;

use super::IntelliShellService;

/// The timeout for the request to check for a new version.
/// This is set to a small amount to avoid hanging the application for too long.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(750);

impl IntelliShellService {
    /// Checks if there's a new version available, returning the version if so
    #[instrument(skip_all)]
    pub async fn check_new_version(&self) -> Result<Option<Version>> {
        // If the check_updates is disabled, skip the version check
        if !self.check_updates {
            tracing::debug!("Skipping version check as it's disabled in the configuration");
            return Ok(None);
        }

        // Get the current version and the last checked version
        let now = Utc::now();
        let current = Version::parse(env!("CARGO_PKG_VERSION")).wrap_err("Failed to parse current version")?;
        let (latest, checked_at) = self.storage.get_version_info().await?;

        tracing::trace!("Current version: v{current}");

        // If the latest version was checked recently, return whether it's newer than the current one
        if (now - checked_at) < chrono::Duration::hours(16) {
            tracing::debug!("Skipping version retrieval as it was checked recently, latest: v{latest}");
            return Ok(Some(latest).filter(|v| v > &current));
        }

        // A simple struct to deserialize the relevant fields from the GitHub API response
        #[derive(Deserialize, Debug)]
        struct Release {
            tag_name: String,
        }

        // Fetch latest release from GitHub
        let release: Release = reqwest::Client::new()
            .get("https://api.github.com/repos/lasantosr/intelli-shell/releases/latest")
            .header(header::USER_AGENT, "intelli-shell")
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .wrap_err("Failed to fetch latest release from GitHub")?
            .json()
            .await
            .wrap_err("Failed to parse latest release response")?;

        // Parse it
        let tag_version = release.tag_name.trim_start_matches('v');
        let latest = Version::parse(tag_version)
            .wrap_err_with(|| format!("Failed to parse latest version from tag: {tag_version}"))?;

        tracing::debug!("Fetched latest version: v{latest}");

        // Store the new information in the database
        self.storage.update_version_info(latest.clone(), now).await?;

        // Return whether the latest version is newer than the current one
        Ok(Some(latest).filter(|v| v > &current))
    }
}
