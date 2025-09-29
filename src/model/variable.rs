/// Type to represent a variable value
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct VariableValue {
    /// The unique identifier for the value (if stored)
    pub id: Option<i32>,
    /// The flattened root command (i.e., the first word)
    pub flat_root_cmd: String,
    /// The flattened variable name (or multiple, e.g., "var1|var2")
    pub flat_variable: String,
    /// The variable value
    pub value: String,
}

impl VariableValue {
    /// Creates a new variable value
    pub fn new(
        flat_root_cmd: impl Into<String>,
        flat_variable_name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            flat_root_cmd: flat_root_cmd.into(),
            flat_variable: flat_variable_name.into(),
            value: value.into(),
        }
    }
}

/// Suggestion for a variable value
#[cfg_attr(test, derive(Debug))]
pub enum VariableSuggestion {
    /// A new secret value, the user must input it and it won't be stored
    Secret,
    /// A new value, if the user enters it, it must be then stored
    New,
    /// A value previously selected on the same command for this variable
    Previous(String),
    /// Suggestion from the environment variables
    Environment {
        env_var_name: String,
        value: Option<String>,
    },
    /// Suggestion for an already-stored value
    Existing(VariableValue),
    /// Suggestion from a variable completion
    Completion(String),
    /// Literal suggestion, derived from the variable name itself
    Derived(String),
}
