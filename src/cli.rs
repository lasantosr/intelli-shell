use std::str::FromStr;

use clap::{
    Args, Command, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum,
    builder::{ValueParser, styling::Style},
};
use clap_stdin::{FileOrStdin, MaybeStdin};
use color_eyre::{Result, eyre::eyre};
use itertools::Itertools;
use regex::Regex;
use reqwest::{
    Method,
    header::{HeaderName, HeaderValue},
};
use tracing::instrument;

use crate::model::SearchMode;

/// Like IntelliSense, but for shells
///
/// Interactive commands are best used with the default shell bindings:
/// - `ctrl+space` to search for commands
/// - `ctrl+b` to bookmark a new command
/// - `ctrl+l` to replace variables from a command
/// - `ctrl+x` to fix a command that is failing
#[derive(Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(
    author,
    version,
    verbatim_doc_comment,
    infer_subcommands = true,
    subcommand_required = true,
    after_long_help = include_str!("_examples/cli.txt")
)]
pub struct Cli {
    /// Whether to skip the execution of the command
    ///
    /// Primarily used by shell integrations capable of running the command themselves
    #[arg(long, hide = true)]
    pub skip_execution: bool,

    /// Whether to add an extra line when rendering the inline TUI
    ///
    /// Primarily used by shell integrations (e.g., Readline keybindings) to skip and preserve the shell prompt
    #[arg(long, hide = true)]
    pub extra_line: bool,

    /// Path of the file to write the final textual output to (defaults to stdout)
    ///
    /// Primarily used by shell integrations (e.g., Readline/PSReadLine keybindings) to capture the result of an
    /// interactive TUI session
    #[arg(long, hide = true)]
    pub file_output: Option<String>,

    /// Command to be executed
    #[command(name = "command", subcommand)]
    pub process: CliProcess,
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum CliProcess {
    #[cfg(debug_assertions)]
    /// (debug) Runs an sql query against the database
    Query(QueryProcess),

    /// Generates the shell integration script
    #[command(after_long_help = include_str!("_examples/init.txt"))]
    Init(InitProcess),

    /// Bookmarks a new command
    #[command(after_long_help = include_str!("_examples/new.txt"))]
    New(Interactive<BookmarkCommandProcess>),

    /// Search stored commands
    #[command(after_long_help = include_str!("_examples/search.txt"))]
    Search(Interactive<SearchCommandsProcess>),

    /// Replace the variables of a command
    ///
    /// Anything enclosed in double brackets is considered a variable: echo {{message}}
    ///
    /// This command also supports an alternative <variable> syntax, to improve compatibility
    #[command(after_long_help = include_str!("_examples/replace.txt"))]
    Replace(Interactive<VariableReplaceProcess>),

    /// Fix a command that is failing
    ///
    /// The command will be run in order to capture its output and exit code, only non-interactive commands are
    /// supported
    #[command(after_long_help = include_str!("_examples/fix.txt"))]
    Fix(CommandFixProcess),

    /// Exports stored user commands and completions to an external location
    ///
    /// Commands fetched from tldr are not exported
    #[command(after_long_help = include_str!("_examples/export.txt"))]
    Export(Interactive<ExportItemsProcess>),

    /// Imports user commands and completions from an external location
    #[command(after_long_help = include_str!("_examples/import.txt"))]
    Import(Interactive<ImportItemsProcess>),

    /// Manages tldr integration
    #[command(name = "tldr", subcommand)]
    Tldr(TldrProcess),

    /// Manages dynamic completions for variables
    #[command(subcommand)]
    Completion(CompletionProcess),

    /// Updates intelli-shell to the latest version if possible, or shows update instructions
    Update(UpdateProcess),
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TldrProcess {
    /// Fetches command examples from tldr pages and imports them
    ///
    /// Imported commands will reside on a different category and can be excluded when querying
    #[command(after_long_help = include_str!("_examples/tldr_fetch.txt"))]
    Fetch(TldrFetchProcess),

    /// Clear command examples imported from tldr pages
    #[command(after_long_help = include_str!("_examples/tldr_clear.txt"))]
    Clear(TldrClearProcess),
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum CompletionProcess {
    /// Adds a new dynamic completion for a variable
    #[command(after_long_help = include_str!("_examples/completion_new.txt"))]
    New(Interactive<CompletionNewProcess>),
    /// Deletes an existing dynamic variable completions
    #[command(after_long_help = include_str!("_examples/completion_delete.txt"))]
    Delete(CompletionDeleteProcess),
    /// Lists all configured dynamic variable completions
    #[command(alias = "ls", after_long_help = include_str!("_examples/completion_list.txt"))]
    List(Interactive<CompletionListProcess>),
}

/// A generic struct that combines process-specific arguments with common interactive mode options.
///
/// This struct is used to wrap processes that can be run in both interactive and non-interactive modes.
#[derive(Args, Debug)]
pub struct Interactive<T: FromArgMatches + Args> {
    /// Options for the process
    #[command(flatten)]
    pub process: T,

    /// Options for interactive display mode
    #[command(flatten)]
    pub opts: InteractiveOptions,
}

/// Options common to interactive processes
#[derive(Args, Debug)]
pub struct InteractiveOptions {
    /// Open an interactive interface
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Force the interactive interface to render inline (takes less space)
    #[arg(short = 'l', long, requires = "interactive", conflicts_with = "full_screen")]
    pub inline: bool,

    /// Force the interactive interface to render in full screen
    #[arg(short = 'f', long, requires = "interactive", conflicts_with = "inline")]
    pub full_screen: bool,
}

#[cfg(debug_assertions)]
/// Runs an SQL query against the database
#[derive(Args, Debug)]
pub struct QueryProcess {
    /// The query to run (reads from stdin if '-')
    #[arg(default_value = "-")]
    pub sql: FileOrStdin,
}

/// Generates the integration shell script
#[derive(Args, Debug)]
pub struct InitProcess {
    /// The shell to generate the script for
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(ValueEnum, Copy, Clone, PartialEq, Eq, Debug)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

/// Bookmarks a new command
#[derive(Args, Debug)]
pub struct BookmarkCommandProcess {
    /// Command to be stored (mandatory when non-interactive)
    ///
    /// Take into consideration shell expansion and quote any special character intended to be stored
    #[arg(required_unless_present = "interactive")]
    pub command: Option<String>,

    /// Alias for the command
    #[arg(short = 'a', long)]
    pub alias: Option<String>,

    /// Description of the command
    #[arg(short = 'd', long)]
    pub description: Option<String>,

    /// Use AI to suggest the command and description
    #[arg(long)]
    pub ai: bool,
}

/// Search stored commands
#[derive(Args, Debug)]
pub struct SearchCommandsProcess {
    /// Initial search query to filter commands
    pub query: Option<String>,

    /// Search mode, overwriting the default one on the config
    #[arg(short = 'm', long)]
    pub mode: Option<SearchMode>,

    /// Whether to search for user commands only (ignoring tldr), overwriting the config
    #[arg(short = 'u', long)]
    pub user_only: bool,

    /// Use AI to suggest commands instead of searching for them on the database
    #[arg(long, requires = "query")]
    pub ai: bool,
}

/// Replace the variables of a command
#[derive(Args, Debug)]
pub struct VariableReplaceProcess {
    /// Command to replace variables from (reads from stdin if '-')
    ///
    /// Take into consideration shell expansion and quote any special character that must be kept
    #[arg(default_value = "-")]
    pub command: MaybeStdin<String>,

    /// Values for the variables, can be specified multiple times
    ///
    /// If only `KEY` is given (e.g., `--env api-token`), its value is read from the `API_TOKEN` environment variable
    #[arg(short = 'e', long = "env", value_name = "KEY[=VALUE]", value_parser = ValueParser::new(parse_env_var))]
    pub values: Vec<(String, Option<String>)>,

    /// Automatically populates remaining unspecified variables from environment variables
    ///
    /// Unlike `--env` this flag will provide access to any environment variable, not only those explicitly listed
    ///
    /// Variable names are converted to SCREAMING_SNAKE_CASE to find matching variables (e.g., `{{http-header}}` checks
    /// env var `HTTP_HEADER`)
    ///
    /// When run in interactive mode, env variables will be always suggested if found
    #[arg(short = 'E', long)]
    pub use_env: bool,
}

/// Fix a command that is failing
#[derive(Args, Debug)]
pub struct CommandFixProcess {
    /// The non-interactive failing command
    pub command: String,

    /// Recent history from the shell, to be used as additional context in the prompt
    ///
    /// It has to contain recent commands, separated by a newline, from oldest to newest
    #[arg(long, value_name = "HISTORY")]
    pub history: Option<String>,
}

/// Exports stored user commands and completions
#[derive(Args, Clone, Debug)]
pub struct ExportItemsProcess {
    /// Location to export items to (writes to stdout if '-')
    ///
    /// The location type will be auto detected based on the content, if no type is specified
    #[arg(default_value = "-")]
    pub location: String,
    /// Treat the location as a file path
    #[arg(long, group = "location_type")]
    pub file: bool,
    /// Treat the location as a generic http(s) URL
    #[arg(long, group = "location_type")]
    pub http: bool,
    /// Treat the location as a GitHub Gist URL or ID
    #[arg(long, group = "location_type")]
    pub gist: bool,
    /// Export commands matching the given regular expression only
    ///
    /// The regular expression will be checked against both the command and the description
    #[arg(long, value_name = "REGEX")]
    pub filter: Option<Regex>,
    /// Custom headers to include in the request
    ///
    /// This argument can be specified multiple times to add more than one header, but it will be only used for HTTP
    /// locations
    #[arg(short = 'H', long = "header", value_name = "KEY: VALUE", value_parser = ValueParser::new(parse_header))]
    pub headers: Vec<(HeaderName, HeaderValue)>,
    /// HTTP method to use for the request
    ///
    /// It will be only used for HTTP locations
    #[arg(short = 'X', long = "request", value_enum, default_value_t = HttpMethod::PUT)]
    pub method: HttpMethod,
}

/// Imports user commands and completions
#[derive(Args, Clone, Debug)]
pub struct ImportItemsProcess {
    /// Location to import items from (reads from stdin if '-')
    ///
    /// The location type will be auto detected based on the content, if no type is specified
    #[arg(default_value = "-", required_unless_present = "history")]
    pub location: String,
    /// Use AI to parse and extract commands
    #[arg(long)]
    pub ai: bool,
    /// Do not import the commands, just output them
    ///
    /// This is useful when we're not sure about the format of the location we're importing
    #[arg(long)]
    pub dry_run: bool,
    /// Treat the location as a file path
    #[arg(long, group = "location_type")]
    pub file: bool,
    /// Treat the location as a generic http(s) URL
    #[arg(long, group = "location_type")]
    pub http: bool,
    /// Treat the location as a GitHub Gist URL or ID
    #[arg(long, group = "location_type")]
    pub gist: bool,
    /// Treat the location as a shell history (requires --ai)
    #[arg(long, value_enum, group = "location_type", requires = "ai")]
    pub history: Option<HistorySource>,
    /// Import commands matching the given regular expression only
    ///
    /// The regular expression will be checked against both the command and the description
    #[arg(long, value_name = "REGEX")]
    pub filter: Option<Regex>,
    /// Add hashtags to imported commands
    ///
    /// This argument can be specified multiple times to add more than one, hashtags will be included at the end of the
    /// description
    #[arg(short = 't', long = "add-tag", value_name = "TAG")]
    pub tags: Vec<String>,
    /// Custom headers to include in the request
    ///
    /// This argument can be specified multiple times to add more than one header, but it will be only used for http
    /// locations
    #[arg(short = 'H', long = "header", value_name = "KEY: VALUE", value_parser = ValueParser::new(parse_header))]
    pub headers: Vec<(HeaderName, HeaderValue)>,
    /// HTTP method to use for the request
    ///
    /// It will be only used for http locations
    #[arg(short = 'X', long = "request", value_enum, default_value_t = HttpMethod::GET)]
    pub method: HttpMethod,
}

#[derive(ValueEnum, Copy, Clone, PartialEq, Eq, Debug)]
pub enum HistorySource {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Atuin,
}

#[derive(ValueEnum, Copy, Clone, PartialEq, Eq, Debug)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
}
impl From<HttpMethod> for Method {
    fn from(value: HttpMethod) -> Self {
        match value {
            HttpMethod::GET => Method::GET,
            HttpMethod::POST => Method::POST,
            HttpMethod::PUT => Method::PUT,
            HttpMethod::PATCH => Method::PATCH,
        }
    }
}

/// Fetches command examples from tldr pages and imports them
#[derive(Args, Debug)]
pub struct TldrFetchProcess {
    /// Category to fetch, skip to fetch for current platform (e.g., `common`, `linux`, `osx`, `windows`)
    ///
    /// For a full list of available categories, see: https://github.com/tldr-pages/tldr/tree/main/pages
    pub category: Option<String>,

    /// Fetches examples only for the specified command(s) (e.g., `git`, `docker`, `tar`)
    ///
    /// Command names should match their corresponding filenames (without the `.md` extension)
    /// as found in the tldr pages repository
    #[arg(short = 'c', long = "command", value_name = "COMMAND_NAME")]
    pub commands: Vec<String>,

    /// Fetches examples only for the command(s) from the file specified (reads from stdin if '-')
    ///
    /// The file or stdin must contain the command names as found in the tldr pages repository separated by newlines
    #[arg(short = 'C', long, value_name = "FILE_OR_STDIN", num_args = 0..=1, default_missing_value = "-")]
    pub filter_commands: Option<FileOrStdin>,
}

/// Clear command examples from tldr pages
#[derive(Args, Debug)]
pub struct TldrClearProcess {
    /// Category to clear, skip to clear all categories
    ///
    /// For a full list of available categories, see: https://github.com/tldr-pages/tldr/tree/main/pages
    pub category: Option<String>,
}

/// Adds a new dynamic completion for a variable
#[derive(Args, Debug)]
pub struct CompletionNewProcess {
    /// The root command where this completion must be triggered
    #[arg(short = 'c', long)]
    pub command: Option<String>,
    /// The name of the variable to provide completions for
    #[arg(required_unless_present = "interactive")]
    pub variable: Option<String>,
    /// The shell command that generates the suggestion values when executed (newline-separated)
    #[arg(required_unless_present_any = ["interactive", "ai"])]
    pub provider: Option<String>,
    /// Use AI to suggest the completion command
    #[arg(long)]
    pub ai: bool,
}

/// Deletes an existing variable dynamic completion
#[derive(Args, Debug)]
pub struct CompletionDeleteProcess {
    /// The root command of the completion to delete
    #[arg(short = 'c', long)]
    pub command: Option<String>,
    /// The variable name of the completion to delete
    pub variable: String,
}

/// Lists all configured variable dynamic completions
#[derive(Args, Debug)]
pub struct CompletionListProcess {
    /// The root command to filter the list of completions by
    pub command: Option<String>,
}

#[derive(Args, Debug)]
pub struct UpdateProcess {}

impl Cli {
    /// Parses the [Cli] command, with any runtime extension required
    #[instrument]
    pub fn parse_extended() -> Self {
        // Command definition
        let mut cmd = Self::command_for_update();

        // Update after_long_help to match the style, if present
        let style = cmd.get_styles().clone();
        let dimmed = style.get_placeholder().dimmed();
        let plain_examples_header = "Examples:";
        let styled_examples_header = format!(
            "{}Examples:{}",
            style.get_usage().render(),
            style.get_usage().render_reset()
        );
        style_after_long_help(&mut cmd, &dimmed, plain_examples_header, &styled_examples_header);

        // Parse the arguments
        let matches = cmd.get_matches();

        // Convert the argument matches back into the strongly typed `Cli` struct
        match Cli::from_arg_matches(&matches) {
            Ok(args) => args,
            Err(err) => err.exit(),
        }
    }
}

fn style_after_long_help(
    command_ref: &mut Command,
    dimmed: &Style,
    plain_examples_header: &str,
    styled_examples_header: &str,
) {
    let mut command = std::mem::take(command_ref);
    if let Some(after_long_help) = command.get_after_long_help() {
        let current_help_text = after_long_help.to_string();
        let modified_help_text = current_help_text
            // Replace the examples header to match the same usage style
            .replace(plain_examples_header, styled_examples_header)
            // Style the comment lines to be dimmed
            .lines()
            .map(|line| {
                if line.trim_start().starts_with('#') {
                    format!("{}{}{}", dimmed.render(), line, dimmed.render_reset())
                } else {
                    line.to_string()
                }
            }).join("\n");
        command = command.after_long_help(modified_help_text);
    }
    for subcommand_ref in command.get_subcommands_mut() {
        style_after_long_help(subcommand_ref, dimmed, plain_examples_header, styled_examples_header);
    }
    *command_ref = command;
}

fn parse_env_var(env: &str) -> Result<(String, Option<String>)> {
    if let Some((var, value)) = env.split_once('=') {
        Ok((var.to_owned(), Some(value.to_owned())))
    } else {
        Ok((env.to_owned(), None))
    }
}

fn parse_header(env: &str) -> Result<(HeaderName, HeaderValue)> {
    if let Some((name, value)) = env.split_once(':') {
        Ok((HeaderName::from_str(name)?, HeaderValue::from_str(value.trim_start())?))
    } else {
        Err(eyre!("Missing a colon between the header name and value"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_asserts() {
        Cli::command().debug_assert()
    }
}
