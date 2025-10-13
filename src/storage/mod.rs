use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use client::{SqliteClient, SqliteClientBuilder};
use color_eyre::eyre::Context;
use itertools::Itertools;
use migrations::MIGRATIONS;
use regex::Regex;
use rusqlite::{OpenFlags, functions::FunctionFlags};

use crate::{
    errors::Result,
    utils::{COMMAND_VARIABLE_REGEX_QUOTES, SplitCaptures, SplitItem},
};

mod client;
mod migrations;
mod queries;

mod command;
mod completion;
mod import_export;
mod variable;
mod version;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// `SqliteStorage` provides an interface for interacting with a SQLite database to store and retrieve application data,
/// primarily [`Command`] and [`VariableValue`] entities
#[derive(Clone)]
pub struct SqliteStorage {
    /// Whether the workspace-level temp tables are created
    workspace_tables_loaded: Arc<AtomicBool>,
    /// The SQLite client used for database operations
    client: Arc<SqliteClient>,
}

impl SqliteStorage {
    /// Creates a new instance of [`SqliteStorage`] using a persistent database file.
    ///
    /// If INTELLI_STORAGE environment variable is set, it will use the specified path for the database file.
    pub async fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
        let builder = if let Some(path) = std::env::var_os("INTELLI_STORAGE") {
            // If INTELLI_STORAGE is set, use it as the database path
            tracing::info!("Using INTELLI_STORAGE path: {}", path.to_string_lossy());
            SqliteClientBuilder::new().path(path)
        } else {
            // Otherwise, use the provided data directory
            let db_path = data_dir.as_ref().join("storage.db3");
            tracing::info!("Using default storage path: {}", db_path.display());
            SqliteClientBuilder::new().path(db_path)
        };
        Ok(Self {
            workspace_tables_loaded: Arc::new(AtomicBool::new(false)),
            client: Arc::new(Self::open_client(builder).await?),
        })
    }

    /// Creates a new in-memory instance of [`SqliteStorage`].
    ///
    /// This is primarily intended for testing purposes, where a persistent database is not required.
    #[cfg(test)]
    pub async fn new_in_memory() -> Result<Self> {
        let client = Self::open_client(SqliteClientBuilder::new()).await?;
        Ok(Self {
            workspace_tables_loaded: Arc::new(AtomicBool::new(false)),
            client: Arc::new(client),
        })
    }

    /// Opens and initializes an SQLite client.
    ///
    /// This internal helper function configures the client with necessary PRAGMA settings for optimal performance and
    /// data integrity (WAL mode, normal sync, foreign keys) and applies all pending database migrations.
    async fn open_client(builder: SqliteClientBuilder) -> Result<SqliteClient> {
        // Build the client
        let client = builder
            .flags(OpenFlags::default())
            .open()
            .await
            .wrap_err("Error initializing SQLite client")?;

        // Use Write-Ahead Logging (WAL) mode for better concurrency and performance.
        client
            .conn(|conn| {
                Ok(conn
                    .pragma_update(None, "journal_mode", "wal")
                    .wrap_err("Error applying journal mode pragma")?)
            })
            .await?;

        // Set synchronous mode to NORMAL. This means SQLite will still sync at critical moments, but less frequently
        // than FULL, offering a good balance between safety and performance.
        client
            .conn(|conn| {
                Ok(conn
                    .pragma_update(None, "synchronous", "normal")
                    .wrap_err("Error applying synchronous pragma")?)
            })
            .await?;

        // Enforce foreign key constraints to maintain data integrity.
        // This has a slight performance cost but is crucial for relational data.
        client
            .conn(|conn| {
                Ok(conn
                    .pragma_update(None, "foreign_keys", "on")
                    .wrap_err("Error applying foreign keys pragma")?)
            })
            .await?;

        // Store temp schema in memory
        client
            .conn(|conn| {
                Ok(conn
                    .pragma_update(None, "temp_store", "memory")
                    .wrap_err("Error applying temp store pragma")?)
            })
            .await?;

        // Apply all defined database migrations to bring the schema to the latest version.
        // This is done atomically within a transaction.
        client
            .conn_mut(|conn| Ok(MIGRATIONS.to_latest(conn).wrap_err("Error applying migrations")?))
            .await?;

        // Add a regexp function to the client
        client
            .conn(|conn| {
                Ok(conn
                    .create_scalar_function(
                        "regexp",
                        2,
                        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
                        |ctx| {
                            assert_eq!(ctx.len(), 2, "regexp() called with unexpected number of arguments");

                            let text = ctx
                                .get_raw(1)
                                .as_str_or_null()
                                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

                            let Some(text) = text else {
                                return Ok(false);
                            };

                            let cached_re: Arc<Regex> =
                                ctx.get_or_create_aux(0, |vr| Ok::<_, BoxError>(Regex::new(vr.as_str()?)?))?;

                            Ok(cached_re.is_match(text))
                        },
                    )
                    .wrap_err("Error adding regexp function")?)
            })
            .await?;

        // Add a cmd-to-regex function
        client
            .conn(|conn| {
                Ok(conn
                    .create_scalar_function(
                        "cmd_to_regex",
                        1,
                        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
                        |ctx| {
                            assert_eq!(
                                ctx.len(),
                                1,
                                "cmd_to_regex() called with unexpected number of arguments"
                            );
                            let cmd_template = ctx.get::<String>(0)?;

                            // Use the SplitCaptures iterator to process both unmatched literals and captured variables
                            let regex_body = SplitCaptures::new(&COMMAND_VARIABLE_REGEX_QUOTES, &cmd_template)
                                .filter_map(|item| match item {
                                    // For unmatched parts, trim them and escape any special regex chars
                                    SplitItem::Unmatched(s) => {
                                        let trimmed = s.trim();
                                        if trimmed.is_empty() {
                                            None
                                        } else {
                                            Some(regex::escape(trimmed))
                                        }
                                    }
                                    // For captured parts (the variables), replace them with a capture group
                                    SplitItem::Captured(caps) => {
                                        // Check which capture group matched to see if the placeholder was quoted
                                        let placeholder_regex = if caps.get(1).is_some() {
                                            // Group 1 matched '{{...}}', so expect a single-quoted argument
                                            r"('[^']*')"
                                        } else if caps.get(2).is_some() {
                                            // Group 2 matched "{{...}}", so expect a double-quoted argument
                                            r#"("[^"]*")"#
                                        } else {
                                            // Group 3 matched {{...}}, so expect a generic argument
                                            r#"('[^']*'|"[^"]*"|\S+)"#
                                        };
                                        Some(String::from(placeholder_regex))
                                    },
                                })
                                // Join them by any number of whitespaces
                                .join(r"\s+");

                            // Build the final regex
                            Ok(format!("^{regex_body}$"))
                        },
                    )
                    .wrap_err("Error adding cmd-to-regex function")?)
            })
            .await?;

        Ok(client)
    }

    #[cfg(debug_assertions)]
    pub async fn query(&self, sql: String) -> Result<String> {
        self.client
            .conn(move |conn| {
                use prettytable::{Cell, Row, Table};
                use rusqlite::types::Value;

                let mut stmt = conn.prepare(&sql)?;
                let column_names = stmt
                    .column_names()
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<String>>();
                let columns_len = column_names.len();
                let mut table = Table::new();
                table.add_row(Row::from(column_names));
                let rows = stmt.query_map([], |row| {
                    let mut cells = Vec::new();
                    for i in 0..columns_len {
                        let value: Value = row.get(i)?;
                        let cell_value = match value {
                            Value::Null => "NULL".to_string(),
                            Value::Integer(i) => i.to_string(),
                            Value::Real(f) => f.to_string(),
                            Value::Text(t) => t,
                            Value::Blob(_) => "[BLOB]".to_string(),
                        };
                        cells.push(Cell::new(&cell_value));
                    }
                    Ok(Row::from(cells))
                })?;
                for row in rows {
                    table.add_row(row?);
                }
                Ok(table.to_string())
            })
            .await
    }
}
