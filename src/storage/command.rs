use std::{cmp::Ordering, sync::atomic::Ordering as AtomicOrdering};

use color_eyre::{Report, eyre::eyre};
use rusqlite::{Row, fallible_iterator::FallibleIterator, ffi, types::Type};
use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::RusqliteBinder;
use tracing::instrument;
use uuid::Uuid;

use super::{SqliteStorage, queries::*};
use crate::{
    config::SearchCommandTuning,
    errors::{Result, UserFacingError},
    model::{Command, SOURCE_TLDR, SearchCommandsFilter},
};

impl SqliteStorage {
    /// Creates temporary tables for workspace-specific commands and completions for the current session by reflecting
    /// the schema of the main tables.
    #[instrument(skip_all)]
    pub async fn setup_workspace_storage(&self) -> Result<()> {
        tracing::trace!("Creating workspace-specific tables");
        self.client
            .conn_mut(|conn| {
                // Fetch the schema for the main tables and triggers
                let schemas: Vec<String> = conn
                    .prepare(
                        r"SELECT sql 
                        FROM sqlite_master 
                        WHERE (type = 'table' AND name = 'variable_completion') 
                            OR (type = 'table' AND name = 'command') 
                            OR (type = 'table' AND name LIKE 'command_%fts')
                            OR (type = 'trigger' AND name LIKE 'command_%_fts' AND tbl_name = 'command')",
                    )?
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;

                let tx = conn.transaction()?;

                // Modify and execute each schema statement to create temporary versions
                for schema in schemas {
                    let temp_schema = schema
                        .replace("variable_completion", "workspace_variable_completion")
                        .replace("command", "workspace_command")
                        .replace("CREATE TABLE ", "CREATE TEMP TABLE ")
                        .replace("CREATE VIRTUAL TABLE ", "CREATE VIRTUAL TABLE temp.")
                        .replace("CREATE TRIGGER ", "CREATE TEMP TRIGGER ");
                    tracing::trace!("Executing:\n{temp_schema}");
                    tx.execute(&temp_schema, [])?;
                }

                tx.commit()?;
                Ok(())
            })
            .await?;

        self.workspace_tables_loaded.store(true, AtomicOrdering::SeqCst);

        Ok(())
    }

    /// Determines if the storage is empty, i.e., if there are no commands in the database
    #[instrument(skip_all)]
    pub async fn is_empty(&self) -> Result<bool> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);
        self.client
            .conn(move |conn| {
                let query = if workspace_tables_loaded {
                    "SELECT NOT EXISTS (SELECT 1 FROM command UNION ALL SELECT 1 FROM workspace_command)"
                } else {
                    "SELECT NOT EXISTS(SELECT 1 FROM command)"
                };
                tracing::trace!("Checking if storage is empty:\n{query}");
                Ok(conn.query_row(query, [], |r| r.get(0))?)
            })
            .await
    }

    /// Retrieves all tags from the database along with their usage statistics and if it's an exact match for the prefix
    #[instrument(skip_all)]
    pub async fn find_tags(
        &self,
        filter: SearchCommandsFilter,
        tag_prefix: Option<String>,
        tuning: &SearchCommandTuning,
    ) -> Result<Vec<(String, u64, bool)>> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);
        let query = query_find_tags(filter, tag_prefix, tuning, workspace_tables_loaded)?;
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Querying tags:\n{}", query.to_string(SqliteQueryBuilder));
        }
        let (stmt, values) = query.build_rusqlite(SqliteQueryBuilder);
        self.client
            .conn(move |conn| {
                conn.prepare(&stmt)?
                    .query(&*values.as_params())?
                    .and_then(|r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .collect()
            })
            .await
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
    ) -> Result<(Vec<Command>, bool)> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);
        let cleaned_filter = filter.cleaned();

        // When there's a search term
        let mut query_alias = None;
        if let Some(ref term) = cleaned_filter.search_term {
            // Try to find a command matching the alias exactly
            if workspace_tables_loaded {
                query_alias = Some((
                    format!(
                        r#"SELECT * 
                        FROM (
                            SELECT rowid, * FROM workspace_command
                            UNION ALL
                            SELECT rowid, * FROM command
                        ) c 
                        WHERE c.alias IS NOT NULL AND c.alias = ?1 
                        LIMIT {QUERY_LIMIT}"#
                    ),
                    (term.clone(),),
                ));
            } else {
                query_alias = Some((
                    format!(
                        r#"SELECT c.rowid, c.* 
                        FROM command c 
                        WHERE c.alias IS NOT NULL AND c.alias = ?1 
                        LIMIT {QUERY_LIMIT}"#
                    ),
                    (term.clone(),),
                ));
            }
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
        self.client
            .conn(move |conn| {
                // If there's a query to find the command by alias
                if let Some((query_alias, a_params)) = query_alias {
                    tracing::trace!("Querying aliased commands:\n{query_alias}");
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
    }

    /// Removes tldr commands
    #[instrument(skip_all)]
    pub async fn delete_tldr_commands(&self, category: Option<String>) -> Result<u64> {
        self.client
            .conn_mut(move |conn| {
                let mut query = String::from("DELETE FROM command WHERE source = ?1");
                let mut params: Vec<String> = vec![SOURCE_TLDR.to_owned()];
                if let Some(cat) = category {
                    query.push_str(" AND category = ?2");
                    params.push(cat);
                }
                tracing::trace!("Deleting tldr commands:\n{query}");
                let affected = conn.execute(&query, rusqlite::params_from_iter(params))?;
                Ok(affected as u64)
            })
            .await
    }

    /// Inserts a new command into the database.
    ///
    /// If a command with the same `id` or `cmd` already exists in the database, an error will be returned.
    #[instrument(skip_all)]
    pub async fn insert_command(&self, command: Command) -> Result<Command> {
        self.client
            .conn(move |conn| {
                let query = r#"INSERT INTO command (
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
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#;
                tracing::trace!("Inserting a command:\n{query}");
                let res = conn.execute(
                    query,
                    (
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
                        &command.updated_at,
                    ),
                );
                match res {
                    Ok(_) => Ok(command),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE || code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                            Err(UserFacingError::CommandAlreadyExists.into())
                        } else {
                            Err(Report::from(err).into())
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
    pub async fn update_command(&self, command: Command) -> Result<Command> {
        self.client
            .conn(move |conn| {
                let query = r#"UPDATE command SET 
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
                    WHERE id = ?1"#;
                tracing::trace!("Updating a command:\n{query}");
                let res = conn.execute(
                    query,
                    (
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
                        &command.updated_at,
                    ),
                );
                match res {
                    Ok(0) => Err(eyre!("Command not found: {}", command.id).into()),
                    Ok(_) => Ok(command),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE {
                            Err(UserFacingError::CommandAlreadyExists.into())
                        } else {
                            Err(Report::from(err).into())
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
    ) -> Result<i32> {
        self.client
            .conn_mut(move |conn| {
                let query = r#"
                    INSERT INTO command_usage (command_id, path, usage_count)
                    VALUES (?1, ?2, 1)
                    ON CONFLICT(command_id, path) DO UPDATE SET
                        usage_count = usage_count + 1
                    RETURNING usage_count;"#;
                tracing::trace!("Incrementing command usage:\n{query}");
                Ok(conn.query_row(query, (&command_id, &path.as_ref()), |r| r.get(0))?)
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
                let query = "DELETE FROM command WHERE id = ?1";
                tracing::trace!("Deleting command:\n{query}");
                let res = conn.execute(query, (&command_id,));
                match res {
                    Ok(0) => Err(eyre!("Command not found: {command_id}").into()),
                    Ok(_) => Ok(()),
                    Err(err) => Err(Report::from(err).into()),
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
    // Minimums are set to zero to avoid penalizing when all results are high-scoring
    let mut min_text = 0f64;
    let mut min_path = 0f64;
    let mut min_usage = 0f64;
    let mut max_text = f64::NEG_INFINITY;
    let mut max_path = f64::NEG_INFINITY;
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
    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;
    use tokio_stream::iter;
    use uuid::Uuid;

    use super::*;
    use crate::{
        errors::AppError,
        model::{CATEGORY_USER, ImportExportItem, SOURCE_IMPORT, SOURCE_USER, SearchMode},
    };

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
    async fn test_find_tags_no_filters() -> Result<()> {
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
    async fn test_find_tags_filter_by_tags_only() -> Result<()> {
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
    async fn test_find_tags_filter_by_prefix_only() -> Result<()> {
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
    async fn test_find_tags_filter_by_tags_and_prefix() -> Result<()> {
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
        storage.setup_workspace_storage().await.unwrap();
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
        storage.setup_workspace_storage().await.unwrap();
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
        storage.setup_workspace_storage().await.unwrap();
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
            Err(AppError::UserFacing(UserFacingError::InvalidRegex))
        ));
    }

    #[tokio::test]
    async fn test_find_commands_search_mode_fuzzy() {
        let storage = setup_ranking_storage().await;
        storage.setup_workspace_storage().await.unwrap();
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
            Err(AppError::UserFacing(UserFacingError::InvalidFuzzy))
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
            ImportExportItem::Command(Command {
                id: Uuid::now_v7(),
                cmd: "cmd1".to_string(),
                ..Default::default()
            }),
            ImportExportItem::Command(Command {
                id: Uuid::now_v7(),
                cmd: "cmd2".to_string(),
                ..Default::default()
            }),
        ];
        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        storage.import_items(stream, false, true).await.unwrap();

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
        let commands_to_import = vec![ImportExportItem::Command(Command {
            id: Uuid::now_v7(),
            cmd: "git checkout -b feature/{{name:kebab}}".to_string(),
            ..Default::default()
        })];
        let stream = iter(commands_to_import.clone().into_iter().map(Ok));
        storage.import_items(stream, false, true).await.unwrap();

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
            Err(AppError::UserFacing(UserFacingError::CommandAlreadyExists)) => (),
            _ => panic!("Expected CommandAlreadyExists error on duplicate id"),
        }

        // Test duplicate cmd insert fails
        cmd.id = Uuid::now_v7();
        match storage.insert_command(cmd).await {
            Err(AppError::UserFacing(UserFacingError::CommandAlreadyExists)) => (),
            _ => panic!("Expected CommandAlreadyExists error on duplicate cmd"),
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
            Err(AppError::UserFacing(UserFacingError::CommandAlreadyExists)) => (),
            _ => panic!("Expected CommandAlreadyExists error when updating to existing cmd"),
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
                .conn_mut(|conn| {
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
                .conn_mut(move |conn| {
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
