use std::{env, path::PathBuf};

use tokio::fs::File;
use tracing::instrument;
use uuid::Uuid;

use super::IntelliShellService;
use crate::{
    errors::{Result, UserFacingError},
    model::{CATEGORY_USER, CATEGORY_WORKSPACE, Command, SOURCE_WORKSPACE, SearchCommandsFilter, SearchMode},
    service::import_export::parse_commands,
    utils::{extract_tags_and_cleaned_text, extract_tags_with_editing_and_cleaned_text, get_working_dir},
};

/// A Tag consist on the text, the amount of times it has been used and whether it was an exact match from the query
type Tag = (String, u64, bool);

impl IntelliShellService {
    /// Loads workspace commands from the `.intellishell` file in the current working directory setting up the temporary
    /// tables in the database if they don't exist.
    ///
    /// Returns the number of commands loaded.
    #[instrument(skip_all)]
    pub async fn load_workspace_commands(&self) -> Result<u64> {
        if env::var("INTELLI_SKIP_WORKSPACE")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true)
            && let Some((workspace_commands, folder_name)) = find_workspace_commands_file()
        {
            tracing::debug!("Found workspace commands at {}", workspace_commands.display());

            // Set up the temporary tables in the database
            self.storage.setup_workspace_storage().await?;

            // Parse the commands from the file
            let file = File::open(&workspace_commands).await?;
            let tag = format!("#{}", folder_name.as_deref().unwrap_or("workspace"));
            let commands_stream = parse_commands(file, vec![tag], CATEGORY_WORKSPACE, SOURCE_WORKSPACE);

            // Import commands into the temp tables
            let (loaded, _) = self.storage.import_commands(commands_stream, false, true).await?;

            tracing::info!(
                "Loaded {loaded} workspace commands from {}",
                workspace_commands.display()
            );

            Ok(loaded)
        } else {
            Ok(0)
        }
    }

    /// Returns whether the commands storage is empty
    #[instrument(skip_all)]
    pub async fn is_storage_empty(&self) -> Result<bool> {
        self.storage.is_empty().await
    }

    /// Bookmarks a new command
    #[instrument(skip_all)]
    pub async fn insert_command(&self, command: Command) -> Result<Command> {
        // Validate
        if command.cmd.is_empty() {
            return Err(UserFacingError::EmptyCommand.into());
        }

        // Insert it
        tracing::info!("Bookmarking command: {}", command.cmd);
        self.storage.insert_command(command).await
    }

    /// Updates an existing command
    #[instrument(skip_all)]
    pub async fn update_command(&self, command: Command) -> Result<Command> {
        // Validate
        if command.cmd.is_empty() {
            return Err(UserFacingError::EmptyCommand.into());
        }

        // Update it
        tracing::info!("Updating command '{}': {}", command.id, command.cmd);
        self.storage.update_command(command).await
    }

    /// Increases the usage of a command, returning the new usage count
    #[instrument(skip_all)]
    pub async fn increment_command_usage(&self, command_id: Uuid) -> Result<i32> {
        tracing::info!("Increasing usage for command '{command_id}'");
        self.storage
            .increment_command_usage(command_id, get_working_dir())
            .await
    }

    /// Deletes an existing command
    #[instrument(skip_all)]
    pub async fn delete_command(&self, id: Uuid) -> Result<()> {
        tracing::info!("Deleting command: {}", id);
        self.storage.delete_command(id).await
    }

    /// Searches for tags based on a query string
    #[instrument(skip_all)]
    pub async fn search_tags(
        &self,
        mode: SearchMode,
        user_only: bool,
        query: &str,
        cursor_pos: usize,
    ) -> Result<Option<Vec<Tag>>> {
        let Some((editing_tag, other_tags, cleaned_text)) =
            extract_tags_with_editing_and_cleaned_text(query, cursor_pos)
        else {
            return Ok(None);
        };

        tracing::info!(
            "Searching for tags{} [{mode:?}]: {query}",
            if user_only { " (user only)" } else { "" }
        );
        tracing::trace!("Editing: {editing_tag} Other: {other_tags:?}");

        let filter = SearchCommandsFilter {
            category: user_only.then(|| vec![CATEGORY_USER.to_string()]),
            source: None,
            tags: Some(other_tags),
            search_mode: mode,
            search_term: Some(cleaned_text),
        };

        Ok(Some(
            self.storage
                .find_tags(filter, Some(editing_tag), &self.tuning.commands)
                .await?,
        ))
    }

    /// Searches for commands based on a query string, returning both the command and whether it was an alias match
    #[instrument(skip_all)]
    pub async fn search_commands(
        &self,
        mode: SearchMode,
        user_only: bool,
        query: &str,
    ) -> Result<(Vec<Command>, bool)> {
        tracing::info!(
            "Searching for commands{} [{mode:?}]: {query}",
            if user_only { " (user only)" } else { "" }
        );

        let query = query.trim();
        let filter = if query.is_empty() {
            // If there are no query, just display user commands
            SearchCommandsFilter {
                category: Some(if user_only {
                    vec![CATEGORY_USER.to_string()]
                } else {
                    vec![CATEGORY_USER.to_string(), CATEGORY_WORKSPACE.to_string()]
                }),
                search_mode: mode,
                ..Default::default()
            }
        } else {
            // Else, parse user query into tags and search term
            let (tags, search_term) = match extract_tags_and_cleaned_text(query) {
                Some((tags, cleaned_query)) => (Some(tags), Some(cleaned_query)),
                None => (None, Some(query.to_string())),
            };

            // Build the filter
            SearchCommandsFilter {
                category: user_only.then(|| vec![CATEGORY_USER.to_string()]),
                source: None,
                tags,
                search_mode: mode,
                search_term,
            }
        };

        // Query the storage
        self.storage
            .find_commands(filter, get_working_dir(), &self.tuning.commands)
            .await
    }
}
/// Searches upwards from the current working dir for a `.intellishell` file.
///
/// The search stops if a `.git` directory or the filesystem root is found.
/// Returns a tuple of (file_path, folder_name) if found.
fn find_workspace_commands_file() -> Option<(PathBuf, Option<String>)> {
    let working_dir = PathBuf::from(get_working_dir());
    let mut current = Some(working_dir.as_path());
    while let Some(parent) = current {
        let candidate = parent.join(".intellishell");
        if candidate.is_file() {
            let folder_name = parent.file_name().and_then(|n| n.to_str()).map(String::from);
            return Some((candidate, folder_name));
        }

        if parent.join(".git").is_dir() {
            // Workspace boundary found
            return None;
        }

        current = parent.parent();
    }
    None
}
