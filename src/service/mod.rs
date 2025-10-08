use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use directories::BaseDirs;
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

    /// Loads workspace commands and completions from `.intellishell` files using a built-in search hierarchy.
    ///
    /// Search order:
    /// 1. Local workspace: searches upward from current directory until `.git` or filesystem root
    /// 2. Home directory: `~/.intellishell` (file or directory)
    /// 3. System-wide: `/etc/.intellishell` (Unix) or `C:\ProgramData\.intellishell` (Windows)
    ///
    /// Each location can be either a file or directory. Directories are recursively searched for all files.
    /// Sets up temporary tables in the database if they don't exist.
    ///
    /// Returns whether any workspace file was processed
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

/// Collects `.intellishell` files or directories from a given path.
///
/// If the path is a file, returns it with the parent folder name as tag.
/// If the path is a directory, recursively collects all files inside (excluding
/// dotfiles, using the file name as tag.
fn collect_intellishell_files_from_location(
    path: &Path,
    seen_paths: &mut HashSet<PathBuf>,
) -> Vec<(PathBuf, Option<String>)> {
    let mut result = Vec::new();

    if path.is_file() {
        // Single file: use parent folder name as tag
        if seen_paths.insert(path.to_path_buf()) {
            let folder_name = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(String::from);
            result.push((path.to_path_buf(), folder_name));
        } else {
            tracing::debug!("Skipping duplicate workspace file: {}", path.display());
        }
    } else if path.is_dir() {
        // Directory: recursively collect all files
        collect_files_recursive(path, seen_paths, &mut result);
    }

    result
}

/// Recursively collects files from a directory, skipping dotfiles
fn collect_files_recursive(dir: &Path, seen_paths: &mut HashSet<PathBuf>, result: &mut Vec<(PathBuf, Option<String>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        tracing::trace!("Could not read directory: {}", dir.display());
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Get file name for filtering
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // If it is a file but not a dotfile then process it
        if path.is_file() && !file_name.starts_with(".") {
            // Add file with its name as tag
            if seen_paths.insert(path.clone()) {
                let tag = path.file_stem().and_then(|n| n.to_str()).map(String::from);
                tracing::debug!("Found workspace file in directory: {}", path.display());
                result.push((path, tag));
            } else {
                tracing::debug!("Skipping duplicate workspace file: {}", path.display());
            }
        } else if path.is_dir() {
            collect_files_recursive(&path, seen_paths, result);
        }
    }
}

/// Searches for `.intellishell` files using a built-in hierarchy.
///
/// Search order:
/// 1. Local workspace: searches upward from current directory until `.git` or filesystem root
/// 2. Home directory: `~/.intellishell` (file or directory)
/// 3. System-wide: `/etc/.intellishell` (Unix) or `C:\ProgramData\.intellishell` (Windows)
///
/// Each location can be either a file or directory:
/// - File: loaded with parent folder name as tag
/// - Directory: all files inside are loaded recursively with file name as tag
///
/// Returns a vector of tuples (file_path, tag) for all found files.
fn find_workspace_files() -> Vec<(PathBuf, Option<String>)> {
    let mut result = Vec::new();
    let mut seen_paths = HashSet::new();

    // 1. Search upwards from current directory
    let working_dir = PathBuf::from(get_working_dir());
    let mut current = Some(working_dir.as_path());
    while let Some(parent) = current {
        let candidate = parent.join(".intellishell");
        if candidate.exists() {
            result.extend(collect_intellishell_files_from_location(&candidate, &mut seen_paths));
            break;
        }

        if parent.join(".git").is_dir() {
            // Workspace boundary found
            break;
        }

        current = parent.parent();
    }

    // 2. Search in home directory
    if let Some(base_dirs) = BaseDirs::new() {
        let home_candidate = base_dirs.home_dir().join(".intellishell");
        if home_candidate.exists() {
            tracing::debug!(
                "Checking home directory for .intellishell: {}",
                home_candidate.display()
            );
            result.extend(collect_intellishell_files_from_location(
                &home_candidate,
                &mut seen_paths,
            ));
        }
    }

    // 3. Search in system-wide location
    #[cfg(target_os = "windows")]
    let system_candidate = PathBuf::from(r"C:\ProgramData\.intellishell");
    #[cfg(not(target_os = "windows"))]
    let system_candidate = PathBuf::from("/etc/.intellishell");

    if system_candidate.exists() {
        tracing::debug!(
            "Checking system-wide location for .intellishell: {}",
            system_candidate.display()
        );
        result.extend(collect_intellishell_files_from_location(
            &system_candidate,
            &mut seen_paths,
        ));
    }

    result
}
