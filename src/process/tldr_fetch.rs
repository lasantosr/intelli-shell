use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    time::Duration,
};

use color_eyre::eyre::Context;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use super::{Process, ProcessOutput};
use crate::{
    cli::TldrFetchProcess,
    config::Config,
    errors::AppError,
    format_error,
    service::{IntelliShellService, RepoStatus, TldrFetchProgress},
    widgets::SPINNER_CHARS,
};

impl Process for TldrFetchProcess {
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput> {
        let mut commands = self.commands;
        if let Some(filter_commands) = self.filter_commands {
            let content = filter_commands
                .into_reader()
                .wrap_err("Couldn't read filter commands file")?;
            let reader = BufReader::new(content);
            for line in reader.lines() {
                let line = line.wrap_err("Failed to read line from filter commands")?;
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("#") && !trimmed.starts_with("//") {
                    commands.push(trimmed.to_string());
                }
            }
        }

        let (tx, mut rx) = mpsc::channel(32);

        let service_handle =
            tokio::spawn(async move { service.fetch_tldr_commands(self.category, commands, tx).await });

        let m = MultiProgress::new();
        let pb1 = m.add(ProgressBar::new_spinner());
        pb1.set_style(style_active());
        pb1.set_prefix("[1/3]");
        pb1.enable_steady_tick(Duration::from_millis(100));

        let pb2 = m.add(ProgressBar::new_spinner());
        pb2.set_style(style_active());

        let pb3 = m.add(ProgressBar::new(0));
        let spinner_style = ProgressStyle::with_template("      {spinner:.dim.white} {msg}").unwrap();
        let mut file_spinners: HashMap<String, ProgressBar> = HashMap::new();

        while let Some(progress) = rx.recv().await {
            match progress {
                TldrFetchProgress::Repository(status) => {
                    let (msg, done) = match status {
                        RepoStatus::Cloning => ("Cloning tldr repository ...", false),
                        RepoStatus::DoneCloning => ("Cloned tldr repository", true),
                        RepoStatus::Fetching => ("Fetching latest tldr changes ...", false),
                        RepoStatus::UpToDate => ("Up-to-date tldr repository", true),
                        RepoStatus::Updating => ("Updating tldr repository ...", false),
                        RepoStatus::DoneUpdating => ("Updated tldr repository", true),
                    };
                    pb1.set_message(msg);
                    if done {
                        pb1.set_style(style_done());
                        pb1.finish();
                    }
                }
                TldrFetchProgress::LocatingFiles => {
                    pb2.set_prefix("[2/3]");
                    pb2.set_message("Locating files ...");
                    pb2.enable_steady_tick(Duration::from_millis(100));
                }
                TldrFetchProgress::FilesLocated(count) => {
                    pb2.set_style(style_done());
                    pb2.finish_with_message(format!("Found {count} files"));
                }
                TldrFetchProgress::ProcessingStart(total) => {
                    pb3.set_length(total);
                    pb3.set_style(
                        ProgressStyle::with_template("{prefix:.blue.bold} [{bar:40.cyan/blue}] {pos}/{len} {wide_msg}")
                            .unwrap()
                            .progress_chars("##-"),
                    );
                    pb3.set_prefix("[3/3]");
                    pb3.set_message("Processing files ...");
                }
                TldrFetchProgress::ProcessingFile(command) => {
                    let spinner = m.add(ProgressBar::new_spinner());
                    spinner.set_style(spinner_style.clone());
                    spinner.set_message(format!("Processing {command} ..."));
                    file_spinners.insert(command, spinner);
                }
                TldrFetchProgress::FileProcessed(command) => {
                    if let Some(spinner) = file_spinners.remove(&command) {
                        spinner.finish_and_clear();
                    }
                    pb3.inc(1);
                }
            }
        }

        pb3.set_style(style_done());
        pb3.finish_with_message("Done processing files");

        match service_handle.await? {
            Ok(stats) => Ok(stats.into_output(&config.theme)),
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}

fn style_active() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.blue.bold} {spinner} {wide_msg}")
        .unwrap()
        .tick_strings(&SPINNER_CHARS)
}

fn style_done() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.green.bold} {wide_msg}").unwrap()
}
