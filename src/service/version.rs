use std::{env, time::Duration};

use chrono::Utc;
use color_eyre::eyre::Context;
use reqwest::header;
use semver::Version;
use serde::Deserialize;
use tracing::{Instrument, instrument};

use super::IntelliShellService;
use crate::{errors::Result, storage::SqliteStorage};

/// The timeout for the request to check for a new version
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Represents the state of the background version check
#[derive(Debug)]
pub(super) enum VersionCheckState {
    /// The check has not been started yet
    NotStarted,
    /// The check is currently in progress
    InProgress,
    /// The check has finished, and the result is cached
    Finished(Option<Version>),
}

impl IntelliShellService {
    /// Checks if there's a new version available. This method returns immediately.
    ///
    /// On the first call, it spawns a background task to check for a new version.
    /// Subsequent calls will return `None` until the check is complete.
    /// Once finished, it will always return the cached result.
    #[instrument(skip_all)]
    pub fn check_new_version(&self) -> Option<Version> {
        // Lock the state to check the current status of the version check
        let mut state = self.version_check_state.lock().expect("poisoned lock");

        match &*state {
            // If the check has already finished, return the cached result
            VersionCheckState::Finished(version) => version.clone(),

            // If the check is in progress, do nothing and return None
            VersionCheckState::InProgress => None,

            // If the check hasn't started
            VersionCheckState::NotStarted => {
                // When check_updates is disabled, skip the version check
                if !self.check_updates {
                    tracing::debug!("Skipping version check as it's disabled in the configuration");
                    *state = VersionCheckState::Finished(None);
                    return None;
                }

                // If not disabled, spawn a background task to perform the version check and return `None` immediately
                *state = VersionCheckState::InProgress;
                tracing::trace!("Spawning background task for version check");
                drop(state);

                let storage = self.storage.clone();
                let state_clone = self.version_check_state.clone();
                tokio::spawn(
                    async move {
                        let result = perform_version_check(storage).await;

                        // Once the check is done, lock the state and update it with the result
                        let mut state = state_clone.lock().expect("poisoned lock");
                        match result {
                            Ok(version) => {
                                if let Some(ref v) = version {
                                    tracing::info!("New version available: v{v}");
                                } else {
                                    tracing::debug!("No new version available");
                                }
                                *state = VersionCheckState::Finished(version);
                            }
                            Err(err) => {
                                tracing::error!("Failed to check for new version: {err:#?}");
                                *state = VersionCheckState::Finished(None);
                            }
                        }
                    }
                    .instrument(tracing::info_span!("bg")),
                );

                None
            }
        }
    }
}

/// Performs the actual version check against the remote source
async fn perform_version_check(storage: SqliteStorage) -> Result<Option<Version>> {
    // Get the current version and the last checked version
    let now = Utc::now();
    let current = Version::parse(env!("CARGO_PKG_VERSION")).wrap_err("Failed to parse current version")?;
    let (latest, checked_at) = storage.get_version_info().await?;

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

    tracing::debug!("Latest version fetched: v{latest}");

    // Store the new information in the database
    storage.update_version_info(latest.clone(), now).await?;

    // Return whether the latest version is newer than the current one
    Ok(Some(latest).filter(|v| v > &current))
}
