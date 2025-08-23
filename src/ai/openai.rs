use std::fmt::Debug;

use color_eyre::eyre::Context;
use reqwest::{
    Client, RequestBuilder, Response,
    header::{self, HeaderName},
};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value as Json, json};

use super::{AiProvider, AiProviderBase};
use crate::{
    config::OpenAiModelConfig,
    errors::{Result, UserFacingError},
};

impl AiProviderBase for OpenAiModelConfig {
    fn provider_name(&self) -> &'static str {
        "OpenAI"
    }

    fn auth_header(&self, api_key: String) -> (HeaderName, String) {
        (header::AUTHORIZATION, format!("Bearer {api_key}"))
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
        // https://platform.openai.com/docs/api-reference/chat/create
        let request_body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": sys_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "command_suggestions",
                    "strict": true,
                    "schema": json_schema
                }
            }
        });

        tracing::trace!("Request:\n{request_body:#}");

        // Chat completions url
        let url = format!("{}/chat/completions", self.url);

        // Request
        client.post(url).json(&request_body)
    }
}

impl AiProvider for OpenAiModelConfig {
    async fn parse_response<T>(&self, res: Response) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        // Parse successful response
        let res: Json = res.json().await.wrap_err("OpenAI response not a json")?;
        tracing::trace!("Response:\n{res:#}");
        let mut res: OpenAiResponse = serde_json::from_value(res).wrap_err("Couldn't parse OpenAI response")?;

        // Validate the response content
        if res.choices.is_empty() {
            tracing::error!("Response got no choices: {res:?}");
            return Err(UserFacingError::AiRequestFailed(String::from("received response with no choices")).into());
        } else if res.choices.len() > 1 {
            tracing::warn!("Response got {} choices", res.choices.len());
        }

        let choice = res.choices.remove(0);
        if choice.finish_reason != "stop" {
            tracing::error!("OpenAI response got an invalid finish reason: {}", choice.finish_reason);
            return Err(UserFacingError::AiRequestFailed(format!(
                "couldn't generate a valid response: {}",
                choice.finish_reason
            ))
            .into());
        }

        if let Some(refusal) = choice.message.refusal
            && !refusal.is_empty()
        {
            tracing::error!("OpenAI refused to answer: {refusal}");
            return Err(UserFacingError::AiRequestFailed(format!("response refused: {refusal}")).into());
        }

        let Some(message) = choice.message.content.filter(|c| !c.trim().is_empty()) else {
            tracing::error!("OpenAI returned an empty response");
            return Err(UserFacingError::AiRequestFailed(String::from("received an empty response")).into());
        };

        // Parse the message
        Ok(serde_json::from_str(&message).map_err(|err| {
            tracing::error!("Couldn't parse API response into the expected format: {err}\nMessage:\n{message}");
            UserFacingError::AiRequestFailed(String::from("couldn't parse api response into the expected format"))
        })?)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    #[serde(default)]
    refusal: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    use super::*;
    use crate::{ai::AiClient, config::AiModelConfig};

    #[tokio::test]
    #[ignore] // Real API calls require valid api keys
    async fn test_openai_api() -> Result<()> {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
        let config = AiModelConfig::Openai(OpenAiModelConfig {
            model: "gpt-4.1-nano".into(),
            url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
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
