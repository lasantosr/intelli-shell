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
    config::OllamaModelConfig,
    errors::{Result, UserFacingError},
};

impl AiProviderBase for OllamaModelConfig {
    fn provider_name(&self) -> &'static str {
        "Ollama"
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
        // https://github.com/jmorganca/ollama/blob/main/docs/api.md#generate-a-chat-completion
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
            "format": json_schema,
            "stream": false
        });

        tracing::trace!("Request:\n{request_body:#}");

        // Chat url
        let url = format!("{}/api/chat", self.url);

        // Request
        client.post(url).json(&request_body)
    }
}

impl AiProvider for OllamaModelConfig {
    async fn parse_response<T>(&self, res: Response) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        // Parse successful response
        let res: Json = res.json().await.wrap_err("Ollama response not a json")?;
        tracing::trace!("Response:\n{res:#}");
        let res: OllamaResponse = serde_json::from_value(res).wrap_err("Couldn't parse Ollama response")?;

        // Validate the response content
        let Some(message) = res.message.content.filter(|c| !c.trim().is_empty()) else {
            tracing::error!("Ollama returned an empty response");
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
struct OllamaResponse {
    message: OllamaResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    use super::*;
    use crate::{ai::AiClient, config::AiModelConfig};

    #[tokio::test]
    #[ignore] // Real API calls require a running ollama server
    async fn test_ollama_api() -> Result<()> {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
        let config = AiModelConfig::Ollama(OllamaModelConfig {
            model: "gemma3:1b".into(),
            url: "http://localhost:11434".into(),
            api_key_env: "OLLAMA_API_KEY".into(),
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
