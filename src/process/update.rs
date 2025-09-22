use crossterm::style::Stylize;
use tokio_util::sync::CancellationToken;

use crate::{
    cli::UpdateProcess,
    config::Config,
    errors::AppError,
    format_error,
    process::{Process, ProcessOutput},
    service::IntelliShellService,
    utils::{InstallationMethod, detect_installation_method},
};

impl Process for UpdateProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        _cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        let current_version_str = env!("CARGO_PKG_VERSION");
        let current_version_tag = format!("v{current_version_str}");
        let latest_version = match service.check_new_version().await {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Ok(ProcessOutput::success().stdout(format!(
                    "You're all set! You are running the latest version of intelli-shell ({}).",
                    current_version_tag.cyan()
                )));
            }
            Err(AppError::UserFacing(err)) => {
                return Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}")));
            }
            Err(AppError::Unexpected(report)) => return Err(report),
        };
        let latest_version_tag = format!("v{latest_version}");

        // Common header for all update-needed messages
        let header = format!(
            "🚀 A new version is available! ({} -> {})",
            current_version_tag.yellow(),
            latest_version_tag.clone().green(),
        );

        // Detect the installation method to provide tailored instructions
        match detect_installation_method(&config.data_dir) {
            // Handle automatic update via the installer
            InstallationMethod::Installer => {
                let initial_message = format!("{header}\n\nDownloading ...");
                println!("{initial_message}");

                let target_version_tag = latest_version_tag.clone();
                let status = tokio::task::spawn_blocking(move || {
                    self_update::backends::github::Update::configure()
                        .repo_owner("lasantosr")
                        .repo_name("intelli-shell")
                        .bin_name("intelli-shell")
                        .show_output(false)
                        .show_download_progress(true)
                        .no_confirm(true)
                        .current_version(current_version_str)
                        .target_version_tag(&target_version_tag)
                        .build()?
                        .update()
                })
                .await?;

                println!("\n");

                match status {
                    Ok(self_update::Status::UpToDate(_)) => unreachable!(),
                    Ok(self_update::Status::Updated(_)) => Ok(ProcessOutput::success().stdout(format!(
                        "✅ Update complete! You are now on intelli-shell {}.\n\n💡 Some updates refine shell \
                         integration; a terminal restart might be required if you experience any issues.",
                        latest_version_tag.cyan()
                    ))),
                    Err(e) => Ok(ProcessOutput::fail().stderr(format!(
                        "❌ Update failed:\n{e}\n\nPlease check your network connection or file permissions.",
                    ))),
                }
            }
            // Provide clear, copyable instructions for other installation methods
            installation_method => {
                let instructions = get_manual_update_instructions(installation_method);
                let full_message = format!("{header}\n\n{instructions}");
                Ok(ProcessOutput::success().stdout(full_message))
            }
        }
    }
}

/// Generates user-friendly update instructions based on the installation method
fn get_manual_update_instructions(method: InstallationMethod) -> String {
    match method {
        InstallationMethod::Cargo => format!(
            "It looks like you installed with {}. To update, please run:\n\n{}\n",
            "cargo".yellow(),
            "  LIBSQLITE3_FLAGS=\"-DSQLITE_ENABLE_MATH_FUNCTIONS\" cargo install intelli-shell --locked".cyan()
        ),
        InstallationMethod::Nix => format!(
            "It looks like you installed with {}. Consider updating it via your Nix configuration.",
            "Nix".yellow()
        ),
        InstallationMethod::Source => format!(
            "It looks like you installed from {}. You might need to run:\n\n{}\n",
            "source".yellow(),
            "  git pull && cargo build --release".cyan()
        ),
        InstallationMethod::Unknown(Some(path)) => format!(
            "Could not determine the installation method. Your executable is located at:\n\n  {}\n\nPlease update \
             manually or consider reinstalling with the recommended script.",
            path.cyan()
        ),
        InstallationMethod::Unknown(None) => {
            "Could not determine the installation method. Please update manually.".to_string()
        }
        InstallationMethod::Installer => unreachable!(),
    }
}
