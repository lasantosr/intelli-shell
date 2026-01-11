use chrono::{DateTime, Utc};
use semver::Version;

/// Represents an intelli-shell release
#[derive(Debug, Clone)]
pub struct IntelliShellRelease {
    pub tag: String,
    pub version: Version,
    pub title: String,
    pub body: Option<String>,
    pub published_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
}
