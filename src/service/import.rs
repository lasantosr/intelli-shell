use std::{
    io::{Cursor, ErrorKind},
    sync::LazyLock,
};

use color_eyre::{Report, eyre::Context};
use futures_util::{TryStreamExt, stream};
use itertools::Itertools;
use regex::Regex;
use reqwest::{
    Url,
    header::{self, HeaderName, HeaderValue},
};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncRead, BufReader, Lines},
};
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    cli::{HistorySource, HttpMethod, ImportItemsProcess},
    config::GistConfig,
    errors::{AppError, Result, UserFacingError},
    model::{
        CATEGORY_USER, Command, ImportExportItem, ImportExportStream, ImportStats, SOURCE_IMPORT, VariableCompletion,
    },
    utils::{
        add_tags_to_description, convert_alt_to_regular,
        dto::{GIST_README_FILENAME, GIST_README_FILENAME_UPPER, GistDto, ImportExportItemDto},
        extract_gist_data, github_to_raw, read_history,
    },
};

impl IntelliShellService {
    /// Import commands and completions
    pub async fn import_items(&self, items: ImportExportStream, overwrite: bool) -> Result<ImportStats> {
        self.storage.import_items(items, overwrite, false).await
    }

    /// Returns a list of items to import from a location
    pub async fn get_items_from_location(
        &self,
        args: ImportItemsProcess,
        gist_config: GistConfig,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        let ImportItemsProcess {
            location,
            file,
            http,
            gist,
            history,
            ai,
            filter,
            dry_run: _,
            tags,
            headers,
            method,
        } = args;

        // Make sure the tags starts with a hashtag (#)
        let tags = tags
            .into_iter()
            .filter_map(|mut tag| {
                tag.chars().next().map(|first_char| {
                    if first_char == '#' {
                        tag
                    } else {
                        tag.insert(0, '#');
                        tag
                    }
                })
            })
            .collect::<Vec<_>>();

        // Retrieve the commands from the location
        let commands = if let Some(history) = history {
            self.get_history_items(history, filter, tags, ai, cancellation_token)
                .await?
        } else if file {
            if location == "-" {
                self.get_stdin_items(filter, tags, ai, cancellation_token).await?
            } else {
                self.get_file_items(location, filter, tags, ai, cancellation_token)
                    .await?
            }
        } else if http {
            self.get_http_items(location, headers, method, filter, tags, ai, cancellation_token)
                .await?
        } else if gist {
            self.get_gist_items(location, gist_config, filter, tags, ai, cancellation_token)
                .await?
        } else {
            // Determine which mode based on the location
            if location == "gist"
                || location.starts_with("https://gist.github.com")
                || location.starts_with("https://api.github.com/gists")
            {
                self.get_gist_items(location, gist_config, filter, tags, ai, cancellation_token)
                    .await?
            } else if location.starts_with("http://") || location.starts_with("https://") {
                self.get_http_items(location, headers, method, filter, tags, ai, cancellation_token)
                    .await?
            } else if location == "-" {
                self.get_stdin_items(filter, tags, ai, cancellation_token).await?
            } else {
                self.get_file_items(location, filter, tags, ai, cancellation_token)
                    .await?
            }
        };

        Ok(commands)
    }

    #[instrument(skip_all)]
    async fn get_history_items(
        &self,
        history: HistorySource,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        if let Some(ref filter) = filter {
            tracing::info!(ai, "Importing commands matching `{filter}` from {history:?} history");
        } else {
            tracing::info!(ai, "Importing commands from {history:?} history");
        }
        let content = Cursor::new(read_history(history)?);
        self.extract_and_filter_items(content, filter, tags, ai, cancellation_token)
            .await
    }

    #[instrument(skip_all)]
    async fn get_stdin_items(
        &self,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        if let Some(ref filter) = filter {
            tracing::info!(ai, "Importing commands matching `{filter}` from stdin");
        } else {
            tracing::info!(ai, "Importing commands from stdin");
        }
        let content = tokio::io::stdin();
        self.extract_and_filter_items(content, filter, tags, ai, cancellation_token)
            .await
    }

    #[instrument(skip_all)]
    async fn get_file_items(
        &self,
        path: String,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        // Otherwise, check the path to import the file
        match fs::metadata(&path).await {
            Ok(m) if m.is_file() => (),
            Ok(_) => return Err(UserFacingError::ImportLocationNotAFile.into()),
            Err(err) if err.kind() == ErrorKind::NotFound => return Err(UserFacingError::ImportFileNotFound.into()),
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                return Err(UserFacingError::FileNotAccessible("read").into());
            }
            Err(err) => return Err(Report::from(err).into()),
        }
        if let Some(ref filter) = filter {
            tracing::info!(ai, "Importing commands matching `{filter}` from file: {path}");
        } else {
            tracing::info!(ai, "Importing commands from file: {path}");
        }
        let content = File::open(path).await.wrap_err("Couldn't open the file")?;
        self.extract_and_filter_items(content, filter, tags, ai, cancellation_token)
            .await
    }

    #[instrument(skip_all)]
    async fn get_http_items(
        &self,
        mut url: String,
        headers: Vec<(HeaderName, HeaderValue)>,
        method: HttpMethod,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        // If the URL is the stdin placeholder, read a line from it
        if url == "-" {
            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer)?;
            url = buffer.trim_end_matches("\n").to_string();
            tracing::debug!("Read url from stdin: {url}");
        }

        // Parse the URL
        let mut url = Url::parse(&url).map_err(|err| {
            tracing::error!("Couldn't parse url: {err}");
            UserFacingError::HttpInvalidUrl
        })?;

        // Try to convert github regular urls to raw
        if let Some(raw_url) = github_to_raw(&url) {
            url = raw_url;
        }

        let method = method.into();
        if let Some(ref filter) = filter {
            tracing::info!(ai, "Importing commands matching `{filter}` from http: {method} {url}");
        } else {
            tracing::info!(ai, "Importing commands from http: {method} {url}");
        }

        // Build the request
        let client = reqwest::Client::new();
        let mut req = client.request(method, url);

        // Add headers
        for (name, value) in headers {
            tracing::debug!("Appending '{name}' header");
            req = req.header(name, value);
        }

        // Send the request
        let res = req.send().await.map_err(|err| {
            tracing::error!("{err:?}");
            UserFacingError::HttpRequestFailed(err.to_string())
        })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(
                    UserFacingError::HttpRequestFailed(format!("received {status_str} {reason} response")).into(),
                );
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(UserFacingError::HttpRequestFailed(format!("received {status_str} response")).into());
            }
        }

        // Check the response content type
        let mut json = false;
        if let Some(content_type) = res.headers().get(header::CONTENT_TYPE) {
            let Ok(content_type) = content_type.to_str() else {
                return Err(
                    UserFacingError::HttpRequestFailed(String::from("couldn't read content-type header")).into(),
                );
            };
            if content_type.starts_with("application/json") {
                json = true;
            } else if !content_type.starts_with("text") {
                return Err(
                    UserFacingError::HttpRequestFailed(format!("unsupported content-type: {content_type}")).into(),
                );
            }
        }

        if json {
            // Parse the body as a list of commands
            let items: Vec<ImportExportItemDto> = match res.json().await {
                Ok(b) => b,
                Err(err) if err.is_decode() => {
                    tracing::error!("Couldn't parse api response: {err}");
                    return Err(UserFacingError::GistRequestFailed(String::from("couldn't parse api response")).into());
                }
                Err(err) => {
                    tracing::error!("{err:?}");
                    return Err(UserFacingError::GistRequestFailed(err.to_string()).into());
                }
            };

            Ok(Box::pin(stream::iter(
                items.into_iter().map(ImportExportItem::from).map(Ok),
            )))
        } else {
            let content = Cursor::new(res.text().await.map_err(|err| {
                tracing::error!("Couldn't read api response: {err}");
                UserFacingError::HttpRequestFailed(String::from("couldn't read api response"))
            })?);
            self.extract_and_filter_items(content, filter, tags, ai, cancellation_token)
                .await
        }
    }

    #[instrument(skip_all)]
    async fn get_gist_items(
        &self,
        mut gist: String,
        gist_config: GistConfig,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        // If the gist is the stdin placeholder, read a line from it
        if gist == "-" {
            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer)?;
            gist = buffer.trim_end_matches("\n").to_string();
            tracing::debug!("Read gist from stdin: {gist}");
        }

        // For raw gists, import as regular http requests
        if gist.starts_with("https://gist.githubusercontent.com") {
            return self
                .get_http_items(gist, Vec::new(), HttpMethod::GET, filter, tags, ai, cancellation_token)
                .await;
        }

        // Retrieve the gist id and optional sha and file
        let (gist_id, gist_sha, gist_file) = extract_gist_data(&gist, &gist_config)?;

        // Determine the URL based on the presence of sha
        let url = if let Some(sha) = gist_sha {
            format!("https://api.github.com/gists/{gist_id}/{sha}")
        } else {
            format!("https://api.github.com/gists/{gist_id}")
        };

        if let Some(ref filter) = filter {
            tracing::info!(ai, "Importing commands matching `{filter}` from gist: {url}");
        } else {
            tracing::info!(ai, "Importing commands from gist: {url}");
        }

        // Call the API
        let client = reqwest::Client::new();
        let res = client
            .get(url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(header::USER_AGENT, "intelli-shell")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|err| {
                tracing::error!("{err:?}");
                UserFacingError::GistRequestFailed(err.to_string())
            })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(
                    UserFacingError::GistRequestFailed(format!("received {status_str} {reason} response")).into(),
                );
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(UserFacingError::GistRequestFailed(format!("received {status_str} response")).into());
            }
        }

        // Parse the body as a json
        let mut body: GistDto = match res.json().await {
            Ok(b) => b,
            Err(err) if err.is_decode() => {
                tracing::error!("Couldn't parse api response: {err}");
                return Err(UserFacingError::GistRequestFailed(String::from("couldn't parse api response")).into());
            }
            Err(err) => {
                tracing::error!("{err:?}");
                return Err(UserFacingError::GistRequestFailed(err.to_string()).into());
            }
        };

        let full_content = if let Some(ref gist_file) = gist_file {
            // If there's a file specified, import just it
            body.files
                .remove(gist_file)
                .ok_or(UserFacingError::GistFileNotFound)?
                .content
        } else {
            // Otherwise import all of the files (except the readme)
            body.files
                .into_iter()
                .filter(|(k, _)| k != GIST_README_FILENAME && k != GIST_README_FILENAME_UPPER)
                .map(|(_, f)| f.content)
                .join("\n")
        };

        let content = Cursor::new(full_content);
        self.extract_and_filter_items(content, filter, tags, ai, cancellation_token)
            .await
    }

    /// Extract the commands from the given content, prompting ai or parsing it, and then filters them
    async fn extract_and_filter_items(
        &self,
        content: impl AsyncRead + Unpin + Send + 'static,
        filter: Option<Regex>,
        tags: Vec<String>,
        ai: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ImportExportStream> {
        let stream: ImportExportStream = if ai {
            let commands = self
                .prompt_commands_import(content, tags, CATEGORY_USER, SOURCE_IMPORT, cancellation_token)
                .await?;
            Box::pin(commands.map_ok(ImportExportItem::Command))
        } else {
            Box::pin(parse_import_items(content, tags, CATEGORY_USER, SOURCE_IMPORT))
        };

        if let Some(filter) = filter {
            Ok(Box::pin(stream.try_filter(move |item| {
                let pass = match item {
                    ImportExportItem::Command(c) => c.matches(&filter),
                    ImportExportItem::Completion(_) => true,
                };
                async move { pass }
            })))
        } else {
            Ok(stream)
        }
    }
}

/// Lazily parses a stream of text into a [`Stream`] of [`ImportExportItem`].
///
/// This function is the primary entry point for parsing command definitions from a file or any other async source.
/// It operates in a streaming fashion, meaning it reads the input line-by-line without loading the entire content into
/// memory, making it highly efficient for large files.
///
/// # Format Rules
///
/// The parser follows a set of rules to interpret the text content:
///
/// - **Completions**: Any line starting with `$` is treated as a completion. It must follow the format `$ (root_cmd)
///   variable: provider`.
///
/// - **Commands**: Any line that is not a blank line or a comment is treated as the start of a command.
///
/// - **Multi-line Commands**: A command can span multiple lines if a line ends with a backslash (`\`). The parser will
///   join these lines into a single command string.
///
/// - **Descriptions**: A command can have an optional description, specified in one of two ways:
///   1. **Preceding Comments**: A block of lines starting with `#`, `//`, `::` or `- ` immediately before a command
///      will be treated as its multi-line description. The comment markers are stripped and the lines are joined with
///      newlines. Empty comment lines (e.g., `# `) are preserved as blank lines within the description.
///   2. **Inline Comments** (legacy): An inline description can be provided on the same line, separated by ` ## `. If
///      both a preceding and an inline description are present, the _inline_ one takes precedence.
///
/// - **Aliases**: An optional alias can be extracted from the description by using the format `[alias:your-alias]`.
///   - The alias tag must be at the very beginning or very end of the entire description block (including multi-line
///     descriptions).
///   - The parser extracts the alias and removes it from the final description. For example, `# [alias:a] my command`
///     results in the alias `a` and the description `my command`.
///
/// - **Comments & Spacing**:
///   - Lines starting with `#`, `//`, `::`, or `- ` (ignoring leading whitespace) are treated as comments.
///   - Comment lines found _within_ a multi-line command block are ignored and do not become part of the command or its
///     description.
///   - Blank lines (i.e., empty or whitespace-only lines) act as separators for description blocks. The description for
///     a command is the comment block that immediately precedes it.
///       - A blank line between a comment block and a command is allowed and does not break the association.
///       - A blank line between two comment blocks makes them distinct; only the latter block will be considered as a
///         potential description for a subsequent command.
///
/// # Errors
///
/// The stream will yield an `Err` if an underlying I/O error occurs while reading from the `content` stream.
#[instrument(skip_all)]
pub(super) fn parse_import_items(
    content: impl AsyncRead + Unpin + Send,
    tags: Vec<String>,
    category: impl Into<String>,
    source: impl Into<String>,
) -> impl Stream<Item = Result<ImportExportItem>> + Send {
    /// The state of the parser
    struct ParserState<R: AsyncRead> {
        category: String,
        source: String,
        tags: Vec<String>,
        lines: Lines<BufReader<R>>,
        description_buffer: Vec<String>,
        description_paused: bool,
    }

    // The initial state for the stream generator
    let initial_state = ParserState {
        category: category.into(),
        source: source.into(),
        tags,
        lines: BufReader::new(content).lines(),
        description_buffer: Vec::new(),
        description_paused: false,
    };

    /// Helper to extract the comment content from a trimmed line
    fn get_comment_content(trimmed_line: &str) -> Option<&str> {
        if let Some(stripped) = trimmed_line.strip_prefix('#') {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed_line.strip_prefix("//") {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed_line.strip_prefix("- ") {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed_line.strip_prefix("::") {
            return Some(stripped.trim());
        }
        None
    }

    // Return the commands stream
    stream::unfold(initial_state, move |mut state| async move {
        loop {
            // Read the next line from the input
            let line: String = match state.lines.next_line().await {
                // A line is found
                Ok(Some(line)) => line,
                // End of the input stream, so we end our command stream
                Ok(None) => return None,
                // An I/O error occurred, yield it
                Err(err) => return Some((Err(AppError::from(err)), state)),
            };
            let trimmed_line = line.trim();

            // If the line is the shebang header, skip it
            if trimmed_line == "#!intelli-shell" {
                continue;
            }

            // Skip some line prefixes
            if trimmed_line.starts_with(">")
                || trimmed_line.starts_with("```")
                || trimmed_line.starts_with("%")
                || trimmed_line.starts_with(";")
                || trimmed_line.starts_with("@")
            {
                continue;
            }

            // If the line is a completion, parse it
            if trimmed_line.starts_with('$') {
                // Regex for completions, with an optional command part
                // It matches both `$ (cmd) var: provider` and `$ var: provider`
                static COMPLETION_RE: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(r"^\$\s*(?:\((?P<cmd>[\w-]+)\)\s*)?(?P<var>[^:|{}]+):\s*(?P<provider>.+)$").unwrap()
                });

                let item = if let Some(caps) = COMPLETION_RE.captures(trimmed_line) {
                    let cmd = caps.name("cmd").map_or("", |m| m.as_str()).trim();
                    let var = caps.name("var").map_or("", |m| m.as_str()).trim();
                    let provider = caps.name("provider").map_or("", |m| m.as_str()).trim();

                    if var.is_empty() || provider.is_empty() {
                        Err(UserFacingError::ImportCompletionInvalidFormat(line).into())
                    } else {
                        Ok(ImportExportItem::Completion(VariableCompletion::new(
                            state.source.clone(),
                            cmd,
                            var,
                            provider,
                        )))
                    }
                } else {
                    Err(UserFacingError::ImportCompletionInvalidFormat(line).into())
                };

                // In all completion cases, we reset the description buffer and yield the item
                state.description_buffer.clear();
                state.description_paused = false;
                return Some((item, state));
            }

            // If the line is a comment, accumulate it and continue to the next line
            if let Some(comment_content) = get_comment_content(trimmed_line) {
                if state.description_paused {
                    // If the description was 'paused' by a blank line, a new comment indicates a new description block
                    state.description_buffer.clear();
                }
                state.description_buffer.push(comment_content.to_string());
                state.description_paused = false;
                continue;
            }

            // If the line is blank, it might be a separator between comment blocks or trailing after a description
            if trimmed_line.is_empty() {
                // We 'pause' the description accumulation.
                if !state.description_buffer.is_empty() {
                    state.description_paused = true;
                }
                continue;
            }

            // Otherwise the line is a command that can potentially span across multiple lines
            let mut current_trimmed_line = trimmed_line.to_string();
            let mut command_parts: Vec<String> = Vec::new();
            let mut inline_description: Option<String> = None;

            // Inner loop to handle multi-line commands
            loop {
                // Before processing a line as part of a command
                if get_comment_content(&current_trimmed_line).is_some() || current_trimmed_line.is_empty() {
                    // If the line is a comment or a blank line, restart the loop with the next line
                    if let Some(next_line_res) = state.lines.next_line().await.transpose() {
                        current_trimmed_line = match next_line_res {
                            Ok(next_line) => next_line.trim().to_string(),
                            Err(err) => return Some((Err(AppError::from(err)), state)),
                        };
                        continue;
                    } else {
                        // End of stream mid-command
                        break;
                    }
                }

                // Check if theres an inline comment after the command
                let (command_segment, desc) = match current_trimmed_line.split_once(" ## ") {
                    Some((cmd, desc)) => (cmd, Some(desc.trim().to_string())),
                    None => (current_trimmed_line.as_str(), None),
                };
                if inline_description.is_none() {
                    inline_description = desc;
                }

                // If the line ends with the escape char, that means the newline was escaped
                if let Some(stripped) = command_segment.strip_suffix('\\') {
                    command_parts.push(stripped.trim().to_string());
                    // This command spans multiple lines, read the next one and continue with the loop
                    if let Some(next_line_res) = state.lines.next_line().await.transpose() {
                        current_trimmed_line = match next_line_res {
                            Ok(next_line) => next_line.trim().to_string(),
                            Err(err) => return Some((Err(AppError::from(err)), state)),
                        };
                    } else {
                        // End of stream mid-command
                        break;
                    }
                } else {
                    // This command consist of a single line, break out of the loop
                    command_parts.push(command_segment.to_string());
                    break;
                }
            }

            // Setup the cmd
            let mut full_cmd = command_parts.join(" ");
            if full_cmd.starts_with('`') && full_cmd.ends_with('`') {
                full_cmd = full_cmd[1..full_cmd.len() - 1].to_string();
            }
            full_cmd = convert_alt_to_regular(&full_cmd);
            // Setup the description
            let pre_description = if let Some(inline) = inline_description {
                inline
            } else {
                state.description_buffer.join("\n")
            };
            // Extract the alias from the description and clean it up
            let (alias, mut full_description) = extract_alias(pre_description);
            // Remove ending colon
            if let Some(stripped) = full_description.strip_suffix(':') {
                full_description = stripped.to_owned();
            }
            // Include tags if any
            if !state.tags.is_empty() {
                full_description = add_tags_to_description(&state.tags, full_description);
            }

            // Create the command
            let command = Command::new(state.category.clone(), state.source.clone(), full_cmd)
                .with_description(Some(full_description))
                .with_alias(alias);

            // Clear the buffer for the next iteration
            state.description_buffer.clear();
            state.description_paused = false;

            // Yield the command and the updated state for the next run
            return Some((Ok(ImportExportItem::Command(command)), state));
        }
    })
}

/// Extracts an alias `[alias:...]` from the start or end of a description string.
///
/// It returns a tuple containing an `Option<String>` for the alias and the cleaned description.
fn extract_alias(description: String) -> (Option<String>, String) {
    /// Regex to find an alias at the very start or very end of the string
    /// Group 2 captures the alias from the start, Group 4 from the end
    static ALIAS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)(?:\A\s*\[alias:([^\]]+)\]\s*)|(?:\s*\[alias:([^\]]+)\]\s*\z)").unwrap());

    let mut alias = None;

    // Use `replace` with a closure to capture the alias while removing the tag
    let new_description = ALIAS_RE.replace(&description, |caps: &regex::Captures| {
        alias = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str().to_string());
        // The matched tag is replaced with an empty string
        ""
    });

    (alias, new_description.trim().to_string())
}

#[cfg(test)]
mod tests {
    use futures_util::TryStreamExt;

    use super::*;

    const CMD_1: &str = "cmd number 1";
    const CMD_2: &str = "cmd number 2";
    const CMD_3: &str = "cmd number 3";

    const ALIAS_1: &str = "a1";
    const ALIAS_2: &str = "a2";
    const ALIAS_3: &str = "a3";

    const DESCRIPTION_1: &str = "Line of a description 1";
    const DESCRIPTION_2: &str = "Line of a description 2";
    const DESCRIPTION_3: &str = "Line of a description 3";

    const CMD_MULTI_1: &str = "cmd very long";
    const CMD_MULTI_2: &str = "that is split across";
    const CMD_MULTI_3: &str = "multiple lines for readability";

    #[tokio::test]
    async fn test_parse_import_items_empty_input() {
        let items = parse_import_items("".as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_parse_import_items_simple() {
        let input = format!(
            r"{CMD_1}
              {CMD_2}
              {CMD_3}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert!(get_command(&items[0]).description.is_none());
        assert_eq!(get_command(&items[1]).cmd, CMD_2);
        assert!(get_command(&items[1]).description.is_none());
        assert_eq!(get_command(&items[2]).cmd, CMD_3);
        assert!(get_command(&items[2]).description.is_none());
    }

    #[tokio::test]
    async fn test_parse_import_items_legacy() {
        let input = format!(
            r"{CMD_1} ## {DESCRIPTION_1}
              {CMD_2} ## {DESCRIPTION_2}
              {CMD_3} ## {DESCRIPTION_3}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert_eq!(get_command(&items[0]).description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(get_command(&items[1]).cmd, CMD_2);
        assert_eq!(get_command(&items[1]).description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(get_command(&items[2]).cmd, CMD_3);
        assert_eq!(get_command(&items[2]).description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_import_items_sh_style() {
        let input = format!(
            r"# {DESCRIPTION_1}
              {CMD_1}

              # {DESCRIPTION_2}
              {CMD_2}

              # {DESCRIPTION_3}
              {CMD_3}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert_eq!(get_command(&items[0]).description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(get_command(&items[1]).cmd, CMD_2);
        assert_eq!(get_command(&items[1]).description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(get_command(&items[2]).cmd, CMD_3);
        assert_eq!(get_command(&items[2]).description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_import_items_tldr_style() {
        // https://github.com/tldr-pages/tldr/blob/main/CONTRIBUTING.md#markdown-format
        let input = format!(
            r"# command-name

              > Short, snappy description.
              > Preferably one line; two are acceptable if necessary.
              > More information: <https://url-to-upstream.tld>.

              - {DESCRIPTION_1}:
              
              `{CMD_1}`

              - {DESCRIPTION_2}:

              `{CMD_2}`

              - {DESCRIPTION_3}:

              `{CMD_3}`"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert_eq!(get_command(&items[0]).description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(get_command(&items[1]).cmd, CMD_2);
        assert_eq!(get_command(&items[1]).description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(get_command(&items[2]).cmd, CMD_3);
        assert_eq!(get_command(&items[2]).description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_import_items_discard_orphan_descriptions() {
        let input = format!(
            r"# This is a comment without a command

              # {DESCRIPTION_1}
              {CMD_1}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert_eq!(get_command(&items[0]).description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_import_items_inline_description_takes_precedence() {
        let input = format!(
            r"# {DESCRIPTION_2}
              {CMD_1} ## {DESCRIPTION_1}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(get_command(&items[0]).cmd, CMD_1);
        assert_eq!(get_command(&items[0]).description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_import_items_multiline_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
              # 
              # {DESCRIPTION_2}
              {CMD_1}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, CMD_1);
        assert_eq!(
            cmd.description.as_ref(),
            Some(&format!("{DESCRIPTION_1}\n\n{DESCRIPTION_2}"))
        );
    }

    #[tokio::test]
    async fn test_parse_import_items_multiline() {
        let input = format!(
            r"# {DESCRIPTION_1}
              {CMD_MULTI_1} \
                  # inner comment, not part of the description or command
                  {CMD_MULTI_2} \ 
                  {CMD_MULTI_3}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, format!("{CMD_MULTI_1} {CMD_MULTI_2} {CMD_MULTI_3}"));
        assert_eq!(cmd.description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_import_items_with_tags_no_description() {
        let input = CMD_1;
        let tags = vec!["#test".to_string(), "#tag2".to_string()];
        let items = parse_import_items(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, CMD_1);
        assert_eq!(cmd.description.as_deref(), Some("#test #tag2"));
    }

    #[tokio::test]
    async fn test_parse_import_items_with_tags_simple_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
              {CMD_1}
                    
              {CMD_2} ## {DESCRIPTION_2}"
        );
        let tags = vec!["#test".to_string()];
        let items = parse_import_items(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 2);
        let cmd0 = get_command(&items[0]);
        assert_eq!(cmd0.cmd, CMD_1);
        assert_eq!(cmd0.description.as_ref(), Some(&format!("{DESCRIPTION_1} #test")));
        let cmd1 = get_command(&items[1]);
        assert_eq!(cmd1.cmd, CMD_2);
        assert_eq!(cmd1.description.as_ref(), Some(&format!("{DESCRIPTION_2} #test")));
    }

    #[tokio::test]
    async fn test_parse_import_items_with_tags_and_multiline_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
              # {DESCRIPTION_2}
              {CMD_1}"
        );
        let tags = vec!["#test".to_string()];
        let items = parse_import_items(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, CMD_1);
        assert_eq!(
            cmd.description.as_ref(),
            Some(&format!("{DESCRIPTION_1}\n{DESCRIPTION_2}\n#test"))
        );
    }

    #[tokio::test]
    async fn test_parse_import_items_skips_existing_tags() {
        let input = format!(
            r"# {DESCRIPTION_1} #test
              {CMD_1}"
        );
        let tags = vec!["#test".to_string(), "#new".to_string()];
        let items = parse_import_items(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, CMD_1);
        assert_eq!(cmd.description.as_ref(), Some(&format!("{DESCRIPTION_1} #test #new")));
    }

    #[tokio::test]
    async fn test_parse_import_items_with_aliases() {
        let input = format!(
            r"# [alias:{ALIAS_1}] {DESCRIPTION_1}
              {CMD_1}

              # [alias:{ALIAS_2}] 
              # {DESCRIPTION_2}
              # {DESCRIPTION_2}
              {CMD_2}

              # [alias:{ALIAS_3}]
              {CMD_3}"
        );
        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        let cmd0 = get_command(&items[0]);
        assert_eq!(cmd0.cmd, CMD_1);
        assert_eq!(cmd0.description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(cmd0.alias.as_deref(), Some(ALIAS_1));

        let cmd1 = get_command(&items[1]);
        assert_eq!(cmd1.cmd, CMD_2);
        assert_eq!(
            cmd1.description.as_ref(),
            Some(&format!("{DESCRIPTION_2}\n{DESCRIPTION_2}"))
        );
        assert_eq!(cmd1.alias.as_deref(), Some(ALIAS_2));

        let cmd2 = get_command(&items[2]);
        assert_eq!(cmd2.cmd, CMD_3);
        assert!(cmd2.description.is_none());
        assert_eq!(cmd2.alias.as_deref(), Some(ALIAS_3));
    }

    #[tokio::test]
    async fn test_parse_import_items_completions() {
        let input = r#"
            # A command to ensure both types are handled
            ls -l ## list files

            # Completions
            $(git) branch: git branch --all
            $ file: ls -F
            $ (az) group: az group list --output tsv
            "#;

        let items = parse_import_items(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(items.len(), 4);

        let cmd = get_command(&items[0]);
        assert_eq!(cmd.cmd, "ls -l");
        assert_eq!(cmd.description.as_deref(), Some("list files"));

        if let ImportExportItem::Completion(c) = &items[1] {
            assert_eq!(c.flat_root_cmd, "git");
            assert_eq!(c.flat_variable, "branch");
            assert_eq!(c.suggestions_provider, "git branch --all");
        } else {
            panic!("Expected a Completion at index 1");
        }

        if let ImportExportItem::Completion(c) = &items[2] {
            assert_eq!(c.flat_root_cmd, ""); // Global
            assert_eq!(c.flat_variable, "file");
            assert_eq!(c.suggestions_provider, "ls -F");
        } else {
            panic!("Expected a Completion at index 2");
        }

        if let ImportExportItem::Completion(c) = &items[3] {
            assert_eq!(c.flat_root_cmd, "az");
            assert_eq!(c.flat_variable, "group");
            assert_eq!(c.suggestions_provider, "az group list --output tsv");
        } else {
            panic!("Expected a Completion at index 3");
        }
    }

    #[tokio::test]
    async fn test_parse_import_items_invalid_completion_format() {
        let line = "$ invalid completion format";
        let result = parse_import_items(line.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(
                matches!(err, AppError::UserFacing(UserFacingError::ImportCompletionInvalidFormat(s)) if s == line)
            );
        }
    }

    /// Test helper to extract a Command from an ImportExportItem, panicking if it's the wrong variant
    fn get_command(item: &ImportExportItem) -> &Command {
        match item {
            ImportExportItem::Command(command) => command,
            ImportExportItem::Completion(_) => panic!("Expected ImportExportItem::Command, found completion"),
        }
    }
}
