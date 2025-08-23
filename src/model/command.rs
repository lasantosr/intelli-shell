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
#[cfg_attr(debug_assertions, derive(Debug))]
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
#[cfg_attr(debug_assertions, derive(Debug))]
#[cfg_attr(test, derive(Default))]
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
