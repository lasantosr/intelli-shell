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
    /// Returns whether a workspace file was processed or not
    #[instrument(skip_all)]
    pub async fn load_workspace_items(&self) -> Result<bool> {
        if env::var("INTELLI_SKIP_WORKSPACE")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true)
            && let Some((workspace_file, folder_name)) = find_workspace_file()
        {
            tracing::debug!("Found workspace file at {}", workspace_file.display());

            // Set up the temporary tables in the database
            self.storage.setup_workspace_storage().await?;

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

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Searches upwards from the current working dir for a `.intellishell` file.
///
/// The search stops if a `.git` directory or the filesystem root is found.
/// Returns a tuple of (file_path, folder_name) if found.
fn find_workspace_file() -> Option<(PathBuf, Option<String>)> {
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
