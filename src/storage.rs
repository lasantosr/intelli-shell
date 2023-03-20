use core::slice;
use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
};

use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use iter_flow::Iterflow;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, Row};
use rusqlite_migration::{Migrations, M};

use crate::{common::flatten_str, model::Command};

/// Database migrations
static MIGRATIONS: Lazy<Migrations> = Lazy::new(|| {
    Migrations::new(vec![
        M::up(
            r#"CREATE TABLE command (
                category TEXT NOT NULL,
                alias TEXT NULL,
                cmd TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL,
                usage INTEGER DEFAULT 0
            );"#,
        ),
        M::up(r#"CREATE VIRTUAL TABLE command_fts USING fts5(flat_cmd, flat_description);"#),
    ])
});

/// Category for user defined commands
pub const USER_CATEGORY: &str = "user";

/// Regex to match not allowed FTS characters
static ALLOWED_FTS_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"[^a-zA-Z0-9 ]"#).unwrap());

/// SQLite-based storage
pub struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
    /// Builds a new SQLite storage on the default path
    pub fn new() -> Result<Self> {
        let path = ProjectDirs::from("org", "IntelliShell", "Intelli-Shell")
            .context("Error initializing project dir")?
            .data_dir()
            .to_path_buf();

        fs::create_dir_all(&path).context("Could't create data dir")?;

        Ok(Self {
            conn: Self::initialize_connection(
                Connection::open(path.join("storage.db3")).context("Error opening SQLite connection")?,
            )
                .context("Error initializing SQLite connection")?,
        })
    }

    /// Builds a new in-memory SQLite storage for testing purposes
    pub fn new_in_memory() -> Result<Self> {
        Ok(Self {
            conn: Self::initialize_connection(Connection::open_in_memory()?)
                .context("Error initializing SQLite connection")?,
        })
    }

    /// Initializes an SQLite connection applying migrations and common pragmas
    fn initialize_connection(mut conn: Connection) -> Result<Connection> {
        // Different implementation of the atomicity properties
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("Error applying journal mode pragma")?;
        // Synchronize less often to the filesystem
        conn.pragma_update(None, "synchronous", "normal")
            .context("Error applying synchronous pragma")?;
        // Check foreign key reference, slightly worst performance
        conn.pragma_update(None, "foreign_keys", "on")
            .context("Error applying foreign keys pragma")?;

        // Update the database schema, atomically
        MIGRATIONS.to_latest(&mut conn).context("Error applying migrations")?;

        Ok(conn)
    }

    /// Inserts a command and updates its `id` with the inserted value.
    ///
    /// If the command already exist on the database, its description will be updated.
    ///
    /// Returns wether the command was inserted or not (updated)
    pub fn insert_command(&mut self, command: &mut Command) -> Result<bool> {
        Ok(self.insert_commands(slice::from_mut(command))? == 1)
    }

    /// Inserts a bunch of commands and updates its `id` with the inserted value.
    ///
    /// If any command already exist on the database, its description will be updated.
    ///
    /// Returns the number of commands inserted (the rest are updated)
    pub fn insert_commands(&mut self, commands: &mut [Command]) -> Result<u64> {
        let mut res = 0;

        let tx = self.conn.transaction()?;

        {
            let mut stmt_cmd = tx.prepare(
                r#"INSERT INTO command (category, alias, cmd, description) VALUES (?, ?, ?, ?)
                ON CONFLICT(cmd) DO UPDATE SET description=excluded.description
                RETURNING rowid"#,
            )?;
            let mut stmt_fts_check = tx.prepare("SELECT rowid FROM command_fts WHERE rowid = ?")?;
            let mut stmt_fts_update = tx.prepare("UPDATE command_fts SET flat_description = ? WHERE rowid = ?")?;
            let mut stmt_fts_insert =
                tx.prepare("INSERT INTO command_fts (rowid, flat_cmd, flat_description) VALUES (?, ?, ?)")?;

            for command in commands {
                let row_id = stmt_cmd
                    .query_row(
                        (
                            &command.category,
                            command.alias.as_deref(),
                            &command.cmd,
                            &command.description,
                        ),
                        |r| r.get(0),
                    )
                    .context("Error inserting command")?;

                command.id = row_id;

                let current_row: Option<i32> = stmt_fts_check
                    .query_row([row_id], |r| r.get(0))
                    .optional()
                    .context("Error checking fts")?;

                match current_row {
                    Some(_) => {
                        stmt_fts_update
                            .execute((flatten_str(&command.description), row_id))
                            .context("Error updating command fts")?;
                    }
                    None => {
                        res += 1;
                        stmt_fts_insert
                            .execute((row_id, flatten_str(&command.cmd), flatten_str(&command.description)))
                            .context("Error inserting command fts")?;
                    }
                }
            }
        }

        tx.commit()?;

        Ok(res)
    }

    /// Updates an existing command
    ///
    /// Returns wether the command exists and was updated or not.
    pub fn update_command(&mut self, command: &Command) -> Result<bool> {
        let tx = self.conn.transaction()?;

        let updated = tx
            .execute(
                r#"UPDATE command SET alias = ?, cmd = ?, description = ?, usage = ? WHERE rowid = ?"#,
                (
                    command.alias.as_deref(),
                    &command.cmd,
                    &command.description,
                    command.usage,
                    command.id,
                ),
            )
            .context("Error updating command")?;

        if updated == 1 {
            let updated = tx
                .execute(
                    r#"UPDATE command_fts SET flat_cmd = ?, flat_description = ? WHERE rowid = ?"#,
                    (flatten_str(&command.cmd), flatten_str(&command.description), command.id),
                )
                .context("Error updating command fts")?;
            if updated == 1 {
                tx.commit()?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Deletes an existing command
    ///
    /// Returns wether the command exists and was updated or not.
    pub fn delete_command(&mut self, command_id: i64) -> Result<bool> {
        let tx = self.conn.transaction()?;

        let deleted = tx
            .execute(r#"DELETE FROM command WHERE rowid = ?"#, [command_id])
            .context("Error deleting command")?;

        if deleted == 1 {
            let deleted = tx
                .execute(r#"DELETE FROM command_fts WHERE rowid = ?"#, [command_id])
                .context("Error deleting command fts")?;
            if deleted == 1 {
                tx.commit()?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Determines if the store is empty (no commands stored)
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Returns the number of stored commands
    pub fn len(&self) -> Result<u64> {
        let mut stmt = self.conn.prepare(r#"SELECT COUNT(*) FROM command"#)?;
        Ok(stmt.query_row([], |r| r.get(0))?)
    }

    /// Get commands matching a category
    pub fn get_commands(&self, category: impl AsRef<str>) -> Result<Vec<Command>> {
        let category = category.as_ref();

        let mut stmt = self.conn.prepare(
            r#"SELECT rowid, category, alias, cmd, description, usage 
            FROM command
            WHERE category = ?
            ORDER BY usage DESC"#,
        )?;

        let commands = stmt
            .query([category])?
            .mapped(command_from_row)
            .finish_vec()
            .context("Error querying commands")?;

        Ok(commands)
    }

    /// Finds commands matching the given search criteria
    pub fn find_commands(&self, search: impl AsRef<str>) -> Result<Vec<Command>> {
        let search = search.as_ref();
        if search.is_empty() {
            return self.get_commands(USER_CATEGORY);
        }
        let flat_search = flatten_str(search);

        let alias_cmd = self
            .conn
            .query_row(
                r#"SELECT rowid, category, alias, cmd, description, usage 
                FROM command
                WHERE alias = :flat_search OR alias = :search"#,
                &[(":flat_search", flat_search.as_str()), (":search", search)],
                command_from_row,
            )
            .optional()
            .context("Error querying command by alias")?;
        if let Some(cmd) = alias_cmd {
            return Ok(vec![cmd]);
        }

        let flat_search = ALLOWED_FTS_REGEX.replace(&flat_search, "");

        let mut stmt = self.conn.prepare(
            r#"SELECT c.rowid, c.category, c.alias, c.cmd, c.description, c.usage 
            FROM command_fts s
            JOIN command c ON s.rowid = c.rowid
            WHERE command_fts MATCH :match
            ORDER BY 
                (CASE WHEN s.flat_cmd LIKE :like THEN 2 WHEN s.flat_description LIKE :like THEN 1 ELSE 0 END) DESC, 
                usage DESC, 
                (CASE WHEN c.category = 'user' THEN 1 ELSE 0 END) DESC"#,
        )?;

        let commands = stmt
            .query(&[
                (":match", &format!("{flat_search}*")),
                (":like", &format!("{flat_search}%")),
            ])?
            .mapped(command_from_row)
            .finish_vec()
            .context("Error querying fts command")?;

        Ok(commands)
    }

    /// Exports the commands from a given category into the given file path
    ///
    /// ## Returns
    ///
    /// The number of exported commands
    pub fn export(&self, category: impl AsRef<str>, file_path: impl Into<String>) -> Result<usize> {
        let category = category.as_ref();
        let file_path = file_path.into();
        let commands = self.get_commands(category)?;
        let size = commands.len();
        let file = fs::File::create(&file_path).context("Error creating output file")?;
        let mut w = BufWriter::new(file);
        for command in commands {
            writeln!(w, "{} ## {}", command.cmd, command.description).context("Error writing file")?;
        }
        w.flush().context("Error writing file")?;
        Ok(size)
    }

    /// Imports commands from the given file into a category.
    ///
    /// ## Returns
    ///
    /// The number of newly inserted commands
    pub fn import(&mut self, category: impl AsRef<str>, file_path: String) -> Result<u64> {
        let category = category.as_ref();
        let file = fs::File::open(file_path).context("Error opening file")?;
        let r = BufReader::new(file);
        let mut commands = r
            .lines()
            .map_err(anyhow::Error::from)
            .and_then(|line| {
                let (cmd, description) = line.split_once(" ## ").ok_or_else(|| anyhow!("Unexpected format"))?;
                Ok::<_, anyhow::Error>(Command::new(category, cmd, description))
            })
            .finish_vec()?;

        let new = self.insert_commands(&mut commands)?;

        Ok(new)
    }
}

/// Maps a [Command] from a [Row]
fn command_from_row(row: &Row<'_>) -> rusqlite::Result<Command> {
    Ok(Command {
        id: row.get(0)?,
        category: row.get(1)?,
        alias: row.get(2)?,
        cmd: row.get(3)?,
        description: row.get(4)?,
        usage: row.get(5)?,
    })
}

impl Drop for SqliteStorage {
    fn drop(&mut self) {
        // Make sure pragma optimize does not take too long
        self.conn
            .pragma_update(None, "analysis_limit", "400")
            .expect("Failed analysis_limit PRAGMA");
        // Gather statistics to improve query optimization
        self.conn
            .execute_batch("PRAGMA optimize;")
            .expect("Failed optimize PRAGMA");
    }
}

#[cfg(test)]
mod tests {
    use super::MIGRATIONS;

    #[test]
    fn migrations_test() {
        assert!(MIGRATIONS.validate().is_ok());
    }
}
