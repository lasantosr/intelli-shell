use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    config::{AiConfig, SearchTuning},
    storage::SqliteStorage,
};

mod ai;
mod command;
mod import_export;
mod tldr;
mod variable;
mod version;

pub use ai::AiFixProgress;
pub use tldr::{RepoStatus, TldrFetchProgress};

/// Service for managing user commands in IntelliShell
#[derive(Clone)]
pub struct IntelliShellService {
    check_updates: bool,
    storage: SqliteStorage,
    tuning: SearchTuning,
    ai: AiConfig,
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
            tldr_repo_path: data_dir.as_ref().join("tldr"),
            version_check_state: Arc::new(Mutex::new(version::VersionCheckState::NotStarted)),
        }
    }

    #[cfg(debug_assertions)]
    pub async fn query(&self, sql: String) -> crate::errors::Result<String> {
        self.storage.query(sql).await
    }
}
