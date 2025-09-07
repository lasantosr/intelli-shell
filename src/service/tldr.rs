use color_eyre::{
    Report,
    eyre::{Context, OptionExt, eyre},
};
use futures_util::{StreamExt, TryStreamExt, stream};
use git2::{
    FetchOptions, Repository,
    build::{CheckoutBuilder, RepoBuilder},
};
use tokio::{fs::File, sync::mpsc};
use tracing::instrument;
use walkdir::WalkDir;

use super::{IntelliShellService, import::parse_import_items};
use crate::{
    errors::Result,
    model::{ImportStats, SOURCE_TLDR},
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
        commands: Vec<String>,
        progress: mpsc::Sender<TldrFetchProgress>,
    ) -> Result<ImportStats> {
        // Setup repository
        self.setup_tldr_repo(progress.clone()).await?;

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

        // Import the commands
        self.storage.import_items(items_stream, true, false).await
    }

    #[instrument(skip_all)]
    async fn setup_tldr_repo(&self, progress: mpsc::Sender<TldrFetchProgress>) -> Result<bool> {
        const BRANCH: &str = "main";
        const REPO_URL: &str = "https://github.com/tldr-pages/tldr.git";

        let tldr_repo_path = self.tldr_repo_path.clone();

        tokio::task::spawn_blocking(move || {
            let send_progress = |status| {
                // Use blocking_send as we are in a sync context
                progress.blocking_send(TldrFetchProgress::Repository(status)).ok();
            };
            if tldr_repo_path.exists() {
                tracing::info!("Fetching latest tldr changes ...");
                send_progress(RepoStatus::Fetching);

                // Open the existing repository.
                let repo = Repository::open(&tldr_repo_path).wrap_err("Failed to open existing tldr repository")?;

                // Get the 'origin' remote
                let mut remote = repo.find_remote("origin")?;

                // Configure fetch options for a shallow fetch
                let mut fetch_options = FetchOptions::new();
                fetch_options.depth(1);

                // Fetch the latest changes from the remote 'main' branch
                let refspec = format!("refs/heads/{BRANCH}:refs/remotes/origin/{BRANCH}");
                remote
                    .fetch(&[refspec], Some(&mut fetch_options), None)
                    .wrap_err("Failed to fetch from tldr remote")?;

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
                tracing::info!("Performing a shallow clone of '{REPO_URL}' ...");
                send_progress(RepoStatus::Cloning);

                // Configure fetch options for a shallow fetch
                let mut fetch_options = FetchOptions::new();
                fetch_options.depth(1);

                // Clone the repository
                RepoBuilder::new()
                    .branch(BRANCH)
                    .fetch_options(fetch_options)
                    .clone(REPO_URL, &tldr_repo_path)
                    .wrap_err("Failed to clone tldr repository")?;

                tracing::info!("Repository successfully cloned");
                send_progress(RepoStatus::DoneCloning);
                Ok(true)
            }
        })
        .await
        .wrap_err("tldr repository task failed")?
    }
}
