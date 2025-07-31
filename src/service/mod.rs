use std::path::{Path, PathBuf};

use crate::{config::SearchTuning, storage::SqliteStorage};

mod command;
mod import_export;
mod tldr;
mod variable;
mod version;

/// Service for managing user commands in IntelliShell
#[derive(Clone)]
pub struct IntelliShellService {
    check_updates: bool,
    storage: SqliteStorage,
    tuning: SearchTuning,
    tldr_repo_path: PathBuf,
}

impl IntelliShellService {
    /// Creates a new instance of `IntelliShellService` with the provided storage
    pub fn new(storage: SqliteStorage, tuning: SearchTuning, data_dir: impl AsRef<Path>, check_updates: bool) -> Self {
        Self {
            check_updates,
            storage,
            tuning,
            tldr_repo_path: data_dir.as_ref().join("tldr"),
        }
    }

    #[cfg(debug_assertions)]
    pub async fn query(&self, sql: String) -> color_eyre::eyre::Result<String> {
        self.storage.query(sql).await
    }
}
