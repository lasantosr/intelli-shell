use std::sync::LazyLock;

use regex::Regex;
use tracing::instrument;
use uuid::Uuid;

use super::IntelliShellService;
use crate::{
    errors::{Result, UserFacingError},
    model::VariableCompletion,
    utils::{flatten_str, flatten_variable_name},
};

pub static FORBIDDEN_COMPLETION_ROOT_CMD_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w-]").unwrap());
pub static FORBIDDEN_COMPLETION_VARIABLE_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[:|{}]").unwrap());

impl IntelliShellService {
    /// Lists all unique root commands for variable completions
    #[instrument(skip_all)]
    pub async fn list_variable_completion_root_cmds(&self) -> Result<Vec<String>> {
        tracing::debug!("Listing variable completion root commands");
        self.storage.list_variable_completion_root_cmds().await
    }

    /// Lists variable completions, optionally filtering by root command and variable name
    #[instrument(skip_all)]
    pub async fn list_variable_completions(&self, root_cmd: Option<&str>) -> Result<Vec<VariableCompletion>> {
        tracing::debug!("Listing variable completions for '{:?}'", root_cmd);

        let flat_root_cmd = root_cmd.map(flatten_str);

        self.storage.list_variable_completions(flat_root_cmd, None, false).await
    }

    /// Creates a new variable completion
    #[instrument(skip_all)]
    pub async fn create_variable_completion(&self, var: VariableCompletion) -> Result<VariableCompletion> {
        if var.flat_variable.is_empty() {
            return Err(UserFacingError::CompletionEmptyVariable.into());
        }
        if var.suggestions_provider.is_empty() {
            return Err(UserFacingError::CompletionEmptySuggestionsProvider.into());
        }
        if FORBIDDEN_COMPLETION_ROOT_CMD_CHARS.is_match(&var.root_cmd) {
            return Err(UserFacingError::CompletionInvalidCommand.into());
        }
        if FORBIDDEN_COMPLETION_VARIABLE_CHARS.is_match(&var.variable) {
            return Err(UserFacingError::CompletionInvalidVariable.into());
        }

        if var.is_global() {
            tracing::info!(
                "Creating a global variable completion for '{}': {}",
                var.flat_variable,
                var.suggestions_provider
            );
        } else {
            tracing::info!(
                "Creating a variable completion for '{}' '{}': {}",
                var.flat_root_cmd,
                var.flat_variable,
                var.suggestions_provider
            );
        }

        self.storage.insert_variable_completion(var).await
    }

    /// Updates a variable completion
    #[instrument(skip_all)]
    pub async fn update_variable_completion(&self, var: VariableCompletion) -> Result<VariableCompletion> {
        if var.flat_variable.is_empty() {
            return Err(UserFacingError::CompletionEmptyVariable.into());
        }
        if var.suggestions_provider.is_empty() {
            return Err(UserFacingError::CompletionEmptySuggestionsProvider.into());
        }
        if FORBIDDEN_COMPLETION_ROOT_CMD_CHARS.is_match(&var.root_cmd) {
            return Err(UserFacingError::CompletionInvalidCommand.into());
        }
        if FORBIDDEN_COMPLETION_VARIABLE_CHARS.is_match(&var.variable) {
            return Err(UserFacingError::CompletionInvalidVariable.into());
        }

        tracing::info!(
            "Updating variable completion '{}': {}",
            var.id,
            var.suggestions_provider
        );

        self.storage.update_variable_completion(var).await
    }

    /// Deletes a variable completion
    #[instrument(skip_all)]
    pub async fn delete_variable_completion(&self, id: Uuid) -> Result<()> {
        tracing::info!("Deleting variable completion: {id}");
        self.storage.delete_variable_completion(id).await
    }

    /// Deletes a variable completion by its unique key, returning it
    #[instrument(skip_all)]
    pub async fn delete_variable_completion_by_key(
        &self,
        root_cmd: impl AsRef<str>,
        variable_name: impl AsRef<str>,
    ) -> Result<Option<VariableCompletion>> {
        let flat_root_cmd = flatten_str(root_cmd);
        let flat_variable_name = flatten_variable_name(variable_name);

        if flat_root_cmd.is_empty() {
            tracing::info!("Deleting global variable completion for '{flat_variable_name}'");
        } else {
            tracing::info!("Deleting variable completion for '{flat_root_cmd}' '{flat_variable_name}'");
        }

        self.storage
            .delete_variable_completion_by_key(flat_root_cmd, flat_variable_name)
            .await
    }
}
