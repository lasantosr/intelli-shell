use std::fmt;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::utils::{flatten_str, flatten_variable_name};

/// Type to represent a variable completion
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct VariableCompletion {
    /// The unique identifier for the completion
    pub id: Uuid,
    /// Category of the completion (`user`, `ai`, `tldr`, `import`, `workspace`)
    pub source: String,
    /// The root command (i.e., the first word)
    pub root_cmd: String,
    /// The flattened root command
    pub flat_root_cmd: String,
    /// The variable name
    pub variable: String,
    /// The flattened variable name
    pub flat_variable: String,
    /// The command to be executed to retrieve the suggestions
    pub suggestions_provider: String,
    /// The date and time when the completion was created
    pub created_at: DateTime<Utc>,
    /// The date and time when the completion was last updated
    pub updated_at: Option<DateTime<Utc>>,
}

impl VariableCompletion {
    /// Creates a new dynamic variable completion
    pub fn new(
        source: impl Into<String>,
        root_cmd: impl AsRef<str>,
        variable_name: impl AsRef<str>,
        suggestions_provider: impl Into<String>,
    ) -> Self {
        let root_cmd = root_cmd.as_ref().trim().to_string();
        let variable = variable_name.as_ref().trim().to_string();
        Self {
            id: Uuid::now_v7(),
            source: source.into(),
            flat_root_cmd: flatten_str(&root_cmd),
            root_cmd,
            flat_variable: flatten_variable_name(&variable),
            variable,
            suggestions_provider: suggestions_provider.into(),
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    /// Sets the root command of the variable completion
    pub fn with_root_cmd(mut self, root_cmd: impl AsRef<str>) -> Self {
        self.root_cmd = root_cmd.as_ref().trim().to_string();
        self.flat_root_cmd = flatten_str(&self.root_cmd);
        self
    }

    /// Sets the variable name of the variable completion
    pub fn with_variable(mut self, variable_name: impl AsRef<str>) -> Self {
        self.variable = variable_name.as_ref().trim().to_string();
        self.flat_variable = flatten_variable_name(&self.variable);
        self
    }

    /// Sets the suggestions command of the variable completion
    pub fn with_suggestions_provider(mut self, suggestions_provider: impl Into<String>) -> Self {
        self.suggestions_provider = suggestions_provider.into();
        self
    }

    /// Returns `true` if this is a global variable completion (without root command)
    pub fn is_global(&self) -> bool {
        self.flat_root_cmd.is_empty()
    }
}

impl fmt::Display for VariableCompletion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_global() {
            write!(f, "$ {}: {}", self.variable, self.suggestions_provider)
        } else {
            write!(
                f,
                "$ ({}) {}: {}",
                self.root_cmd, self.variable, self.suggestions_provider
            )
        }
    }
}
