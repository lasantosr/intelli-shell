use std::{env, time::Duration};

use chrono::Utc;
use color_eyre::eyre::Context;
use reqwest::header;
use semver::Version;
use serde::Deserialize;
use tracing::{Instrument, instrument};

use super::IntelliShellService;
use crate::{
    errors::{Result, UserFacingError},
    storage::SqliteStorage,
};

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
    /// Polls for a new version in a non-blocking manner.
    ///
    /// This method returns immediately. On the first call, it spawns a background task to fetch the latest version.
    /// Subsequent calls will return `None` while the check is in progress. Once the check is finished, this method
    /// will consistently return the cached result.
    #[instrument(skip_all)]
    pub fn poll_new_version(&self) -> Option<Version> {
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
                        let result = fetch_latest_version(&storage, false).await;

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

    /// Checks for a new version, performing a network request if necessary.
    ///
    /// This method performs the version check immediately, blocking the caller.
    ///
    /// For a non-blocking alternative that spawns a background task, see [`poll_new_version`](Self::poll_new_version).
    #[instrument(skip_all)]
    pub async fn check_new_version(&self, force_fetch: bool) -> Result<Option<Version>> {
        fetch_latest_version(&self.storage, force_fetch).await
    }
}

/// Fetches the latest version from the remote source, respecting a time-based cache.
///
/// It first consults the local database to see if a check was performed within the last hours.
/// If not, it proceeds to fetch the latest release from the GitHub API, updates the database with the new version and
/// timestamp, and returns the result.
///
/// It will return `None` if the latest version is not newer than the actual one.
async fn fetch_latest_version(storage: &SqliteStorage, force_fetch: bool) -> Result<Option<Version>> {
    // Get the current version
    let now = Utc::now();
    let current = Version::parse(env!("CARGO_PKG_VERSION")).wrap_err("Failed to parse current version")?;

    // When not forcing a fetch, check the database cache for recent version info
    if !force_fetch {
        let (latest, checked_at) = storage.get_version_info().await?;

        // If the latest version was checked recently, return whether it's newer than the current one
        if (now - checked_at) < chrono::Duration::hours(16) {
            tracing::debug!("Skipping version retrieval as it was checked recently, latest: v{latest}");
            return Ok(Some(latest).filter(|v| v > &current));
        }
    }

    // A simple struct to deserialize the relevant fields from the GitHub API response
    #[derive(Deserialize, Debug)]
    struct Release {
        tag_name: String,
    }

    // Fetch latest release from GitHub
    let res = reqwest::Client::new()
        .get("https://api.github.com/repos/lasantosr/intelli-shell/releases/latest")
        .header(header::USER_AGENT, "intelli-shell")
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|err| {
            tracing::error!("{err:?}");
            UserFacingError::LatestVersionRequestFailed(err.to_string())
        })?;

    if !res.status().is_success() {
        let status = res.status();
        let status_str = status.as_str();
        let body = res.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| format!("received {status_str} response"));
        if let Some(reason) = status.canonical_reason() {
            tracing::error!("Got response [{status_str}] {reason}:\n{body}");
            return Err(UserFacingError::LatestVersionRequestFailed(message).into());
        } else {
            tracing::error!("Got response [{status_str}]:\n{body}");
            return Err(UserFacingError::LatestVersionRequestFailed(message).into());
        }
    }

    let release: Release = res.json().await.wrap_err("Failed to parse latest release response")?;

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
