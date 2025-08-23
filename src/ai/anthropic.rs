use std::fmt::Debug;

use color_eyre::eyre::Context;
use reqwest::{Client, RequestBuilder, Response, header::HeaderName};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value as Json, json};

use super::{AiProvider, AiProviderBase};
use crate::{
    config::AnthropicModelConfig,
    errors::{Result, UserFacingError},
};

const TOOL_NAME: &str = "propose_response";

impl AiProviderBase for AnthropicModelConfig {
    fn provider_name(&self) -> &'static str {
        "Anthropic"
    }

    fn auth_header(&self, api_key: String) -> (HeaderName, String) {
        (HeaderName::from_static("x-api-key"), api_key)
    }

    fn api_key_env_var_name(&self) -> &str {
        &self.api_key_env
    }

    fn build_request(
        &self,
        client: &Client,
        sys_prompt: &str,
        user_prompt: &str,
        json_schema: &Schema,
    ) -> RequestBuilder {
        // Request body
        // https://docs.anthropic.com/en/api/messages
        let request_body = json!({
            "model": self.model,
            "system": sys_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "max_tokens": 4096,
            "tools": [{
                "name": TOOL_NAME,
                "description": "Propose an structured response to the end user",
                "input_schema": json_schema,
            }],
            "tool_choice": {
                "type": "tool",
                "name": TOOL_NAME,
                "disable_parallel_tool_use": true
            }
        });

        tracing::trace!("Request:\n{request_body:#}");

        // Messages url
        let url = format!("{}/messages", self.url);

        // Request
        client
            .post(url)
            .header("anthropic-version", "2023-06-01")
            .json(&request_body)
    }
}

impl AiProvider for AnthropicModelConfig {
    async fn parse_response<T>(&self, res: Response) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        // Parse successful response
        let res: Json = res.json().await.wrap_err("Anthropic response not a json")?;
        tracing::trace!("Response:\n{res:#}");
        let mut res: AnthropicResponse<T> =
            serde_json::from_value(res).wrap_err("Couldn't parse Anthropic response")?;

        // Validate the response content
        if res.stop_reason != "end_turn" && res.stop_reason != "tool_use" {
            tracing::error!("OpenAI response got an invalid stop reason: {}", res.stop_reason);
            return Err(UserFacingError::AiRequestFailed(format!(
                "couldn't generate a valid response: {}",
                res.stop_reason
            ))
            .into());
        }

        if res.content.is_empty() {
            tracing::error!("Response got no content: {res:?}");
            return Err(UserFacingError::AiRequestFailed(String::from("received response with no content")).into());
        } else if res.content.len() > 1 {
            tracing::warn!("Response got {} content blocks", res.content.len());
        }

        let block = res.content.remove(0);
        if block.r#type != "tool_use" {
            tracing::error!("Anthropic response got an invalid content type: {}", block.r#type);
            return Err(UserFacingError::AiRequestFailed(format!("unexpected response type: {}", block.r#type)).into());
        }

        if block.name != TOOL_NAME {
            tracing::error!("Anthropic response got an invalid tool name: {}", block.name);
            return Err(UserFacingError::AiRequestFailed(format!("received invalid tool name: {}", block.name)).into());
        }

        Ok(block.input)
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse<T> {
    content: Vec<ContentBlock<T>>,
    stop_reason: String,
}

#[derive(Debug, Deserialize)]
struct ContentBlock<T> {
    r#type: String,
    name: String,
    input: T,
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    use super::*;
    use crate::{ai::AiClient, config::AiModelConfig};

    #[tokio::test]
    #[ignore] // Real API calls require valid api keys
    async fn test_anthropic_api() -> Result<()> {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
        let config = AiModelConfig::Anthropic(AnthropicModelConfig {
            model: "claude-sonnet-4-0".into(),
            url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        });
        let client = AiClient::new("test", &config, "", None)?;
        let res = client
            .generate_command_suggestions(
                "you're a cli expert, that will proide command suggestions based on what the user want to do",
                "undo last n amount of commits",
            )
            .await?;
        tracing::info!("Suggestions:");
        for command in res.suggestions {
            tracing::info!("  # {}", command.description);
            tracing::info!("  {}", command.command);
        }
        Ok(())
    }
}
