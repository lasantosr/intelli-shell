use std::{
    env,
    panic::{self, UnwindSafe},
    path::PathBuf,
    process,
};

use color_eyre::{Report, Section, config::HookBuilder, owo_colors::style};
use futures_util::FutureExt;
use tokio::sync::mpsc;

/// A top-level error enum for the entire application
#[derive(Debug)]
pub enum AppError {
    /// A controlled, expected error that can be safely displayed to the end-user
    UserFacing(UserFacingError),
    /// An unexpected, internal error
    Unexpected(Report),
}

/// A specialized `Result` type for this application
pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// Initializes error and panics handling
pub async fn init<F>(log_path: Option<PathBuf>, fut: F) -> Result<(), Report>
where
    F: Future<Output = Result<(), Report>> + UnwindSafe,
{
    tracing::trace!("Initializing error handlers");
    // Initialize hooks
    let panic_section = if let Some(log_path) = log_path {
        format!(
            "This is a bug. Consider reporting it at {}\nLogs can be found at {}",
            env!("CARGO_PKG_REPOSITORY"),
            log_path.display()
        )
    } else {
        format!(
            "This is a bug. Consider reporting it at {}\nLogs were not generated, consider enabling them on the \
             config or running with INTELLI_LOG=debug.",
            env!("CARGO_PKG_REPOSITORY")
        )
    };
    let (panic_hook, eyre_hook) = HookBuilder::default()
        .panic_section(panic_section.clone())
        .display_env_section(false)
        .display_location_section(true)
        .capture_span_trace_by_default(true)
        .into_hooks();

    // Initialize panic notifier
    let (panic_tx, mut panic_rx) = mpsc::channel(1);

    // Install both hooks
    eyre_hook.install()?;
    panic::set_hook(Box::new(move |panic_info| {
        // At this point the TUI might still be in raw mode, so we can't print to stderr here
        // Instead, we're sending the report through a channel, to be handled after the main future is dropped
        let panic_report = panic_hook.panic_report(panic_info).to_string();
        tracing::error!("Error: {}", strip_ansi_escapes::strip_str(&panic_report));
        if panic_tx.try_send(panic_report).is_err() {
            tracing::error!("Error sending panic report",);
            process::exit(2);
        }
    }));

    tokio::select! {
        biased;
        // Wait for a panic to be notified
        panic_report = panic_rx.recv().fuse() => {
            if let Some(report) = panic_report {
                eprintln!("{report}");
            } else {
                eprintln!(
                    "{}\n\n{panic_section}",
                    style().bright_red().style("A panic occurred, but the detailed report could not be captured.")
                );
                tracing::error!("A panic occurred, but the detailed report could not be captured.");
            }
            // Exit with a non-zero status code
            process::exit(1);
        }
        // Or for the main future to finish, catching unwinding panics
        res = Box::pin(fut).catch_unwind() => {
            match res {
                Ok(r) => r
                    .with_section(move || panic_section)
                    .inspect_err(|err| tracing::error!("Error: {}", strip_ansi_escapes::strip_str(format!("{err:?}")))),
                Err(err) => {
                    if let Ok(report) = panic_rx.try_recv() {
                        eprintln!("{report}");
                    } else if let Some(err) = err.downcast_ref::<&str>() {
                        print_panic_msg(err, panic_section);
                    } else if let Some(err) = err.downcast_ref::<String>() {
                        print_panic_msg(err, panic_section);
                    } else {
                        eprintln!(
                            "{}\n\n{panic_section}",
                            style().bright_red().style("An unexpected panic happened")
                        );
                        tracing::error!("An unexpected panic happened");
                    }
                    // Exit with a non-zero status code
                    process::exit(1);
                }
            }
        }
    }
}

fn print_panic_msg(err: impl AsRef<str>, panic_section: String) {
    let err = err.as_ref();
    eprintln!(
        "{}\nMessage: {}\n\n{panic_section}",
        style().bright_red().style("The application panicked (crashed)."),
        style().blue().style(err)
    );
    tracing::error!("Panic: {err}");
}

/// Represents all possible errors that are meant to be displayed to the end-user
#[derive(Debug, strum::Display)]
pub enum UserFacingError {
    /// The operation was cancelled by the user
    #[strum(to_string = "Operation cancelled by user")]
    Cancelled,
    /// The regex pattern provided for a search is invalid
    #[strum(to_string = "Invalid regex pattern")]
    InvalidRegex,
    /// A fuzzy search was attempted without providing a valid search term
    #[strum(to_string = "Invalid fuzzy search")]
    InvalidFuzzy,
    /// An attempt was made to save an empty command
    #[strum(to_string = "Command cannot be empty")]
    EmptyCommand,
    /// The user tried to save a command that is already bookmarked
    #[strum(to_string = "Command is already bookmarked")]
    CommandAlreadyExists,
    /// The user tried to save a variable value that already exists
    #[strum(to_string = "Value already exists")]
    VariableValueAlreadyExists,
    /// The user tried to save a completion that already exists
    #[strum(to_string = "Variable completion already exists")]
    CompletionAlreadyExists,
    /// An attempt was made to save a completion with an invalid command
    #[strum(to_string = "Completion command can contain only alphanumeric characters or hyphen")]
    CompletionInvalidCommand,
    /// An attempt was made to save a completion with an empty variable
    #[strum(to_string = "Completion variable cannot be empty")]
    CompletionEmptyVariable,
    /// An attempt was made to save a completion with an invalid variable
    #[strum(to_string = "Completion variable can't contain pipe, colon or braces")]
    CompletionInvalidVariable,
    /// An attempt was made to save a completion with an empty provider
    #[strum(to_string = "Completion provider cannot be empty")]
    CompletionEmptySuggestionsProvider,
    /// A completion was not properly formatted when importing
    #[strum(to_string = "Invalid completion format: {0}")]
    ImportCompletionInvalidFormat(String),
    /// The path for an import operation points to a directory or symlink, not a regular file
    #[strum(to_string = "Import path must be a file; directories and symlinks are not supported")]
    ImportLocationNotAFile,
    /// The file specified for an import operation could not be found
    #[strum(to_string = "File not found")]
    ImportFileNotFound,
    /// The path for an export operation already exists and is not a regular file
    #[strum(to_string = "The path already exists and it's not a file")]
    ExportLocationNotAFile,
    /// The parent directory for a file to be exported does not exist
    #[strum(to_string = "Destination directory does not exist")]
    ExportFileParentNotFound,
    /// An attempt was made to export to a specific Gist revision (SHA), which is not allowed
    #[strum(to_string = "Cannot export to a gist revision, provide a gist without a revision")]
    ExportGistLocationHasSha,
    /// A GitHub personal access token is required for exporting to a Gist but was not found
    #[strum(to_string = "GitHub token required for Gist export, set GIST_TOKEN env var or update config")]
    ExportGistMissingToken,
    /// The application lacks the necessary permissions to read from or write to a file
    #[strum(to_string = "Cannot access the file, check {0} permissions")]
    FileNotAccessible(&'static str),
    /// A "broken pipe" error occurred while writing to a file
    #[strum(to_string = "broken pipe")]
    FileBrokenPipe,
    /// The URL provided for an HTTP operation is malformed
    #[strum(to_string = "Invalid URL, please provide a valid HTTP/S address")]
    HttpInvalidUrl,
    /// An HTTP request to a remote URL has failed
    #[strum(to_string = "HTTP request failed: {0}")]
    HttpRequestFailed(String),
    /// A required GitHub Gist ID was not provided via arguments or configuration
    #[strum(to_string = "Gist ID is missing, provide it as an argument or in the config file")]
    GistMissingId,
    /// The provided Gist identifier (ID or URL) is malformed or invalid
    #[strum(to_string = "The provided gist is not valid, please provide a valid id or URL")]
    GistInvalidLocation,
    /// The specified file within the target GitHub Gist could not be found
    #[strum(to_string = "File not found within the specified Gist")]
    GistFileNotFound,
    /// A request to the GitHub Gist API has failed
    #[strum(to_string = "Gist request failed: {0}")]
    GistRequestFailed(String),
    /// The user's home directory could not be determined, preventing access to shell history
    #[strum(to_string = "Could not determine home directory")]
    HistoryHomeDirNotFound,
    /// The history file for the specified shell could not be found
    #[strum(to_string = "History file not found at: {0}")]
    HistoryFileNotFound(String),
    /// The `nu` command is required for importing history but was not found in the system's PATH
    #[strum(to_string = "Nushell not found, make sure it is installed and in your PATH")]
    HistoryNushellNotFound,
    /// The `nu` command failed to execute
    #[strum(to_string = "Error running nu, maybe it is an old version")]
    HistoryNushellFailed,
    /// The `atuin` command is required for importing history but was not found in the system's PATH
    #[strum(to_string = "Atuin not found, make sure it is installed and in your PATH")]
    HistoryAtuinNotFound,
    /// The `atuin` command failed to execute
    #[strum(to_string = "Error running atuin, maybe it is an old version")]
    HistoryAtuinFailed,
    /// An AI-related feature was used, but AI is not enabled in the configuration
    #[strum(to_string = "AI feature is disabled, enable it in the config file to use this functionality")]
    AiRequired,
    /// The command is missing or empty
    #[strum(to_string = "A command must be provided")]
    AiEmptyCommand,
    /// The API key for the AI service is either missing, invalid, or lacks necessary permissions
    #[strum(to_string = "API key in '{0}' env variable is missing, invalid, or lacks permissions")]
    AiMissingOrInvalidApiKey(String),
    /// The request to the AI provider timed out while waiting for a response
    #[strum(to_string = "Request to AI provider timed out")]
    AiRequestTimeout,
    /// Service unavailable when calling AI provider
    #[strum(to_string = "AI provider responded with status 503 Service Unavailable")]
    AiUnavailable,
    /// A generic error occurred while making a request to the AI provider's API
    #[strum(to_string = "AI request failed: {0}")]
    AiRequestFailed(String),
    /// The request was rejected by the AI provider due to rate limiting
    #[strum(to_string = "AI request rate-limited, try again later")]
    AiRateLimit,
    /// An HTTP request to fetch the latest version has failed
    #[strum(to_string = "Couldn't check for latest version: {0}")]
    LatestVersionRequestFailed(String),
}

impl AppError {
    /// Converts this error into a [Report]
    pub fn into_report(self) -> Report {
        match self {
            AppError::UserFacing(err) => Report::msg(err),
            AppError::Unexpected(report) => report,
        }
    }
}
impl From<UserFacingError> for AppError {
    fn from(err: UserFacingError) -> Self {
        Self::UserFacing(err)
    }
}
impl<T: Into<Report>> From<T> for AppError {
    fn from(err: T) -> Self {
        Self::Unexpected(err.into())
    }
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
    (target: $target:expr, level: $level:expr, $ex:expr) => {
        {
            match $ex {
                value => {
                    tracing::event!(target: $target, $level, ?value, stringify!($ex));
                    value
                }
            }
        }
    };
    (level: $level:expr, $ex:expr) => {
        $crate::trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        $crate::trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        $crate::trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}
