use std::{path::Path, process::Command};

use color_eyre::{
    Report,
    eyre::{Context, OptionExt, eyre},
};
use futures_util::{FutureExt, StreamExt, TryStreamExt, stream};
use git2::{Remote, Repository, build::CheckoutBuilder};
use tokio::{fs::File, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use walkdir::WalkDir;

use super::{IntelliShellService, import::parse_import_items};
use crate::{
    errors::{Result, UserFacingError},
    model::{ImportStats, SOURCE_TLDR, TldrConnectionMode},
};

/// Progress events for the `tldr fetch` operation
#[derive(Debug)]
pub enum TldrFetchProgress {
    /// Indicates the status of the tldr git repository
    Repository(RepoStatus),
    /// Indicates that the tldr command files are being located
    LocatingFiles,
    /// Indicates that the tldr command files have been located
    FilesLocated(u64),
    /// Indicates the start of the file processing stage
    ProcessingStart(u64),
    /// Indicates that a single file is being processed
    ProcessingFile(String),
    /// Indicates that a single file has been processed
    FileProcessed(String),
}

/// The status of the tldr git repository
#[derive(Debug)]
pub enum RepoStatus {
    /// Cloning the repository for the first time
    Cloning,
    /// The repository has been successfully cloned
    DoneCloning,
    /// Fetching latest changes
    Fetching,
    /// The repository is already up-to-date
    UpToDate,
    /// Updating the local repository
    Updating,
    /// The repository has been successfully updated
    DoneUpdating,
}

impl IntelliShellService {
    /// Removes tldr commands matching the given criteria.
    ///
    /// Returns the number of commands removed
    #[instrument(skip_all)]
    pub async fn clear_tldr_commands(&self, category: Option<String>) -> Result<u64> {
        self.storage.delete_tldr_commands(category).await
    }

    /// Fetches and imports tldr commands matching the given criteria
    #[instrument(skip_all)]
    pub async fn fetch_tldr_commands(
        &self,
        category: Option<String>,
        connection_mode: TldrConnectionMode,
        commands: Vec<String>,
        progress: mpsc::Sender<TldrFetchProgress>,
        cancellation_token: CancellationToken,
    ) -> Result<ImportStats> {
        // Check for cancellation at the beginning
        if cancellation_token.is_cancelled() {
            tracing::info!("TLDR fetch cancelled before starting");
            return Err(UserFacingError::Cancelled.into());
        }

        // Setup repository
        self.setup_tldr_repo(connection_mode, progress.clone(), cancellation_token.clone())
            .await?;

        // Determine which categories to import
        let categories = if let Some(cat) = category {
            vec![cat]
        } else {
            vec![
                "common".to_owned(),
                #[cfg(target_os = "windows")]
                "windows".to_owned(),
                #[cfg(target_os = "android")]
                "android".to_owned(),
                #[cfg(target_os = "macos")]
                "osx".to_owned(),
                #[cfg(target_os = "freebsd")]
                "freebsd".to_owned(),
                #[cfg(target_os = "openbsd")]
                "openbsd".to_owned(),
                #[cfg(target_os = "netbsd")]
                "netbsd".to_owned(),
                #[cfg(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "openbsd",
                    target_os = "netbsd",
                    target_os = "dragonfly",
                ))]
                "linux".to_owned(),
            ]
        };

        // Construct the path to the tldr pages directory
        let pages_path = self.tldr_repo_path.join("pages");

        tracing::info!("Locating files for categories: {}", categories.join(", "));
        progress.send(TldrFetchProgress::LocatingFiles).await.ok();

        // Iterate over directory entries
        let mut command_files = Vec::new();
        let mut iter = WalkDir::new(&pages_path).max_depth(2).into_iter();
        while let Some(result) = iter.next() {
            // Check for cancellation within the file discovery loop
            if cancellation_token.is_cancelled() {
                tracing::info!("TLDR fetch cancelled during file discovery");
                return Err(UserFacingError::Cancelled.into());
            }

            let entry = result.wrap_err("Couldn't read tldr repository files")?;
            let path = entry.path();

            // Skip base path
            if path == pages_path {
                continue;
            }

            // Skip non-included categories
            let file_name = entry.file_name().to_str().ok_or_eyre("Non valid file name")?;
            if entry.file_type().is_dir() {
                if !categories.iter().any(|c| c == file_name) {
                    tracing::trace!("Skipped directory: {file_name}");
                    iter.skip_current_dir();
                    continue;
                } else {
                    // The directory entry itself must be skipped as well, we only care about files
                    continue;
                }
            }

            // We only care about markdown files
            let Some(file_name_no_ext) = file_name.strip_suffix(".md") else {
                tracing::warn!("Unexpected file found: {}", path.display());
                continue;
            };

            // Skip non-included commands
            if !commands.is_empty() {
                if !commands.iter().any(|c| c == file_name_no_ext) {
                    continue;
                } else {
                    tracing::trace!("Included command: {file_name_no_ext}");
                }
            }

            // Retrieve the category
            let category = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|p| p.to_str())
                .ok_or_eyre("Couldn't read tldr category")?
                .to_owned();

            // Include the command file
            command_files.push((path.to_path_buf(), category, file_name_no_ext.to_owned()));
        }

        progress
            .send(TldrFetchProgress::FilesLocated(command_files.len() as u64))
            .await
            .ok();

        tracing::info!("Found {} files to be processed", command_files.len());

        progress
            .send(TldrFetchProgress::ProcessingStart(command_files.len() as u64))
            .await
            .ok();

        // Create a stream that reads and parses each command file concurrently
        let items_stream = stream::iter(command_files)
            .map(move |(path, category, command)| {
                let progress = progress.clone();
                async move {
                    progress
                        .send(TldrFetchProgress::ProcessingFile(command.clone()))
                        .await
                        .ok();

                    // Open and parse the file
                    let file = File::open(&path)
                        .await
                        .wrap_err_with(|| format!("Failed to open tldr file: {}", path.display()))?;
                    let stream = parse_import_items(file, vec![], category, SOURCE_TLDR);

                    progress.send(TldrFetchProgress::FileProcessed(command)).await.ok();
                    Ok::<_, Report>(stream)
                }
            })
            .buffered(5)
            .try_flatten();

        // Import items while the token is not cancelled
        let stats = self
            .storage
            .import_items(
                items_stream.take_until(cancellation_token.clone().cancelled_owned().fuse()),
                true,
                false,
            )
            .await?;

        // After processing, check if cancellation was the reason the stream ended
        if cancellation_token.is_cancelled() {
            tracing::info!("TLDR fetch cancelled during command processing");
            return Err(UserFacingError::Cancelled.into());
        }

        Ok(stats)
    }

    #[instrument(skip_all)]
    async fn setup_tldr_repo(
        &self,
        connection_mode: TldrConnectionMode,
        progress: mpsc::Sender<TldrFetchProgress>,
        cancellation_token: CancellationToken,
    ) -> Result<bool> {
        const BRANCH: &str = "main";

        let tldr_repo_path = self.tldr_repo_path.clone();

        tokio::task::spawn_blocking(move || {
            // Helper to send progress to the channel
            let send_progress = |status| {
                // Use blocking_send as we are in a sync context
                progress.blocking_send(TldrFetchProgress::Repository(status)).ok();
            };
            let repo_url = tldr_repo_url(connection_mode);
            // Fetch latest repo changes or clone it if it doesn't exist yet
            if tldr_repo_path.exists() {
                tracing::info!("Fetching latest tldr changes ...");
                send_progress(RepoStatus::Fetching);

                // Open the existing repository.
                let repo = Repository::open(&tldr_repo_path).wrap_err("Failed to open existing tldr repository")?;

                // Ensure the 'origin' remote uses the requested transport.
                let _remote = ensure_tldr_remote(&repo, connection_mode)?;

                if cancellation_token.is_cancelled() {
                    return Err(UserFacingError::Cancelled.into());
                }
                run_git_fetch(&tldr_repo_path, BRANCH)?;

                // Get the commit OID from the fetched data (FETCH_HEAD)
                let fetch_head = repo.find_reference("FETCH_HEAD")?;
                let fetch_commit_oid = fetch_head
                    .target()
                    .ok_or_else(|| eyre!("FETCH_HEAD is not a direct reference"))?;

                // Get the OID of the current commit on the local branch
                let local_ref_name = format!("refs/heads/{BRANCH}");
                let local_commit_oid = repo.find_reference(&local_ref_name)?.target();

                // If the commit OIDs are the same, the repo is already up-to-date
                if Some(fetch_commit_oid) == local_commit_oid {
                    tracing::info!("Repository is already up-to-date");
                    send_progress(RepoStatus::UpToDate);
                    return Ok(false);
                }

                tracing::info!("Updating to the latest version ...");
                send_progress(RepoStatus::Updating);

                // Find the local branch reference
                let mut local_ref = repo.find_reference(&local_ref_name)?;
                // Update the local branch to point directly to the newly fetched commit
                let msg = format!("Resetting to latest commit {fetch_commit_oid}");
                local_ref.set_target(fetch_commit_oid, &msg)?;

                // Point HEAD to the updated local branch
                repo.set_head(&local_ref_name)?;

                // Checkout the new HEAD to update the files in the working directory
                let mut checkout_builder = CheckoutBuilder::new();
                checkout_builder.force();
                repo.checkout_head(Some(&mut checkout_builder))?;

                tracing::info!("Repository successfully updated");
                send_progress(RepoStatus::DoneUpdating);
                Ok(true)
            } else {
                tracing::info!("Performing a shallow clone of '{repo_url}' ...");
                send_progress(RepoStatus::Cloning);

                if cancellation_token.is_cancelled() {
                    return Err(UserFacingError::Cancelled.into());
                }
                run_git_clone(repo_url, BRANCH, &tldr_repo_path)?;

                tracing::info!("Repository successfully cloned");
                send_progress(RepoStatus::DoneCloning);
                Ok(true)
            }
        })
        .await
        .wrap_err("tldr repository task failed")?
    }
}

fn tldr_repo_url(connection_mode: TldrConnectionMode) -> &'static str {
    match connection_mode {
        TldrConnectionMode::Https => "https://github.com/tldr-pages/tldr.git",
        TldrConnectionMode::Ssh => "git@github.com:tldr-pages/tldr.git",
    }
}

fn ensure_tldr_remote<'repo>(repo: &'repo Repository, connection_mode: TldrConnectionMode) -> Result<Remote<'repo>> {
    let repo_url = tldr_repo_url(connection_mode);
    let current_url = current_tldr_origin_url(repo)?;

    match current_url.as_deref() {
        Some(url) if url == repo_url => {}
        Some(_) => repo
            .remote_set_url("origin", repo_url)
            .wrap_err("Failed to update tldr origin remote URL")?,
        None => {
            repo.remote("origin", repo_url)
                .wrap_err("Failed to create tldr origin remote")?;
        }
    }

    repo.find_remote("origin")
        .wrap_err("Failed to open tldr origin remote")
        .map_err(Into::into)
}

fn run_git_clone(repo_url: &str, branch: &str, repo_path: &Path) -> Result<()> {
    run_git_command(
        Command::new("git")
            .args(["clone", "--depth", "1", "--branch", branch, repo_url])
            .arg(repo_path),
    )
}

fn run_git_fetch(repo_path: &Path, branch: &str) -> Result<()> {
    let refspec = format!("refs/heads/{branch}:refs/remotes/origin/{branch}");
    run_git_command(
        Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["fetch", "--depth", "1", "origin"])
            .arg(refspec),
    )
}

fn run_git_command(command: &mut Command) -> Result<()> {
    let output = command.output().wrap_err("Failed to execute git command")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let message = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stderr}\n{stdout}")
    };

    Err(eyre!("git command failed: {message}").into())
}

fn current_tldr_origin_url(repo: &Repository) -> Result<Option<String>> {
    let config = repo.config().wrap_err("Failed to read tldr repository git config")?;

    match config.get_string("remote.origin.url") {
        Ok(url) => Ok(Some(url)),
        Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(err) => Err(Report::from(err)
            .wrap_err("Failed to read tldr origin remote URL")
            .into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
    };

    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_tldr_repo_url_for_ssh() {
        assert_eq!(
            tldr_repo_url(TldrConnectionMode::Ssh),
            "git@github.com:tldr-pages/tldr.git"
        );
    }

    #[test]
    fn test_ensure_tldr_remote_creates_origin() -> Result<()> {
        let temp_dir = TempRepoDir::new();
        let repo = Repository::init(temp_dir.path())?;

        let _remote = ensure_tldr_remote(&repo, TldrConnectionMode::Https)?;
        assert_eq!(
            current_tldr_origin_url(&repo)?,
            Some(tldr_repo_url(TldrConnectionMode::Https).to_string())
        );

        Ok(())
    }

    #[test]
    fn test_ensure_tldr_remote_updates_existing_origin_url() -> Result<()> {
        let temp_dir = TempRepoDir::new();
        let repo = Repository::init(temp_dir.path())?;
        repo.remote("origin", tldr_repo_url(TldrConnectionMode::Https))?;

        let _remote = ensure_tldr_remote(&repo, TldrConnectionMode::Ssh)?;
        assert_eq!(
            current_tldr_origin_url(&repo)?,
            Some(tldr_repo_url(TldrConnectionMode::Ssh).to_string())
        );

        Ok(())
    }

    struct TempRepoDir {
        path: PathBuf,
    }

    impl TempRepoDir {
        fn new() -> Self {
            Self {
                path: env::temp_dir().join(format!("intelli-shell-tldr-test-{}", Uuid::now_v7())),
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRepoDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
