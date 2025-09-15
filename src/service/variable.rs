use std::{
    collections::{BTreeMap, HashMap},
    env,
};

use futures_util::{Stream, StreamExt};
use heck::ToShoutySnakeCase;
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    errors::Result,
    model::{CommandTemplate, TemplatePart, Variable, VariableSuggestion, VariableValue},
    utils::{format_env_var, get_working_dir, resolve_completions},
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

        // Parse the command into a template
        let template = CommandTemplate::parse(command, true);
        // For each one of the parts
        for part in template.parts {
            match part {
                // Just parsed commands doesn't contain variable values
                TemplatePart::VariableValue(_, _) => unreachable!(),
                // Text parts are not variables so we can push them directly to the output
                TemplatePart::Text(t) => output.push_str(&t),
                // Variables must be replaced
                TemplatePart::Variable(v) => {
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
                                (None, _) => missing.push(v.display),
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
                            missing.push(v.display);
                        }
                    }
                }
            }
        }

        if !missing.is_empty() { Err(missing) } else { Ok(output) }
    }

    /// Searches suggestions for the given variable
    #[instrument(skip_all)]
    #[allow(clippy::type_complexity)]
    pub async fn search_variable_suggestions(
        &self,
        flat_root_cmd: &str,
        variable: &Variable,
        previous_values: Option<Vec<String>>,
        context: BTreeMap<String, String>,
    ) -> Result<(
        Vec<(u8, VariableSuggestion, f64)>,
        Option<impl Stream<Item = (f64, Result<Vec<String>, String>)> + use<>>,
    )> {
        tracing::info!(
            "Searching for variable suggestions: [{flat_root_cmd}] {}",
            variable.flat_name
        );

        let mut suggestions = Vec::new();
        if variable.secret {
            // If the variable is a secret, suggest a new secret value
            suggestions.push((0, VariableSuggestion::Secret, 0.0));
            // Check if the user already selected any value for the same variable, to be suggested again
            if let Some(values) = previous_values {
                for (ix, value) in values.into_iter().enumerate() {
                    suggestions.push((1, VariableSuggestion::Previous(value), ix as f64));
                }
            }
            // And check if there's any env var that matches the variable to include it as a suggestion
            let env_var_names = variable.env_var_names(true);
            let env_var_len = env_var_names.len();
            for (rev_ix, env_var_name) in env_var_names
                .into_iter()
                .enumerate()
                .map(|(ix, item)| (env_var_len - 1 - ix, item))
            {
                if env::var(&env_var_name).is_ok() {
                    suggestions.push((
                        2,
                        VariableSuggestion::Environment {
                            env_var_name,
                            value: None,
                        },
                        rev_ix as f64,
                    ));
                }
            }
        } else {
            // Otherwise it's a regular variable, suggest a new value
            suggestions.push((0, VariableSuggestion::New, 0.0));

            // Find existing stored values for the variable
            let mut existing_values = self
                .storage
                .find_variable_values(
                    flat_root_cmd,
                    &variable.flat_name,
                    variable.flat_names.clone(),
                    get_working_dir(),
                    &context,
                    &self.tuning.variables,
                )
                .await?;

            // Check if the user already selected some value for the same variable
            if let Some(values) = previous_values {
                for (ix, value) in values.into_iter().enumerate() {
                    // If there's an existing suggestion for a value previously selected
                    if let Some(index) = existing_values.iter().position(|(s, _)| s.value == value) {
                        // Include it first, ignoring its relevance ordering
                        let (existing, _) = existing_values.remove(index);
                        suggestions.push((1, VariableSuggestion::Existing(existing), ix as f64));
                    } else {
                        // If there's no stored value (previous value was secret), suggest it
                        suggestions.push((1, VariableSuggestion::Previous(value), ix as f64));
                    }
                }
            }

            // Check if there's any env var that matches the variable to include it as a suggestion
            let env_var_names = variable.env_var_names(true);
            let env_var_len = env_var_names.len();
            for (rev_ix, env_var_name) in env_var_names
                .into_iter()
                .enumerate()
                .map(|(ix, item)| (env_var_len - 1 - ix, item))
            {
                if let Ok(value) = env::var(&env_var_name)
                    && !value.trim().is_empty()
                {
                    let value = variable.apply_functions_to(value);
                    // Check if there's already an existing value for that suggestion
                    if let Some(existing_index) = existing_values.iter().position(|(s, _)| s.value == value) {
                        // Include it now, ignoring its ordering
                        let (existing, _) = existing_values.remove(existing_index);
                        suggestions.push((2, VariableSuggestion::Existing(existing), rev_ix as f64));
                    } else {
                        // Otherwise, include the environment suggestion
                        suggestions.push((
                            2,
                            VariableSuggestion::Environment {
                                env_var_name,
                                value: Some(value),
                            },
                            rev_ix as f64,
                        ));
                    };
                }
            }

            // Include Remaining existing suggestions
            suggestions.extend(
                existing_values
                    .into_iter()
                    .map(|(s, score)| (3, VariableSuggestion::Existing(s), score)),
            );

            // And suggestions from the variable options (not already present)
            let options = variable
                .options
                .iter()
                .filter(|o| {
                    !suggestions.iter().any(|(_, s, _)| match s {
                        VariableSuggestion::Environment { value: Some(value), .. } => value == *o,
                        VariableSuggestion::Existing(sv) => &sv.value == *o,
                        VariableSuggestion::Completion(value) => value == *o,
                        _ => false,
                    })
                })
                .map(|o| (4, VariableSuggestion::Derived(o.to_owned()), 0.0))
                .collect::<Vec<_>>();
            suggestions.extend(options);
        }

        // Find and stream completions for the variable, which might be slow for network related completions
        let completions = self
            .storage
            .get_completions_for(flat_root_cmd, variable.flat_names.clone())
            .await?;
        let completion_stream = if !completions.is_empty() {
            let completion_points = self.tuning.variables.completion.points as f64;
            let stream = resolve_completions(completions, context.clone()).await;
            Some(stream.map(move |(score_boost, result)| (completion_points + score_boost, result)))
        } else {
            None
        };

        Ok((suggestions, completion_stream))
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
        context: BTreeMap<String, String>,
    ) -> Result<i32> {
        tracing::info!("Increasing usage for variable value '{value_id}'");
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
