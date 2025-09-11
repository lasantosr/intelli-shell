use std::sync::atomic::Ordering as AtomicOrdering;

use color_eyre::{Report, eyre::eyre};
use rusqlite::{Row, ToSql, ffi};
use tracing::instrument;
use uuid::Uuid;

use super::SqliteStorage;
use crate::{
    errors::{Result, UserFacingError},
    model::VariableCompletion,
};

impl SqliteStorage {
    /// Lists all unique root commands for variable completions
    #[instrument(skip_all)]
    pub async fn list_variable_completion_root_cmds(&self) -> Result<Vec<String>> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);
        self.client
            .conn(move |conn| {
                let query = if workspace_tables_loaded {
                    r"SELECT root_cmd
                    FROM ( 
                        SELECT root_cmd FROM variable_completion
                        UNION
                        SELECT root_cmd FROM workspace_variable_completion
                    )
                    ORDER BY root_cmd"
                } else {
                    "SELECT root_cmd
                    FROM (SELECT DISTINCT root_cmd FROM variable_completion)
                    ORDER BY root_cmd"
                };
                tracing::trace!("Listing root commands completions:\n{query}");
                Ok(conn
                    .prepare(query)?
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?)
            })
            .await
    }

    /// Lists variable completions, optionally filtering by root command and variable
    ///
    /// Stored completions take precedence over workspace ones
    #[instrument(skip_all)]
    pub async fn list_variable_completions(
        &self,
        flat_root_cmd: Option<String>,
        flat_variable_names: Option<Vec<String>>,
        skip_workspace: bool,
    ) -> Result<Vec<VariableCompletion>> {
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);

        self.client
            .conn(move |conn| {
                let mut conditions = Vec::new();
                let mut params = Vec::<&dyn ToSql>::new();
                let base_query = if !skip_workspace && workspace_tables_loaded {
                    conditions.push("rn = 1".to_string());
                    r"SELECT *
                    FROM (
                        SELECT
                            id,
                            source,
                            root_cmd,
                            flat_root_cmd,
                            variable,
                            flat_variable,
                            suggestions_provider,
                            created_at,
                            updated_at,
                            ROW_NUMBER() OVER (PARTITION BY flat_root_cmd, flat_variable ORDER BY is_workspace ASC) as rn
                        FROM (
                            SELECT *, 0 AS is_workspace FROM variable_completion 
                            UNION ALL 
                            SELECT *, 1 AS is_workspace FROM workspace_variable_completion
                        )
                    )"
                } else {
                    r"SELECT
                        id,
                        source,
                        root_cmd,
                        flat_root_cmd,
                        variable,
                        flat_variable,
                        suggestions_provider,
                        created_at,
                        updated_at
                    FROM variable_completion"
                };

                // Add condition for the root command if provided
                if let Some(cmd) = &flat_root_cmd {
                    conditions.push("flat_root_cmd = ?".to_string());
                    params.push(cmd);
                }

                // Add condition for variable names if provided
                if let Some(vars) = &flat_variable_names {
                    if vars.is_empty() {
                        // An empty variable list should not match anything
                        conditions.push(String::from("1=0"));
                    } else if vars.len() == 1 {
                        // Optimize for the common case of a single variable
                        conditions.push("flat_variable = ?".to_string());
                        params.push(&vars[0]);
                    } else {
                        // Handle multiple variables using an IN clause
                        let placeholders = vec!["?"; vars.len()].join(",");
                        conditions.push(format!("flat_variable IN ({placeholders})"));
                        for var in vars {
                            params.push(var);
                        }
                    }
                }

                let query = if conditions.is_empty() {
                    format!("{base_query}\nORDER BY root_cmd, variable")
                } else {
                    format!("{base_query}\nWHERE {}\nORDER BY root_cmd, variable", conditions.join(" AND "))
                };

                tracing::trace!("Listing completions:\n{query}");

                Ok(conn
                    .prepare(&query)?
                    .query_map(&params[..], |row| VariableCompletion::try_from(row))?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// Retrieves the variable completions that must be considered for a given root command and variable, considering
    /// the order preference:
    /// 1. A completion matching the specific root command is preferred over a generic one
    /// 2. A user-defined completion is preferred over a workspace one
    pub async fn get_completions_for(
        &self,
        flat_root_cmd: impl Into<String>,
        flat_variable_names: Vec<String>,
    ) -> Result<Vec<VariableCompletion>> {
        // No variables to resolve, so we can return early
        if flat_variable_names.is_empty() {
            return Ok(Vec::new());
        }

        let flat_root_cmd = flat_root_cmd.into();
        let workspace_tables_loaded = self.workspace_tables_loaded.load(AtomicOrdering::SeqCst);

        self.client
            .conn(move |conn| {
                // We need to pass `flat_root_cmd` twice: once for the ORDER BY clause and once for the WHERE clause
                let mut params: Vec<&dyn ToSql> = vec![&flat_root_cmd, &flat_root_cmd];

                let placeholders = vec!["?"; flat_variable_names.len()].join(",");
                for var in &flat_variable_names {
                    params.push(var);
                }

                // Order elements by the original vec order
                let mut order_by_clause = "ORDER BY CASE flat_variable ".to_string();
                for (index, var_name) in flat_variable_names.iter().enumerate() {
                    order_by_clause.push_str(&format!("WHEN ? THEN {index} "));
                    params.push(var_name);
                }
                order_by_clause.push_str("END");

                // Determine the base set of completions to query from
                let sub_query = if workspace_tables_loaded {
                    r"SELECT *, 0 AS is_workspace FROM variable_completion
                      UNION ALL
                      SELECT *, 1 AS is_workspace FROM workspace_variable_completion"
                } else {
                    "SELECT *, 0 AS is_workspace FROM variable_completion"
                };

                // This query resolves the best completion for each requested variable based on a specific precedence:
                // 1. A completion matching the specific root command is preferred over a generic one
                // 2. A user-defined completion (`is_workspace=0`) is preferred over a workspace one
                let query = format!(
                    r"SELECT
                        id,
                        source,
                        root_cmd,
                        flat_root_cmd,
                        variable,
                        flat_variable,
                        suggestions_provider,
                        created_at,
                        updated_at
                    FROM (
                        SELECT
                            *,
                            ROW_NUMBER() OVER (
                                PARTITION BY flat_variable
                                ORDER BY
                                    CASE WHEN flat_root_cmd = ? THEN 0 ELSE 1 END,
                                    is_workspace
                            ) as rn
                        FROM (
                            {sub_query}
                        )
                        WHERE (flat_root_cmd = ? OR flat_root_cmd = '') 
                            AND flat_variable IN ({placeholders})
                    )
                    WHERE rn = 1
                    {order_by_clause}"
                );

                tracing::trace!("Retrieving completions for a variable:\n{query}");

                Ok(conn
                    .prepare(&query)?
                    .query_map(&params[..], |row| VariableCompletion::try_from(row))?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// Inserts a new variable completion into the database if it doesn't already exist
    #[instrument(skip_all)]
    pub async fn insert_variable_completion(&self, var: VariableCompletion) -> Result<VariableCompletion> {
        self.client
            .conn_mut(move |conn| {
                let query = r#"INSERT INTO variable_completion (
                        id,
                        source,
                        root_cmd,
                        flat_root_cmd,
                        variable,
                        flat_variable,
                        suggestions_provider,
                        created_at,
                        updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#;
                tracing::trace!("Inserting a completion:\n{query}");
                let res = conn.execute(
                    query,
                    (
                        &var.id,
                        &var.source,
                        &var.root_cmd,
                        &var.flat_root_cmd,
                        &var.variable,
                        &var.flat_variable,
                        &var.suggestions_provider,
                        &var.created_at,
                        &var.updated_at,
                    ),
                );
                match res {
                    Ok(_) => Ok(var),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE || code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                            Err(UserFacingError::CompletionAlreadyExists.into())
                        } else {
                            Err(Report::from(err).into())
                        }
                    }
                }
            })
            .await
    }

    /// Updates an existing variable completion
    #[instrument(skip_all)]
    pub async fn update_variable_completion(&self, var: VariableCompletion) -> Result<VariableCompletion> {
        self.client
            .conn_mut(move |conn| {
                let query = r#"
                    UPDATE variable_completion
                    SET source = ?2,
                        root_cmd = ?3,
                        flat_root_cmd = ?4,
                        variable = ?5,
                        flat_variable = ?6,
                        suggestions_provider = ?7,
                        created_at = ?8,
                        updated_at = ?9
                    WHERE id = ?1
                    "#;
                tracing::trace!("Updating a completion:\n{query}");
                let res = conn.execute(
                    query,
                    (
                        &var.id,
                        &var.source,
                        &var.root_cmd,
                        &var.flat_root_cmd,
                        &var.variable,
                        &var.flat_variable,
                        &var.suggestions_provider,
                        &var.created_at,
                        &var.updated_at,
                    ),
                );
                match res {
                    Ok(0) => Err(eyre!("Variable completion not found: {}", var.id)
                        .wrap_err("Couldn't update a variable completion")
                        .into()),
                    Ok(_) => Ok(var),
                    Err(err) => {
                        let code = err.sqlite_error().map(|e| e.extended_code).unwrap_or_default();
                        if code == ffi::SQLITE_CONSTRAINT_UNIQUE {
                            Err(UserFacingError::CompletionAlreadyExists.into())
                        } else {
                            Err(Report::from(err).into())
                        }
                    }
                }
            })
            .await
    }

    /// Deletes an existing variable completion from the database
    #[instrument(skip_all)]
    pub async fn delete_variable_completion(&self, completion_id: Uuid) -> Result<()> {
        self.client
            .conn_mut(move |conn| {
                let query = "DELETE FROM variable_completion WHERE id = ?1";
                tracing::trace!("Deleting a completion:\n{query}");
                let res = conn.execute(query, (&completion_id,));
                match res {
                    Ok(0) => Err(eyre!("Variable completion not found: {completion_id}").into()),
                    Ok(_) => Ok(()),
                    Err(err) => Err(Report::from(err).into()),
                }
            })
            .await
    }

    /// Deletes an existing variable completion from the database given its unique key
    #[instrument(skip_all)]
    pub async fn delete_variable_completion_by_key(
        &self,
        flat_root_cmd: impl Into<String>,
        flat_variable_name: impl Into<String>,
    ) -> Result<Option<VariableCompletion>> {
        let flat_root_cmd = flat_root_cmd.into();
        let flat_variable_name = flat_variable_name.into();

        self.client
            .conn_mut(move |conn| {
                let query = r"DELETE FROM variable_completion 
                    WHERE flat_root_cmd = ?1 AND flat_variable = ?2 
                    RETURNING 
                        id,
                        source,
                        root_cmd,
                        flat_root_cmd,
                        variable,
                        flat_variable,
                        suggestions_provider,
                        created_at,
                        updated_at";
                tracing::trace!("Deleting a completion:\n{query}");
                let res = conn.query_row(query, (&flat_root_cmd, &flat_variable_name), |row| {
                    VariableCompletion::try_from(row)
                });

                match res {
                    Ok(completion) => Ok(Some(completion)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(err) => Err(Report::from(err).into()),
                }
            })
            .await
    }
}

impl<'a> TryFrom<&'a Row<'a>> for VariableCompletion {
    type Error = rusqlite::Error;

    fn try_from(row: &'a Row<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.get(0)?,
            source: row.get(1)?,
            root_cmd: row.get(2)?,
            flat_root_cmd: row.get(3)?,
            variable: row.get(4)?,
            flat_variable: row.get(5)?,
            suggestions_provider: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        errors::AppError,
        model::{ImportExportItem, SOURCE_IMPORT, SOURCE_USER, VariableCompletion},
    };

    #[tokio::test]
    async fn test_list_variable_completion_root_cmds() {
        // Create an in-memory database instance for testing
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Test with an empty database
        let root_cmds = storage.list_variable_completion_root_cmds().await.unwrap();
        assert!(
            root_cmds.is_empty(),
            "Should return an empty vector when the database is empty"
        );

        // Insert some test data with duplicate root commands
        let var1 = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");
        let var2 = VariableCompletion::new(SOURCE_USER, "git", "commit", "git log --oneline");
        let var3 = VariableCompletion::new(SOURCE_USER, "docker", "container", "docker ps");
        storage.insert_variable_completion(var1).await.unwrap();
        storage.insert_variable_completion(var2).await.unwrap();
        storage.insert_variable_completion(var3).await.unwrap();

        // List unique root commands from global storage
        let root_cmds = storage.list_variable_completion_root_cmds().await.unwrap();
        let expected = vec!["docker".to_string(), "git".to_string()];
        assert_eq!(root_cmds.len(), 2, "Should return only unique root commands");
        assert_eq!(
            root_cmds, expected,
            "The returned root commands should match the expected unique values"
        );

        // --- Test with workspace storage ---
        storage.setup_workspace_storage().await.unwrap();

        // Insert workspace completions (one new, one duplicate root cmd)
        let workspace_items = vec![
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "git",
                "tag",
                "git tag",
            ))),
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "npm",
                "install",
                "npm i",
            ))),
        ];
        let stream = stream::iter(workspace_items);
        storage.import_items(stream, false, true).await.unwrap();

        // List unique root commands from both global and workspace tables
        let root_cmds_with_workspace = storage.list_variable_completion_root_cmds().await.unwrap();
        let expected_with_workspace = vec!["docker".to_string(), "git".to_string(), "npm".to_string()];
        assert_eq!(
            root_cmds_with_workspace, expected_with_workspace,
            "Should include unique root cmds from workspace"
        );
    }

    #[tokio::test]
    async fn test_list_variable_completions() {
        // Create an in-memory database and insert test data
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let var1 = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");
        let var2 = VariableCompletion::new(SOURCE_USER, "git", "commit", "git log --oneline");
        let var3 = VariableCompletion::new(SOURCE_IMPORT, "docker", "container", "docker ps");
        storage.insert_variable_completion(var1).await.unwrap();
        storage.insert_variable_completion(var2).await.unwrap();
        storage.insert_variable_completion(var3).await.unwrap();

        // List all completions without any filters
        let all = storage.list_variable_completions(None, None, false).await.unwrap();
        assert_eq!(all.len(), 3);

        // Filter by root command only
        let git_cmds = storage
            .list_variable_completions(Some("git".into()), None, false)
            .await
            .unwrap();
        assert_eq!(git_cmds.len(), 2);

        // Filter by a single variable name only
        let branch_vars = storage
            .list_variable_completions(None, Some(vec!["branch".into()]), false)
            .await
            .unwrap();
        assert_eq!(branch_vars.len(), 1);

        // Filter by both root command and a single variable name
        let git_branch = storage
            .list_variable_completions(Some("git".into()), Some(vec!["branch".into()]), false)
            .await
            .unwrap();
        assert_eq!(git_branch.len(), 1);
        assert_eq!(git_branch[0].flat_root_cmd, "git");
        assert_eq!(git_branch[0].flat_variable, "branch");

        // Filter by root command and multiple variable names
        let git_multi_vars = storage
            .list_variable_completions(Some("git".into()), Some(vec!["commit".into(), "branch".into()]), false)
            .await
            .unwrap();
        assert_eq!(git_multi_vars.len(), 2);
        assert_eq!(git_multi_vars[0].variable, "branch");
        assert_eq!(git_multi_vars[1].variable, "commit");

        // No results should be returned for non-existent filters
        let none_cmd = storage
            .list_variable_completions(Some("nonexistent".into()), None, false)
            .await
            .unwrap();
        assert_eq!(none_cmd.len(), 0);

        let none_var = storage
            .list_variable_completions(Some("git".into()), Some(vec!["nonexistent".into()]), false)
            .await
            .unwrap();
        assert_eq!(none_var.len(), 0);
    }

    #[tokio::test]
    async fn test_list_variable_completions_with_workspace_precedence() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage.setup_workspace_storage().await.unwrap();

        // Insert a global completion
        let global_var = VariableCompletion::new(SOURCE_USER, "git", "checkout", "git branch --global");
        storage.insert_variable_completion(global_var).await.unwrap();

        // Insert a workspace completion with the same key and another completion that only exists in the workspace
        let workspace_var = VariableCompletion::new(SOURCE_IMPORT, "git", "checkout", "git branch --workspace");
        let workspace_only_var = VariableCompletion::new(SOURCE_IMPORT, "npm", "install", "npm i --workspace");
        let stream = stream::iter(vec![
            Ok(ImportExportItem::Completion(workspace_var)),
            Ok(ImportExportItem::Completion(workspace_only_var)),
        ]);
        storage.import_items(stream, false, true).await.unwrap();

        // List 'git checkout': global should take precedence
        let completions = storage
            .list_variable_completions(Some("git".into()), Some(vec!["checkout".into()]), false)
            .await
            .unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(
            completions[0].source, SOURCE_USER,
            "Global completion should take precedence"
        );
        assert_eq!(completions[0].suggestions_provider, "git branch --global");

        // List 'npm install': should get the workspace one as no global exists
        let completions_npm = storage
            .list_variable_completions(Some("npm".into()), Some(vec!["install".into()]), false)
            .await
            .unwrap();
        assert_eq!(completions_npm.len(), 1);
        assert_eq!(
            completions_npm[0].source, SOURCE_IMPORT,
            "Should get workspace completion when no global exists"
        );

        // List completions, but explicitly skip workspace
        let completions_skip_workspace = storage
            .list_variable_completions(Some("git".into()), Some(vec!["checkout".into()]), true)
            .await
            .unwrap();
        assert_eq!(completions_skip_workspace.len(), 1);
        assert_eq!(
            completions_skip_workspace[0].source, SOURCE_USER,
            "Should only find global completion when skipping workspace"
        );
    }

    #[tokio::test]
    async fn test_get_completions_for() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        storage.setup_workspace_storage().await.unwrap();

        // Insert test data to cover all precedence scenarios
        // 1. User completion with a specific root command
        // 2. Workspace completion with a specific root command
        // 3. User completion with an empty root command
        // 4. Workspace completion with an empty root command

        let user_completions = vec![
            // P1 for (docker, image)
            VariableCompletion::new(SOURCE_USER, "docker", "image", "docker images --user-specific"),
            // P3 for (docker, image)
            VariableCompletion::new(SOURCE_USER, "", "image", "generic images --user"),
            // P3 for (docker, container)
            VariableCompletion::new(SOURCE_USER, "", "container", "generic container --user"),
            // P3 for (docker, version)
            VariableCompletion::new(SOURCE_USER, "", "version", "generic version --user"),
        ];
        for completion in user_completions {
            storage.insert_variable_completion(completion).await.unwrap();
        }

        let workspace_items = vec![
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "docker",
                "image",
                "docker images --workspace-specific", // P2 for (docker, image)
            ))),
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "",
                "image",
                "generic images --workspace", // P4 for (docker, image)
            ))),
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "",
                "container",
                "generic container --workspace", // P4 for (docker, container)
            ))),
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "docker",
                "volume",
                "docker volume ls --workspace", // P2 for (docker, volume)
            ))),
            Ok(ImportExportItem::Completion(VariableCompletion::new(
                SOURCE_IMPORT,
                "",
                "network",
                "generic network --workspace", // P4 for (docker, network)
            ))),
        ];
        storage
            .import_items(stream::iter(workspace_items), false, true)
            .await
            .unwrap();

        // Get completions for docker variables
        let completions = storage
            .get_completions_for(
                "docker",
                vec![
                    "image".into(),
                    "container".into(),
                    "nonexistent".into(),
                    "volume".into(),
                    "network".into(),
                    "version".into(),
                ],
            )
            .await
            .unwrap();

        assert_eq!(
            completions.len(),
            5,
            "Should resolve one completion for each existing variable and ignore non-existent ones"
        );

        // -- Assert 'image' -> User completion with specific root_cmd (P1)
        let image = &completions[0];
        assert_eq!(image.flat_variable, "image");
        assert_eq!(image.flat_root_cmd, "docker");
        assert_eq!(image.source, SOURCE_USER);
        assert_eq!(image.suggestions_provider, "docker images --user-specific");

        // -- Assert 'container' -> User completion with empty root_cmd (P3)
        let container = &completions[1];
        assert_eq!(container.flat_variable, "container");
        assert_eq!(container.flat_root_cmd, "");
        assert_eq!(container.source, SOURCE_USER);
        assert_eq!(container.suggestions_provider, "generic container --user");

        // -- Assert 'volume' -> Workspace completion with specific root_cmd (P2)
        let volume = &completions[2];
        assert_eq!(volume.flat_variable, "volume");
        assert_eq!(volume.flat_root_cmd, "docker");
        assert_eq!(volume.source, SOURCE_IMPORT);
        assert_eq!(volume.suggestions_provider, "docker volume ls --workspace");

        // -- Assert 'network' -> Workspace completion with empty root_cmd (P4)
        let network = &completions[3];
        assert_eq!(network.flat_variable, "network");
        assert_eq!(network.flat_root_cmd, "");
        assert_eq!(network.source, SOURCE_IMPORT);
        assert_eq!(network.suggestions_provider, "generic network --workspace");

        // -- Assert 'version' -> User completion with empty root_cmd (P3)
        let version = &completions[4];
        assert_eq!(version.flat_variable, "version");
        assert_eq!(version.flat_root_cmd, "");
        assert_eq!(version.source, SOURCE_USER);
        assert_eq!(version.suggestions_provider, "generic version --user");
    }

    #[tokio::test]
    async fn test_insert_variable_completion() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let var = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");

        let inserted_var = storage.insert_variable_completion(var.clone()).await.unwrap();
        assert_eq!(inserted_var.flat_root_cmd, var.flat_root_cmd);

        // Try inserting the same value again
        match storage.insert_variable_completion(var).await {
            Err(AppError::UserFacing(UserFacingError::CompletionAlreadyExists)) => {}
            res => panic!("Expected CompletionAlreadyExists error, got {res:?}"),
        }
    }

    #[tokio::test]
    async fn test_update_variable_completion() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let var = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");
        let mut inserted_var = storage.insert_variable_completion(var).await.unwrap();

        inserted_var.suggestions_provider = "git branch --all".to_string();
        storage.update_variable_completion(inserted_var).await.unwrap();

        let mut found = storage
            .list_variable_completions(Some("git".into()), Some(vec!["branch".into()]), false)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        let found = found.pop().unwrap();
        assert_eq!(found.suggestions_provider, "git branch --all");
    }

    #[tokio::test]
    async fn test_delete_variable_completion() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let var = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");
        let inserted_var = storage.insert_variable_completion(var).await.unwrap();

        storage.delete_variable_completion(inserted_var.id).await.unwrap();

        let found = storage
            .list_variable_completions(Some("git".into()), Some(vec!["branch".into()]), false)
            .await
            .unwrap();
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_delete_variable_completion_by_key() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let var = VariableCompletion::new(SOURCE_USER, "git", "branch", "git branch");
        storage.insert_variable_completion(var.clone()).await.unwrap();

        // Delete by key and assert that the deleted completion is returned
        let deleted = storage
            .delete_variable_completion_by_key("git", "branch")
            .await
            .unwrap();
        assert_eq!(deleted, Some(var));

        // Should not find it anymore
        let found = storage
            .list_variable_completions(Some("git".into()), Some(vec!["branch".into()]), false)
            .await
            .unwrap();
        assert!(found.is_empty());

        // Try deleting again, should return None as the completion is already gone
        let deleted_again = storage
            .delete_variable_completion_by_key("git", "branch")
            .await
            .unwrap();
        assert_eq!(deleted_again, None);
    }
}
