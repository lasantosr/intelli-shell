use std::collections::BTreeMap;

use color_eyre::{Report, eyre::eyre};
use rusqlite::{ErrorCode, Row, types::Value};
use tracing::instrument;

use super::SqliteStorage;
use crate::{
    config::SearchVariableTuning,
    errors::{Result, UserFacingError},
    model::VariableValue,
};

impl SqliteStorage {
    /// Finds variable values for a given root command, variable and context.
    ///
    /// The method searches for values matching any of these individual `flat_variable_names` terms, as well as the
    /// `flat_variable_name` composite variable itself.
    ///
    /// Results are returned for the original input variable, even if they don't explicitly exists, ordered to
    /// prioritize overall relevance.
    #[instrument(skip_all)]
    pub async fn find_variable_values(
        &self,
        flat_root_cmd: impl Into<String>,
        flat_variable_name: impl Into<String>,
        mut flat_variable_names: Vec<String>,
        working_path: impl Into<String>,
        context: &BTreeMap<String, String>,
        tuning: &SearchVariableTuning,
    ) -> Result<Vec<(VariableValue, f64)>> {
        // Also search for values of the composite variable name itself
        let flat_variable_name = flat_variable_name.into();
        flat_variable_names.push(flat_variable_name.clone());
        flat_variable_names.dedup();

        // Prepare the query params:
        // -- ?1~5: tuning params
        // -- ?7: flat_root_cmd
        // -- ?8: flat_name of the variable
        // -- ?9: working_path
        // -- ?10: context json
        // -- ?n: all variable flat_names placeholders
        let mut all_sql_params = Vec::with_capacity(10 + flat_variable_names.len());
        all_sql_params.push(Value::from(tuning.path.exact));
        all_sql_params.push(Value::from(tuning.path.ancestor));
        all_sql_params.push(Value::from(tuning.path.descendant));
        all_sql_params.push(Value::from(tuning.path.unrelated));
        all_sql_params.push(Value::from(tuning.path.points));
        all_sql_params.push(Value::from(tuning.context.points));
        all_sql_params.push(Value::from(flat_root_cmd.into()));
        all_sql_params.push(Value::from(flat_variable_name));
        all_sql_params.push(Value::from(working_path.into()));
        all_sql_params.push(Value::from(serde_json::to_string(context)?));
        let prev_params_len = all_sql_params.len();
        let mut in_placeholders = Vec::new();
        for (idx, flat_name) in flat_variable_names.into_iter().enumerate() {
            all_sql_params.push(Value::from(flat_name));
            in_placeholders.push(format!("?{}", idx + prev_params_len + 1));
        }
        let in_placeholders = in_placeholders.join(",");

        // Construct the SQL query
        let query = format!(
            r#"WITH
            -- Pre-calculate the total number of variables in the query context
            context_info AS (
                SELECT MAX(CAST(total AS REAL)) AS total_variables
                FROM (
                    SELECT COUNT(*) as total FROM json_each(?10)
                    UNION ALL SELECT 0
                )
            ),
            -- Calculate the individual relevance score for each unique usage record
            value_scores AS (
                SELECT
                    v.value,
                    u.usage_count,
                    CASE
                        -- Exact path match
                        WHEN u.path = ?9 THEN ?1
                        -- Ascendant path match (parent)
                        WHEN ?9 LIKE u.path || '/%' THEN ?2
                        -- Descendant path match (child)
                        WHEN u.path LIKE ?9 || '/%' THEN ?3
                        -- Other/unrelated path
                        ELSE ?4
                    END AS path_relevance,
                    (
                        SELECT
                            CASE
                                WHEN ci.total_variables > 0 THEN (CAST(COUNT(*) AS REAL) / ci.total_variables)
                                ELSE 0
                            END
                        FROM json_each(?10) AS query_ctx
                        CROSS JOIN context_info ci
                        WHERE json_extract(u.context_json, '$."' || query_ctx.key || '"') = query_ctx.value
                    ) AS context_relevance
                FROM variable_value v
                JOIN variable_value_usage u ON v.id = u.value_id
                WHERE v.flat_root_cmd = ?7 AND v.flat_variable IN ({in_placeholders})
            ),
            -- Group by values to find the best relevance score and the total usage count
            agg_values AS (
                SELECT
                    vs.value,
                    MAX(
                        (vs.path_relevance * ?5)
                        + (vs.context_relevance * ?6)
                    ) as relevance_score,
                    SUM(vs.usage_count) as total_usage
                FROM value_scores vs
                GROUP BY vs.value
            )
            -- Calculate the final score and join back to find the ID
            SELECT
                v.id,
                ?7 AS flat_root_cmd,
                ?8 AS flat_variable,
                a.value,
                (a.relevance_score + log(a.total_usage + 1)) AS final_score
            FROM agg_values a
            LEFT JOIN variable_value v ON v.flat_root_cmd = ?7 AND v.flat_variable = ?8 AND v.value = a.value
            ORDER BY final_score DESC;"#
        );

        // Execute the query
        self.client
            .conn(move |conn| {
                tracing::trace!("Querying variable values:\n{query}");
                tracing::trace!("With parameters:\n{all_sql_params:?}");
                Ok(conn
                    .prepare(&query)?
                    .query_map(rusqlite::params_from_iter(all_sql_params.iter()), |r| {
                        Ok((VariableValue::try_from(r)?, r.get(4)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// Inserts a new variable value into the database if it doesn't already exist
    #[instrument(skip_all)]
    pub async fn insert_variable_value(&self, mut value: VariableValue) -> Result<VariableValue> {
        // Check if the value already has an ID
        if value.id.is_some() {
            return Err(eyre!("ID should not be set when inserting a new value").into());
        };

        // Insert the value into the database
        self.client
            .conn_mut(move |conn| {
                let query = r#"INSERT INTO variable_value (flat_root_cmd, flat_variable, value) 
                    VALUES (?1, ?2, ?3)
                    RETURNING id"#;
                tracing::trace!("Inserting a variable value: {query}");
                let res = conn.query_row(query, (&value.flat_root_cmd, &value.flat_variable, &value.value), |r| {
                    r.get(0)
                });
                match res {
                    Ok(id) => {
                        value.id = Some(id);
                        Ok(value)
                    }
                    Err(err) => match err.sqlite_error_code() {
                        Some(ErrorCode::ConstraintViolation) => Err(UserFacingError::VariableValueAlreadyExists.into()),
                        _ => Err(Report::from(err).into()),
                    },
                }
            })
            .await
    }

    /// Updates an existing variable value
    #[instrument(skip_all)]
    pub async fn update_variable_value(&self, value: VariableValue) -> Result<VariableValue> {
        // Check if the value doesn't have an ID to update
        let Some(value_id) = value.id else {
            return Err(eyre!("ID must be set when updating a variable value").into());
        };

        // Update the value in the database
        self.client
            .conn_mut(move |conn| {
                let query = r#"
                    UPDATE variable_value 
                    SET flat_root_cmd = ?2, 
                        flat_variable = ?3, 
                        value = ?4
                    WHERE rowid = ?1
                    "#;
                tracing::trace!("Updating a variable value: {query}");
                let res = conn.execute(
                    query,
                    (&value_id, &value.flat_root_cmd, &value.flat_variable, &value.value),
                );
                match res {
                    Ok(0) => Err(eyre!("Variable value not found: {value_id}")
                        .wrap_err("Couldn't update a variable value")
                        .into()),
                    Ok(_) => Ok(value),
                    Err(err) => match err.sqlite_error_code() {
                        Some(ErrorCode::ConstraintViolation) => Err(UserFacingError::VariableValueAlreadyExists.into()),
                        _ => Err(Report::from(err).into()),
                    },
                }
            })
            .await
    }

    /// Increments the usage of a variable value
    #[instrument(skip_all)]
    pub async fn increment_variable_value_usage(
        &self,
        value_id: i32,
        path: impl AsRef<str> + Send + 'static,
        context: &BTreeMap<String, String>,
    ) -> Result<i32> {
        let context = serde_json::to_string(context)?;
        self.client
            .conn_mut(move |conn| {
                let query = r#"
                    INSERT INTO variable_value_usage (value_id, path, context_json, usage_count)
                    VALUES (?1, ?2, ?3, 1)
                    ON CONFLICT(value_id, path, context_json) DO UPDATE SET
                        usage_count = usage_count + 1
                    RETURNING usage_count;"#;
                tracing::trace!("Incrementing variable value usage: {query}");
                Ok(conn.query_row(query, (&value_id, &path.as_ref(), &context), |r| r.get(0))?)
            })
            .await
    }

    /// Deletes an existing variable value from the database.
    ///
    /// If the value to be deleted does not exist, an error will be returned.
    #[instrument(skip_all)]
    pub async fn delete_variable_value(&self, value_id: i32) -> Result<()> {
        self.client
            .conn_mut(move |conn| {
                let query = "DELETE FROM variable_value WHERE rowid = ?1";
                tracing::trace!("Deleting a variable value: {query}");
                let res = conn.execute(query, (&value_id,));
                match res {
                    Ok(0) => Err(eyre!("Variable value not found: {value_id}").into()),
                    Ok(_) => Ok(()),
                    Err(err) => Err(Report::from(err).into()),
                }
            })
            .await
    }
}

impl<'a> TryFrom<&'a Row<'a>> for VariableValue {
    type Error = rusqlite::Error;

    fn try_from(row: &'a Row<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.get(0)?,
            flat_root_cmd: row.get(1)?,
            flat_variable: row.get(2)?,
            value: row.get(3)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::errors::AppError;

    #[tokio::test]
    async fn test_find_variable_values_empty() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let values = storage
            .find_variable_values(
                "cmd",
                "variable",
                Vec::new(),
                "/some/path",
                &BTreeMap::new(),
                &SearchVariableTuning::default(),
            )
            .await
            .unwrap();
        assert!(values.is_empty());
    }

    #[tokio::test]
    async fn test_find_variable_values_path_relevance_ranking() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let root = "docker";
        let variable = "image";
        let current_path = "/home/user/project-a/api";

        // Setup values with different path relevance, but identical usage and context
        storage
            .setup_variable_value(root, variable, "unrelated-path", "/var/www", [], 1)
            .await;
        storage
            .setup_variable_value(root, variable, "child-path", "/home/user/project-a/api/db", [], 1)
            .await;
        storage
            .setup_variable_value(root, variable, "parent-path", "/home/user/project-a", [], 1)
            .await;
        storage
            .setup_variable_value(root, variable, "exact-path", current_path, [], 1)
            .await;

        let matches = storage
            .find_variable_values(
                root,
                variable,
                Vec::new(),
                current_path,
                &BTreeMap::new(),
                &SearchVariableTuning::default(),
            )
            .await
            .unwrap();

        // Assert the order based on path relevance
        assert_eq!(matches.len(), 4);
        assert_eq!(matches[0].0.value, "exact-path");
        assert_eq!(matches[1].0.value, "parent-path");
        assert_eq!(matches[2].0.value, "child-path");
        assert_eq!(matches[3].0.value, "unrelated-path");
    }

    #[tokio::test]
    async fn test_find_variable_values_context_relevance_ranking() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let root = "kubectl";
        let variable = "port";
        let current_path = "/home/user/k8s";
        let query_context = [("namespace", "prod"), ("service", "api-gateway")];

        // Setup values with different context relevance, but identical paths and usage
        storage
            .setup_variable_value(root, variable, "no-context", current_path, [], 1)
            .await;
        storage
            .setup_variable_value(
                root,
                variable,
                "partial-context",
                current_path,
                [("namespace", "prod")],
                1,
            )
            .await;
        storage
            .setup_variable_value(root, variable, "full-context", current_path, query_context, 1)
            .await;

        let matches = storage
            .find_variable_values(
                root,
                variable,
                Vec::new(),
                current_path,
                &BTreeMap::from_iter(query_context.into_iter().map(|(k, v)| (k.to_owned(), v.to_owned()))),
                &SearchVariableTuning::default(),
            )
            .await
            .unwrap();

        // Assert the order based on context relevance
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].0.value, "full-context");
        assert_eq!(matches[1].0.value, "partial-context");
        assert_eq!(matches[2].0.value, "no-context");
    }

    #[tokio::test]
    async fn test_find_variable_values_usage_count_is_tiebreaker_only() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let root = "git";
        let variable = "branch";
        let current_path = "/home/user/project";

        // Setup two values with identical path/context, but different usage
        storage
            .setup_variable_value(root, variable, "feature-a", current_path, [], 5)
            .await;
        storage
            .setup_variable_value(root, variable, "feature-b", current_path, [], 50)
            .await;
        // A third value with worse path relevance but massive usage
        storage
            .setup_variable_value(root, variable, "release-1.0", "/other/path", [], 9999)
            .await;

        let matches = storage
            .find_variable_values(
                root,
                variable,
                Vec::new(),
                current_path,
                &BTreeMap::new(),
                &SearchVariableTuning::default(),
            )
            .await
            .unwrap();

        // Assert that usage count correctly breaks the tie, but doesn't override relevance
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].0.value, "feature-b");
        assert_eq!(matches[1].0.value, "feature-a");
        assert_eq!(matches[2].0.value, "release-1.0");
    }

    #[tokio::test]
    async fn test_find_variable_values_aggregates_from_multiple_variables() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let root = "kubectl";
        let variable_composite = "pod|service";
        let variable_composite_names = variable_composite.split("|").map(String::from).collect::<Vec<_>>();

        // Setup values for the individual variables
        storage
            .setup_variable_value(root, "pod", "api-pod-123", "/path", [], 4)
            .await;
        storage
            .setup_variable_value(root, "service", "api-service", "/path", [], 5)
            .await;
        // Setup a value that also exists for the composite variable
        let sug_composite = storage
            .setup_variable_value(root, variable_composite, "api-pod-123", "/path", [], 4)
            .await;

        let matches = storage
            .find_variable_values(
                root,
                variable_composite,
                variable_composite_names,
                "/path",
                &BTreeMap::new(),
                &SearchVariableTuning::default(),
            )
            .await
            .unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].0.value, "api-pod-123");
        assert_eq!(matches[0].0.id, sug_composite.id);
        assert_eq!(matches[1].0.value, "api-service");
        assert!(matches[1].0.id.is_none());
    }

    #[tokio::test]
    async fn test_insert_variable_value() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let sug = VariableValue::new("cmd", "variable", "value");

        let inserted_sug = storage.insert_variable_value(sug.clone()).await.unwrap();
        assert_eq!(inserted_sug.value, sug.value);

        // Try inserting the same value again
        match storage.insert_variable_value(sug.clone()).await {
            Err(AppError::UserFacing(UserFacingError::VariableValueAlreadyExists)) => (),
            res => panic!("Expected VariableValueAlreadyExists error, got {res:?}"),
        }
    }

    #[tokio::test]
    async fn test_update_variable_value() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let sug1 = VariableValue::new("cmd", "variable", "value_orig");

        // Insert initial value
        let mut var1 = storage.insert_variable_value(sug1).await.unwrap();

        // Test successful update
        var1.value = "value_updated".to_string();
        let res = storage.update_variable_value(var1.clone()).await;
        assert!(res.is_ok(), "Expected successful update, got {res:?}");
        let sug1 = res.unwrap();
        assert_eq!(sug1.value, "value_updated");

        // Test update non-existent value (wrong ID)
        let mut non_existent_sug = sug1.clone();
        non_existent_sug.id = Some(999);
        match storage.update_variable_value(non_existent_sug).await {
            Err(_) => (),
            res => panic!("Expected error, got {res:?}"),
        }

        // Test update causing constraint violation
        let var2 = VariableValue::new("cmd", "variable", "value_other");
        let mut sug2 = storage.insert_variable_value(var2).await.unwrap();
        sug2.value = "value_updated".to_string();
        match storage.update_variable_value(sug2).await {
            Err(AppError::UserFacing(UserFacingError::VariableValueAlreadyExists)) => (),
            res => panic!("Expected VariableValueAlreadyExists error for constraint violation, got {res:?}"),
        }
    }

    #[tokio::test]
    async fn test_increment_variable_value_usage() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();

        // Setup the value
        let val = storage
            .insert_variable_value(VariableValue::new("root", "variable", "value"))
            .await
            .unwrap();
        let val_id = val.id.unwrap();

        // Insert
        let count = storage
            .increment_variable_value_usage(val_id, "/path", &BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Update
        let count = storage
            .increment_variable_value_usage(val_id, "/path", &BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_delete_variable_value() {
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let sug = VariableValue::new("cmd", "variable_del", "value_to_delete");

        // Insert values
        let sug = storage.insert_variable_value(sug).await.unwrap();
        let id_to_delete = sug.id.unwrap();

        // Test successful deletion
        let res = storage.delete_variable_value(id_to_delete).await;
        assert!(res.is_ok(), "Expected successful update, got {res:?}");

        // Test deleting a non-existent value
        match storage.delete_variable_value(id_to_delete).await {
            Err(_) => (),
            res => panic!("Expected error, got {res:?}"),
        }
    }

    impl SqliteStorage {
        /// A helper function to make setting up test data cleaner.
        /// It inserts a variable value if it doesn't exist and then increments its usage.
        async fn setup_variable_value(
            &self,
            root: &'static str,
            variable: &'static str,
            value: &'static str,
            path: &'static str,
            context: impl IntoIterator<Item = (&str, &str)>,
            usage_count: i32,
        ) -> VariableValue {
            let context = serde_json::to_string(&BTreeMap::<String, String>::from_iter(
                context.into_iter().map(|(k, v)| (k.to_string(), v.to_string())),
            ))
            .unwrap();

            self.client
                .conn_mut(move |conn| {
                    let sug = conn.query_row(
                        r#"INSERT INTO variable_value (flat_root_cmd, flat_variable, value) 
                    VALUES (?1, ?2, ?3)
                    ON CONFLICT (flat_root_cmd, flat_variable, value) DO UPDATE SET
                        value = excluded.value
                    RETURNING id, flat_root_cmd, flat_variable, value"#,
                        (root, variable, value),
                        |r| VariableValue::try_from(r),
                    )?;
                    conn.execute(
                        r#"INSERT INTO variable_value_usage (value_id, path, context_json, usage_count)
                        VALUES (?1, ?2, ?3, ?4)
                        ON CONFLICT(value_id, path, context_json) DO UPDATE SET
                            usage_count = excluded.usage_count;
                        "#,
                        (&sug.id, path, &context, usage_count),
                    )?;
                    Ok(sug)
                })
                .await
                .unwrap()
        }
    }
}
