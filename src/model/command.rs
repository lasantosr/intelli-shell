use std::{
    collections::HashSet,
    fmt::{self, Display},
};

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use enum_cycling::EnumCycle;
use regex::Regex;
use serde::Deserialize;
use uuid::Uuid;

use crate::utils::{extract_tags_from_description, flatten_str, remove_newlines};

/// Category for user defined commands
pub const CATEGORY_USER: &str = "user";

/// Category for workspace defined commands
pub const CATEGORY_WORKSPACE: &str = "workspace";

/// Source for user defined commands
pub const SOURCE_USER: &str = "user";

/// Source for ai suggested commands
pub const SOURCE_AI: &str = "ai";

/// Source for tldr fetched commands
pub const SOURCE_TLDR: &str = "tldr";

/// Source for imported commands
pub const SOURCE_IMPORT: &str = "import";

/// Source for workspace-level commands
pub const SOURCE_WORKSPACE: &str = "workspace";

const DESTRUCTIVE_COMMANDS: &[&str] = &["rm", "rmdir", "del", "erase", "rd", "remove-item"];
const PRIVILEGE_WRAPPERS: &[&str] = &["sudo", "doas"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, ValueEnum, EnumCycle, strum::Display)]
#[cfg_attr(test, derive(strum::EnumIter))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
/// Determines the strategy used for searching commands
pub enum SearchMode {
    /// An internal algorithm will be used to understand human search patterns and decide the best search strategy
    #[default]
    Auto,
    /// Employs a set of predefined rules to perform a fuzzy search
    Fuzzy,
    /// Treats the input query as a regular expression, allowing for complex pattern matching
    Regex,
    /// Return commands that precisely match the entire input query only
    Exact,
    /// Attempts to find the maximum number of potentially relevant commands.
    ///
    /// It uses a broader set of matching criteria and may include partial matches, matches within descriptions, or
    /// commands that share keywords.
    Relaxed,
}

/// Represents the filtering criteria for searching for commands
#[derive(Default, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct SearchCommandsFilter {
    /// Filter commands by a specific category (`user`, `workspace` or tldr's category)
    pub category: Option<Vec<String>>,
    /// Filter commands by their original source (`user`, `ai`, `tldr`, `import`, `workspace`)
    pub source: Option<String>,
    /// Filter commands by a list of tags, only commands matching all of the provided tags will be included
    pub tags: Option<Vec<String>>,
    /// Specifies the search strategy to be used for matching the `search_term`
    pub search_mode: SearchMode,
    /// The actual term or query string to search for.
    ///
    /// This term will be matched against command names, aliases, or descriptions according to the specified
    /// `search_mode`.
    pub search_term: Option<String>,
}
impl SearchCommandsFilter {
    /// Returns a cleaned version of self, trimming and removing empty or duplicated filters
    pub fn cleaned(self) -> Self {
        let SearchCommandsFilter {
            category,
            source,
            tags,
            search_mode,
            search_term,
        } = self;
        Self {
            category: category
                .map(|v| {
                    let mut final_vec: Vec<String> = Vec::with_capacity(v.len());
                    let mut seen: HashSet<&str> = HashSet::with_capacity(v.len());
                    for t in &v {
                        let t = t.trim();
                        if !t.is_empty() && seen.insert(t) {
                            final_vec.push(t.to_string());
                        }
                    }
                    final_vec
                })
                .filter(|t| !t.is_empty()),
            source: source.map(|t| t.trim().to_string()).filter(|s| !s.is_empty()),
            tags: tags
                .map(|v| {
                    let mut final_vec: Vec<String> = Vec::with_capacity(v.len());
                    let mut seen: HashSet<&str> = HashSet::with_capacity(v.len());
                    for t in &v {
                        let t = t.trim();
                        if !t.is_empty() && seen.insert(t) {
                            final_vec.push(t.to_string());
                        }
                    }
                    final_vec
                })
                .filter(|t| !t.is_empty()),
            search_mode,
            search_term: search_term.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
        }
    }
}

#[derive(Clone)]
#[cfg_attr(test, derive(Default, Debug))]
pub struct Command {
    /// Unique identifier for the command
    pub id: Uuid,
    /// Category of the command (`user`, `workspace` or tldr's category)
    pub category: String,
    /// Category of the command (`user`, `ai`, `tldr`, `import`, `workspace`)
    pub source: String,
    /// Optional alias for easier recall
    pub alias: Option<String>,
    /// The actual command string, potentially with `{{placeholders}}`
    pub cmd: String,
    /// Flattened version of `cmd`
    pub flat_cmd: String,
    /// Optional user-provided description
    pub description: Option<String>,
    /// Flattened version of `description`
    pub flat_description: Option<String>,
    /// Tags associated with the command (including the hashtag `#`)
    pub tags: Option<Vec<String>>,
    /// The date and time when the command was created
    pub created_at: DateTime<Utc>,
    /// The date and time when the command was last updated
    pub updated_at: Option<DateTime<Utc>>,
}

impl Command {
    /// Creates a new command, with zero usage
    pub fn new(category: impl Into<String>, source: impl Into<String>, cmd: impl Into<String>) -> Self {
        let cmd = remove_newlines(cmd.into());
        Self {
            id: Uuid::now_v7(),
            category: category.into(),
            source: source.into(),
            alias: None,
            flat_cmd: flatten_str(&cmd),
            cmd,
            description: None,
            flat_description: None,
            tags: None,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    /// Updates the alias of the command
    pub fn with_alias(mut self, alias: Option<String>) -> Self {
        self.alias = alias.filter(|a| !a.trim().is_empty());
        self
    }

    /// Updates the cmd of the command
    pub fn with_cmd(mut self, cmd: String) -> Self {
        self.flat_cmd = flatten_str(&cmd);
        self.cmd = cmd;
        self
    }

    /// Updates the description (and tags) of the command
    pub fn with_description(mut self, description: Option<String>) -> Self {
        let description = description.filter(|d| !d.trim().is_empty());
        self.tags = extract_tags_from_description(description.as_deref());
        self.flat_description = description.as_ref().map(flatten_str);
        self.description = description;
        self
    }

    #[cfg(test)]
    /// Updates the tags of the command
    pub fn with_tags(mut self, tags: Option<Vec<String>>) -> Self {
        self.tags = tags.filter(|t| !t.is_empty());
        self
    }

    /// Checks whether a command matches a regex filter
    pub fn matches(&self, regex: &Regex) -> bool {
        regex.is_match(&self.cmd) || self.description.as_ref().is_some_and(|d| regex.is_match(d))
    }

    /// Checks whether the command string contains a destructive shell action.
    pub fn is_destructive(&self) -> bool {
        Self::is_destructive_command(&self.cmd)
    }

    /// Checks whether a command string contains a destructive shell action.
    pub fn is_destructive_command(command: &str) -> bool {
        split_shell_segments(command).into_iter().any(is_destructive_segment)
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Get the description and alias, treating empty strings or None as absent
        let cmd = &self.cmd;
        let desc = self.description.as_deref().filter(|s| !s.is_empty());
        let alias = self.alias.as_deref();

        match (desc, alias) {
            // If there's no description or alias, output an empty comment and the command
            (None, None) => return writeln!(f, "#\n{cmd}"),
            // Both description and alias exist
            (Some(d), Some(a)) => {
                if d.contains('\n') {
                    // For multi-line descriptions, place the alias on its own line for clarity
                    writeln!(f, "# [alias:{a}]")?;
                    for line in d.lines() {
                        writeln!(f, "# {line}")?;
                    }
                } else {
                    // For single-line descriptions, combine them on one line
                    writeln!(f, "# [alias:{a}] {d}")?;
                }
            }
            // Only a description exists
            (Some(d), None) => {
                for line in d.lines() {
                    writeln!(f, "# {line}")?;
                }
            }
            // Only an alias exists
            (None, Some(a)) => {
                writeln!(f, "# [alias:{a}]")?;
            }
        };

        // Finally, write the command itself
        writeln!(f, "{cmd}")
    }
}

fn is_destructive_segment(segment: &str) -> bool {
    let mut words = ShellWordIter::new(segment);

    for word in words.by_ref() {
        if is_env_assignment(word) || is_privilege_wrapper(word) {
            continue;
        }

        return is_destructive_verb(word) || is_destructive_subcommand(word, &mut words);
    }

    false
}

fn is_destructive_verb(word: &str) -> bool {
    DESTRUCTIVE_COMMANDS.iter().any(|verb| word.eq_ignore_ascii_case(verb))
}

fn is_privilege_wrapper(word: &str) -> bool {
    PRIVILEGE_WRAPPERS.iter().any(|wrapper| word.eq_ignore_ascii_case(wrapper))
}

fn is_destructive_subcommand(command: &str, remaining_words: &mut ShellWordIter<'_>) -> bool {
    if !command.eq_ignore_ascii_case("git") {
        return false;
    }

    remaining_words
        .next()
        .is_some_and(is_destructive_verb)
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn split_shell_segments(command: &str) -> Vec<&str> {
    let bytes = command.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if escaped {
            escaped = false;
            index += 1;
            continue;
        }

        if let Some(active_quote) = quote {
            if byte == b'\\' && active_quote == b'"' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\\' => {
                escaped = true;
                index += 1;
            }
            b'\'' | b'"' => {
                quote = Some(byte);
                index += 1;
            }
            b';' | b'\n' => {
                segments.push(&command[start..index]);
                start = index + 1;
                index += 1;
            }
            b'&' if bytes.get(index + 1) == Some(&b'&') => {
                segments.push(&command[start..index]);
                start = index + 2;
                index += 2;
            }
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                segments.push(&command[start..index]);
                start = index + 2;
                index += 2;
            }
            b'|' => {
                segments.push(&command[start..index]);
                start = index + 1;
                index += 1;
            }
            _ => index += 1,
        }
    }

    segments.push(&command[start..]);
    segments
}

struct ShellWordIter<'a> {
    segment: &'a str,
    cursor: usize,
}

impl<'a> ShellWordIter<'a> {
    fn new(segment: &'a str) -> Self {
        Self { segment, cursor: 0 }
    }
}

impl<'a> Iterator for ShellWordIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.segment.as_bytes();

        while let Some(byte) = bytes.get(self.cursor) {
            if byte.is_ascii_whitespace() {
                self.cursor += 1;
            } else {
                break;
            }
        }

        if self.cursor >= bytes.len() {
            return None;
        }

        let start = self.cursor;
        let mut index = self.cursor;
        let mut quote: Option<u8> = None;
        let mut escaped = false;

        while index < bytes.len() {
            let byte = bytes[index];

            if escaped {
                escaped = false;
                index += 1;
                continue;
            }

            if let Some(active_quote) = quote {
                if byte == b'\\' && active_quote == b'"' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                index += 1;
                continue;
            }

            match byte {
                b'\\' => {
                    escaped = true;
                    index += 1;
                }
                b'\'' | b'"' => {
                    quote = Some(byte);
                    index += 1;
                }
                _ if byte.is_ascii_whitespace() => break,
                _ => index += 1,
            }
        }

        self.cursor = index;
        Some(&self.segment[start..index])
    }
}

#[cfg(test)]
mod tests {
    use super::{CATEGORY_USER, Command, SOURCE_USER};

    #[test]
    fn test_is_destructive_command_positive_cases() {
        for command in [
            "rm file",
            "sudo rm -rf /tmp/x",
            "VAR=1 rm file",
            "echo ok && rm file",
            "git rm file",
            "Remove-Item foo",
            "del foo",
        ] {
            assert!(Command::is_destructive_command(command), "expected destructive: {command}");
        }
    }

    #[test]
    fn test_is_destructive_command_negative_cases() {
        for command in [
            "docker run --rm image",
            "echo rm file",
            "printf 'rm file'",
            "git status",
            "rmdir_backup",
            "trash-put foo",
        ] {
            assert!(
                !Command::is_destructive_command(command),
                "expected non-destructive: {command}"
            );
        }
    }

    #[test]
    fn test_command_is_destructive_uses_command_text() {
        let command = Command::new(CATEGORY_USER, SOURCE_USER, "doas erase temp.txt");
        assert!(command.is_destructive());
    }
}
