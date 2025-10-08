use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::fs::File;
use tracing::instrument;

use crate::{
    config::{AiConfig, SearchTuning},
    errors::Result,
    model::{CATEGORY_WORKSPACE, SOURCE_WORKSPACE},
    service::import::parse_import_items,
    storage::SqliteStorage,
    utils::get_working_dir,
};

mod ai;
mod command;
mod completion;
mod export;
mod import;
mod variable;
mod version;

#[cfg(feature = "tldr")]
mod tldr;

pub use ai::AiFixProgress;
pub use completion::{FORBIDDEN_COMPLETION_ROOT_CMD_CHARS, FORBIDDEN_COMPLETION_VARIABLE_CHARS};
#[cfg(feature = "tldr")]
pub use tldr::{RepoStatus, TldrFetchProgress};

/// Service for managing user commands in IntelliShell
#[derive(Clone)]
pub struct IntelliShellService {
    check_updates: bool,
    storage: SqliteStorage,
    tuning: SearchTuning,
    ai: AiConfig,
    #[cfg(feature = "tldr")]
    tldr_repo_path: PathBuf,
    version_check_state: Arc<Mutex<version::VersionCheckState>>,
}

impl IntelliShellService {
    /// Creates a new instance of `IntelliShellService`
    pub fn new(
        storage: SqliteStorage,
        tuning: SearchTuning,
        ai: AiConfig,
        data_dir: impl AsRef<Path>,
        check_updates: bool,
    ) -> Self {
        Self {
            check_updates,
            storage,
            tuning,
            ai,
            #[cfg(feature = "tldr")]
            tldr_repo_path: data_dir.as_ref().join("tldr"),
            version_check_state: Arc::new(Mutex::new(version::VersionCheckState::NotStarted)),
        }
    }

    #[cfg(debug_assertions)]
    pub async fn query(&self, sql: String) -> crate::errors::Result<String> {
        self.storage.query(sql).await
    }

    /// Loads workspace commands and completions from the `.intellishell` file in the current working directory setting
    /// up the temporary tables in the database if they don't exist.
    ///
    /// Additionally, loads `.intellishell` files from paths specified in the `INTELLI_WORKSPACE_PATH` environment
    /// variable (colon-separated on Unix, semicolon-separated on Windows).
    ///
    /// Returns whether a workspace file was processed or not
    #[instrument(skip_all)]
    pub async fn load_workspace_items(&self) -> Result<bool> {
        if !env::var("INTELLI_SKIP_WORKSPACE")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true)
        {
            return Ok(false);
        }

        // Collect all workspace files
        let workspace_files = find_workspace_files();

        if workspace_files.is_empty() {
            return Ok(false);
        }

        // Set up the temporary tables in the database
        self.storage.setup_workspace_storage().await?;

        // Load all workspace files
        for (workspace_file, folder_name) in workspace_files {
            tracing::debug!("Found workspace file at {}", workspace_file.display());

            // Parse the items from the file
            let file = File::open(&workspace_file).await?;
            let tag = format!("#{}", folder_name.as_deref().unwrap_or("workspace"));
            let items_stream = parse_import_items(file, vec![tag], CATEGORY_WORKSPACE, SOURCE_WORKSPACE);

            // Import items into the temp tables
            let stats = self.storage.import_items(items_stream, false, true).await?;

            tracing::info!(
                "Loaded {} commands and {} completions from workspace {}",
                stats.commands_imported,
                stats.completions_imported,
                workspace_file.display()
            );
        }

        Ok(true)
    }
}

/// Searches for `.intellishell` files in the current working directory tree and additional paths.
///
/// First searches upwards from the current working dir until a `.git` directory or the filesystem root is found.
/// Then searches in directories specified by the `INTELLI_WORKSPACE_PATH` environment variable.
///
/// The paths in `INTELLI_WORKSPACE_PATH` should be separated by:
/// - `:` (colon) on Unix-like systems
/// - `;` (semicolon) on Windows
///
/// Returns a vector of tuples (file_path, folder_name) for all found files, with local workspace file first.
fn find_workspace_files() -> Vec<(PathBuf, Option<String>)> {
    let mut result = Vec::new();

    // Search upwards from current directory
    let working_dir = PathBuf::from(get_working_dir());
    let mut current = Some(working_dir.as_path());
    while let Some(parent) = current {
        let candidate = parent.join(".intellishell");
        if candidate.is_file() {
            let folder_name = parent.file_name().and_then(|n| n.to_str()).map(String::from);
            result.push((candidate, folder_name));
            break;
        }

        if parent.join(".git").is_dir() {
            // Workspace boundary found
            break;
        }

        current = parent.parent();
    }

    // Search in INTELLI_WORKSPACE_PATH directories
    if let Ok(path_var) = env::var("INTELLI_WORKSPACE_PATH") {
        if !path_var.is_empty() {
            #[cfg(target_os = "windows")]
            let separator = ';';
            #[cfg(not(target_os = "windows"))]
            let separator = ':';

            for path_str in path_var.split(separator) {
                let path_str = path_str.trim();
                if path_str.is_empty() {
                    continue;
                }

                let dir_path = PathBuf::from(path_str);
                let candidate = dir_path.join(".intellishell");

                if candidate.is_file() {
                    let folder_name = dir_path.file_name().and_then(|n| n.to_str()).map(String::from);
                    tracing::debug!("Found .intellishell in INTELLI_WORKSPACE_PATH: {}", candidate.display());
                    result.push((candidate, folder_name));
                } else {
                    tracing::trace!(
                        "No .intellishell file found in INTELLI_WORKSPACE_PATH directory: {}",
                        dir_path.display()
                    );
                }
            }
        }
    }

    result
}
