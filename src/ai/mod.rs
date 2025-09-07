use std::{env, fmt::Debug, time::Duration};

use color_eyre::eyre::{Context, ContextCompat, eyre};
use reqwest::{
    Client, ClientBuilder, RequestBuilder, Response, StatusCode,
    header::{self, HeaderMap, HeaderName, HeaderValue},
};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::instrument;

use crate::{
    config::AiModelConfig,
    errors::{AppError, Result, UserFacingError},
};

mod anthropic;
mod gemini;
mod ollama;
mod openai;

/// A trait that defines the provider-specific logic for the generic [`AiClient`]
pub trait AiProviderBase: Send + Sync {
    /// The name of the provider
    fn provider_name(&self) -> &'static str;

    /// Returns the header name and value to authenticate the given api key
    fn auth_header(&self, api_key: String) -> (HeaderName, String);

    /// The name of the environent variable expected to have the api key
    fn api_key_env_var_name(&self) -> &str;

    /// Build the provider-specific request
    fn build_request(
        &self,
        client: &Client,
        sys_prompt: &str,
        user_prompt: &str,
        json_schema: &Schema,
    ) -> RequestBuilder;
}

/// A trait that defines the provider-specific logic for the generic [`AiClient`]
#[trait_variant::make(Send)]
pub trait AiProvider: AiProviderBase {
    /// Parse the provider-specific response
    async fn parse_response<T>(&self, res: Response) -> Result<T>
    where
        Self: Sized,
        T: DeserializeOwned + JsonSchema + Debug;
}

/// A generic client to communicate with AI providers
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct AiClient<'a> {
    inner: Client,
    primary_alias: &'a str,
    primary: &'a AiModelConfig,
    fallback_alias: &'a str,
    fallback: Option<&'a AiModelConfig>,
}
impl<'a> AiClient<'a> {
    /// Creates a new AI client with a primary and an optional fallback model configuration
    pub fn new(
        primary_alias: &'a str,
        primary: &'a AiModelConfig,
        fallback_alias: &'a str,
        fallback: Option<&'a AiModelConfig>,
    ) -> Result<Self> {
        // Construct the base headers for all requests
        let mut headers = HeaderMap::new();
        headers.append(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Build the reqwest client
        let inner = ClientBuilder::new()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(5 * 60))
            .user_agent("intelli-shell")
            .default_headers(headers)
            .build()
            .wrap_err("Couldn't build AI client")?;

        Ok(AiClient {
            inner,
            primary_alias,
            primary,
            fallback_alias,
            fallback,
        })
    }

    /// Generate some command suggestions based on the given prompt
    #[instrument(skip_all)]
    pub async fn generate_command_suggestions(
        &self,
        sys_prompt: &str,
        user_prompt: &str,
    ) -> Result<CommandSuggestions> {
        self.generate_content(sys_prompt, user_prompt).await
    }

    /// Generate a command fix based on the given prompt
    #[instrument(skip_all)]
    pub async fn generate_command_fix(&self, sys_prompt: &str, user_prompt: &str) -> Result<CommandFix> {
        self.generate_content(sys_prompt, user_prompt).await
    }

    /// Generate a command for a dynamic variable completion
    #[instrument(skip_all)]
    pub async fn generate_completion_suggestion(
        &self,
        sys_prompt: &str,
        user_prompt: &str,
    ) -> Result<VariableCompletionSuggestion> {
        self.generate_content(sys_prompt, user_prompt).await
    }

    /// The inner logic to generate content from a prompt with an AI provider.
    ///
    /// It attempts the primary model first, and uses the fallback model if the primary is rate-limited.
    async fn generate_content<T>(&self, sys_prompt: &str, user_prompt: &str) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        // First, try with the primary model
        let primary_result = self.execute_request(self.primary, sys_prompt, user_prompt).await;

        // Check if the primary attempt failed with a rate limit error
        if let Err(AppError::UserFacing(UserFacingError::AiRateLimit)) = &primary_result {
            // If it's a rate limit error and we have a fallback model, try again with it
            if let Some(fallback) = self.fallback {
                tracing::warn!(
                    "Primary model ({}) rate-limited, retrying with fallback ({})",
                    self.primary_alias,
                    self.fallback_alias
                );
                return self.execute_request(fallback, sys_prompt, user_prompt).await;
            }
        }

        // Check if the primary attempt failed with a service unavailable error
        if let Err(AppError::UserFacing(UserFacingError::AiUnavailable)) = &primary_result {
            // Some APIs respond this status when a specific model is overloaded, so we try with the fallback
            if let Some(fallback) = self.fallback {
                tracing::warn!(
                    "Primary model ({}) unavailable, retrying with fallback ({})",
                    self.primary_alias,
                    self.fallback_alias
                );
                return self.execute_request(fallback, sys_prompt, user_prompt).await;
            }
        }

        // Otherwise, return the result of the primary attempt
        primary_result
    }

    /// Executes a single AI content generation request against a specific model configuration
    #[instrument(skip_all, fields(provider = config.provider().provider_name()))]
    async fn execute_request<T>(&self, config: &AiModelConfig, sys_prompt: &str, user_prompt: &str) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        let provider = config.provider();

        // Generate the json schema from the expected type
        let json_schema = build_json_schema_for::<T>()?;

        // Prepare the request body
        let mut req_builder = provider.build_request(&self.inner, sys_prompt, user_prompt, &json_schema);

        // Include auth header for this config
        let api_key_env = provider.api_key_env_var_name();
        if let Ok(api_key) = env::var(api_key_env) {
            let (header_name, header_value) = provider.auth_header(api_key);
            let mut header_value =
                HeaderValue::from_str(&header_value).wrap_err_with(|| format!("Invalid '{api_key_env}' value"))?;
            header_value.set_sensitive(true);
            req_builder = req_builder.header(header_name, header_value);
        }

        // Build the request
        let req = req_builder.build().wrap_err("Couldn't build api request")?;

        // Call the API
        tracing::debug!("Calling {} API: {}", provider.provider_name(), req.url());
        let res = self.inner.execute(req).await.map_err(|err| {
            if err.is_timeout() {
                tracing::error!("Request timeout: {err:?}");
                UserFacingError::AiRequestTimeout
            } else if err.is_connect() {
                tracing::error!("Couldn't connect to the API: {err:?}");
                UserFacingError::AiRequestFailed(String::from("error connecting to the provider"))
            } else {
                tracing::error!("Couldn't perform the request: {err:?}");
                UserFacingError::AiRequestFailed(err.to_string())
            }
        })?;

        // Check the response status
        if !res.status().is_success() {
            let status = res.status();
            let status_str = status.as_str();
            let body = res.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                tracing::warn!(
                    "Got response [{status_str}] {}",
                    status.canonical_reason().unwrap_or_default()
                );
                tracing::debug!("{body}");
                return Err(
                    UserFacingError::AiMissingOrInvalidApiKey(provider.api_key_env_var_name().to_string()).into(),
                );
            } else if status == StatusCode::TOO_MANY_REQUESTS {
                tracing::info!("Got response [{status_str}] Too Many Requests");
                tracing::debug!("{body}");
                return Err(UserFacingError::AiRateLimit.into());
            } else if status == StatusCode::SERVICE_UNAVAILABLE {
                tracing::info!("Got response [{status_str}] Service Unavailable");
                tracing::debug!("{body}");
                return Err(UserFacingError::AiUnavailable.into());
            } else if status == StatusCode::BAD_REQUEST {
                tracing::error!("Got response [{status_str}] Bad Request:\n{body}");
                return Err(eyre!("Bad request while fetching {} API:\n{body}", provider.provider_name()).into());
            } else if let Some(reason) = status.canonical_reason() {
                tracing::error!("Got response [{status_str}] {reason}:\n{body}");
                return Err(
                    UserFacingError::AiRequestFailed(format!("received {status_str} {reason} response")).into(),
                );
            } else {
                tracing::error!("Got response [{status_str}]:\n{body}");
                return Err(UserFacingError::AiRequestFailed(format!("received {status_str} response")).into());
            }
        }

        // Parse successful response
        match &config {
            AiModelConfig::Openai(conf) => conf.parse_response(res).await,
            AiModelConfig::Gemini(conf) => conf.parse_response(res).await,
            AiModelConfig::Anthropic(conf) => conf.parse_response(res).await,
            AiModelConfig::Ollama(conf) => conf.parse_response(res).await,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommandSuggestions {
    /// The list of suggested commands for the user to choose from
    pub suggestions: Vec<CommandSuggestion>,
}

/// A structured object representing a suggestion for a shell command and its explanation
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommandSuggestion {
    /// A clear, concise, human-readable explanation of the generated command, usually a single sentence.
    /// This description is for the end-user to help them understand the command before executing it.
    pub description: String,
    /// The command template string. Use `{{variable-name}}` syntax for any placeholders that require user input.
    /// For ephemeral values like commit messages or sensitive values like API keys or passwords, use the triple-brace
    /// syntax `{{{variable-name}}}`.
    pub command: String,
}

/// A structured object to propose a fix to a failed command
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommandFix {
    /// A very brief, 2-5 word summary of the error category.
    /// Examples: "Command Not Found", "Permission Denied", "Invalid Argument", "Git Typo".
    pub summary: String,
    /// A detailed, human-readable explanation of the root cause of the error.
    /// This section should explain *what* went wrong and *why*, based on the provided error message,
    /// but should not contain the solution itself.
    pub diagnosis: String,
    /// A human-readable string describing the recommended next steps.
    /// This can be a description of a fix, diagnostic command(s) to run, or a suggested workaround.
    pub proposal: String,
    /// The corrected, valid, ready-to-execute command string if the error was a simple typo or syntax issue.
    /// This field should only be populated if a direct command correction is the primary solution.
    /// Example: "git status"
    pub fixed_command: String,
}

/// A structured object to propose a command for a dynamic variable completion
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VariableCompletionSuggestion {
    /// The shell command that generates the suggestion values when executed
    pub command: String,
}

/// Build the json schema for the given type, including `additionalProperties: false`
fn build_json_schema_for<T: JsonSchema>() -> Result<Schema> {
    // Generate the derived schema
    let mut schema = schema_for!(T);

    // The schema must be an object, for most LLMs to support it
    let root = schema.as_object_mut().wrap_err("The type must be an object")?;
    root.insert("additionalProperties".into(), false.into());

    // If there's any additional object definition, also update the additionalProperties
    if let Some(defs) = root.get_mut("$defs") {
        for definition in defs.as_object_mut().wrap_err("Expected objects at $defs")?.values_mut() {
            if let Some(def_obj) = definition.as_object_mut()
                && def_obj.get("type").and_then(|t| t.as_str()) == Some("object")
            {
                def_obj.insert("additionalProperties".into(), false.into());
            }
        }
    }

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_suggestions_schema() {
        let schema = build_json_schema_for::<CommandSuggestions>().unwrap();
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    }

    #[test]
    fn test_command_fix_schema() {
        let schema = build_json_schema_for::<CommandFix>().unwrap();
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    }
}
