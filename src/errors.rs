use std::{
    env,
    panic::{self, UnwindSafe},
    path::PathBuf,
    process,
};

use color_eyre::{Report, Result, Section, config::HookBuilder, owo_colors::style};
use futures_util::FutureExt;
use tokio::sync::mpsc;

/// Initializes error and panics handling
pub async fn init<F>(log_path: Option<PathBuf>, fut: F) -> Result<()>
where
    F: Future<Output = Result<()>> + UnwindSafe,
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

/// Error type for command searching operations
#[derive(Debug)]
pub enum SearchError {
    /// The provided regex is not valid
    InvalidRegex(regex::Error),
    /// The provided fuzzy search hasn't any term
    InvalidFuzzy,
    /// An unexpected error occurred
    Unexpected(Report),
}

/// Error type for add operations
#[derive(Debug)]
pub enum InsertError {
    /// The entity content is not valid
    Invalid(&'static str),
    /// The entity already exists
    AlreadyExists,
    /// An unexpected error occurred
    Unexpected(Report),
}

/// Error type for update operations
#[derive(Debug)]
pub enum UpdateError {
    /// The entity content is not valid
    Invalid(&'static str),
    /// The entity already exists
    AlreadyExists,
    /// An unexpected error occurred
    Unexpected(Report),
}

/// Error type for commands import/export
#[derive(Debug)]
pub enum ImportExportError {
    /// The provided path points to a directory or symlink, not a file
    NotAFile,
    /// The file could not be found at the given path
    FileNotFound,
    /// The application lacks the necessary permissions to read or write the file
    FileNotAccessible,
    /// Content couldn't be written to the file: broken pipe
    FileBrokenPipe,
    /// The provided HTTP URL is malformed or invalid
    HttpInvalidUrl,
    /// The request to the HTTP URL failed
    HttpRequestFailed(String),
    /// A gist id was not provided via arguments or the configuration file
    GistMissingId,
    /// The provided gist location is malformed or invalid
    GistInvalidLocation,
    /// The provided gist location has a sha version, which is immutable
    GistLocationHasSha,
    /// The provided gist file was not found
    GistFileNotFound,
    /// A gh token is required to export commands to a gist
    GistMissingToken,
    /// The request to the gist API failed
    GistRequestFailed(String),
    /// An unexpected error occurred
    Unexpected(Report),
}

impl UpdateError {
    pub fn into_report(self) -> Report {
        match self {
            UpdateError::Invalid(msg) => Report::msg(msg),
            UpdateError::AlreadyExists => Report::msg("Entity already exists"),
            UpdateError::Unexpected(report) => report,
        }
    }
}

macro_rules! impl_from_report {
    ($err:ty) => {
        impl<T> From<T> for $err
        where
            T: Into<Report>,
        {
            fn from(err: T) -> Self {
                Self::Unexpected(err.into())
            }
        }
    };
}
impl_from_report!(SearchError);
impl_from_report!(InsertError);
impl_from_report!(UpdateError);
impl_from_report!(ImportExportError);

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
        trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}
