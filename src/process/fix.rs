use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio_util::sync::CancellationToken;

use super::{Process, ProcessOutput};
use crate::{
    ai::CommandFix,
    cli::CommandFixProcess,
    config::Config,
    errors::AppError,
    format_error,
    service::{AiFixProgress, IntelliShellService},
    widgets::SPINNER_CHARS,
};

impl Process for CommandFixProcess {
    async fn execute(
        self,
        config: Config,
        service: IntelliShellService,
        cancellation_token: CancellationToken,
    ) -> color_eyre::Result<ProcessOutput> {
        // Setup the progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                .unwrap()
                .tick_strings(&SPINNER_CHARS),
        );

        // Setup callback for progress updates
        let on_progress = |progress: AiFixProgress| match progress {
            // When the command has been executed
            AiFixProgress::Thinking => {
                // Print a separator line to indicate the end of the command output
                eprintln!("\n────────────────────────────────────────────────────────────────────────────────\n");
                // Display the spinner
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Thinking ...");
            }
        };

        // Call the service to fix the command
        let res = service
            .fix_command(&self.command, self.history.as_deref(), on_progress, cancellation_token)
            .await;

        // Clear the spinner
        pb.finish_and_clear();

        // Handle the result
        match res {
            Ok(None) => Ok(ProcessOutput::success()),
            Ok(Some(CommandFix {
                summary,
                diagnosis,
                proposal,
                fixed_command,
            })) => {
                let mut msg = format!(
                    r"🧠 IntelliShell Diagnosis

❌ {summary}
{}

✨ Fix
{}
",
                    config.theme.secondary.apply(diagnosis),
                    config.theme.secondary.apply(proposal)
                );
                let mut out = ProcessOutput::success();
                if !fixed_command.trim().is_empty() {
                    msg += "\nSuggested Command 👉";
                    out = out.stdout(&fixed_command).fileout(fixed_command);
                }
                Ok(out.stderr(msg))
            }
            Err(AppError::UserFacing(err)) => Ok(ProcessOutput::fail().stderr(format_error!(config.theme, "{err}"))),
            Err(AppError::Unexpected(report)) => Err(report),
        }
    }
}
