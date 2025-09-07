use std::pin::pin;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use regex::Regex;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tracing::instrument;

use super::SqliteStorage;
use crate::{
    errors::{AppError, Result},
    model::{CATEGORY_USER, Command, ImportExportItem, ImportStats, VariableCompletion},
};

impl SqliteStorage {
    /// Imports a collection of commands and completions into the database.
    ///
    /// This function allows for bulk insertion or updating of items from a stream.
    /// The behavior for existing items depends on the `overwrite` flag.
    #[instrument(skip_all)]
    pub async fn import_items(
        &self,
        items: impl Stream<Item = Result<ImportExportItem>> + Send + 'static,
        overwrite: bool,
        workspace: bool,
    ) -> Result<ImportStats> {
        // Create a channel to bridge the async stream with the sync database operations
        let (tx, mut rx) = mpsc::channel(100);

        // Spawn a producer task to read from the async stream and send to the channel
        tokio::spawn(async move {
            // Pin the stream to be able to iterate over it
            let mut items = pin!(items);
            while let Some(item_res) = items.next().await {
                if tx.send(item_res).await.is_err() {
                    // Receiver has been dropped, so we can stop
                    tracing::debug!("Import stream channel closed by receiver");
                    break;
                }
            }
        });

        // Determine which tables to import into based on the `workspace` flag
        let commands_table = if workspace { "workspace_command" } else { "command" };
        let completions_table = if workspace {
            "workspace_variable_completion"
        } else {
            "variable_completion"
        };

        self.client
            .conn_mut(move |conn| {
                let mut stats = ImportStats::default();
                let tx = conn.transaction()?;

                let mut cmd_stmt = if overwrite {
                    tx.prepare(&format!(
                        r#"INSERT INTO {commands_table} (
                            id,
                            category,
                            source,
                            alias,
                            cmd,
                            flat_cmd,
                            description,
                            flat_description,
                            tags,
                            created_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                        ON CONFLICT (cmd) DO UPDATE SET
                            alias = COALESCE(excluded.alias, alias),
                            cmd = excluded.cmd,
                            flat_cmd = excluded.flat_cmd,
                            description = COALESCE(excluded.description, description),
                            flat_description = COALESCE(excluded.flat_description, flat_description),
                            tags = COALESCE(excluded.tags, tags),
                            updated_at = excluded.created_at
                        RETURNING updated_at;"#
                    ))?
                } else {
                    tx.prepare(&format!(
                        r#"INSERT OR IGNORE INTO {commands_table} (
                            id,
                            category,
                            source,
                            alias,
                            cmd,
                            flat_cmd,
                            description,
                            flat_description,
                            tags,
                            created_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                        RETURNING updated_at;"#,
                    ))?
                };

                let mut cmp_stmt = if overwrite {
                    tx.prepare(&format!(
                        r#"INSERT INTO {completions_table} (
                            id,
                            source,
                            root_cmd,
                            flat_root_cmd,
                            variable,
                            flat_variable,
                            suggestions_provider,
                            created_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                        ON CONFLICT (flat_root_cmd, flat_variable) DO UPDATE SET
                            source = excluded.source,
                            root_cmd = excluded.root_cmd,
                            flat_root_cmd = excluded.flat_root_cmd,
                            variable = excluded.variable,
                            flat_variable = excluded.flat_variable,
                            suggestions_provider = excluded.suggestions_provider,
                            updated_at = excluded.created_at
                        RETURNING updated_at;"#
                    ))?
                } else {
                    tx.prepare(&format!(
                        r#"INSERT OR IGNORE INTO {completions_table} (
                            id,
                            source,
                            root_cmd,
                            flat_root_cmd,
                            variable,
                            flat_variable,
                            suggestions_provider,
                            created_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                        RETURNING updated_at;"#,
                    ))?
                };

                // Process items from the channel
                while let Some(item_result) = rx.blocking_recv() {
                    match item_result? {
                        ImportExportItem::Command(command) => {
                            tracing::trace!("Importing a {commands_table}: {}", command.cmd);
                            let mut rows = cmd_stmt.query((
                                &command.id,
                                &command.category,
                                &command.source,
                                &command.alias,
                                &command.cmd,
                                &command.flat_cmd,
                                &command.description,
                                &command.flat_description,
                                serde_json::to_value(&command.tags)?,
                                &command.created_at,
                            ))?;
                            match rows.next()? {
                                // No row returned, this happens only when overwrite = false, meaning it was skipped
                                None => stats.commands_skipped += 1,
                                // When a row is returned (can happen on both paths)
                                Some(r) => {
                                    let updated_at = r.get::<_, Option<DateTime<Utc>>>(0)?;
                                    match updated_at {
                                        // If there's no update date, it's a new insert
                                        None => stats.commands_imported += 1,
                                        // If it has a value, it was updated
                                        Some(_) => stats.commands_updated += 1,
                                    }
                                }
                            }
                        }
                        ImportExportItem::Completion(completion) => {
                            tracing::trace!("Importing a {completions_table}: {completion}");
                            let mut rows = cmp_stmt.query((
                                &completion.id,
                                &completion.source,
                                &completion.root_cmd,
                                &completion.flat_root_cmd,
                                &completion.variable,
                                &completion.flat_variable,
                                &completion.suggestions_provider,
                                &completion.created_at,
                            ))?;
                            match rows.next()? {
                                // No row returned, this happens only when overwrite = false, meaning it was skipped
                                None => stats.completions_skipped += 1,
                                // When a row is returned (can happen on both paths)
                                Some(r) => {
                                    let updated_at = r.get::<_, Option<DateTime<Utc>>>(0)?;
                                    match updated_at {
                                        // If there's no update date, it's a new insert
                                        None => stats.completions_imported += 1,
                                        // If it has a value, it was updated
                                        Some(_) => stats.completions_updated += 1,
                                    }
                                }
                            }
                        }
                    }
                }

                drop(cmd_stmt);
                drop(cmp_stmt);
                tx.commit()?;
                Ok(stats)
            })
            .await
    }

    /// Export user commands
    #[instrument(skip_all)]
    pub async fn export_user_commands(
        &self,
        filter: Option<Regex>,
    ) -> impl Stream<Item = Result<Command>> + Send + 'static {
        // Create a channel to stream results from the database with a small buffer to provide backpressure
        let (tx, rx) = mpsc::channel(100);

        // Spawn a new task to run the query and send results back through the channel
        let client = self.client.clone();
        tokio::spawn(async move {
            let res = client
                .conn_mut(move |conn| {
                    // Prepare the query
                    let mut q_values = vec![CATEGORY_USER.to_owned()];
                    let mut query = String::from(
                        r"SELECT
                            rowid,
                            id,
                            category,
                            source,
                            alias,
                            cmd,
                            flat_cmd,
                            description,
                            flat_description,
                            tags,
                            created_at,
                            updated_at
                        FROM command
                        WHERE category = ?1",
                    );
                    if let Some(filter) = filter {
                        q_values.push(filter.as_str().to_owned());
                        query.push_str(" AND (cmd REGEXP ?2 OR (description IS NOT NULL AND description REGEXP ?2))");
                    }
                    query.push_str("\nORDER BY cmd ASC");

                    tracing::trace!("Exporting commands: {query}");

                    // Create an iterator over the rows
                    let mut stmt = conn.prepare(&query)?;
                    let records_iter =
                        stmt.query_and_then(rusqlite::params_from_iter(q_values), |r| Command::try_from(r))?;

                    // Iterate and send each record back through the channel
                    for record_result in records_iter {
                        if tx.blocking_send(record_result.map_err(AppError::from)).is_err() {
                            tracing::debug!("Async stream receiver dropped, closing db query");
                            break;
                        }
                    }

                    Ok(())
                })
                .await;
            if let Err(err) = res {
                panic!("Couldn't fetch commands to export: {err:?}");
            }
        });

        // Return the receiver stream
        ReceiverStream::new(rx)
    }

    /// Exports user variable completions for a given set of (flat_root_cmd, flat_variable_name) pairs.
    ///
    /// For each pair, it resolves the best match by first looking for a completion with the
    /// specific `flat_root_cmd`, falling back to one with an empty one if not found.
    ///
    /// **Note**: This method does not consider workspace-specific completions, only user tables.
    #[instrument(skip_all)]
    pub async fn export_user_variable_completions(
        &self,
        flat_root_cmd_and_var: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Vec<VariableCompletion>> {
        // Flatten the incoming (command, variable) key pairs
        let flat_keys = flat_root_cmd_and_var.into_iter().collect::<Vec<_>>();

        if flat_keys.is_empty() {
            return Ok(Vec::new());
        }

        self.client
            .conn(move |conn| {
                let values_placeholders = vec!["(?, ?)"; flat_keys.len()].join(", ");
                let query = format!(
                    r#"WITH input_keys(flat_root_cmd, flat_variable) AS (VALUES {values_placeholders})
                    SELECT
                        t.id,
                        t.source,
                        t.root_cmd,
                        t.flat_root_cmd,
                        t.variable,
                        t.flat_variable,
                        t.suggestions_provider,
                        t.created_at,
                        t.updated_at
                    FROM (
                        SELECT
                            vc.*,
                            ROW_NUMBER() OVER (
                                PARTITION BY ik.flat_root_cmd, ik.flat_variable
                                ORDER BY
                                    CASE WHEN vc.flat_root_cmd = ik.flat_root_cmd THEN 0 ELSE 1 END
                            ) as rn
                        FROM variable_completion vc
                        JOIN input_keys ik ON vc.flat_variable = ik.flat_variable
                        WHERE vc.flat_root_cmd = ik.flat_root_cmd 
                            OR vc.flat_root_cmd = ''
                    ) AS t
                    WHERE t.rn = 1
                    ORDER BY t.root_cmd, t.variable"#
                );
                tracing::trace!("Exporting completions: {query}");

                Ok(conn
                    .prepare(&query)?
                    .query_map(
                        rusqlite::params_from_iter(flat_keys.into_iter().flat_map(|(cmd, var)| vec![cmd, var])),
                        |row| VariableCompletion::try_from(row),
                    )?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use tokio_stream::iter;

    use super::*;
    use crate::model::{SOURCE_TLDR, SOURCE_USER};

    #[tokio::test]
    async fn test_import_items_commands() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let items_to_import = vec![
            ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "cmd1")),
            ImportExportItem::Command(
                Command::new(CATEGORY_USER, SOURCE_USER, "cmd2").with_description(Some("original desc".to_string())),
            ),
        ];

        // First import (new items)
        let stream = iter(items_to_import.clone().into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.commands_imported, 2, "Expected 2 new commands to be imported");
        assert_eq!(stats.commands_skipped, 0);

        // Second import (no overwrite)
        let stream = iter(items_to_import.clone().into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.commands_imported, 0, "Expected 0 commands to be imported");
        assert_eq!(stats.commands_skipped, 2, "Expected 2 commands to be skipped");

        // Third import (with overwrite)
        let items_to_update = vec![ImportExportItem::Command(
            Command::new(CATEGORY_USER, SOURCE_USER, "cmd2").with_description(Some("updated desc".to_string())),
        )];
        let stream = iter(items_to_update.into_iter().map(Ok));
        let stats = storage.import_items(stream, true, false).await.unwrap();
        assert_eq!(stats.commands_imported, 0, "Expected 0 new commands to be imported");
        assert_eq!(stats.commands_updated, 1, "Expected 1 command to be updated");
    }

    #[tokio::test]
    async fn test_import_items_completions() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let items_to_import = vec![
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch")),
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "docker", "container", "docker ps")),
        ];

        // First import (new items)
        let stream = iter(items_to_import.clone().into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.completions_imported, 2);
        assert_eq!(stats.completions_skipped, 0);

        // Second import (no overwrite)
        let stream = iter(items_to_import.clone().into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.completions_imported, 0);
        assert_eq!(stats.completions_skipped, 2);

        // Third import (with overwrite)
        let items_to_update = vec![ImportExportItem::Completion(VariableCompletion::new(
            SOURCE_USER,
            "git",
            "branch",
            "git branch -a",
        ))];
        let stream = iter(items_to_update.into_iter().map(Ok));
        let stats = storage.import_items(stream, true, false).await.unwrap();
        assert_eq!(stats.completions_imported, 0);
        assert_eq!(stats.completions_updated, 1);
    }

    #[tokio::test]
    async fn test_import_workspace_items() {
        let (_, stats) = setup_storage(true, true, true).await;

        assert_eq!(
            stats.commands_imported, 8,
            "Expected 8 commands inserted into workspace"
        );
        assert_eq!(
            stats.completions_imported, 3,
            "Expected 3 completions inserted into workspace"
        );
        assert_eq!(stats.commands_skipped, 0, "Expected 0 commands skipped in workspace");
        assert_eq!(
            stats.completions_skipped, 0,
            "Expected 0 completions skipped in workspace"
        );
    }

    #[tokio::test]
    async fn test_import_items_mixed_no_overwrite() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let items_to_import = vec![
            ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "cmd1")),
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch")),
            ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "cmd2")),
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "docker", "container", "docker ps")),
        ];

        // First import (new items)
        let stream = iter(items_to_import.clone().into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.commands_imported, 2);
        assert_eq!(stats.completions_imported, 2);
        assert_eq!(stats.commands_skipped, 0);
        assert_eq!(stats.completions_skipped, 0);

        // Second import (no overwrite)
        let stream = iter(items_to_import.into_iter().map(Ok));
        let stats = storage.import_items(stream, false, false).await.unwrap();
        assert_eq!(stats.commands_imported, 0);
        assert_eq!(stats.completions_imported, 0);
        assert_eq!(stats.commands_skipped, 2);
        assert_eq!(stats.completions_skipped, 2);
    }

    #[tokio::test]
    async fn test_import_items_mixed_with_overwrite() {
        let (storage, _) = setup_storage(true, true, false).await;

        let items_to_import = vec![
            // Update an existing command
            ImportExportItem::Command(
                Command::new(CATEGORY_USER, SOURCE_USER, "git status")
                    .with_description(Some("new description".to_string())),
            ),
            // Add a new command
            ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "new command")),
            // Update an existing completion
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch -a")),
            // Add a new completion
            ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "npm", "script", "npm run")),
        ];

        let stream = iter(items_to_import.into_iter().map(Ok));
        let stats = storage.import_items(stream, true, false).await.unwrap();

        assert_eq!(stats.commands_updated, 1, "Expected 1 command to be updated");
        assert_eq!(stats.commands_imported, 1, "Expected 1 new command to be imported");
        assert_eq!(stats.completions_updated, 1, "Expected 1 completion to be updated");
        assert_eq!(
            stats.completions_imported, 1,
            "Expected 1 new completion to be imported"
        );
    }

    #[tokio::test]
    async fn test_export_user_commands_no_filter() {
        let (storage, _) = setup_storage(true, false, false).await;
        let mut exported_commands = Vec::new();
        let mut stream = storage.export_user_commands(None).await;
        while let Some(Ok(cmd)) = stream.next().await {
            exported_commands.push(cmd);
        }

        assert_eq!(exported_commands.len(), 7, "Expected 7 user commands to be exported");
    }

    #[tokio::test]
    async fn test_export_user_commands_with_filter() {
        let (storage, _) = setup_storage(true, false, false).await;
        let filter = Regex::new(r"^git").unwrap();
        let mut exported_commands = Vec::new();
        let mut stream = storage.export_user_commands(Some(filter)).await;
        while let Some(Ok(cmd)) = stream.next().await {
            exported_commands.push(cmd);
        }

        assert_eq!(exported_commands.len(), 3, "Expected 3 git commands to be exported");

        let exported_cmd_values: Vec<String> = exported_commands.into_iter().map(|c| c.cmd).collect();
        assert!(exported_cmd_values.contains(&"git status".to_string()));
        assert!(exported_cmd_values.contains(&"git checkout main".to_string()));
        assert!(exported_cmd_values.contains(&"git pull".to_string()));
    }

    #[tokio::test]
    async fn test_export_user_variable_completions() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let completions_to_insert = vec![
            // A specific and a generic completion exist for "branch"
            VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch --specific"),
            VariableCompletion::new(SOURCE_USER, "", "branch", "git branch --generic"),
            // Only a generic completion exists for "commit"
            VariableCompletion::new(SOURCE_USER, "", "commit", "git log --oneline --generic"),
            // Only a specific completion exists for "container"
            VariableCompletion::new(SOURCE_USER, "docker", "container", "docker ps"),
        ];
        for c in completions_to_insert {
            storage.insert_variable_completion(c).await.unwrap();
        }

        // Define keys to export, covering all resolution cases
        let keys_to_export = vec![
            ("git".to_string(), "branch".to_string()), // Should resolve to the specific version
            ("git".to_string(), "commit".to_string()), // Should fall back to the generic version
            ("docker".to_string(), "container".to_string()), // Should resolve to its specific version
            ("docker".to_string(), "nonexistent".to_string()), // Should find nothing
        ];

        // Export completions
        let found = storage.export_user_variable_completions(keys_to_export).await.unwrap();
        assert_eq!(found.len(), 3, "Should export 3 completions based on precedence rules");

        // Assert 'commit' fell back to the generic completion
        let commit = &found[0];
        assert_eq!(
            commit.flat_root_cmd, "",
            "Should have fallen back to the empty root cmd for commit"
        );
        assert_eq!(commit.flat_variable, "commit");
        assert_eq!(commit.suggestions_provider, "git log --oneline --generic");

        // Assert 'container' was resolved to its specific completion
        let container = &found[1];
        assert_eq!(container.flat_root_cmd, "docker");
        assert_eq!(container.flat_variable, "container");
        assert_eq!(container.suggestions_provider, "docker ps");

        // Assert 'branch' was resolved to the specific completion
        let branch = &found[2];
        assert_eq!(
            branch.flat_root_cmd, "git",
            "Should have picked the specific root cmd for branch"
        );
        assert_eq!(branch.flat_variable, "branch");
        assert_eq!(branch.suggestions_provider, "git branch --specific");

        // Test the edge case of exporting with an empty list of keys
        let found_empty = storage.export_user_variable_completions([]).await.unwrap();
        assert!(found_empty.is_empty(), "Should return an empty vec for empty keys");
    }

    /// Helper function to set up storage with predefined test data
    async fn setup_storage(
        with_commands: bool,
        with_completions: bool,
        workspace: bool,
    ) -> (SqliteStorage, ImportStats) {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        if workspace {
            storage.setup_workspace_storage().await.unwrap();
        }

        let mut items_to_import = Vec::new();
        if with_commands {
            items_to_import.extend(vec![
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "git status")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "git checkout main")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "git pull")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "docker ps")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "docker-compose up")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "npm install")),
                ImportExportItem::Command(Command::new(CATEGORY_USER, SOURCE_USER, "cargo build")),
                // A non-user command that should not be exported by user-only functions
                ImportExportItem::Command(Command::new("common", SOURCE_TLDR, "ls -la")),
            ]);
        }
        if with_completions {
            items_to_import.extend(vec![
                ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch")),
                ImportExportItem::Completion(VariableCompletion::new(
                    SOURCE_USER,
                    "git",
                    "commit",
                    "git log --oneline",
                )),
                ImportExportItem::Completion(VariableCompletion::new(SOURCE_USER, "docker", "container", "docker ps")),
            ]);
        }

        let stats = if !items_to_import.is_empty() {
            let stream = iter(items_to_import.into_iter().map(Ok));
            storage.import_items(stream, false, workspace).await.unwrap()
        } else {
            ImportStats::default()
        };

        (storage, stats)
    }
}
