use std::{
    collections::HashMap,
    env,
    io::{Cursor, ErrorKind},
    sync::LazyLock,
};

use color_eyre::{Result, eyre::Context};
use futures_util::{StreamExt, TryStreamExt, stream};
use itertools::Itertools;
use regex::Regex;
use reqwest::{
    StatusCode, Url,
    header::{self, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines},
};
use tokio_stream::Stream;
use tokio_util::io::StreamReader;
use tracing::instrument;
use uuid::Uuid;

use super::IntelliShellService;
use crate::{
    cli::{ExportCommandsProcess, HttpMethod, ImportCommandsProcess},
    config::GistConfig,
    errors::ImportExportError,
    model::{CATEGORY_USER, Command, SOURCE_IMPORT},
    utils::{ShellType, get_shell},
};

const README_FILENAME: &str = "readme.md";
const README_FILENAME_UPPER: &str = "README.md";

impl IntelliShellService {
    /// Import commands, returning the number of new commands inserted and skipped (because they already existed)
    pub async fn import_commands(
        &self,
        args: ImportCommandsProcess,
        gist_config: GistConfig,
    ) -> Result<(u64, u64), ImportExportError> {
        let ImportCommandsProcess {
            location,
            file,
            http,
            gist,
            filter,
            dry_run,
            tags,
            headers,
            method,
        } = args;
        if file {
            self.import_file_commands(location, filter, dry_run, tags).await
        } else if http {
            self.import_http_commands(location, filter, dry_run, tags, headers, method)
                .await
        } else if gist {
            self.import_gist_commands(location, gist_config, filter, dry_run, tags)
                .await
        } else {
            // Determine which mode based on the location
            if location == "gist"
                || location.starts_with("https://gist.github.com")
                || location.starts_with("https://api.github.com/gists")
            {
                self.import_gist_commands(location, gist_config, filter, dry_run, tags)
                    .await
            } else if location.starts_with("http://") || location.starts_with("https://") {
                self.import_http_commands(location, filter, dry_run, tags, headers, method)
                    .await
            } else {
                self.import_file_commands(location, filter, dry_run, tags).await
            }
        }
    }

    #[instrument(skip_all)]
    async fn import_file_commands(
        &self,
        path: String,
        filter: Option<Regex>,
        dry_run: bool,
        tags: Vec<String>,
    ) -> Result<(u64, u64), ImportExportError> {
        // If the path is the hypen placeholder
        if path == "-" {
            if let Some(ref filter) = filter {
                tracing::info!("Importing commands matching `{filter}` from stdin");
            } else {
                tracing::info!("Importing commands from stdin");
            }
            // We read the content from stdin
            let content = tokio::io::stdin();
            let commands_stream = parse_commands(content, tags, CATEGORY_USER, SOURCE_IMPORT);
            if dry_run {
                import_dry_run(commands_stream, filter).await
            } else {
                Ok(self
                    .storage
                    .import_commands(commands_stream, filter, false, false)
                    .await?)
            }
        } else {
            // Otherwise, check the path to import the file
            match fs::metadata(&path).await {
                Ok(m) if m.is_file() => (),
                Ok(_) => return Err(ImportExportError::NotAFile),
                Err(err) if err.kind() == ErrorKind::NotFound => return Err(ImportExportError::FileNotFound),
                Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                    return Err(ImportExportError::FileNotAccessible);
                }
                Err(err) => return Err(ImportExportError::Unexpected(err.into())),
            }
            if let Some(ref filter) = filter {
                tracing::info!("Importing commands matching `{filter}` from file: {path}");
            } else {
                tracing::info!("Importing commands from file: {path}");
            }
            let file = File::open(path).await.wrap_err("Couldn't open the file")?;
            let commands_stream = parse_commands(file, tags, CATEGORY_USER, SOURCE_IMPORT);
            if dry_run {
                import_dry_run(commands_stream, filter).await
            } else {
                Ok(self
                    .storage
                    .import_commands(commands_stream, filter, false, false)
                    .await?)
            }
        }
    }

    #[instrument(skip_all)]
    async fn import_http_commands(
        &self,
        mut url: String,
        filter: Option<Regex>,
        dry_run: bool,
        tags: Vec<String>,
        headers: Vec<(HeaderName, HeaderValue)>,
        method: HttpMethod,
    ) -> Result<(u64, u64), ImportExportError> {
        // If the URL is the stdin placeholder, read a line from it
        if url == "-" {
            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer)?;
            url = buffer.trim_end_matches("\n").to_string();
            tracing::debug!("Read url from stdin: {url}");
        }

        // Parse the URL
        let url = Url::parse(&url).map_err(|err| {
            tracing::error!("Couldn't parse url: {err}");
            ImportExportError::HttpInvalidUrl
        })?;

        let method = method.into();
        if let Some(ref filter) = filter {
            tracing::info!("Importing commands matching `{filter}` from http: {method} {url}");
        } else {
            tracing::info!("Importing commands from http: {method} {url}");
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
            ImportExportError::HttpRequestFailed(err.to_string())
        })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(ImportExportError::HttpRequestFailed(format!(
                    "received {status_str} {reason} response"
                )));
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(ImportExportError::HttpRequestFailed(format!(
                    "received {status_str} response"
                )));
            }
        }

        // Check the response content type
        let mut json = false;
        if let Some(content_type) = res.headers().get(header::CONTENT_TYPE) {
            let Ok(content_type) = content_type.to_str() else {
                return Err(ImportExportError::HttpRequestFailed(String::from(
                    "couldn't read content-type header",
                )));
            };
            if content_type.starts_with("application/json") {
                json = true;
            } else if !content_type.starts_with("text") {
                return Err(ImportExportError::HttpRequestFailed(format!(
                    "unsupported content-type: {content_type}",
                )));
            }
        }

        if json {
            // Parse the body as a list of commands
            let commands: Vec<CommandDto> = match res.json().await {
                Ok(b) => b,
                Err(err) if err.is_decode() => {
                    tracing::error!("Couldn't parse api response: {err}");
                    return Err(ImportExportError::GistRequestFailed(String::from(
                        "couldn't parse api response",
                    )));
                }
                Err(err) => {
                    tracing::error!("{err:?}");
                    return Err(ImportExportError::GistRequestFailed(err.to_string()));
                }
            };

            // Import them
            let commands_stream = stream::iter(commands.into_iter().map(|c| Ok(Command::from(c))));
            if dry_run {
                import_dry_run(commands_stream, filter).await
            } else {
                Ok(self
                    .storage
                    .import_commands(commands_stream, filter, false, false)
                    .await?)
            }
        } else {
            // Get the response body as a stream
            let stream_reader = StreamReader::new(res.bytes_stream().map_err(std::io::Error::other));

            // Import commands from the response body
            let commands_stream = parse_commands(stream_reader, tags, CATEGORY_USER, SOURCE_IMPORT);
            if dry_run {
                import_dry_run(commands_stream, filter).await
            } else {
                Ok(self
                    .storage
                    .import_commands(commands_stream, filter, false, false)
                    .await?)
            }
        }
    }

    #[instrument(skip_all)]
    async fn import_gist_commands(
        &self,
        mut gist: String,
        gist_config: GistConfig,
        filter: Option<Regex>,
        dry_run: bool,
        tags: Vec<String>,
    ) -> Result<(u64, u64), ImportExportError> {
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
                .import_http_commands(gist, filter, dry_run, tags, Vec::new(), HttpMethod::GET)
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
            tracing::info!("Importing commands matching `{filter}` from gist: {url}");
        } else {
            tracing::info!("Importing commands from gist: {url}");
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
                ImportExportError::GistRequestFailed(err.to_string())
            })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} {reason} response"
                )));
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} response"
                )));
            }
        }

        // Parse the body as a json
        let mut body: GistDto = match res.json().await {
            Ok(b) => b,
            Err(err) if err.is_decode() => {
                tracing::error!("Couldn't parse api response: {err}");
                return Err(ImportExportError::GistRequestFailed(String::from(
                    "couldn't parse api response",
                )));
            }
            Err(err) => {
                tracing::error!("{err:?}");
                return Err(ImportExportError::GistRequestFailed(err.to_string()));
            }
        };

        let full_content = if let Some(ref gist_file) = gist_file {
            // If there's a file specified, import just it
            body.files
                .remove(gist_file)
                .ok_or(ImportExportError::GistFileNotFound)?
                .content
        } else {
            // Otherwise import all of the files (except the readme)
            body.files
                .into_iter()
                .filter(|(k, _)| k != README_FILENAME && k != README_FILENAME_UPPER)
                .map(|(_, f)| f.content)
                .join("\n")
        };

        // Import the commands
        let commands_stream = parse_commands(Cursor::new(full_content), tags, CATEGORY_USER, SOURCE_IMPORT);
        if dry_run {
            import_dry_run(commands_stream, filter).await
        } else {
            Ok(self
                .storage
                .import_commands(commands_stream, filter, false, false)
                .await?)
        }
    }

    /// Exports commands, returning the number of commands exported
    pub async fn export_commands(
        &self,
        args: ExportCommandsProcess,
        gist_config: GistConfig,
    ) -> Result<u64, ImportExportError> {
        let ExportCommandsProcess {
            location,
            file,
            http,
            gist,
            filter,
            headers,
            method,
        } = args;

        if file {
            self.export_file_commands(location, filter).await
        } else if http {
            self.export_http_commands(location, filter, headers, method).await
        } else if gist {
            self.export_gist_commands(location, gist_config, filter).await
        } else {
            // Determine which mode based on the location
            if location == "gist"
                || location.starts_with("https://gist.github.com")
                || location.starts_with("https://gist.githubusercontent.com")
                || location.starts_with("https://api.github.com/gists")
            {
                self.export_gist_commands(location, gist_config, filter).await
            } else if location.starts_with("http://") || location.starts_with("https://") {
                self.export_http_commands(location, filter, headers, method).await
            } else {
                self.export_file_commands(location, filter).await
            }
        }
    }

    #[instrument(skip_all)]
    async fn export_file_commands(&self, path: String, filter: Option<Regex>) -> Result<u64, ImportExportError> {
        if path == "-" {
            if let Some(ref filter) = filter {
                tracing::info!("Exporting commands matching `{filter}` to stdout");
            } else {
                tracing::info!("Exporting commands to stdout");
            }
            self.export_file_commands_inner(filter, tokio::io::stdout(), false)
                .await
        } else {
            match File::create(&path).await {
                Ok(file) => {
                    if let Some(ref filter) = filter {
                        tracing::info!("Exporting commands matching `{filter}` to file: {path}");
                    } else {
                        tracing::info!("Exporting commands to file: {path}");
                    }
                    self.export_file_commands_inner(filter, file, path.ends_with(".cmd") || path.ends_with(".bat"))
                        .await
                }
                Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                    return Err(ImportExportError::FileNotAccessible);
                }
                Err(err) if err.kind() == ErrorKind::NotFound => return Err(ImportExportError::FileNotFound),
                Err(err) if err.kind() == ErrorKind::IsADirectory => return Err(ImportExportError::NotAFile),
                Err(err) => return Err(ImportExportError::Unexpected(err.into())),
            }
        }
    }

    async fn export_file_commands_inner(
        &self,
        filter: Option<Regex>,
        mut location: impl AsyncWrite + Unpin,
        is_batch: bool,
    ) -> Result<u64, ImportExportError> {
        let mut commands = self.storage.export_user_commands(filter).await;
        let mut count = 0;
        while let Some(command) = commands.next().await {
            location
                .write_all(write_command(command?, is_batch).as_bytes())
                .await
                .map_err(|err| {
                    if err.kind() == ErrorKind::BrokenPipe {
                        ImportExportError::FileBrokenPipe
                    } else {
                        err.into()
                    }
                })?;
            count += 1;
        }
        location.flush().await?;
        Ok(count)
    }

    #[instrument(skip_all)]
    async fn export_http_commands(
        &self,
        url: String,
        filter: Option<Regex>,
        headers: Vec<(HeaderName, HeaderValue)>,
        method: HttpMethod,
    ) -> Result<u64, ImportExportError> {
        // Parse the URL
        let url = Url::parse(&url).map_err(|err| {
            tracing::error!("Couldn't parse url: {err}");
            ImportExportError::HttpInvalidUrl
        })?;

        let method = method.into();
        if let Some(ref filter) = filter {
            tracing::info!("Exporting commands matching `{filter}` to http: {method} {url}");
        } else {
            tracing::info!("Exporting commands to http: {method} {url}");
        }

        // Collect commands to export
        let mut commands_stream = self.storage.export_user_commands(filter).await;
        let mut commands = Vec::new();
        while let Some(command) = commands_stream.next().await {
            commands.push(CommandDto::from(command?));
        }

        // Build the request
        let client = reqwest::Client::new();
        let mut req = client.request(method, url);

        // Add headers
        for (name, value) in headers {
            tracing::debug!("Appending '{name}' header");
            req = req.header(name, value);
        }

        // Set JSON body
        req = req.json(&commands);

        // Send the request
        let res = req.send().await.map_err(|err| {
            tracing::error!("{err:?}");
            ImportExportError::HttpRequestFailed(err.to_string())
        })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(ImportExportError::HttpRequestFailed(format!(
                    "received {status_str} {reason} response"
                )));
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(ImportExportError::HttpRequestFailed(format!(
                    "received {status_str} response"
                )));
            }
        }

        Ok(commands.len() as u64)
    }

    #[instrument(skip_all)]
    async fn export_gist_commands(
        &self,
        gist: String,
        gist_config: GistConfig,
        filter: Option<Regex>,
    ) -> Result<u64, ImportExportError> {
        // Retrieve the gist id and optional sha and file
        let (gist_id, gist_sha, gist_file) = extract_gist_data(&gist, &gist_config)?;

        // If a sha is found, return an error as we can't modify it
        if gist_sha.is_some() {
            return Err(ImportExportError::GistLocationHasSha);
        }

        // Retrieve the gist token to be used
        let gist_token = get_gist_token(&gist_config)?;

        let url = format!("https://api.github.com/gists/{gist_id}");
        if let Some(ref filter) = filter {
            tracing::info!("Exporting commands matching `{filter}` to gist: {url}");
        } else {
            tracing::info!("Exporting commands to gist: {url}");
        }

        // Retrieve the gist to verify its existence
        let client = reqwest::Client::new();
        let res = client
            .get(&url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(header::USER_AGENT, "intelli-shell")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|err| {
                tracing::error!("{err:?}");
                ImportExportError::GistRequestFailed(err.to_string())
            })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} {reason} response"
                )));
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} response"
                )));
            }
        }

        // Parse the body as a json
        let actual_gist: GistDto = match res.json().await {
            Ok(b) => b,
            Err(err) if err.is_decode() => {
                tracing::error!("Couldn't parse api response: {err}");
                return Err(ImportExportError::GistRequestFailed(String::from(
                    "couldn't parse api response",
                )));
            }
            Err(err) => {
                tracing::error!("{err:?}");
                return Err(ImportExportError::GistRequestFailed(err.to_string()));
            }
        };

        // Determine the extension based on the file or shell
        let extension = if let Some(ref gist_file) = gist_file
            && let Some((_, ext)) = gist_file.rfind('.').map(|i| gist_file.split_at(i))
        {
            ext.to_owned()
        } else {
            match get_shell() {
                ShellType::Cmd => ".cmd",
                ShellType::WindowsPowerShell | ShellType::PowerShellCore => ".ps1",
                _ => ".sh",
            }
            .to_owned()
        };

        // Collect commands to export
        let mut commands_stream = self.storage.export_user_commands(filter).await;
        let mut content = String::new();
        let mut count = 0;
        while let Some(command) = commands_stream.next().await {
            content.push_str(&write_command(command?, &extension == ".cmd"));
            count += 1;
        }

        // Prepare the data to be sent
        let explicit_file = gist_file.is_some();
        let mut files = vec![(
            gist_file
                .or_else(|| {
                    let command_files = actual_gist
                        .files
                        .keys()
                        .filter(|f| f.ends_with(&extension))
                        .collect::<Vec<_>>();
                    if command_files.len() == 1 {
                        Some(command_files[0].to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| format!("commands{extension}")),
            GistFileDto { content },
        )];
        if !explicit_file
            && !actual_gist.files.contains_key(README_FILENAME)
            && !actual_gist.files.contains_key(README_FILENAME_UPPER)
        {
            files.push((
                String::from(README_FILENAME),
                GistFileDto {
                    content: format!(
                        r"# IntelliShell Commands

These commands have been exported using [intelli-shell]({}), a command-line tool to bookmark and search commands.

You can easily import all the commands by running:

```sh
intelli-shell import --gist {gist_id}
```",
                        env!("CARGO_PKG_REPOSITORY")
                    ),
                },
            ));
        }
        let gist = GistDto {
            files: HashMap::from_iter(files),
        };

        // Call the API
        let client = reqwest::Client::new();
        let res = client
            .patch(url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(header::USER_AGENT, "intelli-shell")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(gist_token)
            .json(&gist)
            .send()
            .await
            .map_err(|err| {
                tracing::error!("{err:?}");
                ImportExportError::GistRequestFailed(err.to_string())
            })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if status == StatusCode::NOT_FOUND {
                tracing::error!("Update got not found after a succesful get request");
                return Err(ImportExportError::GistRequestFailed(
                    "token missing permissions to update the gist".into(),
                ));
            } else if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} {reason} response"
                )));
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(ImportExportError::GistRequestFailed(format!(
                    "received {status_str} response"
                )));
            }
        }

        Ok(count)
    }
}

/// Retrieves a GitHub personal access token for gist, checking configuration and environment variables.
///
/// This function attempts to find a token by searching in the following locations, in order of
/// precedence:
///
/// 1. The `GIST_TOKEN` environment variable
/// 2. The `token` field of the provided `gist_config` object
///
/// If a token is not found in any of these locations, the function will return an error.
fn get_gist_token(gist_config: &GistConfig) -> Result<String, ImportExportError> {
    if let Ok(token) = env::var("GIST_TOKEN")
        && !token.is_empty()
    {
        Ok(token)
    } else if !gist_config.token.is_empty() {
        Ok(gist_config.token.clone())
    } else {
        Err(ImportExportError::GistMissingToken)
    }
}

/// Parses a Gist location string to extract its ID, and optional SHA and filename.
///
/// This function is highly flexible and can interpret several Gist location formats, including full URLs, shorthand
/// notations, and special placeholder values.
///
/// ### Placeholder Behavior
///
/// If the `location` string is a placeholder (`"gist"`, or an empty/whitespace string), the function will attempt
/// to use the `id` from the provided `gist_config` as a fallback. If `gist_config` is `None` in this case, it will
/// return an error.
///
/// ### Supported URL Formats
///
/// - `https://gist.github.com/{user}/{id}`
/// - `https://gist.github.com/{user}/{id}/{sha}`
/// - `https://gist.githubusercontent.com/{user}/{id}/raw`
/// - `https://gist.githubusercontent.com/{user}/{id}/raw/{file}`
/// - `https://gist.githubusercontent.com/{user}/{id}/raw/{sha}`
/// - `https://gist.githubusercontent.com/{user}/{id}/raw/{sha}/{file}`
/// - `https://api.github.com/gists/{id}`
/// - `https://api.github.com/gists/{id}/{sha}`
///
/// ### Supported Shorthand Formats
///
/// - `{file}` (with the id from the config)
/// - `{id}`
/// - `{id}/{file}`
/// - `{id}/{sha}`
/// - `{id}/{sha}/{file}`
fn extract_gist_data(
    location: &str,
    gist_config: &GistConfig,
) -> Result<(String, Option<String>, Option<String>), ImportExportError> {
    let location = location.trim();
    if location.is_empty() || location == "gist" {
        if !gist_config.id.is_empty() {
            Ok((gist_config.id.clone(), None, None))
        } else {
            Err(ImportExportError::GistMissingId)
        }
    } else {
        /// Helper function to check if a string is a commit sha
        fn is_sha(s: &str) -> bool {
            s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
        }
        /// Helper function to check if a string is a gist id
        fn is_id(s: &str) -> bool {
            s.chars().all(|c| c.is_ascii_hexdigit())
        }
        // First, attempt to parse the location as a full URL
        if let Ok(url) = Url::parse(location) {
            let host = url.host_str().unwrap_or_default();
            let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
            let gist_data = match host {
                "gist.github.com" => {
                    // Handles: https://gist.github.com/{user}/{id}/{sha?}
                    if segments.len() < 2 {
                        return Err(ImportExportError::GistInvalidLocation);
                    }
                    let id = segments[1].to_string();
                    let mut sha = None;
                    if segments.len() > 2 {
                        if is_sha(segments[2]) {
                            sha = Some(segments[2].to_string());
                        } else {
                            return Err(ImportExportError::GistInvalidLocation);
                        }
                    }
                    (id, sha, None)
                }
                "gist.githubusercontent.com" => {
                    // Handles: https://gist.githubusercontent.com/{user}/{id}/raw/{sha?}/{file?}
                    if segments.len() < 3 || segments[2] != "raw" {
                        return Err(ImportExportError::GistInvalidLocation);
                    }
                    let id = segments[1].to_string();
                    let mut sha = None;
                    let mut file = None;
                    if segments.len() > 3 {
                        if is_sha(segments[3]) {
                            sha = Some(segments[3].to_string());
                            if segments.len() > 4 {
                                file = Some(segments[4].to_string());
                            }
                        } else {
                            file = Some(segments[3].to_string());
                        }
                    }
                    (id, sha, file)
                }
                "api.github.com" => {
                    // Handles: https://api.github.com/gists/{id}/{sha?}
                    if segments.len() < 2 || segments[0] != "gists" {
                        return Err(ImportExportError::GistInvalidLocation);
                    }
                    let id = segments[1].to_string();
                    let mut sha = None;
                    if segments.len() > 2 {
                        if is_sha(segments[2]) {
                            sha = Some(segments[2].to_string());
                        } else {
                            return Err(ImportExportError::GistInvalidLocation);
                        }
                    }
                    (id, sha, None)
                }
                // Any other host is considered an invalid location
                _ => return Err(ImportExportError::GistInvalidLocation),
            };
            return Ok(gist_data);
        }

        // If it's not a valid URL, treat it as a shorthand format
        let id;
        let mut sha = None;
        let mut file = None;

        let parts: Vec<&str> = location.split('/').collect();
        match parts.len() {
            // Handles:
            // - {file} (with id from config)
            // - {id}
            1 => {
                if is_id(parts[0]) {
                    // Looks like an id
                    id = parts[0].to_string();
                } else if !gist_config.id.is_empty() {
                    // If it doesn't look like an id, treat it like a file and pick the id from the config
                    id = gist_config.id.clone();
                    file = Some(parts[0].to_string());
                } else {
                    return Err(ImportExportError::GistMissingId);
                }
            }
            // Handles:
            // - {id}/{file}
            // - {id}/{sha}
            2 => {
                if is_id(parts[0]) {
                    id = parts[0].to_string();
                } else {
                    return Err(ImportExportError::GistInvalidLocation);
                }
                if is_sha(parts[1]) {
                    sha = Some(parts[1].to_string());
                } else {
                    file = Some(parts[1].to_string());
                }
            }
            // Handles:
            // - {id}/{sha}/{file}
            3 => {
                if is_id(parts[0]) {
                    id = parts[0].to_string();
                } else {
                    return Err(ImportExportError::GistInvalidLocation);
                }
                if is_sha(parts[1]) {
                    sha = Some(parts[1].to_string());
                } else {
                    return Err(ImportExportError::GistInvalidLocation);
                }
                file = Some(parts[2].to_string());
            }
            // Too many segments
            _ => {
                return Err(ImportExportError::GistInvalidLocation);
            }
        }

        Ok((id, sha, file))
    }
}

/// Lazily parses a stream of text into a [`Stream`] of [`Command`].
///
/// This function is the primary entry point for parsing command definitions from a file or any other async source.
/// It operates in a streaming fashion, meaning it reads the input line-by-line without loading the entire content into
/// memory, making it highly efficient for large files.
///
/// # Format Rules
///
/// The parser follows a set of rules to interpret the text content:
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
///   - Lines starting with `#`, `//`, or `- ` (ignoring leading whitespace) are treated as comments.
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
pub(super) fn parse_commands(
    content: impl AsyncRead + Unpin + Send,
    tags: Vec<String>,
    category: impl Into<String>,
    source: impl Into<String>,
) -> impl Stream<Item = Result<Command>> + Send {
    // Make sure the tags starts with a hastah (#)
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

    /// Helper to extract the comment content from a line
    fn get_comment_content(line: &str) -> Option<&str> {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix('#') {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed.strip_prefix("//") {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed.strip_prefix("- ") {
            return Some(stripped.trim());
        }
        if let Some(stripped) = trimmed.strip_prefix("::") {
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
                Err(err) => return Some((Err(err).wrap_err("Error reading commands"), state)),
            };

            // If the line is the shebang header, skip it
            if line == "#!intelli-shell" {
                continue;
            }

            // If the line is an MD quote or code block, skip it
            if line.trim().starts_with("> ") || line.trim().starts_with("```") {
                continue;
            }

            // If the line is a comment, accumulate it and continue to the next line
            if let Some(comment_content) = get_comment_content(&line) {
                if state.description_paused {
                    // If the description was 'paused' by a blank line, a new comment indicates a new description block
                    state.description_buffer.clear();
                }
                state.description_buffer.push(comment_content.to_string());
                state.description_paused = false;
                continue;
            }

            // If the line is blank, it might be a separator between comment blocks or trailing after a description
            if line.trim().is_empty() {
                // We 'pause' the description accumulation.
                if !state.description_buffer.is_empty() {
                    state.description_paused = true;
                }
                continue;
            }

            // Otherwise the line is a command that can potentially span across multiple lines
            let mut current_line = line;
            let mut command_parts: Vec<String> = Vec::new();
            let mut inline_description: Option<String> = None;

            // Inner loop to handle multi-line commands
            loop {
                // Before processing a line as part of a command
                if get_comment_content(&current_line).is_some() || current_line.trim().is_empty() {
                    // If the line is a comment or a blank line, restart the loop with the next line
                    if let Some(next_line_res) = state.lines.next_line().await.transpose() {
                        current_line = match next_line_res {
                            Ok(next_line) => next_line,
                            Err(err) => return Some((Err(err).wrap_err("Error reading commands"), state)),
                        };
                        continue;
                    } else {
                        // End of stream mid-command
                        break;
                    }
                }

                // Check if theres an inline comment after the command
                let (command_segment, desc) = match current_line.split_once(" ## ") {
                    Some((cmd, desc)) => (cmd, Some(desc.trim().to_string())),
                    None => (current_line.as_str(), None),
                };
                if inline_description.is_none() {
                    inline_description = desc;
                }

                // If the line ends with the escape char, that means the newline was escaped
                if let Some(stripped) = command_segment.trim().strip_suffix('\\') {
                    command_parts.push(stripped.trim().to_string());
                    // This command spans multiple lines, read the next one and continue with the loop
                    if let Some(next_line_res) = state.lines.next_line().await.transpose() {
                        current_line = match next_line_res {
                            Ok(next_line) => next_line,
                            Err(err) => return Some((Err(err).wrap_err("Error reading commands"), state)),
                        };
                    } else {
                        // End of stream mid-command
                        break;
                    }
                } else {
                    // This command consist of a single line, break out of the loop
                    command_parts.push(command_segment.trim().to_string());
                    break;
                }
            }

            // Setup the cmd
            let mut full_cmd = command_parts.join(" ");
            if full_cmd.starts_with('`') && full_cmd.ends_with('`') {
                full_cmd = full_cmd[1..full_cmd.len() - 1].to_string();
            }
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
                let tags = state
                    .tags
                    .iter()
                    .filter(|tag| !full_description.contains(*tag))
                    .join(" ");
                if !tags.is_empty() {
                    let multiline = full_description.contains('\n');
                    if multiline {
                        full_description += "\n";
                    } else if !full_description.is_empty() {
                        full_description += " ";
                    }
                    full_description += &tags;
                }
            }

            // Create the command
            let command = Command::new(state.category.clone(), state.source.clone(), full_cmd)
                .with_description(Some(full_description))
                .with_alias(alias);

            // Clear the buffer for the next iteration
            state.description_buffer.clear();
            state.description_paused = false;

            // Yield the command and the updated state for the next run
            return Some((Ok(command), state));
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

/// Performs a dry-run import by writing commands to the stdout
async fn import_dry_run(
    commands: impl Stream<Item = Result<Command>> + Send + 'static,
    filter: Option<Regex>,
) -> Result<(u64, u64), ImportExportError> {
    let mut processed = 0;
    let mut stdout = io::stdout();

    futures_util::pin_mut!(commands);
    while let Some(command_result) = commands.next().await {
        let command = command_result?;
        if let Some(ref filter) = filter {
            let matches_filter =
                filter.is_match(&command.cmd) || command.description.as_ref().is_some_and(|d| filter.is_match(d));
            if !matches_filter {
                continue;
            }
        }
        stdout.write_all(write_command(command, false).as_bytes()).await?;
        processed += 1;
    }
    stdout.flush().await?;

    Ok((processed, 0))
}

/// Writes the command as a String, to be exported.
///
/// This function formats a `Command` struct into a string representation suitable for saving to a file.
/// It handles the command's description and alias according to the parsing rules:
/// - An alias is formatted as `[alias:your-alias]`
/// - If the description is multi-line, the alias is placed on its own line at the beginning
/// - If the description is single-line or absent, the alias is placed on the same line as the description
fn write_command(command: Command, is_batch: bool) -> String {
    let comment = if is_batch { "::" } else { "#" };
    let cmd = command.cmd;
    let alias_tag = command.alias.as_ref().map(|a| format!("[alias:{a}]"));

    // Determine the full description string, including the alias if it exists.
    let description_content = match (command.description.as_deref(), alias_tag) {
        // If there's no description and no alias, output a single comment character to represent an empty description
        (None | Some(""), None) => return format!("{comment}\n{cmd}\n\n"),
        // If there's a description but no alias, use it as is
        (Some(desc), None) => desc.to_string(),
        // If there's an alias but no description, use the alias tag as the description
        (None | Some(""), Some(alias)) => alias,
        // If both description and alias exist, combine them
        (Some(desc), Some(alias)) => {
            if desc.contains('\n') {
                // For multi-line descriptions, place the alias on a new line at the start
                format!("{alias}\n{desc}")
            } else {
                // For single-line descriptions, place the alias on the same line
                format!("{alias} {desc}")
            }
        }
    };

    // Format the combined description content, prefixing each line with a comment marker
    let formatted_description = description_content
        .lines()
        .map(|line| format!("{comment} {line}"))
        .join("\n");

    format!("{formatted_description}\n{cmd}\n\n")
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug))]
struct CommandDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}
impl From<CommandDto> for Command {
    fn from(value: CommandDto) -> Self {
        Command::new(CATEGORY_USER, SOURCE_IMPORT, value.cmd)
            .with_description(value.description)
            .with_alias(value.alias)
    }
}
impl From<Command> for CommandDto {
    fn from(value: Command) -> Self {
        CommandDto {
            id: Some(value.id),
            alias: value.alias,
            cmd: value.cmd,
            description: value.description,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug))]
struct GistDto {
    files: HashMap<String, GistFileDto>,
}
#[derive(Serialize, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug))]
struct GistFileDto {
    content: String,
}

#[cfg(test)]
mod tests {
    use futures_util::TryStreamExt;

    use super::*;

    const TEST_GIST_ID: &str = "b3a462e23db5c99d1f3f4abf0dae5bd8";
    const TEST_GIST_SHA: &str = "330286d6e41f8ae0a5b4ddc3e01d5521b87a15ca";
    const TEST_GIST_FILE: &str = "my_commands.sh";

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

    #[test]
    fn test_extract_gist_data_config() {
        let (id, sha, file) = extract_gist_data(
            "gist",
            &GistConfig {
                id: String::from(TEST_GIST_ID),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data() {
        let location = format!("https://gist.github.com/username/{TEST_GIST_ID}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_with_sha() {
        let location = format!("https://gist.github.com/username/{TEST_GIST_ID}/{TEST_GIST_SHA}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_raw() {
        let location = format!("https://gist.githubusercontent.com/username/{TEST_GIST_ID}/raw");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_raw_with_file() {
        let location = format!("https://gist.githubusercontent.com/username/{TEST_GIST_ID}/raw/{TEST_GIST_FILE}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file.as_deref(), Some(TEST_GIST_FILE));
    }

    #[test]
    fn test_extract_gist_data_raw_with_sha() {
        let location = format!("https://gist.githubusercontent.com/username/{TEST_GIST_ID}/raw/{TEST_GIST_SHA}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_raw_with_sha_and_file() {
        let location =
            format!("https://gist.githubusercontent.com/username/{TEST_GIST_ID}/raw/{TEST_GIST_SHA}/{TEST_GIST_FILE}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file.as_deref(), Some(TEST_GIST_FILE));
    }

    #[test]
    fn test_extract_gist_data_api() {
        let location = format!("https://api.github.com/gists/{TEST_GIST_ID}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_api_with_sha() {
        let location = format!("https://api.github.com/gists/{TEST_GIST_ID}/{TEST_GIST_SHA}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_shorthand_file() {
        let (id, sha, file) = extract_gist_data(
            TEST_GIST_FILE,
            &GistConfig {
                id: String::from(TEST_GIST_ID),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file.as_deref(), Some(TEST_GIST_FILE));
    }

    #[test]
    fn test_extract_gist_data_shorthand_id() {
        let (id, sha, file) = extract_gist_data(TEST_GIST_ID, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_shorthand_id_and_file() {
        let location = format!("{TEST_GIST_ID}/{TEST_GIST_FILE}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha, None);
        assert_eq!(file.as_deref(), Some(TEST_GIST_FILE));
    }

    #[test]
    fn test_extract_gist_data_shorthand_id_and_sha() {
        let location = format!("{TEST_GIST_ID}/{TEST_GIST_SHA}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file, None);
    }

    #[test]
    fn test_extract_gist_data_shorthand_id_and_sha_and_file() {
        let location = format!("{TEST_GIST_ID}/{TEST_GIST_SHA}/{TEST_GIST_FILE}");
        let (id, sha, file) = extract_gist_data(&location, &GistConfig::default()).unwrap();
        assert_eq!(id, TEST_GIST_ID);
        assert_eq!(sha.as_deref(), Some(TEST_GIST_SHA));
        assert_eq!(file.as_deref(), Some(TEST_GIST_FILE));
    }

    #[tokio::test]
    async fn test_parse_commands_empty_input() {
        let commands = parse_commands("".as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_parse_commands_simple() {
        let input = format!(
            r"{CMD_1}
            {CMD_2}
            {CMD_3}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].cmd, CMD_1);
        assert!(commands[0].description.is_none());
        assert_eq!(commands[1].cmd, CMD_2);
        assert!(commands[1].description.is_none());
        assert_eq!(commands[2].cmd, CMD_3);
        assert!(commands[2].description.is_none());
    }

    #[tokio::test]
    async fn test_parse_commands_legacy() {
        let input = format!(
            r"{CMD_1} ## {DESCRIPTION_1}
            {CMD_2} ## {DESCRIPTION_2}
            {CMD_3} ## {DESCRIPTION_3}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(commands[1].cmd, CMD_2);
        assert_eq!(commands[1].description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(commands[2].cmd, CMD_3);
        assert_eq!(commands[2].description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_commands_sh_style() {
        let input = format!(
            r"# {DESCRIPTION_1}
            {CMD_1}

            # {DESCRIPTION_2}
            {CMD_2}

            # {DESCRIPTION_3}
            {CMD_3}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(commands[1].cmd, CMD_2);
        assert_eq!(commands[1].description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(commands[2].cmd, CMD_3);
        assert_eq!(commands[2].description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_commands_tldr_style() {
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
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(commands[1].cmd, CMD_2);
        assert_eq!(commands[1].description.as_deref(), Some(DESCRIPTION_2));
        assert_eq!(commands[2].cmd, CMD_3);
        assert_eq!(commands[2].description.as_deref(), Some(DESCRIPTION_3));
    }

    #[tokio::test]
    async fn test_parse_commands_discard_orphan_descriptions() {
        let input = format!(
            r"# This is a comment without a command

            # {DESCRIPTION_1}
            {CMD_1}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_commands_inline_description_takes_precedence() {
        let input = format!(
            r"# {DESCRIPTION_2}
            {CMD_1} ## {DESCRIPTION_1}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_commands_multiline_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
            # 
            # {DESCRIPTION_2}
            {CMD_1}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(
            commands[0].description,
            Some(format!("{DESCRIPTION_1}\n\n{DESCRIPTION_2}"))
        );
    }

    #[tokio::test]
    async fn test_parse_commands_multiline() {
        let input = format!(
            r"# {DESCRIPTION_1}
            {CMD_MULTI_1} \
                # inner comment, not part of the description or command
                {CMD_MULTI_2} \ 
                {CMD_MULTI_3}"
        );
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, format!("{CMD_MULTI_1} {CMD_MULTI_2} {CMD_MULTI_3}"));
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
    }

    #[tokio::test]
    async fn test_parse_commands_with_tags_no_description() {
        let input = CMD_1;
        let tags = vec!["#test".to_string(), "tag2".to_string()];
        let commands = parse_commands(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some("#test #tag2"));
    }

    #[tokio::test]
    async fn test_parse_commands_with_tags_simple_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
               {CMD_1}
               
               {CMD_2} ## {DESCRIPTION_2}"
        );
        let tags = vec!["#test".to_string()];
        let commands = parse_commands(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description, Some(format!("{DESCRIPTION_1} #test")));
        assert_eq!(commands[1].cmd, CMD_2);
        assert_eq!(commands[1].description, Some(format!("{DESCRIPTION_2} #test")));
    }

    #[tokio::test]
    async fn test_parse_commands_with_tags_and_multiline_description() {
        let input = format!(
            r"# {DESCRIPTION_1}
               # {DESCRIPTION_2}
               {CMD_1}"
        );
        let tags = vec!["#test".to_string()];
        let commands = parse_commands(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(
            commands[0].description,
            Some(format!("{DESCRIPTION_1}\n{DESCRIPTION_2}\n#test"))
        );
    }

    #[tokio::test]
    async fn test_parse_commands_skips_existing_tags() {
        let input = format!(
            r"# {DESCRIPTION_1} #test
               {CMD_1}"
        );
        let tags = vec!["#test".to_string(), "#new".to_string()];
        let commands = parse_commands(input.as_bytes(), tags, CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description, Some(format!("{DESCRIPTION_1} #test #new")));
    }

    #[tokio::test]
    async fn test_parse_commands_with_aliases() {
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
        let commands = parse_commands(input.as_bytes(), Vec::new(), CATEGORY_USER, SOURCE_IMPORT)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].cmd, CMD_1);
        assert_eq!(commands[0].description.as_deref(), Some(DESCRIPTION_1));
        assert_eq!(commands[0].alias.as_deref(), Some(ALIAS_1));
        assert_eq!(commands[1].cmd, CMD_2);
        assert_eq!(
            commands[1].description,
            Some(format!("{DESCRIPTION_2}\n{DESCRIPTION_2}"))
        );
        assert_eq!(commands[1].alias.as_deref(), Some(ALIAS_2));
        assert_eq!(commands[2].cmd, CMD_3);
        assert_eq!(commands[2].description, None);
        assert_eq!(commands[2].alias.as_deref(), Some(ALIAS_3));
    }
}
