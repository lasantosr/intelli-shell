use chrono::{DateTime, Utc};
use itertools::Itertools;
use rusqlite::{OptionalExtension, params};
use semver::Version;
use tracing::instrument;

use super::SqliteStorage;
use crate::{errors::Result, model::IntelliShellRelease};

impl SqliteStorage {
    /// Upserts a list of releases into the database
    #[instrument(skip(self, releases))]
    pub async fn upsert_releases(&self, releases: Vec<IntelliShellRelease>) -> Result<()> {
        if releases.is_empty() {
            return Ok(());
        }

        self.client
            .conn_mut(move |conn| {
                let tx = conn.transaction()?;
                {
                    let mut stmt = tx.prepare(
                        "INSERT OR REPLACE INTO release_info (tag, version, title, body, published_at, fetched_at)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )?;

                    for release in releases {
                        tracing::trace!("Upserting release: {}", release.tag);
                        stmt.execute(params![
                            release.tag,
                            release.version.to_string(),
                            release.title,
                            release.body,
                            release.published_at,
                            release.fetched_at,
                        ])?;
                    }
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Retrieves releases from the database, sorted by descending version
    #[instrument(skip_all)]
    pub async fn get_releases(&self) -> Result<Vec<IntelliShellRelease>> {
        self.client
            .conn(move |conn| {
                let query = "SELECT tag, version, title, body, published_at, fetched_at FROM release_info ORDER BY \
                             published_at DESC";
                tracing::trace!("Querying release versions:\n{query}");
                Ok(conn
                    .prepare(query)?
                    .query_map([], |row| {
                        Ok(IntelliShellRelease {
                            tag: row.get(0)?,
                            version: Version::parse(&row.get::<_, String>(1)?).expect("valid version"),
                            title: row.get(2)?,
                            body: row.get(3)?,
                            published_at: row.get(4)?,
                            fetched_at: row.get(5)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .await
            .map(|all_releases| {
                all_releases
                    .into_iter()
                    .sorted_by(|a, b| b.version.cmp(&a.version))
                    .collect()
            })
    }

    /// Gets the latest stored version and its fetch time
    #[instrument(skip_all)]
    pub async fn get_latest_stored_version(&self) -> Result<Option<(Version, DateTime<Utc>)>> {
        self.client
            .conn(move |conn| {
                // Determine latest by published_at
                let query = "SELECT version, fetched_at FROM release_info ORDER BY published_at DESC LIMIT 1";
                tracing::trace!("Checking latest release version:\n{query}");
                Ok(conn
                    .query_row(query, [], |row| {
                        Ok((
                            Version::parse(&row.get::<_, String>(0)?).expect("valid version"),
                            row.get(1)?,
                        ))
                    })
                    .optional()?)
            })
            .await
    }

    /// Prunes the release history to keep only the specified number of recent releases
    #[instrument(skip(self))]
    pub async fn prune_releases(&self, keep: usize) -> Result<()> {
        self.client
            .conn_mut(move |conn| {
                let query = "DELETE FROM release_info WHERE tag NOT IN (
                    SELECT tag FROM release_info ORDER BY published_at DESC LIMIT ?1
                )";
                tracing::trace!("Pruning releases to keep {keep}:\n{query}");
                conn.execute(query, params![keep])?;
                Ok(())
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_release_storage_ops() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // 1. Initial State: Empty
        let releases = storage.get_releases().await.unwrap();
        assert!(releases.is_empty());
        assert!(storage.get_latest_stored_version().await.unwrap().is_none());

        // 2. Upsert Releases
        let release_v1 = IntelliShellRelease {
            tag: "v1.0.0".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            title: "Release 1".to_string(),
            body: Some("Body 1".to_string()),
            published_at: Utc::now() - chrono::Duration::days(10),
            fetched_at: Utc::now(),
        };
        let release_v2 = IntelliShellRelease {
            tag: "v2.0.0".to_string(),
            version: Version::parse("2.0.0").unwrap(),
            title: "Release 2".to_string(),
            body: None,
            published_at: Utc::now() - chrono::Duration::days(5),
            fetched_at: Utc::now(),
        };
        // v3 is newest
        let release_v3 = IntelliShellRelease {
            tag: "v3.0.0".to_string(),
            version: Version::parse("3.0.0").unwrap(),
            title: "Release 3".to_string(),
            body: Some("Body 3".to_string()),
            published_at: Utc::now(),
            fetched_at: Utc::now(),
        };

        storage
            .upsert_releases(vec![release_v1.clone(), release_v2.clone(), release_v3.clone()])
            .await
            .unwrap();

        // 3. Get Latest
        let latest = storage.get_latest_stored_version().await.unwrap();
        assert_eq!(latest.unwrap().0, Version::parse("3.0.0").unwrap());

        // 4. Get Releases (All, should be sorted DESC)
        let all = storage.get_releases().await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].version, release_v3.version);
        assert_eq!(all[1].version, release_v2.version);
        assert_eq!(all[2].version, release_v1.version);

        // 5. Prune (Keep 2) -> Should keep v3 and v2
        storage.prune_releases(2).await.unwrap();
        let remaining = storage.get_releases().await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].version, release_v3.version);
        assert_eq!(remaining[1].version, release_v2.version);
    }
}
