use std::sync::LazyLock;

use futures_util::{Stream, stream};
use regex::{Captures, Regex};
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::instrument;

use super::{IntelliShellService, import_export::add_tags_to_description};
use crate::{
    ai::CommandFix,
    errors::{Result, UserFacingError},
    model::{CATEGORY_USER, Command, SOURCE_AI},
    utils::{
        execute_shell_command_capture, generate_working_dir_tree, get_executable_version, get_os_info, get_shell_info,
    },
};

/// Maximum depth level to include in the working directory tree
const WD_MAX_DEPTH: usize = 5;
/// Maximum number ofentries displayed on the working directory tree
const WD_ENTRY_LIMIT: usize = 30;

// Regex to find placeholders like ##VAR_NAME##
static PROMPT_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"##([A-Z_]+)##").unwrap());

/// Progress events for AI fix command
#[derive(Debug)]
pub enum AiFixProgress {
    /// The command has already been executed and the AI is now processing the request
    Thinking,
}

impl IntelliShellService {
    /// Tries to fix a failing command by using an AI model.
    ///
    /// If the command was successfully executed, this method will return [None].
    #[instrument(skip_all)]
    pub async fn fix_command<F>(
        &self,
        command: &str,
        history: Option<&str>,
        mut on_progress: F,
    ) -> Result<Option<CommandFix>>
    where
        F: FnMut(AiFixProgress),
    {
        // Check if ai is enabled
        if !self.ai.enabled {
            return Err(UserFacingError::AiRequired.into());
        }

        // Make sure we've got a command to fix
        if command.trim().is_empty() {
            return Err(UserFacingError::AiEmptyCommand.into());
        }

        // Execute the command and capture its output
        let (status, output, terminated_by_ctrl_c) = execute_shell_command_capture(command, true).await?;

        // If the command was interrupted by Ctrl+C, skip the fix
        if terminated_by_ctrl_c {
            tracing::info!("Command execution was interrupted by user (Ctrl+C), skipping fix");
            return Ok(None);
        }

        // If the command succeeded, return without fix
        if status.success() {
            tracing::info!("The command to fix was succesfully executed, skipping fix");
            return Ok(None);
        }

        on_progress(AiFixProgress::Thinking);

        // Prepare prompts and call the AI provider
        let root_cmd = command.split_whitespace().next();
        let sys_prompt = replace_prompt_placeholders(&self.ai.prompts.fix, root_cmd, history);
        let user_prompt = format!(
            "I've run a command but it failed, help me fix it.\n\ncommand: \
             {command}\n{status}\noutput:\n```\n{output}\n```"
        );

        tracing::trace!("System Prompt:\n{sys_prompt}");
        tracing::trace!("User Prompt:\n{user_prompt}");

        // Call provider
        let fix = self
            .ai
            .fix_client()?
            .generate_command_fix(&sys_prompt, &user_prompt)
            .await?;

        Ok(Some(fix))
    }

    /// Suggest command templates from an user prompt using an AI model
    #[instrument(skip_all)]
    pub async fn suggest_commands(&self, prompt: &str) -> Result<Vec<Command>> {
        // Check if ai is enabled
        if !self.ai.enabled {
            return Err(UserFacingError::AiRequired.into());
        }

        // Prepare prompts and call the AI provider
        let sys_prompt = replace_prompt_placeholders(&self.ai.prompts.suggest, None, None);

        tracing::trace!("System Prompt:\n{sys_prompt}");

        // Call provider
        let res = self
            .ai
            .suggest_client()?
            .generate_command_suggestions(&sys_prompt, prompt)
            .await?;

        Ok(res
            .suggestions
            .into_iter()
            .map(|s| Command::new(CATEGORY_USER, SOURCE_AI, s.command).with_description(Some(s.description)))
            .collect())
    }

    /// Extracts command templates from a given content using an AI model
    #[instrument(skip_all)]
    pub(super) async fn prompt_commands_import(
        &self,
        mut content: impl AsyncRead + Unpin + Send,
        tags: Vec<String>,
        category: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<impl Stream<Item = Result<Command>> + Send + 'static> {
        // Check if ai is enabled
        if !self.ai.enabled {
            return Err(UserFacingError::AiRequired.into());
        }

        // Read the content
        let mut prompt = String::new();
        content.read_to_string(&mut prompt).await?;

        let suggestions = if prompt.is_empty() {
            Vec::new()
        } else {
            // Prepare prompts and call the AI provider
            let sys_prompt = replace_prompt_placeholders(&self.ai.prompts.import, None, None);

            tracing::trace!("System Prompt:\n{sys_prompt}");

            // Call provider
            let res = self
                .ai
                .suggest_client()?
                .generate_command_suggestions(&sys_prompt, &prompt)
                .await?;

            res.suggestions
        };

        // Return commands
        let category = category.into();
        let source = source.into();
        Ok(stream::iter(
            suggestions
                .into_iter()
                .map(move |s| {
                    let mut description = s.description;
                    if !tags.is_empty() {
                        description = add_tags_to_description(&tags, description);
                    }
                    Command::new(category.clone(), source.clone(), s.command).with_description(Some(description))
                })
                .map(Ok),
        ))
    }
}

/// Replace placeholders present on the prompt for its value
fn replace_prompt_placeholders(prompt: &str, root_cmd: Option<&str>, history: Option<&str>) -> String {
    PROMPT_PLACEHOLDER_RE
        .replace_all(prompt, |caps: &Captures| match &caps[1] {
            "OS_SHELL_INFO" => {
                let shell_info = get_shell_info();
                let os_info = get_os_info();
                format!(
                    "### Context:\n- {os_info}\n- {}{}\n",
                    shell_info
                        .version
                        .clone()
                        .unwrap_or_else(|| shell_info.kind.to_string()),
                    root_cmd
                        .and_then(get_executable_version)
                        .map(|v| format!("\n- {v}"))
                        .unwrap_or_default(),
                )
            }
            "WORKING_DIR" => generate_working_dir_tree(WD_MAX_DEPTH, WD_ENTRY_LIMIT).unwrap_or_default(),
            "SHELL_HISTORY" => history
                .map(|h| format!("### User Shell History (oldest to newest):\n{h}\n"))
                .unwrap_or_default(),
            _ => {
                tracing::warn!("Prompt placeholder '{}' not recognized", &caps[0]);
                String::default()
            }
        })
        .to_string()
}
