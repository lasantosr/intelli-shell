use clap::{Args, FromArgMatches};

use crate::{component::Component, config::Config, service::IntelliShellService};

pub mod export;
pub mod fix;
pub mod import;
pub mod new;
pub mod replace;
pub mod search;
pub mod tldr_clear;
pub mod tldr_fetch;

#[cfg(debug_assertions)]
pub mod query;

/// Represents the final outcome of a [`Process`] execution.
///
/// This enum determines the action the shell should take after a command has been processed.
/// It can either lead to the execution of a new shell command or result in exiting the process with specific output
/// information.
pub enum ProcessOutput {
    /// Instructs the shell to execute the specified command
    Execute { cmd: String },
    /// Instructs the shell to terminate and output the provided information
    Output(OutputInfo),
}

/// Holds the information to be written to output streams upon process termination.
///
/// This structure defines what should be sent to standard output (stdout), standard error (stderr), and/or a dedicated
/// file, along with a status indicating if the operation itself succeeded or failed.
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct OutputInfo {
    /// Indicates whether the operation that generated this output bundle was considered a failure
    pub failed: bool,

    /// Content to be written to a specified output file.
    ///
    /// When this is `Some`, it typically takes precedence over `stdout`. This is useful for redirecting the main
    /// output of a command to a file, for instance, via an `--output` flag.
    pub fileout: Option<String>,

    /// Content to be written to the standard output (stdout) stream.
    ///
    /// This is generally used when no file output is specified.
    pub stdout: Option<String>,

    /// Content to be written to the standard error (stderr) stream.
    ///
    /// Used for error messages or diagnostic information.
    pub stderr: Option<String>,
}

impl ProcessOutput {
    /// Creates a [`ProcessOutput::Execute`] variant to run a shell command
    pub fn execute(cmd: impl Into<String>) -> Self {
        Self::Execute { cmd: cmd.into() }
    }

    /// Creates a successful [`ProcessOutput::Output`] with no content and succesful exit code
    pub fn success() -> Self {
        Self::Output(OutputInfo {
            failed: false,
            ..Default::default()
        })
    }

    /// Creates a failed [`ProcessOutput::Output`] with no content and failure exit code
    pub fn fail() -> Self {
        Self::Output(OutputInfo {
            failed: true,
            ..Default::default()
        })
    }

    /// Sets the file output content for the [`ProcessOutput::Output`] variant.
    ///
    /// Note: This has no effect if the instance is a `ProcessOutput::Execute` variant.
    pub fn fileout(self, fileout: impl Into<String>) -> Self {
        match self {
            e @ ProcessOutput::Execute { .. } => e,
            ProcessOutput::Output(data) => ProcessOutput::Output(OutputInfo {
                fileout: Some(fileout.into()),
                ..data
            }),
        }
    }

    /// Sets the standard output content for the [`ProcessOutput::Output`] variant.
    ///
    /// Note: This has no effect if the instance is a `ProcessOutput::Execute` variant.
    pub fn stdout(self, stdout: impl Into<String>) -> Self {
        match self {
            e @ ProcessOutput::Execute { .. } => e,
            ProcessOutput::Output(data) => ProcessOutput::Output(OutputInfo {
                stdout: Some(stdout.into()),
                ..data
            }),
        }
    }

    /// Sets the standard error content for the [`ProcessOutput::Output`] variant.
    ///
    /// Note: This has no effect if the instance is a `ProcessOutput::Execute` variant.
    pub fn stderr(self, stderr: impl Into<String>) -> Self {
        match self {
            e @ ProcessOutput::Execute { .. } => e,
            ProcessOutput::Output(data) => ProcessOutput::Output(OutputInfo {
                stderr: Some(stderr.into()),
                ..data
            }),
        }
    }
}

/// Trait for non-interactive processes
#[trait_variant::make(Send)]
pub trait Process {
    /// Executes the process non-interactively and returns the output
    async fn execute(self, config: Config, service: IntelliShellService) -> color_eyre::Result<ProcessOutput>;
}

/// Trait for interactive processes
pub trait InteractiveProcess: Process + FromArgMatches + Args {
    /// Converts the process into a renderable component
    fn into_component(
        self,
        config: Config,
        service: IntelliShellService,
        inline: bool,
    ) -> color_eyre::Result<Box<dyn Component>>;
}
