use std::time::Duration;

use color_eyre::{
    Report, Result,
    eyre::{Context, OptionExt, eyre},
};
use futures_util::{StreamExt, TryStreamExt, stream};
use git2::{
    FetchOptions, Repository,
    build::{CheckoutBuilder, RepoBuilder},
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::fs::File;
use tracing::instrument;
use walkdir::WalkDir;

use super::{IntelliShellService, import_export::parse_commands};
use crate::model::SOURCE_TLDR;

impl IntelliShellService {
    /// Removes tldr commands matching the given criteria.
    ///
    /// Returns the number of commands removed
    #[instrument(skip_all)]
    pub async fn clear_tldr_commands(&self, category: Option<String>) -> Result<u64> {
        self.storage.delete_tldr_commands(category).await
    }

    /// Fetches and imports tldr commands matching the given criteria.
    ///
    /// Returns the number of new commands inserted and potentially updated (because they already existed)
    #[instrument(skip_all)]
    pub async fn fetch_tldr_commands(&self, category: Option<String>, commands: Vec<String>) -> Result<(u64, u64)> {
        let m = MultiProgress::new();

        // Setup repository
        let pb1 = m.add(ProgressBar::new_spinner());
        pb1.set_style(style_active());
        pb1.set_prefix("[1/3]");
        pb1.enable_steady_tick(Duration::from_millis(100));
        self.setup_tldr_repo(&pb1).await?;
        pb1.set_style(style_done());
        pb1.finish();

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
        let pb2 = m.add(ProgressBar::new_spinner());
        pb2.set_style(style_active());
        pb2.set_prefix("[2/3]");
        pb2.enable_steady_tick(Duration::from_millis(100));
        pb2.set_message("Locating files ...");

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

        tracing::info!("Found {} files to be processed", command_files.len());
        pb2.set_style(style_done());
        pb2.finish_with_message(format!("Found {} files", command_files.len()));

        let pb3 = m.add(ProgressBar::new(command_files.len() as u64));
        pb3.set_style(
            ProgressStyle::with_template("{prefix:.blue.bold} [{bar:40.cyan/blue}] {pos}/{len} {wide_msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb3.set_prefix("[3/3]");
        pb3.set_message("Processing files ...");
        let spinner_style = ProgressStyle::with_template("      {spinner:.dim.white} {msg}").unwrap();

        // Create a stream that reads and parses each command file concurrently
        let pb3_clone = pb3.clone();
        let commands_stream = stream::iter(command_files)
            .map(move |(path, category, command)| {
                let m_clone = m.clone();
                let spinner_style_clone = spinner_style.clone();
                let pb3_clone = pb3_clone.clone();
                async move {
                    // Add a temporary spinner for this specific file operation
                    let spinner = m_clone.add(ProgressBar::new_spinner());
                    spinner.set_style(spinner_style_clone);
                    spinner.set_message(format!("Processing {command} ..."));

                    // Open and parse the file
                    let file = File::open(&path)
                        .await
                        .wrap_err_with(|| format!("Failed to open tldr file: {}", path.display()))?;
                    let stream = parse_commands(file, vec![], category, SOURCE_TLDR);

                    // When done, remove the spinner for this file
                    spinner.finish_and_clear();
                    pb3_clone.inc(1);
                    Ok::<_, Report>(stream)
                }
            })
            .buffered(5)
            .try_flatten();

        // Import the commands
        let (new, updated) = self.storage.import_commands(commands_stream, None, true, false).await?;

        pb3.set_style(style_done());
        pb3.finish_with_message(format!("{} comands processed", new + updated));

        Ok((new, updated))
    }

    #[instrument(skip_all)]
    async fn setup_tldr_repo(&self, pb: &ProgressBar) -> Result<bool> {
        const BRANCH: &str = "main";
        const REPO_URL: &str = "https://github.com/tldr-pages/tldr.git";

        if self.tldr_repo_path.exists() {
            tracing::info!("Fetching latest tldr changes ...");
            pb.set_message("Fetching latest tldr changes ...");

            // Open the existing repository.
            let repo = Repository::open(&self.tldr_repo_path).wrap_err("Failed to open existing tldr repository")?;

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
                pb.set_message("Up-to-date tldr repository");
                return Ok(false);
            }

            tracing::info!("Updating to the latest version ...");
            pb.set_message("Updating tldr repository ...");

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
            pb.set_message("Updated tldr repository");
            Ok(true)
        } else {
            tracing::info!("Performing a shallow clone of '{REPO_URL}' ...");
            pb.set_message("Cloning tldr repository ...");

            // Configure fetch options for a shallow fetch
            let mut fetch_options = FetchOptions::new();
            fetch_options.depth(1);

            // Clone the repository
            RepoBuilder::new()
                .branch(BRANCH)
                .fetch_options(fetch_options)
                .clone(REPO_URL, &self.tldr_repo_path)
                .wrap_err("Failed to clone tldr repository")?;

            tracing::info!("Repository successfully cloned");
            pb.set_message("Cloned tldr repository");
            Ok(true)
        }
    }
}

fn style_active() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.blue.bold} {spinner} {wide_msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn style_done() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.green.bold} {wide_msg}").unwrap()
}
