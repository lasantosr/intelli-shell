use std::{
    collections::{BTreeMap, HashMap},
    env,
};

use heck::ToShoutySnakeCase;
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    errors::Result,
    model::{CommandPart, DynamicCommand, Variable, VariableSuggestion, VariableValue},
    utils::{format_env_var, get_working_dir},
};

impl IntelliShellService {
    /// Replaces the variables found in a command with their values.
    ///
    /// If one or more values are not found, they will be returned as an error.
    #[instrument(skip_all)]
    pub fn replace_command_variables(
        &self,
        command: String,
        values: Vec<(String, Option<String>)>,
        use_env: bool,
    ) -> Result<String, Vec<String>> {
        // Collect the values into a map for fast access
        let values: HashMap<String, Option<String>> =
            values.into_iter().map(|(n, v)| (n.to_shouty_snake_case(), v)).collect();

        let mut output = String::new();
        let mut missing = Vec::new();

        // Parse the command into a dynamic one
        let dynamic = DynamicCommand::parse(command, true);
        // For each one of the parts
        for part in dynamic.parts {
            match part {
                // Just parsed commands doesn't contain variable values
                CommandPart::VariableValue(_, _) => unreachable!(),
                // Text parts are not variables so we can push them directly to the output
                CommandPart::Text(t) => output.push_str(&t),
                // Variables must be replaced
                CommandPart::Variable(v) => {
                    let env_var_names = v.env_var_names(false);
                    let variable_value = env_var_names.iter().find_map(|env_var_name| values.get(env_var_name));
                    match (variable_value, use_env) {
                        // If the variable is present on the values map and it has a value
                        (Some(Some(value)), _) => {
                            // Push it to the output after applying the functions
                            output.push_str(&v.apply_functions_to(value));
                        }
                        // If the variable is present on the map without a value, or the env can be read
                        (Some(None), _) | (None, true) => {
                            // Check the env
                            let variable_value_env = env_var_names
                                .iter()
                                .find_map(|env_var_name| env::var(env_var_name).ok().map(|v| (env_var_name, v)));
                            match (variable_value_env, v.secret) {
                                // If there's no env var available, the value is missing
                                (None, _) => missing.push(v.name),
                                // If there' a value for a non-secret variable, push it
                                (Some((_, env_value)), false) => {
                                    output.push_str(&v.apply_functions_to(env_value));
                                }
                                // If there's a value but the variable is secret
                                (Some((env_var_name, _)), true) => {
                                    // Use the env var itself instead of the value, to avoid exposing the secret
                                    output.push_str(&format_env_var(env_var_name));
                                }
                            }
                        }
                        // Otherwise, the variable value is missing
                        _ => {
                            missing.push(v.name);
                        }
                    }
                }
            }
        }

        if !missing.is_empty() { Err(missing) } else { Ok(output) }
    }

    /// Searches suggestions for the given variable
    #[instrument(skip_all)]
    pub async fn search_variable_suggestions(
        &self,
        root_cmd: &str,
        variable: &Variable,
        context: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Vec<VariableSuggestion>> {
        tracing::info!("Searching for variable suggestions: [{root_cmd}] {}", variable.name);

        let mut suggestions = Vec::new();

        if variable.secret {
            // If the variable is a secret, suggest a new secret value
            suggestions.push(VariableSuggestion::Secret);
            // And check if there's any env var that matches the variable to include it as a suggestion
            for env_var_name in variable.env_var_names(true) {
                if env::var(&env_var_name).is_ok() {
                    suggestions.push(VariableSuggestion::Environment {
                        env_var_name,
                        value: None,
                    });
                }
            }
        } else {
            // Otherwise it's a regular variable, suggest a new value
            suggestions.push(VariableSuggestion::New);

            // Find sorted values for the variable
            let context = BTreeMap::from_iter(context);
            let mut existing_values = self
                .storage
                .find_variable_values(
                    root_cmd,
                    &variable.name,
                    get_working_dir(),
                    &context,
                    &self.tuning.variables,
                )
                .await?;

            // If there's a suggestion for a value previously selected for the same variable
            let previous_value = context.get(&variable.name).cloned();
            if let Some(previous_value) = previous_value
                && let Some(index) = existing_values.iter().position(|s| s.value == previous_value)
            {
                // Include it first, ignoring its relevance ordering
                suggestions.push(VariableSuggestion::Existing(existing_values.remove(index)));
            }

            // Check if there's any env var that matches the variable to include it as a suggestion
            for env_var_name in variable.env_var_names(true) {
                if let Ok(value) = env::var(&env_var_name)
                    && !value.trim().is_empty()
                {
                    let value = variable.apply_functions_to(value);
                    // Check if there's already an existing value for that suggestion
                    if let Some(index) = existing_values.iter().position(|s| s.value == value) {
                        // Include it now, ignoring its ordering
                        suggestions.push(VariableSuggestion::Existing(existing_values.remove(index)));
                    } else {
                        // Otherwise, include the environment suggestion
                        suggestions.push(VariableSuggestion::Environment {
                            env_var_name,
                            value: Some(value),
                        });
                    };
                }
            }
            // Include remaining existing values
            suggestions.extend(existing_values.into_iter().map(VariableSuggestion::Existing));
            // And suggestions from the variable options (not already present)
            let options = variable
                .options
                .iter()
                .filter(|o| {
                    !suggestions.iter().any(|s| match s {
                        VariableSuggestion::Environment { value: Some(value), .. } => value == *o,
                        VariableSuggestion::Existing(sv) => &sv.value == *o,
                        _ => false,
                    })
                })
                .map(|o| VariableSuggestion::Derived(o.to_owned()))
                .collect::<Vec<_>>();
            suggestions.extend(options);
        }

        Ok(suggestions)
    }

    /// Inserts a new variable value
    #[instrument(skip_all)]
    pub async fn insert_variable_value(&self, value: VariableValue) -> Result<VariableValue> {
        tracing::info!(
            "Inserting a variable value for '{}' '{}': {}",
            value.flat_root_cmd,
            value.flat_variable,
            value.value
        );
        self.storage.insert_variable_value(value).await
    }

    /// Updates an existing variable value
    #[instrument(skip_all)]
    pub async fn update_variable_value(&self, value: VariableValue) -> Result<VariableValue> {
        tracing::info!(
            "Updating variable value '{}': {}",
            value.id.unwrap_or_default(),
            value.value
        );
        self.storage.update_variable_value(value).await
    }

    /// Increases the usage of a variable value, returning the new usage count
    #[instrument(skip_all)]
    pub async fn increment_variable_value_usage(
        &self,
        value_id: i32,
        context: impl IntoIterator<Item = (String, String)>,
    ) -> Result<i32> {
        tracing::info!("Increasing usage for variable value '{value_id}'");
        let context = BTreeMap::from_iter(context);
        self.storage
            .increment_variable_value_usage(value_id, get_working_dir(), &context)
            .await
    }

    /// Deletes an existing variable value
    #[instrument(skip_all)]
    pub async fn delete_variable_value(&self, id: i32) -> Result<()> {
        tracing::info!("Deleting variable value: {}", id);
        self.storage.delete_variable_value(id).await
    }
}
