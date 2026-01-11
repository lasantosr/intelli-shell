use chrono::{DateTime, Utc};
use color_eyre::eyre::Context;
use reqwest::header;
use semver::Version;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    errors::{Result, UserFacingError},
    model::IntelliShellRelease,
};

/// Maximum number of releases to keep in the database
const MAX_RELEASES_KEPT: usize = 50;
/// Number of releases to fetch per page
const PAGE_SIZE: usize = 30;
/// Maximum number of pages to fetch, as a safeguard to prevent infinite loops
const MAX_PAGES: usize = MAX_RELEASES_KEPT.div_ceil(PAGE_SIZE);

impl IntelliShellService {
    /// Retrieves releases from the database sorted by descending version, optionally fetching them from GitHub.
    ///
    /// Returns stored releases, fetching from GitHub if data is missing, stale or if `force_fetch` is true.
    #[instrument(skip_all)]
    pub async fn get_or_fetch_releases(
        &self,
        force_fetch: bool,
        token: CancellationToken,
    ) -> Result<Vec<IntelliShellRelease>> {
        let now = Utc::now();
        let latest_info = self.storage.get_latest_stored_version().await?;

        // Determine if we should hit the GitHub API
        let should_fetch = force_fetch
            || latest_info.as_ref().is_none_or(|(version, fetched_at)| {
                // Fetch if interval passed or we lack current version's data
                let is_stale = (now - *fetched_at) >= super::FETCH_INTERVAL;
                let is_behind_current = version < &*super::CURRENT_VERSION;
                is_stale || is_behind_current
            });

        if should_fetch {
            let target_version = latest_info.as_ref().map(|(v, _)| v.clone());
            let fetched_releases = fetch_release_history_from_github(target_version, token).await?;
            if !fetched_releases.is_empty() {
                // Upsert all fetched releases and maintain database size.
                // We always upsert to:
                // 1. Refresh 'fetched_at' timestamp (preventing immediate re-fetch).
                // 2. Pull content updates/fixes (e.g. body typo fixes) from GitHub.
                self.storage.upsert_releases(fetched_releases).await?;
                self.storage.prune_releases(MAX_RELEASES_KEPT).await?;
            }
        } else {
            tracing::debug!("Skipping release retrieval as it was checked recently");
        }

        // Return stored releases
        self.storage.get_releases().await
    }
}

/// A simple struct to deserialize the relevant fields from the GitHub API response
#[derive(Deserialize, Debug)]
struct GithubRelease {
    tag_name: String,
    published_at: DateTime<Utc>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

/// Fetches release history from GitHub starting from the `target_version` (or strictly newer).
///
/// This function handles pagination to ensure all intermediate releases are fetched.
/// It returns a vector of releases found (newer than target_version).
async fn fetch_release_history_from_github(
    target_version: Option<Version>,
    token: CancellationToken,
) -> Result<Vec<IntelliShellRelease>> {
    tracing::debug!("Fetching GitHub release history (target: {target_version:?})");

    let client = reqwest::Client::new();
    let fetch_limit = if target_version.is_none() { 1 } else { MAX_PAGES };
    let mut results = Vec::new();

    // Fetch paginated releases from GitHub
    for page in 1..=fetch_limit {
        let releases = fetch_github_page(&client, page, token.clone()).await?;

        if releases.is_empty() {
            break;
        }

        let mut should_stop_after_page = false;
        for r in releases {
            // Keep only published stable releases
            if r.draft || r.prerelease {
                continue;
            }

            // Parse version
            let version_str = r.tag_name.trim_start_matches('v');
            if let Ok(v) = Version::parse(version_str) {
                // If we hit the target, we mark that we should stop AFTER this page.
                // This ensures all releases on this page are returned and upserted,
                // but we won't fetch any more pages.
                if let Some(target) = &target_version
                    && &v <= target
                {
                    should_stop_after_page = true;
                }

                results.push(IntelliShellRelease {
                    title: r.name.unwrap_or_else(|| r.tag_name.clone()),
                    tag: r.tag_name,
                    body: r.body,
                    published_at: r.published_at,
                    version: v,
                    fetched_at: Utc::now(),
                });
            }
        }

        if should_stop_after_page {
            break;
        }
    }

    Ok(results)
}

/// Fetches a single page of releases from GitHub
async fn fetch_github_page(
    client: &reqwest::Client,
    page: usize,
    token: CancellationToken,
) -> Result<Vec<GithubRelease>> {
    tracing::trace!("Fetching page {page}...");

    let res = tokio::select! {
        biased;
        _ = token.cancelled() => {
            return Err(UserFacingError::Cancelled.into());
        }
        res = client
            .get("https://api.github.com/repos/lasantosr/intelli-shell/releases")
            .query(&[("per_page", PAGE_SIZE), ("page", page)])
            .header(header::USER_AGENT, "intelli-shell")
            .timeout(super::REQUEST_TIMEOUT)
            .send() => res.map_err(|err| {
                tracing::error!("{err:?}");
                UserFacingError::ReleaseRequestFailed(err.to_string())
            })?
    };

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
            return Err(UserFacingError::ReleaseRequestFailed(message).into());
        } else {
            tracing::error!("Got response [{status_str}]:\n{body}");
            return Err(UserFacingError::ReleaseRequestFailed(message).into());
        }
    }

    Ok(res.json().await.wrap_err("Failed to parse releases response")?)
}
