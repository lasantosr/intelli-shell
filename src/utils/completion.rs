use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
    time::Duration,
};

use futures_util::{
    Stream,
    stream::{self, StreamExt},
};
use regex::{Captures, Regex};
use tracing::instrument;

use crate::{
    errors::UserFacingError,
    model::VariableCompletion,
    utils::{COMMAND_VARIABLE_REGEX, decode_output, flatten_variable_name, prepare_command_execution},
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetches suggestions from variable completions by executing their commands
///
/// Returns a stream where each item is a tuple of (score_boost, result)
pub async fn resolve_completions(
    completions: Vec<VariableCompletion>,
    context: BTreeMap<String, String>,
) -> impl Stream<Item = (f64, Result<Vec<String>, String>)> {
    let context = Arc::new(context);
    let num_completions = completions.len();

    stream::iter(completions.into_iter().enumerate())
        .map(move |(ix, completion)| {
            let context = context.clone();
            let score_boost = (num_completions - 1 - ix) as f64;
            async move {
                let result = resolve_completion(&completion, Some(context)).await;
                (score_boost, result)
            }
        })
        .buffer_unordered(4)
}

/// Fetches suggestions from a variable completion by executing its provider command
#[instrument(skip_all)]
pub async fn resolve_completion(
    completion: &VariableCompletion,
    context: Option<Arc<BTreeMap<String, String>>>,
) -> Result<Vec<String>, String> {
    // Resolve the command based on the context
    let command = resolve_suggestions_provider(&completion.suggestions_provider, context.as_deref());
    if command.is_empty() {
        return Err(UserFacingError::CompletionEmptySuggestionsProvider.to_string());
    }

    if completion.is_global() {
        tracing::info!("Resolving completion for global {} variable", completion.flat_variable);
    } else {
        tracing::info!(
            "Resolving completion for {} variable ({} command)",
            completion.flat_variable,
            completion.flat_root_cmd
        );
    }

    let mut cmd = prepare_command_execution(&command, false, false).expect("infallible");
    Ok(match tokio::time::timeout(COMMAND_TIMEOUT, cmd.output()).await {
        Err(_) => {
            tracing::warn!("Timeout executing dynamic completion command: '{command}'");
            return Err(String::from("Timeout executing command provider"));
        }
        Ok(Ok(output)) if output.status.success() => {
            let stdout = decode_output(&output.stdout);
            tracing::trace!("Output:\n{stdout}");
            let suggestions = stdout
                .lines()
                .map(String::from)
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>();
            tracing::debug!("Resolved {} suggestions", suggestions.len());
            suggestions
        }
        Ok(Ok(output)) => {
            let stderr = decode_output(&output.stderr);
            tracing::error!("Error executing dynamic completion command: '{command}':\n{stderr}");
            return Err(stderr.into());
        }
        Ok(Err(err)) => {
            tracing::error!("Failed to execute dynamic completion command: '{command}': {err}");
            return Err(err.to_string());
        }
    })
}

/// Resolves a command with implicit conditional blocks
fn resolve_suggestions_provider(suggestions_provider: &str, context: Option<&BTreeMap<String, String>>) -> String {
    /// Regex to find the outer conditional blocks
    static OUTER_CONDITIONAL_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{\{((?:[^{}]*\{\{[^}]*\}\})+[^{}]*)\}\}").unwrap());

    OUTER_CONDITIONAL_REGEX
        .replace_all(suggestions_provider, |caps: &Captures| {
            let block_content = &caps[1];
            let required_vars = find_variables_in_block(block_content);

            // Check if all variables required by this block are in the context
            if let Some(context) = context
                && required_vars
                    .iter()
                    .all(|(_, flat_name)| context.contains_key(flat_name))
            {
                // If so, replace the variables within the block and return it
                let mut resolved_block = block_content.to_string();
                for (variable, flat_name) in required_vars {
                    if let Some(value) = context.get(&flat_name) {
                        resolved_block = resolved_block.replace(&format!("{{{{{variable}}}}}"), value);
                    }
                }
                resolved_block
            } else {
                // Otherwise, this block is omitted
                String::new()
            }
        })
        .to_string()
}

/// Extracts all variable names from a string segment, returning both the variable and the flattened variable name
fn find_variables_in_block(block_content: &str) -> Vec<(String, String)> {
    COMMAND_VARIABLE_REGEX
        .captures_iter(block_content)
        .map(|cap| (cap[1].to_string(), flatten_variable_name(&cap[1])))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use futures_util::StreamExt;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_resolve_completions_empty() {
        let stream = resolve_completions(Vec::new(), BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;
        assert!(suggestions.is_empty());
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_completions_with_empty_command() {
        let completions = vec![VariableCompletion::new("user", "test", "VAR", "")];
        let stream = resolve_completions(completions, BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;
        assert!(suggestions.is_empty());
        assert_eq!(errors.len(), 1, "Expected an error for an empty provider");
    }

    #[tokio::test]
    async fn test_resolve_completions_with_invalid_command() {
        let completions = vec![VariableCompletion::new("user", "test", "VAR", "nonexistent_command")];
        let stream = resolve_completions(completions, BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;
        assert!(suggestions.is_empty());
        assert_eq!(errors.len(), 1, "Expected an error for a nonexistent command");
    }

    #[tokio::test]
    async fn test_resolve_completions_returns_all_results_including_duplicates() {
        let completions = vec![
            VariableCompletion::new("user", "test", "VAR", "printf 'foo\nbar'"),
            VariableCompletion::new("user", "test", "VAR2", "printf 'baz\nfoo'"),
        ];
        let stream = resolve_completions(completions, BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;

        assert!(errors.is_empty());
        assert_eq!(suggestions.len(), 2);

        // Sort by score to have a deterministic order for assertion
        let mut suggestions = suggestions;
        suggestions.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        assert_eq!(suggestions[0].0, 1.0); // First completion gets higher score boost
        assert_eq!(
            HashSet::<String>::from_iter(suggestions[0].1.iter().cloned()),
            HashSet::from_iter(vec!["foo".to_string(), "bar".to_string()])
        );

        assert_eq!(suggestions[1].0, 0.0); // Second completion gets lower score boost
        assert_eq!(
            HashSet::<String>::from_iter(suggestions[1].1.iter().cloned()),
            HashSet::from_iter(vec!["baz".to_string(), "foo".to_string()])
        );
    }

    #[tokio::test]
    async fn test_resolve_completions_with_mixed_success_and_failure() {
        let completions = vec![
            VariableCompletion::new("user", "test", "VAR1", "printf 'success1'"),
            VariableCompletion::new("user", "test", "VAR2", "this_is_not_a_command"),
            VariableCompletion::new("user", "test", "VAR3", "printf 'success2'"),
        ];
        let stream = resolve_completions(completions, BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;

        assert_eq!(suggestions.len(), 2);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("this_is_not_a_command"));
    }

    #[tokio::test]
    async fn test_resolve_completions_with_multiple_errors() {
        let completions = vec![
            VariableCompletion::new("user", "test", "VAR1", "cmd1_invalid"),
            VariableCompletion::new("user", "test", "VAR2", "cmd2_also_invalid"),
        ];
        let stream = resolve_completions(completions, BTreeMap::new()).await;
        let (suggestions, errors) = run_and_collect(stream).await;

        assert!(suggestions.is_empty());
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|e| e.contains("cmd1_invalid")));
        assert!(errors.iter().any(|e| e.contains("cmd2_also_invalid")));
    }

    #[test]
    fn test_no_conditional_blocks() {
        let command = "kubectl get pods";
        let context = context_from(&[("context", "my-cluster")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "kubectl get pods");
    }

    #[test]
    fn test_single_conditional_variable_present() {
        let command = "echo Hello {{{{name}}}}";
        let context = context_from(&[("name", "World")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "echo Hello World");
    }

    #[test]
    fn test_single_conditional_variable_absent() {
        let command = "echo Hello {{{{name}}}}";
        let context = BTreeMap::new();
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "echo Hello ");
    }

    #[test]
    fn test_single_conditional_block_present() {
        let command = "kubectl get pods {{--context {{context}}}}";
        let context = context_from(&[("context", "my-cluster")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "kubectl get pods --context my-cluster");
    }

    #[test]
    fn test_single_conditional_block_absent() {
        let command = "kubectl get pods {{--context {{context}}}}";
        let result = resolve_suggestions_provider(command, None);
        assert_eq!(result, "kubectl get pods ");
    }

    #[test]
    fn test_multiple_conditional_blocks_all_present() {
        let command = "kubectl get pods {{--context {{context}}}} {{-n {{namespace}}}}";
        let context = context_from(&[("context", "my-cluster"), ("namespace", "prod")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "kubectl get pods --context my-cluster -n prod");
    }

    #[test]
    fn test_multiple_conditional_blocks_some_present() {
        let command = "kubectl get pods {{--context {{context}}}} {{-n {{namespace}}}}";
        let context = context_from(&[("namespace", "prod")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "kubectl get pods  -n prod");
    }

    #[test]
    fn test_multiple_conditional_blocks_none_present() {
        let command = "kubectl get pods {{--context {{context}}}} {{-n {{namespace}}}}";
        let context = BTreeMap::new();
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "kubectl get pods  ");
    }

    #[test]
    fn test_block_with_multiple_inner_variables_all_present() {
        let command = "command {{--user {{user}} --password {{password}}}}";
        let context = context_from(&[("user", "admin"), ("password", "secret")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "command --user admin --password secret");
    }

    #[test]
    fn test_block_with_multiple_inner_variables_some_present() {
        let command = "command {{--user {{user}} --password {{password}}}}";
        let context = context_from(&[("user", "admin")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "command ");
    }

    #[test]
    fn test_mixed_static_and_conditional_parts() {
        let command = "docker run {{--name {{container_name}}}} -p 8080:80 {{image_name}}";
        let context = context_from(&[("container_name", "my-app")]);
        let result = resolve_suggestions_provider(command, Some(&context));
        assert_eq!(result, "docker run --name my-app -p 8080:80 {{image_name}}");
    }

    /// Helper to create a BTreeMap from a slice of tuples
    fn context_from(data: &[(&str, &str)]) -> BTreeMap<String, String> {
        data.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// Helper to collect results from the stream for testing purposes
    async fn run_and_collect(
        stream: impl Stream<Item = (f64, Result<Vec<String>, String>)>,
    ) -> (Vec<(f64, Vec<String>)>, Vec<String>) {
        let results = stream.collect::<Vec<_>>().await;
        let mut suggestions = Vec::new();
        let mut errors = Vec::new();

        for (score, result) in results {
            match result {
                Ok(s) => suggestions.push((score, s)),
                Err(e) => errors.push(e),
            }
        }
        (suggestions, errors)
    }
}
