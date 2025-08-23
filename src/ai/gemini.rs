use std::fmt::Debug;

use color_eyre::eyre::Context;
use reqwest::{Client, RequestBuilder, Response, header::HeaderName};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value as Json, json};

use super::{AiProvider, AiProviderBase};
use crate::{
    config::GeminiModelConfig,
    errors::{Result, UserFacingError},
};

impl AiProviderBase for GeminiModelConfig {
    fn provider_name(&self) -> &'static str {
        "Gemini"
    }

    fn auth_header(&self, api_key: String) -> (HeaderName, String) {
        (HeaderName::from_static("x-goog-api-key"), api_key)
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
        // https://ai.google.dev/api/rest/v1beta/models/generateContent
        let request_body = json!({
            "system_instruction": {
                "parts": [{ "text": sys_prompt }]
            },
            "contents": [{
                "role": "user",
                "parts": [{ "text": user_prompt }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseJsonSchema": json_schema,
            }
        });

        tracing::trace!("Request:\n{request_body:#}");

        // Generate content url
        let url = format!("{}/models/{}:generateContent", self.url, self.model);

        // Request
        client.post(url).json(&request_body)
    }
}

impl AiProvider for GeminiModelConfig {
    async fn parse_response<T>(&self, res: Response) -> Result<T>
    where
        T: DeserializeOwned + JsonSchema + Debug,
    {
        // Parse successful response
        let res: Json = res.json().await.wrap_err("Gemini response not a json")?;
        tracing::trace!("Response:\n{res:#}");
        let mut res: GeminiResponse = serde_json::from_value(res).wrap_err("Couldn't parse Gemini response")?;

        // Validate the response content
        if res.candidates.is_empty() {
            tracing::error!("Response got no candidates: {res:?}");
            return Err(UserFacingError::AiRequestFailed(String::from("received response with no candidates")).into());
        } else if res.candidates.len() > 1 {
            tracing::warn!("Response got {} candidates", res.candidates.len());
        }

        let mut candidate = res.candidates.remove(0);
        if let Some(finish_reason) = candidate.finish_reason
            && finish_reason != "STOP"
        {
            tracing::error!("Gemini response got an invalid finish reason: {finish_reason}");
            return Err(UserFacingError::AiRequestFailed(format!(
                "couldn't generate a valid response: {finish_reason}"
            ))
            .into());
        }

        if candidate.content.parts.is_empty() {
            tracing::error!("Response candidate got no parts");
            return Err(
                UserFacingError::AiRequestFailed(String::from("received response candidate with no parts")).into(),
            );
        } else if candidate.content.parts.len() > 1 {
            tracing::warn!("Response candidate got {} parts", candidate.content.parts.len());
        }

        let part = candidate.content.parts.remove(0);
        let Some(text) = part.text.filter(|c| !c.trim().is_empty()) else {
            tracing::error!("Gemini returned an empty candidate part");
            return Err(UserFacingError::AiRequestFailed(String::from("received an empty response")).into());
        };

        // Parse the text
        Ok(serde_json::from_str(&text).map_err(|err| {
            tracing::error!("Couldn't parse API response into the expected format: {err}\nText:\n{text}");
            UserFacingError::AiRequestFailed(String::from("couldn't parse api response into the expected format"))
        })?)
    }
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: GeminiContent,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    use super::*;
    use crate::{ai::AiClient, config::AiModelConfig};

    #[tokio::test]
    #[ignore] // Real API calls require valid api keys
    async fn test_gemini_api() -> Result<()> {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
        let config = AiModelConfig::Gemini(GeminiModelConfig {
            model: "gemini-2.5-flash-lite".into(),
            url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key_env: "GEMINI_API_KEY".into(),
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
