use chrono::{DateTime, Utc};
use color_eyre::{Report, Result, eyre::Context};
use semver::Version;
use tracing::instrument;

use super::SqliteStorage;

impl SqliteStorage {
    /// Gets the current version info from the database
    #[instrument(skip_all)]
    pub async fn get_version_info(&self) -> Result<(Version, DateTime<Utc>)> {
        self.client
            .conn(move |conn| {
                conn.query_one(
                    "SELECT latest_version, last_checked_at FROM version_info LIMIT 1",
                    [],
                    |r| {
                        Ok((
                            Version::parse(&r.get::<_, String>(0)?).expect("valid version"),
                            r.get(1)?,
                        ))
                    },
                )
                .map_err(Report::from)
            })
            .await
            .wrap_err("Couldn't retrieve latest version info")
    }

    /// Updates the version info in the database
    #[instrument(skip_all)]
    pub async fn update_version_info(&self, latest_version: Version, last_checked_at: DateTime<Utc>) -> Result<()> {
        self.client
            .conn_mut(move |conn| {
                conn.execute(
                    "UPDATE version_info SET latest_version = ?1, last_checked_at = ?2",
                    (latest_version.to_string(), last_checked_at),
                )
                .map_err(Report::from)
            })
            .await
            .wrap_err("Couldn't update latest version info")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_get_and_update_version_info() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // There should always be a row in version_info after migrations
        let (version, checked_at) = storage.get_version_info().await.unwrap();
        assert_eq!(version, Version::parse("0.0.0").unwrap());
        assert!(checked_at <= Utc::now());

        // Update version info
        let new_version = Version::parse("1.2.3").unwrap();
        let new_checked_at = Utc::now();
        storage
            .update_version_info(new_version.clone(), new_checked_at)
            .await
            .unwrap();

        // Check that the update is reflected
        let (updated_version, updated_checked_at) = storage.get_version_info().await.unwrap();
        assert_eq!(updated_version, new_version);
        assert_eq!(updated_checked_at.timestamp(), new_checked_at.timestamp());
    }
}
