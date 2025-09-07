use std::{
    collections::{HashMap, HashSet},
    env,
    io::ErrorKind,
};

use async_stream::try_stream;
use color_eyre::Report;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::{
    StatusCode, Url,
    header::{self, HeaderName, HeaderValue},
};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    cli::{ExportItemsProcess, HttpMethod},
    config::GistConfig,
    errors::{AppError, Result, UserFacingError},
    model::{ExportStats, ImportExportItem, ImportExportStream},
    utils::{
        ShellType,
        dto::{GIST_README_FILENAME, GIST_README_FILENAME_UPPER, GistDto, GistFileDto, ImportExportItemDto},
        extract_gist_data, extract_variables, flatten_str, get_export_gist_token, get_shell_type,
    },
};

impl IntelliShellService {
    /// Prepare a stream of items to export, optionally filtering commands
    pub async fn prepare_items_export(&self, filter: Option<Regex>) -> Result<ImportExportStream> {
        if let Some(ref filter) = filter {
            tracing::info!("Exporting commands matching `{filter}` and their related completions");
        } else {
            tracing::info!("Exporting all commands and completions");
        }

        let storage = self.storage.clone();
        let export_stream = try_stream! {
            // This set will accumulate the unique variable identifiers as we stream commands
            let mut unique_flat_vars = HashSet::new();

            // Get the initial stream of commands from the storage layer
            let is_filtered = filter.is_some();
            let mut command_stream = storage.export_user_commands(filter).await;

            // Process each command from the stream one by one
            while let Some(command_result) = command_stream.next().await {
                let command = command_result?;

                // Extract all variables from the command and accumulate them for later
                if is_filtered {
                    let flat_root_cmd = flatten_str(command.cmd.split_whitespace().next().unwrap_or(""));
                    if !flat_root_cmd.is_empty() {
                        let variables = extract_variables(&command.cmd);
                        for variable in variables {
                            for flat_name in variable.flat_names {
                                unique_flat_vars.insert((flat_root_cmd.clone(), flat_name));
                            }
                        }
                    }
                }

                // Yield the command immediately
                yield ImportExportItem::Command(command);
            }

            // Once the command stream is exhausted, export completions
            let completions = if is_filtered {
                // When filtering commands, export only related completions
                storage.export_user_variable_completions(unique_flat_vars).await?
            } else {
                // Otherwise, export all completions
                storage.list_variable_completions(None, None, true).await?
            };
            // Yield each completion
            for completion in completions {
                yield ImportExportItem::Completion(completion);
            }
        };

        Ok(Box::pin(export_stream))
    }

    /// Exports given commands and completions
    pub async fn export_items(
        &self,
        items: ImportExportStream,
        args: ExportItemsProcess,
        gist_config: GistConfig,
    ) -> Result<ExportStats> {
        let ExportItemsProcess {
            location,
            file,
            http,
            gist,
            filter: _,
            headers,
            method,
        } = args;

        if file {
            if location == "-" {
                self.export_stdout_items(items).await
            } else {
                self.export_file_items(items, location).await
            }
        } else if http {
            self.export_http_items(items, location, headers, method).await
        } else if gist {
            self.export_gist_items(items, location, gist_config).await
        } else {
            // Determine which mode based on the location
            if location == "gist"
                || location.starts_with("https://gist.github.com")
                || location.starts_with("https://gist.githubusercontent.com")
                || location.starts_with("https://api.github.com/gists")
            {
                self.export_gist_items(items, location, gist_config).await
            } else if location.starts_with("http://") || location.starts_with("https://") {
                self.export_http_items(items, location, headers, method).await
            } else if location == "-" {
                self.export_stdout_items(items).await
            } else {
                self.export_file_items(items, location).await
            }
        }
    }

    #[instrument(skip_all)]
    async fn export_stdout_items(&self, mut items: ImportExportStream) -> Result<ExportStats> {
        tracing::info!("Writing items to stdout");
        let mut stats = ExportStats::default();
        let mut stdout = String::new();
        while let Some(item) = items.next().await {
            stdout += &match item? {
                ImportExportItem::Command(c) => {
                    stats.commands_exported += 1;
                    c.to_string()
                }
                ImportExportItem::Completion(c) => {
                    stats.completions_exported += 1;
                    c.to_string()
                }
            };
            stdout += "\n";
        }
        stats.stdout = Some(stdout);
        Ok(stats)
    }

    #[instrument(skip_all)]
    async fn export_file_items(&self, mut items: ImportExportStream, path: String) -> Result<ExportStats> {
        let mut file = match File::create(&path).await {
            Ok(f) => f,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                return Err(UserFacingError::FileNotAccessible("write").into());
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(UserFacingError::ExportFileParentNotFound.into());
            }
            Err(err) if err.kind() == ErrorKind::IsADirectory => {
                return Err(UserFacingError::ExportLocationNotAFile.into());
            }
            Err(err) => return Err(Report::from(err).into()),
        };
        tracing::info!("Writing items to file: {path}");

        let mut stats = ExportStats::default();
        while let Some(item) = items.next().await {
            let content = match item? {
                ImportExportItem::Command(c) => {
                    stats.commands_exported += 1;
                    format!("{c}\n")
                }
                ImportExportItem::Completion(c) => {
                    stats.completions_exported += 1;
                    format!("{c}\n")
                }
            };
            file.write_all(content.as_bytes()).await.map_err(|err| {
                if err.kind() == ErrorKind::BrokenPipe {
                    AppError::from(UserFacingError::FileBrokenPipe)
                } else {
                    AppError::from(err)
                }
            })?;
        }
        file.flush().await?;
        Ok(stats)
    }

    #[instrument(skip_all)]
    async fn export_http_items(
        &self,
        mut items: ImportExportStream,
        url: String,
        headers: Vec<(HeaderName, HeaderValue)>,
        method: HttpMethod,
    ) -> Result<ExportStats> {
        // Parse the URL
        let url = Url::parse(&url).map_err(|err| {
            tracing::error!("Couldn't parse url: {err}");
            UserFacingError::HttpInvalidUrl
        })?;

        let method = method.into();
        tracing::info!("Writing items to http: {method} {url}");

        // Collect items to export
        let mut stats = ExportStats::default();
        let mut items_to_export = Vec::new();
        while let Some(item) = items.next().await {
            items_to_export.push(match item? {
                ImportExportItem::Command(c) => {
                    stats.commands_exported += 1;
                    ImportExportItemDto::Command(c.into())
                }
                ImportExportItem::Completion(c) => {
                    stats.completions_exported += 1;
                    ImportExportItemDto::Completion(c.into())
                }
            });
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
        req = req.json(&items_to_export);

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

        Ok(stats)
    }

    #[instrument(skip_all)]
    async fn export_gist_items(
        &self,
        mut items: ImportExportStream,
        gist: String,
        gist_config: GistConfig,
    ) -> Result<ExportStats> {
        // Retrieve the gist id and optional sha and file
        let (gist_id, gist_sha, gist_file) = extract_gist_data(&gist, &gist_config)?;

        // If a sha is found, return an error as we can't modify it
        if gist_sha.is_some() {
            return Err(UserFacingError::ExportGistLocationHasSha.into());
        }

        // Retrieve the gist token to be used
        let gist_token = get_export_gist_token(&gist_config)?;

        let url = format!("https://api.github.com/gists/{gist_id}");
        tracing::info!("Writing items to gist: {url}");

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
        let actual_gist: GistDto = match res.json().await {
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

        // Determine the extension based on the file or shell
        let extension = if let Some(ref gist_file) = gist_file
            && let Some((_, ext)) = gist_file.rfind('.').map(|i| gist_file.split_at(i))
        {
            ext.to_owned()
        } else {
            match get_shell_type() {
                ShellType::Cmd => ".cmd",
                ShellType::WindowsPowerShell | ShellType::PowerShellCore => ".ps1",
                _ => ".sh",
            }
            .to_owned()
        };

        // Collect items to export
        let mut stats = ExportStats::default();
        let mut content = String::new();
        while let Some(item) = items.next().await {
            match item? {
                ImportExportItem::Command(c) => {
                    stats.commands_exported += 1;
                    content.push_str(&c.to_string());
                }
                ImportExportItem::Completion(c) => {
                    stats.completions_exported += 1;
                    content.push_str(&c.to_string());
                }
            }
            content.push('\n');
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
            && !actual_gist.files.contains_key(GIST_README_FILENAME)
            && !actual_gist.files.contains_key(GIST_README_FILENAME_UPPER)
        {
            files.push((
                String::from(GIST_README_FILENAME),
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
                UserFacingError::GistRequestFailed(err.to_string())
            })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if status == StatusCode::NOT_FOUND {
                tracing::error!("Update got not found after a succesful get request");
                return Err(
                    UserFacingError::GistRequestFailed("token missing permissions to update the gist".into()).into(),
                );
            } else if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(
                    UserFacingError::GistRequestFailed(format!("received {status_str} {reason} response")).into(),
                );
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(UserFacingError::GistRequestFailed(format!("received {status_str} response")).into());
            }
        }

        Ok(stats)
    }
}
