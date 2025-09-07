use std::{fmt::Write, sync::LazyLock};

use futures_util::{Stream, stream};
use itertools::Itertools;
use regex::{Captures, Regex};
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::instrument;

use super::IntelliShellService;
use crate::{
    ai::CommandFix,
    errors::{AppError, Result, UserFacingError},
    model::{CATEGORY_USER, Command, SOURCE_AI, SearchMode},
    utils::{
        add_tags_to_description, execute_shell_command_capture, generate_working_dir_tree, get_executable_version,
        get_os_info, get_shell_info,
    },
};

/// Maximum depth level to include in the working directory tree
const WD_MAX_DEPTH: usize = 5;
/// Maximum number of entries displayed on the working directory tree
const WD_ENTRY_LIMIT: usize = 30;

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
            tracing::info!("The command to fix was successfully executed, skipping fix");
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

    /// Suggest a command template from a command and description using an AI model
    #[instrument(skip_all)]
    pub async fn suggest_command(&self, cmd: impl AsRef<str>, description: impl AsRef<str>) -> Result<Option<Command>> {
        // Check if ai is enabled
        if !self.ai.enabled {
            return Err(UserFacingError::AiRequired.into());
        }

        let cmd = Some(cmd.as_ref().trim()).filter(|c| !c.is_empty());
        let description = Some(description.as_ref().trim()).filter(|d| !d.is_empty());

        // Prepare prompts and call the AI provider
        let intro = "Output a single suggestion, with just one command template.";
        let sys_prompt = replace_prompt_placeholders(&self.ai.prompts.suggest, None, None);
        let user_prompt = match (cmd, description) {
            (Some(cmd), Some(desc)) => format!("{intro}\nGoal: {desc}\nYou can use this as the base: {cmd}"),
            (Some(prompt), None) | (None, Some(prompt)) => format!("{intro}\nGoal: {prompt}"),
            (None, None) => return Ok(None),
        };

        tracing::trace!("System Prompt:\n{sys_prompt}");
        tracing::trace!("User Prompt:\n{user_prompt}");

        // Call provider
        let res = self
            .ai
            .suggest_client()?
            .generate_command_suggestions(&sys_prompt, &user_prompt)
            .await?;

        Ok(res
            .suggestions
            .into_iter()
            .next()
            .map(|s| Command::new(CATEGORY_USER, SOURCE_AI, s.command).with_description(Some(s.description))))
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

    /// Suggest a command for a dynamic completion using an AI model
    #[instrument(skip_all)]
    pub async fn suggest_completion(
        &self,
        root_cmd: impl AsRef<str>,
        variable: impl AsRef<str>,
        description: impl AsRef<str>,
    ) -> Result<String> {
        // Check if ai is enabled
        if !self.ai.enabled {
            return Err(UserFacingError::AiRequired.into());
        }

        // Prepare variables
        let root_cmd = Some(root_cmd.as_ref().trim()).filter(|c| !c.is_empty());
        let variable = Some(variable.as_ref().trim()).filter(|v| !v.is_empty());
        let description = Some(description.as_ref().trim()).filter(|d| !d.is_empty());
        let Some(variable) = variable else {
            return Err(UserFacingError::CompletionEmptyVariable.into());
        };

        // Build a regex to match commands that would use the required completion
        let escaped_variable = regex::escape(variable);
        let variable_pattern = format!(r"\{{\{{(?:[^}}]+[|:])?{escaped_variable}(?:[|:][^}}]+)?\}}\}}");
        let cmd_regex = if let Some(root_cmd) = root_cmd {
            let root_cmd = regex::escape(root_cmd);
            format!(r"^{root_cmd}\s.*{variable_pattern}.*$")
        } else {
            format!(r"^.*{variable_pattern}.*$")
        };

        // Find those commands
        let (commands, _) = self
            .search_commands(SearchMode::Regex, false, &cmd_regex)
            .await
            .map_err(AppError::into_report)?;
        let commands_str = commands.into_iter().map(|c| c.cmd).join("\n");

        // Prepare prompts and call the AI provider
        let sys_prompt = replace_prompt_placeholders(&self.ai.prompts.completion, None, None);
        let mut user_prompt = String::new();
        writeln!(
            user_prompt,
            "Write a shell command that generates completion suggestions for the `{variable}` variable."
        )
        .unwrap();
        if let Some(rc) = root_cmd {
            writeln!(
                user_prompt,
                "This completion will be used only for commands starting with `{rc}`."
            )
            .unwrap();
        }
        if !commands_str.is_empty() {
            writeln!(
                user_prompt,
                "\nFor context, here are some existing command templates that use this \
                 variable:\n---\n{commands_str}\n---"
            )
            .unwrap();
        }
        if let Some(d) = description {
            writeln!(user_prompt, "\n{d}").unwrap();
        }

        tracing::trace!("System Prompt:\n{sys_prompt}");
        tracing::trace!("User Prompt:\n{user_prompt}");

        // Call provider
        let res = self
            .ai
            .completion_client()?
            .generate_completion_suggestion(&sys_prompt, &user_prompt)
            .await?;

        Ok(res.command)
    }
}

/// Replace placeholders present on the prompt for its value
fn replace_prompt_placeholders(prompt: &str, root_cmd: Option<&str>, history: Option<&str>) -> String {
    // Regex to find placeholders like ##VAR_NAME##
    static PROMPT_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"##([A-Z_]+)##").unwrap());

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
