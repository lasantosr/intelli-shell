use std::{cmp::Ordering, pin::pin};

use chrono::{DateTime, Utc};
use color_eyre::{
    Report, Result,
    eyre::{Context, eyre},
};
use futures_util::StreamExt;
use regex::Regex;
use rusqlite::{Row, fallible_iterator::FallibleIterator, ffi, types::Type};
use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::RusqliteBinder;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tracing::instrument;
use uuid::Uuid;

use super::{SqliteStorage, queries::*};
use crate::{
    config::SearchCommandTuning,
    errors::{InsertError, SearchError, UpdateError},
    model::{CATEGORY_USER, Command, SOURCE_TLDR, SearchCommandsFilter},
};

impl SqliteStorage {
    /// Creates temporary tables for workspace-specific commands for the current session by reflecting the schema of the
    /// main `command` table.
    #[instrument(skip_all)]
    pub async fn setup_workspace_storage(&self) -> Result<()> {
        self.client
            .conn_mut::<_, _, Report>(|conn| {
                // Fetch the schema for the main tables and triggers
                let schemas: Vec<String> = conn
                    .prepare(
                        r"SELECT sql 
                        FROM sqlite_master 
                        WHERE (type = 'table' AND name = 'command') 
                            OR (type = 'table' AND name LIKE 'command_%fts')
                            OR (type = 'trigger' AND name LIKE 'command_%_fts' AND tbl_name = 'command')",
                    )?
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;

                let tx = conn.transaction()?;

                // Modify and execute each schema statement to create temporary versions
                for schema in schemas {
                    let temp_schema = schema
                        .replace("command", "workspace_command")
                        .replace("CREATE TABLE", "CREATE TEMP TABLE")
                        .replace("CREATE VIRTUAL TABLE ", "CREATE VIRTUAL TABLE temp.")
                        .replace("CREATE TRIGGER", "CREATE TEMP TRIGGER");
                    tx.execute(&temp_schema, [])?;
                }

                tx.commit()?;
                Ok(())
            })
            .await
            .wrap_err("Failed to create temporary workspace storage from schema")?;

        self.workspace_tables_loaded
            .store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// Determines if the storage is empty, i.e., if there are no commands in the database
    #[instrument(skip_all)]
    pub async fn is_empty(&self) -> Result<bool> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(std::sync::atomic::Ordering::SeqCst);
        self.client
            .conn::<_, _, Report>(move |conn| {
                if workspace_tables_loaded {
                    Ok(conn.query_row(
                        "SELECT NOT EXISTS (SELECT 1 FROM command UNION ALL SELECT 1 FROM workspace_command)",
                        [],
                        |r| r.get(0),
                    )?)
                } else {
                    Ok(conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM command)", [], |r| r.get(0))?)
                }
            })
            .await
            .wrap_err("Couldn't check if storage is empty")
    }

    /// Retrieves all tags from the database along with their usage statistics and if it's an exact match for the prefix
    #[instrument(skip_all)]
    pub async fn find_tags(
        &self,
        filter: SearchCommandsFilter,
        tag_prefix: Option<String>,
        tuning: &SearchCommandTuning,
    ) -> Result<Vec<(String, u64, bool)>, SearchError> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(std::sync::atomic::Ordering::SeqCst);
        let query = query_find_tags(filter, tag_prefix, tuning, workspace_tables_loaded)?;
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Querying tags:\n{}", query.to_string(SqliteQueryBuilder));
        }
        let (stmt, values) = query.build_rusqlite(SqliteQueryBuilder);
        Ok(self
            .client
            .conn::<_, _, Report>(move |conn| {
                conn.prepare(&stmt)?
                    .query(&*values.as_params())?
                    .and_then(|r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .collect()
            })
            .await
            .wrap_err("Couldn't find tags")?)
    }

    /// Finds and retrieves commands from the database.
    ///
    /// When a search term is present, if there's a command which alias exactly match the term, that'll be the only one
    /// returned.
    #[instrument(skip_all)]
    pub async fn find_commands(
        &self,
        filter: SearchCommandsFilter,
        working_path: impl Into<String>,
        tuning: &SearchCommandTuning,
    ) -> Result<(Vec<Command>, bool), SearchError> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(std::sync::atomic::Ordering::SeqCst);
        let cleaned_filter = filter.cleaned();

        // When there's a search term
        let mut query_alias = None;
        if let Some(ref term) = cleaned_filter.search_term {
            // Prepare the query for the alias as well
            query_alias = Some((
                format!(
                    r#"SELECT c.rowid, c.* 
                    FROM command c 
                    WHERE c.alias IS NOT NULL AND c.alias = ?1 
                    ORDER BY c.cmd ASC
                    LIMIT {QUERY_LIMIT}"#
                ),
                (term.clone(),),
            ));
        }

        // Build the commands query when no alias is matched
        let query = query_find_commands(cleaned_filter, working_path, tuning, workspace_tables_loaded)?;
        let query_trace = if tracing::enabled!(tracing::Level::TRACE) {
            query.to_string(SqliteQueryBuilder)
        } else {
            String::default()
        };
        let (stmt, values) = query.build_rusqlite(SqliteQueryBuilder);

        // Execute the queries
        let tuning = *tuning;
        Ok(self
            .client
            .conn::<_, _, Report>(move |conn| {
                // If there's a query to find the command by alias
                if let Some((query_alias, a_params)) = query_alias {
                    // Run the query
                    let rows = conn
                        .prepare(&query_alias)?
                        .query(a_params)?
                        .map(|r| Command::try_from(r))
                        .collect::<Vec<_>>()?;
                    // Return the rows if there's a match
                    if !rows.is_empty() {
                        return Ok((rows, true));
                    }
                }
                // Otherwise, run the regular search query and re-rank results
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("Querying commands:\n{query_trace}");
                }
                Ok((
                    rerank_query_results(
                        conn.prepare(&stmt)?
                            .query(&*values.as_params())?
                            .and_then(|r| QueryResultItem::try_from(r))
                            .collect::<Result<Vec<_>, _>>()?,
                        &tuning,
                    ),
                    false,
                ))
            })
            .await
            .wrap_err("Couldn't search commands")?)
    }

    /// Imports a collection of commands into the database.
    ///
    /// This function allows for bulk insertion or updating of commands from a stream.
    /// The behavior for existing commands depends on the `overwrite` flag.
    ///
    /// Returns the number of new commands inserted and skipped/updated.
    #[instrument(skip_all)]
    pub async fn import_commands(
        &self,
        commands: impl Stream<Item = Result<Command>> + Send + 'static,
        filter: Option<Regex>,
        overwrite: bool,
        workspace: bool,
    ) -> Result<(u64, u64)> {
        // Create a channel to bridge the async stream with the sync database operations
        let (tx, mut rx) = mpsc::channel(100);

        // Spawn a producer task to read from the async stream and send to the channel
        tokio::spawn(async move {
            // Pin the stream to be able to iterate over it
            let mut commands = pin!(commands);
            while let Some(command_result) = commands.next().await {
                if tx.send(command_result).await.is_err() {
                    // Receiver has been dropped, so we can stop
                    tracing::debug!("Import stream channel closed by receiver");
                    break;
                }
            }
        });

        // Determine which table to import into based on the `workspace` flag
        let table = if workspace { "workspace_command" } else { "command" };

        self.client
            .conn_mut::<_, _, Report>(move |conn| {
                let mut inserted = 0;
                let mut skipped_or_updated = 0;
                let tx = conn.transaction()?;
                let mut stmt = if overwrite {
                    tx.prepare(&format!(
                        r#"INSERT INTO {table} (
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
                        r#"INSERT OR IGNORE INTO {table} (
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

                // Process commands from the channel
                while let Some(command_result) = rx.blocking_recv() {
                    let command = command_result?;

                    // If there's a filter for imported commands
                    if let Some(ref filter) = filter {
                        // Skip the command when it doesn't pass the filter
                        let matches_filter = filter.is_match(&command.cmd)
                            || command.description.as_ref().is_some_and(|d| filter.is_match(d));
                        if !matches_filter {
                            continue;
                        }
                    }

                    let mut rows = stmt.query((
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
                        None => skipped_or_updated += 1,
                        // When a row is returned (can happen on both paths)
                        Some(r) => {
                            let updated_at = r.get::<_, Option<DateTime<Utc>>>(0)?;
                            match updated_at {
                                // If there's no update date, it's a new insert
                                None => inserted += 1,
                                // If it has a value, it was updated
                                Some(_) => skipped_or_updated += 1,
                            }
                        }
                    }
                }

                drop(stmt);
                tx.commit()?;
                Ok((inserted, skipped_or_updated))
            })
            .await
            .wrap_err("Couldn't import commands")
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
            let res: Result<(), Report> = client
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

                    // Create an iterator over the rows
                    let mut stmt = conn.prepare(&query)?;
                    let records_iter =
                        stmt.query_and_then(rusqlite::params_from_iter(q_values), |r| Command::try_from(r))?;

                    // Iterate and send each record back through the channel
                    for record_result in records_iter {
                        if tx
                            .blocking_send(record_result.wrap_err("Error fetching command"))
                            .is_err()
                        {
                            tracing::debug!("Async stream receiver dropped, closing db query");
                            break;
                        }
                    }

                    Ok(())
                })
                .await;
            if let Err(e) = res {
                panic!("Couldn't fetch commands to export: {e:?}");
            }
        });

        // Return the receiver stream
        ReceiverStream::new(rx)
    }

    /// Removes tldr commands
    #[instrument(skip_all)]
    pub async fn delete_tldr_commands(&self, category: Option<String>) -> Result<u64> {
        self.client
            .conn_mut::<_, _, Report>(move |conn| {
                let mut query = String::from("DELETE FROM command WHERE source = ?1");
                let mut params: Vec<String> = vec![SOURCE_TLDR.to_owned()];
                if let Some(cat) = category {
                    query.push_str(" AND category = ?2");
                    params.push(cat);
                }
                let affected = conn.execute(&query, rusqlite::params_from_iter(params))?;
                Ok(affected as u64)
            })
            .await
            .wrap_err("Couldn't remove tldr commands")
    }

    /// Inserts a new command into the database.
    ///
    /// If a command with the same `id` or `cmd` already exists in the database, an error will be returned.
    #[instrument(skip_all)]
    pub async fn insert_command(&self, command: Command) -> Result<Command, InsertError> {
        self.client
            .conn(move |conn| {
                let res = conn.execute(
                    r#"INSERT INTO command (
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
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                    (
                        &command.id,
                        &command.category,
                        &command.source,
                        &command.alias,
                        &command.cmd,
                        &command.flat_cmd,
                        &command.description,
                        &command.flat_description,
                        serde_json::to_value(&command.tags).wrap_err("Couldn't insert a command")?,
                        &command.created_at,
                        &command.updated_at,
                    ),
                );
                match res {
                    Ok(_) => Ok(command),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE || code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                            Err(InsertError::AlreadyExists)
                        } else {
                            Err(Report::from(err).wrap_err("Couldn't insert a command").into())
                        }
                    }
                }
            })
            .await
    }

    /// Updates an existing command in the database.
    ///
    /// If the command to be updated does not exist, an error will be returned.
    #[instrument(skip_all)]
    pub async fn update_command(&self, command: Command) -> Result<Command, UpdateError> {
        self.client
            .conn(move |conn| {
                let res = conn.execute(
                    r#"UPDATE command SET 
                        category = ?2,
                        source = ?3,
                        alias = ?4,
                        cmd = ?5,
                        flat_cmd = ?6,
                        description = ?7,
                        flat_description = ?8,
                        tags = ?9,
                        created_at = ?10,
                        updated_at = ?11
                    WHERE id = ?1"#,
                    (
                        &command.id,
                        &command.category,
                        &command.source,
                        &command.alias,
                        &command.cmd,
                        &command.flat_cmd,
                        &command.description,
                        &command.flat_description,
                        serde_json::to_value(&command.tags).wrap_err("Couldn't update a command")?,
                        &command.created_at,
                        &command.updated_at,
                    ),
                );
                match res {
                    Ok(0) => Err(eyre!("Command not found: {}", command.id)
                        .wrap_err("Couldn't update a command")
                        .into()),
                    Ok(_) => Ok(command),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE {
                            Err(UpdateError::AlreadyExists)
                        } else {
                            Err(Report::from(err).wrap_err("Couldn't update a command").into())
                        }
                    }
                }
            })
            .await
    }

    /// Increments the usage of a command
    #[instrument(skip_all)]
    pub async fn increment_command_usage(
        &self,
        command_id: Uuid,
        path: impl AsRef<str> + Send + 'static,
    ) -> Result<i32, UpdateError> {
        self.client
            .conn_mut(move |conn| {
                let res = conn.query_row(
                    r#"
                    INSERT INTO command_usage (command_id, path, usage_count)
                    VALUES (?1, ?2, 1)
                    ON CONFLICT(command_id, path) DO UPDATE SET
                        usage_count = usage_count + 1
                    RETURNING usage_count;"#,
                    (&command_id, &path.as_ref()),
                    |r| r.get(0),
                );
                match res {
                    Ok(u) => Ok(u),
                    Err(err) => Err(Report::from(err).wrap_err("Couldn't update a command usage").into()),
                }
            })
            .await
    }

    /// Deletes an existing command from the database.
    ///
    /// If the command to be deleted does not exist, an error will be returned.
    #[instrument(skip_all)]
    pub async fn delete_command(&self, command_id: Uuid) -> Result<()> {
        self.client
            .conn(move |conn| {
                let res = conn.execute("DELETE FROM command WHERE id = ?1", (&command_id,));
                match res {
                    Ok(0) => Err(eyre!("Command not found: {command_id}").wrap_err("Couldn't delete a command")),
                    Ok(_) => Ok(()),
                    Err(err) => Err(Report::from(err).wrap_err("Couldn't delete a command")),
                }
            })
            .await
    }
}

/// Re-ranks a vector of [`QueryResultItem`] based on a combined score and command type.
///
/// The ranking priority is as follows:
/// 1. Template matches (highest priority)
/// 2. Workspace-specific commands
/// 3. Other commands
///
/// Within categories 2 and 3, items are sorted based on a combined score of normalized text, path, and usage scores.
fn rerank_query_results(items: Vec<QueryResultItem>, tuning: &SearchCommandTuning) -> Vec<Command> {
    // Handle empty or single-item input
    if items.is_empty() {
        return Vec::new();
    }
    if items.len() == 1 {
        return items.into_iter().map(|item| item.command).collect();
    }

    // 1. Partition results into template matches and all others
    // Template matches have a fixed high rank and are handled separately to ensure they are always first
    let (template_matches, mut other_items): (Vec<_>, Vec<_>) = items
        .into_iter()
        .partition(|item| item.text_score >= TEMPLATE_MATCH_RANK);
    if !template_matches.is_empty() {
        tracing::trace!("Found {} template matches", template_matches.len());
    }

    // Convert template matches to Command structs
    let mut final_commands: Vec<Command> = template_matches.into_iter().map(|item| item.command).collect();

    // If there are no other items, or only one, no complex normalization is needed
    if other_items.len() <= 1 {
        final_commands.extend(other_items.into_iter().map(|item| item.command));
        return final_commands;
    }

    // Find min / max for all three scores
    let mut min_text = f64::INFINITY;
    let mut max_text = f64::NEG_INFINITY;
    let mut min_path = f64::INFINITY;
    let mut max_path = f64::NEG_INFINITY;
    let mut min_usage = f64::INFINITY;
    let mut max_usage = f64::NEG_INFINITY;
    for item in &other_items {
        min_text = min_text.min(item.text_score);
        max_text = max_text.max(item.text_score);
        min_path = min_path.min(item.path_score);
        max_path = max_path.max(item.path_score);
        min_usage = min_usage.min(item.usage_score);
        max_usage = max_usage.max(item.usage_score);
    }

    // Calculate score ranges for normalization
    let range_text = (max_text > min_text).then_some(max_text - min_text);
    let range_path = (max_path > min_path).then_some(max_path - min_path);
    let range_usage = (max_usage > min_usage).then_some(max_usage - min_usage);

    // Sort items based on the combined normalized score
    other_items.sort_by(|a, b| {
        // Primary sort key: Workspace commands first (descending order for bool)
        match b.is_workspace_command.cmp(&a.is_workspace_command) {
            Ordering::Equal => {
                // Secondary sort key: Calculated score. Only compute if primary keys are equal.
                let calculate_score = |item: &QueryResultItem| -> f64 {
                    // Normalize each score to a 0.0 ~ 1.0 range
                    // If the range is 0, the score is neutral (0.5)
                    let norm_text = range_text.map_or(0.5, |range| (item.text_score - min_text) / range);
                    let norm_path = range_path.map_or(0.5, |range| (item.path_score - min_path) / range);
                    let norm_usage = range_usage.map_or(0.5, |range| (item.usage_score - min_usage) / range);

                    // Apply points from tuning configuration
                    (norm_text * tuning.text.points as f64)
                        + (norm_path * tuning.path.points as f64)
                        + (norm_usage * tuning.usage.points as f64)
                };

                let final_score_a = calculate_score(a);
                let final_score_b = calculate_score(b);

                // Sort by final_score in descending order (higher score is better)
                final_score_b.partial_cmp(&final_score_a).unwrap_or(Ordering::Equal)
            }
            // If items are in different categories (workspace vs. other), use the primary ordering
            other => other,
        }
    });

    // Append the sorted "other" items to the high-priority template commands
    final_commands.extend(other_items.into_iter().map(|item| item.command));
    final_commands
}

impl<'a> TryFrom<&'a Row<'a>> for Command {
    type Error = rusqlite::Error;

    fn try_from(row: &'a Row<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            // rowid is skipped
            id: row.get(1)?,
            category: row.get(2)?,
            source: row.get(3)?,
            alias: row.get(4)?,
            cmd: row.get(5)?,
            flat_cmd: row.get(6)?,
            description: row.get(7)?,
            flat_description: row.get(8)?,
            tags: serde_json::from_value(row.get::<_, serde_json::Value>(9)?)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(9, Type::Text, Box::new(e)))?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}

/// Struct representing a command query result item when using FTS ranking
struct QueryResultItem {
    /// The command associated with this result item
    command: Command,
    /// Whether this command is included in the workspace commands file
    is_workspace_command: bool,
    /// Score for the command global usage
    usage_score: f64,
    /// Score for the command path usage relevance
    path_score: f64,
    /// Score for the text relevance
    text_score: f64,
}

impl<'a> TryFrom<&'a Row<'a>> for QueryResultItem {
    type Error = rusqlite::Error;

    fn try_from(row: &'a Row<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            command: Command::try_from(row)?,
            is_workspace_command: row.get(12)?,
            usage_score: row.get(13)?,
            path_score: row.get(14)?,
            text_score: row.get(15)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;
    use tokio_stream::iter;
    use uuid::Uuid;

    use super::*;
    use crate::model::{CATEGORY_USER, SOURCE_IMPORT, SOURCE_USER, SearchMode};

    const PROJ_A_PATH: &str = "/home/user/project-a";
    const PROJ_A_API_PATH: &str = "/home/user/project-a/api";
    const PROJ_B_PATH: &str = "/home/user/project-b";
    const UNRELATED_PATH: &str = "/var/log";

    #[tokio::test]
    async fn test_setup_workspace_storage() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage.check_sqlite_version().await;
        let res = storage.setup_workspace_storage().await;
        assert!(res.is_ok(), "Expected workspace storage setup to succeed: {res:?}");
    }

    #[tokio::test]
    async fn test_is_empty() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        assert!(storage.is_empty().await.unwrap(), "Expected empty storage initially");

        let cmd = Command {
            id: Uuid::now_v7(),
            cmd: "test_cmd".to_string(),
            ..Default::default()
        };
        storage.insert_command(cmd).await.unwrap();

        assert!(!storage.is_empty().await.unwrap(), "Expected non-empty after insert");
    }

    #[tokio::test]
    async fn test_is_empty_with_workspace() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage.setup_workspace_storage().await.unwrap();
        assert!(storage.is_empty().await.unwrap(), "Expected empty storage initially");

        let cmd = Command {
            id: Uuid::now_v7(),
            cmd: "test_cmd".to_string(),
            ..Default::default()
        };
        storage.insert_command(cmd).await.unwrap();

        assert!(!storage.is_empty().await.unwrap(), "Expected non-empty after insert");
    }

    #[tokio::test]
    async fn test_find_tags_no_filters() -> Result<(), SearchError> {
        let storage = setup_ranking_storage().await;

        let result = storage
            .find_tags(SearchCommandsFilter::default(), None, &SearchCommandTuning::default())
            .await?;

        let expected = vec![
            ("#git".to_string(), 5, false),
            ("#build".to_string(), 2, false),
            ("#commit".to_string(), 2, false),
            ("#docker".to_string(), 2, false),
            ("#list".to_string(), 2, false),
            ("#k8s".to_string(), 1, false),
            ("#npm".to_string(), 1, false),
            ("#pod".to_string(), 1, false),
            ("#push".to_string(), 1, false),
            ("#unix".to_string(), 1, false),
        ];

        assert_eq!(result.len(), 10, "Expected 10 unique tags");
        assert_eq!(result, expected, "Tags list or order mismatch");

        Ok(())
    }

    #[tokio::test]
    async fn test_find_tags_filter_by_tags_only() -> Result<(), SearchError> {
        let storage = setup_ranking_storage().await;

        let filter1 = SearchCommandsFilter {
            tags: Some(vec!["#git".to_string()]),
            ..Default::default()
        };
        let result1 = storage
            .find_tags(filter1, None, &SearchCommandTuning::default())
            .await?;
        let expected1 = vec![("#commit".to_string(), 2, false), ("#push".to_string(), 1, false)];
        assert_eq!(result1.len(), 2,);
        assert_eq!(result1, expected1);

        let filter2 = SearchCommandsFilter {
            tags: Some(vec!["#docker".to_string(), "#list".to_string()]),
            ..Default::default()
        };
        let result2 = storage
            .find_tags(filter2, None, &SearchCommandTuning::default())
            .await?;
        assert!(result2.is_empty());

        let filter3 = SearchCommandsFilter {
            tags: Some(vec!["#list".to_string()]),
            ..Default::default()
        };
        let result3 = storage
            .find_tags(filter3, None, &SearchCommandTuning::default())
            .await?;
        let expected3 = vec![("#docker".to_string(), 1, false), ("#unix".to_string(), 1, false)];
        assert_eq!(result3.len(), 2);
        assert_eq!(result3, expected3);

        Ok(())
    }

    #[tokio::test]
    async fn test_find_tags_filter_by_prefix_only() -> Result<(), SearchError> {
        let storage = setup_ranking_storage().await;

        let result = storage
            .find_tags(
                SearchCommandsFilter::default(),
                Some("#comm".to_string()),
                &SearchCommandTuning::default(),
            )
            .await?;
        let expected = vec![("#commit".to_string(), 2, false)];
        assert_eq!(result.len(), 1);
        assert_eq!(result, expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_find_tags_filter_by_tags_and_prefix() -> Result<(), SearchError> {
        let storage = setup_ranking_storage().await;

        let filter1 = SearchCommandsFilter {
            tags: Some(vec!["#git".to_string()]),
            ..Default::default()
        };
        let result1 = storage
            .find_tags(filter1, Some("#comm".to_string()), &SearchCommandTuning::default())
            .await?;
        let expected1 = vec![("#commit".to_string(), 2, false)];
        assert_eq!(result1.len(), 1);
        assert_eq!(result1, expected1);

        let filter2 = SearchCommandsFilter {
            tags: Some(vec!["#git".to_string()]),
            ..Default::default()
        };
        let result2 = storage
            .find_tags(filter2, Some("#push".to_string()), &SearchCommandTuning::default())
            .await?;
        let expected2 = vec![("#push".to_string(), 1, true)];
        assert_eq!(result2.len(), 1);
        assert_eq!(result2, expected2);

        Ok(())
    }

    #[tokio::test]
    async fn test_find_commands_no_filter() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter::default();
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 10, "Expected all sample commands");
    }

    #[tokio::test]
    async fn test_find_commands_filter_by_category() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter {
            category: Some(vec!["git".to_string()]),
            ..Default::default()
        };
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().all(|c| c.category == "git"));

        let filter_no_match = SearchCommandsFilter {
            category: Some(vec!["nonexistent".to_string()]),
            ..Default::default()
        };
        let (commands_no_match, _) = storage
            .find_commands(filter_no_match, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert!(commands_no_match.is_empty());
    }

    #[tokio::test]
    async fn test_find_commands_filter_by_source() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter {
            source: Some(SOURCE_TLDR.to_string()),
            ..Default::default()
        };
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 3);
        assert!(commands.iter().all(|c| c.source == SOURCE_TLDR));
    }

    #[tokio::test]
    async fn test_find_commands_filter_by_tags() {
        let storage = setup_ranking_storage().await;
        let filter_single_tag = SearchCommandsFilter {
            tags: Some(vec!["#git".to_string()]),
            ..Default::default()
        };
        let (commands_single_tag, _) = storage
            .find_commands(filter_single_tag, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands_single_tag.len(), 5);

        let filter_multiple_tags = SearchCommandsFilter {
            tags: Some(vec!["#docker".to_string(), "#list".to_string()]),
            ..Default::default()
        };
        let (commands_multiple_tags, _) = storage
            .find_commands(filter_multiple_tags, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands_multiple_tags.len(), 1);

        let filter_empty_tags = SearchCommandsFilter {
            tags: Some(vec![]),
            ..Default::default()
        };
        let (commands_empty_tags, _) = storage
            .find_commands(filter_empty_tags, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands_empty_tags.len(), 10);
    }

    #[tokio::test]
    async fn test_find_commands_alias_precedence() {
        let storage = setup_ranking_storage().await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "gc command interfering"),
                [("/some/path", 100)],
            )
            .await;

        for mode in SearchMode::iter() {
            let filter = SearchCommandsFilter {
                search_term: Some("gc".to_string()),
                search_mode: mode,
                ..Default::default()
            };
            let (commands, alias_match) = storage
                .find_commands(filter, "", &SearchCommandTuning::default())
                .await
                .unwrap();
            assert!(alias_match, "Expected alias match for mode {mode:?}");
            assert_eq!(commands.len(), 1, "Expected only alias match for mode {mode:?}");
            assert_eq!(
                commands[0].cmd, "git commit -m",
                "Expected correct alias command for mode {mode:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_exact() {
        let storage = setup_ranking_storage().await;
        let filter_token_match = SearchCommandsFilter {
            search_term: Some("commit".to_string()),
            search_mode: SearchMode::Exact,
            ..Default::default()
        };
        let (commands_token_match, _) = storage
            .find_commands(filter_token_match, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands_token_match.len(), 2);
        assert_eq!(commands_token_match[0].cmd, "git commit -m");
        assert_eq!(commands_token_match[1].cmd, "git commit -m '{{message}}'");

        let filter_no_match = SearchCommandsFilter {
            search_term: Some("nonexistentterm".to_string()),
            search_mode: SearchMode::Exact,
            ..Default::default()
        };
        let (commands_no_match, _) = storage
            .find_commands(filter_no_match, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert!(commands_no_match.is_empty());
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_relaxed() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter {
            search_term: Some("docker list".to_string()),
            search_mode: SearchMode::Relaxed,
            ..Default::default()
        };
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().any(|c| c.cmd == "docker ps -a"));
        assert!(commands.iter().any(|c| c.cmd == "ls -lha"));
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_regex() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter {
            search_term: Some(r"git\s.*it".to_string()),
            search_mode: SearchMode::Regex,
            ..Default::default()
        };
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].cmd, "git commit -m '{{message}}'");
        assert_eq!(commands[1].cmd, "git commit -m");

        let filter_invalid = SearchCommandsFilter {
            search_term: Some("[[invalid_regex".to_string()),
            search_mode: SearchMode::Regex,
            ..Default::default()
        };
        assert!(matches!(
            storage
                .find_commands(filter_invalid, "/some/path", &SearchCommandTuning::default())
                .await,
            Err(SearchError::InvalidRegex(_))
        ));
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_fuzzy() {
        let storage = setup_ranking_storage().await;
        let filter = SearchCommandsFilter {
            search_term: Some("gtcomit".to_string()),
            search_mode: SearchMode::Fuzzy,
            ..Default::default()
        };
        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].cmd, "git commit -m '{{message}}'");
        assert_eq!(commands[1].cmd, "git commit -m");

        let filter_empty_fuzzy = SearchCommandsFilter {
            search_term: Some("'' | ^".to_string()),
            search_mode: SearchMode::Fuzzy,
            ..Default::default()
        };
        assert!(matches!(
            storage
                .find_commands(filter_empty_fuzzy, "/some/path", &SearchCommandTuning::default())
                .await,
            Err(SearchError::InvalidFuzzy)
        ));
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_auto() {
        let storage = setup_ranking_storage().await;
        let default_tuning = SearchCommandTuning::default();

        // Helper closure for running a search and making assertions
        let run_search = |term: &'static str, path: &'static str| {
            let storage = storage.clone();
            async move {
                let filter = SearchCommandsFilter {
                    search_term: Some(term.to_string()),
                    search_mode: SearchMode::Auto,
                    ..Default::default()
                };
                storage.find_commands(filter, path, &default_tuning).await.unwrap()
            }
        };

        // Scenario 1: Basic text and description search
        let (commands, _) = run_search("list containers", UNRELATED_PATH).await;
        assert!(!commands.is_empty(), "Expected results for 'list containers'");
        assert_eq!(
            commands[0].cmd, "docker ps -a",
            "Expected 'docker ps -a' to be the top result for 'list containers'"
        );

        // Scenario 2: Prefix and usage search
        let (commands, _) = run_search("git commit", PROJ_A_PATH).await;
        assert!(commands.len() >= 2, "Expected at least two results for 'git commit'");
        assert_eq!(
            commands[0].cmd, "git commit -m",
            "Expected 'git commit -m' to be the top result for 'git commit' due to usage"
        );
        assert_eq!(
            commands[1].cmd, "git commit -m '{{message}}'",
            "Expected template command to be second for 'git commit'"
        );

        // Scenario 3: Template matching
        let (commands, _) = run_search("git commit -m 'my new feature'", PROJ_A_PATH).await;
        assert!(!commands.is_empty(), "Expected results for template match");
        assert_eq!(
            commands[0].cmd, "git commit -m '{{message}}'",
            "Expected template command to be the top result for a matching search term"
        );

        // Scenario 4: Path relevance
        let (commands, _) = run_search("build", PROJ_A_API_PATH).await;
        assert!(!commands.is_empty(), "Expected results for 'build'");
        assert_eq!(
            commands[0].cmd, "npm run build:prod",
            "Expected 'npm run build:prod' to be top result for 'build' in its project path"
        );

        // Scenario 5: Fuzzy search fallback
        let (commands, _) = run_search("gt sta", PROJ_A_PATH).await;
        assert!(!commands.is_empty(), "Expected results for fuzzy search 'gt sta'");
        assert_eq!(
            commands[0].cmd, "git status",
            "Expected 'git status' as top result for fuzzy search 'gt sta'"
        );

        // Scenario 6: Specific description search with low usage
        let (commands, _) = run_search("get pod monitoring", UNRELATED_PATH).await;
        assert!(!commands.is_empty(), "Expected results for 'get pod monitoring'");
        assert_eq!(
            commands[0].cmd, "kubectl get pod -n monitoring my-specific-pod-12345",
            "Expected specific 'kubectl' command to be found"
        );

        // Scenario 7: High usage in parent path
        let (commands, _) = run_search("status", PROJ_A_API_PATH).await;
        assert!(!commands.is_empty(), "Expected results for 'status'");
        assert_eq!(
            commands[0].cmd, "git status",
            "Expected 'git status' to be top due to high usage in parent path"
        );
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_auto_hastag_only() {
        let storage = setup_ranking_storage().await;

        // This test forces fts2 and fts3 to be omited and the final query to contain fts1 only
        // If the query planner tries to inline it, it would fail because bm25 functión can't be used on that context
        let filter = SearchCommandsFilter {
            search_term: Some("#".to_string()),
            search_mode: SearchMode::Auto,
            ..Default::default()
        };

        let res = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await;
        assert!(res.is_ok(), "Expected a success response, got: {res:?}")
    }

    #[tokio::test]
    async fn test_find_commands_including_workspace() {
        let storage = setup_ranking_storage().await;

        storage.setup_workspace_storage().await.unwrap();
        let commands_to_import = vec![
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd1".to_string(),
                ..Default::default()
            },
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd2".to_string(),
                ..Default::default()
            },
        ];
        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        storage.import_commands(stream, None, false, true).await.unwrap();

        let (commands, _) = storage
            .find_commands(
                SearchCommandsFilter::default(),
                "/some/path",
                &SearchCommandTuning::default(),
            )
            .await
            .unwrap();
        assert_eq!(commands.len(), 12, "Expected 12 commands including workspace");
    }

    #[tokio::test]
    async fn test_find_commands_with_text_including_workspace() {
        let storage = setup_ranking_storage().await;

        storage.setup_workspace_storage().await.unwrap();
        let commands_to_import = vec![Command {
            id: Uuid::now_v7(),
            cmd: "git checkout -b feature/{{name:kebab}}".to_string(),
            ..Default::default()
        }];
        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        storage.import_commands(stream, None, false, true).await.unwrap();

        let filter = SearchCommandsFilter {
            search_term: Some("git".to_string()),
            ..Default::default()
        };

        let (commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(commands.len(), 6, "Expected 6 git commands including workspace");
        assert!(
            commands
                .iter()
                .any(|c| c.cmd == "git checkout -b feature/{{name:kebab}}")
        );
    }

    #[tokio::test]
    async fn test_import_commands_no_overwrite() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let commands_to_import = vec![
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd1".to_string(),
                ..Default::default()
            },
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd2".to_string(),
                ..Default::default()
            },
        ];

        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        let (inserted, skipped_or_updated) = storage.import_commands(stream, None, false, false).await.unwrap();

        assert_eq!(inserted, 2, "Expected 2 commands inserted");
        assert_eq!(skipped_or_updated, 0, "Expected 0 commands skipped or updated");

        // Import the same commands again with no overwrite
        let stream = iter(commands_to_import.into_iter().map(Ok));
        let (inserted, skipped_or_updated) = storage.import_commands(stream, None, false, false).await.unwrap();

        assert_eq!(
            inserted, 0,
            "Expected 0 commands inserted on second import (no overwrite)"
        );
        assert_eq!(
            skipped_or_updated, 2,
            "Expected 2 commands skipped on second import (no overwrite)"
        );
    }

    #[tokio::test]
    async fn test_import_commands_overwrite() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let existing_cmd = Command {
            id: Uuid::now_v7(),
            cmd: "existing_cmd".to_string(),
            description: Some("original desc".to_string()),
            alias: Some("original_alias".to_string()),
            tags: Some(vec!["tag_a".to_string()]),
            ..Default::default()
        };
        storage.insert_command(existing_cmd.clone()).await.unwrap();

        let new_cmd = Command {
            id: Uuid::now_v7(),
            cmd: "new_cmd".to_string(),
            ..Default::default()
        };

        // Import a list containing the existing command (modified) and a new command
        let commands_to_import = vec![
            Command {
                id: Uuid::now_v7(),
                cmd: "existing_cmd".to_string(),
                description: Some("updated desc".to_string()),
                alias: None,
                tags: Some(vec!["tag_b".to_string()]),
                ..Default::default()
            },
            new_cmd.clone(),
        ];

        let stream = iter(commands_to_import.into_iter().map(Ok));
        let (inserted, skipped_or_updated) = storage.import_commands(stream, None, true, false).await.unwrap();

        assert_eq!(inserted, 1, "Expected 1 new command inserted");
        assert_eq!(skipped_or_updated, 1, "Expected 1 existing command updated");

        // Verify the existing command was updated
        let filter = SearchCommandsFilter {
            search_term: Some("existing_cmd".to_string()),
            search_mode: SearchMode::Exact,
            ..Default::default()
        };
        let (found_commands, _) = storage
            .find_commands(filter, "/some/path", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(found_commands.len(), 1);
        let updated_cmd_in_db = &found_commands[0];
        assert_eq!(
            updated_cmd_in_db.description,
            Some("updated desc".to_string()),
            "Description should be updated"
        );
        assert_eq!(
            updated_cmd_in_db.alias,
            Some("original_alias".to_string()),
            "Alias should NOT be updated to NULL"
        );
        assert_eq!(
            updated_cmd_in_db.tags,
            Some(vec!["tag_b".to_string()]),
            "Tags should be updated"
        );
    }

    #[tokio::test]
    async fn test_import_commands_with_filter() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let commands_to_import = vec![
            Command {
                id: Uuid::now_v7(),
                cmd: "git commit".to_string(),
                ..Default::default()
            },
            Command {
                id: Uuid::now_v7(),
                cmd: "docker ps".to_string(),
                ..Default::default()
            },
            Command {
                id: Uuid::now_v7(),
                cmd: "git push".to_string(),
                ..Default::default()
            },
        ];

        let filter = Some(Regex::new("^git").unwrap());
        let stream = iter(commands_to_import.into_iter().map(Ok));
        let (inserted, _) = storage.import_commands(stream, filter, false, false).await.unwrap();

        assert_eq!(inserted, 2, "Expected 2 commands to be inserted");

        let (all_commands, _) = storage
            .find_commands(
                SearchCommandsFilter::default(),
                "/some/path",
                &SearchCommandTuning::default(),
            )
            .await
            .unwrap();
        assert!(all_commands.iter().all(|c| c.cmd.starts_with("git")));
        assert!(!all_commands.iter().any(|c| c.cmd.starts_with("docker")));
    }

    #[tokio::test]
    async fn test_import_workspace_commands() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage.setup_workspace_storage().await.unwrap();

        let commands_to_import = vec![
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd1".to_string(),
                ..Default::default()
            },
            Command {
                id: Uuid::now_v7(),
                cmd: "cmd2".to_string(),
                ..Default::default()
            },
        ];

        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        let (inserted, skipped_or_updated) = storage.import_commands(stream, None, false, true).await.unwrap();

        assert_eq!(inserted, 2, "Expected 2 commands inserted");
        assert_eq!(skipped_or_updated, 0, "Expected 0 commands skipped or updated");
    }

    #[tokio::test]
    async fn test_export_user_commands_no_filter() {
        let storage = setup_ranking_storage().await;
        let mut exported_commands = Vec::new();
        let mut stream = storage.export_user_commands(None).await;
        while let Some(Ok(cmd)) = stream.next().await {
            exported_commands.push(cmd);
        }

        assert_eq!(exported_commands.len(), 7, "Expected 7 user commands to be exported");
    }

    #[tokio::test]
    async fn test_export_user_commands_with_filter() {
        let storage = setup_ranking_storage().await;
        let filter = Regex::new(r"^git").unwrap(); // Commands starting with "git"
        let mut exported_commands = Vec::new();
        let mut stream = storage.export_user_commands(Some(filter)).await;
        while let Some(Ok(cmd)) = stream.next().await {
            exported_commands.push(cmd);
        }

        assert_eq!(exported_commands.len(), 3, "Expected 3 git commands to be exported");

        let exported_cmd_values: Vec<String> = exported_commands.into_iter().map(|c| c.cmd).collect();
        assert!(exported_cmd_values.contains(&"git status".to_string()));
        assert!(exported_cmd_values.contains(&"git checkout main".to_string()));
    }

    #[tokio::test]
    async fn test_delete_tldr_commands() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Insert some tldr and non-tldr commands
        let tldr_cmd1 = Command {
            id: Uuid::now_v7(),
            category: "git".to_string(),
            source: SOURCE_TLDR.to_string(),
            cmd: "git status".to_string(),
            ..Default::default()
        };
        let tldr_cmd2 = Command {
            id: Uuid::now_v7(),
            category: "docker".to_string(),
            source: SOURCE_TLDR.to_string(),
            cmd: "docker ps".to_string(),
            ..Default::default()
        };
        let user_cmd = Command {
            id: Uuid::now_v7(),
            category: "git".to_string(),
            source: SOURCE_USER.to_string(),
            cmd: "git log".to_string(),
            ..Default::default()
        };

        storage.insert_command(tldr_cmd1.clone()).await.unwrap();
        storage.insert_command(tldr_cmd2.clone()).await.unwrap();
        storage.insert_command(user_cmd.clone()).await.unwrap();

        // Delete all tldr commands
        let removed = storage.delete_tldr_commands(None).await.unwrap();
        assert_eq!(removed, 2, "Should remove both tldr commands");

        let (remaining, _) = storage
            .find_commands(SearchCommandsFilter::default(), "", &SearchCommandTuning::default())
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1, "Only user command should remain");
        assert_eq!(remaining[0].cmd, user_cmd.cmd);

        // Re-insert tldr commands for category-specific removal
        storage.insert_command(tldr_cmd1.clone()).await.unwrap();
        storage.insert_command(tldr_cmd2.clone()).await.unwrap();

        // Remove only tldr commands in 'git' category
        let removed_git = storage.delete_tldr_commands(Some("git".to_string())).await.unwrap();
        assert_eq!(removed_git, 1, "Should remove one tldr command in 'git' category");

        let (remaining, _) = storage
            .find_commands(SearchCommandsFilter::default(), "", &SearchCommandTuning::default())
            .await
            .unwrap();
        let remaining_cmds: Vec<_> = remaining.iter().map(|c| &c.cmd).collect();
        assert!(remaining_cmds.contains(&&tldr_cmd2.cmd));
        assert!(remaining_cmds.contains(&&user_cmd.cmd));
        assert!(!remaining_cmds.contains(&&tldr_cmd1.cmd));
    }

    #[tokio::test]
    async fn test_insert_command() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let mut cmd = Command {
            id: Uuid::now_v7(),
            category: "test".to_string(),
            cmd: "test_cmd".to_string(),
            description: Some("test desc".to_string()),
            tags: Some(vec!["tag1".to_string()]),
            ..Default::default()
        };

        let mut inserted = storage.insert_command(cmd.clone()).await.unwrap();
        assert_eq!(inserted.cmd, cmd.cmd);

        // Test duplicate id insert fails
        inserted.cmd = "other_cmd".to_string();
        match storage.insert_command(inserted).await {
            Err(InsertError::AlreadyExists) => (),
            _ => panic!("Expected AlreadyExists error on duplicate id"),
        }

        // Test duplicate cmd insert fails
        cmd.id = Uuid::now_v7();
        match storage.insert_command(cmd).await {
            Err(InsertError::AlreadyExists) => (),
            _ => panic!("Expected AlreadyExists error on duplicate cmd"),
        }
    }

    #[tokio::test]
    async fn test_update_command() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let cmd = Command {
            id: Uuid::now_v7(),
            cmd: "original".to_string(),
            description: Some("desc".to_string()),
            ..Default::default()
        };

        storage.insert_command(cmd.clone()).await.unwrap();

        let mut updated = cmd.clone();
        updated.cmd = "updated".to_string();
        updated.description = Some("new desc".to_string());

        let result = storage.update_command(updated.clone()).await.unwrap();
        assert_eq!(result.cmd, "updated");
        assert_eq!(result.description, Some("new desc".to_string()));

        // Test update non-existent fails
        let mut non_existent = cmd;
        non_existent.id = Uuid::now_v7();
        match storage.update_command(non_existent).await {
            Err(_) => (),
            _ => panic!("Expected error when updating non-existent command"),
        }

        // Test update to existing cmd fails
        let another_cmd = Command {
            id: Uuid::now_v7(),
            cmd: "another".to_string(),
            ..Default::default()
        };
        let mut result = storage.insert_command(another_cmd.clone()).await.unwrap();
        result.cmd = "updated".to_string();
        match storage.update_command(result).await {
            Err(UpdateError::AlreadyExists) => (),
            _ => panic!("Expected AlreadyExists error when updating to existing cmd"),
        }
    }

    #[tokio::test]
    async fn test_increment_command_usage() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Setup the command
        let command = storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "gc command interfering"),
                [("/some/path", 100)],
            )
            .await;

        // Insert
        let count = storage.increment_command_usage(command.id, "/path").await.unwrap();
        assert_eq!(count, 1);

        // Update
        let count = storage.increment_command_usage(command.id, "/some/path").await.unwrap();
        assert_eq!(count, 101);
    }

    #[tokio::test]
    async fn test_delete_command() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        let cmd = Command {
            id: Uuid::now_v7(),
            cmd: "to_delete".to_string(),
            ..Default::default()
        };

        let cmd = storage.insert_command(cmd).await.unwrap();
        let res = storage.delete_command(cmd.id).await;
        assert!(res.is_ok());

        // Test delete non-existent fails
        match storage.delete_command(cmd.id).await {
            Err(_) => (),
            _ => panic!("Expected error when deleting non-existent command"),
        }
    }

    /// Helper to setup a storage instance with a comprehensive suite of commands for testing all scenarios.
    async fn setup_ranking_storage() -> SqliteStorage {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage
            .setup_command(
                Command::new(
                    CATEGORY_USER,
                    SOURCE_USER,
                    "kubectl get pod -n monitoring my-specific-pod-12345",
                )
                .with_description(Some(
                    "Get a very specific pod by its full name in the monitoring namespace".to_string(),
                ))
                .with_tags(Some(vec!["#k8s".to_string(), "#pod".to_string()])),
                [("/other/path", 1)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "git status")
                    .with_description(Some("Check the status of the git repository".to_string()))
                    .with_tags(Some(vec!["#git".to_string()])),
                [(PROJ_A_PATH, 50), (PROJ_B_PATH, 50), (UNRELATED_PATH, 100)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "npm run build:prod")
                    .with_description(Some("Build the project for production".to_string()))
                    .with_tags(Some(vec!["#npm".to_string(), "#build".to_string()])),
                [(PROJ_A_API_PATH, 25)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "container-image-build.sh")
                    .with_description(Some("A generic script to build a container image".to_string()))
                    .with_tags(Some(vec!["#docker".to_string(), "#build".to_string()])),
                [(UNRELATED_PATH, 35)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "git commit -m '{{message}}'")
                    .with_description(Some("Commit with a message".to_string()))
                    .with_tags(Some(vec!["#git".to_string(), "#commit".to_string()])),
                [(PROJ_A_PATH, 10), (PROJ_B_PATH, 10)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_USER, "git checkout main")
                    .with_alias(Some("gco".to_string()))
                    .with_description(Some("Checkout the main branch".to_string()))
                    .with_tags(Some(vec!["#git".to_string()])),
                [(PROJ_A_PATH, 30), (PROJ_B_PATH, 30)],
            )
            .await;
        storage
            .setup_command(
                Command::new("git", SOURCE_TLDR, "git commit -m")
                    .with_alias(Some("gc".to_string()))
                    .with_description(Some("Commit changes".to_string()))
                    .with_tags(Some(vec!["#git".to_string(), "#commit".to_string()])),
                [(PROJ_A_PATH, 15)],
            )
            .await;
        storage
            .setup_command(
                Command::new("docker", SOURCE_TLDR, "docker ps -a")
                    .with_description(Some("List all containers".to_string()))
                    .with_tags(Some(vec!["#docker".to_string(), "#list".to_string()])),
                [(PROJ_A_PATH, 5), (PROJ_B_PATH, 5)],
            )
            .await;
        storage
            .setup_command(
                Command::new("git", SOURCE_TLDR, "git push")
                    .with_description(Some("Push changes".to_string()))
                    .with_tags(Some(vec!["#git".to_string(), "#push".to_string()])),
                [(PROJ_A_PATH, 20), (PROJ_B_PATH, 20)],
            )
            .await;
        storage
            .setup_command(
                Command::new(CATEGORY_USER, SOURCE_IMPORT, "ls -lha")
                    .with_description(Some("List files".to_string()))
                    .with_tags(Some(vec!["#unix".to_string(), "#list".to_string()])),
                [(PROJ_A_PATH, 100), (PROJ_B_PATH, 100), (UNRELATED_PATH, 100)],
            )
            .await;

        storage
    }

    impl SqliteStorage {
        /// A helper function to validate the SQLite version
        async fn check_sqlite_version(&self) {
            let version: String = self
                .client
                .conn_mut::<_, _, Report>(|conn| {
                    conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))
                        .map_err(Into::into)
                })
                .await
                .unwrap();
            println!("Running with SQLite version: {version}");
        }

        /// A helper function to make setting up test data cleaner.
        /// It inserts a command and then increments its usage.
        async fn setup_command(
            &self,
            command: Command,
            usage: impl IntoIterator<Item = (&str, i32)> + Send + 'static,
        ) -> Command {
            let command = self.insert_command(command).await.unwrap();
            self.client
                .conn_mut::<_, _, Report>(move |conn| {
                    for (path, usage_count) in usage {
                        conn.execute(
                            r#"
                        INSERT INTO command_usage (command_id, path, usage_count)
                        VALUES (?1, ?2, ?3)
                        ON CONFLICT(command_id, path) DO UPDATE SET
                            usage_count = excluded.usage_count"#,
                            (&command.id, path, usage_count),
                        )?;
                    }
                    Ok(command)
                })
                .await
                .unwrap()
        }
    }
}
