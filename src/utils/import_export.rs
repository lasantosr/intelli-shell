use std::env;

use itertools::Itertools;
use reqwest::Url;

use crate::{
    config::GistConfig,
    errors::{Result, UserFacingError},
};

/// Retrieves a GitHub personal access token for gist, checking configuration and environment variables.
///
/// This function attempts to find a token by searching in the following locations, in order of
/// precedence:
///
/// 1. The `GIST_TOKEN` environment variable
/// 2. The `token` field of the provided `gist_config` object
///
/// If a token is not found in any of these locations, the function will return an error.
pub fn get_export_gist_token(gist_config: &GistConfig) -> Result<String> {
    if let Ok(token) = env::var("GIST_TOKEN")
        && !token.is_empty()
    {
        Ok(token)
    } else if !gist_config.token.is_empty() {
        Ok(gist_config.token.clone())
    } else {
        Err(UserFacingError::ExportGistMissingToken.into())
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
pub fn extract_gist_data(location: &str, gist_config: &GistConfig) -> Result<(String, Option<String>, Option<String>)> {
    let location = location.trim();
    if location.is_empty() || location == "gist" {
        if !gist_config.id.is_empty() {
            Ok((gist_config.id.clone(), None, None))
        } else {
            Err(UserFacingError::GistMissingId.into())
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
                        return Err(UserFacingError::GistInvalidLocation.into());
                    }
                    let id = segments[1].to_string();
                    let mut sha = None;
                    if segments.len() > 2 {
                        if is_sha(segments[2]) {
                            sha = Some(segments[2].to_string());
                        } else {
                            return Err(UserFacingError::GistInvalidLocation.into());
                        }
                    }
                    (id, sha, None)
                }
                "gist.githubusercontent.com" => {
                    // Handles: https://gist.githubusercontent.com/{user}/{id}/raw/{sha?}/{file?}
                    if segments.len() < 3 || segments[2] != "raw" {
                        return Err(UserFacingError::GistInvalidLocation.into());
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
                        return Err(UserFacingError::GistInvalidLocation.into());
                    }
                    let id = segments[1].to_string();
                    let mut sha = None;
                    if segments.len() > 2 {
                        if is_sha(segments[2]) {
                            sha = Some(segments[2].to_string());
                        } else {
                            return Err(UserFacingError::GistInvalidLocation.into());
                        }
                    }
                    (id, sha, None)
                }
                // Any other host is considered an invalid location
                _ => return Err(UserFacingError::GistInvalidLocation.into()),
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
                    return Err(UserFacingError::GistMissingId.into());
                }
            }
            // Handles:
            // - {id}/{file}
            // - {id}/{sha}
            2 => {
                if is_id(parts[0]) {
                    id = parts[0].to_string();
                } else {
                    return Err(UserFacingError::GistInvalidLocation.into());
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
                    return Err(UserFacingError::GistInvalidLocation.into());
                }
                if is_sha(parts[1]) {
                    sha = Some(parts[1].to_string());
                } else {
                    return Err(UserFacingError::GistInvalidLocation.into());
                }
                file = Some(parts[2].to_string());
            }
            // Too many segments
            _ => {
                return Err(UserFacingError::GistInvalidLocation.into());
            }
        }

        Ok((id, sha, file))
    }
}

/// Converts a GitHub file URL to its raw.githubusercontent.com equivalent.
///
/// It handles URLs in the format: `https://github.com/{user}/{repo}/blob/{branch_or_commit}/{file_path}`.
/// It correctly ignores any query parameters or fragments in the original URL.
pub fn github_to_raw(url: &Url) -> Option<Url> {
    // Skip non-github urls
    if url.host_str() != Some("github.com") {
        return None;
    }

    let segments: Vec<&str> = url.path_segments()?.collect();

    // A valid URL must have at least 4 parts: user, repo, "blob", and a branch/commit name
    // We search for the "blob" segment, which separates the repo info from the file path
    if let Some(blob_pos) = segments.iter().position(|&s| s == "blob") {
        // The expected structure is /<user>/<repo>/blob/...
        // So, "blob" must be the third segment (index 2)
        if blob_pos != 2 {
            return None;
        }

        let user = segments[0];
        let repo = segments[1];

        // The parts after "blob" are the branch/commit and the file path
        let rest_of_path = &segments[blob_pos + 1..];
        if rest_of_path.len() < 2 {
            return None;
        }

        // Assemble the new raw content URL
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            user,
            repo,
            rest_of_path.join("/")
        );

        Url::parse(&raw_url).ok()
    } else {
        // If "blob" is not in the path, it's not a URL we can convert (e.g., it might be a /tree/ URL)
        None
    }
}

/// Adds tags to a description, only those not already present will be added
pub fn add_tags_to_description(tags: &[String], mut description: String) -> String {
    let tags = tags.iter().filter(|tag| !description.contains(*tag)).join(" ");
    if !tags.is_empty() {
        let multiline = description.contains('\n');
        if multiline {
            description += "\n";
        } else if !description.is_empty() {
            description += " ";
        }
        description += &tags;
    }
    description
}

/// Data Transfer Objects when importing and exporting
pub mod dto {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use crate::model::{CATEGORY_USER, Command, ImportExportItem, SOURCE_IMPORT, VariableCompletion};

    pub const GIST_README_FILENAME: &str = "readme.md";
    pub const GIST_README_FILENAME_UPPER: &str = "README.md";

    #[derive(Serialize, Deserialize)]
    #[cfg_attr(test, derive(Debug))]
    #[serde(untagged)]
    pub enum ImportExportItemDto {
        Command(CommandDto),
        Completion(VariableCompletionDto),
    }
    impl From<ImportExportItemDto> for ImportExportItem {
        fn from(value: ImportExportItemDto) -> Self {
            match value {
                ImportExportItemDto::Command(dto) => ImportExportItem::Command(dto.into()),
                ImportExportItemDto::Completion(dto) => ImportExportItem::Completion(dto.into()),
            }
        }
    }
    impl From<ImportExportItem> for ImportExportItemDto {
        fn from(value: ImportExportItem) -> Self {
            match value {
                ImportExportItem::Command(c) => ImportExportItemDto::Command(c.into()),
                ImportExportItem::Completion(c) => ImportExportItemDto::Completion(c.into()),
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    #[cfg_attr(test, derive(Debug))]
    pub struct CommandDto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub alias: Option<String>,
        pub cmd: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
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
    #[cfg_attr(test, derive(Debug))]
    pub struct VariableCompletionDto {
        pub command: String,
        pub variable: String,
        pub provider: String,
    }
    impl From<VariableCompletionDto> for VariableCompletion {
        fn from(value: VariableCompletionDto) -> Self {
            VariableCompletion::new(SOURCE_IMPORT, value.command, value.variable, value.provider)
        }
    }
    impl From<VariableCompletion> for VariableCompletionDto {
        fn from(value: VariableCompletion) -> Self {
            VariableCompletionDto {
                command: value.flat_root_cmd,
                variable: value.flat_variable,
                provider: value.suggestions_provider,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    #[cfg_attr(test, derive(Debug))]
    pub struct GistDto {
        pub files: HashMap<String, GistFileDto>,
    }
    #[derive(Serialize, Deserialize)]
    #[cfg_attr(test, derive(Debug))]
    pub struct GistFileDto {
        pub content: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_GIST_ID: &str = "b3a462e23db5c99d1f3f4abf0dae5bd8";
    const TEST_GIST_SHA: &str = "330286d6e41f8ae0a5b4ddc3e01d5521b87a15ca";
    const TEST_GIST_FILE: &str = "my_commands.sh";

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

    #[test]
    fn test_github_to_url_valid() {
        let github_url = Url::parse("https://github.com/rust-lang/rust/blob/master/README.md").unwrap();
        let expected = Url::parse("https://raw.githubusercontent.com/rust-lang/rust/master/README.md").unwrap();
        assert_eq!(github_to_raw(&github_url), Some(expected));
    }

    #[test]
    fn test_github_to_url_with_subdirectories() {
        let github_url = Url::parse("https://github.com/user/repo/blob/main/src/app/main.rs").unwrap();
        let expected = Url::parse("https://raw.githubusercontent.com/user/repo/main/src/app/main.rs").unwrap();
        assert_eq!(github_to_raw(&github_url), Some(expected));
    }

    #[test]
    fn test_github_to_url_with_commit_hash() {
        let github_url = Url::parse("https://github.com/user/repo/blob/a1b2c3d4e5f6/path/to/file.txt").unwrap();
        let expected = Url::parse("https://raw.githubusercontent.com/user/repo/a1b2c3d4e5f6/path/to/file.txt").unwrap();
        assert_eq!(github_to_raw(&github_url), Some(expected));
    }

    #[test]
    fn test_github_to_url_invalid_domain() {
        let url = Url::parse("https://gitlab.com/user/repo/blob/main/file.txt").unwrap();
        assert_eq!(github_to_raw(&url), None);
    }

    #[test]
    fn test_github_to_url_not_a_blob() {
        let url = Url::parse("https://github.com/user/repo/tree/main/src").unwrap();
        assert_eq!(github_to_raw(&url), None);
    }

    #[test]
    fn test_github_to_url_root_repo() {
        let url = Url::parse("https://github.com/user/repo").unwrap();
        assert_eq!(github_to_raw(&url), None);
    }

    #[test]
    fn test_github_to_url_with_query_params_and_fragment() {
        let github_url = Url::parse("https://github.com/user/repo/blob/main/file.txt?raw=true#L10").unwrap();
        let expected = Url::parse("https://raw.githubusercontent.com/user/repo/main/file.txt").unwrap();
        assert_eq!(github_to_raw(&github_url), Some(expected));
    }

    #[test]
    fn test_github_to_url_with_insufficient_segments() {
        let url = Url::parse("https://github.com/user/repo/blob/").unwrap();
        assert_eq!(github_to_raw(&url), None);
    }
}
